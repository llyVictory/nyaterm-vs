//! Native event/IPC adapter for the shared VNC protocol engine.
use crate::error::{AppError, AppResult};
use nyaterm_core::vnc::{
    self as shared,
    runtime::{FrameChannel, VncContext},
};
pub use nyaterm_core::vnc::{VncConnectConfig, VncInputEvent};
use std::sync::Arc;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Emitter};

fn context(app: &AppHandle) -> VncContext {
    let events_app = app.clone();
    let transport_app = app.clone();
    VncContext {
        events: Arc::new(move |target, event, payload| {
            let result = if let Some(target) = target {
                events_app.emit_to(target, event, payload)
            } else {
                events_app.emit(event, payload)
            };
            result.map_err(|e| AppError::Channel(e.to_string()))
        }),
        transport: Arc::new(move |host, port, network, owner| {
            let app = transport_app.clone();
            Box::pin(async move {
                crate::core::network::open_tcp_transport(&app, &host, port, network.as_ref(), owner)
                    .await
            })
        }),
    }
}
pub fn load_saved_vnc_config(
    app: &AppHandle,
    id: &str,
    owner: String,
) -> AppResult<VncConnectConfig> {
    shared::load_saved_vnc_config(&context(app), id, owner)
}
pub struct VncSessionManager {
    inner: Arc<shared::VncSessionManager>,
}
impl Default for VncSessionManager {
    fn default() -> Self {
        Self::new()
    }
}
impl VncSessionManager {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(shared::VncSessionManager::new()),
        }
    }
    pub async fn create_session(
        self: &Arc<Self>,
        app: AppHandle,
        config: VncConnectConfig,
    ) -> AppResult<String> {
        if !crate::window_state::is_main_window_label(&config.owner_window_label) {
            return Err(AppError::Config(
                "VNC session requires an owner main window".into(),
            ));
        }
        self.inner.create_session(context(&app), config).await
    }
    pub async fn attach_frame_channel(
        &self,
        app: &AppHandle,
        id: &str,
        channel: Channel<InvokeResponseBody>,
    ) -> AppResult<()> {
        self.inner
            .attach_frame_channel(
                &context(app),
                id,
                FrameChannel(Arc::new(move |frame| {
                    channel
                        .send(InvokeResponseBody::Raw(frame))
                        .map_err(|e| AppError::Channel(e.to_string()))
                })),
            )
            .await
    }
    pub async fn detach_frame_channel(&self, id: &str) -> AppResult<()> {
        self.inner.detach_frame_channel(id).await
    }
    pub async fn send_input(&self, id: &str, events: Vec<VncInputEvent>) -> AppResult<()> {
        self.inner.send_input(id, events).await
    }
    pub async fn set_clipboard_text(&self, id: &str, text: String) -> AppResult<()> {
        self.inner.set_clipboard_text(id, text).await
    }
    pub async fn reconnect(self: &Arc<Self>, app: AppHandle, id: &str) -> AppResult<()> {
        self.inner.reconnect(context(&app), id).await
    }
    pub async fn respond_server_key(&self, id: &str, accepted: bool) -> AppResult<()> {
        self.inner.respond_server_key(id, accepted).await
    }
    pub async fn close(&self, app: &AppHandle, id: &str) -> AppResult<()> {
        self.inner.close(&context(app), id).await
    }
    pub async fn close_all(&self, app: &AppHandle) {
        self.inner.close_all(&context(app)).await
    }
}
