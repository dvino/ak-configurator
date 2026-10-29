//! AK820's checked settings path. The proxy owns transport; this parent-side
//! session serializes a setter with its storage check and exact readback.
use super::{HidDevice, MSG_LEN, padded_request};
use crate::ak820::ImageContract;
use crate::app::ak820_beta1_profiles::KeycodeSource;
use anyhow::{Context, Result, bail, ensure};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, Ordering};

/// An ownership observation, valid only with the same live HID handle. It is
/// never serialized. Negotiated Direct v1 sessions retain the original RAM palette.
#[derive(Clone, Debug)]
pub(crate) struct Ak820RgbStream {
    mode: [u8; MSG_LEN],
    previous: Option<[u8; MSG_LEN]>,
    image: ImageContract,
    owner: std::sync::Arc<()>,
    direct_previous: Option<crate::ak820_rgb::direct::Snapshot>,
    direct_generation: Option<u32>,
}

impl Ak820RgbStream {
    pub(crate) fn can_restore_builtin(&self) -> bool {
        self.previous.is_some() || self.direct_previous.is_some()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Ak820RgbRestoreResult {
    Restored([u8; 6]),
    Unavailable,
    OwnershipLost,
    Cancelled,
}

#[derive(Debug)]
pub(crate) enum Ak820RgbStreamResult {
    Applied(Ak820RgbStream),
    Cancelled,
    OwnershipLost,
}

#[derive(Default)]
pub(super) struct Ak820Session {
    image: Option<ImageContract>,
    loaded: bool,
    failure: Option<String>,
    rgb_owner: Option<std::sync::Arc<()>>,
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
fn validate_macro_upload(stream: &[u8], image: ImageContract) -> Result<usize> {
    validate_macro_actions(stream, image, true)
}

fn validate_macro_actions(stream: &[u8], image: ImageContract, assignment: bool) -> Result<usize> {
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
            let source = match opcode {
                1 | 5 => KeycodeSource::MacroTap,
                2 | 6 => KeycodeSource::MacroDown,
                _ => KeycodeSource::MacroUp,
            };
            ensure!(
                if assignment {
                    image.assignable(keycode, source)
                } else {
                    image.supports_keycode(keycode, source)
                },
                "Macro keycode {keycode:#06X} is unavailable in the AK820 editor; nothing was written"
            );
        }
        offset += 2 + operands;
    }
    Ok(length)
}

/// Whitelist the application's existing settings protocol. No reset, loader,
/// factory erase, unknown extensions or speculative QMK setters reach HID.
fn allowed_request(p: &[u8; MSG_LEN], image: Option<ImageContract>) -> Result<bool> {
    let tail = |at: usize| p[at..].iter().all(|&b| b == 0);
    let be = |at| u16::from_be_bytes([p[at], p[at + 1]]) as usize;
    let matrix = || p[1] < 6 && p[2] < 16 && p[3] < 8;
    let (valid, write) = match p[0] {
        0x01 | 0x0C | 0x0D | 0x11 => (tail(1), false),
        0x02 => ((p[1] == 2 || p[1] == 3) && tail(2), false),
        0x03 => (p[1] == 2 && p[2..5] == [0, 0, 0] && tail(6), true),
        0x04 => (matrix() && tail(4), false),
        0x05 => (
            matrix()
                && tail(6)
                && crate::ak820::MATRIX_MASK[p[2] as usize] & (1 << p[3]) != 0
                && image.is_some_and(|i| i.assignable(be(4) as u16, KeycodeSource::Matrix)),
            true,
        ),
        0x07 => (
            p[1] == 0x41
                && tail(8)
                && image.is_some_and(|i| i.rgb_modes().contains(&p[2]))
                && p[3] == 0
                && p[7] <= 160,
            true,
        ),
        0x08 => (
            match p[1] {
                0x40 | 0x41 => tail(2),
                0x43 => tail(2) && image.is_some_and(|i| i.rgb_modes().contains(&1)),
                0x42 => tail(4),
                0x44 => {
                    p[2] < 82
                        && p[3] == 0
                        && tail(4)
                        && image.is_some_and(|i| i.rgb_modes().contains(&1))
                }
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
            0 | 1 | 5 => (tail(2), false),
            9 => (tail(4), false),
            10 => {
                let id = u16::from_le_bytes([p[2], p[3]]);
                (
                    tail(4) && image.is_some_and(|i| i.qsid_ids().contains(&id)),
                    false,
                )
            }
            11 => {
                let id = u16::from_le_bytes([p[2], p[3]]);
                let value = u16::from_le_bytes([p[4], p[5]]);
                let valid = image.is_some_and(|i| i.qsid_ids().contains(&id))
                    && tail(if id == 1 { 5 } else { 6 })
                    && crate::ak820_typed_storage::QmkSettings::default()
                        .set(id, value)
                        .is_ok();
                (valid, true)
            }
            2 => (p[4] == 0 && p[5] == 0 && tail(6), false),
            3 => (p[2] < 6 && p[3] == 0 && tail(4), false),
            4 => (
                p[2] < 6
                    && p[3] == 0
                    && p[4] <= 1
                    && tail(7)
                    && image.is_some_and(|i| i.assignable(be(5) as u16, KeycodeSource::Encoder)),
                true,
            ),
            // Unlock is available only as an explicit UI action; connection is
            // read-only and never starts, polls or cancels an unlock itself.
            6 | 7 | 8 => (tail(2), true),
            13 => match p[2] {
                0 => (tail(3), false),
                3 => (p[3] < 8 && tail(4), false),
                4 => (
                    tail(14) && image.is_some_and(|i| i.valid_combo_record(&p[..14])),
                    true,
                ),
                1 | 5 => {
                    let count = image
                        .map(|i| {
                            if p[2] == 1 {
                                i.dynamic_counts().0
                            } else {
                                i.dynamic_counts().2
                            }
                        })
                        .unwrap_or(0);
                    (p[3] < count && tail(4), false)
                }
                2 | 6 => {
                    let count = image
                        .map(|i| {
                            if p[2] == 2 {
                                i.dynamic_counts().0
                            } else {
                                i.dynamic_counts().2
                            }
                        })
                        .unwrap_or(0);
                    let record_ok = if p[2] == 2 {
                        crate::ak820_typed_storage::TapDance::decode(&p[4..14]).is_ok()
                    } else {
                        crate::ak820_typed_storage::KeyOverride::decode(&p[4..14]).is_ok()
                    };
                    (p[3] < count && tail(14) && record_ok, true)
                }
                _ => (false, true),
            },
            _ => (false, true),
        },
        _ => (false, true),
    };
    ensure!(
        valid,
        "This command or value is unavailable in the verified AK820 image"
    );
    let beta4_write_authorized = match image {
        Some(ImageContract::Beta4(profile)) => {
            crate::app::ak820_beta1_profiles::beta4_settings_writes_authorized(profile)
        }
        _ => true,
    };
    ensure!(
        !write || beta4_write_authorized,
        "AK820 beta.4 release metadata does not authorize device writes"
    );
    Ok(write)
}

fn typed_setter(p: &[u8; MSG_LEN]) -> bool {
    p[0] == 0xFE && (p[1] == 11 || (p[1] == 13 && matches!(p[2], 2 | 6)))
}

fn typed_candidate(p: &[u8; MSG_LEN], image: ImageContract, before: &[u8]) -> Result<Vec<u8>> {
    use crate::ak820_typed_storage::{Extensions, KeyOverride, TapDance};
    let contract = image
        .matrix_profile()
        .context("Typed settings require an exact matrix image contract")?;
    crate::ak820_settings_model::validate_nvm(before, image.schema())
        .map_err(anyhow::Error::msg)?;
    let mut model = Extensions::decode_nvm(before).map_err(anyhow::Error::msg)?;
    match (p[1], p[2]) {
        (11, _) => model
            .settings
            .set(
                u16::from_le_bytes([p[2], p[3]]),
                u16::from_le_bytes([p[4], p[5]]),
            )
            .map_err(anyhow::Error::msg)?,
        (13, 2) => {
            model.tap_dance[p[3] as usize] =
                TapDance::decode(&p[4..14]).map_err(anyhow::Error::msg)?
        }
        (13, 6) => {
            model.key_overrides[p[3] as usize] =
                KeyOverride::decode(&p[4..14]).map_err(anyhow::Error::msg)?
        }
        _ => bail!("Unknown typed setting"),
    }
    model
        .apply_to_nvm_for_contract(before, contract)
        .map_err(anyhow::Error::msg)
}

#[derive(Debug)]
struct CaptureCancelled;
impl std::fmt::Display for CaptureCancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Backup capture cancelled")
    }
}
impl std::error::Error for CaptureCancelled {}
fn capture_check_cancel(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Acquire) {
        return Err(CaptureCancelled.into());
    }
    Ok(())
}

fn capture_decode_rom(image: ImageContract, rom: &[u8], nvm: &[u8]) -> Result<serde_json::Value> {
    let exact = image
        .matrix_profile()
        .context("Missing exact image profile")?
        .settings_identity
        .as_ref()
        .context("Image lacks qualified settings identity")?;
    let identity = crate::ak820_settings_backup::ImageIdentity {
        ihx_sha256: exact.ihx_sha256.clone(),
        code_sha256: exact.code_sha256.clone(),
        definition_sha256: exact.definition_sha256.clone(),
        schema: exact.nvm_schema,
    };
    crate::ak820_settings_model::decode_trusted_rom_for_image(rom, nvm, &identity, image)
        .map_err(anyhow::Error::msg)
}

// Private reconciliation seam: tests model getters without manufacturing a
// trusted full ROM. Production always validates the exact ROM first.
fn capture_reconcile_getters(
    profile: crate::app::ak820_beta1_profiles::ProfileId,
    image: ImageContract,
    nvm: &[u8],
    settings: &serde_json::Value,
    send: &mut impl FnMut(&[u8]) -> Result<[u8; MSG_LEN]>,
) -> Result<bool> {
    let contract = image
        .matrix_profile()
        .context("Missing exact image profile")?;
    for key in settings["keys"]
        .as_array()
        .context("Missing captured keys")?
    {
        let request = [
            4,
            key["layer"].as_u64().unwrap() as u8,
            key["row"].as_u64().unwrap() as u8,
            key["column"].as_u64().unwrap() as u8,
        ];
        let reply = send(&request)?;
        ensure!(
            reply[..4] == request
                && u16::from_be_bytes([reply[4], reply[5]]) as u64
                    == key["keycode"].as_u64().unwrap(),
            "Captured key getter differs"
        );
    }
    for layer in 0..6 {
        let reply = send(&[0xfe, 3, layer as u8, 0])?;
        ensure!(
            reply[..2] == nvm[0x642 + layer * 4..0x644 + layer * 4]
                && reply[2..4] == nvm[0x640 + layer * 4..0x642 + layer * 4],
            "Captured encoder getter differs"
        );
    }
    let layout = send(&[2, 2])?;
    ensure!(
        layout[..2] == [2, 2]
            && u32::from_be_bytes(layout[2..6].try_into().unwrap()) == nvm[40] as u32,
        "Captured layout getter differs"
    );
    let counts = send(&[0xfe, 13, 0])?;
    ensure!(
        counts[0] == contract.capabilities.dynamic_entries.tap_dance
            && counts[1] == contract.capabilities.dynamic_entries.combos
            && counts[2] == contract.capabilities.dynamic_entries.key_overrides,
        "Captured dynamic capability counts differ"
    );
    let qsid_reply = send(&[0xfe, 9, 0, 0])?;
    let ids: Vec<u16> = qsid_reply
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect();
    let end = ids
        .iter()
        .position(|&id| id == 0xffff)
        .context("Captured QSID query lacks terminator")?;
    ensure!(
        ids[..end] == contract.capabilities.runtime_qmk_settings_ids
            && ids[end..].iter().all(|&id| id == 0xffff),
        "Captured QSID capabilities differ"
    );
    let macro_count = send(&[0x0c])?;
    let macro_size = send(&[0x0d])?;
    ensure!(
        macro_count[1] == 16 && macro_size[1..3] == [0x20, 0],
        "Captured macro capabilities differ"
    );
    let length = u16::from_le_bytes([nvm[0x7f1], nvm[0x7f2]]) as usize;
    for offset in (0..length).step_by(28) {
        let count = (length - offset).min(28);
        let request = [0x0e, (offset >> 8) as u8, offset as u8, count as u8];
        let reply = send(&request)?;
        ensure!(
            reply[..4] == request
                && reply[4..4 + count] == nvm[0x800 + offset..0x800 + offset + count],
            "Captured macro getter differs"
        );
    }
    let (qs_values, tap_dance, key_overrides) = if matches!(image, ImageContract::Beta4(_)) {
        use crate::ak820_qs_versioned_offline::{ImageVersion, SettingsRecord};
        let qs = SettingsRecord::decode_for_image(
            &nvm[0x6a8..0x6c0],
            ImageVersion::V2,
            ImageVersion::V2,
        )
        .map_err(anyhow::Error::msg)?;
        let extensions = crate::ak820_typed_storage::NonQsExtensions::decode_nvm(nvm)
            .map_err(anyhow::Error::msg)?;
        let values = image
            .qsid_ids()
            .iter()
            .map(|&id| qs.get(id).map(|value| (id, value)))
            .collect::<std::result::Result<std::collections::BTreeMap<_, _>, _>>()
            .map_err(anyhow::Error::msg)?;
        (values, extensions.tap_dance, extensions.key_overrides)
    } else {
        let extensions = crate::ak820_typed_storage::Extensions::decode_nvm(nvm)
            .map_err(anyhow::Error::msg)?;
        let values = image
            .qsid_ids()
            .iter()
            .map(|&id| extensions.settings.get(id).map(|value| (id, value)))
            .collect::<std::result::Result<std::collections::BTreeMap<_, _>, _>>()
            .map_err(anyhow::Error::msg)?;
        (values, extensions.tap_dance, extensions.key_overrides)
    };
    for &id in &contract.capabilities.runtime_qmk_settings_ids {
        let reply = send(&[0xfe, 10, id as u8, (id >> 8) as u8])?;
        let actual = if matches!(id, 1 | 3 | 22) {
            reply[1] as u16
        } else {
            u16::from_le_bytes([reply[1], reply[2]])
        };
        ensure!(
            reply[0] == 0 && qs_values.get(&id) == Some(&actual),
            "Captured QMK getter differs"
        );
    }
    for (op, count) in [
        (1, contract.capabilities.dynamic_entries.tap_dance),
        (5, contract.capabilities.dynamic_entries.key_overrides),
    ] {
        for index in 0..count {
            let reply = send(&[0xfe, 13, op, index as u8])?;
            let expected = if op == 1 {
                tap_dance[index as usize].encode()
            } else {
                key_overrides[index as usize].encode()
            }
            .map_err(anyhow::Error::msg)?;
            ensure!(
                reply[0] == 0 && reply[1..11] == expected,
                "Captured extension getter differs"
            );
        }
    }
    if let Some(entries) = settings["combos"]["entries_hex"].as_array() {
        for (index, entry) in entries.iter().enumerate() {
            let reply = send(&[0xfe, 13, 3, index as u8])?;
            let hex: String = reply[1..11].iter().map(|b| format!("{b:02x}")).collect();
            ensure!(
                reply[0] == 0 && Some(hex.as_str()) == entry.as_str(),
                "Captured Combo getter differs"
            );
        }
    }
    let rgb = send(&[8, 0x41])?;
    ensure!(
        rgb[..2] == [8, 0x41]
            && rgb[3] == 0
            && image.rgb_modes().contains(&rgb[2])
            && rgb[7] <= 160,
        "Captured live RGB is invalid"
    );
    let saved_rgb =
        crate::ak820_settings_backup::rgb_wire_for_profile(&nvm[24..32], false, profile)
            .map_err(anyhow::Error::msg)?;
    Ok(rgb[2..8] == saved_rgb)
}

impl HidDevice {
    /// One volatile FD02 exchange on the verified existing owner. This does
    /// not enter a saved-setting transaction or perform any NVM operation.
    pub(crate) fn ak820_os_indicator_set(
        &self,
        mode: crate::ak820_os_indicator::Mode,
    ) -> Result<()> {
        let request = crate::ak820_os_indicator::request(mode);
        if let Some(session) = &self.read_only {
            return Err(session.refuse(&request));
        }
        let state = self.ak820.as_ref().context("Not an AK820 session")?;
        let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
        ensure!(
            state.loaded && state.failure.is_none(),
            "Read AK820 settings before OS indication"
        );
        let image = state
            .image
            .context("Exact image required for OS indication")?;
        ensure!(
            image.has_feature("os-indicators"),
            "Image has no OS indicator module"
        );
        let result = self.usb_send_traced(&request).and_then(|reply| {
            crate::ak820_os_indicator::validate_reply(mode, &reply).map_err(anyhow::Error::msg)
        });
        if let Err(error) = &result {
            state.failure = Some(format!("{error:#}"));
        }
        result
    }

    /// Capture on the existing owner only. No unlock, setter or retry is issued.
    pub(crate) fn ak820_capture_backup(
        &self,
        cancel: &AtomicBool,
        progress: &std::sync::atomic::AtomicU32,
    ) -> Result<crate::ak820_backup_capture::CapturedBackup> {
        progress.store(0, Ordering::Release);
        let state = self.ak820.as_ref().context("Not an AK820 session")?;
        let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
        ensure!(
            state.loaded && state.failure.is_none(),
            "Read complete AK820 settings before backup"
        );
        let image = state.image.context("AK820 image is not verified")?;
        let profile = image
            .profile()
            .context("Complete backup requires an exact beta2 profile")?;
        let result = (|| {
            let mut packets = 0u32;
            let mut send = |request: &[u8]| -> Result<[u8; MSG_LEN]> {
                capture_check_cancel(cancel)?;
                // Only fixed read-only requests are constructed below. Keep the
                // session lock across the whole snapshot to exclude setters.
                let reply = self
                    .usb_send_traced(request)
                    .map_err(|_| anyhow::anyhow!("Backup capture read failed"))?;
                ensure!(
                    super::response_matches_command(request, &reply),
                    "Backup capture reply differs"
                );
                packets += 1;
                progress.store((packets * 99 / 4000).min(99), Ordering::Release);
                Ok(reply)
            };
            let initial_storage = send(&[0xfd, 1])?;
            storage_ready(&initial_storage)?;
            let protocol = send(&[1])?;
            ensure!(protocol[1..3] == [0, 9], "Backup VIA protocol differs");
            let uid = send(&[0xfe, 0])?;
            ensure!(
                u32::from_le_bytes(uid[..4].try_into().unwrap()) == 6
                    && u64::from_le_bytes(uid[4..12].try_into().unwrap()) == crate::ak820::VIAL_UID,
                "Backup Vial identity differs"
            );
            fn read(
                send: &mut impl FnMut(&[u8]) -> Result<[u8; MSG_LEN]>,
                start: usize,
                len: usize,
            ) -> Result<Vec<u8>> {
                let mut bytes = Vec::with_capacity(len);
                while bytes.len() < len {
                    let at = start + bytes.len();
                    let count = (len - bytes.len()).min(27);
                    let request = [0xfd, 0, (at >> 8) as u8, at as u8, count as u8];
                    let reply = send(&request)?;
                    ensure!(reply[..5] == request, "Backup ROM reply range differs");
                    bytes.extend_from_slice(&reply[5..5 + count]);
                }
                Ok(bytes)
            }
            let rom = read(&mut send, 0, 65536)?;
            let nvm = &rom[0xc700..0xef00];
            // Production decoder is mandatory before interpreting offsets or
            // accepting any live getter reconciliation.
            let settings = capture_decode_rom(image, &rom, nvm)
                .map_err(|_| anyhow::anyhow!("Captured ROM failed exact archive validation"))?;
            let rgb_matches_saved =
                capture_reconcile_getters(profile, image, nvm, &settings, &mut send)?;
            let repeated_nvm = read(&mut send, 0xc700, 0x2800)?;
            let repeated_manifest = read(&mut send, 0xef80, 64)?;
            crate::ak820_backup_capture::check_stability(&rom, &repeated_nvm, &repeated_manifest)
                .map_err(anyhow::Error::msg)?;
            let final_storage = send(&[0xfd, 1])?;
            storage_ready(&final_storage)?;
            ensure!(
                final_storage == initial_storage,
                "Storage changed during backup capture"
            );
            capture_check_cancel(cancel)?;
            crate::ak820_backup_capture::CapturedBackup::from_rom(profile, rom, rgb_matches_saved)
                .map_err(|_| anyhow::anyhow!("Captured backup validation failed"))
        })();
        if let Err(error) = &result {
            if error.downcast_ref::<CaptureCancelled>().is_none() {
                state.failure = Some(format!("{error:#}"));
            }
        } else {
            progress.store(100, Ordering::Release);
        }
        result
    }

    pub(crate) fn ak820_validate_macro_snapshot(&self, data: &[u8]) -> Result<()> {
        let image = self.ak820_image().context("AK820 image is not verified")?;
        validate_macro_actions(data, image, false)?;
        Ok(())
    }

    /// Read the stored RAM palette without selecting Direct or enabling RGB.
    /// Exact catalog identity and runtime negotiation are both mandatory.
    pub(crate) fn ak820_direct_snapshot(&self) -> Result<crate::ak820_rgb::direct::Snapshot> {
        self.ak820_direct_snapshot_with_cancel(&AtomicBool::new(false))
    }

    pub(crate) fn ak820_direct_snapshot_with_cancel(
        &self,
        cancel: &AtomicBool,
    ) -> Result<crate::ak820_rgb::direct::Snapshot> {
        let state = self.ak820.as_ref().context("Not an AK820 session")?;
        let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
        let image = state.image.context("AK820 image is not verified")?;
        ensure!(
            image.direct_protocol() == Some(1),
            "This exact image has no Direct v1 palette getter"
        );
        ensure!(
            state.failure.is_none(),
            "Read AK820 settings again after the failed operation"
        );
        let result = crate::ak820_rgb::direct::snapshot(&mut |p: &[u8]| {
            crate::ak820_rgb::direct::check_cancel(cancel)?;
            self.usb_send_traced(p)
        });
        if let Err(error) = &result {
            if error
                .downcast_ref::<crate::ak820_rgb::direct::Cancelled>()
                .is_none()
            {
                state.failure = Some(format!("{error:#}"));
            }
        }
        result
    }

    /// Explicit separate RAM import. Uses a fresh generation and never selects
    /// Direct, enables lighting, saves EEPROM, retries or restores after failure.
    pub(crate) fn ak820_restore_direct_palette(
        &self,
        palette: &crate::ak820_rgb::direct::Snapshot,
    ) -> Result<()> {
        self.ak820_restore_direct_palette_with_cancel(palette, &AtomicBool::new(false))
    }

    pub(crate) fn ak820_restore_direct_palette_with_cancel(
        &self,
        palette: &crate::ak820_rgb::direct::Snapshot,
        cancel: &AtomicBool,
    ) -> Result<()> {
        use crate::ak820_rgb::direct;
        ensure!(
            palette.hsv.iter().all(|c| c[2] <= 160),
            "Direct RAM brightness exceeds contract"
        );
        if let Some(read_only) = &self.read_only {
            return Err(read_only.refuse(&[7, 0x43]));
        }
        let state = self.ak820.as_ref().context("Not an AK820 session")?;
        let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
        ensure!(
            state.image.is_some_and(|i| i.direct_protocol() == Some(1))
                && state.loaded
                && state.failure.is_none(),
            "Read an exact Direct v1 image before restoring its RAM palette"
        );
        state.rgb_owner = None;
        let result: Result<()> = (|| {
            let mut send = |p: &[u8]| {
                direct::check_cancel(cancel)?;
                self.usb_send_traced(p)
            };
            storage_ready(&send(&[0xfd, 1])?)?;
            let fresh = direct::snapshot(&mut send)?;
            direct::commit(
                &mut send,
                &palette.hsv,
                (fresh.generation as u16).wrapping_add(1).max(1),
                fresh.generation,
                cancel,
            )?;
            storage_ready(&send(&[0xfd, 1])?)?;
            Ok(())
        })();
        if let Err(error) = &result {
            if error.downcast_ref::<direct::Cancelled>().is_none() {
                state.failure = Some(format!("{error:#}"));
            }
        }
        result
    }

    /// Confirm the pinned matrix-to-LED mapping using read-only VialRGB metadata.
    /// This is available only on the four beta.2 profiles with Direct support.
    pub(crate) fn ak820_validate_led_map(&self) -> Result<()> {
        use crate::ak820_rgb::{LED_COUNT, SLOT_MAP};
        let image = self.ak820_image().context("AK820 image is not verified")?;
        ensure!(
            image.profile().is_some() && image.rgb_modes().contains(&1),
            "This image has no beta.2 Direct RGB contract"
        );
        let count = self.usb_send(&[8, 0x43])?;
        ensure!(
            count[..4] == [8, 0x43, LED_COUNT as u8, 0],
            "AK820 LED count differs from the image contract"
        );
        for (index, &slot) in SLOT_MAP.iter().enumerate() {
            let info = self.usb_send(&[8, 0x44, index as u8, 0])?;
            ensure!(
                info[..2] == [8, 0x44] && info[4..7] == [4, slot >> 3, slot & 7],
                "AK820 LED {index} mapping differs from the image contract"
            );
        }
        Ok(())
    }

    /// Explicit static palette apply under the single session owner. Exact
    /// Direct v1 images commit atomically with readback; legacy fastset images
    /// provide acknowledgements only.
    pub(crate) fn ak820_apply_palette(
        &self,
        frame: &[[u8; 3]; crate::ak820_rgb::LED_COUNT],
    ) -> Result<()> {
        match self.ak820_stream_rgb_frame(frame, None, &AtomicBool::new(false))? {
            Ak820RgbStreamResult::Applied(_) => Ok(()),
            _ => bail!("AK820 palette was not applied"),
        }
    }

    /// Send one bounded frame through the existing session owner. Only an
    /// explicit start enters Direct. Later frames never select a mode, and
    /// yield when a physical lighting control changes it. Cancellation is
    /// checked between packets; already acknowledged RAM writes remain.
    pub(crate) fn ak820_stream_rgb_frame(
        &self,
        frame: &[[u8; 3]; crate::ak820_rgb::LED_COUNT],
        owned: Option<&Ak820RgbStream>,
        cancel: &AtomicBool,
    ) -> Result<Ak820RgbStreamResult> {
        let packets = crate::ak820_rgb::frame_packets(frame).map_err(anyhow::Error::msg)?;
        if let Some(read_only) = &self.read_only {
            return Err(read_only.refuse(&packets[0]));
        }
        let state = self.ak820.as_ref().context("Not an AK820 session")?;
        let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
        let image = state.image.context("AK820 image is not verified")?;
        ensure!(
            image.profile().is_some() && image.rgb_modes().contains(&1),
            "Direct RGB is unavailable in this exact image"
        );
        ensure!(
            image.direct_protocol() == Some(1) || matches!(image, ImageContract::Beta2(_)),
            "This image has neither the exact Direct v1 contract nor the legacy beta.2 contract"
        );
        ensure!(
            state.loaded && state.failure.is_none(),
            "Read complete AK820 settings before applying RGB"
        );
        if owned.is_some_and(|stream| {
            stream.image != image
                || !state
                    .rgb_owner
                    .as_ref()
                    .is_some_and(|owner| std::sync::Arc::ptr_eq(owner, &stream.owner))
        }) {
            return Ok(Ak820RgbStreamResult::OwnershipLost);
        }
        let owner = owned
            .map(|stream| stream.owner.clone())
            .unwrap_or_else(|| std::sync::Arc::new(()));
        state.rgb_owner = Some(owner.clone());
        let result = (|| {
            if cancel.load(Ordering::Acquire) {
                return Ok(Ak820RgbStreamResult::Cancelled);
            }
            storage_ready(&self.usb_send_traced(&[0xFD, 1])?)?;
            let previous = self.usb_send_traced(&[8, 0x41])?;
            ensure!(
                previous[..2] == [8, 0x41]
                    && previous[3] == 0
                    && image.rgb_modes().contains(&previous[2])
                    && previous[7] <= 160,
                "AK820 current RGB mode is invalid"
            );
            if owned.is_some_and(|stream| previous[2..8] != stream.mode[2..8]) {
                return Ok(Ak820RgbStreamResult::OwnershipLost);
            }
            let mut direct_previous = owned.and_then(|stream| stream.direct_previous.clone());
            let mut direct_generation = None;
            if image.direct_protocol() == Some(1) {
                use crate::ak820_rgb::direct;
                let mut send = |p: &[u8]| {
                    direct::check_cancel(cancel)?;
                    self.usb_send_traced(p)
                };
                let generation = if let Some(generation) = owned.and_then(|s| s.direct_generation) {
                    if !direct::wait_ready(&mut send, generation, cancel)? {
                        return Ok(Ak820RgbStreamResult::OwnershipLost);
                    }
                    generation
                } else {
                    let captured = direct::snapshot(&mut send)?;
                    let generation = captured.generation;
                    direct_previous = Some(captured);
                    generation
                };
                let hsv = frame.map(crate::ak820_rgb::rgb_to_hsv);
                let frame_id = (generation as u16).wrapping_add(1).max(1);
                let accepted = direct::commit(&mut send, &hsv, frame_id, generation, cancel)?;
                direct_generation = Some(accepted.generation);
            } else {
                for packet in packets {
                    if cancel.load(Ordering::Acquire) {
                        return Ok(Ak820RgbStreamResult::Cancelled);
                    }
                    let reply = self.usb_send_traced(&packet)?;
                    ensure!(
                        reply == padded_request(&packet),
                        "AK820 Direct RGB packet was not acknowledged"
                    );
                }
            }
            let mut mode = previous;
            mode[2] = 1;
            mode[8..].fill(0);
            if owned.is_none() {
                if cancel.load(Ordering::Acquire) {
                    return Ok(Ak820RgbStreamResult::Cancelled);
                }
                let mut setter = mode;
                setter[0] = 7;
                let reply = self.usb_send_traced(&setter)?;
                self.ak820_verify_write(&setter, &reply, image)?;
            } else {
                let current = self.usb_send_traced(&[8, 0x41])?;
                ensure!(
                    current[..2] == [8, 0x41]
                        && current[3] == 0
                        && image.rgb_modes().contains(&current[2])
                        && current[7] <= 160,
                    "AK820 current RGB mode is invalid"
                );
                if current[2..8] != mode[2..8] {
                    return Ok(Ak820RgbStreamResult::OwnershipLost);
                }
            }
            storage_ready(&self.usb_send_traced(&[0xFD, 1])?)?;
            let previous = owned
                .map(|stream| stream.previous)
                .unwrap_or_else(|| (previous[2] != 1).then_some(previous));
            Ok(Ak820RgbStreamResult::Applied(Ak820RgbStream {
                mode,
                previous,
                image,
                owner,
                direct_previous,
                direct_generation,
            }))
        })();
        let result = match result {
            Err(error)
                if error
                    .downcast_ref::<crate::ak820_rgb::direct::Cancelled>()
                    .is_some() =>
            {
                Ok(Ak820RgbStreamResult::Cancelled)
            }
            other => other,
        };
        if !matches!(&result, Ok(Ak820RgbStreamResult::Applied(_))) {
            state.rgb_owner = None;
        }
        if let Err(error) = &result {
            state.failure = Some(format!("{error:#}"));
        }
        result
    }

    /// Explicit, single-attempt volatile restore. Direct v1 restores the captured
    /// palette even when playback began in Direct. Legacy images can restore
    /// built-in modes only. Errors never trigger a retry or automatic restore.
    pub(crate) fn ak820_restore_rgb_stream(
        &self,
        stream: &Ak820RgbStream,
        cancel: &AtomicBool,
    ) -> Result<Ak820RgbRestoreResult> {
        use Ak820RgbRestoreResult::*;
        if self.read_only.is_some() {
            return Ok(OwnershipLost);
        }
        let Some(state) = &self.ak820 else {
            return Ok(OwnershipLost);
        };
        let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
        if !state.loaded
            || state.failure.is_some()
            || state.image != Some(stream.image)
            || !state
                .rgb_owner
                .as_ref()
                .is_some_and(|owner| std::sync::Arc::ptr_eq(owner, &stream.owner))
        {
            return Ok(OwnershipLost);
        }
        // Retire this observation before the first exchange, including when a
        // read, cancellation, ownership check or write fails.
        state.rgb_owner = None;
        if cancel.load(Ordering::Acquire) {
            return Ok(Cancelled);
        }
        let previous = stream
            .previous
            .or_else(|| stream.direct_previous.as_ref().map(|_| stream.mode));
        let Some(previous) = previous else {
            return Ok(Unavailable);
        };
        if !stream.image.rgb_modes().contains(&previous[2]) {
            return Ok(Unavailable);
        }
        let result = (|| {
            storage_ready(&self.usb_send_traced(&[0xFD, 1])?)?;
            if cancel.load(Ordering::Acquire) {
                return Ok(Cancelled);
            }
            let current = self.usb_send_traced(&[8, 0x41])?;
            ensure!(
                current[..2] == [8, 0x41]
                    && current[3] == 0
                    && stream.image.rgb_modes().contains(&current[2])
                    && current[7] <= 160,
                "AK820 current RGB mode is invalid"
            );
            if current[2..8] != stream.mode[2..8] || current[2] != 1 {
                return Ok(OwnershipLost);
            }
            if stream.image.direct_protocol() == Some(1) {
                use crate::ak820_rgb::direct;
                let mut send = |p: &[u8]| {
                    direct::check_cancel(cancel)?;
                    self.usb_send_traced(p)
                };
                let generation = stream
                    .direct_generation
                    .context("Missing Direct ownership generation")?;
                if !direct::wait_ready(&mut send, generation, cancel)? {
                    return Ok(OwnershipLost);
                }
                let captured = stream
                    .direct_previous
                    .as_ref()
                    .context("Missing original Direct palette")?;
                direct::commit(
                    &mut send,
                    &captured.hsv,
                    (generation as u16).wrapping_add(1).max(1),
                    generation,
                    cancel,
                )?;
                if previous[2] == 1 {
                    return Ok(Restored(previous[2..8].try_into().unwrap()));
                }
            }
            if cancel.load(Ordering::Acquire) {
                return Ok(Cancelled);
            }
            let mut setter = previous;
            setter[0] = 7;
            setter[8..].fill(0);
            let reply = self.usb_send_traced(&setter)?;
            self.ak820_verify_write(&setter, &reply, stream.image)?;
            storage_ready(&self.usb_send_traced(&[0xFD, 1])?)?;
            Ok(Restored(previous[2..8].try_into().unwrap()))
        })();
        let result = match result {
            Err(error)
                if error
                    .downcast_ref::<crate::ak820_rgb::direct::Cancelled>()
                    .is_some() =>
            {
                Ok(Cancelled)
            }
            other => other,
        };
        if let Err(error) = &result {
            state.failure = Some(format!("{error:#}"));
        }
        result
    }

    pub(super) fn ak820_set_macro_buffer(&self, data: &[u8]) -> Result<()> {
        let image = self.ak820_image().context("AK820 image is not verified")?;
        let logical = validate_macro_upload(data, image)?;
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

    pub(crate) fn ak820_image(&self) -> Option<ImageContract> {
        self.ak820
            .as_ref()
            .and_then(|state| state.lock().unwrap_or_else(|e| e.into_inner()).image)
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
            state.image.is_some() && state.failure.is_none(),
            "AK820 image is not verified"
        );
        state.rgb_owner = None;
        state.loaded = true;
        Ok(())
    }

    pub(crate) fn ak820_read_rom(&self, start: usize, length: usize) -> Result<Vec<u8>> {
        self.ak820_read_rom_checked(start, length, || Ok(()))
    }

    fn ak820_read_rom_checked(
        &self, start: usize, length: usize, boundary: impl Fn() -> Result<()>,
    ) -> Result<Vec<u8>> {
        ensure!(
            start <= 65536 && length <= 65536 - start,
            "Invalid AK820 ROM read range"
        );
        let mut bytes = Vec::with_capacity(length);
        while bytes.len() < length {
            boundary()?;
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
        let image = self.ak820_identify_code(progress)?;
        // FD00 recognition precedes FD01; normal getters may flush an already
        // dirty firmware cache and are admitted only after a clean snapshot.
        self.ak820_storage_check()?;
        ensure!(
            self.get_protocol_version()? == 9,
            "Unknown AK820 VIA protocol"
        );
        ensure!(
            self.get_keyboard_id()? == (6, crate::ak820::VIAL_UID),
            "Unknown AK820 Vial identity"
        );
        self.ak820_storage_check()?;
        self.ak820.as_ref().unwrap().lock().unwrap_or_else(|e| e.into_inner()).image = Some(image);
        Ok(())
    }

    /// Shared exact CODE recognition for GUI loading and headless diagnostics.
    /// No normal VIA/Vial getters, settings or diagnostic guesses are sent here.
    fn ak820_identify_code(
        &self,
        progress: impl Fn(&str) -> Result<(), String>,
    ) -> Result<ImageContract> {
        let manifest = self.ak820_read_rom_checked(0xEF80, 64,
            || progress("Reading AK820 manifest").map_err(anyhow::Error::msg))?;
        let parsed = crate::ak820::parse_manifest(&manifest).map_err(|e| {
            anyhow::anyhow!("Unknown or trial AK820 firmware ({e:?}); settings remain read-only")
        })?;
        let expected: String = parsed
            .code_sha256
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        ensure!(
            crate::ak820::contract_for_hash(parsed.schema, &expected).is_some(),
            "This AK820 build has no reviewed capability entry; settings remain read-only"
        );
        let mut hash = Sha256::new();
        for start in (0x2800..0xC700).step_by(27 * 32) {
            let message = format!(
                "Checking AK820 firmware… {}%",
                (start - 0x2800) * 100 / (0xC700 - 0x2800)
            );
            hash.update(self.ak820_read_rom_checked(start, (0xC700 - start).min(27 * 32),
                || progress(&message).map_err(anyhow::Error::msg))?);
        }
        let digest: [u8; 32] = hash.finalize().into();
        let image = crate::ak820::identify_image(&manifest, &digest).context(
            "AK820 CODE differs from the catalog or manifest; settings remain read-only",
        )?;
        Ok(image)
    }

    /// Read-only bootstrap. An unhealthy snapshot is returned for diagnosis,
    /// with no ordinary getters; the caller must keep that session stopped.
    #[cfg(any(feature = "mcp", test))]
    pub(crate) fn ak820_diagnostic_bootstrap(
        &self,
        cancel: &AtomicBool,
    ) -> Result<(ImageContract, crate::ak820_diagnostics::Status)> {
        ensure!(self.ak820.is_some(), "Not an AK820 connection");
        ensure!(self.ak820_failure().is_none(), "AK820 diagnostic session is stopped");
        let started = std::time::Instant::now();
        let boundary = || -> Result<(), String> {
            if cancel.load(Ordering::Acquire) { return Err("AK820 diagnostic operation cancelled".into()); }
            if started.elapsed() > std::time::Duration::from_secs(120) {
                return Err("AK820 diagnostic identity deadline exceeded".into());
            }
            Ok(())
        };
        let result = (|| {
            let image = self.ak820_identify_code(|_| boundary())?;
            ensure!(matches!(image, ImageContract::Beta3(_)),
                "Diagnostic bootstrap supports exact beta.3 images only");
            self.ak820.as_ref().unwrap().lock().unwrap_or_else(|e| e.into_inner()).image = Some(image);
            boundary().map_err(anyhow::Error::msg)?;
            let status = self.ak820_diagnostic_status()?;
            if status.ensure_ready().is_err() {
                return Ok((image, status));
            }
            boundary().map_err(anyhow::Error::msg)?;
            ensure!(self.get_protocol_version()? == 9, "Unknown AK820 VIA protocol");
            boundary().map_err(anyhow::Error::msg)?;
            // Recheck before each ordinary group; a dirty status must never be
            // consumed by a getter that can enter the firmware NVM batching.
            let status = self.ak820_diagnostic_status()?;
            if status.ensure_ready().is_err() { return Ok((image, status)); }
            ensure!(self.get_keyboard_id()? == (6, crate::ak820::VIAL_UID),
                "Unknown AK820 Vial identity");
            boundary().map_err(anyhow::Error::msg)?;
            Ok((image, self.ak820_diagnostic_status()?))
        })();
        if let Err(error) = &result { self.ak820_fail(error); }
        result
    }

    #[cfg(any(feature = "mcp", test))]
    pub(crate) fn ak820_diagnostic_status(&self) -> Result<crate::ak820_diagnostics::Status> {
        ensure!(matches!(self.ak820_image(), Some(ImageContract::Beta3(_))),
            "FD01 diagnostics require an exact beta.3 image");
        ensure!(self.ak820_failure().is_none(), "AK820 diagnostic session is stopped");
        let result = self.usb_send(&[0xfd, 1]).and_then(|reply| crate::ak820_diagnostics::Status::decode(&reply));
        if let Err(error) = &result { self.ak820_fail(error); }
        result
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
        let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(reason) = &state.failure {
            bail!("AK820 operation stopped: {reason}. Select Read settings again before editing");
        }
        let write = allowed_request(&p, state.image)?;
        if write {
            state.rgb_owner = None;
            ensure!(
                state.image.is_some() && state.loaded,
                "AK820 settings remain read-only until the complete device state is read"
            );
        }
        let operation = (|| {
            let unlock = p[0] == 0xFE && (6..=8).contains(&p[1]);
            if write && !unlock {
                storage_ready(&self.usb_send_traced(&[0xFD, 1])?)?;
            }
            let expected_nvm = if write && typed_setter(&p) {
                let image = state.image.context("AK820 image is not verified")?;
                let before = self.ak820_read_rom_traced(0xC700, 0x2800)?;
                let candidate = typed_candidate(&p, image, &before)?;
                if candidate == before {
                    // An unchanged effective value must not materialize blank
                    // default storage or wear flash. No setter was sent.
                    let mut unchanged = p;
                    unchanged[0] = 0;
                    return Ok(unchanged);
                }
                Some(candidate)
            } else {
                None
            };
            let reply = self.usb_send_traced(data)?;
            if !super::response_matches_command(data, &reply) {
                bail!("AK820 reply does not match the request");
            }
            if write {
                self.ak820_verify_write(&p, &reply, state.image.unwrap())?;
                if !unlock {
                    storage_ready(&self.usb_send_traced(&[0xFD, 1])?)?;
                }
                if let Some(expected) = expected_nvm {
                    let actual = self.ak820_read_rom_traced(0xC700, 0x2800)?;
                    ensure!(
                        actual == expected,
                        "AK820 full NVM readback differs after module setting; operation stopped"
                    );
                }
            }
            Ok(reply)
        })();
        if let Err(error) = &operation {
            state.failure = Some(format!("{error:#}"));
        }
        operation
    }

    /// Internal ROM reader used while the session lock is held. It cannot
    /// recurse through usb_send or perform a mutation.
    fn ak820_read_rom_traced(&self, start: usize, length: usize) -> Result<Vec<u8>> {
        ensure!(
            start <= 65536 && length <= 65536 - start,
            "Invalid ROM range"
        );
        let mut data = Vec::with_capacity(length);
        while data.len() < length {
            let at = start + data.len();
            let count = (length - data.len()).min(27);
            let request = [0xFD, 0, (at >> 8) as u8, at as u8, count as u8];
            let reply = self.usb_send_traced(&request)?;
            ensure!(reply[..5] == request, "AK820 ROM reply range differs");
            data.extend_from_slice(&reply[5..5 + count]);
        }
        Ok(data)
    }

    fn ak820_verify_write(
        &self,
        p: &[u8; MSG_LEN],
        reply: &[u8; MSG_LEN],
        image: ImageContract,
    ) -> Result<()> {
        match (p[0], p[1]) {
            (0x03, 2) => {
                ensure!(reply == p, "AK820 layout setter was rejected");
                let actual = self.usb_send_traced(&[2, 2])?;
                ensure!(
                    actual[..6] == [2, 2, p[2], p[3], p[4], p[5]],
                    "AK820 layout readback differs"
                );
            }
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
            (0xFE, 11) => {
                ensure!(reply[0] == 0, "AK820 QMK setting was rejected");
                let actual = self.usb_send_traced(&[0xFE, 10, p[2], p[3]])?;
                let width = if p[2..4] == [1, 0] { 1 } else { 2 };
                ensure!(
                    actual[0] == 0 && actual[1..1 + width] == p[4..4 + width],
                    "AK820 QMK setting readback differs"
                );
            }
            (0xFE, 13) => {
                ensure!(
                    reply[0] == 0 && reply[1..14] == p[1..14],
                    "AK820 dynamic entry setter was rejected"
                );
                let actual = self.usb_send_traced(&[0xFE, 13, p[2] - 1, p[3]])?;
                ensure!(
                    actual[0] == 0 && actual[1..11] == p[4..14],
                    "AK820 dynamic entry readback differs"
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
                let selected = image
                    .saved_rgb_mode(raw[0] >> 2)
                    .context("AK820 saved RGB mode is invalid")?;
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
            image: ready.then_some(ImageContract::Alpha3),
            loaded: ready,
            failure: None,
            rgb_owner: None,
        }));
        (hid, recorder)
    }

    #[cfg(test)]
    pub(crate) fn test_verified_ak820_device() -> (Self, super::TestHidRecorder) {
        let (hid, recorder) = Self::test_ak820_device(false);
        hid.ak820.as_ref().unwrap().lock().unwrap().image = Some(ImageContract::Alpha3);
        (hid, recorder)
    }

    #[cfg(test)]
    pub(crate) fn test_beta2_ak820_device(
        profile: crate::app::ak820_beta1_profiles::ProfileId,
        ready: bool,
    ) -> (Self, super::TestHidRecorder) {
        let (hid, recorder) = Self::test_ak820_device(ready);
        hid.ak820.as_ref().unwrap().lock().unwrap().image = Some(ImageContract::Beta2(profile));
        (hid, recorder)
    }

    #[cfg(test)]
    pub(crate) fn test_beta3_ak820_device(
        profile: crate::app::ak820_beta1_profiles::ProfileId,
        ready: bool,
    ) -> (Self, super::TestHidRecorder) {
        let (hid, recorder) = Self::test_ak820_device(ready);
        hid.ak820.as_ref().unwrap().lock().unwrap().image = Some(ImageContract::Beta3(profile));
        (hid, recorder)
    }
}

#[cfg(test)]
#[path = "hid_ak820_tests.rs"]
mod transaction_tests;

#[cfg(test)]
mod beta4_write_policy_tests {
    use super::*;

    #[test]
    fn beta4_release_metadata_never_authorizes_hid_writes() {
        let image = ImageContract::Beta4(crate::app::ak820_beta1_profiles::ProfileId::Main);
        for packet in [
            [0xFE, 11, 1, 0, 2, 0], // persisted setting write
            [0x05, 0, 0, 0, 0, 4],  // keymap write
            [0xFE, 6, 0, 0, 0, 0],  // unlock transition
        ] {
            let mut request = [0; MSG_LEN];
            request[..packet.len()].copy_from_slice(&packet);
            assert!(allowed_request(&request, Some(image)).is_err());
        }
        let mut read = [0; MSG_LEN];
        read[..4].copy_from_slice(&[0xFE, 10, 22, 0]);
        assert!(!allowed_request(&read, Some(image)).unwrap());
    }
}

#[cfg(test)]
mod os_indicator_tests {
    use super::*;
    use crate::ak820_os_indicator::Mode;
    use crate::app::ak820_beta1_profiles::ProfileId;

    #[test]
    fn fd02_has_one_exchange_and_never_touches_nvm() {
        let (hid, log) = HidDevice::test_beta2_ak820_device(ProfileId::Main, true);
        log.respond_by(|p| {
            let mut reply = [0; 32];
            reply[..10].copy_from_slice(&[0xfd, 2, 1, 0, p[4], 0xa8, 0xb8, 0x0b, 0, 0x10]);
            Some(Ok(reply))
        });
        for mode in [Mode::Off, Mode::On, Mode::Breathing, Mode::Normal] {
            hid.ak820_os_indicator_set(mode).unwrap();
        }
        let requests = log.requests();
        assert_eq!(requests.len(), 4);
        assert!(requests.iter().all(|p| p[..4] == [0xfd, 2, 1, 1]));
        assert!(hid.ak820_editing_ready());
    }

    #[test]
    fn fd02_bad_reply_latches_without_retry_or_next_refresh() {
        let (hid, log) = HidDevice::test_beta2_ak820_device(ProfileId::Main, true);
        log.respond_by(|_| Some(Ok([0; 32])));
        assert!(hid.ak820_os_indicator_set(Mode::On).is_err());
        assert!(hid.ak820_os_indicator_set(Mode::Normal).is_err());
        assert_eq!(log.requests().len(), 1);
        assert!(!hid.ak820_editing_ready());
    }

    #[test]
    fn fd02_requires_full_load_and_refuses_read_only_without_packets() {
        let (mut hid, log) = HidDevice::test_beta2_ak820_device(ProfileId::Main, false);
        assert!(hid.ak820_os_indicator_set(Mode::On).is_err());
        assert!(log.requests().is_empty());
        hid.ak820.as_ref().unwrap().lock().unwrap().loaded = true;
        hid.read_only = Some(super::super::ReadOnlyHidSession::default());
        assert!(hid.ak820_os_indicator_set(Mode::On).is_err());
        assert!(log.requests().is_empty());
    }
}

#[cfg(test)]
#[path = "hid_ak820_beta2_tests.rs"]
mod beta2_transaction_tests;

#[cfg(test)]
mod tests {
    use super::*;
    fn ready() -> [u8; 32] {
        let mut r = [0; 32];
        r[..4].copy_from_slice(&[0xFD, 1, 1, 0xA8]);
        r
    }

    #[test]
    fn capture_decoder_uses_exact_release_and_rejects_untrusted_rom() {
        let rom = vec![0; 65536];
        let nvm = &rom[0xc700..0xef00];
        for row in crate::app::ak820_beta1_profiles::profiles() {
            for image in [
                ImageContract::Beta2(row.profile),
                ImageContract::Beta3(row.profile),
                ImageContract::Beta4(row.profile),
            ] {
                let error = capture_decode_rom(image, &rom, nvm).unwrap_err().to_string();
                assert!(error.contains("protected"), "{image:?}: {error}");
                assert!(capture_decode_rom(image, &rom[..100], nvm).is_err());
            }
        }
        assert!(capture_decode_rom(ImageContract::Alpha3, &rom, nvm).is_err());
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

#[cfg(test)]
#[path = "hid_ak820_backup_tests.rs"]
mod backup_tests;

#[path = "ak820_restore_executor.rs"]
mod restore_executor;

#[path = "hid_ak820_install.rs"]
mod install;
pub(crate) use install::{InstallUsb, WireCommand, usb_port};
