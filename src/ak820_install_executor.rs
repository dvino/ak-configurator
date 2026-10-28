// SPDX-License-Identifier: GPL-3.0-or-later
//! One-attempt installation transaction. Only the native owner implements IO.
use crate::ak820_image_pack::{APP_END, APP_START};
use crate::ak820_installer::{CheckedLocalPackage, SnapshotKind};
use crate::ak820_loader::{self as loader, Command, PayloadReports};
use crate::ak820_settings_backup::{BackupResult, sha256};
use crate::app::ak820_beta1_profiles::ProfileId;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

pub(crate) enum InstallRequest {
    /// Build locally only after a fresh source read. Missing archived OEM bytes
    /// are permitted solely when that source itself is verified OEM firmware.
    BundledFirmware {
        profile: ProfileId,
        image: crate::ak820::ImageContract,
        recovery: Option<Vec<u8>>,
        settings: InstallSettingsChoice,
    },
    Firmware {
        profile: ProfileId,
        image: crate::ak820::ImageContract,
        payload: Vec<u8>,
        recovery: Vec<u8>,
        settings: InstallSettingsChoice,
    },
    OemRollback {
        recovery: Vec<u8>,
        provenance: RecoveryProvenance,
    },
}
fn preserved_nvm(
    source_kind: SnapshotKind,
    source: &[u8],
    target: ProfileId,
) -> BackupResult<Vec<u8>> {
    let source_profile=match source_kind { SnapshotKind::Project(p)=>p,SnapshotKind::Oem=>return Err("Project settings preservation requires a verified project source; select defaults for the initial OEM installation".into()) };
    let capture = crate::ak820_backup_capture::CapturedBackup::from_rom(
        source_profile,
        source.to_vec(),
        false,
    )?;
    let archive = crate::ak820_backup_files::validate_captured(&capture)?;
    let exact = crate::app::ak820_beta1_profiles::matrix_profile(target)
        .settings_identity
        .as_ref()
        .ok_or("Missing target settings identity")?;
    let identity = crate::ak820_settings_backup::ImageIdentity {
        ihx_sha256: exact.ihx_sha256.clone(),
        code_sha256: exact.code_sha256.clone(),
        definition_sha256: exact.definition_sha256.clone(),
        schema: exact.nvm_schema,
    };
    Ok(crate::ak820_restore_plan::plan_restore(
        &archive,
        capture.nvm(),
        target,
        &identity,
        &crate::ak820_release::bundled_defaults(target)?,
    )?
    .expected_nvm()
    .to_vec())
}
fn bundled_recovery(
    source_kind: SnapshotKind,
    source: &[u8],
    supplied: Option<Vec<u8>>,
) -> BackupResult<Vec<u8>> {
    if source.len() != 65536 {
        return Err("Bundled preparation requires a complete fresh ROM".into());
    }
    let recovery=match supplied {
        Some(recovery)=>recovery,
        None=>match source_kind {
            SnapshotKind::Oem=>source.to_vec(),
            SnapshotKind::Project(_)=>return Err("Updating project firmware requires an existing personal OEM reference; the current project ROM cannot replace it".into()),
        },
    };
    crate::ak820_image_pack::validate_native_revision(&recovery)?;
    if source_kind == SnapshotKind::Oem && recovery != source {
        return Err("Initial OEM reference must equal the complete fresh device capture".into());
    }
    Ok(recovery)
}

/// Locally retained first-capture provenance, never an image revision claim.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RecoveryProvenance {
    format: String,
    recovery_sha256: String,
    owner_binding: String,
}
#[derive(Clone, Debug)]
pub(crate) enum InstallSettingsChoice {
    Defaults,
    PreservedInPayload { nvm_sha256: String },
}
#[derive(Clone, Debug)]
pub(crate) struct InstallationSummary {
    pub(crate) source_is_oem: bool,
    pub(crate) action: String,
    pub(crate) source_sha256: String,
    pub(crate) payload_sha256: String,
    pub(crate) expected_sha256: String,
    pub(crate) confirmation_pin: String,
    pub(crate) physical_hold: bool,
}
pub(crate) struct PreparedInstallation {
    source: Vec<u8>,
    expected: Vec<u8>,
    payload: Vec<u8>,
    summary: InstallationSummary,
    settings: InstallSettingsOutcome,
    provenance: Option<RecoveryProvenance>,
    target: Option<ProfileId>,
    release_metadata_sha: String,
}
impl PreparedInstallation {
    pub(crate) fn recovery_provenance(&self) -> Option<&RecoveryProvenance> {
        self.provenance.as_ref()
    }
    pub(crate) fn summary(&self) -> &InstallationSummary {
        &self.summary
    }
    pub(crate) fn confirmation_pin(&self) -> &str {
        &self.summary.confirmation_pin
    }
    pub(crate) fn prepare(
        request: InstallRequest,
        source: Vec<u8>,
        admission: Option<&crate::ak820_release::ReleaseAdmission>,
        owner_binding: Option<String>,
    ) -> BackupResult<Self> {
        let release_pin = admission
            .map(|a| format!("{}:{}", a.version(), a.metadata_sha256()))
            .unwrap_or_else(|| "personal-oem-recovery".into());
        let source_kind = crate::ak820_installer::classify_snapshot(&source)?;
        if let InstallRequest::BundledFirmware {
            profile,
            image,
            recovery,
            settings,
        } = request
        {
            let recovery = bundled_recovery(source_kind, &source, recovery)?;
            let nvm = match &settings {
                InstallSettingsChoice::Defaults => crate::ak820_release::bundled_defaults_for_image(image)?,
                InstallSettingsChoice::PreservedInPayload { nvm_sha256 } => {
                    if matches!(image, crate::ak820::ImageContract::Beta4(_)) {
                        return Err("Beta.4.1 does not authorize preserving saved settings".into());
                    }
                    let nvm = preserved_nvm(source_kind, &source, profile)?;
                    if sha256(&nvm) != *nvm_sha256 {
                        return Err(
                            "Prepared settings NVM pin differs from the checked preservation plan"
                                .into(),
                        );
                    }
                    nvm
                }
            };
            let payload = crate::ak820_release::bundled_payload_for_image(image, &recovery, Some(&nvm))?;
            return Self::prepare(
                InstallRequest::Firmware {
                    profile,
                    image,
                    payload,
                    recovery,
                    settings,
                },
                source,
                admission,
                owner_binding,
            );
        }

        let captured_provenance = if source_kind == SnapshotKind::Oem {
            owner_binding.as_ref().map(|binding| RecoveryProvenance {
                format: "ak820-oem-provenance-1".into(),
                recovery_sha256: sha256(&source),
                owner_binding: binding.clone(),
            })
        } else {
            None
        };
        let target = match &request {
            InstallRequest::Firmware { profile, .. } => Some(*profile),
            _ => None,
        };
        let (payload, action, hold, settings) = match request {
            InstallRequest::BundledFirmware { .. } => {
                return Err("Bundled request was not normalized".into());
            }
            InstallRequest::Firmware {
                profile,
                image,
                payload,
                recovery,
                settings,
            } => {
                let admission = admission.ok_or("Firmware release admission is required")?;
                if admission.image() != image {
                    return Err("Selected firmware image differs from its release authorization".into());
                }
                admission.validate_payload(profile, &payload)?;
                let package = CheckedLocalPackage::prepare_for_image(profile, &payload, &source, &recovery, image)?;
                if source_kind == SnapshotKind::Oem && recovery != source {
                    return Err(
                        "Initial OEM reference must equal the complete fresh device capture".into(),
                    );
                }
                if matches!(image, crate::ak820::ImageContract::Beta4(_))
                    && !matches!(&settings, InstallSettingsChoice::Defaults)
                {
                    return Err("Beta.4.1 installation requires explicit image defaults; saved settings are not authorized".into());
                }
                let hold = matches!(package.summary().snapshot_kind, SnapshotKind::Project(_))
                    || matches!(image, crate::ak820::ImageContract::Beta4(_));
                let outcome = match settings {
                    InstallSettingsChoice::Defaults => {
                        if payload[0xc700 - APP_START..0xef00 - APP_START]
                            != crate::ak820_release::bundled_defaults_for_image(image)?
                        {
                            return Err("Payload does not contain the selected defaults".into());
                        }
                        InstallSettingsOutcome::DefaultsRequested
                    }
                    InstallSettingsChoice::PreservedInPayload { nvm_sha256 } => {
                        if sha256(&payload[0xc700 - APP_START..0xef00 - APP_START]) != nvm_sha256 {
                            return Err("Prepared settings NVM pin differs".into());
                        }
                        let preserved = preserved_nvm(source_kind, &source, profile)?;
                        if payload[0xc700 - APP_START..0xef00 - APP_START] != preserved {
                            return Err("Payload settings do not match the checked source/target preservation plan".into());
                        }
                        InstallSettingsOutcome::PreparedImageSettingsVerified
                    }
                };
                (
                    payload,
                    format!("firmware:{}:{}", admission.version(), profile.as_str()),
                    hold,
                    outcome,
                )
            }
            InstallRequest::OemRollback {
                recovery,
                provenance,
            } => {
                let binding=owner_binding.as_ref().ok_or("OEM rollback requires a stable nonempty per-unit serial and physical USB port; this device cannot establish ownership")?;
                if provenance.format != "ak820-oem-provenance-1"
                    || provenance.owner_binding != *binding
                    || provenance.recovery_sha256 != sha256(&recovery)
                {
                    return Err("OEM recovery has no matching first-capture provenance for this keyboard and USB port".into());
                }
                crate::ak820_image_pack::validate_native_revision(&recovery)?;
                if source.len() != 65536
                    || recovery[..APP_START] != source[..APP_START]
                    || recovery[APP_END..] != source[APP_END..]
                {
                    return Err(
                        "OEM recovery protected regions differ from the fresh device".into(),
                    );
                }
                let hold = crate::ak820_installer::classify_snapshot(&source)? != SnapshotKind::Oem;
                (
                    recovery[APP_START..APP_END].to_vec(),
                    "oem-rollback".into(),
                    hold,
                    InstallSettingsOutcome::OemSettings,
                )
            }
        };
        let mut expected = source.clone();
        expected[APP_START..APP_END].copy_from_slice(&payload);
        let source_sha256 = sha256(&source);
        let payload_sha256 = sha256(&payload);
        let expected_sha256 = sha256(&expected);
        let confirmation_pin = sha256(format!("ak820-install-1|{release_pin}|{action}|{source_sha256}|{payload_sha256}|{expected_sha256}|{settings:?}|{hold}").as_bytes());
        Ok(Self {
            source,
            expected,
            payload,
            settings,
            provenance: captured_provenance,
            target,
            release_metadata_sha: admission
                .map(|a| a.metadata_sha256().to_owned())
                .unwrap_or_default(),
            summary: InstallationSummary {
                source_is_oem: source_kind == SnapshotKind::Oem,
                action,
                source_sha256,
                payload_sha256,
                expected_sha256,
                confirmation_pin,
                physical_hold: hold,
            },
        })
    }
}
/// IO is deliberately unavailable to simulation events and generic HID requests.
pub(super) trait InstallTransport {
    fn read_rom(&mut self) -> BackupResult<Vec<u8>>;
    fn fresh_hold(&mut self, cancel: &AtomicBool) -> BackupResult<()>;
    fn transition(&mut self) -> BackupResult<()>;
    fn loader_command(&mut self, command: Command, checksum: u32) -> BackupResult<()>;
    fn loader_block(&mut self, report: &[u8; 65]) -> BackupResult<()>;
    fn reconnect(&mut self) -> BackupResult<()>;
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum InstallFirmwareOutcome {
    NotAttempted,
    Unknown,
    Verified,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum InstallSettingsOutcome {
    DefaultsRequested,
    PreparedImageSettingsVerified,
    OemSettings,
    Unverified,
}
#[derive(Clone, Debug)]
pub(crate) struct InstallationResult {
    pub(crate) firmware: InstallFirmwareOutcome,
    pub(crate) settings: InstallSettingsOutcome,
    pub(crate) error: Option<String>,
}
pub(super) fn execute(
    io: &mut impl InstallTransport,
    plan: &PreparedInstallation,
    admission: Option<&crate::ak820_release::ReleaseAdmission>,
    confirmed_pin: &str,
    cancel: &AtomicBool,
    progress: &AtomicU32,
    save_safety: impl FnOnce(&[u8]) -> BackupResult<()>,
) -> InstallationResult {
    let mut outcome = InstallFirmwareOutcome::NotAttempted;
    let check = || {
        if cancel.load(Ordering::Acquire) {
            Err("Installation cancelled before erase".to_string())
        } else {
            Ok(())
        }
    };
    let result = (|| -> BackupResult<()> {
        check()?;
        if let Some(target) = plan.target {
            let admission =
                admission.ok_or("Fresh release admission is required before execution")?;
            admission.validate_payload(target, &plan.payload)?;
            if admission.metadata_sha256() != plan.release_metadata_sha {
                return Err("Release metadata changed since preview".into());
            }
        }
        if confirmed_pin != plan.confirmation_pin() {
            return Err("Installation confirmation does not match the displayed plan".into());
        }
        progress.store(0, Ordering::Release);
        if io.read_rom()? != plan.source {
            return Err("Device changed since installation preview".into());
        }
        save_safety(&plan.source)?;
        check()?;
        progress.store(10, Ordering::Release);
        if plan.summary.physical_hold {
            io.fresh_hold(cancel)?;
        }
        check()?;
        if io.read_rom()? != plan.source {
            return Err("Device changed during confirmation".into());
        }
        io.transition()?;
        io.loader_command(Command::Identity, 0)?;
        check()?;
        progress.store(20, Ordering::Release);
        // From this boundary cancellation cannot interrupt the single attempt.
        outcome = InstallFirmwareOutcome::Unknown;
        io.loader_command(Command::Erase, 0)?;
        io.loader_command(Command::Begin, 0)?;
        for (index, block) in PayloadReports::new(&plan.payload)?.enumerate() {
            io.loader_block(&block)?;
            progress.store(25 + ((index + 1) * 60 / 799) as u32, Ordering::Release);
        }
        io.loader_command(Command::Checksum, loader::checksum(&plan.payload))?;
        io.loader_command(Command::Restart, 0)?;
        io.reconnect()?;
        progress.store(90, Ordering::Release);
        if io.read_rom()? != plan.expected {
            return Err(
                "Full-ROM readback differs; stop without retry or automatic recovery".into(),
            );
        }
        outcome = InstallFirmwareOutcome::Verified;
        progress.store(100, Ordering::Release);
        Ok(())
    })();
    InstallationResult {
        settings: if outcome == InstallFirmwareOutcome::Verified {
            plan.settings.clone()
        } else {
            InstallSettingsOutcome::Unverified
        },
        firmware: outcome,
        error: result.err(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fake {
        calls: Vec<String>,
        fail: Option<usize>,
        rom: Vec<u8>,
        after: Vec<u8>,
    }
    impl Fake {
        fn hit(&mut self, name: &str) -> BackupResult<()> {
            self.calls.push(name.into());
            if self.fail == Some(self.calls.len()) {
                Err("injected transport fault".into())
            } else {
                Ok(())
            }
        }
    }
    impl InstallTransport for Fake {
        fn read_rom(&mut self) -> BackupResult<Vec<u8>> {
            self.hit("read-rom")?;
            Ok(if self.calls.iter().any(|s| s == "reconnect") {
                self.after.clone()
            } else {
                self.rom.clone()
            })
        }
        fn fresh_hold(&mut self, _: &AtomicBool) -> BackupResult<()> {
            self.hit("fresh-hold")
        }
        fn transition(&mut self) -> BackupResult<()> {
            self.hit("transition")
        }
        fn loader_command(&mut self, c: Command, _: u32) -> BackupResult<()> {
            self.hit(&format!("{c:?}"))
        }
        fn loader_block(&mut self, _: &[u8; 65]) -> BackupResult<()> {
            self.hit("block")
        }
        fn reconnect(&mut self) -> BackupResult<()> {
            self.hit("reconnect")
        }
    }
    fn fixture() -> PreparedInstallation {
        // Private engine-only fixture: cannot create a real native transport.
        let source = vec![0; 65536];
        let payload = vec![1; APP_END - APP_START];
        let mut expected = source.clone();
        expected[APP_START..APP_END].copy_from_slice(&payload);
        PreparedInstallation {
            source,
            expected,
            payload,
            settings: InstallSettingsOutcome::DefaultsRequested,
            provenance: None,
            target: None,
            release_metadata_sha: String::new(),
            summary: InstallationSummary {
                source_is_oem: false,
                action: "synthetic".into(),
                source_sha256: String::new(),
                payload_sha256: String::new(),
                expected_sha256: String::new(),
                confirmation_pin: "fixture".into(),
                physical_hold: true,
            },
        }
    }
    fn fake(p: &PreparedInstallation) -> Fake {
        Fake {
            calls: vec![],
            fail: None,
            rom: p.source.clone(),
            after: p.expected.clone(),
        }
    }
    #[test]
    fn successful_order_and_every_fault_stop_without_replay() {
        let p = fixture();
        let mut io = fake(&p);
        let progress = AtomicU32::new(0);
        let cancel = AtomicBool::new(false);
        let result = execute(&mut io, &p, None, "fixture", &cancel, &progress, |rom| {
            assert_eq!(rom, p.source);
            Ok(())
        });
        assert_eq!(result.firmware, InstallFirmwareOutcome::Verified);
        assert!(result.error.is_none());
        assert_eq!(
            &io.calls[..7],
            &[
                "read-rom",
                "fresh-hold",
                "read-rom",
                "transition",
                "Identity",
                "Erase",
                "Begin"
            ]
        );
        assert_eq!(
            io.calls.iter().filter(|s| s.as_str() == "block").count(),
            799
        );
        assert_eq!(
            &io.calls[806..],
            &["Checksum", "Restart", "reconnect", "read-rom"]
        );
        for fault in 1..=io.calls.len() {
            let mut bad = fake(&p);
            bad.fail = Some(fault);
            let result = execute(
                &mut bad,
                &p,
                None,
                "fixture",
                &cancel,
                &progress,
                |_| Ok(()),
            );
            assert!(result.error.is_some(), "fault {fault}");
            assert_ne!(result.firmware, InstallFirmwareOutcome::Verified);
            assert_eq!(
                bad.calls.len(),
                fault,
                "no continuation or retry at fault {fault}"
            );
        }
    }
    #[test]
    fn safety_failure_confirmation_cancel_and_changed_source_precede_transition() {
        let p = fixture();
        let progress = AtomicU32::new(0);
        for case in 0..4 {
            let mut io = fake(&p);
            if case == 3 {
                io.rom[0] ^= 1;
            }
            let result = execute(
                &mut io,
                &p,
                None,
                if case == 0 { "stale" } else { "fixture" },
                &AtomicBool::new(case == 1),
                &progress,
                |_| {
                    if case == 2 {
                        Err("save failed".into())
                    } else {
                        Ok(())
                    }
                },
            );
            assert!(result.error.is_some());
            assert_eq!(result.firmware, InstallFirmwareOutcome::NotAttempted);
            assert!(!io.calls.iter().any(|s| s == "transition"));
        }
    }
    #[test]
    fn bundled_preparation_has_no_unverified_oem_reference_fallback() {
        let source = vec![0; 65536];
        let error =
            bundled_recovery(SnapshotKind::Project(ProfileId::Main), &source, None).unwrap_err();
        assert!(error.contains("existing personal OEM reference"));
        for size in [0, 1, 65535, 65537] {
            assert!(bundled_recovery(SnapshotKind::Oem, &vec![0; size], None).is_err());
        }
        // Synthetic bytes never become an accepted native revision, whether
        // supplied explicitly or selected as a no-archive source candidate.
        assert!(bundled_recovery(SnapshotKind::Oem, &source, None).is_err());
        assert!(bundled_recovery(SnapshotKind::Oem, &source, Some(vec![0; 65536])).is_err());
        assert!(
            bundled_recovery(
                SnapshotKind::Project(ProfileId::Main),
                &source,
                Some(vec![])
            )
            .is_err()
        );
        assert!(preserved_nvm(SnapshotKind::Oem, &source, ProfileId::Main).is_err());
        let request = InstallRequest::BundledFirmware {
            profile: ProfileId::Main,
            image: crate::ak820::ImageContract::Beta3(ProfileId::Main),
            recovery: None,
            settings: InstallSettingsChoice::Defaults,
        };
        assert!(PreparedInstallation::prepare(request, source, None, None).is_err());
    }
    #[test]
    fn firmware_execution_cannot_omit_current_release_admission() {
        let mut p = fixture();
        p.target = Some(ProfileId::Main);
        let mut io = fake(&p);
        let result = execute(
            &mut io,
            &p,
            None,
            "fixture",
            &AtomicBool::new(false),
            &AtomicU32::new(0),
            |_| panic!("must not save"),
        );
        assert_eq!(result.firmware, InstallFirmwareOutcome::NotAttempted);
        assert!(result.error.unwrap().contains("Fresh release admission"));
        assert!(io.calls.is_empty());
    }
    #[test]
    fn complete_readback_checks_protected_bytes_and_short_read() {
        let p = fixture();
        for offset in [0, 0x27c0, APP_START, 0xc700, APP_END, 65535, 65536] {
            let mut io = fake(&p);
            if offset == 65536 {
                io.after.pop();
            } else {
                io.after[offset] ^= 1;
            }
            let result = execute(
                &mut io,
                &p,
                None,
                "fixture",
                &AtomicBool::new(false),
                &AtomicU32::new(0),
                |_| Ok(()),
            );
            assert_eq!(result.firmware, InstallFirmwareOutcome::Unknown);
            assert!(result.error.is_some());
            assert_eq!(io.calls.last().unwrap(), "read-rom");
        }
    }
}
