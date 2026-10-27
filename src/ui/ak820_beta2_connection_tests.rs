//! Offline checks for beta.2 connection identity. CODE admission is supplied
//! by a synthetic verified session; no packed-image hash is proven here.
use super::*;

fn test_device() -> crate::device::Device {
    crate::device::Device {
        name: "synthetic beta.2".into(),
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

fn profiles() -> [crate::app::ak820_beta1_profiles::ProfileId; 10] {
    use crate::app::ak820_beta1_profiles::ProfileId::*;
    [
        Main, Game, Mmo, Typing, Code, Pointer, Onehand, Creative, Shortcuts, Multios,
    ]
}

fn beta2_scripted(
    profile: crate::app::ak820_beta1_profiles::ProfileId,
    alter: Option<&'static str>,
) -> (crate::hid::HidDevice, crate::hid::TestHidRecorder) {
    use crate::ak820_rgb::SLOT_MAP;
    let (hid, log) = crate::hid::HidDevice::test_beta2_ak820_device(profile, false);
    let image = hid.ak820_image().unwrap();
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
    let mut nvm = vec![0; 0x2800];
    nvm[..2].copy_from_slice(&[0xE6, 0xFE]);
    nvm[37..40].copy_from_slice(&image.schema().to_le_bytes()[..3]);
    for layer in 0..6 {
        for (i, &code) in base.iter().enumerate() {
            let row = i / 8;
            let col = i % 8;
            let at = 0x40 + layer * 256 + row * 16 + col * 2;
            nvm[at..at + 2].copy_from_slice(&(if layer == 0 { code } else { 1 }).to_be_bytes());
        }
        for dir in 0..2 {
            let at = 0x640 + layer * 4 + (1 - dir) * 2;
            nvm[at..at + 2].copy_from_slice(&(0x80 + layer as u16).to_be_bytes());
        }
    }
    nvm[0x7F1..0x7F3].copy_from_slice(&16u16.to_le_bytes());
    nvm[0x18] = 5;
    nvm[0x19] = 80;
    nvm[0x1A] = 200;
    nvm[0x1B] = 100;
    nvm[0x1C] = 42;
    let ids = image.qsid_ids().to_vec();
    let modes: Vec<u8> = image.rgb_modes().to_vec();
    let counts = image.dynamic_counts();
    let mut nvm_passes = 0usize;
    log.respond_by(move |p| {
        let mut r = [0u8; 32];
        match (p[0], p[1]) {
            (1, _) => r[..3].copy_from_slice(&[1, 0, 9]),
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
            (0x0D, _) => r[..3].copy_from_slice(&[0x0D, 0x20, 0]),
            (0xFE, 13) if p[2] == 0 => {
                r[..4].copy_from_slice(&[counts.0, counts.1, counts.2, 0]);
                r[31] = counts.4;
            }
            (0x12, _) => {
                let at = u16::from_be_bytes([p[1], p[2]]) as usize;
                let n = p[3] as usize;
                r[..4].copy_from_slice(&p[..4]);
                r[4..4 + n].copy_from_slice(&nvm[0x40 + at..0x40 + at + n]);
            }
            (0xFE, 3) => {
                let layer = p[2] as usize;
                r[..4].copy_from_slice(&nvm[0x640 + layer * 4..0x644 + layer * 4]);
            }
            (0x0E, _) => {
                r[..4].copy_from_slice(&p[..4]);
            }
            (0xFE, 13) if p[2] == 3 => {}
            (0xFE, 13) if p[2] == 1 => {
                r[0] = 0;
            }
            (0xFE, 13) if p[2] == 5 => {
                r[0] = 0;
            }
            (0xFE, 13) if p[2] == 2 || p[2] == 6 => {
                return Some(Err(
                    "read-only connection issued a typed setter opcode".into()
                ));
            }
            (0xFE, 9) => {
                r.fill(0xFF);
                let mut q = ids.iter().copied().chain([0xFFFF]).collect::<Vec<_>>();
                q.retain(|id| *id > u16::from_le_bytes([p[2], p[3]]));
                for (i, id) in q.iter().take(16).enumerate() {
                    r[i * 2..i * 2 + 2].copy_from_slice(&id.to_le_bytes());
                }
            }
            (0xFE, 10) => {
                let id = u16::from_le_bytes([p[2], p[3]]);
                r[0] = 0;
                let storage = crate::ak820_typed_storage::Extensions::decode_nvm(&nvm).unwrap();
                let v = storage.settings.get(id).unwrap();
                r[1..3].copy_from_slice(&v.to_le_bytes());
            }
            (8, 0x40) => r[..5].copy_from_slice(&[8, 0x40, 1, 0, 160]),
            (8, 0x41) => r[..8].copy_from_slice(&[8, 0x41, 2, 0, 42, 80, 200, 100]),
            (8, 0x42) => {
                r.fill(0xFF);
                r[..2].copy_from_slice(&[8, 0x42]);
                for (i, e) in modes.iter().filter(|&&e| e != 0).enumerate() {
                    r[2 + i * 2..4 + i * 2].copy_from_slice(&(*e as u16).to_le_bytes());
                }
            }
            (8, 0x43) => r[..4].copy_from_slice(&[8, 0x43, 82, 0]),
            (8, 0x44) => {
                let idx = p[2] as usize;
                let slot = SLOT_MAP[idx];
                r[..7].copy_from_slice(&[8, 0x44, p[2], p[3], 4, slot >> 3, slot & 7]);
            }
            (0xFD, 0) => {
                let at = u16::from_be_bytes([p[2], p[3]]) as usize;
                let n = p[4] as usize;
                r[..5].copy_from_slice(&p[..5]);
                if at == 0xC700 {
                    nvm_passes += 1;
                }
                r[5..5 + n].copy_from_slice(&nvm[at - 0xC700..at - 0xC700 + n]);
                if alter == Some("nvm") && nvm_passes > 2 && at == 0xC700 {
                    r[5] ^= 1;
                }
            }
            (0xFD, 1) => r[..4].copy_from_slice(&[0xFD, 1, 1, 0xA8]),
            (0xFE, 5) => {
                r.fill(0xFF);
                r[..6].copy_from_slice(&[0, 0, 0, 0, 0, 5]);
            }
            _ => return Some(Err(format!("unexpected beta2 request: {p:02x?}"))),
        }
        if alter == Some("qcount") && p[..2] == [0xFE, 13] && p[2] == 0 {
            r[0] = r[0].wrapping_add(1);
        }
        if alter == Some("keymap") && p[0] == 0x12 {
            r[4] ^= 1;
        }
        if alter == Some("typed") && p[..2] == [0xFE, 10] {
            r[1] ^= 1;
        }
        if alter == Some("definition") && p[..2] == [0xFE, 2] {
            r[0] ^= 1;
        }
        if alter == Some("qsid") && p[..2] == [0xFE, 9] {
            r[..2].copy_from_slice(&0x7777u16.to_le_bytes());
        }
        Some(Ok(r))
    });
    (hid, log)
}

#[test]
fn complete_beta2_live_read_contract_succeeds_for_all_ten_synthetic_profiles() {
    for profile in profiles() {
        let (hid, log) = beta2_scripted(profile, None);
        let result = load_verified_ak820(&test_device(), hid, |_| Ok(()))
            .unwrap_or_else(|e| panic!("{}: {e}", profile.as_str()));
        assert_eq!(result.layer_count, 6);
        assert_eq!(result.layout.layers.len(), 6);
        assert_eq!(
            result.supported_qmk_settings,
            crate::ak820::ImageContract::Beta2(profile).qsid_ids()
        );
        assert!(result.hid_device.as_ref().unwrap().ak820_editing_ready());
        assert!(
            log.requests()
                .iter()
                .all(|p| !matches!(p[0], 0x05 | 0x07 | 0x0F))
        );
        assert!(
            log.requests()
                .iter()
                .all(|p| !(p[0] == 0xFE && matches!(p[1], 6 | 7 | 8)))
        );
    }
}

#[test]
fn beta2_connection_rejects_definition_keymap_typed_count_and_stability_mismatches() {
    for fault in ["definition", "keymap", "typed", "qcount", "qsid", "nvm"] {
        let profile = crate::app::ak820_beta1_profiles::ProfileId::Code;
        let (hid, log) = beta2_scripted(profile, Some(fault));
        assert!(
            load_verified_ak820(&test_device(), hid, |_| Ok(())).is_err(),
            "{fault}"
        );
        assert!(
            log.requests()
                .iter()
                .all(|p| !matches!(p[0], 0x05 | 0x07 | 0x0F))
        );
        assert!(
            log.requests()
                .iter()
                .all(|p| !(p[0] == 0xFE && matches!(p[1], 6 | 7 | 8)))
        );
    }
}

#[test]
fn beta2_public_definition_matches_each_profile_digest() {
    let json: serde_json::Value =
        serde_json::from_str(include_str!("../../assets/ak820/layout.json")).unwrap();
    let mut canonical = serde_json::to_vec_pretty(&json).unwrap();
    canonical.push(b'\n');
    let digest = crate::ak820_settings_backup::sha256(&canonical);
    for profile in [
        crate::app::ak820_beta1_profiles::ProfileId::Main,
        crate::app::ak820_beta1_profiles::ProfileId::Game,
        crate::app::ak820_beta1_profiles::ProfileId::Mmo,
        crate::app::ak820_beta1_profiles::ProfileId::Typing,
        crate::app::ak820_beta1_profiles::ProfileId::Code,
        crate::app::ak820_beta1_profiles::ProfileId::Pointer,
        crate::app::ak820_beta1_profiles::ProfileId::Onehand,
        crate::app::ak820_beta1_profiles::ProfileId::Creative,
        crate::app::ak820_beta1_profiles::ProfileId::Shortcuts,
        crate::app::ak820_beta1_profiles::ProfileId::Multios,
    ] {
        let identity = crate::app::ak820_beta1_profiles::matrix_profile(profile)
            .settings_identity
            .as_ref()
            .unwrap();
        assert_eq!(
            digest,
            identity.definition_sha256,
            "profile {}",
            profile.as_str()
        );
    }
}

#[test]
fn beta2_nvm_fixture_has_schema03_and_expected_typed_defaults() {
    let nvm = {
        let mut nvm = vec![0; 0x2800];
        nvm[..2].copy_from_slice(&[0xE6, 0xFE]);
        nvm[37..40].copy_from_slice(&[3, 0x20, 8]);
        nvm[0x7f1..0x7f3].copy_from_slice(&16u16.to_le_bytes());
        nvm
    };
    crate::ak820_settings_model::validate_nvm(&nvm, 0x082003).unwrap();
    let storage = crate::ak820_typed_storage::Extensions::decode_nvm(&nvm).unwrap();
    for (id, expected) in [(1, 0), (2, 40), (4, 175), (7, 200), (18, 0), (19, 80)] {
        assert_eq!(storage.settings.get(id).unwrap(), expected);
    }
}
