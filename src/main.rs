#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod action_icons;
mod ak820;
mod ak820_backup_capture;
mod ak820_backup_files;
mod ak820_diagnostics;
mod ak820_help;
mod ak820_image_pack;
mod ak820_install_executor;
mod ak820_install_plan;
mod ak820_installer;
mod ak820_loader;
mod ak820_offline_project;
#[cfg(not(target_arch = "wasm32"))]
mod ak820_openrgb_bridge;
mod ak820_os_indicator;
mod ak820_release;
mod ak820_restore_plan;
mod ak820_rgb;
#[cfg(not(target_arch = "wasm32"))]
mod ak820_rgb_audio;
mod ak820_rgb_compositor;
mod ak820_rgb_scheduler;
mod ak820_settings_backup;
mod ak820_settings_model;
mod ak820_typed_storage;
mod ak820_qs_versioned_offline;
mod ak820_qs_nvm_migration_offline;
mod ak820_settings_identity_metadata_offline;
mod ak820_versioned_backup_offline;
mod app;
#[cfg(not(target_arch = "wasm32"))]
mod app_discovery;
pub(crate) mod app_icon;
mod application_layouts;
mod device;
mod diagnostics;
mod firmware;
mod firmware_builtin_icons;
#[cfg(not(target_arch = "wasm32"))]
mod hid;
#[cfg(not(target_arch = "wasm32"))]
mod hid_ownership;
mod i18n;
mod integration_icons;
mod keyboard;
mod keycode;
mod keycode_picker;
#[cfg(all(feature = "mcp", not(target_arch = "wasm32")))]
mod mcp;
#[cfg(test)]
mod layouts;
#[cfg(target_os = "linux")]
mod linux_ble;
#[cfg(target_os = "linux")]
mod linux_setup;
#[cfg(not(target_arch = "wasm32"))]
mod package_self_check;
#[cfg(not(target_arch = "wasm32"))]
mod pdf;
mod popup_state;
#[cfg(not(target_arch = "wasm32"))]
mod qmk_hid_host;
mod rmk_native;
#[cfg(not(target_arch = "wasm32"))]
mod runtime_safety;
#[cfg(target_os = "linux")]
mod runtime_smoke_linux;
mod smart_input;
mod startup;
mod text_expander;
mod ui_style;
mod universal_symbols;

use app::EntropyApp;

const APP_TITLE: &str = "AK Configurator";
const APP_ID: &str = "ak-configurator";

/// Offline test/demo processes cannot acquire native devices or host services.
/// Resolve the mode before preferences, discovery, helpers or GUI bootstrap.
pub(crate) fn isolated_runtime() -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        cfg!(test) || runtime_safety::isolated()
    }
    #[cfg(target_arch = "wasm32")]
    {
        cfg!(test)
    }
}

// Route configuration and cache lookup through isolated roots in unit tests.
pub(crate) fn config_dir() -> Option<std::path::PathBuf> {
    #[cfg(test)]
    {
        Some(app::test_config_base_dir())
    }
    #[cfg(not(test))]
    {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(root) = runtime_safety::config_dir() {
            return Some(root);
        }
        dirs::config_dir()
    }
}

pub(crate) fn cache_dir() -> Option<std::path::PathBuf> {
    #[cfg(test)]
    {
        Some(app::test_config_base_dir().join("cache"))
    }
    #[cfg(not(test))]
    {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(root) = runtime_safety::config_dir() {
            return Some(root.join("cache"));
        }
        dirs::cache_dir()
    }
}
const SINGLE_INSTANCE_ENV: &str = "AK_CONFIGURATOR_SINGLE_INSTANCE";
const LAUNCH_MINIMIZED_ARG: &str = "--minimized";
#[cfg(not(target_arch = "wasm32"))]
static BACKGROUND_HELPER_REQUESTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn background_helper_allowed() -> bool {
    !isolated_runtime() && single_instance_enabled()
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn request_background_helper() {
    if background_helper_allowed() {
        BACKGROUND_HELPER_REQUESTED.store(true, std::sync::atomic::Ordering::Release);
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn background_helper_requested() -> bool {
    BACKGROUND_HELPER_REQUESTED.load(std::sync::atomic::Ordering::Acquire)
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn cancel_background_helper_request() {
    BACKGROUND_HELPER_REQUESTED.store(false, std::sync::atomic::Ordering::Release);
}

#[cfg(target_os = "windows")]
struct SingleInstanceGuard(*mut core::ffi::c_void);

#[cfg(any(target_os = "linux", target_os = "macos"))]
struct SingleInstanceGuard(i32);

#[cfg(target_os = "windows")]
impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        unsafe {
            if !self.0.is_null() {
                CloseHandle(self.0);
            }
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = flock(self.0, LOCK_UN);
            let _ = <std::fs::File as std::os::fd::FromRawFd>::from_raw_fd(self.0);
        }
    }
}

#[cfg(target_os = "windows")]
fn try_acquire_single_instance() -> bool {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null_mut;

    const ERROR_ALREADY_EXISTS: u32 = 183;
    let name: Vec<u16> = OsStr::new("Global\\AKConfiguratorSingleInstance")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let handle = CreateMutexW(null_mut(), 1, name.as_ptr());
        if handle.is_null() {
            return true;
        }
        let already_exists = GetLastError() == ERROR_ALREADY_EXISTS;
        if already_exists {
            CloseHandle(handle);
            false
        } else {
            let _guard = Box::leak(Box::new(SingleInstanceGuard(handle)));
            true
        }
    }
}

fn notify_existing_instance() {
    let dir = crate::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(crate::APP_ID);
    let _ = std::fs::create_dir_all(&dir);
    let signal_path = dir.join("single_instance_signal");
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis().to_string())
        .unwrap_or_else(|_| "0".to_string());
    let _ = std::fs::write(signal_path, now_ms);
}

fn single_instance_enabled_from_env(value: Option<&str>) -> bool {
    !matches!(
        value.map(str::trim).map(str::to_ascii_lowercase).as_deref(),
        Some("0" | "false" | "off" | "no")
    )
}

fn single_instance_enabled() -> bool {
    if isolated_runtime() {
        return false;
    }
    let value = std::env::var(SINGLE_INSTANCE_ENV).ok();
    single_instance_enabled_from_env(value.as_deref())
}

fn launch_minimized_from_args<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    args.into_iter()
        .any(|arg| arg.as_ref() == std::ffi::OsStr::new(LAUNCH_MINIMIZED_ARG))
}

#[cfg(target_os = "windows")]
fn restore_existing_instance_window() {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, FindWindowW, GetWindowTextLengthW, GetWindowTextW, SetForegroundWindow,
        ShowWindow, SW_RESTORE, SW_SHOW,
    };

    unsafe extern "system" fn find_entropy_window(hwnd: HWND, lparam: LPARAM) -> i32 {
        let len = unsafe { GetWindowTextLengthW(hwnd) };
        if len <= 0 {
            return 1;
        }

        let mut title = vec![0u16; len as usize + 1];
        let copied = unsafe { GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32) };
        if copied <= 0 {
            return 1;
        }

        let title = String::from_utf16_lossy(&title[..copied as usize]);
        if title == APP_TITLE || title.starts_with("AK Configurator (v") {
            let out = lparam as *mut HWND;
            if !out.is_null() {
                unsafe {
                    *out = hwnd;
                }
            }
            return 0;
        }

        1
    }

    let exact_title: Vec<u16> = OsStr::new(APP_TITLE)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let mut hwnd = FindWindowW(std::ptr::null(), exact_title.as_ptr());
        if hwnd.is_null() {
            let mut found: HWND = std::ptr::null_mut();
            EnumWindows(Some(find_entropy_window), &mut found as *mut HWND as LPARAM);
            hwnd = found;
        }

        if !hwnd.is_null() {
            ShowWindow(hwnd, SW_SHOW);
            ShowWindow(hwnd, SW_RESTORE);
            SetForegroundWindow(hwnd);
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn try_acquire_single_instance() -> bool {
    use std::fs::OpenOptions;
    use std::os::fd::IntoRawFd;

    let dir = crate::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(crate::APP_ID);
    if std::fs::create_dir_all(&dir).is_err() {
        return true;
    }

    let Ok(file) = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(dir.join("single_instance.lock"))
    else {
        return true;
    };
    let fd = file.into_raw_fd();
    let locked = unsafe { flock(fd, LOCK_EX | LOCK_NB) == 0 };
    if locked {
        let _guard = Box::leak(Box::new(SingleInstanceGuard(fd)));
        true
    } else {
        unsafe {
            let _ = <std::fs::File as std::os::fd::FromRawFd>::from_raw_fd(fd);
        }
        false
    }
}

#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
extern "system" {
    fn CreateMutexW(
        lpMutexAttributes: *mut core::ffi::c_void,
        bInitialOwner: i32,
        lpName: *const u16,
    ) -> *mut core::ffi::c_void;
    fn GetLastError() -> u32;
    fn CloseHandle(hObject: *mut core::ffi::c_void) -> i32;
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
const LOCK_EX: i32 = 2;
#[cfg(any(target_os = "linux", target_os = "macos"))]
const LOCK_NB: i32 = 4;
#[cfg(any(target_os = "linux", target_os = "macos"))]
const LOCK_UN: i32 = 8;

#[cfg(any(target_os = "linux", target_os = "macos"))]
extern "C" {
    fn flock(fd: i32, operation: i32) -> i32;
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
fn try_acquire_single_instance() -> bool {
    true
}

fn eframe_persistence_path() -> std::path::PathBuf {
    let dir = crate::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(APP_ID);
    let _ = std::fs::create_dir_all(&dir);
    dir.join("eframe_state.ron")
}

fn initial_window_size() -> [f32; 2] {
    #[derive(serde::Deserialize)]
    struct StartupSettings {
        #[serde(default)]
        window_size: Option<[f32; 2]>,
    }

    let path = crate::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(APP_ID)
        .join("app_settings.json");
    let Some(size) = std::fs::read_to_string(path)
        .ok()
        .and_then(|data| serde_json::from_str::<StartupSettings>(&data).ok())
        .and_then(|settings| settings.window_size)
    else {
        return [1200.0, 700.0];
    };

    if size[0].is_finite() && size[1].is_finite() {
        [size[0].clamp(800.0, 10000.0), size[1].clamp(500.0, 10000.0)]
    } else {
        [1200.0, 700.0]
    }
}

fn main() -> eframe::Result<()> {
    startup::mark_process_entry();
    #[cfg(not(target_arch = "wasm32"))]
    if let Err(error) = runtime_safety::initialize(std::env::args_os()) {
        eprintln!("{error}");
        std::process::exit(2);
    }
    // Packaging probes must return before proxy/device discovery, preferences,
    // single-instance signalling, platform HID initialization or GUI startup.
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(code) = package_self_check::run_if_requested(std::env::args_os()) {
        std::process::exit(code);
    }

    #[cfg(not(target_arch = "wasm32"))]
    if hid::run_hid_proxy_if_requested() {
        return Ok(());
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let args: Vec<_> = std::env::args_os().skip(1).collect();
        if args.iter().any(|arg| arg.to_string_lossy().starts_with("--mcp-")) {
            #[cfg(feature = "mcp")]
            {
                if let Err(error) = mcp::run(&args) {
                    eprintln!("{error}");
                    std::process::exit(2);
                }
                return Ok(());
            }
            #[cfg(not(feature = "mcp"))]
            {
                eprintln!("MCP is not enabled in this build; build with --features mcp");
                std::process::exit(2);
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    let headless_export = match app::HeadlessExportRequest::from_args(std::env::args_os()) {
        Ok(request) => request,
        Err(usage) => {
            eprintln!("{usage}");
            std::process::exit(app::EXIT_USAGE);
        }
    };

    diagnostics::init(diagnostics::settings_file_enabled());
    let mut launch_minimized = launch_minimized_from_args(std::env::args_os());

    // A headless export shares the single-instance lock with the GUI: two
    // processes speaking Vial to one keyboard would interleave requests.
    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Some(request) = headless_export {
            if single_instance_enabled() && !try_acquire_single_instance() {
                log::error!(
                    "Another AK Configurator instance is running; close it or export from its Layout menu"
                );
                std::process::exit(app::EXIT_INSTANCE_RUNNING);
            }
            std::process::exit(app::run_headless_export(request));
        }
    }

    #[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
    {
        if single_instance_enabled() {
            if !try_acquire_single_instance() {
                if !launch_minimized {
                    notify_existing_instance();
                    #[cfg(target_os = "windows")]
                    restore_existing_instance_window();
                }
                return Ok(());
            }
        } else {
            log::info!("Single-instance guard disabled by {SINGLE_INSTANCE_ENV}");
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    let _single_instance_available = try_acquire_single_instance();

    #[cfg(target_os = "macos")]
    if !isolated_runtime() {
        hid::initialize_macos_hid_on_main_thread();
    }

    // Autostart can enter the opted-in helper without creating a renderer or
    // loading GUI fonts. The same process retains the single-instance lock.
    #[cfg(not(target_arch = "wasm32"))]
    if std::env::args_os().any(|a| a == "--ak820-helper")
        || (launch_minimized && app::background_helper_enabled() && background_helper_allowed())
    {
        if !background_helper_allowed() {
            return Ok(());
        }
        if matches!(app::run_background_helper(), app::HelperExit::Disabled) {
            return Ok(());
        }
        launch_minimized = false;
    }

    loop {
        run_native_gui(launch_minimized)?;
        #[cfg(not(target_arch = "wasm32"))]
        if runtime_safety::smoke_requested() {
            let report = app::smoke_result().unwrap_or_else(|| serde_json::json!({
                "format": "ak820-runtime-smoke-1", "status": "failed",
                "error": "Native application exited before producing its smoke report"
            }));
            println!("{}", report);
            if report["status"] != "passed" { std::process::exit(1); }
            return Ok(());
        }
        #[cfg(not(target_arch = "wasm32"))]
        if BACKGROUND_HELPER_REQUESTED.swap(false, std::sync::atomic::Ordering::AcqRel)
            && background_helper_allowed()
            && matches!(app::run_background_helper(), app::HelperExit::OpenInterface)
        {
            launch_minimized = false;
            continue;
        }
        return Ok(());
    }
}

fn run_native_gui(launch_minimized: bool) -> eframe::Result {
    let viewport = egui::ViewportBuilder::default()
        .with_title(APP_TITLE)
        .with_app_id(APP_ID)
        .with_icon(app_icon::egui_icon(64))
        .with_inner_size(initial_window_size())
        .with_min_inner_size([800.0, 500.0]);
    #[cfg(target_os = "windows")]
    let viewport = viewport.with_visible(!launch_minimized);

    let options = eframe::NativeOptions {
        viewport,
        persistence_path: Some(eframe_persistence_path()),
        centered: true,
        // eframe reuses its native event loop but drops the app, windows and
        // renderer before returning. Only the helper remains between opens.
        run_and_return: true,
        ..Default::default()
    };

    eframe::run_native(
        APP_TITLE,
        options,
        Box::new(move |_cc| Ok(Box::new(startup::StartupApp::new(launch_minimized)))),
    )
}

fn initialize_gui(ctx: &egui::Context, launch_minimized: bool) -> Box<dyn eframe::App> {
    // Roboto as primary UI font, with Unicode/symbol fallbacks.
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "roboto".to_owned(),
        egui::FontData::from_static(include_bytes!("../assets/Roboto-Regular.ttf")).into(),
    );
    fonts.font_data.insert(
        "montserrat_medium".to_owned(),
        egui::FontData::from_static(include_bytes!("../assets/Montserrat-Medium.ttf"))
            .into(),
    );
    fonts.font_data.insert(
        "dejavu".to_owned(),
        egui::FontData::from_static(include_bytes!("../assets/DejaVuSans.ttf")).into(),
    );
    fonts.font_data.insert(
        "noto_symbols".to_owned(),
        egui::FontData::from_static(include_bytes!(
            "../assets/NotoSansSymbols2-Regular.ttf"
        ))
        .into(),
    );
    fonts.font_data.insert(
        "noto_emoji".to_owned(),
        egui::FontData::from_static(include_bytes!("../assets/NotoEmoji-Regular.ttf"))
            .into(),
    );
    for (name, data) in [
        (
            "clock_ubuntu_sans",
            include_bytes!("../assets/Clock-UbuntuSans.ttf").as_slice(),
        ),
        (
            "clock_ubuntu_mono",
            include_bytes!("../assets/Clock-UbuntuMono.ttf").as_slice(),
        ),
        (
            "clock_liberation_mono",
            include_bytes!("../assets/Clock-LiberationMono.ttf").as_slice(),
        ),
        (
            "clock_dejavu_sans",
            include_bytes!("../assets/Clock-DejaVuSans.ttf").as_slice(),
        ),
        (
            "clock_dejavu_serif",
            include_bytes!("../assets/Clock-DejaVuSerif.ttf").as_slice(),
        ),
        (
            "clock_dejavu_mono",
            include_bytes!("../assets/Clock-DejaVuMono.ttf").as_slice(),
        ),
        (
            "clock_liberation_sans",
            include_bytes!("../assets/Clock-LiberationSans.ttf").as_slice(),
        ),
        (
            "clock_liberation_serif",
            include_bytes!("../assets/Clock-LiberationSerif.ttf").as_slice(),
        ),
        (
            "clock_liberation_narrow",
            include_bytes!("../assets/EntropyDisplay-Narrow.ttf").as_slice(),
        ),
    ] {
        fonts
            .font_data
            .insert(name.to_owned(), egui::FontData::from_static(data).into());
    }
    let prop = fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default();
    prop.insert(0, "roboto".to_owned());
    prop.push("dejavu".to_owned());
    prop.push("noto_symbols".to_owned());
    prop.push("noto_emoji".to_owned());
    let mono = fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default();
    mono.push("dejavu".to_owned());
    mono.push("noto_symbols".to_owned());
    mono.push("noto_emoji".to_owned());
    fonts.families.insert(
        egui::FontFamily::Name("emoji_preview".into()),
        vec![
            "noto_emoji".to_owned(),
            "noto_symbols".to_owned(),
            "dejavu".to_owned(),
        ],
    );
    fonts.families.insert(
        egui::FontFamily::Name("display_preview".into()),
        vec![
            "montserrat_medium".to_owned(),
            "dejavu".to_owned(),
            "noto_symbols".to_owned(),
        ],
    );
    for (family, font) in [
        ("clock_montserrat", "montserrat_medium"),
        ("clock_ubuntu_sans", "clock_ubuntu_sans"),
        ("clock_ubuntu_mono", "clock_ubuntu_mono"),
        ("clock_liberation_mono", "clock_liberation_mono"),
        ("clock_dejavu_sans", "clock_dejavu_sans"),
        ("clock_dejavu_serif", "clock_dejavu_serif"),
        ("clock_dejavu_mono", "clock_dejavu_mono"),
        ("clock_liberation_sans", "clock_liberation_sans"),
        ("clock_liberation_serif", "clock_liberation_serif"),
        ("clock_liberation_narrow", "clock_liberation_narrow"),
    ] {
        fonts
            .families
            .insert(egui::FontFamily::Name(family.into()), vec![font.to_owned()]);
    }
    ctx.set_fonts(fonts);
    startup::mark("editor_fonts_configured");
    let mut app = if isolated_runtime() {
        EntropyApp::new_ak820_demo_for_context(ctx)
    } else {
        EntropyApp::new_for_context(ctx)
    };
    app.apply_initial_ui_theme(ctx);
    #[cfg(target_os = "windows")]
    let app = if launch_minimized {
        app.with_start_hidden_to_tray()
    } else {
        app
    };
    #[cfg(not(target_os = "windows"))]
    if launch_minimized {
        ctx
            .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
    }
    #[cfg(not(target_arch = "wasm32"))]
    if runtime_safety::smoke_requested() {
        return Box::new(app::RuntimeSmokeApp::new(app));
    }
    Box::new(app)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_and_cache_paths_use_the_isolated_test_root() {
        let root = app::test_config_base_dir();
        assert_eq!(config_dir(), Some(root.clone()));
        assert_eq!(cache_dir(), Some(root.join("cache")));
        assert!(eframe_persistence_path().starts_with(&root));
        let worker_root = std::thread::spawn(|| {
            let root = app::test_config_base_dir();
            assert_eq!(config_dir(), Some(root.clone()));
            assert_eq!(cache_dir(), Some(root.join("cache")));
            root
        })
        .join()
        .unwrap();
        assert_eq!(worker_root, root);
    }

    #[test]
    fn single_instance_is_enabled_by_default() {
        assert!(single_instance_enabled_from_env(None));
    }

    #[test]
    fn single_instance_can_be_disabled_for_debugging() {
        for value in ["0", "false", "off", "no", " FALSE "] {
            assert!(!single_instance_enabled_from_env(Some(value)));
        }
    }

    #[test]
    fn single_instance_stays_enabled_for_other_values() {
        for value in ["1", "true", "yes", "anything"] {
            assert!(single_instance_enabled_from_env(Some(value)));
        }
    }

    #[test]
    fn minimized_launch_argument_is_explicit() {
        assert!(launch_minimized_from_args(["entropy", "--minimized"]));
        assert!(!launch_minimized_from_args(["entropy"]));
        assert!(!launch_minimized_from_args(["entropy", "--minimize"]));
    }
}
