use super::ak820_beta1_profiles as catalog;
use crate::i18n::Language;

fn disclaimer(russian: bool) -> &'static str {
    if russian {
        "Только историческая справка: это не возможности бета.5 или подключённого устройства и не разрешение на установку, запись или подтверждение совместимости настроек."
    } else {
        "Historical reference only: not beta.5 or connected-device capabilities, and not permission to install or write, or confirmation of settings compatibility."
    }
}

fn feature_help(ui: &mut egui::Ui, id: &str, russian: bool) {
    if let Some(feature) = catalog::feature_description(id) {
        ui.strong(feature.title.for_language(russian));
        ui.label(feature.summary.for_language(russian));
        ui.label(feature.limits.for_language(russian));
    } else {
        ui.label(format!(
            "{id}: {}",
            if russian {
                "описание недоступно"
            } else {
                "description unavailable"
            }
        ));
    }
}

/// Profile descriptions are read-only and live in the firmware help article.
pub(crate) fn profile_help(ui: &mut egui::Ui, language: Language) {
    let russian = language == Language::Russian;
    ui.heading(format!(
        "{} — {}",
        if russian {
            "Исторический каталог профилей"
        } else {
            "Historical profile catalog"
        },
        catalog::BETA_RELEASE
    ));
    ui.label(disclaimer(russian));
    for image in catalog::profiles() {
        let description = catalog::profile_description(image.profile);
        let title = description.title.for_language(russian);
        ui.heading(title);
        ui.label(description.summary.for_language(russian));
        if description.purpose.for_language(russian) != description.summary.for_language(russian) {
            ui.label(description.purpose.for_language(russian));
        }
        egui::CollapsingHeader::new(format!("{}: {}", if russian { "Исторические подробности" } else { "Historical details" }, image.profile.as_str()))
            .id_salt(("ak820_profile_help_details", catalog::BETA_RELEASE, image.profile.as_str()))
            .show(ui, |ui| {
                ui.label(disclaimer(russian));
                ui.label(format!("{}: {}", if russian { "Статус каталога (без интерпретации)" } else { "Catalog status (uninterpreted)" }, image.status));
                ui.strong(if russian { "Заявленные возможности исторического каталога (не проверенная доступность)" } else { "Declared historical catalog features (not validated availability)" });
                for id in &image.features {
                    feature_help(ui, id, russian);
                }
                let limits = &image.fixed_limits;
                ui.strong(if russian { "Исторические фиксированные максимумы (не активные количества и не доступность хранилища)" } else { "Historical fixed maxima (not active counts or storage availability)" });
                for (label, value) in [
                    (if russian { "Слои" } else { "Layers" }, u32::from(limits.layers)),
                    (if russian { "Энкодеры" } else { "Encoders" }, u32::from(limits.encoders)),
                    (if russian { "Макросы" } else { "Macros" }, u32::from(limits.macros)),
                    (if russian { "Байты макросов" } else { "Macro bytes" }, u32::from(limits.macro_bytes)),
                    (if russian { "Комбо" } else { "Combos" }, u32::from(limits.combos)),
                    ("Tap dance", u32::from(limits.tap_dance)),
                    (if russian { "Переопределения клавиш" } else { "Key overrides" }, u32::from(limits.key_overrides)),
                    (if russian { "Светодиоды RGB" } else { "RGB LEDs" }, u32::from(limits.rgb_leds)),
                    (if russian { "Максимальная яркость" } else { "Maximum brightness" }, u32::from(limits.rgb_maximum_brightness)),
                ] { ui.label(format!("{label}: {value}")); }
                egui::CollapsingHeader::new(if russian { "Происхождение сборки" } else { "Build provenance" })
                    .id_salt(("ak820_profile_help_build", catalog::BETA_RELEASE, image.profile.as_str()))
                    .show(ui, |ui| {
                        ui.label(if russian { "Хеши идентифицируют входные данные и артефакты сборки каталога, но не подтверждают подключённый образ или упакованный CODE." } else { "Hashes identify catalog build inputs and artifacts, not the connected image or packed CODE verification." });
                        let build = &image.build_identity;
                        for (label, hash) in [
                            (if russian { "SHA256 файла IHX" } else { "IHX file SHA256" }, &build.ihx_sha256),
                            (if russian { "SHA256 конфигурации" } else { "Configuration SHA256" }, &build.configuration_sha256),
                            (if russian { "SHA256 компилятора" } else { "Compiler SHA256" }, &build.compiler_sha256),
                            (if russian { "SHA256 манифеста исходников" } else { "Source-manifest SHA256" }, &build.source_manifest_sha256),
                            (if russian { "SHA256 отчёта памяти" } else { "Memory-report SHA256" }, &build.mem_sha256),
                            (if russian { "SHA256 отчёта сборки" } else { "Build-report SHA256" }, &build.build_report_sha256),
                        ] { ui.label(label); ui.add(egui::Label::new(hash).wrap()); }
                    });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::Queryable;

    #[test]
    fn historical_profile_details_open_and_close_in_both_languages() {
        for language in Language::ALL {
            let russian = language == Language::Russian;
            let mut harness = egui_kittest::Harness::builder()
                .with_size(egui::vec2(1000.0, 10000.0))
                .build_ui(move |ui| profile_help(ui, language));
            harness.run();
            let images = catalog::profiles();
            let ids: std::collections::HashSet<_> = images
                .iter()
                .map(|image| {
                    egui::Id::new((
                        "ak820_profile_help_details",
                        catalog::BETA_RELEASE,
                        image.profile.as_str(),
                    ))
                })
                .collect();
            assert_eq!(ids.len(), 10);
            assert_eq!(images.len(), 10);
            for image in images {
                let header = format!(
                    "{}: {}",
                    if russian {
                        "Исторические подробности"
                    } else {
                        "Historical details"
                    },
                    image.profile.as_str()
                );
                assert!(harness.query_by_label(&header).is_some());
                let status = format!(
                    "{}: {}",
                    if russian {
                        "Статус каталога (без интерпретации)"
                    } else {
                        "Catalog status (uninterpreted)"
                    },
                    image.status
                );
                assert!(harness.query_by_label(&status).is_none());
            }
            let image = &images[0];
            let header = format!(
                "{}: {}",
                if russian {
                    "Исторические подробности"
                } else {
                    "Historical details"
                },
                image.profile.as_str()
            );
            let status = format!(
                "{}: {}",
                if russian {
                    "Статус каталога (без интерпретации)"
                } else {
                    "Catalog status (uninterpreted)"
                },
                image.status
            );
            assert!(harness.query_by_label(disclaimer(russian)).is_some());
            assert!(harness.query_by_label(&status).is_none());
            harness.get_by_label(&header).click();
            harness.run();
            assert!(harness.query_by_label(&status).is_some());
            let maxima = if russian {
                "Исторические фиксированные максимумы (не активные количества и не доступность хранилища)"
            } else {
                "Historical fixed maxima (not active counts or storage availability)"
            };
            assert!(harness.query_by_label(maxima).is_some());
            let feature = catalog::feature_description(&image.features[0]).unwrap();
            assert!(harness
                .query_by_label(feature.title.for_language(russian))
                .is_some());
            let provenance = if russian {
                "Происхождение сборки"
            } else {
                "Build provenance"
            };
            assert!(harness
                .query_by_label(&image.build_identity.ihx_sha256)
                .is_none());
            harness.get_by_label(provenance).click();
            harness.run();
            assert!(harness
                .query_by_label(&image.build_identity.ihx_sha256)
                .is_some());
            harness.get_by_label(&header).click();
            harness.run();
            assert!(harness.query_by_label(&status).is_none());
        }
    }

    #[test]
    fn profile_folds_survive_locale_and_theme_changes_and_wrap_provenance() {
        let image = &catalog::profiles()[0];
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(360.0, 10000.0))
            .build_ui_state(
                |ui, state: &mut (Language, bool)| {
                    if ui.visuals().dark_mode != state.1 {
                        ui.ctx().set_visuals(if state.1 {
                            egui::Visuals::dark()
                        } else {
                            egui::Visuals::light()
                        });
                    }
                    profile_help(ui, state.0);
                },
                (Language::English, false),
            );
        harness.run();
        harness
            .get_by_label(&format!("Historical details: {}", image.profile.as_str()))
            .click();
        harness.run();
        harness.get_by_label("Build provenance").click();
        harness.run();
        for (language, dark) in [
            (Language::English, false),
            (Language::Russian, true),
            (Language::English, true),
            (Language::Russian, false),
        ] {
            *harness.state_mut() = (language, dark);
            harness.run();
            let russian = language == Language::Russian;
            let labels = if russian {
                [
                    "SHA256 файла IHX",
                    "SHA256 конфигурации",
                    "SHA256 компилятора",
                    "SHA256 манифеста исходников",
                    "SHA256 отчёта памяти",
                    "SHA256 отчёта сборки",
                ]
            } else {
                [
                    "IHX file SHA256",
                    "Configuration SHA256",
                    "Compiler SHA256",
                    "Source-manifest SHA256",
                    "Memory-report SHA256",
                    "Build-report SHA256",
                ]
            };
            let build = &image.build_identity;
            for (label, hash) in labels.into_iter().zip([
                &build.ihx_sha256,
                &build.configuration_sha256,
                &build.compiler_sha256,
                &build.source_manifest_sha256,
                &build.mem_sha256,
                &build.build_report_sha256,
            ]) {
                let label_rect = harness.get_by_label(label).rect();
                let hash_rect = harness.get_by_label(hash).rect();
                assert!(hash_rect.right() <= 360.0, "{label}: {hash_rect:?}");
                assert!(
                    hash_rect.height() > label_rect.height(),
                    "{label}: hash did not wrap"
                );
            }
            let qualification = if russian {
                "Хеши идентифицируют входные данные и артефакты сборки каталога, но не подтверждают подключённый образ или упакованный CODE."
            } else {
                "Hashes identify catalog build inputs and artifacts, not the connected image or packed CODE verification."
            };
            assert!(harness.query_by_label(qualification).is_some());
        }
    }

    #[test]
    fn missing_feature_description_is_explicit() {
        for language in Language::ALL {
            let russian = language == Language::Russian;
            let mut harness = egui_kittest::Harness::new_ui(move |ui| {
                feature_help(ui, "unknown-test-feature", russian)
            });
            harness.run();
            let expected = format!(
                "unknown-test-feature: {}",
                if russian {
                    "описание недоступно"
                } else {
                    "description unavailable"
                }
            );
            assert!(harness.query_by_label(&expected).is_some());
        }
    }
}
