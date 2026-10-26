//! Synthetic beta.2 typed-setting transaction tests. These exercise the
//! parent-side contract only; they are not device or durability evidence.
use super::*;

fn profile() -> crate::app::ak820_beta1_profiles::ProfileId {
    crate::app::ak820_beta1_profiles::ProfileId::Main
}

fn status() -> [u8; 32] {
    let mut r = [0; 32];
    r[..4].copy_from_slice(&[0xFD, 1, 1, 0xA8]);
    r
}

fn qsid_transaction(
    initial: Vec<u8>,
    fault: impl Fn(&[u8; 32], &mut [u8; 32]) -> Result<(), String> + Send + 'static,
) -> (HidDevice, super::super::TestHidRecorder) {
    let (hid, log) = HidDevice::test_beta2_ak820_device(profile(), true);
    let mut nvm = initial;
    log.respond_by(move |p| {
        let mut r = [0; 32];
        match (p[0], p[1]) {
            (0xFD, 1) => r = status(),
            (0xFD, 0) => {
                let at = u16::from_be_bytes([p[2], p[3]]) as usize;
                let n = p[4] as usize;
                if at < 0xC700 || at + n > 0xEF00 {
                    return Some(Err("NVM bounds".into()));
                }
                r[..5].copy_from_slice(&p[..5]);
                r[5..5 + n].copy_from_slice(&nvm[at - 0xC700..at - 0xC700 + n]);
            }
            (0xFE, 11) => {
                r = *p;
                r[0] = 0;
                let id = u16::from_le_bytes([p[2], p[3]]);
                let value = u16::from_le_bytes([p[4], p[5]]);
                let rec = &mut nvm[0x6A8..0x6C0];
                if rec.iter().all(|b| *b == 0) {
                    rec[..5].copy_from_slice(b"QS\x01\x0b\x3f");
                    rec[6..8].copy_from_slice(&40u16.to_le_bytes());
                    rec[14..16].copy_from_slice(&175u16.to_le_bytes());
                    rec[8..10].copy_from_slice(&200u16.to_le_bytes());
                    rec[10..12].copy_from_slice(&0u16.to_le_bytes());
                    rec[12..14].copy_from_slice(&80u16.to_le_bytes());
                }
                let at = match id {
                    1 => (5, 1),
                    2 => (6, 2),
                    4 => (14, 2),
                    7 => (8, 2),
                    18 => (10, 2),
                    19 => (12, 2),
                    _ => return Some(Err("unexpected QSID setter".into())),
                };
                rec[at.0..at.0 + at.1].copy_from_slice(&value.to_le_bytes()[..at.1]);
                let mut c = 0xffffu16;
                for &b in &rec[..22] {
                    c ^= (b as u16) << 8;
                    for _ in 0..8 {
                        c = if c & 0x8000 != 0 {
                            (c << 1) ^ 0x1021
                        } else {
                            c << 1
                        };
                    }
                }
                rec[22..24].copy_from_slice(&c.to_le_bytes());
            }
            (0xFE, 10) => {
                let id = u16::from_le_bytes([p[2], p[3]]);
                r[0] = 0;
                let at = match id {
                    1 => 0x6AD,
                    2 => 0x6AE,
                    4 => 0x6B6,
                    7 => 0x6B0,
                    18 => 0x6B2,
                    19 => 0x6B4,
                    _ => return Some(Err("unexpected QSID".into())),
                };
                let width = if id == 1 { 1 } else { 2 };
                if nvm[0x6A8..0x6C0].iter().all(|b| *b == 0) {
                    let v: u16 = match id {
                        1 => 0,
                        2 => 40,
                        4 => 175,
                        7 => 200,
                        18 => 0,
                        19 => 80,
                        _ => 0,
                    };
                    r[1..1 + width].copy_from_slice(&v.to_le_bytes()[..width]);
                } else {
                    r[1..1 + width].copy_from_slice(&nvm[at..at + width]);
                }
            }
            _ => return Some(Err(format!("unexpected request {p:02x?}"))),
        }
        Some(fault(p, &mut r).map(|()| r))
    });
    (hid, log)
}

fn blank_nvm() -> Vec<u8> {
    let mut n = vec![0; 0x2800];
    n[..2].copy_from_slice(&[0xE6, 0xFE]);
    n[37..40].copy_from_slice(&[3, 0x20, 8]);
    n[0x7F1..0x7F3].copy_from_slice(&16u16.to_le_bytes());
    n
}

#[test]
fn beta2_qsid_setter_reads_back_and_preserves_neighboring_nvm_bytes() {
    let nvm = blank_nvm();
    let (hid, log) = qsid_transaction(nvm, |_, _| Ok(()));
    hid.set_qmk_setting_u16(2, 55).unwrap();
    let requests = log.requests();
    assert!(requests.iter().any(|p| p[..2] == [0xFE, 11]));
    assert!(requests.iter().filter(|p| p[..2] == [0xFD, 0]).count() > 4);
    assert!(hid.ak820_editing_ready());
}

#[test]
fn beta2_unchanged_effective_setting_skips_setter_and_blank_materialization() {
    let nvm = blank_nvm();
    let (hid, log) = qsid_transaction(nvm, |_, _| Ok(()));
    hid.set_qmk_setting_u16(2, 40).unwrap();
    assert!(!log.requests().iter().any(|p| p[..2] == [0xFE, 11]));
    assert!(hid.ak820_editing_ready());
}

#[test]
fn beta2_typed_setting_fault_latches_without_retry() {
    let (hid, log) = qsid_transaction(blank_nvm(), |p, _| {
        if p[..2] == [0xFE, 11] {
            Err("scripted setter timeout".into())
        } else {
            Ok(())
        }
    });
    assert!(hid.set_qmk_setting_u16(2, 55).is_err());
    let before = log.requests().len();
    assert!(hid.set_qmk_setting_u16(2, 60).is_err());
    assert_eq!(log.requests().len(), before);
    assert!(hid.ak820_failure().is_some());
}

#[test]
fn inactive_tap_dance_and_key_override_are_rejected_before_transport() {
    let (hid, log) =
        HidDevice::test_beta2_ak820_device(crate::app::ak820_beta1_profiles::ProfileId::Main, true);
    assert!(hid.set_tap_dance(0, 4, 5, 6, 7, 200).is_err());
    assert!(log.requests().is_empty());
    let (hid, log) = HidDevice::test_beta2_ak820_device(
        crate::app::ak820_beta1_profiles::ProfileId::Onehand,
        true,
    );
    assert!(hid.set_key_override(0, 4, 5, 1, 0, 0, 0, 1).is_err());
    assert!(log.requests().is_empty());
}

#[test]
fn active_typed_dynamic_rows_preserve_neighbor_nvm_and_verify_readback() {
    use crate::app::ak820_beta1_profiles::ProfileId::Shortcuts;
    let (hid, log) = HidDevice::test_beta2_ak820_device(Shortcuts, true);
    let mut nvm = vec![0; 0x2800];
    nvm[..2].copy_from_slice(&[0xE6, 0xFE]);
    nvm[37..40].copy_from_slice(&[3, 0x20, 8]);
    nvm[0x7F1..0x7F3].copy_from_slice(&16u16.to_le_bytes());
    // Sentinel in an unrelated, validly-preserved byte catches broad rewrites.
    nvm[0x1200] = 0xA5;
    let preserved = std::sync::Arc::new(std::sync::Mutex::new(None));
    let preserved_reply = preserved.clone();
    log.respond_by(move |p| {
        let mut r = [0u8; 32];
        match (p[0], p[1]) {
            (0xFD, 1) => r = status(),
            (0xFD, 0) => {
                let at = u16::from_be_bytes([p[2], p[3]]) as usize;
                let n = p[4] as usize;
                if at < 0xC700 || at + n > 0xEF00 {
                    return Some(Err("NVM bounds".into()));
                }
                r[..5].copy_from_slice(&p[..5]);
                r[5..5 + n].copy_from_slice(&nvm[at - 0xC700..at - 0xC700 + n]);
            }
            (0xFE, 13) if matches!(p[2], 1 | 2 | 5 | 6) => {
                let td = matches!(p[2], 1 | 2);
                let index = p[3] as usize;
                let at = crate::ak820_typed_storage::slot_offset(td, index).unwrap();
                if matches!(p[2], 2 | 6) {
                    nvm[at..at + 10].copy_from_slice(&p[4..14]);
                    r = *p;
                    r[0] = 0;
                } else {
                    r[0] = 0;
                    r[1..11].copy_from_slice(&nvm[at..at + 10]);
                }
            }
            _ => {
                return Some(Err(format!(
                    "unexpected dynamic transaction request {p:02x?}"
                )))
            }
        }
        if p[0] == 0xFD
            && p[1] == 0
            && u16::from_be_bytes([p[2], p[3]]) as usize == 0xC700
            && nvm[0x1200] == 0xA5
        {
            *preserved_reply.lock().unwrap() = Some(nvm[0x1200]);
        }
        Some(Ok(r))
    });
    hid.set_tap_dance(0, 4, 5, 6, 7, 200).unwrap();
    hid.set_key_override(0, 4, 5, 1, 0, 0, 0, 1).unwrap();
    assert_eq!(*preserved.lock().unwrap(), Some(0xA5));
    let writes = log
        .requests()
        .iter()
        .filter(|p| p[0] == 0xFE && p[1] == 13 && matches!(p[2], 2 | 6))
        .count();
    assert_eq!(writes, 2);
}
