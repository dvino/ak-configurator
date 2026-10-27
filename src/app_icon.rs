const ICON_PNG: &[u8] = include_bytes!("../assets/ak-configurator-icon.png");

pub(crate) fn rgba_icon(size: u32) -> Vec<u8> {
    let size = size.max(1);
    image::load_from_memory_with_format(ICON_PNG, image::ImageFormat::Png)
        .expect("bundled application icon must be valid PNG")
        .resize_exact(size, size, image::imageops::FilterType::Lanczos3)
        .into_rgba8()
        .into_raw()
}

pub(crate) fn egui_icon(size: u32) -> egui::IconData {
    let size = size.max(1);
    egui::IconData { rgba: rgba_icon(size), width: size, height: size }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub(crate) fn tray_icon(size: u32) -> Option<tray_icon::Icon> {
    let size = size.max(1);
    tray_icon::Icon::from_rgba(rgba_icon(size), size, size).ok()
}
