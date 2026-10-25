//! Read-only local inspection of complete firmware settings backup directories.
//! This is distinct from the editable `.ak820project` demo format.

use crate::ak820_settings_backup::ValidatedArchive;
use crate::app::ak820_beta1_profiles::{self as beta, ProfileId};
use crate::app::EntropyApp;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};

pub(crate) const OPEN_ID: &str = "ak820_backup_inspection_open";
const STATE_ID: &str = "ak820_backup_inspection_state";

// Dependency diagnostics never become presentation text: retain neither paths
// nor untranslated implementation details in the displayed error.
fn display_error(ru: bool, _diagnostic: &str) -> &'static str {
    if !cfg!(target_os = "linux") {
        if ru {
            "Работа с каталогами копий в macOS/Windows пока недоступна: приватный доступ к файлам не проверен."
        } else {
            "Backup directories are unavailable on macOS/Windows: private file access is not qualified."
        }
    } else if ru {
        "Копия не прошла проверку. Проверьте формат, состав файлов и выбранный профиль."
    } else {
        "Backup validation failed. Check the format, complete file set and selected profile."
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Summary {
    profile: ProfileId,
    format: String,
    source_build_id: String,
    source_ihx_sha256: String,
    code_sha256: String,
    definition_sha256: String,
    schema: u32,
    manifest_sha256: String,
    dimensions: Vec<(String, u64)>,
}

impl Summary {
    fn from_archive(profile: ProfileId, archive: &ValidatedArchive) -> Self {
        let manifest = &archive.manifest;
        let dimensions = manifest
            .get("counts")
            .and_then(serde_json::Value::as_object)
            .map(|counts| {
                counts
                    .iter()
                    .filter_map(|(key, value)| value.as_u64().map(|n| (key.clone(), n)))
                    .collect()
            })
            .unwrap_or_default();
        Self {
            profile,
            format: manifest["format"].as_str().unwrap_or("unknown").to_owned(),
            source_build_id: manifest["source_build_id"]
                .as_u64()
                .map(|v| v.to_string())
                .unwrap_or_else(|| "unknown".into()),
            source_ihx_sha256: archive.source.ihx_sha256.clone(),
            code_sha256: archive.source.code_sha256.clone(),
            definition_sha256: archive.source.definition_sha256.clone(),
            schema: archive.source.schema,
            manifest_sha256: archive.manifest_sha256.clone(),
            dimensions,
        }
    }
}

#[derive(Clone)]
struct ResultMessage {
    request_id: u64,
    profile: ProfileId,
    result: Option<Result<Summary, String>>,
}

#[derive(Clone)]
struct PendingRead {
    request_id: u64,
    profile: ProfileId,
    receiver: std::sync::Arc<std::sync::Mutex<Receiver<ResultMessage>>>,
}

#[derive(Clone, Default)]
struct WindowState {
    request_id: u64,
    profile: Option<ProfileId>,
    pending: Option<PendingRead>,
    summary: Option<Summary>,
    error: Option<String>,
}

impl WindowState {
    fn invalidate(&mut self) {
        self.request_id = self.request_id.wrapping_add(1);
        self.pending = None;
        self.summary = None;
        self.error = None;
    }

    fn accept(&mut self, message: ResultMessage) -> bool {
        if message.request_id != self.request_id || self.profile != Some(message.profile) {
            return false;
        }
        self.pending = None;
        let Some(result) = message.result else {
            self.pending = None;
            return true;
        };
        match result {
            Ok(summary) => {
                self.summary = Some(summary);
                self.error = None;
            }
            Err(error) => {
                self.summary = None;
                self.error = Some(error);
            }
        }
        true
    }
}

/// Render the inspection window. Root should call this from its normal UI frame.
pub(crate) fn show(_app: &mut EntropyApp, ctx: &egui::Context) {
    let open = ctx.data(|d| d.get_temp::<bool>(egui::Id::new(OPEN_ID)).unwrap_or(false));
    if !open {
        return;
    }
    let ru = _app.app_settings.language == crate::i18n::Language::Russian;
    let tr = |en: &'static str, ru_text: &'static str| if ru { ru_text } else { en };

    let mut state = ctx
        .data(|d| d.get_temp::<WindowState>(egui::Id::new(STATE_ID)))
        .unwrap_or_default();

    if let Some(pending) = &state.pending {
        if pending.request_id != state.request_id || state.profile != Some(pending.profile) {
            state.pending = None;
        } else {
            let outcome = pending.receiver.lock().unwrap().try_recv();
            match outcome {
                Ok(message) => {
                    state.accept(message);
                    ctx.request_repaint();
                }
                Err(TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100))
                }
                Err(TryRecvError::Disconnected) => {
                    state.pending = None;
                    state.error = Some(
                        tr(
                            "Backup validation failed",
                            "Проверка резервной копии не пройдена",
                        )
                        .into(),
                    );
                }
            }
        }
    }

    let mut choose_directory = false;
    let mut changed_profile = None;
    let mut close = false;
    let mut window_open = true;
    egui::Window::new(tr("Firmware backup inspection", "Просмотр резервной копии прошивки"))
        .id(egui::Id::new("ak820_backup_inspection_window"))
        .open(&mut window_open)
        .resizable(true)
        .default_width(540.0)
        .max_width((ctx.content_rect().width() - 32.0).max(260.0))
        .max_height((ctx.content_rect().height() - 64.0).max(100.0))
        .vscroll(true)
        .show(ctx, |ui| {
            ui.label(tr(
                "Complete firmware backup · read-only · local directory",
                "Полная резервная копия прошивки · только чтение · локальная папка",
            ));
            ui.label(tr(
                "This is separate from an editable .ak820project.json file. No device operation is performed.",
                "Это отдельный формат от редактируемого файла .ak820project.json. Операции с устройством не выполняются.",
            ));

            let selected = state.profile.unwrap_or(ProfileId::Work);
            egui::ComboBox::from_id_salt("ak820_backup_inspection_profile")
                .selected_text(format!("{}: {}", tr("Profile", "Профиль"), selected.as_str()))
                .show_ui(ui, |ui| {
                    for entry in beta::profiles() {
                        let description = beta::profile_description(entry.profile);
                        if ui.selectable_label(
                            selected == entry.profile,
                            description.title.for_language(ru),
                        ).clicked() {
                            changed_profile = Some(entry.profile);
                        }
                    }
                });

            let identity_available = beta::matrix_profile(selected).settings_identity.is_some();
            if !identity_available {
                ui.colored_label(egui::Color32::YELLOW, tr(
                    "This profile has no qualified source identity; validation is blocked.",
                    "Для этого профиля нет подтверждённой идентичности источника; проверка заблокирована.",
                ));
            }
            if !cfg!(target_os = "linux") {
                ui.label(display_error(ru, ""));
            }
            ui.add_enabled_ui(state.pending.is_none() && identity_available && cfg!(target_os = "linux"), |ui| {
                if ui.button(tr("Choose backup directory…", "Выбрать папку резервной копии…")).clicked() {
                    choose_directory = true;
                }
            });
            if state.pending.is_some() {
                ui.spinner();
                ui.label(tr("Validating local backup…", "Проверка локальной копии…"));
            }
            if let Some(error) = &state.error {
                ui.colored_label(egui::Color32::LIGHT_RED, display_error(ru, error));
            }
            if let Some(summary) = &state.summary {
                ui.separator();
                ui.label(tr("Validated firmware backup", "Проверенная резервная копия прошивки"));
                ui.label(format!("{}: {}", tr("Format", "Формат"), summary.format));
                ui.label(format!("{}: {}", tr("Profile", "Профиль"), summary.profile.as_str()));
                ui.label(format!("{}: {}", tr("Source build", "Сборка источника"), summary.source_build_id));
                ui.label(format!("{}: {}", tr("Manifest SHA-256", "SHA-256 манифеста"), summary.manifest_sha256));
                ui.label(format!("IHX SHA-256: {}", summary.source_ihx_sha256));
                ui.label(format!("CODE SHA-256: {}", summary.code_sha256));
                ui.label(format!("{}: {}", tr("Definition SHA-256", "SHA-256 описания"), summary.definition_sha256));
                ui.label(format!("{}: {}", tr("Settings schema", "Схема настроек"), summary.schema));
                ui.label(tr("Manifest dimensions", "Параметры манифеста"));
                for (name, value) in &summary.dimensions {
                    let label = match name.as_str() {
                        "layers" => tr("Layers", "Слои"),
                        "physical_keys" => tr("Physical keys", "Физические клавиши"),
                        "assignments" => tr("Assignments", "Назначения"),
                        "encoder_assignments" => tr("Encoder assignments", "Назначения энкодера"),
                        "macros" => tr("Macro slots", "Слоты макросов"),
                        "macro_capacity" => tr("Macro capacity (bytes)", "Объём макросов (байты)"),
                        _ => tr("Count", "Количество"),
                    };
                    ui.label(format!("{label}: {value}"));
                }
                ui.label(tr(
                    "ROM contents and read logs are intentionally not shown.",
                    "Содержимое ROM и журнал чтения намеренно не отображаются.",
                ));
            }
            if ui.button(tr("Close", "Закрыть")).clicked() { close = true; }
        });

    if let Some(profile) = changed_profile {
        state.invalidate();
        state.profile = Some(profile);
    }
    if !window_open {
        close = true;
    }
    #[cfg(not(target_arch = "wasm32"))]
    if choose_directory {
        let profile = state.profile.unwrap_or(ProfileId::Work);
        let id = state.request_id.wrapping_add(1);
        state.invalidate();
        state.request_id = id;
        state.profile = Some(profile);
        let (tx, rx) = mpsc::channel();
        state.pending = Some(PendingRead {
            request_id: id,
            profile,
            receiver: std::sync::Arc::new(std::sync::Mutex::new(rx)),
        });
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let picked: Option<PathBuf> = rfd::FileDialog::new().pick_folder();
            if let Some(path) = picked {
                let result = crate::ak820_backup_files::read_validated_archive(&path, profile)
                    .map(|archive| Summary::from_archive(profile, &archive));
                let _ = tx.send(ResultMessage {
                    request_id: id,
                    profile,
                    // read_validated_archive returns path-free, redacted errors.
                    result: Some(result),
                });
            } else {
                // A canceled picker returns a neutral local result.
                let _ = tx.send(ResultMessage {
                    request_id: id,
                    profile,
                    result: None,
                });
            }
            repaint.request_repaint();
        });
    }
    #[cfg(target_arch = "wasm32")]
    let _ = choose_directory;

    if close {
        ctx.data_mut(|d| {
            d.insert_temp(egui::Id::new(OPEN_ID), false);
            state.invalidate();
        });
    }
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(STATE_ID), state));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_summary(profile: ProfileId) -> Summary {
        Summary {
            profile,
            format: "synthetic-test-only".into(),
            source_build_id: "test-only".into(),
            source_ihx_sha256: "0".repeat(64),
            code_sha256: "1".repeat(64),
            definition_sha256: "2".repeat(64),
            schema: 0,
            manifest_sha256: "3".repeat(64),
            dimensions: vec![],
        }
    }

    #[test]
    fn accepts_only_current_request_and_profile() {
        let mut state = WindowState {
            request_id: 5,
            profile: Some(ProfileId::Work),
            ..Default::default()
        };
        let stale = ResultMessage {
            request_id: 4,
            profile: ProfileId::Work,
            result: Some(Ok(fake_summary(ProfileId::Work))),
        };
        assert!(!state.accept(stale));
        assert!(state.summary.is_none());
        let wrong_profile = ResultMessage {
            request_id: 5,
            profile: ProfileId::Settings,
            result: Some(Ok(fake_summary(ProfileId::Settings))),
        };
        assert!(!state.accept(wrong_profile));
        let success = ResultMessage {
            request_id: 5,
            profile: ProfileId::Work,
            result: Some(Ok(fake_summary(ProfileId::Work))),
        };
        assert!(state.accept(success));
        assert_eq!(
            state.summary.as_ref().unwrap().format,
            "synthetic-test-only"
        );
        let failure = ResultMessage {
            request_id: 5,
            profile: ProfileId::Work,
            result: Some(Err("Backup validation failed".into())),
        };
        assert!(state.accept(failure));
        assert_eq!(state.error.as_deref(), Some("Backup validation failed"));
    }

    #[test]
    fn cancellation_close_and_replacement_drop_actual_pending_receivers() {
        let (tx, rx) = mpsc::channel();
        let mut state = WindowState {
            request_id: 1,
            profile: Some(ProfileId::Work),
            pending: Some(PendingRead {
                request_id: 1,
                profile: ProfileId::Work,
                receiver: std::sync::Arc::new(std::sync::Mutex::new(rx)),
            }),
            ..Default::default()
        };
        assert!(state.accept(ResultMessage {
            request_id: 1,
            profile: ProfileId::Work,
            result: None
        }));
        assert!(state.pending.is_none());
        assert!(state.summary.is_none());
        assert!(tx
            .send(ResultMessage {
                request_id: 1,
                profile: ProfileId::Work,
                result: None
            })
            .is_err());
        let (tx, rx) = mpsc::channel();
        state.pending = Some(PendingRead {
            request_id: 1,
            profile: ProfileId::Work,
            receiver: std::sync::Arc::new(std::sync::Mutex::new(rx)),
        });
        state.invalidate(); // same operation used by close and profile replacement
        assert!(tx
            .send(ResultMessage {
                request_id: 1,
                profile: ProfileId::Work,
                result: None
            })
            .is_err());
        assert!(!state.accept(ResultMessage {
            request_id: 1,
            profile: ProfileId::Work,
            result: Some(Ok(fake_summary(ProfileId::Work)))
        }));
        assert!(state.accept(ResultMessage {
            request_id: 2,
            profile: ProfileId::Work,
            result: Some(Ok(fake_summary(ProfileId::Work)))
        }));
    }

    #[test]
    fn displayed_errors_are_localized_and_never_echo_diagnostic_paths() {
        let diagnostic = "/home/private/ROM.bin contains secret";
        for ru in [true, false] {
            let text = display_error(ru, diagnostic);
            assert!(!text.contains("private") || !text.contains("ROM.bin"));
            assert!(!text.contains("/home/") && !text.contains("secret"));
        }
        assert_ne!(
            display_error(true, diagnostic),
            display_error(false, diagnostic)
        );
    }

    #[test]
    fn summary_window_fits_small_viewport_without_hid() {
        for language in [
            crate::i18n::Language::Russian,
            crate::i18n::Language::English,
        ] {
            let ctx = egui::Context::default();
            let cc = eframe::CreationContext::_new_kittest(ctx.clone());
            let mut app = EntropyApp::new_ak820_demo(&cc);
            app.app_settings.language = language;
            let mut summary = fake_summary(ProfileId::Work);
            summary.dimensions = (0..12).map(|i| (format!("synthetic-{i}"), 498)).collect();
            ctx.data_mut(|d| {
                d.insert_temp(egui::Id::new(OPEN_ID), true);
                d.insert_temp(
                    egui::Id::new(STATE_ID),
                    WindowState {
                        profile: Some(ProfileId::Work),
                        summary: Some(summary),
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
            assert!(rect.height() <= 500.0 && rect.width() <= 800.0, "{rect:?}");
            assert!(app.hid_device.is_none());
        }
    }

    #[test]
    fn profile_invalidation_clears_prior_result_and_pending_request() {
        let mut state = WindowState {
            request_id: 8,
            profile: Some(ProfileId::Work),
            summary: Some(fake_summary(ProfileId::Work)),
            ..Default::default()
        };
        state.invalidate();
        state.profile = Some(ProfileId::Mouse);
        assert_eq!(state.request_id, 9);
        assert!(state.summary.is_none());
        assert!(state.pending.is_none());
        assert!(!state.accept(ResultMessage {
            request_id: 8,
            profile: ProfileId::Work,
            result: Some(Ok(fake_summary(ProfileId::Work)))
        }));
    }
}
