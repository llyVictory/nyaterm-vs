use crate::config::{self, ConnectionAuth, ConnectionNetwork, ConnectionType};
use crate::core::network::open_tcp_transport;
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tauri::async_runtime::JoinHandle;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Emitter};
use tokio::sync::{Mutex, mpsc, watch};
use tokio::time::{Duration, timeout};
use vnc::{
    ClientKeyEvent, ClientMouseEvent, PixelFormat, Rect, Screen, VncClient, VncConnector,
    VncEncoding, VncError, VncEvent, VncLimits, VncSecurityPolicy, X11Event,
};
use zeroize::Zeroizing;

use crate::core::remote_desktop::frame::{
    RemoteDesktopFramePatch, RemoteDesktopPixelFormat, encode_frame_patch,
};

const MAX_FRAMEBUFFER_WIDTH: u16 = 7680;
const MAX_FRAMEBUFFER_HEIGHT: u16 = 4320;
const MAX_VNC_CLIPBOARD_BYTES: usize = 1024 * 1024;
const MAX_INPUT_BATCH: usize = 256;
const WORKER_COMMAND_CHANNEL_CAPACITY: usize = 256;
const MAX_PENDING_FRAMES: usize = 2;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);
const WORKER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3);
const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(8);
const UPDATE_REQUEST_INTERVAL: Duration = Duration::from_millis(16);

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VncSessionState {
    Connecting,
    Authenticating,
    Negotiating,
    Active,
    Reconnecting,
    Disconnected,
    Failed,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VncErrorKind {
    Transport,
    Authentication,
    Protocol,
    Encoding,
    Clipboard,
    Internal,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VncStateEvent {
    pub session_id: String,
    pub state: VncSessionState,
    pub message: Option<String>,
    pub error_kind: Option<VncErrorKind>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VncClipboardEvent {
    pub session_id: String,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum VncInputEvent {
    #[serde(rename = "key")]
    Key { keysym: u32, pressed: bool },
    #[serde(rename = "pointer")]
    Pointer {
        x: u16,
        y: u16,
        #[serde(alias = "buttonMask")]
        button_mask: u8,
    },
    #[serde(rename = "release-all-keys")]
    ReleaseAllKeys,
}

#[derive(Debug)]
enum VncWorkerCommand {
    Input(VncInputEvent),
    Clipboard(String),
    FullRefresh,
}

#[derive(Debug, Clone)]
pub struct VncConnectConfig {
    pub session_id: String,
    pub connection_id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub password: Option<String>,
    pub security_mode: String,
    pub scale_mode: String,
    pub clipboard_enabled: bool,
    pub reconnect_enabled: bool,
    pub reconnect_max_attempts: u32,
    pub shared: bool,
    pub view_only: bool,
    pub network: Option<ConnectionNetwork>,
}

struct VncFramebuffer {
    width: u16,
    height: u16,
    rgba: Vec<u8>,
}

impl VncFramebuffer {
    fn new(width: u16, height: u16) -> AppResult<Self> {
        validate_framebuffer_dimensions(width, height)?;
        let len = usize::from(width)
            .checked_mul(usize::from(height))
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| AppError::Config("VNC framebuffer size overflows".to_string()))?;
        Ok(Self {
            width,
            height,
            rgba: vec![0; len],
        })
    }

    fn apply_rgba(&mut self, rect: Rect, pixels: &[u8]) -> AppResult<()> {
        validate_rectangle(rect, self.width, self.height)?;
        let row_bytes = usize::from(rect.width)
            .checked_mul(4)
            .ok_or_else(|| AppError::Config("VNC rectangle row size overflows".to_string()))?;
        let required = row_bytes
            .checked_mul(usize::from(rect.height))
            .ok_or_else(|| AppError::Config("VNC rectangle payload size overflows".to_string()))?;
        if pixels.len() != required {
            return Err(AppError::Config(
                "VNC rectangle payload length is invalid".to_string(),
            ));
        }
        let framebuffer_stride = usize::from(self.width) * 4;
        for row in 0..usize::from(rect.height) {
            let src_start = row * row_bytes;
            let dst_start =
                (usize::from(rect.y) + row) * framebuffer_stride + usize::from(rect.x) * 4;
            self.rgba[dst_start..dst_start + row_bytes]
                .copy_from_slice(&pixels[src_start..src_start + row_bytes]);
        }
        Ok(())
    }

    fn patch_bytes(&self, sequence: u64, rect: Rect, pixels: &[u8]) -> AppResult<Vec<u8>> {
        encode_frame_patch(&RemoteDesktopFramePatch {
            sequence,
            desktop_width: u32::from(self.width),
            desktop_height: u32::from(self.height),
            x: u32::from(rect.x),
            y: u32::from(rect.y),
            width: u32::from(rect.width),
            height: u32::from(rect.height),
            stride: u32::from(rect.width) * 4,
            pixel_format: RemoteDesktopPixelFormat::Rgba8888,
            payload: pixels,
        })
    }

    fn full_frame_bytes(&self, sequence: u64) -> AppResult<Vec<u8>> {
        self.patch_bytes(
            sequence,
            Rect {
                x: 0,
                y: 0,
                width: self.width,
                height: self.height,
            },
            &self.rgba,
        )
    }
}

pub struct VncSession {
    pub config: VncConnectConfig,
    state: Mutex<VncSessionState>,
    message: Mutex<Option<String>>,
    error_kind: Mutex<Option<VncErrorKind>>,
    generation: AtomicU64,
    frame_sequence: AtomicU64,
    frame_attach_id: AtomicU64,
    frame_channel: Mutex<Option<(u64, Channel<InvokeResponseBody>)>>,
    pending_frames: Mutex<VecDeque<Vec<u8>>>,
    framebuffer: Mutex<Option<VncFramebuffer>>,
    command_sender: Mutex<Option<mpsc::Sender<VncWorkerCommand>>>,
    cancel_sender: Mutex<Option<watch::Sender<bool>>>,
    worker: Mutex<Option<JoinHandle<()>>>,
    close_requested: AtomicBool,
}

pub struct VncSessionManager {
    sessions: Mutex<HashMap<String, Arc<VncSession>>>,
}

impl VncSessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }

    pub async fn create_session(
        self: &Arc<Self>,
        app: AppHandle,
        config: VncConnectConfig,
    ) -> AppResult<String> {
        let session_id = config.session_id.clone();
        let session = Arc::new(VncSession {
            config,
            state: Mutex::new(VncSessionState::Connecting),
            message: Mutex::new(None),
            error_kind: Mutex::new(None),
            generation: AtomicU64::new(0),
            frame_sequence: AtomicU64::new(0),
            frame_attach_id: AtomicU64::new(0),
            frame_channel: Mutex::new(None),
            pending_frames: Mutex::new(VecDeque::with_capacity(MAX_PENDING_FRAMES)),
            framebuffer: Mutex::new(None),
            command_sender: Mutex::new(None),
            cancel_sender: Mutex::new(None),
            worker: Mutex::new(None),
            close_requested: AtomicBool::new(false),
        });
        self.sessions
            .lock()
            .await
            .insert(session_id.clone(), session.clone());
        emit_state(&app, &session, VncSessionState::Connecting, None, None);
        tracing::info!(
            session_id = %session_id,
            connection_id = %session.config.connection_id,
            name = %session.config.name,
            scale_mode = %session.config.scale_mode,
            "Created VNC session"
        );
        spawn_session_worker(app.clone(), session).await;
        let _ = app.emit("sessions-changed", ());
        Ok(session_id)
    }

    pub async fn attach_frame_channel(
        &self,
        app: &AppHandle,
        session_id: &str,
        channel: Channel<InvokeResponseBody>,
    ) -> AppResult<()> {
        let session = self.get(session_id).await?;
        let attach_id = session.frame_attach_id.fetch_add(1, Ordering::AcqRel) + 1;
        *session.frame_channel.lock().await = Some((attach_id, channel));
        flush_pending_frames(&session, attach_id).await;
        send_full_frame(&session, Some(attach_id)).await?;
        let _ = send_worker_command(&session, VncWorkerCommand::FullRefresh).await;
        replay_state(app, &session).await;
        Ok(())
    }

    pub async fn send_input(&self, session_id: &str, events: Vec<VncInputEvent>) -> AppResult<()> {
        let session = self.get(session_id).await?;
        if session.config.view_only {
            return Err(AppError::Config(
                "VNC input is disabled for a view-only session".to_string(),
            ));
        }
        if events.len() > MAX_INPUT_BATCH {
            return Err(AppError::Config(format!(
                "VNC input batch exceeds {MAX_INPUT_BATCH} events"
            )));
        }
        let sender = session
            .command_sender
            .lock()
            .await
            .clone()
            .ok_or_else(|| AppError::Channel("VNC worker channel is not active".to_string()))?;
        for event in events {
            sender
                .send(VncWorkerCommand::Input(event))
                .await
                .map_err(|_| AppError::Channel("VNC worker channel closed".to_string()))?;
        }
        Ok(())
    }

    pub async fn set_clipboard_text(&self, session_id: &str, text: String) -> AppResult<()> {
        let session = self.get(session_id).await?;
        if session.config.view_only || !session.config.clipboard_enabled {
            return Err(AppError::Config(
                "VNC clipboard sending is disabled".to_string(),
            ));
        }
        if text.len() > MAX_VNC_CLIPBOARD_BYTES || !text.chars().all(|ch| u32::from(ch) <= 0xff) {
            return Err(AppError::Config(
                "VNC clipboard text must be Latin-1 and no larger than 1 MiB".to_string(),
            ));
        }
        send_worker_command(&session, VncWorkerCommand::Clipboard(text)).await
    }

    pub async fn reconnect(self: &Arc<Self>, app: AppHandle, session_id: &str) -> AppResult<()> {
        let session = self.get(session_id).await?;
        stop_worker(&session).await;
        session.close_requested.store(false, Ordering::Release);
        *session.framebuffer.lock().await = None;
        session.pending_frames.lock().await.clear();
        session.frame_sequence.store(0, Ordering::Release);
        emit_state(&app, &session, VncSessionState::Reconnecting, None, None);
        spawn_session_worker(app, session).await;
        Ok(())
    }

    pub async fn close(&self, app: &AppHandle, session_id: &str) -> AppResult<()> {
        let Some(session) = self.sessions.lock().await.remove(session_id) else {
            return Ok(());
        };
        session.close_requested.store(true, Ordering::Release);
        stop_worker(&session).await;
        session.frame_attach_id.fetch_add(1, Ordering::AcqRel);
        *session.frame_channel.lock().await = None;
        session.pending_frames.lock().await.clear();
        *session.framebuffer.lock().await = None;
        set_state(&session, VncSessionState::Disconnected, None, None).await;
        emit_state(app, &session, VncSessionState::Disconnected, None, None);
        let _ = app.emit("sessions-changed", ());
        Ok(())
    }

    pub async fn close_all(&self, app: &AppHandle) {
        let ids = self
            .sessions
            .lock()
            .await
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for id in ids {
            if let Err(error) = self.close(app, &id).await {
                tracing::warn!(session_id = %id, "Failed to close VNC session: {error}");
            }
        }
    }

    async fn get(&self, session_id: &str) -> AppResult<Arc<VncSession>> {
        self.sessions
            .lock()
            .await
            .get(session_id)
            .cloned()
            .ok_or_else(|| {
                AppError::SessionNotFound(format!("VNC session '{session_id}' not found"))
            })
    }
}

impl Default for VncSessionManager {
    fn default() -> Self {
        Self::new()
    }
}

async fn spawn_session_worker(app: AppHandle, session: Arc<VncSession>) {
    let (cancel_tx, cancel_rx) = watch::channel(false);
    *session.cancel_sender.lock().await = Some(cancel_tx);
    let worker_session = session.clone();
    let handle = tauri::async_runtime::spawn(async move {
        run_session_worker(app, worker_session, cancel_rx).await;
    });
    *session.worker.lock().await = Some(handle);
}

async fn run_session_worker(
    app: AppHandle,
    session: Arc<VncSession>,
    mut cancel_rx: watch::Receiver<bool>,
) {
    let mut attempt = 0_u32;
    loop {
        if session.close_requested.load(Ordering::Acquire) || *cancel_rx.borrow() {
            return;
        }
        let generation = session.generation.fetch_add(1, Ordering::AcqRel) + 1;
        let state = if attempt == 0 {
            VncSessionState::Connecting
        } else {
            VncSessionState::Reconnecting
        };
        set_state(&session, state.clone(), None, None).await;
        emit_state(&app, &session, state, None, None);

        match run_protocol_generation(&app, &session, generation, &mut cancel_rx).await {
            Ok(()) => return,
            Err((kind, message, retryable)) => {
                if session.close_requested.load(Ordering::Acquire) || *cancel_rx.borrow() {
                    return;
                }
                if !retryable
                    || !session.config.reconnect_enabled
                    || attempt >= session.config.reconnect_max_attempts
                {
                    set_state(
                        &session,
                        VncSessionState::Failed,
                        Some(message.clone()),
                        Some(kind),
                    )
                    .await;
                    emit_state(
                        &app,
                        &session,
                        VncSessionState::Failed,
                        Some(message),
                        Some(kind),
                    );
                    let _ = app.emit("sessions-changed", ());
                    return;
                }
                attempt += 1;
                let delay = reconnect_delay(attempt);
                let sleep = tokio::time::sleep(delay);
                tokio::pin!(sleep);
                tokio::select! {
                    _ = &mut sleep => {},
                    changed = cancel_rx.changed() => {
                        if changed.is_err() || *cancel_rx.borrow() { return; }
                    }
                }
            }
        }
    }
}

async fn run_protocol_generation(
    app: &AppHandle,
    session: &Arc<VncSession>,
    generation: u64,
    cancel_rx: &mut watch::Receiver<bool>,
) -> Result<(), (VncErrorKind, String, bool)> {
    let (host, port) = vnc_connect_target(&session.config);
    let transport = tokio::select! {
        result = timeout(CONNECT_TIMEOUT, open_tcp_transport(
            app,
            host,
            port,
            session.config.network.as_ref(),
            None,
        )) => {
            result.map_err(|_| (VncErrorKind::Transport, "VNC connection timed out".to_string(), true))?
                .map_err(|error| (VncErrorKind::Transport, format!("Unable to connect to the VNC server: {error}"), true))?
        }
        changed = cancel_rx.changed() => {
            let _ = changed;
            return Ok(());
        }
    };
    let stream = transport.stream;

    set_state(session, VncSessionState::Authenticating, None, None).await;
    emit_state(app, session, VncSessionState::Authenticating, None, None);
    let password = Zeroizing::new(session.config.password.clone().unwrap_or_default());
    if password.as_bytes().len() > 8 {
        return Err((
            VncErrorKind::Authentication,
            "Classic VNC authentication passwords must be 8 bytes or fewer".to_string(),
            false,
        ));
    }

    let limits = VncLimits {
        max_framebuffer_width: MAX_FRAMEBUFFER_WIDTH,
        max_framebuffer_height: MAX_FRAMEBUFFER_HEIGHT,
        max_framebuffer_pixels: usize::from(MAX_FRAMEBUFFER_WIDTH)
            * usize::from(MAX_FRAMEBUFFER_HEIGHT),
        max_clipboard_bytes: MAX_VNC_CLIPBOARD_BYTES,
        max_rectangles_per_update: 1024,
        max_encoded_payload_bytes: 64 * 1024 * 1024,
        max_decoded_payload_bytes: usize::from(MAX_FRAMEBUFFER_WIDTH)
            * usize::from(MAX_FRAMEBUFFER_HEIGHT)
            * 4,
        channel_capacity: 32,
        ..VncLimits::default()
    };
    let auth_password = password.to_string();
    let state = VncConnector::new(stream)
        .set_auth_method(async move { Ok(auth_password) })
        .set_security_policy(security_policy(
            &session.config.security_mode,
            session.config.password.is_some(),
        ))
        .set_pixel_format(PixelFormat::rgba())
        .set_limits(limits)
        .add_encoding(VncEncoding::DesktopSizePseudo)
        .add_encoding(VncEncoding::Zrle)
        .add_encoding(VncEncoding::Tight)
        .add_encoding(VncEncoding::Raw)
        .allow_shared(session.config.shared)
        .build()
        .map_err(classify_vnc_error)?;
    let client = timeout(HANDSHAKE_TIMEOUT, state.try_start())
        .await
        .map_err(|_| {
            (
                VncErrorKind::Transport,
                "VNC protocol negotiation timed out".to_string(),
                true,
            )
        })?
        .and_then(|state| state.finish())
        .map_err(classify_vnc_error)?;

    set_state(session, VncSessionState::Negotiating, None, None).await;
    emit_state(app, session, VncSessionState::Negotiating, None, None);
    let (command_tx, mut command_rx) = mpsc::channel(WORKER_COMMAND_CHANNEL_CAPACITY);
    *session.command_sender.lock().await = Some(command_tx);
    let mut pressed_keys = Vec::<u32>::new();
    let mut poll = tokio::time::interval(EVENT_POLL_INTERVAL);
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut refresh_due = false;
    let refresh_delay = tokio::time::sleep(Duration::from_secs(86_400));
    tokio::pin!(refresh_delay);

    loop {
        if session.generation.load(Ordering::Acquire) != generation {
            let _ = client.close().await;
            return Ok(());
        }
        tokio::select! {
            changed = cancel_rx.changed() => {
                let _ = changed;
                release_pressed_keys(&client, &mut pressed_keys).await;
                let _ = client.close().await;
                return Ok(());
            }
            _ = poll.tick() => {
                loop {
                    match client.poll_event().await {
                        Ok(Some(event)) => {
                            handle_vnc_event(app, session, generation, event).await?;
                            refresh_due = true;
                            refresh_delay.as_mut().reset(tokio::time::Instant::now() + UPDATE_REQUEST_INTERVAL);
                        }
                        Ok(None) => break,
                        Err(error) => return Err(classify_vnc_error(error)),
                    }
                }
            }
            _ = &mut refresh_delay, if refresh_due => {
                client.input(X11Event::Refresh).await.map_err(classify_vnc_error)?;
                refresh_due = false;
            }
            command = command_rx.recv() => {
                let Some(command) = command else {
                    let _ = client.close().await;
                    return Ok(());
                };
                handle_worker_command(&client, command, &mut pressed_keys).await?;
            }
        }
    }
}

fn vnc_connect_target(config: &VncConnectConfig) -> (&str, u16) {
    (config.host.as_str(), config.port)
}

fn security_policy(mode: &str, has_password: bool) -> VncSecurityPolicy {
    match mode {
        "none" => VncSecurityPolicy::NoneOnly,
        "vnc-auth" => VncSecurityPolicy::VncAuthOnly,
        _ if has_password => VncSecurityPolicy::VncAuthOnly,
        _ => VncSecurityPolicy::NoneOnly,
    }
}

async fn handle_vnc_event(
    app: &AppHandle,
    session: &Arc<VncSession>,
    generation: u64,
    event: VncEvent,
) -> Result<(), (VncErrorKind, String, bool)> {
    if session.generation.load(Ordering::Acquire) != generation {
        return Ok(());
    }
    match event {
        VncEvent::SetResolution(Screen { width, height }) => {
            let framebuffer = VncFramebuffer::new(width, height)
                .map_err(|error| (VncErrorKind::Protocol, error.to_string(), false))?;
            *session.framebuffer.lock().await = Some(framebuffer);
        }
        VncEvent::RawImage(rect, pixels) => {
            let mut framebuffer_guard = session.framebuffer.lock().await;
            if framebuffer_guard.is_none() {
                let width = rect.x.checked_add(rect.width).ok_or_else(|| {
                    (
                        VncErrorKind::Protocol,
                        "VNC rectangle bounds overflow".to_string(),
                        false,
                    )
                })?;
                let height = rect.y.checked_add(rect.height).ok_or_else(|| {
                    (
                        VncErrorKind::Protocol,
                        "VNC rectangle bounds overflow".to_string(),
                        false,
                    )
                })?;
                *framebuffer_guard = Some(
                    VncFramebuffer::new(width, height)
                        .map_err(|error| (VncErrorKind::Protocol, error.to_string(), false))?,
                );
            }
            let framebuffer = framebuffer_guard.as_mut().expect("framebuffer initialized");
            framebuffer
                .apply_rgba(rect, &pixels)
                .map_err(|error| (VncErrorKind::Protocol, error.to_string(), false))?;
            let sequence = session.frame_sequence.fetch_add(1, Ordering::AcqRel);
            let frame = framebuffer
                .patch_bytes(sequence, rect, &pixels)
                .map_err(|error| (VncErrorKind::Internal, error.to_string(), false))?;
            drop(framebuffer_guard);
            queue_or_send_frame(session, frame).await;
            let was_active = matches!(*session.state.lock().await, VncSessionState::Active);
            if !was_active {
                set_state(session, VncSessionState::Active, None, None).await;
                emit_state(app, session, VncSessionState::Active, None, None);
                let _ = app.emit("sessions-changed", ());
            }
        }
        VncEvent::Text(text) => {
            if session.config.clipboard_enabled && text.len() <= MAX_VNC_CLIPBOARD_BYTES {
                let _ = app.emit(
                    format!("vnc-clipboard-{}", session.config.session_id).as_str(),
                    VncClipboardEvent {
                        session_id: session.config.session_id.clone(),
                        text,
                    },
                );
            }
        }
        VncEvent::Error(message) => {
            return Err(classify_vnc_event_error(message));
        }
        VncEvent::JpegImage(_, _) => {
            return Err((
                VncErrorKind::Encoding,
                "The server sent a Tight JPEG event instead of decoded RGBA pixels".to_string(),
                false,
            ));
        }
        VncEvent::Copy(_, _) | VncEvent::SetCursor(_, _) => {
            return Err((
                VncErrorKind::Encoding,
                "The server sent an unrequested VNC encoding".to_string(),
                false,
            ));
        }
        VncEvent::SetPixelFormat(_) | VncEvent::Bell => {}
        _ => {}
    }
    Ok(())
}

fn classify_vnc_event_error(message: String) -> (VncErrorKind, String, bool) {
    let lower = message.to_ascii_lowercase();
    let kind = if lower.contains("image")
        || lower.contains("encoding")
        || lower.contains("zrle")
        || lower.contains("tight")
        || lower.contains("jpeg")
    {
        VncErrorKind::Encoding
    } else {
        VncErrorKind::Protocol
    };
    (kind, message, false)
}

async fn handle_worker_command(
    client: &VncClient,
    command: VncWorkerCommand,
    pressed_keys: &mut Vec<u32>,
) -> Result<(), (VncErrorKind, String, bool)> {
    match command {
        VncWorkerCommand::Input(input) => send_vnc_input(client, input, pressed_keys).await,
        VncWorkerCommand::Clipboard(text) => {
            client
                .input(X11Event::CopyText(text))
                .await
                .map_err(|error| {
                    let (_, message, retryable) = classify_vnc_error(error);
                    (VncErrorKind::Clipboard, message, retryable)
                })
        }
        VncWorkerCommand::FullRefresh => client
            .input(X11Event::FullRefresh)
            .await
            .map_err(classify_vnc_error),
    }
}

async fn send_vnc_input(
    client: &VncClient,
    input: VncInputEvent,
    pressed_keys: &mut Vec<u32>,
) -> Result<(), (VncErrorKind, String, bool)> {
    match input {
        VncInputEvent::Key { keysym, pressed } => {
            if pressed {
                if !pressed_keys.contains(&keysym) {
                    pressed_keys.push(keysym);
                }
            } else {
                pressed_keys.retain(|key| *key != keysym);
            }
            client
                .input(X11Event::KeyEvent(ClientKeyEvent {
                    keycode: keysym,
                    down: pressed,
                }))
                .await
                .map_err(classify_vnc_error)?;
        }
        VncInputEvent::Pointer { x, y, button_mask } => {
            client
                .input(X11Event::PointerEvent(ClientMouseEvent {
                    position_x: x,
                    position_y: y,
                    bottons: button_mask,
                }))
                .await
                .map_err(classify_vnc_error)?;
        }
        VncInputEvent::ReleaseAllKeys => release_pressed_keys(client, pressed_keys).await,
    }
    Ok(())
}

async fn release_pressed_keys(client: &VncClient, pressed_keys: &mut Vec<u32>) {
    for keysym in pressed_keys.drain(..) {
        let _ = client
            .input(X11Event::KeyEvent(ClientKeyEvent {
                keycode: keysym,
                down: false,
            }))
            .await;
    }
}

async fn send_worker_command(
    session: &Arc<VncSession>,
    command: VncWorkerCommand,
) -> AppResult<()> {
    let sender = session
        .command_sender
        .lock()
        .await
        .clone()
        .ok_or_else(|| AppError::Channel("VNC worker channel is not active".to_string()))?;
    sender
        .send(command)
        .await
        .map_err(|_| AppError::Channel("VNC worker channel closed".to_string()))
}

async fn stop_worker(session: &Arc<VncSession>) {
    if let Some(cancel) = session.cancel_sender.lock().await.take() {
        let _ = cancel.send(true);
    }
    *session.command_sender.lock().await = None;
    if let Some(mut worker) = session.worker.lock().await.take() {
        if timeout(WORKER_SHUTDOWN_TIMEOUT, &mut worker).await.is_err() {
            worker.abort();
            let _ = worker.await;
        }
    }
}

async fn queue_or_send_frame(session: &Arc<VncSession>, frame: Vec<u8>) {
    let attach_id = session.frame_attach_id.load(Ordering::Acquire);
    let sent = {
        let mut channel = session.frame_channel.lock().await;
        match channel.as_ref() {
            Some((current_attach_id, sender)) if *current_attach_id == attach_id => {
                if sender.send(InvokeResponseBody::Raw(frame.clone())).is_ok() {
                    true
                } else {
                    if channel.as_ref().is_some_and(|(id, _)| *id == attach_id) {
                        *channel = None;
                    }
                    false
                }
            }
            _ => false,
        }
    };
    if sent {
        return;
    }

    let mut pending = session.pending_frames.lock().await;
    while pending.len() >= MAX_PENDING_FRAMES {
        pending.pop_front();
    }
    pending.push_back(frame);
}

async fn flush_pending_frames(session: &Arc<VncSession>, attach_id: u64) {
    let channel = session.frame_channel.lock().await;
    let Some((current_attach_id, channel)) = channel.as_ref() else {
        return;
    };
    if *current_attach_id != attach_id {
        return;
    }
    let mut pending = session.pending_frames.lock().await;
    while let Some(frame) = pending.pop_front() {
        if channel.send(InvokeResponseBody::Raw(frame)).is_err() {
            break;
        }
    }
}

async fn send_full_frame(session: &Arc<VncSession>, attach_id: Option<u64>) -> AppResult<()> {
    let frame = {
        let framebuffer = session.framebuffer.lock().await;
        let Some(framebuffer) = framebuffer.as_ref() else {
            return Ok(());
        };
        let sequence = session.frame_sequence.fetch_add(1, Ordering::AcqRel);
        framebuffer.full_frame_bytes(sequence)?
    };
    if let Some(expected_attach_id) = attach_id {
        let mut channel = session.frame_channel.lock().await;
        let failed = channel
            .as_ref()
            .filter(|(current_attach_id, _)| *current_attach_id == expected_attach_id)
            .is_some_and(|(_, channel)| {
                channel
                    .send(InvokeResponseBody::Raw(frame.clone()))
                    .is_err()
            });
        if failed
            && channel
                .as_ref()
                .is_some_and(|(current_attach_id, _)| *current_attach_id == expected_attach_id)
        {
            *channel = None;
        }
    } else {
        queue_or_send_frame(session, frame).await;
    }
    Ok(())
}

async fn set_state(
    session: &VncSession,
    state: VncSessionState,
    message: Option<String>,
    error_kind: Option<VncErrorKind>,
) {
    *session.state.lock().await = state;
    *session.message.lock().await = message;
    *session.error_kind.lock().await = error_kind;
}

fn emit_state(
    app: &AppHandle,
    session: &VncSession,
    state: VncSessionState,
    message: Option<String>,
    error_kind: Option<VncErrorKind>,
) {
    let _ = app.emit(
        format!("vnc-state-{}", session.config.session_id).as_str(),
        VncStateEvent {
            session_id: session.config.session_id.clone(),
            state,
            message,
            error_kind,
        },
    );
}

async fn replay_state(app: &AppHandle, session: &VncSession) {
    emit_state(
        app,
        session,
        session.state.lock().await.clone(),
        session.message.lock().await.clone(),
        *session.error_kind.lock().await,
    );
}

fn classify_vnc_error(error: VncError) -> (VncErrorKind, String, bool) {
    match error {
        VncError::NoPassword | VncError::WrongPassword => (
            VncErrorKind::Authentication,
            "VNC authentication failed".to_string(),
            false,
        ),
        VncError::UnsupportedSecurityType
        | VncError::RequiredSecurityTypeUnavailable(_)
        | VncError::InvalidSecurityType(_) => (
            VncErrorKind::Authentication,
            format!(
                "The VNC server requires an unsupported security type. Currently supported: None and VNC Authentication. Details: {error}"
            ),
            false,
        ),
        VncError::InvalidEncoding(_) | VncError::InvalidImageData => {
            (VncErrorKind::Encoding, error.to_string(), false)
        }
        VncError::IoError(_) => (VncErrorKind::Transport, error.to_string(), true),
        VncError::LimitExceeded { .. }
        | VncError::InvalidDimensions
        | VncError::IntegerOverflow(_)
        | VncError::WrongPixelFormat
        | VncError::WrongServerMessage
        | VncError::InvalidSecurityResult(_)
        | VncError::SecurityFailure(_) => (VncErrorKind::Protocol, error.to_string(), false),
        _ => (VncErrorKind::Internal, error.to_string(), true),
    }
}

fn reconnect_delay(attempt: u32) -> Duration {
    Duration::from_secs(match attempt {
        0 | 1 => 1,
        2 => 2,
        3 => 4,
        4 => 8,
        5 => 15,
        _ => 30,
    })
}

fn validate_framebuffer_dimensions(width: u16, height: u16) -> AppResult<()> {
    if width == 0 || height == 0 || width > MAX_FRAMEBUFFER_WIDTH || height > MAX_FRAMEBUFFER_HEIGHT
    {
        return Err(AppError::Config(format!(
            "VNC framebuffer {width}x{height} is outside the supported range"
        )));
    }
    Ok(())
}

fn validate_rectangle(rect: Rect, desktop_width: u16, desktop_height: u16) -> AppResult<()> {
    let right = rect
        .x
        .checked_add(rect.width)
        .ok_or_else(|| AppError::Config("VNC rectangle horizontal bounds overflow".to_string()))?;
    let bottom = rect
        .y
        .checked_add(rect.height)
        .ok_or_else(|| AppError::Config("VNC rectangle vertical bounds overflow".to_string()))?;
    if rect.width == 0 || rect.height == 0 || right > desktop_width || bottom > desktop_height {
        return Err(AppError::Config(
            "VNC rectangle exceeds framebuffer bounds".to_string(),
        ));
    }
    Ok(())
}

pub fn load_saved_vnc_config(app: &AppHandle, connection_id: &str) -> AppResult<VncConnectConfig> {
    let connection = config::load_connection_by_id(app, connection_id)?;
    let network = connection.network.clone();
    let password = resolve_vnc_password(app, connection.auth.as_ref())?;
    let ConnectionType::Vnc {
        host,
        port,
        security,
        display,
        clipboard,
        reconnect,
        shared,
        view_only,
    } = connection.config
    else {
        return Err(AppError::Config(
            "Connection is not a VNC connection".to_string(),
        ));
    };
    if matches!(security.mode.as_str(), "vnc-auth") && password.is_none() {
        return Err(AppError::Config(
            "VNC Authentication requires a password".to_string(),
        ));
    }
    if let Some(password) = password.as_ref()
        && password.as_bytes().len() > 8
    {
        return Err(AppError::Config(
            "Classic VNC authentication passwords must be 8 bytes or fewer".to_string(),
        ));
    }

    Ok(VncConnectConfig {
        session_id: uuid::Uuid::new_v4().to_string(),
        connection_id: connection_id.to_string(),
        name: connection.name,
        host,
        port,
        password,
        security_mode: security.mode,
        scale_mode: display.scale_mode,
        clipboard_enabled: clipboard.enabled,
        reconnect_enabled: reconnect.enabled,
        reconnect_max_attempts: reconnect.max_attempts,
        shared,
        view_only,
        network,
    })
}

fn resolve_vnc_password(
    app: &AppHandle,
    auth: Option<&ConnectionAuth>,
) -> AppResult<Option<String>> {
    let Some(auth) = auth else {
        return Ok(None);
    };
    if auth.mode != "password" {
        return Ok(None);
    }
    if let Some(password_id) = auth.password_id.as_deref().filter(|id| !id.is_empty()) {
        let account = config::load_saved_account(app, None, Some(password_id))?;
        return config::decrypt_account_password(account.as_ref());
    }
    crate::utils::crypto::decrypt_optional(&auth.password)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framebuffer_rejects_oversized_and_out_of_bounds_data() {
        assert!(VncFramebuffer::new(7681, 1).is_err());
        let mut framebuffer = VncFramebuffer::new(10, 10).expect("framebuffer");
        assert!(
            framebuffer
                .apply_rgba(
                    Rect {
                        x: 10,
                        y: 0,
                        width: 1,
                        height: 1,
                    },
                    &[0; 4]
                )
                .is_err()
        );
    }

    #[test]
    fn framebuffer_applies_patches_and_builds_full_refreshes() {
        let mut framebuffer = VncFramebuffer::new(2, 2).expect("framebuffer");
        framebuffer
            .apply_rgba(
                Rect {
                    x: 1,
                    y: 1,
                    width: 1,
                    height: 1,
                },
                &[1, 2, 3, 255],
            )
            .expect("patch");
        let frame = framebuffer.full_frame_bytes(7).expect("full frame");
        assert_eq!(&frame[0..8], &7_u64.to_le_bytes());
        assert_eq!(&frame[40..44], &16_u32.to_le_bytes());
        assert_eq!(&frame[56..60], &[1, 2, 3, 255]);
    }

    #[test]
    fn reconnect_delay_is_bounded() {
        assert_eq!(reconnect_delay(1), Duration::from_secs(1));
        assert_eq!(reconnect_delay(100), Duration::from_secs(30));
    }

    #[test]
    fn security_mode_maps_to_fail_closed_policy() {
        assert_eq!(security_policy("none", true), VncSecurityPolicy::NoneOnly);
        assert_eq!(
            security_policy("vnc-auth", false),
            VncSecurityPolicy::VncAuthOnly
        );
        assert_eq!(
            security_policy("auto", true),
            VncSecurityPolicy::VncAuthOnly
        );
        assert_eq!(security_policy("auto", false), VncSecurityPolicy::NoneOnly);
    }

    #[test]
    fn connect_target_preserves_ipv6_literals_without_host_port_concatenation() {
        let config = VncConnectConfig {
            session_id: "vnc-test".to_string(),
            connection_id: "connection-test".to_string(),
            name: "IPv6 VNC".to_string(),
            host: "::1".to_string(),
            port: 5900,
            password: None,
            security_mode: "none".to_string(),
            scale_mode: "fit".to_string(),
            clipboard_enabled: true,
            reconnect_enabled: false,
            reconnect_max_attempts: 0,
            shared: true,
            view_only: false,
            network: None,
        };

        assert_eq!(vnc_connect_target(&config), ("::1", 5900));
    }

    #[tokio::test]
    async fn pending_frame_queue_keeps_latest_frames_under_pressure() {
        let session = Arc::new(VncSession {
            config: VncConnectConfig {
                session_id: "vnc-test".to_string(),
                connection_id: "connection-test".to_string(),
                name: "Test VNC".to_string(),
                host: "127.0.0.1".to_string(),
                port: 5900,
                password: None,
                security_mode: "none".to_string(),
                scale_mode: "fit".to_string(),
                clipboard_enabled: true,
                reconnect_enabled: false,
                reconnect_max_attempts: 0,
                shared: true,
                view_only: false,
                network: None,
            },
            state: Mutex::new(VncSessionState::Connecting),
            message: Mutex::new(None),
            error_kind: Mutex::new(None),
            generation: AtomicU64::new(0),
            frame_sequence: AtomicU64::new(0),
            frame_attach_id: AtomicU64::new(0),
            frame_channel: Mutex::new(None),
            pending_frames: Mutex::new(VecDeque::with_capacity(MAX_PENDING_FRAMES)),
            framebuffer: Mutex::new(None),
            command_sender: Mutex::new(None),
            cancel_sender: Mutex::new(None),
            worker: Mutex::new(None),
            close_requested: AtomicBool::new(false),
        });

        queue_or_send_frame(&session, vec![1]).await;
        queue_or_send_frame(&session, vec![2]).await;
        queue_or_send_frame(&session, vec![3]).await;

        let pending = session.pending_frames.lock().await;
        assert_eq!(pending.len(), MAX_PENDING_FRAMES);
        assert_eq!(pending[0], vec![2]);
        assert_eq!(pending[1], vec![3]);
    }
}
