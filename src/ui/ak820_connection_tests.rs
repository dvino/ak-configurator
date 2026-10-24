//! Offline HID protocol model for the already CODE-verified alpha.3 loader.
//! This does not model firmware hashing, USB hardware, retention, or physical input.
//! Reply shapes follow quantum/{via,vial,vialrgb}.c and AK820 config.h;
//! settings bytes model the alpha.3 NVM schema consumed by hid_ak820.rs.
use super::*;

fn device() -> crate::device::Device {
    crate::device::Device {
        name: "AK820 offline fixture".into(),
        manufacturer: "AJAZZ".into(),
        vendor_id: crate::ak820::VID,
        product_id: crate::ak820::PID,
        serial_number: String::new(),
        bus_type: "USB".into(),
        path: "offline-script".into(),
        instance_token: String::new(),
        firmware: crate::firmware::FirmwareProtocol::Vial,
    }
}

/// All replies are explicit: an unexpected request fails instead of falling
/// through to TestHidRecorder's generic keyboard defaults.
fn scripted(
    fault: impl Fn(&[u8; 32], &mut [u8; 32]) -> Result<(), String> + Send + 'static,
) -> (crate::hid::HidDevice, crate::hid::TestHidRecorder) {
    let (hid, log) = crate::hid::HidDevice::test_verified_ak820_device();
    let mut compressed = Vec::new();
    lzma_rs::xz_compress(
        &mut &include_bytes!("../../assets/ak820/layout.json")[..],
        &mut compressed,
    )
    .unwrap();
    let demo: serde_json::Value =
        serde_json::from_str(include_str!("../../assets/ak820/demo-keymap.json")).unwrap();
    let base: Vec<u16> = demo["base_layer"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as u16)
        .collect();
    assert_eq!(base.len(), 128);
    let keymap: Vec<u8> = (0..6)
        .flat_map(|layer| {
            base.iter()
                .map(move |&key| if layer == 0 { key } else { 1 })
                .flat_map(u16::to_be_bytes)
        })
        .collect();
    log.respond_by(move |p| {
        let mut r = [0u8; 32];
        match (p[0], p[1]) {
            (1, _) => {
                r[..3].copy_from_slice(&[1, 0, 9]);
            }
            (0xFE, 0) => {
                r[..4].copy_from_slice(&6u32.to_le_bytes());
                r[4..12].copy_from_slice(&crate::ak820::VIAL_UID.to_le_bytes());
            }
            (0xFE, 1) => r[..4].copy_from_slice(&(compressed.len() as u32).to_le_bytes()),
            (0xFE, 2) => {
                let at = u32::from_le_bytes(p[2..6].try_into().unwrap()) as usize * 32;
                if at >= compressed.len() {
                    return Some(Err("definition block outside fixture".into()));
                }
                let n = (compressed.len() - at).min(32);
                r[..n].copy_from_slice(&compressed[at..at + n]);
            }
            (0x11, _) => {
                r[0] = 0x11;
                r[1] = 6;
            }
            (0x0C, _) => {
                r[0] = 0x0C;
                r[1] = 16;
            }
            (0x0D, _) => {
                r[..3].copy_from_slice(&[0x0D, 0x20, 0]);
            }
            (0xFE, 13) if p[2] == 0 => {
                r[1] = 8;
                r[31] = 1;
            }
            (0x12, _) => {
                let at = u16::from_be_bytes([p[1], p[2]]) as usize;
                let n = p[3] as usize;
                r[..4].copy_from_slice(&p[..4]);
                r[4..4 + n].copy_from_slice(&keymap[at..at + n]);
            }
            (0xFE, 3) => {
                r[..2].copy_from_slice(&(0x80u16 + p[2] as u16).to_be_bytes());
                r[2..4].copy_from_slice(&(0x90u16 + p[2] as u16).to_be_bytes());
            }
            (0x0E, _) => {
                r[..4].copy_from_slice(&p[..4]);
            } // sixteen empty, terminated macros
            (0xFE, 13) if p[2] == 3 => {} // empty combo record, successful status
            (8, 0x40) => {
                r[..5].copy_from_slice(&[8, 0x40, 1, 0, 160]);
            }
            (8, 0x41) => {
                r[..8].copy_from_slice(&[8, 0x41, 2, 0, 42, 80, 200, 100]);
            }
            (8, 0x42) => {
                r.fill(0xFF);
                r[..2].copy_from_slice(&[8, 0x42]);
                for (i, effect) in [1u16, 2, 6, 13, 14, 31].into_iter().enumerate() {
                    r[2 + i * 2..4 + i * 2].copy_from_slice(&effect.to_le_bytes());
                }
            }
            (0xFD, 0) => {
                r[..5].copy_from_slice(&p[..5]);
                let at = u16::from_be_bytes([p[2], p[3]]) as usize;
                for i in 0..p[4] as usize {
                    let address = at + i;
                    r[5 + i] = match address {
                        0xC700 => 0xE6,
                        0xC701 => 0xFE,
                        0xC718 => 5,
                        0xC725 => 2,
                        0xC726 => 0x20,
                        0xC727 => 8,
                        0xCEF1 => 16,
                        _ => 0,
                    };
                }
            }
            (0xFD, 1) => r[..4].copy_from_slice(&[0xFD, 1, 1, 0xA8]),
            (0xFE, 5) => {
                r.fill(0xFF);
                r[..6].copy_from_slice(&[0, 0, 0, 0, 0, 5]);
            }
            _ => return Some(Err(format!("unexpected offline request: {p:02x?}"))),
        }
        Some(fault(p, &mut r).map(|()| r))
    });
    (hid, log)
}

fn assert_read_only(log: &crate::hid::TestHidRecorder) {
    let requests = log.requests();
    assert!(!requests.is_empty());
    assert!(requests.len() < 400, "connection must remain bounded");
    for p in requests {
        assert!(
            matches!(p[0], 1 | 0x0C | 0x0D | 0x0E | 0x11 | 0x12)
                || (p[0] == 8 && matches!(p[1], 0x40..=0x42))
                || (p[0] == 0xFD && p[1] <= 1)
                || (p[0] == 0xFE
                    && (matches!(p[1], 0..=3 | 5) || (p[1] == 13 && matches!(p[2], 0 | 3)))),
            "non-read request: {p:02x?}"
        );
    }
}

#[test]
fn complete_verified_connection_reads_actual_definition_and_all_settings() {
    let (hid, log) = scripted(|_, _| Ok(()));
    assert!(!hid.ak820_editing_ready());
    let result = load_verified_ak820(&device(), hid, |_| Ok(())).unwrap();
    assert_eq!(result.layer_count, 6);
    assert_eq!(result.layout.keys.len(), 83);
    assert_eq!(result.layout.layers.len(), 6);
    assert!(result.layout.layers.iter().all(|layer| layer.len() == 83));
    let unique: std::collections::HashSet<_> = result
        .layout
        .keys
        .iter()
        .map(|key| (key.row, key.col))
        .collect();
    assert_eq!(unique.len(), 83);
    for layer in 0..6 {
        for (i, encoder) in result.layout.encoders.iter().enumerate() {
            assert_eq!(
                result.layout.encoder_layers[layer][i],
                if encoder.direction == 0 {
                    0x80 + layer as u16
                } else {
                    0x90 + layer as u16
                }
            );
        }
    }
    let demo: serde_json::Value =
        serde_json::from_str(include_str!("../../assets/ak820/demo-keymap.json")).unwrap();
    for (i, key) in result.layout.keys.iter().enumerate() {
        let expected = demo["base_layer"][key.row as usize * 8 + key.col as usize]
            .as_u64()
            .unwrap() as u16;
        assert_eq!(result.layout.layers[0][i].vial_keycode(), expected);
        assert_eq!(result.layout.layers[5][i].vial_keycode(), 1);
    }
    assert_eq!(result.macro_texts.len(), 16);
    assert!(result.macro_texts.iter().all(|m| m.is_empty()));
    assert_eq!(result.combo_entries.len(), 8);
    assert_eq!(
        (
            result.rgb_settings.effect,
            result.rgb_settings.max_brightness,
            result.rgb_settings.brightness
        ),
        (2, 160, 100)
    );
    assert_eq!(result.rgb_settings.supported_effects, EXPECTED_RGB_MODES);
    assert!(result.vial_features.repeat_key && result.vial_features.alt_repeat_key);
    assert!(!result.vial_features.persistent_default_layer);
    assert!(!result.about_info.qmk_settings);
    assert!(!result.mouse_keys_settings.supported);
    assert!(result.layout.live_features.is_empty());
    assert!(!result.layout.live_features.extended_host_protocol);
    assert_eq!(
        (
            result.rgb_settings.speed,
            result.rgb_settings.hue,
            result.rgb_settings.saturation
        ),
        (42, 80, 200)
    );
    assert_eq!(result.about_info.macro_memory_bytes, Some(8192));
    assert!(!result.supports_rmk_native_key_actions);
    assert!(result.supported_qmk_settings.is_empty() && result.tap_dance_entries.is_empty());
    assert!(result.hid_device.as_ref().unwrap().ak820_editing_ready());
    assert_read_only(&log);
}

#[test]
fn required_read_timeouts_never_return_a_partial_success() {
    for prefix in [
        vec![0x12],
        vec![0x0E],
        vec![0xFE, 13, 3],
        vec![0xFE, 3],
        vec![8, 0x41],
    ] {
        let target = prefix.clone();
        let (hid, log) = scripted(move |p, _| {
            if p.starts_with(&target) {
                Err("offline injected timeout".into())
            } else {
                Ok(())
            }
        });
        assert!(
            load_verified_ak820(&device(), hid, |_| Ok(())).is_err(),
            "prefix {prefix:x?}"
        );
        assert_read_only(&log);
        assert_eq!(
            log.requests()
                .iter()
                .filter(|p| p.starts_with(&prefix))
                .count(),
            1,
            "failed reads must not retry"
        );
    }
}

#[test]
fn malformed_required_payloads_and_wrong_capacities_are_rejected() {
    // Encoder replies have no status/length marker in Vial: transport failure
    // is covered above; arbitrary four-byte encoder data cannot signal truncation.
    for case in 0..10 {
        let (hid, log) = scripted(move |p, r| {
            match case {
                0 if p[0] == 0x12 => r[3] ^= 1,
                1 if p[0] == 0x0E => r[4] = 0xFF,
                2 if p[..3] == [0xFE, 13, 3] => r[0] = 1,
                3 if p[..2] == [8, 0x41] => r[7] = 161,
                4 if p[0] == 0x11 => r[1] = 5,
                5 if p[0] == 0x0C => r[1] = 15,
                6 if p[0] == 0x0D => r[2] = 1,
                7 if p[..3] == [0xFE, 13, 0] => r[1] = 7,
                8 if p[..2] == [0xFD, 1] => r[4] = 1,
                9 if p[..2] == [8, 0x40] => r[4] = 159,
                _ => {}
            }
            Ok(())
        });
        assert!(
            load_verified_ak820(&device(), hid, |_| Ok(())).is_err(),
            "case {case}"
        );
        assert_read_only(&log);
    }
}
