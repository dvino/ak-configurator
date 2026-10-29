use super::*;

/// Which follow-up action to run once the inline path entry returns a path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileDialogAction {
    ImportApplicationLayout(String),
    ExportApplicationLayout(String),
    ImportEntlayout,
    ExportEntlayout,
    ImportEntsettings,
    ExportEntsettings,
    ExportLayoutImage,
    StartupImage,
    StandbyBackground,
    Pictogram,
    ImportPictograms,
    ExportPictograms,
}

impl FileDialogAction {
    /// Whether the follow-up reads or writes the currently connected device.
    /// These must be rejected if the active device changed while the picker was
    /// open. App-settings import/export are device-independent.
    fn is_device_scoped(self) -> bool {
        match self {
            FileDialogAction::ImportApplicationLayout(_)
            | FileDialogAction::ExportApplicationLayout(_)
            | FileDialogAction::ImportEntlayout
            | FileDialogAction::ExportEntlayout
            | FileDialogAction::ExportLayoutImage
            | FileDialogAction::StartupImage
            | FileDialogAction::StandbyBackground
            | FileDialogAction::Pictogram => true,
            FileDialogAction::ImportEntsettings
            | FileDialogAction::ExportEntsettings
            | FileDialogAction::ImportPictograms
            | FileDialogAction::ExportPictograms => false,
        }
    }
}

/// A device-scoped operation captured at `opened_generation` is stale once the
/// active device changes to `current_generation`. Used both at dialog
/// completion and again just before the deferred `.entlayout` firmware write,
/// since the device can also change during that later gap.
pub(super) fn device_generation_stale(opened_generation: u64, current_generation: u64) -> bool {
    opened_generation != current_generation
}

/// Whether a completed dialog result must be discarded because the active device
/// changed while the picker was open. App-settings actions are never rejected.
fn dialog_result_stale(
    action: FileDialogAction,
    opened_generation: u64,
    current_generation: u64,
) -> bool {
    action.is_device_scoped() && device_generation_stale(opened_generation, current_generation)
}

/// A new dialog may start only when none is in flight — the single-slot
/// invariant that stops overlapping portal round-trips from racing each other.
#[cfg(not(target_arch = "wasm32"))]
fn can_start_file_dialog(dialog_in_flight: bool) -> bool {
    !dialog_in_flight
}

/// The decision a single `poll_file_dialog` step makes for one `try_recv`
/// outcome. Split out from the method so the whole lifecycle — success, cancel,
/// stale device, and a lost worker — is testable without a live dialog thread or
/// an egui context.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, PartialEq, Eq)]
pub(super) enum FileDialogPoll {
    /// No result yet; keep waiting.
    Pending,
    /// The user closed the dialog without choosing a file.
    Cancelled,
    /// A path was chosen, but the active device changed meanwhile; discard it.
    StaleDevice,
    /// A path was chosen and is safe to dispatch.
    Dispatch(std::path::PathBuf),
    /// The worker vanished (e.g. panicked) without ever sending a result.
    WorkerLost,
}

/// Classify one channel poll into the next lifecycle step. `Pending` is the only
/// non-terminal outcome; every other result frees the dialog slot.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn classify_file_dialog_poll(
    recv: Result<Option<std::path::PathBuf>, std::sync::mpsc::TryRecvError>,
    action: FileDialogAction,
    opened_generation: u64,
    current_generation: u64,
) -> FileDialogPoll {
    match recv {
        Ok(Some(path)) => {
            if dialog_result_stale(action, opened_generation, current_generation) {
                FileDialogPoll::StaleDevice
            } else {
                FileDialogPoll::Dispatch(path)
            }
        }
        Ok(None) => FileDialogPoll::Cancelled,
        Err(std::sync::mpsc::TryRecvError::Empty) => FileDialogPoll::Pending,
        Err(std::sync::mpsc::TryRecvError::Disconnected) => FileDialogPoll::WorkerLost,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_scoped_actions_are_classified_correctly() {
        assert!(FileDialogAction::ImportEntlayout.is_device_scoped());
        assert!(FileDialogAction::ExportEntlayout.is_device_scoped());
        assert!(FileDialogAction::ExportLayoutImage.is_device_scoped());
        assert!(!FileDialogAction::ImportEntsettings.is_device_scoped());
        assert!(!FileDialogAction::ExportEntsettings.is_device_scoped());
    }

    #[test]
    fn device_scoped_result_is_stale_only_when_generation_changed() {
        // Same generation: apply.
        assert!(!dialog_result_stale(
            FileDialogAction::ImportEntlayout,
            7,
            7
        ));
        assert!(!dialog_result_stale(
            FileDialogAction::ExportEntlayout,
            7,
            7
        ));
        // Device changed while picker open: reject.
        assert!(dialog_result_stale(FileDialogAction::ImportEntlayout, 7, 8));
        assert!(dialog_result_stale(
            FileDialogAction::ExportLayoutImage,
            7,
            8
        ));
    }

    #[test]
    fn app_settings_result_never_stale() {
        // App settings are device-independent, so a generation change is fine.
        assert!(!dialog_result_stale(
            FileDialogAction::ImportEntsettings,
            1,
            99
        ));
        assert!(!dialog_result_stale(
            FileDialogAction::ExportEntsettings,
            1,
            99
        ));
    }

    #[test]
    fn deferred_import_generation_check() {
        // The deferred .entlayout write in handle_pending_imports applies the
        // same rule: proceed only if the device is unchanged since the pick.
        assert!(!device_generation_stale(3, 3));
        assert!(device_generation_stale(3, 4));
    }

    #[cfg(not(target_arch = "wasm32"))]
    use std::path::PathBuf;
    #[cfg(not(target_arch = "wasm32"))]
    use std::sync::mpsc::TryRecvError;

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn image_confirmation_uses_the_actual_normalized_destination() {
        assert_eq!(destination("report.txt", Some("png")), PathBuf::from("report.png"));
        assert_eq!(destination("report.PNG", Some("png")), PathBuf::from("report.PNG"));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn every_export_uses_its_expected_extension() {
        for (action, extension) in [
            (FileDialogAction::ExportApplicationLayout("profile".into()), "entlayout"),
            (FileDialogAction::ExportEntlayout, "entlayout"),
            (FileDialogAction::ExportEntsettings, "entsettings"),
            (FileDialogAction::ExportPictograms, "json"),
            (FileDialogAction::ExportLayoutImage, "svg"),
        ] {
            assert_eq!(destination("backup", export_extension(&action, "svg")), PathBuf::from(format!("backup.{extension}")));
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn save_requires_confirmation_and_a_path() {
        assert!(!path_can_submit("", true, true));
        assert!(!path_can_submit("existing.txt", true, false));
        assert!(path_can_submit("existing.txt", true, true));
        assert!(path_can_submit("input.txt", false, false));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn browser_selection_only_populates_path_and_requires_new_save_confirmation() {
        use super::super::file_browser::BrowserResult;
        let (sender, _receiver) = std::sync::mpsc::channel();
        let mut dialog = InlineFileDialog {
            sender, path: "original".into(), save: true, confirmed: true,
            extension: None, action: FileDialogAction::ExportEntlayout,
            browser: None, browser_error: false,
        };
        dialog.finish_browsing(BrowserResult::Cancelled);
        assert_eq!(dialog.path, "original");
        assert!(dialog.confirmed);
        dialog.finish_browsing(BrowserResult::Selected(PathBuf::from("chosen.entlayout")));
        assert_eq!(dialog.path, "chosen.entlayout");
        assert!(!path_can_submit(&dialog.path, dialog.save, dialog.confirmed));
        assert!(_receiver.try_recv().is_err());
        dialog.confirmed = true;
        dialog.finish_browsing(BrowserResult::Directory(PathBuf::from("folder")));
        assert_eq!(PathBuf::from(&dialog.path).join("new.entlayout"), PathBuf::from("folder").join("new.entlayout"));
        assert!(dialog.path.ends_with(std::path::MAIN_SEPARATOR));
        assert!(!dialog.confirmed);
    }

    #[cfg(all(unix, not(target_arch = "wasm32")))]
    #[test]
    fn non_utf8_browser_selection_does_not_replace_path() {
        use std::os::unix::ffi::OsStringExt;
        use super::super::file_browser::BrowserResult;
        let (sender, _receiver) = std::sync::mpsc::channel();
        let mut dialog = InlineFileDialog {
            sender, path: "original".into(), save: false, confirmed: false,
            extension: None, action: FileDialogAction::ImportEntlayout,
            browser: None, browser_error: false,
        };
        dialog.finish_browsing(BrowserResult::Selected(PathBuf::from(std::ffi::OsString::from_vec(vec![0xff]))));
        assert_eq!(dialog.path, "original");
        assert!(dialog.browser_error);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn only_one_dialog_runs_at_a_time() {
        // No dialog in flight → a new one may start; one already running → not.
        assert!(can_start_file_dialog(false));
        assert!(!can_start_file_dialog(true));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn poll_pending_keeps_waiting_without_freeing_the_slot() {
        let poll = classify_file_dialog_poll(
            Err(TryRecvError::Empty),
            FileDialogAction::ImportEntlayout,
            5,
            5,
        );
        assert_eq!(poll, FileDialogPoll::Pending);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn poll_success_dispatches_the_chosen_path() {
        let path = PathBuf::from("/tmp/layout.entlayout");
        let poll = classify_file_dialog_poll(
            Ok(Some(path.clone())),
            FileDialogAction::ImportEntlayout,
            5,
            5,
        );
        assert_eq!(poll, FileDialogPoll::Dispatch(path));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn poll_cancel_frees_the_slot_without_dispatching() {
        let poll = classify_file_dialog_poll(Ok(None), FileDialogAction::ExportLayoutImage, 5, 5);
        assert_eq!(poll, FileDialogPoll::Cancelled);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn poll_stale_device_discards_a_device_scoped_result() {
        // Device changed (5 → 6) while a device-scoped picker was open.
        let poll = classify_file_dialog_poll(
            Ok(Some(PathBuf::from("/tmp/b.entlayout"))),
            FileDialogAction::ExportEntlayout,
            5,
            6,
        );
        assert_eq!(poll, FileDialogPoll::StaleDevice);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn poll_device_change_still_dispatches_app_settings() {
        // App-settings actions are device-independent, so a generation change
        // does not make the result stale.
        let path = PathBuf::from("/tmp/settings.entsettings");
        let poll = classify_file_dialog_poll(
            Ok(Some(path.clone())),
            FileDialogAction::ImportEntsettings,
            5,
            6,
        );
        assert_eq!(poll, FileDialogPoll::Dispatch(path));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn poll_worker_disconnect_is_surfaced_not_swallowed() {
        let poll = classify_file_dialog_poll(
            Err(TryRecvError::Disconnected),
            FileDialogAction::ImportEntlayout,
            5,
            5,
        );
        assert_eq!(poll, FileDialogPoll::WorkerLost);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn every_terminal_poll_outcome_frees_the_slot() {
        // Only `Pending` keeps the dialog slot occupied; all other outcomes are
        // terminal so the single-slot invariant can never wedge permanently.
        let terminal = [
            classify_file_dialog_poll(Ok(None), FileDialogAction::ImportEntlayout, 1, 1),
            classify_file_dialog_poll(
                Ok(Some(PathBuf::from("/tmp/x"))),
                FileDialogAction::ImportEntlayout,
                1,
                1,
            ),
            classify_file_dialog_poll(
                Ok(Some(PathBuf::from("/tmp/x"))),
                FileDialogAction::ExportEntlayout,
                1,
                2,
            ),
            classify_file_dialog_poll(
                Err(TryRecvError::Disconnected),
                FileDialogAction::ImportEntlayout,
                1,
                1,
            ),
        ];
        for outcome in terminal {
            assert_ne!(outcome, FileDialogPoll::Pending);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct InlineFileDialog {
    sender: std::sync::mpsc::Sender<Option<std::path::PathBuf>>,
    path: String,
    save: bool,
    confirmed: bool,
    extension: Option<&'static str>,
    action: FileDialogAction,
    browser: Option<super::file_browser::FileBrowser>,
    browser_error: bool,
}

#[cfg(not(target_arch = "wasm32"))]
impl InlineFileDialog {
    fn finish_browsing(&mut self, selection: super::file_browser::BrowserResult) {
        use super::file_browser::BrowserResult;
        let selected = match selection {
            BrowserResult::Selected(path) => Some((path, false)),
            BrowserResult::Directory(path) => Some((path, true)),
            BrowserResult::Cancelled => None,
        };
        if let Some((path, directory)) = selected {
            if let Some(path) = browser_selection_path(&path, directory) {
                self.path = path;
                self.confirmed = false;
                self.browser_error = false;
            } else {
                self.browser_error = true;
            }
        }
        self.browser = None;
    }
}

#[cfg(not(target_arch = "wasm32"))]
thread_local! {
    // The native app has one UI thread and one main window.
    static INLINE_FILE_DIALOG: std::cell::RefCell<Option<InlineFileDialog>> = const { std::cell::RefCell::new(None) };
}

#[cfg(not(target_arch = "wasm32"))]
fn browser_selection_path(path: &std::path::Path, directory: bool) -> Option<String> {
    let mut path = path.to_path_buf();
    if directory {
        path.push("");
    }
    path.to_str().map(str::to_owned)
}

#[cfg(not(target_arch = "wasm32"))]
fn path_can_submit(path: &str, save: bool, confirmed: bool) -> bool {
    !path.trim().is_empty() && (!save || confirmed)
}

#[cfg(not(target_arch = "wasm32"))]
fn export_extension(action: &FileDialogAction, image_extension: &'static str) -> Option<&'static str> {
    match action {
        FileDialogAction::ExportApplicationLayout(_) | FileDialogAction::ExportEntlayout => Some("entlayout"),
        FileDialogAction::ExportEntsettings => Some("entsettings"),
        FileDialogAction::ExportPictograms => Some("json"),
        FileDialogAction::ExportLayoutImage => Some(image_extension),
        _ => None,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn destination(path: &str, extension: Option<&str>) -> std::path::PathBuf {
    let mut path = std::path::PathBuf::from(path);
    if let Some(extension) = extension {
        if !path.extension().and_then(|v| v.to_str()).is_some_and(|v| v.eq_ignore_ascii_case(extension)) {
            path.set_extension(extension);
        }
    }
    path
}

#[cfg(test)]
thread_local! {
    static TEST_DIALOG_RESULT: std::cell::RefCell<Option<Option<std::path::PathBuf>>> = const { std::cell::RefCell::new(None) };
    static TEST_DIALOG_OPTIONS: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// Replace only the native portal boundary in UI click tests.
#[cfg(test)]
pub(crate) fn set_test_dialog_result(path: Option<std::path::PathBuf>) {
    TEST_DIALOG_RESULT.with(|result| *result.borrow_mut() = Some(path));
}

#[cfg(test)]
pub(crate) fn test_dialog_options() -> String {
    TEST_DIALOG_OPTIONS.with(|options| options.borrow().clone())
}

impl EntropyApp {
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn cache_parent_window_handles(&mut self, frame: &eframe::Frame) {
        use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
        if let Ok(handle) = frame.window_handle() {
            self.parent_window_handle = Some(handle.as_raw());
        }
        if let Ok(handle) = frame.display_handle() {
            self.parent_display_handle = Some(handle.as_raw());
        }
    }

    /// Start one inline path request; the caller renders it in the main window.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn spawn_file_dialog(
        &mut self,
        action: FileDialogAction,
        dialog: rfd::FileDialog,
        save: bool,
    ) {
        self.spawn_file_dialog_with_default(action, dialog, save, None);
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn spawn_file_dialog_with_default(
        &mut self,
        action: FileDialogAction,
        dialog: rfd::FileDialog,
        save: bool,
        default_name: Option<&str>,
    ) {
        if !can_start_file_dialog(self.pending_file_dialog.is_some()) {
            return;
        }
        INLINE_FILE_DIALOG.with(|slot| *slot.borrow_mut() = None);
        let (tx, rx) = std::sync::mpsc::channel();
        #[cfg(test)]
        if let Some(path) = TEST_DIALOG_RESULT.with(|result| result.borrow_mut().take()) {
            TEST_DIALOG_OPTIONS.with(|options| *options.borrow_mut() = format!("{dialog:?}"));
            tx.send(path).unwrap();
            self.pending_file_dialog = Some((action, self.connection_generation, rx));
            return;
        }
        let _ = dialog;
        let extension = if save {
            export_extension(&action, super::layout_image_export::layout_image_export_descriptor(self.app_settings.layout_image_export.format).0)
        } else {
            None
        };
        INLINE_FILE_DIALOG.with(|slot| {
            *slot.borrow_mut() = Some(InlineFileDialog {
                sender: tx,
                path: default_name.unwrap_or_default().to_owned(),
                save,
                confirmed: false,
                extension,
                action: action.clone(),
                browser: None,
                browser_error: false,
            });
        });
        self.pending_file_dialog = Some((action, self.connection_generation, rx));
    }

    /// Render inside the main window while a request is pending.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn show_inline_file_dialog(&mut self, ui: &mut egui::Ui) {
        if self.pending_file_dialog.is_none() {
            INLINE_FILE_DIALOG.with(|slot| *slot.borrow_mut() = None);
            return;
        }
        let russian = self.app_settings.language == crate::i18n::Language::Russian;
        INLINE_FILE_DIALOG.with(|slot| {
            let mut slot = slot.borrow_mut();
            let Some(dialog) = slot.as_mut() else { return };
            let mut result = None;
            ui.group(|ui| {
                ui.heading(if russian { "Путь к файлу" } else { "File path" });
                ui.label(match &dialog.action {
                    FileDialogAction::ImportApplicationLayout(name) | FileDialogAction::ExportApplicationLayout(name) => name.as_str(),
                    FileDialogAction::ImportEntlayout | FileDialogAction::ExportEntlayout => if russian { "Раскладка (.entlayout)" } else { "Layout (.entlayout)" },
                    FileDialogAction::ImportEntsettings | FileDialogAction::ExportEntsettings => if russian { "Настройки (.entsettings)" } else { "Settings (.entsettings)" },
                    FileDialogAction::ExportLayoutImage => if russian { "Изображение раскладки" } else { "Layout image" },
                    FileDialogAction::StartupImage => if russian { "Изображение запуска" } else { "Startup image" },
                    FileDialogAction::StandbyBackground => if russian { "Фон ожидания" } else { "Standby background" },
                    FileDialogAction::Pictogram => if russian { "Пиктограмма" } else { "Pictogram" },
                    FileDialogAction::ImportPictograms | FileDialogAction::ExportPictograms => if russian { "Пиктограммы" } else { "Pictograms" },
                });
                ui.label(if russian { "Путь к файлу (можно ввести вручную):" } else { "File path (or enter it manually):" });
                if ui.text_edit_singleline(&mut dialog.path).changed() {
                    dialog.confirmed = false;
                }
                if dialog.browser.is_none() && ui.button(if russian { "Обзор" } else { "Browse" }).clicked() {
                    let mode = if dialog.save { super::file_browser::BrowserMode::FileAndDirectory } else { super::file_browser::BrowserMode::File };
                    dialog.browser = Some(super::file_browser::FileBrowser::new(std::path::PathBuf::from("."), mode, ui.ctx()));
                    dialog.browser_error = false;
                }
                if let Some(browser) = &mut dialog.browser {
                    if let Some(selection) = browser.show(ui, russian) {
                        dialog.finish_browsing(selection);
                    }
                }
                if dialog.browser_error {
                    ui.label(if russian { "Этот путь нельзя представить как текст; выберите другой файл." } else { "This path cannot be represented as text; select another file." });
                }
                let destination = destination(&dialog.path, dialog.extension);
                if dialog.save {
                    ui.label(format!("{}: {}", if russian { "Путь сохранения" } else { "Save destination" }, destination.display()));
                    ui.checkbox(&mut dialog.confirmed, if russian {
                        "Подтверждаю перезапись файла, если он существует"
                    } else {
                        "I confirm overwriting the file if it exists"
                    });
                }
                ui.horizontal(|ui| {
                    if ui.button(if russian { "Отмена" } else { "Cancel" }).clicked() {
                        result = Some(None);
                    }
                    if ui.add_enabled(path_can_submit(&dialog.path, dialog.save, dialog.confirmed),
                        egui::Button::new(if russian { "Подтвердить" } else { "Confirm" })).clicked() {
                        result = Some(Some(destination));
                    }
                });
            });
            if let Some(result) = result {
                let _ = dialog.sender.send(result);
                *slot = None;
            }
        });
    }

    /// Restore input state after a dialog closes. The dialog steals
    /// window focus, and on return egui can be left holding a pointer that never
    /// got its release, which wedges subsequent clicks — dead menus. Drop that
    /// state and close the hover dropdown that launched the dialog.
    #[cfg(not(target_arch = "wasm32"))]
    fn recover_input_after_dialog(&mut self, ctx: &egui::Context) {
        INLINE_FILE_DIALOG.with(|slot| *slot.borrow_mut() = None);
        self.close_hover_top_dropdowns(ctx);
        ctx.input_mut(|i| i.pointer = egui::PointerState::default());
        ctx.memory_mut(|m| m.stop_text_input());
        ctx.request_repaint();
    }

    /// Poll the in-flight file dialog, if any, and dispatch its result.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn poll_file_dialog(&mut self, ctx: &egui::Context) {
        let Some((action, generation, rx)) = &self.pending_file_dialog else {
            return;
        };
        let action = action.clone();
        let generation = *generation;
        let recv = rx.try_recv();
        match classify_file_dialog_poll(
            recv,
            action.clone(),
            generation,
            self.connection_generation,
        ) {
            FileDialogPoll::Pending => {
                ctx.request_repaint_after(std::time::Duration::from_millis(50));
            }
            FileDialogPoll::Cancelled => {
                self.pending_file_dialog = None;
                self.recover_input_after_dialog(ctx);
            }
            FileDialogPoll::StaleDevice => {
                // The active device changed while the picker was open — otherwise
                // we could save device B's layout under device A's filename, or
                // program B with A's data. Discard the result.
                self.pending_file_dialog = None;
                self.recover_input_after_dialog(ctx);
                self.status_msg = crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "status_messages.file_dialog_device_changed",
                )
                .into();
            }
            FileDialogPoll::Dispatch(path) => {
                self.pending_file_dialog = None;
                self.recover_input_after_dialog(ctx);
                self.handle_file_dialog_result(ctx, action, path);
            }
            FileDialogPoll::WorkerLost => {
                // Worker vanished (e.g. the dialog thread panicked) without ever
                // sending a result. Surface it instead of swallowing it, then run
                // the same input recovery so a lost dialog can't wedge the UI.
                log::error!("file dialog worker disconnected before returning a result");
                self.pending_file_dialog = None;
                self.status_msg = crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "status_messages.file_dialog_worker_lost",
                )
                .into();
                self.recover_input_after_dialog(ctx);
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn handle_file_dialog_result(
        &mut self,
        ctx: &egui::Context,
        action: FileDialogAction,
        path: std::path::PathBuf,
    ) {
        match action {
            FileDialogAction::ImportApplicationLayout(id) => {
                self.status_msg = match self.import_application_layout_from_path(&path, &id) {
                    Ok(()) => format!("Imported application layout into {id}. Other profiles, app matching and shared resources were preserved."),
                    Err(error) => format!("Application import failed: {error:#}"),
                };
            }
            FileDialogAction::ExportApplicationLayout(id) => {
                self.status_msg = match self.write_application_layout_export(&path, &id) {
                    Ok(()) => format!("Exported application layout: {} (keys, layer names and encoder stacks only; shared resources are not included)", path.display()),
                    Err(error) => format!("Application export failed: {error:#}"),
                };
            }
            FileDialogAction::ImportEntlayout => self.begin_entlayout_import(path),
            FileDialogAction::ExportEntlayout => self.write_entlayout_export(&path),
            FileDialogAction::ImportEntsettings => self.begin_entsettings_import(path),
            FileDialogAction::ExportEntsettings => self.write_entsettings_export(&path),
            FileDialogAction::ExportLayoutImage => self.write_layout_image_export(path),
            FileDialogAction::ExportPictograms => {
                self.status_msg =
                    match write_pictogram_file(&path, &self.app_settings.saved_pictograms) {
                        Ok(()) => format!(
                            "{}: {}",
                            crate::i18n::tr_catalog(
                                self.app_settings.language,
                                "display_settings.export_icons"
                            ),
                            path.display()
                        ),
                        Err(e) => format!("{e:#}"),
                    };
            }
            FileDialogAction::ImportPictograms => match read_pictogram_file(&path) {
                Ok(icons) => {
                    for mut icon in icons {
                        if let Some(existing) =
                            self.app_settings.saved_pictograms.iter_mut().find(|p| {
                                p.name == icon.name
                                    && p.bitmap == icon.bitmap
                                    && p.color == icon.color
                            })
                        {
                            // Explicit authorship survives importing an exact
                            // duplicate of a previously hidden stock snapshot.
                            // Importing stock must never downgrade a user record.
                            if icon.origin == PictogramOrigin::User {
                                existing.origin = PictogramOrigin::User;
                            }
                            continue;
                        }
                        let base = icon.name.clone();
                        let mut n = 2;
                        while self
                            .app_settings
                            .saved_pictograms
                            .iter()
                            .any(|p| p.name.to_lowercase() == icon.name.to_lowercase())
                        {
                            icon.name = format!("{base} {n}");
                            n += 1;
                        }
                        self.app_settings.saved_pictograms.push(icon);
                    }
                    save_app_settings(&self.app_settings);
                    self.status_msg = crate::i18n::tr_catalog(
                        self.app_settings.language,
                        "display_settings.icons_imported",
                    )
                    .to_owned();
                }
                Err(e) => self.status_msg = format!("{e:#}"),
            },
            FileDialogAction::Pictogram => match crate::app::prepare_pictogram(&path) {
                Ok(prepared) => {
                    let pictograms = &mut self.display_settings.pictograms;
                    pictograms.source_levels = prepared.levels;
                    pictograms.source_file_name = prepared.file_name;
                    pictograms.editor_name = path
                        .file_stem()
                        .and_then(|name| name.to_str())
                        .unwrap_or("")
                        .to_owned();
                    pictograms.threshold = prepared.threshold;
                    pictograms.inverted = prepared.inverted;
                    pictograms.selected_builtin = None;
                    pictograms.selected_saved_pictogram = None;
                    pictograms.undo.clear();
                }
                Err(error) => {
                    self.status_msg = crate::i18n::tr_catalog(
                        self.app_settings.language,
                        "display_settings.pictogram_decode_error",
                    )
                    .replace("{error}", &error.to_string());
                }
            },
            FileDialogAction::StartupImage => {
                let fallback = self.display_settings.background_color;
                match self.start_vial_hid_operation(
                    ctx,
                    super::vial_hid_task::VialHidOperation::StartupImageUpload { path, fallback },
                ) {
                    super::vial_hid_task::VialHidTaskStart::Started => {
                        self.status_msg = crate::i18n::tr_catalog(
                            self.app_settings.language,
                            "display_settings.startup_image_uploading",
                        )
                        .into();
                    }
                    super::vial_hid_task::VialHidTaskStart::Busy => {
                        self.status_msg = crate::i18n::tr_catalog(
                            self.app_settings.language,
                            "display_settings.background_busy",
                        )
                        .into();
                    }
                    super::vial_hid_task::VialHidTaskStart::NoDevice => {
                        self.status_msg = crate::i18n::tr_catalog(
                            self.app_settings.language,
                            "display_settings.background_no_device",
                        )
                        .into();
                    }
                }
            }
            FileDialogAction::StandbyBackground => {
                let fallback = self.display_settings.clock_background_color;
                // Standby images always use an aspect-preserving cover crop.
                let scale = StandbyBackgroundScale::Fill;
                match self.start_vial_hid_operation(
                    ctx,
                    super::vial_hid_task::VialHidOperation::BackgroundUpload {
                        path,
                        fallback,
                        scale,
                    },
                ) {
                    super::vial_hid_task::VialHidTaskStart::Started => {
                        self.status_msg = crate::i18n::tr_catalog(
                            self.app_settings.language,
                            "display_settings.background_uploading",
                        )
                        .into();
                    }
                    super::vial_hid_task::VialHidTaskStart::Busy => {
                        self.status_msg = crate::i18n::tr_catalog(
                            self.app_settings.language,
                            "display_settings.background_busy",
                        )
                        .into();
                    }
                    super::vial_hid_task::VialHidTaskStart::NoDevice => {
                        self.status_msg = crate::i18n::tr_catalog(
                            self.app_settings.language,
                            "display_settings.background_no_device",
                        )
                        .into();
                    }
                }
            }
        }
    }
}
