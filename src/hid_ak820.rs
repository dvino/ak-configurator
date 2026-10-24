//! AK820's checked settings path. The proxy owns transport; this parent-side
//! session serializes a setter with its storage check and exact readback.
use super::{padded_request, HidDevice, MSG_LEN};
use anyhow::{bail, ensure, Context, Result};
use sha2::{Digest, Sha256};

#[derive(Default)]
pub(super) struct Ak820Session {
    verified: bool,
    loaded: bool,
    failure: Option<String>,
}

fn storage_ready(response: &[u8; MSG_LEN]) -> Result<()> {
    ensure!(
        response[..4] == [0xFD, 1, 1, 0xA8],
        "AK820 storage status has an unknown format"
    );
    ensure!(
        response[4] == 0
            && response[6] == 0
            && response[7] == 0
            && response[10] == 0
            && response[18] == 0,
        "AK820 storage failed or is busy; stop and read the device again"
    );
    ensure!(
        response[5] <= 1 && response[17] <= 1 && response[22..].iter().all(|&b| b == 0),
        "AK820 storage status is malformed"
    );
    Ok(())
}

/// Validate the complete Vial stream before any upload. Binary operands cannot
/// contain zero in this firmware; its terminators therefore remain unambiguous.
pub(crate) fn macro_stream_length(stream: &[u8]) -> Result<usize> {
    ensure!(
        (16..=8192).contains(&stream.len()),
        "AK820 macros need 16 terminators and at most 8192 bytes"
    );
    let (mut terminated, mut arguments, mut logical_end) = (0, 0, 0);
    for (offset, &value) in stream.iter().enumerate() {
        if terminated == 16 {
            ensure!(value == 0, "Data follows the last AK820 macro");
        } else if arguments == 3 {
            ensure!((1..=7).contains(&value), "Unsupported AK820 macro opcode");
            arguments = if value <= 3 { 1 } else { 2 };
        } else if arguments > 0 {
            ensure!(value != 0, "Zero byte in an AK820 macro operand");
            arguments -= 1;
        } else if value == 0 {
            terminated += 1;
            logical_end = offset + 1;
        } else if value == 1 {
            arguments = 3;
        } else {
            ensure!(value < 128, "AK820 macro text must be ASCII");
        }
    }
    ensure!(
        terminated == 16 && arguments == 0,
        "Incomplete AK820 macro stream"
    );
    Ok(logical_end)
}

/// The firmware's macro operands can encode the same actions as the keymap.
/// Apply the assignment policy to decoded operands before the first setter.
fn validate_macro_upload(stream: &[u8]) -> Result<usize> {
    let length = macro_stream_length(stream)?;
    let mut offset = 0;
    while offset < length {
        if stream[offset] != 1 {
            offset += 1;
            continue;
        }
        let opcode = stream[offset + 1];
        let operands = if opcode <= 3 { 1 } else { 2 };
        if opcode != 4 {
            let mut keycode = stream[offset + 2] as u16;
            if operands == 2 {
                keycode = u16::from_le_bytes([stream[offset + 2], stream[offset + 3]]);
                if keycode > 0xFF00 {
                    keycode = (keycode & 255) << 8;
                }
            }
            ensure!(crate::ak820::is_assignable_keycode(keycode),
                "Macro keycode {keycode:#06X} is unavailable in the AK820 editor; nothing was written");
        }
        offset += 2 + operands;
    }
    Ok(length)
}

/// Whitelist the application's existing settings protocol. No reset, loader,
/// factory erase, unknown extensions or speculative QMK setters reach HID.
fn allowed_request(p: &[u8; MSG_LEN]) -> Result<bool> {
    let tail = |at: usize| p[at..].iter().all(|&b| b == 0);
    let be = |at| u16::from_be_bytes([p[at], p[at + 1]]) as usize;
    let matrix = || p[1] < 6 && p[2] < 16 && p[3] < 8;
    let (valid, write) = match p[0] {
        0x01 | 0x0C | 0x0D | 0x11 => (tail(1), false),
        0x02 => ((p[1] == 2 || p[1] == 3) && tail(2), false),
        0x04 => (matrix() && tail(4), false),
        0x05 => (
            matrix() && tail(6) && crate::ak820::is_assignable_keycode(be(4) as u16),
            true,
        ),
        0x07 => (
            p[1] == 0x41
                && tail(8)
                && crate::ak820::CAPABILITIES.rgb_modes.contains(&p[2])
                && p[3] == 0
                && p[7] <= 160,
            true,
        ),
        0x08 => (
            match p[1] {
                0x40 | 0x41 | 0x43 => tail(2),
                0x42 => tail(4),
                0x44 => p[2] < 82 && p[3] == 0 && tail(4),
                _ => false,
            },
            false,
        ),
        0x09 => (tail(1), true),
        0x0E | 0x0F => {
            let n = p[3] as usize;
            (
                (1..=28).contains(&n)
                    && be(1) + n <= 8192
                    && tail(if p[0] == 0x0E { 4 } else { 4 + n }),
                p[0] == 0x0F,
            )
        }
        0x12 => (
            (1..=28).contains(&p[3]) && be(1) + p[3] as usize <= 6 * 16 * 8 * 2 && tail(4),
            false,
        ),
        0xFD => (
            match p[1] {
                0 => (1..=27).contains(&p[4]) && be(2) + p[4] as usize <= 65536 && tail(5),
                1 => tail(2),
                _ => false,
            },
            false,
        ),
        0xFE => match p[1] {
            0 | 1 | 5 | 9 => (tail(2), false),
            2 => (p[4] == 0 && p[5] == 0 && tail(6), false),
            3 => (p[2] < 6 && p[3] == 0 && tail(4), false),
            4 => (
                p[2] < 6
                    && p[3] == 0
                    && p[4] <= 1
                    && tail(7)
                    && crate::ak820::is_assignable_keycode(be(5) as u16),
                true,
            ),
            // Unlock is available only as an explicit UI action; connection is
            // read-only and never starts, polls or cancels an unlock itself.
            6 | 7 | 8 => (tail(2), true),
            13 => match p[2] {
                0 => (tail(3), false),
                3 => (p[3] < 8 && tail(4), false),
                4 => (tail(14) && crate::ak820::valid_combo_record(&p[..14]), true),
                _ => (false, true),
            },
            _ => (false, true),
        },
        _ => (false, true),
    };
    ensure!(
        valid,
        "This command or value is unavailable in AK820 alpha.3"
    );
    Ok(write)
}

impl HidDevice {
    pub(super) fn ak820_set_macro_buffer(&self, data: &[u8]) -> Result<()> {
        let logical = validate_macro_upload(data)?;
        // Only the complete logical stream is published. No padding can cause
        // a later chunk to be mistaken for a new upload after terminator 16.
        let data = &data[..logical];
        ensure!(
            self.get_unlock_status()?.0,
            "Unlock the AK820 before changing macros"
        );
        let result = (|| {
            for (index, chunk) in data.chunks(28).enumerate() {
                let offset = index * 28;
                let mut request = [0; 32];
                request[..4].copy_from_slice(&[
                    0x0F,
                    (offset >> 8) as u8,
                    offset as u8,
                    chunk.len() as u8,
                ]);
                request[4..4 + chunk.len()].copy_from_slice(chunk);
                self.usb_send(&request)?;
            }
            let actual = self.get_macro_buffer(8192, 16)?;
            let actual_len = macro_stream_length(&actual)?;
            ensure!(
                actual[..actual_len] == *data,
                "AK820 macro readback differs; upload stopped"
            );
            self.ak820_storage_check()?;
            Ok(())
        })();
        if let Err(error) = &result {
            self.ak820_fail(error);
        }
        result
    }

    pub(crate) fn is_ak820_session(&self) -> bool {
        self.ak820.is_some()
    }

    pub(crate) fn ak820_editing_ready(&self) -> bool {
        self.ak820.as_ref().is_some_and(|state| {
            let state = state.lock().unwrap_or_else(|e| e.into_inner());
            state.loaded && state.failure.is_none()
        })
    }

    pub(crate) fn ak820_failure(&self) -> Option<String> {
        self.ak820
            .as_ref()
            .and_then(|s| s.lock().unwrap_or_else(|e| e.into_inner()).failure.clone())
    }

    pub(crate) fn ak820_fail(&self, error: &anyhow::Error) {
        if let Some(state) = &self.ak820 {
            state.lock().unwrap_or_else(|e| e.into_inner()).failure = Some(format!("{error:#}"));
        }
    }

    pub(crate) fn ak820_storage_check(&self) -> Result<()> {
        let result = storage_ready(&self.usb_send(&[0xFD, 1])?);
        if let Err(error) = &result {
            self.ak820_fail(error);
        }
        result
    }

    pub(crate) fn ak820_enable_settings(&self) -> Result<()> {
        let state = self.ak820.as_ref().context("Not an AK820 session")?;
        let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
        ensure!(
            state.verified && state.failure.is_none(),
            "AK820 image is not verified"
        );
        state.loaded = true;
        Ok(())
    }

    pub(crate) fn ak820_read_rom(&self, start: usize, length: usize) -> Result<Vec<u8>> {
        ensure!(
            start <= 65536 && length <= 65536 - start,
            "Invalid AK820 ROM read range"
        );
        let mut bytes = Vec::with_capacity(length);
        while bytes.len() < length {
            let address = start + bytes.len();
            let count = (length - bytes.len()).min(27);
            let request = [0xFD, 0, (address >> 8) as u8, address as u8, count as u8];
            let reply = self.usb_send(&request)?;
            ensure!(
                reply[..5] == request,
                "AK820 ROM reply address or length differs"
            );
            bytes.extend_from_slice(&reply[5..5 + count]);
        }
        Ok(bytes)
    }

    pub(crate) fn verify_ak820_firmware(
        &self,
        progress: impl Fn(&str) -> Result<(), String>,
    ) -> Result<()> {
        ensure!(self.ak820.is_some(), "Not an AK820 connection");
        ensure!(
            self.get_protocol_version()? == 9,
            "Unknown AK820 VIA protocol"
        );
        ensure!(
            self.get_keyboard_id()? == (6, crate::ak820::VIAL_UID),
            "Unknown AK820 Vial identity"
        );
        let manifest = self.ak820_read_rom(0xEF80, 64)?;
        let parsed = crate::ak820::parse_manifest(&manifest).map_err(|e| {
            anyhow::anyhow!("Unknown or trial AK820 firmware ({e:?}); settings remain read-only")
        })?;
        let expected: String = parsed
            .code_sha256
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        ensure!(
            expected == crate::ak820::CODE_SHA256,
            "This AK820 build has no reviewed capability entry; settings remain read-only"
        );
        let mut hash = Sha256::new();
        for start in (0x2800..0xC700).step_by(27 * 32) {
            progress(&format!(
                "Checking AK820 firmware… {}%",
                (start - 0x2800) * 100 / (0xC700 - 0x2800)
            ))
            .map_err(anyhow::Error::msg)?;
            hash.update(self.ak820_read_rom(start, (0xC700 - start).min(27 * 32))?);
        }
        let digest: [u8; 32] = hash.finalize().into();
        ensure!(
            crate::ak820::recognize_image(&manifest, &digest),
            "AK820 CODE differs from the catalog or manifest; settings remain read-only"
        );
        self.ak820_storage_check()?;
        self.ak820
            .as_ref()
            .unwrap()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .verified = true;
        Ok(())
    }

    pub(super) fn ak820_send_or_passthrough(&self, data: &[u8]) -> Result<[u8; MSG_LEN]> {
        let Some(state) = &self.ak820 else {
            return self.usb_send_traced(data);
        };
        ensure!(
            !data.is_empty() && data.len() <= MSG_LEN,
            "Malformed AK820 HID request"
        );
        let p = padded_request(data);
        let write = allowed_request(&p)?;
        let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(reason) = &state.failure {
            bail!("AK820 operation stopped: {reason}. Select Read settings again before editing");
        }
        if write {
            ensure!(
                state.verified && state.loaded,
                "AK820 settings remain read-only until the complete device state is read"
            );
        }
        let operation = (|| {
            let unlock = p[0] == 0xFE && (6..=8).contains(&p[1]);
            if write && !unlock {
                storage_ready(&self.usb_send_traced(&[0xFD, 1])?)?;
            }
            let reply = self.usb_send_traced(data)?;
            if !super::response_matches_command(data, &reply) {
                bail!("AK820 reply does not match the request");
            }
            if write {
                self.ak820_verify_write(&p, &reply)?;
                if !unlock {
                    storage_ready(&self.usb_send_traced(&[0xFD, 1])?)?;
                }
            }
            Ok(reply)
        })();
        if let Err(error) = &operation {
            state.failure = Some(format!("{error:#}"));
        }
        operation
    }

    fn ak820_verify_write(&self, p: &[u8; MSG_LEN], reply: &[u8; MSG_LEN]) -> Result<()> {
        match (p[0], p[1]) {
            (0x05, _) => {
                ensure!(reply == p, "AK820 key setter was rejected");
                let actual = self.usb_send_traced(&[4, p[1], p[2], p[3]])?;
                ensure!(
                    actual[..4] == [4, p[1], p[2], p[3]] && actual[4..6] == p[4..6],
                    "AK820 key readback differs"
                );
            }
            (0xFE, 4) => {
                ensure!(reply == p, "AK820 encoder setter was rejected");
                let actual = self.usb_send_traced(&[0xFE, 3, p[2], p[3]])?;
                let at = p[4] as usize * 2;
                ensure!(
                    actual[at..at + 2] == p[5..7],
                    "AK820 encoder readback differs"
                );
            }
            (0xFE, 13) => {
                ensure!(
                    reply[0] == 0 && reply[1..14] == p[1..14],
                    "AK820 Combo setter was rejected"
                );
                let actual = self.usb_send_traced(&[0xFE, 13, 3, p[3]])?;
                ensure!(
                    actual[0] == 0 && actual[1..11] == p[4..14],
                    "AK820 Combo readback differs"
                );
            }
            (0x07, 0x41) => {
                ensure!(reply == p, "AK820 RGB setter was rejected");
                let actual = self.usb_send_traced(&[8, 0x41])?;
                ensure!(
                    actual[..2] == [8, 0x41] && actual[2..8] == p[2..8],
                    "AK820 RGB readback differs"
                );
            }
            (0x09, _) => {
                ensure!(reply == p, "AK820 RGB save was rejected");
                let actual = self.usb_send_traced(&[8, 0x41])?;
                let saved = self.usb_send_traced(&[0xFD, 0, 0xC7, 0x18, 8])?;
                ensure!(
                    saved[..5] == [0xFD, 0, 0xC7, 0x18, 8],
                    "AK820 saved RGB reply differs"
                );
                let raw = &saved[5..13];
                ensure!(
                    raw[0] & 3 <= 1 && raw[3] <= 160,
                    "AK820 saved RGB configuration is invalid"
                );
                let selected = match raw[0] >> 2 {
                    1 => 2,
                    2 => 6,
                    3 => 13,
                    4 => 14,
                    5 => 1,
                    6 => 31,
                    _ => bail!("AK820 saved RGB mode is invalid"),
                };
                let effective = if raw[0] & 3 == 0 { 0 } else { selected };
                ensure!(
                    actual[..8] == [8, 0x41, effective, 0, raw[4], raw[1], raw[2], raw[3]],
                    "AK820 saved RGB settings differ from the current state"
                );
            }
            (0x0F, _) => ensure!(reply == p, "AK820 macro setter was rejected"),
            (0xFE, 6..=8) => {}
            _ => bail!("Unverified AK820 setter"),
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn test_ak820_device(ready: bool) -> (Self, super::TestHidRecorder) {
        let (mut hid, recorder) = Self::test_device();
        hid.ak820 = Some(std::sync::Mutex::new(Ak820Session {
            verified: ready,
            loaded: ready,
            failure: None,
        }));
        (hid, recorder)
    }

    #[cfg(test)]
    pub(crate) fn test_verified_ak820_device() -> (Self, super::TestHidRecorder) {
        let (hid, recorder) = Self::test_ak820_device(false);
        hid.ak820.as_ref().unwrap().lock().unwrap().verified = true;
        (hid, recorder)
    }
}

#[cfg(test)]
#[path = "hid_ak820_tests.rs"]
mod transaction_tests;

#[cfg(test)]
mod tests {
    use super::*;
    fn ready() -> [u8; 32] {
        let mut r = [0; 32];
        r[..4].copy_from_slice(&[0xFD, 1, 1, 0xA8]);
        r
    }

    #[test]
    fn macros_reject_incomplete_oversized_and_binary_zero_operands() {
        assert_eq!(macro_stream_length(&[0; 16]).unwrap(), 16);
        assert!(macro_stream_length(&[0; 15]).is_err());
        assert!(macro_stream_length(&[0; 8193]).is_err());
        let mut bytes = vec![1, 5, 0, 4];
        bytes.extend([0; 16]);
        assert!(macro_stream_length(&bytes).is_err());
        bytes[2] = 1;
        assert!(macro_stream_length(&bytes).is_ok());
    }

    #[test]
    fn unknown_session_cannot_write_or_share_host_output() {
        let (hid, log) = HidDevice::test_ak820_device(false);
        assert!(hid.set_keycode(0, 0, 0, 4).is_err());
        assert!(hid.write_output_report(&[0xAF]).is_err());
        assert!(hid.shared_output().is_none());
        assert!(log.requests().is_empty());
    }

    #[test]
    fn verified_image_stays_read_only_until_settings_are_loaded() {
        let (hid, log) = HidDevice::test_verified_ak820_device();
        assert!(!hid.ak820_editing_ready());
        assert!(hid.set_keycode(0, 0, 0, 4).is_err());
        assert!(log.requests().is_empty());
        hid.ak820_enable_settings().unwrap();
        assert!(hid.ak820_editing_ready());
    }

    #[test]
    fn mismatch_latches_and_never_retries_or_writes_next_key() {
        let (hid, log) = HidDevice::test_ak820_device(true);
        log.respond_by(|p| {
            Some(Ok(if p[0] == 0xFD {
                ready()
            } else if p[0] == 4 {
                let mut r = *p;
                r[5] = 5;
                r
            } else {
                *p
            }))
        });
        assert!(hid.set_keycode(0, 0, 0, 4).is_err());
        let before = log.requests().len();
        assert!(hid.set_keycode(0, 0, 1, 5).is_err());
        assert_eq!(log.requests().len(), before);
        assert_eq!(log.requests().iter().filter(|p| p[0] == 5).count(), 1);
        assert_eq!(log.requests().iter().filter(|p| p[0] == 4).count(), 1);
    }

    #[test]
    fn failed_storage_blocks_the_first_setter() {
        let (hid, log) = HidDevice::test_ak820_device(true);
        let mut failed = ready();
        failed[4] = 1;
        log.respond_by(move |_| Some(Ok(failed)));
        assert!(hid.set_encoder(0, 0, 1, 4).is_err());
        assert_eq!(log.requests().len(), 1);
        assert_eq!(log.requests()[0][..2], [0xFD, 1]);
    }

    #[test]
    fn unsupported_commands_never_reach_transport() {
        let (hid, log) = HidDevice::test_ak820_device(true);
        for p in [
            &[0x0B][..],
            &[0x0A],
            &[0xFE, 0x0B, 1],
            &[0x03, 1],
            &[0xFE, 4, 6, 0, 0, 0, 4],
        ] {
            assert!(hid.usb_send(p).is_err());
        }
        assert!(log.requests().is_empty());
    }

    #[test]
    fn every_ak820_request_has_one_transport_attempt() {
        for p in [
            &[0x05][..],
            &[0x0F],
            &[0x07],
            &[0x09],
            &[0xFE, 4],
            &[0xFD, 0],
            &[0x11],
        ] {
            assert_eq!(
                super::super::usb_send_max_attempts(super::super::HidTransport::Ak820Usb, p),
                1
            );
        }
    }
}
