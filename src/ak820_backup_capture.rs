//! Immutable complete capture bytes. Construction never grants restore permission.
use crate::ak820_settings_backup::{canonical, BackupResult, ImageIdentity};
use crate::app::ak820_beta1_profiles::{matrix_profile, ProfileId};
use std::collections::BTreeMap;

/// Byte stability only: success does not establish firmware trust or ownership.
pub(crate) fn check_stability(
    rom: &[u8],
    repeated_nvm: &[u8],
    repeated_manifest: &[u8],
) -> BackupResult<()> {
    if rom.len() != 65536 || repeated_nvm.len() != 0x2800 || repeated_manifest.len() != 64 {
        return Err("Incomplete repeated capture".into());
    }
    if rom[0xc700..0xef00] != *repeated_nvm {
        return Err("Settings changed during backup capture".into());
    }
    if rom[0xef80..0xefc0] != *repeated_manifest {
        return Err("Image changed during backup capture".into());
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub(crate) struct CapturedBackup {
    profile: ProfileId,
    identity: ImageIdentity,
    files: BTreeMap<String, Vec<u8>>,
}
impl CapturedBackup {
    pub(crate) fn profile(&self) -> ProfileId {
        self.profile
    }
    pub(crate) fn identity(&self) -> &ImageIdentity {
        &self.identity
    }
    pub(crate) fn files(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.files
    }
    pub(crate) fn nvm(&self) -> &[u8] {
        &self.files["saved-nvm.bin"]
    }
    pub(crate) fn from_rom(
        profile: ProfileId,
        rom: Vec<u8>,
        rgb_matches_saved: bool,
    ) -> BackupResult<Self> {
        let exact = matrix_profile(profile)
            .settings_identity
            .as_ref()
            .ok_or("Image lacks qualified settings identity")?;
        let identity = ImageIdentity {
            ihx_sha256: exact.ihx_sha256.clone(),
            code_sha256: exact.code_sha256.clone(),
            definition_sha256: exact.definition_sha256.clone(),
            schema: exact.nvm_schema,
        };
        if rom.len() != 65536 {
            return Err("Incomplete capture ROM".into());
        }
        let nvm = rom[0xc700..0xef00].to_vec();
        let settings =
            crate::ak820_settings_model::decode_trusted_rom(&rom, &nvm, &identity, profile)?;
        let mut files = BTreeMap::new();
        files.insert("settings.json".into(), canonical(&settings)?);
        files.insert("before-full-rom.bin".into(), rom);
        files.insert("saved-nvm.bin".into(), nvm);
        files.insert(".gitignore".into(), b"*\n".to_vec());
        // Values, device paths and packet contents are deliberately excluded.
        let log = serde_json::json!({"operation":"stable-read-only-capture","full_rom_bytes":65536,"repeated_nvm_bytes":10240,"manifest_rechecked":true,"live_getters_reconciled":true,"live_rgb_matches_saved":rgb_matches_saved,"scope":"durable settings; volatile RGB is not substituted"});
        let mut log = serde_json::to_vec(&log).map_err(|_| "Capture metadata encoding failed")?;
        log.push(b'\n');
        files.insert("read-log.jsonl".into(), log);
        Ok(Self {
            profile,
            identity,
            files,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capture_stability_checks_exact_ranges_without_granting_trust() {
        let rom = vec![0; 65536];
        let mut nvm = vec![0; 0x2800];
        let mut manifest = vec![0; 64];
        assert!(check_stability(&rom, &nvm, &manifest).is_ok());
        nvm[0x27ff] = 1;
        assert!(check_stability(&rom, &nvm, &manifest).is_err());
        nvm[0x27ff] = 0;
        manifest[63] = 1;
        assert!(check_stability(&rom, &nvm, &manifest).is_err());
        assert!(check_stability(&rom[..65535], &nvm, &manifest).is_err());
        assert!(CapturedBackup::from_rom(ProfileId::Main, rom, false).is_err());
    }
    #[test]
    fn capture_rejects_incomplete_or_untrusted_rom() {
        assert!(CapturedBackup::from_rom(ProfileId::Main, vec![0; 100], false).is_err());
        // Synthetic zero bytes must never become a trusted positive archive.
        assert!(CapturedBackup::from_rom(ProfileId::Main, vec![0; 65536], false).is_err());
    }
}
