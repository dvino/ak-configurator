use super::ak820_beta1_profiles as catalog;
use crate::i18n::Language;
use egui::RichText;

const OPTIONAL_MODULE_LABELS: &[(&str, &str, &str)] = &[
    ("auto-shift", "Auto Shift", "Auto Shift"),
    ("ctrl-gui-swap", "Ctrl/GUI swap", "Обмен Ctrl/GUI"),
    ("dynamic-macros", "Dynamic Macros", "Динамические макросы"),
    ("grave-escape", "Grave Escape", "Grave Escape"),
    ("gui-lock", "GUI lock", "Блокировка GUI"),
    ("key-overrides", "Key Overrides", "Замены клавиш"),
    ("leader", "Leader", "Leader"),
    ("mac-pc", "Mac/PC mode", "Режим Mac/PC"),
    ("mouse-keys", "Mouse Keys", "Управление мышью"),
    ("nkro", "NKRO", "NKRO"),
    ("nkro-control", "NKRO control", "Управление NKRO"),
    ("nkro-reports", "NKRO reports", "Отчёты NKRO"),
    ("one-shot", "One Shot", "Одноразовые модификаторы"),
    ("os-indicators", "OS indicators", "Индикаторы ОС"),
    ("qmk-settings", "QMK Settings", "Настройки QMK"),
    ("space-cadet", "Space Cadet", "Space Cadet"),
    ("tap-dance", "Tap Dance", "Tap Dance"),
    ("vialrgb-direct", "Per-key RGB", "RGB для каждой клавиши"),
    (
        "vialrgb-direct-ram",
        "Volatile RGB palette",
        "Палитра RGB в ОЗУ",
    ),
];

#[derive(Clone)]
struct SelectorState {
    open: bool,
    selected: catalog::ProfileId,
    applied_to_offline_project: catalog::ProfileId,
}

impl Default for SelectorState {
    fn default() -> Self {
        Self {
            open: false,
            selected: catalog::ProfileId::Main,
            applied_to_offline_project: catalog::ProfileId::Main,
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

fn select_preview_profile(model: &mut SelectorState, profile: catalog::ProfileId) {
    model.selected = profile;
}

fn apply_selected_profile(
    model: &mut SelectorState,
    offline_demo: bool,
) -> Option<catalog::ProfileId> {
    if !offline_demo || model.applied_to_offline_project == model.selected {
        return None;
    }
    model.applied_to_offline_project = model.selected;
    Some(model.selected)
}

/// Keep the current project label in sync after a file import.
pub(crate) fn sync_offline_project(ctx: &egui::Context, profile: catalog::ProfileId) {
    let mut model = state(ctx);
    model.applied_to_offline_project = profile;
    store(ctx, model);
}

/// Open the catalog window from profile navigation.
pub(crate) fn open(ctx: &egui::Context) {
    let mut current = state(ctx);
    current.open = true;
    store(ctx, current);
}

pub(crate) fn close(ctx: &egui::Context) {
    let mut current = state(ctx);
    current.open = false;
    store(ctx, current);
}

pub(crate) fn is_open(ctx: &egui::Context) -> bool {
    state(ctx).open
}

pub(super) fn button(ui: &mut egui::Ui, ctx: &egui::Context, language: Language) -> egui::Response {
    let label = if language == Language::Russian {
        "Профили прошивки beta.2"
    } else {
        "beta.2 firmware profiles"
    };
    let response = crate::ui_style::modern_button(ui, label, egui::vec2(228.0, 34.0), true);
    if response.clicked() {
        open(ctx);
    }
    response
}

/// Draw the beta.2 catalog and return a profile ID when the local selection changes.
pub(crate) fn show(
    ctx: &egui::Context,
    language: Language,
    offline_demo: bool,
) -> Option<catalog::ProfileId> {
    let mut model = state(ctx);
    if !model.open {
        return None;
    }

    let russian = language == Language::Russian;
    let mut apply_profile = None;
    let mut keep_open = model.open;
    let mut close_clicked = false;
    let preferred_size =
        (ctx.content_rect().size() - egui::vec2(64.0, 100.0)).min(egui::vec2(960.0, 600.0));
    crate::ui_style::utility_window(ctx, if russian {
        "Профили AK820 · beta.2"
    } else {
        "AK820 profiles · beta.2"
    }, egui::Id::new("ak820_beta1_profile_window"), &mut keep_open, preferred_size)
    .fixed_size(preferred_size)
    .resizable(false)
    .show(ctx, |ui| {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
        ui.label(if russian {
            "Выберите профиль, чтобы просмотреть его возможности и точную идентичность образа."
        } else {
            "Choose a profile to preview its capabilities and exact image identity."
        });
        ui.add_space(8.0);

        ui.separator();
        let available = ui.available_size();
        let footer_reserve = 52.0;
        let columns_height = (available.y - footer_reserve).max(100.0);
        let list_width = (available.x * 0.32).clamp(132.0, 230.0);
        let detail_width = (available.x - list_width - 16.0 - 2.0 * ui.spacing().item_spacing.x).max(100.0);
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(list_width, columns_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
            egui::ScrollArea::vertical().id_salt("ak820_beta1_profile_list").max_height(columns_height).show(ui, |ui| {
                ui.set_width((list_width - ui.spacing().scroll.bar_width - 4.0).max(100.0));
                let mut profiles: Vec<_> = catalog::profiles().iter().collect();
                profiles.sort_by_key(|image| image.profile != catalog::ProfileId::Main);
                for image in profiles {
                            let id = image.profile;
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
                            let response = ui.add_sized([ui.available_width(), 42.0], egui::Button::selectable(is_selected, egui::RichText::new(title).size(12.5)).wrap());
                            if response.clicked() && model.selected != id {
                                select_preview_profile(&mut model, id);
                            }
                }
            });
                },
            );
            ui.add_space(16.0);
            ui.allocate_ui_with_layout(
                egui::vec2(detail_width, columns_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
            egui::ScrollArea::vertical().id_salt("ak820_beta1_profile_details").auto_shrink([false, false]).max_height(columns_height).show(ui, |ui| {
                ui.set_width((detail_width - ui.spacing().scroll.bar_width - 4.0).max(100.0));
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
                if profile_description.purpose.for_language(russian) != profile_description.summary.for_language(russian) {
                    ui.label(profile_description.purpose.for_language(russian));
                }
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
                ui.collapsing(if russian { "Технические сведения об образе" } else { "Image technical details" }, |ui| {
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
                    if matrix.release_approved { "Релиз beta.2: одобрен" } else { "Релиз beta.2: не одобрен; профиль — кандидат каталога" }
                } else if matrix.release_approved {
                    "beta.2 release: approved"
                } else {
                    "beta.2 release: not approved; catalog candidate"
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
                });
                ui.heading(if russian {
                    "Возможности и ограничения"
                } else {
                    "Features and limits"
                });
                for feature_id in &image.features {
                    if let Some(description) = catalog::feature_description(feature_id) {
                        ui.label(RichText::new(description.title.for_language(russian)).strong());
                        ui.label(description.summary.for_language(russian));
                        if ui.link(if russian { "Подробнее" } else { "Learn more" }).clicked() {
                            super::ak820_help_ui::open_section(ctx, &format!("feature-{feature_id}"));
                        }
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
                    "Для установки выберите Меню → Установить прошивку. Просмотр этого каталога не изменяет клавиатуру."
                } else {
                    "Installation is a separate action in Menu → Install firmware. Browsing this catalog never changes the keyboard."
                }).weak());
                ui.label(RichText::new(if russian {
                    "Выбор в списке показывает возможности. В деморежиме кнопка ниже открывает офлайн-проект; клавиатура и её прошивка не изменяются."
                } else {
                    "The list previews capabilities. In demo mode, the button below opens the selected offline project; it does not change the keyboard or flash firmware."
                }).weak());
                if offline_demo {
                    ui.add_space(8.0);
                    ui.separator();
                    ui.label(if russian {
                        format!("Офлайн-проект: {}", model.applied_to_offline_project.as_str())
                    } else {
                        format!("Offline project: {}", model.applied_to_offline_project.as_str())
                    });
                    ui.label(if russian {
                        "Офлайн доступны назначения клавиш и энкодеров, макросы, Combo, RGB и модули schema03 по выбранному профилю. Правки хранятся в памяти до закрытия приложения. Устройство не подключается."
                    } else {
                        "Offline controls include profile-supported key and encoder assignments, macros, Combo, RGB and schema03 modules. Edits stay in memory until the application closes. No device is connected."
                    });

                }
            });
                },
            );
        });
        ui.separator();
        ui.horizontal(|ui| {
            if offline_demo && crate::ui_style::action_button(ui,
                if russian { "Использовать в проекте" } else { "Use in project" },
                model.selected != model.applied_to_offline_project).clicked() {
                apply_profile = apply_selected_profile(&mut model, offline_demo);
                close_clicked = true;
            }

            if crate::ui_style::modern_button(ui, if russian { "Закрыть" } else { "Close" }, egui::vec2(100.0, 32.0), true).clicked() { close_clicked = true; }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if crate::ui_style::modern_button(ui, if russian { "Справка" } else { "Help" }, egui::vec2(100.0, 32.0), true).clicked() { super::ak820_help_ui::open_section(ctx, "supported-firmware"); }
            });
        });
    });
    if close_clicked {
        keep_open = false;
    }
    model.open = keep_open;
    store(ctx, model);
    apply_profile
}

fn hash_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).strong());
        ui.add(egui::Label::new(RichText::new(value).monospace()).wrap());
    });
}

fn profile_modules(profile: &catalog::MatrixProfile, russian: bool) -> String {
    OPTIONAL_MODULE_LABELS
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn optional_module_labels_cover_every_catalog_module_once() {
        let labels: Vec<_> = OPTIONAL_MODULE_LABELS
            .iter()
            .map(|(id, _, _)| *id)
            .collect();
        let unique: BTreeSet<_> = labels.iter().copied().collect();
        let catalog: BTreeSet<_> = catalog::module_descriptions()
            .features
            .iter()
            .filter(|feature| feature.kind == "optional")
            .map(|feature| feature.id.as_str())
            .collect();

        assert_eq!(labels.len(), unique.len(), "module labels must be unique");
        assert_eq!(unique, catalog, "every optional module needs one UI label");
    }

    #[test]
    fn every_profile_summary_lists_exactly_its_enabled_modules_in_both_languages() {
        let profiles = &catalog::firmware_matrix().profiles;
        assert_eq!(profiles.len(), 10);
        for profile in profiles {
            let enabled = OPTIONAL_MODULE_LABELS
                .iter()
                .filter(|(id, _, _)| profile.capabilities.optional_modules.get(*id) == Some(&true))
                .collect::<Vec<_>>();
            let expected_en = enabled
                .iter()
                .map(|(_, en, _)| *en)
                .collect::<Vec<_>>()
                .join(" · ");
            let expected_ru = enabled
                .iter()
                .map(|(_, _, ru)| *ru)
                .collect::<Vec<_>>()
                .join(" · ");
            assert_eq!(
                profile_modules(profile, false),
                expected_en,
                "{} EN",
                profile.profile.as_str()
            );
            assert_eq!(
                profile_modules(profile, true),
                expected_ru,
                "{} RU",
                profile.profile.as_str()
            );
        }

        let no_modules = catalog::MatrixProfile {
            identity: catalog::BuildIdentity {
                nvm_schema: 0,
                compiler_sha256: String::new(),
                profile: "main".into(),
                configuration_sha256: String::new(),
                source_manifest_sha256: String::new(),
                ihx_sha256: String::new(),
                map_sha256: String::new(),
                mem_sha256: String::new(),
                build_report_sha256: String::new(),
            },
            configuration: serde_json::Value::Null,
            profile: catalog::ProfileId::Main,
            installable: false,
            release_approved: false,
            optional_features: Vec::new(),
            capabilities: catalog::MatrixCapabilities {
                dynamic_entries: catalog::DynamicEntries {
                    combos: 0,
                    tap_dance: 0,
                    key_overrides: 0,
                },
                runtime_qmk_settings_ids: Vec::new(),
                optional_modules: Default::default(),
            },
            measurement_status: String::new(),
            offline_tests_status: String::new(),
            runtime_status: String::new(),
            settings_status: None,
            settings_identity: None,
        };
        assert!(profile_modules(&no_modules, false).is_empty());
        assert!(profile_modules(&no_modules, true).is_empty());
    }

    #[test]
    fn opening_and_rendering_catalog_only_changes_its_preview_window_state() {
        let ctx = egui::Context::default();
        assert!(!state(&ctx).open);

        open(&ctx);
        let opened = state(&ctx);
        assert!(opened.open);
        assert_eq!(opened.selected.as_str(), "main");

        for language in [Language::English, Language::Russian] {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 1000.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    let _ = show(ctx, language, false);
                },
            );
            let rendered = state(&ctx);
            assert!(rendered.open);
            assert_eq!(rendered.selected.as_str(), "main");
        }
    }

    #[test]
    fn localized_navigation_buttons_open_the_catalog_window() {
        #[derive(Default)]
        struct NavigationState {
            open: bool,
            button_rect: Option<egui::Rect>,
        }

        for (language, label) in [
            (Language::English, "beta.2 firmware profiles"),
            (Language::Russian, "Профили прошивки beta.2"),
        ] {
            let mut harness = egui_kittest::Harness::new_ui_state(
                move |ctx, nav_state: &mut NavigationState| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        nav_state.button_rect = Some(button(ui, ctx, language).rect);
                    });
                    nav_state.open = state(ctx).open;
                },
                NavigationState::default(),
            );
            harness.run();
            assert!(!harness.state().open);

            let button_center = harness.state().button_rect.unwrap().center();
            harness.hover_at(button_center);
            harness.run();
            harness.drag_at(button_center);
            harness.run();
            harness.drop_at(button_center);
            harness.run();
            assert!(
                harness.state().open,
                "{label} should open the profile catalog"
            );
        }
    }

    #[test]
    fn selecting_any_catalog_profile_only_updates_the_local_preview_selection() {
        let ctx = egui::Context::default();
        open(&ctx);

        for image in catalog::profiles() {
            let mut preview = state(&ctx);
            select_preview_profile(&mut preview, image.profile);
            store(&ctx, preview.clone());

            let selected = state(&ctx);
            assert!(selected.open);
            assert_eq!(selected.selected, image.profile);
            assert_eq!(selected.selected.as_str(), image.profile.as_str());
            assert_eq!(
                selected.applied_to_offline_project,
                catalog::ProfileId::Main
            );
        }
    }

    #[test]
    fn selected_profile_applies_only_to_the_explicit_offline_demo_project() {
        let mut model = SelectorState::default();
        select_preview_profile(&mut model, catalog::ProfileId::Pointer);

        assert_eq!(apply_selected_profile(&mut model, false), None);
        assert_eq!(model.applied_to_offline_project, catalog::ProfileId::Main);
        assert_eq!(
            apply_selected_profile(&mut model, true),
            Some(catalog::ProfileId::Pointer)
        );
        assert_eq!(
            model.applied_to_offline_project,
            catalog::ProfileId::Pointer
        );
        assert_eq!(apply_selected_profile(&mut model, true), None);
    }
}

#[cfg(test)]
mod catalog_geometry_tests {
    use super::*;
    #[test]
    fn catalog_cards_wrap_and_window_stays_inside_desktop_viewport() {
        for viewport in [egui::vec2(1200.0, 700.0), egui::vec2(800.0, 500.0)] {
            for language in [Language::English, Language::Russian] {
                let ctx = egui::Context::default();
                open(&ctx);
                for _ in 0..4 {
                    let _ = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                viewport,
                            )),
                            ..Default::default()
                        },
                        |ctx| {
                            let _ = show(ctx, language, true);
                        },
                    );
                }
                let rect = ctx
                    .memory(|memory| memory.area_rect(egui::Id::new("ak820_beta1_profile_window")))
                    .unwrap();
                assert!(
                    rect.width() <= viewport.x,
                    "catalog overflows horizontally: {rect:?}"
                );
                assert!(
                    rect.height() <= viewport.y,
                    "catalog overflows vertically: {rect:?}"
                );
            }
        }
    }
}
