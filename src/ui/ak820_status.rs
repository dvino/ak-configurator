use super::*;

impl EntropyApp {
    pub(super) fn draw_ak820_status(&mut self, ui: &mut egui::Ui) {
        let ru = self.app_settings.language == crate::i18n::Language::Russian;
        let text = |r, e| if ru { r } else { e };
        let help_id = egui::Id::new("ak820_help");
        let mut help_open = ui
            .ctx()
            .data(|d| d.get_temp::<bool>(help_id))
            .unwrap_or(false);
        let failed = self
            .hid_device
            .as_ref()
            .and_then(crate::hid::HidDevice::ak820_failure);
        ui.horizontal_wrapped(|ui| {
            ui.strong("AK Configurator");
            if self.ak820_demo {
                ui.label(text(
                    "ДЕМО · без подключения к клавиатуре",
                    "DEMO · no keyboard connection",
                ));
            } else if self.keycode_picker.ak820_profile {
                if let Some(image) = self.hid_device.as_ref().and_then(crate::hid::HidDevice::ak820_image) {
                    ui.label(format!("AK820 · {}", image.label()));
                }
                if ui
                    .add_enabled(
                        !self.hid_write_task_owner_active()
                            && matches!(self.connect_state, ConnectState::Idle),
                        egui::Button::new(text("Прочитать настройки снова", "Read settings again")),
                    )
                    .clicked()
                {
                    if let Some(index) = self.selected_device {
                        self.start_connect(index);
                    }
                }
            } else {
                ui.label(text(
                    "Проводная AK820 · проверка прошивки перед редактированием",
                    "Wired AK820 · firmware verified before editing",
                ));
            }
            if ui.button(text("Справка", "Help")).clicked() {
                help_open = !help_open;
            }
            if ui.button(text("Локальная копия", "Local backup")).clicked() {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::new(super::ak820_backup_inspection::OPEN_ID), true)
                });
            }
        });
        if let Some(reason) = &failed {
            ui.colored_label(
                Color32::from_rgb(220, 70, 60),
                text(
                    "Операция остановлена. Перед продолжением прочитайте настройки снова.",
                    "Operation stopped. Read the settings again before continuing.",
                ),
            );
            ui.label(reason);
        }
        ui.separator();
        egui::Window::new(text("Справка AK820", "AK820 help"))
            .id(help_id)
            .open(&mut help_open)
            .default_width(620.0)
            .show(ui.ctx(), |ui| {
                egui::ScrollArea::vertical()
                    .max_height(580.0)
                    .show(ui, |ui| {
                        for (title, body) in crate::ak820_help::sections(self.app_settings.language)
                        {
                            ui.heading(*title);
                            ui.label(*body);
                            ui.add_space(12.0);
                        }
                    });
            });
        ui.ctx().data_mut(|d| d.insert_temp(help_id, help_open));
        if failed.is_some() {
            ui.disable();
        }
    }
}
