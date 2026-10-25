// SPDX-License-Identifier: GPL-3.0-or-later
//! Pure offline installer model. Ordering informed by dvino/ak820
//! bef20f89d3da01d770767919bc7c8fc3c3a1a808 tools/installer/ak820.py
//! (install/update_prepared/recover), GPL-3.0-or-later. No Python code copied.
//! Every event is a simulation, including ownership and physical confirmation.
//! This module cannot mint hardware authorization, open a device, or execute IO.
use crate::ak820_image_pack::{
    validate_catalog_payload, validate_native_revision, OfflineCandidate, APP_END, APP_START,
    SOURCE_REVISION,
};
use crate::ak820_settings_backup::{sha256, BackupResult};
use crate::app::ak820_beta1_profiles::{matrix_profile, ProfileId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MetadataFreshness {
    Known { checked_at: u64, expires_at: u64 },
    OfflineUnknown,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReleaseSelection {
    pub(crate) version: String,
    pub(crate) metadata_sha256: String,
    pub(crate) freshness: MetadataFreshness,
    pub(crate) prerelease: bool,
    pub(crate) prerelease_opt_in: bool,
    pub(crate) revoked_images: Vec<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InputOrigin {
    Bundled,
    Local,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConfirmationKind {
    FreshEscLeftCtrlHold,
    ExplicitStartFlashing,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Firmware(ProfileId),
    OemRollback,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SettingsChoice {
    Defaults,
    /// Exact immutable settings plan digest; separate restore model must validate it.
    RestorePlan(String),
}
/// These remain unmet by this module, even after a successful fake trace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HardwareGate {
    ExactPackedImageQualification,
    ReleaseAuthorityAndFreshRevocation,
    DeviceSpecificSourceOwnership,
    FreshCompleteBackup,
    ExclusiveProcessAndHelperOwnership,
    FreshOperationConfirmation,
    ProtectedRegionWriteQualification,
    FullReconnectReadback,
    OptionalSettingsRestoreQualification,
}
pub(crate) const UNMET_HARDWARE_GATES: [HardwareGate; 9] = [
    HardwareGate::ExactPackedImageQualification,
    HardwareGate::ReleaseAuthorityAndFreshRevocation,
    HardwareGate::DeviceSpecificSourceOwnership,
    HardwareGate::FreshCompleteBackup,
    HardwareGate::ExclusiveProcessAndHelperOwnership,
    HardwareGate::FreshOperationConfirmation,
    HardwareGate::ProtectedRegionWriteQualification,
    HardwareGate::FullReconnectReadback,
    HardwareGate::OptionalSettingsRestoreQualification,
];

/// Immutable inputs, including complete expected ROM. Hash pins identify bytes,
/// never ownership of a physical keyboard. No public constructor bypasses checks.
#[derive(Clone, Debug)]
pub(crate) struct PreparedInstall {
    action: Action,
    origin: InputOrigin,
    release: ReleaseSelection,
    source: Vec<u8>,
    expected: Vec<u8>,
    fresh_backup: Vec<u8>,
    settings: SettingsChoice,
    confirmation: ConfirmationKind,
    generation: u64,
    prepared_at: u64,
    confirmation_sha256: String,
}
impl PreparedInstall {
    pub(crate) fn firmware(
        candidate: &OfflineCandidate,
        profile: ProfileId,
        origin: InputOrigin,
        source: &[u8],
        fresh_backup: &[u8],
        release: ReleaseSelection,
        settings: SettingsChoice,
        generation: u64,
        now: u64,
    ) -> BackupResult<Self> {
        // Catalog presence or a linked fingerprint is insufficient.
        if matrix_profile(profile).settings_identity.is_none() {
            return Err("Missing qualified packed image identity".into());
        }
        validate_catalog_payload(candidate.bytes(), profile)?;
        Self::prepare(
            Action::Firmware(profile),
            candidate.bytes(),
            origin,
            source,
            fresh_backup,
            release,
            settings,
            generation,
            now,
        )
    }
    pub(crate) fn oem_rollback(
        dump: &[u8],
        source: &[u8],
        fresh_backup: &[u8],
        release: ReleaseSelection,
        generation: u64,
        now: u64,
    ) -> BackupResult<Self> {
        validate_native_revision(dump)?; // Supported revision only; NOT unit ownership.
        if dump.len() != source.len()
            || dump[..APP_START] != source[..APP_START]
            || dump[APP_END..] != source[APP_END..]
        {
            return Err("OEM protected regions differ from source".into());
        }
        Self::prepare(
            Action::OemRollback,
            &dump[APP_START..APP_END],
            InputOrigin::Local,
            source,
            fresh_backup,
            release,
            SettingsChoice::Defaults,
            generation,
            now,
        )
    }
    fn prepare(
        action: Action,
        payload: &[u8],
        origin: InputOrigin,
        source: &[u8],
        fresh_backup: &[u8],
        release: ReleaseSelection,
        settings: SettingsChoice,
        generation: u64,
        now: u64,
    ) -> BackupResult<Self> {
        if source.len() != 65536 || fresh_backup != source || payload.len() != APP_END - APP_START {
            return Err(
                "Missing complete source, matching fresh backup, or application payload".into(),
            );
        }
        if release.version.is_empty()
            || !is_hash(&release.metadata_sha256)
            || matches!(&settings, SettingsChoice::RestorePlan(pin) if !is_hash(pin))
        {
            return Err("Missing version, release metadata or settings-plan pin".into());
        }
        if release.prerelease && !release.prerelease_opt_in {
            return Err("Prerelease requires opt-in".into());
        }
        if release.revoked_images.contains(&sha256(payload)) {
            return Err("Selected image revoked".into());
        }
        if let MetadataFreshness::Known {
            checked_at,
            expires_at,
        } = release.freshness
        {
            if checked_at > now || expires_at < now {
                return Err("Stale release metadata".into());
            }
        }
        // No qualified source lock capability exists in this offline model.
        // Conservatively require the fresh physical hold for every project image.
        let confirmation = match action {
            Action::Firmware(_) => ConfirmationKind::FreshEscLeftCtrlHold,
            Action::OemRollback => ConfirmationKind::ExplicitStartFlashing,
        };
        let mut expected = source.to_vec();
        expected[APP_START..APP_END].copy_from_slice(payload);
        let content = format!("{SOURCE_REVISION}|{action:?}|{origin:?}|{release:?}|{}|{}|{}|{settings:?}|{confirmation:?}|{generation}|{now}",
            sha256(source), sha256(fresh_backup), sha256(&expected));
        Ok(Self {
            action,
            origin,
            release,
            source: source.to_vec(),
            expected,
            fresh_backup: fresh_backup.to_vec(),
            settings,
            confirmation,
            generation,
            prepared_at: now,
            confirmation_sha256: sha256(content.as_bytes()),
        })
    }
    pub(crate) fn action(&self) -> &Action {
        &self.action
    }
    pub(crate) fn origin(&self) -> InputOrigin {
        self.origin
    }
    pub(crate) fn confirmation_pin(&self) -> &str {
        &self.confirmation_sha256
    }
    pub(crate) fn source_pin(&self) -> String {
        sha256(&self.source)
    }
    pub(crate) fn backup_pin(&self) -> String {
        sha256(&self.fresh_backup)
    }
    pub(crate) fn write_window(&self) -> std::ops::Range<usize> {
        APP_START..APP_END
    }
    pub(crate) fn release(&self) -> &ReleaseSelection {
        &self.release
    }
}
fn is_hash(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stage {
    Prepared,
    InputsChecked,
    Owned,
    Confirmed,
    Loader,
    Erasing,
    Writing,
    Reconnecting,
    Verifying,
    RestoringSettings,
    Complete,
    Cancelled,
    Failed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FirmwareOutcome {
    NotAttempted,
    Unknown,
    SimulatedVerified,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SettingsOutcome {
    NotRequested,
    Pending,
    SimulatedVerified,
    Failed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fault {
    Timeout,
    Disconnect,
    Storage,
    BadReadback,
    OwnershipLost,
    InvalidOrder,
}
/// Fake observations only: never accepted by any transport or live executor.
#[derive(Clone, Debug)]
pub(crate) enum SimulationEvent {
    InputsRechecked {
        source: Vec<u8>,
        backup: Vec<u8>,
        expected: Vec<u8>,
        now: u64,
    },
    ExclusiveOwnerAndHelperPaused {
        source_pin: String,
    },
    Confirmation {
        plan_pin: String,
        generation: u64,
        began_at: u64,
        ended_at: u64,
        kind: ConfirmationKind,
    },
    LoaderObserved {
        source_pin: String,
    },
    BeginErase {
        plan_pin: String,
        now: u64,
    },
    EraseCompleted,
    FlashCompleted,
    Reconnected {
        source_pin: String,
    },
    BeginFullReadback,
    FullReadback(Vec<u8>),
    SettingsReadback {
        plan_pin: String,
    },
    Cancel,
    Fault(Fault),
}
/// A successful run means only that these simulated events obeyed the model.
/// No commands, retry, reset, fetch, live permission or automatic recovery exist.
pub(crate) struct InstallSimulation {
    prepared: PreparedInstall,
    stage: Stage,
    firmware: FirmwareOutcome,
    settings: SettingsOutcome,
    fault: Option<Fault>,
    erase_started: bool,
    last_time: u64,
}
impl InstallSimulation {
    pub(crate) fn new(prepared: PreparedInstall) -> Self {
        let settings = if matches!(prepared.settings, SettingsChoice::Defaults) {
            SettingsOutcome::NotRequested
        } else {
            SettingsOutcome::Pending
        };
        let last_time = prepared.prepared_at;
        Self {
            prepared,
            stage: Stage::Prepared,
            firmware: FirmwareOutcome::NotAttempted,
            settings,
            fault: None,
            erase_started: false,
            last_time,
        }
    }
    pub(crate) fn stage(&self) -> Stage {
        self.stage
    }
    pub(crate) fn outcomes(&self) -> (FirmwareOutcome, SettingsOutcome) {
        (self.firmware, self.settings)
    }
    pub(crate) fn fault(&self) -> Option<Fault> {
        self.fault
    }
    pub(crate) fn unmet_hardware_gates(&self) -> &[HardwareGate] {
        &UNMET_HARDWARE_GATES
    }
    pub(crate) fn step(&mut self, event: SimulationEvent) -> Result<Stage, Fault> {
        if matches!(
            self.stage,
            Stage::Complete | Stage::Cancelled | Stage::Failed
        ) {
            return Err(Fault::InvalidOrder); // Terminal result is immutable.
        }
        if let SimulationEvent::Fault(fault) = event {
            return self.stop(fault);
        }
        if matches!(event, SimulationEvent::Cancel) {
            if self.erase_started {
                return Err(Fault::InvalidOrder); // Refused cancellation cannot interrupt flashing.
            }
            self.stage = Stage::Cancelled;
            return Ok(self.stage);
        }
        let p = &self.prepared;
        let next = match (self.stage, event) {
            (
                Stage::Prepared,
                SimulationEvent::InputsRechecked {
                    source,
                    backup,
                    expected,
                    now,
                },
            ) if source == p.source
                && backup == p.fresh_backup
                && expected == p.expected
                && now >= self.last_time =>
            {
                self.last_time = now;
                Stage::InputsChecked
            }
            (
                Stage::InputsChecked,
                SimulationEvent::ExclusiveOwnerAndHelperPaused { source_pin },
            ) if source_pin == p.source_pin() => Stage::Owned,
            (
                Stage::Owned,
                SimulationEvent::Confirmation {
                    plan_pin,
                    generation,
                    began_at,
                    ended_at,
                    kind,
                },
            ) if plan_pin == p.confirmation_sha256
                && generation == p.generation
                && kind == p.confirmation
                && began_at > self.last_time
                && ended_at >= began_at
                && (kind == ConfirmationKind::ExplicitStartFlashing
                    || ended_at - began_at >= 3) =>
            {
                self.last_time = ended_at;
                Stage::Confirmed
            }
            (Stage::Confirmed, SimulationEvent::LoaderObserved { source_pin })
                if source_pin == p.source_pin() =>
            {
                Stage::Loader
            }
            (Stage::Loader, SimulationEvent::BeginErase { plan_pin, now })
                if plan_pin == p.confirmation_sha256
                    && now >= self.last_time
                    && self.metadata_current(now) =>
            {
                self.erase_started = true;
                self.last_time = now;
                self.firmware = FirmwareOutcome::Unknown;
                Stage::Erasing
            }
            (Stage::Erasing, SimulationEvent::EraseCompleted) => Stage::Writing,
            (Stage::Writing, SimulationEvent::FlashCompleted) => Stage::Reconnecting,
            (Stage::Reconnecting, SimulationEvent::Reconnected { source_pin })
                if source_pin == p.source_pin() =>
            {
                Stage::Verifying
            }
            (Stage::Verifying, SimulationEvent::BeginFullReadback) => return Ok(self.stage),
            (Stage::Verifying, SimulationEvent::FullReadback(rom)) if rom == p.expected => {
                self.firmware = FirmwareOutcome::SimulatedVerified;
                if self.settings == SettingsOutcome::Pending {
                    Stage::RestoringSettings
                } else {
                    Stage::Complete
                }
            }
            (Stage::Verifying, SimulationEvent::FullReadback(_)) => {
                return self.stop(Fault::BadReadback)
            }
            (Stage::RestoringSettings, SimulationEvent::SettingsReadback { plan_pin }) if matches!(&p.settings, SettingsChoice::RestorePlan(pin) if *pin == plan_pin) =>
            {
                self.settings = SettingsOutcome::SimulatedVerified;
                Stage::Complete
            }
            _ => return self.stop(Fault::InvalidOrder),
        };
        self.stage = next;
        Ok(next)
    }
    fn metadata_current(&self, now: u64) -> bool {
        // Offline unknown remains visible and blocks even simulated erase.
        matches!(self.prepared.release.freshness,
            MetadataFreshness::Known { checked_at, expires_at } if checked_at <= now && now <= expires_at)
    }
    fn stop(&mut self, fault: Fault) -> Result<Stage, Fault> {
        if self.settings == SettingsOutcome::Pending {
            self.settings = SettingsOutcome::Failed;
        }
        self.fault = Some(fault);
        self.stage = Stage::Failed;
        Err(fault)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn release() -> ReleaseSelection {
        ReleaseSelection {
            version: "fixture-only".into(),
            metadata_sha256: "a".repeat(64),
            freshness: MetadataFreshness::Known {
                checked_at: 1,
                expires_at: 100,
            },
            prerelease: false,
            prerelease_opt_in: false,
            revoked_images: vec![],
        }
    }
    fn fixture(settings: SettingsChoice) -> PreparedInstall {
        // Private model fixture bypasses packer identity, not public preparation.
        let source = vec![0; 65536];
        PreparedInstall::prepare(
            Action::Firmware(ProfileId::Work),
            &vec![1; APP_END - APP_START],
            InputOrigin::Bundled,
            &source,
            &source,
            release(),
            settings,
            7,
            10,
        )
        .unwrap()
    }
    fn trace(p: &PreparedInstall) -> Vec<SimulationEvent> {
        vec![
            SimulationEvent::InputsRechecked {
                source: p.source.clone(),
                backup: p.fresh_backup.clone(),
                expected: p.expected.clone(),
                now: 11,
            },
            SimulationEvent::ExclusiveOwnerAndHelperPaused {
                source_pin: p.source_pin(),
            },
            SimulationEvent::Confirmation {
                plan_pin: p.confirmation_pin().into(),
                generation: 7,
                began_at: 12,
                ended_at: 15,
                kind: p.confirmation,
            },
            SimulationEvent::LoaderObserved {
                source_pin: p.source_pin(),
            },
            SimulationEvent::BeginErase {
                plan_pin: p.confirmation_pin().into(),
                now: 16,
            },
            SimulationEvent::EraseCompleted,
            SimulationEvent::FlashCompleted,
            SimulationEvent::Reconnected {
                source_pin: p.source_pin(),
            },
            SimulationEvent::FullReadback(p.expected.clone()),
        ]
    }
    #[test]
    fn complete_trace_and_separate_settings_failure() {
        for settings in [
            SettingsChoice::Defaults,
            SettingsChoice::RestorePlan("b".repeat(64)),
        ] {
            let p = fixture(settings.clone());
            let events = trace(&p);
            let mut s = InstallSimulation::new(p);
            for e in events {
                s.step(e).unwrap();
            }
            assert_eq!(s.firmware, FirmwareOutcome::SimulatedVerified);
            assert_eq!(s.unmet_hardware_gates().len(), 9);
            if let SettingsChoice::RestorePlan(pin) = settings {
                assert_eq!(s.stage(), Stage::RestoringSettings);
                assert_eq!(
                    s.step(SimulationEvent::Fault(Fault::Storage)),
                    Err(Fault::Storage)
                );
                assert_eq!(
                    s.outcomes(),
                    (FirmwareOutcome::SimulatedVerified, SettingsOutcome::Failed)
                );
                assert!(s
                    .step(SimulationEvent::SettingsReadback { plan_pin: pin })
                    .is_err());
            } else {
                assert_eq!(s.stage(), Stage::Complete);
            }
        }
        let p = fixture(SettingsChoice::RestorePlan("b".repeat(64)));
        let events = trace(&p);
        let mut s = InstallSimulation::new(p);
        for e in events {
            s.step(e).unwrap();
        }
        assert_eq!(
            s.step(SimulationEvent::SettingsReadback {
                plan_pin: "b".repeat(64)
            }),
            Ok(Stage::Complete)
        );
    }
    #[test]
    fn faults_are_terminal_and_cancel_is_refused_after_erase() {
        let p = fixture(SettingsChoice::RestorePlan("b".repeat(64)));
        let events = trace(&p);
        for prefix in 0..=events.len() {
            for fault in [
                Fault::Timeout,
                Fault::Disconnect,
                Fault::Storage,
                Fault::BadReadback,
                Fault::OwnershipLost,
            ] {
                let mut s = InstallSimulation::new(p.clone());
                for e in events[..prefix].iter().cloned() {
                    s.step(e).unwrap();
                }
                assert_eq!(s.step(SimulationEvent::Fault(fault)), Err(fault));
                assert_eq!(
                    s.firmware,
                    if prefix < 5 {
                        FirmwareOutcome::NotAttempted
                    } else if prefix < events.len() {
                        FirmwareOutcome::Unknown
                    } else {
                        FirmwareOutcome::SimulatedVerified
                    }
                );
                let outcomes = s.outcomes();
                for e in events.iter().cloned() {
                    assert!(s.step(e).is_err());
                }
                assert_eq!(s.outcomes(), outcomes);
                assert_eq!(s.stage(), Stage::Failed);
            }
            let mut s = InstallSimulation::new(p.clone());
            for e in events[..prefix].iter().cloned() {
                s.step(e).unwrap();
            }
            if prefix < 5 {
                assert_eq!(s.step(SimulationEvent::Cancel), Ok(Stage::Cancelled));
                assert!(s.step(events[0].clone()).is_err());
            } else {
                let before = (s.stage(), s.outcomes(), s.fault(), s.last_time);
                assert_eq!(s.step(SimulationEvent::Cancel), Err(Fault::InvalidOrder));
                assert_eq!((s.stage(), s.outcomes(), s.fault(), s.last_time), before);
                for e in events[prefix..].iter().cloned() {
                    s.step(e).unwrap();
                }
                assert_eq!(s.firmware, FirmwareOutcome::SimulatedVerified);
                assert_eq!(
                    s.step(SimulationEvent::SettingsReadback {
                        plan_pin: "b".repeat(64),
                    }),
                    Ok(Stage::Complete)
                );
            }
        }
    }
    #[test]
    fn project_policy_rejects_button_even_with_valid_plan_and_generation() {
        let p = fixture(SettingsChoice::Defaults);
        assert_eq!(p.confirmation, ConfirmationKind::FreshEscLeftCtrlHold);
        let events = trace(&p);
        let mut s = InstallSimulation::new(p.clone());
        for e in events[..2].iter().cloned() {
            s.step(e).unwrap();
        }
        assert_eq!(
            s.step(SimulationEvent::Confirmation {
                plan_pin: p.confirmation_pin().into(),
                generation: 7,
                began_at: 12,
                ended_at: 15,
                kind: ConfirmationKind::ExplicitStartFlashing,
            }),
            Err(Fault::InvalidOrder)
        );
        assert!(!s.erase_started);
    }
    #[test]
    fn mismatched_rechecked_bytes_and_identity_pins_fail_closed() {
        let p = fixture(SettingsChoice::Defaults);
        let events = trace(&p);
        for field in 0..3 {
            let mut source = p.source.clone();
            let mut backup = p.fresh_backup.clone();
            let mut expected = p.expected.clone();
            match field {
                0 => source[0] ^= 1,
                1 => backup[0] ^= 1,
                _ => expected[APP_START] ^= 1,
            }
            let mut s = InstallSimulation::new(p.clone());
            assert_eq!(
                s.step(SimulationEvent::InputsRechecked {
                    source,
                    backup,
                    expected,
                    now: 11,
                }),
                Err(Fault::InvalidOrder)
            );
            assert_eq!(s.stage(), Stage::Failed);
            assert_eq!(s.firmware, FirmwareOutcome::NotAttempted);
            assert!(!s.erase_started);
        }
        for (prefix, wrong) in [
            (
                1,
                SimulationEvent::ExclusiveOwnerAndHelperPaused {
                    source_pin: "wrong".into(),
                },
            ),
            (
                3,
                SimulationEvent::LoaderObserved {
                    source_pin: "wrong".into(),
                },
            ),
            (
                7,
                SimulationEvent::Reconnected {
                    source_pin: "wrong".into(),
                },
            ),
        ] {
            let mut s = InstallSimulation::new(p.clone());
            for e in events[..prefix].iter().cloned() {
                s.step(e).unwrap();
            }
            assert_eq!(s.step(wrong), Err(Fault::InvalidOrder));
            assert_eq!(s.stage(), Stage::Failed);
            assert_eq!(
                s.firmware,
                if prefix < 5 {
                    FirmwareOutcome::NotAttempted
                } else {
                    FirmwareOutcome::Unknown
                }
            );
            assert!(s.step(events[prefix].clone()).is_err());
        }
    }
    #[test]
    fn bad_readback_and_stale_or_short_confirmation_stop() {
        let p = fixture(SettingsChoice::Defaults);
        let events = trace(&p);
        let mut s = InstallSimulation::new(p.clone());
        for e in events[..8].iter().cloned() {
            s.step(e).unwrap();
        }
        let mut bad = p.expected.clone();
        bad[0] ^= 1; // protected region also verified
        assert_eq!(
            s.step(SimulationEvent::FullReadback(bad)),
            Err(Fault::BadReadback)
        );
        assert_eq!(s.firmware, FirmwareOutcome::Unknown);
        for (generation, began_at, ended_at, pin) in [
            (6, 12, 15, p.confirmation_pin()),
            (7, 10, 15, p.confirmation_pin()),
            (7, 12, 14, p.confirmation_pin()),
            (7, 12, 15, "changed"),
        ] {
            let mut s = InstallSimulation::new(p.clone());
            for e in events[..2].iter().cloned() {
                s.step(e).unwrap();
            }
            assert!(s
                .step(SimulationEvent::Confirmation {
                    generation,
                    began_at,
                    ended_at,
                    plan_pin: pin.into(),
                    kind: p.confirmation
                })
                .is_err());
        }
    }
    #[test]
    fn missing_inputs_and_out_of_order_events_never_erase() {
        let source = vec![0; 65536];
        let payload = vec![1; APP_END - APP_START];
        for (src, backup, image) in [
            (&source[..0], &source[..], &payload[..]),
            (&source[..], &source[..0], &payload[..]),
            (&source[..], &source[..], &payload[..0]),
        ] {
            assert!(PreparedInstall::prepare(
                Action::Firmware(ProfileId::Work),
                image,
                InputOrigin::Local,
                src,
                backup,
                release(),
                SettingsChoice::Defaults,
                1,
                10
            )
            .is_err());
        }
        let p = fixture(SettingsChoice::Defaults);
        let events = trace(&p);
        for prefix in 0..events.len() {
            let mut s = InstallSimulation::new(p.clone());
            for e in events[..prefix].iter().cloned() {
                s.step(e).unwrap();
            }
            // A second write/erase and any skipped precondition fail closed.
            let wrong = if prefix == 6 {
                SimulationEvent::EraseCompleted
            } else {
                SimulationEvent::FlashCompleted
            };
            assert!(s.step(wrong).is_err());
        }
    }
    #[test]
    fn freshness_revocation_prerelease_and_unknown_identity() {
        let source = vec![0; 65536];
        let payload = vec![1; APP_END - APP_START];
        for mut r in [release(), release(), release()] {
            r.revoked_images.push(sha256(&payload));
            assert!(PreparedInstall::prepare(
                Action::Firmware(ProfileId::Work),
                &payload,
                InputOrigin::Local,
                &source,
                &source,
                r,
                SettingsChoice::Defaults,
                1,
                10
            )
            .is_err());
        }
        let mut p = fixture(SettingsChoice::Defaults);
        p.release.freshness = MetadataFreshness::OfflineUnknown;
        let events = trace(&p);
        let mut s = InstallSimulation::new(p);
        for e in events[..4].iter().cloned() {
            s.step(e).unwrap();
        }
        assert!(s.step(events[4].clone()).is_err());
        assert!(!s.erase_started);
        let mut p = fixture(SettingsChoice::Defaults);
        p.release.freshness = MetadataFreshness::Known {
            checked_at: 1,
            expires_at: 15,
        };
        let events = trace(&p);
        let mut s = InstallSimulation::new(p);
        for e in events[..4].iter().cloned() {
            s.step(e).unwrap();
        }
        assert!(s.step(events[4].clone()).is_err());
        assert!(!s.erase_started);
        let mut r = release();
        r.prerelease = true;
        assert!(PreparedInstall::prepare(
            Action::Firmware(ProfileId::Work),
            &payload,
            InputOrigin::Local,
            &source,
            &source,
            r,
            SettingsChoice::Defaults,
            1,
            10
        )
        .is_err());
        for id in [
            ProfileId::Work,
            ProfileId::Essential,
            ProfileId::Typing,
            ProfileId::Mouse,
            ProfileId::MouseTyping,
            ProfileId::System,
            ProfileId::TapDance,
            ProfileId::KeyOverrides,
            ProfileId::Settings,
        ] {
            // Absent catalog packed identities stay absent; no synthetic qualification.
            if matrix_profile(id).settings_identity.is_none() {
                assert!(validate_catalog_payload(&payload, id).is_err());
            }
        }
    }
    #[test]
    fn oem_rollback_never_accepts_synthetic_revision_or_project_settings() {
        let p = fixture(SettingsChoice::Defaults);
        assert!(PreparedInstall::oem_rollback(
            &p.source,
            &p.source,
            &p.fresh_backup,
            release(),
            1,
            10
        )
        .is_err());
        // Pure synthetic lifecycle only, not a validated OEM dump or owner proof.
        let p = PreparedInstall::prepare(
            Action::OemRollback,
            &p.expected[APP_START..APP_END],
            InputOrigin::Local,
            &p.source,
            &p.fresh_backup,
            release(),
            SettingsChoice::Defaults,
            7,
            10,
        )
        .unwrap();
        assert_eq!(p.confirmation, ConfirmationKind::ExplicitStartFlashing);
        let events = trace(&p);
        let mut s = InstallSimulation::new(p);
        for e in events {
            s.step(e).unwrap();
        }
        assert_eq!(
            s.outcomes(),
            (
                FirmwareOutcome::SimulatedVerified,
                SettingsOutcome::NotRequested
            )
        );
    }
}
