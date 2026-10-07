use crate::{
    auth::Owner,
    error::{Result, WebError},
    session::{self, SessionMetadata, SessionProtocol},
    state::State,
};
use axum::{
    Extension,
    extract::{
        Path, State as ExtractState, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::Response,
};
use nyaterm_core::{
    error::AppError,
    vnc::{
        self, VncInputEvent, VncSessionManager,
        runtime::{FrameChannel, VncContext},
    },
};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    sync::{
        Arc, Mutex as StdMutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::{broadcast, mpsc};
use tracing::Instrument;

pub struct WebVnc {
    pub manager: Arc<VncSessionManager>,
    pub context: VncContext,
    pub pending_keys: Arc<StdMutex<HashSet<String>>>,
    pub state: Arc<StdMutex<String>>,
    events: broadcast::Sender<(String, Value)>,
}
pub async fn create(state: Arc<State>, owner: &str, args: Value) -> Result<String> {
    let (mut conn, saved) = session::connection(&args, "vnc")?;
    nyaterm_core::services::validate_vnc_config(&conn)?;
    let login = state.login(owner).await?;
    let cancel = login.cancel.child_token();
    // Only persisted secrets need decryption. Encrypt transient input through the same path.
    if !saved {
        if let Some(auth) = &mut conn.auth {
            if let Some(password) = &auth.password {
                auth.password = Some(nyaterm_core::utils::crypto::encrypt(password)?);
            }
        }
    }
    let mut config = vnc::config_from_connection(&(), conn, "main".into())?;
    let pending = Arc::new(StdMutex::new(HashSet::new()));
    let latest_state = Arc::new(StdMutex::new("connecting".into()));
    let (events, _) = broadcast::channel(64);
    let event_state = state.clone();
    let event_owner = owner.to_owned();
    let event_pending = pending.clone();
    let event_sender = events.clone();
    let event_latest_state = latest_state.clone();
    let transport_state = state.clone();
    let transport_owner = owner.to_owned();
    let transport_cancel = cancel.clone();
    let event_span = tracing::Span::current();
    let transport_span = tracing::Span::current();
    let context = VncContext {
        events: Arc::new(move |_, event, payload| {
            let _entered = event_span.enter();
            if event.starts_with("vnc-state-") {
                if let Some(state) = payload["state"].as_str() {
                    *event_latest_state.lock().unwrap() = state.into();
                    tracing::info!(
                        event = "vnc.state_changed",
                        session_id = event.trim_start_matches("vnc-state-"),
                        reason = state,
                        "VNC state changed"
                    );
                }
            }
            if let Some(id) = payload["requestId"].as_str() {
                let mut pending = event_pending.lock().unwrap();
                if event == "vnc-server-key-verify" {
                    pending.insert(id.into());
                } else if event == "vnc-server-key-verify-resolved" {
                    pending.remove(id);
                }
            }
            if event.starts_with("vnc-state-") || event.starts_with("vnc-clipboard-") {
                let _ = event_sender.send((event.to_owned(), payload));
                return Ok(());
            }
            let state = event_state.clone();
            let owner = event_owner.clone();
            let event = event.to_owned();
            crate::observability::spawn(async move {
                state.event(&owner, &event, payload).await;
            });
            Ok(())
        }),
        transport: Arc::new(move |host, port, network, _| {
            let state = transport_state.clone();
            let owner = transport_owner.clone();
            let cancel = transport_cancel.clone();
            let span = transport_span.clone();
            Box::pin(
                async move {
                    crate::network::open(state, owner, cancel, host, port, network)
                        .await
                        .map_err(|e| AppError::Channel(e.1))
                }
                .instrument(span),
            )
        }),
    };
    let web = Arc::new(WebVnc {
        manager: Arc::new(VncSessionManager::new()),
        context,
        pending_keys: pending,
        state: latest_state,
        events,
    });
    let metadata = SessionMetadata {
        host: config.host.clone(),
        port: config.port,
        username: config.username.clone(),
        name: config.name.clone(),
        connection_id: saved.then_some(config.connection_id.clone()),
        sftp: None,
    };
    let (session, _, _) = session::register(
        &state,
        owner,
        &args,
        SessionProtocol::Vnc(web.clone()),
        metadata,
        cancel,
    )
    .await?;
    config.session_id = session.id.clone();
    if let Err(e) = web
        .manager
        .create_session(web.context.clone(), config)
        .await
    {
        state.sessions.lock().await.remove(&session.id);
        return Err(e.into());
    }
    session.ready.store(true, Ordering::Release);
    if let Some(id) = &session.connection_id {
        nyaterm_core::storage::mark_connection_used(id)?;
    }
    let id = session.id.clone();
    crate::observability::spawn(async move {
        session.cancel.cancelled().await;
        let _ = web.manager.close(&web.context, &session.id).await;
        session::finish(&state, &session).await;
    });
    Ok(id)
}
pub async fn command(
    state: &Arc<State>,
    owner: &str,
    command: &str,
    args: &Value,
) -> Result<Value> {
    if command == "respond_vnc_server_key" {
        let id = args["requestId"]
            .as_str()
            .ok_or(WebError::bad("Missing key request id"))?;
        let sessions = state
            .sessions
            .lock()
            .await
            .values()
            .filter(|s| s.owner == owner)
            .cloned()
            .collect::<Vec<_>>();
        for session in sessions {
            if let SessionProtocol::Vnc(web) = &session.protocol {
                if web.pending_keys.lock().unwrap().contains(id) {
                    web.manager
                        .respond_server_key(
                            id,
                            args["accepted"]
                                .as_bool()
                                .ok_or(WebError::bad("Missing key response"))?,
                        )
                        .await?;
                    return Ok(Value::Null);
                }
            }
        }
        return Err(WebError::forbidden());
    }
    let session = state
        .session(
            owner,
            args["sessionId"]
                .as_str()
                .ok_or(WebError::bad("Missing session id"))?,
        )
        .await?;
    let SessionProtocol::Vnc(web) = &session.protocol else {
        return Err(WebError::bad("Not a VNC session"));
    };
    match command {
        "vnc_input_batch" => {
            web.manager
                .send_input(&session.id, serde_json::from_value(args["events"].clone())?)
                .await?
        }
        "vnc_set_clipboard_text" => {
            web.manager
                .set_clipboard_text(
                    &session.id,
                    args["text"]
                        .as_str()
                        .ok_or(WebError::bad("Missing clipboard text"))?
                        .into(),
                )
                .await?
        }
        "vnc_reconnect" => {
            web.manager
                .reconnect(web.context.clone(), &session.id)
                .await?
        }
        "close_vnc_session" => session.cancel.cancel(),
        _ => return Err(WebError::unsupported()),
    }
    Ok(Value::Null)
}
pub async fn ws_route(
    ExtractState(state): ExtractState<Arc<State>>,
    Extension(owner): Extension<Owner>,
    Path(id): Path<String>,
    upgrade: WebSocketUpgrade,
) -> Result<Response> {
    let session = state.session(&owner.0, &id).await?;
    if !matches!(session.protocol, SessionProtocol::Vnc(_)) {
        return Err(WebError::bad("Not a VNC session"));
    }
    if session
        .attached
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err(WebError(
            axum::http::StatusCode::CONFLICT,
            "VNC already attached".into(),
        ));
    }
    Ok(upgrade
        .max_message_size(2 * 1024 * 1024)
        .max_frame_size(2 * 1024 * 1024)
        .max_write_buffer_size(140 * 1024 * 1024)
        .on_failed_upgrade({
            let session = session.clone();
            move |_| {
                session.attached.store(false, Ordering::Release);
            }
        })
        .on_upgrade({
            let span = tracing::Span::current();
            let guard = crate::observability::StreamLogGuard::new(
                "vnc.disconnected",
                Some(session.id.clone()),
            );
            move |socket| {
                async move {
                let _guard = guard;
                tracing::info!(event="vnc.attached", session_id=%session.id, "Web stream attached");
                socket_loop(session, socket).await;
            }.instrument(span)
            }
        }))
}
async fn socket_loop(session: Arc<session::WebSession>, mut socket: WebSocket) {
    let SessionProtocol::Vnc(web) = &session.protocol else {
        return;
    };
    let (tx, mut frames) = mpsc::channel(2);
    let overflow = Arc::new(AtomicBool::new(false));
    let overflow_sink = overflow.clone();
    let sink = FrameChannel(Arc::new(move |frame| {
        tx.try_send(frame).map_err(|_| {
            overflow_sink.store(true, Ordering::Release);
            AppError::Channel("VNC consumer needs a full refresh".into())
        })
    }));
    let mut events = web.events.subscribe();
    let _ = web
        .manager
        .attach_frame_channel(&web.context, &session.id, sink.clone())
        .await;
    let mut heartbeat = tokio::time::interval(Duration::from_secs(15));
    let mut refresh = tokio::time::interval(Duration::from_millis(32));
    let mut last_peer = Instant::now();
    loop {
        let message = tokio::select! {
            _ = session.cancel.cancelled() => break,
            _ = refresh.tick() => {
                if overflow.swap(false, Ordering::AcqRel) {
                    while frames.try_recv().is_ok() {}
                    let _ = web.manager.attach_frame_channel(&web.context, &session.id, sink.clone()).await;
                }
                continue;
            },
            frame = frames.recv() => match frame { Some(frame) => Message::Binary(frame.into()), None => break },
            event = events.recv() => match event {
                Ok((event, payload)) => Message::Text(json!({"type":"event","event":event,"payload":payload}).to_string().into()),
                Err(broadcast::error::RecvError::Lagged(_)) => { let _ = web.manager.attach_frame_channel(&web.context, &session.id, sink.clone()).await; continue; },
                Err(_) => break,
            },
            next = socket.recv() => {
                match next {
                    Some(Ok(Message::Text(text))) => {
                        last_peer = Instant::now();
                        let result = async {
                            let value: Value = serde_json::from_str(&text)?;
                            match value["type"].as_str() {
                                Some("input") => web.manager.send_input(&session.id, serde_json::from_value::<Vec<VncInputEvent>>(value["events"].clone())?).await?,
                                Some("clipboard") => web.manager.set_clipboard_text(&session.id, value["text"].as_str().ok_or(WebError::bad("Missing clipboard"))?.into()).await?,
                                Some("close") => session.cancel.cancel(),
                                _ => return Err(WebError::bad("Invalid VNC message")),
                            }
                            Ok::<(), WebError>(())
                        }.await;
                        if let Err(e) = result { Message::Text(json!({"type":"error","error":e.1}).to_string().into()) } else { continue; }
                    },
                    Some(Ok(Message::Ping(data))) => { last_peer = Instant::now(); Message::Pong(data) },
                    Some(Ok(Message::Pong(_))) => { last_peer = Instant::now(); continue; },
                    _ => break,
                }
            },
            _ = heartbeat.tick() => { if last_peer.elapsed() > Duration::from_secs(45) { break; } Message::Ping(Vec::new().into()) },
        };
        if !matches!(
            tokio::time::timeout(Duration::from_secs(5), socket.send(message)).await,
            Ok(Ok(()))
        ) {
            break;
        }
    }
    let _ = web.manager.detach_frame_channel(&session.id).await;
    *session.detached_at.lock().unwrap() = Instant::now();
    session.attached.store(false, Ordering::Release);
    let _ = socket.send(Message::Close(None)).await;
}
