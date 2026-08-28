use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex, OnceLock, Weak};
use std::time::{Duration, Instant};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use nyaterm_mcp_protocol::{
    AuthParams, CapabilityExecuteParams, ClientIdentifyParams, DiscoveryDocument,
    MAX_INLINE_OUTPUT_BYTES, MAX_RPC_LINE_BYTES, MAX_TEXT_READ_BYTES, MAX_TEXT_WRITE_BYTES,
    PROTOCOL_VERSION, PathArgs, RpcError, RpcRequest, RpcResponse, SessionArgs, SftpChmodArgs,
    SftpMkdirArgs, SftpReadTextArgs, SftpRenameArgs, SftpWriteTextArgs, TerminalExecuteArgs,
    TerminalRecentOutputArgs, tool,
};
use rand::RngCore;
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;

use super::approval::{ApprovalDecision, ApprovalRequestEvent, McpApprovalManager};
use super::discovery::DiscoveryStore;
use crate::config::{
    AiExecutionProfile, AiPermissionMode, ExternalMcpServerMode, ExternalMcpSessionScope,
    ExternalMcpSettings, RiskLevel,
};
use crate::core::SessionManager;
use crate::core::ai::{AppendAiAuditRequest, append_ai_audit, redact_sensitive_text};
use crate::core::capabilities::sftp as sftp_capability;
use crate::core::capabilities::{
    CapabilityAccess, McpScope, OutputStore, PolicyDecision, TerminalExecuteRequest,
    assess_command_risk, capability_for_tool, decide_policy, execute_terminal_command,
};
use crate::core::session::{SessionInfo, SessionType};
use crate::error::{AppError, AppResult};

const EXTERNAL_SOURCE: &str = "external_mcp";
const DEFAULT_CLIENT: &str = "External MCP client";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpRuntimeStatus {
    pub enabled: bool,
    pub running: bool,
    pub error: Option<String>,
    pub owner_window_label: Option<String>,
    pub scoped_session_count: usize,
    pub connection_count: usize,
    pub port: Option<u16>,
    pub generation: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpClientConfigs {
    pub sidecar_path: String,
    pub codex: Value,
    pub claude_code: Value,
    pub cursor: Value,
}

#[derive(Clone)]
pub struct EphemeralMcpCredential {
    pub host: String,
    pub port: u16,
    pub token: String,
    pub generation: String,
    pub sidecar_path: PathBuf,
    cancellation: CancellationToken,
    manager: Weak<McpManager>,
}

impl EphemeralMcpCredential {
    pub fn env(&self) -> HashMap<String, String> {
        HashMap::from([
            ("NYATERM_MCP_EPHEMERAL".into(), "1".into()),
            ("NYATERM_MCP_HOST".into(), self.host.clone()),
            ("NYATERM_MCP_PORT".into(), self.port.to_string()),
            ("NYATERM_MCP_TOKEN".into(), self.token.clone()),
            ("NYATERM_MCP_GENERATION".into(), self.generation.clone()),
        ])
    }
}

impl Drop for EphemeralMcpCredential {
    fn drop(&mut self) {
        self.cancellation.cancel();
        let Some(manager) = self.manager.upgrade() else {
            return;
        };
        let generation = self.generation.clone();
        if let Ok(mut credentials) = manager.credentials.try_write() {
            credentials.remove(&generation);
            return;
        }
        tauri::async_runtime::spawn(async move {
            manager.credentials.write().await.remove(&generation);
        });
    }
}

struct Credential {
    token: String,
    scope: Arc<McpScope>,
    permission_mode: AiPermissionMode,
    source: String,
    owner_window_label: Option<String>,
    cancellation: CancellationToken,
}

struct ExternalRuntime {
    settings: ExternalMcpSettings,
    owner_window_label: String,
    generation: String,
    scoped_session_count: usize,
    cancellation: CancellationToken,
    last_activity: Arc<StdMutex<Instant>>,
    approval_waiters: Arc<AtomicUsize>,
}

struct ConnectionContext {
    id: String,
    generation: String,
    credential: Arc<Credential>,
    client: StdMutex<String>,
    grants: Mutex<HashSet<(String, String)>>,
    outputs: Mutex<OutputStore>,
    cancellation: CancellationToken,
}

pub struct McpManager {
    sessions: Arc<SessionManager>,
    config_dir: PathBuf,
    executable_dir: PathBuf,
    app: OnceLock<AppHandle>,
    port: AtomicU16,
    initialized: AtomicBool,
    shutdown: CancellationToken,
    credentials: RwLock<HashMap<String, Arc<Credential>>>,
    external: Mutex<Option<ExternalRuntime>>,
    persistent_startup: Mutex<Option<ExternalMcpSettings>>,
    approvals: Arc<McpApprovalManager>,
    request_cancellations: Mutex<HashMap<String, (String, CancellationToken)>>,
    external_connections: AtomicUsize,
    last_error: StdMutex<Option<String>>,
}

impl McpManager {
    pub fn new(
        sessions: Arc<SessionManager>,
        config_dir: impl Into<PathBuf>,
        executable_dir: impl Into<PathBuf>,
    ) -> Arc<Self> {
        Arc::new(Self {
            sessions,
            config_dir: config_dir.into(),
            executable_dir: executable_dir.into(),
            app: OnceLock::new(),
            port: AtomicU16::new(0),
            initialized: AtomicBool::new(false),
            shutdown: CancellationToken::new(),
            credentials: RwLock::new(HashMap::new()),
            external: Mutex::new(None),
            persistent_startup: Mutex::new(None),
            approvals: Arc::new(McpApprovalManager::default()),
            request_cancellations: Mutex::new(HashMap::new()),
            external_connections: AtomicUsize::new(0),
            last_error: StdMutex::new(None),
        })
    }

    pub async fn initialize(self: &Arc<Self>, app: AppHandle) -> AppResult<()> {
        if self.initialized.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        let _ = self.app.set(app.clone());
        DiscoveryStore::new(&self.config_dir).remove()?;
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        self.port
            .store(listener.local_addr()?.port(), Ordering::SeqCst);
        let manager = self.clone();
        tauri::async_runtime::spawn(async move { manager.accept_loop(listener).await });

        let settings = crate::config::load_app_settings(&app)?.ai.external_mcp;
        if settings.enabled && settings.server_mode == ExternalMcpServerMode::Persistent {
            *self.persistent_startup.lock().await = Some(settings);
        } else if settings.enabled {
            let _ = crate::storage::update_settings_doc(
                crate::storage::SettingsDocKey::AppSettings,
                |stored: &mut crate::config::AppSettings| {
                    stored.ai.external_mcp.enabled = false;
                    Ok(())
                },
            );
        }
        Ok(())
    }

    pub async fn session_restore_complete(
        self: &Arc<Self>,
        owner_window_label: &str,
    ) -> AppResult<McpRuntimeStatus> {
        let settings = self.persistent_startup.lock().await.take();
        if let Some(settings) = settings {
            self.configure_external(settings, owner_window_label).await
        } else {
            Ok(self.status().await)
        }
    }

    pub fn record_startup_error(&self, error: &dyn std::fmt::Display) {
        self.set_error(Some(error.to_string()));
        self.emit_status();
    }

    pub async fn configure_external(
        self: &Arc<Self>,
        settings: ExternalMcpSettings,
        owner_window_label: &str,
    ) -> AppResult<McpRuntimeStatus> {
        if !(1..=120).contains(&settings.idle_timeout_minutes) {
            return Err(AppError::Config(
                "External MCP idle timeout must be between 1 and 120 minutes.".into(),
            ));
        }
        if !settings.enabled {
            self.disable_external(false).await?;
            return Ok(self.status().await);
        }
        if self.port.load(Ordering::SeqCst) == 0 {
            return Err(AppError::Config(
                "The MCP bridge is not initialized.".into(),
            ));
        }
        {
            let current = self.external.lock().await;
            if let Some(current) = current.as_ref() {
                if current.owner_window_label != owner_window_label {
                    return Err(AppError::Config("External MCP is already bound to another NyaTerm window. Disable it before enabling it from this window.".into()));
                }
                if current.settings == settings {
                    return Ok(self.status_from(Some(current)));
                }
            }
        }
        self.disable_external(false).await?;
        let mut session_ids = self
            .sessions
            .list_sessions()
            .await
            .into_iter()
            .filter(|session| {
                settings.session_scope == ExternalMcpSessionScope::AllSessions
                    || session.owner_window_label.as_deref() == Some(owner_window_label)
            })
            .map(|session| session.id)
            .collect::<Vec<_>>();
        session_ids.sort();
        let default_session_id = (session_ids.len() == 1).then(|| session_ids[0].clone());
        let generation = uuid::Uuid::new_v4().to_string();
        let token = random_token();
        let cancellation = CancellationToken::new();
        let credential = Arc::new(Credential {
            token: token.clone(),
            scope: Arc::new(McpScope::new(session_ids.clone(), default_session_id)),
            permission_mode: settings.permission_mode.clone(),
            source: EXTERNAL_SOURCE.into(),
            owner_window_label: Some(owner_window_label.to_string()),
            cancellation: cancellation.clone(),
        });
        self.credentials
            .write()
            .await
            .insert(generation.clone(), credential);
        let last_activity = Arc::new(StdMutex::new(Instant::now()));
        let approval_waiters = Arc::new(AtomicUsize::new(0));
        *self.external.lock().await = Some(ExternalRuntime {
            settings: settings.clone(),
            owner_window_label: owner_window_label.to_string(),
            generation: generation.clone(),
            scoped_session_count: session_ids.len(),
            cancellation: cancellation.clone(),
            last_activity: last_activity.clone(),
            approval_waiters: approval_waiters.clone(),
        });
        let document = DiscoveryDocument {
            version: PROTOCOL_VERSION,
            pid: std::process::id(),
            host: "127.0.0.1".into(),
            port: self.port.load(Ordering::SeqCst),
            token,
            generation: generation.clone(),
            permission_mode: permission_mode_name(&settings.permission_mode).into(),
        };
        if let Err(error) = DiscoveryStore::new(&self.config_dir).write(&document) {
            self.disable_external(false).await?;
            self.set_error(Some(error.to_string()));
            return Err(error);
        }
        self.set_error(None);
        if settings.server_mode == ExternalMcpServerMode::Temporary {
            let manager = self.clone();
            tauri::async_runtime::spawn(async move {
                manager
                    .temporary_idle_worker(
                        generation,
                        settings.idle_timeout_minutes,
                        last_activity,
                        approval_waiters,
                        cancellation,
                    )
                    .await;
            });
        }
        self.emit_status();
        Ok(self.status().await)
    }

    pub async fn disable_external(&self, persist_disabled: bool) -> AppResult<()> {
        if let Some(previous) = self.external.lock().await.take() {
            previous.cancellation.cancel();
            self.credentials.write().await.remove(&previous.generation);
        }
        DiscoveryStore::new(&self.config_dir).remove()?;
        if persist_disabled {
            if let Some(app) = self.app.get() {
                crate::storage::update_settings_doc(
                    crate::storage::SettingsDocKey::AppSettings,
                    |settings: &mut crate::config::AppSettings| {
                        settings.ai.external_mcp.enabled = false;
                        Ok(())
                    },
                )?;
                let _ = app.emit("settings-changed", ());
            }
        }
        self.emit_status();
        Ok(())
    }

    pub async fn owner_window_closed(&self, label: &str) {
        let matches = self
            .external
            .lock()
            .await
            .as_ref()
            .is_some_and(|state| state.owner_window_label == label);
        if matches {
            let _ = self.disable_external(true).await;
        }
    }

    pub fn shutdown_cleanup(&self) {
        self.shutdown.cancel();
        if let Ok(mut external) = self.external.try_lock() {
            if let Some(state) = external.take() {
                state.cancellation.cancel();
            }
        }
        let _ = DiscoveryStore::new(&self.config_dir).remove();
    }

    pub async fn status(&self) -> McpRuntimeStatus {
        let external = self.external.lock().await;
        self.status_from(external.as_ref())
    }

    fn status_from(&self, external: Option<&ExternalRuntime>) -> McpRuntimeStatus {
        let port = self.port.load(Ordering::SeqCst);
        McpRuntimeStatus {
            enabled: external.is_some(),
            running: external.is_some() && port != 0,
            error: self.last_error.lock().unwrap().clone(),
            owner_window_label: external.map(|state| state.owner_window_label.clone()),
            scoped_session_count: external.map_or(0, |state| state.scoped_session_count),
            connection_count: self.external_connections.load(Ordering::SeqCst),
            port: (port != 0).then_some(port),
            generation: external.map(|state| state.generation.clone()),
        }
    }

    pub fn client_configs(&self) -> AppResult<McpClientConfigs> {
        let sidecar = self.sidecar_path()?;
        let path = sidecar.to_string_lossy().to_string();
        let server = json!({ "command": path, "args": [] });
        Ok(McpClientConfigs {
            sidecar_path: sidecar.to_string_lossy().to_string(),
            codex: json!({ "mcp_servers": { "nyaterm": server.clone() } }),
            claude_code: json!({ "mcpServers": { "nyaterm": server.clone() } }),
            cursor: json!({ "mcpServers": { "nyaterm": server } }),
        })
    }

    pub async fn create_ephemeral_credential(
        self: &Arc<Self>,
        source: &str,
        session_ids: Vec<String>,
        default_session_id: Option<String>,
        permission_mode: AiPermissionMode,
        owner_window_label: Option<String>,
    ) -> AppResult<EphemeralMcpCredential> {
        let port = self.port.load(Ordering::SeqCst);
        if port == 0 {
            return Err(AppError::Config(
                "The MCP bridge is not initialized.".into(),
            ));
        }
        let sidecar_path = self.sidecar_path()?;
        let generation = uuid::Uuid::new_v4().to_string();
        let token = random_token();
        let cancellation = CancellationToken::new();
        self.credentials.write().await.insert(
            generation.clone(),
            Arc::new(Credential {
                token: token.clone(),
                scope: Arc::new(McpScope::new(session_ids, default_session_id)),
                permission_mode,
                source: source.to_string(),
                owner_window_label,
                cancellation: cancellation.clone(),
            }),
        );
        Ok(EphemeralMcpCredential {
            host: "127.0.0.1".into(),
            port,
            token,
            generation,
            sidecar_path,
            cancellation,
            manager: Arc::downgrade(self),
        })
    }

    pub async fn respond_approval(
        &self,
        request_id: &str,
        decision: ApprovalDecision,
    ) -> AppResult<()> {
        self.approvals.respond(request_id, decision).await
    }

    pub async fn cancel_pending_approvals(&self) {
        self.approvals.cancel_all().await;
    }

    fn sidecar_path(&self) -> AppResult<PathBuf> {
        let name = if cfg!(windows) {
            "nyaterm-mcp.exe"
        } else {
            "nyaterm-mcp"
        };
        [
            self.executable_dir.join(name),
            self.executable_dir.join("resources").join(name),
            self.executable_dir.join("..").join("Resources").join(name),
        ]
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| AppError::Config("The nyaterm-mcp sidecar is not installed.".into()))
    }

    async fn accept_loop(self: Arc<Self>, listener: TcpListener) {
        loop {
            tokio::select! {
                _ = self.shutdown.cancelled() => break,
                result = listener.accept() => match result {
                    Ok((stream, address)) if address.ip().is_loopback() => {
                        let manager = self.clone();
                        tauri::async_runtime::spawn(async move { manager.handle_connection(stream).await });
                    }
                    Ok(_) => {}
                    Err(error) => { self.set_error(Some(format!("MCP bridge listener failed: {error}"))); break; }
                }
            }
        }
    }

    async fn handle_connection(self: Arc<Self>, stream: TcpStream) {
        let (read, mut write) = stream.into_split();
        let mut reader = BufReader::new(read);
        let Some(first) = read_rpc_line(&mut reader).await.ok().flatten() else {
            return;
        };
        let request: RpcRequest = match serde_json::from_slice(&first) {
            Ok(value) => value,
            Err(_) => return,
        };
        if request.method != "auth" {
            let _ = write_response(
                &mut write,
                rpc_error(
                    request.id,
                    "authentication_required",
                    "The first MCP bridge request must be auth.",
                ),
            )
            .await;
            return;
        }
        let auth: AuthParams = match serde_json::from_value(request.params) {
            Ok(value) => value,
            Err(_) => {
                let _ = write_response(
                    &mut write,
                    rpc_error(request.id, "invalid_argument", "Invalid auth request."),
                )
                .await;
                return;
            }
        };
        let Some(credential) = self.authenticate(&auth).await else {
            let _ = write_response(
                &mut write,
                rpc_error(
                    request.id,
                    "authentication_failed",
                    "Invalid or expired MCP credential.",
                ),
            )
            .await;
            return;
        };
        let cancellation = credential.cancellation.child_token();
        let context = Arc::new(ConnectionContext {
            id: uuid::Uuid::new_v4().to_string(),
            generation: auth.generation,
            credential: credential.clone(),
            client: StdMutex::new(DEFAULT_CLIENT.into()),
            grants: Mutex::new(HashSet::new()),
            outputs: Mutex::new(OutputStore::default()),
            cancellation: cancellation.clone(),
        });
        let is_external = credential.source == EXTERNAL_SOURCE;
        if write_response(
            &mut write,
            rpc_ok(request.id, json!({ "authenticated": true })),
        )
        .await
        .is_err()
        {
            return;
        }
        if is_external {
            self.external_connections.fetch_add(1, Ordering::SeqCst);
            self.touch_external(&context.generation).await;
            self.emit_status();
        }
        loop {
            let line = tokio::select! {
                _ = cancellation.cancelled() => break,
                result = read_rpc_line(&mut reader) => match result { Ok(Some(line)) => line, _ => break }
            };
            let request: RpcRequest = match serde_json::from_slice(&line) {
                Ok(value) => value,
                Err(_) => break,
            };
            if is_external {
                self.touch_external(&context.generation).await;
            }
            let response = self.handle_request(&context, request).await;
            if write_response(&mut write, response).await.is_err() {
                break;
            }
        }
        context.cancellation.cancel();
        self.approvals.cancel_connection(&context.id).await;
        if is_external {
            self.external_connections.fetch_sub(1, Ordering::SeqCst);
            self.emit_status();
        }
    }

    async fn handle_request(
        &self,
        context: &Arc<ConnectionContext>,
        request: RpcRequest,
    ) -> RpcResponse {
        match request.method.as_str() {
            "client.identify" => {
                match serde_json::from_value::<ClientIdentifyParams>(request.params) {
                    Ok(client) if !client.name.trim().is_empty() && client.name.len() <= 128 => {
                        *context.client.lock().unwrap() =
                            client.version.map_or(client.name.clone(), |version| {
                                format!("{} {}", client.name, version)
                            });
                        rpc_ok(request.id, json!({ "accepted": true }))
                    }
                    _ => rpc_error(
                        request.id,
                        "invalid_argument",
                        "Invalid MCP client metadata.",
                    ),
                }
            }
            "request.cancel" => match request.params.get("requestId").and_then(Value::as_str) {
                Some(id) => {
                    if let Some((_, token)) = self
                        .request_cancellations
                        .lock()
                        .await
                        .get(id)
                        .filter(|(generation, _)| generation == &context.generation)
                    {
                        token.cancel();
                    }
                    rpc_ok(request.id, json!({ "cancelled": true }))
                }
                None => rpc_error(request.id, "invalid_argument", "requestId is required."),
            },
            "capability.execute" => {
                let params = match serde_json::from_value::<CapabilityExecuteParams>(request.params)
                {
                    Ok(value) => value,
                    Err(error) => {
                        return rpc_error(request.id, "invalid_argument", &error.to_string());
                    }
                };
                let token = context.cancellation.child_token();
                if let Some(id) = params.request_id.as_ref() {
                    self.request_cancellations
                        .lock()
                        .await
                        .insert(id.clone(), (context.generation.clone(), token.clone()));
                }
                let result = self
                    .execute_tool(context, &params.tool, params.arguments, token)
                    .await;
                if context.credential.source == EXTERNAL_SOURCE {
                    self.touch_external(&context.generation).await;
                }
                if let Some(id) = params.request_id.as_ref() {
                    self.request_cancellations.lock().await.remove(id);
                }
                match result {
                    Ok(value) => rpc_ok(request.id, value),
                    Err(error) => RpcResponse {
                        id: request.id,
                        result: None,
                        error: Some(error),
                    },
                }
            }
            _ => rpc_error(request.id, "method_not_found", "Unknown MCP bridge method."),
        }
    }

    async fn execute_tool(
        &self,
        context: &Arc<ConnectionContext>,
        tool_name: &str,
        arguments: Value,
        cancellation: CancellationToken,
    ) -> Result<Value, RpcError> {
        let started = Instant::now();
        let definition = capability_for_tool(tool_name)
            .ok_or_else(|| failure("invalid_argument", "Unknown NyaTerm MCP tool."))?;
        if tool_name == tool::OUTPUT_READ {
            let args: nyaterm_mcp_protocol::OutputReadArgs = parse(arguments.clone())?;
            let max_bytes = args.max_bytes.unwrap_or(MAX_INLINE_OUTPUT_BYTES);
            if max_bytes == 0 || max_bytes > MAX_INLINE_OUTPUT_BYTES {
                return Err(failure(
                    "invalid_argument",
                    "maxBytes must be between 1 and 65536.",
                ));
            }
            let result = context
                .outputs
                .lock()
                .await
                .read(&args.output_id, args.offset, max_bytes)
                .map_err(map_error)
                .and_then(|value| serde_json::to_value(value).map_err(internal_error));
            match result {
                Ok(value) => {
                    self.audit(
                        context,
                        definition.id,
                        None,
                        None,
                        Some("inherited"),
                        true,
                        started.elapsed(),
                        None,
                        tool_name,
                        &arguments,
                    );
                    return Ok(value);
                }
                Err(error) => {
                    self.audit(
                        context,
                        definition.id,
                        None,
                        None,
                        Some("inherited"),
                        false,
                        started.elapsed(),
                        Some(&error.message),
                        tool_name,
                        &arguments,
                    );
                    return Err(error);
                }
            }
        }
        let session_id = match self.resolve_session(context, tool_name, &arguments) {
            Ok(session_id) => session_id,
            Err(error) => {
                let mapped = map_error(error);
                self.audit(
                    context,
                    definition.id,
                    arguments.get("sessionId").and_then(Value::as_str),
                    None,
                    Some("validation_denied"),
                    false,
                    started.elapsed(),
                    Some(&mapped.message),
                    tool_name,
                    &arguments,
                );
                return Err(mapped);
            }
        };
        if definition.requires_session && session_id.is_none() {
            return Err(failure(
                "invalid_argument",
                "A target session is required for this capability.",
            ));
        }
        let risk = if tool_name == tool::TERMINAL_EXECUTE {
            Some(
                assess_command_risk(&parse::<TerminalExecuteArgs>(arguments.clone())?.command)
                    .level,
            )
        } else {
            None
        };
        let policy = decide_policy(
            &context.credential.permission_mode,
            definition.access,
            risk.as_ref(),
        );
        let grant_key = session_id.clone().map(|id| (id, definition.id.to_string()));
        let grantable = definition.access != CapabilityAccess::DestructiveWrite
            && risk.as_ref().is_none_or(|value| *value < RiskLevel::High);
        let granted = grantable
            && match grant_key.as_ref() {
                Some(key) => context.grants.lock().await.contains(key),
                None => false,
            };
        let mut approval = None;
        if policy == PolicyDecision::Deny {
            self.audit(
                context,
                definition.id,
                session_id.as_deref(),
                risk,
                Some("policy_denied"),
                false,
                started.elapsed(),
                Some("Permission mode denied the capability."),
                tool_name,
                &arguments,
            );
            return Err(failure(
                "permission_denied",
                "The current MCP permission mode does not allow this operation.",
            ));
        }
        if policy == PolicyDecision::RequireApproval && !granted {
            let target = session_id.as_deref().ok_or_else(|| {
                failure(
                    "approval_denied",
                    "A target session is required for approval.",
                )
            })?;
            let info = self
                .sessions
                .session_info(target)
                .await
                .map_err(map_error)?;
            let owner = info
                .owner_window_label
                .as_deref()
                .or(context.credential.owner_window_label.as_deref())
                .ok_or_else(|| {
                    failure(
                        "approval_denied",
                        "The session owner window is unavailable for approval.",
                    )
                })?;
            let waiter = self.external_waiter(&context.generation).await;
            if let Some(waiter) = waiter.as_ref() {
                waiter.fetch_add(1, Ordering::SeqCst);
            }
            let event = ApprovalRequestEvent {
                request_id: uuid::Uuid::new_v4().to_string(),
                client: context.client.lock().unwrap().clone(),
                capability: definition.id.to_string(),
                session_id: Some(target.to_string()),
                session_name: Some(info.name),
                parameter_summary: summarize(tool_name, &arguments),
                risk: risk
                    .clone()
                    .unwrap_or_else(|| access_risk(definition.access)),
            };
            let result = self
                .approvals
                .request(
                    self.app
                        .get()
                        .ok_or_else(|| failure("approval_denied", "NyaTerm is not ready."))?,
                    owner,
                    &context.id,
                    event,
                    &cancellation,
                )
                .await;
            if let Some(waiter) = waiter.as_ref() {
                waiter.fetch_sub(1, Ordering::SeqCst);
            }
            let decision = match result {
                Ok(decision) => decision,
                Err(error) => {
                    self.audit(
                        context,
                        definition.id,
                        Some(target),
                        risk.clone(),
                        Some("approval_unavailable"),
                        false,
                        started.elapsed(),
                        Some(&error.to_string()),
                        tool_name,
                        &arguments,
                    );
                    return Err(failure("approval_denied", &error.to_string()));
                }
            };
            approval = Some(decision.as_str());
            if decision == ApprovalDecision::Deny {
                self.audit(
                    context,
                    definition.id,
                    Some(target),
                    risk,
                    approval,
                    false,
                    started.elapsed(),
                    Some("User denied the MCP approval request."),
                    tool_name,
                    &arguments,
                );
                return Err(failure(
                    "approval_denied",
                    "The operation was denied by the user.",
                ));
            }
            if decision == ApprovalDecision::AllowSession && grantable {
                if let Some(key) = grant_key {
                    context.grants.lock().await.insert(key);
                }
            }
        }
        let result = tokio::select! {
            _ = cancellation.cancelled() => Err(failure("cancelled", "The MCP request was cancelled.")),
            value = self.dispatch(
                context,
                tool_name,
                arguments.clone(),
                session_id.as_deref(),
                cancellation.clone(),
            ) => value,
        };
        let elapsed = started.elapsed();
        match result {
            Ok(value) => {
                self.audit(
                    context,
                    definition.id,
                    session_id.as_deref(),
                    risk,
                    approval,
                    true,
                    elapsed,
                    None,
                    tool_name,
                    &arguments,
                );
                protect(&context.outputs, value).await
            }
            Err(error) => {
                self.audit(
                    context,
                    definition.id,
                    session_id.as_deref(),
                    risk,
                    approval,
                    false,
                    elapsed,
                    Some(&error.message),
                    tool_name,
                    &arguments,
                );
                Err(error)
            }
        }
    }

    fn resolve_session(
        &self,
        context: &ConnectionContext,
        tool_name: &str,
        arguments: &Value,
    ) -> AppResult<Option<String>> {
        if tool_name == tool::GET_ENVIRONMENT {
            return Ok(None);
        }
        if tool_name == tool::TERMINAL_EXECUTE {
            let args: TerminalExecuteArgs = serde_json::from_value(arguments.clone())?;
            return context
                .credential
                .scope
                .resolve_terminal_session(args.session_id.as_deref())
                .map(Some);
        }
        let id = arguments
            .get("sessionId")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::Config("sessionId is required.".into()))?;
        context.credential.scope.require(id)?;
        Ok(Some(id.to_string()))
    }

    async fn dispatch(
        &self,
        context: &ConnectionContext,
        name: &str,
        arguments: Value,
        session_id: Option<&str>,
        cancellation: CancellationToken,
    ) -> Result<Value, RpcError> {
        match name {
            tool::GET_ENVIRONMENT => {
                let mut sessions = Vec::new();
                for id in &context.credential.scope.session_ids {
                    if let Ok(info) = self.sessions.session_info(id).await {
                        sessions.push(safe_metadata(&info));
                    }
                }
                sessions.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
                Ok(
                    json!({ "defaultSessionId": context.credential.scope.default_session_id, "sessions": sessions }),
                )
            }
            tool::SESSION_GET => {
                let args: SessionArgs = parse(arguments)?;
                let info = self
                    .sessions
                    .session_info(&args.session_id)
                    .await
                    .map_err(map_error)?;
                let cwd = self
                    .sessions
                    .session_cwd(&args.session_id)
                    .await
                    .map_err(map_error)?;
                Ok(json!({
                    "id": info.id, "name": info.name, "type": session_type_name(&info.session_type),
                    "connected": info.connected, "cwd": cwd,
                    "terminalExecution": execution_profile_name(info.ai_execution_profile),
                    "sftpAvailable": sftp_available(&info),
                }))
            }
            tool::TERMINAL_EXECUTE => {
                let args: TerminalExecuteArgs = parse(arguments)?;
                if args.command.trim().is_empty() {
                    return Err(failure("invalid_argument", "command must not be empty."));
                }
                let timeout_ms = args.timeout_ms.unwrap_or(30_000);
                if !(1_000..=300_000).contains(&timeout_ms) {
                    return Err(failure(
                        "invalid_argument",
                        "timeoutMs must be between 1 and 300000.",
                    ));
                }
                let result = execute_terminal_command(
                    self.sessions.clone(),
                    TerminalExecuteRequest {
                        session_id: session_id.unwrap().to_string(),
                        command: args.command,
                        timeout_ms,
                    },
                    None,
                    cancellation,
                )
                .await
                .map_err(map_error)?;
                serde_json::to_value(result).map_err(internal_error)
            }
            tool::TERMINAL_RECENT_OUTPUT => {
                let args: TerminalRecentOutputArgs = parse(arguments)?;
                Ok(
                    json!({ "sessionId": args.session_id, "output": self.sessions.recent_output(session_id.unwrap(), args.lines.unwrap_or(100).clamp(1, 500)) }),
                )
            }
            tool::SFTP_HOME => {
                let args: SessionArgs = parse(arguments)?;
                Ok(json!({
                    "path": sftp_capability::home(self.sessions.clone(), &args.session_id)
                        .await
                        .map_err(map_error)?
                }))
            }
            tool::SFTP_LIST => {
                let args: PathArgs = parse(arguments)?;
                serde_json::to_value(
                    sftp_capability::list(self.sessions.clone(), &args.session_id, &args.path)
                        .await
                        .map_err(map_error)?,
                )
                .map_err(internal_error)
            }
            tool::SFTP_STAT => {
                let args: PathArgs = parse(arguments)?;
                serde_json::to_value(
                    sftp_capability::stat(self.sessions.clone(), &args.session_id, &args.path)
                        .await
                        .map_err(map_error)?,
                )
                .map_err(internal_error)
            }
            tool::SFTP_READ_TEXT => {
                let args: SftpReadTextArgs = parse(arguments)?;
                let max = args.max_bytes.unwrap_or(MAX_TEXT_READ_BYTES);
                if max == 0 || max > MAX_TEXT_READ_BYTES {
                    return Err(failure(
                        "invalid_argument",
                        "maxBytes must be between 1 and 65536.",
                    ));
                }
                serde_json::to_value(
                    sftp_capability::read_text(
                        self.sessions.clone(),
                        &args.session_id,
                        &args.path,
                        max,
                    )
                    .await
                    .map_err(map_error)?,
                )
                .map_err(internal_error)
            }
            tool::SFTP_WRITE_TEXT => {
                let args: SftpWriteTextArgs = parse(arguments)?;
                if args.content.len() > MAX_TEXT_WRITE_BYTES {
                    return Err(failure(
                        "invalid_argument",
                        "content exceeds the 1 MiB limit.",
                    ));
                }
                let value = sftp_capability::write_text(
                    self.sessions.clone(),
                    &args.session_id,
                    &args.path,
                    &args.content,
                    args.expected_mtime,
                    args.expected_size,
                    args.expected_hash.as_deref(),
                    args.force.unwrap_or(false),
                )
                .await
                .map_err(map_error)?;
                if value.status == "conflict" {
                    return Err(failure(
                        "conflict",
                        &serde_json::to_string(&value)
                            .unwrap_or_else(|_| "Remote file changed.".into()),
                    ));
                }
                serde_json::to_value(value).map_err(internal_error)
            }
            tool::SFTP_MKDIR => {
                let args: SftpMkdirArgs = parse(arguments)?;
                sftp_capability::mkdir(
                    self.sessions.clone(),
                    &args.session_id,
                    &args.path,
                    args.mode,
                )
                .await
                .map_err(map_error)?;
                Ok(json!({ "created": true }))
            }
            tool::SFTP_RENAME => {
                let args: SftpRenameArgs = parse(arguments)?;
                sftp_capability::rename(
                    self.sessions.clone(),
                    &args.session_id,
                    &args.old_path,
                    &args.new_path,
                )
                .await
                .map_err(map_error)?;
                Ok(json!({ "renamed": true }))
            }
            tool::SFTP_DELETE => {
                let args: PathArgs = parse(arguments)?;
                sftp_capability::delete(self.sessions.clone(), &args.session_id, &args.path)
                    .await
                    .map_err(map_error)?;
                Ok(json!({ "deleted": true }))
            }
            tool::SFTP_CHMOD => {
                let args: SftpChmodArgs = parse(arguments)?;
                sftp_capability::chmod(
                    self.sessions.clone(),
                    &args.session_id,
                    &args.path,
                    &args.mode,
                )
                .await
                .map_err(map_error)?;
                Ok(json!({ "changed": true }))
            }
            _ => Err(failure("invalid_argument", "Unknown NyaTerm MCP tool.")),
        }
    }

    async fn authenticate(&self, auth: &AuthParams) -> Option<Arc<Credential>> {
        let credential = self
            .credentials
            .read()
            .await
            .get(&auth.generation)
            .cloned()?;
        (!credential.cancellation.is_cancelled()
            && constant_time_eq(credential.token.as_bytes(), auth.token.as_bytes()))
        .then_some(credential)
    }

    async fn temporary_idle_worker(
        self: Arc<Self>,
        generation: String,
        minutes: u16,
        last_activity: Arc<StdMutex<Instant>>,
        approval_waiters: Arc<AtomicUsize>,
        cancellation: CancellationToken,
    ) {
        let timeout = Duration::from_secs(u64::from(minutes) * 60);
        loop {
            tokio::select! { _ = cancellation.cancelled() => return, _ = tokio::time::sleep(Duration::from_secs(5)) => {} }
            if approval_waiters.load(Ordering::SeqCst) > 0 {
                continue;
            }
            if last_activity.lock().unwrap().elapsed() >= timeout {
                if self
                    .external
                    .lock()
                    .await
                    .as_ref()
                    .is_some_and(|state| state.generation == generation)
                {
                    let _ = self.disable_external(true).await;
                }
                return;
            }
        }
    }

    async fn touch_external(&self, generation: &str) {
        if let Some(state) = self.external.lock().await.as_ref() {
            if state.generation == generation {
                *state.last_activity.lock().unwrap() = Instant::now();
            }
        }
    }

    async fn external_waiter(&self, generation: &str) -> Option<Arc<AtomicUsize>> {
        self.external
            .lock()
            .await
            .as_ref()
            .filter(|state| state.generation == generation)
            .map(|state| state.approval_waiters.clone())
    }

    #[allow(clippy::too_many_arguments)]
    fn audit(
        &self,
        context: &ConnectionContext,
        capability: &str,
        session_id: Option<&str>,
        risk: Option<RiskLevel>,
        approval: Option<&str>,
        success: bool,
        duration: Duration,
        error: Option<&str>,
        tool_name: &str,
        arguments: &Value,
    ) {
        let Some(app) = self.app.get() else { return };
        let command = (tool_name == tool::TERMINAL_EXECUTE)
            .then(|| {
                arguments
                    .get("command")
                    .and_then(Value::as_str)
                    .map(redact_sensitive_text)
            })
            .flatten();
        let error = error.map(|text| redact_sensitive_text(text).chars().take(512).collect());
        let _ = append_ai_audit(
            app,
            AppendAiAuditRequest {
                connection_id: Some(context.id.clone()),
                action: "mcp_capability".into(),
                user_input: None,
                generated_command: command,
                risk_level: risk,
                inserted_to_terminal: tool_name == tool::TERMINAL_EXECUTE && success,
                executed: success,
                blocked: !success,
                source: Some(context.credential.source.clone()),
                client: Some(context.client.lock().unwrap().clone()),
                capability: Some(capability.into()),
                session_id: session_id.map(str::to_string),
                permission_mode: Some(context.credential.permission_mode.clone()),
                approval_decision: approval.map(str::to_string),
                success: Some(success),
                duration_ms: Some(duration.as_millis() as u64),
                error,
            },
        );
    }

    fn set_error(&self, value: Option<String>) {
        *self.last_error.lock().unwrap() = value;
    }

    fn emit_status(&self) {
        let Some(app) = self.app.get().cloned() else {
            return;
        };
        let manager = app
            .try_state::<Arc<McpManager>>()
            .map(|state| state.inner().clone());
        if let Some(manager) = manager {
            tauri::async_runtime::spawn(async move {
                let _ = app.emit("mcp-status-changed", manager.status().await);
            });
        }
    }
}

fn random_token() -> String {
    let mut bytes = [0_u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    Sha256::digest(a)
        .iter()
        .zip(Sha256::digest(b).iter())
        .fold(0_u8, |value, (a, b)| value | (a ^ b))
        == 0
}
fn permission_mode_name(value: &AiPermissionMode) -> &'static str {
    match value {
        AiPermissionMode::Observer => "observer",
        AiPermissionMode::Confirm => "confirm",
        AiPermissionMode::Auto => "auto",
    }
}
fn session_type_name(value: &SessionType) -> &'static str {
    match value {
        SessionType::SSH => "ssh",
        SessionType::Local => "local",
        SessionType::Telnet => "telnet",
        SessionType::Serial => "serial",
    }
}
fn execution_profile_name(value: AiExecutionProfile) -> &'static str {
    match value {
        AiExecutionProfile::Disabled => "disabled",
        AiExecutionProfile::Auto | AiExecutionProfile::SendOnly => "send_only",
        _ => "capture",
    }
}
fn sftp_available(info: &SessionInfo) -> bool {
    sftp_capability::is_available(info)
}
fn safe_metadata(info: &SessionInfo) -> Value {
    json!({ "id": info.id, "name": info.name, "type": session_type_name(&info.session_type), "connected": info.connected })
}
fn access_risk(value: CapabilityAccess) -> RiskLevel {
    match value {
        CapabilityAccess::Read => RiskLevel::Low,
        CapabilityAccess::SensitiveRead | CapabilityAccess::Write => RiskLevel::Medium,
        CapabilityAccess::DestructiveWrite => RiskLevel::High,
    }
}
fn summarize(name: &str, args: &Value) -> String {
    let value = match name {
        tool::TERMINAL_EXECUTE => args
            .get("command")
            .and_then(Value::as_str)
            .map(redact_sensitive_text)
            .unwrap_or_else(|| "terminal command".into()),
        tool::SFTP_WRITE_TEXT => format!(
            "path={}",
            args.get("path").and_then(Value::as_str).unwrap_or("")
        ),
        _ => redact_sensitive_text(&serde_json::to_string(args).unwrap_or_default()),
    };
    value.chars().take(512).collect()
}
fn parse<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, RpcError> {
    serde_json::from_value(value).map_err(|error| failure("invalid_argument", &error.to_string()))
}
fn map_error(error: AppError) -> RpcError {
    match error {
        AppError::Cancelled(message) => failure("cancelled", &message),
        AppError::Config(message) if message.contains("MCP scope") => {
            failure("scope_denied", &message)
        }
        AppError::Config(message) if message.contains("Output is unavailable") => {
            failure("output_not_found", &message)
        }
        AppError::Config(message)
            if message == "SFTP is not available for this session."
                || message == "Terminal command execution is disabled for this session." =>
        {
            failure("permission_denied", &message)
        }
        AppError::SessionNotFound(message) | AppError::Config(message) => {
            failure("invalid_argument", &message)
        }
        other => failure("execution_failed", &other.to_string()),
    }
}
fn internal_error(error: serde_json::Error) -> RpcError {
    failure("internal_error", &error.to_string())
}
async fn protect(store: &Mutex<OutputStore>, value: Value) -> Result<Value, RpcError> {
    let text = serde_json::to_string(&value).map_err(internal_error)?;
    if text.len() <= MAX_INLINE_OUTPUT_BYTES {
        return Ok(value);
    }
    serde_json::to_value(store.lock().await.protect(text, MAX_INLINE_OUTPUT_BYTES))
        .map_err(internal_error)
}
async fn read_rpc_line<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
) -> std::io::Result<Option<Vec<u8>>> {
    let mut line = Vec::new();
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            if line.is_empty() {
                return Ok(None);
            }
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "MCP bridge request line is not newline terminated",
            ));
        }
        let take = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |index| index + 1);
        if line.len() + take > MAX_RPC_LINE_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "MCP bridge request line is too large",
            ));
        }
        line.extend_from_slice(&available[..take]);
        reader.consume(take);
        if line.last() == Some(&b'\n') {
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            return Ok(Some(line));
        }
    }
}
async fn write_response<W: tokio::io::AsyncWrite + Unpin>(
    writer: &mut W,
    response: RpcResponse,
) -> std::io::Result<()> {
    let mut bytes = serde_json::to_vec(&response).map_err(std::io::Error::other)?;
    bytes.push(b'\n');
    writer.write_all(&bytes).await
}
fn rpc_ok(id: u64, result: Value) -> RpcResponse {
    RpcResponse {
        id,
        result: Some(result),
        error: None,
    }
}
fn rpc_error(id: u64, code: &str, message: &str) -> RpcResponse {
    RpcResponse {
        id,
        result: None,
        error: Some(failure(code, message)),
    }
}
fn failure(code: &str, message: &str) -> RpcError {
    RpcError {
        code: code.into(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    #[test]
    fn token_is_32_bytes() {
        assert_eq!(URL_SAFE_NO_PAD.decode(random_token()).unwrap().len(), 32);
    }

    #[tokio::test]
    async fn rpc_reader_requires_a_newline_and_enforces_the_limit() {
        let (mut writer, reader) = tokio::io::duplex(MAX_RPC_LINE_BYTES + 16);
        writer.write_all(b"{}").await.unwrap();
        writer.shutdown().await.unwrap();
        let error = read_rpc_line(&mut BufReader::new(reader))
            .await
            .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::UnexpectedEof);

        let (mut writer, reader) = tokio::io::duplex(MAX_RPC_LINE_BYTES + 16);
        let task = tokio::spawn(async move {
            writer
                .write_all(&vec![b'x'; MAX_RPC_LINE_BYTES + 1])
                .await
                .unwrap();
            writer.write_all(b"\n").await.unwrap();
        });
        let error = read_rpc_line(&mut BufReader::new(reader))
            .await
            .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        task.abort();
    }

    #[tokio::test]
    async fn ephemeral_credentials_force_environment_auth_and_cleanup_on_drop() {
        let root = std::env::temp_dir().join(format!("nyaterm-mcp-host-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let sidecar = root.join(if cfg!(windows) {
            "nyaterm-mcp.exe"
        } else {
            "nyaterm-mcp"
        });
        std::fs::write(&sidecar, b"test").unwrap();
        let manager = McpManager::new(Arc::new(SessionManager::new()), &root, &root);
        manager.port.store(12345, Ordering::SeqCst);

        let credential = manager
            .create_ephemeral_credential(
                "test",
                vec!["session-a".into()],
                Some("session-a".into()),
                AiPermissionMode::Confirm,
                None,
            )
            .await
            .unwrap();
        assert_eq!(credential.env().get("NYATERM_MCP_EPHEMERAL").unwrap(), "1");
        assert_eq!(manager.credentials.read().await.len(), 1);
        drop(credential);
        tokio::task::yield_now().await;
        assert!(manager.credentials.read().await.is_empty());

        let configs = manager.client_configs().unwrap();
        let serialized = serde_json::to_string(&configs).unwrap();
        assert!(!serialized.contains("NYATERM_MCP_TOKEN"));
        assert!(!serialized.contains("generation"));
        let _ = std::fs::remove_dir_all(root);
    }
}
