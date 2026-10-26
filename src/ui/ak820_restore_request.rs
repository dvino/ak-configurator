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
pub(super) struct PreparedRestore {
    plan: Arc<RestorePlan>,
    source: LoadedBackup,
    location: BackupLocation,
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
            source,
            location,
            safety_path: backup_root.join(name),
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
                let reopened = files::read_backup_auto(
                    &self.location.path,
                    Some(&self.location.manifest_sha256),
                )
                .map_err(anyhow::Error::msg)?;
                anyhow::ensure!(
                    reopened.archive().source == self.source.archive().source
                        && reopened.files() == self.source.files(),
                    "Restore source changed since preview"
                );
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
