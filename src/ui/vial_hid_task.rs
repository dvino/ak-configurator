use super::*;

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
pub(super) struct Ak820RgbWrite {
    pub(super) request_id: u64,
    pub(super) generation: u64,
    pub(super) device: crate::device::Device,
    pub(super) image: crate::ak820::ImageContract,
    pub(super) old: RgbSettingsState,
    pub(super) value: RgbSettingsState,
    pub(super) save: bool,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
pub(super) struct LayerNameWrite {
    pub(super) device: crate::device::Device,
    pub(super) device_key: String,
    pub(super) layout_id: String,
    pub(super) generation: u64,
    pub(super) layer: usize,
    pub(super) name: String,
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn layer_name_write_error(language: crate::i18n::Language, error: &str) -> String {
    use super::application_layout_runtime::app_layout_text;
    let reason = match error {
        "device changed" => app_layout_text(language, "устройство изменилось", "device changed"),
        "device is busy" => app_layout_text(language, "устройство занято", "device is busy"),
        "device is disconnected" => {
            app_layout_text(language, "устройство отключено", "device is disconnected")
        }
        "layer data is not ready" => {
            app_layout_text(language, "данные слоя не готовы", "layer data is not ready")
        }
        "layer name readback mismatch" => app_layout_text(
            language,
            "прочитанное имя слоя не совпадает",
            "layer name readback mismatch",
        ),
        _ => error,
    };
    format!(
        "{}: {reason}",
        app_layout_text(language, "Имя слоя не сохранено", "Layer name not saved")
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn report_stale_restore(ctx: &egui::Context, generation: u64, operation: &VialHidOperation) {
    if let VialHidOperation::Ak820Restore {
        request_id,
        request,
    } = operation
    {
        // Only the saved local safety reference survives a generation change.
        // Neither the old device handle nor its result is adopted.
        super::ak820_backup_inspection::restore_failed(
            ctx,
            *request_id,
            generation,
            request.safety_location(),
        );
    }
}

impl EntropyApp {
    #[cfg(not(target_arch = "wasm32"))]
    fn retire_unconfirmed_hid_loan(&mut self, hid: Option<&crate::hid::HidDevice>) {
        if let Some(retirement) = hid.and_then(|device| device.retirement_handle()) {
            retirement.retire();
        }
        self.retire_selected_qmk_hid_host_bridges();
        self.shared_hid_output = None;
        self.pending_device_connect = None;
        self.vial_unlocked = None;
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn validate_selected_hid_owner(&mut self) -> bool {
        let Some(hid) = self.hid_device.as_ref() else {
            return false;
        };
        let matches = self
            .selected_device
            .and_then(|index| self.device_manager.devices().get(index))
            .is_some_and(|target| hid.is_bound_to(target));
        if matches {
            return true;
        }
        if let Some(retirement) = hid.retirement_handle() {
            retirement.retire();
        }
        self.retire_selected_qmk_hid_host_bridges();
        self.hid_device = None;
        self.shared_hid_output = None;
        self.pending_device_connect = None;
        self.rgb_draft = None;
        self.vial_unlocked = None;
        self.status_msg = if self.app_settings.language == crate::i18n::Language::Russian {
            "Соединение с устройством изменилось. Прочитайте данные снова."
        } else {
            "Device connection changed. Read the data again."
        }
        .into();
        false
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn rgb_target_matches(&self, request: &Ak820RgbWrite) -> bool {
        request.generation == self.connection_generation
            && self
                .selected_device
                .and_then(|i| self.device_manager.devices().get(i))
                .is_some_and(|device| {
                    device.path == request.device.path
                        && device.instance_token == request.device.instance_token
                        && device.permits_hid_target(&request.device)
                })
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn cancel_rgb_for_exit(&mut self) {
        self.rgb_draft = None;
        if let Some(task) = &self.vial_hid_task {
            if matches!(
                task.operation,
                VialHidOperation::Ak820RgbWrite(_) | VialHidOperation::UnlockSession(_)
            ) {
                task.cancel
                    .store(true, std::sync::atomic::Ordering::Release);
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn begin_rgb_exit(&mut self) -> bool {
        if self.rgb_exit_pending
            || self.rgb_draft.is_some()
            || self.active_rgb_request().is_some()
            || self
                .vial_hid_task
                .as_ref()
                .is_some_and(|task| matches!(task.operation, VialHidOperation::UnlockSession(_)))
        {
            self.rgb_exit_pending = true;
            self.cancel_rgb_for_exit();
        }
        self.rgb_exit_pending
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn active_rgb_request(&self) -> Option<&Ak820RgbWrite> {
        match &self.vial_hid_task.as_ref()?.operation {
            VialHidOperation::Ak820RgbWrite(request) => Some(request),
            _ => None,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn start_rgb_draft(&mut self, ctx: &egui::Context) {
        let Some(request) = self.rgb_draft.take() else {
            return;
        };
        if !self.validate_selected_hid_owner() {
            return;
        }
        if self.exit_after_hid_write
            || self.settings_tab != SettingsTab::Rgb
            || !matches!(
                self.main_menu_tab,
                MainMenuTab::Settings | MainMenuTab::Advanced
            )
            || !self.rgb_target_matches(&request)
            || self.rgb_uncertain == Some(request.generation)
        {
            return;
        }
        let admitted = self.hid_device.as_ref().is_some_and(|hid| {
            hid.is_bound_to(&request.device)
                && hid.ak820_image() == Some(request.image)
                && hid
                    .ak820_prevalidate_rgb(
                        request.value.effect,
                        request.value.speed,
                        request.value.hue,
                        request.value.saturation,
                        request.value.brightness,
                        request.save,
                    )
                    .is_ok()
        });
        if !admitted {
            return;
        }
        if self.start_vial_hid_operation(ctx, VialHidOperation::Ak820RgbWrite(request.clone()))
            == VialHidTaskStart::Busy
        {
            self.rgb_draft = Some(request);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn selected_layer_names_uncertain(&self) -> bool {
        let key = self.application_layout_device_key();
        self.uncertain_layer_names.iter().any(|r| {
            key.as_ref() == Some(&r.device_key)
                && self
                    .selected_device
                    .and_then(|i| self.device_manager.devices().get(i))
                    .is_some_and(|d| {
                        d.path == r.device.path
                            && d.instance_token == r.device.instance_token
                            && d.permits_hid_target(&r.device)
                    })
        })
    }

    // Explicit consumers may recover uncertainty with GET only. Repaints never
    // call this, and the host Default remains the user's draft, not a read cache.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn confirm_uncertain_layer_names(&mut self) -> anyhow::Result<()> {
        let key = self.application_layout_device_key();
        let requests: Vec<_> = self
            .uncertain_layer_names
            .iter()
            .filter(|r| {
                key.as_ref() == Some(&r.device_key)
                    && self
                        .selected_device
                        .and_then(|i| self.device_manager.devices().get(i))
                        .is_some_and(|d| {
                            d.path == r.device.path
                                && d.instance_token == r.device.instance_token
                                && d.permits_hid_target(&r.device)
                        })
            })
            .cloned()
            .collect();
        if requests.is_empty() {
            return Ok(());
        }
        anyhow::ensure!(
            !self.hid_write_task_active(),
            layer_name_write_error(self.app_settings.language, "device is busy")
        );
        for request in requests {
            anyhow::ensure!(
                self.selected_device
                    .and_then(|i| self.device_manager.devices().get(i))
                    .is_some_and(|d| d.path == request.device.path
                        && d.instance_token == request.device.instance_token
                        && d.permits_hid_target(&request.device)),
                layer_name_write_error(self.app_settings.language, "device changed")
            );
            let hid = self.hid_device.as_ref().ok_or_else(|| {
                anyhow::anyhow!(layer_name_write_error(
                    self.app_settings.language,
                    "device is disconnected"
                ))
            })?;
            let actual = hid
                .get_qmk_setting_string(200 + request.layer as u16)
                .map_err(|error| {
                    anyhow::anyhow!(layer_name_write_error(
                        self.app_settings.language,
                        &error.to_string()
                    ))
                })?;
            if let Some(name) = self.layer_names.get_mut(request.layer) {
                *name = actual.clone();
            }
            if let Some(name) = self
                .layout
                .as_mut()
                .and_then(|l| l.layer_names.get_mut(request.layer))
            {
                *name = actual;
            }
            self.uncertain_layer_names.retain(|r| {
                r.device_key != request.device_key
                    || r.layer != request.layer
                    || r.generation != request.generation
                    || r.device.path != request.device.path
                    || r.device.instance_token != request.device.instance_token
            });
        }
        self.keycode_picker.layer_names = self.layer_names.clone();
        Ok(())
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn maybe_start_layer_name_write(&mut self, ctx: &egui::Context) {
        while let Some(request) = self.pending_layer_name_writes.front() {
            let target_matches = request.generation == self.connection_generation
                && self.application_layout_device_key().as_ref() == Some(&request.device_key)
                && self
                    .selected_device
                    .and_then(|i| self.device_manager.devices().get(i))
                    .is_some_and(|device| {
                        device.path == request.device.path
                            && device.instance_token == request.device.instance_token
                            && device.permits_hid_target(&request.device)
                    });
            if !target_matches {
                self.status_msg =
                    layer_name_write_error(self.app_settings.language, "device changed");
                self.pending_layer_name_writes.pop_front();
                continue;
            }
            if self.hid_device.is_none() && !self.hid_write_task_owner_active() {
                self.status_msg =
                    layer_name_write_error(self.app_settings.language, "device is disconnected");
                self.pending_layer_name_writes.pop_front();
                continue;
            }
            // Required cold reads belong to this queue, including during exit.
            // Ordinary background admission may be blocked by the queued write.
            match self.deferred_device_load.layer_status(request.layer) {
                DeferredLoadStatus::NotLoaded => {
                    if let Some(context) = self.deferred_device_load.context.clone() {
                        let layer = request.layer;
                        let operation = VialHidOperation::Deferred(
                            super::device_deferred_load::DeferredLoadRequest::Layer {
                                layer,
                                context,
                            },
                        );
                        if self.start_vial_hid_operation(ctx, operation)
                            == VialHidTaskStart::Started
                        {
                            self.deferred_device_load
                                .set_layer_status(layer, DeferredLoadStatus::Loading);
                        }
                        break;
                    }
                    self.status_msg = layer_name_write_error(
                        self.app_settings.language,
                        "layer data is not ready",
                    );
                    self.pending_layer_name_writes.pop_front();
                    continue;
                }
                DeferredLoadStatus::Loading => break,
                DeferredLoadStatus::Failed(_) => {
                    self.status_msg = layer_name_write_error(
                        self.app_settings.language,
                        "layer data is not ready",
                    );
                    self.pending_layer_name_writes.pop_front();
                    continue;
                }
                _ => {}
            }
            let operation = VialHidOperation::LayerNameWrite(request.clone());
            if self.start_vial_hid_operation(ctx, operation) == VialHidTaskStart::Started {
                self.pending_layer_name_writes.pop_front();
            }
            break;
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
const BATTERY_REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5 * 60);
#[cfg(not(target_arch = "wasm32"))]
const BATTERY_REFRESH_RETRY_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);
#[cfg(not(target_arch = "wasm32"))]
const BATTERY_INCOMPLETE_REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(10);
#[cfg(not(target_arch = "wasm32"))]
const INITIAL_BATTERY_REFRESH_DELAY: std::time::Duration = std::time::Duration::from_secs(2);

#[cfg(not(target_arch = "wasm32"))]
fn battery_refresh_delay(battery: Option<crate::hid::BatteryHalves>) -> std::time::Duration {
    if battery.is_some_and(crate::hid::BatteryHalves::is_complete) {
        BATTERY_REFRESH_INTERVAL
    } else {
        BATTERY_INCOMPLETE_REFRESH_INTERVAL
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
pub(super) enum VialHidOperation {
    Ak820RgbWrite(Ak820RgbWrite),
    Ak820BackupCapture {
        request_id: u64,
    },
    Ak820Restore {
        request_id: u64,
        request: std::sync::Arc<super::ak820_restore_request::PreparedRestore>,
    },
    LayerNameWrite(LayerNameWrite),
    UnlockSession(std::sync::Arc<std::sync::Mutex<Option<VialUnlockStatus>>>),
    #[cfg(test)]
    UnlockStart,
    #[cfg(test)]
    UnlockPoll,
    Lock,
    Matrix {
        rows: usize,
        cols: usize,
        remember_ever_pressed: bool,
    },
    BatteryRefresh,
    KeyWrite {
        layer: usize,
        key_index: usize,
        row: u8,
        col: u8,
        old_binding: crate::keyboard::KeyBinding,
        binding: crate::keyboard::KeyBinding,
        is_undo: bool,
    },
    EncoderWrite {
        layer: usize,
        encoder_visual_index: usize,
        encoder_index: u8,
        direction: u8,
        old_keycode: u16,
        keycode: u16,
        is_undo: bool,
    },
    MacroWrite {
        macros: Vec<Vec<u8>>,
        revision: u64,
    },
    BackgroundUpload {
        path: std::path::PathBuf,
        fallback: [u8; 3],
        scale: crate::app::StandbyBackgroundScale,
    },
    BackgroundClear,
    #[cfg(test)]
    BackgroundSpeed {
        old_percent: u16,
        percent: u16,
    },
    StartupImageUpload {
        path: std::path::PathBuf,
        fallback: [u8; 3],
    },
    StartupImageClear,
    PictogramLoad {
        preserve_editor: bool,
    },
    #[cfg(test)]
    PictogramUpload {
        library: crate::app::PictogramLibrary,
    },
    PictogramSlotUpload {
        library: crate::app::PictogramLibrary,
        kind: crate::app::PictogramKind,
        slot: usize,
    },
    Deferred(super::device_deferred_load::DeferredLoadRequest),
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
pub(super) struct VialUnlockStatus {
    pub(super) keys: Vec<(u8, u8)>,
    pub(super) counter: u8,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug)]
enum UnlockResult {
    Confirmed,
    Denied,
    TimedOut,
    Cancelled,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
struct UnlockBoundary(UnlockResult);
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug)]
enum UnlockFailureKind {
    Status,
    Start,
    Poll,
    TransportTimeout,
    Disconnected,
}
#[cfg(not(target_arch = "wasm32"))]
impl std::fmt::Display for UnlockFailureKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unlock {self:?}")
    }
}
#[cfg(not(target_arch = "wasm32"))]
impl std::error::Error for UnlockFailureKind {}
#[cfg(not(target_arch = "wasm32"))]
impl std::fmt::Display for UnlockBoundary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("unlock dispatch stopped")
    }
}
#[cfg(not(target_arch = "wasm32"))]
impl std::error::Error for UnlockBoundary {}

#[cfg(not(target_arch = "wasm32"))]
type UnlockInitialStatus = (bool, bool, Vec<(u8, u8)>);

#[cfg(not(target_arch = "wasm32"))]
trait UnlockIo {
    fn status(&mut self) -> anyhow::Result<UnlockInitialStatus>;
    fn start(&mut self) -> anyhow::Result<()>;
    fn poll(
        &mut self,
        guard: &mut dyn FnMut() -> anyhow::Result<()>,
    ) -> anyhow::Result<(bool, bool, u8)>;
}

#[cfg(not(target_arch = "wasm32"))]
impl UnlockIo for &crate::hid::HidDevice {
    fn status(&mut self) -> anyhow::Result<UnlockInitialStatus> {
        self.get_unlock_status_with_progress()
    }
    fn start(&mut self) -> anyhow::Result<()> {
        self.unlock_start()
    }
    fn poll(
        &mut self,
        guard: &mut dyn FnMut() -> anyhow::Result<()>,
    ) -> anyhow::Result<(bool, bool, u8)> {
        self.unlock_poll_guarded(guard)
    }
}

#[cfg(not(target_arch = "wasm32"))]
trait UnlockClock {
    fn now(&self) -> std::time::Duration;
    fn wait(&mut self, duration: std::time::Duration);
}

#[cfg(not(target_arch = "wasm32"))]
struct RealUnlockClock(std::time::Instant);
#[cfg(not(target_arch = "wasm32"))]
impl UnlockClock for RealUnlockClock {
    fn now(&self) -> std::time::Duration {
        self.0.elapsed()
    }
    fn wait(&mut self, duration: std::time::Duration) {
        std::thread::sleep(duration);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn run_unlock_session(
    io: &mut impl UnlockIo,
    clock: &mut impl UnlockClock,
    cancel: &std::sync::atomic::AtomicBool,
    status: &std::sync::Mutex<Option<VialUnlockStatus>>,
) -> anyhow::Result<UnlockResult> {
    use std::sync::atomic::Ordering;
    let cancelled = || cancel.load(Ordering::Acquire);
    if cancelled() {
        return Ok(UnlockResult::Cancelled);
    }
    let (unlocked, in_progress, keys) = io
        .status()
        .map_err(|e| e.context(UnlockFailureKind::Status))?;
    if cancelled() {
        return Ok(UnlockResult::Cancelled);
    }
    if unlocked && !in_progress {
        return Ok(UnlockResult::Confirmed);
    }
    if !in_progress {
        io.start()
            .map_err(|e| e.context(UnlockFailureKind::Start))?;
    }
    let deadline = clock.now() + std::time::Duration::from_secs(15);
    let mut latest = VialUnlockStatus {
        counter: u8::try_from(keys.len()).unwrap_or(u8::MAX).max(1),
        keys,
    };
    *status.lock().unwrap_or_else(|e| e.into_inner()) = Some(latest.clone());
    loop {
        clock.wait(std::time::Duration::from_millis(200));
        if cancelled() {
            return Ok(UnlockResult::Cancelled);
        }
        if clock.now() >= deadline {
            return Ok(UnlockResult::TimedOut);
        }
        let mut guard = || -> anyhow::Result<()> {
            if cancelled() {
                return Err(UnlockBoundary(UnlockResult::Cancelled).into());
            }
            if clock.now() >= deadline {
                return Err(UnlockBoundary(UnlockResult::TimedOut).into());
            }
            Ok(())
        };
        let (unlocked, in_progress, counter) = match io.poll(&mut guard) {
            Ok(value) => value,
            Err(error) if error.is::<UnlockBoundary>() => {
                return Ok(error
                    .downcast::<UnlockBoundary>()
                    .expect("checked boundary")
                    .0)
            }
            Err(error) => return Err(error.context(UnlockFailureKind::Poll)),
        };
        if cancelled() {
            return Ok(UnlockResult::Cancelled);
        }
        latest.counter = counter;
        *status.lock().unwrap_or_else(|e| e.into_inner()) = Some(latest.clone());
        if !in_progress {
            return Ok(if unlocked {
                UnlockResult::Confirmed
            } else {
                UnlockResult::Denied
            });
        }
    }
}

#[cfg(test)]
mod finite_unlock_tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    struct Clock(Duration);
    impl UnlockClock for Clock {
        fn now(&self) -> Duration {
            self.0
        }
        fn wait(&mut self, duration: Duration) {
            self.0 += duration;
        }
    }
    struct Fake<'a> {
        initial: (bool, bool),
        packets: Vec<&'static str>,
        fail: bool,
        cancel_after_poll: Option<&'a AtomicBool>,
    }
    impl UnlockIo for Fake<'_> {
        fn status(&mut self) -> anyhow::Result<UnlockInitialStatus> {
            self.packets.push("FE05");
            Ok((self.initial.0, self.initial.1, vec![(1, 2)]))
        }
        fn start(&mut self) -> anyhow::Result<()> {
            self.packets.push("FE06");
            Ok(())
        }
        fn poll(
            &mut self,
            guard: &mut dyn FnMut() -> anyhow::Result<()>,
        ) -> anyhow::Result<(bool, bool, u8)> {
            guard()?;
            self.packets.push("FE07");
            if self.fail {
                anyhow::bail!("injected fault");
            }
            if let Some(cancel) = self.cancel_after_poll {
                cancel.store(true, Ordering::Release);
                guard()?;
                self.packets.push("FE05");
            }
            Ok((true, true, 3))
        }
    }
    fn fake(initial: (bool, bool)) -> Fake<'static> {
        Fake {
            initial,
            packets: vec![],
            fail: false,
            cancel_after_poll: None,
        }
    }
    #[test]
    fn vial_unlock_finite_virtual_timeout_and_progress_without_frames() {
        let mut io = fake((false, false));
        let mut clock = Clock(Duration::ZERO);
        let status = std::sync::Mutex::new(None);
        let result =
            run_unlock_session(&mut io, &mut clock, &AtomicBool::new(false), &status).unwrap();
        assert!(matches!(result, UnlockResult::TimedOut));
        assert_eq!(clock.0, Duration::from_secs(15));
        assert_eq!(&io.packets[..2], &["FE05", "FE06"]);
        assert_eq!(io.packets.iter().filter(|p| **p == "FE07").count(), 74);
        assert_eq!(status.lock().unwrap().as_ref().unwrap().counter, 3);
    }
    #[test]
    fn vial_unlock_finite_already_unlocked_or_in_progress_never_starts() {
        for initial in [(true, false), (true, true), (false, true)] {
            let mut io = fake(initial);
            let result = run_unlock_session(
                &mut io,
                &mut Clock(Duration::ZERO),
                &AtomicBool::new(false),
                &std::sync::Mutex::new(None),
            )
            .unwrap();
            assert!(matches!(
                result,
                UnlockResult::Confirmed | UnlockResult::TimedOut
            ));
            assert!(!io.packets.contains(&"FE06"));
        }
    }
    #[test]
    fn vial_unlock_finite_stops_on_fault_and_guarded_cancellation() {
        let mut io = fake((false, false));
        io.fail = true;
        assert!(run_unlock_session(
            &mut io,
            &mut Clock(Duration::ZERO),
            &AtomicBool::new(false),
            &std::sync::Mutex::new(None)
        )
        .is_err());
        assert_eq!(io.packets, ["FE05", "FE06", "FE07"]);
        let cancel = AtomicBool::new(true);
        let mut io = fake((false, false));
        assert!(matches!(
            run_unlock_session(
                &mut io,
                &mut Clock(Duration::ZERO),
                &cancel,
                &std::sync::Mutex::new(None)
            )
            .unwrap(),
            UnlockResult::Cancelled
        ));
        assert!(io.packets.is_empty());
        cancel.store(false, Ordering::Release);
        let mut io = fake((false, false));
        io.cancel_after_poll = Some(&cancel);
        assert!(matches!(
            run_unlock_session(
                &mut io,
                &mut Clock(Duration::ZERO),
                &cancel,
                &std::sync::Mutex::new(None)
            )
            .unwrap(),
            UnlockResult::Cancelled
        ));
        assert_eq!(io.packets, ["FE05", "FE06", "FE07"]);
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl VialHidOperation {
    fn is_pictogram_upload(&self) -> bool {
        match self {
            Self::PictogramSlotUpload { .. } => true,
            #[cfg(test)]
            Self::PictogramUpload { .. } => true,
            _ => false,
        }
    }

    fn display_diagnostic_label(&self) -> Option<&'static str> {
        match self {
            Self::UnlockSession(_) => Some("unlock-session"),
            #[cfg(test)]
            Self::UnlockStart => Some("unlock-start"),
            #[cfg(test)]
            Self::UnlockPoll => Some("unlock-poll"),
            Self::Lock => Some("lock"),
            Self::PictogramLoad { .. } => Some("pictogram-load"),
            #[cfg(test)]
            Self::PictogramUpload { .. } => Some("pictogram-upload"),
            Self::PictogramSlotUpload { .. } => Some("pictogram-slot-upload"),
            _ => None,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
enum VialHidOutcome {
    UnlockSession(UnlockResult),
    Ak820RgbWritten,
    Ak820BackupCaptured(crate::ak820_backup_capture::CapturedBackup),
    Ak820Restored(super::ak820_restore_request::RestoreCompleted),
    LayerNameWritten(String),
    #[cfg(test)]
    UnlockStarted {
        unlocked: bool,
        keys: Vec<(u8, u8)>,
    },
    #[cfg(test)]
    UnlockPolled {
        unlocked: bool,
        in_progress: bool,
        counter: u8,
    },
    Locked,
    Matrix(Vec<bool>),
    Battery(Option<crate::hid::BatteryHalves>),
    KeyWritten,
    EncoderWritten,
    MacrosWritten,
    BackgroundUploaded(crate::app::standby_background::BackgroundUploadResult),
    BackgroundCleared,
    BackgroundCancelled {
        cleared: bool,
    },
    #[cfg(test)]
    BackgroundSpeedSet,
    StartupImageUploaded(crate::app::standby_background::StartupImageUploadResult),
    StartupImageCleared,
    PictogramsLoaded(crate::app::PictogramLibrary),
    PictogramsUploaded(crate::app::PictogramLibrary),
    Deferred(super::device_deferred_load::DeferredLoadPayload),
}

#[cfg(not(target_arch = "wasm32"))]
struct VialHidTaskResult {
    hid_device: Option<crate::hid::HidDevice>,
    operation: VialHidOperation,
    outcome: Result<VialHidOutcome, String>,
    disconnected: bool,
    unlock_failure: Option<UnlockFailureKind>,
    unlock_target: Option<crate::device::Device>,
    unlock_image: Option<crate::ak820::ImageContract>,
    generation: u64,
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) struct VialHidTask {
    receiver: std::sync::mpsc::Receiver<VialHidTaskResult>,
    operation: VialHidOperation,
    generation: u64,
    progress: std::sync::Arc<std::sync::atomic::AtomicU32>,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum VialHidTaskStart {
    Started,
    Busy,
    NoDevice,
}

/// Bounded causes contain no device paths, packet contents or settings values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CaptureFailure {
    OperationBusy,
    LeaseBusy,
    Disconnected,
    Cancelled,
    Storage,
    Reconciliation,
    Failed,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CaptureAvailability {
    Ready,
    Unavailable(CaptureFailure),
}

impl CaptureFailure {
    fn from_worker(error: &str, disconnected: bool) -> Self {
        if disconnected {
            return Self::Disconnected;
        }
        if error.contains("Backup capture cancelled") {
            return Self::Cancelled;
        }
        if error.contains("Storage") || error.contains("storage") {
            return Self::Storage;
        }
        if error.contains("getter differs")
            || error.contains("capabilities differ")
            || error.contains("changed during backup capture")
        {
            return Self::Reconciliation;
        }
        Self::Failed
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn run_vial_hid_operation_with_progress(
    hid: &crate::hid::HidDevice,
    operation: VialHidOperation,
    progress: &std::sync::atomic::AtomicU32,
    cancel: &std::sync::atomic::AtomicBool,
) -> anyhow::Result<VialHidOutcome> {
    match operation {
        VialHidOperation::UnlockSession(status) => run_unlock_session(
            &mut &*hid,
            &mut RealUnlockClock(std::time::Instant::now()),
            cancel,
            &status,
        )
        .map(VialHidOutcome::UnlockSession),
        VialHidOperation::Ak820RgbWrite(request) => {
            anyhow::ensure!(
                hid.ak820_image() == Some(request.image),
                "RGB image changed"
            );
            hid.ak820_prevalidate_rgb(
                request.value.effect,
                request.value.speed,
                request.value.hue,
                request.value.saturation,
                request.value.brightness,
                request.save,
            )?;
            hid.ak820_set_rgb_guarded(
                request.value.effect,
                request.value.speed,
                request.value.hue,
                request.value.saturation,
                request.value.brightness,
                request.save,
                cancel,
            )?;
            Ok(VialHidOutcome::Ak820RgbWritten)
        }
        VialHidOperation::Ak820Restore { request, .. } => request
            .run(hid, cancel, progress)
            .map(VialHidOutcome::Ak820Restored),
        VialHidOperation::Ak820BackupCapture { .. } => hid
            .ak820_capture_backup(cancel, progress)
            .map(VialHidOutcome::Ak820BackupCaptured),
        VialHidOperation::LayerNameWrite(request) => {
            let qsid = 200 + request.layer as u16;
            hid.set_qmk_setting_string(qsid, &request.name)?;
            let actual = hid.get_qmk_setting_string(qsid)?;
            let expected = crate::hid::HidDevice::normalized_qmk_setting_string(&request.name);
            anyhow::ensure!(actual == expected, "layer name readback mismatch");
            Ok(VialHidOutcome::LayerNameWritten(actual))
        }
        #[cfg(test)]
        VialHidOperation::UnlockStart => {
            let (unlocked, in_progress, keys) = hid.get_unlock_status_with_progress()?;
            if !unlocked && !in_progress {
                hid.unlock_start()?;
            }
            Ok(VialHidOutcome::UnlockStarted {
                unlocked: unlocked && !in_progress,
                keys,
            })
        }
        #[cfg(test)]
        VialHidOperation::UnlockPoll => {
            let (unlocked, in_progress, counter) = hid.unlock_poll()?;
            Ok(VialHidOutcome::UnlockPolled {
                unlocked,
                in_progress,
                counter,
            })
        }
        VialHidOperation::Lock => {
            hid.lock()?;
            Ok(VialHidOutcome::Locked)
        }
        VialHidOperation::Matrix { rows, cols, .. } => hid
            .get_switch_matrix(rows, cols)
            .map(VialHidOutcome::Matrix),
        VialHidOperation::BatteryRefresh => hid.get_battery_halves().map(VialHidOutcome::Battery),
        VialHidOperation::KeyWrite {
            layer,
            row,
            col,
            binding,
            ..
        } => match binding {
            crate::keyboard::KeyBinding::Vial(keycode) => hid
                .set_keycode(layer as u8, row, col, keycode)
                .map(|()| VialHidOutcome::KeyWritten),
            crate::keyboard::KeyBinding::Rmk(action) => hid
                .set_rmk_key_action(layer as u8, row, col, action)
                .map(|()| VialHidOutcome::KeyWritten),
        },
        VialHidOperation::EncoderWrite {
            layer,
            encoder_index,
            direction,
            keycode,
            ..
        } => hid
            .set_encoder(layer as u8, encoder_index, direction, keycode)
            .map(|()| VialHidOutcome::EncoderWritten),
        VialHidOperation::MacroWrite { macros, .. } => {
            hid.write_macros(&macros)?;
            Ok(VialHidOutcome::MacrosWritten)
        }
        VialHidOperation::BackgroundUpload {
            path,
            fallback,
            scale,
        } => hid
            .upload_standby_background(&path, fallback, scale, progress, cancel)
            .map(|result| match result {
                Some(upload) => VialHidOutcome::BackgroundUploaded(upload),
                None => VialHidOutcome::BackgroundCancelled {
                    cleared: progress.load(std::sync::atomic::Ordering::Relaxed) >= 150,
                },
            }),
        VialHidOperation::BackgroundClear => {
            hid.clear_standby_background()?;
            Ok(VialHidOutcome::BackgroundCleared)
        }
        #[cfg(test)]
        VialHidOperation::BackgroundSpeed { percent, .. } => {
            hid.set_standby_background_speed(percent)?;
            Ok(VialHidOutcome::BackgroundSpeedSet)
        }
        VialHidOperation::StartupImageUpload { path, fallback } => hid
            .upload_startup_image(&path, fallback, progress)
            .map(VialHidOutcome::StartupImageUploaded),
        VialHidOperation::StartupImageClear => {
            hid.clear_startup_image()?;
            Ok(VialHidOutcome::StartupImageCleared)
        }
        VialHidOperation::PictogramLoad { .. } => hid
            .load_pictograms(progress)
            .map(VialHidOutcome::PictogramsLoaded),
        #[cfg(test)]
        VialHidOperation::PictogramUpload { library } => hid
            .upload_pictograms(library, progress)
            .map(VialHidOutcome::PictogramsUploaded),
        VialHidOperation::PictogramSlotUpload {
            library,
            kind,
            slot,
        } => hid
            .upload_pictogram_slot(library, kind, slot, progress)
            .map(VialHidOutcome::PictogramsUploaded),
        VialHidOperation::Deferred(request) => {
            super::device_deferred_load::run_deferred_load(hid, &request)
                .map(VialHidOutcome::Deferred)
        }
    }
}

#[cfg(test)]
fn run_vial_hid_operation(
    hid: &crate::hid::HidDevice,
    operation: VialHidOperation,
) -> anyhow::Result<VialHidOutcome> {
    let progress = std::sync::atomic::AtomicU32::new(0);
    run_vial_hid_operation_with_progress(
        hid,
        operation,
        &progress,
        &std::sync::atomic::AtomicBool::new(false),
    )
}

impl EntropyApp {
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn schedule_initial_battery_refresh(&mut self) {
        self.next_battery_refresh_at = self
            .device_about_info
            .as_ref()
            .filter(|info| info.supports_battery_halves)
            .map(|_| std::time::Instant::now() + INITIAL_BATTERY_REFRESH_DELAY);
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn schedule_battery_refresh_for_result(
        &mut self,
        battery: Option<crate::hid::BatteryHalves>,
    ) {
        self.next_battery_refresh_at = self
            .device_about_info
            .as_ref()
            .filter(|info| info.supports_battery_halves)
            .map(|_| std::time::Instant::now() + battery_refresh_delay(battery));
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn maybe_start_periodic_battery_refresh(
        &mut self,
        ctx: &egui::Context,
        main_window_hidden_to_tray: bool,
    ) {
        if main_window_hidden_to_tray
            || self.bluetooth_reconnect_active()
            || !self
                .device_about_info
                .as_ref()
                .map(|info| info.supports_battery_halves)
                .unwrap_or(false)
        {
            return;
        }
        let Some(next_refresh_at) = self.next_battery_refresh_at else {
            return;
        };
        let now = std::time::Instant::now();
        if now < next_refresh_at {
            ctx.request_repaint_after(next_refresh_at.saturating_duration_since(now));
            return;
        }

        match self.start_vial_hid_operation(ctx, VialHidOperation::BatteryRefresh) {
            VialHidTaskStart::Started => {
                self.next_battery_refresh_at = None;
            }
            VialHidTaskStart::Busy => {
                ctx.request_repaint_after(std::time::Duration::from_secs(1));
            }
            VialHidTaskStart::NoDevice => {
                self.next_battery_refresh_at = Some(now + BATTERY_REFRESH_RETRY_INTERVAL);
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn vial_hid_task_active(&self) -> bool {
        self.vial_hid_task.is_some()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn vial_hid_background_layer_active(&self) -> bool {
        self.vial_hid_task.as_ref().is_some_and(|task| {
            matches!(
                &task.operation,
                VialHidOperation::Deferred(request) if request.is_background_layer()
            )
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn vial_hid_task_blocks_user_action(&self) -> bool {
        self.vial_hid_task.is_some() && !self.vial_hid_background_layer_active()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn cancel_background_upload(&self) {
        if let Some(task) = &self.vial_hid_task {
            if matches!(task.operation, VialHidOperation::BackgroundUpload { .. }) {
                task.cancel
                    .store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn standby_background_upload_progress(&self) -> Option<f32> {
        self.vial_hid_task.as_ref().and_then(|task| {
            matches!(task.operation, VialHidOperation::BackgroundUpload { .. })
                .then(|| task.progress.load(std::sync::atomic::Ordering::Relaxed) as f32 / 1000.0)
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn startup_image_upload_progress(&self) -> Option<f32> {
        self.vial_hid_task.as_ref().and_then(|task| {
            matches!(task.operation, VialHidOperation::StartupImageUpload { .. })
                .then(|| task.progress.load(std::sync::atomic::Ordering::Relaxed) as f32 / 1000.0)
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn another_hid_owner_or_write_is_pending(&self) -> bool {
        self.ak820_rgb_scene_owns_hid()
            || self.layer_write_task.is_some()
            || self.combo_write_task.is_some()
            || self.settings_write_task.is_some()
            || self.qmk_settings_write_busy()
            || self.qmk_settings_write_pending()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn start_vial_hid_operation(
        &mut self,
        ctx: &egui::Context,
        operation: VialHidOperation,
    ) -> VialHidTaskStart {
        self.start_vial_hid_operation_with_runner(
            ctx,
            operation,
            run_vial_hid_operation_with_progress,
        )
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn can_capture_ak820_backup(&self) -> bool {
        !self.ak820_demo
            && !self.headless
            && matches!(self.connect_state, ConnectState::Idle)
            && !self.hid_write_lifecycle_busy()
            && self.pending_layer_write.is_none()
            && !self.pending_layout_undo
            && !self.combo_dirty
            && !self.keycode_picker.macros_dirty
            && !self.import_pending()
            && self.pending_file_dialog.is_none()
            && self.pending_device_connect.is_none()
            && !self.unlock_open
            && !self.exit_after_hid_write
            && self.hid_device.as_ref().is_some_and(|hid| {
                hid.ak820_editing_ready()
                    && hid
                        .ak820_image()
                        .is_some_and(|image| image.profile().is_some())
            })
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn capture_availability(&self) -> CaptureAvailability {
        use CaptureAvailability::*;
        use CaptureFailure::*;
        if self.vial_hid_task.is_some()
            || self.another_hid_owner_or_write_is_pending()
            || !matches!(self.connect_state, ConnectState::Idle)
            || self.pending_device_connect.is_some()
        {
            return Unavailable(OperationBusy);
        }
        if let Some((generation, failure)) = self.connection_failure {
            if generation == self.connection_generation {
                return Unavailable(match failure {
                    ConnectionFailure::LeaseBusy => LeaseBusy,
                    ConnectionFailure::Failed => Failed,
                });
            }
        }
        if self.hid_device.is_none() {
            return Unavailable(if self.ak820_os_indicator_display_connected() {
                OperationBusy
            } else {
                Disconnected
            });
        }
        if self.can_capture_ak820_backup() {
            Ready
        } else {
            Unavailable(OperationBusy)
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn start_ak820_backup_capture(
        &mut self,
        ctx: &egui::Context,
        request_id: u64,
    ) -> Result<(), CaptureFailure> {
        if let CaptureAvailability::Unavailable(cause) = self.capture_availability() {
            return Err(cause);
        }
        match self
            .start_vial_hid_operation(ctx, VialHidOperation::Ak820BackupCapture { request_id })
        {
            VialHidTaskStart::Started => Ok(()),
            VialHidTaskStart::Busy => Err(CaptureFailure::OperationBusy),
            VialHidTaskStart::NoDevice => Err(CaptureFailure::Disconnected),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn start_ak820_restore(
        &mut self,
        ctx: &egui::Context,
        request_id: u64,
        request: std::sync::Arc<super::ak820_restore_request::PreparedRestore>,
    ) -> VialHidTaskStart {
        if !self.can_capture_ak820_backup() {
            return VialHidTaskStart::Busy;
        }
        // Playback can never resume using a pre-restore volatile RGB state.
        self.ak820_rgb_scene_stop();
        self.start_vial_hid_operation(
            ctx,
            VialHidOperation::Ak820Restore {
                request_id,
                request,
            },
        )
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn ak820_restore_progress(&self, request_id: u64) -> Option<u32> {
        self.vial_hid_task.as_ref().and_then(|task| {
            matches!(task.operation, VialHidOperation::Ak820Restore { request_id: id, .. } if id == request_id)
                .then(|| task.progress.load(std::sync::atomic::Ordering::Acquire).min(100))
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn cancel_ak820_restore(&self, request_id: u64) {
        if let Some(task) = self.vial_hid_task.as_ref().filter(|task| {
            matches!(task.operation, VialHidOperation::Ak820Restore { request_id: id, .. } if id == request_id)
        }) { task.cancel.store(true, std::sync::atomic::Ordering::Release); }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn ak820_backup_capture_progress(&self, request_id: u64) -> Option<f32> {
        self.vial_hid_task.as_ref().and_then(|task| {
            matches!(task.operation, VialHidOperation::Ak820BackupCapture { request_id: id } if id == request_id)
            .then(|| (task.progress.load(std::sync::atomic::Ordering::Relaxed) as f32 / 100.0).clamp(0.0, 1.0))
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn cancel_ak820_backup_capture(&self, request_id: u64) {
        if let Some(task) = self.vial_hid_task.as_ref().filter(|task| {
            matches!(task.operation, VialHidOperation::Ak820BackupCapture { request_id: id } if id == request_id)
        }) {
            task.cancel.store(true, std::sync::atomic::Ordering::Release);
        }
    }

    // Retain the owner until the cancelled worker returns its handle. Dropping
    // the receiver alone would let an old read overlap the next connection.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn cancel_active_ak820_backup_capture(&self) -> bool {
        if let Some(task) = self.vial_hid_task.as_ref().filter(|task| {
            matches!(
                task.operation,
                VialHidOperation::Ak820BackupCapture { .. }
                    | VialHidOperation::Ak820Restore { .. }
                    | VialHidOperation::UnlockSession(_)
            )
        }) {
            task.cancel
                .store(true, std::sync::atomic::Ordering::Release);
            return true;
        }
        false
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn start_vial_hid_operation_with_runner(
        &mut self,
        ctx: &egui::Context,
        operation: VialHidOperation,
        run: impl FnOnce(
                &crate::hid::HidDevice,
                VialHidOperation,
                &std::sync::atomic::AtomicU32,
                &std::sync::atomic::AtomicBool,
            ) -> anyhow::Result<VialHidOutcome>
            + Send
            + 'static,
    ) -> VialHidTaskStart {
        if self.rgb_exit_pending {
            return VialHidTaskStart::Busy;
        }
        if self.vial_hid_task.is_some() || self.another_hid_owner_or_write_is_pending() {
            return VialHidTaskStart::Busy;
        }
        if matches!(
            operation,
            VialHidOperation::Ak820RgbWrite(_) | VialHidOperation::UnlockSession(_)
        ) && !self.validate_selected_hid_owner()
        {
            return VialHidTaskStart::NoDevice;
        }
        if let VialHidOperation::Ak820RgbWrite(request) = &operation {
            if !self.rgb_target_matches(request)
                || !self.hid_device.as_ref().is_some_and(|hid| {
                    hid.is_bound_to(&request.device) && hid.ak820_image() == Some(request.image)
                })
            {
                return VialHidTaskStart::NoDevice;
            }
        }
        let unlock_target = if matches!(operation, VialHidOperation::UnlockSession(_)) {
            self.selected_device
                .and_then(|index| self.device_manager.devices().get(index))
                .cloned()
        } else {
            None
        };
        if matches!(operation, VialHidOperation::UnlockSession(_)) {
            let Some(target) = unlock_target.as_ref() else {
                return VialHidTaskStart::NoDevice;
            };
            let Some(hid) = self.hid_device.as_ref() else {
                return VialHidTaskStart::NoDevice;
            };
            if !hid.is_bound_to(target) || (target.is_ak820() && hid.ak820_image().is_none()) {
                return VialHidTaskStart::NoDevice;
            }
        }
        let Some(hid_device) = self.hid_device.take() else {
            return VialHidTaskStart::NoDevice;
        };
        let unlock_image = hid_device.ak820_image();

        if matches!(operation, VialHidOperation::PictogramLoad { .. }) {
            self.display_settings.pictograms.loading = true;
            self.display_settings.pictograms.load_failure = None;
        }
        // SET can mutate even when its acknowledgement or subsequent GET fails.
        // Invalidate before handing ownership to the worker, not on completion.
        if let VialHidOperation::LayerNameWrite(request) = &operation {
            self.uncertain_layer_names.retain(|old| {
                old.device_key != request.device_key
                    || old.layer != request.layer
                    || old.device.path != request.device.path
                    || old.device.instance_token != request.device.instance_token
            });
            self.uncertain_layer_names.push(request.clone());
        }
        let generation = self.connection_generation;
        let (sender, receiver) = std::sync::mpsc::channel();
        let repaint_ctx = ctx.clone();
        let task_operation = operation.clone();
        let progress = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let worker_progress = progress.clone();
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let diagnostic_label = operation.display_diagnostic_label();
        if let Some(label) = diagnostic_label {
            let target = self
                .selected_device
                .and_then(|index| self.device_manager.devices().get(index))
                .map(|device| device.path.as_str());
            log::debug!(
                "Display operation started: {label} generation={generation} target={target:?}"
            );
        }
        std::thread::spawn(move || {
            let started = std::time::Instant::now();
            #[cfg(target_os = "macos")]
            let _hid_lock = hid_device.macos_hid_operation_lock();

            let outcome = run(
                &hid_device,
                operation.clone(),
                &worker_progress,
                &worker_cancel,
            );
            let disconnected = outcome
                .as_ref()
                .err()
                .map(crate::hid::is_disconnect_error)
                .unwrap_or(false);
            let unlock_failure = outcome.as_ref().err().and_then(|error| {
                error
                    .downcast_ref::<UnlockFailureKind>()
                    .copied()
                    .map(|kind| {
                        let timed_out = error.chain().any(|cause| {
                            let message = cause.to_string().to_ascii_lowercase();
                            message.contains("timed out")
                                || message.contains("timeout")
                                || message.contains("device did not respond")
                        });
                        if timed_out {
                            UnlockFailureKind::TransportTimeout
                        } else if disconnected {
                            UnlockFailureKind::Disconnected
                        } else {
                            kind
                        }
                    })
            });
            if let Some(label) = diagnostic_label {
                match &outcome {
                    Ok(_) => log::debug!(
                        "Display operation completed: {label} generation={generation} elapsed_ms={}",
                        started.elapsed().as_millis(),
                    ),
                    Err(error) => log::debug!(
                        "Display operation failed: {label} generation={generation} elapsed_ms={} disconnected={disconnected} error={error:#}",
                        started.elapsed().as_millis(),
                    ),
                }
            }
            if matches!(operation, VialHidOperation::UnlockSession(_))
                && !matches!(
                    &outcome,
                    Ok(VialHidOutcome::UnlockSession(UnlockResult::Confirmed))
                )
            {
                if let Some(retirement) = hid_device.retirement_handle() {
                    retirement.retire();
                }
            }
            let hid_device = (!disconnected).then_some(hid_device);
            let outcome = outcome.map_err(|error| format!("{error:#}"));
            let _ = sender.send(VialHidTaskResult {
                hid_device,
                operation,
                outcome,
                disconnected,
                unlock_failure,
                unlock_target,
                unlock_image,
                generation,
            });
            repaint_ctx.request_repaint();
        });
        self.vial_hid_task = Some(VialHidTask {
            receiver,
            operation: task_operation,
            generation,
            progress,
            cancel,
        });
        VialHidTaskStart::Started
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn start_vial_unlock(&mut self, ctx: &egui::Context) -> VialHidTaskStart {
        self.start_vial_hid_operation(
            ctx,
            VialHidOperation::UnlockSession(std::sync::Arc::new(std::sync::Mutex::new(None))),
        )
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn observe_vial_unlock(&mut self) {
        let latest = self
            .vial_hid_task
            .as_ref()
            .and_then(|task| match &task.operation {
                VialHidOperation::UnlockSession(status) => {
                    status.lock().ok().and_then(|s| s.clone())
                }
                _ => None,
            });
        if let Some(latest) = latest {
            if !self.vial_unlock_polling {
                self.finish_vial_unlock_start(false, latest.keys);
            }
            self.vial_unlocked = None;
            self.vial_unlock_counter = latest.counter;
            self.vial_unlock_total = self.vial_unlock_total.max(latest.counter);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn cancel_vial_unlock_wait(&self) {
        if let Some(task) = self
            .vial_hid_task
            .as_ref()
            .filter(|task| matches!(task.operation, VialHidOperation::UnlockSession(_)))
        {
            task.cancel
                .store(true, std::sync::atomic::Ordering::Release);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn start_vial_lock(&mut self, ctx: &egui::Context) -> VialHidTaskStart {
        self.start_vial_hid_operation(ctx, VialHidOperation::Lock)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn start_vial_matrix_poll(
        &mut self,
        ctx: &egui::Context,
        rows: usize,
        cols: usize,
        remember_ever_pressed: bool,
    ) -> VialHidTaskStart {
        self.start_vial_hid_operation(
            ctx,
            VialHidOperation::Matrix {
                rows,
                cols,
                remember_ever_pressed,
            },
        )
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn maybe_start_macro_write(&mut self, ctx: &egui::Context) {
        // Offline edits are persisted only through the local project file.
        if self.ak820_demo {
            return;
        }
        if !self.keycode_picker.macros_dirty
            || self.keycode_picker.open
            || self.hid_write_task_active()
            || self.keycode_picker.macro_attempted_revision
                == Some(self.keycode_picker.macro_edit_revision)
        {
            return;
        }
        if self.unlock_open || self.vial_unlock_polling {
            return;
        }
        if self.is_vial_locked() {
            // Preserve the dirty edit for an explicit unlock; do not reopen a
            // cancelled preflight just because the picker closed.
            return;
        }

        let revision = self.keycode_picker.macro_edit_revision;
        let operation = VialHidOperation::MacroWrite {
            macros: self.keycode_picker.macro_texts.clone(),
            revision,
        };
        match self.start_vial_hid_operation(ctx, operation) {
            VialHidTaskStart::Started => {
                // A later edit can set this again while the snapshot is being
                // written; completion must not clear that newer dirty state.
                self.keycode_picker.macros_dirty = false;
                self.keycode_picker.macro_attempted_revision = Some(revision);
                self.status_msg = crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "status_messages.macros_saving",
                )
                .into();
            }
            VialHidTaskStart::Busy => {}
            VialHidTaskStart::NoDevice => {
                self.keycode_picker.macro_attempted_revision = Some(revision);
                self.status_msg = crate::i18n::tr_catalog_format(
                    self.app_settings.language,
                    "status_messages.macro_write_error",
                    &[("error", "device handle is not available")],
                );
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn poll_vial_hid_task(&mut self, ctx: &egui::Context) {
        if self
            .rgb_uncertain
            .is_some_and(|generation| generation != self.connection_generation)
        {
            self.rgb_uncertain = None;
        }
        if self
            .rgb_draft
            .as_ref()
            .is_some_and(|draft| draft.generation != self.connection_generation)
        {
            self.rgb_draft = None;
        }
        if self.exit_after_hid_write
            || self.settings_tab != SettingsTab::Rgb
            || !matches!(
                self.main_menu_tab,
                MainMenuTab::Settings | MainMenuTab::Advanced
            )
        {
            self.rgb_draft = None;
        }
        self.poll_vial_hid_task_with_settings_save(ctx, save_app_settings);
        if self.vial_hid_task.is_none() {
            self.start_rgb_draft(ctx);
        }
        if !self.rgb_exit_pending {
            self.maybe_start_layer_name_write(ctx);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn poll_vial_hid_task_with_settings_save(
        &mut self,
        ctx: &egui::Context,
        save_settings: impl FnOnce(&AppSettings),
    ) {
        let received = match self.vial_hid_task.as_ref() {
            Some(task) => task.receiver.try_recv(),
            None => return,
        };

        let result = match received {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                let task = self
                    .vial_hid_task
                    .take()
                    .expect("Vial HID task checked above");
                if task.generation != self.connection_generation {
                    report_stale_restore(ctx, task.generation, &task.operation);
                }
                if task.generation == self.connection_generation {
                    self.hid_device = None;
                    if matches!(task.operation, VialHidOperation::UnlockSession(_)) {
                        self.vial_unlocked = None;
                        self.stop_vial_unlock_with_status(
                            if self.app_settings.language == crate::i18n::Language::Russian {
                                "Ошибка работника разблокировки. Состояние не подтверждено."
                            } else {
                                "Unlock worker failed. State is unconfirmed."
                            },
                        );
                        return;
                    }
                    if let VialHidOperation::Ak820BackupCapture { request_id } = &task.operation {
                        super::ak820_backup_inspection::capture_finished(
                            ctx,
                            *request_id,
                            task.generation,
                            Err(CaptureFailure::Failed),
                        );
                    }
                    if let VialHidOperation::Ak820Restore {
                        request_id,
                        request,
                    } = &task.operation
                    {
                        super::ak820_backup_inspection::restore_failed(
                            ctx,
                            *request_id,
                            task.generation,
                            request.safety_location(),
                        );
                    }
                    self.finish_vial_hid_error(
                        task.operation,
                        "Vial HID worker stopped".to_owned(),
                        true,
                    );
                }
                if !self.rgb_exit_pending {
                    self.resume_pending_device_connect();
                }
                return;
            }
        };

        self.vial_hid_task = None;
        if result.generation != self.connection_generation {
            if matches!(result.operation, VialHidOperation::Ak820RgbWrite(_)) {
                self.rgb_draft = None;
            }
            report_stale_restore(ctx, result.generation, &result.operation);
            if !self.rgb_exit_pending {
                self.resume_pending_device_connect();
            }
            return;
        }

        if let VialHidOperation::Ak820RgbWrite(request) = &result.operation {
            let returned_image_matches = result
                .hid_device
                .as_ref()
                .map(|hid| {
                    hid.is_bound_to(&request.device) && hid.ak820_image() == Some(request.image)
                })
                .unwrap_or(result.outcome.is_err());
            if !self.rgb_target_matches(request) || !returned_image_matches {
                self.retire_unconfirmed_hid_loan(result.hid_device.as_ref());
                self.rgb_draft = None;
                self.rgb_uncertain = Some(request.generation);
                self.status_msg = if self.app_settings.language == crate::i18n::Language::Russian {
                    "Устройство или образ изменились. Прочитайте настройки снова."
                } else {
                    "Device or image changed. Read settings again."
                }
                .into();
                return;
            }
        }

        if self.rgb_exit_pending
            && matches!(
                result.operation,
                VialHidOperation::Ak820RgbWrite(_) | VialHidOperation::UnlockSession(_)
            )
        {
            if result.outcome.is_err() {
                self.rgb_uncertain = Some(result.generation);
            }
            // The closing UI must not regain a transport between worker return
            // and the final viewport Close event.
            drop(result.hid_device);
            return;
        }

        if matches!(result.operation, VialHidOperation::UnlockSession(_)) {
            let target_matches = match &result.unlock_target {
                Some(target) => self
                    .selected_device
                    .and_then(|index| self.device_manager.devices().get(index))
                    .is_some_and(|current| {
                        current.path == target.path
                            && current.instance_token == target.instance_token
                            && current.permits_hid_target(target)
                    }),
                None => false,
            };
            let confirmed = matches!(
                &result.outcome,
                Ok(VialHidOutcome::UnlockSession(UnlockResult::Confirmed))
            );
            let owner_matches = !confirmed
                || result
                    .hid_device
                    .as_ref()
                    .zip(result.unlock_target.as_ref())
                    .is_some_and(|(hid, target)| hid.is_bound_to(target));
            let image_matches = result
                .hid_device
                .as_ref()
                .map(|hid| hid.ak820_image() == result.unlock_image)
                .unwrap_or(result.outcome.is_err());
            if !target_matches || !owner_matches || !image_matches {
                self.retire_unconfirmed_hid_loan(result.hid_device.as_ref());
                drop(result.hid_device);
                self.vial_unlocked = None;
                self.stop_vial_unlock_with_status(
                    if self.app_settings.language == crate::i18n::Language::Russian {
                        "Устройство или образ изменились. Состояние не подтверждено."
                    } else {
                        "Device or image changed. State is unconfirmed."
                    },
                );
                return;
            }
        }
        if matches!(result.operation, VialHidOperation::UnlockSession(_))
            && !matches!(
                &result.outcome,
                Ok(VialHidOutcome::UnlockSession(UnlockResult::Confirmed))
            )
        {
            self.retire_unconfirmed_hid_loan(result.hid_device.as_ref());
            drop(result.hid_device);
            self.vial_unlocked = None;
            let ru = self.app_settings.language == crate::i18n::Language::Russian;
            let message = match (&result.outcome, result.unlock_failure, ru) {
                (Ok(VialHidOutcome::UnlockSession(UnlockResult::Denied)), _, true) => {
                    "Устройство отклонило разблокировку. Состояние не подтверждено."
                }
                (Ok(VialHidOutcome::UnlockSession(UnlockResult::Denied)), _, false) => {
                    "Device denied unlock. State is unconfirmed."
                }
                (Ok(VialHidOutcome::UnlockSession(UnlockResult::TimedOut)), _, true) => {
                    "Время ожидания истекло. Отпустите клавиши, проверьте клавиатуру и при необходимости повторите попытку вручную."
                }
                (Ok(VialHidOutcome::UnlockSession(UnlockResult::TimedOut)), _, false) => {
                    "Unlock wait timed out. Release the keys, check the keyboard, and retry manually if needed."
                }
                (Ok(VialHidOutcome::UnlockSession(UnlockResult::Cancelled)), _, true) => {
                    "Ожидание остановлено; устройство может продолжать разблокировку."
                }
                (Ok(VialHidOutcome::UnlockSession(UnlockResult::Cancelled)), _, false) => {
                    "Stopped waiting; the device may still be unlocking."
                }
                (_, Some(UnlockFailureKind::Disconnected), true) => "Устройство отключено. Состояние не подтверждено.",
                (_, Some(UnlockFailureKind::Disconnected), false) => "Device disconnected. State is unconfirmed.",
                (_, Some(UnlockFailureKind::TransportTimeout), true) => "Время ответа устройства истекло. Состояние не подтверждено.",
                (_, Some(UnlockFailureKind::TransportTimeout), false) => "Device response timed out. State is unconfirmed.",
                (_, Some(UnlockFailureKind::Status), true) => "Не удалось прочитать состояние разблокировки.",
                (_, Some(UnlockFailureKind::Status), false) => "Could not read unlock status.",
                (_, Some(UnlockFailureKind::Start), true) => "Не удалось запустить разблокировку. Состояние не подтверждено.",
                (_, Some(UnlockFailureKind::Start), false) => "Unlock start failed. State is unconfirmed.",
                (_, Some(UnlockFailureKind::Poll), true) => "Ошибка проверки разблокировки. Состояние не подтверждено.",
                (_, Some(UnlockFailureKind::Poll), false) => "Unlock check failed. State is unconfirmed.",
                (_, None, true) => "Ошибка разблокировки. Состояние не подтверждено.",
                (_, None, false) => "Unlock failed. State is unconfirmed.",
            };
            self.stop_vial_unlock_with_status(message);
            return;
        }
        self.hid_device = result.hid_device;
        match result.outcome {
            Ok(VialHidOutcome::UnlockSession(_)) => self.finish_vial_unlock_poll(true, false, 0),
            Ok(VialHidOutcome::Ak820RgbWritten) => {
                if let VialHidOperation::Ak820RgbWrite(request) = result.operation {
                    log::debug!("RGB request {} completed", request.request_id);
                    if self.rgb_target_matches(&request) && !self.exit_after_hid_write {
                        self.rgb_settings = request.value;
                        if let Some(draft) = &mut self.rgb_draft {
                            draft.old = self.rgb_settings.clone();
                        }
                        self.rgb_uncertain = None;
                    }
                }
            }
            Ok(VialHidOutcome::Ak820Restored(completed)) => {
                if let VialHidOperation::Ak820Restore { request_id, .. } = result.operation {
                    super::ak820_backup_inspection::restore_finished(
                        ctx,
                        request_id,
                        result.generation,
                        Ok(completed),
                    );
                    // Retire the old editor values and perform the ordinary
                    // verified read-only load. No setter or retry is scheduled.
                    if let Some(index) = self.selected_device {
                        self.start_connect(index);
                    }
                }
                return;
            }
            Ok(VialHidOutcome::Ak820BackupCaptured(snapshot)) => {
                if let VialHidOperation::Ak820BackupCapture { request_id } = result.operation {
                    super::ak820_backup_inspection::capture_finished(
                        ctx,
                        request_id,
                        result.generation,
                        Ok(snapshot),
                    );
                }
            }
            Ok(VialHidOutcome::LayerNameWritten(actual)) => {
                if let VialHidOperation::LayerNameWrite(request) = result.operation {
                    if !self
                        .selected_device
                        .and_then(|i| self.device_manager.devices().get(i))
                        .is_some_and(|device| {
                            device.path == request.device.path
                                && device.instance_token == request.device.instance_token
                                && device.permits_hid_target(&request.device)
                        })
                    {
                        return;
                    }
                    // Completion belongs to the captured Default, never the
                    // profile selected while HID was borrowed by this worker.
                    if let Some(name) = self.layer_names.get_mut(request.layer) {
                        *name = actual.clone();
                    }
                    if let Some(layout) = self.layout.as_mut() {
                        if let Some(name) = layout.layer_names.get_mut(request.layer) {
                            *name = actual.clone();
                        }
                    }
                    self.uncertain_layer_names.retain(|old| {
                        old.device_key != request.device_key
                            || old.layer != request.layer
                            || old.generation != request.generation
                            || old.device.path != request.device.path
                            || old.device.instance_token != request.device.instance_token
                    });
                    if !self.selected_layer_names_uncertain() {
                        save_layer_names(&self.layer_names, &self.current_device_name);
                    }
                    if let Some(profile) = self
                        .app_settings
                        .application_layouts
                        .get_mut(&request.device_key)
                        .and_then(|s| s.layouts.get_mut(&request.layout_id))
                    {
                        if profile.layer_names.get(request.layer) == Some(&request.name) {
                            profile.set_layer_name(request.layer, actual);
                            save_settings(&self.app_settings);
                        }
                    }
                }
            }
            #[cfg(test)]
            Ok(VialHidOutcome::UnlockStarted { unlocked, keys }) => {
                self.finish_vial_unlock_start(unlocked, keys);
            }
            #[cfg(test)]
            Ok(VialHidOutcome::UnlockPolled {
                unlocked,
                in_progress,
                counter,
            }) => {
                self.finish_vial_unlock_poll(unlocked, in_progress, counter);
            }
            Ok(VialHidOutcome::Locked) => {
                self.finish_vial_lock();
            }
            Ok(VialHidOutcome::Matrix(pressed)) => {
                let remember_ever_pressed = match &result.operation {
                    VialHidOperation::Matrix {
                        remember_ever_pressed,
                        ..
                    } => *remember_ever_pressed,
                    _ => false,
                };
                self.finish_matrix_tester_poll(pressed, remember_ever_pressed);
                if self.app_settings.sticky_layout_window {
                    ctx.request_repaint_of(
                        super::layout_indicator_window::sticky_layout_viewport_id(),
                    );
                }
            }
            Ok(VialHidOutcome::Battery(battery)) => {
                if let Some(info) = self.device_about_info.as_mut() {
                    info.battery_halves = battery;
                }
                self.schedule_battery_refresh_for_result(battery);
            }
            Ok(VialHidOutcome::KeyWritten) => {
                if let VialHidOperation::KeyWrite {
                    layer,
                    key_index,
                    old_binding,
                    binding,
                    is_undo,
                    ..
                } = result.operation
                {
                    self.finish_keycode_write(layer, key_index, old_binding, binding, is_undo);
                }
            }
            Ok(VialHidOutcome::EncoderWritten) => {
                if let VialHidOperation::EncoderWrite {
                    layer,
                    encoder_visual_index,
                    encoder_index,
                    direction,
                    old_keycode,
                    keycode,
                    is_undo,
                } = result.operation
                {
                    self.finish_encoder_write(
                        layer,
                        encoder_visual_index,
                        encoder_index,
                        direction,
                        old_keycode,
                        keycode,
                        is_undo,
                    );
                }
            }
            Ok(VialHidOutcome::MacrosWritten) => {
                let VialHidOperation::MacroWrite { revision, .. } = &result.operation else {
                    unreachable!("macro outcome must come from a macro write operation");
                };
                if self.keycode_picker.macro_edit_revision == *revision {
                    self.keycode_picker.mark_macros_clean();
                }
                self.status_msg = crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "status_messages.macros_saved",
                )
                .into();
            }
            Ok(VialHidOutcome::BackgroundUploaded(upload)) => {
                self.app_settings.standby_background_source_path = Some(upload.source_path.clone());
                save_settings(&self.app_settings);
                self.display_settings.clock_background_kind = upload.kind;
                self.display_settings.clock_background_frames = upload.frame_count;
                self.display_settings.clock_background_bytes = upload.total_size;
                self.display_settings.clock_background_file_name = Some(upload.file_name);
                self.display_settings.clock_background_preview_rgba = upload.preview_rgba;
                self.display_settings.clock_background_preview_frames_rgba =
                    upload.preview_frames_rgba;
                self.display_settings.clock_background_preview_delays_ms = upload.preview_delays_ms;
                self.display_settings.clock_background_preview_revision = self
                    .display_settings
                    .clock_background_preview_revision
                    .wrapping_add(1);
                self.status_msg = crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "display_settings.background_uploaded",
                )
                .into();
            }
            Ok(VialHidOutcome::BackgroundCancelled { cleared: false }) => {
                self.status_msg = crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "display_settings.upload_cancelled",
                )
                .into();
            }
            Ok(
                VialHidOutcome::BackgroundCleared
                | VialHidOutcome::BackgroundCancelled { cleared: true },
            ) => {
                self.display_settings.clock_background_kind = 0;
                self.display_settings.clock_background_frames = 0;
                self.display_settings.clock_background_bytes = 0;
                self.display_settings.clock_background_file_name = None;
                self.display_settings.clock_background_preview_rgba.clear();
                self.display_settings
                    .clock_background_preview_frames_rgba
                    .clear();
                self.display_settings
                    .clock_background_preview_delays_ms
                    .clear();
                self.display_settings.clock_background_preview_revision = self
                    .display_settings
                    .clock_background_preview_revision
                    .wrapping_add(1);
                self.status_msg = crate::i18n::tr_catalog(
                    self.app_settings.language,
                    if matches!(result.operation, VialHidOperation::BackgroundUpload { .. }) {
                        "display_settings.upload_cancelled"
                    } else {
                        "display_settings.background_cleared"
                    },
                )
                .into();
            }
            #[cfg(test)]
            Ok(VialHidOutcome::BackgroundSpeedSet) => {
                let VialHidOperation::BackgroundSpeed { percent, .. } = result.operation else {
                    unreachable!("background speed outcome must come from a speed operation");
                };
                self.display_settings.clock_background_speed_percent = percent;
                self.display_settings
                    .confirmed_clock_background_speed_percent = percent;
                self.status_msg = crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "display_settings.background_speed_saved",
                )
                .into();
            }
            Ok(VialHidOutcome::StartupImageUploaded(upload)) => {
                self.display_settings.startup_image_present = true;
                self.display_settings.startup_image_bytes = upload.total_size;
                self.display_settings.startup_image_file_name = Some(upload.file_name);
                self.display_settings.startup_image_preview_rgba = upload.preview_rgba;
                self.display_settings.startup_image_preview_revision = self
                    .display_settings
                    .startup_image_preview_revision
                    .wrapping_add(1);
                self.status_msg = crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "display_settings.startup_image_uploaded",
                )
                .into();
            }
            Ok(VialHidOutcome::StartupImageCleared) => {
                self.display_settings.startup_image_present = false;
                self.display_settings.startup_image_bytes = 0;
                self.display_settings.startup_image_file_name = None;
                self.display_settings.startup_image_preview_rgba.clear();
                self.display_settings.startup_image_preview_revision = self
                    .display_settings
                    .startup_image_preview_revision
                    .wrapping_add(1);
                self.status_msg = crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "display_settings.startup_image_cleared",
                )
                .into();
            }
            Ok(VialHidOutcome::PictogramsLoaded(library)) => {
                self.display_settings.pictograms.load_failure = None;
                self.display_settings.pictograms.supported = Some(true);
                self.display_settings.pictograms.loaded = true;
                self.display_settings.pictograms.loading = false;
                self.display_settings.pictograms.library = library;
                self.display_settings.pictograms.preserve_editor_on_load = false;
                if !matches!(
                    result.operation,
                    VialHidOperation::PictogramLoad {
                        preserve_editor: true
                    }
                ) {
                    self.restore_pictogram_editor_from_device();
                }
                self.status_msg = crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "display_settings.pictograms_loaded",
                )
                .into();
            }
            Ok(VialHidOutcome::PictogramsUploaded(library)) => {
                self.display_settings.pictograms.load_failure = None;
                self.display_settings.pictograms.preserve_editor_on_load = false;
                self.display_settings.pictograms.supported = Some(true);
                self.display_settings.pictograms.loaded = true;
                self.display_settings.pictograms.loading = false;
                self.display_settings.pictograms.saving = false;
                self.display_settings.pictograms.library = library;
                self.status_msg = crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "display_settings.pictograms_saved",
                )
                .into();
            }
            Ok(VialHidOutcome::Deferred(payload)) => {
                if matches!(
                    &result.operation,
                    VialHidOperation::Deferred(request) if request.is_background_layer()
                ) {
                    self.deferred_device_load.mark_background_layer_finished();
                }
                self.finish_deferred_device_load(payload);
            }
            Err(error) => {
                if let VialHidOperation::Ak820BackupCapture { request_id } = &result.operation {
                    super::ak820_backup_inspection::capture_finished(
                        ctx,
                        *request_id,
                        result.generation,
                        Err(CaptureFailure::from_worker(&error, result.disconnected)),
                    );
                }
                if let VialHidOperation::Ak820Restore {
                    request_id,
                    request,
                } = &result.operation
                {
                    super::ak820_backup_inspection::restore_failed(
                        ctx,
                        *request_id,
                        result.generation,
                        request.safety_location(),
                    );
                }
                self.finish_vial_hid_error(result.operation, error, result.disconnected);
            }
        }

        if !self.rgb_exit_pending {
            self.continue_pending_settings_writes(ctx);
            self.resume_pending_device_connect();
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn finish_keycode_write(
        &mut self,
        layer: usize,
        key_index: usize,
        old_binding: crate::keyboard::KeyBinding,
        binding: crate::keyboard::KeyBinding,
        is_undo: bool,
    ) {
        if let Some(layout) = self.layout.as_mut() {
            layout.set_key_binding(layer, key_index, binding);
        }
        if !is_undo {
            self.undo_stack.push(UndoAction::Key {
                layer,
                key_idx: key_index,
                old_binding,
            });
        }
        self.refresh_layer_picker_content_flags();
        self.status_msg = "✓ Saved".into();
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[allow(clippy::too_many_arguments)]
    fn finish_encoder_write(
        &mut self,
        layer: usize,
        encoder_visual_index: usize,
        encoder_index: u8,
        direction: u8,
        old_keycode: u16,
        keycode: u16,
        is_undo: bool,
    ) {
        if let Some(layout) = self.layout.as_mut() {
            layout.set_encoder_keycode(layer, encoder_visual_index, keycode);
        }
        if !is_undo {
            self.undo_stack.push(UndoAction::Encoder {
                layer,
                encoder_visual_idx: encoder_visual_index,
                old_kc: old_keycode,
            });
        }
        self.status_msg = format!(
            "Assigned encoder {encoder_index} direction {direction} on layer {}",
            layer + 1
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn rollback_single_write(&mut self, operation: &VialHidOperation) {
        match operation {
            VialHidOperation::KeyWrite {
                layer,
                key_index,
                old_binding,
                binding,
                is_undo,
                ..
            } => {
                if let Some(layout) = self.layout.as_mut() {
                    if layout.get_key_binding(*layer, *key_index) == *binding {
                        layout.set_key_binding(*layer, *key_index, *old_binding);
                    }
                }
                if *is_undo {
                    self.undo_stack.push(UndoAction::Key {
                        layer: *layer,
                        key_idx: *key_index,
                        old_binding: *binding,
                    });
                }
                self.refresh_layer_picker_content_flags();
            }
            VialHidOperation::EncoderWrite {
                layer,
                encoder_visual_index,
                old_keycode,
                keycode,
                is_undo,
                ..
            } => {
                if let Some(layout) = self.layout.as_mut() {
                    if layout.get_encoder_keycode(*layer, *encoder_visual_index) == *keycode {
                        layout.set_encoder_keycode(*layer, *encoder_visual_index, *old_keycode);
                    }
                }
                if *is_undo {
                    self.undo_stack.push(UndoAction::Encoder {
                        layer: *layer,
                        encoder_visual_idx: *encoder_visual_index,
                        old_kc: *keycode,
                    });
                }
            }
            _ => {}
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn finish_vial_lock(&mut self) {
        self.vial_unlocked = Some(false);
        // A protected settings page must not remain interactive after lock.
        self.main_menu_tab = MainMenuTab::Keyboard;
        self.matrix_tester_pressed.clear();
        self.matrix_tester_unlock_prompted = false;
        self.matrix_tester_lock_checked = false;
        if self.app_settings.sticky_layout_window {
            self.app_settings.sticky_layout_window = false;
            self.pending_layout_indicator_open_after_unlock = false;
            self.sticky_layout_last_size = None;
            save_app_settings(&self.app_settings);
            self.status_msg = crate::i18n::tr_catalog(
                self.app_settings.language,
                "ui.sticky_layout_closed_due_to_lock",
            )
            .into();
        } else {
            self.status_msg =
                crate::i18n::tr_catalog(self.app_settings.language, "dynamic_status.device_locked")
                    .into();
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn finish_vial_hid_error(
        &mut self,
        operation: VialHidOperation,
        error: String,
        disconnected: bool,
    ) {
        if let VialHidOperation::Ak820RgbWrite(request) = &operation {
            self.rgb_draft = None;
            self.rgb_uncertain = Some(request.generation);
            self.rgb_settings = request.old.clone();
            if disconnected {
                let old = request.old.clone();
                let status = if self.app_settings.language == crate::i18n::Language::Russian {
                    "Изменение RGB остановлено: устройство отключено. Прочитайте настройки снова."
                } else {
                    "RGB update stopped: device disconnected. Read settings again."
                };
                self.clear_connected_keyboard_state(status);
                self.rgb_settings = old;
                self.rgb_uncertain = Some(self.connection_generation);
                return;
            }
        }
        if matches!(operation, VialHidOperation::Ak820Restore { .. }) {
            let text = if self.app_settings.language == crate::i18n::Language::Russian {
                "Восстановление остановлено. Прочитайте настройки снова; сохранённые защитные копии доступны в выбранной папке."
            } else {
                "Restore stopped. Read settings again; saved safety backups remain in the selected folder."
            };
            // Even a pre-write file error cannot leave a potentially stale
            // editor accepting follow-up writes after a failed restore.
            self.clear_connected_keyboard_state(text);
            self.status_msg = text.into();
            return;
        }
        // A capture has no setter or reconnect continuation. Keep transport
        // diagnostics, paths and potentially private bytes out of presentation.
        if matches!(operation, VialHidOperation::Ak820BackupCapture { .. }) {
            let text = if self.app_settings.language == crate::i18n::Language::Russian {
                "Чтение резервной копии остановлено. Новая копия не сохранена."
            } else {
                "Backup reading stopped. No new backup was saved."
            };
            if disconnected {
                self.clear_connected_keyboard_state(text);
            }
            self.status_msg = text.into();
            return;
        }
        match &operation {
            VialHidOperation::PictogramLoad { .. } => {
                self.display_settings.pictograms.loading = false;
                self.display_settings.pictograms.load_failure =
                    Some((self.connection_generation, error.clone()));
                // Only the QUERY's explicit command/format rejection establishes
                // unsupported storage (these errors originate in load_pictograms).
                // Status/transport/read failures leave capability unconfirmed so
                // Choose can retry, but loaded=false still prevents assignments.
                if self.display_settings.pictograms.supported != Some(true)
                    && matches!(
                        error.as_str(),
                        "pictogram command 0xC0 is unsupported" | "unsupported pictogram format"
                    )
                {
                    self.display_settings.pictograms.supported = Some(false);
                }
            }
            operation if operation.is_pictogram_upload() => {
                let pictograms = &mut self.display_settings.pictograms;
                // Flash/transport errors can leave even the old library uncertain.
                // Invalidate the device snapshot, not the user's editor; the next
                // storage read preserves that editor and confirms the actual bytes.
                pictograms.loaded = false;
                pictograms.loading = false;
                pictograms.saving = false;
                pictograms.load_failure = None;
                pictograms.preserve_editor_on_load = true;
                pictograms.library = PictogramLibrary::default();
                pictograms.upload_due = None;
            }
            _ => {}
        }

        if let VialHidOperation::MacroWrite { revision, .. } = &operation {
            self.keycode_picker.macros_dirty = true;
            self.keycode_picker.macro_attempted_revision =
                (self.keycode_picker.macro_edit_revision == *revision).then_some(*revision);
            self.status_msg = crate::i18n::tr_catalog_format(
                self.app_settings.language,
                "status_messages.macro_write_error",
                &[("error", &error)],
            );
        }

        if matches!(
            &operation,
            VialHidOperation::KeyWrite { .. } | VialHidOperation::EncoderWrite { .. }
        ) {
            self.rollback_single_write(&operation);
            self.status_msg = match operation {
                VialHidOperation::EncoderWrite { .. } => {
                    format!("Set encoder failed: {error}")
                }
                _ => format!("Write error: {error}"),
            };
            if disconnected && !self.begin_bluetooth_reconnect(error.clone()) {
                self.clear_connected_keyboard_state(error);
            }
            return;
        }

        if disconnected {
            // Connection cleanup owns device state, but an uncertain write must
            // not erase the user's independent editor. Carry the invalidated
            // snapshot through cleanup; never restore a confirmed device library.
            let draft = (operation.is_pictogram_upload()
                || matches!(
                    operation,
                    VialHidOperation::PictogramLoad {
                        preserve_editor: true
                    }
                ))
            .then(|| {
                let mut draft = std::mem::take(&mut self.display_settings.pictograms);
                draft.supported = None;
                draft.loaded = false;
                draft.loading = false;
                draft.saving = false;
                draft.library = PictogramLibrary::default();
                draft
            });
            if !self.begin_bluetooth_reconnect(error.clone()) {
                self.clear_connected_keyboard_state(error);
            }
            if let Some(draft) = draft {
                self.display_settings.pictograms = draft;
            }
            return;
        }

        match operation {
            VialHidOperation::Ak820BackupCapture { .. } | VialHidOperation::Ak820Restore { .. } => {
                unreachable!("backup operation handled above")
            }
            VialHidOperation::Ak820RgbWrite(_) => {
                self.status_msg = if self.app_settings.language == crate::i18n::Language::Russian {
                    "Изменение RGB остановлено. Прочитайте настройки снова."
                } else {
                    "RGB update stopped. Read settings again."
                }
                .into();
            }
            VialHidOperation::UnlockSession(_) => self.fail_vial_unlock_start(error),
            #[cfg(test)]
            VialHidOperation::UnlockStart => self.fail_vial_unlock_start(error),
            VialHidOperation::LayerNameWrite(_) => {
                self.status_msg = layer_name_write_error(self.app_settings.language, &error);
            }
            #[cfg(test)]
            VialHidOperation::UnlockPoll => self.fail_vial_unlock_poll(error),
            VialHidOperation::Lock => {
                self.status_msg = crate::i18n::tr_catalog_format(
                    self.app_settings.language,
                    "dynamic_status.lock_failed",
                    &[("error", &error)],
                );
            }
            VialHidOperation::Matrix { .. } => self.fail_matrix_tester_poll(error),
            VialHidOperation::BatteryRefresh => {
                log::warn!("Battery refresh failed: {error}");
                self.next_battery_refresh_at =
                    Some(std::time::Instant::now() + BATTERY_REFRESH_RETRY_INTERVAL);
            }
            VialHidOperation::KeyWrite { .. } | VialHidOperation::EncoderWrite { .. } => {
                unreachable!("single writes are handled before disconnect processing")
            }
            VialHidOperation::MacroWrite { .. } => {
                // The localized error and edit-gated dirty state were set above.
            }
            VialHidOperation::BackgroundUpload { .. } | VialHidOperation::BackgroundClear => {
                self.status_msg = crate::i18n::tr_catalog_format(
                    self.app_settings.language,
                    "display_settings.background_error",
                    &[("error", &error)],
                );
            }
            #[cfg(test)]
            VialHidOperation::BackgroundSpeed { old_percent, .. } => {
                self.display_settings.clock_background_speed_percent = old_percent;
                self.status_msg = crate::i18n::tr_catalog_format(
                    self.app_settings.language,
                    "display_settings.background_speed_error",
                    &[("error", &error)],
                );
            }
            VialHidOperation::StartupImageUpload { .. } | VialHidOperation::StartupImageClear => {
                self.status_msg = crate::i18n::tr_catalog_format(
                    self.app_settings.language,
                    "display_settings.startup_image_error",
                    &[("error", &error)],
                );
            }
            VialHidOperation::PictogramLoad { .. } => {
                self.status_msg = format!(
                    "{}: {error}",
                    crate::i18n::tr_catalog(
                        self.app_settings.language,
                        "display_settings.pictograms_firmware_required",
                    )
                );
            }
            VialHidOperation::PictogramSlotUpload { .. } => {
                self.status_msg = format!(
                    "{}: {error}",
                    crate::i18n::tr_catalog(
                        self.app_settings.language,
                        "display_settings.pictograms_status",
                    )
                );
            }
            #[cfg(test)]
            VialHidOperation::PictogramUpload { .. } => {
                self.status_msg = format!(
                    "{}: {error}",
                    crate::i18n::tr_catalog(
                        self.app_settings.language,
                        "display_settings.pictograms_status",
                    )
                );
            }
            VialHidOperation::Deferred(request) => {
                log::warn!("Deferred Bluetooth device load failed: {error}");
                if request.is_background_layer() {
                    self.deferred_device_load.mark_background_layer_finished();
                }
                self.fail_deferred_device_load(&request, error);
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn deferred_vial_hid_task_blocks_keyboard(&self) -> bool {
        self.vial_hid_task
            .as_ref()
            .map(|task| match &task.operation {
                VialHidOperation::Deferred(request) => request.blocks_keyboard(),
                _ => false,
            })
            .unwrap_or(false)
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod repeat_lifecycle_tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicU32};
    use std::time::{Duration, Instant};

    #[test]
    fn rgb_selected_device_must_match_bound_handle_before_worker() {
        let (mut app, ctx, _) = app();
        let (hid, recorder) = crate::hid::HidDevice::test_beta2_ak820_device(
            crate::app::ak820_beta1_profiles::ProfileId::Main,
            true,
        );
        let image = hid.ak820_image().unwrap();
        let selected = crate::device::test_usb_device("3-1", 1, crate::ak820::PID);
        let mut old = selected.clone();
        old.instance_token.push_str("-old");
        recorder.bind_device(old);
        app.device_manager.replace_devices(vec![selected.clone()]);
        app.selected_device = Some(0);
        app.hid_device = Some(hid);
        let request = Ak820RgbWrite {
            request_id: 1,
            generation: app.connection_generation,
            device: selected,
            image,
            old: app.rgb_settings.clone(),
            value: app.rgb_settings.clone(),
            save: true,
        };
        assert_eq!(
            app.start_vial_hid_operation(&ctx, VialHidOperation::Ak820RgbWrite(request)),
            VialHidTaskStart::NoDevice
        );
        assert!(app.vial_hid_task.is_none());
        assert!(app.hid_device.is_none());
        assert!(recorder.requests().is_empty());
    }

    #[test]
    fn rgb_delayed_worker_coalesces_and_quit_discards_draft() {
        let (mut app, ctx, _) = app();
        let (hid, recorder) = crate::hid::HidDevice::test_beta2_ak820_device(
            crate::app::ak820_beta1_profiles::ProfileId::Main,
            true,
        );
        let image = hid.ak820_image().unwrap();
        app.hid_device = Some(hid);
        let device = crate::device::test_usb_device("3-1", 1, crate::ak820::PID);
        recorder.bind_device(device.clone());
        app.device_manager.replace_devices(vec![device.clone()]);
        app.selected_device = Some(0);
        app.settings_tab = SettingsTab::Rgb;
        app.main_menu_tab = MainMenuTab::Settings;
        app.rgb_settings = RgbSettingsState {
            supported: true,
            kind: RgbSupportKind::VialRgb,
            effect: 2,
            brightness: 100,
            speed: 20,
            max_brightness: 160,
            supported_effects: vec![0, 1, 2],
            last_enabled_effect: 2,
            ..Default::default()
        };
        let request = Ak820RgbWrite {
            request_id: 1,
            generation: app.connection_generation,
            device,
            image,
            old: app.rgb_settings.clone(),
            value: app.rgb_settings.clone(),
            save: true,
        };
        let (release, gate) = std::sync::mpsc::channel();
        assert_eq!(
            app.start_vial_hid_operation_with_runner(
                &ctx,
                VialHidOperation::Ak820RgbWrite(request),
                move |_, _, _, cancel| {
                    gate.recv_timeout(Duration::from_secs(8)).unwrap();
                    assert!(cancel.load(std::sync::atomic::Ordering::Acquire));
                    Err(anyhow::anyhow!("cancelled"))
                }
            ),
            VialHidTaskStart::Started
        );
        assert!(app.queue_ak820_rgb(&ctx, |value| value.brightness = 110));
        assert!(app.queue_ak820_rgb(&ctx, |value| value.brightness = 120));
        assert_eq!(app.rgb_draft.as_ref().unwrap().value.brightness, 120);
        assert_eq!(app.rgb_settings.brightness, 100);
        app.pending_handed_swap = Some((0, 0, crate::keyboard::KeyBinding::Vial(4)));
        app.debounce_touchpad_setting_write(&ctx, "fixture".into(), 134, 0, 1);
        app.request_explicit_quit(&ctx);
        assert!(app.exit_after_hid_write && app.rgb_exit_pending);
        assert!(app.rgb_draft.is_none());
        let started = Instant::now();
        let release_thread = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(5100));
            release.send(()).unwrap();
        });
        let mut frames = 0;
        let mut emitted_close = false;
        while app.vial_hid_task_active() {
            assert!(started.elapsed() < Duration::from_secs(8));
            let frame_started = Instant::now();
            let events = vec![egui::Event::PointerButton {
                pos: egui::pos2(400.0, 300.0),
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            }];
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 1000.0),
                )),
                events,
                ..Default::default()
            };
            let rendered = ctx.run_ui(input, |ui| {
                eframe::App::ui(&mut app, ui, &mut eframe::Frame::_new_kittest());
            });
            assert!(!rendered.shapes.is_empty());
            let output = ctx.run_ui(egui::RawInput::default(), |_ui| {
                app.update_native_background_for_platform(
                    &ctx,
                    started.elapsed().as_secs_f64(),
                    false,
                    false,
                    "windows",
                );
            });
            emitted_close |= output.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .contains(&egui::ViewportCommand::Close);
            assert!(frame_started.elapsed() < Duration::from_secs(1));
            frames += 1;
            std::thread::sleep(Duration::from_millis(20));
        }
        release_thread.join().unwrap();
        assert!(started.elapsed() >= Duration::from_secs(5));
        assert!(frames > 3);
        assert!(emitted_close);
        assert_eq!(app.pending_qmk_settings_write_value(134), Some(1));
        assert!(app.pending_handed_swap.is_some());
        assert!(app.hid_device.is_none());
        app.settings_tab = SettingsTab::AutoShift;
        app.auto_shift_timeout = Some(200);
        for pressed in [true, false] {
            let input = egui::RawInput {
                events: vec![
                    egui::Event::PointerMoved(egui::pos2(400.0, 300.0)),
                    egui::Event::PointerButton {
                        pos: egui::pos2(400.0, 300.0),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
                ..Default::default()
            };
            let rendered = ctx.run_ui(input, |ui| {
                eframe::App::ui(&mut app, ui, &mut eframe::Frame::_new_kittest());
            });
            assert!(!rendered.shapes.is_empty());
        }
        assert_eq!(app.auto_shift_timeout, Some(200));
        assert!(app.pending_handed_swap.is_some());
        assert!(app.hid_device.is_none());
        assert!(app.rgb_exit_ready);
        assert_rgb_terminal_close_event(&mut app, &ctx);
        assert_eq!(app.rgb_settings.brightness, 100);
        assert!(app.rgb_uncertain.is_some());
        assert!(recorder.requests().is_empty());
    }

    #[test]
    fn rgb_stale_completion_does_not_restore_handle_or_value() {
        let (mut app, ctx, _) = app();
        let (hid, recorder) = crate::hid::HidDevice::test_beta2_ak820_device(
            crate::app::ak820_beta1_profiles::ProfileId::Main,
            true,
        );
        let image = hid.ak820_image().unwrap();
        app.hid_device = Some(hid);
        let device = crate::device::test_usb_device("3-1", 1, crate::ak820::PID);
        recorder.bind_device(device.clone());
        app.device_manager.replace_devices(vec![device.clone()]);
        app.selected_device = Some(0);
        app.settings_tab = SettingsTab::Rgb;
        app.main_menu_tab = MainMenuTab::Settings;
        app.rgb_settings = RgbSettingsState {
            supported: true,
            kind: RgbSupportKind::VialRgb,
            effect: 2,
            brightness: 100,
            speed: 20,
            max_brightness: 160,
            supported_effects: vec![0, 1, 2],
            last_enabled_effect: 2,
            ..Default::default()
        };
        let mut value = app.rgb_settings.clone();
        value.brightness = 120;
        let request = Ak820RgbWrite {
            request_id: 1,
            generation: app.connection_generation,
            device,
            image,
            old: app.rgb_settings.clone(),
            value,
            save: true,
        };
        let (release, gate) = std::sync::mpsc::channel();
        assert_eq!(
            app.start_vial_hid_operation_with_runner(
                &ctx,
                VialHidOperation::Ak820RgbWrite(request),
                move |_, _, _, _| {
                    gate.recv_timeout(Duration::from_secs(5)).unwrap();
                    Ok(VialHidOutcome::Ak820RgbWritten)
                }
            ),
            VialHidTaskStart::Started
        );
        assert!(app.queue_ak820_rgb(&ctx, |value| value.brightness = 130));
        app.rgb_uncertain = Some(app.connection_generation);
        app.connection_generation = app.connection_generation.wrapping_add(1);
        app.status_msg = "new connection".into();
        app.poll_vial_hid_task(&ctx);
        assert!(app.rgb_uncertain.is_none());
        assert!(app.rgb_draft.is_none());
        release.send(()).unwrap();
        poll(&mut app, &ctx, &mut 0);
        assert!(app.hid_device.is_none());
        assert!(app.rgb_draft.is_none());
        assert_eq!(app.rgb_settings.brightness, 100);
        assert_eq!(app.status_msg, "new connection");
        assert!(recorder.requests().is_empty());
    }

    #[test]
    fn rgb_same_generation_rejects_changed_image_path_or_instance() {
        for change in ["image", "path", "instance"] {
            let (mut app, ctx, _) = app();
            let (hid, recorder) = crate::hid::HidDevice::test_beta2_ak820_device(
                crate::app::ak820_beta1_profiles::ProfileId::Main,
                true,
            );
            let actual = hid.ak820_image().unwrap();
            app.hid_device = Some(hid);
            let device = crate::device::test_usb_device("3-1", 1, crate::ak820::PID);
            recorder.bind_device(device.clone());
            app.device_manager.replace_devices(vec![device.clone()]);
            app.selected_device = Some(0);
            app.rgb_settings.brightness = 100;
            let mut value = app.rgb_settings.clone();
            value.brightness = 120;
            let image = actual;
            let request = Ak820RgbWrite {
                request_id: 1,
                generation: app.connection_generation,
                device: device.clone(),
                image,
                old: app.rgb_settings.clone(),
                value,
                save: true,
            };
            let (release, gate) = std::sync::mpsc::channel();
            assert_eq!(
                app.start_vial_hid_operation_with_runner(
                    &ctx,
                    VialHidOperation::Ak820RgbWrite(request),
                    move |_, _, _, _| {
                        gate.recv_timeout(Duration::from_secs(5)).unwrap();
                        Ok(VialHidOutcome::Ak820RgbWritten)
                    }
                ),
                VialHidTaskStart::Started
            );
            let mut replacement = device;
            if change == "path" {
                replacement.path.push_str("-changed");
            }
            if change == "instance" {
                replacement.instance_token.push_str("-changed");
            }
            app.device_manager.replace_devices(vec![replacement]);
            release.send(()).unwrap();
            if change == "image" {
                let task = app.vial_hid_task.as_mut().unwrap();
                let mut result = task.receiver.recv_timeout(Duration::from_secs(5)).unwrap();
                if let VialHidOperation::Ak820RgbWrite(request) = &mut result.operation {
                    request.image = crate::ak820::ImageContract::Beta3(
                        crate::app::ak820_beta1_profiles::ProfileId::Main,
                    );
                }
                let (sender, receiver) = std::sync::mpsc::channel();
                assert!(sender.send(result).is_ok());
                task.receiver = receiver;
            }
            poll(&mut app, &ctx, &mut 0);
            assert!(app.hid_device.is_none(), "{change}");
            assert_eq!(app.rgb_settings.brightness, 100, "{change}");
            assert!(recorder.requests().is_empty());
        }
    }

    #[test]
    fn rgb_window_close_only_cancels_on_committed_exit() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        for choice in ["tray", "ask_cancel", "unlock", "close"] {
            let (mut app, ctx, _) = app();
            app.app_settings.language = crate::i18n::Language::English;
            let (hid, recorder) = crate::hid::HidDevice::test_beta2_ak820_device(
                crate::app::ak820_beta1_profiles::ProfileId::Main,
                true,
            );
            let image = hid.ak820_image().unwrap();
            app.hid_device = Some(hid);
            let device = crate::device::test_usb_device("3-1", 1, crate::ak820::PID);
            recorder.bind_device(device.clone());
            app.device_manager.replace_devices(vec![device.clone()]);
            app.selected_device = Some(0);
            app.settings_tab = SettingsTab::Rgb;
            app.main_menu_tab = MainMenuTab::Settings;
            let request = Ak820RgbWrite {
                request_id: 1,
                generation: app.connection_generation,
                device,
                image,
                old: app.rgb_settings.clone(),
                value: app.rgb_settings.clone(),
                save: true,
            };
            let (release, gate) = std::sync::mpsc::channel();
            let expect_cancel = choice == "close";
            assert_eq!(
                app.start_vial_hid_operation_with_runner(
                    &ctx,
                    VialHidOperation::Ak820RgbWrite(request),
                    move |_, _, _, cancel| {
                        gate.recv_timeout(Duration::from_secs(5)).unwrap();
                        assert_eq!(
                            cancel.load(std::sync::atomic::Ordering::Acquire),
                            expect_cancel
                        );
                        Err(anyhow::anyhow!("fixture"))
                    }
                ),
                VialHidTaskStart::Started
            );
            app.app_settings.close_to_tray_behavior = match choice {
                "tray" => CloseToTrayBehavior::Tray,
                "ask_cancel" => CloseToTrayBehavior::Ask,
                _ => CloseToTrayBehavior::Close,
            };
            app.app_settings.text_expander_enabled = choice == "ask_cancel";
            app.unlock_open = choice == "unlock";
            if choice == "close" {
                app.debounce_touchpad_setting_write(&ctx, "fixture".into(), 134, 0, 1);
            }
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 1000.0),
                )),
                ..Default::default()
            };
            input
                .viewports
                .get_mut(&egui::ViewportId::ROOT)
                .unwrap()
                .events
                .push(egui::ViewportEvent::Close);
            let _ = ctx.run_ui(input, |_ui| app.handle_close_to_tray(&ctx));
            if choice == "ask_cancel" {
                assert!(app.close_to_tray_prompt_open);
                let prompt_input = || egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1100.0, 1000.0),
                    )),
                    ..Default::default()
                };
                let _ = ctx.run_ui(prompt_input(), |_ui| app.draw_close_to_tray_prompt(&ctx));
                let output = ctx.run_ui(prompt_input(), |_ui| app.draw_close_to_tray_prompt(&ctx));
                let pos = text_position(&output, "Cancel");
                for pressed in [true, false] {
                    let input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1100.0, 1000.0),
                        )),
                        events: vec![
                            egui::Event::PointerMoved(pos),
                            egui::Event::PointerButton {
                                pos,
                                button: egui::PointerButton::Primary,
                                pressed,
                                modifiers: Default::default(),
                            },
                        ],
                        ..Default::default()
                    };
                    let _ = ctx.run_ui(input, |_ui| app.draw_close_to_tray_prompt(&ctx));
                }
                assert!(!app.close_to_tray_prompt_open);
            }
            assert_eq!(app.rgb_exit_pending, expect_cancel, "{choice}");
            assert_eq!(app.exit_after_hid_write, expect_cancel, "{choice}");
            release.send(()).unwrap();
            poll(&mut app, &ctx, &mut 0);
            if choice == "close" {
                let output = ctx.run_ui(egui::RawInput::default(), |_ui| {
                    app.finish_deferred_exit_after_hid_write(&ctx)
                });
                assert!(output.viewport_output[&egui::ViewportId::ROOT]
                    .commands
                    .contains(&egui::ViewportCommand::Close));
                assert!(app.rgb_exit_ready);
                assert_rgb_terminal_close_event(&mut app, &ctx);
                assert_eq!(app.pending_qmk_settings_write_value(134), Some(1));
            }
            assert!(recorder.requests().is_empty());
            #[cfg(any(target_os = "windows", target_os = "macos"))]
            app.release_tray_before_gui_drop();
        }
    }

    fn assert_rgb_terminal_close_event(app: &mut EntropyApp, ctx: &egui::Context) {
        let mut input = egui::RawInput::default();
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .events
            .push(egui::ViewportEvent::Close);
        let output = ctx.run_ui(input, |_ui| app.handle_close_to_tray(ctx));
        assert!(!output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::CancelClose));
        assert!(app.rgb_exit_pending && app.rgb_exit_ready);
        assert_eq!(
            app.start_vial_hid_operation(ctx, VialHidOperation::Lock),
            VialHidTaskStart::Busy
        );
    }

    fn app() -> (EntropyApp, egui::Context, crate::hid::TestHidRecorder) {
        let mut app = EntropyApp::new_inert_for_test();
        assert!(app.device_manager.devices().is_empty());
        assert!(matches!(app.update_check, UpdateCheckState::Idle));
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(hid);
        app.app_settings.language = crate::i18n::Language::Russian;
        app.display_settings.supported = true;
        app.display_settings.clock_settings_supported = true;
        app.display_settings.clock_background_asset_supported = true;
        app.display_settings.pictograms.supported = Some(true);
        app.display_settings.pictograms.loaded = true;
        app.keycode_picker.macro_count = 4;
        let ctx = egui::Context::default();
        let mut fonts = egui::FontDefinitions::default();
        for family in ["display_preview", "clock_montserrat"] {
            fonts.families.insert(
                egui::FontFamily::Name(family.into()),
                fonts.families[&egui::FontFamily::Proportional].clone(),
            );
        }
        ctx.set_fonts(fonts);
        (app, ctx, recorder)
    }

    fn frame(
        app: &mut EntropyApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 1000.0),
                )),
                events,
                ..Default::default()
            },
            |ui| app.draw_display_settings_page(ui, ui.max_rect()),
        )
    }

    fn text_position(output: &egui::FullOutput, label: &str) -> egui::Pos2 {
        output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == label => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing rendered text: {label}"))
    }

    fn tab(app: &mut EntropyApp, ctx: &egui::Context, label: &str) {
        let output = frame(app, ctx, Vec::new());
        let pos = text_position(&output, label);
        for pressed in [true, false] {
            frame(
                app,
                ctx,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
    }

    fn poll(app: &mut EntropyApp, ctx: &egui::Context, saved: &mut usize) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.vial_hid_task_active() {
            assert!(
                Instant::now() < deadline,
                "same-session worker did not finish"
            );
            app.poll_vial_hid_task_with_settings_save(ctx, |_| *saved += 1);
            frame(app, ctx, Vec::new());
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn repeat_lifecycle_two_pictogram_uploads_use_one_app_and_remain_renderable() {
        let (mut app, ctx, recorder) = app();
        tab(&mut app, &ctx, "Пиктограммы");
        let generation = app.connection_generation;
        let mut saved = 0;
        for byte in [0xAA, 0x55] {
            recorder.respond_with(test_pictogram_upload_responses(true, 0));
            app.display_settings.pictograms.source_levels =
                pictogram_bitmap_levels(&[byte; PICTOGRAM_BYTES]);
            assert!(app.apply_current_pictogram(&ctx));
            frame(&mut app, &ctx, Vec::new());
            poll(&mut app, &ctx, &mut saved);
            assert_eq!(app.connection_generation, generation);
            assert!(app.hid_device.is_some());
            assert!(app.display_settings.pictograms.loaded);
            assert!(!app.display_settings.pictograms.loading);
            assert!(!app.vial_hid_task_blocks_user_action());
            let output = frame(&mut app, &ctx, Vec::new());
            text_position(&output, "Выбрать");
            tab(&mut app, &ctx, "Выбрать");
            assert!(
                egui::Popup::is_any_open(&ctx),
                "Select did not reopen after upload"
            );
            egui::Popup::close_all(&ctx);
        }
        assert_eq!(saved, 0);
        assert_eq!(
            recorder.requests().iter().filter(|r| r[0] == 0xC9).count(),
            2
        );
    }

    fn directory() -> tempfile::TempDir {
        let root = std::env::var_os("ENTROPY_TEST_ARTIFACT_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        tempfile::Builder::new()
            .prefix("repeat-lifecycle-")
            .tempdir_in(root)
            .unwrap()
    }

    #[test]
    fn repeat_lifecycle_failed_recovery_read_stops_until_explicit_retry() {
        let (mut app, ctx, recorder) = app();
        tab(&mut app, &ctx, "Пиктограммы");
        let before = app.display_settings.pictograms.library.clone();
        app.display_settings.pictograms.editor_name = "Unsaved draft".into();
        let draft = app.display_settings.pictograms.source_levels.clone();
        recorder.respond_with(test_pictogram_upload_responses(true, 4));
        // A real firmware error on the automatic recovery QUERY, not a timeout
        // or an unsupported capability. Known support must remain known.
        let mut read_error = [0; 32];
        read_error[0] = 0xC0;
        read_error[1] = 4;
        recorder.respond_with([read_error]);
        assert!(app.apply_current_pictogram(&ctx));
        for _ in 0..30 {
            app.poll_vial_hid_task_with_settings_save(&ctx, |_| panic!("no settings write"));
            frame(&mut app, &ctx, Vec::new());
            std::thread::sleep(Duration::from_millis(2));
        }
        let queries = recorder.requests().iter().filter(|r| r[0] == 0xC0).count();
        assert_eq!(
            queries, 2,
            "upload QUERY + one recovery QUERY, not a render-driven retry loop"
        );
        assert!(!app.vial_hid_task_active());
        assert_eq!(app.display_settings.pictograms.supported, Some(true));
        assert!(!app.display_settings.pictograms.loaded);
        assert_eq!(app.display_settings.pictograms.source_levels, draft);
        assert_eq!(app.display_settings.pictograms.editor_name, "Unsaved draft");
        assert!(app
            .display_settings
            .pictograms
            .load_failure
            .as_ref()
            .unwrap()
            .1
            .contains("status 4"));
        // Merely navigating away and back is not a recovery attempt.
        tab(&mut app, &ctx, "Экран ожидания");
        tab(&mut app, &ctx, "Пиктограммы");
        assert_eq!(
            recorder.requests().iter().filter(|r| r[0] == 0xC0).count(),
            2
        );
        // The existing Select control is an explicit read retry boundary when
        // the snapshot is unknown; it must never seed another upload itself.
        recorder.respond_with(test_pictogram_read_responses(&before));
        tab(&mut app, &ctx, "Выбрать");
        poll(&mut app, &ctx, &mut 0);
        assert!(app.display_settings.pictograms.loaded);
        assert_eq!(app.display_settings.pictograms.library, before);
        assert!(app.display_settings.pictograms.load_failure.is_none());
        assert_eq!(app.display_settings.pictograms.source_levels, draft);
        recorder.respond_with(test_pictogram_upload_responses(true, 0));
        assert!(app.apply_current_pictogram(&ctx));
        poll(&mut app, &ctx, &mut 0);
        assert!(app.hid_device.is_some());
        assert!(app.display_settings.pictograms.loaded);
    }

    #[test]
    fn repeat_lifecycle_stale_read_result_cannot_overwrite_successor_state() {
        let (mut app, ctx, _) = app();
        let (release, gate) = std::sync::mpsc::channel();
        assert_eq!(
            app.start_vial_hid_operation_with_runner(
                &ctx,
                VialHidOperation::PictogramLoad {
                    preserve_editor: true
                },
                move |_, _, _, _| {
                    gate.recv_timeout(Duration::from_secs(3)).unwrap();
                    anyhow::bail!("old read failed")
                },
            ),
            VialHidTaskStart::Started
        );
        app.connection_generation += 1;
        app.display_settings.pictograms = PictogramSettingsState::default();
        app.display_settings.pictograms.editor_name = "Successor".into();
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.vial_hid_task_active() {
            assert!(Instant::now() < deadline);
            app.poll_vial_hid_task_with_settings_save(&ctx, |_| panic!("no save"));
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(app.hid_device.is_none(), "stale HID handle was restored");
        assert!(app.display_settings.pictograms.load_failure.is_none());
        assert_eq!(app.display_settings.pictograms.editor_name, "Successor");
    }

    fn start_background(
        app: &mut EntropyApp,
        ctx: &egui::Context,
        directory: &std::path::Path,
        gate: Option<std::sync::mpsc::Receiver<()>>,
    ) {
        let path = directory.join("source.png");
        let red = if app.display_settings.clock_background_preview_revision == 0 {
            32
        } else {
            224
        };
        image::RgbaImage::from_pixel(8, 8, image::Rgba([red, 87, 120, 255]))
            .save(&path)
            .unwrap();
        let cache = directory.join("cache.ehbg");
        assert_eq!(
            app.start_vial_hid_operation_with_runner(
                ctx,
                VialHidOperation::BackgroundUpload {
                    path,
                    fallback: [0; 3],
                    scale: StandbyBackgroundScale::Fill
                },
                move |hid, operation, progress: &AtomicU32, cancel: &AtomicBool| {
                    if let Some(gate) = gate {
                        gate.recv_timeout(Duration::from_secs(3)).unwrap();
                    }
                    let VialHidOperation::BackgroundUpload {
                        path,
                        fallback,
                        scale,
                    } = operation
                    else {
                        panic!("wrong operation")
                    };
                    hid.upload_standby_background_with_cache(
                        &path,
                        fallback,
                        scale,
                        progress,
                        cancel,
                        |package| Ok(std::fs::write(&cache, package)?),
                    )
                    .map(|upload| match upload {
                        Some(upload) => VialHidOutcome::BackgroundUploaded(upload),
                        None => VialHidOutcome::BackgroundCancelled {
                            cleared: progress.load(std::sync::atomic::Ordering::Relaxed) >= 150,
                        },
                    })
                },
            ),
            VialHidTaskStart::Started
        );
    }

    #[test]
    fn repeat_lifecycle_two_standby_uploads_keep_same_handle_and_gui_live() {
        let (mut app, ctx, recorder) = app();
        tab(&mut app, &ctx, "Экран ожидания");
        let directory = directory();
        let generation = app.connection_generation;
        let mut saved = 0;
        let mut previous_package = Vec::new();
        for revision in 1..=2 {
            recorder
                .respond_with(crate::app::standby_background::test_background_upload_responses(0));
            let (release, gate) = std::sync::mpsc::channel();
            start_background(&mut app, &ctx, directory.path(), Some(gate));
            // Deterministically stalled worker, not HID. GUI polling/rendering
            // must remain independent while the original owner is in-flight.
            let started = Instant::now();
            for _ in 0..3 {
                app.poll_vial_hid_task_with_settings_save(&ctx, |_| panic!("worker is gated"));
                let output = frame(&mut app, &ctx, Vec::new());
                text_position(&output, "Отмена");
                assert!(app.standby_background_upload_progress().is_some());
            }
            assert!(started.elapsed() < Duration::from_secs(2));
            release.send(()).unwrap();
            poll(&mut app, &ctx, &mut saved);
            assert_eq!(
                app.display_settings.clock_background_preview_revision,
                revision
            );
            assert!(app.standby_background_upload_progress().is_none());
            assert!(!app.vial_hid_task_blocks_user_action());
            assert!(app.hid_device.is_some());
            assert_eq!(app.connection_generation, generation);
            let output = frame(&mut app, &ctx, Vec::new());
            text_position(&output, "Загрузить");
            text_position(&output, "Сбросить");
            assert!(
                std::fs::metadata(directory.path().join("cache.ehbg"))
                    .unwrap()
                    .len()
                    > 256
            );
            let package = std::fs::read(directory.path().join("cache.ehbg")).unwrap();
            assert_ne!(
                package, previous_package,
                "second upload must replace different pixels"
            );
            previous_package = package;
        }
        assert_eq!(saved, 2);
        assert_eq!(
            recorder.requests().iter().filter(|r| r[0] == 0xB3).count(),
            2
        );
    }

    #[test]
    fn repeat_lifecycle_failed_or_cancelled_standby_upload_can_retry() {
        for cancel_first in [false, true] {
            let (mut app, ctx, recorder) = app();
            tab(&mut app, &ctx, "Экран ожидания");
            let directory = directory();
            let mut saved = 0;
            let generation = app.connection_generation;
            let (release, gate) = std::sync::mpsc::channel();
            if !cancel_first {
                recorder.respond_with(
                    crate::app::standby_background::test_background_upload_responses(4),
                );
            }
            start_background(&mut app, &ctx, directory.path(), Some(gate));
            if cancel_first {
                app.cancel_background_upload();
            }
            release.send(()).unwrap();
            poll(&mut app, &ctx, &mut saved);
            assert!(app.hid_device.is_some());
            assert_eq!(saved, 0);
            assert_eq!(app.display_settings.clock_background_preview_revision, 0);
            if cancel_first {
                assert!(recorder.requests().is_empty());
            } else {
                assert!(app.status_msg.contains("status 4"));
            }
            recorder
                .respond_with(crate::app::standby_background::test_background_upload_responses(0));
            start_background(&mut app, &ctx, directory.path(), None);
            poll(&mut app, &ctx, &mut saved);
            assert_eq!(saved, 1);
            assert_eq!(app.connection_generation, generation);
            assert!(app.hid_device.is_some());
            assert!(!app.vial_hid_task_blocks_user_action());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyboard::KeyBinding;
    use std::time::Duration;

    fn bind_unlock_fixture(app: &mut EntropyApp, recorder: &crate::hid::TestHidRecorder) {
        let device = crate::device::test_usb_device("3-1", 1, 0x42);
        recorder.bind_device(device.clone());
        app.device_manager.replace_devices(vec![device]);
        app.selected_device = Some(0);
    }

    #[test]
    fn vial_unlock_rejects_unbound_or_wrong_owner_before_first_packet() {
        for case in [
            "no_selection",
            "unbound",
            "path",
            "instance",
            "unknown_ak_image",
        ] {
            let mut app = EntropyApp::new_inert_for_test();
            let ctx = egui::Context::default();
            let (hid, recorder) = crate::hid::HidDevice::test_device();
            let mut selected = crate::device::test_usb_device(
                "3-1",
                1,
                if case == "unknown_ak_image" {
                    crate::ak820::PID
                } else {
                    0x42
                },
            );
            if case == "unknown_ak_image" {
                selected.vendor_id = crate::ak820::VID;
            }
            let mut bound = selected.clone();
            if case == "path" {
                bound.path.push_str("-old");
            }
            if case == "instance" {
                bound.instance_token.push_str("-old");
            }
            if case != "unbound" {
                recorder.bind_device(bound);
            }
            if case != "no_selection" {
                app.device_manager.replace_devices(vec![selected]);
                app.selected_device = Some(0);
            }
            app.hid_device = Some(hid);
            assert_eq!(
                app.start_vial_unlock(&ctx),
                VialHidTaskStart::NoDevice,
                "{case}"
            );
            assert!(app.vial_hid_task.is_none());
            assert_eq!(app.hid_device.is_some(), case == "unknown_ak_image");
            assert!(app.shared_hid_output.is_none());
            assert!(recorder.requests().is_empty());
        }
    }

    fn single_key_layout(keycode: u16) -> KeyboardLayout {
        KeyboardLayout {
            name: "Async key write".into(),
            rows: 1,
            cols: 1,
            keys: vec![PhysicalKey {
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h: 1.0,
                row: 0,
                col: 0,
                label: String::new(),
                rotation: 0.0,
                rotation_x: 0.0,
                rotation_y: 0.0,
                layout_condition: None,
            }],
            encoders: vec![],
            layers: vec![vec![keycode.into()]],
            encoder_layers: vec![vec![]],
            layer_names: vec!["Layer 0".into()],
            custom_keycodes: vec![],
            layout_options: vec![],
            live_features: Default::default(),
            supports_rgb: false,
            lighting_mode: None,
            firmware: FirmwareProtocol::Vial,
        }
    }

    fn background_layer_context() -> std::sync::Arc<DeferredDeviceLoadContext> {
        std::sync::Arc::new(DeferredDeviceLoadContext {
            json: std::sync::Arc::new(serde_json::json!({})),
            supported_qmk_settings: std::sync::Arc::new(Vec::new()),
            definition_fingerprint: 1,
            layer_count: 2,
            rows: 1,
            cols: 15,
            encoder_count: 0,
            macro_count: 0,
            macro_memory_bytes: None,
            tap_dance_count: 0,
            combo_count: 0,
            key_override_count: 0,
            alt_repeat_count: 0,
            modules_supported: false,
            touchpad_supported: false,
            bluetooth_supported: false,
            layer_leds_supported: false,
            rgb_supported: false,
            lighting_mode: None,
            supports_rmk_native_key_actions: false,
            supports_universal_symbols: false,
            supports_universal_russian_letters: false,
            supports_rmk_native_combo_output: false,
            supports_rmk_native_tap_dance_actions: false,
            supports_rmk_combo_layers: false,
        })
    }

    fn poll_until_vial_hid_idle(app: &mut EntropyApp, ctx: &egui::Context) {
        for _ in 0..100 {
            app.poll_vial_hid_task(ctx);
            if !app.vial_hid_task_active() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("Vial HID task did not finish");
    }

    #[test]
    fn vial_unlock_runner_busy_stale_and_committed_exit_stop_packets() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        for exit in [false, true] {
            let mut app = EntropyApp::new_inert_for_test();
            let ctx = egui::Context::default();
            let (hid, recorder) = crate::hid::HidDevice::test_device();
            bind_unlock_fixture(&mut app, &recorder);
            app.hid_device = Some(hid);
            let (release, gate) = std::sync::mpsc::channel();
            let status = std::sync::Arc::new(std::sync::Mutex::new(None));
            assert_eq!(
                app.start_vial_hid_operation_with_runner(
                    &ctx,
                    VialHidOperation::UnlockSession(status),
                    move |hid, operation, progress, cancel| {
                        gate.recv_timeout(Duration::from_secs(5)).unwrap();
                        run_vial_hid_operation_with_progress(hid, operation, progress, cancel)
                    }
                ),
                VialHidTaskStart::Started
            );
            assert_eq!(app.start_vial_unlock(&ctx), VialHidTaskStart::Busy);
            if exit {
                assert!(app.defer_exit_until_hid_write_returns(&ctx));
            } else {
                assert!(app.cancel_active_ak820_backup_capture());
                app.connection_generation = app.connection_generation.wrapping_add(1);
            }
            release.send(()).unwrap();
            poll_until_vial_hid_idle(&mut app, &ctx);
            assert!(recorder.requests().is_empty());
            assert!(app.hid_device.is_none());
            if exit {
                app.finish_deferred_exit_after_hid_write(&ctx);
                assert!(app.rgb_exit_ready);
            }
        }
    }

    #[test]
    fn vial_unlock_runner_real_transport_virtual_clock_confirms_without_start() {
        struct Clock(std::time::Duration);
        impl UnlockClock for Clock {
            fn now(&self) -> std::time::Duration {
                self.0
            }
            fn wait(&mut self, duration: std::time::Duration) {
                self.0 += duration;
            }
        }
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let mut app = EntropyApp::new_inert_for_test();
        let ctx = egui::Context::default();
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        bind_unlock_fixture(&mut app, &recorder);
        let mut status_response = [0xff; 32];
        status_response[..2].copy_from_slice(&[1, 0]);
        recorder.respond_with([status_response]);
        app.hid_device = Some(hid);
        app.unlock_open = true;
        let status = std::sync::Arc::new(std::sync::Mutex::new(None));
        assert_eq!(
            app.start_vial_hid_operation_with_runner(
                &ctx,
                VialHidOperation::UnlockSession(status),
                |hid, operation, _, cancel| {
                    let VialHidOperation::UnlockSession(status) = operation else {
                        unreachable!()
                    };
                    run_unlock_session(
                        &mut &*hid,
                        &mut Clock(std::time::Duration::ZERO),
                        cancel,
                        &status,
                    )
                    .map(VialHidOutcome::UnlockSession)
                }
            ),
            VialHidTaskStart::Started
        );
        poll_until_vial_hid_idle(&mut app, &ctx);
        assert_eq!(app.vial_unlocked, Some(true));
        assert_eq!(recorder.requests().len(), 1);
        assert!(recorder.requests()[0].starts_with(&[0xfe, 5]));
    }

    #[test]
    fn vial_unlock_runner_real_transport_full_sessions_and_first_fault() {
        struct Clock(Duration);
        impl UnlockClock for Clock {
            fn now(&self) -> Duration {
                self.0
            }
            fn wait(&mut self, duration: Duration) {
                self.0 += duration;
            }
        }
        for mode in ["new", "pending", "deny", "fault", "disconnect"] {
            let _storage = crate::app::app_storage::ScopedTempConfig::new();
            let mut app = EntropyApp::new_inert_for_test();
            app.app_settings.language = crate::i18n::Language::English;
            let ctx = egui::Context::default();
            let (hid, recorder) = if mode == "fault" || mode == "disconnect" {
                crate::hid::HidDevice::test_device_with_fault_after_requests(Some((
                    2,
                    if mode == "fault" {
                        crate::hid::TestHidFault::Timeout
                    } else {
                        crate::hid::TestHidFault::Disconnect
                    },
                )))
            } else {
                crate::hid::HidDevice::test_device()
            };
            bind_unlock_fixture(&mut app, &recorder);
            let mut initial = [0xff; 32];
            initial[..4].copy_from_slice(&[0, u8::from(mode == "pending"), 1, 2]);
            let mut pending = [0; 32];
            pending[..3].copy_from_slice(&[0, 1, 3]);
            let mut regressed = [0; 32];
            regressed[..3].copy_from_slice(&[0, 1, 5]);
            let mut confirm = [0; 32];
            confirm[..3].copy_from_slice(&[0, 1, 0]);
            let mut success = [0xff; 32];
            success[..2].copy_from_slice(&[1, 0]);
            let denied = [0; 32];
            let mut responses = vec![initial];
            if mode != "pending" {
                responses.push([0; 32]);
            }
            if mode == "deny" {
                responses.push(denied);
            } else {
                responses.extend([pending, regressed, confirm, success]);
            }
            recorder.respond_with(responses);
            app.hid_device = Some(hid);
            app.unlock_open = true;
            let status = std::sync::Arc::new(std::sync::Mutex::new(None));
            assert_eq!(
                app.start_vial_hid_operation_with_runner(
                    &ctx,
                    VialHidOperation::UnlockSession(status.clone()),
                    |hid, operation, _, cancel| {
                        let VialHidOperation::UnlockSession(status) = operation else {
                            unreachable!()
                        };
                        run_unlock_session(&mut &*hid, &mut Clock(Duration::ZERO), cancel, &status)
                            .map(VialHidOutcome::UnlockSession)
                    }
                ),
                VialHidTaskStart::Started
            );
            poll_until_vial_hid_idle(&mut app, &ctx);
            let commands: Vec<_> = recorder
                .requests()
                .iter()
                .map(|request| request[1])
                .collect();
            match mode {
                "new" => {
                    assert_eq!(commands, [5, 6, 7, 7, 7, 5]);
                    assert_eq!(app.vial_unlocked, Some(true));
                }
                "pending" => {
                    assert_eq!(commands, [5, 7, 7, 7, 5]);
                    assert_eq!(app.vial_unlocked, Some(true));
                }
                "deny" | "fault" | "disconnect" => {
                    assert_eq!(commands, [5, 6, 7]);
                    assert_eq!(app.vial_unlocked, None);
                    assert!(app.hid_device.is_none());
                    if mode == "fault" {
                        assert!(app.status_msg.contains("response timed out"));
                    }
                    if mode == "disconnect" {
                        assert!(app.status_msg.contains("disconnected"));
                    }
                }
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn vial_unlock_runner_deadline_starts_after_ack_or_initial_observation() {
        use std::sync::{
            atomic::{AtomicU64, Ordering},
            Arc,
        };
        struct Clock(Arc<AtomicU64>);
        impl UnlockClock for Clock {
            fn now(&self) -> Duration {
                Duration::from_millis(self.0.load(Ordering::Acquire))
            }
            fn wait(&mut self, duration: Duration) {
                self.0
                    .fetch_add(duration.as_millis() as u64, Ordering::AcqRel);
            }
        }
        for pending in [false, true] {
            let _storage = crate::app::app_storage::ScopedTempConfig::new();
            let mut app = EntropyApp::new_inert_for_test();
            let ctx = egui::Context::default();
            let (hid, recorder) = crate::hid::HidDevice::test_device();
            bind_unlock_fixture(&mut app, &recorder);
            let time = Arc::new(AtomicU64::new(0));
            let responder_time = time.clone();
            let dispatches = Arc::new(std::sync::Mutex::new(Vec::new()));
            let observed = dispatches.clone();
            recorder.respond_by(move |request| {
                let mut response = [0xff; 32];
                match request[1] {
                    5 => {
                        responder_time.fetch_add(5000, Ordering::AcqRel);
                        response[..4].copy_from_slice(&[0, u8::from(pending), 1, 2]);
                    }
                    6 => {
                        responder_time.fetch_add(8000, Ordering::AcqRel);
                        response = [0; 32];
                    }
                    7 => {
                        let mut times = observed.lock().unwrap();
                        times.push(responder_time.load(Ordering::Acquire));
                        response[..3].copy_from_slice(&[
                            0,
                            1,
                            if times.len() % 2 == 0 { 5 } else { 3 },
                        ]);
                    }
                    _ => unreachable!(),
                }
                Some(Ok(response))
            });
            app.hid_device = Some(hid);
            let status = Arc::new(std::sync::Mutex::new(None));
            let worker_status = status.clone();
            assert_eq!(
                app.start_vial_hid_operation_with_runner(
                    &ctx,
                    VialHidOperation::UnlockSession(worker_status),
                    move |hid, operation, _, cancel| {
                        let VialHidOperation::UnlockSession(status) = operation else {
                            unreachable!()
                        };
                        run_unlock_session(&mut &*hid, &mut Clock(time), cancel, &status)
                            .map(VialHidOutcome::UnlockSession)
                    }
                ),
                VialHidTaskStart::Started
            );
            poll_until_vial_hid_idle(&mut app, &ctx);
            let times = dispatches.lock().unwrap();
            let origin = if pending { 5000 } else { 13000 };
            assert_eq!(times.len(), 74);
            assert_eq!(times[0], origin + 200);
            assert_eq!(*times.last().unwrap(), origin + 14800);
            assert!(times.windows(2).all(|pair| pair[1] - pair[0] == 200));
            assert_eq!(status.lock().unwrap().as_ref().unwrap().counter, 5);
            assert_eq!(
                recorder.requests().iter().filter(|r| r[1] == 6).count(),
                usize::from(!pending)
            );
            assert_eq!(app.vial_unlocked, None);
        }
    }

    #[test]
    fn vial_unlock_runner_rejects_changed_target_or_image_and_ignores_stale() {
        for change in ["path", "instance", "owner", "image", "generation"] {
            let _storage = crate::app::app_storage::ScopedTempConfig::new();
            let mut app = EntropyApp::new_inert_for_test();
            let ctx = egui::Context::default();
            let device = crate::device::test_usb_device("3-1", 1, 0x42);
            app.device_manager.replace_devices(vec![device.clone()]);
            app.selected_device = Some(0);
            let (hid, recorder) = crate::hid::HidDevice::test_device();
            recorder.bind_device(device.clone());
            app.hid_device = Some(hid);
            let status = std::sync::Arc::new(std::sync::Mutex::new(None));
            assert_eq!(
                app.start_vial_hid_operation_with_runner(
                    &ctx,
                    VialHidOperation::UnlockSession(status),
                    |_, _, _, _| Ok(VialHidOutcome::UnlockSession(UnlockResult::Confirmed))
                ),
                VialHidTaskStart::Started
            );
            let task = app.vial_hid_task.as_mut().unwrap();
            let mut result = task.receiver.recv_timeout(Duration::from_secs(5)).unwrap();
            if change == "image" {
                result.unlock_image = Some(crate::ak820::ImageContract::Beta2(
                    crate::app::ak820_beta1_profiles::ProfileId::Main,
                ));
            }
            let (sender, receiver) = std::sync::mpsc::channel();
            assert!(sender.send(result).is_ok());
            task.receiver = receiver;
            if change == "path" || change == "instance" {
                let mut replacement = device.clone();
                if change == "path" {
                    replacement.path.push_str("-changed");
                } else {
                    replacement.instance_token.push_str("-changed");
                }
                app.device_manager.replace_devices(vec![replacement]);
            }
            if change == "owner" {
                let mut changed = device.clone();
                changed.instance_token.push_str("-changed");
                recorder.bind_device(changed);
            }
            if change == "generation" {
                app.connection_generation = app.connection_generation.wrapping_add(1);
                app.vial_unlocked = Some(true);
            }
            poll_until_vial_hid_idle(&mut app, &ctx);
            assert!(recorder.requests().is_empty());
            assert!(app.hid_device.is_none());
            assert_eq!(
                app.vial_unlocked,
                if change == "generation" {
                    Some(true)
                } else {
                    None
                }
            );
        }
    }

    #[test]
    fn vial_unlock_runner_confirmation_boundary_and_transport_fault_keep_first_cause() {
        use std::sync::{
            atomic::{AtomicU64, Ordering},
            Arc,
        };
        struct Clock(Arc<AtomicU64>);
        impl UnlockClock for Clock {
            fn now(&self) -> Duration {
                Duration::from_millis(self.0.load(Ordering::Acquire))
            }
            fn wait(&mut self, duration: Duration) {
                self.0
                    .fetch_add(duration.as_millis() as u64, Ordering::AcqRel);
            }
        }
        for mode in ["cancel", "exit", "deadline", "fault"] {
            let _storage = crate::app::app_storage::ScopedTempConfig::new();
            let mut app = EntropyApp::new_inert_for_test();
            app.app_settings.language = crate::i18n::Language::English;
            let ctx = egui::Context::default();
            let (hid, recorder) = crate::hid::HidDevice::test_device();
            bind_unlock_fixture(&mut app, &recorder);
            let time = Arc::new(AtomicU64::new(0));
            let responder_time = time.clone();
            let (entered, entered_rx) = std::sync::mpsc::channel();
            let (release, release_rx) = std::sync::mpsc::channel();
            recorder.respond_by(move |request| {
                let mut response = [0xff; 32];
                match request[1] {
                    5 => response[..4].copy_from_slice(&[0, 0, 1, 2]),
                    6 => response = [0; 32],
                    7 => {
                        if matches!(mode, "cancel" | "exit") {
                            entered.send(()).unwrap();
                            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                        } else {
                            responder_time.store(15_000, Ordering::Release);
                        }
                        if mode == "fault" {
                            return Some(Err("HID timeout — device did not respond".into()));
                        }
                        response[..3].copy_from_slice(&[0, 1, 0]);
                    }
                    _ => unreachable!(),
                }
                Some(Ok(response))
            });
            app.hid_device = Some(hid);
            app.unlock_open = true;
            let status = Arc::new(std::sync::Mutex::new(None));
            assert_eq!(
                app.start_vial_hid_operation_with_runner(
                    &ctx,
                    VialHidOperation::UnlockSession(status),
                    move |hid, operation, _, cancel| {
                        let VialHidOperation::UnlockSession(status) = operation else {
                            unreachable!()
                        };
                        run_unlock_session(&mut &*hid, &mut Clock(time), cancel, &status)
                            .map(VialHidOutcome::UnlockSession)
                    }
                ),
                VialHidTaskStart::Started
            );
            if matches!(mode, "cancel" | "exit") {
                entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                if mode == "exit" {
                    assert!(app.defer_exit_until_hid_write_returns(&ctx));
                } else {
                    app.cancel_vial_unlock_wait();
                }
                release.send(()).unwrap();
            }
            poll_until_vial_hid_idle(&mut app, &ctx);
            assert_eq!(
                recorder.requests().iter().map(|r| r[1]).collect::<Vec<_>>(),
                [5, 6, 7]
            );
            assert!(app.hid_device.is_none());
            if mode == "exit" {
                app.finish_deferred_exit_after_hid_write(&ctx);
                assert!(app.rgb_exit_ready);
            } else {
                assert_eq!(app.vial_unlocked, None);
                assert!(app.status_msg.contains(match mode {
                    "cancel" => "Stopped waiting",
                    "deadline" => "timed out",
                    _ => "response timed out",
                }));
            }
        }
    }

    #[test]
    fn pending_unlock_is_not_ready_and_does_not_restart_physical_hold() {
        // The firmware keeps the unlocked bit set if another unlock sequence
        // starts on an unlocked board, but gates normal commands while pending.
        for unlocked in [0, 1] {
            let (hid, recorder) = crate::hid::HidDevice::test_device();
            let mut status = [0xFF; 32];
            status[..4].copy_from_slice(&[unlocked, 1, 0, 0]);
            recorder.respond_with([status]);
            let result = run_vial_hid_operation(&hid, VialHidOperation::UnlockStart).unwrap();
            assert!(matches!(
                result,
                VialHidOutcome::UnlockStarted {
                    unlocked: false,
                    ..
                }
            ));
            assert_eq!(
                recorder.requests().len(),
                1,
                "an existing physical hold must not receive another FE06"
            );
            assert!(recorder.requests()[0].starts_with(&[0xFE, 5]));
        }
    }

    #[test]
    fn pending_unlock_status_is_not_exposed_as_available_for_normal_commands() {
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        let mut status = [0xFF; 32];
        status[..4].copy_from_slice(&[1, 1, 0, 0]);
        recorder.respond_with([status]);
        let (ready, keys) = hid.get_unlock_status().unwrap();
        assert!(!ready);
        assert_eq!(keys, vec![(0, 0)]);
    }

    #[test]
    fn pending_unlock_poll_does_not_complete_on_the_unlocked_bit_alone() {
        let mut app = EntropyApp::new_inert_for_test();
        app.unlock_open = true;
        app.finish_vial_unlock_start(false, vec![(0, 0)]);
        app.finish_vial_unlock_poll(true, true, 10);
        assert!(app.unlock_open && app.vial_unlock_polling);
        assert_eq!(app.vial_unlocked, Some(false));
        app.finish_vial_unlock_poll(true, false, 0);
        assert!(!app.unlock_open && !app.vial_unlock_polling);
        assert_eq!(app.vial_unlocked, Some(true));
    }

    #[test]
    fn stopped_unlock_poll_releases_ui_instead_of_polling_forever() {
        let mut app = EntropyApp::new_inert_for_test();
        app.unlock_open = true;
        app.finish_vial_unlock_start(false, vec![(0, 0)]);
        app.finish_vial_unlock_poll(false, false, 0);
        assert!(!app.unlock_open && !app.vial_unlock_polling);
        assert_eq!(app.vial_unlocked, Some(false));
        assert!(app.macro_auto_unlock_cancelled);
    }

    #[test]
    fn unlock_start_uses_status_then_start_commands() {
        let (hid, recorder) = crate::hid::HidDevice::test_device();

        let outcome = run_vial_hid_operation(&hid, VialHidOperation::UnlockStart).unwrap();

        assert!(matches!(
            outcome,
            VialHidOutcome::UnlockStarted {
                unlocked: false,
                ..
            }
        ));
        let requests = recorder.requests();
        assert_eq!(&requests[0][..2], &[0xFE, 0x05]);
        assert_eq!(&requests[1][..2], &[0xFE, 0x06]);
    }

    #[test]
    fn matrix_poll_uses_via_switch_matrix_command() {
        let (hid, recorder) = crate::hid::HidDevice::test_device();

        let outcome = run_vial_hid_operation(
            &hid,
            VialHidOperation::Matrix {
                rows: 2,
                cols: 3,
                remember_ever_pressed: true,
            },
        )
        .unwrap();

        assert!(matches!(
            outcome,
            VialHidOutcome::Matrix(pressed) if pressed == vec![false; 6]
        ));
        let requests = recorder.requests();
        assert_eq!(&requests[0][..2], &[0x02, 0x03]);
    }

    #[test]
    fn successful_lock_exits_protected_settings_pages() {
        for tab in [SettingsTab::Macros, SettingsTab::TapDance] {
            let ctx = egui::Context::default();
            let mut app = EntropyApp::new_inert_for_test();
            let (hid, recorder) = crate::hid::HidDevice::test_device();
            app.hid_device = Some(hid);
            app.firmware = FirmwareProtocol::Vial;
            app.layout = Some(single_key_layout(0x0004));
            app.vial_unlocked = Some(true);
            app.main_menu_tab = MainMenuTab::Advanced;
            app.settings_tab = tab;

            assert_eq!(app.start_vial_lock(&ctx), VialHidTaskStart::Started);
            poll_until_vial_hid_idle(&mut app, &ctx);

            assert_eq!(app.vial_unlocked, Some(false));
            assert!(app.main_menu_tab == MainMenuTab::Keyboard);
            assert!(app.settings_tab == tab);
            assert_eq!(&recorder.requests()[0][..2], &[0xFE, 0x08]);
        }
    }

    #[test]
    fn unavailable_device_does_not_exit_settings_page() {
        let ctx = egui::Context::default();
        let mut app = EntropyApp::new_inert_for_test();
        app.firmware = FirmwareProtocol::Vial;
        app.layout = Some(single_key_layout(0x0004));
        app.vial_unlocked = Some(true);
        app.main_menu_tab = MainMenuTab::Advanced;
        app.settings_tab = SettingsTab::Macros;

        assert_eq!(app.start_vial_lock(&ctx), VialHidTaskStart::NoDevice);
        assert_eq!(app.vial_unlocked, Some(true));
        assert!(app.main_menu_tab == MainMenuTab::Advanced);
    }

    #[test]
    fn unlock_poll_and_lock_use_vial_commands() {
        let (hid, recorder) = crate::hid::HidDevice::test_device();

        let _ = run_vial_hid_operation(&hid, VialHidOperation::UnlockPoll).unwrap();
        let _ = run_vial_hid_operation(&hid, VialHidOperation::Lock).unwrap();

        let requests = recorder.requests();
        assert_eq!(&requests[0][..2], &[0xFE, 0x07]);
        assert_eq!(&requests[1][..2], &[0xFE, 0x08]);
    }

    #[test]
    fn guarded_unlock_poll_stops_before_confirmation_packet() {
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        let mut pending = [0u8; 32];
        pending[..3].copy_from_slice(&[0, 1, 0]);
        recorder.respond_with([pending]);
        let mut checks = 0;
        let result = hid.unlock_poll_guarded(|| {
            checks += 1;
            anyhow::ensure!(checks == 1, "cancelled");
            Ok(())
        });
        assert!(result.is_err());
        assert_eq!(checks, 2);
        let requests = recorder.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(&requests[0][..2], &[0xFE, 0x07]);
    }

    #[test]
    fn stale_rmk_confirmation_timeout_retries_status_only() {
        let (hid, recorder) = crate::hid::HidDevice::test_device_with_fault_after_requests(Some((
            1,
            crate::hid::TestHidFault::Timeout,
        )));
        let mut stale_poll = [0u8; 32];
        stale_poll[..3].copy_from_slice(&[0, 1, 0]);
        recorder.respond_with([stale_poll]);

        let error = hid.unlock_poll().unwrap_err();
        assert!(format!("{error:#}").contains("failed to read Vial unlock status"));

        let requests = recorder.requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(&requests[0][..2], &[0xFE, 0x07]);
        assert_eq!(&requests[1][..2], &[0xFE, 0x05]);

        // The physical keys are now released; firmware reports the latched
        // result. The retry must not send another poll and re-arm old RMK.
        let mut released_status = [0xFFu8; 32];
        released_status[..2].copy_from_slice(&[1, 0]);
        recorder.respond_with([released_status]);

        assert_eq!(hid.unlock_poll().unwrap(), (true, false, 0));
        let requests = recorder.requests();
        assert_eq!(requests.len(), 3);
        assert_eq!(&requests[2][..2], &[0xFE, 0x05]);
    }

    #[test]
    fn still_pending_unlock_remains_pending_after_confirmation() {
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        let mut poll = [0u8; 32];
        poll[..3].copy_from_slice(&[1, 1, 0]);
        let mut status = [0xFFu8; 32];
        status[..4].copy_from_slice(&[1, 1, 0, 0]);
        recorder.respond_with([poll, status]);

        let outcome = run_vial_hid_operation(&hid, VialHidOperation::UnlockPoll).unwrap();

        assert!(matches!(
            outcome,
            VialHidOutcome::UnlockPolled {
                unlocked: true,
                in_progress: true,
                counter: 0,
            }
        ));

        let mut completed_status = [0xFFu8; 32];
        completed_status[..2].copy_from_slice(&[1, 0]);
        recorder.respond_with([completed_status]);

        let outcome = run_vial_hid_operation(&hid, VialHidOperation::UnlockPoll).unwrap();
        assert!(matches!(
            outcome,
            VialHidOutcome::UnlockPolled {
                unlocked: true,
                in_progress: false,
                counter: 0,
            }
        ));
        let requests = recorder.requests();
        assert_eq!(requests.len(), 3);
        assert_eq!(&requests[0][..2], &[0xFE, 0x07]);
        assert_eq!(&requests[1][..2], &[0xFE, 0x05]);
        assert_eq!(&requests[2][..2], &[0xFE, 0x05]);
    }

    #[test]
    fn battery_refresh_uses_the_existing_vial_transport() {
        let (hid, recorder) = crate::hid::HidDevice::test_device();

        let outcome = run_vial_hid_operation(&hid, VialHidOperation::BatteryRefresh).unwrap();

        assert!(matches!(outcome, VialHidOutcome::Battery(_)));
        let requests = recorder.requests();
        assert_eq!(requests.len(), 5);
        assert!(requests
            .iter()
            .all(|request| request[..3] == [0x08, 0xE8, 0x01]));
    }

    #[test]
    fn incomplete_split_battery_results_retry_soon() {
        assert_eq!(
            battery_refresh_delay(Some(crate::hid::BatteryHalves {
                left: Some(80),
                right: Some(75),
            })),
            BATTERY_REFRESH_INTERVAL
        );
        assert_eq!(
            battery_refresh_delay(Some(crate::hid::BatteryHalves {
                left: Some(80),
                right: None,
            })),
            BATTERY_INCOMPLETE_REFRESH_INTERVAL
        );
        assert_eq!(
            battery_refresh_delay(None),
            BATTERY_INCOMPLETE_REFRESH_INTERVAL
        );
    }

    #[test]
    fn key_and_encoder_writes_use_the_serialized_vial_transport() {
        let (hid, recorder) = crate::hid::HidDevice::test_device();

        let key_outcome = run_vial_hid_operation(
            &hid,
            VialHidOperation::KeyWrite {
                layer: 0,
                key_index: 0,
                row: 2,
                col: 3,
                old_binding: 0x0004.into(),
                binding: 0.into(),
                is_undo: false,
            },
        )
        .unwrap();
        let encoder_outcome = run_vial_hid_operation(
            &hid,
            VialHidOperation::EncoderWrite {
                layer: 1,
                encoder_visual_index: 0,
                encoder_index: 2,
                direction: 1,
                old_keycode: 0,
                keycode: 0x0005,
                is_undo: false,
            },
        )
        .unwrap();

        assert!(matches!(key_outcome, VialHidOutcome::KeyWritten));
        assert!(matches!(encoder_outcome, VialHidOutcome::EncoderWritten));
        let requests = recorder.requests();
        assert_eq!(&requests[0][..6], &[0x05, 0, 2, 3, 0, 0]);
        assert_eq!(&requests[1][..4], &[0x04, 0, 2, 3]);
        assert_eq!(&requests[2][..7], &[0xFE, 0x04, 1, 2, 1, 0, 5]);
    }

    #[test]
    fn macro_write_uses_the_serialized_background_transport() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        let mut macros = vec![Vec::new(); 32];
        macros[0] = vec![b'x'; 30];
        app.hid_device = Some(hid);
        app.keycode_picker.macro_texts = macros;
        app.keycode_picker.mark_macros_dirty();

        app.maybe_start_macro_write(&ctx);

        assert!(app.vial_hid_task_active());
        assert!(app.hid_device.is_none());
        assert!(!app.keycode_picker.macros_dirty);

        poll_until_vial_hid_idle(&mut app, &ctx);

        assert!(app.hid_device.is_some());
        assert!(!app.keycode_picker.macros_dirty);
        let requests = recorder.requests();
        assert_eq!(requests.len(), 4);
        assert_eq!(requests[0][0], 0x0D);
        assert_eq!(&requests[1][..4], &[0x0F, 0, 0, 28]);
        assert_eq!(&requests[2][..4], &[0x0F, 0, 28, 28]);
        assert_eq!(&requests[3][..4], &[0x0F, 0, 56, 6]);
    }

    #[test]
    fn failed_background_macro_write_waits_for_a_new_edit_before_retrying() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        let (hid, recorder) = crate::hid::HidDevice::test_device_with_fault_after_requests(Some((
            1,
            crate::hid::TestHidFault::Timeout,
        )));
        app.hid_device = Some(hid);
        app.keycode_picker.macro_texts = vec![Vec::new(); 32];
        app.keycode_picker.mark_macros_dirty();
        let failed_revision = app.keycode_picker.macro_edit_revision;

        app.maybe_start_macro_write(&ctx);
        poll_until_vial_hid_idle(&mut app, &ctx);

        assert!(app.keycode_picker.macros_dirty);
        assert_eq!(
            app.keycode_picker.macro_attempted_revision,
            Some(failed_revision)
        );
        let request_count = recorder.requests().len();
        app.maybe_start_macro_write(&ctx);
        assert!(!app.vial_hid_task_active());
        assert_eq!(recorder.requests().len(), request_count);
    }

    #[test]
    fn a_new_macro_edit_after_failure_can_start_another_write() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        let (hid, _) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(hid);
        app.keycode_picker.macro_texts = vec![Vec::new(); 32];
        app.keycode_picker.mark_macros_dirty();
        let failed_revision = app.keycode_picker.macro_edit_revision;
        app.keycode_picker.macro_attempted_revision = Some(failed_revision);

        app.maybe_start_macro_write(&ctx);
        assert!(!app.vial_hid_task_active());

        app.keycode_picker.macro_texts[0].push(b'x');
        app.keycode_picker.mark_macros_dirty();
        assert_ne!(app.keycode_picker.macro_edit_revision, failed_revision);
        app.maybe_start_macro_write(&ctx);
        assert!(app.vial_hid_task_active());
        poll_until_vial_hid_idle(&mut app, &ctx);
        assert!(!app.keycode_picker.macros_dirty);
    }

    #[test]
    fn assigning_a_key_updates_ui_before_the_hid_round_trip_finishes() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(hid);
        app.layout = Some(single_key_layout(0x0004));

        assert!(app.assign_keycode(&ctx, 0, 0, 0));
        assert!(app.vial_hid_task_active());
        assert_eq!(app.layout.as_ref().unwrap().get_keycode(0, 0), 0);
        assert!(app.undo_stack.is_empty());

        poll_until_vial_hid_idle(&mut app, &ctx);

        assert_eq!(app.layout.as_ref().unwrap().get_keycode(0, 0), 0);
        assert!(matches!(
            app.undo_stack.last(),
            Some(UndoAction::Key {
                layer: 0,
                key_idx: 0,
                old_binding: KeyBinding::Vial(0x0004),
            })
        ));
        assert_eq!(recorder.requests().len(), 2);
    }

    #[test]
    fn background_layer_does_not_disable_user_actions_and_queued_undo_runs_next() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        let context = background_layer_context();
        let mut layout = single_key_layout(0x0004);
        layout.layers.push(vec![0.into()]);
        layout.layer_names.push("Layer 1".into());
        layout.encoder_layers.push(Vec::new());
        app.layout = Some(layout);
        app.deferred_device_load = DeferredDeviceLoadState::staged((*context).clone());
        app.undo_stack.push(UndoAction::Key {
            layer: 0,
            key_idx: 0,
            old_binding: 0.into(),
        });
        app.hid_device = Some(hid);

        assert_eq!(
            app.start_vial_hid_operation(
                &ctx,
                VialHidOperation::Deferred(
                    super::device_deferred_load::DeferredLoadRequest::BackgroundLayerStep {
                        layer: 1,
                        step: BackgroundLayerStep::Keymap { local_offset: 0 },
                        context,
                    },
                ),
            ),
            VialHidTaskStart::Started
        );
        assert!(app.vial_hid_background_layer_active());
        assert!(!app.hid_user_action_busy());

        app.undo(&ctx);

        assert!(app.pending_layout_undo);
        assert_eq!(app.undo_stack.len(), 1);

        for _ in 0..100 {
            app.poll_vial_hid_task(&ctx);
            app.maybe_start_pending_layout_undo(&ctx);
            if !app.vial_hid_task_active() && !app.pending_layout_undo {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        assert!(!app.pending_layout_undo);
        assert!(!app.vial_hid_task_active());
        assert!(app.undo_stack.is_empty());
        assert_eq!(app.layout.as_ref().unwrap().get_keycode(0, 0), 0);
        assert_eq!(
            app.deferred_device_load.layer_status(1),
            DeferredLoadStatus::NotLoaded
        );
        let requests = recorder.requests();
        assert_eq!(requests.len(), 3);
        assert_eq!(requests[0][0], 0x12);
        assert_eq!(requests[1][0], 0x05);
        assert_eq!(requests[2][0], 0x04);
    }

    #[test]
    fn background_layer_queues_layer_operation_and_runs_it_before_more_loading() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        let context = background_layer_context();
        let mut layout = single_key_layout(0x0004);
        layout.layers.push(vec![0.into()]);
        layout.layer_names.push("Layer 1".into());
        layout.encoder_layers.push(Vec::new());
        app.layout = Some(layout);
        app.deferred_device_load = DeferredDeviceLoadState::staged((*context).clone());
        app.hid_device = Some(hid);

        assert_eq!(
            app.start_vial_hid_operation(
                &ctx,
                VialHidOperation::Deferred(
                    super::device_deferred_load::DeferredLoadRequest::BackgroundLayerStep {
                        layer: 1,
                        step: BackgroundLayerStep::Keymap { local_offset: 0 },
                        context,
                    },
                ),
            ),
            VialHidTaskStart::Started
        );
        assert!(app.vial_hid_background_layer_active());
        assert!(!app.hid_user_action_busy());

        app.apply_layer_snapshot(
            0,
            LayerSnapshot {
                keycodes: vec![0.into()],
                encoder_keycodes: Vec::new(),
            },
            "layer_actions.fill_none",
        );

        assert!(app.pending_layer_write.is_some());
        assert_eq!(app.layout.as_ref().unwrap().get_keycode(0, 0), 0x0004);

        for _ in 0..100 {
            app.poll_vial_hid_task(&ctx);
            app.maybe_start_pending_layer_write();
            app.poll_layer_write(&ctx);
            if !app.vial_hid_task_active()
                && app.pending_layer_write.is_none()
                && app.layer_write_task.is_none()
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        assert!(app.pending_layer_write.is_none());
        assert!(app.layer_write_task.is_none());
        assert_eq!(app.layout.as_ref().unwrap().get_keycode(0, 0), 0);
        assert!(matches!(
            app.undo_stack.last(),
            Some(UndoAction::Layer {
                layer: 0,
                old: LayerSnapshot { keycodes, .. },
                requires_firmware: true,
            }) if keycodes == &[0x0004.into()]
        ));
        let requests = recorder.requests();
        assert_eq!(requests.len(), 3);
        assert_eq!(requests[0][0], 0x12);
        assert_eq!(requests[1][0], 0x05);
        assert_eq!(requests[2][0], 0x04);
    }

    #[test]
    fn config_writes_wait_for_background_layer_and_keep_transport_available() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        let context = background_layer_context();
        app.deferred_device_load = DeferredDeviceLoadState::staged((*context).clone());
        app.module_settings.set_value(134, 0);
        app.layer_led_settings = LayerLedSettingsState {
            brightness: Some(LayerLedNumericSetting {
                qsid: 316,
                width: 2,
                value: 32,
                max: 255,
                variants: Vec::new(),
            }),
            supported: true,
            ..LayerLedSettingsState::default()
        };
        app.hid_device = Some(hid);

        assert_eq!(
            app.start_vial_hid_operation(
                &ctx,
                VialHidOperation::Deferred(
                    super::device_deferred_load::DeferredLoadRequest::BackgroundLayerStep {
                        layer: 1,
                        step: BackgroundLayerStep::Keymap { local_offset: 0 },
                        context,
                    },
                ),
            ),
            VialHidTaskStart::Started
        );
        assert!(app.hid_device.is_none());
        assert!(app.qmk_setting_transport_available());

        app.queue_module_setting_write(
            super::super::settings_write_queue::ModuleSettingMetadata {
                group_title: "Left Modules".to_owned(),
                field_title: "Mode".to_owned(),
                display_label: "Mode".to_owned(),
            },
            134,
            1,
            0,
            3,
        );
        app.queue_layer_led_setting_write("Layer LED brightness".to_owned(), 316, 2, 32, 128);

        assert!(app.settings_write_task.is_none());
        assert_eq!(app.pending_settings_write_value(134), Some(3));
        assert_eq!(app.pending_settings_write_value(316), Some(128));

        for _ in 0..500 {
            app.poll_vial_hid_task(&ctx);
            app.poll_settings_write(&ctx);
            if !app.vial_hid_task_active() && !app.qmk_settings_write_busy() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        assert!(!app.vial_hid_task_active());
        assert!(!app.qmk_settings_write_busy());
        assert_eq!(app.module_settings.value(134), 3);
        assert_eq!(
            app.layer_led_settings
                .brightness
                .as_ref()
                .map(|setting| setting.value),
            Some(128)
        );

        let requests = recorder.requests();
        let module_set = requests.iter().position(|request| {
            request[0] == 0xFE
                && request[1] == 0x0B
                && u16::from_le_bytes([request[2], request[3]]) == 134
        });
        let layer_led_set = requests.iter().position(|request| {
            request[0] == 0xFE
                && request[1] == 0x0B
                && u16::from_le_bytes([request[2], request[3]]) == 316
        });
        assert_eq!(requests.first().map(|request| request[0]), Some(0x12));
        assert!(module_set.is_some_and(|index| index > 0));
        assert!(layer_led_set.is_some_and(|index| Some(index) > module_set));
        let layer_led_requests = requests
            .iter()
            .filter(|request| {
                request[0] == 0xFE
                    && matches!(request[1], 0x0A | 0x0B)
                    && u16::from_le_bytes([request[2], request[3]]) == 316
            })
            .collect::<Vec<_>>();
        assert_eq!(layer_led_requests.len(), 2);
        assert_eq!(layer_led_requests[0][1], 0x0B);
        assert_eq!(layer_led_requests[1][1], 0x0A);
    }

    #[test]
    fn failed_background_key_write_rolls_back_the_optimistic_value() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = EntropyApp::new(&creation_context);
        let (hid, _recorder) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(hid);
        app.layout = Some(single_key_layout(0x0004));

        assert!(app.assign_keycode(&ctx, 0, 0, 0x0005));
        assert_eq!(app.layout.as_ref().unwrap().get_keycode(0, 0), 0x0005);

        poll_until_vial_hid_idle(&mut app, &ctx);

        assert_eq!(app.layout.as_ref().unwrap().get_keycode(0, 0), 0x0004);
        assert!(app.undo_stack.is_empty());
        assert!(app.status_msg.starts_with("Write error:"));
        assert!(app.hid_device.is_some());
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod ak820_backup_task_tests {
    use super::*;
    use std::sync::atomic::Ordering;

    fn ready_app() -> EntropyApp {
        let mut app = EntropyApp::new_inert_for_test();
        let (hid, _) = crate::hid::HidDevice::test_beta2_ak820_device(
            crate::app::ak820_beta1_profiles::ProfileId::Main,
            true,
        );
        app.hid_device = Some(hid);
        app.connect_state = ConnectState::Idle;
        app
    }

    #[test]
    fn backup_capture_admission_blocks_queued_edits_demo_and_failed_session() {
        let mut app = ready_app();
        assert!(app.can_capture_ak820_backup());
        app.combo_dirty = true;
        assert!(!app.can_capture_ak820_backup());
        app.combo_dirty = false;
        app.keycode_picker.macros_dirty = true;
        assert!(!app.can_capture_ak820_backup());
        app.keycode_picker.macros_dirty = false;
        app.pending_layout_undo = true;
        assert!(!app.can_capture_ak820_backup());
        app.pending_layout_undo = false;
        app.ak820_demo = true;
        assert!(!app.can_capture_ak820_backup());
        app.ak820_demo = false;
        app.hid_device
            .as_ref()
            .unwrap()
            .ak820_fail(&anyhow::anyhow!("modeled fault"));
        assert!(!app.can_capture_ak820_backup());
    }

    #[test]
    fn backup_capture_failure_categories_are_bounded() {
        assert_eq!(
            CaptureFailure::from_worker("Backup capture read failed: private path", true),
            CaptureFailure::Disconnected
        );
        assert_eq!(
            CaptureFailure::from_worker("Backup capture cancelled", false),
            CaptureFailure::Cancelled
        );
        assert_eq!(
            CaptureFailure::from_worker("AK820 storage failed or is busy", false),
            CaptureFailure::Storage
        );
        assert_eq!(
            CaptureFailure::from_worker("Captured macro getter differs", false),
            CaptureFailure::Reconciliation
        );
        assert_eq!(
            CaptureFailure::from_worker("private path or packet", false),
            CaptureFailure::Failed
        );
        let mut app = ready_app();
        let ctx = egui::Context::default();
        app.hid_device = None;
        assert_eq!(
            app.start_ak820_backup_capture(&ctx, 1),
            Err(CaptureFailure::Disconnected)
        );
    }

    #[test]
    fn backup_capture_worker_disconnect_and_channel_failure_are_distinct() {
        for (panic_worker, expected) in [
            (false, CaptureFailure::Disconnected),
            (true, CaptureFailure::Failed),
        ] {
            let mut app = ready_app();
            let ctx = egui::Context::default();
            super::super::ak820_backup_inspection::test_capture_pending(
                &ctx,
                19,
                app.connection_generation,
            );
            assert_eq!(
                app.start_vial_hid_operation_with_runner(
                    &ctx,
                    VialHidOperation::Ak820BackupCapture { request_id: 19 },
                    move |_, _, _, _| {
                        if panic_worker {
                            panic!("synthetic worker failure");
                        }
                        Err(anyhow::anyhow!("hid read failed"))
                    }
                ),
                VialHidTaskStart::Started
            );
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while app.vial_hid_task_active() && std::time::Instant::now() < deadline {
                app.poll_vial_hid_task(&ctx);
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            assert!(!app.vial_hid_task_active());
            assert_eq!(
                super::super::ak820_backup_inspection::test_capture_failure(&ctx),
                Some(expected)
            );
        }
    }

    #[test]
    fn backup_connection_retirement_cancels_and_retains_owner_until_worker_finishes() {
        let mut app = ready_app();
        let ctx = egui::Context::default();
        let (release, gate) = std::sync::mpsc::channel();
        let generation = app.connection_generation;
        assert_eq!(
            app.start_vial_hid_operation_with_runner(
                &ctx,
                VialHidOperation::Ak820BackupCapture { request_id: 17 },
                move |_, _, _, cancel| {
                    gate.recv_timeout(std::time::Duration::from_secs(5))
                        .unwrap();
                    assert!(cancel.load(Ordering::Acquire));
                    Err(anyhow::anyhow!("synthetic cancellation"))
                },
            ),
            VialHidTaskStart::Started
        );
        let flag = app.vial_hid_task.as_ref().unwrap().cancel.clone();
        app.vial_hid_task
            .as_ref()
            .unwrap()
            .progress
            .store(50, Ordering::Release);
        assert_eq!(app.ak820_backup_capture_progress(17), Some(0.5));
        assert_eq!(app.ak820_backup_capture_progress(18), None);
        assert!(app.hid_write_task_owner_active());
        assert!(
            app.hid_device.is_none(),
            "worker owns the HID handle during the loan"
        );
        assert_eq!(
            app.capture_availability(),
            CaptureAvailability::Unavailable(CaptureFailure::OperationBusy)
        );
        assert_eq!(
            app.start_ak820_backup_capture(&ctx, 18),
            Err(CaptureFailure::OperationBusy)
        );
        app.clear_connected_keyboard_state("retired");
        assert!(flag.load(Ordering::Acquire));
        assert!(app.hid_write_task_owner_active());
        assert_ne!(app.connection_generation, generation);
        release.send(()).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while app.vial_hid_task_active() && std::time::Instant::now() < deadline {
            app.poll_vial_hid_task(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(!app.vial_hid_task_active());
        assert!(
            app.hid_device.is_none(),
            "retired worker cannot restore the old handle"
        );
        assert_eq!(app.status_msg, "retired");
    }
}
