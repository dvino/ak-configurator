use crate::ak820_help::{topics, Topic};
use crate::i18n::Language;

#[derive(Clone, Default)]
struct HelpState {
    open: bool,
    selected: usize,
    query: String,
    reset_scroll: bool,
}

fn state_id() -> egui::Id {
    egui::Id::new("ak820_help_window_state")
}

pub(super) fn open(ctx: &egui::Context) {
    ctx.data_mut(|data| {
        let mut state = data.get_temp::<HelpState>(state_id()).unwrap_or_default();
        state.open = true;
        data.insert_temp(state_id(), state);
    });
}

pub(super) fn close(ctx: &egui::Context) {
    ctx.data_mut(|data| {
        let mut state = data.get_temp::<HelpState>(state_id()).unwrap_or_default();
        state.open = false;
        data.insert_temp(state_id(), state);
    });
}

pub(super) fn is_open(ctx: &egui::Context) -> bool {
    ctx.data(|data| {
        data.get_temp::<HelpState>(state_id())
            .is_some_and(|state| state.open)
    })
}

pub(super) fn open_section(ctx: &egui::Context, section: &str) {
    ctx.data_mut(|data| {
        let mut state = data.get_temp::<HelpState>(state_id()).unwrap_or_default();
        state.open = true;
        if let Some(selected) = resolve_section(section) {
            state.selected = selected;
            state.query.clear();
            state.reset_scroll = true;
        }
        data.insert_temp(state_id(), state);
    });
}

fn resolve_section(section: &str) -> Option<usize> {
    let value = section.trim().to_ascii_lowercase().replace('_', "-");
    let alias = match value.as_str() {
        "connection" | "device" => "connection-read-errors",
        "key" | "encoder" | "mapping" => "layers-modules",
        "macro" | "combo" => "macros-combo",
        "rgb" | "direct" | "scene" | "music" | "openrgb" => "lighting-palettes",
        "project" => "offline-projects",
        "backup" | "restore" | "reset" => "backups-restore",
        "firmware" | "install" | "recovery" => "firmware-install-recovery",
        "helper" | "layout" => "os-layout-helper",
        "update" | "settings" => "application-settings-updates",
        _ => &value,
    };
    topics(Language::English)
        .iter()
        .position(|topic| topic.id == alias)
}

fn select(state: &mut HelpState, index: usize) {
    if state.selected != index {
        state.selected = index;
        state.reset_scroll = true;
    }
}

fn matching_topics(content: &[Topic], query: &str) -> Vec<usize> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    content
        .iter()
        .enumerate()
        .filter_map(|(index, topic)| {
            let text = format!("{}\n{}", topic.title, topic.body).to_lowercase();
            words
                .iter()
                .all(|word| text.contains(word))
                .then_some(index)
        })
        .collect()
}

pub(super) fn show(ctx: &egui::Context, language: Language) {
    let mut state = ctx.data(|data| data.get_temp::<HelpState>(state_id()).unwrap_or_default());
    if !state.open {
        return;
    }
    let ru = language == Language::Russian;
    let tr = |en, russian| if ru { russian } else { en };
    let content = topics(language);
    state.selected = state.selected.min(content.len().saturating_sub(1));
    let mut keep_open = true;
    let size = (ctx.content_rect().size() - egui::vec2(48.0, 88.0))
        .min(egui::vec2(900.0, 610.0))
        .max(egui::vec2(300.0, 240.0));
    crate::ui_style::utility_window(
        ctx,
        tr("AK Configurator Help", "Справка AK Configurator"),
        egui::Id::new("ak820_help_window"),
        &mut keep_open,
        size,
    )
    .fixed_size(size)
    .resizable(false)
    .show(ctx, |ui| {
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 98.0).max(100.0);
            if ui
                .add_sized(
                    egui::vec2(width, 30.0),
                    egui::TextEdit::singleline(&mut state.query)
                        .hint_text(tr("Search help…", "Поиск по справке…")),
                )
                .changed()
            {
                state.reset_scroll = true;
            }
            if crate::ui_style::modern_button(
                ui,
                tr("Clear", "Очистить"),
                egui::vec2(88.0, 30.0),
                !state.query.is_empty(),
            )
            .clicked()
            {
                state.query.clear();
                state.reset_scroll = true;
            }
        });
        let matches = matching_topics(content, &state.query);
        if !matches.contains(&state.selected) {
            if let Some(&first) = matches.first() {
                select(&mut state, first);
            }
        }
        ui.add_space(8.0);
        let pane_height = (ui.available_height() - 50.0).max(100.0);
        if ui.available_width() >= 620.0 {
            ui.horizontal_top(|ui| {
                let toc_width = (ui.available_width() * 0.31).min(245.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(toc_width, pane_height),
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        ui.set_width(toc_width);
                        ui.label(egui::RichText::new(tr("Contents", "Содержание")).strong());
                        egui::ScrollArea::vertical()
                            .id_salt("ak820_help_toc")
                            .max_height((pane_height - 26.0).max(60.0))
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                if matches.is_empty() {
                                    ui.label(tr("No matching topics", "Совпадений нет"));
                                }
                                for &index in &matches {
                                    if ui
                                        .add(
                                            egui::Button::selectable(
                                                state.selected == index,
                                                &content[index].title,
                                            )
                                            .wrap(),
                                        )
                                        .clicked()
                                    {
                                        select(&mut state, index);
                                    }
                                }
                            });
                    },
                );
                ui.add_space(10.0);
                let width = ui.available_width();
                ui.allocate_ui_with_layout(
                    egui::vec2(width, pane_height),
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        ui.set_width(width);
                        render_article(
                            ui,
                            content,
                            &mut state,
                            pane_height,
                            matches.is_empty(),
                            ru,
                        );
                    },
                );
            });
        } else {
            let labels: Vec<String> = matches.iter().map(|&i| content[i].title.clone()).collect();
            if matches.is_empty() {
                ui.label(tr("No matching topics", "Совпадений нет"));
            } else {
                let selected = matches
                    .iter()
                    .position(|&i| i == state.selected)
                    .unwrap_or(0);
                if let (_, Some(index)) = crate::ui_style::modern_dropdown_select_sized(
                    ui,
                    egui::Id::new("ak820_help_topics"),
                    &labels,
                    selected,
                    ui.available_width(),
                    32.0,
                    12.5,
                ) {
                    select(&mut state, matches[index]);
                }
            }
            ui.add_space(6.0);
            render_article(
                ui,
                content,
                &mut state,
                (pane_height - 42.0).max(60.0),
                matches.is_empty(),
                ru,
            );
        }
        ui.separator();
        ui.horizontal(|ui| {
            let position = matches.iter().position(|&i| i == state.selected);
            if crate::ui_style::action_button(
                ui,
                tr("← Previous topic", "← Предыдущая тема"),
                position.is_some_and(|i| i > 0),
            )
            .clicked()
            {
                select(&mut state, matches[position.unwrap() - 1]);
            }
            if crate::ui_style::action_button(
                ui,
                tr("Next topic →", "Следующая тема →"),
                position.is_some_and(|i| i + 1 < matches.len()),
            )
            .clicked()
            {
                select(&mut state, matches[position.unwrap() + 1]);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if crate::ui_style::action_button(ui, tr("Close", "Закрыть"), true).clicked()
                {
                    state.open = false;
                }
            });
        });
    });
    state.open &= keep_open;
    ctx.data_mut(|data| data.insert_temp(state_id(), state));
}

fn render_article(
    ui: &mut egui::Ui,
    content: &[Topic],
    state: &mut HelpState,
    height: f32,
    empty: bool,
    ru: bool,
) {
    let mut scroll = egui::ScrollArea::vertical()
        .id_salt("ak820_help_article")
        .max_height(height)
        .auto_shrink([false, false]);
    if std::mem::take(&mut state.reset_scroll) {
        scroll = scroll.vertical_scroll_offset(0.0);
    }
    scroll.show(ui, |ui| {
        if empty {
            ui.label(if ru {
                "Измените запрос или очистите поиск, чтобы открыть справку."
            } else {
                "Change the query or clear the search to view help."
            });
            return;
        }
        let topic = &content[state.selected];
        ui.label(egui::RichText::new(&topic.title).size(18.0).strong());
        ui.add_space(10.0);
        for paragraph in topic.body.split("\n\n").filter(|p| !p.trim().is_empty()) {
            ui.add(egui::Label::new(paragraph).wrap());
            ui.add_space(8.0);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contextual_topic_open_clears_search_and_resets_article() {
        let ctx = egui::Context::default();
        open(&ctx);
        ctx.data_mut(|data| {
            let mut state = data.get_temp::<HelpState>(state_id()).unwrap();
            state.query = "stale search".into();
            data.insert_temp(state_id(), state);
        });
        open_section(&ctx, "backups-restore");
        let state = ctx.data(|data| data.get_temp::<HelpState>(state_id()).unwrap());
        assert_eq!(content_id(&state), "backups-restore");
        assert!(state.query.is_empty() && state.reset_scroll);
        open_section(&ctx, "feature-one-shot");
        assert!(ctx.data(|d| d.get_temp::<HelpState>(state_id()).unwrap().selected) >= 13);
    }

    fn content_id(state: &HelpState) -> &str {
        &topics(Language::English)[state.selected].id
    }

    #[test]
    fn help_search_is_localized_and_requires_all_words() {
        let ru = topics(Language::Russian);
        assert!(!matching_topics(ru, "ОДНОРАЗОВЫЕ").is_empty());
        assert!(matching_topics(ru, "no-such-help-topic").is_empty());
        assert_eq!(matching_topics(ru, "").len(), ru.len());
        let en = topics(Language::English);
        assert!(matching_topics(en, "safety backup").iter().all(|&i| {
            let text = en[i].body.to_lowercase();
            text.contains("safety") && text.contains("backup")
        }));
    }

    #[test]
    fn help_fits_small_viewport_in_both_languages_and_themes() {
        for language in Language::ALL {
            for dark in [false, true] {
                let ctx = egui::Context::default();
                ctx.set_visuals(if dark {
                    egui::Visuals::dark()
                } else {
                    egui::Visuals::light()
                });
                open(&ctx);
                let viewport =
                    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 500.0));
                for _ in 0..4 {
                    let _ = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(viewport),
                            ..Default::default()
                        },
                        |ui| show(ui.ctx(), language),
                    );
                }
                let rect = ctx
                    .memory(|m| m.area_rect(egui::Id::new("ak820_help_window")))
                    .unwrap();
                assert!(
                    rect.width() <= viewport.width() && rect.height() <= viewport.height(),
                    "{rect:?}"
                );
                assert!(is_open(&ctx));
            }
        }
    }
}
