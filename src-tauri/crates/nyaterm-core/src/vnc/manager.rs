use super::runtime::{FrameChannel, VncContext as AppHandle, open_tcp_transport};
use crate::config::{self, ConnectionAuth, ConnectionNetwork, ConnectionType};
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::sync::{Mutex, mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tokio::time::{Duration, timeout};
use vnc::{
    ClientKeyEvent, ClientMouseEvent, PixelFormat, Rect, Screen, VncClient, VncConnector,
    VncEncoding, VncError, VncEvent, VncLimits, VncSecurityPolicy, VncServerKey, X11Event,
};
use zeroize::Zeroizing;

use crate::remote_desktop_frame::{
    RemoteDesktopFramePatch, RemoteDesktopPixelFormat, encode_frame_patch,
};

const MAX_FRAMEBUFFER_WIDTH: u16 = 7680;
const MAX_FRAMEBUFFER_HEIGHT: u16 = 4320;
const MAX_VNC_CLIPBOARD_BYTES: usize = 1024 * 1024;
const MAX_INPUT_BATCH: usize = 256;
const WORKER_COMMAND_CHANNEL_CAPACITY: usize = 256;
const MAX_PENDING_FRAMES: usize = 2;
// A routed connection can require interactive SSH jump authentication.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(120);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);
const RA2_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(75);
const SERVER_KEY_PROMPT_TIMEOUT: Duration = Duration::from_mins(1);
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
pub struct VncServerKeyVerifyEvent {
    pub request_id: String,
    pub session_id: String,
    pub host: String,
    pub port: u16,
    pub fingerprint: String,
    pub key_bits: u32,
    pub known_host_status: String,
    pub target_window_label: String,
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

struct VncServerKeyPending {
    payload: VncServerKeyVerifyEvent,
    session_id: String,
    generation: u64,
    responder: oneshot::Sender<bool>,
}

#[derive(Debug, Clone)]
pub struct VncConnectConfig {
    pub owner_window_label: String,
    pub session_id: String,
    pub connection_id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
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
    frame_channel: Mutex<Option<(u64, FrameChannel)>>,
    pending_frames: Mutex<VecDeque<Vec<u8>>>,
    framebuffer: Mutex<Option<VncFramebuffer>>,
    command_sender: Mutex<Option<mpsc::Sender<VncWorkerCommand>>>,
    cancel_sender: Mutex<Option<watch::Sender<bool>>>,
    worker: Mutex<Option<JoinHandle<()>>>,
    close_requested: AtomicBool,
    trust_commit_guard: std::sync::Mutex<()>,
    pending_server_keys: Arc<Mutex<HashMap<String, VncServerKeyPending>>>,
}

pub struct VncSessionManager {
    sessions: Mutex<HashMap<String, Arc<VncSession>>>,
    pending_server_keys: Arc<Mutex<HashMap<String, VncServerKeyPending>>>,
}

impl VncSessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            pending_server_keys: Arc::new(Mutex::new(HashMap::new())),
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
            trust_commit_guard: std::sync::Mutex::new(()),
            pending_server_keys: self.pending_server_keys.clone(),
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
        channel: FrameChannel,
    ) -> AppResult<()> {
        let session = self.get(session_id).await?;
        let attach_id = session.frame_attach_id.fetch_add(1, Ordering::AcqRel) + 1;
        *session.frame_channel.lock().await = Some((attach_id, channel));
        // The framebuffer is authoritative; replaying partial history before it
        // fills bounded Web queues and can perpetuate refresh overflows.
        if session.framebuffer.lock().await.is_some() {
            session.pending_frames.lock().await.clear();
        } else {
            flush_pending_frames(&session, attach_id).await;
        }
        send_full_frame(&session, Some(attach_id)).await?;
        let _ = send_worker_command(&session, VncWorkerCommand::FullRefresh).await;
        replay_state(app, &session).await;
        for pending in session
            .pending_server_keys
            .lock()
            .await
            .values()
            .filter(|p| {
                p.session_id == session_id
                    && p.generation == session.generation.load(Ordering::Acquire)
            })
        {
            let _ = app.emit_to(
                &session.config.owner_window_label,
                "vnc-server-key-verify",
                pending.payload.clone(),
            );
        }
        Ok(())
    }

    pub async fn detach_frame_channel(&self, session_id: &str) -> AppResult<()> {
        let session = self.get(session_id).await?;
        session.frame_attach_id.fetch_add(1, Ordering::AcqRel);
        *session.frame_channel.lock().await = None;
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
        invalidate_vnc_generation(&session, false);
        stop_worker(&session).await;
        self.cancel_pending_server_keys_for_session(app.clone(), session_id)
            .await;
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
        invalidate_vnc_generation(&session, true);
        stop_worker(&session).await;
        self.cancel_pending_server_keys_for_session(app.clone(), session_id)
            .await;
        session.frame_attach_id.fetch_add(1, Ordering::AcqRel);
        *session.frame_channel.lock().await = None;
        session.pending_frames.lock().await.clear();
        *session.framebuffer.lock().await = None;
        set_state(&session, VncSessionState::Disconnected, None, None).await;
        emit_state(app, &session, VncSessionState::Disconnected, None, None);
        let _ = app.emit("sessions-changed", ());
        Ok(())
    }

    pub async fn respond_server_key(&self, request_id: &str, accepted: bool) -> AppResult<()> {
        let Some(pending) = self.pending_server_keys.lock().await.remove(request_id) else {
            return Err(AppError::Cancelled(format!(
                "No pending VNC server key request with id '{request_id}'"
            )));
        };
        let session = self.get(&pending.session_id).await?;
        if session.generation.load(Ordering::Acquire) != pending.generation
            || session.close_requested.load(Ordering::Acquire)
        {
            return Ok(());
        }
        let _ = pending.responder.send(accepted);
        Ok(())
    }

    async fn cancel_pending_server_keys_for_session(&self, app: AppHandle, session_id: &str) {
        let mut pending = self.pending_server_keys.lock().await;
        let request_ids = pending
            .iter()
            .filter(|(_, request)| request.session_id == session_id)
            .map(|(request_id, _)| request_id.clone())
            .collect::<Vec<_>>();
        for request_id in request_ids {
            if let Some(request) = pending.remove(&request_id) {
                let _ = request.responder.send(false);
                let _ = app.emit(
                    "vnc-server-key-verify-resolved",
                    serde_json::json!({"requestId": request_id}),
                );
            }
        }
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
    let handle = tokio::spawn(async move {
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
            Some(session.config.owner_window_label.clone()),
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
    if session.config.security_mode == "vnc-auth" && password.len() > 8 {
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
    let has_credentials = session.config.password.is_some();
    let handshake_timeout = if session.config.security_mode == "auto" && has_credentials {
        RA2_HANDSHAKE_TIMEOUT
    } else {
        HANDSHAKE_TIMEOUT
    };
    let trust_candidate = Arc::new(std::sync::Mutex::new(None::<String>));
    let verifier_trust_candidate = trust_candidate.clone();
    let authenticated_key = Arc::new(std::sync::Mutex::new(None::<String>));
    let notified_key = authenticated_key.clone();
    let verifier_app = app.clone();
    let verifier_session = session.clone();
    let state = VncConnector::new(stream)
        .set_auth_method(async move { Ok(auth_password) })
        .set_credentials_available(has_credentials)
        .set_username(session.config.username.clone())
        .set_server_key_verifier(move |server_key| {
            let app = verifier_app.clone();
            let session = verifier_session.clone();
            let trust_candidate = verifier_trust_candidate.clone();
            async move {
                if let Some(fingerprint) =
                    verify_vnc_server_key(&app, &session, generation, &server_key).await?
                {
                    *trust_candidate.lock().expect("VNC trust candidate lock") = Some(fingerprint);
                }
                Ok(())
            }
        })
        .set_server_key_authenticated(move |key| {
            *notified_key.lock().expect("VNC authenticated key lock") =
                Some(vnc_key_fingerprint(&key));
        })
        .set_security_policy(security_policy(&session.config.security_mode))
        .set_pixel_format(PixelFormat::rgba())
        .set_limits(limits)
        .add_encoding(VncEncoding::DesktopSizePseudo)
        .add_encoding(VncEncoding::Zrle)
        .add_encoding(VncEncoding::Tight)
        .add_encoding(VncEncoding::Raw)
        .allow_shared(session.config.shared)
        .build()
        .map_err(classify_vnc_error)?;
    let handshake_result = tokio::select! {
        biased;
        changed = cancel_rx.changed() => { let _ = changed; None },
        result = timeout(handshake_timeout, state.try_start()) => { Some(result) },
    };
    // Also clean up a prompt if the outer handshake timeout dropped its future.
    resolve_vnc_prompts(app, session, generation).await;
    let Some(handshake_result) = handshake_result else {
        return Ok(());
    };
    let client = handshake_result
        .map_err(|_| {
            (
                VncErrorKind::Transport,
                "VNC protocol negotiation timed out".to_string(),
                true,
            )
        })?
        .and_then(|state| state.finish())
        .map_err(classify_vnc_error)?;
    {
        // Serialize the trust write with close/reconnect invalidation. No await
        // separates the final generation/cancellation check from the storage write.
        let _guard = session
            .trust_commit_guard
            .lock()
            .expect("VNC trust commit lock");
        let trust_candidate = trust_candidate.lock().expect("VNC trust candidate lock");
        let authenticated_key = authenticated_key
            .lock()
            .expect("VNC authenticated key lock");
        commit_vnc_trust(
            trust_candidate.as_deref(),
            authenticated_key.as_deref(),
            generation,
            session.generation.load(Ordering::Acquire),
            session.close_requested.load(Ordering::Acquire) || *cancel_rx.borrow(),
            |fingerprint| {
                crate::storage::upsert_vnc_known_host(
                    &session.config.host,
                    session.config.port,
                    fingerprint,
                )
            },
        )
        .map_err(|error| (VncErrorKind::Internal, error.to_string(), false))?;
    }

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

fn security_policy(mode: &str) -> VncSecurityPolicy {
    match mode {
        "none" => VncSecurityPolicy::NoneOnly,
        "vnc-auth" => VncSecurityPolicy::VncAuthOnly,
        _ => VncSecurityPolicy::Auto,
    }
}

fn vnc_key_fingerprint(key: &VncServerKey) -> String {
    format!("SHA256:{}", hex::encode(Sha256::digest(key.encoded())))
}

fn invalidate_vnc_generation(session: &VncSession, closing: bool) {
    let _guard = session
        .trust_commit_guard
        .lock()
        .expect("VNC trust commit lock");
    session.generation.fetch_add(1, Ordering::AcqRel);
    if closing {
        session.close_requested.store(true, Ordering::Release);
    }
}

fn commit_vnc_trust(
    trust_candidate: Option<&str>,
    authenticated_key: Option<&str>,
    generation: u64,
    current_generation: u64,
    cancelled: bool,
    save: impl FnOnce(&str) -> AppResult<()>,
) -> AppResult<()> {
    if generation == current_generation
        && !cancelled
        && let (Some(trust_candidate), Some(authenticated_key)) =
            (trust_candidate, authenticated_key)
        && trust_candidate == authenticated_key
    {
        save(trust_candidate)?;
    }
    Ok(())
}

async fn resolve_vnc_prompts(app: &AppHandle, session: &VncSession, generation: u64) {
    let mut pending = session.pending_server_keys.lock().await;
    let ids: Vec<_> = pending
        .iter()
        .filter(|(_, request)| {
            request.session_id == session.config.session_id && request.generation == generation
        })
        .map(|(id, _)| id.clone())
        .collect();
    for id in ids {
        pending.remove(&id);
        let _ = app.emit(
            "vnc-server-key-verify-resolved",
            serde_json::json!({"requestId": id}),
        );
    }
}

async fn verify_vnc_server_key(
    app: &AppHandle,
    session: &Arc<VncSession>,
    generation: u64,
    server_key: &VncServerKey,
) -> Result<Option<String>, VncError> {
    if session.generation.load(Ordering::Acquire) != generation
        || session.close_requested.load(Ordering::Acquire)
    {
        return Err(VncError::Ra2ServerKeyRejected(
            "stale VNC connection generation".to_string(),
        ));
    }

    let fingerprint = vnc_key_fingerprint(server_key);
    let known_host_status = crate::storage::check_vnc_known_host(
        &session.config.host,
        session.config.port,
        &fingerprint,
    )
    .map_err(|error| VncError::Ra2ServerKeyRejected(error.to_string()))?;
    let confirmed_new_trust = confirm_vnc_trust(
        known_host_status,
        prompt_vnc_server_key(
            app,
            session,
            generation,
            server_key,
            fingerprint.clone(),
            known_host_status,
        ),
    )
    .await?;
    Ok(confirmed_new_trust.then_some(fingerprint))
}

async fn confirm_vnc_trust(
    known_host_status: crate::storage::KnownHostCheck,
    prompt: impl std::future::Future<Output = Result<(), VncError>>,
) -> Result<bool, VncError> {
    if known_host_status == crate::storage::KnownHostCheck::Match {
        return Ok(false);
    }
    prompt.await?;
    Ok(true)
}

async fn prompt_vnc_server_key(
    app: &AppHandle,
    session: &Arc<VncSession>,
    generation: u64,
    server_key: &VncServerKey,
    fingerprint: String,
    known_host_status: crate::storage::KnownHostCheck,
) -> Result<(), VncError> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = oneshot::channel();
    let payload = VncServerKeyVerifyEvent {
        request_id: request_id.clone(),
        session_id: session.config.session_id.clone(),
        host: session.config.host.clone(),
        port: session.config.port,
        fingerprint,
        key_bits: server_key.bits(),
        target_window_label: session.config.owner_window_label.clone(),
        known_host_status: match known_host_status {
            crate::storage::KnownHostCheck::Match => "match",
            crate::storage::KnownHostCheck::HostSeen => "changed",
            crate::storage::KnownHostCheck::UnknownHost => "unknown",
        }
        .to_string(),
    };
    session.pending_server_keys.lock().await.insert(
        request_id.clone(),
        VncServerKeyPending {
            payload: payload.clone(),
            session_id: session.config.session_id.clone(),
            generation,
            responder: tx,
        },
    );
    app.emit_to(
        &session.config.owner_window_label,
        "vnc-server-key-verify",
        payload,
    )
    .map_err(|error| VncError::Ra2ServerKeyRejected(error.to_string()))?;

    let response = timeout(SERVER_KEY_PROMPT_TIMEOUT, rx).await;
    session.pending_server_keys.lock().await.remove(&request_id);
    let _ = app.emit(
        "vnc-server-key-verify-resolved",
        serde_json::json!({"requestId": request_id}),
    );
    let Ok(Ok(accepted)) = response else {
        return Err(VncError::Ra2ServerKeyRejected(
            "verification timed out".to_string(),
        ));
    };
    if session.generation.load(Ordering::Acquire) != generation
        || session.close_requested.load(Ordering::Acquire)
    {
        return Err(VncError::Ra2ServerKeyRejected(
            "stale VNC connection generation".to_string(),
        ));
    }
    if !accepted {
        return Err(VncError::Ra2ServerKeyRejected(
            "user rejected the server key".to_string(),
        ));
    }
    Ok(())
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
            queue_or_send_frame(session, frame).await;
            drop(framebuffer_guard);
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
                if sender.send(frame.clone()).is_ok() {
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
        if channel.send(frame).is_err() {
            break;
        }
    }
}

async fn send_full_frame(session: &Arc<VncSession>, attach_id: Option<u64>) -> AppResult<()> {
    // Keep the snapshot and dispatch ordered with incremental updates.
    let framebuffer_guard = session.framebuffer.lock().await;
    let frame = {
        let Some(framebuffer) = framebuffer_guard.as_ref() else {
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
            .is_some_and(|(_, channel)| channel.send(frame.clone()).is_err());
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
        VncError::CredentialTooLong { .. } | VncError::Ra2ServerKeyRejected(_) => {
            (VncErrorKind::Authentication, error.to_string(), false)
        }
        VncError::UnsupportedSecurityType
        | VncError::RequiredSecurityTypeUnavailable(_)
        | VncError::InvalidSecurityType(_) => (
            VncErrorKind::Authentication,
            format!(
                "The VNC server requires an unsupported security type. Currently supported: None, VNC Authentication, and RA2_256 in Auto mode. Details: {error}"
            ),
            false,
        ),
        VncError::InvalidEncoding(_) | VncError::InvalidImageData => {
            (VncErrorKind::Encoding, error.to_string(), false)
        }
        VncError::IoError(ref source) if source.kind() == std::io::ErrorKind::InvalidData => {
            (VncErrorKind::Protocol, error.to_string(), false)
        }
        VncError::IoError(_) => (VncErrorKind::Transport, error.to_string(), true),
        VncError::LimitExceeded { .. }
        | VncError::InvalidDimensions
        | VncError::IntegerOverflow(_)
        | VncError::WrongPixelFormat
        | VncError::WrongServerMessage
        | VncError::InvalidSecurityResult(_)
        | VncError::SecurityFailure(_)
        | VncError::InvalidRa2KeyLength { .. }
        | VncError::InvalidRa2PublicKey
        | VncError::InvalidRa2EncryptedRandomLength { .. }
        | VncError::InvalidRa2RandomLength(_)
        | VncError::Ra2ServerHashMismatch
        | VncError::InvalidRa2Subtype(_)
        | VncError::Ra2ServerKeyVerifierRequired
        | VncError::InvalidRa2RecordLimit(_)
        | VncError::Ra2Crypto(_) => (VncErrorKind::Protocol, error.to_string(), false),
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

pub fn load_saved_vnc_config(
    app: &AppHandle,
    connection_id: &str,
    owner_window_label: String,
) -> AppResult<VncConnectConfig> {
    let connection = config::load_connection_by_id(app, connection_id)?;
    config_from_connection(app, connection, owner_window_label)
}
pub fn config_from_connection(
    app: &impl Sized,
    connection: config::SavedConnection,
    owner_window_label: String,
) -> AppResult<VncConnectConfig> {
    let connection_id = connection.id.clone();
    let network = connection.network.clone();
    let password = resolve_vnc_password(app, connection.auth.as_ref())?;
    let ConnectionType::Vnc {
        host,
        port,
        username,
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
    if security.mode == "vnc-auth"
        && let Some(password) = password.as_ref()
        && password.len() > 8
    {
        return Err(AppError::Config(
            "Classic VNC authentication passwords must be 8 bytes or fewer".to_string(),
        ));
    }

    Ok(VncConnectConfig {
        owner_window_label,
        session_id: uuid::Uuid::new_v4().to_string(),
        connection_id: connection_id.to_string(),
        name: connection.name,
        host,
        port,
        username,
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
    app: &impl Sized,
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

    #[tokio::test]
    async fn vnc_tofu_commits_only_confirmed_successful_current_handshakes() {
        use crate::storage::{KnownHostCheck, Storage};
        for (name, old_key, accepted, authenticated, current_generation, cancelled, expected) in [
            (
                "unknown-success",
                None,
                true,
                true,
                1,
                false,
                KnownHostCheck::Match,
            ),
            (
                "unknown-reject",
                None,
                false,
                false,
                1,
                false,
                KnownHostCheck::UnknownHost,
            ),
            (
                "hash-mismatch",
                None,
                true,
                false,
                1,
                false,
                KnownHostCheck::UnknownHost,
            ),
            (
                "transport-error",
                None,
                true,
                false,
                1,
                false,
                KnownHostCheck::UnknownHost,
            ),
            (
                "timeout",
                None,
                true,
                false,
                1,
                false,
                KnownHostCheck::UnknownHost,
            ),
            (
                "changed-success",
                Some("SHA256:old"),
                true,
                true,
                1,
                false,
                KnownHostCheck::Match,
            ),
            (
                "changed-reject",
                Some("SHA256:old"),
                false,
                false,
                1,
                false,
                KnownHostCheck::HostSeen,
            ),
            (
                "stale-generation",
                None,
                true,
                true,
                2,
                false,
                KnownHostCheck::UnknownHost,
            ),
            (
                "cancelled",
                None,
                true,
                true,
                1,
                true,
                KnownHostCheck::UnknownHost,
            ),
        ] {
            let dir =
                std::env::temp_dir().join(format!("vnc-tofu-{name}-{}", uuid::Uuid::new_v4()));
            let storage = Storage::open(&dir).unwrap();
            if let Some(old_key) = old_key {
                storage
                    .upsert_vnc_known_host("pi.local", 5900, old_key)
                    .unwrap();
            }
            let status = storage
                .check_vnc_known_host("pi.local", 5900, "SHA256:new")
                .unwrap();
            let trust_candidate = match confirm_vnc_trust(status, async {
                if accepted {
                    Ok(())
                } else {
                    Err(VncError::Ra2ServerKeyRejected("rejected".into()))
                }
            })
            .await
            {
                Ok(true) => Some("SHA256:new"),
                Ok(false) | Err(_) => None,
            };
            let authenticated_key = authenticated.then_some("SHA256:new");
            commit_vnc_trust(
                trust_candidate,
                authenticated_key,
                1,
                current_generation,
                cancelled,
                |fingerprint| storage.upsert_vnc_known_host("pi.local", 5900, fingerprint),
            )
            .unwrap();
            assert_eq!(
                storage
                    .check_vnc_known_host("pi.local", 5900, "SHA256:new")
                    .unwrap(),
                expected,
                "{name}"
            );
            if old_key.is_some() && expected != KnownHostCheck::Match {
                assert_eq!(
                    storage
                        .check_vnc_known_host("pi.local", 5900, "SHA256:old")
                        .unwrap(),
                    KnownHostCheck::Match
                );
            }
            drop(storage);
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[tokio::test]
    async fn matching_vnc_key_does_not_poll_user_confirmation() {
        assert!(
            !confirm_vnc_trust(crate::storage::KnownHostCheck::Match, async {
                panic!("matching key must never prompt");
            })
            .await
            .unwrap()
        );
    }

    #[tokio::test]
    async fn matching_vnc_key_cannot_overwrite_newer_explicit_trust() {
        use crate::storage::{KnownHostCheck, Storage};

        let dir = std::env::temp_dir().join(format!("vnc-tofu-race-{}", uuid::Uuid::new_v4()));
        let storage = Storage::open(&dir).unwrap();
        storage
            .upsert_vnc_known_host("pi.local", 5900, "SHA256:k1")
            .unwrap();

        let status = storage
            .check_vnc_known_host("pi.local", 5900, "SHA256:k1")
            .unwrap();
        let trust_candidate = confirm_vnc_trust(status, async {
            panic!("matching key must never prompt");
        })
        .await
        .unwrap()
        .then_some("SHA256:k1");
        assert!(trust_candidate.is_none());

        storage
            .upsert_vnc_known_host("pi.local", 5900, "SHA256:k2")
            .unwrap();

        commit_vnc_trust(
            trust_candidate,
            Some("SHA256:k1"),
            1,
            1,
            false,
            |fingerprint| storage.upsert_vnc_known_host("pi.local", 5900, fingerprint),
        )
        .unwrap();

        assert_eq!(
            storage
                .check_vnc_known_host("pi.local", 5900, "SHA256:k2")
                .unwrap(),
            KnownHostCheck::Match
        );
        drop(storage);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn vnc_verification_payload_routes_to_its_session_owner() {
        let payload = VncServerKeyVerifyEvent {
            request_id: "request".into(),
            session_id: "session".into(),
            host: "pi.local".into(),
            port: 5900,
            fingerprint: "key".into(),
            key_bits: 1024,
            known_host_status: "unknown".into(),
            target_window_label: "main-second".into(),
        };
        assert_eq!(
            serde_json::to_value(payload).unwrap()["targetWindowLabel"],
            "main-second"
        );
    }

    #[test]
    fn security_mode_maps_to_fail_closed_policy() {
        assert_eq!(security_policy("none"), VncSecurityPolicy::NoneOnly);
        assert_eq!(security_policy("vnc-auth"), VncSecurityPolicy::VncAuthOnly);
        assert_eq!(security_policy("auto"), VncSecurityPolicy::Auto);
    }

    #[test]
    fn connect_target_preserves_ipv6_literals_without_host_port_concatenation() {
        let config = VncConnectConfig {
            owner_window_label: "main-test".into(),
            session_id: "vnc-test".to_string(),
            connection_id: "connection-test".to_string(),
            name: "IPv6 VNC".to_string(),
            host: "::1".to_string(),
            port: 5900,
            username: String::new(),
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
        let session = test_session();

        queue_or_send_frame(&session, vec![1]).await;
        queue_or_send_frame(&session, vec![2]).await;
        queue_or_send_frame(&session, vec![3]).await;

        let pending = session.pending_frames.lock().await;
        assert_eq!(pending.len(), MAX_PENDING_FRAMES);
        assert_eq!(pending[0], vec![2]);
        assert_eq!(pending[1], vec![3]);
    }
    fn test_session() -> Arc<VncSession> {
        Arc::new(VncSession {
            config: VncConnectConfig {
                owner_window_label: "main-test".into(),
                session_id: "vnc-test".to_string(),
                connection_id: "connection-test".to_string(),
                name: "Test VNC".to_string(),
                host: "127.0.0.1".to_string(),
                port: 5900,
                username: String::new(),
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
            trust_commit_guard: std::sync::Mutex::new(()),
            pending_server_keys: Arc::new(Mutex::new(HashMap::new())),
        })
    }
    #[tokio::test]
    async fn bounded_subscriber_recovers_dropped_patches_with_a_complete_frame() {
        let manager = VncSessionManager::new();
        let session = test_session();
        *session.framebuffer.lock().await = Some(VncFramebuffer::new(2, 1).unwrap());
        manager
            .sessions
            .lock()
            .await
            .insert(session.config.session_id.clone(), session.clone());
        let context = AppHandle {
            events: Arc::new(|_, _, _| Ok(())),
            transport: Arc::new(|_, _, _, _| Box::pin(async { panic!("no network needed") })),
        };
        let (tx, mut rx) = mpsc::channel(2);
        let channel = FrameChannel(Arc::new(move |bytes| {
            tx.try_send(bytes)
                .map_err(|_| AppError::Channel("slow client".into()))
        }));
        manager
            .attach_frame_channel(&context, "vnc-test", channel.clone())
            .await
            .unwrap();
        for value in 1..=10 {
            handle_vnc_event(
                &context,
                &session,
                0,
                VncEvent::RawImage(
                    Rect {
                        x: 1,
                        y: 0,
                        width: 1,
                        height: 1,
                    },
                    vec![value, 20, 30, 255],
                ),
            )
            .await
            .unwrap();
        }
        assert!(session.frame_channel.lock().await.is_none());
        assert!(session.pending_frames.lock().await.len() <= 2);
        while rx.try_recv().is_ok() {}
        manager
            .attach_frame_channel(&context, "vnc-test", channel)
            .await
            .unwrap();
        let full = rx.recv().await.unwrap();
        assert_eq!(full.len(), 52);
        assert_eq!(u32::from_le_bytes(full[16..20].try_into().unwrap()), 0);
        assert_eq!(u32::from_le_bytes(full[24..28].try_into().unwrap()), 2);
        assert_eq!(&full[48..52], &[10, 20, 30, 255]);
        assert!(rx.try_recv().is_err());
    }
}
