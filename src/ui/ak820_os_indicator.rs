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
        hid: crate::hid::HidDevice,
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
    Write(Finished),
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
                        WorkerResult::Write(Finished {
                            hid,
                            generation,
                            mode,
                            layout,
                            result,
                            sent: true,
                        })
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
                runtime.sampling = false;
                if sample.generation == self.connection_generation
                    && runtime.checked_at == Some(sample.sampled)
                {
                    runtime.sample = Some(sample);
                }
                return;
            }
            WorkerResult::Write(result) => result,
        };
        runtime.in_flight = false;
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
        } else if let Some(layout) = result.layout {
            IndicatorStatus::LayoutDetected(layout == "ru")
        } else {
            IndicatorStatus::LayoutUnavailable
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
            if self.app_settings.ak820_os_indicator.enabled
                && !matches!(
                    runtime.status,
                    IndicatorStatus::DeviceError | IndicatorStatus::WorkerStopped
                )
            {
                runtime.status = IndicatorStatus::Paused;
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
            let mode = self.app_settings.ak820_os_indicator.mode(sample.layout);
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
                runtime.loan_selected = self.selected_device;
                runtime.in_flight = true;
                let job = Job::Write {
                    hid,
                    generation: sample.generation,
                    mode,
                    layout: sample.layout,
                    sampled: sample.sampled,
                };
                if let Err(error) = runtime.worker.as_ref().unwrap().send(job) {
                    if let Job::Write { hid, .. } = error.0 {
                        self.hid_device = Some(hid);
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
            } else if let Some(layout) = sample.layout {
                IndicatorStatus::LayoutDetected(layout == "ru")
            } else {
                IndicatorStatus::LayoutUnavailable
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
                    save_app_settings(&self.app_settings);
                }
            }
            14 | 15 => {
                let (key, label) = if row == 14 {
                    ("en", tr("Английская раскладка", "English layout"))
                } else {
                    ("ru", tr("Русская раскладка", "Russian layout"))
                };
                let mode = self
                    .app_settings
                    .ak820_os_indicator
                    .layouts
                    .entry(key.into())
                    .or_default();
                let values = [Mode::Normal, Mode::Off, Mode::On, Mode::Breathing];
                let labels: Vec<String> = values
                    .iter()
                    .map(|value| mode_label(*value, ru).to_owned())
                    .collect();
                let selected = values.iter().position(|value| *value == *mode).unwrap_or(0);
                let enabled = self.app_settings.ak820_os_indicator.enabled;
                let dropdown_id = ui.make_persistent_id(("ak820_os_layout", key));
                let mut changed = false;
                crate::ui_style::settings_list_row(
                    ui,
                    content_width,
                    row_height,
                    label,
                    enabled,
                    control_width,
                    |ui| {
                        if let (_, Some(index)) = crate::ui_style::modern_dropdown_select_sized(
                            ui,
                            dropdown_id,
                            &labels,
                            selected,
                            control_width,
                            row_height.min(40.0),
                            metrics.value(12.5),
                        ) {
                            *mode = values[index];
                            changed = true;
                        }
                    },
                );
                if changed {
                    save_app_settings(&self.app_settings);
                }
            }
            16 => {
                let status = status_label(self.ak820_os_indicator_runtime.status, ru);
                crate::ui_style::settings_list_row(
                    ui,
                    content_width,
                    row_height,
                    tr("Состояние", "Status"),
                    true,
                    content_width * 0.68,
                    |ui| {
                        ui.label(
                            RichText::new(status)
                                .size(metrics.value(12.5))
                                .color(app_muted_text(ui.visuals().dark_mode)),
                        );
                    },
                );
            }
            _ => {}
        }
    }

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
        (IndicatorStatus::Paused, false) => "Paused while the device is busy or unverified.",
        (IndicatorStatus::Paused, true) => "Пауза: устройство занято или не проверено.",
        (IndicatorStatus::LayoutDetected(false), false) => "English layout detected.",
        (IndicatorStatus::LayoutDetected(true), false) => "Russian layout detected.",
        (IndicatorStatus::LayoutDetected(false), true) => "Определена английская раскладка.",
        (IndicatorStatus::LayoutDetected(true), true) => "Определена русская раскладка.",
        (IndicatorStatus::LayoutUnavailable, false) => {
            "Layout unavailable; normal lighting is restored."
        }
        (IndicatorStatus::LayoutUnavailable, true) => {
            "Раскладка недоступна; включена обычная подсветка."
        }
    }
}

fn mode_label(mode: Mode, ru: bool) -> &'static str {
    match (mode, ru) {
        (Mode::Normal, false) => "Normal",
        (Mode::Normal, true) => "Обычный",
        (Mode::Off, false) => "Off",
        (Mode::Off, true) => "Выключен",
        (Mode::On, false) => "On",
        (Mode::On, true) => "Включён",
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
    let mut enumerated_devices = Vec::new();
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
            enumerated_devices = devices;
            let keys: Vec<String> = enumerated_devices.iter().map(endpoint_key).collect();
            faulted.observed(&keys);
            let candidate = if enumerated_devices.len() == 1 {
                enumerated_devices.first()
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
            let mode = settings.mode(detector.current());
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
