use super::*;
use crate::ak820_rgb::{Palette, Rgb, LED_COUNT, SLOT_MAP};
use crate::ak820_rgb_compositor::{Axis, Effect, Layer, QuietHours, Scene, MAX_SCENE_BYTES};
use chrono::Timelike;
use std::io::Read;

#[derive(Clone)]
struct Editor {
    scene: Scene,
    selected: Vec<usize>,
    anchor: Option<usize>,
    draft: Color32,
    active_layer: usize,
    undo: Vec<Scene>,
    redo: Vec<Scene>,
}
impl Default for Editor {
    fn default() -> Self {
        Self {
            scene: Scene::default(),
            selected: vec![0],
            anchor: Some(0),
            draft: Color32::WHITE,
            active_layer: 0,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }
}

impl Editor {
    fn normalize_active_layer(&mut self) {
        self.active_layer = self
            .active_layer
            .min(self.scene.layers.len().saturating_sub(1));
    }

    fn store_undo(&mut self, scene: Scene) {
        self.undo.push(scene);
        if self.undo.len() > 32 {
            self.undo.remove(0);
        }
    }

    fn push_undo(&mut self, scene: Scene) {
        self.store_undo(scene);
        self.redo.clear();
    }

    fn undo_scene(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo.push(self.scene.clone());
        self.scene = previous;
        self.normalize_active_layer();
        true
    }

    fn redo_scene(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.store_undo(self.scene.clone());
        self.scene = next;
        self.normalize_active_layer();
        true
    }

    fn replace_scene(&mut self, next: Scene) {
        self.push_undo(self.scene.clone());
        self.scene = next;
        self.normalize_active_layer();
    }

    fn move_active_layer(&mut self, direction: isize) -> bool {
        let len = self.scene.layers.len();
        if self.active_layer >= len {
            self.normalize_active_layer();
        }
        let Some(target) = self.active_layer.checked_add_signed(direction) else {
            return false;
        };
        if target >= len {
            return false;
        }
        self.scene.layers.swap(self.active_layer, target);
        self.active_layer = target;
        true
    }

    fn set_active_mask_from_selection(&mut self) {
        let Some(layer) = self.scene.layers.get_mut(self.active_layer) else {
            return;
        };
        layer.mask = normalized_mask(&self.selected);
    }
}

fn normalized_mask(indices: &[usize]) -> Vec<usize> {
    let mut mask: Vec<_> = indices.iter().copied().filter(|&i| i < LED_COUNT).collect();
    mask.sort_unstable();
    mask.dedup();
    mask
}

fn effect_label(effect: Effect, russian: bool) -> &'static str {
    match (effect, russian) {
        (Effect::Solid, true) => "Сплошной",
        (Effect::Gradient, true) => "Градиент",
        (Effect::Wave, true) => "Волна",
        (Effect::Breathing, true) => "Дыхание",
        (Effect::Rain, true) => "Дождь",
        (Effect::Solid, false) => "Solid",
        (Effect::Gradient, false) => "Gradient",
        (Effect::Wave, false) => "Wave",
        (Effect::Breathing, false) => "Breathing",
        (Effect::Rain, false) => "Rain",
    }
}

fn axis_label(axis: Axis, russian: bool) -> &'static str {
    match (axis, russian) {
        (Axis::Horizontal, true) => "По горизонтали",
        (Axis::Vertical, true) => "По вертикали",
        (Axis::Horizontal, false) => "Horizontal",
        (Axis::Vertical, false) => "Vertical",
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
        self.draw_ak820_rgb_inputs(ui, &e.scene);
        let mut changed = false;
        let mut history_move = false;
        let before = e.scene.clone();
        let groups = base_key_groups();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !e.undo.is_empty(),
                    egui::Button::new(if ru { "Отменить" } else { "Undo" }),
                )
                .clicked()
            {
                if e.undo_scene() {
                    changed = true;
                    history_move = true;
                }
            }
            if ui
                .add_enabled(
                    !e.redo.is_empty(),
                    egui::Button::new(if ru { "Повторить" } else { "Redo" }),
                )
                .clicked()
            {
                if e.redo_scene() {
                    changed = true;
                    history_move = true;
                }
            }
            if ui
                .add_enabled(
                    e.scene.layers.len() < 16,
                    egui::Button::new(if ru {
                        "Добавить слой"
                    } else {
                        "Add layer"
                    }),
                )
                .clicked()
            {
                let name = if ru {
                    format!("Слой {}", e.scene.layers.len() + 1)
                } else {
                    format!("Layer {}", e.scene.layers.len() + 1)
                };
                e.scene.layers.push(Layer {
                    name,
                    mask: (0..LED_COUNT).collect(),
                    ..Layer::default()
                });
                e.active_layer = e.scene.layers.len() - 1;
                changed = true;
            }
        });
        let mut remove_layer = None;
        let layer_count = e.scene.layers.len();
        for i in 0..layer_count {
            let layer = &mut e.scene.layers[i];
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(e.active_layer == i, format!("{}: {}", i + 1, layer.name))
                    .clicked()
                {
                    e.active_layer = i;
                }
                if ui
                    .checkbox(&mut layer.enabled, if ru { "Вкл." } else { "On" })
                    .changed()
                {
                    changed = true;
                }
                if ui.small_button("×").clicked() {
                    remove_layer = Some(i);
                }
            });
        }
        if let Some(i) = remove_layer {
            e.scene.layers.remove(i);
            e.active_layer = e.active_layer.min(e.scene.layers.len().saturating_sub(1));
            changed = true;
        }
        if let Some(layer) = e.scene.layers.get_mut(e.active_layer) {
            ui.horizontal_wrapped(|ui| {
                ui.label(if ru { "Имя" } else { "Name" });
                changed |= ui
                    .add_sized(
                        Vec2::new(180.0, ui.spacing().interact_size.y),
                        egui::TextEdit::singleline(&mut layer.name),
                    )
                    .changed();
            });
            ui.horizontal_wrapped(|ui| {
                egui::ComboBox::from_label(if ru { "Эффект" } else { "Effect" })
                    .selected_text(effect_label(layer.effect, ru))
                    .show_ui(ui, |ui| {
                        for effect in [
                            Effect::Solid,
                            Effect::Gradient,
                            Effect::Wave,
                            Effect::Breathing,
                            Effect::Rain,
                        ] {
                            if ui
                                .selectable_value(
                                    &mut layer.effect,
                                    effect,
                                    effect_label(effect, ru),
                                )
                                .changed()
                            {
                                changed = true;
                            }
                        }
                    });
            });
            ui.horizontal_wrapped(|ui| {
                egui::ComboBox::from_label(if ru { "Ось" } else { "Axis" })
                    .selected_text(axis_label(layer.axis, ru))
                    .show_ui(ui, |ui| {
                        for axis in [Axis::Horizontal, Axis::Vertical] {
                            if ui
                                .selectable_value(&mut layer.axis, axis, axis_label(axis, ru))
                                .changed()
                            {
                                changed = true;
                            }
                        }
                    });
            });
            ui.horizontal_wrapped(|ui| {
                let mut opacity = layer.opacity as f32;
                if ui
                    .add_sized(
                        Vec2::new(260.0, ui.spacing().interact_size.y),
                        egui::Slider::new(&mut opacity, 0.0..=255.0).text(if ru {
                            "Непрозрачность"
                        } else {
                            "Opacity"
                        }),
                    )
                    .changed()
                {
                    layer.opacity = opacity as u8;
                    changed = true;
                }
                let mut speed = layer.period_ms as f32;
                if ui
                    .add_sized(
                        Vec2::new(260.0, ui.spacing().interact_size.y),
                        egui::Slider::new(&mut speed, 250.0..=60000.0).text(if ru {
                            "Период, мс"
                        } else {
                            "Period ms"
                        }),
                    )
                    .changed()
                {
                    layer.period_ms = speed as u32;
                    changed = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label(if ru {
                    "Цвета слоя"
                } else {
                    "Layer colors"
                });
                let mut c1 =
                    Color32::from_rgb(layer.color.0[0], layer.color.0[1], layer.color.0[2]);
                if ui.color_edit_button_srgba(&mut c1).changed() {
                    layer.color = Rgb([c1.r(), c1.g(), c1.b()]);
                    changed = true;
                }
                let mut c2 =
                    Color32::from_rgb(layer.color2.0[0], layer.color2.0[1], layer.color2.0[2]);
                if ui.color_edit_button_srgba(&mut c2).changed() {
                    layer.color2 = Rgb([c2.r(), c2.g(), c2.b()]);
                    changed = true;
                }
                ui.label(format!(
                    "{} {}",
                    if ru { "Маска" } else { "Mask" },
                    layer.mask.len()
                ));
                if ui
                    .button(if ru {
                        "Маска из выбора"
                    } else {
                        "Mask from selection"
                    })
                    .clicked()
                {
                    layer.mask = normalized_mask(&e.selected);
                    changed = true;
                }
                if ui
                    .button(if ru {
                        "Вся клавиатура"
                    } else {
                        "All LEDs"
                    })
                    .clicked()
                {
                    layer.mask = (0..LED_COUNT).collect();
                    changed = true;
                }
            });
        }
        ui.collapsing(
            if ru {
                "Тихие часы"
            } else {
                "Quiet hours"
            },
            |ui| {
                let existing = e.scene.quiet_hours.clone();
                let mut q = existing.clone().unwrap_or(QuietHours {
                    start_minute: 22 * 60,
                    end_minute: 7 * 60,
                    brightness: 30,
                });
                let mut enabled = existing.is_some();
                if ui
                    .checkbox(&mut enabled, if ru { "Включить" } else { "Enable" })
                    .changed()
                {
                    changed = true;
                }
                if enabled {
                    let mut start_hour = u32::from(q.start_minute / 60);
                    let mut start_minute = u32::from(q.start_minute % 60);
                    let mut end_hour = u32::from(q.end_minute / 60);
                    let mut end_minute = u32::from(q.end_minute % 60);
                    let mut b = q.brightness as f32;
                    ui.horizontal(|ui| {
                        ui.label(if ru { "Начало" } else { "Start" });
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut start_hour)
                                    .range(0..=23)
                                    .prefix(if ru { "ч " } else { "h " }),
                            )
                            .changed();
                        ui.label(":");
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut start_minute)
                                    .range(0..=59)
                                    .suffix(if ru { " мин" } else { " min" }),
                            )
                            .changed();
                        ui.label(format!("{start_hour:02}:{start_minute:02}"));
                        ui.label(if ru { "Конец" } else { "End" });
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut end_hour)
                                    .range(0..=23)
                                    .prefix(if ru { "ч " } else { "h " }),
                            )
                            .changed();
                        ui.label(":");
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut end_minute)
                                    .range(0..=59)
                                    .suffix(if ru { " мин" } else { " min" }),
                            )
                            .changed();
                        ui.label(format!("{end_hour:02}:{end_minute:02}"));
                    });
                    q.start_minute = (start_hour * 60 + start_minute) as u16;
                    q.end_minute = (end_hour * 60 + end_minute) as u16;
                    if ui
                        .add(egui::Slider::new(&mut b, 0.0..=160.0).text(if ru {
                            "Яркость"
                        } else {
                            "Brightness"
                        }))
                        .changed()
                    {
                        q.brightness = b as u8;
                        changed = true;
                    }
                }
                e.scene.quiet_hours = if enabled { Some(q) } else { None };
            },
        );
        ui.horizontal(|ui| {
            if ui
                .button(if ru {
                    "Поднять выбранный слой"
                } else {
                    "Move selected layer up"
                })
                .clicked()
                && e.active_layer < e.scene.layers.len()
                && e.active_layer > 0
            {
                changed |= e.move_active_layer(-1);
            }
            if ui
                .button(if ru {
                    "Опустить выбранный слой"
                } else {
                    "Move selected layer down"
                })
                .clicked()
                && e.active_layer < e.scene.layers.len()
                && e.active_layer + 1 < e.scene.layers.len()
            {
                changed |= e.move_active_layer(1);
            }
        });
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
                        let c = e.scene.base.colors[first].0;
                        e.draft = Color32::from_rgb(c[0], c[1], c[2]);
                    }
                }
            }
        });
        let Some(layout) = self.layout.as_ref() else {
            ui.label(if ru {
                "Раскладка AK820 недоступна."
            } else {
                "AK820 layout is unavailable."
            });
            return;
        };
        ui.horizontal_wrapped(|ui| {
            if ui.button(if ru { "Выбрать всё" } else { "All" }).clicked() {
                e.selected = (0..LED_COUNT).collect();
            }
            if ui.button(if ru { "Снять выбор" } else { "None" }).clicked() {
                e.selected.clear();
            }
        });
        ui.add(
            egui::Label::new(if ru {
                "Щелчок: выбор · Ctrl/Cmd+щелчок: переключить · Shift+щелчок: диапазон слотов"
            } else {
                "Click: select · Ctrl/Cmd+click: toggle · Shift+click: slot range"
            })
            .wrap(),
        );
        let keycodes = base_keycodes();
        let local_now = chrono::Local::now();
        let local_minute = (local_now.hour() * 60 + local_now.minute()) as u16;
        let preview = if self.ak820_rgb_scene_running() {
            self.ak820_rgb_scene_preview()
        } else {
            None
        }
        .or_else(|| e.scene.render(0, local_minute).ok());
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
                .map(|i| preview.map(|f| f[i]).unwrap_or([0, 0, 0]))
                .map(|c| Color32::from_rgb(c[0], c[1], c[2]))
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
            let c = e.scene.base.colors[index].0;
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
                    e.scene.base.colors[i] = Rgb([e.draft.r(), e.draft.g(), e.draft.b()]);
                    changed = true;
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
                    e.scene.base.colors[i] = Rgb([0, 0, 0]);
                    changed = true;
                }
            }
        });
        let mut brightness = f32::from(e.scene.base.brightness);
        if ui
            .add(egui::Slider::new(&mut brightness, 0.0..=160.0).text(if ru {
                "Яркость (предел 160)"
            } else {
                "Brightness (cap 160)"
            }))
            .changed()
        {
            e.scene.base.brightness = brightness as u8;
            changed = true;
        }
        let playback_running = self.ak820_rgb_scene_running();
        let scene_validation = e.scene.validate();
        ui.horizontal(|ui| {
            if ui
                .button(if ru {
                    "Сохранить палитру основы…"
                } else {
                    "Save base colors…"
                })
                .clicked()
            {
                self.ak820_rgb_scene_stop();
                match e.scene.base.encode_preset() {
                    Ok(json) => {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_file_name("ak820-rgb.json")
                            .save_file()
                        {
                            if std::fs::write(path, json).is_err() {
                                self.status_msg = if ru {
                                    "Не удалось сохранить палитру"
                                } else {
                                    "Could not save base palette"
                                }
                                .into();
                            } else {
                                self.status_msg = if ru {
                                    "Палитра основы сохранена"
                                } else {
                                    "Base palette saved"
                                }
                                .into();
                            }
                        }
                    }
                    Err(_) => {
                        self.status_msg = if ru {
                            "Палитра основы некорректна"
                        } else {
                            "Base palette is invalid"
                        }
                        .into()
                    }
                }
            }
            if ui
                .add_enabled(
                    !playback_running,
                    egui::Button::new(if ru {
                        "Открыть палитру основы…"
                    } else {
                        "Open base palette…"
                    }),
                )
                .clicked()
            {
                self.ak820_rgb_scene_stop();
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("AK820 RGB preset", &["json"])
                    .pick_file()
                {
                    let result = (|| {
                        let metadata =
                            std::fs::metadata(&path).map_err(|_| "could not read preset")?;
                        if metadata.len() > 16 * 1024 {
                            return Err("palette preset exceeds 16 KiB");
                        }
                        let file =
                            std::fs::File::open(path).map_err(|_| "could not read preset")?;
                        let mut bytes = Vec::new();
                        file.take(16 * 1024 + 1)
                            .read_to_end(&mut bytes)
                            .map_err(|_| "could not read preset")?;
                        if bytes.len() > 16 * 1024 {
                            return Err("palette preset exceeds 16 KiB");
                        }
                        let text = String::from_utf8(bytes).map_err(|_| "invalid UTF-8 preset")?;
                        Palette::decode_preset(&text)
                    })();
                    match result {
                        Ok(palette) => {
                            e.scene.base = palette;
                            changed = true;
                            self.status_msg = if ru {
                                "Палитра основы загружена"
                            } else {
                                "Base palette loaded"
                            }
                            .into();
                        }
                        Err(_) => {
                            self.status_msg = if ru {
                                "Палитра RGB отклонена"
                            } else {
                                "RGB palette rejected"
                            }
                            .into()
                        }
                    }
                }
            }
            if ui
                .button(if ru {
                    "Сохранить сцену…"
                } else {
                    "Save scene…"
                })
                .clicked()
            {
                self.ak820_rgb_scene_stop();
                match e.scene.encode_preset() {
                    Ok(json) => {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_file_name("ak820-rgb-scene.json")
                            .save_file()
                        {
                            if std::fs::write(path, json).is_err() {
                                self.status_msg = if ru {
                                    "Не удалось сохранить сцену"
                                } else {
                                    "Could not save scene"
                                }
                                .into();
                            }
                        }
                    }
                    Err(_) => {
                        self.status_msg = if ru {
                            "Сцена некорректна"
                        } else {
                            "Scene is invalid"
                        }
                        .into()
                    }
                }
            }
            if ui
                .add_enabled(
                    !playback_running,
                    egui::Button::new(if ru {
                        "Открыть сцену…"
                    } else {
                        "Open scene…"
                    }),
                )
                .clicked()
            {
                self.ak820_rgb_scene_stop();
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("AK820 RGB scene", &["json"])
                    .pick_file()
                {
                    let result = (|| {
                        let metadata =
                            std::fs::metadata(&path).map_err(|_| "could not read scene")?;
                        if metadata.len() > MAX_SCENE_BYTES as u64 {
                            return Err("scene file exceeds size limit");
                        }
                        let file = std::fs::File::open(path).map_err(|_| "could not read scene")?;
                        let mut bytes = Vec::new();
                        file.take(MAX_SCENE_BYTES as u64 + 1)
                            .read_to_end(&mut bytes)
                            .map_err(|_| "could not read scene")?;
                        let text = String::from_utf8(bytes).map_err(|_| "invalid UTF-8 scene")?;
                        Scene::decode_preset(&text)
                    })();
                    match result {
                        Ok(scene) => {
                            e.replace_scene(scene);
                            changed = true;
                            history_move = true;
                            self.status_msg = if ru {
                                "Сцена загружена"
                            } else {
                                "Scene loaded"
                            }
                            .into();
                        }
                        Err(_) => {
                            self.status_msg = if ru {
                                "Сцена RGB отклонена"
                            } else {
                                "RGB scene rejected"
                            }
                            .into()
                        }
                    }
                }
            }
        });
        if self.ak820_demo {
            let demo_can_start = self.ak820_rgb_scene_can_start() && scene_validation.is_ok();
            if playback_running {
                if ui
                    .button(if ru {
                        "Остановить предпросмотр (демо, без устройства)"
                    } else {
                        "Stop demo preview (no device)"
                    })
                    .clicked()
                {
                    self.ak820_rgb_scene_stop();
                }
            } else if ui
                .add_enabled(
                    demo_can_start,
                    egui::Button::new(if ru {
                        "Запустить предпросмотр (демо, без устройства)"
                    } else {
                        "Start demo preview (no device)"
                    }),
                )
                .clicked()
            {
                self.ak820_rgb_scene_start(e.scene.clone());
            }
            ui.label(if ru {
                "Демо не отправляет данные устройству."
            } else {
                "Demo preview sends no device data."
            });
            if !scene_validation.is_ok() {
                ui.colored_label(
                    Color32::LIGHT_RED,
                    if ru {
                        "Проверьте параметры сцены перед запуском"
                    } else {
                        "Correct the scene settings before starting"
                    },
                );
            }
        } else if playback_running {
            if ui
                .button(if ru {
                    "Остановить RAM-воспроизведение"
                } else {
                    "Stop RAM playback"
                })
                .clicked()
            {
                self.ak820_rgb_scene_stop();
            }
        } else {
            let can_start = self.ak820_rgb_scene_can_start() && scene_validation.is_ok();
            if ui
                .add_enabled(
                    can_start,
                    egui::Button::new(if ru {
                        "Запустить RAM-воспроизведение"
                    } else {
                        "Start RAM playback"
                    }),
                )
                .clicked()
            {
                self.ak820_rgb_scene_start(e.scene.clone());
            }
            if !scene_validation.is_ok() {
                ui.colored_label(
                    Color32::LIGHT_RED,
                    if ru {
                        "Проверьте параметры сцены перед запуском"
                    } else {
                        "Correct the scene settings before starting"
                    },
                );
            }
        }
        if !self.ak820_demo
            && ui
                .button(if ru { "Выкл. RGB" } else { "RGB Off" })
                .clicked()
        {
            self.ak820_rgb_scene_off();
        }
        let demo_rgb = self.ak820_demo
            && self.ak820_demo_profile.is_some_and(|p| {
                crate::app::ak820_beta1_profiles::has_feature(p, "vialrgb-direct")
            });
        let can_apply = demo_rgb
            || (!self.ak820_demo
                && !self.ak820_rgb_scene_running()
                && !self.hid_write_task_owner_active()
                && matches!(self.connect_state, ConnectState::Idle)
                && self.hid_device.as_ref().is_some_and(|h| {
                    h.ak820_editing_ready()
                        && h.ak820_image()
                            .is_some_and(|i| i.profile().is_some() && i.rgb_modes().contains(&1))
                }));
        let apply_label = if demo_rgb {
            if ru {
                "Предпросмотр основы (демо)"
            } else {
                "Preview base colors (demo)"
            }
        } else if ru {
            "Применить только основу (ОЗУ)"
        } else {
            "Apply base colors only (RAM)"
        };
        if ui
            .add_enabled(can_apply, egui::Button::new(apply_label))
            .clicked()
        {
            if demo_rgb {
                self.status_msg = if ru {
                    "Демо: предпросмотр обновлён; устройство не затронуто"
                } else {
                    "Demo preview updated; no device was contacted"
                }
                .into()
            } else {
                self.ak820_rgb_scene_stop();
                if self.hid_write_task_owner_active() {
                    self.status_msg = if ru {
                        "Дождитесь завершения операции RGB"
                    } else {
                        "Wait for the RGB operation to finish"
                    }
                    .into()
                } else if let Some(hid) = self.hid_device.as_mut() {
                    if let Ok(frame) = e.scene.base.rgb_frame() {
                        match hid.ak820_apply_palette(&frame) {
                            Ok(()) => {
                                self.rgb_settings.effect = 1;
                                self.rgb_settings.last_enabled_effect = 1;
                                self.status_msg=if ru {"Direct RGB отправлен; палитра действует до перезапуска клавиатуры"}else{"Direct RGB sent; palette lasts until the keyboard restarts"}.into()
                            }
                            Err(_err) => {
                                self.status_msg = if ru {
                                    "Не удалось применить RGB"
                                } else {
                                    "RGB apply failed"
                                }
                                .into()
                            }
                        }
                    }
                }
            }
        }
        if changed && e.scene != before {
            if !history_move {
                e.push_undo(before);
            }
            if self.ak820_rgb_scene_running() {
                self.ak820_rgb_scene_update(e.scene.clone());
            }
        }
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

    #[test]
    fn editor_history_is_bounded_and_restores_scene_snapshots() {
        let mut e = Editor::default();
        for i in 0..40 {
            e.push_undo(e.scene.clone());
            e.scene.base.brightness = i as u8;
        }
        assert_eq!(e.undo.len(), 32);
        assert!(e.undo_scene());
        assert_eq!(e.scene.base.brightness, 38);
        assert!(e.redo_scene());
        assert_eq!(e.scene.base.brightness, 39);
    }

    #[test]
    fn mask_normalization_handles_empty_duplicates_and_invalid_indices() {
        let mut e = Editor::default();
        e.scene.layers.push(Layer::default());
        e.selected = vec![4, 1, 4, LED_COUNT, 1];
        e.set_active_mask_from_selection();
        assert_eq!(e.scene.layers[0].mask, vec![1, 4]);
        e.selected.clear();
        e.set_active_mask_from_selection();
        assert!(e.scene.layers[0].mask.is_empty());
        assert!(Scene::decode_preset("{bad json").is_err());
    }

    #[test]
    fn imported_scene_replaces_current_scene_and_remains_undoable() {
        let mut e = Editor::default();
        e.scene.base.brightness = 81;
        let imported = Scene {
            base: Palette {
                brightness: 42,
                ..Palette::default()
            },
            ..Scene::default()
        };
        e.replace_scene(imported);
        assert_eq!(e.scene.base.brightness, 42);
        assert!(e.undo_scene());
        assert_eq!(e.scene.base.brightness, 81);
        assert!(e.redo_scene());
        assert_eq!(e.scene.base.brightness, 42);
    }

    #[test]
    fn active_layer_stays_valid_after_import_and_undo() {
        let mut e = Editor::default();
        e.scene.layers = vec![Layer::default(), Layer::default(), Layer::default()];
        e.active_layer = 2;

        e.replace_scene(Scene::default());
        assert_eq!(e.active_layer, 0);
        assert!(e.undo_scene());
        assert_eq!(e.scene.layers.len(), 3);
        assert!(e.active_layer < e.scene.layers.len());
        assert!(!e.move_active_layer(-1));
        assert!(e.redo_scene());
        assert!(e.scene.layers.is_empty());
        assert_eq!(e.active_layer, 0);
        assert!(!e.move_active_layer(1));
        assert!(e.undo_scene());
        e.active_layer = usize::MAX;
        assert!(!e.move_active_layer(1));
        assert_eq!(e.active_layer, 2);
    }

    #[test]
    fn effect_and_axis_controls_have_russian_labels() {
        assert_eq!(effect_label(Effect::Breathing, true), "Дыхание");
        assert_eq!(effect_label(Effect::Breathing, false), "Breathing");
        assert_eq!(axis_label(Axis::Vertical, true), "По вертикали");
    }

    #[test]
    fn invalid_quiet_hours_are_rejected_before_live_start() {
        let scene = Scene {
            quiet_hours: Some(QuietHours {
                start_minute: 480,
                end_minute: 480,
                brightness: 32,
            }),
            ..Scene::default()
        };
        assert!(scene.validate().is_err());
    }
}
