// SPDX-License-Identifier: GPL-2.0-or-later
//! Offline backup settings derivation, ported from firmware bef20f89's
//! settings_backup.py and pack_ak820.py. No HID, OEM bytes or write operations.
//! NVM-only parsing proves storage structure, not a running firmware identity.
use crate::ak820_settings_backup::{
    macro_logical_length, rgb_wire, sha256, BackupResult, ImageIdentity,
};
use crate::ak820_typed_storage::{Extensions, SCHEMA};
use crate::app::ak820_beta1_profiles::{matrix_profile, ProfileId};
use serde_json::{json, Map, Value};

const MASK: [u8; 16] = [
    0x3f, 0x2f, 0x3f, 0x1f, 0x1f, 0x3f, 0x1f, 0x1f, 0x1f, 0x3f, 0x3f, 0x3f, 0x07, 0x3f, 0x3c, 0x2e,
];
const LIMITATIONS: [&str;6] = [
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
    if ok {
        Ok(())
    } else {
        Err(msg.into())
    }
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
/// Validate exact schema, magic, typed records and published macro state.
/// Reserved bytes and inactive valid typed entries are retained, never cleared.
pub(crate) fn validate_nvm(nvm: &[u8], schema: u32) -> BackupResult<NvmMetadata> {
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
    if schema == SCHEMA {
        Extensions::decode_nvm(nvm)?;
    }
    require(nvm[meta] == 0, "Incomplete macro upload")?;
    let length = le(&nvm[meta + 1..meta + 3]) as usize;
    require(
        (16..=capacity).contains(&length),
        "Invalid published macro length",
    )?;
    let logical = macro_logical_length(&nvm[start..start + length])?;
    Ok(NvmMetadata {
        schema,
        published_macro_length: length,
        logical_macro_length: logical,
        macro_offset: start,
    })
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
fn combo_settings(nvm: &[u8]) -> BackupResult<Value> {
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
                    && out < 0x2000
                    && ((4..=0xa4).contains(&(out & 255)) || (0xe0..=0xe7).contains(&(out & 255)))),
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
    if schema == SCHEMA {
        let typed = Extensions::decode_nvm(nvm)?;
        let mut settings = Map::new();
        for id in [1, 2, 4, 7, 18, 19] {
            settings.insert(id.to_string(), json!(typed.settings.get(id)?));
        }
        let td: BackupResult<Vec<_>> = typed
            .tap_dance
            .iter()
            .map(|entry| entry.encode().map(|raw| hex(&raw)))
            .collect();
        let ko: BackupResult<Vec<_>> = typed
            .key_overrides
            .iter()
            .map(|entry| entry.encode().map(|raw| hex(&raw)))
            .collect();
        result["extensions"] =
            json!({"version":1,"qmk_settings":settings,"tap_dance":td?,"key_overrides":ko?});
    }
    if combos {
        result["combos"] = combo_settings(nvm)?;
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
    let contract = matrix_profile(profile);
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
    let rgb = rgb_wire(&nvm[24..32], false)?;
    settings_from_nvm(nvm, &rgb, true)
}

/// Catalog-bound archive entry point. This validates local evidence; it does
/// not authorize a device restore or establish physical-unit ownership.
pub(crate) fn validate_profile_archive(
    manifest: &[u8],
    files: &std::collections::BTreeMap<String, Vec<u8>>,
    profile: ProfileId,
    expected_manifest: Option<&str>,
) -> BackupResult<crate::ak820_settings_backup::ValidatedArchive> {
    let exact = matrix_profile(profile)
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
        assert!(settings_from_nvm(&nvm, &[], false)
            .unwrap_err()
            .contains("explicit migration"));
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
        let id = ProfileId::Work;
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
        assert!(decode_trusted_rom(&rom, &n, &identity, id)
            .unwrap_err()
            .contains("protected"));
        assert!(decode_trusted_rom(&rom[..100], &n, &identity, id).is_err());
        let mut wrong = identity.clone();
        wrong.code_sha256 = "0".repeat(64);
        assert!(decode_trusted_rom(&rom, &n, &wrong, id)
            .unwrap_err()
            .contains("catalog"));
    }
}
