//! Real native event-loop exercise with an offline app and private storage.
//! Platform hide/restore calls production lifecycle functions. Hardware and
//! physical mouse clicks are explicitly outside this automated evidence.
#![cfg_attr(target_os = "macos", allow(unexpected_cfgs))]
use super::*;
use serde_json::{json, Value};
use std::sync::Mutex;
use std::time::{Duration, Instant};

static RESULT: Mutex<Option<Value>> = Mutex::new(None);

pub(crate) fn smoke_result() -> Option<Value> {
    RESULT.lock().unwrap_or_else(|e| e.into_inner()).take()
}

pub(crate) struct RuntimeSmokeApp {
    inner: EntropyApp,
    started: Instant,
    stage: u8,
    frames: u64,
    stage_frames: u64,
    stage_at: Instant,
    cases: Vec<Value>,
    failure: Option<String>,
    finished: bool,
    tray_host: &'static str,
    #[cfg(target_os = "linux")]
    tray_fixture: Option<crate::runtime_smoke_linux::TrayFixture>,
}

impl RuntimeSmokeApp {
    pub(crate) fn new(mut inner: EntropyApp) -> Self {
        assert!(crate::isolated_runtime() && crate::runtime_safety::smoke_requested());
        inner.app_settings.language = crate::i18n::Language::English;
        inner.keycode_picker.language = crate::i18n::Language::English;
        inner.app_settings.close_to_tray_behavior = CloseToTrayBehavior::Close;
        let now = Instant::now();
        let mut result = Self {
            inner,
            started: now,
            stage: 0,
            frames: 0,
            stage_frames: 0,
            stage_at: now,
            cases: vec![],
            failure: None,
            finished: false,
            tray_host: "native_desktop",
            #[cfg(target_os = "linux")]
            tray_fixture: None,
        };
        #[cfg(target_os = "linux")]
        if std::env::var("AK820_RUNTIME_SMOKE_TRAY_FIXTURE").as_deref() == Ok("1") {
            result.tray_host = crate::runtime_smoke_linux::TrayFixture::EVIDENCE;
            match crate::runtime_smoke_linux::TrayFixture::new() {
                Ok(fixture) => result.tray_fixture = Some(fixture),
                Err(error) => {
                    result.failure = Some(format!("Cannot start isolated tray host: {error}"))
                }
            }
        }
        result
    }

    fn passed(&mut self, name: &str, detail: &str) {
        self.cases
            .push(json!({"name": name, "status": "passed", "detail": detail}));
    }

    fn advance(&mut self, stage: u8) {
        self.stage = stage;
        self.stage_frames = self.frames;
        self.stage_at = Instant::now();
    }

    fn finish(&mut self, ctx: &egui::Context) {
        self.finished = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    fn fail(&mut self, ctx: &egui::Context, message: &str) {
        self.failure = Some(message.into());
        self.finish(ctx);
    }

    fn hidden(&self) -> bool {
        #[cfg(target_os = "windows")]
        {
            self.inner.windows_window_hidden_to_tray
        }
        #[cfg(target_os = "macos")]
        {
            self.inner.macos_window_hidden_to_menu_bar
        }
        #[cfg(target_os = "linux")]
        {
            self.inner.linux_window_hidden_to_tray
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        {
            false
        }
    }

    fn native_visible(&self, _frame: &eframe::Frame) -> anyhow::Result<bool> {
        #[cfg(target_os = "windows")]
        {
            use windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible;
            let hwnd = self
                .inner
                .windows_hwnd
                .ok_or_else(|| anyhow::anyhow!("Missing native HWND"))?;
            Ok(unsafe { IsWindowVisible(hwnd as windows_sys::Win32::Foundation::HWND) != 0 })
        }
        #[cfg(target_os = "macos")]
        {
            use objc::{msg_send, sel, sel_impl};
            let window = self
                .inner
                .macos_ns_window
                .ok_or_else(|| anyhow::anyhow!("Missing native NSWindow"))?;
            let visible: objc::runtime::BOOL =
                unsafe { msg_send![window as *mut objc::runtime::Object, isVisible] };
            Ok(visible != objc::runtime::NO)
        }
        #[cfg(target_os = "linux")]
        {
            crate::runtime_smoke_linux::native_window_visible(_frame)
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        {
            anyhow::bail!("Native tray smoke is not supported on this platform")
        }
    }

    fn record(&mut self) {
        let passed = self.finished && self.failure.is_none() && self.stage == 5;
        *RESULT.lock().unwrap_or_else(|e| e.into_inner()) = Some(json!({
            "format": "ak820-runtime-smoke-1", "application_version": env!("CARGO_PKG_VERSION"),
            "os": std::env::consts::OS, "arch": std::env::consts::ARCH,
            "status": if passed { "passed" } else { "failed" },
            "rendered_frames": self.frames, "elapsed_ms": self.started.elapsed().as_millis(),
            "cases": self.cases, "error": self.failure,
            "evidence": "native_gui_offline_model", "isolated_settings": true,
            "tray_host": self.tray_host,
            "startup_timings": crate::startup::timings(),
            "live_hid_allowed": false, "physical_behavior_validated": false,
            "exclusions": ["physical keyboard and LED behavior", "actual tray mouse click routing",
                "desktop panel rendering with synthetic tray host", "Wayland qualification when run under Xvfb",
                "live OS helper and HID latency"]
        }));
    }
}

impl eframe::App for RuntimeSmokeApp {
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        eframe::App::clear_color(&self.inner, visuals)
    }

    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if self.finished {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if self.failure.is_some() {
            self.finish(ctx);
            return;
        }
        if self.started.elapsed() > Duration::from_secs(20) {
            self.fail(ctx, "Native runtime scenario exceeded its deadline");
            return;
        }
        eframe::App::logic(&mut self.inner, ctx, frame);
        #[cfg(target_os = "windows")]
        self.inner.cache_windows_hwnd(frame);
        #[cfg(target_os = "macos")]
        self.inner.cache_macos_ns_window(frame);
        #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
        {
            self.inner.handle_tray_restore_request(ctx);
            self.inner.handle_tray_quit_request(ctx);
            self.inner.poll_tray_events(ctx);
        }
        ctx.request_repaint_after(Duration::from_millis(25));
        let rendered = self.frames.saturating_sub(self.stage_frames);
        match self.stage {
            0 if rendered >= 3 => {
                self.passed(
                    "window_launch",
                    "Native renderer completed at least three application frames",
                );
                self.inner.main_menu_tab = MainMenuTab::Settings;
                self.inner.settings_tab = SettingsTab::Rgb;
                self.advance(1);
            }
            1 if rendered >= 3 => {
                if self.inner.settings_tab != SettingsTab::Rgb {
                    self.fail(ctx, "RGB navigation did not remain on the selected page");
                    return;
                }
                self.passed(
                    "rgb_navigation",
                    "Actual RGB settings page remained selected across rendered frames",
                );
                self.inner.open_app_settings_page();
                self.advance(2);
            }
            2 if rendered >= 3 => {
                self.passed(
                    "settings_navigation",
                    "Application settings rendered using private temporary preferences",
                );
                self.inner.minimize_window_to_tray(ctx);
                if !self.inner.native_tray_available() || !self.hidden() {
                    self.fail(
                        ctx,
                        "Native tray could not be created or window was not hidden",
                    );
                    return;
                }
                self.advance(3);
            }
            3 if self.stage_at.elapsed() >= Duration::from_millis(250) => {
                if !matches!(self.native_visible(frame), Ok(false)) {
                    self.fail(
                        ctx,
                        "Native OS window remained visible after hide or visibility query failed",
                    );
                    return;
                }
                #[cfg(target_os = "linux")]
                if let Some(fixture) = &self.tray_fixture {
                    if !fixture.registered() {
                        self.fail(ctx, "Linux tray did not register with the protocol host");
                        return;
                    }
                    if let Err(error) = fixture.activate() {
                        self.fail(ctx, &format!("Linux tray activation failed: {error}"));
                        return;
                    }
                } else {
                    self.inner.restore_from_tray(ctx);
                }
                #[cfg(not(target_os = "linux"))]
                self.inner.restore_from_tray(ctx);
                self.advance(4);
            }
            4 if rendered >= 3 => {
                if !matches!(self.native_visible(frame), Ok(true)) || self.hidden() {
                    self.fail(ctx, "Native OS window did not restore from the tray");
                    return;
                }
                self.passed("native_tray", "Production tray registered; native visibility changed hidden to visible; rendering resumed");
                #[cfg(target_os = "linux")]
                if self.tray_fixture.is_some() {
                    self.inner.minimize_window_to_tray(ctx);
                    if !self.hidden() {
                        self.fail(ctx, "Linux tray could not hide for host-loss check");
                        return;
                    }
                    self.tray_fixture.take();
                    self.advance(6);
                    return;
                }
                self.passed(
                    "render_after_navigation",
                    "Native application continued rendering after lifecycle exercise",
                );
                self.advance(5);
                self.finish(ctx);
            }
            #[cfg(target_os = "linux")]
            6 if rendered >= 3
                && !self.hidden()
                && matches!(self.native_visible(frame), Ok(true)) =>
            {
                self.passed(
                    "native_tray_host_loss",
                    "Window restored after the isolated tray host disappeared",
                );
                self.passed(
                    "render_after_navigation",
                    "Native application continued rendering after lifecycle exercise",
                );
                self.advance(5);
                self.finish(ctx);
            }
            _ => {}
        }
    }

    fn ui(&mut self, root: &mut egui::Ui, frame: &mut eframe::Frame) {
        eframe::App::ui(&mut self.inner, root, frame);
        self.frames += 1;
    }

    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        eframe::App::on_exit(&mut self.inner, gl);
        self.record();
    }
}
