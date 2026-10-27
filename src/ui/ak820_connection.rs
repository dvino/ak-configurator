use super::*;

const EXPECTED_LAYERS: usize = 6;
const EXPECTED_ROWS: usize = 16;
const EXPECTED_COLS: usize = 8;
const EXPECTED_KEYS: usize = 83;
const EXPECTED_ENCODERS: usize = 1;
const EXPECTED_MACROS: u8 = 16;
const EXPECTED_MACRO_BYTES: u16 = 8192;
const EXPECTED_COMBOS: u8 = 8;

fn validate_ak820_layout(layout: &KeyboardLayout) -> Result<(), String> {
    if (layout.rows, layout.cols, layout.keys.len())
        != (EXPECTED_ROWS, EXPECTED_COLS, EXPECTED_KEYS)
    {
        return Err(format!(
            "AK820 layout contract mismatch: expected {EXPECTED_ROWS}x{EXPECTED_COLS} matrix and {EXPECTED_KEYS} physical keys, got {}x{} and {} keys",
            layout.rows,
            layout.cols,
            layout.keys.len()
        ));
    }
    let mut coordinates = std::collections::HashSet::with_capacity(layout.keys.len());
    for key in &layout.keys {
        if key.row as usize >= layout.rows || key.col as usize >= layout.cols {
            return Err(format!(
                "AK820 layout has an out-of-matrix physical key at ({},{})",
                key.row, key.col
            ));
        }
        if !coordinates.insert((key.row, key.col)) {
            return Err(format!(
                "AK820 layout has duplicate physical-key coordinate ({},{})",
                key.row, key.col
            ));
        }
        if crate::ak820::MATRIX_MASK[key.row as usize] & (1 << key.col) == 0 {
            return Err(format!(
                "AK820 layout has no physical key at ({},{})",
                key.row, key.col
            ));
        }
    }
    if layout.encoders.len() != 2
        || layout.encoder_count() != EXPECTED_ENCODERS
        || layout
            .encoders
            .iter()
            .any(|encoder| encoder.encoder_idx != 0 || encoder.direction > 1)
        || layout.encoders[0].direction == layout.encoders[1].direction
    {
        return Err(format!(
            "AK820 layout must describe one encoder with two directions; definition has {} encoder slots",
            layout.encoders.len()
        ));
    }
    Ok(())
}

pub(super) fn load_ak820(
    dev: &crate::device::Device,
    dev_conn: crate::hid::HidDevice,
    progress: impl Fn(&str) -> Result<(), String>,
) -> Result<ConnectResult, String> {
    progress("Verifying AK820 firmware identity…")?;
    dev_conn
        .verify_ak820_firmware(|message| progress(message))
        .map_err(|error| format!("AK820 firmware verification failed: {error:#}"))?;
    load_verified_ak820(dev, dev_conn, progress)
}

fn load_verified_ak820(
    dev: &crate::device::Device,
    dev_conn: crate::hid::HidDevice,
    progress: impl Fn(&str) -> Result<(), String>,
) -> Result<ConnectResult, String> {
    let image = dev_conn
        .ak820_image()
        .ok_or("AK820 image has not been verified")?;
    progress("Reading AK820 protocol identity…")?;
    let via_protocol = dev_conn
        .get_protocol_version()
        .map_err(|e| format!("VIA protocol read failed: {e:#}"))?;
    let (vial_protocol, keyboard_id) = dev_conn
        .get_keyboard_id()
        .map_err(|e| format!("Vial identity read failed: {e:#}"))?;
    if via_protocol != 9 || vial_protocol != 6 || keyboard_id != crate::ak820::VIAL_UID {
        return Err(format!(
            "Unsupported AK820 protocol identity (VIA {via_protocol}, Vial {vial_protocol}, UID {keyboard_id:016X}); settings remain read-only."
        ));
    }

    progress("Reading fresh AK820 layout definition…")?;
    let definition_size = dev_conn
        .get_definition_size()
        .map_err(|e| format!("Layout size read failed: {e:#}"))?;
    let json = dev_conn
        .get_layout_json_with_size_and_progress(definition_size, |completed, total| {
            if total > 0 {
                progress(&format!(
                    "Reading AK820 layout definition… {completed}/{total}"
                ))
                .map_err(anyhow::Error::msg)?;
            }
            Ok(())
        })
        .map_err(|e| format!("Layout read failed: {e:#}"))?;
    let mut layout =
        KeyboardLayout::from_vial_json(&json).map_err(|e| format!("Layout parse failed: {e:#}"))?;
    validate_ak820_layout(&layout)?;
    if image.profile().is_some() {
        let identity = image
            .matrix_profile()
            .ok_or("Missing exact image profile")?
            .settings_identity
            .as_ref()
            .ok_or("AK820 profile lacks an exact settings identity")?;
        // The firmware's definition digest uses sorted, indented UTF-8 JSON.
        // Geometry contains fractions, unlike the integer-only backup format.
        let mut encoded = serde_json::to_vec_pretty(&json).map_err(|e| e.to_string())?;
        encoded.push(b'\n');
        if crate::ak820_settings_backup::sha256(&encoded) != identity.definition_sha256 {
            return Err("AK820 definition differs from its exact image contract".into());
        }
    }
    layout.live_features = Default::default();
    layout.custom_keycodes.clear();
    layout.lighting_mode = Some("vialrgb".to_owned());
    layout.supports_rgb = true;

    progress("Checking AK820 protocol capacities…")?;
    let layer_count = dev_conn
        .get_layer_count()
        .map_err(|e| format!("Layer count read failed: {e:#}"))? as usize;
    let macro_count = dev_conn
        .get_macro_count()
        .map_err(|e| format!("Macro count read failed: {e:#}"))?;
    let macro_memory_bytes = dev_conn
        .get_macro_buffer_size()
        .map_err(|e| format!("Macro buffer size read failed: {e:#}"))?;
    let dynamic = dev_conn
        .get_dynamic_entry_counts()
        .map_err(|e| format!("Dynamic entry count read failed: {e:#}"))?;
    if layer_count != EXPECTED_LAYERS
        || macro_count != EXPECTED_MACROS
        || macro_memory_bytes != EXPECTED_MACRO_BYTES
        || dynamic != image.dynamic_counts()
    {
        return Err(format!(
            "AK820 capacity mismatch (layers {layer_count}, macros {macro_count}/{macro_memory_bytes} bytes, dynamic {dynamic:?}); settings remain read-only."
        ));
    }

    progress("Reading all AK820 keymaps…")?;
    let keymap = dev_conn
        .get_keymap_buffer(layer_count, layout.rows, layout.cols)
        .map_err(|e| format!("Complete keymap read failed: {e:#}"))?;
    let expected_size = layer_count
        .checked_mul(layout.rows)
        .and_then(|n| n.checked_mul(layout.cols))
        .ok_or("AK820 keymap size overflow")?;
    if keymap.len() != expected_size {
        return Err(format!(
            "AK820 keymap length mismatch: expected {expected_size}, got {}",
            keymap.len()
        ));
    }
    layout.layers =
        vec![vec![crate::keyboard::KeyBinding::default(); layout.keys.len()]; layer_count];
    for layer in 0..layer_count {
        for (key_index, key) in layout.keys.iter().enumerate() {
            let index = layer * layout.rows * layout.cols
                + key.row as usize * layout.cols
                + key.col as usize;
            if !image.supports_keycode(keymap[index], ak820_beta1_profiles::KeycodeSource::Matrix) {
                return Err("AK820 keymap contains an action unavailable in this profile".into());
            }
            layout.layers[layer][key_index] = keymap[index].into();
        }
    }

    progress("Reading all AK820 encoder mappings…")?;
    layout.encoder_layers = vec![vec![0; layout.encoders.len()]; layer_count];
    for layer in 0..layer_count {
        let (ccw, cw) = dev_conn
            .get_encoder(layer as u8, 0)
            .map_err(|e| format!("Encoder read failed for layer {layer}: {e:#}"))?;
        if [ccw, cw].iter().any(|&code| {
            !image.supports_keycode(code, ak820_beta1_profiles::KeycodeSource::Encoder)
        }) {
            return Err(
                "AK820 encoder contains an action unavailable for this source/profile".into(),
            );
        }
        for (index, encoder) in layout.encoders.iter().enumerate() {
            layout.encoder_layers[layer][index] = if encoder.direction == 0 { ccw } else { cw };
        }
    }

    progress("Reading complete AK820 macro buffer…")?;
    let macro_buffer = dev_conn
        .get_macro_buffer(macro_memory_bytes, macro_count)
        .map_err(|e| format!("Complete macro buffer read failed: {e:#}"))?;
    dev_conn
        .ak820_validate_macro_snapshot(&macro_buffer)
        .map_err(|e| format!("AK820 macro actions differ from the image contract: {e:#}"))?;
    let macro_texts = crate::hid::HidDevice::parse_macros(&macro_buffer, macro_count);
    if macro_texts.len() != macro_count as usize {
        return Err("AK820 macro parser returned an incomplete macro set".to_owned());
    }

    progress("Reading and validating AK820 combos…")?;
    let mut combo_entries = Vec::with_capacity(EXPECTED_COMBOS as usize);
    for index in 0..EXPECTED_COMBOS {
        let (keys, output) = dev_conn
            .get_combo(index)
            .map_err(|e| format!("Combo {index} read failed: {e:#}"))?;
        if !image.valid_combo(keys, output) {
            return Err(format!(
                "AK820 combo {index} is invalid; settings remain read-only."
            ));
        }
        combo_entries.push(ComboEntry {
            keys,
            output: output.into(),
            layer: None,
        });
    }

    progress("Reading AK820 RGB state…")?;
    let (rgb_version, max_brightness) = dev_conn
        .get_vialrgb_info()
        .map_err(|e| format!("VialRGB info read failed: {e:#}"))?;
    if rgb_version != 1 || max_brightness != 160 {
        return Err(format!(
            "Unsupported AK820 VialRGB info (version {rgb_version}, max brightness {max_brightness}); settings remain read-only."
        ));
    }
    let (effect, speed, hue, saturation, brightness) = dev_conn
        .get_vialrgb_mode()
        .map_err(|e| format!("VialRGB mode read failed: {e:#}"))?;
    let supported_effects = dev_conn
        .get_vialrgb_supported_effects()
        .map_err(|e| format!("VialRGB mode list read failed: {e:#}"))?;
    let expected_rgb_modes: Vec<u16> = image.rgb_modes().iter().map(|&mode| mode as u16).collect();
    if supported_effects != expected_rgb_modes {
        return Err(format!(
            "AK820 VialRGB modes mismatch: expected {expected_rgb_modes:?}, got {supported_effects:?}; settings remain read-only."
        ));
    }
    if !supported_effects.contains(&effect) || brightness > max_brightness {
        return Err("AK820 VialRGB state is outside the verified capability range; settings remain read-only.".to_owned());
    }
    if image.profile().is_some() && image.rgb_modes().contains(&1) {
        progress("Checking AK820 per-key RGB mapping…")?;
        dev_conn
            .ak820_validate_led_map()
            .map_err(|e| format!("AK820 LED mapping: {e:#}"))?;
    }
    let rgb_settings = RgbSettingsState {
        supported: true,
        kind: RgbSupportKind::VialRgb,
        effect,
        brightness,
        speed,
        hue,
        saturation,
        max_brightness,
        supported_effects,
        last_enabled_effect: if effect == 0 { 2 } else { effect },
    };
    let saved_header = dev_conn
        .ak820_read_rom(0xC700, 40)
        .map_err(|e| format!("AK820 saved settings read failed: {e:#}"))?;
    if saved_header[..2] != [0xE6, 0xFE]
        || saved_header[37..40] != image.schema().to_le_bytes()[..3]
    {
        return Err("AK820 saved settings have an unknown schema".to_owned());
    }
    let saved_mode = image
        .saved_rgb_mode(saved_header[24] >> 2)
        .ok_or("AK820 saved RGB mode is invalid")? as u16;
    let mut rgb_settings = rgb_settings;
    if effect == 0 {
        rgb_settings.last_enabled_effect = saved_mode;
    }

    let mut tap_dance_entries = Vec::new();
    let mut key_override_entries = Vec::new();
    let mut combo_term = Some(40);
    let mut auto_shift_timeout = None;
    let mut tap_hold_settings = TapHoldSettingsState::default();
    let mut grave_escape_settings = GraveEscapeSettingsState::default();
    let mut supported_qmk_settings = Vec::new();
    if image.profile().is_some() {
        progress("Reading and reconciling AK820 module storage…")?;
        let nvm = dev_conn
            .ak820_read_rom(0xC700, 0x2800)
            .map_err(|e| format!("Complete NVM read failed: {e:#}"))?;
        let metadata = crate::ak820_settings_model::validate_nvm(&nvm, image.schema())?;
        if nvm[..40] != saved_header
            || nvm[0x40..0x640]
                != keymap
                    .iter()
                    .flat_map(|key| key.to_be_bytes())
                    .collect::<Vec<_>>()
        {
            return Err("AK820 keymap or saved header differs from its NVM snapshot".into());
        }
        for layer in 0..layer_count {
            for (index, encoder) in layout.encoders.iter().enumerate() {
                let at = 0x640 + layer * 4 + (1 - encoder.direction as usize) * 2;
                if u16::from_be_bytes([nvm[at], nvm[at + 1]]) != layout.encoder_layers[layer][index]
                {
                    return Err("AK820 encoder getter differs from NVM".into());
                }
            }
        }
        if metadata.published_macro_length != macro_buffer.len()
            || nvm[metadata.macro_offset..metadata.macro_offset + metadata.published_macro_length]
                != macro_buffer
        {
            return Err("AK820 macro getter differs from NVM".into());
        }
        // Derivation checks the saved Combo marker/version before comparing
        // its exposed wire fields. Reserved storage bytes remain untouched.
        crate::ak820_settings_model::settings_from_nvm(
            &nvm,
            &[effect as u8, 0, speed, hue, saturation, brightness],
            true,
        )?;
        for (index, combo) in combo_entries.iter().enumerate() {
            let raw = &nvm[0x658 + index * 10..0x662 + index * 10];
            let keys = [
                u16::from_le_bytes([raw[0], raw[1]]),
                u16::from_le_bytes([raw[2], raw[3]]),
                0,
                0,
            ];
            if keys != combo.keys
                || u16::from_le_bytes([raw[8], raw[9]]) != combo.output.vial_keycode()
            {
                return Err("AK820 Combo getter differs from NVM".into());
            }
        }
        let extensions = crate::ak820_typed_storage::Extensions::decode_nvm(&nvm)?;
        supported_qmk_settings = dev_conn
            .query_qmk_settings()
            .map_err(|e| format!("QMK Settings query failed: {e:#}"))?;
        if supported_qmk_settings != image.qsid_ids() {
            return Err("AK820 QMK Settings IDs differ from the image contract".into());
        }
        for &id in image.qsid_ids() {
            let actual = if id == 1 {
                dev_conn.get_qmk_setting_u8(id).map(u16::from)
            } else {
                dev_conn.get_qmk_setting_u16(id)
            }
            .map_err(|e| format!("QSID {id} read failed: {e:#}"))?;
            if actual != extensions.settings.get(id)? {
                return Err(format!(
                    "AK820 QSID {id} getter differs from stored settings"
                ));
            }
            match id {
                1 => {
                    grave_escape_settings = GraveEscapeSettingsState {
                        bits: actual as u8,
                        supported: true,
                    }
                }
                2 => combo_term = Some(actual),
                4 => auto_shift_timeout = Some(actual),
                7 => tap_hold_settings.tapping_term = actual,
                18 => tap_hold_settings.tap_code_delay = actual,
                19 => tap_hold_settings.tap_hold_caps_delay = actual,
                _ => return Err("Unknown AK820 QSID".into()),
            }
            if matches!(id, 7 | 18 | 19) {
                tap_hold_settings.supported = true;
                tap_hold_settings.set_qsid_supported(id);
                tap_hold_settings.set_qsid_loaded(id);
            }
        }
        for index in 0..dynamic.0 {
            let actual = dev_conn
                .get_tap_dance(index)
                .map_err(|e| format!("Tap Dance {index}: {e:#}"))?;
            let row = extensions.tap_dance[index as usize];
            if actual
                != (
                    row.outputs[0],
                    row.outputs[1],
                    row.outputs[2],
                    row.outputs[3],
                    row.term,
                )
            {
                return Err(format!("AK820 Tap Dance {index} getter differs from NVM"));
            }
            tap_dance_entries.push(crate::keycode_picker::TapDanceEntry {
                on_tap: row.outputs[0].into(),
                on_hold: row.outputs[1].into(),
                on_double_tap: row.outputs[2].into(),
                on_tap_hold: row.outputs[3].into(),
                tapping_term: row.term,
            });
        }
        for index in 0..dynamic.2 {
            let actual = dev_conn
                .get_key_override(index)
                .map_err(|e| format!("Key Override {index}: {e:#}"))?;
            let row = extensions.key_overrides[index as usize];
            if actual
                != (
                    row.trigger,
                    row.replacement,
                    row.layers,
                    row.mods,
                    row.negative,
                    row.suppressed,
                    row.options,
                )
            {
                return Err(format!(
                    "AK820 Key Override {index} getter differs from NVM"
                ));
            }
            key_override_entries.push(KeyOverrideEntry {
                trigger: row.trigger,
                replacement: row.replacement,
                layers: row.layers,
                trigger_mods: row.mods,
                negative_mod_mask: row.negative,
                suppressed_mods: row.suppressed,
                options: KeyOverrideOptionsState::from_bits(row.options),
            });
        }
        if dev_conn
            .ak820_read_rom(0xC700, 0x2800)
            .map_err(|e| format!("NVM stability read failed: {e:#}"))?
            != nvm
        {
            return Err("AK820 settings changed during connection; read settings again".into());
        }
    }

    progress("Checking AK820 settings storage…")?;
    dev_conn
        .ak820_storage_check()
        .map_err(|e| format!("AK820 settings storage check failed: {e:#}"))?;
    let vial_unlock_status = dev_conn
        .get_unlock_status()
        .map_err(|e| format!("AK820 unlock state read failed: {e:#}"))?;
    progress("Enabling verified AK820 settings…")?;
    dev_conn
        .ak820_enable_settings()
        .map_err(|e| format!("AK820 settings initialization failed: {e:#}"))?;

    let about_info = DeviceAboutInfo {
        manufacturer: dev.manufacturer.clone(),
        product: dev.name.clone(),
        vendor_id: dev.vendor_id,
        product_id: dev.product_id,
        path: dev.path.clone(),
        firmware_version: Some(image.label()),
        supports_application_layouts: false,
        application_layout_probe_failed: false,
        firmware_update_target: None,
        supports_battery_halves: false,
        battery_halves: None,
        via_protocol,
        vial_protocol,
        keyboard_id,
        macro_entries: macro_texts.len(),
        macro_memory_bytes: Some(macro_memory_bytes),
        supports_macro_delays: true,
        supports_macro_ext_keycodes: true,
        macro_ext_keycodes_disabled_reason: None,
        tap_dance_entries: tap_dance_entries.len(),
        combo_entries: combo_entries.len(),
        key_override_entries: key_override_entries.len(),
        alt_repeat_entries: 0,
        caps_word: true,
        layer_lock: false,
        qmk_settings: !supported_qmk_settings.is_empty(),
    };
    let layer_names_from_firmware = vec![false; layer_count];
    Ok(ConnectResult {
        device_name: dev.name.clone(),
        keyboard_id,
        vial_unlock_status: Some(vial_unlock_status),
        hid_device: Some(dev_conn),
        layout,
        layer_count,
        about_info,
        macro_texts,
        supports_macro_ext_keycodes: true,
        supports_rmk_native_key_actions: false,
        supports_universal_symbols: false,
        supports_universal_russian_letters: false,
        supports_rmk_native_combo_output: false,
        supports_rmk_native_tap_dance_actions: false,
        supports_rmk_combo_layers: false,
        macro_ext_keycodes_disabled_reason: None,
        tap_dance_entries,
        combo_entries,
        combo_term,
        auto_shift_options: Default::default(),
        auto_shift_timeout,
        mouse_keys_settings: Default::default(),
        touchpad_settings: Default::default(),
        bluetooth_settings: Default::default(),
        module_settings: Default::default(),
        tap_hold_settings,
        magic_settings: Default::default(),
        one_shot_settings: Default::default(),
        grave_escape_settings,
        layer_led_settings: Default::default(),
        rgb_settings,
        display_settings: Default::default(),
        layout_options_value: None,
        key_override_entries,
        alt_repeat_entries: Vec::new(),
        vial_features: VialFeatureSupport {
            caps_word: true,
            layer_lock: false,
            persistent_default_layer: false,
            repeat_key: true,
            alt_repeat_key: true,
        },
        layer_names_from_firmware,
        supported_qmk_settings,
        deferred_load: DeferredDeviceLoadState::complete(layer_count),
    })
}

#[cfg(test)]
#[path = "ak820_connection_tests.rs"]
mod protocol_tests;

#[cfg(test)]
#[path = "ak820_beta2_connection_tests.rs"]
mod beta2_protocol_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actual_alpha3_definition_matches_physical_contract() {
        let text = include_str!("../../assets/ak820/layout.json");
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        let layout = KeyboardLayout::from_vial_json(&json).unwrap();
        validate_ak820_layout(&layout).unwrap();
    }

    #[test]
    fn layout_contract_rejects_duplicate_coordinates() {
        let text = include_str!("../../assets/ak820/layout.json");
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        let mut layout = KeyboardLayout::from_vial_json(&json).unwrap();
        layout.keys[1].row = layout.keys[0].row;
        layout.keys[1].col = layout.keys[0].col;
        assert!(validate_ak820_layout(&layout).is_err());
    }
}
