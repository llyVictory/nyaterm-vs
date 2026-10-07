use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use futures_util::{SinkExt, StreamExt};
use nyaterm_web::{auth, state::State};
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{Mutex, mpsc},
};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;
fn state(base_path: &str) -> Arc<State> {
    Arc::new(State {
        base_path: base_path.into(),
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
    _state: &State,
    path: &str,
    body: Option<Value>,
    owner: Option<&str>,
    csrf: Option<&str>,
) -> (StatusCode, axum::http::HeaderMap, Value) {
    let mut builder = Request::builder()
        .uri(format!("/nyaterm/api/{path}"))
        .header("host", "terminal.example")
        .header("origin", "http://terminal.example")
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
    _state: &State,
    id: &str,
    owner: &str,
) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    let mut r = format!("ws://{host}/nyaterm/api/sessions/{id}/terminal")
        .into_client_request()
        .unwrap();
    r.headers_mut()
        .insert("origin", format!("http://{host}").parse().unwrap());
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

type Socket = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>;
async fn vnc_ws(host: &str, _state: &State, id: &str, owner: &str) -> Socket {
    let mut r = format!("ws://{host}/nyaterm/api/sessions/{id}/vnc")
        .into_client_request()
        .unwrap();
    r.headers_mut()
        .insert("origin", format!("http://{host}").parse().unwrap());
    r.headers_mut().insert(
        "cookie",
        format!("{}={owner}", auth::COOKIE).parse().unwrap(),
    );
    connect_async(r).await.unwrap().0
}
async fn rejected_ws(
    host: &str,
    _state: &State,
    id: &str,
    endpoint: &str,
    owner: &str,
    expected: u16,
) {
    let mut r = format!("ws://{host}/nyaterm/api/sessions/{id}/{endpoint}")
        .into_client_request()
        .unwrap();
    r.headers_mut()
        .insert("origin", format!("http://{host}").parse().unwrap());
    r.headers_mut().insert(
        "cookie",
        format!("{}={owner}", auth::COOKIE).parse().unwrap(),
    );
    match connect_async(r).await {
        Err(tokio_tungstenite::tungstenite::Error::Http(response)) => {
            assert_eq!(response.status().as_u16(), expected)
        }
        _ => panic!("Expected a rejected WebSocket upgrade"),
    }
}
async fn next_message(socket: &mut Socket) -> Message {
    loop {
        let message = tokio::time::timeout(Duration::from_secs(5), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        match message {
            Message::Ping(b) => socket.send(Message::Pong(b)).await.unwrap(),
            other => return other,
        }
    }
}
async fn frame(socket: &mut Socket) -> Vec<u8> {
    loop {
        if let Message::Binary(bytes) = next_message(socket).await {
            return bytes.to_vec();
        }
    }
}
async fn command(
    app: &Router,
    state: &State,
    owner: &str,
    csrf: &str,
    name: &str,
    args: Value,
) -> Value {
    let (status, _, value) = request(
        app,
        state,
        &format!("commands/{name}"),
        Some(args),
        Some(owner),
        Some(csrf),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{name}: {value}");
    value
}
async fn create(app: &Router, state: &State, owner: &str, csrf: &str, args: Value) -> String {
    let (status, _, value) =
        request(app, state, "sessions", Some(args), Some(owner), Some(csrf)).await;
    assert_eq!(status, StatusCode::OK, "{value}");
    value["session_id"].as_str().unwrap().into()
}
async fn wait_removed(state: &State, id: &str) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while state.sessions.lock().await.contains_key(id) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}
async fn echo() -> (u16, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    (
        port,
        tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                tokio::spawn(async move {
                    let mut buf = [0; 1024];
                    while let Ok(n) = stream.read(&mut buf).await {
                        if n == 0 {
                            break;
                        }
                        if stream.write_all(&buf[..n]).await.is_err() {
                            break;
                        }
                    }
                });
            }
        }),
    )
}
async fn proxy(http: bool, authenticated: bool) -> (u16, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    (
        port,
        tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                tokio::spawn(async move {
                    let (host, port) = if http {
                        let mut bytes = Vec::new();
                        while !bytes.ends_with(b"\r\n\r\n") {
                            bytes.push(stream.read_u8().await.unwrap());
                        }
                        let request = String::from_utf8(bytes).unwrap();
                        assert_eq!(
                            request.to_lowercase().contains(
                                "proxy-authorization: basic cHJveHk6c2VjcmV0"
                                    .to_lowercase()
                                    .as_str()
                            ),
                            authenticated
                        );
                        let target = request.split_whitespace().nth(1).unwrap();
                        let (host, port) = target.rsplit_once(':').unwrap();
                        let target = (host.to_owned(), port.parse::<u16>().unwrap());
                        stream
                            .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
                            .await
                            .unwrap();
                        target
                    } else {
                        assert_eq!(stream.read_u8().await.unwrap(), 5);
                        let count = stream.read_u8().await.unwrap();
                        let mut methods = vec![0; count as usize];
                        stream.read_exact(&mut methods).await.unwrap();
                        let method = if authenticated { 2 } else { 0 };
                        assert!(methods.contains(&method));
                        stream.write_all(&[5, method]).await.unwrap();
                        if authenticated {
                            assert_eq!(stream.read_u8().await.unwrap(), 1);
                            let len = stream.read_u8().await.unwrap();
                            let mut user = vec![0; len as usize];
                            stream.read_exact(&mut user).await.unwrap();
                            let len = stream.read_u8().await.unwrap();
                            let mut password = vec![0; len as usize];
                            stream.read_exact(&mut password).await.unwrap();
                            assert_eq!(user, b"proxy");
                            assert_eq!(password, b"secret");
                            stream.write_all(&[1, 0]).await.unwrap();
                        }
                        assert_eq!(stream.read_u8().await.unwrap(), 5);
                        assert_eq!(stream.read_u8().await.unwrap(), 1);
                        assert_eq!(stream.read_u8().await.unwrap(), 0);
                        let host = match stream.read_u8().await.unwrap() {
                            1 => {
                                let mut ip = [0; 4];
                                stream.read_exact(&mut ip).await.unwrap();
                                std::net::Ipv4Addr::from(ip).to_string()
                            }
                            3 => {
                                let len = stream.read_u8().await.unwrap();
                                let mut name = vec![0; len as usize];
                                stream.read_exact(&mut name).await.unwrap();
                                String::from_utf8(name).unwrap()
                            }
                            _ => panic!("unsupported address"),
                        };
                        let port = stream.read_u16().await.unwrap();
                        stream
                            .write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 0])
                            .await
                            .unwrap();
                        (host, port)
                    };
                    let mut target = TcpStream::connect((host.as_str(), port)).await.unwrap();
                    let _ = tokio::io::copy_bidirectional(&mut stream, &mut target).await;
                });
            }
        }),
    )
}
async fn rfb() -> (
    u16,
    mpsc::UnboundedReceiver<Vec<u8>>,
    tokio::task::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = mpsc::unbounded_channel();
    (
        port,
        rx,
        tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let tx = tx.clone();
                tokio::spawn(async move {
                    stream.write_all(b"RFB 003.008\n").await.unwrap();
                    let mut version = [0; 12];
                    if stream.read_exact(&mut version).await.is_err() {
                        // A cancelled/replaced connection may stop mid-handshake.
                        return;
                    }
                    stream.write_all(&[1, 1]).await.unwrap();
                    assert_eq!(stream.read_u8().await.unwrap(), 1);
                    stream.write_all(&0_u32.to_be_bytes()).await.unwrap();
                    let shared = stream.read_u8().await.unwrap();
                    tx.send(vec![99, shared]).unwrap();
                    stream
                        .write_all(&[
                            0, 2, 0, 1, 32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 0, 8, 16, 0, 0, 0, 0,
                            0, 0, 4,
                        ])
                        .await
                        .unwrap();
                    stream.write_all(b"test").await.unwrap();
                    let mut sent = false;
                    loop {
                        let Ok(kind) = stream.read_u8().await else {
                            break;
                        };
                        let size = match kind {
                            0 => 19,
                            2 => 3,
                            3 => 9,
                            4 => 7,
                            5 => 5,
                            6 => 7,
                            _ => panic!("Unknown RFB input {kind}"),
                        };
                        let mut packet = vec![kind];
                        packet.resize(size + 1, 0);
                        if stream.read_exact(&mut packet[1..]).await.is_err() {
                            break;
                        }
                        if kind == 2 {
                            let count = u16::from_be_bytes([packet[2], packet[3]]);
                            let mut encodings = vec![0; count as usize * 4];
                            stream.read_exact(&mut encodings).await.unwrap();
                        }
                        if kind == 6 {
                            let count =
                                u32::from_be_bytes(packet[4..8].try_into().unwrap()) as usize;
                            let mut text = vec![0; count];
                            stream.read_exact(&mut text).await.unwrap();
                            packet.extend(&text);
                            if text == b"patch" {
                                stream
                                    .write_all(&[
                                        0, 0, 0, 1, 0, 1, 0, 0, 0, 1, 0, 1, 0, 0, 0, 0, 70, 80, 90,
                                        0,
                                    ])
                                    .await
                                    .unwrap();
                            }
                            if text == b"resize" {
                                let mut resized = vec![0, 0, 0, 2, 0, 0, 0, 0, 0, 3, 0, 1];
                                resized.extend((-223_i32).to_be_bytes());
                                resized.extend([
                                    0, 0, 0, 0, 0, 3, 0, 1, 0, 0, 0, 0, 1, 2, 3, 0, 4, 5, 6, 0, 7,
                                    8, 9, 0,
                                ]);
                                stream.write_all(&resized).await.unwrap();
                            }
                        }
                        if kind == 3 && !sent {
                            // Two-pixel raw rectangle in the client's requested RGBA format.
                            let update = [
                                0, 0, 0, 1, 0, 0, 0, 0, 0, 2, 0, 1, 0, 0, 0, 0, 10, 20, 30, 0, 40,
                                50, 60, 0,
                            ];
                            stream.write_all(&update).await.unwrap();
                            stream.write_all(&[3, 0, 0, 0, 0, 0, 0, 6]).await.unwrap();
                            stream.write_all(b"remote").await.unwrap();
                            sent = true;
                        }
                        if matches!(kind, 4 | 5 | 6) {
                            let _ = tx.send(packet);
                        }
                    }
                });
            }
        }),
    )
}

#[derive(Clone)]
struct Jump;
impl russh::server::Server for Jump {
    type Handler = Self;
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self {
        Self
    }
}
impl russh::server::Handler for Jump {
    type Error = russh::Error;
    async fn auth_password(
        &mut self,
        username: &str,
        password: &str,
    ) -> Result<russh::server::Auth, Self::Error> {
        Ok(if username == "jump" && password == "jump-password" {
            russh::server::Auth::Accept
        } else {
            russh::server::Auth::reject()
        })
    }
    async fn channel_open_direct_tcpip(
        &mut self,
        channel: russh::Channel<russh::server::Msg>,
        host: &str,
        port: u32,
        _: &str,
        _: u32,
        reply: russh::server::ChannelOpenHandle,
        _: &mut russh::server::Session,
    ) -> Result<(), Self::Error> {
        let mut target = TcpStream::connect((host, port as u16)).await?;
        reply.accept().await;
        tokio::spawn(async move {
            let mut stream = channel.into_stream();
            let _ = tokio::io::copy_bidirectional(&mut stream, &mut target).await;
        });
        Ok(())
    }
}
async fn jump() -> (u16, tokio::task::JoinHandle<()>) {
    use russh::{keys::PublicKeyBase64, server::Server as _};
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let key =
        russh::keys::PrivateKey::random(&mut rand_new::rng(), russh::keys::Algorithm::Ed25519)
            .unwrap();
    let host_id = format!("[127.0.0.1]:{port}");
    nyaterm_core::storage::replace_known_host_for_host(
        &host_id,
        &format!(
            "{host_id} {} {}",
            key.algorithm(),
            key.public_key().public_key_base64()
        ),
    )
    .unwrap();
    let config = Arc::new(russh::server::Config {
        keys: vec![key],
        auth_rejection_time: Duration::ZERO,
        auth_rejection_time_initial: Some(Duration::ZERO),
        ..Default::default()
    });
    (
        port,
        tokio::spawn(async move {
            Jump.run_on_socket(config, &listener).await.unwrap();
        }),
    )
}

#[tokio::test]
async fn web_telnet_vnc_proxy_and_lifecycle() {
    let data = tempfile::tempdir().unwrap();
    nyaterm_core::storage::init(data.path()).unwrap();
    nyaterm_core::utils::crypto::set_server_key_material([41; 32]);
    let dist = tempfile::tempdir().unwrap();
    std::fs::write(dist.path().join("index.html"), "test").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let host = listener.local_addr().unwrap().to_string();
    let state = state("/nyaterm");
    let app = nyaterm_web::router(state.clone(), dist.path().into());
    let serving = tokio::spawn(axum::serve(listener, app.clone()).into_future());
    let (owner, csrf) = login(&app, &state).await;
    let (other, other_csrf) = login(&app, &state).await;
    let (port, echo_task) = echo().await;
    let saved = command(&app,&state,&owner,&csrf,"save_connection",json!({"connection":{"id":"","name":"GBK Telnet","type":"telnet","host":"127.0.0.1","port":port,"encoding":"GBK","enter_mode":"crlf","auto_login":{"enabled":false}}})).await;
    let id = create(
        &app,
        &state,
        &owner,
        &csrf,
        json!({"type":"telnet","connectionId":saved,"recordingScopeId":"telnet-pane"}),
    )
    .await;
    let info = command(
        &app,
        &state,
        &owner,
        &csrf,
        "get_session_info",
        json!({"sessionId":id}),
    )
    .await;
    assert_eq!(info["session_type"], "Telnet");
    assert_eq!(info["workspace_pane_id"], "telnet-pane");
    assert_eq!(info["sftp_available"], false);
    rejected_ws(&host, &state, &id, "vnc", &owner, 400).await;
    let (status, _, _) = request(
        &app,
        &state,
        &format!("commands/get_session_info"),
        Some(json!({"sessionId":id})),
        Some(&other),
        Some(&other_csrf),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = request(
        &app,
        &state,
        "commands/sftp_read_dir",
        Some(json!({"sessionId":id,"path":"/"})),
        Some(&owner),
        Some(&csrf),
    )
    .await;
    assert_ne!(status, StatusCode::OK);
    let mut socket = ws(&host, &state, &id, &owner).await;
    socket
        .send(Message::Text(
            json!({"type":"input","data":"你好\r"}).to_string().into(),
        ))
        .await
        .unwrap();
    assert_eq!(binary(&mut socket).await, "你好\r\n".as_bytes());
    // Raw bytes are not re-encoded as UTF-8 before the connection codec.
    socket
        .send(Message::Binary(vec![0xc4, 0xe3].into()))
        .await
        .unwrap();
    assert_eq!(binary(&mut socket).await, "你".as_bytes());
    socket.close(None).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while state.sessions.lock().await[&id]
            .attached
            .load(std::sync::atomic::Ordering::Acquire)
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let mut restored = ws(&host, &state, &id, &owner).await;
    restored
        .send(Message::Text(
            json!({"type":"input","data":"restored"}).to_string().into(),
        ))
        .await
        .unwrap();
    assert_eq!(binary(&mut restored).await, b"restored");
    command(
        &app,
        &state,
        &owner,
        &csrf,
        "close_session",
        json!({"sessionId":id}),
    )
    .await;
    wait_removed(&state, &id).await;

    // Automatic login gates the startup command and NAWS uses escaped dimensions.
    let login_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let login_port = login_listener.local_addr().unwrap().port();
    let login_server = tokio::spawn(async move {
        let (mut stream, _) = login_listener.accept().await.unwrap();
        stream.write_all(&[255, 253, 31]).await.unwrap();
        stream.write_all(b"login:").await.unwrap();
        let mut negotiation = [0; 3];
        stream.read_exact(&mut negotiation).await.unwrap();
        assert_eq!(negotiation, [255, 251, 31]);
        let mut user = [0; 5];
        stream.read_exact(&mut user).await.unwrap();
        assert_eq!(&user, b"test\r");
        stream.write_all(b"Password:").await.unwrap();
        let mut password = [0; 7];
        stream.read_exact(&mut password).await.unwrap();
        assert_eq!(&password, b"secret\r");
        stream.write_all(b"\r\n$ ").await.unwrap();
        let started = tokio::time::Instant::now();
        let mut startup = [0; 7];
        stream.read_exact(&mut startup).await.unwrap();
        assert_eq!(&startup, b"whoami\r");
        assert!(started.elapsed() >= Duration::from_millis(70));
        stream.write_all(b"READY").await.unwrap();
        let mut naws = [0; 11];
        stream.read_exact(&mut naws).await.unwrap();
        assert_eq!(naws, [255, 250, 31, 0, 255, 255, 1, 255, 255, 255, 240]);
    });
    let id = create(&app,&state,&owner,&csrf,json!({"type":"telnet","host":"127.0.0.1","port":login_port,"username":"test","auth":{"mode":"password","password":"secret"},"auto_login":{"success_prompt_regex":"\\$"},"startupCommand":{"command":"whoami","delay_ms":80}})).await;
    let mut socket = ws(&host, &state, &id, &owner).await;
    let mut text = Vec::new();
    while !text.ends_with(b"READY") {
        text.extend(binary(&mut socket).await);
    }
    socket
        .send(Message::Text(
            json!({"type":"resize","cols":255,"rows":511})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    login_server.await.unwrap();
    wait_removed(&state, &id).await;
    // Raw TCP line editing echoes locally and sends only the final edited line.
    let line_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let line_port = line_listener.local_addr().unwrap().port();
    let line_server = tokio::spawn(async move {
        let (mut stream, _) = line_listener.accept().await.unwrap();
        let mut line = [0; 4];
        stream.read_exact(&mut line).await.unwrap();
        assert_eq!(&line, b"ab\r\n");
        stream.write_all(b"DONE").await.unwrap();
    });
    let id = create(&app,&state,&owner,&csrf,json!({"type":"telnet","host":"127.0.0.1","port":line_port,"raw_tcp_cli":true,"local_line_edit":true,"enter_mode":"crlf"})).await;
    let mut socket = ws(&host, &state, &id, &owner).await;
    socket
        .send(Message::Text(
            json!({"type":"input","data":"abX\u{7f}\r"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let mut text = Vec::new();
    while !text.ends_with(b"DONE") {
        text.extend(binary(&mut socket).await);
    }
    assert!(text.starts_with(b"abX"));
    line_server.await.unwrap();
    wait_removed(&state, &id).await;
    for http in [false, true] {
        for authenticated in [false, true] {
            let (proxy_port, task) = proxy(http, authenticated).await;
            let proxy_id = command(&app,&state,&owner,&csrf,"save_proxy",json!({"proxy":{"id":"","name":"fixture","protocol":if http {"http"} else {"socks5"},"host":"127.0.0.1","port":proxy_port,"username":authenticated.then_some("proxy"),"password":authenticated.then_some("secret")}})).await;
            let proxies = command(&app, &state, &owner, &csrf, "get_proxies", json!({})).await;
            assert!(
                proxies
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|p| p["password"].is_null())
            );
            let id = create(&app,&state,&owner,&csrf,json!({"type":"telnet","host":"127.0.0.1","port":port,"raw_tcp_cli":true,"network":{"proxy_id":proxy_id}})).await;
            let mut socket = ws(&host, &state, &id, &owner).await;
            socket
                .send(Message::Binary(vec![0xff, b'x'].into()))
                .await
                .unwrap();
            // GBK is not configured here: input bytes survive on the wire, echo x proves routing.
            let output = binary(&mut socket).await;
            assert!(output.ends_with(b"x"));
            command(
                &app,
                &state,
                &owner,
                &csrf,
                "close_session",
                json!({"sessionId":id}),
            )
            .await;
            wait_removed(&state, &id).await;
            task.abort();
        }
    }

    let group = command(
        &app,
        &state,
        &owner,
        &csrf,
        "save_proxy_group",
        json!({"group":{"id":"","name":"fixtures"}}),
    )
    .await;
    let credential_proxy = command(&app,&state,&owner,&csrf,"save_proxy",json!({"proxy":{"id":"","name":"before edit","protocol":"http","host":"127.0.0.1","port":1,"username":"proxy","password":"secret","group_id":group}})).await;
    command(&app,&state,&owner,&csrf,"save_proxy",json!({"proxy":{"id":credential_proxy,"name":"after edit","protocol":"http","host":"127.0.0.1","port":1,"username":"proxy","group_id":group}})).await;
    assert_eq!(
        command(
            &app,
            &state,
            &owner,
            &csrf,
            "get_proxy_password",
            json!({"proxyId":credential_proxy})
        )
        .await,
        "secret"
    );
    command(
        &app,
        &state,
        &owner,
        &csrf,
        "set_proxy_group",
        json!({"proxyId":credential_proxy,"groupId":null}),
    )
    .await;
    command(
        &app,
        &state,
        &owner,
        &csrf,
        "delete_proxy_group",
        json!({"groupId":group}),
    )
    .await;
    assert!(
        command(&app, &state, &owner, &csrf, "get_proxy_groups", json!({}))
            .await
            .as_array()
            .unwrap()
            .is_empty()
    );
    command(
        &app,
        &state,
        &owner,
        &csrf,
        "delete_proxy",
        json!({"proxyId":credential_proxy}),
    )
    .await;
    // Jump B uses Jump A's own authenticated proxy; the target's bad proxy is
    // intentionally ignored, matching desktop route priority.
    let (jump_a_port, jump_a_task) = jump().await;
    let (jump_b_port, jump_b_task) = jump().await;
    let (jump_proxy_port, jump_proxy_task) = proxy(false, true).await;
    let proxy_id = command(&app,&state,&owner,&csrf,"save_proxy",json!({"proxy":{"id":"","name":"jump proxy","protocol":"socks5","host":"127.0.0.1","port":jump_proxy_port,"username":"proxy","password":"secret"}})).await;
    let a = command(&app,&state,&owner,&csrf,"save_connection",json!({"connection":{"id":"","name":"Jump A","type":"ssh","host":"127.0.0.1","port":jump_a_port,"username":"jump","encoding":"GBK","auth":{"mode":"password","password":"jump-password"},"network":{"proxy_id":proxy_id}}})).await;
    let b = command(&app,&state,&owner,&csrf,"save_connection",json!({"connection":{"id":"","name":"Jump B","type":"ssh","host":"127.0.0.1","port":jump_b_port,"username":"jump","auth":{"mode":"password","password":"jump-password"},"network":{"proxy_jump_id":a}}})).await;
    let telnet_route = command(&app,&state,&owner,&csrf,"save_connection",json!({"connection":{"id":"","name":"Saved routed Telnet","type":"telnet","host":"127.0.0.1","port":port,"network":{"proxy_jump_id":b}}})).await;
    assert_eq!(
        nyaterm_core::config::load_connection_by_id(&(), telnet_route.as_str().unwrap())
            .unwrap()
            .network
            .unwrap()
            .proxy_jump_id
            .as_deref(),
        b.as_str()
    );
    let id = create(&app,&state,&owner,&csrf,json!({"type":"telnet","host":"127.0.0.1","port":port,"network":{"proxy_jump_id":b,"proxy_id":"missing-ignored"}})).await;
    let mut socket = ws(&host, &state, &id, &owner).await;
    socket
        .send(Message::Text(
            json!({"type":"input","data":"nested jumps"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    assert_eq!(binary(&mut socket).await, b"nested jumps");
    command(
        &app,
        &state,
        &owner,
        &csrf,
        "close_session",
        json!({"sessionId":id}),
    )
    .await;
    wait_removed(&state, &id).await;
    let mut events = state.login(&owner).await.unwrap().events.subscribe();
    // Cycle detection happens before any TCP connection or authentication.
    let (status,_,_) = request(&app,&state,"commands/save_connection",Some(json!({"connection":{"id":a,"name":"Jump A","type":"ssh","host":"127.0.0.1","port":jump_a_port,"username":"jump","auth":{"mode":"password"},"network":{"proxy_jump_id":b}}})),Some(&owner),Some(&csrf)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    // Imported/on-disk cycles must also be rejected when opening a route.
    let mut saved = nyaterm_core::config::load_config(&()).unwrap();
    saved
        .connections
        .iter_mut()
        .find(|c| c.id == a.as_str().unwrap())
        .unwrap()
        .network = Some(nyaterm_core::config::ConnectionNetwork {
        proxy_id: None,
        proxy_jump_id: Some(b.as_str().unwrap().into()),
    });
    nyaterm_core::config::save_config(&(), &saved).unwrap();
    let id = create(
        &app,
        &state,
        &owner,
        &csrf,
        json!({"type":"telnet","host":"127.0.0.1","port":port,"network":{"proxy_jump_id":b}}),
    )
    .await;
    let error = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let event = events.recv().await.unwrap();
            if event.event == format!("connection-error-{id}") {
                break event.payload;
            }
        }
    })
    .await
    .unwrap();
    assert!(error["error"].as_str().unwrap().contains("cycle"));
    wait_removed(&state, &id).await;

    for http in [false, true] {
        let denied = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let denied_port = denied.local_addr().unwrap().port();
        let denial = tokio::spawn(async move {
            let (mut stream, _) = denied.accept().await.unwrap();
            if http {
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n") {
                    request.push(stream.read_u8().await.unwrap());
                }
                stream
                    .write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\n\r\n")
                    .await
                    .unwrap();
            } else {
                assert_eq!(stream.read_u8().await.unwrap(), 5);
                let count = stream.read_u8().await.unwrap();
                let mut methods = vec![0; count as usize];
                stream.read_exact(&mut methods).await.unwrap();
                stream.write_all(&[5, 2]).await.unwrap();
                assert_eq!(stream.read_u8().await.unwrap(), 1);
                let count = stream.read_u8().await.unwrap();
                let mut username = vec![0; count as usize];
                stream.read_exact(&mut username).await.unwrap();
                let count = stream.read_u8().await.unwrap();
                let mut password = vec![0; count as usize];
                stream.read_exact(&mut password).await.unwrap();
                stream.write_all(&[1, 1]).await.unwrap();
            }
        });
        let proxy_id = command(&app,&state,&owner,&csrf,"save_proxy",json!({"proxy":{"id":"","name":"reject auth","protocol":if http {"http"} else {"socks5"},"host":"127.0.0.1","port":denied_port,"username":"proxy","password":"wrong"}})).await;
        let id = create(
            &app,
            &state,
            &owner,
            &csrf,
            json!({"type":"telnet","host":"127.0.0.1","port":port,"network":{"proxy_id":proxy_id}}),
        )
        .await;
        let error = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let event = events.recv().await.unwrap();
                if event.event == format!("connection-error-{id}") {
                    break event.payload;
                }
            }
        })
        .await
        .unwrap();
        assert!(error["error"].as_str().unwrap().contains("proxy"));
        wait_removed(&state, &id).await;
        denial.await.unwrap();
    }
    let mut persisted = nyaterm_core::config::load_config(&()).unwrap();
    for index in 0..9 {
        persisted.connections.push(serde_json::from_value(json!({"id":format!("depth-{index}"),"name":"depth","type":"ssh","host":"127.0.0.1","port":jump_a_port,"username":"jump","network":if index < 8 {json!({"proxy_jump_id":format!("depth-{}",index+1)})} else {Value::Null}})).unwrap());
    }
    nyaterm_core::config::save_config(&(), &persisted).unwrap();
    let id = create(&app,&state,&owner,&csrf,json!({"type":"telnet","host":"127.0.0.1","port":port,"network":{"proxy_jump_id":"depth-0"}})).await;
    let error = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let event = events.recv().await.unwrap();
            if event.event == format!("connection-error-{id}") {
                break event.payload;
            }
        }
    })
    .await
    .unwrap();
    assert!(error["error"].as_str().unwrap().contains("depth"));
    wait_removed(&state, &id).await;
    // Missing/decryption errors must fail the selected route, never use direct TCP.
    for proxy_id in ["not-found", "broken-secret"] {
        if proxy_id == "broken-secret" {
            let proxies = vec![serde_json::from_value(json!({"id":proxy_id,"name":"bad","protocol":"socks5","host":"127.0.0.1","port":jump_proxy_port,"username":"proxy","password":"not-encrypted"})).unwrap()];
            nyaterm_core::config::save_proxies(&(), &proxies).unwrap();
        }
        let id = create(
            &app,
            &state,
            &owner,
            &csrf,
            json!({"type":"telnet","host":"127.0.0.1","port":port,"network":{"proxy_id":proxy_id}}),
        )
        .await;
        let error = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let event = events.recv().await.unwrap();
                if event.event == format!("connection-error-{id}") {
                    break event.payload;
                }
            }
        })
        .await
        .unwrap();
        assert!(
            error["error"]
                .as_str()
                .unwrap()
                .contains(if proxy_id == "not-found" {
                    "configuration"
                } else {
                    "decrypt"
                })
        );
        wait_removed(&state, &id).await;
    }
    jump_a_task.abort();
    jump_b_task.abort();
    jump_proxy_task.abort();
    let (vnc_port, mut input, rfb_task) = rfb().await;
    let saved_vnc = command(&app,&state,&owner,&csrf,"save_connection",json!({"connection":{"id":"","name":"VNC fixture","type":"vnc","host":"127.0.0.1","port":vnc_port,"security":{"mode":"none"},"reconnect":{"enabled":false}}})).await;
    let id = create(
        &app,
        &state,
        &owner,
        &csrf,
        json!({"type":"vnc","connectionId":saved_vnc,"recordingScopeId":"vnc-pane"}),
    )
    .await;
    let info = command(
        &app,
        &state,
        &owner,
        &csrf,
        "get_session_info",
        json!({"sessionId":id}),
    )
    .await;
    assert_eq!(info["session_type"], "VNC");
    assert_eq!(info["workspace_pane_id"], "vnc-pane");
    assert_eq!(info["terminal_available"], false);
    rejected_ws(&host, &state, &id, "terminal", &owner, 400).await;
    rejected_ws(&host, &state, &id, "vnc", &other, 404).await;
    let mut socket = vnc_ws(&host, &state, &id, &owner).await;
    let bytes = frame(&mut socket).await;
    assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()), 2);
    assert_eq!(bytes.len(), 52);
    assert_eq!(&bytes[44..47], &[10, 20, 30]);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(3), input.recv())
            .await
            .unwrap()
            .unwrap(),
        [99, 1]
    );
    socket.send(Message::Text(json!({"type":"input","events":[{"type":"key","keysym":65,"pressed":true},{"type":"pointer","x":1,"y":0,"buttonMask":1}]}).to_string().into())).await.unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(3), input.recv())
            .await
            .unwrap()
            .unwrap(),
        [4, 1, 0, 0, 0, 0, 0, 65]
    );
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(3), input.recv())
            .await
            .unwrap()
            .unwrap(),
        [5, 1, 0, 1, 0, 0]
    );
    socket
        .send(Message::Text(
            json!({"type":"clipboard","text":"local"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(3), input.recv())
            .await
            .unwrap()
            .unwrap(),
        [6, 0, 0, 0, 0, 0, 0, 5, b'l', b'o', b'c', b'a', b'l']
    );

    socket
        .send(Message::Text(
            json!({"type":"clipboard","text":"patch"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    input.recv().await.unwrap();
    let patch = frame(&mut socket).await;
    assert_eq!(u32::from_le_bytes(patch[16..20].try_into().unwrap()), 1);
    assert_eq!(&patch[44..47], &[70, 80, 90]);
    socket
        .send(Message::Text(
            json!({"type":"clipboard","text":"resize"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    input.recv().await.unwrap();
    let resized = frame(&mut socket).await;
    assert_eq!(u32::from_le_bytes(resized[8..12].try_into().unwrap()), 3);
    assert_eq!(resized.len(), 56);
    socket.close(None).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while state.sessions.lock().await[&id]
            .attached
            .load(std::sync::atomic::Ordering::Acquire)
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let mut socket = vnc_ws(&host, &state, &id, &owner).await;
    assert_eq!(frame(&mut socket).await.len(), 56);
    command(
        &app,
        &state,
        &owner,
        &csrf,
        "vnc_reconnect",
        json!({"sessionId":id}),
    )
    .await;
    loop {
        let packet = tokio::time::timeout(Duration::from_secs(3), input.recv())
            .await
            .unwrap()
            .unwrap();
        if packet == [99, 1] {
            break;
        }
        // Key releases are best effort while the old RFB connection closes.
        assert_eq!(packet, [4, 0, 0, 0, 0, 0, 0, 65]);
    }
    assert_eq!(frame(&mut socket).await.len(), 52);
    command(
        &app,
        &state,
        &owner,
        &csrf,
        "close_vnc_session",
        json!({"sessionId":id}),
    )
    .await;
    wait_removed(&state, &id).await;
    let readonly = create(&app,&state,&owner,&csrf,json!({"type":"vnc","config":{"name":"Readonly","host":"127.0.0.1","port":vnc_port,"security":{"mode":"none"},"view_only":true,"reconnect":{"enabled":false}}})).await;
    let mut socket = vnc_ws(&host, &state, &readonly, &owner).await;
    frame(&mut socket).await;
    let (status, _, _) = request(
        &app,
        &state,
        "commands/vnc_set_clipboard_text",
        Some(json!({"sessionId":readonly,"text":"denied"})),
        Some(&owner),
        Some(&csrf),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _, _) = request(
        &app,
        &state,
        "commands/vnc_input_batch",
        Some(json!({"sessionId":readonly,"events":[{"type":"key","keysym":65,"pressed":true}]})),
        Some(&owner),
        Some(&csrf),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(input.recv().await.unwrap(), [99, 1]);
    assert!(
        tokio::time::timeout(Duration::from_millis(100), input.recv())
            .await
            .is_err()
    );
    command(
        &app,
        &state,
        &owner,
        &csrf,
        "close_vnc_session",
        json!({"sessionId":readonly}),
    )
    .await;
    wait_removed(&state, &readonly).await;

    let stalled = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let stalled_port = stalled.local_addr().unwrap().port();
    let (accepted, accepted_rx) = tokio::sync::oneshot::channel();
    let stalled_task = tokio::spawn(async move {
        let (mut stream, _) = stalled.accept().await.unwrap();
        let mut bytes = [0; 512];
        let count = stream.read(&mut bytes).await.unwrap();
        assert!(count > 0);
        accepted.send(()).unwrap();
        assert_eq!(stream.read(&mut bytes).await.unwrap(), 0);
    });
    let proxy_id = command(&app,&state,&owner,&csrf,"save_proxy",json!({"proxy":{"id":"","name":"stalled","protocol":"http","host":"127.0.0.1","port":stalled_port}})).await;
    let cancelled = create(&app,&state,&owner,&csrf,json!({"type":"telnet","host":"127.0.0.1","port":port,"createRequestId":"cancel-network","network":{"proxy_id":proxy_id}})).await;
    tokio::time::timeout(Duration::from_secs(3), accepted_rx)
        .await
        .unwrap()
        .unwrap();
    command(
        &app,
        &state,
        &owner,
        &csrf,
        "cancel_session_creation",
        json!({"createRequestId":"cancel-network"}),
    )
    .await;
    wait_removed(&state, &cancelled).await;
    stalled_task.await.unwrap();
    let mut limited = Vec::new();
    for _ in 0..16 {
        limited.push(
            create(
                &app,
                &state,
                &other,
                &other_csrf,
                json!({"type":"telnet","host":"127.0.0.1","port":port}),
            )
            .await,
        );
    }
    let (status, _, _) = request(
        &app,
        &state,
        "sessions",
        Some(json!({"type":"vnc","host":"127.0.0.1","port":vnc_port,"security":{"mode":"none"}})),
        Some(&other),
        Some(&other_csrf),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    // Login cancellation releases sessions created up to the shared limit.
    state.close_owner(&other).await;
    for id in limited {
        wait_removed(&state, &id).await;
    }
    let detached = create(
        &app,
        &state,
        &owner,
        &csrf,
        json!({"type":"telnet","host":"127.0.0.1","port":port}),
    )
    .await;
    let (status, _, _) = request(
        &app,
        &state,
        "auth/logout",
        Some(json!({})),
        Some(&owner),
        Some(&csrf),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    wait_removed(&state, &detached).await;
    let (owner, csrf) = login(&app, &state).await;
    let terminal = create(
        &app,
        &state,
        &owner,
        &csrf,
        json!({"type":"telnet","host":"127.0.0.1","port":port}),
    )
    .await;
    let desktop = create(&app,&state,&owner,&csrf,json!({"type":"vnc","host":"127.0.0.1","port":vnc_port,"security":{"mode":"none"},"reconnect":{"enabled":false}})).await;
    state.shutdown.cancel();
    wait_removed(&state, &terminal).await;
    wait_removed(&state, &desktop).await;
    assert!(state.sessions.lock().await.is_empty());
    echo_task.abort();
    rfb_task.abort();
    serving.abort();
}
