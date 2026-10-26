//! Native scene playback borrows the application's existing HID handle for a
//! single frame. Settings work always gets the next opportunity to use it.
use super::*;
use crate::ak820_openrgb_bridge::{Bridge, Status as BridgeStatus};
use crate::ak820_rgb_audio::{self as audio, Capture, Options as AudioOptions};
use crate::ak820_rgb_compositor::Scene;
use crate::ak820_rgb_scheduler::{Frame, FrameScheduler, FRAME_INTERVAL_MS, SUSPEND_GAP_MS};
use crate::hid::{Ak820RgbRestoreResult, Ak820RgbStream, Ak820RgbStreamResult, HidDevice};
use chrono::Timelike;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant, SystemTime};

struct Playback {
    scene: Scene,
    input: Input,
    started: Instant,
    last_tick: Instant,
    last_wall_tick: SystemTime,
    generation: u64,
    demo_profile: Option<ak820_beta1_profiles::ProfileId>,
    demo: bool,
    owned: Option<Ak820RgbStream>,
    once: bool,
}

enum Input {
    Scene,
    Music {
        capture: Capture,
        options: AudioOptions,
    },
    MusicDemo(AudioOptions),
    OpenRgb {
        bridge: Bridge,
        last: Option<Frame>,
    },
}

impl Input {
    fn frame(&mut self, scene: &Scene, elapsed: u64, minute: u16) -> Result<Option<Frame>, String> {
        let brightness = scene.brightness_at(minute);
        match self {
            Self::Scene => scene
                .render(elapsed, minute)
                .map(Some)
                .map_err(str::to_owned),
            Self::Music { capture, options } => capture
                .poll()
                .map(|bars| bars.map(|bars| audio::render(&bars, options, brightness))),
            Self::MusicDemo(options) => Ok(Some(audio::render(
                &audio::demo_bars(elapsed),
                options,
                brightness,
            ))),
            Self::OpenRgb { bridge, last } => {
                let snapshot = bridge.poll();
                match snapshot.status {
                    BridgeStatus::Listening | BridgeStatus::Connected => {
                        if snapshot.frame.is_some() {
                            *last = snapshot.frame;
                        }
                        Ok(last.map(|frame| {
                            frame.map(|rgb| {
                                rgb.map(|value| {
                                    (u16::from(value) * u16::from(brightness) / 255) as u8
                                })
                            })
                        }))
                    }
                    BridgeStatus::Fault(error) => Err(format!("OpenRGB: {error}")),
                    BridgeStatus::Disconnected | BridgeStatus::Stopped => {
                        Err("OpenRGB source disconnected".into())
                    }
                }
            }
        }
    }
}

struct FrameResult {
    hid: HidDevice,
    generation: u64,
    outcome: Result<Ak820RgbStreamResult, String>,
    elapsed_ms: u64,
}

struct FrameTask {
    receiver: mpsc::Receiver<FrameResult>,
    cancel: Arc<AtomicBool>,
    generation: u64,
}

struct OffRequest {
    generation: u64,
    owned: Option<Ak820RgbStream>,
    requested: Instant,
    wall_requested: SystemTime,
}

struct RestoreRequest {
    generation: u64,
    owned: Option<Ak820RgbStream>,
    requested: Instant,
    wall_requested: SystemTime,
}

struct RestoreResult {
    hid: HidDevice,
    generation: u64,
    outcome: Result<Ak820RgbRestoreResult, String>,
}

struct RestoreTask {
    receiver: mpsc::Receiver<RestoreResult>,
    cancel: Arc<AtomicBool>,
    generation: u64,
}

pub(super) struct Runtime {
    clock: Instant,
    playback: Option<Playback>,
    scheduler: FrameScheduler,
    task: Option<FrameTask>,
    off: Option<OffRequest>,
    restore: Option<RestoreRequest>,
    restore_task: Option<RestoreTask>,
    preview: Option<Frame>,
    last_frame_ms: Option<u64>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self {
            clock: Instant::now(),
            playback: None,
            scheduler: FrameScheduler::default(),
            task: None,
            off: None,
            restore: None,
            restore_task: None,
            preview: None,
            last_frame_ms: None,
        }
    }
}

impl EntropyApp {
    pub(super) fn ak820_rgb_scene_owns_hid(&self) -> bool {
        self.ak820_rgb_runtime.task.is_some() || self.ak820_rgb_runtime.restore_task.is_some()
    }

    pub(crate) fn ak820_rgb_scene_running(&self) -> bool {
        self.ak820_rgb_runtime.playback.is_some()
    }

    fn ak820_rgb_other_work_pending(&self) -> bool {
        self.layer_write_task.is_some()
            || self.combo_write_task.is_some()
            || self.settings_write_task.is_some()
            || self.vial_hid_task_active()
            || !self.settings_write_queue.is_empty()
            || self.qmk_settings_write_pending()
            || !self.pending_layer_name_writes.is_empty()
            || self.pending_layer_write.is_some()
            || self.pending_layout_undo
            || self.combo_dirty
            || self.import_pending()
            || self.pending_file_dialog.is_some()
            || self.pending_device_connect.is_some()
            || self.unlock_open
            || self.exit_after_hid_write
    }

    pub(crate) fn ak820_rgb_scene_can_start(&self) -> bool {
        if self.headless
            || self.ak820_rgb_scene_owns_hid()
            || self.ak820_rgb_runtime.off.is_some()
            || self.ak820_rgb_runtime.restore.is_some()
        {
            return false;
        }
        if self.ak820_demo {
            return self
                .ak820_demo_profile
                .is_some_and(|p| ak820_beta1_profiles::has_feature(p, "vialrgb-direct"));
        }
        matches!(self.connect_state, ConnectState::Idle)
            && !self.ak820_rgb_other_work_pending()
            && self.hid_device.as_ref().is_some_and(|h| {
                h.ak820_editing_ready()
                    && h.ak820_image()
                        .is_some_and(|i| i.profile().is_some() && i.rgb_modes().contains(&1))
            })
    }

    fn ak820_rgb_status(&mut self, ru: &str, en: &str) {
        self.status_msg = if self.app_settings.language == crate::i18n::Language::Russian {
            ru
        } else {
            en
        }
        .into();
    }

    pub(crate) fn ak820_rgb_scene_start(&mut self, scene: Scene) {
        if scene.validate().is_err() || !self.ak820_rgb_scene_can_start() {
            self.ak820_rgb_status(
                "Сцену нельзя запустить: проверьте параметры и состояние подключения.",
                "Cannot start scene: check its parameters and connection state.",
            );
            return;
        }
        self.ak820_rgb_scene_stop();
        let now = Instant::now();
        self.ak820_rgb_runtime.playback = Some(Playback {
            scene,
            input: Input::Scene,
            started: now,
            last_tick: now,
            last_wall_tick: SystemTime::now(),
            generation: self.connection_generation,
            demo_profile: self.ak820_demo_profile,
            demo: self.ak820_demo,
            owned: None,
            once: false,
        });
        self.ak820_rgb_runtime
            .scheduler
            .resume(self.ak820_rgb_runtime.clock.elapsed().as_millis() as u64);
        self.ak820_rgb_status(
            "Воспроизведение RGB запущено. Цвета не сохраняются в клавиатуре.",
            "RGB playback started. Colors are not saved in the keyboard.",
        );
    }

    pub(super) fn ak820_rgb_music_start(
        &mut self,
        scene: Scene,
        options: AudioOptions,
        test: bool,
    ) {
        if scene.validate().is_err()
            || options.validate().is_err()
            || !self.ak820_rgb_scene_can_start()
        {
            self.ak820_rgb_status(
                "Музыка недоступна: проверьте подключение и параметры.",
                "Music is unavailable: check connection and settings.",
            );
            return;
        }
        // Demo always remains device-free; system capture is still explicit
        // and separate from the no-capture test spectrum.
        let input = if test {
            Input::MusicDemo(options)
        } else {
            match Capture::start(&options) {
                Ok(capture) => Input::Music { capture, options },
                Err(error) => {
                    self.ak820_rgb_status(
                        "Не удалось включить захват системного звука.",
                        "Could not start system audio capture.",
                    );
                    self.status_msg.push_str(&format!(" {error}"));
                    return;
                }
            }
        };
        self.ak820_rgb_scene_start(scene);
        if let Some(playback) = self.ak820_rgb_runtime.playback.as_mut() {
            playback.input = input;
        }
        self.ak820_rgb_status(
            if test {
                "Тестовый спектр включён; звук не захватывается."
            } else {
                "Музыка включена: захватывается системный звук, без записи и передачи."
            },
            if test {
                "Test spectrum started; no audio is captured."
            } else {
                "Music started: system audio capture is active, without recording or transmission."
            },
        );
    }

    pub(super) fn ak820_rgb_openrgb_start(&mut self, scene: Scene, port: u16) {
        if scene.validate().is_err() || port == 0 || !self.ak820_rgb_scene_can_start() {
            self.ak820_rgb_status(
                "OpenRGB недоступен: проверьте подключение и порт.",
                "OpenRGB is unavailable: check connection and port.",
            );
            return;
        }
        let bridge = match Bridge::start(port) {
            Ok(bridge) => bridge,
            Err(error) => {
                self.ak820_rgb_status(
                    "Не удалось открыть локальный порт OpenRGB.",
                    "Could not open the local OpenRGB port.",
                );
                self.status_msg.push_str(&format!(" {error}"));
                return;
            }
        };
        self.ak820_rgb_scene_start(scene);
        if let Some(playback) = self.ak820_rgb_runtime.playback.as_mut() {
            playback.input = Input::OpenRgb { bridge, last: None };
        }
        self.ak820_rgb_status(
            "Ожидание локального источника OpenRGB.",
            "Waiting for a local OpenRGB source.",
        );
    }

    pub(super) fn ak820_rgb_input_label(&self, ru: bool) -> Option<String> {
        if self.ak820_rgb_runtime.restore.is_some() || self.ak820_rgb_runtime.restore_task.is_some()
        {
            return Some(
                if ru {
                    "Возврат предыдущего эффекта…"
                } else {
                    "Restoring previous effect…"
                }
                .into(),
            );
        }
        self.ak820_rgb_runtime
            .playback
            .as_ref()
            .map(|p| match &p.input {
                Input::Scene => if ru { "Сцена" } else { "Scene" }.into(),
                Input::Music { .. } => if ru {
                    "Захват системного звука включён"
                } else {
                    "System audio capture is active"
                }
                .into(),
                Input::MusicDemo(_) => if ru {
                    "Тестовый спектр — без захвата звука"
                } else {
                    "Test spectrum — no audio capture"
                }
                .into(),
                Input::OpenRgb { bridge, .. } => format!("OpenRGB · 127.0.0.1:{}", bridge.port()),
            })
    }

    pub(super) fn ak820_rgb_can_restore(&self) -> bool {
        !self.ak820_demo
            && self.ak820_rgb_runtime.playback.as_ref().is_some_and(|p| {
                p.generation == self.connection_generation
                    && (p
                        .owned
                        .as_ref()
                        .is_some_and(Ak820RgbStream::can_restore_builtin)
                        || (p.owned.is_none() && self.ak820_rgb_runtime.task.is_some()))
            })
    }

    /// This explicit action waits for an intact frame. Ordinary Stop, errors,
    /// settings actions and disconnects continue to cancel without any restore.
    pub(super) fn ak820_rgb_stop_and_restore(&mut self) {
        if !self.ak820_rgb_can_restore() {
            return;
        }
        let playback = self.ak820_rgb_runtime.playback.take().unwrap();
        self.ak820_rgb_runtime.scheduler.pause();
        self.ak820_rgb_runtime.off = None;
        self.ak820_rgb_runtime.restore = Some(RestoreRequest {
            generation: playback.generation,
            owned: playback.owned,
            requested: Instant::now(),
            wall_requested: SystemTime::now(),
        });
        // Dropping playback closes capture/the bridge. An in-flight HID frame
        // finishes once, and its fresh ownership observation is used below.
    }

    pub(crate) fn ak820_rgb_scene_update(&mut self, scene: Scene) {
        if scene.validate().is_err() {
            self.ak820_rgb_scene_stop();
            self.ak820_rgb_status(
                "Воспроизведение остановлено: параметры сцены недопустимы.",
                "Playback stopped: scene parameters are invalid.",
            );
        } else if let Some(p) = self.ak820_rgb_runtime.playback.as_mut() {
            p.scene = scene;
        }
    }

    pub(crate) fn ak820_rgb_scene_stop(&mut self) {
        self.ak820_rgb_runtime.playback = None;
        self.ak820_rgb_runtime.off = None;
        self.ak820_rgb_runtime.restore = None;
        self.ak820_rgb_runtime.scheduler.pause();
        if let Some(task) = &self.ak820_rgb_runtime.task {
            task.cancel.store(true, Ordering::Release);
        }
        if let Some(task) = &self.ak820_rgb_runtime.restore_task {
            task.cancel.store(true, Ordering::Release);
        }
    }

    pub(crate) fn ak820_rgb_scene_off(&mut self) {
        let owned = self
            .ak820_rgb_runtime
            .playback
            .as_ref()
            .and_then(|p| p.owned.clone());
        let allowed = self.ak820_rgb_scene_can_start()
            || self
                .ak820_rgb_runtime
                .playback
                .as_ref()
                .is_some_and(|p| p.generation == self.connection_generation && !p.demo);
        self.ak820_rgb_scene_stop();
        if self.ak820_demo {
            self.ak820_rgb_runtime.preview = Some([[0; 3]; crate::ak820_rgb::LED_COUNT]);
        } else if allowed {
            self.ak820_rgb_runtime.off = Some(OffRequest {
                generation: self.connection_generation,
                owned,
                requested: Instant::now(),
                wall_requested: SystemTime::now(),
            });
        } else {
            self.ak820_rgb_status(
                "RGB остановлен. Выключение недоступно, пока устройство занято или не проверено.",
                "RGB stopped. Off is unavailable while the device is busy or unverified.",
            );
        }
    }

    pub(crate) fn ak820_rgb_scene_preview(&self) -> Option<Frame> {
        self.ak820_rgb_runtime.preview
    }

    pub(super) fn ak820_rgb_frame_duration(&self) -> Option<u64> {
        self.ak820_rgb_runtime.last_frame_ms
    }

    /// Poll before settings work so its handle is returned before another
    /// operation is selected. A stale result never replaces a newer handle.
    pub(super) fn poll_ak820_rgb_task(&mut self) {
        self.poll_ak820_rgb_restore();
        let received = match self.ak820_rgb_runtime.task.as_ref() {
            Some(t) => t.receiver.try_recv(),
            None => return,
        };
        let result = match received {
            Ok(r) => r,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                let task = self.ak820_rgb_runtime.task.take().unwrap();
                self.ak820_rgb_scene_stop();
                if task.generation == self.connection_generation {
                    self.ak820_rgb_status(
                        "RGB остановлен: обработчик устройства завершился. Подключитесь заново.",
                        "RGB stopped: device worker exited. Connect again.",
                    );
                }
                self.resume_pending_device_connect();
                return;
            }
        };
        self.ak820_rgb_runtime.task = None;
        if result.generation != self.connection_generation {
            self.ak820_rgb_scene_stop();
            self.resume_pending_device_connect();
            return;
        }
        self.hid_device = Some(result.hid);
        self.ak820_rgb_runtime.last_frame_ms = Some(result.elapsed_ms);
        self.ak820_rgb_runtime
            .scheduler
            .completed(self.ak820_rgb_runtime.clock.elapsed().as_millis() as u64);
        match result.outcome {
            Ok(Ak820RgbStreamResult::Applied(owned)) => {
                self.rgb_settings.effect = 1;
                self.rgb_settings.last_enabled_effect = 1;
                if let Some(off) = self.ak820_rgb_runtime.off.as_mut() {
                    off.owned = Some(owned.clone());
                }
                if let Some(restore) = self.ak820_rgb_runtime.restore.as_mut() {
                    restore.owned = Some(owned.clone());
                }
                if let Some(p) = self.ak820_rgb_runtime.playback.as_mut() {
                    p.owned = Some(owned);
                    if p.once {
                        self.ak820_rgb_scene_stop();
                        self.ak820_rgb_status(
                            "Подсветка выключена в ОЗУ; сохранённые настройки не изменены.",
                            "Lighting is off in RAM; saved settings are unchanged.",
                        );
                    }
                }
            }
            Ok(Ak820RgbStreamResult::Cancelled) => {
                self.ak820_rgb_runtime.restore = None;
                // The transport retires its ownership observation on any
                // cancelled frame, including a later partial frame. Drop the
                // input too; never resume it with a stale ownership token.
                self.ak820_rgb_runtime.playback = None;
                self.ak820_rgb_runtime.scheduler.pause();
                if let Some(off) = self.ak820_rgb_runtime.off.as_mut() {
                    // Explicit Off can start a new checked black frame.
                    off.owned = None;
                } else {
                    self.ak820_rgb_status(
                        "RGB остановлен при передаче управления. Запустите источник снова.",
                        "RGB stopped while yielding control. Start the source again.",
                    );
                }
            }
            Ok(Ak820RgbStreamResult::OwnershipLost) => {
                self.ak820_rgb_scene_stop();
                self.ak820_rgb_status("RGB остановлен: режим подсветки изменён на клавиатуре. Прочитайте настройки заново.",
                    "RGB stopped: keyboard lighting mode changed. Read settings again.");
            }
            Err(error) => {
                log::warn!("AK820 RGB stream stopped: {error}");
                self.ak820_rgb_scene_stop();
                self.ak820_rgb_status("Ошибка RGB: запись остановлена; состояние цветов неизвестно. Прочитайте устройство заново.",
                    "RGB error: writes stopped; color state is unknown. Read the device again.");
            }
        }
        self.resume_pending_device_connect();
    }

    fn poll_ak820_rgb_restore(&mut self) {
        let Some(task) = self.ak820_rgb_runtime.restore_task.as_ref() else {
            return;
        };
        let result = match task.receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                let task = self.ak820_rgb_runtime.restore_task.take().unwrap();
                if task.generation == self.connection_generation {
                    self.ak820_rgb_status(
                        "Возврат эффекта прерван. Подключитесь заново.",
                        "Effect restoration interrupted. Connect again.",
                    );
                }
                self.resume_pending_device_connect();
                return;
            }
        };
        self.ak820_rgb_runtime.restore_task = None;
        if result.generation != self.connection_generation {
            self.resume_pending_device_connect();
            return;
        }
        self.hid_device = Some(result.hid);
        match result.outcome {
            Ok(Ak820RgbRestoreResult::Restored(mode)) => {
                self.rgb_settings.effect = u16::from(mode[0]);
                if mode[0] != 0 { self.rgb_settings.last_enabled_effect = u16::from(mode[0]); }
                self.rgb_settings.speed = mode[2];
                self.rgb_settings.hue = mode[3];
                self.rgb_settings.saturation = mode[4];
                self.rgb_settings.brightness = mode[5];
                self.ak820_rgb_runtime.preview = None;
                self.ak820_rgb_status("Предыдущий встроенный эффект возвращён; память не перезаписывалась.", "Previous built-in effect restored; persistent memory was not rewritten.");
            }
            Ok(_) => self.ak820_rgb_status("RGB остановлен. Возврат отменён: состояние изменилось или прежняя палитра недоступна.", "RGB stopped. Restoration cancelled: state changed or the previous palette is unavailable."),
            Err(error) => {
                log::warn!("AK820 RGB explicit restore failed: {error}");
                self.ak820_rgb_status("Возврат эффекта не выполнен. Прочитайте устройство заново.", "Effect restoration failed. Read the device again.");
            }
        }
        self.resume_pending_device_connect();
    }

    fn dispatch_ak820_rgb_restore(&mut self, ctx: &egui::Context, now: Instant, wall: SystemTime) {
        if self.ak820_rgb_scene_owns_hid() {
            return;
        }
        let Some(request) = self.ak820_rgb_runtime.restore.take() else {
            return;
        };
        let backup_open = ctx.data(|d| {
            d.get_temp::<bool>(egui::Id::new(super::ak820_backup_inspection::OPEN_ID))
                .unwrap_or(false)
        });
        if request.generation != self.connection_generation
            || now.saturating_duration_since(request.requested).as_millis()
                > u128::from(SUSPEND_GAP_MS)
            || wall
                .duration_since(request.wall_requested)
                .map_or(true, |d| d.as_millis() > u128::from(SUSPEND_GAP_MS))
            || !self.ak820_rgb_scene_can_start()
            || backup_open
            || self.modal_or_popup_open(ctx)
        {
            self.ak820_rgb_status(
                "Возврат эффекта отменён: состояние изменилось.",
                "Effect restoration cancelled: state changed.",
            );
            return;
        }
        let Some(owned) = request.owned.filter(Ak820RgbStream::can_restore_builtin) else {
            return;
        };
        let Some(hid) = self.hid_device.take() else {
            return;
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let (sender, receiver) = mpsc::channel();
        let repaint = ctx.clone();
        let generation = request.generation;
        std::thread::spawn(move || {
            #[cfg(target_os = "macos")]
            let _hid_lock = hid.macos_hid_operation_lock();
            let outcome = hid
                .ak820_restore_rgb_stream(&owned, &flag)
                .map_err(|e| format!("{e:#}"));
            let _ = sender.send(RestoreResult {
                hid,
                generation,
                outcome,
            });
            repaint.request_repaint();
        });
        self.ak820_rgb_runtime.restore_task = Some(RestoreTask {
            receiver,
            cancel,
            generation,
        });
    }

    /// Called after all normal native tasks. UI demo calls this too, with no
    /// HID handle or background work. Nothing starts without an explicit click.
    pub(super) fn tick_ak820_rgb_scene(&mut self, ctx: &egui::Context) {
        self.tick_ak820_rgb_scene_at(ctx, Instant::now(), SystemTime::now(), {
            let now = chrono::Local::now();
            (now.hour() * 60 + now.minute()) as u16
        });
    }

    fn tick_ak820_rgb_scene_at(
        &mut self,
        ctx: &egui::Context,
        now: Instant,
        wall_now: SystemTime,
        minute: u16,
    ) {
        if self.headless || self.exit_after_hid_write {
            self.ak820_rgb_scene_stop();
            return;
        }
        self.dispatch_ak820_rgb_restore(ctx, now, wall_now);
        if self.ak820_rgb_runtime.restore.is_some() || self.ak820_rgb_runtime.restore_task.is_some()
        {
            ctx.request_repaint_after(Duration::from_millis(20));
            return;
        }
        if self.ak820_rgb_runtime.off.is_some() && !self.ak820_rgb_scene_owns_hid() {
            let off = self.ak820_rgb_runtime.off.take().unwrap();
            if off.generation == self.connection_generation
                && now.saturating_duration_since(off.requested).as_millis()
                    <= u128::from(SUSPEND_GAP_MS)
                && wall_now
                    .duration_since(off.wall_requested)
                    .is_ok_and(|gap| gap.as_millis() <= u128::from(SUSPEND_GAP_MS))
                && self.ak820_rgb_scene_can_start()
            {
                self.ak820_rgb_runtime.playback = Some(Playback {
                    scene: Scene::default(),
                    input: Input::Scene,
                    started: now,
                    last_tick: now,
                    last_wall_tick: wall_now,
                    generation: off.generation,
                    demo_profile: None,
                    demo: false,
                    owned: off.owned,
                    once: true,
                });
            } else {
                self.ak820_rgb_status(
                    "Выключение RGB отменено: устройство занято или состояние изменилось.",
                    "RGB Off cancelled: device is busy or its state changed.",
                );
            }
        }
        let Some(p) = self.ak820_rgb_runtime.playback.as_ref() else {
            return;
        };
        if p.generation != self.connection_generation
            || p.demo != self.ak820_demo
            || p.demo_profile != self.ak820_demo_profile
            || now.saturating_duration_since(p.last_tick).as_millis() > u128::from(SUSPEND_GAP_MS)
            || wall_now
                .duration_since(p.last_wall_tick)
                .map_or(true, |gap| gap.as_millis() > u128::from(SUSPEND_GAP_MS))
            || (!p.demo && !matches!(self.connect_state, ConnectState::Idle))
        {
            self.ak820_rgb_scene_stop();
            self.ak820_rgb_status(
                "RGB остановлен после паузы или изменения подключения. Запустите сцену снова.",
                "RGB stopped after a suspension or connection change. Start the scene again.",
            );
            return;
        }
        self.ak820_rgb_runtime.playback.as_mut().unwrap().last_tick = now;
        self.ak820_rgb_runtime
            .playback
            .as_mut()
            .unwrap()
            .last_wall_tick = wall_now;
        ctx.request_repaint_after(Duration::from_millis(FRAME_INTERVAL_MS));
        let backup_open = ctx.data(|d| {
            d.get_temp::<bool>(egui::Id::new(super::ak820_backup_inspection::OPEN_ID))
                .unwrap_or(false)
        });
        if !self.ak820_demo
            && (self.ak820_rgb_other_work_pending() || backup_open || self.modal_or_popup_open(ctx))
        {
            self.ak820_rgb_runtime.scheduler.pause();
            if let Some(task) = &self.ak820_rgb_runtime.task {
                task.cancel.store(true, Ordering::Release);
            }
            return;
        }
        if !self.ak820_demo
            && !self.ak820_rgb_scene_owns_hid()
            && !self
                .hid_device
                .as_ref()
                .is_some_and(HidDevice::ak820_editing_ready)
        {
            self.ak820_rgb_scene_stop();
            return;
        }
        let p = self.ak820_rgb_runtime.playback.as_mut().unwrap();
        let frame = match p.input.frame(
            &p.scene,
            now.saturating_duration_since(p.started)
                .as_millis()
                .min(u128::from(u64::MAX)) as u64,
            minute,
        ) {
            Ok(frame) => frame,
            Err(error) => {
                self.ak820_rgb_scene_stop();
                self.ak820_rgb_status(
                    "Источник RGB остановлен. Для продолжения запустите его заново.",
                    "RGB source stopped. Start it again to continue.",
                );
                self.status_msg.push_str(&format!(" {error}"));
                return;
            }
        };
        if let Some(frame) = frame {
            self.ak820_rgb_runtime.preview = Some(frame);
        }
        if self.ak820_demo {
            return;
        }
        let ms = now
            .saturating_duration_since(self.ak820_rgb_runtime.clock)
            .as_millis()
            .min(u128::from(u64::MAX)) as u64;
        self.ak820_rgb_runtime.scheduler.resume(ms);
        if frame.is_some_and(|frame| self.ak820_rgb_runtime.scheduler.submit(frame).is_err()) {
            self.ak820_rgb_scene_stop();
            return;
        }
        if self.ak820_rgb_scene_owns_hid() {
            return;
        }
        let Some(frame) = self.ak820_rgb_runtime.scheduler.take_due(ms) else {
            return;
        };
        let Some(hid) = self.hid_device.take() else {
            self.ak820_rgb_scene_stop();
            return;
        };
        let owned = self
            .ak820_rgb_runtime
            .playback
            .as_ref()
            .unwrap()
            .owned
            .clone();
        let generation = self.connection_generation;
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let (sender, receiver) = mpsc::channel();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let started = Instant::now();
            #[cfg(target_os = "macos")]
            let _hid_lock = hid.macos_hid_operation_lock();
            let outcome = hid
                .ak820_stream_rgb_frame(&frame, owned.as_ref(), &worker_cancel)
                .map_err(|e| format!("{e:#}"));
            let elapsed_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
            let _ = sender.send(FrameResult {
                hid,
                generation,
                outcome,
                elapsed_ms,
            });
            repaint.request_repaint();
        });
        self.ak820_rgb_runtime.task = Some(FrameTask {
            receiver,
            cancel,
            generation,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ak820_beta1_profiles::ProfileId;

    fn live_app(fail: bool) -> (EntropyApp, crate::hid::TestHidRecorder) {
        let mut app = EntropyApp::new_inert_for_test();
        let (hid, log) = HidDevice::test_beta2_ak820_device(ProfileId::Main, true);
        let mut mode = [2, 0, 50, 90, 255, 160];
        log.respond_by(move |p| {
            let mut r = *p;
            match (p[0], p[1]) {
                (0xFD, 1) => r[..4].copy_from_slice(&[0xFD, 1, 1, 0xA8]),
                (8, 0x41) => r[2..8].copy_from_slice(&mode),
                (7, 0x41) => mode.copy_from_slice(&p[2..8]),
                (7, 0x42) if fail => return Some(Err("scripted RGB timeout".into())),
                (7, 0x42) => {}
                _ => return Some(Err("unexpected RGB test request".into())),
            }
            Some(Ok(r))
        });
        app.hid_device = Some(hid);
        app.connect_state = ConnectState::Idle;
        (app, log)
    }

    fn finish_frame(app: &mut EntropyApp) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while app.ak820_rgb_scene_owns_hid() && Instant::now() < deadline {
            app.poll_ak820_rgb_task();
            std::thread::yield_now();
        }
        assert!(
            !app.ak820_rgb_scene_owns_hid(),
            "scripted RGB worker did not complete"
        );
    }

    #[test]
    fn rgb_runtime_profile_gate_and_headless_are_enforced_before_start() {
        let mut app = EntropyApp::new_inert_for_test();
        for (profile, direct) in [
            (ProfileId::Main, true),
            (ProfileId::Game, true),
            (ProfileId::Mmo, true),
            (ProfileId::Creative, true),
            (ProfileId::Typing, false),
            (ProfileId::Code, false),
            (ProfileId::Pointer, false),
            (ProfileId::Onehand, false),
            (ProfileId::Shortcuts, false),
            (ProfileId::Multios, false),
        ] {
            let (hid, log) = HidDevice::test_beta2_ak820_device(profile, true);
            app.hid_device = Some(hid);
            assert_eq!(app.ak820_rgb_scene_can_start(), direct, "{profile:?}");
            app.headless = true;
            assert!(!app.ak820_rgb_scene_can_start());
            app.ak820_rgb_scene_start(Scene::default());
            assert!(!app.ak820_rgb_scene_running());
            app.headless = false;
            assert!(log.requests().is_empty());
        }
    }

    #[test]
    fn rgb_runtime_demo_animates_without_hid_and_stops_on_profile_change() {
        let mut app = EntropyApp::new_inert_for_test();
        app.ak820_demo = true;
        app.ak820_demo_profile = Some(ProfileId::Main);
        let scene = Scene {
            layers: vec![crate::ak820_rgb_compositor::Layer {
                effect: crate::ak820_rgb_compositor::Effect::Breathing,
                ..Default::default()
            }],
            ..Default::default()
        };
        app.ak820_rgb_scene_start(scene);
        let p = app.ak820_rgb_runtime.playback.as_ref().unwrap();
        let (start, wall) = (p.started, p.last_wall_tick);
        let ctx = egui::Context::default();
        app.tick_ak820_rgb_scene_at(&ctx, start, wall, 720);
        let first = app.ak820_rgb_scene_preview();
        app.tick_ak820_rgb_scene_at(
            &ctx,
            start + Duration::from_millis(500),
            wall + Duration::from_millis(500),
            720,
        );
        assert_ne!(first, app.ak820_rgb_scene_preview());
        assert!(app.hid_device.is_none());
        assert!(!app.ak820_rgb_scene_owns_hid());
        app.ak820_demo_profile = Some(ProfileId::Game);
        app.tick_ak820_rgb_scene(&ctx);
        assert!(!app.ak820_rgb_scene_running());
    }

    #[test]
    fn rgb_runtime_suspend_and_wall_clock_jump_require_explicit_restart() {
        for wall_delta in [Duration::from_secs(3), Duration::ZERO] {
            let (mut app, log) = live_app(false);
            app.ak820_rgb_scene_start(Scene::default());
            let p = app.ak820_rgb_runtime.playback.as_ref().unwrap();
            let (now, wall) = (p.started, p.last_wall_tick);
            let instant_delta = if wall_delta.is_zero() {
                Duration::from_secs(3)
            } else {
                Duration::ZERO
            };
            app.tick_ak820_rgb_scene_at(
                &egui::Context::default(),
                now + instant_delta,
                wall + wall_delta,
                720,
            );
            assert!(!app.ak820_rgb_scene_running());
            assert!(log.requests().is_empty());
        }
    }

    #[test]
    fn rgb_runtime_backup_pause_discards_pending_frame_and_resumes_fresh() {
        let (mut app, log) = live_app(false);
        let ctx = egui::Context::default();
        let key = egui::Id::new(super::super::ak820_backup_inspection::OPEN_ID);
        app.ak820_rgb_scene_start(Scene::default());
        ctx.data_mut(|d| d.insert_temp(key, true));
        app.tick_ak820_rgb_scene(&ctx);
        assert!(log.requests().is_empty());
        let mut latest = Scene::default();
        latest.base.colors.fill(crate::ak820_rgb::Rgb([255, 0, 0]));
        app.ak820_rgb_scene_update(latest);
        ctx.data_mut(|d| d.insert_temp(key, false));
        app.tick_ak820_rgb_scene(&ctx);
        assert!(app.hid_write_task_owner_active());
        assert!(app.hid_device.is_none());
        finish_frame(&mut app);
        let requests = log.requests();
        let packet = requests.iter().find(|p| p[..2] == [7, 0x42]).unwrap();
        assert_eq!(&packet[5..8], &[0, 255, 160]);
        app.ak820_rgb_scene_stop();
    }

    #[test]
    fn rgb_runtime_failed_frame_never_retries_or_restores() {
        let (mut app, log) = live_app(true);
        let ctx = egui::Context::default();
        app.ak820_rgb_scene_start(Scene::default());
        app.tick_ak820_rgb_scene(&ctx);
        finish_frame(&mut app);
        assert!(!app.ak820_rgb_scene_running());
        assert!(!app.ak820_rgb_scene_can_start());
        let count = log.requests().len();
        for _ in 0..10 {
            app.tick_ak820_rgb_scene(&ctx);
        }
        assert_eq!(log.requests().len(), count);
        assert!(!log
            .requests()
            .iter()
            .any(|p| p[..2] == [7, 0x41] || p[0] == 9));
    }

    #[test]
    fn rgb_runtime_key_picker_modal_pauses_writes_until_closed() {
        let (mut app, log) = live_app(false);
        let ctx = egui::Context::default();
        app.ak820_rgb_scene_start(Scene::default());
        app.keycode_picker.open = true;
        app.tick_ak820_rgb_scene(&ctx);
        assert!(app.ak820_rgb_scene_running());
        assert!(!app.ak820_rgb_scene_owns_hid());
        assert!(log.requests().is_empty());
        app.keycode_picker.open = false;
        app.tick_ak820_rgb_scene(&ctx);
        finish_frame(&mut app);
        assert_eq!(
            log.requests()
                .iter()
                .filter(|p| p[..2] == [7, 0x42])
                .count(),
            10
        );
        app.ak820_rgb_scene_stop();
    }

    #[test]
    fn rgb_runtime_stale_completion_never_replaces_new_session() {
        let (mut app, current_log) = live_app(false);
        let (old, _) = HidDevice::test_beta2_ak820_device(ProfileId::Code, true);
        let (tx, rx) = mpsc::channel();
        app.ak820_rgb_runtime.task = Some(FrameTask {
            receiver: rx,
            cancel: Arc::new(AtomicBool::new(false)),
            generation: 0,
        });
        app.connection_generation = 1;
        assert!(tx
            .send(FrameResult {
                hid: old,
                generation: 0,
                outcome: Ok(Ak820RgbStreamResult::Cancelled),
                elapsed_ms: 1
            })
            .is_ok());
        app.poll_ak820_rgb_task();
        assert!(app.ak820_rgb_scene_can_start());
        assert!(current_log.requests().is_empty());
        assert!(app
            .hid_device
            .unwrap()
            .ak820_image()
            .unwrap()
            .rgb_modes()
            .contains(&1));
    }

    #[test]
    fn rgb_runtime_off_while_starting_sends_one_black_frame_then_stops() {
        let (mut app, log) = live_app(false);
        let ctx = egui::Context::default();
        app.ak820_rgb_scene_start(Scene::default());
        app.tick_ak820_rgb_scene(&ctx);
        app.ak820_rgb_scene_off();
        finish_frame(&mut app);
        // Off respects the same post-completion cooldown as any other frame.
        let delta = Duration::from_millis(FRAME_INTERVAL_MS + 1);
        app.tick_ak820_rgb_scene_at(&ctx, Instant::now() + delta, SystemTime::now() + delta, 720);
        finish_frame(&mut app);
        assert!(!app.ak820_rgb_scene_running());
        let count = log.requests().len();
        app.tick_ak820_rgb_scene(&ctx);
        assert_eq!(count, log.requests().len());
        let requests = log.requests();
        let packets: Vec<_> = requests.iter().filter(|p| p[..2] == [7, 0x42]).collect();
        assert!(packets.len() >= 10);
        assert!(packets[packets.len() - 10..]
            .iter()
            .all(|p| p[5..].iter().all(|&b| b == 0)));
        assert!(!requests.iter().any(|p| p[0] == 9));
    }

    #[test]
    fn rgb_runtime_pending_off_expires_across_suspend_without_writing() {
        for wall_only in [false, true] {
            let (mut app, log) = live_app(false);
            app.ak820_rgb_scene_off();
            let off = app.ak820_rgb_runtime.off.as_ref().unwrap();
            let (now, wall) = (off.requested, off.wall_requested);
            app.tick_ak820_rgb_scene_at(
                &egui::Context::default(),
                now + if wall_only {
                    Duration::ZERO
                } else {
                    Duration::from_secs(3)
                },
                wall + if wall_only {
                    Duration::from_secs(3)
                } else {
                    Duration::ZERO
                },
                720,
            );
            assert!(app.ak820_rgb_runtime.off.is_none());
            assert!(!app.ak820_rgb_scene_running());
            assert!(!app.ak820_rgb_scene_owns_hid());
            assert!(log.requests().is_empty());
        }
    }

    #[test]
    fn rgb_runtime_music_demo_has_no_capture_and_obeys_quiet_hours() {
        let mut app = EntropyApp::new_inert_for_test();
        app.ak820_demo = true;
        app.ak820_demo_profile = Some(ProfileId::Main);
        let mut scene = Scene::default();
        scene.quiet_hours = Some(crate::ak820_rgb_compositor::QuietHours {
            start_minute: 60,
            end_minute: 120,
            brightness: 7,
        });
        app.ak820_rgb_music_start(scene, AudioOptions::default(), true);
        assert!(matches!(
            app.ak820_rgb_runtime.playback.as_ref().unwrap().input,
            Input::MusicDemo(_)
        ));
        let p = app.ak820_rgb_runtime.playback.as_ref().unwrap();
        let (now, wall) = (p.started, p.last_wall_tick);
        app.tick_ak820_rgb_scene_at(&egui::Context::default(), now, wall, 90);
        assert!(app
            .ak820_rgb_scene_preview()
            .unwrap()
            .iter()
            .flatten()
            .all(|&c| c <= 7));
        assert!(app.hid_device.is_none());
        app.ak820_rgb_scene_stop();
        assert!(!app.ak820_rgb_scene_running());
    }

    #[test]
    fn rgb_runtime_explicit_restore_waits_for_frame_then_restores_once() {
        let (mut app, log) = live_app(false);
        let ctx = egui::Context::default();
        app.ak820_rgb_scene_start(Scene::default());
        app.tick_ak820_rgb_scene(&ctx);
        app.ak820_rgb_stop_and_restore();
        assert!(!app.ak820_rgb_scene_running());
        assert!(app.ak820_rgb_runtime.restore.is_some());
        finish_frame(&mut app);
        app.tick_ak820_rgb_scene(&ctx);
        finish_frame(&mut app);
        assert_eq!(app.rgb_settings.effect, 2);
        assert_eq!(
            (
                app.rgb_settings.speed,
                app.rgb_settings.hue,
                app.rgb_settings.saturation,
                app.rgb_settings.brightness
            ),
            (50, 90, 255, 160)
        );
        let requests = log.requests();
        assert_eq!(requests.iter().filter(|p| p[..2] == [7, 0x41]).count(), 2);
        assert!(!requests.iter().any(|p| p[0] == 9));
        app.tick_ak820_rgb_scene(&ctx);
        assert_eq!(log.requests().len(), requests.len());
    }

    #[test]
    fn rgb_runtime_normal_stop_does_not_restore_and_pending_restore_expires() {
        for explicit in [false, true] {
            let (mut app, log) = live_app(false);
            let ctx = egui::Context::default();
            app.ak820_rgb_scene_start(Scene::default());
            app.tick_ak820_rgb_scene(&ctx);
            finish_frame(&mut app);
            if explicit {
                app.ak820_rgb_stop_and_restore();
                app.tick_ak820_rgb_scene_at(
                    &ctx,
                    Instant::now() + Duration::from_secs(3),
                    SystemTime::now() + Duration::from_secs(3),
                    720,
                );
            } else {
                app.ak820_rgb_scene_stop();
                app.tick_ak820_rgb_scene(&ctx);
            }
            assert!(!app.ak820_rgb_scene_running());
            assert_eq!(
                log.requests()
                    .iter()
                    .filter(|p| p[..2] == [7, 0x41])
                    .count(),
                1
            );
        }
    }

    #[test]
    fn rgb_runtime_restore_yields_to_backup_modal_without_writing() {
        let (mut app, log) = live_app(false);
        let ctx = egui::Context::default();
        app.ak820_rgb_scene_start(Scene::default());
        app.tick_ak820_rgb_scene(&ctx);
        finish_frame(&mut app);
        app.ak820_rgb_stop_and_restore();
        ctx.data_mut(|d| {
            d.insert_temp(
                egui::Id::new(super::super::ak820_backup_inspection::OPEN_ID),
                true,
            )
        });
        let before = log.requests().len();
        app.tick_ak820_rgb_scene(&ctx);
        assert_eq!(log.requests().len(), before);
        assert!(app.ak820_rgb_runtime.restore.is_none());
    }

    #[test]
    fn rgb_runtime_retained_openrgb_colors_follow_live_caps_without_new_packets() {
        let bridge = Bridge::start(0).unwrap();
        let mut input = Input::OpenRgb {
            bridge,
            last: Some([[255, 128, 0]; crate::ak820_rgb::LED_COUNT]),
        };
        let mut scene = Scene::default();
        scene.quiet_hours = Some(crate::ak820_rgb_compositor::QuietHours {
            start_minute: 60,
            end_minute: 120,
            brightness: 10,
        });
        assert_eq!(
            input.frame(&scene, 0, 30).unwrap().unwrap()[0],
            [160, 80, 0]
        );
        assert_eq!(input.frame(&scene, 0, 90).unwrap().unwrap()[0], [10, 5, 0]);
        scene.base.brightness = 4;
        assert_eq!(input.frame(&scene, 0, 30).unwrap().unwrap()[0], [4, 2, 0]);
    }

    #[test]
    fn rgb_runtime_cancelled_owned_frame_is_terminal_and_drops_source() {
        let (mut app, log) = live_app(false);
        let ctx = egui::Context::default();
        app.ak820_rgb_music_start(Scene::default(), AudioOptions::default(), true);
        app.tick_ak820_rgb_scene(&ctx);
        finish_frame(&mut app);
        assert!(app
            .ak820_rgb_runtime
            .playback
            .as_ref()
            .unwrap()
            .owned
            .is_some());
        // A cancellation result from a later frame must never keep playback
        // alive with an ownership token retired by the HID layer.
        let (sender, receiver) = mpsc::channel();
        let hid = app.hid_device.take().unwrap();
        app.ak820_rgb_runtime.task = Some(FrameTask {
            receiver,
            cancel: Arc::new(AtomicBool::new(true)),
            generation: app.connection_generation,
        });
        sender
            .send(FrameResult {
                hid,
                generation: app.connection_generation,
                outcome: Ok(Ak820RgbStreamResult::Cancelled),
                elapsed_ms: 1,
            })
            .ok()
            .unwrap();
        app.poll_ak820_rgb_task();
        assert!(!app.ak820_rgb_scene_running());
        let count = log.requests().len();
        app.tick_ak820_rgb_scene(&ctx);
        assert_eq!(log.requests().len(), count);
    }
}
