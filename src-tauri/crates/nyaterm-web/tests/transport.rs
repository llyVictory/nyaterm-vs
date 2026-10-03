use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use futures_util::{SinkExt, StreamExt};
use nyaterm_web::{auth, state::State};
use russh::{
    Channel, ChannelId, Pty,
    server::{self, Server as _, Session},
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    net::TcpListener,
    sync::{Mutex, mpsc},
};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;

struct Echo {
    resize: mpsc::UnboundedSender<(u32, u32)>,
    channels: HashMap<ChannelId, Channel<server::Msg>>,
    sftp_channels: std::collections::HashSet<ChannelId>,
    files: Arc<std::sync::Mutex<HashMap<String, Vec<u8>>>>,
}
impl Clone for Echo {
    fn clone(&self) -> Self {
        Self {
            resize: self.resize.clone(),
            channels: HashMap::new(),
            sftp_channels: Default::default(),
            files: self.files.clone(),
        }
    }
}
impl server::Server for Echo {
    type Handler = Self;
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self {
        self.clone()
    }
}
impl server::Handler for Echo {
    type Error = russh::Error;
    async fn auth_password(
        &mut self,
        user: &str,
        password: &str,
    ) -> Result<server::Auth, Self::Error> {
        Ok(if user == "test" && password == "ssh-secret" {
            server::Auth::Accept
        } else {
            server::Auth::reject()
        })
    }
    async fn channel_open_session(
        &mut self,
        channel: Channel<server::Msg>,
        reply: server::ChannelOpenHandle,
        _: &mut Session,
    ) -> Result<(), Self::Error> {
        self.channels.insert(channel.id(), channel);
        reply.accept().await;
        Ok(())
    }
    async fn subsystem_request(
        &mut self,
        id: ChannelId,
        name: &str,
        s: &mut Session,
    ) -> Result<(), Self::Error> {
        assert_eq!(name, "sftp");
        self.sftp_channels.insert(id);
        s.channel_success(id)?;
        let channel = self.channels.remove(&id).unwrap();
        russh_sftp::server::run(
            channel.into_stream(),
            MemoryFiles {
                files: self.files.clone(),
                listed: false,
            },
        )
        .await;
        Ok(())
    }
    async fn pty_request(
        &mut self,
        id: ChannelId,
        _: &str,
        _: u32,
        _: u32,
        _: u32,
        _: u32,
        _: &[(Pty, u32)],
        s: &mut Session,
    ) -> Result<(), Self::Error> {
        s.channel_success(id)?;
        Ok(())
    }
    async fn shell_request(&mut self, id: ChannelId, s: &mut Session) -> Result<(), Self::Error> {
        s.channel_success(id)?;
        Ok(())
    }
    async fn window_change_request(
        &mut self,
        _: ChannelId,
        cols: u32,
        rows: u32,
        _: u32,
        _: u32,
        _: &mut Session,
    ) -> Result<(), Self::Error> {
        let _ = self.resize.send((cols, rows));
        Ok(())
    }
    async fn data(
        &mut self,
        id: ChannelId,
        data: &[u8],
        s: &mut Session,
    ) -> Result<(), Self::Error> {
        if self.sftp_channels.contains(&id) {
            return Ok(());
        }
        if data == b"large-and-close" {
            let h = s.handle();
            tokio::spawn(async move {
                let bytes = vec![b'x'; 3 * 1024 * 1024];
                for chunk in bytes.chunks(32768) {
                    h.data(id, chunk.to_vec()).await.unwrap();
                }
                h.data(id, b"final-tail".to_vec()).await.unwrap();
                h.eof(id).await.unwrap();
                h.close(id).await.unwrap();
            });
        } else {
            s.data(id, data.to_vec())?;
        }
        Ok(())
    }
}
use russh_sftp::protocol::{self as sf, StatusCode as SfCode};
struct MemoryFiles {
    files: Arc<std::sync::Mutex<HashMap<String, Vec<u8>>>>,
    listed: bool,
}
fn success(id: u32) -> sf::Status {
    sf::Status {
        id,
        status_code: SfCode::Ok,
        error_message: String::new(),
        language_tag: String::new(),
    }
}
fn attrs(size: usize) -> sf::FileAttributes {
    sf::FileAttributes {
        size: Some(size as u64),
        permissions: Some(0o100644),
        uid: Some(1000),
        gid: Some(1000),
        mtime: Some(1700000000),
        atime: Some(1700000000),
        ..Default::default()
    }
}
impl russh_sftp::server::Handler for MemoryFiles {
    type Error = SfCode;
    fn unimplemented(&self) -> SfCode {
        SfCode::OpUnsupported
    }
    async fn open(
        &mut self,
        id: u32,
        filename: String,
        pflags: sf::OpenFlags,
        _: sf::FileAttributes,
    ) -> Result<sf::Handle, SfCode> {
        let mut files = self.files.lock().unwrap();
        if pflags.contains(sf::OpenFlags::CREATE) {
            files.entry(filename.clone()).or_default();
        }
        if !files.contains_key(&filename) {
            return Err(SfCode::NoSuchFile);
        }
        if pflags.contains(sf::OpenFlags::TRUNCATE) {
            files.insert(filename.clone(), vec![]);
        }
        Ok(sf::Handle {
            id,
            handle: filename,
        })
    }
    async fn close(&mut self, id: u32, _: String) -> Result<sf::Status, SfCode> {
        Ok(success(id))
    }
    async fn read(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        len: u32,
    ) -> Result<sf::Data, SfCode> {
        let files = self.files.lock().unwrap();
        let file = files.get(&handle).ok_or(SfCode::NoSuchFile)?;
        let offset = offset as usize;
        if offset >= file.len() {
            return Err(SfCode::Eof);
        }
        Ok(sf::Data {
            id,
            data: file[offset..(offset + len as usize).min(file.len())].to_vec(),
        })
    }
    async fn write(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<sf::Status, SfCode> {
        let mut files = self.files.lock().unwrap();
        let file = files.get_mut(&handle).ok_or(SfCode::NoSuchFile)?;
        let end = offset as usize + data.len();
        file.resize(file.len().max(end), 0);
        file[offset as usize..end].copy_from_slice(&data);
        Ok(success(id))
    }
    async fn stat(&mut self, id: u32, path: String) -> Result<sf::Attrs, SfCode> {
        let files = self.files.lock().unwrap();
        let file = files.get(&path).ok_or(SfCode::NoSuchFile)?;
        Ok(sf::Attrs {
            id,
            attrs: attrs(file.len()),
        })
    }
    async fn lstat(&mut self, id: u32, path: String) -> Result<sf::Attrs, SfCode> {
        self.stat(id, path).await
    }
    async fn fstat(&mut self, id: u32, path: String) -> Result<sf::Attrs, SfCode> {
        self.stat(id, path).await
    }
    async fn rename(&mut self, id: u32, old: String, new: String) -> Result<sf::Status, SfCode> {
        let mut files = self.files.lock().unwrap();
        let bytes = files.remove(&old).ok_or(SfCode::NoSuchFile)?;
        files.insert(new, bytes);
        Ok(success(id))
    }
    async fn remove(&mut self, id: u32, path: String) -> Result<sf::Status, SfCode> {
        self.files.lock().unwrap().remove(&path);
        Ok(success(id))
    }
    async fn realpath(&mut self, id: u32, _: String) -> Result<sf::Name, SfCode> {
        Ok(sf::Name {
            id,
            files: vec![sf::File::dummy("/")],
        })
    }
    async fn opendir(&mut self, id: u32, _: String) -> Result<sf::Handle, SfCode> {
        self.listed = false;
        Ok(sf::Handle {
            id,
            handle: "directory".into(),
        })
    }
    async fn readdir(&mut self, id: u32, _: String) -> Result<sf::Name, SfCode> {
        if self.listed {
            return Err(SfCode::Eof);
        }
        self.listed = true;
        Ok(sf::Name {
            id,
            files: self
                .files
                .lock()
                .unwrap()
                .iter()
                .map(|(name, bytes)| {
                    sf::File::new(name.trim_start_matches('/'), attrs(bytes.len()))
                })
                .collect(),
        })
    }
}
fn state(host: String, base_path: &str) -> Arc<State> {
    Arc::new(State {
        origin: format!("http://{host}"),
        host,
        base_path: base_path.into(),
        secure_cookie: false,
        password_hash: auth::digest("login-password-at-least-32-characters"),
        logins: Mutex::new(HashMap::new()),
        sessions: Mutex::new(HashMap::new()),
        prompts: Mutex::new(HashMap::new()),
        ai_streams: Mutex::new(HashMap::new()),
        login_attempts: Mutex::new(vec![]),
        mutation: Mutex::new(()),
        shutdown: CancellationToken::new(),
    })
}
async fn request(
    app: &Router,
    state: &State,
    path: &str,
    body: Option<Value>,
    owner: Option<&str>,
    csrf: Option<&str>,
) -> (StatusCode, axum::http::HeaderMap, Value) {
    let mut builder = Request::builder()
        .uri(format!("/nyaterm/api/{path}"))
        .header("host", &state.host)
        .header("origin", &state.origin)
        .header("x-nyaterm-request", "1");
    if let Some(owner) = owner {
        builder = builder.header("cookie", format!("{}={owner}", auth::COOKIE));
    }
    if let Some(csrf) = csrf {
        builder = builder.header("x-nyaterm-csrf", csrf);
    }
    if body.is_some() {
        builder = builder
            .method("POST")
            .header("content-type", "application/json");
    }
    let response = app
        .clone()
        .oneshot(
            builder
                .body(
                    body.map(|v| Body::from(v.to_string()))
                        .unwrap_or(Body::empty()),
                )
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, headers, value)
}
async fn login(app: &Router, state: &State) -> (String, String) {
    let (status, headers, value) = request(
        app,
        state,
        "auth/login",
        Some(json!({"password":"login-password-at-least-32-characters"})),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let token = headers["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .split('=')
        .nth(1)
        .unwrap()
        .to_owned();
    (token, value["csrf"].as_str().unwrap().into())
}
async fn ws(
    host: &str,
    state: &State,
    id: &str,
    owner: &str,
) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    let mut r = format!("ws://{host}/nyaterm/api/sessions/{id}/terminal")
        .into_client_request()
        .unwrap();
    r.headers_mut()
        .insert("origin", state.origin.parse().unwrap());
    r.headers_mut().insert(
        "cookie",
        format!("{}={owner}", auth::COOKIE).parse().unwrap(),
    );
    connect_async(r).await.unwrap().0
}
async fn binary(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> Vec<u8> {
    loop {
        match tokio::time::timeout(Duration::from_secs(20), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap()
        {
            Message::Binary(b) => return b.to_vec(),
            Message::Ping(b) => socket.send(Message::Pong(b)).await.unwrap(),
            other => panic!("Expected bytes, got {other:?}"),
        }
    }
}

#[tokio::test]
async fn authenticated_ssh_vertical_slice_and_security() {
    let data = tempfile::tempdir().unwrap();
    nyaterm_core::storage::init(data.path()).unwrap();
    nyaterm_core::utils::crypto::set_server_key_material([37; 32]);
    let dist = tempfile::tempdir().unwrap();
    std::fs::write(dist.path().join("index.html"), "<html>NyaTerm</html>").unwrap();
    let web = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let host = web.local_addr().unwrap().to_string();
    let state = state(host.clone(), "/nyaterm");
    let app = nyaterm_web::router(state.clone(), dist.path().into());
    let serving = tokio::spawn(axum::serve(web, app.clone()).into_future());
    let (resize, mut resized) = mpsc::unbounded_channel();
    let ssh = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = ssh.local_addr().unwrap().port();
    let config = Arc::new(server::Config {
        keys: vec![
            russh::keys::PrivateKey::random(&mut rand_new::rng(), russh::keys::Algorithm::Ed25519)
                .unwrap(),
        ],
        auth_rejection_time: Duration::ZERO,
        auth_rejection_time_initial: Some(Duration::ZERO),
        ..Default::default()
    });
    let files = Arc::new(std::sync::Mutex::new(HashMap::new()));
    let ssh_files = files.clone();
    let ssh_task = tokio::spawn(async move {
        Echo {
            resize,
            files: ssh_files,
            channels: HashMap::new(),
            sftp_channels: Default::default(),
        }
        .run_on_socket(config, &ssh)
        .await
        .unwrap();
    });
    assert_eq!(
        request(
            &app,
            &state,
            "commands/list_sessions",
            Some(json!({})),
            None,
            None
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let (owner, csrf) = login(&app, &state).await;
    let (other, other_csrf) = login(&app, &state).await;
    assert_eq!(
        request(
            &app,
            &state,
            "commands/list_sessions",
            Some(json!({})),
            Some(&owner),
            None
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let bad = Request::builder()
        .uri("/nyaterm/")
        .header("host", "evil.example")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.clone().oneshot(bad).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    let static_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/nyaterm/settings")
                .header("host", &host)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(static_response.status(), StatusCode::OK);
    assert!(
        static_response
            .headers()
            .contains_key("content-security-policy")
    );
    let mut events = state.login(&owner).await.unwrap().events.subscribe();
    let args = json!({"config":{"encoding":"global","host":"127.0.0.1","port":port,"username":"test","auth":{"type":"password","password":"ssh-secret"}}});
    let (_, _, value) = request(
        &app,
        &state,
        "sessions",
        Some(args.clone()),
        Some(&owner),
        Some(&csrf),
    )
    .await;
    let id = value["session_id"].as_str().unwrap();
    // WebSocket ownership and exact Origin must be enforced during upgrade.
    for (token, origin, expected) in [
        (&other, state.origin.as_str(), 404),
        (&owner, "https://evil.example", 403),
    ] {
        let mut upgrade = format!("ws://{host}/nyaterm/api/sessions/{id}/terminal")
            .into_client_request()
            .unwrap();
        upgrade
            .headers_mut()
            .insert("origin", origin.parse().unwrap());
        upgrade.headers_mut().insert(
            "cookie",
            format!("{}={token}", auth::COOKIE).parse().unwrap(),
        );
        let error = connect_async(upgrade).await.unwrap_err();
        match error {
            tokio_tungstenite::tungstenite::Error::Http(response) => {
                assert_eq!(response.status().as_u16(), expected)
            }
            _ => panic!("Expected rejected upgrade"),
        }
    }
    let mut socket = ws(&host, &state, id, &owner).await;
    let prompt = tokio::time::timeout(Duration::from_secs(10), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(prompt.event, "host-key-verify");
    assert_eq!(prompt.payload["isKeyChanged"], false);
    let reply = json!({"requestId":prompt.payload["requestId"],"accepted":true});
    assert_eq!(
        request(
            &app,
            &state,
            "commands/respond_host_key_verify",
            Some(reply.clone()),
            Some(&other),
            Some(&other_csrf)
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(
            &app,
            &state,
            "commands/respond_host_key_verify",
            Some(reply),
            Some(&owner),
            Some(&csrf)
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        request(
            &app,
            &state,
            "commands/get_session_info",
            Some(json!({"sessionId":id})),
            Some(&other),
            Some(&other_csrf)
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    socket
        .send(Message::Binary(
            b"hello \xe4\xbd\xa0\xe5\xa5\xbd".to_vec().into(),
        ))
        .await
        .unwrap();
    assert_eq!(binary(&mut socket).await, b"hello \xe4\xbd\xa0\xe5\xa5\xbd");
    socket
        .send(Message::Text(
            json!({"type":"resize","cols":132,"rows":43})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(10), resized.recv())
            .await
            .unwrap(),
        Some((132, 43))
    );
    // Stream a body above the JSON limit through the real SFTP subsystem.
    let payload = vec![71u8; 5 * 1024 * 1024];
    let upload = Request::builder()
        .method("POST")
        .uri(format!("/nyaterm/api/sessions/{id}/upload?path=/large.bin"))
        .header("host", &host)
        .header("origin", &state.origin)
        .header("cookie", format!("{}={owner}", auth::COOKIE))
        .header("x-nyaterm-csrf", &csrf)
        .header("content-length", payload.len())
        .body(Body::from(payload.clone()))
        .unwrap();
    let response = app.clone().oneshot(upload).await.unwrap();
    let status = response.status();
    let response_body = to_bytes(response.into_body(), 1024).await.unwrap();
    assert_eq!(
        status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&response_body)
    );
    assert_eq!(files.lock().unwrap().get("/large.bin"), Some(&payload));
    let download = Request::builder()
        .uri(format!(
            "/nyaterm/api/sessions/{id}/download?path=/large.bin"
        ))
        .header("host", &host)
        .header("cookie", format!("{}={owner}", auth::COOKIE))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(download).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 6 * 1024 * 1024)
        .await
        .unwrap();
    assert_eq!(&bytes[..], payload);
    let (_, _, entries) = request(
        &app,
        &state,
        "commands/list_remote_dir",
        Some(json!({"sessionId":id,"path":"/"})),
        Some(&owner),
        Some(&csrf),
    )
    .await;
    assert_eq!(entries[0]["name"], "large.bin");
    assert_eq!(entries[0]["mtime"], 1700000000);
    assert_eq!(entries[0]["owner"], "1000");
    let body = Body::from_stream(futures_util::stream::iter(vec![
        Ok(axum::body::Bytes::from_static(b"partial")),
        Err(std::io::Error::other("test interrupted upload")),
    ]));
    let upload = Request::builder()
        .method("POST")
        .uri(format!(
            "/nyaterm/api/sessions/{id}/upload?path=/interrupted.bin"
        ))
        .header("host", &host)
        .header("origin", &state.origin)
        .header("cookie", format!("{}={owner}", auth::COOKIE))
        .header("x-nyaterm-csrf", &csrf)
        .body(body)
        .unwrap();
    assert_eq!(
        app.clone().oneshot(upload).await.unwrap().status(),
        StatusCode::BAD_REQUEST
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while files.lock().unwrap().keys().any(|p| p.contains(".part")) {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(!files.lock().unwrap().contains_key("/interrupted.bin"));
    // Credential plaintext is only returned by the explicit authenticated getter.
    let (_,_,credential)=request(&app,&state,"commands/save_credential",Some(json!({"entry":{"id":"","name":"Test","username":"test","password":"credential-secret"}})),Some(&owner),Some(&csrf)).await;
    let (_, _, list) = request(
        &app,
        &state,
        "commands/get_saved_credentials",
        Some(json!({})),
        Some(&owner),
        Some(&csrf),
    )
    .await;
    assert_eq!(list[0]["password"], Value::Null);
    assert_eq!(list[0]["has_password"], true);
    let stored = nyaterm_core::config::load_credentials(&()).unwrap();
    assert_ne!(
        stored.credentials[0].password.as_deref(),
        Some("credential-secret")
    );
    let (_, _, plain) = request(
        &app,
        &state,
        "commands/get_saved_credential_password",
        Some(json!({"id":credential})),
        Some(&owner),
        Some(&csrf),
    )
    .await;
    assert_eq!(plain, "credential-secret");
    socket.close(None).await.unwrap();
    drop(socket);
    tokio::time::sleep(Duration::from_millis(100)).await;
    let mut socket = ws(&host, &state, id, &owner).await;
    socket
        .send(Message::Binary(b"after-reattach".to_vec().into()))
        .await
        .unwrap();
    assert_eq!(binary(&mut socket).await, b"after-reattach");
    socket
        .send(Message::Binary(b"large-and-close".to_vec().into()))
        .await
        .unwrap();
    let mut output = Vec::new();
    loop {
        match tokio::time::timeout(Duration::from_secs(20), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap()
        {
            Message::Binary(b) => output.extend_from_slice(&b),
            Message::Text(t) => {
                assert_eq!(serde_json::from_str::<Value>(&t).unwrap()["type"], "closed");
                break;
            }
            Message::Ping(b) => socket.send(Message::Pong(b)).await.unwrap(),
            _ => {}
        }
    }
    assert_eq!(output.len(), 3 * 1024 * 1024 + 10);
    assert!(output.ends_with(b"final-tail"));
    let deadline = Instant::now() + Duration::from_secs(10);
    while !state.sessions.lock().await.is_empty() {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    // The existing saved-connection shape must resolve encrypted auth and honor SFTP disabling.
    let (status, _, saved) = request(&app,&state,"commands/save_connection",Some(json!({"connection":{"id":"","name":"Saved fixture","type":"ssh","host":"127.0.0.1","port":port,"username":"test","auth":{"mode":"password","password":"ssh-secret"},"sftp":{"enabled":false},"terminal_type":"vt100"}})),Some(&owner),Some(&csrf)).await;
    assert_eq!(status, StatusCode::OK);
    let (_, _, listed) = request(
        &app,
        &state,
        "commands/get_saved_connections",
        Some(json!({})),
        Some(&owner),
        Some(&csrf),
    )
    .await;
    assert_eq!(listed[0]["auth"]["password"], Value::Null);
    assert_eq!(listed[0]["auth"]["has_password"], true);
    let (status, _, created) = request(
        &app,
        &state,
        "sessions",
        Some(json!({"connectionId":saved})),
        Some(&owner),
        Some(&csrf),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let saved_id = created["session_id"].as_str().unwrap();
    let mut saved_socket = ws(&host, &state, saved_id, &owner).await;
    saved_socket
        .send(Message::Binary(b"saved-connection".to_vec().into()))
        .await
        .unwrap();
    assert_eq!(binary(&mut saved_socket).await, b"saved-connection");
    let (_, _, info) = request(
        &app,
        &state,
        "commands/get_session_info",
        Some(json!({"sessionId":saved_id})),
        Some(&owner),
        Some(&csrf),
    )
    .await;
    assert_eq!(info["sftp_available"], false);
    assert_eq!(
        request(
            &app,
            &state,
            "commands/get_home_dir",
            Some(json!({"sessionId":saved_id})),
            Some(&owner),
            Some(&csrf)
        )
        .await
        .0,
        StatusCode::NOT_IMPLEMENTED
    );
    assert_eq!(
        request(
            &app,
            &state,
            "commands/close_session",
            Some(json!({"sessionId":saved_id})),
            Some(&owner),
            Some(&csrf)
        )
        .await
        .0,
        StatusCode::OK
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while state.sessions.lock().await.contains_key(saved_id) {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    // Known-host replacement still requires approval; rejection closes the session.
    nyaterm_core::storage::replace_known_host_for_host(
        &format!("[127.0.0.1]:{port}"),
        &format!("[127.0.0.1]:{port} ssh-ed25519 AAAABOGUS"),
    )
    .unwrap();
    while events.try_recv().is_ok() {}
    let (_, _, value) = request(
        &app,
        &state,
        "sessions",
        Some(args),
        Some(&owner),
        Some(&csrf),
    )
    .await;
    let id = value["session_id"].as_str().unwrap();
    let prompt = tokio::time::timeout(Duration::from_secs(10), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(prompt.payload["isKeyChanged"], true);
    request(
        &app,
        &state,
        "commands/respond_host_key_verify",
        Some(json!({"requestId":prompt.payload["requestId"],"accepted":false})),
        Some(&owner),
        Some(&csrf),
    )
    .await;
    // Cancellation while a host-key prompt is pending releases the request and SSH task.
    let deadline = Instant::now() + Duration::from_secs(10);
    while state.sessions.lock().await.contains_key(id) {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    while events.try_recv().is_ok() {}
    let (_,_,pending) = request(&app,&state,"sessions",Some(json!({"createRequestId":"cancel-fixture","config":{"host":"127.0.0.1","port":port,"username":"test","auth":{"type":"password","password":"ssh-secret"}}})),Some(&owner),Some(&csrf)).await;
    let prompt = tokio::time::timeout(Duration::from_secs(10), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(prompt.event, "host-key-verify");
    assert_eq!(
        request(
            &app,
            &state,
            "commands/cancel_session_creation",
            Some(json!({"createRequestId":"cancel-fixture"})),
            Some(&owner),
            Some(&csrf)
        )
        .await
        .0,
        StatusCode::OK
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while state
        .sessions
        .lock()
        .await
        .contains_key(pending["session_id"].as_str().unwrap())
    {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let prompt_id = prompt.payload["requestId"].as_str().unwrap();
    assert!(!state.prompts.lock().await.contains_key(prompt_id));
    // The browser AI path uses the existing provider stream and mandatory redaction.
    let provider = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider_url = format!("http://{}/v1/", provider.local_addr().unwrap());
    let (observed, mut observed_requests) = mpsc::unbounded_channel::<Value>();
    let provider_app=Router::new().route("/v1/chat/completions",axum::routing::post(move |headers:axum::http::HeaderMap,axum::Json(body):axum::Json<Value>|{let observed=observed.clone();async move {assert_eq!(headers["authorization"],"Bearer ai-provider-secret");let _=observed.send(body);([("content-type","text/event-stream")],"data: {\"id\":\"reply\",\"object\":\"chat.completion.chunk\",\"model\":\"test-model\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello from shared AI\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"reply\",\"object\":\"chat.completion.chunk\",\"model\":\"test-model\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n")}}));
    let provider_task = tokio::spawn(axum::serve(provider, provider_app).into_future());
    let mut settings = nyaterm_core::config::load_app_settings(&()).unwrap();
    settings.ai.enabled = true;
    settings.ai.record_history = true;
    settings.ai.provider_credentials=vec![serde_json::from_value(json!({"id":"test-provider","name":"Test","provider_kind":"openai_compatible","base_url":provider_url,"api_key":"ai-provider-secret","enabled":true})).unwrap()];
    settings.ai.models=vec![serde_json::from_value(json!({"id":"test-provider:test-model","name":"test-model","provider_kind":"openai_compatible","credential_id":"test-provider","enabled":true,"source":"manual"})).unwrap()];
    settings.ai.default_model_id = Some("test-provider:test-model".into());
    settings.ai = nyaterm_core::config::encrypt_ai_settings(settings.ai).unwrap();
    settings.cloud_sync =
        nyaterm_core::config::encrypt_cloud_sync_settings(settings.cloud_sync).unwrap();
    nyaterm_core::config::save_app_settings(&(), &settings).unwrap();
    while events.try_recv().is_ok() {}
    let forbidden_context = json!({"request":{"action":"explain_output","userInput":"Explain","context":{},"mode":"ask","targetContexts":[{"target":{"terminalSessionId":"not-owned","label":"Other","sessionType":"SSH"},"context":{}}]}});
    assert_eq!(
        request(
            &app,
            &state,
            "commands/start_ai_chat_stream",
            Some(forbidden_context),
            Some(&owner),
            Some(&csrf)
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let ai_request = json!({"request":{"streamId":"test-ai-stream","sessionId":"test-ai-history","action":"explain_output","userInput":"Please explain password=do-not-send-this-secret","context":{},"targetContexts":[{"context":{"terminalOutput":"password=target-context-secret"}}],"mode":"ask"}});
    assert_eq!(
        request(
            &app,
            &state,
            "commands/start_ai_chat_stream",
            Some(ai_request),
            Some(&owner),
            Some(&csrf)
        )
        .await
        .0,
        StatusCode::OK
    );
    let observed = tokio::time::timeout(Duration::from_secs(10), observed_requests.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(!observed.to_string().contains("do-not-send-this-secret"));
    assert!(!observed.to_string().contains("target-context-secret"));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline);
        let event = tokio::time::timeout(Duration::from_secs(10), events.recv())
            .await
            .unwrap()
            .unwrap();
        if event.event == "ai-stream-test-ai-stream" && event.payload["type"] == "done" {
            assert!(
                event.payload["message"]["content"]
                    .as_str()
                    .unwrap()
                    .contains("Hello from shared AI")
            );
            break;
        }
        if event.event == "ai-stream-test-ai-stream" {
            assert_ne!(event.payload["type"], "error", "{}", event.payload);
        }
    }
    let (_, _, history) = request(
        &app,
        &state,
        "commands/get_ai_messages",
        Some(json!({"sessionId":"test-ai-history"})),
        Some(&owner),
        Some(&csrf),
    )
    .await;
    assert_eq!(history.as_array().unwrap().len(), 2);
    assert!(!history.to_string().contains("do-not-send-this-secret"));
    assert_eq!(
        request(
            &app,
            &state,
            "commands/clear_ai_history",
            Some(json!({})),
            Some(&owner),
            Some(&csrf)
        )
        .await
        .0,
        StatusCode::OK
    );
    assert!(
        nyaterm_core::core::ai::history::get_ai_sessions(&())
            .unwrap()
            .is_empty()
    );
    provider_task.abort();
    nyaterm_core::utils::crypto::verify_master_key_token().unwrap();
    nyaterm_core::utils::crypto::set_server_key_material([38; 32]);
    assert!(nyaterm_core::utils::crypto::verify_master_key_token().is_err());
    nyaterm_core::utils::crypto::set_server_key_material([37; 32]);
    nyaterm_core::utils::crypto::verify_master_key_token().unwrap();
    assert_eq!(
        request(
            &app,
            &state,
            "auth/logout",
            Some(json!({})),
            Some(&owner),
            Some(&csrf)
        )
        .await
        .0,
        StatusCode::OK
    );
    assert!(state.login(&owner).await.is_err());
    assert!(state.session(&owner, id).await.is_err());
    state.shutdown.cancel();
    serving.abort();
    ssh_task.abort();
}
