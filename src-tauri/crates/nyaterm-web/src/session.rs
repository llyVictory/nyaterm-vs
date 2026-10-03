use crate::{
    auth::Owner,
    error::{Result, WebError},
    state::State,
};
use axum::{
    Extension, Json,
    extract::{
        Path, State as ExtractState, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::Response,
};
use nyaterm_core::{
    config,
    ssh::{
        algorithms, protocol,
        terminal::{self, Command, Output},
    },
    storage,
    utils::crypto,
};
use russh::{client, keys::PublicKeyBase64};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc, Mutex as StdMutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, OwnedMutexGuard, mpsc};
use tokio_util::sync::CancellationToken;

pub struct Handler {
    state: Arc<State>,
    owner: String,
    host: String,
    port: u16,
    cancel: CancellationToken,
}
impl client::Handler for Handler {
    type Error = russh::Error;
    async fn check_server_key(
        &mut self,
        key: &russh::keys::PublicKey,
    ) -> std::result::Result<bool, Self::Error> {
        let host_id = if self.port == 22 {
            self.host.clone()
        } else {
            format!("[{}]:{}", self.host, self.port)
        };
        let key_type = key.algorithm().to_string();
        let encoded = key.public_key_base64();
        let known = storage::check_known_host(&host_id, &key_type, &encoded)
            .map_err(|_| russh::Error::UnknownKey)?;
        if matches!(known, storage::KnownHostCheck::Match) {
            return Ok(true);
        }
        let reply = self
            .state
            .prompt(
                &self.owner,
                "host-key-verify",
                json!({
                    "host":self.host,"port":self.port,"keyType":key_type,
                    "fingerprint":key.fingerprint(russh::keys::HashAlg::Sha256).to_string(),
                    "isKeyChanged":matches!(known,storage::KnownHostCheck::HostSeen),
                }),
                &self.cancel,
            )
            .await
            .map_err(|_| russh::Error::UnknownKey)?;
        if reply.as_bool() != Some(true) {
            return Ok(false);
        }
        storage::replace_known_host_for_host(&host_id, &format!("{host_id} {key_type} {encoded}"))
            .map_err(|_| russh::Error::UnknownKey)?;
        Ok(true)
    }
}
pub struct WebSession {
    pub id: String,
    pub owner: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub connection_id: Option<String>,
    pub request_id: Option<String>,
    pub cancel: CancellationToken,
    pub handle: Mutex<Option<client::Handle<Handler>>>,
    pub input: mpsc::Sender<Command>,
    pub output: Arc<Mutex<mpsc::Receiver<Output>>>,
    pub attached: AtomicBool,
    pub detached_at: StdMutex<Instant>,
    pub ready: AtomicBool,
    pub transfers: Arc<tokio::sync::Semaphore>,
    pub sftp: config::SftpSettings,
}
impl WebSession {
    pub fn info(&self) -> Value {
        json!({"id":self.id,"name":self.name,"session_type":"SSH","host":self.host,"port":self.port,"username":self.username,"connection_id":self.connection_id,"owner_window_label":"main","terminal_available":true,"shell_available":true,"sftp_available":self.sftp.enabled && self.ready.load(Ordering::Acquire),"runtime_mode":"standard","dynamic_title_enabled":false,"dynamic_title_integration_active":false})
    }
}

struct Target {
    host: String,
    port: u16,
    username: String,
    name: String,
    mode: String,
    secret: Option<zeroize::Zeroizing<String>>,
    key: Option<String>,
    passphrase: Option<zeroize::Zeroizing<String>>,
    preferences: Option<config::SshAlgorithmPreferences>,
    connection_id: Option<String>,
    terminal_type: config::SshTerminalType,
    sftp: config::SftpSettings,
}
fn require_utf8(encoding: &str) -> Result<()> {
    let effective = if encoding.is_empty() || encoding.eq_ignore_ascii_case("global") {
        config::load_app_settings(&())?.interaction.default_encoding
    } else {
        encoding.to_owned()
    };
    if !matches!(effective.to_lowercase().as_str(), "utf-8" | "utf8") {
        return Err(WebError::bad("Web SSH requires UTF-8 terminal encoding"));
    }
    Ok(())
}
fn validate_sftp(settings: &config::SftpSettings) -> Result<()> {
    if settings.enabled {
        if settings.compatibility_mode {
            return Err(WebError::unsupported());
        }
        if !settings.filename_encoding.is_empty() {
            require_utf8(&settings.filename_encoding)?;
        }
    }
    Ok(())
}
fn target(args: &Value) -> Result<Target> {
    if let Some(id) = args["connectionId"].as_str() {
        let conn = config::load_connection_by_id(&(), id)?;
        let config::ConnectionType::Ssh {
            host,
            port,
            username,
            x11_forwarding,
            agent_forwarding_config,
            encoding,
            ..
        } = &conn.config
        else {
            return Err(WebError::unsupported());
        };
        if *x11_forwarding || agent_forwarding_config.as_ref().is_some_and(|c| c.enabled) {
            return Err(WebError::unsupported());
        }
        require_utf8(encoding)?;
        validate_sftp(&conn.sftp)?;
        if conn.post_login.as_ref().is_some_and(|p| p.enabled)
            || conn.ssh_profile != config::SshProfile::Standard
        {
            return Err(WebError::unsupported());
        }
        let terminal_type =
            config::resolve_ssh_terminal_type(&conn.ssh_profile, conn.terminal_type.as_ref());
        // Unsupported network paths must not silently become direct connections.
        if conn
            .network
            .as_ref()
            .is_some_and(|n| n.proxy_id.is_some() || n.proxy_jump_id.is_some())
        {
            return Err(WebError::unsupported());
        }
        let account = conn
            .auth
            .as_ref()
            .map(|a| {
                config::load_saved_account(&(), a.account_id.as_deref(), a.password_id.as_deref())
            })
            .transpose()?
            .flatten();
        let username = config::resolve_account_username(account.as_ref(), username);
        let mut secret = None;
        let mut key = None;
        let mut passphrase = None;
        let mode = conn
            .auth
            .as_ref()
            .map(|a| a.mode.clone())
            .unwrap_or_else(|| "password".into());
        if let Some(auth) = &conn.auth {
            secret = match &auth.password {
                Some(ciphertext) => Some(crypto::decrypt(ciphertext)?),
                None if auth.password_source.as_deref() != Some("connection") => {
                    config::decrypt_account_password(account.as_ref())?
                }
                _ => None,
            };
            if let Some(key_id) = &auth.key_id {
                let saved = config::load_key_by_id(&(), key_id)?;
                if saved.cert.is_some() {
                    return Err(WebError::unsupported());
                }
                key = saved.key.as_deref().map(crypto::decrypt).transpose()?;
                passphrase = saved.passphrase;
            }
        }
        Ok(Target {
            host: host.clone(),
            port: *port,
            username,
            name: conn.name,
            mode,
            secret: secret.map(zeroize::Zeroizing::new),
            key,
            passphrase: passphrase.map(zeroize::Zeroizing::new),
            preferences: conn.ssh_algorithms,
            connection_id: Some(id.into()),
            terminal_type,
            sftp: conn.sftp,
        })
    } else {
        let value = &args["config"];
        if !value["proxy"].is_null()
            || !value["proxy_jump"].is_null()
            || value["x11_forwarding"].as_bool() == Some(true)
        {
            return Err(WebError::unsupported());
        }
        if value["agent_forwarding_config"]["enabled"].as_bool() == Some(true)
            || !value["auth"]["cert_data"].is_null()
        {
            return Err(WebError::unsupported());
        }
        require_utf8(value["encoding"].as_str().unwrap_or(""))?;
        let sftp: config::SftpSettings = value
            .get("sftp")
            .filter(|v| !v.is_null())
            .map(|v| serde_json::from_value(v.clone()))
            .transpose()?
            .unwrap_or_default();
        validate_sftp(&sftp)?;
        if value["ssh_profile"]
            .as_str()
            .is_some_and(|p| p != "standard")
        {
            return Err(WebError::unsupported());
        }
        let terminal_type = value
            .get("terminal_type")
            .filter(|v| !v.is_null())
            .map(|v| serde_json::from_value(v.clone()))
            .transpose()?
            .unwrap_or(config::SshTerminalType::Xterm256Color);
        let port = match value["port"].as_u64() {
            Some(p) => u16::try_from(p)
                .ok()
                .filter(|p| *p > 0)
                .ok_or(WebError::bad("Invalid SSH port"))?,
            None if value["port"].is_null() => 22,
            _ => return Err(WebError::bad("Invalid SSH port")),
        };
        Ok(Target {
            host: value["host"]
                .as_str()
                .ok_or(WebError::bad("SSH host required"))?
                .into(),
            port,
            username: value["username"].as_str().unwrap_or("root").into(),
            name: value["name"].as_str().unwrap_or("SSH").into(),
            mode: value["auth"]["type"].as_str().unwrap_or("password").into(),
            secret: value["auth"]["password"]
                .as_str()
                .map(|v| zeroize::Zeroizing::new(v.into())),
            key: value["auth"]["key_data"].as_str().map(String::from),
            passphrase: value["auth"]["passphrase"]
                .as_str()
                .map(|v| zeroize::Zeroizing::new(v.into())),
            preferences: value
                .get("ssh_algorithms")
                .filter(|v| !v.is_null())
                .map(|v| serde_json::from_value(v.clone()))
                .transpose()?,
            connection_id: None,
            terminal_type,
            sftp,
        })
    }
}
pub async fn create(state: Arc<State>, owner: &str, args: Value) -> Result<String> {
    if args["startupCommand"].as_object().is_some()
        || args["runtimeMode"]
            .as_str()
            .is_some_and(|m| m != "standard")
        || args["config"]["post_login"]["enabled"].as_bool() == Some(true)
    {
        return Err(WebError::unsupported());
    }
    let target = target(&args)?;
    if !matches!(target.mode.as_str(), "none" | "password" | "key") {
        return Err(WebError::unsupported());
    }
    if target.host.is_empty()
        || target.host.len() > 253
        || target
            .host
            .chars()
            .any(|c| c.is_whitespace() || c.is_control())
        || target.port == 0
        || target.username.is_empty()
        || target.username.len() > 256
        || target.username.contains('\0')
    {
        return Err(WebError::bad("Invalid SSH target"));
    }
    let login = state.login(owner).await?;
    let (input, commands) = mpsc::channel(128);
    let (sender, output) = mpsc::channel(terminal::OUTPUT_CAPACITY);
    let id = uuid::Uuid::new_v4().to_string();
    let session = Arc::new(WebSession {
        id: id.clone(),
        owner: owner.into(),
        name: target.name.clone(),
        host: target.host.clone(),
        port: target.port,
        username: target.username.clone(),
        connection_id: target.connection_id.clone(),
        request_id: args["createRequestId"].as_str().map(String::from),
        cancel: login.cancel.child_token(),
        handle: Mutex::new(None),
        input,
        output: Arc::new(Mutex::new(output)),
        attached: AtomicBool::new(false),
        detached_at: StdMutex::new(Instant::now()),
        ready: AtomicBool::new(false),
        transfers: Arc::new(tokio::sync::Semaphore::new(4)),
        sftp: target.sftp.clone(),
    });
    let mut registry = state.sessions.lock().await;
    if registry.len() >= 128 || registry.values().filter(|s| s.owner == owner).count() >= 16 {
        return Err(WebError::bad("Too many sessions"));
    }
    registry.insert(id.clone(), session.clone());
    drop(registry);
    tokio::spawn(async move {
        let result = tokio::select! {
            _ = session.cancel.cancelled() => Err(WebError::bad("Session creation cancelled")),
            result = tokio::time::timeout(Duration::from_secs(120), connect(&state,&session,target)) => result.unwrap_or(Err(WebError::bad("SSH connection timed out"))),
        };
        match result {
            Ok(channel) => {
                session.ready.store(true, Ordering::Release);
                state
                    .event(&session.owner, "sessions-changed", Value::Null)
                    .await;
                terminal::run(channel, commands, sender, session.cancel.clone()).await;
            }
            Err(_) => {
                state
                    .event(
                        &session.owner,
                        &format!("connection-error-{}", session.id),
                        json!("SSH connection failed or authentication was cancelled"),
                    )
                    .await;
                let _ = sender.try_send(Output::Error);
            }
        }
        session.cancel.cancel();
        if let Some(handle) = session.handle.lock().await.take() {
            let _ = tokio::time::timeout(
                Duration::from_secs(2),
                handle.disconnect(russh::Disconnect::ByApplication, "", ""),
            )
            .await;
        }
        state.sessions.lock().await.remove(&session.id);
        // Attached terminals receive ordered closure on the WS after final bytes.
        if !session.attached.load(Ordering::Acquire) {
            state
                .event(
                    &session.owner,
                    &format!("session-closed-{}", session.id),
                    Value::Null,
                )
                .await;
        }
        state
            .event(&session.owner, "sessions-changed", Value::Null)
            .await;
    });
    Ok(id)
}
async fn connect(
    state: &Arc<State>,
    session: &Arc<WebSession>,
    mut target: Target,
) -> Result<russh::Channel<client::Msg>> {
    let client_config = client::Config {
        preferred: algorithms::resolve_preferred_algorithms(target.preferences.as_ref())?,
        keepalive_interval: Some(Duration::from_secs(30)),
        keepalive_max: 3,
        ..Default::default()
    };
    let handler = Handler {
        state: state.clone(),
        owner: session.owner.clone(),
        host: target.host.clone(),
        port: target.port,
        cancel: session.cancel.clone(),
    };
    let handle =
        protocol::connect(Arc::new(client_config), &target.host, target.port, handler).await?;
    let mut locked = session.handle.lock().await;
    *locked = Some(handle);
    let handle = locked.as_mut().unwrap();
    let mut authenticated = false;
    if target.mode == "none" {
        authenticated = handle.authenticate_none(&target.username).await?.success();
    }
    if target.mode == "key" {
        let key_data = zeroize::Zeroizing::new(
            target
                .key
                .take()
                .ok_or(WebError::bad("Private key required"))?,
        );
        let mut decoded = russh::keys::decode_secret_key(
            &key_data,
            target.passphrase.as_deref().map(|p| p.as_str()),
        );
        if decoded.is_err() && target.passphrase.is_none() {
            let response=state.prompt(&session.owner,"ssh-auth-request",json!({"connectionId":target.connection_id,"connectionName":target.name,"host":target.host,"port":target.port,"username":target.username,"reason":"key_passphrase_required","promptKind":"passphrase","availableMethods":["publickey"],"currentAuthMode":"key","attempt":1,"canSave":false}),&session.cancel).await?;
            target.passphrase = response["secret"]
                .as_str()
                .map(|s| zeroize::Zeroizing::new(s.into()));
            decoded = russh::keys::decode_secret_key(
                &key_data,
                target.passphrase.as_deref().map(|p| p.as_str()),
            );
        }
        let key = decoded.map_err(|_| WebError::bad("Private key could not be unlocked"))?;
        let hash = handle.best_supported_rsa_hash().await?.flatten();
        authenticated = handle
            .authenticate_publickey(
                &target.username,
                russh::keys::PrivateKeyWithHashAlg::new(Arc::new(key), hash),
            )
            .await?
            .success();
    }
    if !authenticated && target.mode != "key" {
        for attempt in 1..=3 {
            if target.secret.is_none() {
                let response = state.prompt(&session.owner,"ssh-auth-request",json!({
                    "connectionId":target.connection_id,"connectionName":target.name,"host":target.host,"port":target.port,"username":target.username,
                    "reason":if attempt == 1 {"missing_password"} else {"password_rejected"},"promptKind":"password","availableMethods":["password"],"currentAuthMode":"password","attempt":attempt,"canSave":false,"accountId":null,
                }),&session.cancel).await?;
                target.secret = response["secret"]
                    .as_str()
                    .map(|s| zeroize::Zeroizing::new(s.into()));
            }
            let Some(secret) = &target.secret else {
                return Err(WebError::bad("SSH authentication cancelled"));
            };
            let result = protocol::password(handle, &target.username, secret).await?;
            if result.success() {
                authenticated = true;
                break;
            }
            if let russh::client::AuthResult::Failure {
                remaining_methods, ..
            } = result
            {
                if remaining_methods.contains(&russh::MethodKind::KeyboardInteractive) {
                    authenticated = keyboard_auth(state, session, handle, &target).await?;
                    break;
                }
            }
            target.secret = None;
        }
    }
    if !authenticated && target.mode == "key" {
        authenticated = keyboard_auth(state, session, handle, &target).await?;
    }
    if !authenticated {
        return Err(WebError::bad("SSH authentication failed"));
    }
    let channel =
        protocol::open_shell_with_terminal(handle, 80, 24, target.terminal_type.as_str()).await?;
    if let Some(id) = &target.connection_id {
        storage::mark_connection_used(id)?;
    }
    Ok(channel)
}
async fn keyboard_auth(
    state: &Arc<State>,
    session: &Arc<WebSession>,
    handle: &mut client::Handle<Handler>,
    target: &Target,
) -> Result<bool> {
    let mut step = protocol::keyboard_start(handle, &target.username).await?;
    for round in 1..=8 {
        match step {
            client::KeyboardInteractiveAuthResponse::Success => return Ok(true),
            client::KeyboardInteractiveAuthResponse::Failure { .. } => return Ok(false),
            client::KeyboardInteractiveAuthResponse::InfoRequest {
                name,
                instructions,
                prompts,
            } => {
                if prompts.len() > 16 {
                    return Err(WebError::bad("Too many authentication prompts"));
                }
                let response=state.prompt(&session.owner,"otp-request",json!({"connectionName":target.name,"name":name,"instructions":instructions,"round":round,"prompts":prompts.iter().map(|p|json!({"prompt":p.prompt,"echo":p.echo})).collect::<Vec<_>>(),"otpEntryId":null}),&session.cancel).await?;
                let responses: Vec<String> = serde_json::from_value(response)?;
                if responses.len() != prompts.len() {
                    return Err(WebError::bad("Invalid authentication response"));
                }
                step = protocol::keyboard_respond(handle, responses).await?;
            }
        }
    }
    Err(WebError::bad("Too many authentication rounds"))
}
pub async fn create_route(
    ExtractState(state): ExtractState<Arc<State>>,
    Extension(owner): Extension<Owner>,
    Json(args): Json<Value>,
) -> Result<Json<Value>> {
    Ok(Json(
        json!({"session_id":create(state,&owner.0,args).await?}),
    ))
}
pub async fn ws_route(
    ExtractState(state): ExtractState<Arc<State>>,
    Extension(owner): Extension<Owner>,
    Path(id): Path<String>,
    upgrade: WebSocketUpgrade,
) -> Result<Response> {
    let session = state.session(&owner.0, &id).await?;
    let receiver = session.output.clone().try_lock_owned().map_err(|_| {
        WebError(
            axum::http::StatusCode::CONFLICT,
            "Terminal already attached".into(),
        )
    })?;
    Ok(upgrade
        .max_message_size(64 * 1024)
        .max_frame_size(64 * 1024)
        .max_write_buffer_size(256 * 1024)
        .on_upgrade(move |socket| socket_loop(session, receiver, socket)))
}
struct Attachment(Arc<WebSession>);
impl Drop for Attachment {
    fn drop(&mut self) {
        *self.0.detached_at.lock().unwrap() = Instant::now();
        self.0.attached.store(false, Ordering::Release);
    }
}
async fn socket_loop(
    session: Arc<WebSession>,
    mut output: OwnedMutexGuard<mpsc::Receiver<Output>>,
    mut socket: WebSocket,
) {
    session.attached.store(true, Ordering::Release);
    let _attachment = Attachment(session.clone());
    let mut heartbeat = tokio::time::interval(Duration::from_secs(15));
    let mut last_peer = Instant::now();
    loop {
        tokio::select! {
            biased;
            next = output.recv() => {
                let final_frame = !matches!(&next, Some(Output::Data(_)));
                let message = match next {
                    Some(Output::Data(data)) => Message::Binary(data.into()),
                    Some(Output::Error) => Message::Text(json!({"type":"error","error":"SSH connection failed"}).to_string().into()),
                    _ => Message::Text(json!({"type":"closed"}).to_string().into()),
                };
                if !matches!(tokio::time::timeout(Duration::from_secs(10),socket.send(message)).await,Ok(Ok(()))) { session.cancel.cancel(); break; }
                if final_frame { break; }
            },
            _ = session.cancel.cancelled() => { let _ = tokio::time::timeout(Duration::from_secs(2),socket.send(Message::Text(json!({"type":"closed"}).to_string().into()))).await; break; },
            message = socket.recv() => match message {
                Some(Ok(Message::Text(text))) => {
                    last_peer = Instant::now();
                    let Ok(value) = serde_json::from_str::<Value>(&text) else { break; };
                    let command = match value["type"].as_str() {
                        Some("input") => value["data"].as_str().map(|s| Command::Input(s.as_bytes().to_vec())),
                        Some("resize") => value["cols"].as_u64().zip(value["rows"].as_u64()).filter(|(c,r)| (1..=1000).contains(c)&&(1..=1000).contains(r)).map(|(c,r)| Command::Resize(c as u32,r as u32)),
                        Some("close") => { session.cancel.cancel(); break; },
                        _ => None,
                    };
                    let Some(command) = command else { break; };
                    if session.input.try_send(command).is_err() { break; }
                },
                Some(Ok(Message::Binary(bytes))) => { last_peer=Instant::now(); if session.input.try_send(Command::Input(bytes.to_vec())).is_err() { break; } },
                Some(Ok(Message::Pong(_))) => last_peer=Instant::now(),
                Some(Ok(Message::Ping(data))) => { if !matches!(tokio::time::timeout(Duration::from_secs(2),socket.send(Message::Pong(data))).await,Ok(Ok(()))) { break; } },
                _ => break,
            },
            _ = heartbeat.tick() => {
                if last_peer.elapsed() > Duration::from_secs(45) { break; }
                if !matches!(tokio::time::timeout(Duration::from_secs(2),socket.send(Message::Ping(Vec::new().into()))).await,Ok(Ok(()))) { break; }
            }
        }
    }
}
