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
    pub(super) fn draw_ak820_rgb_inputs(&mut self, ui: &mut egui::Ui, scene: &Scene) {
        let ru = self.app_settings.language == crate::i18n::Language::Russian;
        let id = ui.make_persistent_id("ak820_rgb_input_controls");
        let mut controls = ui
            .ctx()
            .data_mut(|d| d.get_temp::<Controls>(id).unwrap_or_default());
        let running = self.ak820_rgb_scene_running();
        let ready = !running && self.ak820_rgb_scene_can_start() && scene.validate().is_ok();
        egui::CollapsingHeader::new(if ru { "Музыка и OpenRGB" } else { "Music and OpenRGB" })
            .default_open(true)
            .show(ui, |ui| {
                ui.add_enabled_ui(!running, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(if ru { "Источник звука" } else { "Audio source" });
                        egui::ComboBox::from_id_salt("ak820_music_source")
                            .selected_text(if controls.test {
                                if ru { "Тест без захвата" } else { "Test without capture" }
                            } else if ru { "Системный звук" } else { "System audio" })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut controls.test, false, if ru { "Системный звук (основной выход)" } else { "System audio (default output)" });
                                ui.selectable_value(&mut controls.test, true, if ru { "Тест без захвата" } else { "Test without capture" });
                            });
                        egui::ComboBox::from_id_salt("ak820_music_display")
                            .selected_text(display_label(controls.options.display, ru))
                            .show_ui(ui, |ui| {
                                for display in [Display::Spectrum, Display::Pulse, Display::Bass] {
                                    ui.selectable_value(&mut controls.options.display, display, display_label(display, ru));
                                }
                            });
                        let mut color = Color32::from_rgb(controls.options.color[0], controls.options.color[1], controls.options.color[2]);
                        if ui.color_edit_button_srgba(&mut color).changed() {
                            controls.options.color = [color.r(), color.g(), color.b()];
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.checkbox(&mut controls.options.automatic_gain, if ru { "Авточувствительность" } else { "Automatic sensitivity" });
                        ui.add_enabled(!controls.options.automatic_gain,
                            egui::Slider::new(&mut controls.options.sensitivity, 1..=300)
                                .text(if ru { "Чувствительность" } else { "Sensitivity" }));
                        ui.add(egui::Slider::new(&mut controls.options.smoothing, 0..=95)
                            .text(if ru { "Сглаживание" } else { "Smoothing" }));
                    });
                });
                ui.label(if ru {
                    "Захват начинается только после запуска. Звук не записывается и не передаётся. Яркость и тихие часы берутся из сцены."
                } else {
                    "Capture starts only when requested. Audio is never recorded or transmitted. Scene brightness and quiet hours apply."
                });
                ui.horizontal_wrapped(|ui| {
                    if ui.add_enabled(ready, egui::Button::new(if controls.test {
                        if ru { "Запустить тест" } else { "Start test" }
                    } else if ru { "Запустить музыку" } else { "Start music" })).clicked() {
                        self.ak820_rgb_music_start(scene.clone(), controls.options.clone(), controls.test);
                    }
                    if ui.add_enabled(running, egui::Button::new(if ru { "Остановить источник" } else { "Stop source" })).clicked() {
                        self.ak820_rgb_scene_stop();
                    }
                    if ui.add_enabled(self.ak820_rgb_can_restore(), egui::Button::new(
                        if ru { "Остановить и вернуть эффект" } else { "Stop and restore effect" }
                    )).clicked() {
                        self.ak820_rgb_stop_and_restore();
                    }
                });
                if let Some(label) = self.ak820_rgb_input_label(ru) {
                    ui.label(RichText::new(label).strong());
                }
                ui.collapsing(if ru { "Источник OpenRGB" } else { "OpenRGB source" }, |ui| {
                    ui.label(if ru {
                        "Для совместимых локальных эффектов и плагинов. Клиент подключается к 127.0.0.1; одновременно работает один источник."
                    } else {
                        "For compatible local effects and plugins. The client connects to 127.0.0.1; one source runs at a time."
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.label(if ru { "Порт" } else { "Port" });
                        ui.add_enabled(!running, egui::DragValue::new(&mut controls.port).range(1024..=65535));
                        if ui.add_enabled(ready, egui::Button::new(if ru { "Принимать OpenRGB" } else { "Receive OpenRGB" })).clicked() {
                            self.ak820_rgb_openrgb_start(scene.clone(), controls.port);
                        }
                    });
                });
            });
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
                app.draw_ak820_rgb_inputs(ui, &Scene::default())
            });
            harness.run();
            drop(harness);
            assert!(app.hid_device.is_none());
            assert!(!app.ak820_rgb_scene_running());
        }
    }
}
