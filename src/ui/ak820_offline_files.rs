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
        self.ak820_offline_import_undo = Some(current.clone());
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
        let Some(previous) = self.ak820_offline_import_undo.take() else {
            return false;
        };
        if let Ok(current) = self.snapshot_current_ak820_project() {
            store_project(&mut self.ak820_demo_projects, current);
        }
        let profile = previous.profile;
        store_project(&mut self.ak820_demo_projects, previous);
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
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > MAX_PROJECT_BYTES {
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
#[derive(Clone)]
struct PendingDialog {
    save: bool,
    exported: Option<Vec<u8>>,
    result: std::sync::Arc<std::sync::Mutex<std::sync::mpsc::Receiver<Option<std::path::PathBuf>>>>,
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
            let result = pending.result.lock().unwrap().try_recv();
            if !matches!(result, Err(std::sync::mpsc::TryRecvError::Empty)) {
                ctx.data_mut(|d| d.remove::<PendingDialog>(pending_id));
                let chosen = matches!(&result, Ok(Some(_)));
                let outcome = match result {
                    Ok(Some(path)) if pending.save => {
                        write_project(&path, pending.exported.as_ref().unwrap())
                    }
                    Ok(Some(path)) => {
                        read_project(&path).and_then(|raw| app.import_ak820_project_bytes(&raw))
                    }
                    _ => Ok(()),
                };
                if let Err(error) = outcome {
                    // This local-only message never enters status/report/log APIs.
                    ctx.data_mut(|d| d.insert_temp(message_id, error));
                } else if chosen {
                    ctx.data_mut(|d| {
                        d.insert_temp(
                            message_id,
                            tr("Local project ready", "Локальный проект готов").to_owned(),
                        )
                    });
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
                        save,
                        exported,
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
                let _ = tx.send(path);
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
        app.rename_ak820_offline_layer(0, "Imported");
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
