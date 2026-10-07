use super::*;
use base64::Engine;
use futures_util::StreamExt;
use genai::chat::{ChatMessage, ChatRequest, ChatStreamEvent};
use serde_json::json;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

use super::super::{model, responses};
use crate::config::{
    AiApiFormat, AiBackendKind, AiModelConfigItem, AiModelSource, AiProviderApiProtocol,
    AiProviderKind,
};

const WAIT: Duration = Duration::from_secs(5);
const CHAT_REPLY: &str = r#"{"id":"chat-test","object":"chat.completion","model":"proxy-test","choices":[{"index":0,"message":{"role":"assistant","content":"OK"},"finish_reason":"stop"}]}"#;

async fn read_request(stream: &mut TcpStream) -> String {
    tokio::time::timeout(WAIT, async {
        let mut request = Vec::new();
        loop {
            let mut buffer = [0; 1024];
            let count = stream.read(&mut buffer).await.unwrap();
            assert_ne!(count, 0, "connection closed before complete HTTP request");
            request.extend_from_slice(&buffer[..count]);
            if let Some(end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&request[..end]);
                let length = headers
                    .lines()
                    .find_map(|line| {
                        let (key, value) = line.split_once(':')?;
                        key.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                if request.len() >= end + 4 + length {
                    return String::from_utf8(request).unwrap();
                }
            }
            assert!(request.len() < 65536);
        }
    })
    .await
    .expect("HTTP request timed out")
}

async fn reply(stream: &mut TcpStream, content_type: &str, body: &str) {
    stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
}

async fn server(content_type: &'static str, body: &str) -> (u16, JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let body = body.to_string();
    let task = tokio::spawn(async move {
        let (mut stream, _) = tokio::time::timeout(WAIT, listener.accept())
            .await
            .unwrap()
            .unwrap();
        let request = read_request(&mut stream).await;
        reply(&mut stream, content_type, &body).await;
        request
    });
    (port, task)
}

fn settings(port: u16) -> AiSettings {
    let mut settings = AiSettings::default();
    settings.proxy.mode = AiProxyMode::Custom;
    settings.proxy.port = port;
    settings.proxy.no_proxy.clear();
    settings.timeout_ms = 3000;
    settings.request_user_agent = "nyaterm-proxy-test".to_string();
    settings.provider_credentials.truncate(1);
    let credential = &mut settings.provider_credentials[0];
    credential.id = "proxy-provider".to_string();
    credential.enabled = true;
    credential.provider_kind = AiProviderKind::OpenaiCompatible;
    credential.api_protocol = Some(AiProviderApiProtocol::OpenaiCompatible);
    credential.base_url = Some("http://ai-proxy-test.invalid/v1/".to_string());
    credential.api_key = Some("provider-key".to_string());
    settings.models = vec![AiModelConfigItem {
        id: "proxy-provider:proxy-test".to_string(),
        name: "proxy-test".to_string(),
        backend: AiBackendKind::Genai,
        provider_kind: Some(AiProviderKind::OpenaiCompatible),
        credential_id: Some(credential.id.clone()),
        enabled: true,
        source: AiModelSource::Manual,
        last_seen_at: None,
        supported_reasoning_efforts: None,
    }];
    settings
}

fn resolved(settings: &AiSettings) -> model::ResolvedAiModel {
    model::ResolvedAiModel {
        model_name: "proxy-test".to_string(),
        provider_kind: AiProviderKind::OpenaiCompatible,
        api_format: settings.provider_credentials[0].api_format.clone(),
        credential: Some(settings.provider_credentials[0].clone()),
    }
}

#[test]
fn validates_proxy_host_port_and_credentials_without_exposing_secrets() {
    let mut settings = settings(7890);
    for host in [
        "",
        "http://localhost",
        "user:secret@localhost",
        "host/path",
        "host?query",
        "host\\path",
        "host name",
        "[invalid]",
        "localhost:7890",
    ] {
        settings.proxy.host = host.to_string();
        let error = build_http_client(&settings).unwrap_err().to_string();
        assert!(error.contains("Invalid AI proxy"));
        assert!(!error.contains("secret"));
    }
    settings.proxy.host = "127.0.0.1".to_string();
    settings.proxy.port = 0;
    assert!(build_http_client(&settings).is_err());
    settings.proxy.port = 7890;
    settings.proxy.password = Some("private-password".to_string());
    let error = build_http_client(&settings).unwrap_err().to_string();
    assert!(error.contains("requires a username"));
    assert!(!error.contains("private-password"));
    settings.proxy.username = Some("user".to_string());
    settings.proxy.protocol = AiProxyProtocol::Socks5;
    settings.proxy.password = Some("x".repeat(256));
    assert!(build_http_client(&settings).is_err());
    for mode in [AiProxyMode::Direct, AiProxyMode::System] {
        settings.proxy.mode = mode;
        settings.proxy.host.clear();
        assert!(
            build_http_client(&settings).is_ok(),
            "inactive fields must not affect routing"
        );
    }
}

#[test]
fn supports_ipv6_and_socks_remote_dns() {
    let mut settings = settings(7890);
    for host in ["::1", "[::1]"] {
        settings.proxy.host = host.to_string();
        assert_eq!(
            proxy_url(&settings.proxy).unwrap().as_str(),
            "http://[::1]:7890/"
        );
    }
    settings.proxy.protocol = AiProxyProtocol::Socks5;
    assert_eq!(proxy_url(&settings.proxy).unwrap().scheme(), "socks5h");
}

#[tokio::test]
async fn http_proxy_authenticates_without_leaking_proxy_credentials_to_provider() {
    let (port, task) = server("application/json", r#"{"data":[{"id":"proxy-model"}]}"#).await;
    let mut settings = settings(port);
    settings.proxy.username = Some("user".to_string());
    settings.proxy.password = Some("pass".to_string());
    let names = model::test_provider_connection(&settings, "proxy-provider")
        .await
        .unwrap();
    assert_eq!(names, vec!["proxy-model"]);
    let request = task.await.unwrap().to_lowercase();
    assert!(request.starts_with("get http://ai-proxy-test.invalid/v1/models "));
    assert!(request.contains("proxy-authorization: basic dxnlcjpwyxnz"));
    assert!(request.contains("authorization: bearer provider-key"));
    assert!(request.contains("user-agent: nyaterm-proxy-test"));
}

#[tokio::test]
async fn all_provider_model_discovery_protocols_use_proxy() {
    for (protocol, body, path) in [
        (
            AiProviderApiProtocol::Anthropic,
            r#"{"data":[{"id":"proxy-model"}],"has_more":false}"#,
            "/v1/models?limit=1000",
        ),
        (
            AiProviderApiProtocol::Gemini,
            r#"{"models":[{"name":"models/proxy-model"}]}"#,
            "/v1/models?pageSize=1000",
        ),
        (
            AiProviderApiProtocol::Ollama,
            r#"{"models":[{"name":"proxy-model"}]}"#,
            "/v1/api/tags",
        ),
    ] {
        let (port, task) = server("application/json", body).await;
        let mut settings = settings(port);
        settings.provider_credentials[0].api_protocol = Some(protocol);
        let names = model::test_provider_connection(&settings, "proxy-provider")
            .await
            .unwrap();
        assert_eq!(names, vec!["proxy-model"]);
        assert!(task.await.unwrap().contains(path));
    }
}

#[tokio::test]
async fn genai_chat_and_model_connection_test_use_proxy() {
    let (port, task) = server("application/json", CHAT_REPLY).await;
    let settings = settings(port);
    let client = model::build_client(&resolved(&settings), &settings).unwrap();
    let response = tokio::time::timeout(
        WAIT,
        client.exec_chat(
            "proxy-test",
            ChatRequest::new(vec![ChatMessage::user("hello")]),
            None,
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(response.content.first_text(), Some("OK"));
    assert!(
        task.await
            .unwrap()
            .starts_with("POST http://ai-proxy-test.invalid/v1/chat/completions ")
    );

    let (port, task) = server("application/json", CHAT_REPLY).await;
    model::test_model_connection(
        &settings_with_port(settings, port),
        "proxy-provider:proxy-test",
    )
    .await
    .unwrap();
    assert!(
        task.await
            .unwrap()
            .contains("Confirm that this model can respond.")
    );
}

fn settings_with_port(mut settings: AiSettings, port: u16) -> AiSettings {
    settings.proxy.port = port;
    settings
}

#[tokio::test]
async fn genai_stream_uses_proxy() {
    let body = concat!(
        "data: {\"id\":\"test\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"OK\"},\"finish_reason\":null}]}\n\n",
        "data: {\"id\":\"test\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
        "data: [DONE]\n\n"
    );
    let (port, task) = server("text/event-stream", body).await;
    let settings = settings(port);
    let client = model::build_client(&resolved(&settings), &settings).unwrap();
    let result = tokio::time::timeout(
        WAIT,
        client.exec_chat_stream(
            "proxy-test",
            ChatRequest::new(vec![ChatMessage::user("hello")]),
            None,
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let mut stream = result.stream;
    let mut output = String::new();
    tokio::time::timeout(WAIT, async {
        while let Some(event) = stream.next().await {
            if let ChatStreamEvent::Chunk(chunk) = event.unwrap() {
                output.push_str(&chunk.content);
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(output, "OK");
    assert!(task.await.unwrap().contains("\"stream\":true"));
}

#[tokio::test]
async fn responses_nonstream_test_and_stream_transport_use_proxy() {
    let (port, task) = server("application/json", r#"{"id":"response-test","output":[{"type":"message","content":[{"type":"output_text","text":"OK"}]}]}"#).await;
    let mut settings = settings(port);
    settings.provider_credentials[0].api_format = AiApiFormat::Responses;
    model::test_model_connection(&settings, "proxy-provider:proxy-test")
        .await
        .unwrap();
    let request = task.await.unwrap();
    assert!(request.starts_with("POST http://ai-proxy-test.invalid/v1/responses "));
    assert!(request.contains("\"stream\":false"));

    let body = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"OK\"}\n\n";
    let (port, task) = server("text/event-stream", body).await;
    settings.proxy.port = port;
    let mut response = responses::responses_request(
        &settings,
        &resolved(&settings),
        &json!({"model":"proxy-test", "input":"hello", "stream":true}),
    )
    .unwrap()
    .timeout(WAIT)
    .send()
    .await
    .unwrap();
    let mut received = Vec::new();
    while let Some(chunk) = response.chunk().await.unwrap() {
        received.extend_from_slice(&chunk);
    }
    assert_eq!(String::from_utf8(received).unwrap(), body);
    let request = task.await.unwrap();
    assert!(request.contains("\"stream\":true"));
    assert!(
        request
            .to_lowercase()
            .contains("authorization: bearer provider-key")
    );
}

#[tokio::test]
async fn local_bypass_connects_directly_and_never_sends_proxy_auth_to_origin() {
    for host in ["127.0.0.1", "localhost"] {
        let (port, task) = server("text/plain", "direct").await;
        let mut settings = settings(port);
        settings.proxy.port = 1; // unavailable proxy must not prevent a bypassed request
        settings.proxy.no_proxy = AiProxySettings::default().no_proxy;
        settings.proxy.username = Some("user".to_string());
        settings.proxy.password = Some("pass".to_string());
        let response = build_http_client(&settings)
            .unwrap()
            .get(format!("http://{host}:{port}/local"))
            .timeout(WAIT)
            .send()
            .await
            .unwrap();
        assert_eq!(response.text().await.unwrap(), "direct");
        let request = task.await.unwrap();
        assert!(request.starts_with("GET /local "));
        assert!(!request.to_lowercase().contains("proxy-authorization"));
    }
}

#[tokio::test]
async fn ipv6_loopback_bypasses_proxy() {
    let listener = TcpListener::bind("[::1]:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let task = tokio::spawn(async move {
        let (mut stream, _) = tokio::time::timeout(WAIT, listener.accept())
            .await
            .unwrap()
            .unwrap();
        let request = read_request(&mut stream).await;
        reply(&mut stream, "text/plain", "ipv6").await;
        request
    });
    let mut settings = settings(1);
    settings.proxy.no_proxy = AiProxySettings::default().no_proxy;
    let response = build_http_client(&settings)
        .unwrap()
        .get(format!("http://[::1]:{port}/"))
        .timeout(WAIT)
        .send()
        .await
        .unwrap();
    assert_eq!(response.text().await.unwrap(), "ipv6");
    assert!(task.await.unwrap().starts_with("GET / "));
}

#[tokio::test]
async fn clearing_bypass_routes_local_targets_through_proxy() {
    let (port, task) = server("text/plain", "proxied").await;
    let response = build_http_client(&settings(port))
        .unwrap()
        .get("http://localhost:1/")
        .timeout(WAIT)
        .send()
        .await
        .unwrap();
    assert_eq!(response.text().await.unwrap(), "proxied");
    assert!(task.await.unwrap().starts_with("GET http://localhost:1/ "));
}

#[tokio::test]
async fn https_uses_connect_with_proxy_auth_and_sanitizes_failure() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut settings = settings(listener.local_addr().unwrap().port());
    settings.proxy.username = Some("user".to_string());
    settings.proxy.password = Some("secret-password".to_string());
    let task = tokio::spawn(async move {
        let (mut stream, _) = tokio::time::timeout(WAIT, listener.accept())
            .await
            .unwrap()
            .unwrap();
        let request = read_request(&mut stream).await;
        stream.write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
        request
    });
    let error = build_http_client(&settings)
        .unwrap()
        .get("https://ai-proxy-test.invalid/")
        .timeout(WAIT)
        .send()
        .await
        .unwrap_err();
    assert!(!error.to_string().contains("secret-password"));
    let request = task.await.unwrap();
    assert!(request.starts_with("CONNECT ai-proxy-test.invalid:443 "));
    let auth = base64::engine::general_purpose::STANDARD.encode("user:secret-password");
    assert!(request.to_lowercase().contains(&format!(
        "proxy-authorization: basic {}",
        auth.to_lowercase()
    )));
    assert!(!request.to_lowercase().contains("authorization: bearer"));
}

#[tokio::test]
async fn explicit_direct_and_custom_override_inherited_proxies() {
    for mode in [AiProxyMode::Direct, AiProxyMode::Custom] {
        let (port, task) = server("text/plain", "direct").await;
        let mut settings = settings(1).proxy;
        settings.mode = mode;
        settings.no_proxy = "127.0.0.1".to_string();
        let inherited = Client::builder().proxy(Proxy::all("http://127.0.0.1:1").unwrap());
        let response = apply_proxy(inherited, &settings)
            .unwrap()
            .build()
            .unwrap()
            .get(format!("http://127.0.0.1:{port}/"))
            .timeout(WAIT)
            .send()
            .await
            .unwrap();
        assert_eq!(response.text().await.unwrap(), "direct");
        task.await.unwrap();
    }
    let (port, task) = server("text/plain", "inherited").await;
    let inherited =
        Client::builder().proxy(Proxy::all(format!("http://127.0.0.1:{port}")).unwrap());
    let response = apply_proxy(inherited, &AiProxySettings::default())
        .unwrap()
        .build()
        .unwrap()
        .get("http://ai-proxy-test.invalid/")
        .timeout(WAIT)
        .send()
        .await
        .unwrap();
    assert_eq!(response.text().await.unwrap(), "inherited");
    task.await.unwrap();
}

#[tokio::test]
async fn failed_proxy_does_not_fall_back_to_reachable_origin() {
    let origin = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let unavailable = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let settings = settings(unavailable.local_addr().unwrap().port());
    drop(unavailable);
    let response = build_http_client(&settings)
        .unwrap()
        .get(format!(
            "http://127.0.0.1:{}/",
            origin.local_addr().unwrap().port()
        ))
        .timeout(WAIT)
        .send()
        .await;
    assert!(response.is_err());
    assert!(
        tokio::time::timeout(Duration::from_millis(100), origin.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn socks5_authentication_and_remote_dns_route_actual_requests() {
    for authenticated in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut settings = settings(listener.local_addr().unwrap().port());
        settings.proxy.protocol = AiProxyProtocol::Socks5;
        if authenticated {
            settings.proxy.username = Some("user".to_string());
            settings.proxy.password = Some("pass".to_string());
        }
        let task = tokio::spawn(async move {
            tokio::time::timeout(WAIT, async {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut greeting = [0; 2];
                stream.read_exact(&mut greeting).await.unwrap();
                assert_eq!(greeting[0], 5);
                let mut methods = vec![0; greeting[1] as usize];
                stream.read_exact(&mut methods).await.unwrap();
                let method = if authenticated { 2 } else { 0 };
                assert!(methods.contains(&method));
                stream.write_all(&[5, method]).await.unwrap();
                if authenticated {
                    let mut header = [0; 2];
                    stream.read_exact(&mut header).await.unwrap();
                    assert_eq!(header[0], 1);
                    let mut user = vec![0; header[1] as usize];
                    stream.read_exact(&mut user).await.unwrap();
                    let length = stream.read_u8().await.unwrap();
                    let mut pass = vec![0; length as usize];
                    stream.read_exact(&mut pass).await.unwrap();
                    assert_eq!(user, b"user");
                    assert_eq!(pass, b"pass");
                    stream.write_all(&[1, 0]).await.unwrap();
                }
                let mut header = [0; 4];
                stream.read_exact(&mut header).await.unwrap();
                assert_eq!(
                    header,
                    [5, 1, 0, 3],
                    "SOCKS must resolve target DNS remotely"
                );
                let length = stream.read_u8().await.unwrap();
                let mut host = vec![0; length as usize];
                stream.read_exact(&mut host).await.unwrap();
                assert_eq!(host, b"ai-proxy-test.invalid");
                assert_eq!(stream.read_u16().await.unwrap(), 80);
                stream
                    .write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 80])
                    .await
                    .unwrap();
                let request = read_request(&mut stream).await;
                assert!(!request.to_lowercase().contains("proxy-authorization"));
                reply(
                    &mut stream,
                    "application/json",
                    r#"{"data":[{"id":"socks-model"}]}"#,
                )
                .await;
                request
            })
            .await
            .unwrap()
        });
        assert_eq!(
            model::test_provider_connection(&settings, "proxy-provider")
                .await
                .unwrap(),
            vec!["socks-model"]
        );
        assert!(task.await.unwrap().starts_with("GET /v1/models "));
    }
}
