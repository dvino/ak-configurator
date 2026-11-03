//! Complete local backups and explicitly confirmed, verified settings restore.
use super::ak820_restore_request::{BackupLocation, PreparedRestore, RestoreCompleted};
use crate::ak820_backup_capture::CapturedBackup;
use crate::ak820_backup_files::{self as files, BackupHistory, LoadedBackup};
use crate::app::ak820_beta1_profiles::{self as beta, ProfileId};
use crate::app::EntropyApp;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};

pub(crate) const OPEN_ID: &str = "ak820_backup_inspection_open";
const STATE_ID: &str = "ak820_backup_inspection_state";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Problem {
    File,
    Inspection(files::BackupInspectionError),
    Changed,
    Incompatible,
    Restore,
}
fn problem_text(problem: Problem, ru: bool) -> &'static str {
    match (problem, ru) {
        (Problem::Restore, true) => "Восстановление остановлено. Часть настроек могла измениться. Если защитная копия была сохранена, она остаётся в выбранной папке. Прочитайте настройки снова; автоматического повтора или отката нет.",
        (Problem::Restore, false) => "Restore stopped. Some settings may have changed. Any saved safety backup remains in the selected folder. Read settings again; there is no automatic retry or rollback.",
        (Problem::File, true) => "Не удалось проверить или сохранить копию. Нужна доступная папка с полным составом файлов и поддерживаемой прошивкой. Существующие копии не заменяются.",
        (Problem::File, false) => "Could not validate or save the backup. Select an accessible folder with a complete supported backup. Existing backups are never replaced.",
        (Problem::Inspection(cause), ru) => inspection_problem_text(cause, ru),
        (Problem::Changed, true) => "Подключение изменилось. Для сравнения нужна новая копия текущих настроек.",
        (Problem::Changed, false) => "The connection changed. Read a new snapshot of the current settings before comparing.",
        (Problem::Incompatible, true) => "Восстановление в выбранную прошивку недоступно: переход или сохранённые значения не поддерживаются. Настройки не изменены.",
        (Problem::Incompatible, false) => "Restore to the selected firmware is unavailable: the transition or stored values are unsupported. Settings have not changed.",
    }
}
fn inspection_problem_text(cause: files::BackupInspectionError, ru: bool) -> &'static str {
    use files::BackupInspectionError::*;
    match (cause, ru) {
        (Image, false) => "The backup claims an unsupported firmware image; it was not validated.",
        (Image, true) => "В копии указана неподдерживаемая прошивка; копия не проверена.",
        (Version, false) => {
            "The backup claims an unsupported format or settings version; it was not validated."
        }
        (Version, true) => {
            "В копии указан неподдерживаемый формат или версия настроек; копия не проверена."
        }
        (Policy, false) => {
            "The backup's claimed image is denied by compatibility policy; it was not validated."
        }
        (Policy, true) => {
            "Указанная в копии прошивка запрещена правилами совместимости; копия не проверена."
        }
        (Invalid, false) => "The backup is incomplete or failed validation.",
        (Invalid, true) => "Копия неполная или не прошла проверку.",
        (Io, false) => "The backup files could not be read.",
        (Io, true) => "Не удалось прочитать файлы копии.",
        (Changed, false) => "The backup changed while it was being checked.",
        (Changed, true) => "Копия изменилась во время проверки.",
    }
}
fn capture_problem_text(cause: super::vial_hid_task::CaptureFailure, ru: bool) -> &'static str {
    use super::vial_hid_task::CaptureFailure::*;
    match (cause, ru) {
        (OperationBusy, false) => "Another operation is in progress. Wait for it to finish before reading a backup.",
        (OperationBusy, true) => "Выполняется другая операция. Дождитесь её завершения перед чтением копии.",
        (LeaseBusy, false) => "Another AK Configurator process is using this keyboard. Close it before reading a backup.",
        (LeaseBusy, true) => "Клавиатура занята другим процессом AK Configurator. Закройте его перед чтением копии.",
        (Disconnected, false) => "The device is unavailable. Reconnect it and read settings again.",
        (Disconnected, true) => "Устройство недоступно. Подключите его снова и прочитайте настройки.",
        (Cancelled, false) => "Backup reading was cancelled. No new backup was saved.",
        (Cancelled, true) => "Чтение копии отменено. Новая копия не сохранена.",
        (Storage, false) => "Backup reading stopped because the storage status was invalid or changed. Read settings again.",
        (Storage, true) => "Чтение копии остановлено: состояние памяти недопустимо или изменилось. Прочитайте настройки снова.",
        (Reconciliation, false) => "Backup reading stopped because a device value differed from saved settings. Read settings again.",
        (Reconciliation, true) => "Чтение копии остановлено: значение устройства отличается от сохранённых настроек. Прочитайте настройки снова.",
        (Failed, false) => "Backup reading failed. No new backup was saved; read settings again.",
        (Failed, true) => "Не удалось прочитать копию. Новая копия не сохранена; прочитайте настройки снова.",
    }
}
fn tr<'a>(ru: bool, en: &'a str, russian: &'a str) -> &'a str {
    if ru {
        russian
    } else {
        en
    }
}
fn profile_label(profile: Option<ProfileId>, ru: bool) -> String {
    profile
        .map(|p| {
            beta::profile_description(p)
                .title
                .for_language(ru)
                .to_owned()
        })
        .unwrap_or_else(|| tr(ru, "Historical image", "Предыдущая прошивка").into())
}
// Folder names and manifest timestamps are untrusted local text. Do not render
// control characters or unbounded strings, and never display full private paths.
fn short_text(value: &str) -> String {
    value.chars().filter(|c| !c.is_control()).take(96).collect()
}

#[derive(Clone)]
struct TargetSnapshot {
    data: Arc<CapturedBackup>,
    generation: u64,
    read_at: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Preview {
    source_profile: Option<ProfileId>,
    target_profile: ProfileId,
    groups: Vec<(&'static str, usize)>,
    changed_bytes: usize,
    commands: usize,
}
fn preview(
    source: &LoadedBackup,
    target: &CapturedBackup,
) -> Result<(Preview, Arc<crate::ak820_restore_plan::RestorePlan>), Problem> {
    let plan = crate::ak820_restore_plan::plan_restore(
        source.archive(),
        source.nvm(),
        target.profile(),
        target.identity(),
        target.nvm(),
    )
    .map_err(|_| Problem::Incompatible)?;
    let mut counts = std::collections::BTreeMap::new();
    for step in plan.steps() {
        *counts.entry(step.command()).or_insert(0) += 1;
    }
    let summary = Preview {
        source_profile: source.source_profile(),
        target_profile: target.profile(),
        groups: counts.into_iter().collect(),
        commands: plan.steps().len(),
        changed_bytes: plan
            .before_nvm()
            .iter()
            .zip(plan.expected_nvm())
            .filter(|(a, b)| a != b)
            .count(),
    };
    Ok((summary, Arc::new(plan)))
}

#[derive(Clone)]
enum FilePayload {
    Deleted {
        path: PathBuf,
    },
    History {
        root: PathBuf,
        history: BackupHistory,
    },
    Source {
        path: PathBuf,
        source: LoadedBackup,
    },
    Saved {
        name: String,
        source: LoadedBackup,
        path: PathBuf,
    },
}
#[derive(Clone)]
struct FileMessage {
    id: u64,
    result: Result<Option<FilePayload>, Problem>,
}
#[derive(Clone)]
struct PendingFile {
    id: u64,
    receiver: Arc<Mutex<mpsc::Receiver<FileMessage>>>,
}
#[derive(Clone, Default)]
struct WindowState {
    next_id: u64,
    root: Option<PathBuf>,
    path_entry: String,
    choosing_path: Option<bool>,
    #[cfg(not(target_arch = "wasm32"))]
    browser: Option<Arc<Mutex<super::file_browser::FileBrowser>>>,
    history: Option<Arc<BackupHistory>>,
    pending: Option<PendingFile>,
    file_in_flight: Arc<AtomicBool>,
    capture_pending: Option<(u64, u64)>,
    source: Option<LoadedBackup>,
    source_path: Option<PathBuf>,
    preview_plan: Option<Arc<crate::ak820_restore_plan::RestorePlan>>,
    restore_pending: Option<(u64, u64)>,
    undo_backup: Option<BackupLocation>,
    restore_verified: bool,
    reset_preview: bool,
    delete_confirm: bool,
    target: Option<TargetSnapshot>,
    preview: Option<Preview>,
    saved_name: Option<String>,
    error: Option<Problem>,
    capture_error: Option<super::vial_hid_task::CaptureFailure>,
    target_stale: bool,
}
impl WindowState {
    fn actions(&self, busy: bool, can_capture: bool, files_supported: bool) -> [bool; 4] {
        let current = self.target.is_some() && !self.target_stale;
        [
            !busy && current && self.root.is_some() && files_supported,
            !busy && current && can_capture,
            !busy && current && self.source.is_some(),
            !busy
                && current
                && can_capture
                && self.preview.as_ref().is_some_and(|p| p.commands > 0)
                && self.preview_plan.is_some()
                && (self.reset_preview || self.source_path.is_some())
                && self.root.is_some(),
        ]
    }
    fn id(&mut self) -> u64 {
        self.next_id = self.next_id.wrapping_add(1);
        self.next_id
    }
    fn file_busy(&self) -> bool {
        self.pending.is_some() || self.file_in_flight.load(Ordering::Acquire)
    }
    fn accept(&mut self, message: FileMessage) -> bool {
        if self.pending.as_ref().map(|p| p.id) != Some(message.id) {
            return false;
        }
        self.pending = None;
        match message.result {
            Ok(Some(FilePayload::Deleted { path })) => {
                if self
                    .undo_backup
                    .as_ref()
                    .is_some_and(|location| location.path == path)
                {
                    self.undo_backup = None;
                }
                self.source = None;
                self.source_path = None;
                self.preview = None;
                self.preview_plan = None;
                self.reset_preview = false;
                self.delete_confirm = false;
                self.history = None;
                self.saved_name = None;
            }
            Ok(Some(FilePayload::History { root, history })) => {
                self.root = Some(root);
                self.history = Some(Arc::new(history));
            }
            Ok(Some(FilePayload::Source { path, source })) => {
                self.reset_preview = false;
                self.delete_confirm = false;
                self.source_path = Some(path);
                self.source = Some(source);
                self.preview_plan = None;
                self.restore_verified = false;
                self.preview = None;
            }
            Ok(Some(FilePayload::Saved { name, source, path })) => {
                self.source_path = Some(path);
                self.preview_plan = None;
                self.saved_name = Some(name);
                self.source = Some(source);
                self.preview = None;
                self.history = None; // explicit refresh revalidates the complete directory list
            }
            Ok(None) => {}
            Err(error) => self.error = Some(error),
        }
        true
    }
    fn invalidate_target(&mut self, generation: u64) {
        if (!self.target_stale
            && self
                .target
                .as_ref()
                .is_some_and(|t| t.generation != generation))
            || self.capture_pending.is_some_and(|(_, g)| g != generation)
            || self.restore_pending.is_some_and(|(_, g)| g != generation)
        {
            self.capture_pending = None;
            // Keep the request token until its worker reports a safety copy.
            self.preview_plan = None;
            self.target_stale = true;
            self.error = Some(Problem::Changed);
        }
    }
    fn close(&mut self) {
        self.delete_confirm = false;
        self.choosing_path = None;
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.browser = None;
        }
        self.path_entry.clear();
        self.id();
        self.pending = None;
        self.capture_pending = None;
        // A cancelled restore may already have saved a safety copy. Keep its
        // token until the worker reports the final outcome.
        self.error = None;
    }
}

/// Vial workers already fence connection generations. The window token also
/// rejects completion after cancel, close/reopen or a replacement request.
pub(super) fn capture_finished(
    ctx: &egui::Context,
    id: u64,
    generation: u64,
    result: Result<CapturedBackup, super::vial_hid_task::CaptureFailure>,
) {
    ctx.data_mut(|d| {
        if !d.get_temp::<bool>(egui::Id::new(OPEN_ID)).unwrap_or(false) {
            return;
        }
        let Some(mut state) = d.get_temp::<WindowState>(egui::Id::new(STATE_ID)) else {
            return;
        };
        if state.capture_pending != Some((id, generation)) {
            return;
        }
        state.capture_pending = None;
        match result {
            Ok(snapshot) => {
                state.preview = None;
                state.preview_plan = None;
                state.target_stale = false;
                state.capture_error = None;
                state.target = Some(TargetSnapshot {
                    data: Arc::new(snapshot),
                    generation,
                    read_at: chrono::Local::now().format("%H:%M:%S").to_string(),
                });
                state.saved_name = None;
                state.error = None;
            }
            Err(cause) => {
                state.capture_error = Some(cause);
            }
        }
        d.insert_temp(egui::Id::new(STATE_ID), state);
    });
    ctx.request_repaint();
}

#[cfg(test)]
pub(super) fn test_capture_pending(ctx: &egui::Context, id: u64, generation: u64) {
    ctx.data_mut(|d| {
        d.insert_temp(egui::Id::new(OPEN_ID), true);
        d.insert_temp(
            egui::Id::new(STATE_ID),
            WindowState {
                capture_pending: Some((id, generation)),
                ..Default::default()
            },
        );
    });
}

#[cfg(test)]
pub(super) type TestFrameState = (Option<(u64, u64)>, Option<bool>, String);

#[cfg(test)]
pub(super) fn test_frame_state(ctx: &egui::Context) -> Option<TestFrameState> {
    ctx.data(|d| d.get_temp::<WindowState>(egui::Id::new(STATE_ID)))
        .map(|state| (state.capture_pending, state.choosing_path, state.path_entry))
}

#[cfg(test)]
pub(super) fn test_set_frame_state(ctx: &egui::Context, generation: u64) {
    test_capture_pending(ctx, 42, generation);
    ctx.data_mut(|d| {
        let mut state = d.get_temp::<WindowState>(egui::Id::new(STATE_ID)).unwrap();
        state.choosing_path = Some(false);
        state.path_entry = "synthetic inspection".into();
        d.insert_temp(egui::Id::new(STATE_ID), state);
    });
}

#[cfg(test)]
pub(super) fn test_capture_failure(
    ctx: &egui::Context,
) -> Option<super::vial_hid_task::CaptureFailure> {
    ctx.data(|d| d.get_temp::<WindowState>(egui::Id::new(STATE_ID)))
        .and_then(|state| state.capture_error)
}

/// The worker has already checked the complete result. The editor reloads via
/// its normal read-only connection path; stale UI values are never reused.
pub(super) fn restore_finished(
    ctx: &egui::Context,
    id: u64,
    generation: u64,
    result: Result<RestoreCompleted, ()>,
) {
    ctx.data_mut(|d| {
        let Some(mut state) = d.get_temp::<WindowState>(egui::Id::new(STATE_ID)) else {
            return;
        };
        if state.restore_pending != Some((id, generation)) {
            return;
        }
        state.restore_pending = None;
        state.target = None;
        state.preview = None;
        state.preview_plan = None;
        state.history = None;
        state.saved_name = None;
        match result {
            Ok(completed) => {
                // Keep only the file reference after verified success, not a
                // potentially stale target snapshot across the editor reload.
                let _verified_profile = completed.snapshot.profile();
                state.undo_backup = completed.safety;
                state.restore_verified = true;
                state.error = None;
            }
            Err(()) => {
                state.restore_verified = false;
                state.error = Some(Problem::Restore);
            }
        }
        d.insert_temp(egui::Id::new(STATE_ID), state);
    });
    ctx.request_repaint();
}

pub(super) fn restore_failed(
    ctx: &egui::Context,
    id: u64,
    generation: u64,
    safety: Option<BackupLocation>,
) {
    ctx.data_mut(|d| {
        let Some(mut state) = d.get_temp::<WindowState>(egui::Id::new(STATE_ID)) else {
            return;
        };
        if state.restore_pending != Some((id, generation)) {
            return;
        }
        if safety.is_some() {
            state.undo_backup = safety;
        }
        d.insert_temp(egui::Id::new(STATE_ID), state);
    });
    restore_finished(ctx, id, generation, Err(()));
}

fn backup_directory_from_manifest(path: &std::path::Path) -> Result<PathBuf, Problem> {
    if path.file_name() != Some(std::ffi::OsStr::new("manifest.json")) {
        return Err(Problem::File);
    }
    let parent = path.parent().ok_or(Problem::File)?;
    Ok(if parent.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        parent.to_path_buf()
    })
}

enum FileAction {
    OpenManifest(PathBuf),
    Delete {
        path: PathBuf,
        manifest_sha256: String,
    },
    Open(PathBuf, Option<String>),
    History(PathBuf),
    Save {
        root: PathBuf,
        snapshot: Arc<CapturedBackup>,
    },
}
#[cfg(not(target_arch = "wasm32"))]
struct FilePermit(Arc<AtomicBool>);

#[cfg(not(target_arch = "wasm32"))]
impl Drop for FilePermit {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn start_file(ctx: &egui::Context, state: &mut WindowState, action: FileAction) {
    if state.file_busy() || state.capture_pending.is_some() || state.restore_pending.is_some() {
        return;
    }
    let id = state.id();
    state.error = None;
    state.choosing_path = None;
    state.browser = None;
    if matches!(action, FileAction::History(_)) {
        state.history = None;
    }
    start_file_worker(ctx, state, id, move || {
        (|| -> Result<Option<FilePayload>, Problem> {
            match action {
                FileAction::Delete {
                    path,
                    manifest_sha256,
                } => {
                    files::delete_validated_archive(&path, &manifest_sha256)
                        .map_err(|_| Problem::File)?;
                    Ok(Some(FilePayload::Deleted { path }))
                }
                FileAction::OpenManifest(manifest) => {
                    let path = backup_directory_from_manifest(&manifest)?;
                    files::read_backup_inspection(&path, None)
                        .map(|source| Some(FilePayload::Source { path, source }))
                        .map_err(Problem::Inspection)
                }
                FileAction::Open(path, pin) => files::read_backup_inspection(&path, pin.as_deref())
                    .map(|source| Some(FilePayload::Source { path, source }))
                    .map_err(Problem::Inspection),
                FileAction::History(root) => {
                    let history = files::scan_backup_history(&root).map_err(|_| Problem::File)?;
                    Ok(Some(FilePayload::History { root, history }))
                }
                FileAction::Save { root, snapshot } => {
                    let name = format!(
                        "ak820-{}-{}",
                        snapshot.profile().as_str(),
                        chrono::Utc::now().format("%Y%m%dT%H%M%S%.9fZ")
                    );
                    let path = root.join(&name);
                    let archive = files::publish_validated_archive(
                        &path,
                        snapshot.profile(),
                        snapshot.files(),
                    )
                    .map_err(|_| Problem::File)?;
                    // Reopen the published destination and pin its exact manifest.
                    let source = files::read_backup_auto(&path, Some(&archive.manifest_sha256))
                        .map_err(|_| Problem::File)?;
                    Ok(Some(FilePayload::Saved { name, source, path }))
                }
            }
        })()
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn start_file_worker(
    ctx: &egui::Context,
    state: &mut WindowState,
    id: u64,
    work: impl FnOnce() -> Result<Option<FilePayload>, Problem> + Send + 'static,
) {
    if state
        .file_in_flight
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        return;
    }
    let permit = FilePermit(state.file_in_flight.clone());
    let (tx, rx) = mpsc::channel();
    state.pending = Some(PendingFile {
        id,
        receiver: Arc::new(Mutex::new(rx)),
    });
    let repaint = ctx.clone();
    if std::thread::Builder::new()
        .spawn(move || {
            let _permit = permit;
            let result = work();
            let _ = tx.send(FileMessage { id, result });
            repaint.request_repaint();
        })
        .is_err()
    {
        state.pending = None;
        state.error = Some(Problem::File);
    }
}

fn draw_source(ui: &mut egui::Ui, source: &LoadedBackup, ru: bool) {
    let archive = source.archive();
    ui.strong(tr(ru, "Validated backup", "Проверенная резервная копия"));
    ui.label(format!(
        "{} · {}",
        profile_label(source.source_profile(), ru),
        short_text(archive.manifest["created_utc"].as_str().unwrap_or(""))
    ));
    ui.label(tr(
        ru,
        "6 layers · 83 keys · 12 encoder assignments · 16 macros",
        "6 слоёв · 83 клавиши · 12 назначений энкодера · 16 макросов",
    ));
    ui.label(tr(ru, "Includes durable Combo, QMK Settings and typed storage, including inactive values. Direct colors and host scenes are separate local presets.", "Содержит сохранённые Combo, QMK Settings и таблицы модулей, включая неактивные значения. Цвета Direct и сцены компьютера — отдельные локальные пресеты."));
    ui.collapsing(
        tr(ru, "Image and integrity", "Прошивка и целостность"),
        |ui| {
            ui.label(format!(
                "{}: {}",
                tr(ru, "Schema", "Схема"),
                archive.source.schema
            ));
            ui.label(format!("IHX SHA-256: {}", archive.source.ihx_sha256));
            ui.label(format!("CODE SHA-256: {}", archive.source.code_sha256));
            ui.label(format!(
                "{} SHA-256: {}",
                tr(ru, "Manifest", "Манифест"),
                archive.manifest_sha256
            ));
        },
    );
    draw_raw_manifest(ui, source.manifest_raw(), &archive.manifest_sha256, ru);
}

fn draw_raw_manifest(
    ui: &mut egui::Ui,
    raw: &[u8],
    manifest_sha256: &str,
    ru: bool,
) -> egui::collapsing_header::CollapsingResponse<()> {
    egui::CollapsingHeader::new(tr(
        ru,
        "Original validated manifest",
        "Исходный проверенный манифест",
    ))
    .id_salt(("ak820_original_manifest", manifest_sha256))
    .show(ui, |ui| {
        ui.label(tr(
            ru,
            "Local read-only view; may contain private metadata. Nothing is exported or logged.",
            "Локальный просмотр без изменений; возможны личные метаданные. Экспорт и запись в журнал не выполняются.",
        ));
        match std::str::from_utf8(raw) {
            Ok(text) => {
                egui::ScrollArea::vertical()
                    .id_salt(("ak820_original_manifest_scroll", manifest_sha256))
                    .max_height(240.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::Label::new(egui::RichText::new(text).monospace())
                                .selectable(false)
                                .wrap(),
                        );
                    });
            }
            Err(_) => {
                // Validation normally rejects non-UTF-8 JSON. Never display a
                // lossy substitute as the original bytes, even defensively.
                ui.label(tr(
                    ru,
                    "Original manifest is not valid UTF-8; exact text preview is unavailable.",
                    "Исходный манифест не является UTF-8; точный текстовый просмотр недоступен.",
                ));
            }
        }
    })
}

fn draw_preview(ui: &mut egui::Ui, value: &Preview, ru: bool) {
    ui.label(format!(
        "{} → {}",
        profile_label(value.source_profile, ru),
        profile_label(Some(value.target_profile), ru)
    ));
    if value.commands == 0 {
        ui.label(tr(
            ru,
            "Durable settings already match this snapshot.",
            "Сохранённые настройки уже совпадают с этим снимком.",
        ));
    } else {
        for (kind, count) in &value.groups {
            let label = match *kind {
                "set_key" => tr(ru, "Key assignments", "Назначения клавиш"),
                "set_encoder" => tr(ru, "Encoder assignments", "Назначения энкодера"),
                "set_layout_options" => tr(ru, "Layout options", "Параметры раскладки"),
                "upload_macro_chunk" => tr(ru, "Macro data chunks", "Блоки данных макросов"),
                "set_rgb_mode" => tr(ru, "Lighting operations", "Операции подсветки"),
                "set_combo" => "Combo",
                "set_setting" => "QMK Settings",
                "set_extension" => tr(ru, "Typed module entries", "Записи таблиц модулей"),
                _ => tr(ru, "Other operations", "Другие операции"),
            };
            ui.label(format!("{label}: {count}"));
        }
        ui.label(format!(
            "{}: {}",
            tr(ru, "Changed saved bytes", "Изменяемых сохранённых байтов"),
            value.changed_bytes
        ));
    }
    ui.label(tr(ru, "Before restore, the source and keyboard are checked again and a complete safety backup is saved. A fresh Esc + left Ctrl hold confirms this operation. Any error stops without retry or rollback.", "Перед восстановлением копия и клавиатура проверяются снова, затем сохраняется полный защитный бэкап. Новое удержание Esc + левого Ctrl подтверждает эту операцию. При ошибке запись останавливается без повтора или отката."));
}

/// Invalidate stale snapshots even while the page is closed. Restore completion
/// is delivered separately by the HID worker, including after cancellation.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn invalidate_closed(ctx: &egui::Context, generation: u64) {
    ctx.data_mut(|d| {
        if let Some(mut state) = d.get_temp::<WindowState>(egui::Id::new(STATE_ID)) {
            state.invalidate_target(generation);
            d.insert_temp(egui::Id::new(STATE_ID), state);
        }
    });
}

pub(super) fn close_page(ctx: &egui::Context) {
    ctx.data_mut(|d| {
        if let Some(mut state) = d.get_temp::<WindowState>(egui::Id::new(STATE_ID)) {
            state.close();
            d.insert_temp(egui::Id::new(STATE_ID), state);
        }
    });
}

pub(super) fn page_busy(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<WindowState>(egui::Id::new(STATE_ID)))
        .is_some_and(|state| {
            state.file_busy() || state.capture_pending.is_some() || state.restore_pending.is_some()
        })
}

fn draw_history_warning(ui: &mut egui::Ui, invalid_skipped: usize, ru: bool) {
    if invalid_skipped > 0 {
        ui.label(format!(
            "{}: {}",
            tr(
                ru,
                "Folders skipped because validation failed",
                "Папок не прошли проверку",
            ),
            invalid_skipped
        ));
    }
}

pub(crate) fn show(app: &mut EntropyApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let ctx = &ctx;
    if !ctx.data(|d| d.get_temp::<bool>(egui::Id::new(OPEN_ID)).unwrap_or(false)) {
        return;
    }
    let ru = app.app_settings.language == crate::i18n::Language::Russian;
    let mut state = ctx
        .data(|d| d.get_temp::<WindowState>(egui::Id::new(STATE_ID)))
        .unwrap_or_default();
    #[cfg(not(target_arch = "wasm32"))]
    state.invalidate_target(app.connection_generation);
    if let Some(pending) = &state.pending {
        let outcome = pending
            .receiver
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .try_recv();
        match outcome {
            Ok(message) => {
                state.accept(message);
            }
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(100))
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                state.pending = None;
                state.error = Some(Problem::File);
            }
        }
    }
    let busy =
        state.file_busy() || state.capture_pending.is_some() || state.restore_pending.is_some();
    #[cfg(not(target_arch = "wasm32"))]
    let can_capture =
        app.capture_availability() == super::vial_hid_task::CaptureAvailability::Ready;
    #[cfg(target_arch = "wasm32")]
    let can_capture = false;
    let actions = state.actions(
        busy,
        can_capture,
        cfg!(any(
            target_os = "linux",
            target_os = "macos",
            target_os = "windows"
        )),
    );
    if state.file_in_flight.load(Ordering::Acquire) {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
    let mut file_action = None;
    let mut capture = false;
    let mut cancel = false;
    let mut compare = false;
    let mut reset = false;
    let mut restore = false;
    let mut close = false;
    egui::ScrollArea::vertical().id_salt("ak820_backup_inspection_page").show(ui, |ui| {
            ui.heading(tr(ru, "AK820 backups", "Резервные копии AK820"));
            if crate::ui_style::action_button(ui, tr(ru, "Help", "Справка"), true).clicked() {
                super::ak820_help_ui::open_section(ctx, "backups-restore");
            }
            ui.label(tr(ru, "Complete saved settings and firmware evidence. ROM and macros stay in the chosen local folder.", "Полные сохранённые настройки и данные для проверки прошивки. ROM и макросы остаются в выбранной локальной папке."));
            ui.label(tr(ru, "1. Open an existing backup → 2. Inspect source and compatibility → 3. Review restore prerequisites", "1. Откройте существующую копию → 2. Проверьте источник и совместимость → 3. Изучите условия восстановления"));
            ui.label(tr(ru, "For beta.5: restore unavailable because saved-settings writes are not authorized for this release. Inspection remains available.", "Для beta.5 восстановление недоступно: запись сохранённых настроек для этого выпуска не разрешена. Проверка копии доступна."));
            if !cfg!(any(target_os = "linux", target_os = "macos", target_os = "windows")) {
                ui.label(tr(ru, "Backup folder access is unavailable on this platform.", "Работа с папками копий недоступна на этой платформе."));
            }
            ui.horizontal_wrapped(|ui| {
                if crate::ui_style::action_button(ui, tr(ru, "Open backup…", "Открыть копию…"), !busy).clicked() { state.choosing_path = Some(false); state.path_entry.clear(); #[cfg(not(target_arch = "wasm32"))] { state.browser = None; } }
                if crate::ui_style::action_button(ui, tr(ru, "Choose backup folder…", "Папка для копий…"), !busy).clicked() { state.choosing_path = Some(true); state.path_entry.clear(); #[cfg(not(target_arch = "wasm32"))] { state.browser = None; } }
            });
            if let Some(history) = state.choosing_path {
                ui.label(if history { tr(ru, "Select or enter the backup folder path", "Выберите папку копий или введите путь к ней") } else { tr(ru, "Select or enter the backup manifest.json file", "Выберите файл manifest.json резервной копии или введите путь к нему") });
                ui.text_edit_singleline(&mut state.path_entry);
                ui.horizontal_wrapped(|ui| {
                    if crate::ui_style::action_button(ui, tr(ru, "Open", "Открыть"), !busy && !state.path_entry.trim().is_empty()).clicked() {
                        let path = PathBuf::from(&state.path_entry);
                        file_action = Some(if history { FileAction::History(path) } else { FileAction::OpenManifest(path) });
                        state.choosing_path = None;
                    }
                    if crate::ui_style::action_button(ui, tr(ru, "Cancel", "Отмена"), true).clicked() { state.choosing_path = None; #[cfg(not(target_arch = "wasm32"))] { state.browser = None; } }
                    #[cfg(not(target_arch = "wasm32"))]
                    if crate::ui_style::action_button(ui, tr(ru, "Browse…", "Обзор…"), !busy).clicked() {
                        let mode = if history { super::file_browser::BrowserMode::Directory } else { super::file_browser::BrowserMode::File };
                        state.browser = Some(Arc::new(Mutex::new(super::file_browser::FileBrowser::new(PathBuf::from("."), mode, ctx))));
                    }
                });
                #[cfg(not(target_arch = "wasm32"))]
                if !busy {
                    let result = state.browser.as_ref().and_then(|browser| browser.lock().unwrap_or_else(|e| e.into_inner()).show(ui, ru));
                    if let Some(result) = result {
                        state.browser = None;
                        match result {
                            super::file_browser::BrowserResult::Selected(path) => { file_action = Some(FileAction::OpenManifest(path)); state.choosing_path = None; }
                            super::file_browser::BrowserResult::Cancelled => { state.choosing_path = None; }
                            super::file_browser::BrowserResult::Directory(path) => { if history { file_action = Some(FileAction::History(path)); state.choosing_path = None; } }
                        }
                    }
                }
            }
            if let Some(root) = &state.root {
                let name = root.file_name().unwrap_or_else(|| root.as_os_str()).to_string_lossy();
                ui.label(format!("{}: {}", tr(ru, "Folder", "Папка"), short_text(&name)));
                ui.label(tr(ru, "Backup folder selected. New copies are saved to separate dated folders.", "Папка выбрана. Новые копии сохраняются в отдельных папках с датой."));
                if crate::ui_style::action_button(ui, tr(ru, "Refresh list", "Обновить список"), !busy).clicked() { file_action = Some(FileAction::History(root.clone())); }
            }
            if let Some(history) = &state.history {
                ui.collapsing(format!("{} ({})", tr(ru, "Saved backups", "Сохранённые копии"), history.entries.len()), |ui| {
                    if history.entries.is_empty() { ui.label(tr(ru, "No validated backups in this folder.", "В этой папке нет проверенных копий.")); }
                    for entry in &history.entries {
                        let label = format!("{} · {} · {}", short_text(&entry.created_utc), profile_label(entry.source_profile, ru), short_text(&entry.name));
                        if ui.add_enabled(!busy, egui::Button::new(label).wrap()).clicked() {
                            file_action = Some(FileAction::Open(entry.path.clone(), Some(entry.manifest_sha256.clone())));
                        }
                    }
                });
                draw_history_warning(ui, history.invalid_skipped, ru);
            }
            ui.separator();
            ui.strong(tr(ru, "Current keyboard", "Текущая клавиатура"));
            if crate::ui_style::action_button(ui, tr(ru, "Read complete snapshot", "Прочитать полный снимок"), !busy && can_capture).clicked() { capture = true; }
            if app.ak820_demo {
                ui.label(tr(ru, "Demo has no physical settings snapshot. Connect a supported keyboard to create a complete backup.", "В деморежиме нет снимка физической клавиатуры. Для полной копии подключите поддерживаемую клавиатуру."));
            } else if !can_capture && state.capture_pending.is_none() {
                #[cfg(not(target_arch = "wasm32"))]
                if let super::vial_hid_task::CaptureAvailability::Unavailable(cause) = app.capture_availability() { ui.label(capture_problem_text(cause, ru)); }
                #[cfg(target_arch = "wasm32")]
                ui.label(tr(ru, "A verified supported AK820 connection with no pending edits is required.", "Нужно проверенное подключение поддерживаемой AK820 без ожидающих изменений."));
            }
            if let Some((id, _)) = state.capture_pending {
                #[cfg(not(target_arch = "wasm32"))]
                let progress = app.ak820_backup_capture_progress(id).unwrap_or(0.0);
                #[cfg(target_arch = "wasm32")]
                let progress = 0.0;
                ui.add(egui::ProgressBar::new(progress).show_percentage().text(tr(ru, "Reading and checking…", "Чтение и проверка…")));
                if crate::ui_style::action_button(ui, tr(ru, "Cancel reading", "Отменить чтение"), true).clicked() { cancel = true; }
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
            if let Some(target) = &state.target {
                if state.target_stale { ui.colored_label(egui::Color32::LIGHT_RED, tr(ru, "This snapshot is stale. Read current settings again before using it.", "Этот снимок устарел. Перед использованием снова прочитайте текущие настройки.")); }
                ui.label(format!("{} · {}", profile_label(Some(target.data.profile()), ru), target.read_at));
                ui.label(tr(ru, "64 KiB ROM and 10 KiB saved settings checked. This snapshot reflects the time it was read.", "Проверены 64 КиБ ROM и 10 КиБ сохранённых настроек. Снимок соответствует моменту чтения."));
                if let Some(name) = &state.saved_name { ui.label(format!("{}: {}", tr(ru, "Saved and rechecked", "Сохранено и проверено повторно"), short_text(name))); }
                else { ui.label(tr(ru, "Snapshot is in memory; no backup file has been saved yet.", "Снимок в памяти; файл резервной копии ещё не сохранён.")); }
                if crate::ui_style::action_button(ui, tr(ru, "Save new backup", "Сохранить новую копию"), actions[0]).clicked() {
                    file_action = Some(FileAction::Save { root: state.root.clone().unwrap(), snapshot: target.data.clone() });
                }
                if crate::ui_style::action_button(ui, tr(ru, "Preview reset to release defaults…", "Предпросмотр сброса к настройкам выпуска…"), actions[1]).clicked() { reset = true; }
            }
            if state.restore_verified {
                ui.colored_label(egui::Color32::LIGHT_GREEN, tr(ru, "Restore verified. The editor reads the current settings again.", "Восстановление проверено. Редактор повторно читает текущие настройки."));
            }
            if let Some(location) = &state.undo_backup {
                if crate::ui_style::action_button(ui, tr(ru, "Open safety backup to undo restore", "Открыть защитную копию для отмены восстановления"), !busy).clicked() {
                    file_action = Some(FileAction::Open(location.path.clone(), Some(location.manifest_sha256.clone())));
                }
                ui.label(tr(ru, "Undo uses the same preview, fresh confirmation and verified restore workflow.", "Для отмены используются тот же предпросмотр, новое подтверждение и проверяемое восстановление."));
            }
            if let Some(source) = &state.source {
                ui.separator(); draw_source(ui, source, ru);
                if crate::ui_style::action_button(ui, tr(ru, "Preview restore to this snapshot", "Предпросмотр восстановления в этот снимок"), actions[2]).clicked() { compare = true; }
                if crate::ui_style::action_button(ui, tr(ru, "Delete this backup…", "Удалить эту копию…"), !busy && state.source_path.is_some()).clicked() { state.delete_confirm = true; }
                if state.delete_confirm {
                    let name = state.source_path.as_ref().and_then(|p| p.file_name()).map(|p| p.to_string_lossy()).unwrap_or_default();
                    ui.label(format!("{}: {}", tr(ru, "Permanently delete all files of this validated backup", "Безвозвратно удалить все файлы этой проверенной копии"), short_text(&name)));
                    ui.horizontal_wrapped(|ui| {
                        if crate::ui_style::action_button(ui, tr(ru, "Delete permanently", "Удалить безвозвратно"), !busy).clicked() {
                            if let Some(path) = &state.source_path { file_action = Some(FileAction::Delete { path:path.clone(), manifest_sha256:source.archive().manifest_sha256.clone() }); }
                            state.delete_confirm = false;
                        }
                        if crate::ui_style::action_button(ui, tr(ru, "Keep backup", "Сохранить копию"), true).clicked() { state.delete_confirm = false; }
                    });
                }
            }
            if let Some(value) = &state.preview {
                ui.separator(); draw_preview(ui, value, ru);
                if state.reset_preview { ui.label(tr(ru, "Reset supported keys, encoder mappings, macros, lighting and enabled modules. Inactive and inaccessible data remain intact. A verified safety backup is required before fresh confirmation.", "Сбрасываются поддерживаемые назначения клавиш и энкодера, макросы, подсветка и включённые модули. Неактивные и недоступные данные сохраняются. Перед новым подтверждением обязательна проверенная защитная копия.")); }
                let ready = actions[3];
                let label = if state.reset_preview { tr(ru, "Reset these settings…", "Сбросить эти настройки…") } else { tr(ru, "Restore these settings…", "Восстановить эти настройки…") };
                if crate::ui_style::action_button(ui, label, ready).clicked() { restore = true; }
                if state.root.is_none() { ui.label(tr(ru, "Choose a backup folder before restore.", "Перед восстановлением выберите папку для защитной копии.")); }
            }
            if let Some((id, _)) = state.restore_pending {
                #[cfg(not(target_arch = "wasm32"))]
                let progress = app.ak820_restore_progress(id).unwrap_or(0);
                #[cfg(target_arch = "wasm32")]
                let progress = 0;
                let label = match progress {
                    0..=9 => tr(ru, "Checking source and current keyboard…", "Проверка копии и текущей клавиатуры…"),
                    10..=19 => tr(ru, "Saving and rechecking safety backup…", "Сохранение и повторная проверка защитной копии…"),
                    20..=29 => tr(ru, "Release keys, then hold Esc + left Ctrl for 3 seconds", "Отпустите клавиши, затем удерживайте Esc + левый Ctrl 3 секунды"),
                    30..=94 => tr(ru, "Restoring and verifying settings…", "Восстановление и проверка настроек…"),
                    _ => tr(ru, "Final complete verification…", "Итоговая полная проверка…"),
                };
                ui.add(egui::ProgressBar::new(progress as f32 / 100.0).text(label));
                if crate::ui_style::action_button(ui, tr(ru, "Stop restore", "Остановить восстановление"), true).clicked() { cancel = true; }
                ui.label(tr(ru, "Stopping leaves already written changes in place. The safety copy remains available.", "При остановке уже записанные изменения сохраняются. Защитная копия остаётся доступной."));
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
            if let Some(cause) = state.capture_error { ui.colored_label(egui::Color32::LIGHT_RED, capture_problem_text(cause, ru)); }
            if let Some(error) = state.error { ui.colored_label(egui::Color32::LIGHT_RED, problem_text(error, ru)); }
            if state.file_busy() { ui.spinner(); ui.label(tr(ru, "Checking local files…", "Проверка локальных файлов…")); }
            if crate::ui_style::action_button(ui, tr(ru, "Close", "Закрыть"), true).clicked() { close = true; }
        });
    if compare {
        if let (Some(source), Some(target)) = (&state.source, &state.target) {
            match preview(source, &target.data) {
                Ok((value, plan)) => {
                    state.preview = Some(value);
                    state.preview_plan = Some(plan);
                    state.reset_preview = false;
                    state.error = None;
                }
                Err(error) => {
                    state.error = Some(error);
                }
            }
        }
    }
    if reset {
        if let Some(target) = &state.target {
            match crate::ak820_restore_plan::plan_factory_reset(
                target.data.profile(),
                target.data.identity(),
                target.data.nvm(),
            ) {
                Ok(plan) => {
                    let mut groups = std::collections::BTreeMap::new();
                    for step in plan.steps() {
                        *groups.entry(step.command()).or_insert(0) += 1;
                    }
                    state.preview = Some(Preview {
                        source_profile: Some(target.data.profile()),
                        target_profile: target.data.profile(),
                        groups: groups.into_iter().collect(),
                        commands: plan.steps().len(),
                        changed_bytes: plan
                            .before_nvm()
                            .iter()
                            .zip(plan.expected_nvm())
                            .filter(|(a, b)| a != b)
                            .count(),
                    });
                    state.preview_plan = Some(Arc::new(plan));
                    state.reset_preview = true;
                    state.error = None;
                }
                Err(_) => {
                    state.error = Some(Problem::Incompatible);
                }
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    if capture {
        state.choosing_path = None;
        state.browser = None;
        let id = state.id();
        state.capture_error = None;
        match app.start_ak820_backup_capture(ctx, id) {
            Ok(()) => state.capture_pending = Some((id, app.connection_generation)),
            Err(cause) => state.capture_error = Some(cause),
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    if restore {
        state.choosing_path = None;
        state.browser = None;
        if state.reset_preview {
            if let (Some(plan), Some(target), Some(root)) = (
                state.preview_plan.clone(),
                state.target.as_ref(),
                state.root.as_ref(),
            ) {
                match PreparedRestore::factory_defaults(plan, target.data.profile(), root) {
                    Ok(request) => {
                        let id = state.id();
                        if app.start_ak820_restore(ctx, id, Arc::new(request))
                            == super::vial_hid_task::VialHidTaskStart::Started
                        {
                            state.restore_pending = Some((id, app.connection_generation));
                            state.error = None;
                            state.restore_verified = false;
                        } else {
                            state.error = Some(Problem::Restore);
                        }
                    }
                    Err(_) => state.error = Some(Problem::Incompatible),
                }
            }
        } else {
            if let (Some(plan), Some(source), Some(path), Some(root)) = (
                state.preview_plan.clone(),
                state.source.clone(),
                state.source_path.clone(),
                state.root.as_ref(),
            ) {
                match PreparedRestore::new(plan, source, path, root) {
                    Ok(request) => {
                        let id = state.id();
                        if app.start_ak820_restore(ctx, id, Arc::new(request))
                            == super::vial_hid_task::VialHidTaskStart::Started
                        {
                            state.restore_pending = Some((id, app.connection_generation));
                            state.error = None;
                            state.restore_verified = false;
                        } else {
                            state.error = Some(Problem::Restore);
                        }
                    }
                    Err(_) => state.error = Some(Problem::Incompatible),
                }
            }
        }
    }
    if cancel || close {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some((id, _)) = state.capture_pending {
            app.cancel_ak820_backup_capture(id);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some((id, _)) = state.restore_pending {
            app.cancel_ak820_restore(id);
            state.target_stale = true;
            state.preview_plan = None;
        }
        // Keep the restore request token until the worker reports the actual
        // outcome. Close discards it only after signalling cancellation.
        if state.capture_pending.take().is_some() {
            state.capture_error = Some(super::vial_hid_task::CaptureFailure::Cancelled);
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(action) = file_action {
        start_file(ctx, &mut state, action);
    }
    #[cfg(target_arch = "wasm32")]
    let _ = (file_action, capture, restore);
    if close {
        state.close();
        ctx.data_mut(|d| d.insert_temp(egui::Id::new(OPEN_ID), false));
    }
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(STATE_ID), state));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_warning_uses_only_the_canonical_count_in_both_languages() {
        for (ru, prefix) in [
            (false, "Folders skipped because validation failed"),
            (true, "Папок не прошли проверку"),
        ] {
            for count in [0, 1, 2, 128] {
                let ctx = egui::Context::default();
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(900.0, 700.0),
                        )),
                        ..Default::default()
                    },
                    |root| {
                        egui::CentralPanel::default().show_inside(root, |ui| {
                            draw_history_warning(ui, count, ru);
                        });
                    },
                );
                assert!(output.platform_output.commands.is_empty());
                let labels: Vec<_> = output
                    .shapes
                    .into_iter()
                    .filter_map(|shape| match shape.shape {
                        egui::Shape::Text(text) => Some(text.galley.job.text.clone()),
                        _ => None,
                    })
                    .collect();
                if count == 0 {
                    assert!(labels.is_empty());
                } else {
                    assert_eq!(labels, vec![format!("{prefix}: {count}")]);
                }
            }
        }
    }

    // Presentation-only bytes, not an admitted backup/image fixture.
    fn raw_manifest_labels(raw: &[u8], expanded: bool, ru: bool) -> Vec<String> {
        let ctx = egui::Context::default();
        ctx.global_style_mut(|style| style.animation_time = 0.0);
        let mut output = None;
        for _ in 0..2 {
            output = Some(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 700.0),
                    )),
                    ..Default::default()
                },
                |root| {
                    egui::CentralPanel::default().show_inside(root, |ui| {
                        let response = draw_raw_manifest(ui, raw, "presentation-only", ru);
                        if expanded {
                            let mut state =
                                egui::collapsing_header::CollapsingState::load_with_default_open(
                                    ui.ctx(),
                                    response.header_response.id,
                                    false,
                                );
                            state.set_open(true);
                            state.store(ui.ctx());
                        }
                    });
                },
            ));
            assert!(output.as_ref().unwrap().platform_output.commands.is_empty());
        }
        output
            .unwrap()
            .shapes
            .into_iter()
            .filter_map(|shape| match shape.shape {
                egui::Shape::Text(text) => Some(text.galley.job.text.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn raw_manifest_is_collapsed_by_default_without_exposing_private_text() {
        let raw = br#"{ "private_metadata": "presentation-only" }"#;
        for ru in [false, true] {
            let labels = raw_manifest_labels(raw, false, ru);
            assert!(labels.iter().any(|label| label
                == tr(
                    ru,
                    "Original validated manifest",
                    "Исходный проверенный манифест",
                )));
            assert!(!labels
                .iter()
                .any(|label| label.contains("private_metadata")));
        }
    }

    #[test]
    fn expanded_raw_manifest_preserves_original_whitespace_and_unicode() {
        let raw = "  {\r\n  \"note\": \"Пример\", \"value\": 1  }\n".as_bytes();
        let before = raw.to_vec();
        for ru in [false, true] {
            let labels = raw_manifest_labels(raw, true, ru);
            assert!(labels.iter().any(|label| label.as_bytes() == raw));
            assert_eq!(raw, before);
        }
    }

    #[test]
    fn raw_manifest_preview_never_substitutes_lossy_utf8() {
        for ru in [false, true] {
            let labels = raw_manifest_labels(b"\xffprivate_metadata", true, ru);
            assert!(labels.iter().any(|label| label
                == tr(
                    ru,
                    "Original manifest is not valid UTF-8; exact text preview is unavailable.",
                    "Исходный манифест не является UTF-8; точный текстовый просмотр недоступен.",
                )));
            assert!(!labels
                .iter()
                .any(|label| label.contains("private_metadata") || label.contains('\u{fffd}')));
        }
    }

    fn pending(state: &mut WindowState) -> mpsc::Sender<FileMessage> {
        let id = state.id();
        let (tx, rx) = mpsc::channel();
        state.pending = Some(PendingFile {
            id,
            receiver: Arc::new(Mutex::new(rx)),
        });
        tx
    }
    #[test]
    fn only_manifest_selection_maps_to_an_archive_directory() {
        assert_eq!(
            backup_directory_from_manifest(std::path::Path::new("backup/manifest.json")),
            Ok(PathBuf::from("backup"))
        );
        assert_eq!(
            backup_directory_from_manifest(std::path::Path::new("manifest.json")),
            Ok(PathBuf::from("."))
        );
        for path in ["backup", "backup/saved-nvm.bin", "backup/MANIFEST.JSON", ""] {
            assert!(backup_directory_from_manifest(std::path::Path::new(path)).is_err());
        }
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn selected_manifest_still_requires_complete_archive_validation() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("manifest.json"), b"{}").unwrap();
        let ctx = egui::Context::default();
        let mut state = WindowState::default();
        start_file(
            &ctx,
            &mut state,
            FileAction::OpenManifest(temp.path().join("manifest.json")),
        );
        let message = state
            .pending
            .as_ref()
            .unwrap()
            .receiver
            .lock()
            .unwrap()
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert!(state.accept(message));
        assert_eq!(
            state.error,
            Some(Problem::Inspection(files::BackupInspectionError::Io))
        );
        assert!(state.source.is_none());
        assert!(state.source_path.is_none());
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn file_action_cannot_replace_a_pending_operation() {
        let ctx = egui::Context::default();
        let mut state = WindowState::default();
        let tx = pending(&mut state);
        let id = state.pending.as_ref().unwrap().id;
        start_file(
            &ctx,
            &mut state,
            FileAction::History(PathBuf::from("unused")),
        );
        assert_eq!(state.pending.as_ref().unwrap().id, id);
        assert!(tx
            .send(FileMessage {
                id,
                result: Ok(None)
            })
            .is_ok());
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn held_file_worker_remains_bounded_across_close_and_reopen() {
        let ctx = egui::Context::default();
        let mut state = WindowState::default();
        let (release, held) = mpsc::channel();
        let (started, entered) = mpsc::channel();
        let id = state.id();
        start_file_worker(&ctx, &mut state, id, move || {
            started.send(()).unwrap();
            held.recv().unwrap();
            Ok(None)
        });
        entered
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        state.close();
        assert!(state.pending.is_none());
        for _ in 0..3 {
            start_file(
                &ctx,
                &mut state,
                FileAction::History(PathBuf::from("unused")),
            );
            assert!(state.pending.is_none());
            assert!(state.file_busy());
            state.close();
        }
        assert!(!state.accept(FileMessage {
            id,
            result: Err(Problem::File)
        }));
        release.send(()).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while state.file_busy() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(!state.file_busy());
        start_file(
            &ctx,
            &mut state,
            FileAction::History(PathBuf::from("unused")),
        );
        let message = state
            .pending
            .as_ref()
            .unwrap()
            .receiver
            .lock()
            .unwrap()
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert!(state.accept(message));
    }

    #[test]
    fn leaving_page_clears_destructive_confirmation_and_path_entry() {
        let ctx = egui::Context::default();
        let state = WindowState {
            delete_confirm: true,
            choosing_path: Some(false),
            path_entry: "private path".into(),
            ..Default::default()
        };
        ctx.data_mut(|d| d.insert_temp(egui::Id::new(STATE_ID), state));
        close_page(&ctx);
        let state = ctx.data(|d| d.get_temp::<WindowState>(egui::Id::new(STATE_ID)).unwrap());
        assert!(!state.delete_confirm);
        assert!(state.choosing_path.is_none());
        assert!(state.path_entry.is_empty());
    }

    #[test]
    fn stale_file_completion_and_closed_window_cannot_replace_current_state() {
        let mut state = WindowState::default();
        let tx = pending(&mut state);
        assert!(!state.accept(FileMessage {
            id: 0,
            result: Err(Problem::File)
        }));
        assert!(state.error.is_none());
        state.close();
        assert!(tx
            .send(FileMessage {
                id: 1,
                result: Err(Problem::File)
            })
            .is_err());
        assert!(!state.accept(FileMessage {
            id: 1,
            result: Err(Problem::File)
        }));
        let _new = pending(&mut state);
        assert!(state.accept(FileMessage {
            id: 3,
            result: Ok(None)
        }));
        assert!(state.pending.is_none());
        assert!(state.error.is_none());
    }
    #[test]
    fn capture_results_require_open_window_request_and_connection_generation() {
        let ctx = egui::Context::default();
        ctx.data_mut(|d| {
            d.insert_temp(egui::Id::new(OPEN_ID), true);
            d.insert_temp(
                egui::Id::new(STATE_ID),
                WindowState {
                    capture_pending: Some((7, 5)),
                    ..Default::default()
                },
            );
        });
        for (id, generation) in [(6, 5), (7, 4)] {
            capture_finished(
                &ctx,
                id,
                generation,
                Err(crate::app::vial_hid_task::CaptureFailure::Failed),
            );
        }
        let state = ctx
            .data(|d| d.get_temp::<WindowState>(egui::Id::new(STATE_ID)))
            .unwrap();
        assert_eq!(state.capture_pending, Some((7, 5)));
        assert!(state.error.is_none());
        capture_finished(
            &ctx,
            7,
            5,
            Err(crate::app::vial_hid_task::CaptureFailure::Failed),
        );
        let state = ctx
            .data(|d| d.get_temp::<WindowState>(egui::Id::new(STATE_ID)))
            .unwrap();
        assert_eq!(
            state.capture_error,
            Some(crate::app::vial_hid_task::CaptureFailure::Failed)
        );
        assert!(state.capture_pending.is_none());
    }
    #[test]
    fn navigation_and_failed_capture_preserve_the_previous_preview() {
        use crate::app::vial_hid_task::CaptureFailure;
        let ctx = egui::Context::default();
        let previous = Preview {
            source_profile: Some(ProfileId::Main),
            target_profile: ProfileId::Main,
            groups: vec![],
            changed_bytes: 1,
            commands: 1,
        };
        let mut state = WindowState {
            preview: Some(previous.clone()),
            capture_pending: Some((3, 7)),
            ..Default::default()
        };
        state.close();
        assert_eq!(state.preview, Some(previous.clone()));
        ctx.data_mut(|d| {
            d.insert_temp(egui::Id::new(OPEN_ID), true);
            d.insert_temp(egui::Id::new(STATE_ID), state);
        });
        capture_finished(&ctx, 3, 7, Err(CaptureFailure::Failed));
        let mut state = ctx
            .data(|d| d.get_temp::<WindowState>(egui::Id::new(STATE_ID)))
            .unwrap();
        assert_eq!(state.preview, Some(previous.clone()));
        assert!(state.capture_error.is_none());
        state.capture_pending = Some((4, 7));
        ctx.data_mut(|d| d.insert_temp(egui::Id::new(STATE_ID), state));
        capture_finished(&ctx, 3, 7, Err(CaptureFailure::Failed));
        capture_finished(&ctx, 4, 7, Err(CaptureFailure::OperationBusy));
        let state = ctx
            .data(|d| d.get_temp::<WindowState>(egui::Id::new(STATE_ID)))
            .unwrap();
        assert_eq!(state.preview, Some(previous));
        assert_eq!(state.capture_error, Some(CaptureFailure::OperationBusy));
    }

    #[test]
    fn untrusted_ui_fixture_preserves_bound_objects_until_generation_changes() {
        use crate::app::vial_hid_task::CaptureFailure;
        let ctx = egui::Context::default();
        let plan = Arc::new(crate::ak820_restore_plan::RestorePlan::untrusted_ui_fixture());
        let target = Arc::new(CapturedBackup::untrusted_ui_fixture());
        let state = WindowState {
            source: Some(LoadedBackup::untrusted_ui_fixture()),
            source_path: Some(PathBuf::from("fixture")),
            target: Some(TargetSnapshot {
                data: target.clone(),
                generation: 7,
                read_at: "fixture".into(),
            }),
            preview_plan: Some(plan.clone()),
            preview: Some(Preview {
                source_profile: Some(ProfileId::Main),
                target_profile: ProfileId::Main,
                groups: vec![],
                changed_bytes: 1,
                commands: 1,
            }),
            capture_pending: Some((3, 7)),
            ..Default::default()
        };
        ctx.data_mut(|d| d.insert_temp(egui::Id::new(STATE_ID), state));
        crate::app::ak820_status::leave_service_pages(&ctx);
        let mut state = ctx
            .data(|d| d.get_temp::<WindowState>(egui::Id::new(STATE_ID)))
            .unwrap();
        assert!(Arc::ptr_eq(&state.target.as_ref().unwrap().data, &target));
        assert!(Arc::ptr_eq(state.preview_plan.as_ref().unwrap(), &plan));
        assert!(state.source.is_some());
        let _sender = pending(&mut state);
        let file_id = state.pending.as_ref().unwrap().id;
        assert!(state.accept(FileMessage {
            id: file_id,
            result: Err(Problem::Inspection(files::BackupInspectionError::Image))
        }));
        assert!(state.source.is_some());
        assert!(Arc::ptr_eq(state.preview_plan.as_ref().unwrap(), &plan));
        state.capture_pending = Some((4, 7));
        ctx.data_mut(|d| {
            d.insert_temp(egui::Id::new(OPEN_ID), true);
            d.insert_temp(egui::Id::new(STATE_ID), state);
        });
        capture_finished(&ctx, 3, 7, Err(CaptureFailure::Failed));
        capture_finished(&ctx, 4, 7, Err(CaptureFailure::Cancelled));
        let mut state = ctx
            .data(|d| d.get_temp::<WindowState>(egui::Id::new(STATE_ID)))
            .unwrap();
        assert_eq!(state.capture_error, Some(CaptureFailure::Cancelled));
        assert!(Arc::ptr_eq(state.preview_plan.as_ref().unwrap(), &plan));
        state.invalidate_target(8);
        assert!(state.target_stale);
        assert!(state.preview.is_some());
        assert!(state.preview_plan.is_none());
        assert!(Arc::ptr_eq(&state.target.as_ref().unwrap().data, &target));
        state.capture_pending = Some((5, 8));
        state.invalidate_target(8);
        assert_eq!(state.capture_pending, Some((5, 8)));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn actual_failed_open_and_close_keep_bound_preview() {
        for close in [false, true] {
            let ctx = egui::Context::default();
            let plan = Arc::new(crate::ak820_restore_plan::RestorePlan::untrusted_ui_fixture());
            let target = Arc::new(CapturedBackup::untrusted_ui_fixture());
            let mut state = WindowState {
                source: Some(LoadedBackup::untrusted_ui_fixture()),
                source_path: Some(PathBuf::from("old-fixture")),
                target: Some(TargetSnapshot {
                    data: target.clone(),
                    generation: 7,
                    read_at: "fixture".into(),
                }),
                preview_plan: Some(plan.clone()),
                preview: Some(Preview {
                    source_profile: Some(ProfileId::Main),
                    target_profile: ProfileId::Main,
                    groups: vec![],
                    changed_bytes: 1,
                    commands: 1,
                }),
                ..Default::default()
            };
            let temp = tempfile::tempdir().unwrap();
            let missing = temp.path().join("missing");
            start_file(&ctx, &mut state, FileAction::Open(missing, None));
            assert!(state.source.is_some());
            assert!(Arc::ptr_eq(state.preview_plan.as_ref().unwrap(), &plan));
            assert!(Arc::ptr_eq(&state.target.as_ref().unwrap().data, &target));
            let pending = state.pending.clone().unwrap();
            if close {
                state.close();
                assert!(state.pending.is_none());
            }
            let message = pending
                .receiver
                .lock()
                .unwrap()
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap();
            if close {
                assert!(!state.accept(message));
            } else {
                assert!(state.accept(message));
                assert_eq!(
                    state.error,
                    Some(Problem::Inspection(files::BackupInspectionError::Io))
                );
            }
            assert!(state.source.is_some());
            assert!(state.preview.is_some());
            assert!(Arc::ptr_eq(state.preview_plan.as_ref().unwrap(), &plan));
        }
    }

    #[test]
    fn action_predicates_deny_stale_bound_snapshot() {
        let mut state = WindowState {
            source: Some(LoadedBackup::untrusted_ui_fixture()),
            source_path: Some(PathBuf::from("fixture")),
            root: Some(PathBuf::from("fixture")),
            target: Some(TargetSnapshot {
                data: Arc::new(CapturedBackup::untrusted_ui_fixture()),
                generation: 7,
                read_at: "fixture".into(),
            }),
            preview_plan: Some(Arc::new(
                crate::ak820_restore_plan::RestorePlan::untrusted_ui_fixture(),
            )),
            preview: Some(Preview {
                source_profile: Some(ProfileId::Main),
                target_profile: ProfileId::Main,
                groups: vec![],
                changed_bytes: 1,
                commands: 1,
            }),
            ..Default::default()
        };
        assert_eq!(state.actions(false, true, true), [true; 4]);
        state.invalidate_target(8);
        assert_eq!(state.actions(false, true, true), [false; 4]);
    }

    #[test]
    fn capture_causes_are_rendered_in_both_languages() {
        use crate::app::vial_hid_task::CaptureFailure;
        use egui_kittest::{kittest::Queryable, Harness};
        for (language, ru) in [
            (crate::i18n::Language::English, false),
            (crate::i18n::Language::Russian, true),
        ] {
            for cause in [
                CaptureFailure::OperationBusy,
                CaptureFailure::LeaseBusy,
                CaptureFailure::Disconnected,
                CaptureFailure::Cancelled,
                CaptureFailure::Storage,
                CaptureFailure::Reconciliation,
                CaptureFailure::Failed,
            ] {
                let cc = eframe::CreationContext::_new_kittest(egui::Context::default());
                let mut app = EntropyApp::new_ak820_demo(&cc);
                app.app_settings.language = language;
                let mut harness = Harness::builder()
                    .with_size(egui::vec2(800.0, 900.0))
                    .build_ui_state(
                        move |ui, app: &mut EntropyApp| {
                            ui.ctx().data_mut(|d| {
                                if d.get_temp::<WindowState>(egui::Id::new(STATE_ID)).is_none() {
                                    d.insert_temp(egui::Id::new(OPEN_ID), true);
                                    d.insert_temp(
                                        egui::Id::new(STATE_ID),
                                        WindowState {
                                            capture_error: Some(cause),
                                            ..Default::default()
                                        },
                                    );
                                }
                            });
                            show(app, ui);
                        },
                        app,
                    );
                harness.run();
                assert!(harness
                    .query_by_label(capture_problem_text(cause, ru))
                    .is_some());
            }
        }
    }
    #[test]
    fn stale_snapshot_is_visible_and_comparison_is_disabled() {
        use egui_kittest::{
            kittest::{NodeT, Queryable},
            Harness,
        };
        for (language, ru) in [
            (crate::i18n::Language::English, false),
            (crate::i18n::Language::Russian, true),
        ] {
            let cc = eframe::CreationContext::_new_kittest(egui::Context::default());
            let mut app = EntropyApp::new_ak820_demo(&cc);
            app.app_settings.language = language;
            let mut harness = Harness::builder().with_size(egui::vec2(900.0, 1200.0)).build_ui_state(|ui, app: &mut EntropyApp| {
                ui.ctx().data_mut(|d| {
                    if d.get_temp::<WindowState>(egui::Id::new(STATE_ID)).is_none() {
                        d.insert_temp(egui::Id::new(OPEN_ID), true);
                        d.insert_temp(egui::Id::new(STATE_ID), WindowState {
                            source: Some(LoadedBackup::untrusted_ui_fixture()),
                            target: Some(TargetSnapshot { data: Arc::new(CapturedBackup::untrusted_ui_fixture()), generation: u64::MAX, read_at: "fixture".into() }),
                            preview_plan: Some(Arc::new(crate::ak820_restore_plan::RestorePlan::untrusted_ui_fixture())),
                            ..Default::default()
                        });
                    }
                });
                show(app, ui);
            }, app);
            harness.run();
            assert!(harness
                .query_by_label(tr(
                    ru,
                    "This snapshot is stale. Read current settings again before using it.",
                    "Этот снимок устарел. Перед использованием снова прочитайте текущие настройки."
                ))
                .is_some());
            assert!(harness
                .get_by_label(tr(
                    ru,
                    "Preview restore to this snapshot",
                    "Предпросмотр восстановления в этот снимок"
                ))
                .accesskit_node()
                .is_disabled());
        }
    }
    #[test]
    fn restore_failure_keeps_only_the_matching_operations_safety_backup() {
        let ctx = egui::Context::default();
        ctx.data_mut(|d| {
            d.insert_temp(egui::Id::new(OPEN_ID), true);
            d.insert_temp(
                egui::Id::new(STATE_ID),
                WindowState {
                    restore_pending: Some((10, 4)),
                    ..Default::default()
                },
            );
        });
        let location = BackupLocation {
            path: PathBuf::from("private-test-folder/before-restore"),
            manifest_sha256: "a".repeat(64),
        };
        restore_failed(&ctx, 10, 3, Some(location.clone()));
        let state = ctx
            .data(|d| d.get_temp::<WindowState>(egui::Id::new(STATE_ID)))
            .unwrap();
        assert!(state.undo_backup.is_none());
        assert_eq!(state.restore_pending, Some((10, 4)));
        restore_failed(&ctx, 10, 4, Some(location.clone()));
        let mut state = ctx
            .data(|d| d.get_temp::<WindowState>(egui::Id::new(STATE_ID)))
            .unwrap();
        assert_eq!(
            state.undo_backup.as_ref().unwrap().manifest_sha256,
            location.manifest_sha256
        );
        assert_eq!(state.error, Some(Problem::Restore));
        assert!(state.restore_pending.is_none());
        state.close();
        assert!(
            state.undo_backup.is_some(),
            "closing must not discard the undo reference"
        );
        ctx.data_mut(|d| d.insert_temp(egui::Id::new(STATE_ID), state));
        restore_failed(&ctx, 10, 4, None);
        let state = ctx
            .data(|d| d.get_temp::<WindowState>(egui::Id::new(STATE_ID)))
            .unwrap();
        assert!(
            state.error.is_none(),
            "late results cannot reopen an ended operation"
        );
    }

    #[test]
    fn closed_page_keeps_restore_token_for_safety_result() {
        let ctx = egui::Context::default();
        let mut state = WindowState {
            restore_pending: Some((12, 4)),
            ..Default::default()
        };
        state.close();
        ctx.data_mut(|d| d.insert_temp(egui::Id::new(STATE_ID), state));
        let safety = BackupLocation {
            path: PathBuf::from("private-test-folder/safety"),
            manifest_sha256: "c".repeat(64),
        };
        restore_failed(&ctx, 12, 4, Some(safety));
        let state = ctx
            .data(|d| d.get_temp::<WindowState>(egui::Id::new(STATE_ID)))
            .unwrap();
        assert!(state.restore_pending.is_none());
        assert!(state.undo_backup.is_some());
        assert_eq!(state.error, Some(Problem::Restore));
    }

    #[test]
    fn stale_restore_result_retains_safety_after_reconnect_without_overriding_new_token() {
        let ctx = egui::Context::default();
        let mut state = WindowState {
            restore_pending: Some((12, 4)),
            ..Default::default()
        };
        state.close();
        state.invalidate_target(5);
        ctx.data_mut(|d| d.insert_temp(egui::Id::new(STATE_ID), state));
        let safety = BackupLocation {
            path: PathBuf::from("private-test-folder/safety"),
            manifest_sha256: "d".repeat(64),
        };
        restore_failed(&ctx, 12, 4, Some(safety.clone()));
        let mut state = ctx
            .data(|d| d.get_temp::<WindowState>(egui::Id::new(STATE_ID)))
            .unwrap();
        assert!(state.undo_backup.is_some());
        assert!(state.restore_pending.is_none());
        assert!(!state.restore_verified);
        state.restore_pending = Some((13, 5));
        ctx.data_mut(|d| d.insert_temp(egui::Id::new(STATE_ID), state));
        restore_failed(&ctx, 12, 4, Some(safety));
        let state = ctx
            .data(|d| d.get_temp::<WindowState>(egui::Id::new(STATE_ID)))
            .unwrap();
        assert_eq!(state.restore_pending, Some((13, 5)));
        assert!(!state.restore_verified);
    }

    #[test]
    fn reconnect_invalidates_pending_restore_without_losing_existing_safety_copy() {
        let mut state = WindowState {
            restore_pending: Some((11, 4)),
            undo_backup: Some(BackupLocation {
                path: PathBuf::from("private-test-folder/safety"),
                manifest_sha256: "b".repeat(64),
            }),
            ..Default::default()
        };
        state.invalidate_target(5);
        assert_eq!(state.restore_pending, Some((11, 4)));
        assert!(state.preview_plan.is_none());
        assert!(state.undo_backup.is_some());
        assert_eq!(state.error, Some(Problem::Changed));
    }
    #[test]
    fn reconnect_invalidates_inflight_capture_and_restore_preview() {
        let mut state = WindowState {
            capture_pending: Some((9, 3)),
            preview: Some(Preview {
                source_profile: Some(ProfileId::Main),
                target_profile: ProfileId::Main,
                groups: vec![],
                changed_bytes: 0,
                commands: 0,
            }),
            ..Default::default()
        };
        state.invalidate_target(4);
        assert!(state.capture_pending.is_none());
        assert!(state.preview.is_some());
        assert!(state.target_stale);
        assert_eq!(state.error, Some(Problem::Changed));
    }
    #[test]
    fn local_errors_and_folder_labels_do_not_echo_private_diagnostics() {
        for p in [
            Problem::File,
            Problem::Inspection(files::BackupInspectionError::Invalid),
            Problem::Changed,
            Problem::Incompatible,
            Problem::Restore,
        ] {
            assert_ne!(problem_text(p, true), problem_text(p, false));
            assert!(!problem_text(p, true).contains("/home/"));
        }
        assert_eq!(short_text("a\n\t\u{0}b"), "ab");
        assert_eq!(short_text(&"x".repeat(10000)).len(), 96);
    }
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn backup_folder_can_be_selected_through_inline_browser() {
        use egui_kittest::{kittest::Queryable, Harness};
        let cc = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = EntropyApp::new_ak820_demo(&cc);
        app.app_settings.language = crate::i18n::Language::English;
        let mut harness = Harness::builder()
            .with_size(egui::vec2(800.0, 700.0))
            .build_ui_state(
                |ui, state: &mut (EntropyApp, bool, Option<PathBuf>)| {
                    if !state.1 {
                        ui.ctx()
                            .data_mut(|d| d.insert_temp(egui::Id::new(OPEN_ID), true));
                        state.1 = true;
                    }
                    show(&mut state.0, ui);
                    state.2 = ui
                        .ctx()
                        .data(|d| d.get_temp::<WindowState>(egui::Id::new(STATE_ID)))
                        .and_then(|s| s.root);
                },
                (app, false, None),
            );
        harness.run();
        harness.get_by_label("Choose backup folder…").click();
        harness.run();
        harness.get_by_label("Browse…").click();
        harness.run();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while harness.query_by_label("Select this directory").is_none()
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(10));
            harness.run();
        }
        harness.get_by_label("Select this directory").click();
        harness.step();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while harness.state().2.is_none() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
            harness.step();
        }
        assert_eq!(
            harness.state().2.as_deref(),
            Some(std::fs::canonicalize(".").unwrap().as_path())
        );
        assert!(harness.state().0.hid_device.is_none());
    }

    #[test]
    fn full_shell_keeps_backup_page_state_in_root_viewport_after_reopen() {
        use egui_kittest::{kittest::Queryable, Harness};
        for (language, backups, heading, folder, open, choose, editor) in [
            (
                crate::i18n::Language::English,
                "Backups and restore…",
                "AK820 backups",
                "Folder: b09-fixture-folder",
                "Open backup…",
                "Choose backup folder…",
                "Editor",
            ),
            (
                crate::i18n::Language::Russian,
                "Резервные копии…",
                "Резервные копии AK820",
                "Папка: b09-fixture-folder",
                "Открыть копию…",
                "Папка для копий…",
                "Редактор",
            ),
        ] {
            let cc = eframe::CreationContext::_new_kittest(egui::Context::default());
            let mut app = EntropyApp::new_ak820_demo(&cc);
            app.app_settings.language = language;
            let frame = eframe::Frame::_new_kittest();
            let mut harness = Harness::builder()
                .with_size(egui::vec2(1400.0, 900.0))
                .build_ui_state(
                    |ui, state: &mut (EntropyApp, eframe::Frame, Option<egui::Context>)| {
                        state.2 = Some(ui.ctx().clone());
                        eframe::App::ui(&mut state.0, ui, &mut state.1);
                    },
                    (app, frame, None),
                );
            harness.run();
            let ctx = harness.state().2.as_ref().unwrap().clone();
            assert_eq!(ctx.viewport_id(), egui::ViewportId::ROOT);
            harness.get_by_label(backups).click();
            harness.run();
            harness.get_by_label(heading);
            assert!(harness
                .output()
                .viewport_output
                .keys()
                .all(|id| *id == egui::ViewportId::ROOT));
            ctx.data_mut(|d| {
                let mut state = d
                    .get_temp::<WindowState>(egui::Id::new(STATE_ID))
                    .unwrap_or_default();
                state.root = Some(PathBuf::from("b09-fixture-folder"));
                d.insert_temp(egui::Id::new(STATE_ID), state);
            });
            harness.run();
            harness.get_by_label(folder);
            harness.get_by_label(open).focus();
            harness.run();
            harness.key_press(egui::Key::Tab);
            harness.run();
            assert!(harness.get_by_label(choose).is_focused());
            harness.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::Tab);
            harness.run();
            assert!(harness.get_by_label(open).is_focused());
            harness.get_by_label(editor).click();
            harness.run();
            assert!(harness.query_by_label(heading).is_none());
            harness.get_by_label(backups).click();
            harness.run();
            harness.get_by_label(heading);
            harness.get_by_label(folder);
            assert!(harness
                .output()
                .viewport_output
                .keys()
                .all(|id| *id == egui::ViewportId::ROOT));
            assert_eq!(ctx.viewport_id(), egui::ViewportId::ROOT);
            assert!(harness.state().0.hid_device.is_none());
        }
    }

    #[test]
    fn backup_page_controls_fit_small_viewport_in_both_languages() {
        use egui_kittest::{kittest::Queryable, Harness};
        for (language, heading, help, close) in [
            (
                crate::i18n::Language::Russian,
                "Резервные копии AK820",
                "Справка",
                "Закрыть",
            ),
            (
                crate::i18n::Language::English,
                "AK820 backups",
                "Help",
                "Close",
            ),
        ] {
            let cc = eframe::CreationContext::_new_kittest(egui::Context::default());
            let mut app = EntropyApp::new_ak820_demo(&cc);
            app.app_settings.language = language;
            let mut harness = Harness::builder()
                .with_size(egui::vec2(800.0, 500.0))
                .build_ui_state(
                    |ui, state: &mut (EntropyApp, bool)| {
                        if !state.1 {
                            ui.ctx()
                                .data_mut(|d| d.insert_temp(egui::Id::new(OPEN_ID), true));
                            state.1 = true;
                        }
                        show(&mut state.0, ui);
                    },
                    (app, false),
                );
            harness.run();
            let (steps, denial) = if language == crate::i18n::Language::Russian {
                ("1. Откройте существующую копию → 2. Проверьте источник и совместимость → 3. Изучите условия восстановления", "Для beta.5 восстановление недоступно: запись сохранённых настроек для этого выпуска не разрешена. Проверка копии доступна.")
            } else {
                ("1. Open an existing backup → 2. Inspect source and compatibility → 3. Review restore prerequisites", "For beta.5: restore unavailable because saved-settings writes are not authorized for this release. Inspection remains available.")
            };
            assert!(harness.query_by_label(steps).is_some());
            assert!(harness.query_by_label(denial).is_some());
            let (open, cancel) = if language == crate::i18n::Language::Russian {
                ("Открыть копию…", "Отмена")
            } else {
                ("Open backup…", "Cancel")
            };
            for label in [steps, denial, open] {
                let rect = harness.get_by_label(label).rect();
                assert!(
                    rect.min.x >= 0.0
                        && rect.min.y >= 0.0
                        && rect.max.x <= 800.0
                        && rect.max.y <= 500.0,
                    "{label}: {rect:?}"
                );
            }
            harness.get_by_label(open).click();
            harness.run();
            harness.get_by_label(cancel).click();
            harness.run();
            assert!(harness.query_by_label(cancel).is_none());
            assert!(harness.state().0.hid_device.is_none());
            for label in [heading, help] {
                let rect = harness.get_by_label(label).rect();
                assert!(
                    rect.min.x >= 0.0
                        && rect.min.y >= 0.0
                        && rect.max.x <= 800.0
                        && rect.max.y <= 500.0,
                    "{label}: {rect:?}"
                );
            }
            harness.get_by_label(close).scroll_to_me();
            harness.run();
            let rect = harness.get_by_label(close).rect();
            assert!(
                rect.min.y >= 0.0 && rect.max.y <= 500.0,
                "{close}: {rect:?}"
            );
            harness.get_by_label(close).click();
            harness.run();
            assert!(harness.query_by_label(heading).is_none());
            assert!(harness.state().0.hid_device.is_none());
        }
    }
}
