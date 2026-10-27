//! Paint a launch acknowledgement before full editor/font initialization and discovery.
//! Timings are local observations; a submitted egui frame is not an OS display fence.
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

static PROCESS_ENTRY: OnceLock<Instant> = OnceLock::new();
static TIMINGS: Mutex<Vec<(&'static str, f64)>> = Mutex::new(Vec::new());

pub(crate) fn mark_process_entry() {
    PROCESS_ENTRY.get_or_init(Instant::now);
}

pub(crate) fn mark(phase: &'static str) {
    let elapsed = PROCESS_ENTRY
        .get_or_init(Instant::now)
        .elapsed()
        .as_secs_f64()
        * 1000.0;
    log::info!("Startup {phase}: {elapsed:.1} ms from process entry");
    TIMINGS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push((phase, elapsed));
}

pub(crate) fn timings() -> serde_json::Value {
    let entries = TIMINGS.lock().unwrap_or_else(|e| e.into_inner());
    serde_json::json!({
        "unit": "milliseconds_from_process_entry",
        "phases": entries.iter().map(|(name, elapsed)| serde_json::json!({
            "phase": name, "elapsed_ms": elapsed
        })).collect::<Vec<_>>(),
        "display_fence_measured": false
    })
}

#[derive(Default)]
struct LaunchGate {
    initially_hidden: bool,
    splash_frame: Option<u64>,
    initialized: bool,
    initialized_frame: Option<u64>,
}

impl LaunchGate {
    fn new(initially_hidden: bool) -> Self {
        Self {
            initially_hidden,
            ..Self::default()
        }
    }

    fn splash_submitted(&mut self, frame: u64) -> bool {
        if self.splash_frame.is_none() {
            self.splash_frame = Some(frame);
            true
        } else {
            false
        }
    }

    fn editor_can_render(&self, frame: u64) -> bool {
        self.initialized_frame
            .is_some_and(|initialized| frame > initialized)
    }

    fn take_initialization(&mut self, frame: u64) -> bool {
        if !self.initialized
            && (self.initially_hidden || self.splash_frame.is_some_and(|previous| frame > previous))
        {
            self.initialized = true;
            self.initialized_frame = Some(frame);
            true
        } else {
            false
        }
    }
}

pub(crate) struct StartupApp {
    launch_minimized: bool,
    gate: LaunchGate,
    logo: Option<egui::TextureHandle>,
    inner: Option<Box<dyn eframe::App>>,
}

impl StartupApp {
    pub(crate) fn new(launch_minimized: bool) -> Self {
        mark("window_creator");
        Self {
            launch_minimized,
            // Windows creates --minimized windows invisible. Eframe calls logic
            // there but skips ui; initialize the tray/restore handlers directly.
            // Visible launches still require a completed splash frame.
            gate: LaunchGate::new(cfg!(target_os = "windows") && launch_minimized),
            logo: None,
            inner: None,
        }
    }
}

impl eframe::App for StartupApp {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Visible launches wait for a later completed splash frame, including
        // repeated egui passes. Initially invisible Windows launches have no ui
        // callback and must initialize from logic to install tray/restore handlers.
        if self.gate.take_initialization(ctx.cumulative_frame_nr()) {
            mark("editor_initialization_started");
            self.inner = Some(crate::initialize_gui(ctx, self.launch_minimized));
            mark("editor_initialization_finished");
            ctx.request_repaint();
        }
        if let Some(inner) = &mut self.inner {
            inner.logic(ctx, frame);
        }
    }

    fn ui(&mut self, root: &mut egui::Ui, frame: &mut eframe::Frame) {
        // set_fonts applies at the next egui frame. The root Ui also needs to
        // inherit the newly applied editor visuals; keep the launch screen until
        // that frame instead of submitting a partially initialized editor.
        if self
            .gate
            .editor_can_render(root.ctx().cumulative_frame_nr())
        {
            let inner = self
                .inner
                .as_mut()
                .expect("render readiness requires initialization");
            inner.ui(root, frame);
            self.logo = None;
            if self.gate.splash_frame.take().is_some() {
                mark("editor_first_frame_submitted");
            }
            return;
        }
        let texture = self.logo.get_or_insert_with(|| {
            let image = image::load_from_memory_with_format(
                include_bytes!("../assets/ak-configurator-logo.png"),
                image::ImageFormat::Png,
            )
            .expect("bundled launch logo must be valid PNG")
            .into_rgba8();
            root.ctx().load_texture(
                "launch-logo",
                egui::ColorImage::from_rgba_unmultiplied(
                    [image.width() as usize, image.height() as usize],
                    image.as_raw(),
                ),
                egui::TextureOptions::LINEAR,
            )
        });
        egui::CentralPanel::default().show_inside(root, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space((ui.available_height() - 320.0).max(0.0) * 0.5);
                ui.add(egui::Image::new(&*texture).fit_to_exact_size(egui::vec2(200.0, 200.0)));
                ui.add_space(12.0);
                ui.heading(crate::APP_TITLE);
                ui.add_space(12.0);
                ui.spinner();
                // Both languages acknowledge launch before reading saved settings.
                // The editor then restores its configured language normally.
                ui.label("Запуск приложения… / Starting application…");
                ui.small("Подготовка интерфейса / Preparing the interface");
            });
        });
        if self.gate.splash_submitted(root.ctx().cumulative_frame_nr()) {
            mark("splash_first_frame_submitted");
        }
        root.ctx().request_repaint();
    }

    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        self.inner.as_ref().map_or_else(
            || visuals.panel_fill.to_normalized_gamma_f32(),
            |inner| inner.clear_color(visuals),
        )
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if let Some(inner) = &mut self.inner {
            inner.save(storage);
        }
    }

    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        if let Some(inner) = &mut self.inner {
            inner.on_exit(gl);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LaunchGate;

    #[test]
    fn editor_cannot_initialize_before_a_later_frame() {
        let mut gate = LaunchGate::default();
        assert!(!gate.take_initialization(0));
        assert!(gate.splash_submitted(7));
        assert!(!gate.take_initialization(7));
        assert!(!gate.splash_submitted(7)); // repeated layout pass
        assert!(!gate.take_initialization(6));
        assert!(gate.take_initialization(8));
        assert!(!gate.take_initialization(9)); // initialize exactly once
    }

    #[test]
    fn repaint_without_splash_cannot_start_editor() {
        let mut gate = LaunchGate::default();
        assert!(!gate.take_initialization(100));
        assert!(gate.splash_submitted(100));
        assert!(gate.take_initialization(101)); // no artificial minimum duration
    }

    #[test]
    fn initially_hidden_launch_initializes_once_without_a_ui_frame() {
        let mut gate = LaunchGate::new(true);
        assert!(gate.take_initialization(0));
        assert!(!gate.take_initialization(0));
        assert!(!gate.take_initialization(1));
        assert!(gate.splash_frame.is_none());
    }

    #[test]
    fn visible_launch_still_requires_splash_then_a_later_frame() {
        let mut gate = LaunchGate::new(false);
        assert!(!gate.take_initialization(0));
        assert!(gate.splash_submitted(0));
        assert!(!gate.take_initialization(0));
        assert!(gate.take_initialization(1));
    }

    #[test]
    fn editor_render_waits_for_fonts_and_root_style_to_reach_the_next_frame() {
        for hidden in [false, true] {
            let mut gate = LaunchGate::new(hidden);
            assert!(!gate.editor_can_render(0));
            if !hidden {
                gate.splash_submitted(0);
            }
            assert!(gate.take_initialization(1));
            assert!(!gate.editor_can_render(1)); // old fonts/root style, including repeat passes
            assert!(gate.editor_can_render(2));
            assert!(gate.editor_can_render(3));
        }
    }
}
