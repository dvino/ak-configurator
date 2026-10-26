// SPDX-License-Identifier: GPL-3.0-or-later
//! Local-only checks for three explicitly selected AK820 installer inputs.
use crate::ak820_installer::{CheckedLocalPackage, LocalPlanSummary, SnapshotKind};
use crate::app::ak820_beta1_profiles::{self as beta, ProfileId};
use crate::app::EntropyApp;
use std::{
    path::Path,
    sync::{mpsc, Arc, Mutex},
};

pub(crate) const OPEN_ID: &str = "ak820_install_files_open";
const STATE_ID: &str = "ak820_install_files_state";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Slot {
    Payload,
    Snapshot,
    Recovery,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Problem {
    Open,
    Read,
    Size,
    Content,
    Background,
}
fn problem_text(problem: Problem, ru: bool) -> &'static str {
    match (problem, ru) {
        (Problem::Open, false) => {
            "Could not open a selected file. Choose an accessible local file."
        }
        (Problem::Open, true) => {
            "Не удалось открыть выбранный файл. Выберите доступный локальный файл."
        }
        (Problem::Read, false) => {
            "Could not read a selected file. Check local file access and try again."
        }
        (Problem::Read, true) => {
            "Не удалось прочитать выбранный файл. Проверьте доступ к нему и повторите попытку."
        }
        (Problem::Size, false) => {
            "A selection is not a regular file of the required exact size: firmware 51,136 bytes; each ROM copy 65,536 bytes."
        }
        (Problem::Size, true) => {
            "Выбранный файл не является обычным файлом нужного размера: прошивка — 51 136 байт, каждая копия ROM — 65 536 байт."
        }
        (Problem::Content, false) => {
            "Content checks failed. Verify the selected profile, official prepared firmware, complete current snapshot, and personal OEM recovery copy."
        }
        (Problem::Content, true) => {
            "Проверка содержимого не пройдена. Проверьте профиль, подготовленную официальную прошивку, полный текущий снимок и личную OEM-копию восстановления."
        }
        (Problem::Background, false) => {
            "The local check stopped unexpectedly. Select the files again and retry."
        }
        (Problem::Background, true) => {
            "Локальная проверка неожиданно остановилась. Выберите файлы ещё раз и повторите попытку."
        }
    }
}
#[derive(Clone)]
struct Input {
    name: String,
    bytes: Vec<u8>,
}
#[derive(Clone)]
struct Message {
    token: u64,
    slot: Option<Slot>,
    result: Result<Option<Input>, Problem>,
    checked: Option<Result<LocalPlanSummary, Problem>>,
}
#[derive(Clone)]
struct Pending {
    token: u64,
    rx: Arc<Mutex<mpsc::Receiver<Message>>>,
}
#[derive(Clone)]
struct State {
    token: u64,
    target: ProfileId,
    payload: Option<Input>,
    snapshot: Option<Input>,
    recovery: Option<Input>,
    summary: Option<LocalPlanSummary>,
    error: Option<Problem>,
    pending: Option<Pending>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            token: 0,
            target: ProfileId::Main,
            payload: None,
            snapshot: None,
            recovery: None,
            summary: None,
            error: None,
            pending: None,
        }
    }
}
impl State {
    fn invalidate(&mut self) {
        self.token = self.token.wrapping_add(1);
        self.summary = None;
        self.error = None;
        self.pending = None;
    }
    fn set(&mut self, slot: Slot, value: Input) {
        match slot {
            Slot::Payload => self.payload = Some(value),
            Slot::Snapshot => self.snapshot = Some(value),
            Slot::Recovery => self.recovery = Some(value),
        }
        self.invalidate();
    }
    fn clear(&mut self, slot: Slot) {
        match slot {
            Slot::Payload => self.payload = None,
            Slot::Snapshot => self.snapshot = None,
            Slot::Recovery => self.recovery = None,
        }
        self.invalidate();
    }
}
fn tr<'a>(ru: bool, en: &'a str, ru_text: &'a str) -> &'a str {
    if ru {
        ru_text
    } else {
        en
    }
}
fn short_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .chars()
        .filter(|c| !c.is_control())
        .take(72)
        .collect()
}

// Open without following links/reparse points, then require a regular file and
// enforce both the expected bound and EOF. No file contents or paths are logged.
fn read_bounded(path: &Path, expected: usize) -> Result<Input, Problem> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000);
    } // FILE_FLAG_OPEN_REPARSE_POINT
    let file = options.open(path).map_err(|_| Problem::Open)?;
    let meta = file.metadata().map_err(|_| Problem::Open)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(Problem::Size);
        }
    }
    if !meta.is_file() || meta.len() != expected as u64 {
        return Err(Problem::Size);
    }
    let mut data = Vec::with_capacity(expected);
    file.take(expected as u64 + 1)
        .read_to_end(&mut data)
        .map_err(|_| Problem::Read)?;
    if data.len() != expected {
        return Err(Problem::Size);
    }
    Ok(Input {
        name: short_name(path),
        bytes: data,
    })
}

pub(super) fn read_input_bytes(
    path: &Path,
    expected: usize,
    russian: bool,
) -> Result<Vec<u8>, String> {
    read_bounded(path, expected)
        .map(|input| input.bytes)
        .map_err(|problem| problem_text(problem, russian).to_owned())
}
use std::io::Read;

fn slot_expected(slot: Slot) -> usize {
    match slot {
        Slot::Payload => 51136,
        Slot::Snapshot | Slot::Recovery => 65536,
    }
}
fn apply_message(state: &mut State, msg: Message) -> bool {
    if state.pending.as_ref().map(|p| p.token) != Some(msg.token) || state.token != msg.token {
        return false;
    }
    state.pending = None;
    if let Some(slot) = msg.slot {
        match msg.result {
            Ok(Some(input)) => state.set(slot, input),
            Ok(None) => {}
            Err(e) => state.error = Some(e),
        }
    }
    if let Some(result) = msg.checked {
        match result {
            Ok(summary) => state.summary = Some(summary),
            Err(_) => state.error = Some(Problem::Content),
        }
    }
    true
}

fn snapshot_kind_text(kind: SnapshotKind, ru: bool) -> String {
    match kind {
        SnapshotKind::Oem => tr(ru, "Supported OEM", "Поддерживаемая OEM").to_owned(),
        SnapshotKind::Project(profile) => beta::profile_description(profile)
            .title
            .for_language(ru)
            .to_owned(),
    }
}

fn draw_contents(ui: &mut egui::Ui, ru: bool, state: &mut State) -> (Option<Slot>, bool, bool) {
    let mut pick = None;
    let mut check = false;
    let mut close = false;
    ui.label(tr(
        ru,
        "Checks the selected files locally. Use Install firmware for a separate device-bound installation plan.",
        "Проверяет выбранные файлы локально. Для установки используйте отдельное окно «Установить прошивку» с проверкой устройства.",
    ));
    ui.horizontal(|ui| {
        ui.label(tr(ru, "Target profile", "Целевой профиль"));
        egui::ComboBox::from_id_salt("ak820_local_target")
            .selected_text(
                beta::profile_description(state.target)
                    .title
                    .for_language(ru),
            )
            .show_ui(ui, |ui| {
                for profile in beta::profiles() {
                    let id = profile.profile;
                    if ui
                        .selectable_value(
                            &mut state.target,
                            id,
                            beta::profile_description(id).title.for_language(ru),
                        )
                        .changed()
                    {
                        state.invalidate();
                    }
                }
            });
    });

    let rows = [
        (
            Slot::Payload,
            "Prepared firmware file",
            "Подготовленный файл прошивки",
            "51,136 B",
        ),
        (
            Slot::Snapshot,
            "Current complete snapshot",
            "Текущий полный снимок",
            "65,536 B",
        ),
        (
            Slot::Recovery,
            "Personal OEM recovery copy",
            "Личная OEM-копия восстановления",
            "65,536 B",
        ),
    ];
    for (slot, english, russian, size) in rows {
        ui.horizontal_wrapped(|ui| {
            ui.label(tr(ru, english, russian));
            if ui
                .add_enabled(
                    state.pending.is_none(),
                    egui::Button::new(tr(ru, "Choose…", "Выбрать…")),
                )
                .clicked()
            {
                pick = Some(slot);
            }
            let input = match slot {
                Slot::Payload => &state.payload,
                Slot::Snapshot => &state.snapshot,
                Slot::Recovery => &state.recovery,
            };
            if let Some(input) = input {
                ui.label(format!("{} · {size}", input.name));
                if ui.small_button(tr(ru, "Clear", "Убрать")).clicked() {
                    state.clear(slot);
                }
            } else {
                ui.label(size);
            }
        });
    }

    ui.horizontal(|ui| {
        let ready = state.payload.is_some()
            && state.snapshot.is_some()
            && state.recovery.is_some()
            && state.pending.is_none();
        if ui
            .add_enabled(
                ready,
                egui::Button::new(tr(ru, "Check files", "Проверить файлы")),
            )
            .clicked()
        {
            check = true;
        }
        if state.pending.is_some() {
            ui.spinner();
            ui.label(tr(
                ru,
                "Checking local files…",
                "Проверка локальных файлов…",
            ));
        }
    });

    if let Some(problem) = state.error {
        ui.colored_label(ui.visuals().error_fg_color, problem_text(problem, ru));
    }
    if let Some(summary) = &state.summary {
        ui.separator();
        ui.strong(tr(
            ru,
            "Local byte checks passed",
            "Локальная проверка байтов пройдена",
        ));
        ui.label(format!(
            "{}: {}",
            tr(ru, "Target", "Цель"),
            beta::profile_description(summary.target)
                .title
                .for_language(ru)
        ));
        ui.label(format!(
            "{}: {}",
            tr(ru, "Snapshot kind", "Тип снимка"),
            snapshot_kind_text(summary.snapshot_kind, ru)
        ));
        ui.label(format!(
            "{}: {} B",
            tr(ru, "Firmware payload", "Файл прошивки"),
            summary.application_bytes
        ));
        ui.label(format!(
            "{}: {}",
            tr(ru, "Data blocks", "Блоки данных"),
            summary.data_blocks
        ));
        ui.label(tr(
            ru,
            "These checks verify bytes only; they do not confirm this snapshot came from the current physical keyboard.",
            "Проверка подтверждает только байты; она не подтверждает, что снимок получен с текущей физической клавиатуры.",
        ));
        ui.collapsing(
            tr(ru, "Hashes and details", "Хеши и сведения"),
            |ui| {
                ui.label(format!(
                    "{}: {} B",
                    tr(ru, "Payload size", "Размер прошивки"),
                    summary.application_bytes
                ));
                ui.label(format!(
                    "{}: 65,536 B",
                    tr(ru, "Snapshot size", "Размер снимка")
                ));
                ui.label(format!(
                    "{}: 65,536 B",
                    tr(ru, "Recovery size", "Размер копии восстановления")
                ));
                ui.monospace(format!(
                    "{}: {}",
                    tr(ru, "Payload SHA-256", "SHA-256 прошивки"),
                    summary.payload_sha256
                ));
                ui.monospace(format!(
                    "{}: {}",
                    tr(ru, "Snapshot SHA-256", "SHA-256 снимка"),
                    summary.snapshot_sha256
                ));
                ui.monospace(format!(
                    "{}: {}",
                    tr(ru, "Recovery SHA-256", "SHA-256 копии восстановления"),
                    summary.recovery_sha256
                ));
                ui.monospace(format!(
                    "{}: {}",
                    tr(ru, "Expected ROM SHA-256", "Ожидаемый SHA-256 ROM"),
                    summary.expected_rom_sha256
                ));
                ui.monospace(format!(
                    "{}: {}",
                    tr(ru, "Input binding SHA-256", "SHA-256 связи входных данных"),
                    summary.binding_sha256
                ));
                ui.monospace(format!(
                    "{}: 0x{:08x}",
                    tr(ru, "Loader checksum", "Контрольная сумма загрузчика"),
                    summary.loader_checksum
                ));
            },
        );
    }
    if ui.button(tr(ru, "Close", "Закрыть")).clicked() {
        close = true;
    }
    (pick, check, close)
}

pub(crate) fn show(_app: &mut EntropyApp, ctx: &egui::Context) {
    if !ctx.data(|d| d.get_temp::<bool>(egui::Id::new(OPEN_ID)).unwrap_or(false)) {
        // Fence caller-driven close as well as the Close button below.
        ctx.data_mut(|d| d.insert_temp(egui::Id::new(STATE_ID), State::default()));
        return;
    }
    let ru = _app.app_settings.language == crate::i18n::Language::Russian;
    let mut s = ctx
        .data(|d| d.get_temp::<State>(egui::Id::new(STATE_ID)))
        .unwrap_or_default();
    if let Some(p) = &s.pending {
        let outcome = {
            let receiver = p.rx.lock().unwrap_or_else(|e| e.into_inner());
            receiver.try_recv()
        };
        match outcome {
            Ok(m) => {
                apply_message(&mut s, m);
            }
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(100))
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                s.pending = None;
                s.error = Some(Problem::Background);
            }
        }
    }
    let mut open = true;
    let (pick, check, close) = egui::Window::new(tr(
        ru,
        "Check AK820 local files",
        "Проверка локальных файлов AK820",
    ))
    .id(egui::Id::new("ak820_install_files_window"))
    .open(&mut open)
    .resizable(true)
    .default_width(620.0)
    .max_width((ctx.content_rect().width() - 32.0).max(260.0))
    .max_height((ctx.content_rect().height() - 48.0).max(100.0))
    .vscroll(true)
    .show(ctx, |ui| draw_contents(ui, ru, &mut s))
    .and_then(|response| response.inner)
    .unwrap_or_default();
    if !open || close {
        s.invalidate();
        ctx.data_mut(|d| {
            d.insert_temp(egui::Id::new(OPEN_ID), false);
            d.insert_temp(egui::Id::new(STATE_ID), State::default());
        });
        return;
    }
    if let Some(slot) = pick {
        let token = s.token.wrapping_add(1);
        s.token = token;
        s.summary = None;
        s.error = None;
        let (tx, rx) = mpsc::channel();
        let rx = Arc::new(Mutex::new(rx));
        s.pending = Some(Pending { token, rx });
        std::thread::spawn(move || {
            let chosen = rfd::FileDialog::new().pick_file();
            let result = match chosen {
                Some(path) => read_bounded(&path, slot_expected(slot)).map(Some),
                None => Ok(None),
            };
            let _ = tx.send(Message {
                token,
                slot: Some(slot),
                result,
                checked: None,
            });
        });
    }
    if check {
        if let (Some(p), Some(snap), Some(rec)) = (&s.payload, &s.snapshot, &s.recovery) {
            let (tx, rx) = mpsc::channel();
            let rx = Arc::new(Mutex::new(rx));
            let token = s.token.wrapping_add(1);
            s.token = token;
            s.pending = Some(Pending { token, rx });
            s.summary = None;
            s.error = None;
            let profile = s.target;
            let payload = p.bytes.clone();
            let snapshot = snap.bytes.clone();
            let recovery = rec.bytes.clone();
            std::thread::spawn(move || {
                let result = CheckedLocalPackage::prepare(profile, &payload, &snapshot, &recovery)
                    .map(|p| p.summary().clone())
                    .map_err(|_| Problem::Content);
                let _ = tx.send(Message {
                    token,
                    slot: None,
                    result: Ok(None),
                    checked: Some(result),
                });
            });
        }
    }
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(STATE_ID), s));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_reader_rejects_wrong_sizes_and_directories() {
        let base = std::env::temp_dir().join(format!("ak820-local-check-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&base);
        let file = base.join("bad.bin");
        std::fs::write(&file, [1u8; 17]).unwrap();
        assert!(matches!(read_bounded(&file, 16), Err(Problem::Size)));
        assert!(matches!(
            read_bounded(&base, 16),
            Err(Problem::Open | Problem::Size)
        ));
        std::fs::write(&file, [1u8; 16]).unwrap();
        let got = read_bounded(&file, 16).unwrap();
        assert_eq!(got.bytes.len(), 16);
        assert_eq!(got.name, "bad.bin");
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn each_problem_has_localized_safe_text() {
        for problem in [
            Problem::Open,
            Problem::Read,
            Problem::Size,
            Problem::Content,
            Problem::Background,
        ] {
            let english = problem_text(problem, false);
            let russian = problem_text(problem, true);
            assert_ne!(english, russian);
            assert!(!english.contains('/'));
            assert!(!russian.contains('/'));
        }
        assert!(problem_text(Problem::Content, false).contains("official prepared firmware"));
        assert!(problem_text(Problem::Content, true).contains("личную OEM-копию"));
    }
    #[test]
    fn invalidated_requests_cannot_apply_stale_results() {
        let mut s = State::default();
        let (tx, rx) = mpsc::channel();
        let token = 1;
        s.token = token;
        s.pending = Some(Pending {
            token,
            rx: Arc::new(Mutex::new(rx)),
        });
        s.invalidate();
        assert!(!apply_message(
            &mut s,
            Message {
                token,
                slot: Some(Slot::Payload),
                result: Ok(Some(Input {
                    name: "late".into(),
                    bytes: vec![1]
                })),
                checked: None
            }
        ));
        assert!(s.payload.is_none());
        drop(tx);
    }

    #[test]
    fn local_file_window_fits_800_by_500_in_both_languages_without_hid() {
        for language in [
            crate::i18n::Language::English,
            crate::i18n::Language::Russian,
        ] {
            let ctx = egui::Context::default();
            let cc = eframe::CreationContext::_new_kittest(ctx.clone());
            let mut app = EntropyApp::new_ak820_demo(&cc);
            app.app_settings.language = language;
            ctx.data_mut(|data| {
                data.insert_temp(egui::Id::new(OPEN_ID), true);
                data.insert_temp(
                    egui::Id::new(STATE_ID),
                    State {
                        error: Some(Problem::Content),
                        ..State::default()
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
                .memory(|memory| memory.area_rect(egui::Id::new("ak820_install_files_window")))
                .unwrap();
            assert!(rect.width() <= 800.0 && rect.height() <= 500.0, "{rect:?}");
            assert!(app.hid_device.is_none());
        }
    }
}
