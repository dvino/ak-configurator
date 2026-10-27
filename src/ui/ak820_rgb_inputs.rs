//! Controls for upstream CAVA and optional local OpenRGB sources.
use super::*;
use crate::ak820_rgb_audio::{Display, Options};
use crate::ak820_rgb_compositor::Scene;

#[derive(Clone)]
struct Controls {
    options: Options,
    test: bool,
    port: u16,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum RgbInputSection {
    Music,
    OpenRgb,
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            options: Options::default(),
            test: false,
            port: crate::ak820_openrgb_bridge::DEFAULT_PORT,
        }
    }
}

impl EntropyApp {
    pub(super) fn draw_ak820_rgb_inputs(
        &mut self,
        ui: &mut egui::Ui,
        scene: &Scene,
        section: RgbInputSection,
    ) {
        let ru = self.app_settings.language == crate::i18n::Language::Russian;
        let id = egui::Id::new("ak820_rgb_input_controls");
        let mut controls = ui
            .ctx()
            .data_mut(|d| d.get_temp::<Controls>(id).unwrap_or_default());
        let running = self.ak820_rgb_scene_running();
        let ready = !running && self.ak820_rgb_scene_can_start() && scene.validate().is_ok();
        match section {
            RgbInputSection::Music => {
                ui.add_enabled_ui(!running, |ui| {
                    ui.spacing_mut().interact_size.y = 30.0;
                    ui.spacing_mut().slider_width = 100.0;
                    ui.horizontal_wrapped(|ui| {
                        ui.label(if ru {
                            "Источник звука"
                        } else {
                            "Audio source"
                        });
                        let source_labels = if ru {
                            ["Системный звук", "Тест без захвата"]
                        } else {
                            ["System audio", "Test without capture"]
                        }
                        .map(str::to_owned);
                        if let (_, Some(index)) = crate::ui_style::modern_dropdown_select_sized(
                            ui,
                            id.with("source"),
                            &source_labels,
                            usize::from(controls.test),
                            184.0,
                            32.0,
                            12.5,
                        ) {
                            controls.test = index == 1;
                        }
                        let displays = [Display::Spectrum, Display::Pulse, Display::Bass];
                        let display_labels =
                            displays.map(|value| display_label(value, ru).to_owned());
                        let selected = displays
                            .iter()
                            .position(|value| *value == controls.options.display)
                            .unwrap_or(0);
                        if let (_, Some(index)) = crate::ui_style::modern_dropdown_select_sized(
                            ui,
                            id.with("display"),
                            &display_labels,
                            selected,
                            140.0,
                            32.0,
                            12.5,
                        ) {
                            controls.options.display = displays[index];
                        }
                        let mut color = Color32::from_rgb(
                            controls.options.color[0],
                            controls.options.color[1],
                            controls.options.color[2],
                        );
                        if ui.color_edit_button_srgba(&mut color).changed() {
                            controls.options.color = [color.r(), color.g(), color.b()];
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        crate::ui_style::settings_switch_sized_stable(
                            ui,
                            "ak820_music_automatic_gain",
                            &mut controls.options.automatic_gain,
                            Vec2::new(42.0, 24.0),
                        );
                        ui.label(if ru {
                            "Авточувствительность"
                        } else {
                            "Automatic sensitivity"
                        });
                        ui.add_enabled(
                            !controls.options.automatic_gain,
                            egui::Slider::new(&mut controls.options.sensitivity, 1..=300).text(
                                if ru {
                                    "Чувствительность"
                                } else {
                                    "Sensitivity"
                                },
                            ),
                        );
                        ui.add(
                            egui::Slider::new(&mut controls.options.smoothing, 0..=95).text(
                                if ru {
                                    "Сглаживание"
                                } else {
                                    "Smoothing"
                                },
                            ),
                        );
                    });
                });
                ui.label(if ru {
                    "Захват начинается только после запуска. Звук не записывается и не передаётся. Яркость и тихие часы берутся из сцены."
                } else {
                    "Capture starts only when requested. Audio is never recorded or transmitted. Scene brightness and quiet hours apply."
                });
                ui.horizontal_wrapped(|ui| {
                    let start_label = if controls.test {
                        if ru {
                            "Запустить тест"
                        } else {
                            "Start test"
                        }
                    } else if ru {
                        "Запустить музыку"
                    } else {
                        "Start music"
                    };
                    if crate::ui_style::modern_button(
                        ui,
                        start_label,
                        Vec2::new(140.0, 34.0),
                        ready,
                    )
                    .clicked()
                    {
                        self.ak820_rgb_music_start(
                            scene.clone(),
                            controls.options.clone(),
                            controls.test,
                        );
                    }
                    if crate::ui_style::modern_button(
                        ui,
                        if ru {
                            "Остановить источник"
                        } else {
                            "Stop source"
                        },
                        Vec2::new(150.0, 34.0),
                        running,
                    )
                    .clicked()
                    {
                        self.ak820_rgb_scene_stop();
                    }
                    if crate::ui_style::modern_button(
                        ui,
                        if ru {
                            "Остановить и вернуть эффект"
                        } else {
                            "Stop and restore effect"
                        },
                        Vec2::new(205.0, 34.0),
                        self.ak820_rgb_can_restore(),
                    )
                    .clicked()
                    {
                        self.ak820_rgb_stop_and_restore();
                    }
                });
                if let Some(label) = self.ak820_rgb_input_label(ru) {
                    ui.label(RichText::new(label).strong());
                }
            }
            RgbInputSection::OpenRgb => {
                ui.label(if ru {
                    "Локальный клиент подключается к 127.0.0.1; одновременно работает один источник."
                } else {
                    "Connect an OpenRGB SDK client to 127.0.0.1. Only one source runs at a time."
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label(if ru { "Порт" } else { "Port" });
                    ui.add_enabled(
                        !running,
                        egui::DragValue::new(&mut controls.port).range(1024..=65535),
                    );
                    if crate::ui_style::modern_button(
                        ui,
                        if ru {
                            "Принимать OpenRGB"
                        } else {
                            "Receive OpenRGB"
                        },
                        Vec2::new(164.0, 34.0),
                        ready,
                    )
                    .clicked()
                    {
                        self.ak820_rgb_openrgb_start(scene.clone(), controls.port);
                    }
                    if crate::ui_style::modern_button(
                        ui,
                        if ru {
                            "Остановить источник"
                        } else {
                            "Stop source"
                        },
                        Vec2::new(150.0, 34.0),
                        running,
                    )
                    .clicked()
                    {
                        self.ak820_rgb_scene_stop();
                    }
                    if crate::ui_style::modern_button(
                        ui,
                        if ru {
                            "Остановить и вернуть эффект"
                        } else {
                            "Stop and restore effect"
                        },
                        Vec2::new(205.0, 34.0),
                        self.ak820_rgb_can_restore(),
                    )
                    .clicked()
                    {
                        self.ak820_rgb_stop_and_restore();
                    }
                });
                if let Some(label) = self.ak820_rgb_input_label(ru) {
                    ui.label(RichText::new(label).strong());
                }
            }
        }
        ui.ctx().data_mut(|d| d.insert_temp(id, controls));
    }
}

fn display_label(display: Display, ru: bool) -> &'static str {
    match (display, ru) {
        (Display::Spectrum, true) => "Спектр",
        (Display::Pulse, true) => "Пульсация",
        (Display::Bass, true) => "Бас",
        (Display::Spectrum, false) => "Spectrum",
        (Display::Pulse, false) => "Pulse",
        (Display::Bass, false) => "Bass",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ak820_beta1_profiles::ProfileId;

    #[test]
    fn input_controls_render_in_both_languages_with_no_device() {
        for language in [
            crate::i18n::Language::Russian,
            crate::i18n::Language::English,
        ] {
            let mut app = EntropyApp::new_inert_for_test();
            app.ak820_demo = true;
            app.ak820_demo_profile = Some(ProfileId::Main);
            app.app_settings.language = language;
            let mut harness = egui_kittest::Harness::new_ui(|ui| {
                app.draw_ak820_rgb_inputs(ui, &Scene::default(), RgbInputSection::Music)
            });
            harness.run();
            drop(harness);
            assert!(app.hid_device.is_none());
            assert!(!app.ak820_rgb_scene_running());
        }
    }
}
