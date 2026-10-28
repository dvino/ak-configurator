use super::*;

const AUTO_SHIFT_TIMEOUT_MIN_MS: u16 = 1;
const AUTO_SHIFT_TIMEOUT_MAX_MS: u16 = 1000;

impl EntropyApp {
    pub(super) fn draw_auto_shift_settings_page(
        &mut self,
        ui: &mut egui::Ui,
        content_rect: egui::Rect,
        dark: bool,
    ) {
        let lang = self.app_settings.language;
        let hid_ready = self.hid_connected_for_display();

        crate::ui_style::allocate_ui_at_rect(ui, content_rect, |ui| {
            self.preserve_device_editor_during_hid_loan(ui);
            ui.vertical_centered(|ui| {
                ui.add_space(18.0);
                ui.label(
                    RichText::new(crate::i18n::tr(lang, crate::i18n::Key::AutoShiftTitle))
                        .size(18.0)
                        .strong(),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(crate::i18n::tr(
                        lang,
                        crate::i18n::Key::AutoShiftDescription,
                    ))
                    .size(13.0)
                    .color(app_muted_text(dark)),
                );
                ui.add_space(24.0);

                if self.auto_shift_timeout.is_none() {
                    crate::ui_style::modal_empty_state(
                        ui,
                        crate::i18n::tr(lang, crate::i18n::Key::AutoShiftUnavailable),
                        Some(crate::i18n::tr(lang, crate::i18n::Key::AutoShiftEnableHint)),
                    );
                    return;
                }

                if !hid_ready {
                    crate::ui_style::modal_empty_state(
                        ui,
                        crate::i18n::tr_catalog(self.app_settings.language, "auto_shift_settings.connect_a_vial_keyboard_to_edit_auto_shift_settings"),
                        None,
                    );
                    return;
                }

                if self.is_vial_locked() {
                    crate::ui_style::modal_empty_state(
                        ui,
                        crate::i18n::tr(lang, crate::i18n::Key::KeyboardLocked),
                        Some(crate::i18n::tr(lang, crate::i18n::Key::AutoShiftUnlockHint)),
                    );
                    return;
                }

                let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
                self.draw_auto_shift_editor_content(ui, dark, metrics);
            });
        });
    }

    fn draw_auto_shift_editor_content(
        &mut self,
        ui: &mut egui::Ui,
        dark: bool,
        metrics: crate::ui_style::ResponsiveMetrics,
    ) {
        let flags_supported = self.supported_qmk_settings.contains(&3);
        let timeout_supported = self.supported_qmk_settings.contains(&4);
        let row_count = usize::from(flags_supported) * 7 + usize::from(timeout_supported);
        if !flags_supported {
            ui.label(RichText::new(match self.app_settings.language {
                crate::i18n::Language::Russian => "В этой версии прошивки здесь можно менять только задержку Auto Shift. Для включения и выключения назначьте клавише действие AS_ON, AS_OFF или AS_TOGGLE.",
                crate::i18n::Language::English => "This firmware version only allows the Auto Shift timeout to be changed here. To turn Auto Shift on or off, assign AS_ON, AS_OFF or AS_TOGGLE to a key.",
            }).size(13.0));
            ui.add_space(12.0);
        }
        let field_width = metrics.value(86.0);
        let list = allocate_adaptive_settings_list_viewport(
            ui,
            "auto_shift_settings",
            metrics,
            row_count,
            0.0,
        );

        crate::ui_style::allocate_ui_at_rect(ui, list.content_rect, |ui| {
            ui.set_clip_rect(list.viewport);
            ui.set_min_size(list.content_rect.size());
            ui.spacing_mut().item_spacing.y = 0.0;
            for row_idx in list.first_visible_row..list.last_visible_row {
                let setting_row = if flags_supported { row_idx } else { 7 };
                self.draw_auto_shift_row(ui, setting_row, &list, field_width, dark);
            }
        });

        if list.has_scrollbar {
            crate::ui_style::paint_floating_scrollbar_handle(
                ui,
                list.track_rect,
                list.handle_height,
                list.scroll_ratio,
                list.track_hovered,
            );
        }
    }

    fn draw_auto_shift_row(
        &mut self,
        ui: &mut egui::Ui,
        row_idx: usize,
        list: &AdaptiveSettingsListViewport,
        field_width: f32,
        _dark: bool,
    ) {
        if !self
            .supported_qmk_settings
            .contains(&if row_idx == 7 { 4 } else { 3 })
        {
            return;
        }
        let content_width = list.row_content_width;
        let row_height = list.row_height;
        let suppress_tooltips = list.suppress_tooltips;
        let row = match row_idx {
            0 => (
                crate::i18n::tr_catalog(self.app_settings.language, "common.enable"),
                crate::i18n::tr_catalog(self.app_settings.language, "auto_shift.enable_tooltip"),
                true,
            ),
            1 => (
                crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "auto_shift.enable_for_modifiers",
                ),
                crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "auto_shift.enable_for_modifiers_tooltip",
                ),
                true,
            ),
            2 => (
                crate::i18n::tr_catalog(self.app_settings.language, "auto_shift.no_special_keys"),
                crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "auto_shift.no_special_keys_tooltip",
                ),
                true,
            ),
            3 => (
                crate::i18n::tr_catalog(self.app_settings.language, "auto_shift.no_numeric_keys"),
                crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "auto_shift.no_numeric_keys_tooltip",
                ),
                true,
            ),
            4 => (
                crate::i18n::tr_catalog(self.app_settings.language, "auto_shift.no_alpha_keys"),
                crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "auto_shift.no_alpha_keys_tooltip",
                ),
                true,
            ),
            5 => (
                crate::i18n::tr_catalog(self.app_settings.language, "auto_shift.enable_keyrepeat"),
                crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "auto_shift.enable_keyrepeat_tooltip",
                ),
                true,
            ),
            6 => (
                crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "auto_shift.stop_repeat_after_timeout",
                ),
                crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "auto_shift.stop_repeat_after_timeout_tooltip",
                ),
                true,
            ),
            7 => (
                crate::i18n::tr_catalog(self.app_settings.language, "common.timeout"),
                crate::i18n::tr_catalog(self.app_settings.language, "auto_shift.timeout_tooltip"),
                false,
            ),
            _ => return,
        };
        let enabled = row_idx == 0
            || !self.supported_qmk_settings.contains(&3)
            || (self.auto_shift_options.loaded && self.auto_shift_options.enabled);
        let tooltip = if suppress_tooltips { None } else { Some(row.1) };
        if row.2 {
            let mut value = match row_idx {
                0 => self.auto_shift_options.enabled,
                1 => self.auto_shift_options.enable_for_modifiers,
                2 => self.auto_shift_options.no_special,
                3 => self.auto_shift_options.no_numeric,
                4 => self.auto_shift_options.no_alpha,
                5 => self.auto_shift_options.enable_keyrepeat,
                6 => self.auto_shift_options.disable_keyrepeat_timeout,
                _ => false,
            };
            crate::ui_style::settings_list_row_with_tooltip(
                ui,
                content_width,
                row_height,
                row.0,
                enabled,
                tooltip,
                46.0,
                |ui| {
                    let resp = crate::ui_style::settings_switch_sized_stable_interactive(
                        ui,
                        ("auto_shift_settings", row_idx),
                        &mut value,
                        egui::vec2(46.0, 24.0),
                        enabled,
                    );
                    if resp.changed() {
                        self.write_auto_shift_flag(row_idx, value);
                    }
                },
            );
        } else {
            let timeout_value = self.auto_shift_timeout.unwrap_or(175);
            if self.auto_shift_timeout_text.is_empty() {
                self.auto_shift_timeout_text = timeout_value.to_string();
            }
            crate::ui_style::settings_list_row_with_tooltip(
                ui,
                content_width,
                row_height,
                row.0,
                enabled,
                tooltip,
                field_width,
                |ui| {
                    let edit_id = egui::Id::new("auto_shift_timeout");
                    let resp = crate::ui_style::modern_text_field_interactive(
                        ui,
                        edit_id,
                        &mut self.auto_shift_timeout_text,
                        field_width,
                        "",
                        5,
                        egui::Align::RIGHT,
                        enabled,
                    );
                    let resp = settings_field_unit_tooltip(
                        resp,
                        self.app_settings.language,
                        suppress_tooltips,
                        SettingsFieldUnit::Milliseconds,
                    );
                    if resp.changed() {
                        let filtered: String = self
                            .auto_shift_timeout_text
                            .chars()
                            .filter(|c: &char| c.is_ascii_digit())
                            .collect();
                        if filtered != self.auto_shift_timeout_text {
                            self.auto_shift_timeout_text = filtered.clone();
                        }
                        if let Ok(parsed) = filtered.parse::<u32>() {
                            let timeout_value = parsed.clamp(
                                AUTO_SHIFT_TIMEOUT_MIN_MS as u32,
                                AUTO_SHIFT_TIMEOUT_MAX_MS as u32,
                            ) as u16;
                            self.write_auto_shift_timeout(timeout_value);
                        }
                    }
                },
            );
        }
    }

    fn write_auto_shift_flag(&mut self, bit: usize, enabled: bool) {
        if bit > 6 || !self.supported_qmk_settings.contains(&3) {
            return;
        }
        let result = (|| -> anyhow::Result<u8> {
            let hid = self
                .hid_device
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Keyboard is busy or disconnected"))?;
            // Sample the whole byte before editing a single flag. Other flags,
            // including reserved bits, must survive this operation.
            let previous = hid.get_qmk_setting_u8(3)?;
            let mask = 1u8 << bit;
            let desired = if enabled {
                previous | mask
            } else {
                previous & !mask
            };
            if desired != previous {
                hid.set_qmk_setting_u8(3, desired)?;
                let actual = hid.get_qmk_setting_u8(3)?;
                anyhow::ensure!(actual == desired,
                    "Auto Shift flags readback mismatch: requested {desired:#04x}, received {actual:#04x}");
            }
            Ok(desired)
        })();
        match result {
            Ok(actual) => self.auto_shift_options = AutoShiftOptionsState::from_bits(actual),
            Err(e) => {
                // A failed SET or readback leaves device state uncertain. Do not
                // present an optimistic switch value or allow another blind write.
                self.auto_shift_options.loaded = false;
                self.auto_shift_timeout = None;
                self.status_msg = format!("Failed to save Auto Shift flags: {e}");
                log::warn!("Auto Shift flags transaction failed: {e}");
            }
        }
    }

    fn write_auto_shift_timeout(&mut self, timeout: u16) {
        if !self.supported_qmk_settings.contains(&4) {
            return;
        }
        let result = (|| -> anyhow::Result<()> {
            let hid = self
                .hid_device
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Keyboard is busy or disconnected"))?;
            hid.set_qmk_setting_u16(4, timeout)?;
            let actual = hid.get_qmk_setting_u16(4)?;
            anyhow::ensure!(
                actual == timeout,
                "Auto Shift timeout readback mismatch: requested {timeout}, received {actual}"
            );
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.auto_shift_timeout = Some(timeout);
                self.auto_shift_timeout_text = timeout.to_string();
            }
            Err(e) => {
                self.auto_shift_options.loaded = false;
                self.auto_shift_timeout = None;
                self.status_msg = format!("Failed to save Auto Shift timeout: {e}");
                log::warn!("Auto Shift timeout transaction failed: {e}");
            }
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    fn test_app() -> (EntropyApp, egui::Context, crate::hid::TestHidRecorder) {
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&cc);
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(hid);
        app.supported_qmk_settings = vec![3, 4];
        app.auto_shift_options.loaded = true;
        app.auto_shift_timeout = Some(175);
        app.auto_shift_timeout_text = "175".into();
        (app, ctx, recorder)
    }

    fn draw_page(app: &mut EntropyApp, ctx: &egui::Context) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000.0, 900.0),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let rect = ui.max_rect();
                    app.draw_auto_shift_settings_page(ui, rect, true);
                });
            },
        )
    }

    fn draw_enable_row(app: &mut EntropyApp, ctx: &egui::Context, events: Vec<egui::Event>) {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(640.0, 180.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let origin = ui.cursor().min;
                    let list = AdaptiveSettingsListViewport {
                        viewport: ui.max_rect(),
                        content_rect: ui.max_rect(),
                        track_rect: egui::Rect::NOTHING,
                        handle_height: 0.0,
                        scroll_ratio: 0.0,
                        track_hovered: false,
                        suppress_tooltips: true,
                        first_visible_row: 0,
                        last_visible_row: 1,
                        row_content_width: 520.0,
                        row_height: 54.0,
                        has_scrollbar: false,
                        scroll_offset: 0.0,
                        content_height: 54.0,
                    };
                    app.draw_auto_shift_row(ui, 0, &list, 86.0, true);
                    ctx.data_mut(|data| {
                        data.insert_temp(
                            egui::Id::new("auto_shift_test_switch_center"),
                            egui::pos2(origin.x + 520.0 - 23.0, origin.y + 27.0),
                        );
                    });
                });
            },
        );
    }

    fn enter_timeout_text(app: &mut EntropyApp, ctx: &egui::Context, value: &str) {
        let visible = app.auto_shift_timeout_text.clone();
        let output = draw_page(app, ctx);
        let pos = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == visible => {
                    Some(text.pos + text.galley.size() / 2.0)
                }
                _ => None,
            })
            .expect("Auto Shift timeout field text is visible");
        let events = vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            },
        ];
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000.0, 900.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let rect = ui.max_rect();
                    app.draw_auto_shift_settings_page(ui, rect, true);
                });
            },
        );
        ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("auto_shift_timeout")));
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000.0, 900.0),
                )),
                events: vec![
                    egui::Event::Key {
                        key: egui::Key::A,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers {
                            ctrl: true,
                            command: true,
                            ..Default::default()
                        },
                    },
                    egui::Event::Text(value.into()),
                ],
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let rect = ui.max_rect();
                    app.draw_auto_shift_settings_page(ui, rect, true);
                });
            },
        );
    }

    #[test]
    fn auto_shift_enable_switch_changes_model_and_sends_only_flags_qsid() {
        let (mut app, ctx, recorder) = test_app();
        draw_enable_row(&mut app, &ctx, vec![]);
        let center = ctx.data(|data| {
            data.get_temp::<egui::Pos2>(egui::Id::new("auto_shift_test_switch_center"))
                .unwrap()
        });
        draw_enable_row(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(center),
                egui::Event::PointerButton {
                    pos: center,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
        );
        draw_enable_row(
            &mut app,
            &ctx,
            vec![egui::Event::PointerButton {
                pos: center,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        );

        assert!(app.auto_shift_options.enabled);
        let writes: Vec<_> = recorder
            .requests()
            .into_iter()
            .filter(|request| request[0] == 0xFE && request[1] == 0x0B)
            .collect();
        assert_eq!(writes.len(), 1);
        assert_eq!(u16::from_le_bytes([writes[0][2], writes[0][3]]), 3);
        assert_eq!(writes[0][4], 1);

        // Exercise the same switch after enabling: an off click must send bit0=0.
        for pressed in [true, false] {
            draw_enable_row(
                &mut app,
                &ctx,
                vec![
                    egui::Event::PointerMoved(center),
                    egui::Event::PointerButton {
                        pos: center,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
        assert!(!app.auto_shift_options.enabled);
        assert!(app.auto_shift_options.loaded);
        let writes: Vec<_> = recorder
            .requests()
            .into_iter()
            .filter(|r| r[0] == 0xFE && r[1] == 0x0B)
            .collect();
        assert_eq!(writes.len(), 2);
        assert_eq!(writes[1][4], 0);
        assert_eq!(u16::from_le_bytes([writes[1][2], writes[1][3]]), 3);
    }

    #[test]
    fn auto_shift_disable_preserves_current_device_flags_including_reserved_bit() {
        let (mut app, _, recorder) = test_app();
        app.hid_device
            .as_ref()
            .unwrap()
            .set_qmk_setting_u8(3, 0xef)
            .unwrap();
        // The model is intentionally stale; the transaction samples device flags.
        app.auto_shift_options = AutoShiftOptionsState::from_bits(1);
        app.write_auto_shift_flag(0, false);
        assert!(!app.auto_shift_options.enabled);
        assert_eq!(app.auto_shift_options.bits(), 0x6e);
        assert!(app.auto_shift_options.loaded);
        let writes: Vec<_> = recorder
            .requests()
            .into_iter()
            .filter(|r| r[0] == 0xFE && r[1] == 0x0B)
            .collect();
        assert_eq!(writes.last().unwrap()[4], 0xee);
    }

    #[test]
    fn auto_shift_failed_set_or_readback_stops_without_retry_or_success_state() {
        for fault in [0, 1, 2] {
            let (mut app, _, recorder) = test_app();
            app.auto_shift_options = AutoShiftOptionsState::from_bits(1);
            let mut reads = 0;
            recorder.respond_by(move |request| {
                if request[0] != 0xfe {
                    return None;
                }
                match request[1] {
                    0x0a => {
                        reads += 1;
                        if reads == 2 && fault == 2 {
                            return Some(Err("readback timed out".into()));
                        }
                        let mut reply = [0; 32];
                        reply[1] = 1; // Firmware still reports enabled.
                        Some(Ok(reply))
                    }
                    0x0b if fault == 0 => Some(Err("save failed".into())),
                    0x0b => Some(Ok([0; 32])),
                    _ => None,
                }
            });
            app.write_auto_shift_flag(0, false);
            assert!(
                app.auto_shift_options.enabled,
                "never show unconfirmed off state"
            );
            assert!(!app.auto_shift_options.loaded);
            assert!(app.auto_shift_timeout.is_none());
            assert!(app.status_msg.contains("Failed to save Auto Shift flags"));
            let requests = recorder.requests();
            assert_eq!(requests.iter().filter(|r| r[1] == 0x0b).count(), 1);
            assert_eq!(
                requests.iter().filter(|r| r[1] == 0x0a).count(),
                if fault == 0 { 1 } else { 2 }
            );
        }
    }

    #[test]
    fn auto_shift_missing_owner_does_not_change_confirmed_switch() {
        let (mut app, _, recorder) = test_app();
        app.auto_shift_options = AutoShiftOptionsState::from_bits(1);
        app.hid_device = None;
        app.write_auto_shift_flag(0, false);
        assert!(app.auto_shift_options.enabled);
        assert!(!app.auto_shift_options.loaded);
        assert!(app.auto_shift_timeout.is_none());
        assert!(recorder.requests().is_empty());
    }

    #[test]
    fn beta3_main_and_typing_show_only_editable_timeout_without_qsid3_traffic() {
        use crate::app::ak820_beta1_profiles::ProfileId;
        for profile in [ProfileId::Main, ProfileId::Typing] {
            let (mut app, ctx, recorder) = test_app();
            let image = crate::ak820::ImageContract::Beta3(profile);
            app.supported_qmk_settings = image.qsid_ids().to_vec();
            assert!(app.supported_qmk_settings.contains(&4));
            assert!(!app.supported_qmk_settings.contains(&3));
            app.auto_shift_options = AutoShiftOptionsState::default();
            for language in [
                crate::i18n::Language::English,
                crate::i18n::Language::Russian,
            ] {
                app.app_settings.language = language;
                let output = draw_page(&mut app, &ctx);
                let text: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) => Some(text.galley.job.text.as_str()),
                        _ => None,
                    })
                    .collect();
                assert!(text.iter().any(|text| text.contains("AS_OFF")));
                let enable = crate::i18n::tr_catalog(language, "common.enable");
                assert!(
                    !text.contains(&enable),
                    "unsupported enable switch must be absent"
                );
            }
            enter_timeout_text(&mut app, &ctx, "225");
            assert_eq!(app.auto_shift_timeout, Some(225));
            app.write_auto_shift_flag(0, false); // Defensive writer gate too.
            let requests = recorder.requests();
            assert!(requests.iter().any(|r| r[1] == 0x0b && r[2] == 4));
            assert!(!requests
                .iter()
                .any(|r| matches!(r[1], 0x0a | 0x0b) && r[2..4] == [3, 0]));
        }
    }

    #[test]
    fn auto_shift_render_is_read_only_and_missing_timeout_fails_closed() {
        let (mut app, ctx, recorder) = test_app();
        draw_page(&mut app, &ctx);
        draw_page(&mut app, &ctx);
        assert!(
            recorder.requests().is_empty(),
            "painting controls must not write"
        );

        app.auto_shift_timeout = None;
        app.auto_shift_timeout_text.clear();
        let output = draw_page(&mut app, &ctx);
        let unavailable = crate::i18n::tr(
            app.app_settings.language,
            crate::i18n::Key::AutoShiftUnavailable,
        );
        assert!(output.shapes.iter().any(|shape| matches!(
            &shape.shape,
            egui::Shape::Text(text) if text.galley.job.text == unavailable
        )));
        assert!(recorder.requests().is_empty());
    }

    #[test]
    fn auto_shift_timeout_edit_clamps_valid_input_and_rejects_non_numeric_input() {
        let (mut app, ctx, recorder) = test_app();
        // The timeout editor is intentionally disabled while Auto Shift itself is off.
        app.auto_shift_options.enabled = true;
        enter_timeout_text(&mut app, &ctx, "2500");
        assert_eq!(app.auto_shift_timeout, Some(1000));
        let writes: Vec<_> = recorder
            .requests()
            .into_iter()
            .filter(|request| request[0] == 0xFE && request[1] == 0x0B)
            .collect();
        assert_eq!(writes.len(), 1);
        assert_eq!(u16::from_le_bytes([writes[0][2], writes[0][3]]), 4);
        assert_eq!(u16::from_le_bytes([writes[0][4], writes[0][5]]), 1000);

        // A malformed edit is filtered to an empty field, which is not a value.
        let before = recorder.requests().len();
        enter_timeout_text(&mut app, &ctx, "x");
        assert_eq!(app.auto_shift_timeout, Some(1000));
        assert_eq!(
            recorder.requests().len(),
            before,
            "invalid input must not write"
        );
    }
}
