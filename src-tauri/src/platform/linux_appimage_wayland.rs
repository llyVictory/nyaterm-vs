const PREFERRED_APPIMAGE_WAYLAND_BACKENDS: &str = "wayland,x11";

pub fn prepare_appimage_wayland_backend() {
    let is_appimage = env_var_is_non_empty("APPIMAGE") || env_var_is_non_empty("APPDIR");
    let has_wayland_display = env_var_is_non_empty("WAYLAND_DISPLAY");
    let current_backend = std::env::var("GDK_BACKEND").ok();

    let Some(backend) =
        preferred_gdk_backend(is_appimage, has_wayland_display, current_backend.as_deref())
    else {
        return;
    };

    // SAFETY: `run()` calls this synchronously before Tauri/GTK initialization and before
    // NyaTerm starts worker threads. Changing the process environment is therefore not racing
    // with environment reads in other threads.
    unsafe {
        std::env::set_var("GDK_BACKEND", backend);
    }
}

fn env_var_is_non_empty(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|value| !value.is_empty())
}

fn preferred_gdk_backend(
    is_appimage: bool,
    has_wayland_display: bool,
    current_backend: Option<&str>,
) -> Option<&'static str> {
    // Tauri's AppImage GTK hook currently forces GDK_BACKEND=x11. On modern Wayland
    // sessions that sends WebKitGTK through XWayland and can leave the webview unable to
    // receive pointer/keyboard input. Prefer native Wayland while retaining X11 as a fallback.
    (is_appimage && has_wayland_display && current_backend == Some("x11"))
        .then_some(PREFERRED_APPIMAGE_WAYLAND_BACKENDS)
}

#[cfg(test)]
mod tests {
    use super::{PREFERRED_APPIMAGE_WAYLAND_BACKENDS, preferred_gdk_backend};

    #[test]
    fn prefers_wayland_for_appimage_when_tauri_forced_x11() {
        assert_eq!(
            preferred_gdk_backend(true, true, Some("x11")),
            Some(PREFERRED_APPIMAGE_WAYLAND_BACKENDS)
        );
    }

    #[test]
    fn leaves_non_appimage_linux_packages_unchanged() {
        assert_eq!(preferred_gdk_backend(false, true, Some("x11")), None);
    }

    #[test]
    fn leaves_x11_sessions_unchanged() {
        assert_eq!(preferred_gdk_backend(true, false, Some("x11")), None);
    }

    #[test]
    fn preserves_existing_non_x11_backend_selection() {
        assert_eq!(preferred_gdk_backend(true, true, Some("wayland")), None);
        assert_eq!(preferred_gdk_backend(true, true, Some("wayland,x11")), None);
        assert_eq!(preferred_gdk_backend(true, true, None), None);
    }
}
