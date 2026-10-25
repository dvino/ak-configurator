use super::*;

impl EntropyApp {
    pub(super) fn draw_mouse_keys_settings_page(
        &mut self,
        ui: &mut egui::Ui,
        content_rect: egui::Rect,
    ) {
        let lang = self.app_settings.language;
        let dark = ui.visuals().dark_mode;
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());

        crate::ui_style::allocate_ui_at_rect(ui, content_rect, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(18.0);
                ui.label(
                    RichText::new(crate::i18n::tr(lang, crate::i18n::Key::MouseKeysTitle))
                        .size(18.0)
                        .strong(),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(crate::i18n::tr(
                        lang,
                        crate::i18n::Key::MouseKeysDescription,
                    ))
                    .size(13.0)
                    .color(app_muted_text(dark)),
                );
                ui.add_space(24.0);

                if !self.mouse_keys_settings.supported {
                    crate::ui_style::modal_empty_state(
                        ui,
                        crate::i18n::tr(lang, crate::i18n::Key::MouseKeysUnavailable),
                        Some(crate::i18n::tr(lang, crate::i18n::Key::MouseKeysEnableHint)),
                    );
                    return;
                }

                const TOTAL_MOUSE_KEY_ROWS: usize = 9;
                let list = allocate_adaptive_settings_list_viewport(
                    ui,
                    "mouse_keys_settings",
                    metrics,
                    TOTAL_MOUSE_KEY_ROWS,
                    0.0,
                );
                crate::ui_style::allocate_ui_at_rect(ui, list.content_rect, |ui| {
                    ui.set_clip_rect(list.viewport);
                    ui.set_min_size(list.content_rect.size());
                    ui.spacing_mut().item_spacing.y = 0.0;
                    self.draw_mouse_keys_editor_content(
                        ui,
                        list.first_visible_row..list.last_visible_row,
                        list.row_content_width,
                        list.row_height,
                        metrics.value(86.0),
                        list.suppress_tooltips,
                    );
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
            });
        });
    }

    fn write_mouse_keys_setting(&mut self, qsid: u16, value: u16) {
        let Some(hid) = &self.hid_device else {
            return;
        };
        let value = value.min(u8::MAX as u16) as u8;
        match hid.set_qmk_setting_u8(qsid, value) {
            Ok(()) => self.mouse_keys_settings.loaded_qsids.mark(qsid),
            Err(e) => {
                self.status_msg = format!("Failed to save Mouse keys setting (qsid {qsid}): {}", e);
                log::warn!("set_qmk_setting_u8(mouse_keys qsid {qsid}) failed: {e}");
            }
        }
    }

    fn draw_mouse_keys_editor_content(
        &mut self,
        ui: &mut egui::Ui,
        row_range: std::ops::Range<usize>,
        content_width: f32,
        row_height: f32,
        field_width: f32,
        suppress_tooltips: bool,
    ) {
        // Limits match Vial GUI qmk_settings.json.
        let lang = self.app_settings.language;
        let rows: [(u16, &'static str, &'static str, SettingsFieldUnit, u32); 9] = [
            (
                9,
                "mouse_keys_settings.delay_label",
                "mouse_keys_settings.delay_tooltip",
                SettingsFieldUnit::Milliseconds,
                10000,
            ),
            (
                10,
                "mouse_keys_settings.interval_label",
                "mouse_keys_settings.interval_tooltip",
                SettingsFieldUnit::Milliseconds,
                10000,
            ),
            (
                11,
                "mouse_keys_settings.move_delta_label",
                "mouse_keys_settings.move_delta_tooltip",
                SettingsFieldUnit::CursorSteps,
                1000,
            ),
            (
                12,
                "mouse_keys_settings.max_speed_label",
                "mouse_keys_settings.max_speed_tooltip",
                SettingsFieldUnit::SpeedSteps,
                1000,
            ),
            (
                13,
                "mouse_keys_settings.time_to_max_label",
                "mouse_keys_settings.time_to_max_tooltip",
                SettingsFieldUnit::Milliseconds,
                1000,
            ),
            (
                14,
                "mouse_keys_settings.wheel_delay_label",
                "mouse_keys_settings.wheel_delay_tooltip",
                SettingsFieldUnit::Milliseconds,
                10000,
            ),
            (
                15,
                "mouse_keys_settings.wheel_interval_label",
                "mouse_keys_settings.wheel_interval_tooltip",
                SettingsFieldUnit::Milliseconds,
                10000,
            ),
            (
                16,
                "mouse_keys_settings.wheel_max_speed_label",
                "mouse_keys_settings.wheel_max_speed_tooltip",
                SettingsFieldUnit::SpeedSteps,
                1000,
            ),
            (
                17,
                "mouse_keys_settings.wheel_time_to_max_label",
                "mouse_keys_settings.wheel_time_to_max_tooltip",
                SettingsFieldUnit::Milliseconds,
                1000,
            ),
        ];
        let control_height = (row_height / 54.0).clamp(1.0, 1.12) * 32.0;

        for row_idx in row_range {
            let Some((qsid, label, tooltip, unit, max)) = rows.get(row_idx).copied() else {
                continue;
            };
            let current = match qsid {
                9 => self.mouse_keys_settings.delay,
                10 => self.mouse_keys_settings.interval,
                11 => self.mouse_keys_settings.move_delta,
                12 => self.mouse_keys_settings.max_speed,
                13 => self.mouse_keys_settings.time_to_max,
                14 => self.mouse_keys_settings.wheel_delay,
                15 => self.mouse_keys_settings.wheel_interval,
                16 => self.mouse_keys_settings.wheel_max_speed,
                17 => self.mouse_keys_settings.wheel_time_to_max,
                _ => continue,
            };

            crate::ui_style::settings_list_row_with_tooltip(
                ui,
                content_width,
                row_height,
                crate::i18n::tr_catalog(lang, label),
                true,
                if suppress_tooltips {
                    None
                } else {
                    Some(crate::i18n::tr_catalog(lang, tooltip))
                },
                field_width,
                |ui| {
                    let edit_id = egui::Id::new(("mouse_keys_edit", qsid));
                    let mut text = ui.ctx().data_mut(|d| {
                        d.get_temp::<String>(edit_id)
                            .unwrap_or_else(|| current.to_string())
                    });
                    if text.parse::<u16>().ok() != Some(current)
                        && !ui.memory(|m| m.has_focus(edit_id))
                    {
                        text = current.to_string();
                    }

                    let resp = crate::ui_style::modern_text_field_sized(
                        ui,
                        edit_id,
                        &mut text,
                        field_width,
                        control_height,
                        "",
                        5,
                        egui::Align::RIGHT,
                    );
                    let resp = settings_field_unit_tooltip(resp, lang, suppress_tooltips, unit);

                    if resp.changed() {
                        let filtered: String =
                            text.chars().filter(|c: &char| c.is_ascii_digit()).collect();
                        let Ok(parsed) = filtered.parse::<u32>() else {
                            text = filtered;
                            ui.ctx().data_mut(|d| d.insert_temp(edit_id, text));
                            return;
                        };
                        let parsed = parsed.min(max);
                        let new_value = parsed as u16;
                        if new_value != current {
                            match qsid {
                                9 => self.mouse_keys_settings.delay = new_value,
                                10 => self.mouse_keys_settings.interval = new_value,
                                11 => self.mouse_keys_settings.move_delta = new_value,
                                12 => self.mouse_keys_settings.max_speed = new_value,
                                13 => self.mouse_keys_settings.time_to_max = new_value,
                                14 => self.mouse_keys_settings.wheel_delay = new_value,
                                15 => self.mouse_keys_settings.wheel_interval = new_value,
                                16 => self.mouse_keys_settings.wheel_max_speed = new_value,
                                17 => self.mouse_keys_settings.wheel_time_to_max = new_value,
                                _ => {}
                            }
                            self.write_mouse_keys_setting(qsid, new_value);
                        }
                        text = filtered;
                    }
                    ui.ctx().data_mut(|d| d.insert_temp(edit_id, text));
                },
            );
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
        (app, ctx, recorder)
    }

    fn draw_page(
        app: &mut EntropyApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run(
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
                    app.draw_mouse_keys_settings_page(ui, rect);
                });
            },
        )
    }

    fn replace_mouse_delay_text(app: &mut EntropyApp, ctx: &egui::Context, text: &str) {
        let output = draw_page(app, ctx, vec![]);
        let pos = output.shapes.iter().find_map(|shape| match &shape.shape {
            egui::Shape::Text(galley) if galley.galley.job.text == app.mouse_keys_settings.delay.to_string() => {
                Some(galley.pos + galley.galley.size() / 2.0)
            }
            _ => None,
        }).expect("Mouse Keys delay field text is visible");
        draw_page(app, ctx, vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() },
            egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() },
        ]);
        let edit_id = egui::Id::new(("mouse_keys_edit", 9u16));
        ctx.memory_mut(|memory| memory.request_focus(edit_id));
        draw_page(app, ctx, vec![
            egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers { ctrl: true, mac_cmd: true, command: true, ..Default::default() },
            },
            egui::Event::Text(text.into()),
        ]);
    }

    fn qmk_set_requests(recorder: &crate::hid::TestHidRecorder) -> Vec<[u8; 32]> {
        recorder
            .requests()
            .into_iter()
            .filter(|request| request[0] == 0xFE && request[1] == 0x0B)
            .collect()
    }

    #[test]
    fn unsupported_mouse_keys_page_and_unchanged_render_emit_no_writes() {
        let (mut app, ctx, recorder) = test_app();
        let unsupported = draw_page(&mut app, &ctx, vec![]);
        let unavailable = crate::i18n::tr(
            app.app_settings.language,
            crate::i18n::Key::MouseKeysUnavailable,
        );
        assert!(unsupported.shapes.iter().any(|shape| matches!(
            &shape.shape,
            egui::Shape::Text(text) if text.galley.job.text == unavailable
        )));
        assert!(recorder.requests().is_empty());

        app.mouse_keys_settings.supported = true;
        app.mouse_keys_settings.delay = 20;
        app.mouse_keys_settings.loaded_qsids.mark(9);
        draw_page(&mut app, &ctx, vec![]);
        draw_page(&mut app, &ctx, vec![]);
        assert!(recorder.requests().is_empty(), "rendering editable values must not write");
    }

    #[test]
    fn editing_mouse_key_delay_updates_model_and_writes_only_its_qsid() {
        let (mut app, ctx, recorder) = test_app();
        app.mouse_keys_settings.supported = true;
        app.mouse_keys_settings.delay = 20;
        app.mouse_keys_settings.loaded_qsids.mark(9);
        replace_mouse_delay_text(&mut app, &ctx, "123");

        assert_eq!(app.mouse_keys_settings.delay, 123);
        let writes = qmk_set_requests(&recorder);
        assert_eq!(writes.len(), 1);
        assert_eq!(u16::from_le_bytes([writes[0][2], writes[0][3]]), 9);
        assert_eq!(writes[0][4], 123);
        assert_eq!(app.mouse_keys_settings.loaded_qsids.contains(9), true);
    }

    #[test]
    fn malformed_mouse_key_delay_input_does_not_change_state_or_write() {
        let (mut app, ctx, recorder) = test_app();
        app.mouse_keys_settings.supported = true;
        app.mouse_keys_settings.delay = 20;
        app.mouse_keys_settings.loaded_qsids.mark(9);
        replace_mouse_delay_text(&mut app, &ctx, "x");

        assert_eq!(app.mouse_keys_settings.delay, 20);
        assert!(qmk_set_requests(&recorder).is_empty(), "invalid text must fail closed");
    }
}
