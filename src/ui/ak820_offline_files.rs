//! Local, demo-only project portability. No device session or backup archive.
use super::{Ak820OfflineProfileProject, EntropyApp};
use std::io::{Read, Write};
use std::path::Path;

const MAX_PROJECT_BYTES: u64 = 2 * 1024 * 1024;
const WINDOW_ID: &str = "ak820_local_project_open";

pub(crate) fn open(ctx: &egui::Context) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(WINDOW_ID), true));
}
pub(crate) fn close(ctx: &egui::Context) {
    if is_busy(ctx) {
        return;
    }
    ctx.data_mut(|d| {
        d.insert_temp(egui::Id::new(WINDOW_ID), false);
        #[cfg(not(target_arch = "wasm32"))]
        {
            d.remove::<PathEntry>(egui::Id::new("ak820_local_project_path_entry"));
            d.remove::<VialFlow>(egui::Id::new("ak820_vial_flow"));
        }
    });
}
pub(crate) fn is_open(ctx: &egui::Context) -> bool {
    ctx.data(|d| {
        d.get_temp::<bool>(egui::Id::new(WINDOW_ID))
            .unwrap_or(false)
    })
}

impl EntropyApp {
    pub(crate) fn snapshot_current_ak820_project(
        &self,
    ) -> Result<Ak820OfflineProfileProject, String> {
        if !self.ak820_demo {
            return Err("Offline projects require demo mode".into());
        }
        Ok(Ak820OfflineProfileProject {
            profile: self.ak820_demo_profile.ok_or("Select an offline profile")?,
            layout: self.layout.clone().ok_or("Offline layout is unavailable")?,
            macro_texts: self.keycode_picker.macro_texts.clone(),
            macro_actions: self.keycode_picker.macro_actions.clone(),
            macro_names: self.keycode_picker.macro_names.clone(),
            macro_descriptions: self.keycode_picker.macro_descriptions.clone(),
            combo_entries: self.combo_entries.clone(),
            combo_names: self.combo_names.clone(),
            combo_colors: self.combo_colors.clone(),
            combo_term: self.combo_term,
            layer_names: self.layer_names.clone(),
            rgb_settings: self.rgb_settings.clone(),
            extensions: self.ak820_offline_extensions.clone(),
        })
    }

    pub(crate) fn import_ak820_project_bytes(&mut self, raw: &[u8]) -> Result<(), String> {
        // All parsing and validation complete before either active or cached
        // state changes. Geometry is the same canonical bundled AK820 template.
        let current = self.snapshot_current_ak820_project()?;
        let profile = crate::ak820_offline_project::profile_id(raw)?;
        let mut template = current.clone();
        template.profile = profile;
        let imported = crate::ak820_offline_project::import_project(raw, template)?;
        // Undo must restore the complete cache as well as the active project:
        // an import can replace a previously edited target profile, or create
        // a profile that did not exist in the cache before.
        let previous_target = self
            .ak820_demo_projects
            .iter()
            .find(|p| p.profile == profile)
            .cloned();
        self.ak820_offline_import_undo = Some((current.clone(), profile, previous_target));
        store_project(&mut self.ak820_demo_projects, current);
        store_project(&mut self.ak820_demo_projects, imported);
        self.ak820_demo_profile = None;
        self.configure_ak820_offline_profile(profile);
        Ok(())
    }
    pub(crate) fn undo_ak820_project_import(&mut self) -> bool {
        if !self.ak820_demo {
            return false;
        }
        let Some((previous, imported_profile, previous_target)) =
            self.ak820_offline_import_undo.take()
        else {
            return false;
        };
        // Preserve edits made to another profile after the import. The active
        // profile may have changed since import, so save its current state
        // before restoring only the target that the import replaced.
        if let Ok(current) = self.snapshot_current_ak820_project() {
            if current.profile != imported_profile {
                store_project(&mut self.ak820_demo_projects, current);
            }
        }
        if let Some(previous_target) = previous_target {
            store_project(&mut self.ak820_demo_projects, previous_target);
        } else {
            self.ak820_demo_projects
                .retain(|p| p.profile != imported_profile);
        }
        let profile = previous.profile;
        // Cross-profile import already cached the prior active project before
        // publication. Keep any later edits to it, even if it is no longer
        // active now. Same-profile undo must restore its immediate pre-import
        // state because it is also the import target.
        if previous.profile == imported_profile {
            store_project(&mut self.ak820_demo_projects, previous);
        }
        self.ak820_demo_profile = None;
        self.configure_ak820_offline_profile(profile);
        true
    }
}

fn store_project(
    projects: &mut Vec<Ak820OfflineProfileProject>,
    snapshot: Ak820OfflineProfileProject,
) {
    if let Some(old) = projects.iter_mut().find(|p| p.profile == snapshot.profile) {
        *old = snapshot;
    } else {
        projects.push(snapshot);
    }
}

fn read_project(path: &Path) -> Result<Vec<u8>, String> {
    let metadata = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_file() {
        return Err("Project path is not a regular file".into());
    }
    #[cfg(unix)]
    let file = {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)
            .map_err(|e| e.to_string())?
    };
    #[cfg(not(unix))]
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let opened_metadata = file.metadata().map_err(|e| e.to_string())?;
    if !opened_metadata.is_file() {
        return Err("Project path is not a regular file".into());
    }
    if opened_metadata.len() > MAX_PROJECT_BYTES {
        return Err("Project exceeds 2 MiB size limit".into());
    }
    // Also bound a growing file after metadata was read. Never allocate using
    // untrusted metadata; take permits at most one extra byte for overflow.
    let mut bytes = Vec::new();
    file.take(MAX_PROJECT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_PROJECT_BYTES {
        return Err("Project exceeds 2 MiB size limit".into());
    }
    Ok(bytes)
}

fn write_project(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    temporary.write_all(bytes).map_err(|e| e.to_string())?;
    temporary.flush().map_err(|e| e.to_string())?;
    temporary.as_file().sync_all().map_err(|e| e.to_string())?;
    // Persist errors may contain the private destination path. Return only
    // the underlying OS error; local UI is the sole consumer of these errors.
    temporary.persist_noclobber(path).map_err(|e| e.error.to_string())?;
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
enum FileOutcome {
    Cancelled,
    Saved,
    Loaded(Vec<u8>),
}

#[cfg(not(target_arch = "wasm32"))]
fn run_file_operation(
    save: bool,
    path: Option<std::path::PathBuf>,
    exported: Option<Vec<u8>>,
) -> Result<FileOutcome, String> {
    let Some(path) = path else {
        return Ok(FileOutcome::Cancelled);
    };
    if save {
        write_project(
            &path,
            exported
                .as_deref()
                .ok_or("Export snapshot is unavailable")?,
        )?;
        Ok(FileOutcome::Saved)
    } else {
        Ok(FileOutcome::Loaded(read_project(&path)?))
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn receive_file_result(
    result: Result<Result<FileOutcome, String>, std::sync::mpsc::TryRecvError>,
) -> Option<Result<FileOutcome, String>> {
    match result {
        Ok(result) => Some(result),
        Err(std::sync::mpsc::TryRecvError::Empty) => None,
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
            Some(Err("Local project operation worker disconnected".to_owned()))
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
static FILE_WORKER_BUSY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(not(target_arch = "wasm32"))]
struct WorkerPermit(&'static std::sync::atomic::AtomicBool);

#[cfg(not(target_arch = "wasm32"))]
impl WorkerPermit {
    fn acquire(busy: &'static std::sync::atomic::AtomicBool) -> Option<Self> {
        busy.compare_exchange(false, true, std::sync::atomic::Ordering::Acquire, std::sync::atomic::Ordering::Relaxed)
            .ok().map(|_| Self(busy))
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for WorkerPermit {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::Release);
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
struct PendingDialog {
    generation: u64,
    profile: Option<super::ak820_beta1_profiles::ProfileId>,
    result:
        std::sync::Arc<std::sync::Mutex<std::sync::mpsc::Receiver<Result<FileOutcome, String>>>>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Default)]
struct PathEntry {
    save: bool,
    path: String,
    confirm: bool,
    generation: u64,
    profile: Option<super::ak820_beta1_profiles::ProfileId>,
    browser: Option<std::sync::Arc<std::sync::Mutex<super::file_browser::FileBrowser>>>,
    browser_error: bool,
}

#[cfg(not(target_arch = "wasm32"))]
impl PathEntry {
    fn select(&mut self, result: super::file_browser::BrowserResult) {
        use super::file_browser::BrowserResult;
        let selected = match result {
            BrowserResult::Cancelled => None,
            BrowserResult::Selected(path) if !self.save => Some((path, false)),
            BrowserResult::Directory(path) if self.save => Some((path, true)),
            _ => None,
        };
        if let Some((mut path, directory)) = selected {
            if directory { path.push(""); }
            if let Some(path) = path.to_str() {
                self.path = path.to_owned();
                self.confirm = false;
                self.browser_error = false;
            } else {
                self.path.clear();
                self.browser_error = true;
            }
        }
        self.browser = None;
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
struct VialPreview {
    source: Vec<u8>,
    candidate: Option<Vec<u8>>,
    export: Option<Vec<u8>>,
    warnings: Option<Vec<String>>,
    summary: String,
}

#[cfg(not(target_arch = "wasm32"))]
fn bounded_vial_warnings(warnings: Vec<String>) -> Option<Vec<String>> {
    const MAX_WARNINGS: usize = 64;
    const MAX_WARNING_BYTES: usize = 16 * 1024;
    if warnings.len() > MAX_WARNINGS || warnings.iter().try_fold(0usize, |total, warning| total.checked_add(warning.len())).is_none_or(|total| total > MAX_WARNING_BYTES) {
        None
    } else {
        Some(warnings)
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
struct VialFlow {
    save: bool,
    path: String,
    browser: Option<std::sync::Arc<std::sync::Mutex<super::file_browser::FileBrowser>>>,
    pending: Option<std::sync::Arc<std::sync::Mutex<std::sync::mpsc::Receiver<Result<Option<VialPreview>, String>>>>>,
    preview: Option<VialPreview>,
    writing: bool,
    message: Option<String>,
}

#[cfg(not(target_arch = "wasm32"))]
impl VialFlow {
    fn new(save: bool) -> Self {
        Self { save, path: String::new(), browser: None, pending: None, preview: None, writing: false, message: None }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn vial_source(app: &EntropyApp) -> Result<Vec<u8>, String> {
    crate::ak820_offline_project::export_project(&app.snapshot_current_ak820_project()?)
}

#[cfg(not(target_arch = "wasm32"))]
fn accept_vial(app: &mut EntropyApp, preview: &VialPreview) -> Result<(), String> {
    if vial_source(app)? != preview.source {
        return Err("Project changed; select the Vial file again".into());
    }
    app.import_ak820_project_bytes(preview.candidate.as_deref().ok_or("Import candidate unavailable")?)
}

// The codec currently returns English strings, not stable warning codes. Unknown
// warnings must not be confirmed until their safety implications are translated.
#[cfg(not(target_arch = "wasm32"))]
fn vial_warning(warning: &str, ru: bool) -> Option<String> {
    let translated = match warning {
        "Import replaces every project matrix layer and encoder assignment." => "Импорт заменит все слои матрицы и назначения энкодера в проекте.",
        "Source firmware and profile are not verified; keycode meanings may differ. This is not a full backup." => "Исходные прошивка и профиль не проверены; значения кодов клавиш могут отличаться. Это не полная резервная копия.",
        "Import replaces all project combo slots, including empty slots." => "Импорт заменит все ячейки комбо проекта, включая пустые.",
        "Import replaces all project macro contents and actions, including empty slots." => "Импорт заменит содержимое и действия всех макросов проекта, включая пустые ячейки.",
        "Partial keymap export, not a full backup; source firmware and target compatibility are not verified. Vial restore writes to the device." => "Экспортируется только часть раскладки, а не полная резервная копия; исходная прошивка и совместимость назначения не проверены. Восстановление в Vial записывает данные на устройство.",
        "The exported vial_protocol 6 declares the assumed keycode encoding, not a verified source device protocol or firmware identity; VIA protocol is unknown and omitted." => "Поле vial_protocol 6 в экспортируемом файле обозначает предполагаемую кодировку клавиш, а не проверенный протокол исходного устройства или идентичность прошивки; протокол VIA неизвестен и не включён.",
        "The AK820 keycode encoding declaration describes this file, not a verified source device or firmware; Vial and VIA source protocols are unknown and omitted." => "Объявление кодировки клавиш AK820 описывает файл, а не проверенное исходное устройство или прошивку; исходные протоколы Vial и VIA неизвестны и не включены.",
        "Not exported: tap dance, key overrides, settings, RGB, layer names and project metadata." => "Не экспортируются: tap dance, переопределения клавиш, настройки, RGB, названия слоёв и метаданные проекта.",
        "Not exported: tap dance, key overrides, alternate repeat keys, settings, RGB, layer names and project metadata." => "Не экспортируются: tap dance, переопределения клавиш, альтернативные клавиши повтора, настройки, RGB, названия слоёв и метаданные проекта.",
        "Not exported: macros and combos; actual target capacities are unknown. Including these sections would overwrite target slots, and Vial may truncate macros." => "Макросы и комбо не экспортируются: фактическая ёмкость назначения неизвестна. Эти разделы перезаписали бы ячейки назначения, а Vial может обрезать макросы.",
        "Macro stream and action representations differ; neither is exported." => "Поток макросов и представление действий различаются; ни то ни другое не экспортируется.",
        _ => {
            if let Some(name) = warning.strip_prefix("Not imported: ") {
                return Some(if ru { format!("Не импортируется раздел: {name}") } else { warning.to_owned() });
            }
            if let Some(reason) = warning.strip_prefix("Combo unchanged: ") {
                let translated = known_vial_error(reason)?;
                return Some(if ru { format!("Комбо не изменены. {translated}") } else { format!("Combo unchanged: {}", vial_error(reason, false)) });
            }
            return None;
        }
    };
    Some(if ru { translated.to_owned() } else { warning.to_owned() })
}

#[cfg(not(target_arch = "wasm32"))]
fn vial_warnings_confirmable(warnings: &Option<Vec<String>>) -> bool {
    warnings.as_ref().is_some_and(|warnings| warnings.iter().all(|warning| vial_warning(warning, false).is_some()))
}

#[cfg(not(target_arch = "wasm32"))]
fn known_vial_error(error: &str) -> Option<String> {
    let translated = match error {
        "Invalid target dimensions" => "Неверные размеры целевой раскладки.",
        "Invalid or duplicate physical position" => "Неверная или повторяющаяся позиция клавиши.",
        "Keycode out of range" | "Numeric expression out of range" => "Код клавиши или число вне допустимого диапазона.",
        "Invalid hexadecimal keycode" => "Неверный шестнадцатеричный код клавиши.",
        "Unknown or invalid keycode" => "Неизвестный или неверный код клавиши.",
        "Vial expression nesting limit exceeded" => "Превышена допустимая вложенность выражения Vial.",
        "Vial tap expression would truncate an extended keycode" => "Выражение tap Vial обрезало бы расширенный код клавиши; используйте совместимое назначение.",
        "Vial modifier expression contains an extended keycode" => "Выражение с модификатором Vial содержит расширенный код клавиши; используйте совместимое назначение.",
        "Invalid Vial expression" | "Unsupported nested Vial expression" => "Неверное или неподдерживаемое выражение Vial.",
        "Invalid Vial layer or tap-dance index" | "Vial layer or tap-dance index out of range" => "Неверный индекс слоя или tap dance Vial.",
        "Vial protocol 5 or 6 required for layer expression" | "Vial protocol 5 or 6 required for mod-tap" | "Vial protocol 5 or 6 required for macro keycode" | "Vial protocol 5 or 6 required for mouse keycode" | "Vial protocol 5 or 6 required for RGB_HUI" => "Для этого кода требуется указать протокол Vial 5 или 6.",
        "Expected array" | "Incorrect dimensions" => "Неверная структура или размеры массива Vial.",
        "Vial file exceeds size limit" | "Project exceeds 2 MiB size limit" => "Файл превышает предел размера 2 МиБ.",
        "Expected Vial object" => "Ожидался объект Vial.",
        "Unsupported Vial version" => "Версия файла Vial не поддерживается.",
        "Vial UID mismatch or invalid UID" => "UID файла Vial неверен или не соответствует проекту.",
        "Vial layout options differ or are invalid" => "Параметры раскладки Vial неверны или не соответствуют проекту.",
        "Unsupported AK820 keycode encoding declaration" => "Объявление кодировки клавиш AK820 не поддерживается; выберите файл с поддерживаемой кодировкой.",
        "Conflicting Vial protocol and AK820 keycode encoding" => "Протокол Vial и объявление кодировки клавиш AK820 противоречат друг другу; выберите согласованный файл.",
        "Missing layout" => "В файле отсутствует раскладка.",
        "Missing encoder layout" => "В файле отсутствует раскладка энкодера.",
        "Unexpected populated matrix position" => "Файл содержит назначение в отсутствующей позиции матрицы.",
        "Bundled AK820 reference definition digest mismatch" => "Контрольная сумма встроенного определения AK820 не совпадает; обновите приложение.",
        "Project or target differs from the pinned AK820 reference definition" => "Проект или назначение не соответствует встроенному определению AK820.",
        "Invalid project encoder layers" | "Invalid project dimensions" | "Invalid project layer count" => "Неверные размеры слоёв или энкодеров проекта.",
        "Non-Vial binding" => "Проект содержит назначение, не являющееся кодом Vial.",
        "Invalid macro" | "Invalid macro text" | "Unsupported macro action" | "Unsupported macro text" => "Макрос содержит неверный или неподдерживаемый текст или действие.",
        "Project changed; select the Vial file again" => "Проект изменился; выберите файл Vial заново.",
        "Import candidate unavailable" => "Предпросмотр импорта недоступен; выберите файл заново.",
        "Vial worker disconnected" => "Обработка Vial прервана; повторите выбор файла.",
        "Project path is not a regular file" => "Выбранный путь не является обычным файлом.",
        _ => {
            if let Some(details) = error.strip_prefix("Vial source protocol ").and_then(|s| s.strip_suffix(" may differ from AK820 protocol 6; conversion unsupported")) {
                if let Some((protocol, code)) = details.split_once(": keycode 0x") {
                    let valid_protocol = protocol == "None" || protocol.strip_prefix("Some(").and_then(|s| s.strip_suffix(')')).is_some_and(|s| s.parse::<u64>().is_ok());
                    if valid_protocol && code.len() == 4 && code.bytes().all(|b| b.is_ascii_hexdigit()) {
                        return Some(format!("Исходный протокол Vial: {protocol}; значение кода 0x{code} может отличаться от протокола AK820 6, преобразование не поддерживается. Проверьте протокол источника и выберите совместимый файл или назначение."));
                    }
                }
            }
            if let Some(value) = error.strip_prefix("Unsupported Vial expression: ").or_else(|| error.strip_prefix("Unsupported or unverified Vial symbol or compound expression: ")) {
                return Some(format!("Неподдерживаемое или непроверенное выражение Vial: {value}"));
            }
            return None;
        }
    };
    Some(translated.to_owned())
}

#[cfg(not(target_arch = "wasm32"))]
fn vial_error(error: &str, ru: bool) -> String {
    const MAX_DETAILS: usize = 1024;
    let mut end = error.len().min(MAX_DETAILS);
    while !error.is_char_boundary(end) { end -= 1; }
    let truncated = end < error.len();
    let details = &error[..end];
    let mut message = if !ru {
        if matches!(details, "Vial tap expression would truncate an extended keycode" | "Vial modifier expression contains an extended keycode") {
            format!("{details}. Use a compatible assignment.")
        } else if matches!(details, "Unsupported AK820 keycode encoding declaration" | "Conflicting Vial protocol and AK820 keycode encoding") {
            format!("{details}. Select a file with a supported, consistent encoding declaration.")
        } else if details.starts_with("Vial source protocol ") && known_vial_error(details).is_some() {
            format!("{details}. Check the source protocol and select a compatible file or assignment.")
        } else if truncated {
            format!("Could not process the file or project. Technical details: {details}")
        } else {
            details.to_owned()
        }
    } else {
        known_vial_error(details).unwrap_or_else(|| format!("Не удалось обработать файл или проект. Проверьте файл, путь и совместимость; проект не изменён. Техническая причина (на английском): {details}"))
    };
    if truncated {
        message.push_str(if ru { " [Технические подробности сокращены.]" } else { " [Technical details truncated.]" });
    }
    message
}

#[cfg(not(target_arch = "wasm32"))]
fn show_vial(app: &mut EntropyApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let id = egui::Id::new("ak820_vial_flow");
    let Some(mut flow) = ctx.data(|d| d.get_temp::<VialFlow>(id)) else { return; };
    let ru = app.app_settings.language == crate::i18n::Language::Russian;
    let tr = |en, ru_text| if ru { ru_text } else { en };
    if let Some(pending) = &flow.pending {
        let result = match pending.lock().unwrap().try_recv() {
            Ok(result) => Some(result),
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err("Vial worker disconnected".into())),
        };
        if let Some(result) = result {
            flow.pending = None;
            flow.writing = false;
            match result {
                Ok(Some(preview)) => flow.preview = Some(preview),
                Ok(None) => { flow.preview = None; flow.message = Some(tr("Vial file exported", "Файл Vial экспортирован").into()); },
                Err(error) => flow.message = Some(vial_error(&error, ru)),
            }
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
    if !app.ak820_demo || !is_open(&ctx) {
        ctx.data_mut(|d| d.remove::<VialFlow>(id));
        return;
    }
    let mut start = false;
    let mut write = false;
    let mut done = false;
    ui.heading(tr("Offline Vial interchange", "Офлайн-обмен Vial"));
    egui::ScrollArea::vertical().id_salt("ak820_vial_content").show(ui, |ui| {
        ui.label(tr("Local project only; this is not a device settings backup. No device operation is performed.", "Только локальный проект; это не резервная копия настроек устройства. Операции с устройством не выполняются."));
        if flow.preview.is_none() && flow.pending.is_none() {
            if ui.button(tr("Browse…", "Обзор…")).clicked() {
                let mode = if flow.save { super::file_browser::BrowserMode::Directory } else { super::file_browser::BrowserMode::File };
                flow.browser = Some(std::sync::Arc::new(std::sync::Mutex::new(super::file_browser::FileBrowser::new(std::path::PathBuf::from("."), mode, &ctx))));
            }
            if let Some(browser) = flow.browser.clone() {
                if let Some(result) = browser.lock().unwrap().show(ui, ru) {
                    use super::file_browser::BrowserResult;
                    let selected = match result {
                        BrowserResult::Selected(p) if !flow.save => Some(p),
                        BrowserResult::Directory(mut p) if flow.save => { p.push(""); Some(p) },
                        _ => None,
                    };
                    if let Some(path) = selected {
                        if let Some(path) = path.to_str() { flow.path = path.to_owned(); }
                        else { flow.path.clear(); flow.message = Some(tr("Selected path cannot be represented as text", "Выбранный путь нельзя представить текстом").into()); }
                    }
                    flow.browser = None;
                }
            }
            ui.label(tr("Vial file path", "Путь к файлу Vial"));
            ui.add_enabled(flow.browser.is_none(), egui::TextEdit::singleline(&mut flow.path).desired_width(ui.available_width()));
            if ui.add_enabled(flow.browser.is_none() && !flow.path.trim().is_empty(), egui::Button::new(tr("Preview", "Предпросмотр"))).clicked() { start = true; }
        }
        if let Some(preview) = &flow.preview {
            ui.label(&preview.summary);
            ui.label(tr("Source firmware and profile are unknown; keycode and feature capabilities may differ. Review all warnings before continuing.", "Исходные прошивка и профиль неизвестны; значения кодов и возможности могут отличаться. Перед продолжением изучите все предупреждения."));
            if flow.save {
                ui.label(tr("Export creates a new file only; existing files are never replaced. Vial restore may write to a device.", "Экспорт создаёт только новый файл и не заменяет существующие. Восстановление в Vial может записать данные на устройство."));
            } else {
                ui.label(tr("Confirming replaces project assignments and supported imported sections. Encoder directions, macro contents and combo entries are counted but not individually previewed. Undo import restores the previous project.", "Подтверждение заменит назначения проекта и поддерживаемые импортированные разделы. Направления энкодера, содержимое макросов и записи комбо подсчитаны, но по отдельности не показаны. Отмена импорта восстановит предыдущий проект."));
            }
            let warnings_known = vial_warnings_confirmable(&preview.warnings);
            if preview.warnings.is_none() {
                ui.label(tr("Too many warnings or warning text is too large; confirmation is disabled. Select another file or update the application.", "Слишком много предупреждений или их текст слишком велик; подтверждение отключено. Выберите другой файл или обновите приложение."));
            }
            for warning in preview.warnings.iter().flatten() {
                if let Some(text) = vial_warning(warning, ru) {
                    ui.label(text);
                } else {
                    ui.label(tr("Unrecognized safety warning; confirmation is disabled until this application supports it. Technical detail:", "Неизвестное предупреждение безопасности; подтверждение отключено до поддержки предупреждения приложением. Техническая информация:"));
                    ui.label(warning);
                }
            }
            if flow.pending.is_none() && ui.add_enabled(warnings_known, egui::Button::new(if flow.save { tr("Export new file", "Экспортировать новый файл") } else { tr("Apply import", "Применить импорт") })).clicked() {
                if flow.save {
                    if vial_source(app).as_ref().ok() == Some(&preview.source) { write = true; }
                    else { flow.message = Some(tr("Project changed; preview again", "Проект изменился; повторите предпросмотр").into()); flow.preview = None; }
                } else {
                    match accept_vial(app, preview) {
                        Ok(()) => done = true,
                        Err(error) => { flow.message = Some(vial_error(&error, ru)); flow.preview = None; }
                    }
                }
            }
        }
        if flow.pending.is_some() { ui.label(tr("Working…", "Выполняется…")); }
        if let Some(message) = &flow.message { ui.label(message); }
        if ui.add_enabled(!flow.writing, egui::Button::new(tr("Cancel / Back", "Отмена / Назад"))).clicked() { done = true; }
    });
    if done { ctx.data_mut(|d| d.remove::<VialFlow>(id)); return; }
    if start || write {
        let Some(permit) = WorkerPermit::acquire(&FILE_WORKER_BUSY) else {
            flow.message = Some(tr("A project file operation is still running", "Операция с файлом проекта ещё выполняется").into());
            ctx.data_mut(|d| d.insert_temp(id, flow));
            return;
        };
        let snapshot = app.snapshot_current_ak820_project();
        let path = std::path::PathBuf::from(&flow.path);
        let save = flow.save;
        let preview = flow.preview.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        flow.pending = Some(std::sync::Arc::new(std::sync::Mutex::new(rx)));
        flow.writing = write;
        flow.message = None;
        let repaint = ctx.clone();
        if std::thread::Builder::new().spawn(move || {
            let _permit = permit;
            let result = (|| {
                if write {
                    write_project(&path, preview.as_ref().and_then(|p| p.export.as_deref()).ok_or("Export unavailable")?)?;
                    return Ok(None);
                }
                let project = snapshot?;
                let source = crate::ak820_offline_project::export_project(&project)?;
                // Candidate codec API: import_project(raw, project, UID, layout options)
                // and export_project(project, UID, layout options).
                if save {
                    let exported = crate::ak820_vial::export_project(&project, crate::ak820::VIAL_UID, -1)?;
                    Ok(Some(VialPreview { source, candidate: None, export: Some(exported.bytes), warnings: bounded_vial_warnings(exported.warnings), summary: if ru { "Предпросмотр экспорта раскладки и энкодера" } else { "Keymap and encoder export preview" }.into() }))
                } else {
                    let raw = read_project(&path)?;
                    let imported = crate::ak820_vial::import_project(&raw, &project, crate::ak820::VIAL_UID, -1)?;
                    let changed_keys = project.layout.layers.iter().zip(&imported.project.layout.layers).map(|(a, b)| a.iter().zip(b).filter(|(a, b)| a != b).count()).sum::<usize>();
                    let changed_encoders = project.layout.encoder_layers.iter().zip(&imported.project.layout.encoder_layers).map(|(a, b)| a.iter().zip(b).filter(|(a, b)| a != b).count()).sum::<usize>();
                    let changed_macros = project.macro_texts.iter().zip(&imported.project.macro_texts).zip(project.macro_actions.iter().zip(&imported.project.macro_actions)).filter(|((a, b), (c, d))| a != b || c != d).count();
                    let changed_combos = project.combo_entries.iter().zip(&imported.project.combo_entries).filter(|(a, b)| a != b).count();
                    let mut summary = if ru { format!("Изменено: клавиш {changed_keys}, направлений энкодера {changed_encoders}, макросов {changed_macros}, комбо {changed_combos}.") } else { format!("Changed: {changed_keys} keys, {changed_encoders} encoder directions, {changed_macros} macros, {changed_combos} combos.") };
                    for (layer, (before, after)) in project.layout.layers.iter().zip(&imported.project.layout.layers).enumerate() {
                        for (key, (before, after)) in before.iter().zip(after).enumerate() {
                            if before != after {
                                let position = &project.layout.keys[key];
                                summary.push_str(&format!("\nL{layer} [{},{}]: {:04X} → {:04X}", position.row, position.col, before.vial_keycode(), after.vial_keycode()));
                            }
                        }
                    }
                    let candidate = crate::ak820_offline_project::export_project(&imported.project)?;
                    Ok(Some(VialPreview { source, candidate: Some(candidate), export: None, warnings: bounded_vial_warnings(imported.warnings), summary }))
                }
            })();
            let _ = tx.send(result);
            repaint.request_repaint();
        }).is_err() {
            flow.pending = None;
            flow.writing = false;
            flow.message = Some(tr("Could not start Vial worker", "Не удалось запустить обработку Vial").into());
        }
    }
    ctx.data_mut(|d| d.insert_temp(id, flow));
}

/// Keep the page active until its background result has been consumed.
pub(crate) fn is_busy(ctx: &egui::Context) -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        ctx.data(|d| {
            d.get_temp::<PendingDialog>(egui::Id::new("ak820_local_project_dialog"))
                .is_some()
                || d.get_temp::<VialFlow>(egui::Id::new("ak820_vial_flow"))
                    .is_some_and(|flow| flow.writing)
        })
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = ctx;
        false
    }
}

/// Render in the main content area while this page is open.
pub(crate) fn show(app: &mut EntropyApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    #[cfg(not(target_arch = "wasm32"))]
    {
        let ru = app.app_settings.language == crate::i18n::Language::Russian;
        let tr = |en, ru_text| if ru { ru_text } else { en };
        let pending_id = egui::Id::new("ak820_local_project_dialog");
        let message_id = egui::Id::new("ak820_local_project_message");
        let entry_id = egui::Id::new("ak820_local_project_path_entry");
        if ctx.data(|d| d.get_temp::<VialFlow>(egui::Id::new("ak820_vial_flow")).is_some()) {
            show_vial(app, ui);
            return;
        }
        let mut entry = ctx.data(|d| d.get_temp::<PathEntry>(entry_id));
        let pending = ctx.data(|d| d.get_temp::<PendingDialog>(pending_id));
        if let Some(pending) = &pending {
            let result = receive_file_result(pending.result.lock().unwrap().try_recv());
            if let Some(result) = result {
                ctx.data_mut(|d| d.remove::<PendingDialog>(pending_id));
                if !is_open(&ctx) || !app.ak820_demo || pending.generation != app.connection_generation || pending.profile != app.ak820_demo_profile {
                    ctx.data_mut(|d| d.insert_temp(message_id, tr("Project changed; result discarded", "Проект изменился; результат отклонён").to_owned()));
                } else { match result {
                    Err(error) => {
                        // This local-only message never enters status/report/log APIs.
                        ctx.data_mut(|d| d.insert_temp(message_id, error));
                    }
                    Ok(FileOutcome::Loaded(raw)) => {
                        if let Err(error) = app.import_ak820_project_bytes(&raw) {
                            ctx.data_mut(|d| d.insert_temp(message_id, error));
                        } else {
                            ctx.data_mut(|d| {
                                d.insert_temp(
                                    message_id,
                                    tr("Local project ready", "Локальный проект готов").to_owned(),
                                )
                            });
                        }
                    }
                    Ok(FileOutcome::Saved) => {
                        ctx.data_mut(|d| {
                            d.insert_temp(
                                message_id,
                                tr("Local project ready", "Локальный проект готов").to_owned(),
                            );
                        });
                    }
                    Ok(FileOutcome::Cancelled) => {}
                }}
                ctx.input_mut(|i| i.pointer = egui::PointerState::default());
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
        if !is_open(&ctx) || !app.ak820_demo {
            return;
        }
        let mut action = None;
        let mut profile_to_open = None;
        let mut close_clicked = false;
        let mut cancel_entry = false;
        {
            ui.heading(tr("Offline project", "Офлайн-проект"));
            crate::ui_style::modal_intro(
                ui,
                tr("Demo only · local file", "Только демо · локальный файл"),
            );
            ui.add_space(8.0);
            egui::ScrollArea::vertical()
                .id_salt("ak820_local_project_content")
                .max_height((ui.available_height() - 48.0).max(0.0))
                .show(ui, |ui| {
                    if let Some(profile) = app.ak820_demo_profile.filter(|_| pending.is_none() && entry.is_none()) {
                        egui::ComboBox::from_id_salt("ak820_local_project_profile")
                            .selected_text(format!(
                                "{}: {}",
                                tr("Profile", "Профиль"),
                                profile.as_str()
                            ))
                            .show_ui(ui, |ui| {
                                for image in super::ak820_beta1_profiles::profiles() {
                                    let description =
                                        super::ak820_beta1_profiles::profile_description(
                                            image.profile,
                                        );
                                    if ui
                                        .selectable_label(
                                            image.profile == profile,
                                            description.title.for_language(ru),
                                        )
                                        .clicked()
                                    {
                                        profile_to_open = Some(image.profile);
                                    }
                                }
                            });
                    }
                    ui.add_enabled_ui(pending.is_none(), |ui| {
                        ui.horizontal_wrapped(|ui| {
                            if crate::ui_style::modern_button(
                                ui,
                                tr("Export…", "Экспорт…"),
                                egui::vec2(124.0, 34.0),
                                true,
                            )
                            .clicked()
                            {
                                entry = Some(PathEntry {
                                    save: true,
                                    path: String::new(),
                                    confirm: false,
                                    generation: app.connection_generation,
                                    profile: app.ak820_demo_profile,
                                    browser: None,
                                    browser_error: false,
                                });
                            }
                            if crate::ui_style::modern_button(
                                ui,
                                tr("Import…", "Импорт…"),
                                egui::vec2(124.0, 34.0),
                                true,
                            )
                            .clicked()
                            {
                                entry = Some(PathEntry {
                                    save: false,
                                    path: String::new(),
                                    confirm: false,
                                    generation: app.connection_generation,
                                    profile: app.ak820_demo_profile,
                                    browser: None,
                                    browser_error: false,
                                });
                            }
                        });
                    });
                    ui.add_enabled_ui(pending.is_none() && entry.is_none(), |ui| {
                        ui.horizontal_wrapped(|ui| {
                            for (save, label) in [(false, tr("Import Vial…", "Импорт Vial…")), (true, tr("Export Vial…", "Экспорт Vial…"))] {
                                if ui.button(label).clicked() {
                                    ctx.data_mut(|d| d.insert_temp(egui::Id::new("ak820_vial_flow"), VialFlow::new(save)));
                                }
                            }
                        });
                    });
                    if let Some(form) = &mut entry {
                        if ui.add_enabled(pending.is_none() && !form.confirm && form.browser.is_none(), egui::Button::new(tr("Browse…", "Обзор…"))).clicked() {
                            let mode = if form.save { super::file_browser::BrowserMode::Directory } else { super::file_browser::BrowserMode::File };
                            form.browser = Some(std::sync::Arc::new(std::sync::Mutex::new(super::file_browser::FileBrowser::new(std::path::PathBuf::from("."), mode, &ctx))));
                        }
                        if let Some(browser) = form.browser.clone() {
                            let result = browser.lock().unwrap().show(ui, ru);
                            if let Some(result) = result { form.select(result); }
                        }
                        if form.browser_error { ui.label(tr("Selected path cannot be represented as text", "Выбранный путь нельзя представить текстом")); }
                        ui.label(if form.save {
                            tr("Export file path", "Путь к файлу экспорта")
                        } else {
                            tr("Import file path", "Путь к файлу импорта")
                        });
                        ui.add_enabled(
                            pending.is_none() && !form.confirm && form.browser.is_none(),
                            egui::TextEdit::singleline(&mut form.path)
                                .desired_width(ui.available_width()),
                        );
                        if form.confirm {
                            ui.label(tr(
                                "Export only creates a new file; existing files are never replaced. Continue?",
                                "Экспорт создаёт только новый файл и не заменяет существующие. Продолжить?",
                            ));
                        }
                        ui.horizontal_wrapped(|ui| {
                            let label = if form.confirm {
                                tr("Export new file", "Экспортировать новый файл")
                            } else if form.save {
                                tr("Continue", "Продолжить")
                            } else {
                                tr("Import", "Импорт")
                            };
                            if ui
                                .add_enabled(
                                    pending.is_none() && form.browser.is_none() && !form.path.trim().is_empty(),
                                    egui::Button::new(label),
                                )
                                .clicked()
                            {
                                if form.save && !form.confirm {
                                    form.confirm = true;
                                } else {
                                    action = Some(form.save);
                                }
                            }
                            if ui
                                .add_enabled(
                                    pending.is_none(),
                                    egui::Button::new(tr("Cancel", "Отмена")),
                                )
                                .clicked()
                            {
                                cancel_entry = true;
                            }
                        });
                    }
                    if crate::ui_style::action_button(
                        ui,
                        tr("Undo import", "Отменить импорт"),
                        pending.is_none() && entry.is_none() && app.ak820_offline_import_undo.is_some(),
                    )
                    .clicked()
                    {
                        app.undo_ak820_project_import();
                    }
                    if app.ak820_offline_extensions.is_some()
                        && crate::ui_style::action_button(
                            ui,
                            tr("Module settings", "Настройки модулей"),
                            pending.is_none(),
                        )
                        .clicked()
                    {
                        ctx.data_mut(|data| {
                            data.insert_temp(egui::Id::new("ak820_offline_extensions_open"), true)
                        });
                        close_clicked = true;
                    }
                    if pending.is_some() {
                        ui.label(tr("Working…", "Выполняется…"));
                    }
                    if let Some(message) = ctx.data(|d| d.get_temp::<String>(message_id)) {
                        ui.label(message);
                    }
                });
            ui.separator();
            ui.horizontal(|ui| {
                let close = crate::ui_style::modern_button(
                    ui,
                    tr("Back", "Назад"),
                    egui::vec2(100.0, 32.0),
                    pending.is_none(),
                );
                if close.clicked() && pending.is_none() {
                    close_clicked = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if crate::ui_style::modern_button(
                        ui,
                        tr("Help", "Справка"),
                        egui::vec2(100.0, 32.0),
                        true,
                    )
                    .clicked()
                    {
                        super::ak820_help_ui::open_section(&ctx, "offline-projects");
                    }
                });
            });
        }
        if cancel_entry {
            entry = None;
        }
        if close_clicked && pending.is_none() {
            entry = None;
            close(&ctx);
        }
        if let Some(profile) = profile_to_open {
            app.configure_ak820_offline_profile(profile);
        }
        if let Some(save) = action {
            if entry.as_ref().is_none_or(|form| form.generation != app.connection_generation || form.profile != app.ak820_demo_profile) {
                ctx.data_mut(|d| {
                    d.remove::<PathEntry>(entry_id);
                    d.insert_temp(message_id, tr("Project changed; select the file again", "Проект изменился; выберите файл снова").to_owned());
                });
                return;
            }
            let Some(permit) = WorkerPermit::acquire(&FILE_WORKER_BUSY) else {
                ctx.data_mut(|d| d.insert_temp(message_id, tr("A project file operation is still running", "Операция с файлом проекта ещё выполняется").to_owned()));
                return;
            };
            let exported = if save {
                match app
                    .snapshot_current_ak820_project()
                    .and_then(|p| crate::ak820_offline_project::export_project(&p))
                {
                    Ok(bytes) => Some(bytes),
                    Err(error) => {
                        ctx.data_mut(|d| d.insert_temp(message_id, error));
                        return;
                    }
                }
            } else {
                None
            };
            let path = entry.take().map(|form| std::path::PathBuf::from(form.path));
            let (tx, rx) = std::sync::mpsc::channel();
            ctx.data_mut(|d| {
                d.insert_temp(
                    pending_id,
                    PendingDialog {
                        generation: app.connection_generation,
                        profile: app.ak820_demo_profile,
                        result: std::sync::Arc::new(std::sync::Mutex::new(rx)),
                    },
                )
            });
            let repaint = ctx.clone();
            if std::thread::Builder::new().spawn(move || {
                let _permit = permit;
                let _ = tx.send(run_file_operation(save, path, exported));
                repaint.request_repaint();
            }).is_err() {
                ctx.data_mut(|d| {
                    d.remove::<PendingDialog>(pending_id);
                    d.insert_temp(message_id, tr("Could not start project worker", "Не удалось запустить обработку проекта").to_owned());
                });
            }
        }
        ctx.data_mut(|d| {
            if let Some(entry) = entry {
                d.insert_temp(entry_id, entry);
            } else {
                d.remove::<PathEntry>(entry_id);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ak820_beta1_profiles as beta;
    use crate::keycode_picker::MacroAction;
    fn app() -> EntropyApp {
        let cc = eframe::CreationContext::_new_kittest(egui::Context::default());
        EntropyApp::new_ak820_demo(&cc)
    }
    fn export(app: &EntropyApp) -> Vec<u8> {
        crate::ak820_offline_project::export_project(&app.snapshot_current_ak820_project().unwrap())
            .unwrap()
    }
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn vial_warning_and_error_localization_requires_known_safety_messages() {
        let warnings = [
            "Import replaces every project matrix layer and encoder assignment.",
            "Source firmware and profile are not verified; keycode meanings may differ. This is not a full backup.",
            "Import replaces all project combo slots, including empty slots.",
            "Import replaces all project macro contents and actions, including empty slots.",
            "Partial keymap export, not a full backup; source firmware and target compatibility are not verified. Vial restore writes to the device.",
            "The exported vial_protocol 6 declares the assumed keycode encoding, not a verified source device protocol or firmware identity; VIA protocol is unknown and omitted.",
            "The AK820 keycode encoding declaration describes this file, not a verified source device or firmware; Vial and VIA source protocols are unknown and omitted.",
            "Not exported: tap dance, key overrides, settings, RGB, layer names and project metadata.",
            "Not exported: tap dance, key overrides, alternate repeat keys, settings, RGB, layer names and project metadata.",
            "Not exported: macros and combos; actual target capacities are unknown. Including these sections would overwrite target slots, and Vial may truncate macros.",
            "Macro stream and action representations differ; neither is exported.",
            "Not imported: macro",
            "Combo unchanged: Incorrect dimensions",
        ];
        for warning in warnings {
            assert_eq!(vial_warning(warning, false).as_deref(), Some(warning));
            let russian = vial_warning(warning, true).unwrap();
            assert_ne!(russian, warning);
            assert!(russian.chars().any(|c| ('а'..='я').contains(&c) || ('А'..='Я').contains(&c)));
        }
        let protocol_warning = "The exported vial_protocol 6 declares the assumed keycode encoding, not a verified source device protocol or firmware identity; VIA protocol is unknown and omitted.";
        assert!(vial_warning(protocol_warning, true).unwrap().contains("предполагаемую кодировку"));
        assert!(vial_warnings_confirmable(&Some(vec![protocol_warning.into()])));
        let encoding_warning = "The AK820 keycode encoding declaration describes this file, not a verified source device or firmware; Vial and VIA source protocols are unknown and omitted.";
        assert!(vial_warning(encoding_warning, true).unwrap().contains("не проверенное"));
        assert!(vial_warnings_confirmable(&Some(vec![encoding_warning.into()])));
        for error in ["Unsupported AK820 keycode encoding declaration", "Conflicting Vial protocol and AK820 keycode encoding"] {
            assert!(known_vial_error(error).is_some());
            assert!(vial_error(error, true).contains("выберите"));
            assert!(vial_error(error, false).contains("encoding"));
        }
        assert!(!vial_warnings_confirmable(&Some(vec!["Conflicting Vial protocol and AK820 keycode encoding".into()])));
        assert!(vial_warning("Future safety warning", true).is_none());
        assert!(vial_warning("Future safety warning", false).is_none());
        assert!(vial_warning("Combo unchanged: future safety reason", true).is_none());
        assert!(vial_warning("Combo unchanged: future safety reason", false).is_none());
        assert!(!vial_warnings_confirmable(&Some(vec!["Combo unchanged: future safety reason".into()])));
        assert!(vial_warnings_confirmable(&Some(vec!["Combo unchanged: Incorrect dimensions".into()])));
        assert!(vial_error("Vial UID mismatch or invalid UID", true).contains("не соответствует"));
        assert!(vial_error("Unsupported Vial expression: LSFT(X)", true).contains("LSFT(X)"));
        for protocol in ["None", "Some(5)", "Some(18446744073709551615)"] {
            let error = format!("Vial source protocol {protocol}: keycode 0x5cc5 may differ from AK820 protocol 6; conversion unsupported");
            let russian = vial_error(&error, true);
            assert!(russian.contains("0x5cc5") && russian.contains("совместимый файл"));
            assert!(vial_error(&error, false).contains("select a compatible file or assignment"));
            let warning = format!("Combo unchanged: {error}");
            assert!(vial_warning(&warning, true).unwrap().contains("Комбо не изменены"));
            assert!(vial_warnings_confirmable(&Some(vec![warning])));
        }
        assert!(vial_warning("Combo unchanged: Vial source protocol bogus: keycode 0x5cc5 may differ from AK820 protocol 6; conversion unsupported", true).is_none());
        assert!(vial_error("future error", true).contains("Техническая причина"));
        assert_eq!(vial_error("future error", false), "future error");
    }
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn codec_combo_fallback_warnings_and_direct_errors_are_localized() {
        let app = app();
        let project = app.snapshot_current_ak820_project().unwrap();
        let exported = crate::ak820_vial::export_project(&project, crate::ak820::VIAL_UID, -1).unwrap();
        let base: serde_json::Value = serde_json::from_slice(&exported.bytes).unwrap();
        for (expression, reason) in [
            ("LT1(MO(1))", "Vial tap expression would truncate an extended keycode"),
            ("LSFT(MO(1))", "Vial modifier expression contains an extended keycode"),
        ] {
            let mut doc = base.clone();
            doc["vial_protocol"] = serde_json::json!(6);
            doc["combo"] = serde_json::json!(vec![vec![0; 5]; 8]);
            doc["combo"][0][0] = serde_json::json!(expression);
            let imported = crate::ak820_vial::import_project(&serde_json::to_vec(&doc).unwrap(), &project, crate::ak820::VIAL_UID, -1).unwrap();
            let warning = format!("Combo unchanged: {reason}");
            assert!(imported.warnings.contains(&warning));
            assert_eq!(imported.project.combo_entries, project.combo_entries);
            assert!(vial_warnings_confirmable(&bounded_vial_warnings(imported.warnings)));
            assert!(vial_warning(&warning, true).unwrap().contains("расширенный код"));
            let mut doc = base.clone();
            doc["vial_protocol"] = serde_json::json!(6);
            let key = &project.layout.keys[0];
            doc["layout"][0][key.row as usize][key.col as usize] = serde_json::json!(expression);
            let error = crate::ak820_vial::import_project(&serde_json::to_vec(&doc).unwrap(), &project, crate::ak820::VIAL_UID, -1).err().unwrap();
            assert_eq!(error, reason);
            assert!(vial_error(&error, true).contains("расширенный код"));
            assert!(vial_error(&error, false).contains("Use a compatible assignment"));
        }
    }
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn vial_errors_bound_large_expressions_and_disk_details() {
        let expression = format!("Unsupported or unverified Vial symbol or compound expression: {}", "Ж".repeat(1024 * 1024));
        let disk_error = format!("disk error: {}", "🗂".repeat(1024 * 1024));
        for error in [&expression, &disk_error] {
            for ru in [false, true] {
                let displayed = vial_error(error, ru);
                assert!(displayed.len() < 1400);
                assert!(displayed.contains(if ru { "сокращены" } else { "truncated" }));
                assert!(displayed.contains(if ru { "Не удалось" } else { "Could not process" }) || displayed.contains("Неподдерживаемое"));
            }
        }
        assert_eq!(vial_error("disk error", false), "disk error");
    }
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn vial_warning_limits_block_confirmation() {
        let allowed = bounded_vial_warnings(vec!["a".repeat(256); 64]);
        assert!(allowed.is_some());
        assert!(bounded_vial_warnings(vec!["a".into(); 65]).is_none());
        let too_large = bounded_vial_warnings(vec!["a".repeat(16 * 1024 + 1)]);
        assert!(too_large.is_none());
        assert!(!vial_warnings_confirmable(&too_large));
        assert!(!vial_warnings_confirmable(&bounded_vial_warnings(vec!["a".into(); 65])));
    }
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn vial_accept_undo_and_stale_preview_preserve_project() {
        let mut app = app();
        let source = export(&app);
        let mut candidate = app.snapshot_current_ak820_project().unwrap();
        candidate.layout.layers[0][0] = 5u16.into();
        let preview = VialPreview {
            source: source.clone(),
            candidate: Some(crate::ak820_offline_project::export_project(&candidate).unwrap()),
            export: None, warnings: Some(vec![]), summary: String::new(),
        };
        // Discarding a preview does not apply it.
        drop(preview.clone());
        assert_eq!(export(&app), source);
        accept_vial(&mut app, &preview).unwrap();
        assert_eq!(app.layout.as_ref().unwrap().layers[0][0].vial_keycode(), 5);
        assert!(app.undo_ak820_project_import());
        assert_eq!(export(&app), source);
        app.layout.as_mut().unwrap().layers[0][0] = 6u16.into();
        let changed = export(&app);
        assert!(accept_vial(&mut app, &preview).is_err());
        assert_eq!(export(&app), changed);
    }
    #[test]
    fn real_app_all_ten_projects_transfer_between_instances() {
        for (index, entry) in beta::profiles().iter().enumerate() {
            let mut original = app();
            original.configure_ak820_offline_profile(entry.profile);
            let code = 4 + index as u16;
            original.layout.as_mut().unwrap().layers[5][82] = code.into();
            original.layout.as_mut().unwrap().encoder_layers[5][1] = code;
            original.keycode_picker.macro_texts[15] = b"portable".to_vec();
            original.keycode_picker.macro_actions[15] = vec![MacroAction::Tap(code)];
            original.keycode_picker.macro_names[15] = format!("M{index}");
            original.keycode_picker.macro_descriptions[15] = "Local edit".into();
            original.combo_names[7] = "Local combo".into();
            original.combo_colors[7] = 0x123456;
            original.layer_names[5] = "Portable".into();
            original.rgb_settings.hue = 42;
            if let Some(extensions) = &original.ak820_offline_extensions {
                use crate::ak820_typed_storage::{Extensions, KeyOverride, TapDance};
                use crate::app::ak820_beta1_profiles::ProfileId;
                let mut records = Extensions::decode_nvm(extensions.nvm()).unwrap();
                let matrix = beta::matrix_profile(entry.profile);
                if matrix.capabilities.runtime_qmk_settings_ids.contains(&2) {
                    records.settings.set(2, 73).unwrap();
                }
                if matrix.capabilities.dynamic_entries.tap_dance > 0 {
                    records.tap_dance[7] = TapDance {
                        outputs: [4, 5, 6, 7],
                        term: 123,
                    };
                }
                if matrix.capabilities.dynamic_entries.key_overrides > 0 {
                    records.key_overrides[7] = KeyOverride {
                        trigger: 4,
                        replacement: 5,
                        layers: 1,
                        mods: 2,
                        negative: 4,
                        suppressed: 8,
                        options: 0x81,
                    };
                }
                let bytes = records
                    .apply_to_nvm(extensions.nvm(), entry.profile)
                    .unwrap();
                original.ak820_offline_extensions = Some(
                    crate::app::ak820_offline_extensions::OfflineExtensions::from_nvm(&bytes)
                        .unwrap(),
                );
            }
            let bytes = export(&original);
            let mut restored = app();
            restored.import_ak820_project_bytes(&bytes).unwrap();
            assert_eq!(restored.ak820_demo_profile, Some(entry.profile));
            assert_eq!(export(&restored), bytes);
            assert!(restored.hid_device.is_none());
            // Imported edits must also survive switching away and back.
            let other = beta::profiles()
                .iter()
                .find(|p| p.profile != entry.profile)
                .unwrap()
                .profile;
            restored.configure_ak820_offline_profile(other);
            restored.configure_ak820_offline_profile(entry.profile);
            assert_eq!(export(&restored), bytes);
        }
    }
    #[test]
    fn invalid_import_preserves_active_and_cached_application_state() {
        let mut target = app();
        for entry in beta::profiles() {
            target.configure_ak820_offline_profile(entry.profile);
        }
        target.keycode_picker.macro_names[0] = "Unsaved".into();
        let before = export(&target);
        let cached: Vec<_> = target
            .ak820_demo_projects
            .iter()
            .map(|p| crate::ak820_offline_project::export_project(p).unwrap())
            .collect();
        let mut doc: serde_json::Value = serde_json::from_slice(&before).unwrap();
        doc["rgb"]["brightness"] = 255.into();
        for raw in [
            serde_json::to_vec(&doc).unwrap(),
            b"{}".to_vec(),
            vec![b' '; MAX_PROJECT_BYTES as usize + 1],
        ] {
            assert!(target.import_ak820_project_bytes(&raw).is_err());
            assert_eq!(export(&target), before);
            assert_eq!(
                target
                    .ak820_demo_projects
                    .iter()
                    .map(|p| crate::ak820_offline_project::export_project(p).unwrap())
                    .collect::<Vec<_>>(),
                cached
            );
        }
    }
    #[test]
    fn closing_page_discards_export_confirmation() {
        let ctx = egui::Context::default();
        let id = egui::Id::new("ak820_local_project_path_entry");
        ctx.data_mut(|d| d.insert_temp(id, PathEntry { save: true, path: "old.json".into(), confirm: true, ..Default::default() }));
        close(&ctx);
        assert!(ctx.data(|d| d.get_temp::<PathEntry>(id)).is_none());
    }

    #[test]
    fn picker_selection_cancel_and_worker_permit() {
        use super::super::file_browser::BrowserResult;
        static BUSY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        let permit = WorkerPermit::acquire(&BUSY).unwrap();
        assert!(WorkerPermit::acquire(&BUSY).is_none());
        drop(permit);
        assert!(WorkerPermit::acquire(&BUSY).is_some());
        let mut form = PathEntry { path: "previous".into(), ..Default::default() };
        form.select(BrowserResult::Cancelled);
        assert_eq!(form.path, "previous");
        form.select(BrowserResult::Selected("input.json".into()));
        assert_eq!(form.path, "input.json");
        form.save = true;
        form.confirm = true;
        form.select(BrowserResult::Directory("folder".into()));
        assert_eq!(std::path::PathBuf::from(&form.path).join("new.json"), std::path::PathBuf::from("folder").join("new.json"));
        assert!(!form.confirm);
    }

    #[test]
    fn stale_result_is_discarded_and_pending_prevents_close() {
        let mut target = app();
        let before = export(&target);
        let mut source = app();
        source.layout.as_mut().unwrap().layers[5][82] = 4u16.into();
        let imported = export(&source);
        assert_ne!(before, imported);
        let ctx = egui::Context::default();
        open(&ctx);
        let (tx, rx) = std::sync::mpsc::channel();
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("ak820_local_project_dialog"), PendingDialog {
            generation: target.connection_generation.wrapping_add(1),
            profile: target.ak820_demo_profile,
            result: std::sync::Arc::new(std::sync::Mutex::new(rx)),
        }));
        close(&ctx);
        assert!(is_open(&ctx));
        assert!(tx.send(Ok(FileOutcome::Loaded(imported))).is_ok());
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| show(&mut target, ui));
        });
        assert!(!is_busy(&ctx));
        assert_eq!(export(&target), before);
        assert!(!target.undo_ak820_project_import());
        close(&ctx);
        assert!(!is_open(&ctx));
    }

    #[test]
    fn closed_page_discards_worker_result() {
        let mut target = app();
        let before = export(&target);
        let ctx = egui::Context::default();
        let (tx, rx) = std::sync::mpsc::channel();
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("ak820_local_project_dialog"), PendingDialog {
            generation: target.connection_generation,
            profile: target.ak820_demo_profile,
            result: std::sync::Arc::new(std::sync::Mutex::new(rx)),
        }));
        assert!(tx.send(Ok(FileOutcome::Loaded(before.clone()))).is_ok());
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| show(&mut target, ui));
        });
        assert!(!is_busy(&ctx));
        assert_eq!(export(&target), before);
        assert!(!target.undo_ak820_project_import());
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_selection_is_not_lossily_imported() {
        use std::os::unix::ffi::OsStringExt;
        let mut form = PathEntry { path: "previous".into(), ..Default::default() };
        form.select(super::super::file_browser::BrowserResult::Selected(
            std::ffi::OsString::from_vec(vec![0xff]).into(),
        ));
        assert!(form.path.is_empty());
        assert!(form.browser_error);
    }

    #[test]
    fn atomic_file_bytes_and_size_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.ak820project.json");
        write_project(&path, b"first").unwrap();
        let bytes = export(&app());
        assert!(write_project(&path, &bytes).is_err());
        assert_eq!(read_project(&path).unwrap(), b"first");
        let new_path = dir.path().join("new.ak820project.json");
        write_project(&new_path, &bytes).unwrap();
        assert_eq!(read_project(&new_path).unwrap(), bytes);
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(MAX_PROJECT_BYTES + 1).unwrap();
        assert!(read_project(&path).is_err());
    }

    #[test]
    fn read_project_rejects_directories_and_fifos() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_project(dir.path()).is_err());
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let fifo = dir.path().join("project.fifo");
            let path = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
            // SAFETY: path is a valid NUL-terminated filesystem path.
            assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
            // A blocking FIFO open here would hang forever because there is no
            // writer; read_project must fail promptly during file preflight.
            let fifo_for_read = fifo.clone();
            let reader = std::thread::spawn(move || read_project(&fifo_for_read));
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            while !reader.is_finished() && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            assert!(reader.is_finished(), "FIFO preflight must not block");
            assert!(reader.join().unwrap().is_err());
        }
    }

    #[test]
    fn worker_outcomes_report_cancel_save_load_errors_and_disconnect() {
        assert!(matches!(
            run_file_operation(false, None, None).unwrap(),
            FileOutcome::Cancelled
        ));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("worker.ak820project.json");
        assert!(matches!(
            run_file_operation(true, Some(path.clone()), Some(b"snapshot".to_vec())).unwrap(),
            FileOutcome::Saved
        ));
        assert!(
            matches!(run_file_operation(false, Some(path), None).unwrap(), FileOutcome::Loaded(bytes) if bytes == b"snapshot")
        );
        let missing = dir.path().join("missing");
        assert!(run_file_operation(false, Some(missing), None).is_err());

        let (tx, rx) = std::sync::mpsc::channel();
        assert!(receive_file_result(rx.try_recv()).is_none());
        drop(tx);
        assert!(receive_file_result(rx.try_recv()).unwrap().is_err());
    }

    #[test]
    fn file_operation_runs_off_caller_thread_and_keeps_export_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("snapshot.ak820project.json");
        let snapshot = b"captured at click".to_vec();
        let worker_path = path.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            release_rx.recv().unwrap();
            let result = run_file_operation(true, Some(worker_path), Some(snapshot));
            let _ = tx.send(result);
        });
        // Hold the worker at a deterministic gate while a complete UI frame runs.
        assert!(receive_file_result(rx.try_recv()).is_none());
        let frame_ran = std::cell::Cell::new(false);
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |_| frame_ran.set(true));
        });
        assert!(frame_ran.get());
        assert!(receive_file_result(rx.try_recv()).is_none());
        release_tx.send(()).unwrap();
        let result = rx.recv().unwrap();
        worker.join().unwrap();
        assert!(matches!(result.unwrap(), FileOutcome::Saved));
        assert_eq!(std::fs::read(path).unwrap(), b"captured at click");
    }

    #[test]
    fn show_polls_worker_without_blocking_and_applies_import_once_with_undo() {
        use crate::app::ak820_beta1_profiles::ProfileId;

        let mut target = app();
        target.configure_ak820_offline_profile(ProfileId::Creative);
        assert!(target.rename_ak820_offline_layer(0, "Before"));
        let before = export(&target);

        let mut source = app();
        source.configure_ak820_offline_profile(ProfileId::Creative);
        assert!(source.rename_ak820_offline_layer(0, "Loaded"));
        let loaded = export(&source);

        let ctx = egui::Context::default();
        open(&ctx);
        let (tx, rx) = std::sync::mpsc::channel();
        let pending_id = egui::Id::new("ak820_local_project_dialog");
        ctx.data_mut(|data| {
            data.insert_temp(
                pending_id,
                PendingDialog {
                    generation: target.connection_generation,
                    profile: target.ak820_demo_profile,
                    result: std::sync::Arc::new(std::sync::Mutex::new(rx)),
                },
            );
        });

        // A pending worker result leaves the dialog operation pending while the
        // production UI frame completes on the GUI thread.
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| show(&mut target, ui));
        });
        assert!(ctx.data(|data| data.get_temp::<PendingDialog>(pending_id).is_some()));
        assert_eq!(export(&target), before);

        assert!(tx.send(Ok(FileOutcome::Loaded(loaded))).is_ok());
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| show(&mut target, ui));
        });
        assert_eq!(target.layer_names[0], "Loaded");
        assert!(ctx.data(|data| data.get_temp::<PendingDialog>(pending_id).is_none()));

        // A further production frame has no pending result to reapply. If it
        // imported twice, the single-step undo would restore the imported state.
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| show(&mut target, ui));
        });
        assert!(target.undo_ak820_project_import());
        assert_eq!(export(&target), before);
    }

    #[test]
    fn show_reports_disconnected_worker_in_local_message() {
        let mut target = app();
        let ctx = egui::Context::default();
        open(&ctx);
        let (tx, rx) = std::sync::mpsc::channel::<Result<FileOutcome, String>>();
        drop(tx);
        ctx.data_mut(|data| {
            data.insert_temp(
                egui::Id::new("ak820_local_project_dialog"),
                PendingDialog {
                    generation: target.connection_generation,
                    profile: target.ak820_demo_profile,
                    result: std::sync::Arc::new(std::sync::Mutex::new(rx)),
                },
            );
        });
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| show(&mut target, ui));
        });
        let message =
            ctx.data(|data| data.get_temp::<String>(egui::Id::new("ak820_local_project_message")));
        assert_eq!(
            message.as_deref(),
            Some("Local project operation worker disconnected")
        );
        assert!(!target.undo_ak820_project_import());
    }
}

#[cfg(test)]
mod import_undo_tests {
    use super::*;
    use crate::app::ak820_beta1_profiles::ProfileId;
    #[test]
    fn same_profile_import_can_restore_complete_previous_project() {
        let cc = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = EntropyApp::new_ak820_demo(&cc);
        app.configure_ak820_offline_profile(ProfileId::Creative);
        app.rename_ak820_offline_layer(0, "Before");
        let original = crate::ak820_offline_project::export_project(
            &app.snapshot_current_ak820_project().unwrap(),
        )
        .unwrap();
        assert!(app.rename_ak820_offline_layer(0, "Imported"));
        let imported = crate::ak820_offline_project::export_project(
            &app.snapshot_current_ak820_project().unwrap(),
        )
        .unwrap();
        app.import_ak820_project_bytes(&original).unwrap();
        assert_eq!(app.layer_names[0], "Before");
        assert!(app.undo_ak820_project_import());
        assert_eq!(
            crate::ak820_offline_project::export_project(
                &app.snapshot_current_ak820_project().unwrap()
            )
            .unwrap(),
            imported
        );
        assert!(!app.undo_ak820_project_import());
        assert!(app.hid_device.is_none());
    }

    #[test]
    fn cross_profile_import_undo_restores_cached_target_edits() {
        let cc = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut source = EntropyApp::new_ak820_demo(&cc);
        source.configure_ak820_offline_profile(ProfileId::Creative);
        assert!(source.rename_ak820_offline_layer(0, "Import edit"));
        let imported = crate::ak820_offline_project::export_project(
            &source.snapshot_current_ak820_project().unwrap(),
        )
        .unwrap();
        let mut app = EntropyApp::new_ak820_demo(&cc);
        app.configure_ak820_offline_profile(ProfileId::Creative);
        assert!(app.rename_ak820_offline_layer(0, "Target edit"));
        let target_before = crate::ak820_offline_project::export_project(
            &app.snapshot_current_ak820_project().unwrap(),
        )
        .unwrap();
        app.configure_ak820_offline_profile(ProfileId::Main);
        assert!(app.rename_ak820_offline_layer(0, "Active edit"));
        let active_before = crate::ak820_offline_project::export_project(
            &app.snapshot_current_ak820_project().unwrap(),
        )
        .unwrap();
        app.configure_ak820_offline_profile(ProfileId::Main);
        app.import_ak820_project_bytes(&imported).unwrap();
        assert!(app.undo_ak820_project_import());
        assert_eq!(app.ak820_demo_profile, Some(ProfileId::Main));
        assert_eq!(
            crate::ak820_offline_project::export_project(
                &app.snapshot_current_ak820_project().unwrap()
            )
            .unwrap(),
            active_before
        );
        app.configure_ak820_offline_profile(ProfileId::Creative);
        assert_eq!(
            crate::ak820_offline_project::export_project(
                &app.snapshot_current_ak820_project().unwrap()
            )
            .unwrap(),
            target_before
        );
    }

    #[test]
    fn cross_profile_import_undo_removes_new_target_cache_entry() {
        let cc = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut source = EntropyApp::new_ak820_demo(&cc);
        source.configure_ak820_offline_profile(ProfileId::Creative);
        let imported = crate::ak820_offline_project::export_project(
            &source.snapshot_current_ak820_project().unwrap(),
        )
        .unwrap();
        let mut app = EntropyApp::new_ak820_demo(&cc);
        app.configure_ak820_offline_profile(ProfileId::Main);
        app.ak820_demo_projects
            .retain(|p| p.profile != ProfileId::Creative);
        assert!(app.rename_ak820_offline_layer(0, "Main edit"));
        app.import_ak820_project_bytes(&imported).unwrap();
        assert!(app
            .ak820_demo_projects
            .iter()
            .any(|p| p.profile == ProfileId::Creative));
        assert!(app.undo_ak820_project_import());
        assert!(!app
            .ak820_demo_projects
            .iter()
            .any(|p| p.profile == ProfileId::Creative));
        assert_eq!(app.ak820_demo_profile, Some(ProfileId::Main));
        assert!(app.hid_device.is_none());
    }

    #[test]
    fn undo_preserves_edits_to_other_profiles_made_after_import() {
        let cc = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut source = EntropyApp::new_ak820_demo(&cc);
        source.configure_ak820_offline_profile(ProfileId::Creative);
        assert!(source.rename_ak820_offline_layer(0, "Imported"));
        let imported = crate::ak820_offline_project::export_project(
            &source.snapshot_current_ak820_project().unwrap(),
        )
        .unwrap();

        let mut app = EntropyApp::new_ak820_demo(&cc);
        app.configure_ak820_offline_profile(ProfileId::Main);
        app.import_ak820_project_bytes(&imported).unwrap();

        // Editing the original active profile after import should survive.
        app.configure_ak820_offline_profile(ProfileId::Main);
        assert!(app.rename_ak820_offline_layer(0, "Main later"));
        let main_after = crate::ak820_offline_project::export_project(
            &app.snapshot_current_ak820_project().unwrap(),
        )
        .unwrap();
        app.configure_ak820_offline_profile(ProfileId::Game);
        assert!(app.rename_ak820_offline_layer(0, "Game later"));
        let game_after_switch = crate::ak820_offline_project::export_project(
            &app.snapshot_current_ak820_project().unwrap(),
        )
        .unwrap();
        assert!(app.undo_ak820_project_import());
        app.configure_ak820_offline_profile(ProfileId::Main);
        assert_eq!(
            crate::ak820_offline_project::export_project(
                &app.snapshot_current_ak820_project().unwrap()
            )
            .unwrap(),
            main_after,
            "cached edits to the original profile must survive undo after switching away"
        );
        app.configure_ak820_offline_profile(ProfileId::Game);
        assert_eq!(
            crate::ak820_offline_project::export_project(
                &app.snapshot_current_ak820_project().unwrap()
            )
            .unwrap(),
            game_after_switch
        );
        app.configure_ak820_offline_profile(ProfileId::Creative);
        assert_ne!(app.layer_names[0], "Imported");

        // A third profile edited after import stays cached through undo too.
        app.configure_ak820_offline_profile(ProfileId::Main);
        app.import_ak820_project_bytes(&imported).unwrap();
        app.configure_ak820_offline_profile(ProfileId::Game);
        assert!(app.rename_ak820_offline_layer(0, "Game again"));
        let game_after = crate::ak820_offline_project::export_project(
            &app.snapshot_current_ak820_project().unwrap(),
        )
        .unwrap();
        assert!(app.undo_ak820_project_import());
        app.configure_ak820_offline_profile(ProfileId::Game);
        assert_eq!(
            crate::ak820_offline_project::export_project(
                &app.snapshot_current_ak820_project().unwrap()
            )
            .unwrap(),
            game_after
        );
    }
}

#[cfg(test)]
mod ak820_local_profile_ui_tests {
    use super::*;
    use egui_kittest::{kittest::Queryable, Harness};

    #[test]
    fn export_requires_confirmation_and_cancel_preserves_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("existing.ak820project.json");
        std::fs::write(&path, b"original").unwrap();
        let cc = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = EntropyApp::new_ak820_demo(&cc);
        app.app_settings.language = crate::i18n::Language::English;
        let mut harness = Harness::new_state(
            |ctx, state: &mut (EntropyApp, bool, String)| {
                if !state.1 {
                    open(ctx);
                    ctx.data_mut(|data| {
                        data.insert_temp(
                            egui::Id::new("ak820_local_project_path_entry"),
                            PathEntry {
                                save: true,
                                path: state.2.clone(),
                                confirm: false,
                                generation: state.0.connection_generation,
                                profile: state.0.ak820_demo_profile,
                                ..Default::default()
                            },
                        )
                    });
                    state.1 = true;
                }
                egui::CentralPanel::default().show(ctx, |ui| show(&mut state.0, ui));
            },
            (app, false, path.to_string_lossy().into_owned()),
        );
        harness.run();
        harness.get_by_label("Continue").click();
        harness.run();
        assert_eq!(std::fs::read(&path).unwrap(), b"original");
        harness.get_by_label("Cancel").click();
        harness.run();
        assert_eq!(std::fs::read(&path).unwrap(), b"original");
    }

    #[test]
    fn local_project_profile_selector_opens_typed_example_without_usb() {
        let cc = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = EntropyApp::new_ak820_demo(&cc);
        app.app_settings.language = crate::i18n::Language::Russian;
        let mut harness = Harness::new_state(
            |ctx, app| {
                open(ctx);
                egui::CentralPanel::default().show(ctx, |ui| show(app, ui));
            },
            app,
        );
        harness.run();
        harness.get_by_value("Профиль: main").click();
        harness.run();
        harness.get_by_label("Монтаж и графика").click();
        harness.run();
        assert_eq!(
            harness.state().ak820_demo_profile,
            Some(super::super::ak820_beta1_profiles::ProfileId::Creative)
        );
        assert!(harness.state().ak820_offline_extensions.is_some());
        assert!(harness.state().hid_device.is_none());
    }
}
