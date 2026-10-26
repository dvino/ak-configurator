//! Offline transaction checks; scripted saved bytes do not prove physical durability.
use super::*;

fn palette_device(fail_at: Option<u8>) -> (HidDevice, super::super::TestHidRecorder) {
    let (hid, log) = HidDevice::test_beta2_ak820_device(crate::app::ak820_beta1_profiles::ProfileId::Main, true);
    let mut mode = [2, 0, 42, 80, 200, 100];
    log.respond_by(move |p| {
        let mut reply = [0; 32];
        match (p[0], p[1]) {
            (0xFD, 1) => reply[..4].copy_from_slice(&[0xFD, 1, 1, 0xA8]),
            (8, 0x41) => { reply[..2].copy_from_slice(&[8, 0x41]); reply[2..8].copy_from_slice(&mode); }
            (7, 0x42) => {
                if fail_at == Some(p[2]) { return Some(Err("scripted Direct timeout".into())); }
                reply = *p;
            }
            (7, 0x41) => { mode.copy_from_slice(&p[2..8]); reply = *p; }
            _ => return Some(Err(format!("unexpected palette request {p:02x?}"))),
        }
        Some(Ok(reply))
    });
    (hid, log)
}

#[test]
fn static_palette_is_bounded_and_never_saves_nvm() {
    let (hid, log) = palette_device(None);
    hid.ak820_apply_palette(&[[160, 0, 0]; 82]).unwrap();
    let requests = log.requests();
    let packets: Vec<_> = requests.iter().filter(|p| p[..2] == [7, 0x42]).collect();
    assert_eq!(packets.len(), 10);
    assert_eq!(packets.iter().map(|p| p[4] as usize).sum::<usize>(), 82);
    for (index, p) in packets.iter().enumerate() {
        assert_eq!(p[2] as usize, index * 9);
        for hsv in p[5..5 + p[4] as usize * 3].chunks_exact(3) { assert_eq!(hsv, [0, 255, 160]); }
    }
    assert_eq!(requests.iter().filter(|p| p[..2] == [7, 0x41]).count(), 1);
    assert!(!requests.iter().any(|p| p[0] == 9 || p[0] == 0xFE || p[0] == 0x0F));
    assert!(hid.ak820_editing_ready());
}

#[test]
fn palette_timeout_stops_frame_once_and_latches_without_restore() {
    let (hid, log) = palette_device(Some(27));
    assert!(hid.ak820_apply_palette(&[[0, 160, 0]; 82]).is_err());
    let requests = log.requests();
    assert_eq!(requests.iter().filter(|p| p[..2] == [7, 0x42]).count(), 4);
    assert!(!requests.iter().any(|p| p[..2] == [7, 0x41] || p[0] == 9));
    assert!(hid.ak820_apply_palette(&[[0, 0, 160]; 82]).is_err());
    assert_eq!(log.requests().len(), requests.len());
    assert!(!hid.ak820_editing_ready());
}

#[test]
fn palette_rejects_unavailable_profiles_partial_reads_and_invalid_values_before_transport() {
    use crate::app::ak820_beta1_profiles::ProfileId;
    for (profile, ready) in [(ProfileId::Code, true), (ProfileId::Main, false)] {
        let (hid, log) = HidDevice::test_beta2_ak820_device(profile, ready);
        assert!(hid.ak820_apply_palette(&[[0, 0, 0]; 82]).is_err());
        assert!(log.requests().is_empty());
    }
    let (hid, log) = palette_device(None);
    assert!(hid.ak820_apply_palette(&[[161, 0, 0]; 82]).is_err());
    assert!(log.requests().is_empty());
    let (hid, log) = HidDevice::test_ak820_device(true);
    assert!(hid.ak820_apply_palette(&[[0, 0, 0]; 82]).is_err());
    assert!(log.requests().is_empty());
}

#[test]
fn rgb_stream_enters_direct_once_and_never_saves_frames() {
    let (hid, log) = palette_device(None);
    let cancel = AtomicBool::new(false);
    let Ak820RgbStreamResult::Applied(owned) = hid.ak820_stream_rgb_frame(&[[160, 0, 0]; 82], None, &cancel).unwrap()
        else { panic!("first frame must enter Direct"); };
    let before = log.requests().len();
    assert!(matches!(hid.ak820_stream_rgb_frame(&[[0, 160, 0]; 82], Some(&owned), &cancel).unwrap(),
        Ak820RgbStreamResult::Applied(_)));
    let sent = log.requests();
    let next = &sent[before..];
    assert_eq!(next.iter().filter(|p| p[..2] == [7, 0x42]).count(), 10);
    assert_eq!(next.iter().filter(|p| p[..2] == [8, 0x41]).count(), 2);
    assert!(!next.iter().any(|p| p[..2] == [7, 0x41] || p[0] == 9));
}

#[test]
fn rgb_stream_failure_at_each_packet_boundary_stops_without_retry_or_restore() {
    for packet in 0..10 {
        let (hid, log) = palette_device(Some(packet * 9));
        let cancel = AtomicBool::new(false);
        assert!(hid.ak820_stream_rgb_frame(&[[80; 3]; 82], None, &cancel).is_err());
        let sent = log.requests();
        assert_eq!(sent.iter().filter(|p| p[..2] == [7, 0x42]).count(), packet as usize + 1);
        assert!(!sent.iter().any(|p| p[..2] == [7, 0x41] || p[0] == 9));
        assert!(!hid.ak820_editing_ready());
        assert!(hid.ak820_stream_rgb_frame(&[[0; 3]; 82], None, &cancel).is_err());
        assert_eq!(sent.len(), log.requests().len());
    }
}

#[test]
fn rgb_stream_cancellation_is_checked_before_io_and_between_packets() {
    let (hid, log) = palette_device(None);
    assert!(matches!(hid.ak820_stream_rgb_frame(&[[0; 3]; 82], None, &AtomicBool::new(true)).unwrap(),
        Ak820RgbStreamResult::Cancelled));
    assert!(log.requests().is_empty());
    let cancel = std::sync::Arc::new(AtomicBool::new(false));
    let worker_cancel = cancel.clone();
    log.respond_by(move |p| {
        let mut reply = *p;
        match (p[0], p[1]) {
            (0xFD, 1) => reply[..4].copy_from_slice(&[0xFD, 1, 1, 0xA8]),
            (8, 0x41) => reply[2..8].copy_from_slice(&[2, 0, 0, 0, 0, 100]),
            (7, 0x42) => worker_cancel.store(true, Ordering::Release),
            _ => return Some(Err("unexpected command after cancellation".into())),
        }
        Some(Ok(reply))
    });
    assert!(matches!(hid.ak820_stream_rgb_frame(&[[80; 3]; 82], None, &cancel).unwrap(),
        Ak820RgbStreamResult::Cancelled));
    assert_eq!(log.requests().iter().filter(|p| p[..2] == [7, 0x42]).count(), 1);
    assert!(hid.ak820_editing_ready());
}

#[test]
fn rgb_stream_yields_to_physical_mode_and_color_controls_without_poisoning_settings() {
    for changed_field in [2, 4, 5, 6, 7] {
        let (hid, log) = palette_device(None);
        let cancel = AtomicBool::new(false);
        let Ak820RgbStreamResult::Applied(owned) = hid.ak820_stream_rgb_frame(&[[100; 3]; 82], None, &cancel).unwrap()
            else { panic!("first frame"); };
        let mut changed = owned.mode;
        changed[changed_field] = if changed_field == 2 { 2 } else { changed[changed_field].saturating_sub(1) };
        log.respond_by(move |p| {
            let mut r = [0; 32];
            match (p[0], p[1]) {
                (0xFD, 1) => r[..4].copy_from_slice(&[0xFD, 1, 1, 0xA8]),
                (8, 0x41) => r = changed,
                _ => return Some(Err("stream must not override physical control".into())),
            }
            Some(Ok(r))
        });
        let before = log.requests().len();
        assert!(matches!(hid.ak820_stream_rgb_frame(&[[50; 3]; 82], Some(&owned), &cancel).unwrap(),
            Ak820RgbStreamResult::OwnershipLost));
        assert_eq!(log.requests().len() - before, 2);
        assert!(hid.ak820_editing_ready());
    }
}

#[test]
fn rgb_stream_rejects_read_only_sessions_before_transport() {
    let (mut hid, log) = palette_device(None);
    hid.read_only = Some(super::super::ReadOnlyHidSession::default());
    assert!(hid.ak820_stream_rgb_frame(&[[0; 3]; 82], None, &AtomicBool::new(false)).is_err());
    assert!(log.requests().is_empty());
}

fn stream(first: &[u8]) -> Vec<u8> {
    let mut bytes = first.to_vec();
    bytes.extend([0; 16]);
    bytes
}

fn macro_device(
    unlocked: bool,
    timeout_offset: Option<usize>,
    mismatch: bool,
) -> (HidDevice, super::super::TestHidRecorder) {
    let (hid, log) = HidDevice::test_ak820_device(true);
    let mut uploaded = Vec::<u8>::new();
    log.respond_by(move |p| {
        let mut r = [0; 32];
        match (p[0], p[1]) {
            (0xFD, 1) => r[..4].copy_from_slice(&[0xFD, 1, 1, 0xA8]),
            (0xFE, 5) => {
                r.fill(0xFF);
                r[0] = unlocked as u8;
                r[1] = 0;
            }
            (0x0F, _) => {
                let offset = u16::from_be_bytes([p[1], p[2]]) as usize;
                if timeout_offset == Some(offset) {
                    return Some(Err("injected macro chunk timeout".into()));
                }
                if offset != uploaded.len() {
                    return Some(Err("unexpected macro upload offset".into()));
                }
                uploaded.extend_from_slice(&p[4..4 + p[3] as usize]);
                r = *p;
            }
            (0xFD, 0) if p[2..5] == [0xCE, 0xF0, 3] => {
                r[..5].copy_from_slice(&p[..5]);
                r[6..8].copy_from_slice(&(uploaded.len() as u16).to_le_bytes());
            }
            (0x0E, _) => {
                let offset = u16::from_be_bytes([p[1], p[2]]) as usize;
                let n = p[3] as usize;
                r[..4].copy_from_slice(&p[..4]);
                r[4..4 + n].copy_from_slice(&uploaded[offset..offset + n]);
                if mismatch && offset == 0 {
                    r[4] = b'Z';
                }
            }
            _ => return Some(Err(format!("unexpected transaction request {p:02x?}"))),
        }
        Some(Ok(r))
    });
    (hid, log)
}

fn setters(log: &super::super::TestHidRecorder) -> Vec<[u8; 32]> {
    log.requests()
        .into_iter()
        .filter(|p| p[0] == 0x0F)
        .collect()
}

#[test]
fn macro_rejected_actions_never_reach_a_setter() {
    for operand in [
        vec![1, 5, 1, 0x7C],    // reboot
        vec![1, 5, 0x7C, 0xFF], // encoded low-zero boot keycode
        vec![1, 5, 4, 0x7C],    // unsupported extended keycode
        vec![1, 1, 2],
    ] {
        // unsupported primitive keycode
        let (hid, log) = macro_device(true, None, false);
        assert!(
            hid.set_macro_buffer(&stream(&operand)).is_err(),
            "operand {operand:x?}"
        );
        assert!(
            log.requests().is_empty(),
            "reject before unlock check or writes"
        );
    }
}

#[test]
fn primitive_extended_delay_and_encoded_low_zero_macros_upload() {
    for operand in [
        vec![1, 1, 4, 1, 2, 4, 1, 3, 4],
        vec![1, 5, 0x04, 0x01, 1, 6, 0x04, 0x01, 1, 7, 0x04, 0x01],
        vec![1, 4, 1, 2], // nonzero delay operands are not keycodes
        vec![1, 5, 0x52, 0xFF],
    ] {
        // encoded 0x5200, layer zero
        let (hid, log) = macro_device(true, None, false);
        let bytes = stream(&operand);
        hid.set_macro_buffer(&bytes).unwrap();
        assert!(!setters(&log).is_empty());
        assert!(hid.ak820_editing_ready());
    }
}

#[test]
fn locked_macro_upload_sends_no_setters_or_unlock_commands() {
    let (hid, log) = macro_device(false, None, false);
    assert!(hid.set_macro_buffer(&stream(b"hello")).is_err());
    assert!(setters(&log).is_empty());
    assert_eq!(log.requests().len(), 1);
    assert_eq!(log.requests()[0][..2], [0xFE, 5]);
}

#[test]
fn multi_chunk_macro_upload_publishes_only_complete_logical_stream() {
    let (hid, log) = macro_device(true, None, false);
    let logical = stream(&[b'a'; 60]);
    let mut padded = logical.clone();
    padded.resize(8192, 0);
    hid.set_macro_buffer(&padded).unwrap();
    let writes = setters(&log);
    assert_eq!(writes.len(), 3);
    let mut actual = Vec::new();
    for (index, p) in writes.iter().enumerate() {
        assert_eq!(u16::from_be_bytes([p[1], p[2]]) as usize, index * 28);
        actual.extend_from_slice(&p[4..4 + p[3] as usize]);
    }
    assert_eq!(actual, logical);
    assert_eq!(writes.last().unwrap()[3], 20);
}

#[test]
fn chunk_timeout_stops_once_and_latches_session() {
    let (hid, log) = macro_device(true, Some(28), false);
    assert!(hid.set_macro_buffer(&stream(&[b'a'; 90])).is_err());
    assert_eq!(setters(&log).len(), 2);
    assert!(!log.requests().iter().any(|p| p[0] == 0x0E));
    assert!(hid.ak820_failure().is_some());
    let before = log.requests().len();
    assert!(hid.set_macro_buffer(&stream(b"next")).is_err());
    assert_eq!(log.requests().len(), before);
}

#[test]
fn final_macro_readback_mismatch_latches_session() {
    let (hid, log) = macro_device(true, None, true);
    assert!(hid.set_macro_buffer(&stream(b"abc")).is_err());
    assert_eq!(setters(&log).len(), 1);
    assert!(log.requests().iter().any(|p| p[0] == 0x0E));
    assert!(!hid.ak820_editing_ready());
    let before = log.requests().len();
    assert!(hid.usb_send(&[9]).is_err());
    assert_eq!(log.requests().len(), before);
}

#[test]
fn rgb_save_checks_scripted_persistent_bytes_and_latches_mismatch() {
    for bad_saved_value in [false, true] {
        let (hid, log) = HidDevice::test_ak820_device(true);
        log.respond_by(move |p| {
            let mut r = [0; 32];
            match (p[0], p[1]) {
                (0xFD, 1) => r[..4].copy_from_slice(&[0xFD, 1, 1, 0xA8]),
                (9, _) => r = *p,
                (8, 0x41) => r[..8].copy_from_slice(&[8, 0x41, 2, 0, 42, 80, 200, 100]),
                (0xFD, 0) if p[2..5] == [0xC7, 0x18, 8] => {
                    r[..5].copy_from_slice(&p[..5]);
                    r[5..13].copy_from_slice(&[
                        5,
                        80,
                        200,
                        if bad_saved_value { 99 } else { 100 },
                        42,
                        0,
                        0,
                        0,
                    ]);
                }
                _ => return Some(Err(format!("unexpected RGB request {p:02x?}"))),
            }
            Some(Ok(r))
        });
        assert_eq!(hid.usb_send(&[9]).is_err(), bad_saved_value);
        assert!(log
            .requests()
            .iter()
            .any(|p| p[..5] == [0xFD, 0, 0xC7, 0x18, 8]));
        assert_eq!(hid.ak820_failure().is_some(), bad_saved_value);
        if bad_saved_value {
            let before = log.requests().len();
            assert!(hid.usb_send(&[9]).is_err());
            assert_eq!(log.requests().len(), before);
        }
    }
}

/// Synthetic negative verification fixture: zero CODE cannot match alpha.3.
/// The manifest is constructed from public contract fields, not a ROM capture.
/// These tests establish rejection behavior, with no hardware or image-success claim.
#[test]
fn full_verification_rejects_zero_code_trial_and_unknown_manifest_hash() {
    for case in 0..3 {
        let (hid, log) = HidDevice::test_ak820_device(false);
        let mut manifest = [0u8; 64];
        manifest[..8].copy_from_slice(b"AKV1PKG\0");
        manifest[8] = 1;
        manifest[12..16].copy_from_slice(&0x082002u32.to_le_bytes());
        for (i, pair) in crate::ak820::CODE_SHA256
            .as_bytes()
            .chunks_exact(2)
            .enumerate()
        {
            manifest[16 + i] = u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap();
        }
        if case == 1 {
            manifest[9] = 1;
        }
        if case == 2 {
            manifest[16] ^= 1;
        }
        log.respond_by(move |p| {
            let mut r = [0u8; 32];
            match (p[0], p[1]) {
                (1, _) => r[..3].copy_from_slice(&[1, 0, 9]),
                (0xFE, 0) => {
                    r[..4].copy_from_slice(&6u32.to_le_bytes());
                    r[4..12].copy_from_slice(&crate::ak820::VIAL_UID.to_le_bytes());
                }
                (0xFD, 0) => {
                    let address = u16::from_be_bytes([p[2], p[3]]) as usize;
                    let count = p[4] as usize;
                    r[..5].copy_from_slice(&p[..5]);
                    if address >= 0xEF80 && address + count <= 0xEFC0 {
                        r[5..5 + count]
                            .copy_from_slice(&manifest[address - 0xEF80..address - 0xEF80 + count]);
                    } else if !(address >= 0x2800 && address + count <= 0xC700) {
                        return Some(Err("read outside synthetic verification fixture".into()));
                    }
                }
                (0xFD, 1) => r[..4].copy_from_slice(&[0xFD, 1, 1, 0xA8]),
                _ => return Some(Err(format!("unexpected verification request {p:02x?}"))),
            }
            Some(Ok(r))
        });
        let error = hid
            .verify_ak820_firmware(|_| Ok(()))
            .unwrap_err()
            .to_string();
        assert!(!hid.ak820_editing_ready());
        assert!(hid.ak820_enable_settings().is_err());
        let requests = log.requests();
        assert!(requests.iter().all(|p| p[0] == 1
            || p[..2] == [0xFE, 0]
            || p[..2] == [0xFD, 0]
            || p[..2] == [0xFD, 1]));
        let code_reads: Vec<_> = requests
            .iter()
            .filter(|p| {
                p[..2] == [0xFD, 0]
                    && (0x2800..0xC700).contains(&(u16::from_be_bytes([p[2], p[3]]) as usize))
            })
            .collect();
        if case == 0 {
            assert!(error.contains("CODE differs"), "{error}");
            assert_eq!(
                code_reads.iter().map(|p| p[4] as usize).sum::<usize>(),
                40704
            );
        } else {
            assert!(
                code_reads.is_empty(),
                "trial and unknown pins must fail before CODE reads"
            );
        }
        let before = log.requests().len();
        assert!(hid.usb_send(&[9]).is_err());
        assert_eq!(
            log.requests().len(),
            before,
            "unverified session must block setters"
        );
    }
}
