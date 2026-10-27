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
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(WINDOW_ID), false));
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
    temporary.persist(path).map_err(|e| e.error.to_string())?;
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
#[derive(Clone)]
struct PendingDialog {
    result:
        std::sync::Arc<std::sync::Mutex<std::sync::mpsc::Receiver<Result<FileOutcome, String>>>>,
}

/// Call once per frame; the controls and all their actions require demo mode.
pub(crate) fn show(app: &mut EntropyApp, ctx: &egui::Context) {
    if !app.ak820_demo {
        return;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let ru = app.app_settings.language == crate::i18n::Language::Russian;
        let tr = |en, ru_text| if ru { ru_text } else { en };
        let pending_id = egui::Id::new("ak820_local_project_dialog");
        let message_id = egui::Id::new("ak820_local_project_message");
        let pending = ctx.data(|d| d.get_temp::<PendingDialog>(pending_id));
        if !is_open(ctx) {
            return;
        }
        if let Some(pending) = &pending {
            let result = receive_file_result(pending.result.lock().unwrap().try_recv());
            if let Some(result) = result {
                ctx.data_mut(|d| d.remove::<PendingDialog>(pending_id));
                match result {
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
                }
                ctx.input_mut(|i| i.pointer = egui::PointerState::default());
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
        let mut action = None;
        let mut profile_to_open = None;
        let mut window_open = true;
        let mut close_clicked = false;
        crate::ui_style::utility_window(
            ctx,
            tr("Offline project", "Офлайн-проект"),
            egui::Id::new("ak820_local_project_window"),
            &mut window_open,
            egui::vec2(500.0, 320.0),
        )
        .show(ctx, |ui| {
            crate::ui_style::modal_intro(
                ui,
                tr("Demo only · local file", "Только демо · локальный файл"),
            );
            ui.add_space(8.0);
            egui::ScrollArea::vertical()
                .id_salt("ak820_local_project_content")
                .max_height((ui.available_height() - 48.0).max(100.0))
                .show(ui, |ui| {
                    if let Some(profile) = app.ak820_demo_profile {
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
                        ui.horizontal(|ui| {
                            if crate::ui_style::modern_button(
                                ui,
                                tr("Export…", "Экспорт…"),
                                egui::vec2(124.0, 34.0),
                                true,
                            )
                            .clicked()
                            {
                                action = Some(true);
                            }
                            if crate::ui_style::modern_button(
                                ui,
                                tr("Import…", "Импорт…"),
                                egui::vec2(124.0, 34.0),
                                true,
                            )
                            .clicked()
                            {
                                action = Some(false);
                            }
                        });
                    });
                    if crate::ui_style::action_button(
                        ui,
                        tr("Undo import", "Отменить импорт"),
                        pending.is_none() && app.ak820_offline_import_undo.is_some(),
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
                    if let Some(message) = ctx.data(|d| d.get_temp::<String>(message_id)) {
                        ui.label(message);
                    }
                });
            ui.separator();
            ui.horizontal(|ui| {
                let close = crate::ui_style::modern_button(
                    ui,
                    tr("Close", "Закрыть"),
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
                        super::ak820_help_ui::open_section(ctx, "offline-projects");
                    }
                });
            });
        });
        if close_clicked {
            window_open = false;
        }
        if !window_open && pending.is_none() {
            close(ctx);
        }
        if let Some(profile) = profile_to_open {
            app.configure_ak820_offline_profile(profile);
        }
        if let Some(save) = action {
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
            let (tx, rx) = std::sync::mpsc::channel();
            ctx.data_mut(|d| {
                d.insert_temp(
                    pending_id,
                    PendingDialog {
                        result: std::sync::Arc::new(std::sync::Mutex::new(rx)),
                    },
                )
            });
            let repaint = ctx.clone();
            std::thread::spawn(move || {
                let dialog = rfd::FileDialog::new()
                    .add_filter("AK820 local project", &["ak820project.json"])
                    .set_file_name("example.ak820project.json");
                let path = if save {
                    dialog.save_file()
                } else {
                    dialog.pick_file()
                };
                let _ = tx.send(run_file_operation(save, path, exported));
                repaint.request_repaint();
            });
        }
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
    fn atomic_file_bytes_and_size_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.ak820project.json");
        write_project(&path, b"first").unwrap();
        let bytes = export(&app());
        write_project(&path, &bytes).unwrap();
        assert_eq!(read_project(&path).unwrap(), bytes);
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
                    result: std::sync::Arc::new(std::sync::Mutex::new(rx)),
                },
            );
        });

        // A pending worker result leaves the dialog operation pending while the
        // production UI frame completes on the GUI thread.
        let _ = ctx.run(egui::RawInput::default(), |ctx| show(&mut target, ctx));
        assert!(ctx.data(|data| data.get_temp::<PendingDialog>(pending_id).is_some()));
        assert_eq!(export(&target), before);

        assert!(tx.send(Ok(FileOutcome::Loaded(loaded))).is_ok());
        let _ = ctx.run(egui::RawInput::default(), |ctx| show(&mut target, ctx));
        assert_eq!(target.layer_names[0], "Loaded");
        assert!(ctx.data(|data| data.get_temp::<PendingDialog>(pending_id).is_none()));

        // A further production frame has no pending result to reapply. If it
        // imported twice, the single-step undo would restore the imported state.
        let _ = ctx.run(egui::RawInput::default(), |ctx| show(&mut target, ctx));
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
                    result: std::sync::Arc::new(std::sync::Mutex::new(rx)),
                },
            );
        });
        let _ = ctx.run(egui::RawInput::default(), |ctx| show(&mut target, ctx));
        let message = ctx.data(|data| {
            data.get_temp::<String>(egui::Id::new("ak820_local_project_message"))
        });
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
        assert!(app.ak820_demo_projects.iter().any(|p| p.profile == ProfileId::Creative));
        assert!(app.undo_ak820_project_import());
        assert!(!app.ak820_demo_projects.iter().any(|p| p.profile == ProfileId::Creative));
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
    fn local_project_profile_selector_opens_typed_example_without_usb() {
        let cc = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = EntropyApp::new_ak820_demo(&cc);
        app.app_settings.language = crate::i18n::Language::Russian;
        let mut harness = Harness::new_state(
            |ctx, app| {
                open(ctx);
                show(app, ctx);
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
