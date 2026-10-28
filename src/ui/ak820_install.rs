// SPDX-License-Identifier: GPL-3.0-or-later
//! User-driven AK820 installation flow. Device ownership is exclusive from
//! preparation through readback; this module never retries or resets a device.
use super::*;
use crate::ak820_backup_capture::CapturedBackup;
use crate::ak820_install_executor::{InstallRequest, InstallSettingsChoice, PreparedInstallation};
use crate::ak820_settings_backup::sha256;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc;

pub(crate) const OPEN_ID: &str = "ak820_install_open";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Firmware,
    OemRollback,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Source {
    Bundled,
    PreparedFile,
}

struct Preview {
    hid: crate::hid::HidDevice,
    plan: PreparedInstallation,
    capture: Option<CapturedBackup>,
    backup_root: PathBuf,
    provenance: Option<crate::ak820_install_executor::RecoveryProvenance>,
}

enum Owner {
    Idle,
    Preparing {
        rx: mpsc::Receiver<Result<Preview, String>>,
        cancel: std::sync::Arc<AtomicBool>,
        progress: std::sync::Arc<AtomicU32>,
    },
    Prepared(Preview),
    Executing {
        rx: mpsc::Receiver<ExecutionReport>,
        cancel: std::sync::Arc<AtomicBool>,
        progress: std::sync::Arc<AtomicU32>,
        safety: std::sync::Arc<std::sync::Mutex<SafetyPaths>>,
    },
    Finished {
        text: String,
        success: bool,
    },
}

#[derive(Clone, Default)]
struct SafetyPaths {
    settings_archive: Option<PathBuf>,
    rom_snapshot: Option<PathBuf>,
    recovery_manifest_pin: Option<String>,
}
impl SafetyPaths {
    fn user_text(&self, ru: bool) -> String {
        let settings = self
            .settings_archive
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| tr(ru, "none", "нет").into());
        let rom = self
            .rom_snapshot
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| tr(ru, "none", "нет").into());
        format!(
            "{}: {settings}; {}: {rom}",
            tr(ru, "Settings archive", "Архив настроек"),
            tr(
                ru,
                "Full-ROM safety snapshot",
                "Защитный снимок полного ROM"
            )
        )
    }
}
struct ExecutionReport {
    result: Result<crate::ak820_install_executor::InstallationResult, String>,
    safety: SafetyPaths,
}

#[derive(Clone)]
struct SelectedFile {
    name: String,
    bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pick {
    Payload,
    Recovery,
    BackupRoot,
    OemRecovery,
}

struct Dialog {
    rx: mpsc::Receiver<Option<PathBuf>>,
    pick: Pick,
}

pub(crate) struct Runtime {
    owner: Owner,
    mode: Mode,
    source: Source,
    profile: crate::app::ak820_beta1_profiles::ProfileId,
    beta4_1: bool,
    prerelease_opt_in: bool,
    start_with_defaults: bool,
    external_payload: Option<SelectedFile>,
    recovery: Option<SelectedFile>,
    rollback_recovery: Option<(
        Vec<u8>,
        crate::ak820_install_executor::RecoveryProvenance,
        String,
    )>,
    candidates: Vec<crate::device::Device>,
    candidate_index: Option<usize>,
    backup_root: Option<PathBuf>,
    dialog: Option<Dialog>,
    release_check: Option<mpsc::Receiver<Result<crate::ak820_release::ReleaseObservation, String>>>,
    release_check_text: Option<String>,
    candidate_scan: Option<mpsc::Receiver<Result<Vec<crate::device::Device>, String>>>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self {
            owner: Owner::Idle,
            mode: Mode::Firmware,
            source: Source::Bundled,
            profile: crate::app::ak820_beta1_profiles::ProfileId::Main,
            beta4_1: true,
            prerelease_opt_in: false,
            start_with_defaults: false,
            external_payload: None,
            recovery: None,
            rollback_recovery: None,
            candidates: Vec::new(),
            candidate_index: None,
            backup_root: None,
            dialog: None,
            release_check: None,
            release_check_text: None,
            candidate_scan: None,
        }
    }
}

impl Runtime {
    pub(super) fn owns_hid(&self) -> bool {
        matches!(
            &self.owner,
            Owner::Preparing { .. } | Owner::Prepared(_) | Owner::Executing { .. }
        )
    }
    pub(super) fn blocks_exit(&self) -> bool {
        self.owns_hid()
    }
}

fn tr<'a>(ru: bool, en: &'a str, russian: &'a str) -> &'a str {
    if ru { russian } else { en }
}

fn firmware_outcome_text(
    outcome: &crate::ak820_install_executor::InstallFirmwareOutcome,
    ru: bool,
) -> &'static str {
    use crate::ak820_install_executor::InstallFirmwareOutcome::*;
    match outcome {
        NotAttempted => tr(ru, "not attempted", "не запускалась"),
        Unknown => tr(ru, "unknown", "неизвестен"),
        Verified => tr(ru, "verified", "проверена"),
    }
}

fn settings_outcome_text(
    outcome: &crate::ak820_install_executor::InstallSettingsOutcome,
    ru: bool,
) -> &'static str {
    use crate::ak820_install_executor::InstallSettingsOutcome::*;
    match outcome {
        DefaultsRequested => tr(ru, "defaults requested", "запрошены настройки по умолчанию"),
        PreparedImageSettingsVerified => tr(
            ru,
            "prepared-image settings verified",
            "настройки подготовленного образа проверены",
        ),
        OemSettings => tr(ru, "OEM settings", "настройки OEM"),
        Unverified => tr(ru, "unverified", "не проверены"),
    }
}

fn terminal_report(
    result: &Result<crate::ak820_install_executor::InstallationResult, String>,
    safety: &SafetyPaths,
    ru: bool,
) -> (String, bool) {
    let safety_text = safety.user_text(ru);
    match result {
        Ok(result)
            if result.error.is_none()
                && result.firmware
                    == crate::ak820_install_executor::InstallFirmwareOutcome::Verified =>
        {
            (
                format!(
                    "{}. {}: {}. {}: {}",
                    tr(ru, "Firmware verified", "Прошивка проверена"),
                    tr(ru, "Settings", "Настройки"),
                    settings_outcome_text(&result.settings, ru),
                    tr(ru, "Saved safety data", "Сохранённые защитные данные"),
                    safety_text
                ),
                true,
            )
        }
        Ok(result) => {
            let explanation = tr(ru, "Installation stopped", "Установка остановлена");
            let technical = result.error.as_deref().unwrap_or("unknown error");
            (
                format!(
                    "{explanation}: {technical}. {}: {}; {}: {}. {}. {}",
                    tr(ru, "Firmware outcome", "Результат прошивки"),
                    firmware_outcome_text(&result.firmware, ru),
                    tr(ru, "Settings outcome", "Результат настроек"),
                    settings_outcome_text(&result.settings, ru),
                    safety_text,
                    tr(
                        ru,
                        "No retry or automatic recovery was attempted.",
                        "Повторная попытка и автоматическое восстановление не выполнялись."
                    )
                ),
                false,
            )
        }
        Err(error) => (
            format!(
                "{}. {error}. {}. {}",
                tr(
                    ru,
                    "Installation worker failed",
                    "Сбой рабочего процесса установки"
                ),
                safety_text,
                tr(
                    ru,
                    "Do not retry blindly; reconnect and verify the full image.",
                    "Не повторяйте операцию вслепую; переподключите устройство и проверьте полный образ."
                )
            ),
            false,
        ),
    }
}

fn profile_nvm_identity(
    profile: crate::app::ak820_beta1_profiles::ProfileId,
) -> Result<crate::ak820_settings_backup::ImageIdentity, String> {
    let exact = crate::app::ak820_beta1_profiles::matrix_profile(profile)
        .settings_identity
        .as_ref()
        .ok_or("Selected profile has no exact settings identity")?;
    Ok(crate::ak820_settings_backup::ImageIdentity {
        ihx_sha256: exact.ihx_sha256.clone(),
        code_sha256: exact.code_sha256.clone(),
        definition_sha256: exact.definition_sha256.clone(),
        schema: exact.nvm_schema,
    })
}

fn build_preserved_nvm(
    capture: &CapturedBackup,
    target: crate::app::ak820_beta1_profiles::ProfileId,
) -> Result<Vec<u8>, String> {
    if capture.identity() == &profile_nvm_identity(target)? {
        return Ok(capture.nvm().to_vec());
    }
    let target_identity = profile_nvm_identity(target)?;
    let target_defaults = crate::ak820_release::bundled_defaults(target)?;
    let archive = crate::ak820_backup_files::validate_captured(capture)?;
    let plan = crate::ak820_restore_plan::plan_restore(
        &archive,
        capture.nvm(),
        target,
        &target_identity,
        &target_defaults,
    )?;
    let nvm = plan.expected_nvm().to_vec();
    Ok(nvm)
}

fn validate_payload_nvm(payload: &[u8], expected_nvm: &[u8]) -> Result<String, String> {
    let start = 0xc700 - crate::ak820_image_pack::APP_START;
    let end = 0xef00 - crate::ak820_image_pack::APP_START;
    if payload.len() != 51136 || expected_nvm.len() != end - start {
        return Err("Prepared firmware or expected settings has an invalid exact size".into());
    }
    if payload[start..end] != *expected_nvm {
        return Err(
            "Prepared firmware settings do not match the captured or selected default settings"
                .into(),
        );
    }
    Ok(sha256(expected_nvm))
}

/// A fresh OEM/install-only owner has no project settings to export. Once a
/// verified project capture is attempted, every fault stops preparation even
/// when defaults were selected; defaults never clear a failed session.
fn capture_for_preparation<T>(
    project_session: bool,
    require_saved_settings: bool,
    capture: impl FnOnce() -> Result<T, String>,
) -> Result<Option<T>, String> {
    if project_session {
        return capture().map(Some);
    }
    if require_saved_settings {
        return Err("Read complete project settings before preserving them, or explicitly select default settings".into());
    }
    Ok(None)
}

fn start_prepare(app: &mut EntropyApp, external: Option<SelectedFile>) {
    if app.ak820_install_runtime.owns_hid() || app.hid_write_lifecycle_busy() || app.headless {
        return;
    }
    let device = app
        .ak820_install_runtime
        .candidate_index
        .and_then(|index| app.ak820_install_runtime.candidates.get(index).cloned())
        .or_else(|| {
            app.selected_device
                .and_then(|index| app.device_manager.devices().get(index).cloned())
        });
    let Some(device) = device else {
        let ru = app.app_settings.language == crate::i18n::Language::Russian;
        app.status_msg = tr(
            ru,
            "Refresh candidates and select the AK820 device first",
            "Обновите список устройств и выберите AK820",
        )
        .into();
        return;
    };
    if !device.is_ak820() {
        app.status_msg = tr(
            app.app_settings.language == crate::i18n::Language::Russian,
            "Installation requires an AK820 device",
            "Для установки требуется устройство AK820",
        )
        .into();
        return;
    }
    let mode = app.ak820_install_runtime.mode;
    let recovery = app.ak820_install_runtime.recovery.clone();
    let rollback_recovery = app.ak820_install_runtime.rollback_recovery.clone();
    if mode == Mode::Firmware
        && app.ak820_install_runtime.source == Source::PreparedFile
        && recovery.is_none()
    {
        app.status_msg = tr(
            app.app_settings.language == crate::i18n::Language::Russian,
            "Choose the personal OEM recovery ROM first",
            "Сначала выберите личную OEM-копию ROM для восстановления",
        )
        .into();
        return;
    }
    if mode == Mode::OemRollback && rollback_recovery.is_none() {
        app.status_msg = tr(
            app.app_settings.language == crate::i18n::Language::Russian,
            "Choose a previously recorded OEM recovery folder",
            "Выберите ранее сохранённую папку восстановления OEM",
        )
        .into();
        return;
    }
    let root = match app.ak820_install_runtime.backup_root.clone() {
        Some(x) if x.is_dir() => x,
        _ => {
            app.status_msg = tr(
                app.app_settings.language == crate::i18n::Language::Russian,
                "Choose an existing backup folder first",
                "Сначала выберите существующую папку резервных копий",
            )
            .into();
            return;
        }
    };
    let profile = app.ak820_install_runtime.profile;
    let beta4_1 = app.ak820_install_runtime.beta4_1;
    let image = if beta4_1 {
        crate::ak820::ImageContract::Beta4(profile)
    } else {
        crate::ak820::ImageContract::Beta3(profile)
    };
    let defaults = app.ak820_install_runtime.start_with_defaults || beta4_1;
    let source = app.ak820_install_runtime.source;
    let opt_in = app.ak820_install_runtime.prerelease_opt_in;
    let observation = app.app_settings.ak820_release_observation.clone();
    if app.hid_device.is_none() && !matches!(app.connect_state, ConnectState::Idle) {
        app.status_msg = tr(
            app.app_settings.language == crate::i18n::Language::Russian,
            "Wait for current device activity to finish",
            "Дождитесь завершения текущей операции с устройством",
        )
        .into();
        return;
    }
    let selected_device_matches = app
        .selected_device
        .and_then(|selected| app.device_manager.devices().get(selected))
        .map(|selected| {
            selected.path == device.path && selected.instance_token == device.instance_token
        })
        .unwrap_or(false);
    let hid = if selected_device_matches {
        app.hid_device.take()
    } else {
        // The existing editor session may belong to another AK820 endpoint.
        // Drop it before opening the explicitly selected installation owner.
        app.hid_device.take();
        app.shared_hid_output = None;
        None
    };
    app.ak820_rgb_scene_stop();
    let cancel = std::sync::Arc::new(AtomicBool::new(false));
    let progress = std::sync::Arc::new(AtomicU32::new(0));
    let worker_cancel = cancel.clone();
    let worker_progress = progress.clone();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = (|| -> Result<Preview, String> {
            let hid = match hid {
                Some(h) => h,
                None => crate::hid::HidDevice::open_for_install(&device)
                    .map_err(|e| format!("Could not open installation owner: {e:#}"))?,
            };
            let capture = capture_for_preparation(
                hid.ak820_image()
                    .is_some_and(|image| image.profile().is_some()),
                mode == Mode::Firmware && !defaults,
                || {
                    let capture = hid.ak820_capture_backup(&worker_cancel, &worker_progress)
                        .map_err(|e| format!("Settings capture failed; read and verify the device again before preparing installation: {e}"))?;
                    crate::ak820_backup_files::validate_captured(&capture)?;
                    Ok(capture)
                },
            )?;
            if worker_cancel.load(Ordering::Acquire) {
                return Err("Preparation cancelled before device read".into());
            }
            let request = if mode == Mode::OemRollback {
                let (rom, provenance, _) =
                    rollback_recovery.ok_or("Choose a previously recorded OEM recovery folder")?;
                InstallRequest::OemRollback {
                    recovery: rom,
                    provenance,
                }
            } else {
                match source {
                    Source::PreparedFile => {
                        let recovery = recovery.as_ref().ok_or(
                            "Choose the personal OEM recovery ROM required for prepared payloads",
                        )?;
                        let payload = external
                            .as_ref()
                            .ok_or("Choose the prepared firmware file")?
                            .bytes
                            .clone();
                        let expected = if defaults {
                            crate::ak820_release::bundled_defaults_for_image(image)?
                        } else {
                            build_preserved_nvm(capture.as_ref().ok_or("Complete saved settings are required to preserve/migrate payload settings")?, profile)?
                        };
                        let digest = validate_payload_nvm(&payload, &expected)?;
                        InstallRequest::Firmware {
                            profile,
                            image,
                            payload,
                            recovery: recovery.bytes.clone(),
                            settings: if defaults {
                                InstallSettingsChoice::Defaults
                            } else {
                                InstallSettingsChoice::PreservedInPayload { nvm_sha256: digest }
                            },
                        }
                    }
                    Source::Bundled => {
                        if let Some(capture) = capture.as_ref() {
                            let source_rom = capture
                                .files()
                                .get("before-full-rom.bin")
                                .ok_or("Captured full ROM is missing")?;
                            if crate::ak820_installer::classify_snapshot(source_rom)?
                                == crate::ak820_installer::SnapshotKind::Oem
                                && !defaults
                            {
                                return Err("Initial OEM installation requires the explicit Start with defaults choice".into());
                            }
                            if recovery.is_none()
                                && crate::ak820_installer::classify_snapshot(source_rom)?
                                    != crate::ak820_installer::SnapshotKind::Oem
                            {
                                return Err(
                                    "Project updates require your personal OEM recovery ROM".into(),
                                );
                            }
                        }
                        let settings = if defaults {
                            InstallSettingsChoice::Defaults
                        } else {
                            let capture = capture
                                .as_ref()
                                .ok_or("No complete settings capture is available")?;
                            let nvm = build_preserved_nvm(capture, profile)?;
                            InstallSettingsChoice::PreservedInPayload {
                                nvm_sha256: sha256(&nvm),
                            }
                        };
                        InstallRequest::BundledFirmware {
                            profile,
                            image,
                            recovery: recovery.as_ref().map(|recovery| recovery.bytes.clone()),
                            settings,
                        }
                    }
                }
            };
            let admission = if mode == Mode::Firmware {
                let admission = if beta4_1 {
                    crate::ak820_release::ReleaseAdmission::beta4_1(profile, opt_in)?
                } else {
                    crate::ak820_release::ReleaseAdmission::bundled(profile, opt_in)?
                        .with_observation(&observation)?
                };
                Some(admission)
            } else {
                None
            };
            let plan = hid
                .ak820_prepare_install(request, admission.as_ref(), &worker_cancel)
                .map_err(|e| format!("Installation preview rejected: {e:#}"))?;
            if let Some(capture) = &capture {
                let source = capture
                    .files()
                    .get("before-full-rom.bin")
                    .ok_or("Captured full ROM is missing")?;
                if sha256(source) != plan.summary().source_sha256 {
                    return Err(
                        "Full-ROM digest changed between settings capture and installation preview"
                            .into(),
                    );
                }
            }
            let provenance = plan.recovery_provenance().cloned();
            Ok(Preview {
                hid,
                plan,
                capture,
                backup_root: root,
                provenance,
            })
        })();
        let _ = tx.send(result);
    });
    app.ak820_install_runtime.owner = Owner::Preparing {
        rx,
        cancel,
        progress,
    };
    app.status_msg.clear();
}

impl EntropyApp {
    pub(super) fn poll_ak820_install(&mut self, ctx: &egui::Context) {
        let ru = self.app_settings.language == crate::i18n::Language::Russian;
        if let Some(rx) = self.ak820_install_runtime.candidate_scan.take() {
            match rx.try_recv() {
                Ok(Ok(candidates)) => {
                    self.ak820_install_runtime.candidate_index =
                        if candidates.len() == 1 { Some(0) } else { None };
                    self.ak820_install_runtime.candidates = candidates;
                    self.status_msg = if self.ak820_install_runtime.candidates.is_empty() {
                        tr(
                            ru,
                            "No supported USB installation endpoint found",
                            "Поддерживаемое USB-устройство для установки не найдено",
                        )
                        .into()
                    } else {
                        tr(
                            ru,
                            "Select the exact installation endpoint",
                            "Выберите точное устройство для установки",
                        )
                        .into()
                    };
                }
                Ok(Err(error)) => {
                    self.status_msg = format!(
                        "{}: {error}",
                        tr(
                            ru,
                            "Could not enumerate installation candidates",
                            "Не удалось найти устройства для установки"
                        )
                    )
                }
                Err(mpsc::TryRecvError::Empty) => {
                    self.ak820_install_runtime.candidate_scan = Some(rx);
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.status_msg = tr(
                        ru,
                        "Installation candidate scan stopped unexpectedly",
                        "Поиск устройств для установки неожиданно остановился",
                    )
                    .into();
                }
            }
        }
        let completed = match &self.ak820_install_runtime.owner {
            Owner::Preparing { rx, .. } => match rx.try_recv() {
                Ok(result) => Some(Completion::Prepared(result)),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => Some(Completion::Prepared(Err(
                    "Installation preparation worker stopped unexpectedly".into(),
                ))),
            },
            Owner::Executing { rx, safety, .. } => match rx.try_recv() {
                Ok(mut report) => {
                    report.safety = safety.lock().unwrap_or_else(|e| e.into_inner()).clone();
                    Some(Completion::Executed(report))
                }
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => Some(Completion::Lost(
                    safety.lock().unwrap_or_else(|e| e.into_inner()).clone(),
                )),
            },
            _ => None,
        };
        if let Some(completed) = completed {
            match completed {
                Completion::Prepared(Ok(preview)) => {
                    self.ak820_install_runtime.owner = Owner::Prepared(preview)
                }
                Completion::Prepared(Err(error)) => self.finish_install(
                    format!(
                        "{}: {error}",
                        tr(ru, "Preparation failed", "Не удалось подготовить установку")
                    ),
                    false,
                ),
                Completion::Executed(report) => {
                    self.persist_install_receipts(&report.safety);
                    let result = report.result;
                    let (text, success) = terminal_report(&result, &report.safety, ru);
                    self.finish_install(text, success);
                }
                Completion::Lost(safety) => {
                    self.persist_install_receipts(&safety);
                    self.finish_install(format!("{}. {}. {}", tr(ru, "Installation worker ended without a result", "Рабочий процесс установки завершился без результата"), safety.user_text(ru), tr(ru, "Reconnect and verify; do not retry blindly.", "Переподключите устройство и проверьте его; не повторяйте операцию вслепую.")), false);
                }
            }
            ctx.request_repaint();
        }
        if let Owner::Executing { safety, .. } = &self.ak820_install_runtime.owner {
            let current = safety.lock().unwrap_or_else(|e| e.into_inner()).clone();
            self.persist_install_receipts(&current);
        }
        if let Some(dialog) = &self.ak820_install_runtime.dialog {
            match dialog.rx.try_recv() {
                Ok(Some(path)) => {
                    let pick = dialog.pick;
                    self.ak820_install_runtime.dialog = None;
                    match pick {
                        Pick::BackupRoot => self.ak820_install_runtime.backup_root = Some(path),
                        Pick::OemRecovery => {
                            match crate::ak820_backup_files::read_install_recovery(
                                &path,
                                &self.app_settings.ak820_recovery_manifests,
                            ) {
                                Ok((rom, provenance)) => {
                                    let label = path
                                        .file_name()
                                        .unwrap_or_default()
                                        .to_string_lossy()
                                        .to_string();
                                    self.ak820_install_runtime.rollback_recovery =
                                        Some((rom, provenance, label));
                                    self.retire_install_preview();
                                }
                                Err(error) => {
                                    self.status_msg = format!(
                                        "{}: {error}",
                                        tr(
                                            ru,
                                            "Could not load the selected OEM recovery snapshot",
                                            "Не удалось загрузить выбранный снимок восстановления OEM"
                                        )
                                    )
                                }
                            }
                        }
                        Pick::Payload | Pick::Recovery => {
                            let expected = if pick == Pick::Payload { 51136 } else { 65536 };
                            match super::ak820_install_files::read_input_bytes(
                                &path,
                                expected,
                                self.app_settings.language == crate::i18n::Language::Russian,
                            ) {
                                Ok(bytes) => {
                                    let input = SelectedFile {
                                        name: path
                                            .file_name()
                                            .unwrap_or_default()
                                            .to_string_lossy()
                                            .chars()
                                            .filter(|c| !c.is_control())
                                            .take(72)
                                            .collect(),
                                        bytes,
                                    };
                                    if pick == Pick::Payload {
                                        self.ak820_install_runtime.external_payload = Some(input);
                                        self.ak820_install_runtime.source = Source::PreparedFile;
                                    } else {
                                        self.ak820_install_runtime.recovery = Some(input);
                                    }
                                    self.retire_install_preview();
                                }
                                Err(error) => {
                                    self.status_msg = format!(
                                        "{}: {error}",
                                        tr(
                                            ru,
                                            "Could not read the selected file",
                                            "Не удалось прочитать выбранный файл"
                                        )
                                    )
                                }
                            }
                        }
                    }
                }
                Ok(None) => self.ak820_install_runtime.dialog = None,
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(80))
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.ak820_install_runtime.dialog = None;
                    self.status_msg = tr(
                        ru,
                        "File selection stopped unexpectedly",
                        "Выбор файла неожиданно остановился",
                    )
                    .into();
                }
            }
        }
        if let Some(rx) = self.ak820_install_runtime.release_check.take() {
            match rx.try_recv() {
                Ok(Ok(observation)) => {
                    self.app_settings
                        .ak820_release_observation
                        .apply(observation);
                    self.ak820_install_runtime.release_check_text = Some(
                        if self
                            .app_settings
                            .ak820_release_observation
                            .blocks_current_release()
                        {
                            tr(
                                ru,
                                "Bundled release is blocked",
                                "Встроенный выпуск заблокирован",
                            )
                        } else {
                            tr(ru, "Release check completed", "Проверка выпуска завершена")
                        }
                        .into(),
                    );
                    save_app_settings(&self.app_settings);
                    self.ak820_install_runtime.release_check = None;
                }
                Ok(Err(error)) => {
                    self.ak820_install_runtime.release_check_text = Some(format!(
                        "{}: {error}",
                        tr(ru, "Release check failed", "Не удалось проверить выпуск")
                    ));
                    self.ak820_install_runtime.release_check = None;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    self.ak820_install_runtime.release_check = Some(rx);
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.ak820_install_runtime.release_check_text = Some(
                        tr(
                            ru,
                            "Release check stopped unexpectedly",
                            "Проверка выпуска неожиданно остановилась",
                        )
                        .into(),
                    );
                    self.ak820_install_runtime.release_check = None;
                }
            }
        }
        if self
            .app_settings
            .ak820_release_observation
            .blocks_current_release()
            && matches!(&self.ak820_install_runtime.owner, Owner::Prepared(preview) if preview.plan.summary().action.starts_with("firmware:"))
        {
            self.retire_install_preview();
            self.status_msg = tr(
                ru,
                "Bundled firmware preview retired because the latest release check reports a block",
                "План встроенной прошивки закрыт: последняя проверка выпуска обнаружила блокировку",
            )
            .into();
        }
    }

    fn finish_install(&mut self, text: String, success: bool) {
        // Any installation result invalidates the old session, even when the
        // executor stopped early: its identity and staged editor state are stale.
        self.hid_device = None;
        self.shared_hid_output = None;
        self.connection_generation = self.connection_generation.wrapping_add(1);
        self.selected_device = None;
        self.keycode_picker.ak820_profile = false;
        self.layout = None;
        self.deferred_device_load = Default::default();
        self.ak820_install_runtime.owner = Owner::Finished { text, success };
        self.start_device_scan();
    }

    fn persist_install_receipts(&mut self, receipts: &SafetyPaths) {
        if let Some(pin) = &receipts.recovery_manifest_pin {
            if self
                .app_settings
                .ak820_recovery_manifests
                .insert(pin.clone())
            {
                save_app_settings(&self.app_settings);
            }
        }
    }

    fn retire_install_preview(&mut self) {
        if matches!(self.ak820_install_runtime.owner, Owner::Prepared(_)) {
            self.ak820_install_runtime.owner = Owner::Idle;
            self.connection_generation = self.connection_generation.wrapping_add(1);
            self.selected_device = None;
            self.start_device_scan();
        }
    }
}

enum Completion {
    Prepared(Result<Preview, String>),
    Executed(ExecutionReport),
    Lost(SafetyPaths),
}

fn launch_execute(app: &mut EntropyApp, confirmed_pin: String) {
    if app.ak820_install_runtime.release_check.is_some() {
        app.status_msg = tr(
            app.app_settings.language == crate::i18n::Language::Russian,
            "Wait for the release status check to finish before starting",
            "Перед запуском дождитесь завершения проверки выпуска",
        )
        .into();
        return;
    }
    let firmware_action = matches!(&app.ak820_install_runtime.owner, Owner::Prepared(preview) if preview.plan.summary().action.starts_with("firmware:"));
    let admission = if firmware_action {
        let profile = app.ak820_install_runtime.profile;
        let observation = &app.app_settings.ak820_release_observation;
        let admission = if app.ak820_install_runtime.beta4_1 {
            crate::ak820_release::ReleaseAdmission::beta4_1(
                profile,
                app.ak820_install_runtime.prerelease_opt_in,
            )
        } else {
            crate::ak820_release::ReleaseAdmission::bundled(
                profile,
                app.ak820_install_runtime.prerelease_opt_in,
            )
            .and_then(|admission| admission.with_observation(observation))
        };
        match admission
        {
            Ok(admission) => Some(admission),
            Err(error) => {
                app.retire_install_preview();
                app.status_msg = format!(
                    "{}: {error}",
                    tr(
                        app.app_settings.language == crate::i18n::Language::Russian,
                        "Release admission failed",
                        "Проверка допуска выпуска не пройдена"
                    )
                );
                return;
            }
        }
    } else {
        None
    };
    let owner = std::mem::replace(&mut app.ak820_install_runtime.owner, Owner::Idle);
    let Owner::Prepared(preview) = owner else {
        return;
    };
    if confirmed_pin != preview.plan.confirmation_pin() {
        app.ak820_install_runtime.owner = Owner::Prepared(preview);
        return;
    }
    let cancel = std::sync::Arc::new(AtomicBool::new(false));
    let progress = std::sync::Arc::new(AtomicU32::new(0));
    let worker_cancel = cancel.clone();
    let worker_progress = progress.clone();
    let safety = std::sync::Arc::new(std::sync::Mutex::new(SafetyPaths::default()));
    let worker_safety = safety.clone();
    let provenance = preview.provenance.clone();
    let settings_path = app_settings_path();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%.9fZ");
        let snapshot_path = preview
            .backup_root
            .join(format!("ak820-install-before-{stamp}"));
        let archive_path = preview
            .backup_root
            .join(format!("ak820-settings-before-install-{stamp}"));
        let capture = preview.capture;
        let result = preview
            .hid
            .ak820_execute_install(
                &preview.plan,
                admission.as_ref(),
                &confirmed_pin,
                &worker_cancel,
                &worker_progress,
                |rom| {
                    if let Some(capture) = &capture {
                        if capture
                            .files()
                            .get("before-full-rom.bin")
                            .map(Vec::as_slice)
                            != Some(rom)
                        {
                            return Err(
                                "Fresh install source differs from captured settings backup".into(),
                            );
                        }
                        crate::ak820_backup_files::publish_validated_archive(
                            &archive_path,
                            capture.profile(),
                            capture.files(),
                        )?;
                        worker_safety
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .settings_archive = Some(archive_path.clone());
                    }
                    let receipt = crate::ak820_backup_files::publish_install_snapshot(
                        &snapshot_path,
                        rom,
                        provenance.as_ref(),
                    )?;
                    worker_safety
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .rom_snapshot = Some(snapshot_path.clone());
                    if provenance.is_some() {
                        super::app_storage::record_ak820_recovery_manifest(
                            &settings_path,
                            &receipt.manifest_sha256,
                        )?;
                        worker_safety
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .recovery_manifest_pin = Some(receipt.manifest_sha256.clone());
                    }
                    Ok(())
                },
            )
            .map_err(|e| format!("{e:#}"));
        let _ = tx.send(ExecutionReport {
            result,
            safety: worker_safety
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone(),
        });
    });
    app.ak820_install_runtime.owner = Owner::Executing {
        rx,
        cancel,
        progress,
        safety,
    };
}

fn start_release_check(runtime: &mut Runtime, ru: bool) {
    if runtime.release_check.is_some() {
        return;
    }
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = crate::ak820_release::check_bundled_release();
        let _ = tx.send(result);
    });
    runtime.release_check = Some(rx);
    runtime.release_check_text = Some(
        tr(
            ru,
            "Checking release metadata…",
            "Проверка метаданных выпуска…",
        )
        .into(),
    );
}

fn pick(runtime: &mut Runtime, kind: Pick) {
    if runtime.dialog.is_some() {
        return;
    }
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let path = if matches!(kind, Pick::BackupRoot | Pick::OemRecovery) {
            rfd::FileDialog::new().pick_folder()
        } else {
            rfd::FileDialog::new().pick_file()
        };
        let _ = tx.send(path);
    });
    runtime.dialog = Some(Dialog { rx, pick: kind });
}

fn draw(app: &mut EntropyApp, ui: &mut egui::Ui) {
    let ru = app.app_settings.language == crate::i18n::Language::Russian;
    let selected = app
        .ak820_install_runtime
        .candidate_index
        .and_then(|i| app.ak820_install_runtime.candidates.get(i))
        .or_else(|| {
            app.selected_device
                .and_then(|i| app.device_manager.devices().get(i))
        })
        .map(|d| format!("{} · {}", d.name, d.serial_number));
    let mut retire = false;
    let mut begin = false;
    let mut execute = None;
    let mut rescan = false;
    let mut close_result = false;
    let mut refresh_candidates = false;
    {
        let runtime = &mut app.ak820_install_runtime;
        let busy = runtime.owns_hid();
        ui.label(tr(ru, "Select the exact image and local safety inputs. Preparation reads the device; no flash write occurs until you review the preview and explicitly start.", "Выберите точный образ и локальные защитные данные. Подготовка читает устройство; запись прошивки начнётся только после проверки плана и явного запуска."));
        ui.horizontal(|ui| {
            if crate::ui_style::action_button(
                ui,
                tr(
                    ru,
                    "Refresh install candidates",
                    "Обновить устройства для установки",
                ),
                true,
            )
            .clicked()
            {
                refresh_candidates = true;
            }
            if !runtime.candidates.is_empty() {
                let selected = runtime
                    .candidate_index
                    .filter(|i| *i < runtime.candidates.len());
                let title = selected
                    .map(|i| {
                        format!(
                            "{} · {}",
                            runtime.candidates[i].name, runtime.candidates[i].serial_number
                        )
                    })
                    .unwrap_or_else(|| tr(ru, "Choose device…", "Выбрать устройство…").into());
                egui::ComboBox::from_id_salt("ak820_install_candidate")
                    .selected_text(title)
                    .show_ui(ui, |ui| {
                        for (index, device) in runtime.candidates.iter().enumerate() {
                            if ui
                                .selectable_value(
                                    &mut runtime.candidate_index,
                                    Some(index),
                                    format!("{} · {}", device.name, device.serial_number),
                                )
                                .changed()
                            {
                                retire = true;
                            }
                        }
                    });
            }
        });
        ui.add_enabled_ui(!busy, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(format!(
                "{}: {}",
                tr(ru, "Device", "Устройство"),
                selected.as_deref().unwrap_or(tr(
                    ru,
                    "select in device list",
                    "выберите в списке устройств"
                ))
            ));
            ui.label(format!(
                "{}: {}",
                tr(ru, "Backup folder", "Папка резервных копий"),
                runtime
                    .backup_root
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| tr(ru, "not selected", "не выбрана").into())
            ));
            if crate::ui_style::action_button(ui, tr(ru, "Choose folder…", "Выбрать папку…"), !busy && runtime.dialog.is_none())
                .clicked()
            {
                pick(runtime, Pick::BackupRoot);
            }
        });
        ui.horizontal(|ui| {
            ui.label(tr(ru, "Action", "Действие"));
            egui::ComboBox::from_id_salt("ak820_install_mode")
                .selected_text(match runtime.mode {
                    Mode::Firmware => tr(ru, "Install firmware", "Установить прошивку"),
                    Mode::OemRollback => tr(ru, "OEM rollback", "Восстановить OEM"),
                })
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_value(
                            &mut runtime.mode,
                            Mode::Firmware,
                            tr(ru, "Install firmware", "Установить прошивку"),
                        )
                        .changed()
                    {
                        retire = true;
                    }
                    if ui
                        .selectable_value(
                            &mut runtime.mode,
                            Mode::OemRollback,
                            tr(ru, "OEM rollback", "Восстановить OEM"),
                        )
                        .changed()
                    {
                        retire = true;
                    }
                });
            if runtime.mode == Mode::Firmware {
                ui.label(tr(ru, "Release", "Выпуск"));
                egui::ComboBox::from_id_salt("ak820_install_release")
                    .selected_text(if runtime.beta4_1 { "v0.6.0-beta.4.1" } else { "v0.6.0-beta.3" })
                    .show_ui(ui, |ui| {
                        if ui.selectable_value(&mut runtime.beta4_1, false, "v0.6.0-beta.3").changed() {
                            retire = true;
                        }
                        if ui.selectable_value(&mut runtime.beta4_1, true, "v0.6.0-beta.4.1").changed() {
                            runtime.start_with_defaults = true;
                            retire = true;
                        }
                    });
                ui.label(tr(ru, "Profile", "Профиль"));
                egui::ComboBox::from_id_salt("ak820_install_profile")
                    .selected_text(
                        crate::app::ak820_beta1_profiles::profile_description(runtime.profile)
                            .title
                            .for_language(ru),
                    )
                    .show_ui(ui, |ui| {
                        for image in crate::app::ak820_beta1_profiles::profiles() {
                            if ui
                                .selectable_value(
                                    &mut runtime.profile,
                                    image.profile,
                                    crate::app::ak820_beta1_profiles::profile_description(
                                        image.profile,
                                    )
                                    .title
                                    .for_language(ru),
                                )
                                .changed()
                            {
                                retire = true;
                            }
                        }
                    });
            }
        });
        if runtime.mode == Mode::Firmware {
            ui.horizontal(|ui| {
                ui.label(tr(ru, "Firmware input", "Источник прошивки"));
                if ui
                    .selectable_value(
                        &mut runtime.source,
                        Source::Bundled,
                        if runtime.beta4_1 {
                            tr(ru, "Bundled beta4.1 image", "Встроенный образ beta4.1")
                        } else {
                            tr(ru, "Bundled beta3 image", "Встроенный образ beta3")
                        },
                    )
                    .changed()
                {
                    retire = true;
                }
                if ui
                    .selectable_value(
                        &mut runtime.source,
                        Source::PreparedFile,
                        tr(ru, "Prepared payload file", "Подготовленный файл"),
                    )
                    .changed()
                {
                    retire = true;
                }
                if runtime.source == Source::PreparedFile
                    && crate::ui_style::action_button(ui,
                                runtime
                                    .external_payload
                                    .as_ref()
                                    .map(|x| x.name.as_str())
                                    .unwrap_or(tr(ru, "Choose…", "Выбрать…")), !busy)
                        .clicked()
                {
                    pick(runtime, Pick::Payload);
                }
            });
            if ui
                .checkbox(
                    &mut runtime.prerelease_opt_in,
                    tr(ru, "Allow beta prerelease", "Разрешить бета-версию"),
                )
                .changed()
            {
                retire = true;
            }
            if runtime.beta4_1 {
                ui.label(tr(
                    ru,
                    "Beta.4.1 installs exact release defaults; saved-settings writes and restore are not authorized.",
                    "Beta.4.1 устанавливает точные настройки выпуска; запись сохранённых настроек и восстановление не разрешены.",
                ));
            } else if ui
                .checkbox(&mut runtime.start_with_defaults,
                    tr(ru, "Start with defaults (explicit)", "Начать с настроек по умолчанию (явный выбор)"))
                .changed() {
                retire = true;
            }
            ui.label(tr(
                ru,
                "For a first OEM installation, explicitly choose defaults. Project updates require the personal OEM reference; a prepared payload always requires it.",
                "Для первой установки поверх OEM явно выберите настройки по умолчанию. Для обновления проекта нужна личная OEM-копия; для подготовленного файла она обязательна всегда.",
            ));
            ui.horizontal(|ui| {
                ui.label(tr(
                    ru,
                    "Personal OEM recovery ROM (optional for first OEM install)",
                    "Личная OEM-копия ROM (необязательна при первой установке поверх OEM)",
                ));
                if crate::ui_style::action_button(ui,
                            runtime
                                .recovery
                                .as_ref()
                                .map(|x| x.name.as_str())
                                .unwrap_or(tr(
                                    ru,
                                    "Choose 65,536-byte file…",
                                    "Выбрать файл 65 536 байт…",
                                )), !busy)
                    .clicked()
                {
                    pick(runtime, Pick::Recovery);
                }
            });
        } else {
            ui.horizontal(|ui| {
                ui.label(tr(
                    ru,
                    "Previously recorded OEM recovery snapshot",
                    "Ранее сохранённый снимок OEM",
                ));
                let label = runtime
                    .rollback_recovery
                    .as_ref()
                    .map(|x| x.2.as_str())
                    .unwrap_or(tr(
                        ru,
                        "Choose verified snapshot folder…",
                        "Выбрать проверенную папку снимка…",
                    ));
                if crate::ui_style::action_button(ui, label, !busy).clicked() {
                    pick(runtime, Pick::OemRecovery);
                }
            });
            ui.label(tr(ru, "Rollback is available only from a private same-app first-capture snapshot with a recorded device and USB-port binding.", "Откат доступен только из закрытого снимка первого чтения, сохранённого этим приложением и связанного с устройством и USB-портом."));
        }
        ui.horizontal(|ui| {
            if crate::ui_style::action_button(ui, tr(ru, "Check release status", "Проверить статус выпуска"), !busy && runtime.release_check.is_none())
                .clicked()
            {
                start_release_check(runtime, ru);
            }
        });
        if let Some(text) = &runtime.release_check_text {
            ui.label(text);
        }
        ui.label(tr(ru, "Preserving settings requires a complete verified capture. If it fails, explicitly choose defaults and prepare again; there is no silent fallback.", "Для сохранения настроек нужно полное проверенное чтение. При ошибке явно выберите настройки по умолчанию и подготовьте план повторно."));
        });
        match &runtime.owner {
            Owner::Idle => {
                if crate::ui_style::action_button(
                    ui,
                    tr(ru, "Prepare preview", "Подготовить план"),
                    matches!(app.connect_state, ConnectState::Idle) && runtime.dialog.is_none(),
                )
                .clicked()
                {
                    begin = true;
                }
            }
            Owner::Preparing {
                cancel, progress, ..
            } => {
                ui.spinner();
                ui.label(format!(
                    "{} {}%",
                    tr(
                        ru,
                        "Reading and validating device…",
                        "Чтение и проверка устройства…"
                    ),
                    progress.load(Ordering::Acquire)
                ));
                if crate::ui_style::action_button(
                    ui,
                    tr(ru, "Cancel preparation", "Отменить подготовку"),
                    true,
                )
                .clicked()
                {
                    cancel.store(true, Ordering::Release);
                }
            }
            Owner::Prepared(preview) => {
                let summary = preview.plan.summary();
                ui.separator();
                ui.strong(tr(
                    ru,
                    "Review installation plan",
                    "Проверьте план установки",
                ));
                ui.label(format!(
                    "{}: {}",
                    tr(ru, "Action", "Действие"),
                    summary.action
                ));
                ui.label(format!(
                    "{}: {}",
                    tr(ru, "Source ROM SHA-256", "SHA-256 исходного ROM"),
                    summary.source_sha256
                ));
                ui.label(format!(
                    "{}: {}",
                    tr(ru, "Payload SHA-256", "SHA-256 прошивки"),
                    summary.payload_sha256
                ));
                ui.label(format!(
                    "{}: {}",
                    tr(ru, "Expected ROM SHA-256", "SHA-256 ожидаемого ROM"),
                    summary.expected_sha256
                ));
                ui.label(format!(
                    "{}: {}",
                    tr(ru, "Settings", "Настройки"),
                    if preview.plan.summary().action.starts_with("oem-") {
                        tr(ru, "OEM settings", "Настройки OEM")
                    } else if preview.capture.is_some() && !runtime.start_with_defaults {
                        tr(ru, "preserved/migrated", "сохранены/перенесены")
                    } else {
                        tr(ru, "target defaults", "по умолчанию")
                    }
                ));
                ui.label(if summary.physical_hold {
                    tr(
                        ru,
                        "A fresh Esc + Left Control hold for 3 seconds will be requested.",
                        "Потребуется новое удержание Esc + Left Control в течение 3 секунд.",
                    )
                } else {
                    tr(
                        ru,
                        "No project-image hold is required for this source.",
                        "Для этого образа удержание подтверждения не требуется.",
                    )
                });
                ui.colored_label(ui.visuals().warn_fg_color, tr(ru, "Keep the keyboard connected and on external power. An error stops the one attempt; no retry, reset, or automatic restore follows.", "Не отключайте клавиатуру и используйте внешнее питание. Ошибка останавливает единственную попытку; автоматических повтора, сброса и восстановления не будет."));
                if summary.source_is_oem
                    && preview.plan.recovery_provenance().is_none()
                    && preview.plan.summary().action.starts_with("firmware:")
                {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        tr(
                            ru,
                            "This device has no unique serial binding. A full safety snapshot will be saved, but this app cannot offer a verified OEM rollback for it.",
                            "У устройства нет уникального серийного номера. Полный защитный снимок будет сохранён, но приложение не сможет предложить проверенный откат к OEM для этого устройства.",
                        ),
                    );
                }
                ui.horizontal(|ui| {
                    if crate::ui_style::action_button(
                        ui,
                        tr(ru, "Start flashing", "Начать прошивку"),
                        runtime.release_check.is_none(),
                    )
                    .clicked()
                    {
                        execute = Some(summary.confirmation_pin.clone());
                    }
                    if crate::ui_style::action_button(
                        ui,
                        tr(ru, "Retire preview", "Закрыть план"),
                        true,
                    )
                    .clicked()
                    {
                        retire = true;
                    }
                });
            }
            Owner::Executing {
                cancel, progress, ..
            } => {
                ui.separator();
                ui.strong(tr(
                    ru,
                    "Installation owns the device",
                    "Установка владеет устройством",
                ));
                let value = progress.load(Ordering::Acquire);
                ui.add(egui::ProgressBar::new(value as f32 / 100.0).show_percentage());
                if value < 20
                    && crate::ui_style::action_button(
                        ui,
                        tr(ru, "Cancel before erase", "Отменить до стирания"),
                        true,
                    )
                    .clicked()
                {
                    cancel.store(true, Ordering::Release);
                }
                if value >= 20 {
                    ui.label(tr(
                        ru,
                        "Erase/write started; cancellation is no longer effective.",
                        "Стирание/запись начались; отмена больше не действует.",
                    ));
                }
            }
            Owner::Finished { text, success } => {
                ui.separator();
                ui.colored_label(
                    if *success {
                        ui.visuals().text_color()
                    } else {
                        ui.visuals().error_fg_color
                    },
                    text,
                );
                if crate::ui_style::action_button(
                    ui,
                    tr(
                        ru,
                        "Close result and rescan",
                        "Закрыть результат и проверить снова",
                    ),
                    true,
                )
                .clicked()
                {
                    close_result = true;
                    rescan = true;
                }
            }
        }
    }
    if close_result {
        app.ak820_install_runtime.owner = Owner::Idle;
    }
    if retire {
        app.retire_install_preview();
    }
    if let Some(pin) = execute {
        launch_execute(app, pin);
    }
    if begin {
        let payload = app.ak820_install_runtime.external_payload.clone();
        start_prepare(app, payload);
    }
    if rescan {
        app.start_device_scan();
    }
    if refresh_candidates {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = crate::hid::HidDevice::ak820_install_candidates()
                .map_err(|error| format!("{error:#}"));
            let _ = tx.send(result);
        });
        app.ak820_install_runtime.candidate_scan = Some(rx);
        app.status_msg = tr(
            app.app_settings.language == crate::i18n::Language::Russian,
            "Scanning USB installation endpoints…",
            "Поиск USB-устройств для установки…",
        )
        .into();
    }
}

pub(crate) fn show(app: &mut EntropyApp, ctx: &egui::Context) {
    if !ctx.data(|d| d.get_temp::<bool>(egui::Id::new(OPEN_ID)).unwrap_or(false)) {
        return;
    }
    app.poll_ak820_install(ctx);
    let ru = app.app_settings.language == crate::i18n::Language::Russian;
    let active = app.ak820_install_runtime.owns_hid();
    let mut open = true;
    crate::ui_style::utility_window(
        ctx,
        tr(
            ru,
            "AK820 firmware installation",
            "Установка прошивки AK820",
        ),
        egui::Id::new("ak820_install_window"),
        &mut open,
        egui::vec2(720.0, 430.0),
    )
    .vscroll(true)
    .show(ctx, |ui| {
        if crate::ui_style::action_button(ui, tr(ru, "Help", "Справка"), true).clicked() {
            super::ak820_help_ui::open_section(ctx, "firmware-install-recovery");
        }
        draw(app, ui)
    });
    if !open && !active {
        ctx.data_mut(|d| d.insert_temp(egui::Id::new(OPEN_ID), false));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn terminal_reports_localize_verified_outcomes_and_keep_private_paths_local() {
        let safety = SafetyPaths {
            settings_archive: Some(PathBuf::from("private/settings.zip")),
            rom_snapshot: Some(PathBuf::from("private/full-rom.bin")),
            ..Default::default()
        };
        let verified = crate::ak820_install_executor::InstallationResult {
            firmware: crate::ak820_install_executor::InstallFirmwareOutcome::Verified,
            settings:
                crate::ak820_install_executor::InstallSettingsOutcome::PreparedImageSettingsVerified,
            error: None,
        };
        let (en, ok) = terminal_report(&Ok(verified.clone()), &safety, false);
        assert!(
            ok && en.contains("Firmware verified")
                && en.contains("prepared-image settings verified")
        );
        assert!(en.contains("private/settings.zip") && en.contains("private/full-rom.bin"));
        let (ru, ok) = terminal_report(&Ok(verified), &safety, true);
        assert!(
            ok && ru.contains("Прошивка проверена")
                && ru.contains("настройки подготовленного образа проверены")
        );
        assert!(ru.contains("private/settings.zip") && ru.contains("private/full-rom.bin"));
    }

    #[test]
    fn terminal_reports_distinguish_unknown_firmware_and_settings_outcomes() {
        let safety = SafetyPaths::default();
        let unknown = crate::ak820_install_executor::InstallationResult {
            firmware: crate::ak820_install_executor::InstallFirmwareOutcome::Unknown,
            settings: crate::ak820_install_executor::InstallSettingsOutcome::Unverified,
            error: Some("readback timeout".into()),
        };
        for (ru, expected_fw, expected_settings, wrapper) in [
            (false, "unknown", "unverified", "Installation stopped"),
            (true, "неизвестен", "не проверены", "Установка остановлена"),
        ] {
            let (report, success) = terminal_report(&Ok(unknown.clone()), &safety, ru);
            assert!(!success && report.contains(expected_fw) && report.contains(expected_settings));
            assert!(report.contains(wrapper) && report.contains("readback timeout"));
        }
        let (failure, success) = terminal_report(&Err("worker panic".into()), &safety, true);
        assert!(
            !success
                && failure.contains("Сбой рабочего процесса установки")
                && failure.contains("worker panic")
        );
    }

    #[test]
    fn capture_fault_never_falls_through_to_defaults_or_oem_rollback() {
        for require_saved in [false, true] {
            let fault =
                capture_for_preparation::<()>(true, require_saved, || Err("storage fault".into()));
            assert_eq!(fault.unwrap_err(), "storage fault");
        }
        assert_eq!(
            capture_for_preparation(false, false, || -> Result<(), String> {
                panic!("fresh installation owner has no settings capture")
            })
            .unwrap(),
            None
        );
        assert!(capture_for_preparation(false, true, || Ok(())).is_err());
        assert_eq!(
            capture_for_preparation(true, true, || Ok(42)).unwrap(),
            Some(42)
        );
    }
    #[test]
    fn prepared_payload_settings_must_equal_the_derived_settings_bytes() {
        let expected = crate::ak820_release::bundled_defaults(
            crate::app::ak820_beta1_profiles::ProfileId::Main,
        )
        .unwrap();
        // This is only a comparison model, not a catalog-admitted image.
        let mut payload = vec![0; 51136];
        let start = 0xc700 - crate::ak820_image_pack::APP_START;
        payload[start..start + expected.len()].copy_from_slice(&expected);
        assert_eq!(
            validate_payload_nvm(&payload, &expected).unwrap(),
            sha256(&expected)
        );
        payload[start + 0x40] ^= 1;
        assert!(validate_payload_nvm(&payload, &expected).is_err());
        assert!(validate_payload_nvm(&payload[..51135], &expected).is_err());
        assert!(validate_payload_nvm(&payload, &expected[..expected.len() - 1]).is_err());
    }

    #[test]
    fn owner_gate_blocks_every_live_hid_phase_but_not_terminal_result() {
        let (_tx, rx) = mpsc::channel();
        let mut runtime = Runtime::default();
        assert!(!runtime.owns_hid());
        runtime.owner = Owner::Preparing {
            rx,
            cancel: Default::default(),
            progress: Default::default(),
        };
        assert!(runtime.owns_hid() && runtime.blocks_exit());
        runtime.owner = Owner::Idle;
        assert!(!runtime.owns_hid());
    }
    #[test]
    fn install_view_is_localized_and_demo_never_opens_hid() {
        for language in [
            crate::i18n::Language::English,
            crate::i18n::Language::Russian,
        ] {
            let ctx = egui::Context::default();
            let cc = eframe::CreationContext::_new_kittest(ctx.clone());
            let mut app = EntropyApp::new_ak820_demo(&cc);
            app.app_settings.language = language;
            ctx.data_mut(|d| d.insert_temp(egui::Id::new(OPEN_ID), true));
            for _ in 0..3 {
                ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(800.0, 500.0),
                        )),
                        ..Default::default()
                    },
                    |ctx| show(&mut app, ctx),
                );
            }
            let rect = ctx
                .memory(|m| m.area_rect(egui::Id::new("ak820_install_window")))
                .unwrap();
            assert!(rect.width() <= 800.0 && rect.height() <= 500.0);
            assert!(app.hid_device.is_none());
            assert!(!app.ak820_install_runtime.owns_hid());
        }
    }
}
