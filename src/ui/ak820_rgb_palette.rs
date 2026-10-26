use super::*;
use crate::ak820_rgb::{Palette, Rgb, LED_COUNT, SLOT_MAP};
use std::io::Read;

#[derive(Clone)]
struct Editor {
    palette: Palette,
    selected: Vec<usize>,
    anchor: Option<usize>,
    draft: Color32,
}
impl Default for Editor {
    fn default() -> Self {
        Self {
            palette: Palette::default(),
            selected: vec![0],
            anchor: Some(0),
            draft: Color32::WHITE,
        }
    }
}

fn slot_for_matrix(row: u8, col: u8) -> Option<usize> {
    if row >= 16 || col >= 8 {
        return None;
    }
    let slot = row.checked_mul(8)?.checked_add(col)?;
    SLOT_MAP.iter().position(|&mapped| mapped == slot)
}

fn base_keycodes() -> Vec<u16> {
    let doc: serde_json::Value =
        serde_json::from_str(include_str!("../../assets/ak820/demo-keymap.json"))
            .expect("bundled AK820 base keymap");
    doc["base_layer"]
        .as_array()
        .expect("base_layer array")
        .iter()
        .map(|v| v.as_u64().unwrap_or(0) as u16)
        .collect()
}

fn base_key_groups() -> [Vec<(u8, u8)>; 6] {
    // Immutable v0.6.0-beta.2 default base layer. These groups follow the
    // shipped physical keymap, never the user's currently remapped layout.
    let doc: serde_json::Value =
        serde_json::from_str(include_str!("../../assets/ak820/demo-keymap.json"))
            .expect("bundled AK820 base keymap");
    let keys = doc["base_layer"].as_array().expect("base_layer array");
    let mut groups: [Vec<(u8, u8)>; 6] = std::array::from_fn(|_| Vec::new());
    for (matrix, value) in keys.iter().enumerate() {
        let kc = value.as_u64().unwrap_or(0) as u16;
        let row = (matrix / 8) as u8;
        let col = (matrix % 8) as u8;
        let group = if [26, 22, 4, 7].contains(&kc) {
            Some(0)
        }
        // WASD
        else if [0x52, 0x51, 0x50, 0x4f].contains(&kc) {
            Some(1)
        }
        // arrows
        else if (0x3a..=0x45).contains(&kc) {
            Some(2)
        }
        // F1-F12
        else if (0x1e..=0x27).contains(&kc) {
            Some(3)
        }
        // number row
        else if [0xe0, 0xe1, 0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7].contains(&kc) {
            Some(4)
        }
        // modifiers
        else if [0x4a, 0x4b, 0x4c, 0x4d, 0x4e, 0x4f, 0x50, 0x51, 0x52, 0x49].contains(&kc) {
            Some(5)
        } else {
            None
        };
        if let Some(g) = group {
            groups[g].push((row, col));
        }
    }
    groups
}

impl EntropyApp {
    /// Render the real AK820 physical key geometry with colors indexed only by
    /// the pinned matrix-to-LED map. This edits a local preview, not readback.
    pub(crate) fn draw_ak820_rgb_palette(&mut self, ui: &mut egui::Ui) {
        let profile = self
            .ak820_demo_profile
            .map(|p| p.as_str())
            .unwrap_or("live");
        let id = egui::Id::new(("ak820-rgb-editor", profile));
        let mut e = ui
            .ctx()
            .data(|d| d.get_temp::<Editor>(id))
            .unwrap_or_default();
        let ru = self.app_settings.language == crate::i18n::Language::Russian;
        ui.heading(if ru {
            "AK820: RGB для каждой клавиши"
        } else {
            "AK820 per-key RGB"
        });
        ui.label(if ru { "Локальный предпросмотр. Режим Direct хранится только в ОЗУ клавиатуры; прочитать цвета обратно нельзя." } else { "Local preview only. Direct RGB is RAM-only and cannot be read back from the keyboard." });
        let groups = base_key_groups();
        ui.horizontal_wrapped(|ui| {
            for (label, g) in [
                (if ru { "WASD" } else { "WASD" }, 0),
                (if ru { "Стрелки" } else { "Arrows" }, 1),
                (if ru { "F-клавиши" } else { "Function" }, 2),
                (if ru { "Цифры" } else { "Numbers" }, 3),
                (
                    if ru {
                        "Модификаторы"
                    } else {
                        "Modifiers"
                    },
                    4,
                ),
                (
                    if ru {
                        "Навигация"
                    } else {
                        "Navigation"
                    },
                    5,
                ),
            ] {
                if ui.button(label).clicked() {
                    e.selected = groups[g]
                        .iter()
                        .filter_map(|&(r, c)| slot_for_matrix(r, c))
                        .collect();
                    e.anchor = e.selected.first().copied();
                    if let Some(&first) = e.selected.first() {
                        let c = e.palette.colors[first].0;
                        e.draft = Color32::from_rgb(c[0], c[1], c[2]);
                    }
                }
            }
        });
        let Some(layout) = self.layout.as_ref() else {
            ui.label("AK820 layout is unavailable.");
            return;
        };
        ui.horizontal(|ui| {
            if ui.button(if ru { "Выбрать всё" } else { "All" }).clicked() {
                e.selected = (0..LED_COUNT).collect();
            }
            if ui.button(if ru { "Снять выбор" } else { "None" }).clicked() {
                e.selected.clear();
            }
            ui.label(if ru {
                "Щелчок: выбор · Ctrl/Cmd+щелчок: переключить · Shift+щелчок: диапазон слотов"
            } else {
                "Click: select · Ctrl/Cmd+click: toggle · Shift+click: slot range"
            });
        });
        let keycodes = base_keycodes();
        let (response, painter) =
            ui.allocate_painter(Vec2::new(ui.available_width(), 245.0), Sense::hover());
        let bounds = response.rect;
        let min_x = layout
            .keys
            .iter()
            .map(|k| k.x)
            .fold(f32::INFINITY, f32::min);
        let max_x = layout
            .keys
            .iter()
            .map(|k| k.x + k.w)
            .fold(f32::NEG_INFINITY, f32::max);
        let min_y = layout
            .keys
            .iter()
            .map(|k| k.y)
            .fold(f32::INFINITY, f32::min);
        let max_y = layout
            .keys
            .iter()
            .map(|k| k.y + k.h)
            .fold(f32::NEG_INFINITY, f32::max);
        let unit = (bounds.width() / (max_x - min_x).max(1.0))
            .min(bounds.height() / (max_y - min_y).max(1.0))
            .min(42.0);
        let origin = egui::pos2(
            bounds.center().x - (max_x - min_x) * unit / 2.0,
            bounds.center().y - (max_y - min_y) * unit / 2.0,
        );
        let mut clicked = None;
        for key in &layout.keys {
            let center = egui::pos2(
                origin.x + (key.x - min_x + key.w / 2.0) * unit,
                origin.y + (key.y - min_y + key.h / 2.0) * unit,
            );
            let rect = egui::Rect::from_center_size(
                center,
                Vec2::new((key.w * unit - 3.0).max(7.0), (key.h * unit - 3.0).max(7.0)),
            );
            let slot = slot_for_matrix(key.row, key.col);
            let enabled = slot.is_some(); // encoder press (row 13, col 0) deliberately has no LED
            let fill = slot
                .map(|i| {
                    let c = e.palette.colors[i]
                        .0
                        .map(|v| (u16::from(v) * u16::from(e.palette.brightness) / 255) as u8);
                    Color32::from_rgb(c[0], c[1], c[2])
                })
                .unwrap_or(Color32::from_gray(45));
            let resp = ui.interact(
                rect,
                id.with((key.row, key.col)),
                if enabled {
                    Sense::click()
                } else {
                    Sense::hover()
                },
            );
            painter.rect_filled(rect, 3.0, fill);
            painter.rect_stroke(
                rect,
                3.0,
                Stroke::new(
                    if slot.is_some_and(|i| e.selected.contains(&i)) {
                        2.0
                    } else {
                        1.0
                    },
                    if slot.is_some_and(|i| e.selected.contains(&i)) {
                        Color32::YELLOW
                    } else {
                        Color32::from_gray(130)
                    },
                ),
                egui::StrokeKind::Inside,
            );
            if enabled {
                let matrix = usize::from(key.row) * 8 + usize::from(key.col);
                let kc = keycodes.get(matrix).copied().unwrap_or(0);
                let label = crate::keycode::keycode_label_with_names_and_layout(
                    kc,
                    &[],
                    &[],
                    self.app_settings.key_legend_layout,
                );
                let lum = (u32::from(fill.r()) * 299
                    + u32::from(fill.g()) * 587
                    + u32::from(fill.b()) * 114)
                    / 1000;
                let fg = if lum > 135 {
                    Color32::BLACK
                } else {
                    Color32::WHITE
                };
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    label,
                    egui::FontId::proportional(9.0),
                    fg,
                );
            }
            if resp.clicked() {
                clicked = slot;
            }
        }
        if let Some(index) = clicked {
            let mods = ui.input(|i| i.modifiers);
            if mods.shift {
                if let Some(anchor) = e.anchor {
                    let (lo, hi) = (anchor.min(index), anchor.max(index));
                    e.selected = (lo..=hi).collect();
                } else {
                    e.selected = vec![index];
                    e.anchor = Some(index);
                }
            } else if mods.ctrl || mods.command {
                if e.selected.contains(&index) {
                    e.selected.retain(|i| *i != index)
                } else {
                    e.selected.push(index)
                }
                e.anchor = Some(index);
            } else {
                e.selected = vec![index];
                e.anchor = Some(index);
            }
            let c = e.palette.colors[index].0;
            e.draft = Color32::from_rgb(c[0], c[1], c[2]);
        }
        ui.horizontal(|ui| {
            ui.label(if ru {
                format!("Выбрано клавиш: {}", e.selected.len())
            } else {
                format!("{} key(s) selected", e.selected.len())
            });
            ui.color_edit_button_srgba(&mut e.draft);
            if ui
                .button(if ru {
                    "Применить цвет"
                } else {
                    "Apply color"
                })
                .clicked()
            {
                for &i in &e.selected {
                    e.palette.colors[i] = Rgb([e.draft.r(), e.draft.g(), e.draft.b()]);
                }
            }
            if ui
                .button(if ru {
                    "Сбросить выбор"
                } else {
                    "Reset selection"
                })
                .clicked()
            {
                for &i in &e.selected {
                    e.palette.colors[i] = Rgb([0, 0, 0]);
                }
            }
        });
        let mut brightness = f32::from(e.palette.brightness);
        if ui
            .add(egui::Slider::new(&mut brightness, 0.0..=160.0).text(if ru {
                "Яркость (предел 160)"
            } else {
                "Brightness (cap 160)"
            }))
            .changed()
        {
            e.palette.brightness = brightness as u8;
        }
        ui.horizontal(|ui| {
            if ui.button(if ru {"Сохранить пресет…"} else {"Save app preset…"}).clicked(){if let Ok(json)=e.palette.encode_preset(){if let Some(path)=rfd::FileDialog::new().set_file_name("ak820-rgb.json").save_file(){if let Err(err)=std::fs::write(path,json){self.status_msg=format!("Could not save preset: {err}");}else{self.status_msg=if ru{"Пресет сохранён"}else{"Preset saved"}.into();}}}}
            if ui.button(if ru {"Открыть пресет…"} else {"Open app preset…"}).clicked(){if let Some(path)=rfd::FileDialog::new().add_filter("AK820 RGB preset",&["json"]).pick_file(){match (||{let metadata=std::fs::metadata(&path).map_err(|_|"could not read preset")?;if metadata.len()>16*1024{return Err("palette preset exceeds 16 KiB");}let file=std::fs::File::open(path).map_err(|_|"could not read preset")?;let mut bytes=Vec::new();file.take(16*1024+1).read_to_end(&mut bytes).map_err(|_|"could not read preset")?;if bytes.len()>16*1024{return Err("palette preset exceeds 16 KiB");}let text=String::from_utf8(bytes).map_err(|_|"invalid UTF-8 preset")?;Palette::decode_preset(&text)})(){Ok(p)=>{e.palette=p;self.status_msg=if ru{"Пресет загружен"}else{"Preset loaded"}.into()},Err(err)=>self.status_msg=format!("RGB preset rejected: {err}")}}}
            let demo_rgb=self.ak820_demo && self.ak820_demo_profile.is_some_and(|p|crate::app::ak820_beta1_profiles::has_feature(p,"vialrgb-direct"));
            let can_apply=demo_rgb || (!self.ak820_demo && !self.hid_write_task_owner_active() && matches!(self.connect_state,ConnectState::Idle) && self.hid_device.as_ref().is_some_and(|h|h.ak820_editing_ready() && h.ak820_image().is_some_and(|i|i.profile().is_some() && i.rgb_modes().contains(&1))));
            let apply_label=if demo_rgb {if ru{"Обновить предпросмотр (демо)"}else{"Update demo preview"}} else if ru {"Применить к клавиатуре (ОЗУ)"} else {"Apply to keyboard (RAM only)"};
            if ui.add_enabled(can_apply,egui::Button::new(apply_label)).clicked(){if demo_rgb{self.status_msg=if ru{"Демо: предпросмотр обновлён; устройство не затронуто"}else{"Demo preview updated; no device was contacted"}.into()}else if let Some(hid)=self.hid_device.as_mut(){if let Ok(frame)=e.palette.rgb_frame(){match hid.ak820_apply_palette(&frame){Ok(())=>{self.rgb_settings.effect=1;self.rgb_settings.last_enabled_effect=1;self.status_msg=if ru {"Direct RGB отправлен; палитра действует до перезапуска клавиатуры"}else{"Direct RGB sent; palette lasts until the keyboard restarts"}.into()},Err(err)=>self.status_msg=format!("RGB write failed: {err}")}}}}
        });
        if self.ak820_demo {
            ui.label(if ru {
                "Демо-режим: действие меняет только предпросмотр."
            } else {
                "Demo mode: this action updates only the local preview."
            });
        }
        ui.ctx().data_mut(|d| d.insert_temp(id, e));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mapping_resolves_physical_rows_and_encoder_press_is_unlit() {
        assert_eq!(slot_for_matrix(0, 0), Some(0));
        assert_eq!(slot_for_matrix(13, 0), None);
        assert_eq!(slot_for_matrix(15, 7), None);
        assert_eq!(slot_for_matrix(16, 0), None);
        assert_eq!(slot_for_matrix(0, 8), None);
    }
}
