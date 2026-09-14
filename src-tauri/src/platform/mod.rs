#[cfg(windows)]
mod windows_external_drop;

#[cfg(target_os = "linux")]
mod linux_appimage_wayland;

#[cfg(target_os = "linux")]
pub use linux_appimage_wayland::prepare_appimage_wayland_backend;

#[cfg(not(target_os = "linux"))]
pub fn prepare_appimage_wayland_backend() {}

#[cfg(windows)]
pub use windows_external_drop::install_external_file_drop_bridge;

#[cfg(not(windows))]
pub fn install_external_file_drop_bridge(
    _window: &tauri::WebviewWindow,
) -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

pub mod window_transparency;
pub use window_transparency::{WindowTransparency, apply_to_all_main_windows, apply_to_window};
