//! Opt-in OS indication borrows the selected verified HID owner. It never
//! opens a second handle, guesses a layout or retries a failed exchange.
use super::*;
use crate::ak820_os_indicator::{Mode, Scheduler, Settings, REFRESH_MS};
use std::time::{Duration, Instant};

struct Job {
    hid: crate::hid::HidDevice,
    generation: u64,
    settings: Settings,
    previous: Option<Mode>,
    queued: Instant,
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
}

pub(super) struct Runtime {
    clock: Instant,
    checked_at: Option<Instant>,
    scheduler: Scheduler,
    worker: Option<mpsc::Sender<Job>>,
    receiver: Option<mpsc::Receiver<Finished>>,
    in_flight: bool,
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
            in_flight: false,
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
                let layout = if job.settings.enabled {
                    detector.current()
                } else {
                    None
                };
                let mode = job.settings.mode(layout);
                let result = if job.queued.elapsed() > Duration::from_secs(2) {
                    Err("OS indication stopped: layout sample exceeded freshness limit".into())
                } else if mode != Mode::Normal || job.previous.is_some_and(|m| m != Mode::Normal) {
                    job.hid
                        .ak820_os_indicator_set(mode)
                        .map_err(|e| e.to_string())
                } else {
                    Ok(())
                };
                if finished
                    .send(Finished {
                        hid: job.hid,
                        generation: job.generation,
                        mode,
                        layout,
                        result,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        self.worker = Some(sender);
        self.receiver = Some(receiver);
    }
}

impl EntropyApp {
    pub(super) fn ak820_os_indicator_handoff_allowed(&self) -> bool {
        !self.ak820_os_indicator_runtime.scheduler.failed()
            && self.hid_device.as_ref().is_none_or(|hid| hid.ak820_failure().is_none())
    }

    pub(super) fn ak820_os_indicator_owns_hid(&self) -> bool {
        self.ak820_os_indicator_runtime.in_flight
    }

    pub(super) fn poll_ak820_os_indicator_task(&mut self) {
        if !self.ak820_os_indicator_runtime.in_flight {
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
        runtime.in_flight = false;
        if result.generation != self.connection_generation {
            runtime.previous = None;
            self.resume_pending_device_connect();
            return;
        }
        runtime.previous = Some(result.mode);
        runtime.scheduler.completed(
            runtime.clock.elapsed().as_millis() as u64,
            result.mode,
            result.result.is_ok(),
        );
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
            if self.app_settings.ak820_os_indicator.enabled
                && !matches!(runtime.status, IndicatorStatus::DeviceError | IndicatorStatus::WorkerStopped)
            {
                runtime.status = IndicatorStatus::Paused;
            }
            return;
        }
        if runtime.generation != self.connection_generation {
            runtime.generation = self.connection_generation;
            runtime.previous = None;
            runtime.checked_at = None;
            runtime.status = IndicatorStatus::Ready;
        }
        runtime.scheduler.bind(self.connection_generation);
        if !self.app_settings.ak820_os_indicator.enabled
            && runtime.previous.is_none_or(|m| m == Mode::Normal)
        {
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(REFRESH_MS));
        if runtime.checked_at.is_some_and(|at| {
            now.saturating_duration_since(at).as_millis() < u128::from(REFRESH_MS)
        }) {
            return;
        }
        // The scheduler failure latch prevents all further jobs in this session.
        if !runtime
            .scheduler
            .due(runtime.clock.elapsed().as_millis() as u64, Mode::On)
        {
            return;
        }
        runtime.start_worker();
        let Some(hid) = self.hid_device.take() else {
            return;
        };
        runtime.in_flight = true;
        runtime.checked_at = Some(now);
        let job = Job {
            hid,
            generation: self.connection_generation,
            settings: self.app_settings.ak820_os_indicator.clone(),
            previous: runtime.previous,
            queued: now,
        };
        if let Err(error) = runtime.worker.as_ref().unwrap().send(job) {
            self.hid_device = Some(error.0.hid);
            runtime.in_flight = false;
            runtime.scheduler.completed(
                runtime.clock.elapsed().as_millis() as u64,
                Mode::Normal,
                false,
            );
        }
        ctx.request_repaint_after(Duration::from_millis(20));
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
                        "Показывает активную раскладку на подсветке клавиатуры.",
                        "Shows the active keyboard layout through keyboard lighting.",
                    ))
                    .size(metrics.value(13.0))
                    .color(app_muted_text(dark)),
                );
                ui.add_space(metrics.value(24.0));
                let content_width = metrics
                    .settings_row_content_width()
                    .min(ui.available_width() - 8.0);
                egui::ScrollArea::vertical()
                    .id_salt("ak820_os_indicator_scroll")
                    .max_height(ui.available_height())
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_max_width(content_width);
                        ui.spacing_mut().item_spacing.y = 8.0;
                        let mut changed = false;
                        let switch_size = metrics.size(46.0, 24.0);
                        crate::ui_style::settings_list_row(
                            ui,
                            content_width,
                            metrics.settings_row_height(),
                            tr("Включить помощник", "Enable helper"),
                            true,
                            switch_size.x,
                            |ui| {
                                changed |= crate::ui_style::settings_switch_sized_stable(
                                    ui,
                                    "ak820_os_indicator_enabled",
                                    &mut self.app_settings.ak820_os_indicator.enabled,
                                    switch_size,
                                )
                                .changed();
                            },
                        );
                        for (key, label) in [
                            ("en", tr("Английская раскладка", "English layout")),
                            ("ru", tr("Русская раскладка", "Russian layout")),
                        ] {
                            let mode = self
                                .app_settings
                                .ak820_os_indicator
                                .layouts
                                .entry(key.into())
                                .or_default();
                            let values = [Mode::Normal, Mode::Off, Mode::On, Mode::Breathing];
                            let labels: Vec<String> = values
                                .iter()
                                .map(|v| mode_label(*v, ru).to_owned())
                                .collect();
                            let selected = values.iter().position(|v| *v == *mode).unwrap_or(0);
                            let width = (content_width * 0.44).clamp(148.0, 220.0);
                            let dropdown_id = ui.make_persistent_id(("ak820_os_layout", key));
                            crate::ui_style::settings_list_row(
                                ui,
                                content_width,
                                metrics.settings_row_height(),
                                label,
                                self.app_settings.ak820_os_indicator.enabled,
                                width,
                                |ui| {
                                    if let (_, Some(index)) =
                                        crate::ui_style::modern_dropdown_select_sized(
                                            ui,
                                            dropdown_id,
                                            &labels,
                                            selected,
                                            width,
                                            metrics.settings_row_height().min(40.0),
                                            metrics.value(12.5),
                                        )
                                    {
                                        *mode = values[index];
                                        changed = true;
                                    }
                                },
                            );
                        }
                        ui.add_space(metrics.value(14.0));
                        ui.label(
                            RichText::new(status_label(self.ak820_os_indicator_runtime.status, ru))
                                .size(metrics.value(13.0)),
                        );
                        ui.add_space(metrics.value(12.0));
                        if crate::ui_style::modern_button(
                            ui,
                            tr("Справка", "Help"),
                            metrics.size(104.0, 32.0),
                            true,
                        )
                        .clicked()
                        {
                            super::ak820_help_ui::open_section(ui.ctx(), "os-layout-helper");
                        }
                        let can_hide = self.app_settings.ak820_os_indicator.enabled
                            && !self.ak820_demo
                            && !self.headless
                            && crate::background_helper_allowed()
                            && self.ak820_os_indicator_handoff_allowed()
                            && !self.hid_write_lifecycle_busy()
                            && matches!(self.connect_state, ConnectState::Idle)
                            && !self.unlock_open
                            && self.pending_file_dialog.is_none();
                        if crate::ui_style::action_button(
                            ui,
                            tr(
                                "Закрыть интерфейс, оставить помощник",
                                "Close interface, keep helper",
                            ),
                            can_hide,
                        )
                        .clicked()
                        {
                            save_app_settings(&self.app_settings);
                            crate::request_background_helper();
                            self.force_close_requested = true;
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if crate::ui_style::action_button(
                            ui,
                            tr("Завершить приложение", "Quit application"),
                            !self.ak820_install_runtime.blocks_exit(),
                        )
                        .clicked()
                        {
                            crate::cancel_background_helper_request();
                            self.force_close_requested = true;
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        ui.label(
                            RichText::new(tr(
                                "Помощник работает, пока приложение запущено.",
                                "The helper runs while the application is open.",
                            ))
                            .size(metrics.value(12.5))
                            .color(app_muted_text(dark)),
                        );
                        if changed {
                            save_app_settings(&self.app_settings);
                        }
                    });
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
    let mut active: Option<(String, crate::hid::HidDevice, Mode)> = None;
    loop {
        let cycle_started = Instant::now();
        let settings = load_app_settings().ak820_os_indicator;
        let open_interface = read_single_instance_signal() != initial_signal;
        if open_interface || !settings.enabled {
            if let Some((_, hid, previous)) = active.take() {
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
        let devices = match DeviceManager::scan_usb_devices() {
            Ok(devices) => devices,
            Err(_) => {
                if let Some((key, _, _)) = active.take() {
                    faulted.fault(key);
                }
                // A scan error is not evidence of a physical disconnect.
                std::thread::sleep(Duration::from_millis(REFRESH_MS));
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
            .is_some_and(|(key, _, _)| candidate.is_none_or(|d| endpoint_key(d) != *key))
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
                                    active = Some((key, hid, Mode::Normal));
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
        if let Some((key, hid, previous)) = active.as_mut() {
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
            let result = if !fresh {
                Err(anyhow::anyhow!("OS helper sample expired"))
            } else if mode != Mode::Normal || *previous != Mode::Normal {
                hid.ak820_os_indicator_set(mode)
            } else {
                Ok(())
            };
            if result.is_err() {
                faulted.fault(key.clone());
                active = None;
            } else {
                *previous = mode;
            }
        }
        std::thread::sleep(
            Duration::from_millis(REFRESH_MS).saturating_sub(cycle_started.elapsed()),
        );
    }
}

#[cfg(test)]
mod background_tests {
    use super::*;
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
        assert!(matches!(app.ak820_os_indicator_runtime.status, IndicatorStatus::DeviceError));
        assert!(requests.requests().is_empty());
    }

    #[test]
    fn failed_indicator_scheduler_blocks_handoff_without_a_device() {
        let mut app = EntropyApp::new_inert_for_test();
        assert!(app.ak820_os_indicator_handoff_allowed());
        app.ak820_os_indicator_runtime.scheduler.completed(0, Mode::Off, false);
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
