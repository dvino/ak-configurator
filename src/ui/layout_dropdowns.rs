use super::*;

impl EntropyApp {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_layout_top_dropdowns(
        &mut self,
        ui: &mut egui::Ui,
        layout: Option<&KeyboardLayout>,
        ctx: &egui::Context,
        lang: crate::i18n::Language,
        device_tab_rect: Option<egui::Rect>,
        device_tab_hovered: bool,
        advanced_tab_rect: Option<egui::Rect>,
        advanced_tab_hovered: bool,
        settings_tab_rect: Option<egui::Rect>,
        settings_tab_hovered: bool,
    ) {
        self.draw_layout_device_dropdown(
            ui,
            ctx,
            lang,
            device_tab_rect,
            device_tab_hovered,
            advanced_tab_hovered,
            settings_tab_hovered,
        );
        self.draw_layout_advanced_dropdown(
            ui,
            lang,
            advanced_tab_rect,
            device_tab_hovered,
            advanced_tab_hovered,
            settings_tab_hovered,
        );
        self.draw_layout_settings_dropdown(
            ui,
            layout,
            lang,
            settings_tab_rect,
            device_tab_hovered,
            advanced_tab_hovered,
            settings_tab_hovered,
        );
    }
}
