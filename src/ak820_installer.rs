// SPDX-License-Identifier: GPL-3.0-or-later
//! Native local-input validation. Local files are evidence about bytes only;
//! they cannot prove snapshot freshness, physical ownership or release authority.
//! No HID open, transition, erase, write, restart or settings restore is connected.
use crate::ak820_image_pack::{
    validate_catalog_payload, validate_native_revision, APP_END, APP_START,
};
use crate::ak820_loader::{checksum, PayloadReports};
use crate::ak820_settings_backup::{sha256, BackupResult, ImageIdentity};
use crate::ak820_settings_model::decode_trusted_rom;
use crate::app::ak820_beta1_profiles::{matrix_profile, ProfileId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SnapshotKind {
    Oem,
    Project(ProfileId),
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
        // Validate complete sizes before taking any slices. A full-ROM payload
        // is forbidden even when it happens to contain the approved CODE.
        validate_catalog_payload(payload, target)?;
        if snapshot.len() != 65536 || recovery.len() != 65536 {
            return Err("A complete 65536-byte snapshot and OEM recovery ROM are required".into());
        }
        validate_native_revision(recovery)?;
        if snapshot[..APP_START] != recovery[..APP_START]
            || snapshot[APP_END..] != recovery[APP_END..]
        {
            return Err("Snapshot and recovery protected regions differ".into());
        }
        let snapshot_kind = classify_snapshot(snapshot)?;
        let mut expected = snapshot.to_vec();
        expected[APP_START..APP_END].copy_from_slice(payload);
        let payload_sha256 = sha256(payload);
        let snapshot_sha256 = sha256(snapshot);
        let recovery_sha256 = sha256(recovery);
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
            recovery: recovery.to_vec(),
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
fn verify_exact_readback(rom: &[u8], expected: &[u8]) -> BackupResult<()> {
    if rom.len() != 65536 || expected.len() != 65536 || rom != expected {
        return Err(
            "Full-ROM readback differs; stop without retry, reset or automatic recovery".into(),
        );
    }
    Ok(())
}
fn classify_snapshot(rom: &[u8]) -> BackupResult<SnapshotKind> {
    if validate_native_revision(rom).is_ok() {
        return Ok(SnapshotKind::Oem);
    }
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
        let contract = matrix_profile(profile);
        let Some(exact) = contract.settings_identity.as_ref() else {
            continue;
        };
        let identity = ImageIdentity {
            ihx_sha256: exact.ihx_sha256.clone(),
            code_sha256: exact.code_sha256.clone(),
            definition_sha256: exact.definition_sha256.clone(),
            schema: exact.nvm_schema,
        };
        if decode_trusted_rom(rom, &rom[0xc700..0xef00], &identity, profile).is_ok() {
            return Ok(SnapshotKind::Project(profile));
        }
    }
    Err("Snapshot is neither a supported OEM revision nor an exact supported project image".into())
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert!(classify_snapshot(&[0; 65536]).is_err());
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
