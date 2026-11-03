//! Opt-in OS indication borrows the selected verified HID owner. It never
//! opens a second handle, guesses a layout or retries a failed exchange.
use super::*;
use crate::ak820_os_indicator::{Mode, Scheduler, REFRESH_MS, SAMPLE_INTERVAL_MS};
use std::time::{Duration, Instant};

enum Job {
    Sample {
        generation: u64,
        queued: Instant,
        enabled: bool,
    },
    Write {
        hid: Box<crate::hid::HidDevice>,
        generation: u64,
        mode: Mode,
        layout: Option<&'static str>,
        sampled: Instant,
    },
}

struct Sample {
    generation: u64,
    layout: Option<&'static str>,
    sampled: Instant,
}

enum WorkerResult {
    Sample(Sample),
    Write(Box<Finished>),
}
#[derive(Clone, Copy, Default)]
enum IndicatorStatus {
    #[default]
    Ready,
    WorkerStopped,
    DeviceError,
    Disabled,
    Paused,
    LayoutDetected(bool),
    LayoutUnavailable,
}

struct Finished {
    hid: crate::hid::HidDevice,
    generation: u64,
    mode: Mode,
    layout: Option<&'static str>,
    result: Result<(), String>,
    sent: bool,
}

pub(super) struct Runtime {
    clock: Instant,
    checked_at: Option<Instant>,
    scheduler: Scheduler,
    worker: Option<mpsc::Sender<Job>>,
    receiver: Option<mpsc::Receiver<WorkerResult>>,
    sampling: bool,
    sample: Option<Sample>,
    in_flight: bool,
    loan_verified: bool,
    loan_rgb_direct: bool,
    loan_image: Option<crate::ak820::ImageContract>,
    loan_selected: Option<usize>,
    generation: u64,
    previous: Option<Mode>,
    status: IndicatorStatus,
}
impl Default for Runtime {
    fn default() -> Self {
        Self {
            clock: Instant::now(),
            checked_at: None,
            scheduler: Scheduler::default(),
            worker: None,
            receiver: None,
            sampling: false,
            sample: None,
            in_flight: false,
            loan_verified: false,
            loan_rgb_direct: false,
            loan_image: None,
            loan_selected: None,
            generation: 0,
            previous: None,
            status: IndicatorStatus::Ready,
        }
    }
}

impl Runtime {
    fn start_worker(&mut self) {
        if self.worker.is_some() {
            return;
        }
        let (sender, jobs) = mpsc::channel::<Job>();
        let (finished, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            // Native detector connections stay on their owning thread.
            let mut detector = crate::qmk_hid_host::Ak820LayoutDetector::new();
            while let Ok(job) = jobs.recv() {
                let result = match job {
                    Job::Sample {
                        generation,
                        queued,
                        enabled,
                    } => {
                        // Sampling desktop state never borrows the HID owner.
                        let layout = if enabled { detector.current() } else { None };
                        WorkerResult::Sample(Sample {
                            generation,
                            layout,
                            sampled: queued,
                        })
                    }
                    Job::Write {
                        hid,
                        generation,
                        mode,
                        layout,
                        sampled,
                    } => {
                        let result = if sampled.elapsed() > Duration::from_secs(2) {
                            Err(
                                "OS indication stopped: layout sample exceeded freshness limit"
                                    .into(),
                            )
                        } else {
                            hid.ak820_os_indicator_set(mode).map_err(|e| e.to_string())
                        };
                        WorkerResult::Write(Box::new(Finished {
                            hid: *hid,
                            generation,
                            mode,
                            layout,
                            result,
                            sent: true,
                        }))
                    }
                };
                if finished.send(result).is_err() {
                    break;
                }
            }
        });
        self.worker = Some(sender);
        self.receiver = Some(receiver);
    }
}

fn repaint_interval(enabled: bool, verified: bool, in_flight: bool) -> Option<Duration> {
    if in_flight {
        Some(Duration::from_millis(20))
    } else if enabled && verified {
        Some(Duration::from_millis(SAMPLE_INTERVAL_MS))
    } else {
        None
    }
}

impl EntropyApp {
    /// Keep the native event loop responsive enough to service the borrowed HID
    /// owner while the main window is hidden. The hidden-window idle cadence is
    /// deliberately coarse, so the indicator supplies its own bounded cadence.
    pub(super) fn ak820_os_indicator_repaint_interval(&self) -> Option<std::time::Duration> {
        let runtime = &self.ak820_os_indicator_runtime;
        let verified = self.hid_device.as_ref().is_some_and(|h| {
            h.ak820_editing_ready()
                && h.ak820_image()
                    .and_then(crate::ak820::ImageContract::profile)
                    .is_some_and(|p| ak820_beta1_profiles::has_feature(p, "os-indicators"))
        });
        repaint_interval(
            self.app_settings.ak820_os_indicator.enabled,
            verified,
            runtime.in_flight || runtime.sampling,
        )
    }

    pub(super) fn ak820_os_indicator_handoff_allowed(&self) -> bool {
        !self.ak820_os_indicator_runtime.scheduler.failed()
            && self
                .hid_device
                .as_ref()
                .is_none_or(|hid| hid.ak820_failure().is_none())
    }

    pub(super) fn ak820_os_indicator_owns_hid(&self) -> bool {
        self.ak820_os_indicator_runtime.in_flight
    }

    /// Captured facts keep cached controls visible during an actual verified
    /// loan. This grants no write permission and expires with the connection.
    pub(super) fn ak820_os_indicator_display_connected(&self) -> bool {
        let runtime = &self.ak820_os_indicator_runtime;
        runtime.in_flight
            && runtime.loan_verified
            && runtime.generation == self.connection_generation
            && runtime.loan_selected == self.selected_device
    }

    pub(super) fn ak820_os_indicator_display_image(&self) -> Option<crate::ak820::ImageContract> {
        self.ak820_os_indicator_display_connected()
            .then_some(self.ak820_os_indicator_runtime.loan_image)
            .flatten()
    }

    pub(super) fn ak820_os_indicator_display_rgb_direct(&self) -> bool {
        self.ak820_os_indicator_display_connected()
            && self.ak820_os_indicator_runtime.loan_rgb_direct
    }

    #[cfg(test)]
    pub(super) fn begin_test_indicator_hid_loan(&mut self) -> crate::hid::HidDevice {
        let hid = self.hid_device.take().expect("test HID owner");
        let runtime = &mut self.ak820_os_indicator_runtime;
        runtime.loan_verified = hid.ak820_editing_ready()
            && hid
                .ak820_image()
                .and_then(crate::ak820::ImageContract::profile)
                .is_some_and(|p| ak820_beta1_profiles::has_feature(p, "os-indicators"));
        assert!(
            runtime.loan_verified,
            "test loan requires a verified indicator profile"
        );
        runtime.loan_rgb_direct = hid
            .ak820_image()
            .is_some_and(|image| image.profile().is_some() && image.rgb_modes().contains(&1));
        runtime.loan_image = hid.ak820_image();
        runtime.loan_selected = self.selected_device;
        runtime.generation = self.connection_generation;
        runtime.in_flight = true;
        hid
    }

    pub(super) fn poll_ak820_os_indicator_task(&mut self) {
        if !self.ak820_os_indicator_runtime.in_flight && !self.ak820_os_indicator_runtime.sampling {
            return;
        }
        let received = self
            .ak820_os_indicator_runtime
            .receiver
            .as_ref()
            .unwrap()
            .try_recv();
        let result = match received {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                let runtime = &mut self.ak820_os_indicator_runtime;
                runtime.in_flight = false;
                runtime.sampling = false;
                runtime.scheduler.completed(
                    runtime.clock.elapsed().as_millis() as u64,
                    Mode::Normal,
                    false,
                );
                runtime.status = IndicatorStatus::WorkerStopped;
                self.resume_pending_device_connect();
                return;
            }
        };
        let runtime = &mut self.ak820_os_indicator_runtime;
        let result = match result {
            WorkerResult::Sample(sample) => {
                log::debug!(
                    "indicator sample completion generation={} current={}",
                    sample.generation,
                    self.connection_generation
                );
                runtime.sampling = false;
                if sample.generation == self.connection_generation
                    && runtime.checked_at == Some(sample.sampled)
                {
                    runtime.sample = Some(sample);
                }
                return;
            }
            WorkerResult::Write(result) => *result,
        };
        runtime.in_flight = false;
        log::debug!(
            "indicator loan end generation={} current={}",
            result.generation,
            self.connection_generation
        );
        if result.generation != self.connection_generation {
            runtime.previous = None;
            self.resume_pending_device_connect();
            return;
        }
        runtime.previous = Some(result.mode);
        if result.sent || result.result.is_err() {
            runtime.scheduler.completed(
                runtime.clock.elapsed().as_millis() as u64,
                result.mode,
                result.result.is_ok(),
            );
        }
        runtime.status = if result.result.is_err() {
            IndicatorStatus::DeviceError
        } else if !self.app_settings.ak820_os_indicator.enabled {
            IndicatorStatus::Disabled
        } else {
            layout_status(result.layout)
        };
        self.hid_device = Some(result.hid);
        self.resume_pending_device_connect();
    }

    pub(super) fn tick_ak820_os_indicator(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        if self.ak820_os_indicator_owns_hid() {
            ctx.request_repaint_after(Duration::from_millis(20));
            return;
        }
        let busy = self.hid_write_task_owner_active()
            || self
                .connection_failure
                .is_some_and(|(generation, failure)| {
                    generation == self.connection_generation
                        && matches!(failure, ConnectionFailure::LeaseBusy)
                })
            || self.ak820_rgb_scene_running()
            || !matches!(self.connect_state, ConnectState::Idle)
            || !self.settings_write_queue.is_empty()
            || self.qmk_settings_write_pending()
            || self.pending_device_connect.is_some()
            || self.pending_file_dialog.is_some()
            || self.unlock_open
            || self.exit_after_hid_write
            || crate::background_helper_requested()
            || self.import_pending()
            || self.combo_dirty
            || self.pending_layer_write.is_some()
            || self.pending_layout_undo;
        log::debug!(
            "indicator tick generation={} page={} availability={}",
            self.connection_generation,
            if ctx.data(|d| d
                .get_temp::<bool>(egui::Id::new(super::ak820_backup_inspection::OPEN_ID))
                .unwrap_or(false))
            {
                "backups"
            } else {
                "main-or-other"
            },
            if busy {
                "busy"
            } else if self.hid_device.is_some() {
                "owner"
            } else {
                "unavailable"
            }
        );
        let verified = self.hid_device.as_ref().is_some_and(|h| {
            h.ak820_editing_ready()
                && h.ak820_image()
                    .and_then(crate::ak820::ImageContract::profile)
                    .is_some_and(|p| ak820_beta1_profiles::has_feature(p, "os-indicators"))
        });
        let runtime = &mut self.ak820_os_indicator_runtime;
        if busy || !verified || self.headless {
            runtime.scheduler.pause();
            runtime.previous = None;
            runtime.sample = None;
            runtime.checked_at = None;
            if !matches!(
                runtime.status,
                IndicatorStatus::DeviceError | IndicatorStatus::WorkerStopped
            ) {
                runtime.status = if self.app_settings.ak820_os_indicator.enabled {
                    IndicatorStatus::Paused
                } else {
                    IndicatorStatus::Disabled
                };
            }
            return;
        }
        if runtime.generation != self.connection_generation {
            runtime.generation = self.connection_generation;
            runtime.previous = None;
            runtime.checked_at = None;
            runtime.sample = None;
            runtime.status = IndicatorStatus::Ready;
        }
        runtime.scheduler.bind(self.connection_generation);
        if !self.app_settings.ak820_os_indicator.enabled
            && runtime.previous.is_none_or(|m| m == Mode::Normal)
        {
            if !matches!(
                runtime.status,
                IndicatorStatus::DeviceError | IndicatorStatus::WorkerStopped
            ) {
                runtime.status = IndicatorStatus::Disabled;
            }
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(SAMPLE_INTERVAL_MS));
        if runtime.scheduler.failed() {
            runtime.sample = None;
            return;
        }
        // A completed sample is consumed only after the current connection and
        // all competing write owners have been checked on the UI thread.
        if let Some(sample) = runtime.sample.take() {
            if sample.generation != self.connection_generation {
                return;
            }
            if sample.sampled.elapsed() > Duration::from_secs(2) {
                runtime.scheduler.completed(
                    runtime.clock.elapsed().as_millis() as u64,
                    Mode::Normal,
                    false,
                );
                runtime.status = IndicatorStatus::DeviceError;
                return;
            }
            let Some(mode) = self.app_settings.ak820_os_indicator.mode(sample.layout) else {
                runtime.scheduler.pause();
                runtime.status = layout_status(sample.layout);
                return;
            };
            let refresh_due = runtime
                .scheduler
                .due(runtime.clock.elapsed().as_millis() as u64, mode);
            if crate::ak820_os_indicator::should_send(mode, runtime.previous, refresh_due) {
                let Some(hid) = self.hid_device.take() else {
                    return;
                };
                runtime.loan_verified = verified;
                runtime.loan_rgb_direct = hid.ak820_image().is_some_and(|image| {
                    image.profile().is_some() && image.rgb_modes().contains(&1)
                });
                runtime.loan_image = hid.ak820_image();
                runtime.loan_selected = self.selected_device;
                runtime.in_flight = true;
                log::debug!("indicator loan begin generation={}", sample.generation);
                let job = Job::Write {
                    hid: Box::new(hid),
                    generation: sample.generation,
                    mode,
                    layout: sample.layout,
                    sampled: sample.sampled,
                };
                if let Err(error) = runtime.worker.as_ref().unwrap().send(job) {
                    if let Job::Write { hid, .. } = error.0 {
                        self.hid_device = Some(*hid);
                    }
                    runtime.in_flight = false;
                    runtime.scheduler.completed(
                        runtime.clock.elapsed().as_millis() as u64,
                        mode,
                        false,
                    );
                    runtime.status = IndicatorStatus::WorkerStopped;
                }
                ctx.request_repaint_after(Duration::from_millis(20));
                return;
            }
            runtime.previous = Some(mode);
            runtime.status = if !self.app_settings.ak820_os_indicator.enabled {
                IndicatorStatus::Disabled
            } else {
                layout_status(sample.layout)
            };
        }
        if runtime.sampling
            || runtime.checked_at.is_some_and(|at| {
                now.saturating_duration_since(at).as_millis() < u128::from(SAMPLE_INTERVAL_MS)
            })
        {
            return;
        }
        runtime.start_worker();
        runtime.checked_at = Some(now);
        runtime.sampling = true;
        log::debug!(
            "indicator sample begin generation={}",
            self.connection_generation
        );
        if runtime
            .worker
            .as_ref()
            .unwrap()
            .send(Job::Sample {
                generation: self.connection_generation,
                queued: now,
                enabled: self.app_settings.ak820_os_indicator.enabled,
            })
            .is_err()
        {
            runtime.sampling = false;
            runtime.scheduler.completed(
                runtime.clock.elapsed().as_millis() as u64,
                Mode::Normal,
                false,
            );
            runtime.status = IndicatorStatus::WorkerStopped;
        }
        ctx.request_repaint_after(Duration::from_millis(20));
    }

    /// OS layout indicator preferences are part of the general application
    /// settings list; the runtime and its fault labels remain shared here.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn draw_ak820_os_indicator_app_settings_row(
        &mut self,
        ui: &mut egui::Ui,
        row: usize,
        content_width: f32,
        row_height: f32,
        metrics: crate::ui_style::ResponsiveMetrics,
    ) {
        self.draw_indicator_row_with_save(
            ui,
            row,
            content_width,
            row_height,
            metrics,
            save_app_settings,
        );
    }

    fn draw_indicator_row_with_save(
        &mut self,
        ui: &mut egui::Ui,
        row: usize,
        content_width: f32,
        row_height: f32,
        metrics: crate::ui_style::ResponsiveMetrics,
        save: impl Fn(&AppSettings),
    ) {
        let lang = self.app_settings.language;
        let ru = lang == crate::i18n::Language::Russian;
        let tr = |r: &'static str, e: &'static str| if ru { r } else { e };
        let switch_size = metrics.size(46.0, 24.0);
        let control_width = metrics.settings_control_width();
        match row {
            13 => {
                let mut enabled = self.app_settings.ak820_os_indicator.enabled;
                crate::ui_style::settings_list_row(
                    ui,
                    content_width,
                    row_height,
                    tr("Индикатор раскладки OS", "OS layout indicator"),
                    true,
                    switch_size.x,
                    |ui| {
                        let _ = crate::ui_style::settings_switch_sized_stable(
                            ui,
                            "ak820_os_indicator_enabled",
                            &mut enabled,
                            switch_size,
                        );
                    },
                );
                if enabled != self.app_settings.ak820_os_indicator.enabled {
                    self.app_settings.ak820_os_indicator.enabled = enabled;
                    save(&self.app_settings);
                }
            }
            14 | 15 => {
                let (key, label) = if row == 14 {
                    (
                        "en",
                        tr("LED: английская раскладка", "LED when English is detected"),
                    )
                } else {
                    (
                        "ru",
                        tr("LED: русская раскладка", "LED when Russian is detected"),
                    )
                };
                let mode = self
                    .app_settings
                    .ak820_os_indicator
                    .layouts
                    .get(key)
                    .copied();
                let values = [Mode::Normal, Mode::Off, Mode::On, Mode::Breathing];
                let mut labels = vec![tr("Не задано", "Not set").to_owned()];
                labels.extend(values.iter().map(|value| mode_label(*value, ru).to_owned()));
                let selected = mode
                    .and_then(|mode| values.iter().position(|value| *value == mode))
                    .map_or(0, |index| index + 1);
                let enabled = self.app_settings.ak820_os_indicator.enabled;
                let dropdown_id = mapping_widget_id(ui, key);
                let mut changed = false;
                crate::ui_style::settings_list_row(
                    ui,
                    content_width,
                    row_height,
                    label,
                    enabled,
                    control_width,
                    |ui| {
                        let (response, picked) = crate::ui_style::modern_dropdown_select_sized(
                            ui,
                            dropdown_id,
                            &labels,
                            selected,
                            control_width,
                            row_height.min(40.0),
                            metrics.value(12.5),
                        );
                        response.on_hover_text(mode_tooltip(ru));
                        if let Some(index) = picked {
                            changed = select_mapping(
                                &mut self.app_settings.ak820_os_indicator,
                                key,
                                index,
                            );
                        }
                    },
                );
                if changed {
                    save(&self.app_settings);
                }
            }
            16 => {
                let status = status_label(self.ak820_os_indicator_runtime.status, ru);
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(content_width, row_height),
                    egui::Sense::hover(),
                );
                let painter = ui.painter();
                let color = app_muted_text(ui.visuals().dark_mode);
                for (line, text) in [
                    tr("Обнаруженная раскладка ОС", "Detected OS layout"),
                    status,
                    tr(
                        "Настройки выше сохранены: они задают LED, а не раскладку.",
                        "Saved choices above control the LED, not the layout.",
                    ),
                ]
                .iter()
                .enumerate()
                {
                    painter.text(
                        egui::pos2(
                            rect.left() + 2.0,
                            rect.top() + metrics.value(3.0 + line as f32 * 16.0),
                        ),
                        egui::Align2::LEFT_TOP,
                        *text,
                        egui::FontId::proportional(metrics.value(11.0)),
                        color,
                    );
                }
            }
            _ => {}
        }
    }

    #[cfg(test)]
    pub(super) fn draw_ak820_os_indicator_page(
        &mut self,
        ui: &mut egui::Ui,
        content_rect: egui::Rect,
    ) {
        let lang = self.app_settings.language;
        let ru = lang == crate::i18n::Language::Russian;
        let dark = ui.visuals().dark_mode;
        let tr = |r: &'static str, e: &'static str| if ru { r } else { e };
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
        crate::ui_style::allocate_ui_at_rect(ui, content_rect, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(metrics.value(18.0));
                ui.label(
                    RichText::new(tr("Индикатор раскладки OS", "OS layout indicator"))
                        .size(metrics.value(18.0))
                        .strong(),
                );
                ui.add_space(metrics.value(6.0));
                ui.label(
                    RichText::new(tr(
                        "Настройки помощника перенесены в общие настройки приложения.",
                        "Helper preferences are now part of general application settings.",
                    ))
                    .size(metrics.value(13.0))
                    .color(app_muted_text(dark)),
                );
                ui.add_space(metrics.value(20.0));
                if crate::ui_style::modern_button(
                    ui,
                    tr("Открыть настройки приложения", "Open application settings"),
                    metrics.size(240.0, 34.0),
                    true,
                )
                .clicked()
                {
                    self.open_app_settings_page();
                }
            });
        });
    }
}

fn layout_status(layout: Option<&str>) -> IndicatorStatus {
    match layout {
        Some("en") => IndicatorStatus::LayoutDetected(false),
        Some("ru") => IndicatorStatus::LayoutDetected(true),
        _ => IndicatorStatus::LayoutUnavailable,
    }
}

fn mapping_widget_id(ui: &egui::Ui, key: &str) -> egui::Id {
    ui.make_persistent_id(("ak820_os_layout", key))
}

fn select_mapping(
    settings: &mut crate::ak820_os_indicator::Settings,
    key: &str,
    index: usize,
) -> bool {
    let values = [Mode::Normal, Mode::Off, Mode::On, Mode::Breathing];
    if let Some(mode) = index.checked_sub(1).and_then(|index| values.get(index)) {
        settings.layouts.insert(key.into(), *mode);
        true
    } else {
        false
    }
}

fn mode_tooltip(ru: bool) -> &'static str {
    if ru {
        "Обычно: поведение прошивки; LED выключен: не горит; LED включён: горит; дыхание: медленное дыхание."
    } else {
        "Normal: firmware's ordinary behavior; LED off: unlit; LED on: lit; breathing: slow breathing."
    }
}

fn status_label(status: IndicatorStatus, ru: bool) -> &'static str {
    match (status, ru) {
        (IndicatorStatus::Ready, false) => "Waiting for a supported keyboard.",
        (IndicatorStatus::Ready, true) => "Ожидание поддерживаемой клавиатуры.",
        (IndicatorStatus::WorkerStopped, false) => "The helper stopped unexpectedly.",
        (IndicatorStatus::WorkerStopped, true) => "Помощник неожиданно остановился.",
        (IndicatorStatus::DeviceError, false) => {
            "Stopped after a device error. Read settings again to resume."
        }
        (IndicatorStatus::DeviceError, true) => {
            "Остановлено из-за ошибки устройства. Прочитайте настройки снова."
        }
        (IndicatorStatus::Disabled, false) => "Helper is off.",
        (IndicatorStatus::Disabled, true) => "Помощник выключен.",
        (IndicatorStatus::Paused, false) => "Paused: busy or unverified; layout not refreshed.",
        (IndicatorStatus::Paused, true) => {
            "Пауза: занято или не проверено; раскладка не обновляется."
        }
        (IndicatorStatus::LayoutDetected(false), false) => "Last observed: English.",
        (IndicatorStatus::LayoutDetected(true), false) => "Last observed: Russian.",
        (IndicatorStatus::LayoutDetected(false), true) => "Последнее наблюдение: английская.",
        (IndicatorStatus::LayoutDetected(true), true) => "Последнее наблюдение: русская.",
        (IndicatorStatus::LayoutUnavailable, false) => "OS layout unavailable.",
        (IndicatorStatus::LayoutUnavailable, true) => "Раскладка ОС недоступна.",
    }
}

fn mode_label(mode: Mode, ru: bool) -> &'static str {
    match (mode, ru) {
        (Mode::Normal, false) => "Normal",
        (Mode::Normal, true) => "Обычно",
        (Mode::Off, false) => "LED off",
        (Mode::Off, true) => "LED выключен",
        (Mode::On, false) => "LED on",
        (Mode::On, true) => "LED включён",
        (Mode::Breathing, false) => "Slow breathing",
        (Mode::Breathing, true) => "Медленное дыхание",
    }
}

/// The caller retains the application's ordinary single-instance lock across
/// this loop and the handoff to the GUI. No eframe app or window is allocated.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum HelperExit {
    OpenInterface,
    Disabled,
}

pub(crate) fn background_helper_enabled() -> bool {
    load_app_settings().ak820_os_indicator.enabled
}

fn endpoint_key(device: &crate::device::Device) -> String {
    // A path alone may be reused after unplug/replug. Include the concrete
    // enumeration token and immutable identity metadata without logging it.
    format!(
        "{:04x}:{:04x}\0{}\0{}\0{}",
        device.vendor_id,
        device.product_id,
        device.path,
        device.instance_token,
        device.serial_number
    )
}

/// Faulted endpoints remain blocked until an actual successful enumeration
/// observes their absence or a different concrete enumeration token.
#[derive(Default)]
struct FaultedEndpoints(std::collections::BTreeSet<String>);
impl FaultedEndpoints {
    fn observed(&mut self, keys: &[String]) {
        self.0.retain(|key| keys.contains(key));
    }
    fn fault(&mut self, key: String) {
        self.0.insert(key);
    }
    fn blocked(&self, key: &str) -> bool {
        self.0.contains(key)
    }
}

pub(crate) fn run_background_helper() -> HelperExit {
    #[cfg(target_os = "macos")]
    crate::hid::initialize_macos_hid_on_main_thread();
    let initial_signal = read_single_instance_signal();
    let mut detector = crate::qmk_hid_host::Ak820LayoutDetector::new();
    let mut faulted = FaultedEndpoints::default();
    let mut active: Option<(String, crate::hid::HidDevice, Mode, Option<Instant>)> = None;
    let mut last_scan_at: Option<Instant> = None;
    loop {
        let cycle_started = Instant::now();
        let settings = load_app_settings().ak820_os_indicator;
        let open_interface = read_single_instance_signal() != initial_signal;
        if open_interface || !settings.enabled {
            if let Some((_, hid, previous, _)) = active.take() {
                if previous != Mode::Normal {
                    // Orderly release is attempted once. A failure cannot
                    // trigger a retry; firmware lease expiry is the fallback.
                    let _ = hid.ak820_os_indicator_set(Mode::Normal);
                }
            }
            return if open_interface {
                HelperExit::OpenInterface
            } else {
                HelperExit::Disabled
            };
        }
        let scan_due = last_scan_at.is_none_or(|at| {
            cycle_started.saturating_duration_since(at).as_millis() >= u128::from(REFRESH_MS)
        });
        if scan_due {
            last_scan_at = Some(cycle_started);
            let devices = match DeviceManager::scan_usb_devices() {
                Ok(devices) => devices,
                Err(_) => {
                    if let Some((key, _, _, _)) = active.take() {
                        faulted.fault(key);
                    }
                    // A scan error is not evidence of a physical disconnect.
                    std::thread::sleep(Duration::from_millis(SAMPLE_INTERVAL_MS));
                    continue;
                }
            };
            let keys: Vec<String> = devices.iter().map(endpoint_key).collect();
            faulted.observed(&keys);
            let candidate = if devices.len() == 1 {
                devices.first()
            } else {
                None
            };
            if active
                .as_ref()
                .is_some_and(|(key, _, _, _)| candidate.is_none_or(|d| endpoint_key(d) != *key))
            {
                // Ambiguity or a replaced enumeration revokes the old handle.
                // No report is sent to an identity that enumeration no longer proves.
                active = None;
            }
            if active.is_none() {
                if let Some(device) = candidate {
                    let key = endpoint_key(device);
                    if !faulted.blocked(&key) {
                        let loaded = crate::hid::HidDevice::open_fresh_for(device)
                            .map_err(|e| e.to_string())
                            .and_then(|hid| {
                                super::ak820_connection::load_ak820(device, hid, |_| {
                                    if read_single_instance_signal() != initial_signal {
                                        Err("OS helper connection cancelled for interface handoff"
                                            .into())
                                    } else {
                                        Ok(())
                                    }
                                })
                            });
                        match loaded {
                            Ok(mut result) => {
                                // All layout/editor state is dropped here. The
                                // only retained device object is its checked owner.
                                if let Some(hid) = result.hid_device.take() {
                                    if hid
                                        .ak820_image()
                                        .and_then(crate::ak820::ImageContract::profile)
                                        .is_some_and(|p| {
                                            ak820_beta1_profiles::has_feature(p, "os-indicators")
                                        })
                                    {
                                        active = Some((key, hid, Mode::Normal, None));
                                    } else {
                                        faulted.fault(key);
                                    }
                                } else {
                                    faulted.fault(key);
                                }
                            }
                            Err(_) => faulted.fault(key),
                        }
                    }
                }
            }
        }
        if let Some((key, hid, previous, last_set_at)) = active.as_mut() {
            // Loading the complete image/settings may take time. Reopen host
            // preferences before choosing the mode rather than publishing the
            // pre-connection opt-in/mapping snapshot.
            let settings = load_app_settings().ak820_os_indicator;
            let sampled = Instant::now();
            if let Some(mode) = settings.mode(detector.current()) {
                // Slow desktop APIs must not publish a stale active-window state.
                // Do not make a stale queued report after a system suspension.
                let fresh = sampled.elapsed() <= Duration::from_secs(2)
                    && read_single_instance_signal() == initial_signal;
                let refresh_due = last_set_at.is_some_and(|at| {
                    sampled.saturating_duration_since(at).as_millis() >= u128::from(REFRESH_MS)
                });
                let should_send =
                    crate::ak820_os_indicator::should_send(mode, Some(*previous), refresh_due);
                let result = if !fresh {
                    Err(anyhow::anyhow!("OS helper sample expired"))
                } else if should_send {
                    hid.ak820_os_indicator_set(mode)
                } else {
                    Ok(())
                };
                if result.is_err() {
                    faulted.fault(key.clone());
                    active = None;
                } else {
                    if should_send {
                        *previous = mode;
                        *last_set_at = Some(Instant::now());
                    }
                }
            }
        }
        std::thread::sleep(
            Duration::from_millis(SAMPLE_INTERVAL_MS).saturating_sub(cycle_started.elapsed()),
        );
    }
}

#[cfg(test)]
mod background_tests {
    use super::*;

    #[test]
    fn repaint_cadence_keeps_indicator_jobs_live_when_hidden() {
        assert_eq!(
            repaint_interval(true, true, false),
            Some(Duration::from_millis(SAMPLE_INTERVAL_MS))
        );
        assert_eq!(
            repaint_interval(true, true, true),
            Some(Duration::from_millis(20))
        );
        assert_eq!(repaint_interval(true, false, false), None);
        assert_eq!(repaint_interval(false, true, false), None);
    }

    fn sampled_app() -> (
        EntropyApp,
        mpsc::Receiver<Job>,
        mpsc::Sender<WorkerResult>,
        crate::hid::TestHidRecorder,
    ) {
        let mut app = EntropyApp::new_inert_for_test();
        app.device_manager.replace_devices(vec![Device {
            name: "Synthetic AK820".into(),
            vendor_id: 0x320f,
            product_id: 0x505b,
            manufacturer: "Synthetic".into(),
            serial_number: "fixture".into(),
            bus_type: "USB".into(),
            path: "synthetic-indicator-owner".into(),
            instance_token: "synthetic-indicator-owner".into(),
            firmware: FirmwareProtocol::Vial,
        }]);
        app.selected_device = Some(0);
        let (hid, requests) = crate::hid::HidDevice::test_beta3_ak820_device(
            ak820_beta1_profiles::ProfileId::Main,
            true,
        );
        app.hid_device = Some(hid);
        app.ak820_os_indicator_runtime.generation = app.connection_generation;
        app.app_settings.ak820_os_indicator.enabled = true;
        app.app_settings
            .ak820_os_indicator
            .layouts
            .insert("en".into(), Mode::Off);
        let (jobs, received) = mpsc::channel();
        let (finished, results) = mpsc::channel();
        app.ak820_os_indicator_runtime.worker = Some(jobs);
        app.ak820_os_indicator_runtime.receiver = Some(results);
        (app, received, finished, requests)
    }

    #[test]
    fn multi_frame_main_and_backups_preserve_indicator_loan_state() {
        use super::super::vial_hid_task::{CaptureAvailability, CaptureFailure};
        for backups in [false, true] {
            for scenario in 0..3 {
                let (mut app, jobs, finished, requests) = sampled_app();
                app.app_settings.language = crate::i18n::Language::English;
                let ctx = egui::Context::default();
                let generation = app.connection_generation;
                let selected = app.selected_device;
                assert_eq!(selected, Some(0));
                app.app_settings.ak820_os_indicator.layouts.clear();
                app.ak820_os_indicator_runtime.scheduler.bind(generation);
                app.ak820_os_indicator_runtime
                    .scheduler
                    .completed(0, Mode::Normal, true);
                app.ak820_os_indicator_runtime.previous = Some(Mode::Normal);
                if backups {
                    super::super::ak820_backup_inspection::test_set_frame_state(&ctx, generation);
                }
                super::super::ak820_help_ui::open_section(&ctx, "backups-restore");
                // egui places and reveals a newly opened window over initial
                // frames. The observation starts only after that setup.
                let mut warmup_visible = false;
                for warmup in 0..8 {
                    let output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(1200.0, 900.0),
                            )),
                            time: Some(warmup as f64 / 10.0),
                            ..Default::default()
                        },
                        |ui| eframe::App::ui(&mut app, ui, &mut eframe::Frame::_new_kittest()),
                    );
                    warmup_visible = output.shapes.iter().any(|shape| {
                        matches!(
                            &shape.shape, egui::Shape::Text(text)
                            if text.galley.text() == "AK Configurator Help"
                                && shape.clip_rect.intersects(text.visual_bounding_rect())
                        )
                    });
                }
                assert!(super::super::ak820_help_ui::is_open(&ctx));
                assert!(
                    warmup_visible,
                    "help dialog did not become visible during warmup"
                );
                let expected_state = super::super::ak820_backup_inspection::test_frame_state(&ctx);
                let open_id = egui::Id::new(super::super::ak820_backup_inspection::OPEN_ID);
                let dialog_id = egui::Id::new("ak820_help_window");
                let mut dialog_rect = None;
                for frame in 0..200 {
                    let layout = if frame < 100 || frame % 2 == 0 {
                        "en"
                    } else {
                        "ru"
                    };
                    app.unlock_open = scenario == 1;
                    app.connection_failure =
                        (scenario == 2).then_some((generation, ConnectionFailure::LeaseBusy));
                    if scenario != 0 {
                        app.tick_ak820_os_indicator(&ctx);
                        assert!(matches!(
                            app.ak820_os_indicator_runtime.status,
                            IndicatorStatus::Paused
                        ));
                        assert!(jobs.try_recv().is_err());
                        let expected = if scenario == 1 {
                            CaptureFailure::OperationBusy
                        } else {
                            CaptureFailure::LeaseBusy
                        };
                        assert_eq!(
                            app.capture_availability(),
                            CaptureAvailability::Unavailable(expected)
                        );
                    } else {
                        app.ak820_os_indicator_runtime.checked_at = None;
                        app.tick_ak820_os_indicator(&ctx);
                        let Job::Sample {
                            generation: sampled_generation,
                            queued,
                            ..
                        } = jobs.try_recv().unwrap()
                        else {
                            panic!("sampling must not borrow the owner");
                        };
                        finished
                            .send(WorkerResult::Sample(Sample {
                                generation: sampled_generation,
                                layout: Some(layout),
                                sampled: queued,
                            }))
                            .unwrap();
                        app.poll_ak820_os_indicator_task();
                        app.tick_ak820_os_indicator(&ctx);
                        assert!(
                            jobs.try_recv().is_err(),
                            "observation must not schedule a write"
                        );
                    }
                    let loan = app.begin_test_indicator_hid_loan();
                    let expected = if scenario == 2 {
                        CaptureFailure::LeaseBusy
                    } else {
                        CaptureFailure::OperationBusy
                    };
                    assert_eq!(
                        app.capture_availability(),
                        CaptureAvailability::Unavailable(expected)
                    );
                    let output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(1200.0, 900.0),
                            )),
                            time: Some(1.0 + frame as f64 / 60.0),
                            ..Default::default()
                        },
                        |ui| eframe::App::ui(&mut app, ui, &mut eframe::Frame::_new_kittest()),
                    );
                    assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
                    egui::Shape::Text(text) if text.galley.text() == concat!("AK Configurator ", env!("CARGO_PKG_VERSION"))
                        && shape.clip_rect.intersects(text.visual_bounding_rect()))));
                    let rendered = |expected: &str| {
                        output.shapes.iter().any(|shape| {
                            matches!(
                                &shape.shape, egui::Shape::Text(text)
                                if text.galley.text() == expected
                                    && shape.clip_rect.intersects(text.visual_bounding_rect())
                            )
                        })
                    };
                    if backups {
                        assert!(
                            rendered("AK820 backups"),
                            "backup page must render on frame {frame}"
                        );
                        assert!(
                            rendered("Select or enter the backup manifest.json file"),
                            "inline inspection entry must render on frame {frame}"
                        );
                        assert!(
                            rendered("synthetic inspection"),
                            "inspection draft must render on frame {frame}"
                        );
                    } else {
                        assert!(
                            rendered("K0"),
                            "main keyboard canvas must render on frame {frame}"
                        );
                        assert!(!rendered("AK820 backups"));
                    }
                    assert!(super::super::ak820_help_ui::is_open(&ctx));
                    assert!(
                        rendered("AK Configurator Help"),
                        "help dialog must render on frame {frame}"
                    );
                    let current_rect = ctx
                        .memory(|memory| memory.area_rect(dialog_id))
                        .expect("the same help dialog ID must remain present");
                    if let Some(initial_rect) = dialog_rect {
                        assert_eq!(
                            current_rect, initial_rect,
                            "help dialog was repositioned or recreated"
                        );
                    } else {
                        dialog_rect = Some(current_rect);
                    }
                    assert_eq!(
                        ctx.data(|d| d.get_temp::<bool>(open_id).unwrap_or(false)),
                        backups
                    );
                    assert_eq!(
                        super::super::ak820_backup_inspection::test_frame_state(&ctx),
                        expected_state
                    );
                    assert_eq!(app.connection_generation, generation);
                    assert_eq!(app.selected_device, selected);
                    assert!(app.hid_connected_for_display());
                    assert!(app.ak820_os_indicator_display_image().is_some());
                    assert!(!app.hid_available_for_ui_interaction());
                    assert_eq!(app.app_settings.ak820_os_indicator.mode(Some("en")), None);
                    assert_eq!(app.app_settings.ak820_os_indicator.mode(Some("ru")), None);
                    finished
                        .send(WorkerResult::Write(Box::new(Finished {
                            hid: loan,
                            generation,
                            mode: Mode::Normal,
                            layout: Some(layout),
                            result: Ok(()),
                            sent: false,
                        })))
                        .unwrap();
                    app.poll_ak820_os_indicator_task();
                    assert!(app.hid_device.is_some());
                    assert!(requests.requests().is_empty());
                }
                app.unlock_open = false;
                app.connection_failure = None;
                let loan = app.begin_test_indicator_hid_loan();
                let image = app.ak820_os_indicator_runtime.loan_image;
                app.ak820_os_indicator_runtime.loan_image = None;
                assert!(app.ak820_os_indicator_display_image().is_none());
                app.ak820_os_indicator_runtime.loan_image = image;
                app.selected_device = None;
                assert!(!app.hid_connected_for_display());
                assert!(app.ak820_os_indicator_display_image().is_none());
                app.selected_device = selected;
                app.connection_generation = generation.wrapping_add(1);
                assert!(!app.hid_connected_for_display());
                assert!(app.ak820_os_indicator_display_image().is_none());
                assert_eq!(
                    app.capture_availability(),
                    CaptureAvailability::Unavailable(CaptureFailure::Disconnected)
                );
                finished
                    .send(WorkerResult::Write(Box::new(Finished {
                        hid: loan,
                        generation,
                        mode: Mode::Normal,
                        layout: None,
                        result: Ok(()),
                        sent: false,
                    })))
                    .unwrap();
                app.poll_ak820_os_indicator_task();
                assert!(app.hid_device.is_none());
                assert!(requests.requests().is_empty());
            }
        }
    }

    #[test]
    fn competing_and_stale_samples_pause_without_packets() {
        use super::super::vial_hid_task::{CaptureAvailability, CaptureFailure};
        let (mut app, jobs, finished, requests) = sampled_app();
        let ctx = egui::Context::default();
        let generation = app.connection_generation;
        app.tick_ak820_os_indicator(&ctx);
        let Job::Sample { queued, .. } = jobs.try_recv().unwrap() else {
            panic!("sample")
        };
        finished
            .send(WorkerResult::Sample(Sample {
                generation: generation.wrapping_add(1),
                layout: Some("ru"),
                sampled: queued,
            }))
            .unwrap();
        app.poll_ak820_os_indicator_task();
        assert!(app.ak820_os_indicator_runtime.sample.is_none());
        app.unlock_open = true;
        app.tick_ak820_os_indicator(&ctx);
        assert!(matches!(
            app.ak820_os_indicator_runtime.status,
            IndicatorStatus::Paused
        ));
        assert_eq!(
            app.capture_availability(),
            CaptureAvailability::Unavailable(CaptureFailure::OperationBusy)
        );
        app.unlock_open = false;
        app.connection_failure = Some((generation, ConnectionFailure::LeaseBusy));
        assert_eq!(
            app.capture_availability(),
            CaptureAvailability::Unavailable(CaptureFailure::LeaseBusy)
        );
        assert!(app.hid_connected_for_display());
        assert_eq!(app.connection_generation, generation);
        assert!(requests.requests().is_empty());
    }

    #[test]
    fn busy_state_does_not_claim_a_new_layout_or_change_mappings() {
        let (mut app, _jobs, _finished, requests) = sampled_app();
        app.app_settings
            .ak820_os_indicator
            .layouts
            .insert("ru".into(), Mode::On);
        app.ak820_os_indicator_runtime.status = IndicatorStatus::LayoutDetected(false);
        app.unlock_open = true;
        app.tick_ak820_os_indicator(&egui::Context::default());
        assert!(matches!(
            app.ak820_os_indicator_runtime.status,
            IndicatorStatus::Paused
        ));
        assert_eq!(
            app.app_settings.ak820_os_indicator.mode(Some("en")),
            Some(Mode::Off)
        );
        assert_eq!(
            app.app_settings.ak820_os_indicator.mode(Some("ru")),
            Some(Mode::On)
        );
        assert!(requests.requests().is_empty());
        app.app_settings.ak820_os_indicator.enabled = false;
        app.tick_ak820_os_indicator(&egui::Context::default());
        assert!(matches!(
            app.ak820_os_indicator_runtime.status,
            IndicatorStatus::Disabled
        ));
    }

    #[test]
    fn disabling_with_verified_idle_device_clears_old_layout_status() {
        let (mut app, _jobs, _finished, requests) = sampled_app();
        app.ak820_os_indicator_runtime.status = IndicatorStatus::LayoutDetected(false);
        app.ak820_os_indicator_runtime.previous = Some(Mode::Normal);
        app.app_settings.ak820_os_indicator.enabled = false;
        app.tick_ak820_os_indicator(&egui::Context::default());
        assert!(matches!(
            app.ak820_os_indicator_runtime.status,
            IndicatorStatus::Disabled
        ));
        assert!(requests.requests().is_empty());
    }

    fn render_indicator_rows(app: &mut EntropyApp, size: egui::Vec2) -> Vec<String> {
        let ctx = egui::Context::default();
        let mut bounds = Vec::new();
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            },
            |root_ui| {
                egui::CentralPanel::default().show_inside(root_ui, |ui| {
                    let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
                    let width = metrics.settings_row_content_width();
                    let height = metrics.settings_row_height();
                    for row in 13..=16 {
                        bounds.push(egui::Rect::from_min_size(
                            ui.cursor().min,
                            egui::vec2(width, height),
                        ));
                        app.draw_indicator_row_with_save(ui, row, width, height, metrics, |_| {
                            panic!("render must not save")
                        });
                    }
                });
            },
        );
        let mut texts = Vec::new();
        for clipped in output.shapes {
            if let egui::Shape::Text(text) = clipped.shape {
                let rect = text.visual_bounding_rect();
                assert!(
                    clipped.clip_rect.expand(1.0).contains_rect(rect),
                    "clipped text: {}",
                    text.galley.text()
                );
                assert!(
                    bounds.iter().any(|row| row.expand(1.0).contains_rect(rect)),
                    "text outside row: {}",
                    text.galley.text()
                );
                if [
                    "LED when English is detected",
                    "LED when Russian is detected",
                    "LED: английская раскладка",
                    "LED: русская раскладка",
                ]
                .contains(&text.galley.text())
                {
                    let row = bounds
                        .iter()
                        .find(|row| row.expand(1.0).contains_rect(rect))
                        .unwrap();
                    assert!(
                        rect.right() <= row.right() - 168.0,
                        "label overlaps dropdown: {}",
                        text.galley.text()
                    );
                }
                texts.push(text.galley.text().to_owned());
            }
        }
        texts
    }

    #[test]
    fn indicator_rows_render_without_saving_or_inventing_mappings() {
        let (mut app, jobs, _finished, requests) = sampled_app();
        app.app_settings.language = crate::i18n::Language::English;
        app.app_settings.ak820_os_indicator.layouts.clear();
        for language in [
            crate::i18n::Language::English,
            crate::i18n::Language::Russian,
        ] {
            app.app_settings.language = language;
            for size in [egui::vec2(800.0, 500.0), egui::vec2(1280.0, 800.0)] {
                let texts = render_indicator_rows(&mut app, size);
                let unset = if language == crate::i18n::Language::Russian {
                    "Не задано"
                } else {
                    "Not set"
                };
                assert_eq!(
                    texts.iter().filter(|text| text.as_str() == unset).count(),
                    2
                );
            }
            assert!(app.app_settings.ak820_os_indicator.layouts.is_empty());
            assert!(jobs.try_recv().is_err());
            assert!(requests.requests().is_empty());
        }
    }

    #[test]
    fn unknown_and_unmapped_never_queue_a_transition_from_active_modes() {
        for previous in [Mode::Off, Mode::On] {
            for layout in [None, Some("fr"), Some("ru")] {
                let (mut app, jobs, _finished, requests) = sampled_app();
                app.app_settings
                    .ak820_os_indicator
                    .layouts
                    .insert("fr".into(), Mode::Normal);
                let generation = app.connection_generation;
                let runtime = &mut app.ak820_os_indicator_runtime;
                runtime.scheduler.bind(generation);
                runtime.scheduler.completed(0, previous, true);
                runtime.previous = Some(previous);
                runtime.sample = Some(Sample {
                    generation,
                    layout,
                    sampled: Instant::now(),
                });
                runtime.checked_at = Some(Instant::now());
                app.tick_ak820_os_indicator(&egui::Context::default());
                assert_eq!(app.ak820_os_indicator_runtime.previous, Some(previous));
                assert!(jobs.try_recv().is_err());
                assert!(requests.requests().is_empty());
                assert_eq!(
                    app.app_settings.ak820_os_indicator.layouts.get("fr"),
                    Some(&Mode::Normal)
                );
                app.app_settings
                    .ak820_os_indicator
                    .layouts
                    .insert("ru".into(), Mode::Normal);
                let runtime = &mut app.ak820_os_indicator_runtime;
                runtime.sample = Some(Sample {
                    generation,
                    layout: Some("ru"),
                    sampled: Instant::now(),
                });
                runtime.checked_at = Some(Instant::now());
                app.tick_ak820_os_indicator(&egui::Context::default());
                assert!(matches!(
                    jobs.try_recv(),
                    Ok(Job::Write {
                        mode: Mode::Normal,
                        ..
                    })
                ));
                assert!(requests.requests().is_empty());
            }
        }
    }

    #[test]
    fn explicit_mapping_selection_and_tooltips_preserve_intent() {
        let (mut app, jobs, _finished, requests) = sampled_app();
        app.app_settings.ak820_os_indicator.layouts.remove("ru");
        assert!(!select_mapping(
            &mut app.app_settings.ak820_os_indicator,
            "ru",
            0
        ));
        assert!(!app
            .app_settings
            .ak820_os_indicator
            .layouts
            .contains_key("ru"));
        assert!(select_mapping(
            &mut app.app_settings.ak820_os_indicator,
            "ru",
            3
        ));
        assert_eq!(
            app.app_settings.ak820_os_indicator.layouts.get("ru"),
            Some(&Mode::On)
        );
        assert!(jobs.try_recv().is_err());
        assert!(requests.requests().is_empty());
        for fragment in [
            "Normal: firmware's ordinary behavior",
            "LED off: unlit",
            "LED on: lit",
            "breathing: slow breathing",
        ] {
            assert!(mode_tooltip(false).contains(fragment));
        }
        for fragment in [
            "Обычно: поведение прошивки",
            "LED выключен: не горит",
            "LED включён: горит",
            "дыхание: медленное дыхание",
        ] {
            assert!(mode_tooltip(true).contains(fragment));
        }
    }

    #[test]
    fn mapping_widget_ids_stay_stable_and_distinct() {
        let ctx = egui::Context::default();
        let mut ids = Vec::new();
        for _ in 0..2 {
            let _ = ctx.run_ui(egui::RawInput::default(), |root_ui| {
                egui::CentralPanel::default().show_inside(root_ui, |ui| {
                    ids.push((mapping_widget_id(ui, "en"), mapping_widget_id(ui, "ru")));
                });
            });
        }
        assert_eq!(ids[0], ids[1]);
        assert_ne!(ids[0].0, ids[0].1);
    }

    #[test]
    fn detection_and_saved_mappings_are_independent_across_pause_and_target_change() {
        let (mut app, jobs, _finished, requests) = sampled_app();
        app.app_settings.language = crate::i18n::Language::English;
        app.app_settings
            .ak820_os_indicator
            .layouts
            .insert("ru".into(), Mode::On);
        let ctx = egui::Context::default();
        let generation = app.connection_generation;
        for (layout, mode, text) in [
            ("en", Mode::Off, "Last observed: English."),
            ("ru", Mode::On, "Last observed: Russian."),
        ] {
            let runtime = &mut app.ak820_os_indicator_runtime;
            runtime.scheduler.bind(generation);
            runtime.scheduler.completed(0, mode, true);
            runtime.previous = Some(mode);
            runtime.sample = Some(Sample {
                generation,
                layout: Some(layout),
                sampled: Instant::now(),
            });
            runtime.checked_at = Some(Instant::now());
            app.tick_ak820_os_indicator(&ctx);
            assert_eq!(
                status_label(app.ak820_os_indicator_runtime.status, false),
                text
            );
            let texts = render_indicator_rows(&mut app, egui::vec2(800.0, 500.0));
            assert!(texts.iter().any(|value| value == text));
            assert!(texts.iter().any(|value| value == "LED off"));
            assert!(texts.iter().any(|value| value == "LED on"));
            assert!(texts
                .iter()
                .any(|value| value == "Saved choices above control the LED, not the layout."));
            app.app_settings.language = crate::i18n::Language::Russian;
            let russian = render_indicator_rows(&mut app, egui::vec2(800.0, 500.0));
            let detected = if layout == "en" {
                "Последнее наблюдение: английская."
            } else {
                "Последнее наблюдение: русская."
            };
            assert!(russian.iter().any(|value| value == detected));
            assert!(russian.iter().any(|value| value == "LED выключен"));
            assert!(russian.iter().any(|value| value == "LED включён"));
            app.app_settings.language = crate::i18n::Language::English;
            assert_eq!(
                app.app_settings.ak820_os_indicator.layouts.get("en"),
                Some(&Mode::Off)
            );
            assert_eq!(
                app.app_settings.ak820_os_indicator.layouts.get("ru"),
                Some(&Mode::On)
            );
            assert!(jobs.try_recv().is_err());
        }
        {
            let runtime = &mut app.ak820_os_indicator_runtime;
            runtime.scheduler.completed(0, Mode::Normal, true);
            runtime.previous = Some(Mode::Normal);
            runtime.sample = Some(Sample {
                generation,
                layout: None,
                sampled: Instant::now(),
            });
            runtime.checked_at = Some(Instant::now());
        }
        app.tick_ak820_os_indicator(&ctx);
        assert!(render_indicator_rows(&mut app, egui::vec2(800.0, 500.0))
            .iter()
            .any(|value| value == "OS layout unavailable."));
        assert!(jobs.try_recv().is_err());
        app.unlock_open = true;
        app.tick_ak820_os_indicator(&ctx);
        assert!(matches!(
            app.ak820_os_indicator_runtime.status,
            IndicatorStatus::Paused
        ));
        assert!(
            status_label(app.ak820_os_indicator_runtime.status, false).contains("not refreshed")
        );
        assert!(render_indicator_rows(&mut app, egui::vec2(800.0, 500.0))
            .iter()
            .any(|value| value.contains("not refreshed")));
        assert_eq!(
            app.app_settings.ak820_os_indicator.layouts.get("en"),
            Some(&Mode::Off)
        );
        assert_eq!(
            app.app_settings.ak820_os_indicator.layouts.get("ru"),
            Some(&Mode::On)
        );
        assert!(jobs.try_recv().is_err());
        assert!(requests.requests().is_empty());
        assert_eq!(app.connection_generation, generation);
        assert_eq!(app.selected_device, Some(0));
        assert_eq!(
            app.app_settings.ak820_os_indicator.layouts.get("en"),
            Some(&Mode::Off)
        );
        assert_eq!(
            app.app_settings.ak820_os_indicator.layouts.get("ru"),
            Some(&Mode::On)
        );
        app.unlock_open = false;
        {
            let runtime = &mut app.ak820_os_indicator_runtime;
            runtime.scheduler.bind(generation);
            runtime.scheduler.completed(0, Mode::Off, true);
            runtime.previous = Some(Mode::Off);
            runtime.sample = Some(Sample {
                generation,
                layout: Some("en"),
                sampled: Instant::now(),
            });
            runtime.checked_at = Some(Instant::now());
        }
        app.tick_ak820_os_indicator(&ctx);
        assert!(render_indicator_rows(&mut app, egui::vec2(800.0, 500.0))
            .iter()
            .any(|value| value == "Last observed: English."));
        app.unlock_open = true;
        app.connection_generation = generation.wrapping_add(1);
        app.tick_ak820_os_indicator(&ctx);
        assert!(matches!(
            app.ak820_os_indicator_runtime.status,
            IndicatorStatus::Paused
        ));
        assert!(jobs.try_recv().is_err());
        assert!(requests.requests().is_empty());
        app.app_settings.ak820_os_indicator.enabled = false;
        app.tick_ak820_os_indicator(&ctx);
        assert!(render_indicator_rows(&mut app, egui::vec2(800.0, 500.0))
            .iter()
            .any(|value| value == "Helper is off."));
        app.app_settings.ak820_os_indicator.enabled = true;
        app.unlock_open = false;
        app.hid_device = None;
        app.tick_ak820_os_indicator(&ctx);
        assert!(render_indicator_rows(&mut app, egui::vec2(800.0, 500.0))
            .iter()
            .any(|value| value.contains("not refreshed")));
        for layout in [None, Some("unknown")] {
            assert!(matches!(
                layout_status(layout),
                IndicatorStatus::LayoutUnavailable
            ));
        }
        for ru in [false, true] {
            for mode in [Mode::Normal, Mode::Off, Mode::On, Mode::Breathing] {
                assert!(!mode_label(mode, ru).is_empty());
            }
        }
    }

    #[test]
    fn status_row_fits_scrolled_virtual_settings_viewport() {
        let (mut app, jobs, _finished, requests) = sampled_app();
        for language in [
            crate::i18n::Language::English,
            crate::i18n::Language::Russian,
        ] {
            app.app_settings.language = language;
            for size in [egui::vec2(800.0, 500.0), egui::vec2(1280.0, 800.0)] {
                for status in [
                    IndicatorStatus::Ready,
                    IndicatorStatus::WorkerStopped,
                    IndicatorStatus::DeviceError,
                    IndicatorStatus::Disabled,
                    IndicatorStatus::Paused,
                    IndicatorStatus::LayoutDetected(false),
                    IndicatorStatus::LayoutDetected(true),
                    IndicatorStatus::LayoutUnavailable,
                ] {
                    app.ak820_os_indicator_runtime.status = status;
                    let ctx = egui::Context::default();
                    let mut status_rect = egui::Rect::NOTHING;
                    let mut viewport = egui::Rect::NOTHING;
                    let output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                            ..Default::default()
                        },
                        |root_ui| {
                            egui::CentralPanel::default().show_inside(root_ui, |ui| {
                                let metrics =
                                    crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
                                let offset = metrics.settings_row_height() * 16.0;
                                ui.ctx().data_mut(|d| {
                                    d.insert_persisted(
                                        ui.id().with(("app_settings", "smooth_offset")),
                                        offset,
                                    );
                                    d.insert_persisted(
                                        ui.id().with(("app_settings", "smooth_target")),
                                        offset,
                                    );
                                });
                                let list = allocate_adaptive_settings_list_viewport(
                                    ui,
                                    "app_settings",
                                    metrics,
                                    17,
                                    metrics.value(44.0),
                                );
                                assert!(list.first_visible_row <= 16 && list.last_visible_row > 16);
                                viewport = list.viewport;
                                status_rect = egui::Rect::from_min_size(
                                    list.content_rect.min
                                        + egui::vec2(
                                            0.0,
                                            (16 - list.first_visible_row) as f32 * list.row_height,
                                        ),
                                    egui::vec2(list.row_content_width, list.row_height),
                                );
                                crate::ui_style::allocate_ui_at_rect(ui, status_rect, |ui| {
                                    ui.set_clip_rect(list.viewport);
                                    app.draw_indicator_row_with_save(
                                        ui,
                                        16,
                                        list.row_content_width,
                                        list.row_height,
                                        metrics,
                                        |_| panic!("render must not save"),
                                    );
                                });
                            });
                        },
                    );
                    let mut found = false;
                    for clipped in output.shapes {
                        if let egui::Shape::Text(text) = clipped.shape {
                            let rect = text.visual_bounding_rect();
                            assert!(status_rect.expand(1.0).contains_rect(rect));
                            assert!(viewport.expand(1.0).contains_rect(rect));
                            assert!(clipped.clip_rect.expand(1.0).contains_rect(rect));
                            found = true;
                        }
                    }
                    assert!(found);
                }
            }
        }
        assert!(jobs.try_recv().is_err());
        assert!(requests.requests().is_empty());
    }

    #[test]
    fn unchanged_layout_samples_keep_hid_and_ui_availability() {
        let (mut app, jobs, finished, requests) = sampled_app();
        let runtime = &mut app.ak820_os_indicator_runtime;
        runtime.scheduler.bind(app.connection_generation);
        runtime.scheduler.completed(0, Mode::Off, true);
        runtime.previous = Some(Mode::Off);
        let ctx = egui::Context::default();
        app.tick_ak820_os_indicator(&ctx);
        let Job::Sample {
            generation, queued, ..
        } = jobs.try_recv().unwrap()
        else {
            panic!("layout sampling must not borrow HID");
        };
        assert!(app.hid_device.as_ref().unwrap().ak820_editing_ready());
        assert!(!app.ak820_os_indicator_owns_hid());
        finished
            .send(WorkerResult::Sample(Sample {
                generation,
                layout: Some("en"),
                sampled: queued,
            }))
            .unwrap();
        app.poll_ak820_os_indicator_task();
        app.tick_ak820_os_indicator(&ctx);
        assert!(app.hid_device.as_ref().unwrap().ak820_editing_ready());
        assert!(!app.ak820_os_indicator_owns_hid());
        if let Ok(job) = jobs.try_recv() {
            assert!(matches!(job, Job::Sample { .. }));
        }
        assert!(requests.requests().is_empty());
    }

    #[test]
    fn changed_sample_borrows_only_after_current_write_gates() {
        let (mut app, jobs, finished, requests) = sampled_app();
        let ctx = egui::Context::default();
        app.tick_ak820_os_indicator(&ctx);
        let Job::Sample {
            generation, queued, ..
        } = jobs.try_recv().unwrap()
        else {
            panic!("expected sample");
        };
        finished
            .send(WorkerResult::Sample(Sample {
                generation,
                layout: Some("en"),
                sampled: queued,
            }))
            .unwrap();
        app.poll_ak820_os_indicator_task();
        app.tick_ak820_os_indicator(&ctx);
        let Job::Write { mode, hid, .. } = jobs.try_recv().unwrap() else {
            panic!("changed mode requires exclusive write owner");
        };
        assert_eq!(mode, Mode::Off);
        assert!(app.hid_device.is_none());
        assert!(app.ak820_os_indicator_owns_hid());
        assert!(requests.requests().is_empty());
        drop(hid);
    }

    #[test]
    fn stopped_worker_returns_the_borrowed_hid_without_writing() {
        let (mut app, jobs, _finished, requests) = sampled_app();
        drop(jobs);
        app.ak820_os_indicator_runtime.sample = Some(Sample {
            generation: app.connection_generation,
            layout: Some("en"),
            sampled: Instant::now(),
        });
        app.tick_ak820_os_indicator(&egui::Context::default());
        assert!(app.hid_device.as_ref().unwrap().ak820_editing_ready());
        assert!(!app.ak820_os_indicator_owns_hid());
        assert!(app.ak820_os_indicator_runtime.scheduler.failed());
        assert!(matches!(
            app.ak820_os_indicator_runtime.status,
            IndicatorStatus::WorkerStopped
        ));
        assert!(requests.requests().is_empty());
    }

    #[test]
    fn completed_loan_restores_only_the_matching_connection() {
        for stale in [false, true] {
            let (mut app, jobs, finished, requests) = sampled_app();
            app.ak820_os_indicator_runtime.sample = Some(Sample {
                generation: app.connection_generation,
                layout: Some("en"),
                sampled: Instant::now(),
            });
            app.tick_ak820_os_indicator(&egui::Context::default());
            let Job::Write {
                hid,
                generation,
                mode,
                layout,
                ..
            } = jobs.try_recv().unwrap()
            else {
                panic!("expected write loan");
            };
            if stale {
                app.connection_generation += 1;
            }
            finished
                .send(WorkerResult::Write(Box::new(Finished {
                    hid: *hid,
                    generation,
                    mode,
                    layout,
                    result: Ok(()),
                    sent: true,
                })))
                .unwrap();
            app.poll_ak820_os_indicator_task();
            assert!(!app.ak820_os_indicator_owns_hid());
            assert_eq!(app.hid_device.is_some(), !stale);
            assert!(requests.requests().is_empty());
        }
    }

    #[test]
    fn stale_sample_latches_failure_without_a_hid_loan() {
        let (mut app, jobs, _finished, requests) = sampled_app();
        let runtime = &mut app.ak820_os_indicator_runtime;
        runtime.sample = Some(Sample {
            generation: app.connection_generation,
            layout: Some("en"),
            sampled: Instant::now() - Duration::from_secs(3),
        });
        app.tick_ak820_os_indicator(&egui::Context::default());
        assert!(app.ak820_os_indicator_runtime.scheduler.failed());
        assert!(app.hid_device.is_some());
        assert!(!app.ak820_os_indicator_owns_hid());
        assert!(matches!(jobs.try_recv(), Err(mpsc::TryRecvError::Empty)));
        assert!(requests.requests().is_empty());
    }

    #[test]
    fn competing_work_discards_sample_before_borrowing_hid() {
        let (mut app, jobs, finished, requests) = sampled_app();
        let ctx = egui::Context::default();
        app.tick_ak820_os_indicator(&ctx);
        let Job::Sample {
            generation, queued, ..
        } = jobs.try_recv().unwrap()
        else {
            panic!("expected sample");
        };
        app.unlock_open = true;
        app.tick_ak820_os_indicator(&ctx);
        finished
            .send(WorkerResult::Sample(Sample {
                generation,
                layout: Some("en"),
                sampled: queued,
            }))
            .unwrap();
        app.poll_ak820_os_indicator_task();
        app.unlock_open = false;
        app.tick_ak820_os_indicator(&ctx);
        assert!(matches!(jobs.try_recv().unwrap(), Job::Sample { .. }));
        assert!(app.hid_device.is_some());
        assert!(!app.ak820_os_indicator_owns_hid());
        assert!(requests.requests().is_empty());
    }

    #[test]
    fn rgb_scene_tab_survives_temporary_os_indicator_hid_borrow() {
        let mut app = EntropyApp::new_inert_for_test();
        let (hid, _requests) = crate::hid::HidDevice::test_beta3_ak820_device(
            ak820_beta1_profiles::ProfileId::Main,
            true,
        );
        app.hid_device = Some(hid);
        let selected = 2;
        assert_eq!(
            super::super::rgb_settings_ui::rgb_section_for_availability(selected, true),
            selected
        );

        let borrowed = app.begin_test_indicator_hid_loan();
        let during_borrow = app.ak820_os_indicator_display_connected();
        assert!(during_borrow);
        assert_eq!(
            super::super::rgb_settings_ui::rgb_section_for_availability(selected, during_borrow),
            selected
        );

        app.ak820_os_indicator_runtime.in_flight = false;
        app.hid_device = Some(borrowed);
        assert!(!app.ak820_os_indicator_owns_hid());
        assert_eq!(
            super::super::rgb_settings_ui::rgb_section_for_availability(selected, true),
            selected
        );
    }

    #[test]
    fn indicator_loan_preserves_only_actual_direct_capability() {
        for (profile, direct) in [
            (ak820_beta1_profiles::ProfileId::Main, true),
            (ak820_beta1_profiles::ProfileId::Typing, false),
        ] {
            let mut app = EntropyApp::new_inert_for_test();
            let (hid, requests) = crate::hid::HidDevice::test_beta3_ak820_device(profile, true);
            let before = hid
                .ak820_image()
                .is_some_and(|image| image.profile().is_some() && image.rgb_modes().contains(&1));
            assert_eq!(before, direct);
            let normal_section =
                super::super::rgb_settings_ui::rgb_section_for_availability(2, before);
            app.hid_device = Some(hid);
            let borrowed = app.begin_test_indicator_hid_loan();
            assert!(app.ak820_os_indicator_display_connected());
            assert_eq!(app.ak820_os_indicator_display_rgb_direct(), direct);
            assert_eq!(
                super::super::rgb_settings_ui::rgb_section_for_availability(
                    2,
                    app.ak820_os_indicator_display_rgb_direct()
                ),
                normal_section
            );
            assert_eq!(normal_section, if direct { 2 } else { 0 });
            app.connection_generation = app.connection_generation.wrapping_add(1);
            assert!(!app.ak820_os_indicator_display_rgb_direct());
            assert!(requests.requests().is_empty());
            drop(borrowed);
        }
    }

    #[test]
    fn failed_session_cannot_handoff_or_hide_its_error() {
        let mut app = EntropyApp::new_inert_for_test();
        let (hid, requests) = crate::hid::HidDevice::test_ak820_device(true);
        hid.ak820_fail(&anyhow::anyhow!("synthetic timeout"));
        app.hid_device = Some(hid);
        app.app_settings.ak820_os_indicator.enabled = true;
        app.ak820_os_indicator_runtime.status = IndicatorStatus::DeviceError;
        assert!(!app.ak820_os_indicator_handoff_allowed());
        app.tick_ak820_os_indicator(&egui::Context::default());
        assert!(matches!(
            app.ak820_os_indicator_runtime.status,
            IndicatorStatus::DeviceError
        ));
        assert!(requests.requests().is_empty());
    }

    #[test]
    fn failed_indicator_scheduler_blocks_handoff_without_a_device() {
        let mut app = EntropyApp::new_inert_for_test();
        assert!(app.ak820_os_indicator_handoff_allowed());
        app.ak820_os_indicator_runtime
            .scheduler
            .completed(0, Mode::Off, false);
        assert!(!app.ak820_os_indicator_handoff_allowed());
    }

    #[test]
    fn fault_cannot_be_regranted_by_time_or_repeated_same_endpoint() {
        let mut gate = FaultedEndpoints::default();
        gate.fault("same-path:instance-1".into());
        for _ in 0..10 {
            gate.observed(&["same-path:instance-1".into()]);
            assert!(gate.blocked("same-path:instance-1"));
        }
        gate.observed(&["same-path:instance-2".into()]);
        assert!(!gate.blocked("same-path:instance-2"));
        gate.fault("same-path:instance-2".into());
        gate.observed(&[]);
        assert!(!gate.blocked("same-path:instance-2"));
    }
}
