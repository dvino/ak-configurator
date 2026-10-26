//! Complete local backups and explicitly confirmed, verified settings restore.
use super::ak820_restore_request::{BackupLocation, PreparedRestore, RestoreCompleted};
use crate::ak820_backup_capture::CapturedBackup;
use crate::ak820_backup_files::{self as files, BackupHistory, LoadedBackup};
use crate::app::ak820_beta1_profiles::{self as beta, ProfileId};
use crate::app::EntropyApp;
use std::path::PathBuf;
use std::sync::{mpsc, Arc, Mutex};

pub(crate) const OPEN_ID: &str = "ak820_backup_inspection_open";
const STATE_ID: &str = "ak820_backup_inspection_state";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Problem {
    File,
    Capture,
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
        (Problem::Capture, true) => "Чтение остановлено. Проверьте подключение и прочитайте настройки снова. Новая копия не сохранена.",
        (Problem::Capture, false) => "Reading stopped. Check the connection and read settings again. No new backup was saved.",
        (Problem::Changed, true) => "Подключение изменилось. Для сравнения нужна новая копия текущих настроек.",
        (Problem::Changed, false) => "The connection changed. Read a new snapshot of the current settings before comparing.",
        (Problem::Incompatible, true) => "Восстановление в выбранную прошивку недоступно: переход или сохранённые значения не поддерживаются. Настройки не изменены.",
        (Problem::Incompatible, false) => "Restore to the selected firmware is unavailable: the transition or stored values are unsupported. Settings have not changed.",
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
    history: Option<Arc<BackupHistory>>,
    pending: Option<PendingFile>,
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
}
impl WindowState {
    fn id(&mut self) -> u64 {
        self.next_id = self.next_id.wrapping_add(1);
        self.next_id
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
        if self
            .target
            .as_ref()
            .is_some_and(|t| t.generation != generation)
            || self.capture_pending.is_some_and(|(_, g)| g != generation)
            || self.restore_pending.is_some_and(|(_, g)| g != generation)
        {
            self.target = None;
            self.capture_pending = None;
            self.restore_pending = None;
            self.preview_plan = None;
            self.preview = None;
            self.error = Some(Problem::Changed);
        }
    }
    fn close(&mut self) {
        self.reset_preview = false;
        self.delete_confirm = false;
        self.id();
        self.pending = None;
        self.capture_pending = None;
        self.source = None;
        self.source_path = None;
        self.preview_plan = None;
        self.restore_pending = None;
        self.target = None;
        self.preview = None;
        self.saved_name = None;
        self.error = None;
    }
}

/// Vial workers already fence connection generations. The window token also
/// rejects completion after cancel, close/reopen or a replacement request.
pub(super) fn capture_finished(
    ctx: &egui::Context,
    id: u64,
    generation: u64,
    result: Result<CapturedBackup, ()>,
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
        state.preview = None;
        state.preview_plan = None;
        match result {
            Ok(snapshot) => {
                state.target = Some(TargetSnapshot {
                    data: Arc::new(snapshot),
                    generation,
                    read_at: chrono::Local::now().format("%H:%M:%S").to_string(),
                });
                state.saved_name = None;
                state.error = None;
            }
            Err(()) => {
                state.target = None;
                state.error = Some(Problem::Capture);
            }
        }
        d.insert_temp(egui::Id::new(STATE_ID), state);
    });
    ctx.request_repaint();
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

enum FileAction {
    Delete {
        path: PathBuf,
        manifest_sha256: String,
    },
    Open(Option<(PathBuf, String)>),
    History(Option<PathBuf>),
    Save {
        root: PathBuf,
        snapshot: Arc<CapturedBackup>,
    },
}
#[cfg(not(target_arch = "wasm32"))]
fn start_file(ctx: &egui::Context, state: &mut WindowState, action: FileAction) {
    let id = state.id();
    state.error = None;
    if matches!(action, FileAction::Open(_)) {
        state.source = None;
        state.source_path = None;
        state.preview_plan = None;
        state.preview = None;
    }
    if matches!(action, FileAction::History(_)) {
        state.history = None;
    }
    let (tx, rx) = mpsc::channel();
    state.pending = Some(PendingFile {
        id,
        receiver: Arc::new(Mutex::new(rx)),
    });
    let repaint = ctx.clone();
    std::thread::spawn(move || {
        let result = (|| -> Result<Option<FilePayload>, Problem> {
            match action {
                FileAction::Delete {
                    path,
                    manifest_sha256,
                } => {
                    files::delete_validated_archive(&path, &manifest_sha256)
                        .map_err(|_| Problem::File)?;
                    Ok(Some(FilePayload::Deleted { path }))
                }
                FileAction::Open(selected) => {
                    let (path, pin) = match selected {
                        Some((path, pin)) => (path, Some(pin)),
                        None => match rfd::FileDialog::new().pick_folder() {
                            Some(path) => (path, None),
                            None => return Ok(None),
                        },
                    };
                    files::read_backup_auto(&path, pin.as_deref())
                        .map(|source| Some(FilePayload::Source { path, source }))
                        .map_err(|_| Problem::File)
                }
                FileAction::History(selected) => {
                    let root = match selected.or_else(|| rfd::FileDialog::new().pick_folder()) {
                        Some(path) => path,
                        None => return Ok(None),
                    };
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
        })();
        let _ = tx.send(FileMessage { id, result });
        repaint.request_repaint();
    });
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

pub(crate) fn show(app: &mut EntropyApp, ctx: &egui::Context) {
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
    let busy = state.pending.is_some()
        || state.capture_pending.is_some()
        || state.restore_pending.is_some();
    let mut file_action = None;
    let mut capture = false;
    let mut cancel = false;
    let mut compare = false;
    let mut reset = false;
    let mut restore = false;
    let mut close = false;
    let mut open = true;
    egui::Window::new(tr(ru, "AK820 backups", "Резервные копии AK820"))
        .id(egui::Id::new("ak820_backup_inspection_window"))
        .open(&mut open).resizable(true).default_width(610.0)
        .max_width((ctx.content_rect().width() - 32.0).max(260.0))
        .max_height((ctx.content_rect().height() - 64.0).max(100.0)).vscroll(true)
        .show(ctx, |ui| {
            ui.label(tr(ru, "Complete saved settings and firmware evidence. ROM and macros stay in the chosen local folder.", "Полные сохранённые настройки и данные для проверки прошивки. ROM и макросы остаются в выбранной локальной папке."));
            if !cfg!(any(target_os = "linux", target_os = "macos", target_os = "windows")) {
                ui.label(tr(ru, "Backup folder access is unavailable on this platform.", "Работа с папками копий недоступна на этой платформе."));
            }
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(!busy && cfg!(any(target_os = "linux", target_os = "macos", target_os = "windows")), egui::Button::new(tr(ru, "Open backup…", "Открыть копию…"))).clicked() { file_action = Some(FileAction::Open(None)); }
                if ui.add_enabled(!busy && cfg!(any(target_os = "linux", target_os = "macos", target_os = "windows")), egui::Button::new(tr(ru, "Choose backup folder…", "Папка для копий…"))).clicked() { file_action = Some(FileAction::History(None)); }
            });
            if let Some(root) = &state.root {
                let name = root.file_name().unwrap_or_else(|| root.as_os_str()).to_string_lossy();
                ui.label(format!("{}: {}", tr(ru, "Folder", "Папка"), short_text(&name)));
                ui.label(tr(ru, "Backup folder selected. New copies are saved to separate dated folders.", "Папка выбрана. Новые копии сохраняются в отдельных папках с датой."));
                if ui.add_enabled(!busy, egui::Button::new(tr(ru, "Refresh list", "Обновить список"))).clicked() { file_action = Some(FileAction::History(state.root.clone())); }
            }
            if let Some(history) = &state.history {
                ui.collapsing(format!("{} ({})", tr(ru, "Saved backups", "Сохранённые копии"), history.entries.len()), |ui| {
                    if history.entries.is_empty() { ui.label(tr(ru, "No validated backups in this folder.", "В этой папке нет проверенных копий.")); }
                    for entry in &history.entries {
                        let label = format!("{} · {} · {}", short_text(&entry.created_utc), profile_label(entry.source_profile, ru), short_text(&entry.name));
                        if ui.add_enabled(!busy, egui::Button::new(label).wrap()).clicked() {
                            file_action = Some(FileAction::Open(Some((entry.path.clone(), entry.manifest_sha256.clone()))));
                        }
                    }
                });
                if history.invalid_skipped > 0 {
                    ui.label(format!("{}: {}", tr(ru, "Folders skipped because validation failed", "Папок не прошли проверку"), history.invalid_skipped));
                }
            }
            ui.separator();
            ui.strong(tr(ru, "Current keyboard", "Текущая клавиатура"));
            #[cfg(not(target_arch = "wasm32"))]
            let can_capture = app.can_capture_ak820_backup();
            #[cfg(target_arch = "wasm32")]
            let can_capture = false;
            if ui.add_enabled(!busy && can_capture, egui::Button::new(tr(ru, "Read complete snapshot", "Прочитать полный снимок"))).clicked() { capture = true; }
            if app.ak820_demo {
                ui.label(tr(ru, "Demo has no physical settings snapshot. Connect a supported keyboard to create a complete backup.", "В деморежиме нет снимка физической клавиатуры. Для полной копии подключите поддерживаемую клавиатуру."));
            } else if !can_capture && state.capture_pending.is_none() {
                ui.label(tr(ru, "A verified beta.2 connection with no pending edits is required.", "Нужно проверенное подключение beta.2 без ожидающих изменений."));
            }
            if let Some((id, _)) = state.capture_pending {
                #[cfg(not(target_arch = "wasm32"))]
                let progress = app.ak820_backup_capture_progress(id).unwrap_or(0.0);
                #[cfg(target_arch = "wasm32")]
                let progress = 0.0;
                ui.add(egui::ProgressBar::new(progress).show_percentage().text(tr(ru, "Reading and checking…", "Чтение и проверка…")));
                if ui.button(tr(ru, "Cancel reading", "Отменить чтение")).clicked() { cancel = true; }
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
            if let Some(target) = &state.target {
                ui.label(format!("{} · {}", profile_label(Some(target.data.profile()), ru), target.read_at));
                ui.label(tr(ru, "64 KiB ROM and 10 KiB saved settings checked. This snapshot reflects the time it was read.", "Проверены 64 КиБ ROM и 10 КиБ сохранённых настроек. Снимок соответствует моменту чтения."));
                if let Some(name) = &state.saved_name { ui.label(format!("{}: {}", tr(ru, "Saved and rechecked", "Сохранено и проверено повторно"), short_text(name))); }
                else { ui.label(tr(ru, "Snapshot is in memory; no backup file has been saved yet.", "Снимок в памяти; файл резервной копии ещё не сохранён.")); }
                if ui.add_enabled(!busy && state.root.is_some() && cfg!(any(target_os = "linux", target_os = "macos", target_os = "windows")), egui::Button::new(tr(ru, "Save new backup", "Сохранить новую копию"))).clicked() {
                    file_action = Some(FileAction::Save { root: state.root.clone().unwrap(), snapshot: target.data.clone() });
                }
                if ui.add_enabled(!busy && can_capture, egui::Button::new(tr(ru, "Preview reset to release defaults…", "Предпросмотр сброса к настройкам выпуска…"))).clicked() { reset = true; }
            }
            if state.restore_verified {
                ui.colored_label(egui::Color32::LIGHT_GREEN, tr(ru, "Restore verified. The editor reads the current settings again.", "Восстановление проверено. Редактор повторно читает текущие настройки."));
            }
            if let Some(location) = &state.undo_backup {
                if ui.add_enabled(!busy, egui::Button::new(tr(ru, "Open safety backup to undo restore", "Открыть защитную копию для отмены восстановления"))).clicked() {
                    file_action = Some(FileAction::Open(Some((location.path.clone(), location.manifest_sha256.clone()))));
                }
                ui.label(tr(ru, "Undo uses the same preview, fresh confirmation and verified restore workflow.", "Для отмены используются тот же предпросмотр, новое подтверждение и проверяемое восстановление."));
            }
            if let Some(source) = &state.source {
                ui.separator(); draw_source(ui, source, ru);
                if ui.add_enabled(!busy && state.target.is_some(), egui::Button::new(tr(ru, "Preview restore to this snapshot", "Предпросмотр восстановления в этот снимок"))).clicked() { compare = true; }
                if ui.add_enabled(!busy && state.source_path.is_some(), egui::Button::new(tr(ru, "Delete this backup…", "Удалить эту копию…"))).clicked() { state.delete_confirm = true; }
                if state.delete_confirm {
                    let name = state.source_path.as_ref().and_then(|p| p.file_name()).map(|p| p.to_string_lossy()).unwrap_or_default();
                    ui.label(format!("{}: {}", tr(ru, "Permanently delete all files of this validated backup", "Безвозвратно удалить все файлы этой проверенной копии"), short_text(&name)));
                    ui.horizontal_wrapped(|ui| {
                        if ui.add_enabled(!busy, egui::Button::new(tr(ru, "Delete permanently", "Удалить безвозвратно"))).clicked() {
                            if let Some(path) = &state.source_path { file_action = Some(FileAction::Delete { path:path.clone(), manifest_sha256:source.archive().manifest_sha256.clone() }); }
                            state.delete_confirm = false;
                        }
                        if ui.button(tr(ru, "Keep backup", "Сохранить копию")).clicked() { state.delete_confirm = false; }
                    });
                }
            }
            if let Some(value) = &state.preview {
                ui.separator(); draw_preview(ui, value, ru);
                if state.reset_preview { ui.label(tr(ru, "Reset supported keys, encoder mappings, macros, lighting and enabled modules. Inactive and inaccessible data remain intact. A verified safety backup is required before fresh confirmation.", "Сбрасываются поддерживаемые назначения клавиш и энкодера, макросы, подсветка и включённые модули. Неактивные и недоступные данные сохраняются. Перед новым подтверждением обязательна проверенная защитная копия.")); }
                let ready = !busy && can_capture && value.commands > 0 && state.preview_plan.is_some() && (state.reset_preview || state.source_path.is_some()) && state.root.is_some();
                let label = if state.reset_preview { tr(ru, "Reset these settings…", "Сбросить эти настройки…") } else { tr(ru, "Restore these settings…", "Восстановить эти настройки…") };
                if ui.add_enabled(ready, egui::Button::new(label)).clicked() { restore = true; }
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
                if ui.button(tr(ru, "Stop restore", "Остановить восстановление")).clicked() { cancel = true; }
                ui.label(tr(ru, "Stopping leaves already written changes in place. The safety copy remains available.", "При остановке уже записанные изменения сохраняются. Защитная копия остаётся доступной."));
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
            if let Some(error) = state.error { ui.colored_label(egui::Color32::LIGHT_RED, problem_text(error, ru)); }
            if state.pending.is_some() { ui.spinner(); ui.label(tr(ru, "Checking local files…", "Проверка локальных файлов…")); }
            if ui.button(tr(ru, "Close", "Закрыть")).clicked() { close = true; }
        });
    if compare {
        state.reset_preview = false;
        if let (Some(source), Some(target)) = (&state.source, &state.target) {
            match preview(source, &target.data) {
                Ok((value, plan)) => {
                    state.preview = Some(value);
                    state.preview_plan = Some(plan);
                    state.error = None;
                }
                Err(error) => {
                    state.preview = None;
                    state.preview_plan = None;
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
                    state.preview = None;
                    state.preview_plan = None;
                    state.error = Some(Problem::Incompatible);
                }
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    if capture {
        let id = state.id();
        state.target = None;
        state.preview = None;
        state.preview_plan = None;
        state.restore_verified = false;
        state.saved_name = None;
        state.error = None;
        if app.start_ak820_backup_capture(ctx, id)
            == super::vial_hid_task::VialHidTaskStart::Started
        {
            state.capture_pending = Some((id, app.connection_generation));
        } else {
            state.error = Some(Problem::Capture);
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    if restore {
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
    if cancel || close || !open {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some((id, _)) = state.capture_pending {
            app.cancel_ak820_backup_capture(id);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some((id, _)) = state.restore_pending {
            app.cancel_ak820_restore(id);
        }
        // Keep the restore request token until the worker reports the actual
        // outcome. Close discards it only after signalling cancellation.
        state.capture_pending = None;
        state.target = None;
        state.preview = None;
        state.preview_plan = None;
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(action) = file_action {
        start_file(ctx, &mut state, action);
    }
    #[cfg(target_arch = "wasm32")]
    let _ = (file_action, capture, restore);
    if close || !open {
        state.close();
        ctx.data_mut(|d| d.insert_temp(egui::Id::new(OPEN_ID), false));
    }
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(STATE_ID), state));
}

#[cfg(test)]
mod tests {
    use super::*;
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
            capture_finished(&ctx, id, generation, Err(()));
        }
        let state = ctx
            .data(|d| d.get_temp::<WindowState>(egui::Id::new(STATE_ID)))
            .unwrap();
        assert_eq!(state.capture_pending, Some((7, 5)));
        assert!(state.error.is_none());
        capture_finished(&ctx, 7, 5, Err(()));
        let state = ctx
            .data(|d| d.get_temp::<WindowState>(egui::Id::new(STATE_ID)))
            .unwrap();
        assert_eq!(state.error, Some(Problem::Capture));
        assert!(state.capture_pending.is_none());
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
        assert!(state.restore_pending.is_none());
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
        assert!(state.preview.is_none());
        assert_eq!(state.error, Some(Problem::Changed));
    }
    #[test]
    fn local_errors_and_folder_labels_do_not_echo_private_diagnostics() {
        for p in [
            Problem::File,
            Problem::Capture,
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
    fn backup_window_fits_small_viewport_without_hid_in_both_languages() {
        for language in [
            crate::i18n::Language::Russian,
            crate::i18n::Language::English,
        ] {
            let ctx = egui::Context::default();
            let cc = eframe::CreationContext::_new_kittest(ctx.clone());
            let mut app = EntropyApp::new_ak820_demo(&cc);
            app.app_settings.language = language;
            ctx.data_mut(|d| {
                d.insert_temp(egui::Id::new(OPEN_ID), true);
                d.insert_temp(
                    egui::Id::new(STATE_ID),
                    WindowState {
                        error: Some(Problem::Capture),
                        ..Default::default()
                    },
                );
            });
            for _ in 0..4 {
                let _ = ctx.run(
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
                .memory(|m| m.area_rect(egui::Id::new("ak820_backup_inspection_window")))
                .unwrap();
            assert!(rect.width() <= 800.0 && rect.height() <= 500.0, "{rect:?}");
            assert!(app.hid_device.is_none());
        }
    }
}
