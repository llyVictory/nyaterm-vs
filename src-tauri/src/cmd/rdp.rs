use crate::config::ConnectionType;
use crate::core::rdp::{self, RdpInputEvent, RdpSessionManager};
use crate::error::{AppError, AppResult};
#[cfg(windows)]
use std::process::Command;
use std::sync::Arc;
use tauri::Emitter;
use tauri::ipc::{Channel, InvokeResponseBody};

const DEFAULT_RDP_PORT: u16 = 3389;

fn mstsc_target(host: &str, port: u16) -> String {
    let host = host.trim();
    let server = if host.contains(':') && !(host.starts_with('[') && host.ends_with(']')) {
        format!("[{host}]")
    } else {
        host.to_string()
    };
    if port == DEFAULT_RDP_PORT {
        server
    } else {
        format!("{server}:{port}")
    }
}

#[cfg(windows)]
fn spawn_windows_rdp(target: &str) -> AppResult<()> {
    Command::new("mstsc.exe")
        .arg(format!("/v:{target}"))
        .spawn()?;
    Ok(())
}

#[cfg(not(windows))]
fn spawn_windows_rdp(_target: &str) -> AppResult<()> {
    Err(AppError::Unsupported(
        "Windows Remote Desktop is only available on Windows".to_string(),
    ))
}

#[tauri::command]
pub fn launch_windows_rdp(app: tauri::AppHandle, connection_id: String) -> AppResult<()> {
    let connection = crate::storage::get_connection_without_secret(&connection_id)?
        .ok_or_else(|| AppError::Config("RDP connection not found".to_string()))?;
    let ConnectionType::Rdp { host, port, .. } = connection.config else {
        return Err(AppError::Config(
            "Connection is not an RDP connection".to_string(),
        ));
    };
    let target = mstsc_target(&host, port);
    spawn_windows_rdp(&target)?;

    if let Err(error) = crate::storage::mark_connection_used(&connection_id) {
        tracing::warn!(connection_id, %error, "Failed to mark RDP connection as recently used");
    } else {
        let _ = app.emit("connections-changed", ());
    }
    Ok(())
}

#[tauri::command]
pub async fn create_rdp_session(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<RdpSessionManager>>,
    connection_id: String,
    create_request_id: Option<String>,
) -> AppResult<String> {
    let _ = create_request_id;
    let config = rdp::load_saved_rdp_config(&app, &connection_id)?;
    let session_id = state.create_session(app.clone(), config).await?;
    if let Err(error) = crate::storage::mark_connection_used(&connection_id) {
        tracing::warn!(connection_id, %error, "Failed to mark RDP connection as recently used");
    } else {
        let _ = app.emit("connections-changed", ());
    }
    Ok(session_id)
}

#[tauri::command]
pub async fn rdp_attach_frame_channel(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<RdpSessionManager>>,
    session_id: String,
    frame_channel: Channel<InvokeResponseBody>,
) -> AppResult<()> {
    state
        .attach_frame_channel(&app, &session_id, frame_channel)
        .await
}

#[tauri::command]
pub async fn rdp_input_batch(
    state: tauri::State<'_, Arc<RdpSessionManager>>,
    session_id: String,
    events: Vec<RdpInputEvent>,
) -> AppResult<()> {
    state.send_input(&session_id, events).await
}

#[tauri::command]
pub async fn rdp_set_keyboard_capture(
    state: tauri::State<'_, Arc<RdpSessionManager>>,
    session_id: Option<String>,
) -> AppResult<()> {
    crate::core::rdp_keyboard_capture::set_keyboard_capture(state.inner().clone(), session_id)
}

#[tauri::command]
pub async fn rdp_resize(
    state: tauri::State<'_, Arc<RdpSessionManager>>,
    session_id: String,
    width: u32,
    height: u32,
) -> AppResult<()> {
    state.resize(&session_id, width, height).await
}

#[tauri::command]
pub async fn rdp_set_clipboard_text(
    state: tauri::State<'_, Arc<RdpSessionManager>>,
    session_id: String,
    text: String,
) -> AppResult<()> {
    state.set_clipboard_text(&session_id, text).await
}

#[tauri::command]
pub async fn rdp_reconnect(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<RdpSessionManager>>,
    session_id: String,
) -> AppResult<()> {
    state.reconnect(app, &session_id).await
}

#[tauri::command]
pub async fn close_rdp_session(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<RdpSessionManager>>,
    session_id: String,
) -> AppResult<()> {
    state.close(&app, &session_id).await
}

#[tauri::command]
pub async fn respond_rdp_certificate(
    state: tauri::State<'_, Arc<RdpSessionManager>>,
    request_id: String,
    accepted: bool,
    remember: bool,
) -> AppResult<()> {
    state
        .respond_certificate(&request_id, accepted, remember)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mstsc_target_formats_hosts_and_non_default_ports() {
        assert_eq!(
            mstsc_target("server.example.com", 3389),
            "server.example.com"
        );
        assert_eq!(mstsc_target("192.0.2.10", 3390), "192.0.2.10:3390");
        assert_eq!(mstsc_target("2001:db8::1", 3389), "[2001:db8::1]");
        assert_eq!(mstsc_target("2001:db8::1", 3390), "[2001:db8::1]:3390");
        assert_eq!(mstsc_target("[2001:db8::1]", 3390), "[2001:db8::1]:3390");
    }

    #[cfg(not(windows))]
    #[test]
    fn system_rdp_launch_is_explicitly_unsupported_off_windows() {
        assert!(matches!(
            spawn_windows_rdp("server.example.com"),
            Err(AppError::Unsupported(_))
        ));
    }
}
