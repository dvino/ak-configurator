// SPDX-License-Identifier: GPL-3.0-or-later
//! Native local-input validation. Local files are evidence about bytes only;
//! they cannot prove snapshot freshness, physical ownership or release authority.
//! No HID open, transition, erase, write, restart or settings restore is connected.
use crate::ak820_image_pack::{validate_native_revision, APP_END, APP_START};
use crate::ak820_loader::{checksum, PayloadReports};
use crate::ak820_settings_backup::{sha256, BackupResult, ImageIdentity};
use crate::ak820_settings_model::decode_trusted_rom_for_image;
use crate::app::ak820_beta1_profiles::ProfileId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SnapshotKind {
    Oem,
    Project(crate::ak820::ImageContract),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LocalPlanSummary {
    pub(crate) target: ProfileId,
    pub(crate) snapshot_kind: SnapshotKind,
    pub(crate) payload_sha256: String,
    pub(crate) snapshot_sha256: String,
    pub(crate) recovery_sha256: String,
    pub(crate) expected_rom_sha256: String,
    pub(crate) binding_sha256: String,
    pub(crate) application_bytes: usize,
    pub(crate) data_blocks: usize,
    pub(crate) loader_checksum: u32,
}
/// Immutable copy of all selected bytes. A successful check is never called
/// installation readiness: a live adapter must establish ownership, durable
/// safety backup, metadata/revocation and fresh physical confirmation separately.
pub(crate) struct CheckedLocalPackage {
    summary: LocalPlanSummary,
    payload: Vec<u8>,
    snapshot: Vec<u8>,
    recovery: Vec<u8>,
    expected: Vec<u8>,
}
impl CheckedLocalPackage {
    pub(crate) fn prepare(
        target: ProfileId,
        payload: &[u8],
        snapshot: &[u8],
        recovery: &[u8],
    ) -> BackupResult<Self> {
        Self::prepare_for_image(
            target,
            payload,
            snapshot,
            recovery,
            crate::ak820::ImageContract::Beta3(target),
        )
    }
    pub(crate) fn prepare_for_image(
        target: ProfileId,
        payload: &[u8],
        snapshot: &[u8],
        recovery: &[u8],
        image: crate::ak820::ImageContract,
    ) -> BackupResult<Self> {
        Self::prepare_with_optional_recovery(target, payload, snapshot, Some(recovery), image)
    }
    pub(crate) fn prepare_with_optional_recovery(
        target: ProfileId,
        payload: &[u8],
        snapshot: &[u8],
        recovery: Option<&[u8]>,
        image: crate::ak820::ImageContract,
    ) -> BackupResult<Self> {
        // Validate complete sizes before taking any slices. A full-ROM payload
        // is forbidden even when it happens to contain the approved CODE.
        if image.profile() != Some(target) {
            return Err("Selected image/profile binding differs".into());
        }
        crate::ak820_image_pack::validate_catalog_payload_for_image(payload, image)?;
        let snapshot_kind = classify_snapshot(snapshot)?;
        if let Some(recovery) = recovery {
            if recovery.len() != 65536 {
                return Err("A complete 65536-byte OEM recovery ROM is required".into());
            }
            validate_native_revision(recovery)?;
            if snapshot[..APP_START] != recovery[..APP_START]
                || snapshot[APP_END..] != recovery[APP_END..]
            {
                return Err("Snapshot and recovery protected regions differ".into());
            }
        }
        let mut expected = snapshot.to_vec();
        expected[APP_START..APP_END].copy_from_slice(payload);
        let payload_sha256 = sha256(payload);
        let snapshot_sha256 = sha256(snapshot);
        let recovery_sha256 = recovery.map(sha256).unwrap_or_else(|| "none".into());
        let expected_rom_sha256 = sha256(&expected);
        let binding_sha256 = sha256(format!("ak820-local-inputs-1|{}|{snapshot_kind:?}|{payload_sha256}|{snapshot_sha256}|{recovery_sha256}|{expected_rom_sha256}", target.as_str()).as_bytes());
        let summary = LocalPlanSummary {
            target,
            snapshot_kind,
            payload_sha256,
            snapshot_sha256,
            recovery_sha256,
            expected_rom_sha256,
            binding_sha256,
            application_bytes: payload.len(),
            data_blocks: PayloadReports::new(payload)?.count(),
            loader_checksum: checksum(payload),
        };
        Ok(Self {
            summary,
            payload: payload.to_vec(),
            snapshot: snapshot.to_vec(),
            recovery: recovery.unwrap_or_default().to_vec(),
            expected,
        })
    }
    pub(crate) fn summary(&self) -> &LocalPlanSummary {
        &self.summary
    }
    pub(crate) fn payload(&self) -> &[u8] {
        &self.payload
    }
    pub(crate) fn snapshot(&self) -> &[u8] {
        &self.snapshot
    }
    pub(crate) fn recovery(&self) -> &[u8] {
        &self.recovery
    }
    /// Strict full-ROM comparison, including protected bytes, current marker,
    /// configuration, journal and manifest. Rejects short or changed readback.
    /// A later settings restore has a separate outcome and separate verifier.
    pub(crate) fn verify_full_readback(&self, rom: &[u8]) -> BackupResult<()> {
        verify_exact_readback(rom, &self.expected)
    }
}
// Byte ownership is not installation authority; raw images include OEM and synthetic data.
pub(crate) enum PreparedImageBytes {
    Firmware(Box<CheckedLocalPackage>),
    Raw {
        source: Vec<u8>,
        payload: Vec<u8>,
        expected: Vec<u8>,
    },
}
impl PreparedImageBytes {
    pub(crate) fn source(&self) -> &[u8] {
        match self {
            Self::Firmware(package) => package.snapshot(),
            Self::Raw { source, .. } => source,
        }
    }
    pub(crate) fn payload(&self) -> &[u8] {
        match self {
            Self::Firmware(package) => package.payload(),
            Self::Raw { payload, .. } => payload,
        }
    }
    pub(crate) fn expected(&self) -> &[u8] {
        match self {
            Self::Firmware(package) => &package.expected,
            Self::Raw { expected, .. } => expected,
        }
    }
    pub(crate) fn verify_full_readback(&self, rom: &[u8]) -> BackupResult<()> {
        let matches = match self {
            Self::Firmware(package) => package.verify_full_readback(rom).is_ok(),
            Self::Raw { expected, .. } => rom == expected,
        };
        if !matches {
            return Err(
                "Full-ROM readback differs; stop without retry or automatic recovery".into(),
            );
        }
        Ok(())
    }
}
fn verify_exact_readback(rom: &[u8], expected: &[u8]) -> BackupResult<()> {
    if rom.len() != 65536 || expected.len() != 65536 || rom != expected {
        return Err(
            "Full-ROM readback differs; stop without retry, reset or automatic recovery".into(),
        );
    }
    Ok(())
}
pub(crate) fn classify_snapshot(rom: &[u8]) -> BackupResult<SnapshotKind> {
    if rom.len() != 65536 {
        return Err("Snapshot must contain exactly 65536 bytes".into());
    }
    let mut found = validate_native_revision(rom)
        .is_ok()
        .then_some(SnapshotKind::Oem);
    for profile in [
        ProfileId::Main,
        ProfileId::Game,
        ProfileId::Mmo,
        ProfileId::Typing,
        ProfileId::Code,
        ProfileId::Pointer,
        ProfileId::Onehand,
        ProfileId::Creative,
        ProfileId::Shortcuts,
        ProfileId::Multios,
    ] {
        for image in [
            crate::ak820::ImageContract::Beta3(profile),
            crate::ak820::ImageContract::Beta4(profile),
            crate::ak820::ImageContract::Beta5(profile),
        ] {
            let Some(exact) = image
                .matrix_profile()
                .and_then(|row| row.settings_identity.as_ref())
            else {
                continue;
            };
            let identity = ImageIdentity {
                ihx_sha256: exact.ihx_sha256.clone(),
                code_sha256: exact.code_sha256.clone(),
                definition_sha256: exact.definition_sha256.clone(),
                schema: exact.nvm_schema,
            };
            if decode_trusted_rom_for_image(rom, &rom[0xc700..0xef00], &identity, image).is_ok() {
                if found.is_some() {
                    return Err("Snapshot matches more than one supported identity".into());
                }
                found = Some(SnapshotKind::Project(image));
            }
        }
    }
    found.ok_or_else(|| {
        "Snapshot is neither a supported OEM revision nor an exact supported project image".into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires private AK820_C03_FULL_ROM_PATH fixture"]
    fn beta5_project_fixture_packs_and_checks_complete_reinstall() {
        let source =
            std::fs::read(std::env::var_os("AK820_C03_FULL_ROM_PATH").expect("fixture path"))
                .expect("full ROM fixture");
        let image = crate::ak820::ImageContract::Beta5(ProfileId::Main);
        assert_eq!(
            classify_snapshot(&source).unwrap(),
            SnapshotKind::Project(image)
        );
        let admission = crate::ak820_release::ReleaseAdmission::selected(
            crate::ak820_release::InstallerRelease::Beta5,
            ProfileId::Main,
            true,
        )
        .unwrap();
        let payload = crate::ak820_release::bundled_payload_from_project(
            &admission,
            &source,
            Some(&source[0xc700..0xef00]),
        )
        .unwrap();
        assert!(payload == source[APP_START..APP_END]);
        let package = CheckedLocalPackage::prepare_with_optional_recovery(
            ProfileId::Main,
            &payload,
            &source,
            None,
            image,
        )
        .unwrap();
        package.verify_full_readback(&source).unwrap();
        for offset in [0, 0x2830, 0xc700, 0xef00, 0xefc0, 65535] {
            let mut changed = source.clone();
            changed[offset] ^= 1;
            assert!(package.verify_full_readback(&changed).is_err());
        }
        assert!(classify_snapshot(&source[..65535]).is_err());
        let mut corrupt = source.clone();
        corrupt[0x2830] ^= 1;
        assert!(classify_snapshot(&corrupt).is_err());
        assert!(CheckedLocalPackage::prepare_with_optional_recovery(
            ProfileId::Main,
            &source,
            &source,
            None,
            image,
        )
        .is_err());
        for row in &crate::app::ak820_beta1_profiles::beta5_firmware_matrix().profiles {
            let target = row.profile;
            let target_image = crate::ak820::ImageContract::Beta5(target);
            let target_admission = crate::ak820_release::ReleaseAdmission::selected(
                crate::ak820_release::InstallerRelease::Beta5,
                target,
                true,
            )
            .unwrap();
            let defaults = crate::ak820_release::bundled_defaults_for_image(target_image).unwrap();
            let target_payload = crate::ak820_release::bundled_payload_from_project(
                &target_admission,
                &source,
                None,
            )
            .unwrap();
            assert!(target_payload[0xc700 - APP_START..0xef00 - APP_START] == defaults);
            let target_package = CheckedLocalPackage::prepare_with_optional_recovery(
                target,
                &target_payload,
                &source,
                None,
                target_image,
            )
            .unwrap();
            let mut expected = source.clone();
            expected[APP_START..APP_END].copy_from_slice(&target_payload);
            assert!(expected[..APP_START] == source[..APP_START]);
            assert!(expected[APP_END..] == source[APP_END..]);
            assert_eq!(
                classify_snapshot(&expected).unwrap(),
                SnapshotKind::Project(target_image)
            );
            target_package.verify_full_readback(&expected).unwrap();
            expected[0xefc0] ^= 1;
            assert!(target_package.verify_full_readback(&expected).is_err());
        }
    }
    // Synthetic private data tests representation only, not preparation or admission.
    fn synthetic_package() -> CheckedLocalPackage {
        let snapshot = vec![0x5a; 65536];
        let payload = vec![0xa5; APP_END - APP_START];
        let mut expected = snapshot.clone();
        expected[APP_START..APP_END].copy_from_slice(&payload);
        CheckedLocalPackage {
            summary: LocalPlanSummary {
                target: ProfileId::Main,
                snapshot_kind: SnapshotKind::Oem,
                payload_sha256: String::new(),
                snapshot_sha256: String::new(),
                recovery_sha256: String::new(),
                expected_rom_sha256: String::new(),
                binding_sha256: String::new(),
                application_bytes: payload.len(),
                data_blocks: 0,
                loader_checksum: 0,
            },
            payload,
            snapshot,
            recovery: Vec::new(),
            expected,
        }
    }
    #[test]
    fn prepared_image_bytes_match_raw_and_executor_confirmation_preimage() {
        let package = synthetic_package();
        let raw = PreparedImageBytes::Raw {
            source: package.snapshot.clone(),
            payload: package.payload.clone(),
            expected: package.expected.clone(),
        };
        let checked = PreparedImageBytes::Firmware(Box::new(package));
        assert_eq!(checked.source(), raw.source());
        assert_eq!(checked.payload(), raw.payload());
        assert_eq!(checked.expected(), raw.expected());
        let preimage = |images: &PreparedImageBytes| {
            let release_pin = "synthetic-release";
            let action = "synthetic";
            let source_sha256 = sha256(images.source());
            let payload_sha256 = sha256(images.payload());
            let expected_sha256 = sha256(images.expected());
            let settings = crate::ak820_install_executor::InstallSettingsOutcome::DefaultsRequested;
            let hold = true;
            format!("ak820-install-1|{release_pin}|{action}|{source_sha256}|{payload_sha256}|{expected_sha256}|{settings:?}|{hold}")
        };
        assert_eq!(preimage(&checked), preimage(&raw));
        assert_eq!(
            sha256(preimage(&checked).as_bytes()),
            sha256(preimage(&raw).as_bytes())
        );
    }
    #[test]
    fn prepared_image_bytes_readback_matches_raw_and_preserves_errors() {
        let package = synthetic_package();
        let expected = package.expected.clone();
        let raw = PreparedImageBytes::Raw {
            source: package.snapshot.clone(),
            payload: package.payload.clone(),
            expected: expected.clone(),
        };
        let checked = PreparedImageBytes::Firmware(Box::new(package));
        assert!(checked.verify_full_readback(&expected).is_ok());
        assert!(raw.verify_full_readback(&expected).is_ok());
        let mut reads = Vec::new();
        for offset in [0, APP_START, 0xc700, 0xef00, 0xef80, 0xef90, APP_END, 65535] {
            let mut changed = expected.clone();
            changed[offset] ^= 1;
            reads.push(changed);
        }
        reads.push(expected[..65535].to_vec());
        let mut oversized = expected.clone();
        oversized.push(0);
        reads.push(oversized);
        for read in reads {
            assert_eq!(
                checked.verify_full_readback(&read),
                raw.verify_full_readback(&read)
            );
            assert_eq!(
                checked.verify_full_readback(&read).unwrap_err(),
                "Full-ROM readback differs; stop without retry or automatic recovery"
            );
            if let PreparedImageBytes::Firmware(package) = &checked {
                assert_eq!(
                    package.verify_full_readback(&read).unwrap_err(),
                    "Full-ROM readback differs; stop without retry, reset or automatic recovery"
                );
            }
        }
        let malformed = PreparedImageBytes::Raw {
            source: vec![],
            payload: vec![],
            expected: vec![1],
        };
        assert!(malformed.verify_full_readback(&[1]).is_ok());
        assert!(malformed.verify_full_readback(&[]).is_err());
    }
    #[test]
    fn arbitrary_or_incomplete_local_inputs_never_become_checked() {
        for profile in [
            ProfileId::Main,
            ProfileId::Game,
            ProfileId::Mmo,
            ProfileId::Typing,
            ProfileId::Code,
            ProfileId::Pointer,
            ProfileId::Onehand,
            ProfileId::Creative,
            ProfileId::Shortcuts,
            ProfileId::Multios,
        ] {
            for size in [0, APP_END - APP_START - 1, APP_END - APP_START, 65536] {
                assert!(CheckedLocalPackage::prepare(
                    profile,
                    &vec![0; size],
                    &[0; 65536],
                    &[0; 65536]
                )
                .is_err());
            }
        }
        for size in [0, 1, 0xc700, 65535, 65536, 65537] {
            assert!(classify_snapshot(&vec![0; size]).is_err());
        }
    }
    #[test]
    fn strict_readback_covers_every_region_and_rejects_partial_reads() {
        // Synthetic comparison test only; never bypasses production preparation.
        let expected = vec![0x5a; 65536];
        assert!(verify_exact_readback(&expected, &expected).is_ok());
        for offset in [
            0,
            0x27bf,
            0x27c0,
            0x27ff,
            APP_START,
            0xc6ff,
            0xc700,
            0xeeff,
            0xef00,
            0xef40,
            0xef80,
            APP_END - 1,
            APP_END,
            65535,
        ] {
            let mut changed = expected.clone();
            changed[offset] ^= 1;
            assert!(verify_exact_readback(&changed, &expected).is_err());
        }
        assert!(verify_exact_readback(&expected[..65535], &expected).is_err());
        assert!(verify_exact_readback(&[], &[]).is_err());
    }
}
