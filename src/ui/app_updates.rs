// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit application update UI; no keyboard or preferences operations.
use super::app_update::verified::{self, Channel, Job, Offer, VerifiedDownload};
use super::{UpdateAsset, UpdateAssetState, UpdateCheckResult, UpdateCheckState, VersionRelation};
use crate::i18n::Language;

#[derive(Default)]
pub(crate) struct Runtime {
    pub(crate) open: bool,
    allow_prereleases: bool,
    check: Option<Job<Offer>>,
    download: Option<Job<VerifiedDownload>>,
    offer: Option<Offer>,
    downloaded: Option<VerifiedDownload>,
    error: Option<String>,
    cancelled: bool,
}

fn label(lang: Language, ru: &'static str, en: &'static str) -> &'static str {
    match lang {
        Language::Russian => ru,
        Language::English => en,
    }
}

fn relation(version: &str) -> Option<VersionRelation> {
    let current = semver::Version::parse(env!("CARGO_PKG_VERSION")).ok()?;
    let offered = semver::Version::parse(version).ok()?;
    Some(match offered.cmp_precedence(&current) {
        std::cmp::Ordering::Greater => VersionRelation::UpdateAvailable,
        std::cmp::Ordering::Equal => VersionRelation::UpToDate,
        std::cmp::Ordering::Less => VersionRelation::DevelopmentBuild,
    })
}

impl Runtime {
    pub(crate) fn busy(&self) -> bool {
        self.check.is_some() || self.download.is_some()
    }

    fn cancel(&mut self) {
        // Dropping jobs signals cancellation. A worker may finish an outstanding
        // HTTP read, but it has no installation or keyboard access.
        self.check = None;
        self.download = None;
        self.cancelled = true;
    }

    pub(crate) fn poll(&mut self, summary: &mut UpdateCheckState) {
        if let Some(result) = self.check.as_ref().and_then(Job::poll) {
            self.check = None;
            match result {
                Ok(offer) => {
                    if let Some(relation) = relation(&offer.version) {
                        *summary = UpdateCheckState::Ready(UpdateCheckResult {
                            latest_version: offer.tag.clone(),
                            release_url: offer.release_url.clone(),
                            platform_label: super::current_platform_label(),
                            asset: UpdateAssetState::Available(UpdateAsset {
                                name: offer.name.clone(),
                                url: offer.asset_url().to_owned(),
                            }),
                            relation,
                        });
                        self.offer = Some(offer);
                    } else {
                        self.error = Some("Invalid application version".into());
                    }
                }
                Err(error) => {
                    *summary = UpdateCheckState::Failed(error.clone());
                    self.error = Some(error);
                }
            }
        }
        if let Some(result) = self.download.as_ref().and_then(Job::poll) {
            self.download = None;
            match result {
                Ok(download) => self.downloaded = Some(download),
                Err(error) => self.error = Some(error),
            }
        }
    }

    fn reset_selection(&mut self) {
        self.cancel();
        self.offer = None;
        self.downloaded = None;
        self.error = None;
        self.cancelled = false;
    }

    pub(crate) fn draw(&mut self, ctx: &egui::Context, lang: Language) {
        if !self.open {
            return;
        }
        let mut open = true;
        egui::Window::new(label(lang, "Обновление приложения", "Application update"))
            .id(egui::Id::new("verified_app_update"))
            .open(&mut open)
            .default_width(600.0)
            .max_width((ctx.content_rect().width() - 48.0).clamp(260.0, 680.0))
            .max_height((ctx.content_rect().height() - 48.0).max(240.0))
            .resizable(true)
            .show(ctx, |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.label(format!("AK Configurator {} · {}", env!("CARGO_PKG_VERSION"), super::current_platform_label()));
                    ui.label(label(lang,
                        "Проверка и загрузка запускаются вручную. Обновление приложения сохраняет настройки помощника. Совместимая прошивка остаётся доступна офлайн.",
                        "Check and download start only when requested. Application updates preserve helper preferences. Compatible bundled firmware remains available offline."));
                    ui.add_space(8.0);
                    let busy = self.busy();
                    if ui.add_enabled(!busy, egui::Checkbox::new(&mut self.allow_prereleases,
                        label(lang, "Показывать предварительные версии", "Include prereleases"))).changed() {
                        self.reset_selection();
                    }
                    ui.horizontal_wrapped(|ui| {
                        if ui.add_enabled(!busy, egui::Button::new(label(lang, "Проверить обновления", "Check for updates"))).clicked() {
                            self.reset_selection();
                            self.check = Some(verified::start_check(if self.allow_prereleases {
                                Channel::IncludePrereleases
                            } else { Channel::Stable }));
                        }
                        if busy {
                            ui.spinner();
                            ui.label(if self.download.is_some() {
                                label(lang, "Загрузка и проверка…", "Downloading and verifying…")
                            } else {
                                label(lang, "Проверка релизов…", "Checking releases…")
                            });
                            if ui.button(label(lang, "Отмена", "Cancel")).clicked() {
                                self.cancel();
                            }
                        }
                    });
                    if let Some(offer) = self.offer.as_ref() {
                        ui.separator();
                        ui.strong(&offer.tag);
                        ui.label(&offer.name);
                        ui.label(format!("{:.1} MiB", offer.size as f64 / 1_048_576.0));
                        ui.label(format!("SHA-256: {}", offer.sha256));
                        let newer = relation(&offer.version) == Some(VersionRelation::UpdateAvailable);
                        if !newer {
                            ui.label(label(lang, "Установлена эта или более новая версия.", "This version or a newer version is already installed."));
                        }
                        let selected = offer.clone();
                        ui.horizontal_wrapped(|ui| {
                            if ui.add_enabled(newer && !busy && self.downloaded.is_none(),
                                egui::Button::new(label(lang, "Скачать и проверить", "Download and verify"))).clicked() {
                                self.error = None;
                                self.cancelled = false;
                                self.download = Some(verified::start_download(selected.clone()));
                            }
                            if ui.button(label(lang, "Изменения релиза", "Release notes")).clicked()
                                && !super::open_url_in_browser(&selected.release_url) {
                                    self.error = Some("Cannot open release notes in the browser".into());
                                }
                        });
                    }
                    if let Some(download) = &self.downloaded {
                        ui.separator();
                        ui.strong(label(lang, "Архив проверен", "Archive verified"));
                        let path = download.path.display().to_string();
                        ui.label(&path);
                        if ui.button(label(lang, "Скопировать путь", "Copy path")).clicked() {
                            ctx.copy_text(path);
                        }
                        ui.label(label(lang,
                            "Полностью завершите AK Configurator, включая помощник, через «Выход». Распакуйте архив и выполните инструкцию INSTALL.ru.md: установщик для текущего пользователя обновит приложение и сохранит его настройки.",
                            "Quit AK Configurator completely, including the helper, using Quit. Extract the archive and follow INSTALL.en.md: the per-user installer updates the application and preserves its settings."));
                    }
                    if self.cancelled {
                        ui.label(label(lang, "Операция отменена.", "Operation cancelled."));
                    }
                    if let Some(error) = &self.error {
                        ui.colored_label(ui.visuals().error_fg_color, label(lang,
                            "Проверка или загрузка не завершена. Файл не предлагается к установке.",
                            "Check or download did not complete. No file is offered for installation."));
                        ui.label(error);
                    }
                });
            });
        self.open = open;
        if !open && self.busy() {
            self.cancel();
        }
        if self.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_ui_defaults_to_no_network_no_prereleases_and_no_download() {
        let mut state = Runtime {
            open: true,
            ..Default::default()
        };
        let mut summary = UpdateCheckState::Idle;
        state.poll(&mut summary);
        assert!(!state.busy());
        assert!(!state.allow_prereleases);
        assert!(state.offer.is_none());
        assert!(state.downloaded.is_none());
        assert!(matches!(summary, UpdateCheckState::Idle));
    }

    #[test]
    fn update_window_keeps_long_paths_and_failures_inside_small_viewports() {
        for lang in [Language::Russian, Language::English] {
            let ctx = egui::Context::default();
            let mut state = Runtime {
                open: true,
                error: Some("Network unavailable. ".repeat(25)),
                downloaded: Some(VerifiedDownload {
                    path: std::path::PathBuf::from("/long-cache-path/".repeat(15)),
                }),
                ..Default::default()
            };
            for _ in 0..3 {
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(800.0, 500.0),
                        )),
                        ..Default::default()
                    },
                    |ctx| state.draw(ctx, lang),
                );
            }
            let rect = ctx
                .memory(|memory| memory.area_rect(egui::Id::new("verified_app_update")))
                .unwrap();
            assert!(rect.width() <= 800.0 && rect.height() <= 500.0, "{rect:?}");
            assert!(!state.busy());
        }
    }
}
