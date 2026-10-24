use super::*;

const EXPECTED_LAYERS: usize = 6;
const EXPECTED_ROWS: usize = 16;
const EXPECTED_COLS: usize = 8;
const EXPECTED_KEYS: usize = 83;
const EXPECTED_ENCODERS: usize = 1;
const EXPECTED_MACROS: u8 = 16;
const EXPECTED_MACRO_BYTES: u16 = 8192;
const EXPECTED_COMBOS: u8 = 8;
const EXPECTED_RGB_MODES: &[u16] = &[0, 1, 2, 6, 13, 14, 31];

fn validate_ak820_layout(layout: &KeyboardLayout) -> Result<(), String> {
    if (layout.rows, layout.cols, layout.keys.len())
        != (EXPECTED_ROWS, EXPECTED_COLS, EXPECTED_KEYS)
    {
        return Err(format!(
            "AK820 layout contract mismatch: expected {EXPECTED_ROWS}x{EXPECTED_COLS} matrix and {EXPECTED_KEYS} physical keys, got {}x{} and {} keys",
            layout.rows, layout.cols, layout.keys.len()
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
        return Err(format!("AK820 layout must describe one encoder with two directions; definition has {} encoder slots", layout.encoders.len()));
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
    progress("Reading AK820 protocol identity…")?;
    let via_protocol = dev_conn
        .get_protocol_version()
        .map_err(|e| format!("VIA protocol read failed: {e:#}"))?;
    let (vial_protocol, keyboard_id) = dev_conn
        .get_keyboard_id()
        .map_err(|e| format!("Vial identity read failed: {e:#}"))?;
    if via_protocol != 9 || vial_protocol != 6 || keyboard_id != crate::ak820::VIAL_UID {
        return Err(format!("Unsupported AK820 protocol identity (VIA {via_protocol}, Vial {vial_protocol}, UID {keyboard_id:016X}); settings remain read-only."));
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
        || dynamic != (0, EXPECTED_COMBOS, 0, 0, 1)
    {
        return Err(format!("AK820 capacity mismatch (layers {layer_count}, macros {macro_count}/{macro_memory_bytes} bytes, dynamic {dynamic:?}); settings remain read-only."));
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
            layout.layers[layer][key_index] = keymap[index].into();
        }
    }

    progress("Reading all AK820 encoder mappings…")?;
    layout.encoder_layers = vec![vec![0; layout.encoders.len()]; layer_count];
    for layer in 0..layer_count {
        let (ccw, cw) = dev_conn
            .get_encoder(layer as u8, 0)
            .map_err(|e| format!("Encoder read failed for layer {layer}: {e:#}"))?;
        for (index, encoder) in layout.encoders.iter().enumerate() {
            layout.encoder_layers[layer][index] = if encoder.direction == 0 { ccw } else { cw };
        }
    }

    progress("Reading complete AK820 macro buffer…")?;
    let macro_buffer = dev_conn
        .get_macro_buffer(macro_memory_bytes, macro_count)
        .map_err(|e| format!("Complete macro buffer read failed: {e:#}"))?;
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
        if !crate::ak820::valid_combo(keys, output) {
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
        return Err(format!("Unsupported AK820 VialRGB info (version {rgb_version}, max brightness {max_brightness}); settings remain read-only."));
    }
    let (effect, speed, hue, saturation, brightness) = dev_conn
        .get_vialrgb_mode()
        .map_err(|e| format!("VialRGB mode read failed: {e:#}"))?;
    let supported_effects = dev_conn
        .get_vialrgb_supported_effects()
        .map_err(|e| format!("VialRGB mode list read failed: {e:#}"))?;
    if supported_effects != EXPECTED_RGB_MODES {
        return Err(format!("AK820 VialRGB modes mismatch: expected {EXPECTED_RGB_MODES:?}, got {supported_effects:?}; settings remain read-only."));
    }
    if !supported_effects.contains(&effect) || brightness > max_brightness {
        return Err("AK820 VialRGB state is outside the verified capability range; settings remain read-only.".to_owned());
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
    if saved_header[..2] != [0xE6, 0xFE] || saved_header[37..40] != [2, 0x20, 8] {
        return Err("AK820 saved settings have an unknown schema".to_owned());
    }
    let saved_mode = match saved_header[24] >> 2 {
        1 => 2,
        2 => 6,
        3 => 13,
        4 => 14,
        5 => 1,
        6 => 31,
        _ => return Err("AK820 saved RGB mode is invalid".to_owned()),
    };
    let mut rgb_settings = rgb_settings;
    if effect == 0 {
        rgb_settings.last_enabled_effect = saved_mode;
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
        firmware_version: Some("v0.6.0-alpha.3 / full".to_owned()),
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
        tap_dance_entries: 0,
        combo_entries: combo_entries.len(),
        key_override_entries: 0,
        alt_repeat_entries: 0,
        caps_word: true,
        layer_lock: false,
        qmk_settings: false,
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
        tap_dance_entries: Vec::new(),
        combo_entries,
        combo_term: Some(40),
        auto_shift_options: Default::default(),
        auto_shift_timeout: None,
        mouse_keys_settings: Default::default(),
        touchpad_settings: Default::default(),
        bluetooth_settings: Default::default(),
        module_settings: Default::default(),
        tap_hold_settings: Default::default(),
        magic_settings: Default::default(),
        one_shot_settings: Default::default(),
        grave_escape_settings: Default::default(),
        layer_led_settings: Default::default(),
        rgb_settings,
        display_settings: Default::default(),
        layout_options_value: None,
        key_override_entries: Vec::new(),
        alt_repeat_entries: Vec::new(),
        vial_features: VialFeatureSupport {
            caps_word: true,
            layer_lock: false,
            persistent_default_layer: false,
            repeat_key: true,
            alt_repeat_key: true,
        },
        layer_names_from_firmware,
        supported_qmk_settings: Vec::new(),
        deferred_load: DeferredDeviceLoadState::complete(layer_count),
    })
}

#[cfg(test)]
#[path = "ak820_connection_tests.rs"]
mod protocol_tests;

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
