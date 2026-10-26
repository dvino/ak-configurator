//! Immutable restore input and local safety-backup publication for the HID worker.
use crate::ak820_backup_capture::CapturedBackup;
use crate::ak820_backup_files::{self as files, LoadedBackup};
use crate::ak820_restore_plan::RestorePlan;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub(super) struct BackupLocation {
    pub(super) path: PathBuf,
    pub(super) manifest_sha256: String,
}

#[derive(Clone)]
enum RestoreSource {
    Archive {
        data: LoadedBackup,
        location: BackupLocation,
    },
    Defaults(crate::app::ak820_beta1_profiles::ProfileId),
}
#[derive(Clone)]
pub(super) struct PreparedRestore {
    plan: Arc<RestorePlan>,
    source: RestoreSource,
    safety_path: PathBuf,
    saved: Arc<std::sync::Mutex<Option<BackupLocation>>>,
}

pub(super) struct RestoreCompleted {
    pub(super) snapshot: CapturedBackup,
    pub(super) safety: Option<BackupLocation>,
}

impl PreparedRestore {
    pub(super) fn new(
        plan: Arc<RestorePlan>,
        source: LoadedBackup,
        source_path: PathBuf,
        backup_root: &std::path::Path,
    ) -> Result<Self, String> {
        if plan.source() != &source.archive().source || plan.steps().is_empty() {
            return Err("Restore source differs or no changes are needed".into());
        }
        let name = format!(
            "ak820-before-restore-{}",
            chrono::Utc::now().format("%Y%m%dT%H%M%S%.9fZ")
        );
        let location = BackupLocation {
            path: source_path,
            manifest_sha256: source.archive().manifest_sha256.clone(),
        };
        Ok(Self {
            plan,
            source: RestoreSource::Archive {
                data: source,
                location,
            },
            safety_path: backup_root.join(name),
            saved: Default::default(),
        })
    }

    pub(super) fn factory_defaults(
        plan: Arc<RestorePlan>,
        profile: crate::app::ak820_beta1_profiles::ProfileId,
        backup_root: &std::path::Path,
    ) -> Result<Self, String> {
        let checked = crate::ak820_restore_plan::plan_factory_reset(
            profile,
            plan.target(),
            plan.before_nvm(),
        )?;
        if checked.expected_nvm() != plan.expected_nvm()
            || checked.steps() != plan.steps()
            || plan.steps().is_empty()
        {
            return Err("Factory reset plan differs or no changes are needed".into());
        }
        Ok(Self {
            plan,
            source: RestoreSource::Defaults(profile),
            safety_path: backup_root.join(format!(
                "ak820-before-reset-{}",
                chrono::Utc::now().format("%Y%m%dT%H%M%S%.9fZ")
            )),
            saved: Default::default(),
        })
    }

    pub(super) fn safety_location(&self) -> Option<BackupLocation> {
        self.saved.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn run(
        &self,
        hid: &crate::hid::HidDevice,
        cancel: &AtomicBool,
        progress: &AtomicU32,
    ) -> anyhow::Result<RestoreCompleted> {
        let mut safety = None;
        let snapshot = hid.ak820_restore_backup(
            &self.plan,
            cancel,
            progress,
            || {
                match &self.source {
                    RestoreSource::Archive { data, location } => {
                        let reopened = files::read_backup_auto(
                            &location.path,
                            Some(&location.manifest_sha256),
                        )
                        .map_err(anyhow::Error::msg)?;
                        anyhow::ensure!(
                            reopened.archive().source == data.archive().source
                                && reopened.files() == data.files(),
                            "Restore source changed since preview"
                        );
                    }
                    RestoreSource::Defaults(profile) => {
                        let checked = crate::ak820_restore_plan::plan_factory_reset(
                            *profile,
                            self.plan.target(),
                            self.plan.before_nvm(),
                        )
                        .map_err(anyhow::Error::msg)?;
                        anyhow::ensure!(
                            checked.expected_nvm() == self.plan.expected_nvm()
                                && checked.steps() == self.plan.steps(),
                            "Reset defaults changed since preview"
                        );
                    }
                }
                Ok(())
            },
            |captured| {
                let archive = files::publish_validated_archive(
                    &self.safety_path,
                    captured.profile(),
                    captured.files(),
                )
                .map_err(anyhow::Error::msg)?;
                let reopened =
                    files::read_backup_auto(&self.safety_path, Some(&archive.manifest_sha256))
                        .map_err(anyhow::Error::msg)?;
                anyhow::ensure!(
                    &reopened.archive().source == captured.identity()
                        && reopened.files() == captured.files(),
                    "Safety backup reread differs"
                );
                safety = Some(BackupLocation {
                    path: self.safety_path.clone(),
                    manifest_sha256: archive.manifest_sha256,
                });
                *self.saved.lock().unwrap_or_else(|e| e.into_inner()) = safety.clone();
                Ok(())
            },
        )?;
        Ok(RestoreCompleted { snapshot, safety })
    }
}
