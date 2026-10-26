//! Guarded one-attempt restore on the existing HID owner. No reset or rollback.
use super::*;
use crate::ak820_backup_capture::CapturedBackup;
use crate::ak820_restore_plan::{ReadExpectation, RestorePlan, RestoreStep};
use std::sync::atomic::AtomicU32;

fn number(a: &serde_json::Value, i: usize) -> Result<u16> {
    a[i].as_u64()
        .and_then(|n| u16::try_from(n).ok())
        .context("Invalid restore operand")
}
fn byte(a: &serde_json::Value, i: usize) -> Result<u8> {
    u8::try_from(number(a, i)?).context("Restore byte exceeds range")
}
fn data(a: &serde_json::Value) -> Result<Vec<u8>> {
    a.as_array()
        .context("Missing restore bytes")?
        .iter()
        .map(|v| {
            v.as_u64()
                .and_then(|n| u8::try_from(n).ok())
                .context("Invalid restore byte")
        })
        .collect()
}
/// Compile the whole immutable plan before any unlock or setter. No arbitrary
/// requests are accepted, and the existing image whitelist checks every packet.
fn compile(step: &RestoreStep, image: ImageContract) -> Result<[u8; MSG_LEN]> {
    let a = step.arguments();
    let mut p = [0; MSG_LEN];
    let request = match step.command() {
        "set_key" => {
            let value = number(a, 3)?.to_be_bytes();
            vec![5, byte(a, 0)?, byte(a, 1)?, byte(a, 2)?, value[0], value[1]]
        }
        "set_encoder" => {
            let value = number(a, 2)?.to_be_bytes();
            vec![0xfe, 4, byte(a, 0)?, 0, byte(a, 1)?, value[0], value[1]]
        }
        "set_layout_options" => vec![3, 2, 0, 0, 0, byte(a, 0)?],
        "upload_macro_chunk" => {
            let offset = number(a, 0)?.to_be_bytes();
            let bytes = data(&a[1])?;
            ensure!(
                (1..=28).contains(&bytes.len()),
                "Invalid macro restore chunk"
            );
            let mut r = vec![15, offset[0], offset[1], bytes.len() as u8];
            r.extend(bytes);
            r
        }
        "set_rgb_mode" => {
            let mut r = vec![7, 0x41];
            r.extend(data(a)?);
            ensure!(r.len() == 8, "Invalid RGB restore mode");
            r
        }
        "save_rgb" => vec![9],
        "set_combo" | "set_extension" => {
            let (op, index, bytes) = if step.command() == "set_combo" {
                (4, byte(a, 0)?, data(&a[1])?)
            } else {
                let op = match a[0].as_str() {
                    Some("tap_dance") => 2,
                    Some("key_overrides") => 6,
                    _ => bail!("Unknown extension restore"),
                };
                (op, byte(a, 1)?, data(&a[2])?)
            };
            ensure!(bytes.len() == 10, "Invalid restore record width");
            let mut r = vec![0xfe, 13, op, index];
            r.extend(bytes);
            r
        }
        "set_setting" => {
            let id = number(a, 0)?.to_le_bytes();
            let value = number(a, 1)?.to_le_bytes();
            vec![0xfe, 11, id[0], id[1], value[0], value[1]]
        }
        _ => bail!("Restore command has no reviewed executor"),
    };
    ensure!(request.len() <= MSG_LEN, "Restore packet too long");
    p[..request.len()].copy_from_slice(&request);
    ensure!(allowed_request(&p, Some(image))?, "Restore is not a setter");
    Ok(p)
}
fn unlocked(reply: &[u8; MSG_LEN]) -> Result<()> {
    ensure!(
        reply[0] == 1 && reply[1] == 0,
        "Fresh AK820 physical unlock is not ready"
    );
    Ok(())
}

fn read_nvm(send: &impl Fn(&[u8]) -> Result<[u8; MSG_LEN]>) -> Result<Vec<u8>> {
    read_range(send, 0xc700, 0x2800)
}
fn read_range(
    send: &impl Fn(&[u8]) -> Result<[u8; MSG_LEN]>,
    start: usize,
    length: usize,
) -> Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(length);
    while bytes.len() < length {
        let at = start + bytes.len();
        let count = (length - bytes.len()).min(27);
        let request = [0xfd, 0, (at >> 8) as u8, at as u8, count as u8];
        let reply = send(&request)?;
        ensure!(reply[..5] == request, "Restore ROM address differs");
        bytes.extend_from_slice(&reply[5..5 + count]);
    }
    Ok(bytes)
}
fn macro_getter(expected: &[u8], send: &impl Fn(&[u8]) -> Result<[u8; MSG_LEN]>) -> Result<()> {
    ensure!(
        expected.len() == 8192,
        "Invalid planned macro getter length"
    );
    for offset in (0..8192).step_by(28) {
        let count = (8192 - offset).min(28);
        let request = [0x0e, (offset >> 8) as u8, offset as u8, count as u8];
        let reply = send(&request)?;
        ensure!(
            reply[..4] == request && reply[4..4 + count] == expected[offset..offset + count],
            "Published macro getter differs; restore stopped"
        );
    }
    Ok(())
}
// Each boundary is fallible. A caller must never replay a failed invocation.
fn checked_step(
    packet: &[u8; MSG_LEN],
    expected: &[u8],
    send: &impl Fn(&[u8]) -> Result<[u8; MSG_LEN]>,
    verify: impl FnOnce(&[u8; MSG_LEN]) -> Result<()>,
) -> Result<()> {
    unlocked(&send(&[0xfe, 5])?)?;
    storage_ready(&send(&[0xfd, 1])?)?;
    let reply = send(packet)?;
    verify(&reply)?;
    storage_ready(&send(&[0xfd, 1])?)?;
    ensure!(
        read_nvm(send)? == expected,
        "Restore full NVM readback differs; stopped without retry or rollback"
    );
    Ok(())
}

impl HidDevice {
    /// The callbacks run on the operation worker. Source revalidation must reopen
    /// the pinned archive and compare its digest, identity and bytes. Safety save
    /// must publish, sync and reread the complete supplied capture before success.
    /// Neither callback may issue HID traffic. Progress: 0 capture, 10 safety save,
    /// 20 fresh Esc + left Ctrl hold, 30..90 setters, 95 final verification, 100 done.
    /// The caller cancels on UI/session generation changes. A failed operation
    /// leaves already acknowledged changes in place and poisons editing.
    pub(crate) fn ak820_restore_backup(
        &self,
        plan: &RestorePlan,
        cancel: &AtomicBool,
        progress: &AtomicU32,
        revalidate_source: impl FnOnce() -> Result<()>,
        save_safety: impl FnOnce(&CapturedBackup) -> Result<()>,
    ) -> Result<CapturedBackup> {
        ensure!(
            self.read_only.is_none(),
            "Settings restore is unavailable in a read-only HID session"
        );
        progress.store(0, Ordering::Release);
        capture_check_cancel(cancel)?;
        let scratch_progress = AtomicU32::new(0);
        let captured = self.ak820_capture_backup(cancel, &scratch_progress)?;
        ensure!(
            captured.identity() == plan.target() && captured.nvm() == plan.before_nvm(),
            "Target changed since restore preview; compare again"
        );
        let mut state = self
            .ak820
            .as_ref()
            .context("Not AK820")?
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        ensure!(
            state.loaded && state.failure.is_none(),
            "AK820 session stopped"
        );
        let image = state.image.context("AK820 image is not verified")?;
        ensure!(
            image.profile() == Some(captured.profile()),
            "Restore profile changed"
        );
        let packets: Vec<_> = plan
            .steps()
            .iter()
            .map(|s| compile(s, image))
            .collect::<Result<_>>()?;
        let mut callback_failed = false;
        let mut mutation_started = false;
        let result = (|| {
            let send = |r: &[u8]| -> Result<[u8; MSG_LEN]> {
                capture_check_cancel(cancel)?;
                let reply = self.usb_send_traced(r)?;
                ensure!(
                    super::super::response_matches_command(r, &reply),
                    "Restore reply differs"
                );
                Ok(reply)
            };
            ensure!(
                read_nvm(&send)? == plan.before_nvm(),
                "Target changed after capture"
            );
            let rom = &captured.files()["before-full-rom.bin"];
            for offset in (0xef80..0xefc0).step_by(27) {
                let count = (0xefc0 - offset).min(27);
                let r = [0xfd, 0, (offset >> 8) as u8, offset as u8, count as u8];
                let reply = send(&r)?;
                ensure!(
                    reply[..5] == r && reply[5..5 + count] == rom[offset..offset + count],
                    "Restore image changed"
                );
            }
            storage_ready(&send(&[0xfd, 1])?)?;
            if packets.is_empty() {
                if let Err(error) = revalidate_source() {
                    callback_failed = true;
                    return Err(error);
                }
                capture_check_cancel(cancel)?;
                progress.store(100, Ordering::Release);
                return Ok(captured.clone());
            }
            progress.store(10, Ordering::Release);
            if let Err(error) = save_safety(&captured) {
                callback_failed = true;
                return Err(error);
            }
            capture_check_cancel(cancel)?;
            // A preexisting unlock never authorizes restore. Explicit LOCK then
            // START creates a new firmware-timed physical hold, even if unlocked.
            send(&[0xfe, 8])?;
            let locked = send(&[0xfe, 5])?;
            ensure!(
                locked[..6] == [0, 0, 0, 0, 0, 5] && locked[6..].iter().all(|&b| b == 0xff),
                "AK820 physical unlock contract differs"
            );
            progress.store(20, Ordering::Release);
            send(&[0xfe, 6])?;
            let start = std::time::Instant::now();
            loop {
                let status = send(&[0xfe, 5])?;
                ensure!(status[0] <= 1 && status[1] <= 1, "Malformed unlock status");
                if status[0] == 1 && status[1] == 0 {
                    break;
                }
                ensure!(
                    start.elapsed() < std::time::Duration::from_secs(60),
                    "Fresh physical hold timed out"
                );
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            ensure!(
                read_nvm(&send)? == plan.before_nvm(),
                "Target changed during confirmation"
            );
            if let Err(error) = revalidate_source() {
                callback_failed = true;
                return Err(error);
            }
            let mut expected = plan.before_nvm().to_vec();
            for (index, (step, packet)) in plan.steps().iter().zip(&packets).enumerate() {
                for expectation in step.expectations() {
                    if let ReadExpectation::FullNvm(bytes) = expectation {
                        expected.clone_from(bytes);
                    }
                }
                mutation_started = true;
                checked_step(packet, &expected, &send, |reply| {
                    self.ak820_verify_write(packet, reply, image)?;
                    for expectation in step.expectations() {
                        if let ReadExpectation::Getter {
                            command: "macros",
                            expected,
                            ..
                        } = expectation
                        {
                            macro_getter(&data(expected)?, &send)?;
                        }
                    }
                    Ok(())
                })?;
                progress.store(
                    30 + ((index + 1) * 60 / packets.len()) as u32,
                    Ordering::Release,
                );
            }
            progress.store(95, Ordering::Release);
            ensure!(
                read_nvm(&send)? == plan.expected_nvm(),
                "Final restore NVM differs"
            );
            let restored_rom = read_range(&send, 0, 65536)?;
            ensure!(
                restored_rom[0xc700..0xef00] == *plan.expected_nvm(),
                "NVM changed during final full capture"
            );
            ensure!(
                restored_rom[0xef80..0xefc0] == rom[0xef80..0xefc0],
                "Image manifest changed during restore"
            );
            storage_ready(&send(&[0xfd, 1])?)?;
            let mut reconcile = |r: &[u8]| send(r);
            let rgb_matches_saved = capture_reconcile_getters(
                captured.profile(),
                image,
                plan.expected_nvm(),
                plan.final_getters(),
                &mut reconcile,
            )?;
            if plan
                .steps()
                .iter()
                .any(|step| step.command() == "set_rgb_mode")
            {
                ensure!(rgb_matches_saved, "Final restored RGB getter differs");
            }
            let repeated_nvm = read_nvm(&send)?;
            let repeated_manifest = read_range(&send, 0xef80, 64)?;
            crate::ak820_backup_capture::check_stability(
                &restored_rom,
                &repeated_nvm,
                &repeated_manifest,
            )
            .map_err(anyhow::Error::msg)?;
            storage_ready(&send(&[0xfd, 1])?)?;
            let restored =
                CapturedBackup::from_rom(captured.profile(), restored_rom, rgb_matches_saved)
                    .map_err(anyhow::Error::msg)?;
            capture_check_cancel(cancel)?;
            progress.store(100, Ordering::Release);
            Ok(restored)
        })();
        if let Err(error) = &result {
            if !callback_failed
                && (mutation_started || error.downcast_ref::<CaptureCancelled>().is_none())
            {
                state.failure = Some(format!("Restore stopped: {error:#}"));
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    fn modeled_reply(request: &[u8]) -> [u8; MSG_LEN] {
        let mut reply = [0; MSG_LEN];
        match request {
            [0xfe, 5] => reply[0] = 1,
            [0xfd, 1] => reply[..4].copy_from_slice(&[0xfd, 1, 1, 0xa8]),
            [0xfd, 0, ..] => reply[..5].copy_from_slice(request),
            _ => reply[..request.len()].copy_from_slice(request),
        }
        reply
    }
    #[test]
    fn every_transport_failure_stops_before_a_second_setter_without_replay() {
        let packets = [[5, 0, 0, 0, 0, 4], [5, 0, 0, 1, 0, 5]];
        // status, storage, setter, post-storage, all full NVM read packets.
        let boundary_count = 4 + (0x2800usize + 26) / 27;
        for fail_at in 0..boundary_count {
            let requests = RefCell::new(Vec::<Vec<u8>>::new());
            let send = |r: &[u8]| -> Result<[u8; MSG_LEN]> {
                let mut requests = requests.borrow_mut();
                requests.push(r.to_vec());
                ensure!(requests.len() - 1 != fail_at, "injected timeout");
                Ok(modeled_reply(r))
            };
            let run = || -> Result<()> {
                for packet in packets {
                    checked_step(
                        &padded_request(&packet),
                        &vec![0; 0x2800],
                        &send,
                        |_| Ok(()),
                    )?;
                }
                Ok(())
            };
            assert!(run().is_err(), "boundary {fail_at}");
            let requests = requests.borrow();
            assert_eq!(requests.len(), fail_at + 1, "no requests after failure");
            assert!(requests.iter().filter(|r| r[0] == 5).count() <= 1);
            assert!(!requests.iter().any(|r| r.as_slice() == packets[1]));
        }
    }
    #[test]
    fn locked_storage_fault_rejected_ack_and_adjacent_nvm_mismatch_stop() {
        for failure in 0..4 {
            let writes = Cell::new(0);
            let reads = Cell::new(0);
            let send = |r: &[u8]| -> Result<[u8; MSG_LEN]> {
                let mut reply = modeled_reply(r);
                if r[0] == 5 {
                    writes.set(writes.get() + 1);
                }
                if r == [0xfe, 5] && failure == 0 {
                    reply[0] = 0;
                }
                if r == [0xfd, 1] && failure == 1 {
                    reply[4] = 1;
                }
                if r.starts_with(&[0xfd, 0]) {
                    reads.set(reads.get() + 1);
                    if failure == 3 {
                        reply[5] = 1;
                    }
                }
                Ok(reply)
            };
            let result = checked_step(
                &padded_request(&[5, 0, 0, 0, 0, 4]),
                &vec![0; 0x2800],
                &send,
                |_| {
                    ensure!(failure != 2, "rejected ACK");
                    Ok(())
                },
            );
            assert!(result.is_err());
            assert_eq!(writes.get(), if failure < 2 { 0 } else { 1 });
            if failure == 2 {
                assert_eq!(reads.get(), 0, "ACK failure stops before NVM");
            }
        }
    }
    #[test]
    fn successful_checked_steps_order_unlock_storage_ack_and_complete_readback() {
        let requests = RefCell::new(Vec::new());
        let send = |r: &[u8]| -> Result<[u8; MSG_LEN]> {
            requests.borrow_mut().push(r.to_vec());
            Ok(modeled_reply(r))
        };
        let verified = Cell::new(false);
        checked_step(
            &padded_request(&[5, 0, 0, 0, 0, 4]),
            &vec![0; 0x2800],
            &send,
            |_| {
                assert_eq!(requests.borrow().len(), 3);
                verified.set(true);
                Ok(())
            },
        )
        .unwrap();
        assert!(verified.get());
        let requests = requests.borrow();
        assert_eq!(requests[0], [0xfe, 5]);
        assert_eq!(requests[1], [0xfd, 1]);
        assert_eq!(requests[3], [0xfd, 1]);
        assert_eq!(requests[4], [0xfd, 0, 0xc7, 0, 27]);
        let last = requests.last().unwrap();
        let at = u16::from_be_bytes([last[2], last[3]]) as usize;
        assert_eq!(
            at + last[4] as usize,
            0xef00,
            "complete NVM including neighbors"
        );
    }
    #[test]
    fn published_macro_getter_mismatch_blocks_every_later_setter() {
        let writes = Cell::new(0);
        let macro_reads = Cell::new(0);
        let send = |r: &[u8]| -> Result<[u8; MSG_LEN]> {
            let mut reply = modeled_reply(r);
            if r[0] == 15 || r[0] == 5 {
                writes.set(writes.get() + 1);
            }
            if r[0] == 14 {
                macro_reads.set(macro_reads.get() + 1);
                reply[..4].copy_from_slice(r);
                reply[4] = 1;
            }
            Ok(reply)
        };
        let run = || -> Result<()> {
            checked_step(
                &padded_request(&[15, 0, 0, 16]),
                &vec![0; 0x2800],
                &send,
                |_| macro_getter(&vec![0; 8192], &send),
            )?;
            checked_step(
                &padded_request(&[5, 0, 0, 0, 0, 4]),
                &vec![0; 0x2800],
                &send,
                |_| Ok(()),
            )
        };
        assert!(run()
            .unwrap_err()
            .to_string()
            .contains("Published macro getter differs"));
        assert_eq!(writes.get(), 1);
        assert_eq!(macro_reads.get(), 1);
    }
    #[test]
    fn planner_to_wire_keeps_both_encoder_directions_distinct() {
        use crate::ak820_settings_backup::{
            rgb_wire_for_profile, sha256, ImageIdentity, ValidatedArchive,
        };
        use crate::app::ak820_beta1_profiles::{matrix_profile, ProfileId};
        let profile = ProfileId::Main;
        let id = matrix_profile(profile).settings_identity.as_ref().unwrap();
        let identity = ImageIdentity {
            ihx_sha256: id.ihx_sha256.clone(),
            code_sha256: id.code_sha256.clone(),
            definition_sha256: id.definition_sha256.clone(),
            schema: id.nvm_schema,
        };
        let mut before = vec![0; 0x2800];
        before[..2].copy_from_slice(&[0xe6, 0xfe]);
        before[37..40].copy_from_slice(&id.nvm_schema.to_le_bytes()[..3]);
        before[24] = 5;
        before[27] = 80;
        before[0x7f1] = 16;
        for i in 0..8 {
            before[0x658 + i * 10 + 4..0x658 + i * 10 + 8].copy_from_slice(&[0x20, 8, 1, 0]);
        }
        before[0x6a8..0x6c0]
            .copy_from_slice(&crate::ak820_typed_storage::QmkSettings::default().encode());
        let mut wanted = before.clone();
        wanted[0x640..0x644].copy_from_slice(&[0, 4, 0, 5]);
        let settings = crate::ak820_settings_model::settings_from_nvm(
            &wanted,
            &rgb_wire_for_profile(&wanted[24..32], false, profile).unwrap(),
            true,
        )
        .unwrap();
        let archive = ValidatedArchive {
            source: identity.clone(),
            manifest_sha256: "synthetic-host-model".into(),
            manifest: serde_json::json!({"files":{"saved-nvm.bin":{"size":wanted.len(),"sha256":sha256(&wanted)}}}),
            settings,
        };
        let plan =
            crate::ak820_restore_plan::plan_restore(&archive, &wanted, profile, &identity, &before)
                .unwrap();
        let packets: Vec<_> = plan
            .steps()
            .iter()
            .map(|step| compile(step, ImageContract::Beta2(profile)).unwrap())
            .collect();
        assert_eq!(packets.len(), 2);
        assert_eq!(packets[0][..7], [0xfe, 4, 0, 0, 0, 0, 5]);
        assert_eq!(packets[1][..7], [0xfe, 4, 0, 0, 1, 0, 4]);
        assert_eq!(&plan.expected_nvm()[0x640..0x644], &[0, 4, 0, 5]);
    }
}
