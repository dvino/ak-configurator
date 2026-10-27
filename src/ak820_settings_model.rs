// SPDX-License-Identifier: GPL-2.0-or-later
//! Offline backup settings derivation, ported from firmware bef20f89's
//! settings_backup.py and pack_ak820.py. No HID, OEM bytes or write operations.
//! NVM-only parsing proves storage structure, not a running firmware identity.
use crate::ak820_qs_versioned_offline::{ImageVersion, RecordVersion, SettingsRecord};
use crate::ak820_settings_backup::{
    BackupResult, ImageIdentity, macro_logical_length, rgb_wire, rgb_wire_for_profile, sha256,
};
use crate::ak820_typed_storage::{Extensions, NonQsExtensions, SCHEMA};
use crate::app::ak820_beta1_profiles::{ProfileId, historical_settings_identity, matrix_profile};
use serde_json::{Map, Value, json};

const MASK: [u8; 16] = [
    0x3f, 0x2f, 0x3f, 0x1f, 0x1f, 0x3f, 0x1f, 0x1f, 0x1f, 0x3f, 0x3f, 0x3f, 0x07, 0x3f, 0x3c, 0x2e,
];
const LIMITATIONS: [&str; 6] = [
    "Direct HSV palette: no getter; not exported or restored.",
    "Hidden current RGB mode while off and current RGB flags: no getter; only saved NVM values are preserved.",
    "Unused matrix slots and reserved NVM bytes: captured as evidence, not imported.",
    "Animation phase, reactive ages, held keys and macro execution position: not exported.",
    "No firmware freeze: repeated stable reads do not guarantee an atomic snapshot.",
    "Vial UID identifies a model, not a unique physical keyboard; connect only one supported device.",
];
// Public pack_ak820.PROTECTED_SHA, hashing loader + protected tail only.
// This is hardware-revision validation, never physical-unit ownership evidence.
const PROTECTED_SHA: &str = "9c4d481c14fe26b58f0f3eee81e84f45f118e06d755fd051da8d1cea5d78eb33";
fn require(ok: bool, msg: &str) -> BackupResult<()> {
    if ok { Ok(()) } else { Err(msg.into()) }
}
fn hex(raw: &[u8]) -> String {
    raw.iter().map(|b| format!("{b:02x}")).collect()
}
fn le(raw: &[u8]) -> u16 {
    u16::from_le_bytes([raw[0], raw[1]])
}
fn be(raw: &[u8]) -> u16 {
    u16::from_be_bytes([raw[0], raw[1]])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NvmMetadata {
    pub(crate) schema: u32,
    pub(crate) published_macro_length: usize,
    pub(crate) logical_macro_length: usize,
    pub(crate) macro_offset: usize,
}
/// Structural storage layout only; this does not admit an image or QS version.
#[derive(Debug, Clone, Copy)]
pub(crate) struct NvmLayout {
    schema: u32,
    macro_metadata: usize,
    macro_offset: usize,
    macro_capacity: usize,
}
/// Validate the complete NVM size, magic and known schema before slicing records.
pub(crate) fn validate_nvm_structure(nvm: &[u8], schema: u32) -> BackupResult<NvmLayout> {
    require(nvm.len() == 0x2800, "Incomplete NVM")?;
    require(
        nvm[..2] == [0xe6, 0xfe] && nvm[37..40] == schema.to_le_bytes()[..3],
        "NVM magic/schema differs; explicit migration required",
    )?;
    let (meta, start, capacity) = match schema {
        0x082001 => (0x2f0, 0x300, 9472),
        0x082002 | 0x082003 => (0x7f0, 0x800, 8192),
        _ => return Err("Unknown NVM schema".into()),
    };
    Ok(NvmLayout {
        schema,
        macro_metadata: meta,
        macro_offset: start,
        macro_capacity: capacity,
    })
}
impl NvmLayout {
    /// Recheck structure so a layout cannot validate bytes from a different schema.
    pub(crate) fn validate_macros(self, nvm: &[u8]) -> BackupResult<NvmMetadata> {
        validate_nvm_structure(nvm, self.schema)?;
        let meta = self.macro_metadata;
        require(nvm[meta] == 0, "Incomplete macro upload")?;
        let length = le(&nvm[meta + 1..meta + 3]) as usize;
        require(
            (16..=self.macro_capacity).contains(&length),
            "Invalid published macro length",
        )?;
        let logical = macro_logical_length(&nvm[self.macro_offset..self.macro_offset + length])?;
        Ok(NvmMetadata {
            schema: self.schema,
            published_macro_length: length,
            logical_macro_length: logical,
            macro_offset: self.macro_offset,
        })
    }
}
/// Validate exact schema, magic, typed records and published macro state.
/// Reserved bytes and inactive valid typed entries are retained, never cleared.
pub(crate) fn validate_nvm(nvm: &[u8], schema: u32) -> BackupResult<NvmMetadata> {
    let layout = validate_nvm_structure(nvm, schema)?;
    if schema == SCHEMA {
        Extensions::decode_nvm(nvm)?;
    }
    layout.validate_macros(nvm)
}
fn macro_commands(stream: &[u8]) -> BackupResult<Value> {
    let logical = macro_logical_length(stream)?;
    let mut result = Vec::new();
    let mut commands = Vec::new();
    let mut text = Vec::new();
    let mut at = 0;
    fn flush(text: &mut Vec<u8>, commands: &mut Vec<Value>) {
        if !text.is_empty() {
            commands.push(json!({"text_hex":hex(text)}));
            text.clear();
        }
    }
    while at < logical {
        let value = stream[at];
        at += 1;
        match value {
            0 => {
                flush(&mut text, &mut commands);
                result.push(Value::Array(std::mem::take(&mut commands)));
            }
            1 => {
                flush(&mut text, &mut commands);
                let opcode = stream[at];
                let width = if opcode <= 3 { 1 } else { 2 };
                let name =
                    ["", "tap", "down", "up", "delay", "tap16", "down16", "up16"][opcode as usize];
                commands
                    .push(json!({"command":name,"operands_hex":hex(&stream[at+1..at+1+width])}));
                at += 1 + width;
            }
            _ => text.push(value),
        }
    }
    Ok(Value::Array(result))
}
fn combo_settings(nvm: &[u8], combo_media: bool) -> BackupResult<Value> {
    let mut entries = Vec::new();
    for i in 0..8 {
        let at = 0x658 + i * 10;
        let raw = &nvm[at..at + 10];
        let mut data = [0u8; 10];
        if raw.iter().any(|&b| b != 0) {
            require(
                raw[4..8] == [0x20, 0x08, 1, 0],
                "Unknown Combo marker/version",
            )?;
            data[..4].copy_from_slice(&raw[..4]);
            data[8..].copy_from_slice(&raw[8..]);
        }
        let a = le(&data[..2]);
        let b = le(&data[2..4]);
        let out = le(&data[8..]);
        require(
            (a == 0 && b == 0 && out == 0)
                || ((4..=0xa4).contains(&a)
                    && (4..=0xa4).contains(&b)
                    && a != b
                    && ((combo_media && (0xa8..=0xc2).contains(&out))
                        || out == 0x7c73
                        || (out < 0x2000
                            && ((4..=0xa4).contains(&(out & 255))
                                || (0xe0..=0xe7).contains(&(out & 255)))))),
            "Unsupported Combo keys/output",
        )?;
        entries.push(hex(&data));
    }
    Ok(json!({"version":1,"entries_hex":entries}))
}
/// Complete firmware settings JSON. `combos` is exact image metadata, not a
/// user preference. This preserves raw typed entries even for inactive modules.
/// `rgb_mode` is the observed VialRGB getter; archive validation supplies the
/// effective saved mode, matching the firmware's stable-capture requirement.
pub(crate) fn settings_from_nvm(nvm: &[u8], rgb_mode: &[u8], combos: bool) -> BackupResult<Value> {
    require(nvm.len() == 0x2800, "Incomplete NVM")?;
    let schema = u32::from_le_bytes([nvm[37], nvm[38], nvm[39], 0]);
    require(
        matches!(schema, 0x082002 | 0x082003),
        "Schema01 requires explicit migration; six-layer derivation is unavailable",
    )?;
    let meta = validate_nvm(nvm, schema)?;
    derive_settings(nvm, rgb_mode, combos, false, meta, || {
        if schema != SCHEMA {
            return Ok(None);
        }
        let typed = Extensions::decode_nvm(nvm)?;
        let mut settings = Map::new();
        for id in [1, 2, 4, 7, 18, 19] {
            settings.insert(id.to_string(), json!(typed.settings.get(id)?));
        }
        extension_json(settings, &typed.tap_dance, &typed.key_overrides).map(Some)
    })
}

/// Complete offline schema03 settings derivation under explicit image format
/// assumptions. This admits no image identity or directed device transition.
/// Image V2 may retain persisted QS v1; extensions JSON format remains version 1.
/// Source/target provenance stays in the caller's explicit arguments, not JSON.
#[allow(dead_code)]
pub(crate) fn settings_from_nvm_for_image_offline(
    nvm: &[u8],
    rgb_mode: &[u8],
    combos: bool,
    source: ImageVersion,
    target: ImageVersion,
) -> BackupResult<Value> {
    settings_from_nvm_for_image_with_capabilities_offline(
        nvm,
        rgb_mode,
        combos,
        source,
        target,
        OfflineSettingsCapabilities::default(),
    )
}

/// Caller-supplied provisional format assumptions, never trusted image admission.
/// Legacy callers default to no media Combo support; image version alone grants none.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct OfflineSettingsCapabilities {
    pub(crate) combo_media: bool,
}

/// Decode with an explicit provisional source capability binding. Media Combo
/// requires a source V2 image contract, including when persisted QS remains v1.
/// Target normalization does not confer source capabilities. Disabled Combo
/// storage remains uninterpreted; this capability grants no transport authority.
#[allow(dead_code)]
pub(crate) fn settings_from_nvm_for_image_with_capabilities_offline(
    nvm: &[u8],
    rgb_mode: &[u8],
    combos: bool,
    source: ImageVersion,
    target: ImageVersion,
    capabilities: OfflineSettingsCapabilities,
) -> BackupResult<Value> {
    require(
        !capabilities.combo_media || source == ImageVersion::V2,
        "Media Combo capability requires a source V2 image contract",
    )?;
    let layout = validate_nvm_structure(nvm, SCHEMA)?;
    let record = SettingsRecord::decode_for_image(&nvm[0x6a8..0x6c0], source, target)?;
    let non_qs = NonQsExtensions::decode_nvm(nvm)?;
    let meta = layout.validate_macros(nvm)?;
    derive_settings(
        nvm,
        rgb_mode,
        combos,
        capabilities.combo_media,
        meta,
        || {
            let mut settings = Map::new();
            let ids: &[u16] = if record.version() == RecordVersion::V2 {
                &[1, 2, 4, 7, 18, 19, 3, 6, 22]
            } else {
                &[1, 2, 4, 7, 18, 19]
            };
            for &id in ids {
                settings.insert(id.to_string(), json!(record.get(id)?));
            }
            extension_json(settings, &non_qs.tap_dance, &non_qs.key_overrides).map(Some)
        },
    )
}

fn extension_json(
    settings: Map<String, Value>,
    tap_dance: &[crate::ak820_typed_storage::TapDance; 8],
    key_overrides: &[crate::ak820_typed_storage::KeyOverride; 8],
) -> BackupResult<Value> {
    let td: BackupResult<Vec<_>> = tap_dance
        .iter()
        .map(|entry| entry.encode().map(|raw| hex(&raw)))
        .collect();
    let ko: BackupResult<Vec<_>> = key_overrides
        .iter()
        .map(|entry| entry.encode().map(|raw| hex(&raw)))
        .collect();
    Ok(json!({"version":1,"qmk_settings":settings,"tap_dance":td?,"key_overrides":ko?}))
}

fn derive_settings(
    nvm: &[u8],
    rgb_mode: &[u8],
    combos: bool,
    combo_media: bool,
    meta: NvmMetadata,
    extensions: impl FnOnce() -> BackupResult<Option<Value>>,
) -> BackupResult<Value> {
    let stream = &nvm[meta.macro_offset..meta.macro_offset + meta.published_macro_length];
    let mut keys = Vec::with_capacity(498);
    let mut encoders = Vec::with_capacity(12);
    for layer in 0..6 {
        for (row, &mask) in MASK.iter().enumerate() {
            for col in 0..8 {
                if mask & (1 << col) != 0 {
                    let at = 0x40 + layer * 256 + row * 16 + col * 2;
                    keys.push(
                        json!({"layer":layer,"row":row,"column":col,"keycode":be(&nvm[at..at+2])}),
                    );
                }
            }
        }
        for direction in 0..2 {
            let at = 0x640 + layer * 4 + (1 - direction) * 2;
            encoders
                .push(json!({"layer":layer,"direction":direction,"keycode":be(&nvm[at..at+2])}));
        }
    }
    let mut result = json!({"keys":keys,"encoders":encoders,"layout_options":nvm[40],"macros":{"count":16,"capacity":8192,"published_length":meta.published_macro_length,"logical_length":meta.logical_macro_length,"data_hex":hex(stream),"commands":macro_commands(stream)?},"rgb":{"saved_config_hex":hex(&nvm[24..32]),"current_mode_hex":hex(rgb_mode),"saved_source":"NVM[24:32]","current_source":"VialRGB 0x08/0x41"},"limitations":LIMITATIONS});
    if let Some(extensions) = extensions()? {
        result["extensions"] = extensions;
    }
    if combos {
        result["combos"] = combo_settings(nvm, combo_media)?;
    }
    Ok(result)
}

/// Production archive callback for a catalog-qualified beta image. All checks
/// precede derivation. Protected-span hash pins a hardware revision; unit-level
/// ownership/recovery binding and live stable reads remain separate gates.
pub(crate) fn decode_trusted_rom(
    rom: &[u8],
    nvm: &[u8],
    identity: &ImageIdentity,
    profile: ProfileId,
) -> BackupResult<Value> {
    decode_catalog_rom(rom, nvm, identity, Some(profile))
}
fn decode_catalog_rom(
    rom: &[u8],
    nvm: &[u8],
    identity: &ImageIdentity,
    profile: Option<ProfileId>,
) -> BackupResult<Value> {
    if let Some(profile) = profile {
        let contract = crate::app::ak820_beta1_profiles::previous_firmware_matrix()
            .profiles
            .iter()
            .find(|r| {
                r.settings_identity
                    .as_ref()
                    .is_some_and(|i| i.ihx_sha256 == identity.ihx_sha256)
            })
            .unwrap_or_else(|| matrix_profile(profile));
        let exact = contract
            .settings_identity
            .as_ref()
            .ok_or("Image lacks qualified packed CODE identity")?;
        require(
            identity.ihx_sha256 == exact.ihx_sha256
                && identity.code_sha256 == exact.code_sha256
                && identity.definition_sha256 == exact.definition_sha256
                && identity.schema == exact.nvm_schema,
            "Archive image differs from exact catalog settings contract",
        )?;
        require(
            exact.combos_version == 1
                && exact.extensions_version
                    == if identity.schema == SCHEMA {
                        Some(1)
                    } else {
                        None
                    },
            "Unsupported typed contract version",
        )?;
    } else {
        require(
            identity.definition_sha256
                == "72b2261e1f7da5ec8ba37b8276bab12f12642e3d6514ec76ca1e4cd9b499fa6e"
                && historical_settings_identity(
                    &identity.ihx_sha256,
                    &identity.code_sha256,
                    identity.schema,
                )
                .is_some(),
            "Unknown exact historical settings contract",
        )?;
    }
    require(
        rom.len() == 65536 && nvm.len() == 0x2800,
        "Incomplete ROM/NVM",
    )?;
    let mut protected = Vec::with_capacity(0x2800);
    protected.extend_from_slice(&rom[..0x27c0]);
    protected.extend_from_slice(&rom[0xefc0..]);
    require(
        sha256(&protected) == PROTECTED_SHA,
        "OEM protected loader/tail differs from pinned hardware revision",
    )?;
    require(
        rom[0x27c0..0x27c4] == [0xaa, 0x55, 0x33, 0xcc],
        "Permanent boot marker missing",
    )?;
    require(
        sha256(&rom[0x2800..0xc700]) == identity.code_sha256,
        "Packed CODE differs from exact image",
    )?;
    let manifest = &rom[0xef80..0xefc0];
    require(
        &manifest[..12] == b"AKV1PKG\0\x01\0\0\0"
            && manifest[12..16] == identity.schema.to_le_bytes(),
        "Application manifest/build identity differs",
    )?;
    require(
        hex(&manifest[16..48]) == identity.code_sha256,
        "Application manifest CODE digest differs",
    )?;
    // Last 16 bytes describe installation-time NVM, not the current settings.
    require(
        rom[0xef40..0xef80].iter().all(|&b| b == 0),
        "Journal header is not empty",
    )?;
    require(&rom[0xc700..0xef00] == nvm, "Saved NVM differs from ROM")?;
    validate_nvm(nvm, identity.schema)?;
    let historical_profile =
        historical_settings_identity(&identity.ihx_sha256, &identity.code_sha256, identity.schema)
            .and_then(|h| h.profile);
    if historical_settings_identity(&identity.ihx_sha256, &identity.code_sha256, identity.schema)
        .is_some()
    {
        for record in nvm[0x658..0x6a8].chunks_exact(10) {
            require(
                le(&record[8..]) != 0x7c73,
                "Historical image does not support Combo Caps Word action",
            )?;
        }
    }
    let rgb = match profile.or(historical_profile) {
        Some(p) => rgb_wire_for_profile(&nvm[24..32], false, p)?,
        None => rgb_wire(&nvm[24..32], false)?,
    };
    let combos = match profile {
        Some(p) => matrix_profile(p).capabilities.dynamic_entries.combos > 0,
        None => {
            historical_settings_identity(
                &identity.ihx_sha256,
                &identity.code_sha256,
                identity.schema,
            )
            .ok_or("Unknown historical Combo contract")?
            .combos
        }
    };
    let mut settings = settings_from_nvm(nvm, &rgb, combos)?;
    apply_image_limitations(&mut settings, identity);
    Ok(settings)
}
/// RAM readback support does not make the durable backup a palette archive.
pub(crate) fn apply_image_limitations(settings: &mut Value, identity: &ImageIdentity) {
    let direct = crate::app::ak820_beta1_profiles::firmware_matrix()
        .profiles
        .iter()
        .any(|r| {
            r.settings_identity.as_ref().is_some_and(|i| {
                i.ihx_sha256 == identity.ihx_sha256
                    && i.code_sha256 == identity.code_sha256
                    && i.direct_protocol == Some(1)
            })
        });
    if direct {
        settings["limitations"][0] = json!(
            "Direct RAM palette is separate and is not included in this durable-settings backup."
        );
    }
}

/// Catalog-bound archive entry point. This validates local evidence; it does
/// not authorize a device restore or establish physical-unit ownership.
pub(crate) fn validate_profile_archive(
    manifest: &[u8],
    files: &std::collections::BTreeMap<String, Vec<u8>>,
    profile: ProfileId,
    expected_manifest: Option<&str>,
) -> BackupResult<crate::ak820_settings_backup::ValidatedArchive> {
    let value: Value = serde_json::from_slice(manifest).map_err(|_| "Invalid archive manifest")?;
    let row = crate::app::ak820_beta1_profiles::previous_firmware_matrix()
        .profiles
        .iter()
        .find(|r| {
            r.profile == profile
                && value["source_ihx_sha256"].as_str() == Some(r.identity.ihx_sha256.as_str())
        })
        .unwrap_or_else(|| matrix_profile(profile));
    let exact = row
        .settings_identity
        .as_ref()
        .ok_or("Image lacks qualified packed CODE identity")?;
    let identity = ImageIdentity {
        ihx_sha256: exact.ihx_sha256.clone(),
        code_sha256: exact.code_sha256.clone(),
        definition_sha256: exact.definition_sha256.clone(),
        schema: exact.nvm_schema,
    };
    crate::ak820_settings_backup::validate_archive(
        manifest,
        files,
        &[identity],
        expected_manifest,
        |rom, nvm, image| decode_trusted_rom(rom, nvm, image, profile),
    )
}

/// Source-only historical backup admission, with the same full ROM checks as
/// current images. No historical profile alias or target is introduced.
pub(crate) fn validate_historical_archive(
    manifest: &[u8],
    files: &std::collections::BTreeMap<String, Vec<u8>>,
    identity: &ImageIdentity,
    expected_manifest: Option<&str>,
) -> BackupResult<crate::ak820_settings_backup::ValidatedArchive> {
    require(
        identity.definition_sha256
            == "72b2261e1f7da5ec8ba37b8276bab12f12642e3d6514ec76ca1e4cd9b499fa6e"
            && historical_settings_identity(
                &identity.ihx_sha256,
                &identity.code_sha256,
                identity.schema,
            )
            .is_some(),
        "Unknown exact historical settings contract",
    )?;
    crate::ak820_settings_backup::validate_archive(
        manifest,
        files,
        &[identity.clone()],
        expected_manifest,
        |rom, nvm, image| decode_catalog_rom(rom, nvm, image, None),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ak820_settings_backup::canonical;
    // Public, synthetic bytes only. This constructs NVM, never an accepted ROM.
    // Golden SHA256 and byte counts were generated by the pinned firmware's
    // settings_backup.settings_from_nvm + canonical on 2026-10-25 using the
    // same inputs below. All seven macro opcodes and inactive typed records
    // are covered. Python is not required to build or run these tests.
    fn modeled_nvm(schema: u32) -> Vec<u8> {
        let mut n = vec![0; 0x2800];
        n[..2].copy_from_slice(&[0xe6, 0xfe]);
        n[37..40].copy_from_slice(&schema.to_le_bytes()[..3]);
        n[24..32].copy_from_slice(&[9, 20, 30, 160, 4, 0, 0, 0]);
        n[40] = 3;
        for layer in 0..6 {
            for (row, &mask) in MASK.iter().enumerate() {
                for col in 0..8 {
                    if mask & (1 << col) != 0 {
                        let at = 0x40 + layer * 256 + row * 16 + col * 2;
                        let key = (4 + (layer + row + col) % 100) as u16;
                        n[at..at + 2].copy_from_slice(&key.to_be_bytes());
                    }
                }
            }
            for direction in 0..2 {
                let at = 0x640 + layer * 4 + (1 - direction) * 2;
                n[at..at + 2].copy_from_slice(&(0xa8 + direction as u16).to_be_bytes());
            }
        }
        let mut stream = b"Hello".to_vec();
        for op in 1..=7 {
            stream.extend([1, op, 2]);
            if op > 3 {
                stream.push(3);
            }
        }
        stream.extend([0; 19]);
        let (start, meta) = if schema == 0x082001 {
            (0x300, 0x2f0)
        } else {
            (0x800, 0x7f0)
        };
        n[meta] = 0;
        n[meta + 1..meta + 3].copy_from_slice(&(stream.len() as u16).to_le_bytes());
        n[start..start + stream.len()].copy_from_slice(&stream);
        if schema == SCHEMA {
            n[0x6c0..0x6ca].copy_from_slice(&[4, 0, 5, 0, 6, 0, 7, 0, 200, 0]);
            n[0x740..0x74a].copy_from_slice(&[4, 0, 5, 0, 3, 0, 1, 0, 0, 1]);
        }
        n
    }
    fn offline(
        nvm: &[u8],
        source: ImageVersion,
        target: ImageVersion,
        combos: bool,
    ) -> BackupResult<Value> {
        settings_from_nvm_for_image_offline(nvm, &[0; 6], combos, source, target)
    }
    #[test]
    fn provisional_media_combo_requires_explicit_source_capability() {
        let capable = OfflineSettingsCapabilities { combo_media: true };
        for output in [0xa7u16, 0xa8, 0xc2, 0xc3, 0x01a8, 0x01c2, 0x1fa8, 0x20a8] {
            // A V2 image may still contain a v1 QS record; this must not grant
            // capability by itself or prevent an explicit source binding.
            let mut nvm = modeled_nvm(SCHEMA);
            nvm[0x658..0x662].copy_from_slice(&[4, 0, 5, 0, 0x20, 8, 1, 0, 0, 0]);
            nvm[0x660..0x662].copy_from_slice(&output.to_le_bytes());
            let original = nvm.clone();
            assert!(settings_from_nvm(&nvm, &[0; 6], true).is_err());
            assert!(offline(&nvm, ImageVersion::V2, ImageVersion::V2, true).is_err());
            let decoded = settings_from_nvm_for_image_with_capabilities_offline(
                &nvm,
                &[0; 6],
                true,
                ImageVersion::V2,
                ImageVersion::V2,
                capable,
            );
            assert_eq!(
                decoded.is_ok(),
                (0xa8..=0xc2).contains(&output),
                "output {output:x}"
            );
            if let Ok(value) = decoded {
                assert_eq!(
                    value["combos"]["entries_hex"][0],
                    hex(&[4, 0, 5, 0, 0, 0, 0, 0, output as u8, 0])
                );
            }
            assert!(
                settings_from_nvm_for_image_with_capabilities_offline(
                    &nvm,
                    &[0; 6],
                    true,
                    ImageVersion::V1,
                    ImageVersion::V2,
                    capable,
                )
                .is_err()
            );
            let disabled = settings_from_nvm_for_image_with_capabilities_offline(
                &nvm,
                &[0; 6],
                false,
                ImageVersion::V2,
                ImageVersion::V2,
                capable,
            )
            .unwrap();
            assert!(disabled.get("combos").is_none());
            assert_eq!(nvm, original);
        }
    }
    #[test]
    fn provisional_media_combo_preserves_trigger_marker_and_disabled_storage_rules() {
        let capable = OfflineSettingsCapabilities { combo_media: true };
        for (a, b) in [
            (0u16, 5u16),
            (3, 5),
            (4, 4),
            (4, 0xa5),
            (0xa8, 5),
            (0x0104, 5),
        ] {
            let mut nvm = modeled_nvm(SCHEMA);
            nvm[0x658..0x662].copy_from_slice(&[0, 0, 0, 0, 0x20, 8, 1, 0, 0xa8, 0]);
            nvm[0x658..0x65a].copy_from_slice(&a.to_le_bytes());
            nvm[0x65a..0x65c].copy_from_slice(&b.to_le_bytes());
            assert!(
                settings_from_nvm_for_image_with_capabilities_offline(
                    &nvm,
                    &[0; 6],
                    true,
                    ImageVersion::V2,
                    ImageVersion::V2,
                    capable,
                )
                .is_err()
            );
            assert!(
                settings_from_nvm_for_image_with_capabilities_offline(
                    &nvm,
                    &[0; 6],
                    false,
                    ImageVersion::V2,
                    ImageVersion::V2,
                    capable,
                )
                .is_ok()
            );
        }
        let mut nvm = modeled_nvm(SCHEMA);
        nvm[0x658..0x662].fill(255);
        assert!(
            settings_from_nvm_for_image_with_capabilities_offline(
                &nvm,
                &[0; 6],
                true,
                ImageVersion::V2,
                ImageVersion::V2,
                capable,
            )
            .is_err()
        );
        assert!(
            settings_from_nvm_for_image_with_capabilities_offline(
                &nvm,
                &[0; 6],
                false,
                ImageVersion::V2,
                ImageVersion::V2,
                capable,
            )
            .is_ok()
        );
    }
    #[test]
    fn offline_v1_complete_json_matches_production_and_preserves_input() {
        let nvm = modeled_nvm(SCHEMA);
        let original = nvm.clone();
        for combos in [false, true] {
            let legacy = settings_from_nvm(&nvm, &[0; 6], combos).unwrap();
            let versioned = offline(&nvm, ImageVersion::V1, ImageVersion::V1, combos).unwrap();
            assert_eq!(canonical(&versioned).unwrap(), canonical(&legacy).unwrap());
        }
        assert_eq!(nvm, original);
    }
    #[test]
    fn offline_image_v2_accepts_v1_v2_and_erased_qs_without_writing() {
        for persisted in [None, Some(RecordVersion::V1), Some(RecordVersion::V2)] {
            for blank in [0, 255] {
                let mut nvm = modeled_nvm(SCHEMA);
                nvm[0x6a8..0x6c0].fill(blank);
                if let Some(version) = persisted {
                    let mut record = SettingsRecord::defaults(version);
                    record.set(4, 321).unwrap();
                    if version == RecordVersion::V2 {
                        record.set(3, 29).unwrap();
                        record.set(6, 30000).unwrap();
                        record.set(22, 1).unwrap();
                    }
                    nvm[0x6a8..0x6c0].copy_from_slice(&record.encode());
                }
                let original = nvm.clone();
                let result = offline(&nvm, ImageVersion::V2, ImageVersion::V2, true).unwrap();
                assert_eq!(result["extensions"]["version"], 1);
                let qs = &result["extensions"]["qmk_settings"];
                assert_eq!(qs.as_object().unwrap().len(), 9);
                assert_eq!(qs["4"], if persisted.is_some() { 321 } else { 175 });
                assert_eq!(
                    qs["3"],
                    if persisted == Some(RecordVersion::V2) {
                        29
                    } else {
                        1
                    }
                );
                assert_eq!(
                    qs["6"],
                    if persisted == Some(RecordVersion::V2) {
                        30000
                    } else {
                        0
                    }
                );
                assert_eq!(
                    qs["22"],
                    if persisted == Some(RecordVersion::V2) {
                        1
                    } else {
                        0
                    }
                );
                assert_eq!(nvm, original);
                assert!(offline(&nvm, ImageVersion::V2, ImageVersion::V1, true).is_err());
                if persisted == Some(RecordVersion::V2) {
                    assert!(offline(&nvm, ImageVersion::V1, ImageVersion::V2, true).is_err());
                    assert!(settings_from_nvm(&nvm, &[0; 6], true).is_err());
                } else {
                    assert_eq!(
                        offline(&nvm, ImageVersion::V1, ImageVersion::V2, true).unwrap(),
                        result
                    );
                }
            }
        }
    }
    #[test]
    fn offline_versioned_full_decoder_rejects_corruption_and_respects_combo_gate() {
        for version in [ImageVersion::V1, ImageVersion::V2] {
            let mut baseline = modeled_nvm(SCHEMA);
            let record = SettingsRecord::defaults(if version == ImageVersion::V1 {
                RecordVersion::V1
            } else {
                RecordVersion::V2
            });
            baseline[0x6a8..0x6c0].copy_from_slice(&record.encode());
            for (at, value) in [
                (0, 0),
                (37, 9),
                (0x6a8, 0),
                (0x6ae, 0),
                (0x7f0, 1),
                (0x7f1, 0),
                (0x7f2, 255),
                (0x800, 128),
                (0x806, 8),
            ] {
                let mut bad = baseline.clone();
                bad[at] = value;
                assert!(
                    offline(&bad, version, version, true).is_err(),
                    "mutation at {at:x} for {version:?}"
                );
            }
            let mut bad_crc = baseline.clone();
            bad_crc[0x6bf] ^= 1;
            assert!(offline(&bad_crc, version, version, true).is_err());
            for (stream, error) in [
                (vec![0; 15], "Invalid published macro length"),
                (
                    [vec![1, 8], vec![0; 16]].concat(),
                    "Unsupported macro opcode",
                ),
                ([vec![1, 1, 0], vec![0; 16]].concat(), "Zero macro operand"),
                (
                    [vec![0; 16], vec![2]].concat(),
                    "Nonzero tail after macro terminators",
                ),
                (vec![2; 16], "Incomplete macro stream"),
            ] {
                let mut bad = baseline.clone();
                bad[0x7f1..0x7f3].copy_from_slice(&(stream.len() as u16).to_le_bytes());
                bad[0x800..0x800 + stream.len()].copy_from_slice(&stream);
                assert_eq!(offline(&bad, version, version, true).unwrap_err(), error);
            }
            for slot in 0..8 {
                for td in [true, false] {
                    let mut bad = baseline.clone();
                    let at = crate::ak820_typed_storage::slot_offset(td, slot).unwrap();
                    bad[at] = 1;
                    assert!(offline(&bad, version, version, true).is_err());
                }
            }
            assert!(offline(&baseline[..100], version, version, true).is_err());
            let mut oversized = baseline.clone();
            oversized.push(0);
            assert!(offline(&oversized, version, version, true).is_err());
            // A media Combo output remains outside the existing accepted domain.
            baseline[0x658..0x662].copy_from_slice(&[4, 0, 5, 0, 0x20, 8, 1, 0, 0xa8, 0]);
            assert!(offline(&baseline, version, version, true).is_err());
            let result = offline(&baseline, version, version, false).unwrap();
            assert!(result.get("combos").is_none());
        }
    }
    #[test]
    fn shared_structure_and_macro_validation_preserve_legacy_rejection() {
        for schema in [0x082001, 0x082002, SCHEMA] {
            let baseline = modeled_nvm(schema);
            let layout = validate_nvm_structure(&baseline, schema).unwrap();
            assert_eq!(
                layout.validate_macros(&baseline).unwrap(),
                validate_nvm(&baseline, schema).unwrap()
            );
            let meta = if schema == 0x082001 { 0x2f0 } else { 0x7f0 };
            let start = if schema == 0x082001 { 0x300 } else { 0x800 };
            for (stream, error) in [
                (vec![0; 15], "Invalid published macro length"),
                (vec![128; 16], "Macro text must be ASCII"),
                (
                    [vec![1, 8], vec![0; 16]].concat(),
                    "Unsupported macro opcode",
                ),
                ([vec![1, 1, 0], vec![0; 16]].concat(), "Zero macro operand"),
                (
                    [vec![0; 16], vec![2]].concat(),
                    "Nonzero tail after macro terminators",
                ),
                (vec![2; 16], "Incomplete macro stream"),
            ] {
                let mut bad = baseline.clone();
                bad[meta + 1..meta + 3].copy_from_slice(&(stream.len() as u16).to_le_bytes());
                bad[start..start + stream.len()].copy_from_slice(&stream);
                assert_eq!(layout.validate_macros(&bad).unwrap_err(), error);
                assert_eq!(validate_nvm(&bad, schema).unwrap_err(), error);
            }
            let mut busy = baseline.clone();
            busy[meta] = 1;
            assert_eq!(
                layout.validate_macros(&busy).unwrap_err(),
                "Incomplete macro upload"
            );
            assert!(layout.validate_macros(&baseline[..100]).is_err());
            let mut wrong_schema = baseline.clone();
            wrong_schema[37] ^= 1;
            assert!(layout.validate_macros(&wrong_schema).is_err());
        }
    }
    #[test]
    fn production_still_rejects_persisted_qs_v2_before_macro_errors() {
        let mut nvm = modeled_nvm(SCHEMA);
        nvm[0x6a8..0x6c0]
            .copy_from_slice(&crate::ak820_typed_storage::QmkSettings::default().encode());
        nvm[0x6aa] = 2;
        nvm[0x7f0] = 1;
        assert_eq!(
            validate_nvm(&nvm, SCHEMA).unwrap_err(),
            "Invalid settings header, reserved bytes or CRC"
        );
        assert!(settings_from_nvm(&nvm, &[0; 6], true).is_err());
    }
    #[test]
    fn firmware_python_canonical_golden_supported_schemas() {
        for (schema, len, digest) in [
            (
                0x082002,
                45995,
                "4f8725ae93479c0d5c6ef7e3dc5b35023429f28b735ad7ac1653518d66872c79",
            ),
            (
                0x082003,
                46689,
                "cb990ecfa63492c198bedc4e8dbb8f6bd99013cf9a4d1acf1ee3ce795f448bcd",
            ),
        ] {
            let n = modeled_nvm(schema);
            let rgb = rgb_wire(&n[24..32], false).unwrap();
            let settings = settings_from_nvm(&n, &rgb, true).unwrap();
            let encoded = canonical(&settings).unwrap();
            assert_eq!(encoded.len(), len);
            assert_eq!(sha256(&encoded), digest);
            assert_eq!(settings["keys"].as_array().unwrap().len(), 498);
            assert_eq!(settings["encoders"][0]["keycode"], 0xa8);
            assert_eq!(settings["encoders"][1]["keycode"], 0xa9);
        }
    }
    #[test]
    fn legacy_nvm_needs_explicit_migration_before_derivation() {
        let nvm = modeled_nvm(0x082001);
        assert!(validate_nvm(&nvm, 0x082001).is_ok());
        assert!(
            settings_from_nvm(&nvm, &[], false)
                .unwrap_err()
                .contains("explicit migration")
        );
    }
    #[test]
    fn malformed_nvm_fails_before_derivation() {
        assert!(settings_from_nvm(&[], &[], true).is_err());
        let baseline = modeled_nvm(SCHEMA);
        for (at, value) in [
            (0, 0),
            (37, 9),
            (0x7f0, 1),
            (0x7f1, 0),
            (0x6a8, 1),
            (0x6c9, 255),
            (0x744, 0),
            (0x749, 0x40),
        ] {
            let mut n = baseline.clone();
            n[at] = value;
            // Length byte 0x7f1=0 leaves length zero; otherwise validation
            // failures are magic/schema, busy, malformed typed record/ranges.
            assert!(validate_nvm(&n, SCHEMA).is_err(), "mutation at {at:x}");
        }
        let mut n = baseline.clone();
        n[0x658..0x662].copy_from_slice(&[4, 0, 5, 0, 0, 0, 0, 0, 6, 0]);
        assert!(settings_from_nvm(&n, &[0; 6], true).is_err());
        // Inaccessible Combo storage is not interpreted for a legacy image
        // without Combo support, matching the firmware reference.
        assert!(settings_from_nvm(&n, &[0; 6], false).is_ok());
    }
    #[test]
    fn schema03_preserves_inactive_records_and_blank_settings() {
        let n = modeled_nvm(SCHEMA);
        let original = n.clone();
        let settings = settings_from_nvm(&n, &[0; 6], true).unwrap();
        assert_eq!(settings["extensions"]["qmk_settings"]["4"], 175);
        assert_eq!(
            settings["extensions"]["tap_dance"][0],
            "0400050006000700c800"
        );
        assert_eq!(
            settings["extensions"]["key_overrides"][0],
            "04000500030001000001"
        );
        assert_eq!(n, original);
        let mut erased = n.clone();
        erased[0x6a8..0x6c0].fill(255);
        assert_eq!(settings_from_nvm(&erased, &[0; 6], true).unwrap(), settings);
        assert!(erased[0x6a8..0x6c0].iter().all(|&b| b == 255));
    }
    #[test]
    fn synthesized_rom_never_qualifies() {
        let id = ProfileId::Main;
        let exact = matrix_profile(id).settings_identity.as_ref().unwrap();
        let identity = ImageIdentity {
            ihx_sha256: exact.ihx_sha256.clone(),
            code_sha256: exact.code_sha256.clone(),
            definition_sha256: exact.definition_sha256.clone(),
            schema: exact.nvm_schema,
        };
        let n = modeled_nvm(identity.schema);
        let mut rom = vec![0; 65536];
        rom[0xc700..0xef00].copy_from_slice(&n);
        assert!(
            decode_trusted_rom(&rom, &n, &identity, id)
                .unwrap_err()
                .contains("protected")
        );
        assert!(decode_trusted_rom(&rom[..100], &n, &identity, id).is_err());
        let mut wrong = identity.clone();
        wrong.code_sha256 = "0".repeat(64);
        assert!(
            decode_trusted_rom(&rom, &n, &wrong, id)
                .unwrap_err()
                .contains("catalog")
        );
    }
}
