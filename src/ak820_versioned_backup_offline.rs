// SPDX-License-Identifier: GPL-2.0-or-later
//! Five-file envelope validation under caller-supplied offline assumptions.
//! No catalog admission, restore plan, transport or executable backup conversion.
//! A synthetic validator in tests proves ordering only, never ROM trust.
#![allow(dead_code)]

use crate::ak820_qs_versioned_offline::ImageVersion;
use crate::ak820_settings_backup::{self, BackupResult, ImageIdentity};
use crate::ak820_settings_model::{
    OfflineSettingsCapabilities, settings_from_nvm_for_image_with_capabilities_offline,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// Explicit format assumptions for one exact identity, not trusted admission.
/// RGB enum/wire pairs, brightness and the complete limitations text must come
/// from the caller's inspected image contract. No profile or version defaults.
#[derive(Clone, Debug)]
pub(crate) struct OfflineImageContract {
    pub(crate) identity: ImageIdentity,
    pub(crate) version: ImageVersion,
    pub(crate) combos: bool,
    pub(crate) combo_media: bool,
    pub(crate) rgb_modes: Vec<(u8, u16)>,
    pub(crate) rgb_max_brightness: u8,
    pub(crate) limitations: Vec<String>,
}

/// Local evidence only. Deliberately distinct from ValidatedArchive/LoadedBackup.
/// This type grants no image admission, transition, or device authority.
#[derive(Debug)]
pub(crate) struct OfflineValidatedEnvelope {
    pub(crate) manifest_sha256: String,
    pub(crate) manifest: Value,
    pub(crate) settings: Value,
    pub(crate) contract: OfflineImageContract,
}

fn check_contract(contract: &OfflineImageContract) -> BackupResult<()> {
    if contract.identity.schema != 0x082003 {
        return Err("Offline versioned envelope requires schema03".into());
    }
    if contract.combo_media && (!contract.combos || contract.version != ImageVersion::V2) {
        return Err("Media Combo requires an enabled source V2 Combo contract".into());
    }
    let mut saved = BTreeSet::new();
    let mut wire = BTreeSet::new();
    if contract.rgb_modes.is_empty()
        || contract
            .rgb_modes
            .iter()
            .any(|&(s, w)| s == 0 || s > 63 || w == 0 || !saved.insert(s) || !wire.insert(w))
        || contract.rgb_max_brightness > 160
        || contract.limitations.is_empty()
        || contract.limitations.iter().any(|s| s.trim().is_empty())
    {
        return Err("Incomplete or ambiguous offline RGB/limitations contract".into());
    }
    Ok(())
}

// Mirrors settings_backup.py rgb_wire(selected=False) with an explicit enum.
fn saved_rgb(nvm: &[u8], contract: &OfflineImageContract) -> BackupResult<[u8; 6]> {
    let raw = &nvm[24..32]; // validate_archive checks full NVM length first.
    let mode = contract
        .rgb_modes
        .iter()
        .find(|&&(s, _)| s == raw[0] >> 2)
        .map(|&(_, w)| w)
        .ok_or("Unsupported saved RGB mode")?;
    if raw[0] & 3 > 1 || raw[3] > contract.rgb_max_brightness {
        return Err("Invalid saved RGB enable/brightness".into());
    }
    let effect = if raw[0] & 3 == 1 { mode } else { 0 };
    Ok([
        effect as u8,
        (effect >> 8) as u8,
        raw[4],
        raw[1],
        raw[2],
        raw[3],
    ])
}

/// Reuse the real envelope/hash parser, resolve one complete exact identity,
/// then validate the ROM before deriving any settings. The mandatory callback
/// MUST validate exact packed CODE, manifest/build, protected OEM regions,
/// journal and complete NVM under this exact contract. Returning Ok is the
/// caller's assertion of those checks; this bridge supplies no trust validator.
/// Hashes and injected fixture callbacks alone never establish ROM trust.
/// Source and target versions are equal: this is validation, not migration.
pub(crate) fn validate_offline_envelope<F>(
    manifest: &[u8],
    files: &BTreeMap<String, Vec<u8>>,
    registry: &[OfflineImageContract],
    expected_manifest: Option<&str>,
    validate_complete_exact_rom: F,
) -> BackupResult<OfflineValidatedEnvelope>
where
    F: FnOnce(&[u8], &[u8], &OfflineImageContract) -> BackupResult<()>,
{
    // Reject duplicate/conflicting identity keys even if their schemas differ.
    // validate_archive's lookup binds all three hashes before checking schema.
    let mut keys = BTreeSet::new();
    let mut ihx_keys = BTreeSet::new();
    for contract in registry {
        check_contract(contract)?;
        let i = &contract.identity;
        if !keys.insert((&i.ihx_sha256, &i.code_sha256, &i.definition_sha256))
            || !ihx_keys.insert(&i.ihx_sha256)
        {
            return Err("Duplicate or conflicting offline image contracts".into());
        }
    }
    let identities: Vec<_> = registry.iter().map(|c| c.identity.clone()).collect();
    let archive = ak820_settings_backup::validate_archive(
        manifest,
        files,
        &identities,
        expected_manifest,
        |rom, nvm, identity| {
            let matches: Vec<_> = registry
                .iter()
                .filter(|c| c.identity == *identity)
                .collect();
            if matches.len() != 1 {
                return Err("Missing or ambiguous complete offline image contract".into());
            }
            let contract = matches[0];
            validate_complete_exact_rom(rom, nvm, contract)?;
            let rgb = saved_rgb(nvm, contract)?;
            let mut settings = settings_from_nvm_for_image_with_capabilities_offline(
                nvm,
                &rgb,
                contract.combos,
                contract.version,
                contract.version,
                OfflineSettingsCapabilities {
                    combo_media: contract.combo_media,
                },
            )?;
            settings["limitations"] = json!(contract.limitations);
            Ok(settings)
        },
    )?;
    let contract = registry
        .iter()
        .find(|c| c.identity == archive.source)
        .ok_or("Offline contract disappeared")?
        .clone();
    Ok(OfflineValidatedEnvelope {
        manifest_sha256: archive.manifest_sha256,
        manifest: archive.manifest,
        settings: archive.settings,
        contract,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ak820_qs_versioned_offline::{RecordVersion, SettingsRecord};
    use crate::ak820_settings_backup::{canonical, sha256};
    use std::cell::Cell;

    fn contract() -> OfflineImageContract {
        OfflineImageContract {
            // These are synthetic fixture identities, never admitted firmware.
            identity: ImageIdentity {
                ihx_sha256: "a".repeat(64),
                code_sha256: "b".repeat(64),
                definition_sha256: "c".repeat(64),
                schema: 0x082003,
            },
            version: ImageVersion::V2,
            combos: true,
            combo_media: false,
            rgb_modes: vec![(1, 2), (2, 6)],
            rgb_max_brightness: 160,
            limitations: vec!["Synthetic offline fixture limitations; no ROM trust.".into()],
        }
    }
    fn nvm(record: Option<RecordVersion>) -> Vec<u8> {
        let mut nvm = vec![0; 0x2800];
        nvm[..2].copy_from_slice(&[0xe6, 0xfe]);
        nvm[37..40].copy_from_slice(&[3, 0x20, 8]);
        nvm[24..32].copy_from_slice(&[9, 20, 30, 160, 4, 0, 0, 0]);
        nvm[0x7f1..0x7f3].copy_from_slice(&16u16.to_le_bytes());
        if let Some(record) = record {
            nvm[0x6a8..0x6c0].copy_from_slice(&SettingsRecord::defaults(record).encode());
        }
        nvm
    }
    fn expected(nvm: &[u8], c: &OfflineImageContract) -> Value {
        let mut settings = settings_from_nvm_for_image_with_capabilities_offline(
            nvm,
            &saved_rgb(nvm, c).unwrap(),
            c.combos,
            c.version,
            c.version,
            OfflineSettingsCapabilities {
                combo_media: c.combo_media,
            },
        )
        .unwrap();
        settings["limitations"] = json!(c.limitations);
        settings
    }
    fn envelope(nvm: &[u8], c: &OfflineImageContract) -> (Vec<u8>, BTreeMap<String, Vec<u8>>) {
        let mut rom = vec![0; 65536];
        rom[0xc700..0xef00].copy_from_slice(nvm);
        let files = BTreeMap::from([
            (
                "settings.json".into(),
                canonical(&expected(nvm, c)).unwrap(),
            ),
            ("before-full-rom.bin".into(), rom),
            ("saved-nvm.bin".into(), nvm.to_vec()),
            ("read-log.jsonl".into(), vec![]),
            (".gitignore".into(), b"*\n".to_vec()),
        ]);
        let manifest = json!({"format":ak820_settings_backup::FORMAT_V2,
            "created_utc":"synthetic", "device":{"vid":0x320f,"pid":0x505b,"vial_uid":"0470ff9001b915d3"},
            "definition_sha256":c.identity.definition_sha256,"nvm_schema":c.identity.schema,
            "source_build_id":c.identity.schema,"source_ihx_sha256":c.identity.ihx_sha256,
            "code_sha256":c.identity.code_sha256,
            "counts":{"layers":6,"physical_keys":83,"assignments":498,"encoder_assignments":12,"macros":16,"macro_capacity":8192},
            "scope":"saved supported settings and exposed RGB values; not all current RAM","files":{}});
        (rehash(manifest, &files), files)
    }
    fn rehash(mut manifest: Value, files: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
        manifest["files"] = Value::Object(
            files
                .iter()
                .map(|(name, bytes)| {
                    (
                        name.clone(),
                        json!({"size":bytes.len(),"sha256":sha256(bytes)}),
                    )
                })
                .collect(),
        );
        canonical(&manifest).unwrap()
    }
    fn mutate_nvm(
        manifest: &[u8],
        files: &mut BTreeMap<String, Vec<u8>>,
        at: usize,
        byte: u8,
    ) -> Vec<u8> {
        files.get_mut("saved-nvm.bin").unwrap()[at] = byte;
        files.get_mut("before-full-rom.bin").unwrap()[0xc700 + at] = byte;
        rehash(ak820_settings_backup::parse_json(manifest).unwrap(), files)
    }
    // Deliberately accepts synthetic ROMs: tests of the envelope and decoder,
    // not a substitute for the caller's mandatory production-quality checks.
    fn fixture_only(_: &[u8], _: &[u8], _: &OfflineImageContract) -> BackupResult<()> {
        Ok(())
    }

    #[test]
    fn actual_envelopes_v2_retained_v1_and_blank_normalize_to_v2() {
        let c = contract();
        for record in [Some(RecordVersion::V1), Some(RecordVersion::V2), None] {
            let (manifest, files) = envelope(&nvm(record), &c);
            let original = files.clone();
            let result = validate_offline_envelope(
                &manifest,
                &files,
                &[c.clone()],
                Some(&sha256(&manifest)),
                fixture_only,
            )
            .unwrap();
            assert_eq!(result.settings["extensions"]["qmk_settings"]["3"], 1);
            assert_eq!(result.settings["rgb"]["current_mode_hex"], "060004141ea0");
            assert_eq!(result.settings["limitations"], json!(c.limitations));
            assert_eq!(result.contract.identity, c.identity);
            assert_eq!(result.manifest_sha256, sha256(&manifest));
            assert_eq!(files, original);
        }
    }

    #[test]
    fn identity_and_registry_ambiguity_fail_before_callback() {
        let c = contract();
        let (manifest, files) = envelope(&nvm(None), &c);
        let mut changed = c.clone();
        for field in 0..4 {
            changed.identity = c.identity.clone();
            match field {
                0 => changed.identity.ihx_sha256 = "d".repeat(64),
                1 => changed.identity.code_sha256 = "d".repeat(64),
                2 => changed.identity.definition_sha256 = "d".repeat(64),
                _ => changed.identity.schema = 0x082002,
            }
            assert!(
                validate_offline_envelope(
                    &manifest,
                    &files,
                    &[changed.clone()],
                    None,
                    |_, _, _| panic!("identity failure must precede callback")
                )
                .is_err()
            );
        }
        changed = c.clone();
        changed.identity.code_sha256 = "d".repeat(64);
        assert!(
            validate_offline_envelope(
                &manifest,
                &files,
                &[c.clone(), changed.clone()],
                None,
                |_, _, _| panic!("conflicting IHX must precede callback")
            )
            .unwrap_err()
            .contains("conflicting")
        );
        for version in [ImageVersion::V1, ImageVersion::V2] {
            changed = c.clone();
            changed.version = version;
            assert!(
                validate_offline_envelope(
                    &manifest,
                    &files,
                    &[c.clone(), changed.clone()],
                    None,
                    |_, _, _| panic!("duplicate must precede callback")
                )
                .unwrap_err()
                .contains("conflicting")
            );
        }
    }

    #[test]
    fn complete_rom_validator_precedes_semantics_and_comparison() {
        let c = contract();
        let (manifest, mut files) = envelope(&nvm(None), &c);
        let manifest = mutate_nvm(&manifest, &mut files, 0x6a8, 0x99);
        let calls = Cell::new(0);
        let error =
            validate_offline_envelope(&manifest, &files, &[c.clone()], None, |rom, nvm, got| {
                calls.set(calls.get() + 1);
                assert_eq!(got.identity, c.identity);
                assert_eq!(&rom[0xc700..0xef00], nvm);
                Err("fixture validator rejected ROM".into())
            })
            .unwrap_err();
        assert_eq!(error, "fixture validator rejected ROM");
        assert_eq!(calls.get(), 1);
        // Rehashed bytes reach QS validation after the callback succeeds.
        assert!(
            validate_offline_envelope(&manifest, &files, &[c], None, fixture_only)
                .unwrap_err()
                .contains("QS")
        );
    }

    #[test]
    fn rehashed_semantic_corruption_and_json_mismatch_are_rejected() {
        let c = contract();
        for (at, byte, message) in [
            (0x6a8, 1, "QS"),
            (0x6c0, 0xff, "Tap Dance"),
            (0x740, 0xff, "Key Override"),
            (0x7f0, 1, "macro"),
            (24, 0xff, "RGB"),
        ] {
            let (manifest, mut files) = envelope(&nvm(None), &c);
            let manifest = mutate_nvm(&manifest, &mut files, at, byte);
            let error =
                validate_offline_envelope(&manifest, &files, &[c.clone()], None, fixture_only)
                    .unwrap_err();
            assert!(
                error.to_lowercase().contains(&message.to_lowercase()),
                "{at:x}: {error}"
            );
        }
        let (manifest, mut files) = envelope(&nvm(None), &c);
        let mut settings = ak820_settings_backup::parse_json(&files["settings.json"]).unwrap();
        settings["layout_options"] = json!(9);
        files.insert("settings.json".into(), canonical(&settings).unwrap());
        let manifest = rehash(
            ak820_settings_backup::parse_json(&manifest).unwrap(),
            &files,
        );
        assert_eq!(
            validate_offline_envelope(&manifest, &files, &[c], None, fixture_only).unwrap_err(),
            "Settings differ from validated NVM"
        );
    }

    #[test]
    fn media_is_bound_to_source_contract_even_with_retained_v1() {
        let mut capable = contract();
        capable.combo_media = true;
        let mut nvm = nvm(Some(RecordVersion::V1));
        nvm[0x658..0x662].copy_from_slice(&[4, 0, 5, 0, 0x20, 8, 1, 0, 0xa8, 0]);
        let (manifest, files) = envelope(&nvm, &capable);
        assert!(
            validate_offline_envelope(&manifest, &files, &[capable.clone()], None, fixture_only)
                .is_ok()
        );
        assert_eq!(
            validate_offline_envelope(&manifest, &files, &[contract()], None, fixture_only)
                .unwrap_err(),
            "Unsupported Combo keys/output"
        );
        capable.version = ImageVersion::V1;
        assert!(
            validate_offline_envelope(&manifest, &files, &[capable], None, fixture_only)
                .unwrap_err()
                .contains("source V2")
        );
    }

    #[test]
    fn hashes_and_explicit_rgb_limitations_precede_acceptance() {
        let c = contract();
        let (manifest, mut files) = envelope(&nvm(None), &c);
        files.get_mut("saved-nvm.bin").unwrap()[0] ^= 1;
        assert_eq!(
            validate_offline_envelope(&manifest, &files, &[c.clone()], None, |_, _, _| panic!(
                "hash failure must precede callback"
            ))
            .unwrap_err(),
            "Backup hash/size mismatch"
        );
        let (manifest, files) = envelope(&nvm(None), &c);
        let mut different = c.clone();
        different.rgb_modes[1].1 = 13;
        assert_eq!(
            validate_offline_envelope(&manifest, &files, &[different], None, fixture_only)
                .unwrap_err(),
            "Settings differ from validated NVM"
        );
        let mut different = c.clone();
        different.limitations = vec!["Other explicit contract".into()];
        assert_eq!(
            validate_offline_envelope(&manifest, &files, &[different], None, fixture_only)
                .unwrap_err(),
            "Settings differ from validated NVM"
        );
        let mut incomplete = c;
        incomplete.limitations.clear();
        assert!(
            validate_offline_envelope(&manifest, &files, &[incomplete], None, |_, _, _| panic!(
                "incomplete contract must precede callback"
            ))
            .is_err()
        );
    }
}
