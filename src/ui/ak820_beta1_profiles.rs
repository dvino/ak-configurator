use super::ak820_beta1_profiles as catalog;
use crate::i18n::Language;
use egui::{Color32, RichText};

#[derive(Clone)]
struct SelectorState {
    open: bool,
    selected: catalog::ProfileId,
}

impl Default for SelectorState {
    fn default() -> Self {
        Self {
            open: false,
            selected: catalog::ProfileId::Work,
        }
    }
}

fn state(ctx: &egui::Context) -> SelectorState {
    ctx.data(|data| {
        data.get_temp::<SelectorState>(egui::Id::new("ak820_beta1_profile_selector"))
            .unwrap_or_default()
    })
}

fn store(ctx: &egui::Context, state: SelectorState) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::new("ak820_beta1_profile_selector"), state));
}

/// Open the catalog window. Call this from the app's profile navigation action.
pub(crate) fn open(ctx: &egui::Context) {
    let mut current = state(ctx);
    current.open = true;
    store(ctx, current);
}

pub(super) fn button(ui: &mut egui::Ui, ctx: &egui::Context, language: Language) {
    let label = if language == Language::Russian {
        "Профили прошивки beta.1"
    } else {
        "beta.1 firmware profiles"
    };
    if crate::ui_style::modern_button(ui, label, egui::vec2(228.0, 34.0), true).clicked() {
        open(ctx);
    }
}

/// Draw the beta.1 catalog and return a profile ID when the local selection changes.
pub(crate) fn show(ctx: &egui::Context, language: Language) {
    let mut model = state(ctx);
    if !model.open {
        return;
    }

    let russian = language == Language::Russian;
    let mut keep_open = model.open;
    egui::Window::new(if russian {
        "Профили AK820 · beta.1"
    } else {
        "AK820 profiles · beta.1"
    })
    .id(egui::Id::new("ak820_beta1_profile_window"))
    .open(&mut keep_open)
    .default_size(egui::vec2(820.0, 660.0))
    .min_size(egui::vec2(540.0, 420.0))
    .show(ctx, |ui| {
        ui.label(if russian {
            "Выберите профиль, чтобы просмотреть его возможности и точную идентичность образа."
        } else {
            "Choose a profile to preview its capabilities and exact image identity."
        });
        ui.add_space(8.0);

        egui::ScrollArea::vertical()
            .id_salt("ak820_beta1_profile_list")
            .show(ui, |ui| {
                egui::Grid::new("ak820_beta1_profile_cards")
                    .num_columns(2)
                    .spacing(egui::vec2(10.0, 8.0))
                    .show(ui, |ui| {
                        for (index, image) in catalog::profiles().iter().enumerate() {
                            let id = image.profile;
                            let matrix = catalog::matrix_profile(id);
                            let profile_description = catalog::profile_description(id);
                            let is_selected = model.selected == id;
                            let title = format!(
                                "{}{}",
                                profile_description.title.for_language(russian),
                                if profile_description.default {
                                    if russian { "  ·  По умолчанию" } else { "  ·  Default" }
                                } else {
                                    ""
                                }
                            );
                            let card = egui::Frame::group(ui.style())
                                .fill(if is_selected {
                                    ui.visuals().selection.bg_fill
                                } else {
                                    ui.visuals().faint_bg_color
                                })
                                .show(ui, |ui| {
                                    ui.set_min_width(330.0);
                                    let response = ui.selectable_label(is_selected, title);
                                    ui.label(RichText::new(id.as_str()).monospace().weak());
                                    ui.label(profile_description.summary.for_language(russian));
                                    let extra_modules = profile_modules(matrix, russian);
                                    ui.label(if extra_modules.is_empty() {
                                        if russian { "Базовые функции каталога" } else { "Catalog base features" }
                                    } else {
                                        extra_modules.as_str()
                                    });
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label(RichText::new(image.status.as_str()).weak());
                                        ui.label(RichText::new(if image.hardware_validated {
                                            if russian { "Устройство проверено" } else { "Hardware checked" }
                                        } else if russian {
                                            "Устройство не проверено"
                                        } else {
                                            "Hardware untested"
                                        }).color(if image.hardware_validated {
                                            Color32::LIGHT_GREEN
                                        } else {
                                            Color32::YELLOW
                                        }));
                                        ui.label(RichText::new(if matrix.release_approved {
                                            if russian { "Релиз beta.1 одобрен" } else { "beta.1 release approved" }
                                        } else if russian {
                                            "Кандидат каталога"
                                        } else {
                                            "Catalog candidate"
                                        }).weak());
                                        if matrix.installable {
                                            ui.label(RichText::new(if russian {
                                                "Входит в отдельный установщик"
                                            } else {
                                                "In standalone installer"
                                            }).strong());
                                        } else {
                                            ui.label(RichText::new(if russian {
                                                "Нет в установщиках beta.1"
                                            } else {
                                                "Not in beta.1 installers"
                                            }).weak());
                                        }
                                    });
                                    response
                                });
                            if card.inner.clicked() && model.selected != id {
                                model.selected = id;
                            }
                            if index % 2 == 1 {
                                ui.end_row();
                            }
                        }
                    });

                ui.add_space(12.0);
                let image = catalog::profile(model.selected);
                let matrix = catalog::matrix_profile(model.selected);
                ui.separator();
                ui.heading(if russian {
                    "Сводка выбранного профиля"
                } else {
                    "Selected profile preview"
                });
                let profile_description = catalog::profile_description(model.selected);
                ui.label(RichText::new(profile_description.summary.for_language(russian)).strong());
                ui.label(profile_description.purpose.for_language(russian));
                ui.label(format!("{} · {}", image.model, image.profile.as_str()));
                ui.label(format!(
                    "{}: {}",
                    if russian {
                        "Модули профиля"
                    } else {
                        "Profile modules"
                    },
                    profile_modules(matrix, russian)
                ));
                let enabled_module_count = matrix
                    .capabilities
                    .optional_modules
                    .values()
                    .filter(|enabled| **enabled)
                    .count();
                ui.label(if russian {
                    format!(
                        "Модулей в матрице: {}; функций профиля: {}",
                        enabled_module_count,
                        matrix.optional_features.len()
                    )
                } else {
                    format!(
                        "Modules in matrix: {}; profile features: {}",
                        enabled_module_count,
                        matrix.optional_features.len()
                    )
                });
                ui.label(if russian {
                    format!("Статус образа: {}", image.status)
                } else {
                    format!("Image status: {}", image.status)
                });
                ui.label(if russian {
                    format!("Статус проверок сборки: {}; тесты: {}; runtime: {}", matrix.measurement_status, matrix.offline_tests_status, matrix.runtime_status)
                } else {
                    format!("Build checks: {}; tests: {}; runtime: {}", matrix.measurement_status, matrix.offline_tests_status, matrix.runtime_status)
                });
                if let Some(settings_status) = matrix.settings_status.as_deref() {
                    ui.label(if russian {
                        format!("Проверка настроек: {settings_status}")
                    } else {
                        format!("Settings identity status: {settings_status}")
                    });
                }
                ui.label(if russian {
                    if matrix.release_approved { "Релиз beta.1: одобрен" } else { "Релиз beta.1: не одобрен; профиль — кандидат каталога" }
                } else if matrix.release_approved {
                    "beta.1 release: approved"
                } else {
                    "beta.1 release: not approved; catalog candidate"
                });
                ui.add_space(4.0);
                ui.heading(if russian {
                    "Идентичность образа"
                } else {
                    "Image identity"
                });
                hash_row(ui, if russian { "IHX SHA-256" } else { "IHX SHA-256" }, &image.build_identity.ihx_sha256);
                hash_row(
                    ui,
                    if russian {
                        "Хеш связанных окон CODE (не полный CODE)"
                    } else {
                        "Linked CODE windows fingerprint (not full CODE)"
                    },
                    &image.code_identity.sha256,
                );
                if let Some(settings_identity) = &matrix.settings_identity {
                    hash_row(
                        ui,
                        if russian { "Упакованный CODE SHA-256" } else { "Packed CODE SHA-256" },
                        &settings_identity.code_sha256,
                    );
                } else {
                    ui.label(if russian {
                        "Точная идентичность упакованного CODE для профиля не опубликована."
                    } else {
                        "Exact packed CODE identity is not published for this profile."
                    });
                }
                hash_row(ui, if russian { "Настройка сборки SHA-256" } else { "Configuration SHA-256" }, &image.build_identity.configuration_sha256);
                hash_row(ui, if russian { "Схема настроек" } else { "Settings schema" }, &format!("{}", image.settings_schema));
                ui.add_space(8.0);
                ui.heading(if russian {
                    "Возможности и ограничения"
                } else {
                    "Features and limits"
                });
                for feature_id in &image.features {
                    if let Some(description) = catalog::feature_description(feature_id) {
                        ui.label(RichText::new(description.title.for_language(russian)).strong());
                        ui.label(description.summary.for_language(russian));
                        ui.label(RichText::new(description.limits.for_language(russian)).weak());
                        ui.add_space(5.0);
                    } else {
                        ui.label(if russian {
                            format!("Описание в каталоге отсутствует: {feature_id}")
                        } else {
                            format!("Catalog description is missing: {feature_id}")
                        });
                    }
                }
                ui.add_space(8.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(format!(
                            "{} · catalog archive SHA-256: {}",
                            catalog::BETA_RELEASE,
                            catalog::SOURCE_ARCHIVE_SHA256
                        ))
                        .weak()
                        .monospace(),
                    )
                    .wrap(),
                );
                ui.label(RichText::new(if russian {
                    "Аппаратная проверка не проводилась ни для одного профиля. Companion пока не устанавливает эти образы. Только профиль work входит в отдельные установщики beta.1; это не разрешает прошивку из companion."
                } else {
                    "No profile has been tested on hardware. The companion cannot install these images yet. Only work is included in the separate beta.1 installers; this does not enable flashing from the companion."
                }).color(Color32::YELLOW));
                ui.label(RichText::new(if russian {
                    "Это только просмотр каталога. Выбор не меняет профиль клавиатуры или локального редактора и не запускает прошивку."
                } else {
                    "This is a catalog preview only. Selection does not change the keyboard or local editor profile and does not start flashing."
                }).weak());
            });
    });
    model.open = keep_open;
    store(ctx, model);
}

fn hash_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).strong());
        ui.add(egui::Label::new(RichText::new(value).monospace()).wrap());
    });
}

fn profile_modules(profile: &catalog::MatrixProfile, russian: bool) -> String {
    const NAMES: &[(&str, &str, &str)] = &[
        ("auto-shift", "Auto Shift", "Auto Shift"),
        ("ctrl-gui-swap", "Ctrl/GUI swap", "Обмен Ctrl/GUI"),
        ("dynamic-macros", "Dynamic Macros", "Динамические макросы"),
        ("gui-lock", "GUI lock", "Блокировка GUI"),
        ("key-overrides", "Key Overrides", "Замены клавиш"),
        ("leader", "Leader", "Leader"),
        ("mac-pc", "Mac/PC mode", "Режим Mac/PC"),
        ("mouse-keys", "Mouse Keys", "Управление мышью"),
        ("nkro-control", "NKRO control", "Управление NKRO"),
        ("one-shot", "One Shot", "Одноразовые модификаторы"),
        ("os-indicators", "OS indicators", "Индикаторы ОС"),
        ("qmk-settings", "QMK Settings", "Настройки QMK"),
        ("space-cadet", "Space Cadet", "Space Cadet"),
        ("tap-dance", "Tap Dance", "Tap Dance"),
    ];
    NAMES
        .iter()
        .filter(|(key, _, _)| {
            profile
                .capabilities
                .optional_modules
                .get(*key)
                .copied()
                .unwrap_or(false)
        })
        .map(|(_, en, ru)| if russian { *ru } else { *en })
        .collect::<Vec<_>>()
        .join(" · ")
}
