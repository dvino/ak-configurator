//! Scripted getter-only host checks; no positive released-ROM qualification.
use super::*;
use crate::app::ak820_beta1_profiles::ProfileId;
use std::sync::atomic::AtomicU32;

fn capture_device() -> (HidDevice, super::super::TestHidRecorder) {
    let (hid, log) = HidDevice::test_beta2_ak820_device(ProfileId::Main, true);
    log.respond_by(|p| {
        let mut r = [0; 32];
        match (p[0], p[1]) {
            (0xfd, 1) => r[..4].copy_from_slice(&[0xfd, 1, 1, 0xa8]),
            (1, _) => r[..3].copy_from_slice(&[1, 0, 9]),
            (0xfe, 0) => {
                r[..4].copy_from_slice(&6u32.to_le_bytes());
                r[4..12].copy_from_slice(&crate::ak820::VIAL_UID.to_le_bytes());
            }
            (0xfd, 0) => r[..5].copy_from_slice(&p[..5]),
            _ => return Some(Err("Unexpected capture request".into())),
        }
        Some(Ok(r))
    });
    (hid, log)
}
#[test]
fn capture_cancelled_before_first_packet_sends_nothing() {
    let (hid, log) = capture_device();
    assert!(hid
        .ak820_capture_backup(&AtomicBool::new(true), &AtomicU32::new(0))
        .is_err());
    assert!(log.requests().is_empty());
    assert!(hid.ak820_editing_ready());
}
#[test]
fn capture_rejects_unloaded_or_historical_session_without_packets() {
    for (hid, log) in [
        HidDevice::test_beta2_ak820_device(ProfileId::Main, false),
        HidDevice::test_ak820_device(true),
    ] {
        assert!(hid
            .ak820_capture_backup(&AtomicBool::new(false), &AtomicU32::new(0))
            .is_err());
        assert!(log.requests().is_empty());
    }
}
#[test]
fn capture_zero_rom_stops_at_production_trust_gate_using_getters_only() {
    let (hid, log) = capture_device();
    let progress = AtomicU32::new(0);
    let error = hid
        .ak820_capture_backup(&AtomicBool::new(false), &progress)
        .unwrap_err();
    assert!(error.to_string().contains("exact archive validation"));
    let requests = log.requests();
    assert_eq!(
        requests.iter().filter(|p| p[..2] == [0xfd, 0]).count(),
        65536usize.div_ceil(27)
    );
    assert!(requests
        .iter()
        .all(|p| p[0] == 1 || p[..2] == [0xfe, 0] || p[..2] == [0xfd, 0] || p[..2] == [0xfd, 1]));
    assert!(progress.load(Ordering::Acquire) < 100);
    assert!(!hid.ak820_editing_ready());
}
#[test]
fn capture_bad_storage_stops_once_without_rom_reads() {
    let (hid, log) = capture_device();
    log.respond_by(|_| Some(Ok([0; 32])));
    assert!(hid
        .ak820_capture_backup(&AtomicBool::new(false), &AtomicU32::new(0))
        .is_err());
    assert_eq!(log.requests().len(), 1);
}
#[test]
fn capture_rom_address_mismatch_stops_without_retry() {
    let (hid, log) = capture_device();
    log.respond_by(|p| {
        let mut r = [0; 32];
        match (p[0], p[1]) {
            (0xfd, 1) => r[..4].copy_from_slice(&[0xfd, 1, 1, 0xa8]),
            (1, _) => r[..3].copy_from_slice(&[1, 0, 9]),
            (0xfe, 0) => {
                r[..4].copy_from_slice(&6u32.to_le_bytes());
                r[4..12].copy_from_slice(&crate::ak820::VIAL_UID.to_le_bytes());
            }
            (0xfd, 0) => {}
            _ => return Some(Err("Unexpected request".into())),
        }
        Some(Ok(r))
    });
    assert!(hid
        .ak820_capture_backup(&AtomicBool::new(false), &AtomicU32::new(0))
        .is_err());
    assert_eq!(log.requests().len(), 4);
}

#[test]
fn capture_cancellation_between_rom_packets_never_retries_or_publishes() {
    let (hid, log) = capture_device();
    let cancel = std::sync::Arc::new(AtomicBool::new(false));
    let worker_cancel = cancel.clone();
    log.respond_by(move |p| {
        let mut r = [0; 32];
        match (p[0], p[1]) {
            (0xfd, 1) => r[..4].copy_from_slice(&[0xfd, 1, 1, 0xa8]),
            (1, _) => r[..3].copy_from_slice(&[1, 0, 9]),
            (0xfe, 0) => {
                r[..4].copy_from_slice(&6u32.to_le_bytes());
                r[4..12].copy_from_slice(&crate::ak820::VIAL_UID.to_le_bytes());
            }
            (0xfd, 0) => {
                r[..5].copy_from_slice(&p[..5]);
                worker_cancel.store(true, Ordering::Release);
            }
            _ => return Some(Err("Unexpected capture request".into())),
        }
        Some(Ok(r))
    });
    let progress = AtomicU32::new(0);
    assert!(hid.ak820_capture_backup(&cancel, &progress).is_err());
    assert_eq!(log.requests().len(), 4);
    assert!(progress.load(Ordering::Acquire) < 100);
    assert!(hid.ak820_editing_ready());
}

#[test]
fn capture_real_failure_racing_with_cancel_still_latches_session() {
    let (hid, log) = capture_device();
    let cancel = std::sync::Arc::new(AtomicBool::new(false));
    let during_read = cancel.clone();
    log.respond_by(move |_| {
        during_read.store(true, Ordering::Release);
        Some(Err("scripted transport failure".into()))
    });
    let error = hid
        .ak820_capture_backup(&cancel, &AtomicU32::new(0))
        .unwrap_err();
    assert!(error.to_string().contains("read failed"));
    assert!(!hid.ak820_editing_ready());
    assert_eq!(log.requests().len(), 1);
}

fn reconciliation_fixture() -> (Vec<u8>, serde_json::Value) {
    // Host-only durable model, intentionally no ROM/trust injection.
    let mut nvm = vec![0; 0x2800];
    nvm[..2].copy_from_slice(&[0xe6, 0xfe]);
    nvm[37..40].copy_from_slice(&[3, 0x20, 8]);
    nvm[24] = 5;
    nvm[27] = 80;
    nvm[0x6a8..0x6c0].copy_from_slice(&crate::ak820_typed_storage::QmkSettings::default().encode());
    nvm[0x7f1] = 46;
    nvm[0x800..0x81e].fill(b'x');
    nvm[0x81e..0x82e].fill(0);
    // Both directions differ, so accidental swapping cannot pass.
    nvm[0x640..0x644].copy_from_slice(&[0, 4, 0, 5]);
    let rgb = crate::ak820_settings_backup::rgb_wire_for_profile(
        &nvm[24..32],
        false,
        ProfileId::Shortcuts,
    )
    .unwrap();
    let settings = crate::ak820_settings_model::settings_from_nvm(&nvm, &rgb, true).unwrap();
    (nvm, settings)
}

fn modeled_getter(p: &[u8], nvm: &[u8]) -> [u8; 32] {
    let contract = crate::app::ak820_beta1_profiles::matrix_profile(ProfileId::Shortcuts);
    let extensions = crate::ak820_typed_storage::Extensions::decode_nvm(nvm).unwrap();
    let mut r = [0; 32];
    match (p[0], p.get(1).copied().unwrap_or(0)) {
        (4, _) => {
            r[..4].copy_from_slice(p);
            let at = 0x40 + p[1] as usize * 256 + p[2] as usize * 16 + p[3] as usize * 2;
            r[4..6].copy_from_slice(&nvm[at..at + 2]);
        }
        (0xfe, 3) => {
            let at = 0x640 + p[2] as usize * 4;
            r[..4].copy_from_slice(&nvm[at..at + 4]);
        }
        (2, 2) => {
            r[..2].copy_from_slice(p);
            r[5] = nvm[40];
        }
        (0xfe, 13) if p[2] == 0 => {
            r[0] = contract.capabilities.dynamic_entries.tap_dance;
            r[1] = contract.capabilities.dynamic_entries.combos;
            r[2] = contract.capabilities.dynamic_entries.key_overrides;
        }
        (0xfe, 9) => {
            r.fill(0xff);
            for (i, id) in contract
                .capabilities
                .runtime_qmk_settings_ids
                .iter()
                .enumerate()
            {
                r[i * 2..i * 2 + 2].copy_from_slice(&id.to_le_bytes());
            }
        }
        (0x0c, _) => {
            r[0] = 0x0c;
            r[1] = 16;
        }
        (0x0d, _) => {
            r[0] = 0x0d;
            r[1] = 0x20;
        }
        (0x0e, _) => {
            r[..4].copy_from_slice(p);
            let at = 0x800 + u16::from_be_bytes([p[1], p[2]]) as usize;
            r[4..4 + p[3] as usize].copy_from_slice(&nvm[at..at + p[3] as usize]);
        }
        (0xfe, 10) => {
            let value = extensions
                .settings
                .get(u16::from_le_bytes([p[2], p[3]]))
                .unwrap();
            r[1..3].copy_from_slice(&value.to_le_bytes());
        }
        (0xfe, 13) if p[2] == 1 || p[2] == 5 => {
            let raw = if p[2] == 1 {
                extensions.tap_dance[p[3] as usize].encode()
            } else {
                extensions.key_overrides[p[3] as usize].encode()
            }
            .unwrap();
            r[1..11].copy_from_slice(&raw);
        }
        (0xfe, 13) if p[2] == 3 => {} // Fixture's empty Combo getter.
        (8, 0x41) => {
            r[..2].copy_from_slice(p);
            r[2..8].copy_from_slice(
                &crate::ak820_settings_backup::rgb_wire_for_profile(
                    &nvm[24..32],
                    false,
                    ProfileId::Shortcuts,
                )
                .unwrap(),
            );
        }
        _ => panic!("unexpected modeled getter"),
    }
    r
}

#[test]
fn capture_reconciles_all_modeled_durable_getters_and_allows_volatile_rgb_difference() {
    let (nvm, settings) = reconciliation_fixture();
    let mut requests = Vec::new();
    assert!(capture_reconcile_getters(
        ProfileId::Shortcuts,
        ImageContract::Beta2(ProfileId::Shortcuts),
        &nvm,
        &settings,
        &mut |p| {
            requests.push(p.to_vec());
            Ok(modeled_getter(p, &nvm))
        }
    )
    .unwrap());
    assert_eq!(requests.iter().filter(|p| p[0] == 4).count(), 498);
    assert_eq!(requests.iter().filter(|p| p[0] == 0x0e).count(), 2);
    assert!(!capture_reconcile_getters(
        ProfileId::Shortcuts,
        ImageContract::Beta2(ProfileId::Shortcuts),
        &nvm,
        &settings,
        &mut |p| {
            let mut r = modeled_getter(p, &nvm);
            if p == [8, 0x41] {
                r[7] = 81;
            }
            Ok(r)
        }
    )
    .unwrap());
}

#[test]
fn capture_modeled_getter_mismatches_stop_in_each_durable_section() {
    let (nvm, settings) = reconciliation_fixture();
    // Corrupt distinct fields/packets to check every reconciliation section.
    for target in 0..13 {
        let mut hit = false;
        let result = capture_reconcile_getters(
            ProfileId::Shortcuts,
            ImageContract::Beta2(ProfileId::Shortcuts),
            &nvm,
            &settings,
            &mut |p| {
                let mut r = modeled_getter(p, &nvm);
                let field = match target {
                    0 if p[0] == 4 => Some(4),
                    1 if p.starts_with(&[0xfe, 3]) => Some(0),
                    2 if p.starts_with(&[0xfe, 3]) => Some(2),
                    3 if p == [2, 2] => Some(5),
                    4 if p == [0xfe, 13, 0] => Some(1),
                    5 if p == [0xfe, 9, 0, 0] => Some(0),
                    6 if p[0] == 0x0c => Some(1),
                    7 if p[0] == 0x0e && p[2] == 28 => Some(4),
                    8 if p.starts_with(&[0xfe, 10]) => Some(1),
                    9 if p.starts_with(&[0xfe, 13, 1]) => Some(1),
                    10 if p.starts_with(&[0xfe, 13, 5]) => Some(1),
                    11 if p.starts_with(&[0xfe, 13, 3]) => Some(1),
                    12 if p == [8, 0x41] => Some(3),
                    _ => None,
                };
                if let Some(field) = field {
                    r[field] ^= 1;
                    hit = true;
                }
                Ok(r)
            },
        );
        assert!(hit, "modeled section not exercised: {target}");
        assert!(result.is_err(), "mismatch accepted: {target}");
    }
}
