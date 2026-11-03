use super::*;

pub(super) struct LayoutChromeGeometry {
    pub(super) top_base_y: f32,
    pub(super) main_tabs_h: f32,
    pub(super) layer_bar_h: f32,
    pub(super) top_reserved_h: f32,
    pub(super) viewport: egui::Rect,
}

impl EntropyApp {
    pub(super) fn draw_layout_chrome(
        &mut self,
        ui: &mut egui::Ui,
        layout: &KeyboardLayout,
        ctx: &egui::Context,
        geometry: LayoutChromeGeometry,
    ) -> bool {
        let LayoutChromeGeometry {
            top_base_y,
            main_tabs_h,
            layer_bar_h,
            top_reserved_h,
            viewport,
        } = geometry;
        let typing_trainer_page = self.main_menu_tab == MainMenuTab::Advanced
            && self.settings_tab == SettingsTab::TypingTrainer;
        if typing_trainer_page {
            let now = std::time::Instant::now();
            self.typing_trainer.remaining_secs_at(now);
            if self.typing_trainer.is_finished() {
                self.typing_trainer.ui_hidden = false;
            }
            self.handle_typing_trainer_input(ctx);
            if self.typing_trainer.ui_hidden {
                self.close_hover_top_dropdowns(ctx);
            }
        }
        let chrome_opacity = self.typing_trainer_chrome_opacity(ctx);
        if chrome_opacity > 0.0 && chrome_opacity < 1.0 {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }

        // ── Main menu tabs ────────────────────────────────────────────────
        {
            let chrome_rect = viewport;
            let top_tabs = crate::ui_style::allocate_ui_at_rect(ui, chrome_rect, |ui| {
                ui.set_opacity(chrome_opacity);
                if chrome_opacity <= 0.96 {
                    ui.disable();
                }
                self.draw_layout_top_tabs(
                    ui,
                    ctx,
                    chrome_rect,
                    top_base_y,
                    self.unlock_open || self.vial_unlock_polling || chrome_opacity < 0.96,
                )
            })
            .inner;

            if self.unlock_open || self.vial_unlock_polling {
                self.close_hover_top_dropdowns(ctx);
            } else if chrome_opacity > 0.01 {
                crate::ui_style::allocate_ui_at_rect(ui, chrome_rect, |ui| {
                    ui.set_opacity(chrome_opacity);
                    if chrome_opacity <= 0.96 {
                        ui.disable();
                    }
                    self.draw_layout_top_dropdowns(
                        ui,
                        Some(layout),
                        ctx,
                        top_tabs.lang,
                        top_tabs.device_tab_rect,
                        top_tabs.device_tab_hovered && chrome_opacity > 0.96,
                        top_tabs.advanced_tab_rect,
                        top_tabs.advanced_tab_hovered && chrome_opacity > 0.96,
                        top_tabs.settings_tab_rect,
                        top_tabs.settings_tab_hovered && chrome_opacity > 0.96,
                    );
                });
            }
            self.dismiss_application_layouts_dialogs_if_page_inactive();
            self.dismiss_text_expander_rule_editor_if_page_inactive();
            if matches!(
                self.main_menu_tab,
                MainMenuTab::Settings | MainMenuTab::Advanced
            ) {
                #[cfg(not(target_arch = "wasm32"))]
                let editable = matches!(
                    self.settings_tab,
                    SettingsTab::AppSettings | SettingsTab::AboutEntropy
                ) || self
                    .hid_device
                    .as_ref()
                    .and_then(crate::hid::HidDevice::ak820_failure)
                    .is_none();
                #[cfg(all(test, not(target_arch = "wasm32")))]
                let editable = editable || matches!(self.settings_tab, SettingsTab::OsIndicator);
                #[cfg(target_arch = "wasm32")]
                let editable = true;
                ui.add_enabled_ui(editable, |ui| {
                    self.draw_settings_screen(
                        ui,
                        layout,
                        ctx,
                        chrome_rect.top() + top_reserved_h,
                        chrome_rect,
                    );
                });
                return true;
            }

            crate::ui_style::allocate_ui_at_rect(ui, chrome_rect, |ui| {
                ui.set_opacity(chrome_opacity);
                if chrome_opacity <= 0.96 {
                    ui.disable();
                }
                self.draw_layout_layer_switcher_and_hints(ui, top_base_y, main_tabs_h, layer_bar_h);
            });
        }
        false
    }
}
