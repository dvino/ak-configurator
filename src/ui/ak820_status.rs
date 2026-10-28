//! Application controls share Entropy's navigation row, including before connection.
//! Opening a page or browsing a catalog never starts a device operation.
use super::*;

fn text(language: crate::i18n::Language, key: &'static str) -> &'static str {
    crate::i18n::tr_catalog(language, key)
}

pub(super) fn utility_window_open(ctx: &egui::Context) -> bool {
    [
        super::ak820_backup_inspection::OPEN_ID,
        super::ak820_install_files::OPEN_ID,
        super::ak820_install::OPEN_ID,
        "ak820_offline_extensions_open",
    ]
    .iter()
    .any(|id| ctx.data(|d| d.get_temp::<bool>(egui::Id::new(*id)).unwrap_or(false)))
        || super::ak820_beta1_profiles_ui::is_open(ctx)
        || super::ak820_offline_files::is_open(ctx)
}

impl EntropyApp {
    /// One preference drives the shell, settings, picker and open dialogs.
    pub(super) fn set_app_language(&mut self, language: crate::i18n::Language) {
        if self.app_settings.language == language {
            return;
        }
        self.app_settings.language = language;
        self.keycode_picker.language = language;
        save_app_settings(&self.app_settings);
    }

    pub(super) fn draw_ak820_shell_tools(
        &mut self,
        ui: &mut egui::Ui,
        viewport: egui::Rect,
        y: f32,
        suppress_tooltips: bool,
    ) {
        let lang = self.app_settings.language;
        let ctx = ui.ctx().clone();
        let menu_rect = egui::Rect::from_min_size(
            egui::pos2(viewport.left() + 18.0, y),
            egui::vec2(74.0, 28.0),
        );
        crate::ui_style::allocate_ui_at_rect(ui, menu_rect, |ui| {
            let id = egui::Id::new(super::top_dropdown::APP_MENU_ID);
            let response = crate::ui_style::modern_dropdown_button_sized(
                ui,
                id,
                text(lang, "ak820_ui.menu"),
                ui.visuals().text_color(),
                74.0,
                28.0,
                12.5,
            );
            if response.clicked() {
                self.close_hover_top_dropdowns(&ctx);
            }
            let width = 270.0_f32.min((ctx.content_rect().width() - 48.0).max(180.0));
            crate::ui_style::popup_below_widget_with_width(
                ui,
                id,
                &response,
                egui::PopupCloseBehavior::CloseOnClickOutside,
                width,
                |ui| {
                    let item_width = ui.available_width();
                    let can_open =
                        !utility_window_open(&ctx) && !self.ak820_install_runtime.owns_hid();
                    let mut picked = None;
                    egui::ScrollArea::vertical()
                        .id_salt("ak820_application_menu_scroll")
                        .max_height((ctx.content_rect().height() - y - 56.0).max(120.0))
                        .show(ui, |ui| {
                            for (action, label, icon) in [
                                ("profiles", "ak820_ui.profiles", TopMenuIcon::Device),
                                ("install", "ak820_ui.install", TopMenuIcon::ImportLayout),
                                ("backups", "ak820_ui.backups", TopMenuIcon::ExportLayout),
                                ("files", "ak820_ui.check_files", TopMenuIcon::AboutDevice),
                            ] {
                                let response = top_dropdown_icon_item(
                                    ui,
                                    item_width,
                                    icon,
                                    text(lang, label),
                                    can_open,
                                    false,
                                );
                                if !can_open {
                                    response
                                        .clone()
                                        .on_hover_text(text(lang, "ak820_ui.finish_dialog"));
                                }
                                if response.clicked() {
                                    picked = Some(action);
                                }
                            }
                            if self.ak820_demo {
                                top_dropdown_divider(ui, item_width);
                                if top_dropdown_icon_item(
                                    ui,
                                    item_width,
                                    TopMenuIcon::ImportLayout,
                                    text(lang, "ak820_ui.project"),
                                    can_open,
                                    false,
                                )
                                .clicked()
                                {
                                    picked = Some("project");
                                }
                                if top_dropdown_icon_item(
                                    ui,
                                    item_width,
                                    TopMenuIcon::Modules,
                                    text(lang, "ak820_ui.offline_modules"),
                                    can_open,
                                    false,
                                )
                                .clicked()
                                {
                                    picked = Some("modules");
                                }
                            }
                            top_dropdown_divider(ui, item_width);
                            for (action, label, icon, selected) in [
                                (
                                    "settings",
                                    "ak820_ui.app_settings",
                                    TopMenuIcon::AppSettings,
                                    self.main_menu_tab == MainMenuTab::Settings
                                        && self.settings_tab == SettingsTab::AppSettings,
                                ),
                                (
                                    "about",
                                    "ak820_ui.about",
                                    TopMenuIcon::AboutEntropy,
                                    self.main_menu_tab == MainMenuTab::Settings
                                        && self.settings_tab == SettingsTab::AboutEntropy,
                                ),
                            ] {
                                if top_dropdown_icon_item(
                                    ui,
                                    item_width,
                                    icon,
                                    text(lang, label),
                                    can_open,
                                    selected,
                                )
                                .clicked()
                                {
                                    picked = Some(action);
                                }
                            }
                            top_dropdown_divider(ui, item_width);
                            if top_dropdown_icon_item(
                                ui,
                                item_width,
                                TopMenuIcon::AboutDevice,
                                text(lang, "ak820_ui.help"),
                                true,
                                false,
                            )
                            .clicked()
                            {
                                picked = Some("help");
                            }
                            if self.keycode_picker.ak820_profile && !self.ak820_demo {
                                let ready = can_open
                                    && !self.hid_write_lifecycle_busy()
                                    && matches!(self.connect_state, ConnectState::Idle)
                                    && self.selected_device.is_some();
                                if top_dropdown_icon_item(
                                    ui,
                                    item_width,
                                    TopMenuIcon::Device,
                                    text(lang, "ak820_ui.reread"),
                                    ready,
                                    false,
                                )
                                .clicked()
                                {
                                    picked = Some("reread");
                                }
                            }
                            top_dropdown_divider(ui, item_width);
                            if top_dropdown_icon_item(
                                ui,
                                item_width,
                                TopMenuIcon::AboutEntropy,
                                text(lang, "ak820_ui.quit_with_shortcut"),
                                true,
                                false,
                            )
                            .clicked()
                            {
                                picked = Some("quit");
                            }
                        });
                    if let Some(action) = picked {
                        self.close_top_dropdowns(&ctx);
                        egui::Popup::close_all(&ctx);
                        match action {
                            "profiles" => super::ak820_beta1_profiles_ui::open(&ctx),
                            "project" => super::ak820_offline_files::open(&ctx),
                            "modules" => {
                                ctx.data_mut(|d| {
                                    d.insert_temp(
                                        egui::Id::new("ak820_offline_extensions_open"),
                                        true,
                                    )
                                });
                            }
                            "install" | "backups" | "files" => {
                                let id = match action {
                                    "install" => super::ak820_install::OPEN_ID,
                                    "backups" => super::ak820_backup_inspection::OPEN_ID,
                                    _ => super::ak820_install_files::OPEN_ID,
                                };
                                ctx.data_mut(|d| d.insert_temp(egui::Id::new(id), true));
                            }
                            "settings" => self.open_app_settings_page(),
                            "about" => self.open_about_entropy_page(),
                            "quit" => self.request_explicit_quit(&ctx),
                            "help" => super::ak820_help_ui::open(&ctx),
                            "reread" => {
                                if let Some(index) = self.selected_device {
                                    self.start_connect(index);
                                }
                            }
                            _ => {}
                        }
                    }
                },
            );
        });
        let language_rect = egui::Rect::from_min_size(
            egui::pos2(viewport.right() - 108.0, y),
            egui::vec2(54.0, 28.0),
        );
        crate::ui_style::allocate_ui_at_rect(ui, language_rect, |ui| {
            let id = egui::Id::new(super::top_dropdown::LANGUAGE_MENU_ID);
            let label = if lang == crate::i18n::Language::Russian {
                "RU"
            } else {
                "EN"
            };
            let response = crate::ui_style::modern_dropdown_button_sized(
                ui,
                id,
                label,
                ui.visuals().text_color(),
                54.0,
                28.0,
                12.5,
            );
            if response.clicked() {
                self.close_hover_top_dropdowns(&ctx);
            }
            if !suppress_tooltips {
                response
                    .clone()
                    .on_hover_text(crate::i18n::tr(lang, crate::i18n::Key::LanguageTooltip));
            }
            let labels = crate::i18n::Language::ALL.map(|l| l.native_name().to_owned());
            let selected = usize::from(lang == crate::i18n::Language::Russian);
            if let Some(index) = crate::ui_style::modern_dropdown_popup_options(
                ui, id, &response, &labels, selected, 128.0, 12.5,
            ) {
                self.set_app_language(crate::i18n::Language::ALL[index]);
                ctx.request_repaint();
            }
        });
        let help_rect = egui::Rect::from_min_size(
            egui::pos2(viewport.right() - 46.0, y),
            egui::vec2(28.0, 28.0),
        );
        crate::ui_style::allocate_ui_at_rect(ui, help_rect, |ui| {
            if crate::ui_style::modern_button(ui, "?", help_rect.size(), true)
                .on_hover_text(text(lang, "ak820_ui.help"))
                .clicked()
            {
                super::ak820_help_ui::open(&ctx);
            }
        });
    }

    pub(super) fn draw_ak820_status(&self, ui: &mut egui::Ui) {
        let lang = self.app_settings.language;
        let failure = self
            .hid_device
            .as_ref()
            .and_then(crate::hid::HidDevice::ak820_failure);
        let label = if failure.is_some() {
            text(lang, "ak820_ui.stopped").to_owned()
        } else if self.ak820_demo {
            format!(
                "{} · {}",
                text(lang, "ak820_ui.offline"),
                self.ak820_demo_profile
                    .map(|p| p.as_str())
                    .unwrap_or("main")
            )
        } else if let Some(image) = self
            .hid_device
            .as_ref()
            .and_then(crate::hid::HidDevice::ak820_image)
        {
            format!("AK820 · {}", image.label())
        } else {
            text(lang, "ak820_ui.disconnected").to_owned()
        };
        let color = if failure.is_some() {
            ui.visuals().error_fg_color
        } else {
            app_muted_text(self.dark_mode)
        };
        let response =
            ui.add(egui::Label::new(RichText::new(label).size(12.0).color(color)).truncate());
        if let Some(failure) = failure {
            response.on_hover_text(format!(
                "{}\n{}",
                text(lang, "ak820_ui.reread_hint"),
                failure
            ));
        }
    }

    pub(super) fn draw_disconnected_navigation(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        viewport: egui::Rect,
    ) -> bool {
        let tabs = self.draw_layout_top_tabs(ui, ctx, viewport, viewport.top() + 6.0, false);
        self.draw_layout_top_dropdowns(
            ui,
            None,
            ctx,
            tabs.lang,
            tabs.device_tab_rect,
            tabs.device_tab_hovered,
            tabs.advanced_tab_rect,
            tabs.advanced_tab_hovered,
            tabs.settings_tab_rect,
            tabs.settings_tab_hovered,
        );
        if self.main_menu_tab == MainMenuTab::Keyboard {
            return false;
        }
        let content = egui::Rect::from_min_max(
            viewport.min + egui::vec2(20.0, 52.0),
            viewport.max - egui::vec2(20.0, 70.0),
        );
        match self.settings_tab {
            SettingsTab::AppSettings => self.draw_app_settings_page(ui, content),
            SettingsTab::OsIndicator => self.draw_ak820_os_indicator_page(ui, content),
            SettingsTab::AboutEntropy => self.draw_about_entropy_page(ui, content),
            SettingsTab::ApplicationLayouts if self.offline_application_layouts_available() => {
                self.draw_application_layouts_settings_page(ui, content)
            }
            _ => {
                self.main_menu_tab = MainMenuTab::Keyboard;
                return false;
            }
        }
        self.draw_settings_navigation_hint(ui, viewport.center().x, viewport.bottom(), false);
        true
    }
}
