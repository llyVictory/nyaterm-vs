//! Real server processes exercise independent key hierarchies and persisted restores.
use base64::{Engine, engine::general_purpose::STANDARD};
use nyaterm_core::{
    config,
    core::{backup, backup_crypto, portable_snapshot::*},
    storage,
    utils::crypto,
};
use serde_json::{Value, json};
use std::{
    io::{Cursor, Read, Write},
    path::Path,
    process::{Child, Command, Stdio},
    time::Duration,
};

const LOGIN: &str = "integration-login-password-more-than-32";
const DESKTOP_PASSWORD: &str = "desktop-backup-password";
const WEB_PASSWORD: &str = "portable-web-backup-password";
const SECRET: &str = "integration-secret-never-in-logs";
const LEGACY_SHORT_PASSWORD: &str = "test";
const LEGACY_LONG_PASSWORD: &str = "legacy-inline-password-with-symbols!";
const AI_SECRET: &str = "integration-ai-proxy-password-never-in-logs";
struct Server {
    process: Child,
    client: reqwest::Client,
    base: String,
    cookie: String,
    csrf: String,
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}
impl Server {
    async fn start(data: &Path, dist: &Path, key: u8, subpath: &str) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let base = format!("http://{address}/{subpath}");
        let process = Command::new(env!("CARGO_BIN_EXE_nyaterm-web"))
            .env_remove("NYATERM_WEB_PASSWORD_FILE")
            .env_remove("NYATERM_WEB_ENCRYPTION_KEY_FILE")
            .env("NYATERM_WEB_PASSWORD", LOGIN)
            .env("NYATERM_WEB_ENCRYPTION_KEY", STANDARD.encode([key; 32]))
            .env("NYATERM_WEB_DATA_DIR", data)
            .env("NYATERM_WEB_DIST", dist)
            .env("NYATERM_WEB_BASE_PATH", format!("/{subpath}"))
            .env("NYATERM_WEB_BIND", address.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut server = Self {
            process,
            client: reqwest::Client::builder().no_proxy().build().unwrap(),
            base,
            cookie: String::new(),
            csrf: String::new(),
        };
        let mut ready = false;
        for _ in 0..200 {
            if server.client.get(&server.base).send().await.is_ok() {
                ready = true;
                break;
            }
            assert!(
                server.process.try_wait().unwrap().is_none(),
                "Server exited during startup"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        assert!(ready);
        let response = server
            .client
            .post(format!("{}api/auth/login", server.base))
            .header("origin", server.origin())
            .header("x-nyaterm-request", "1")
            .json(&json!({"password":LOGIN}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        server.cookie = response.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .into();
        server.csrf = response.json::<Value>().await.unwrap()["csrf"]
            .as_str()
            .unwrap()
            .into();
        server
    }
    fn origin(&self) -> String {
        url::Url::parse(&self.base)
            .unwrap()
            .origin()
            .ascii_serialization()
    }
    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        self.client
            .request(method, format!("{}api/{path}", self.base))
            .header("origin", self.origin())
            .header("cookie", &self.cookie)
            .header("x-nyaterm-csrf", &self.csrf)
            .header("x-nyaterm-request", "1")
    }
    async fn command(&self, command: &str, args: Value) -> Value {
        let response = self
            .request(reqwest::Method::POST, &format!("commands/{command}"))
            .json(&args)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200, "{command}");
        response.json().await.unwrap()
    }
    async fn import(&self, bytes: &[u8], password: &str) -> reqwest::Response {
        self.upload("backups/import", "password", password, bytes)
            .await
    }
    async fn upload(
        &self,
        route: &str,
        field: &str,
        value: &str,
        bytes: &[u8],
    ) -> reqwest::Response {
        let mut body=format!("--fixture\r\nContent-Disposition: form-data; name=\"{field}\"\r\n\r\n{value}\r\n--fixture\r\nContent-Disposition: form-data; name=\"file\"; filename=\"fixture.nya\"\r\nContent-Type: application/octet-stream\r\n\r\n").into_bytes();
        body.extend_from_slice(bytes);
        body.extend_from_slice(b"\r\n--fixture--\r\n");
        self.request(reqwest::Method::POST, route)
            .header("content-type", "multipart/form-data; boundary=fixture")
            .body(body)
            .send()
            .await
            .unwrap()
    }
    async fn export(&self) -> Vec<u8> {
        let response = self
            .request(reqwest::Method::POST, "backups/export")
            .json(&json!({"password":WEB_PASSWORD}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        response.bytes().await.unwrap().to_vec()
    }
    async fn verify_secrets(&self) {
        for (command, id, expected) in [
            ("get_saved_password_value", "password", SECRET),
            ("get_saved_credential_password", "credential", SECRET),
            ("get_ssh_key_private_key", "key", "private-key-fixture"),
            ("get_connection_password_value", "connection", SECRET),
            (
                "get_connection_password_value",
                "legacy-short",
                LEGACY_SHORT_PASSWORD,
            ),
            (
                "get_connection_password_value",
                "legacy-long",
                LEGACY_LONG_PASSWORD,
            ),
            ("get_otp_secret_value", "otp", "JBSWY3DPEHPK3PXP"),
        ] {
            assert_eq!(
                self.command(command, json!({"id":id})).await,
                json!(expected)
            );
        }
        assert_eq!(
            self.command(
                "get_connection_password_value",
                json!({"id":"legacy-empty"})
            )
            .await,
            Value::Null
        );
    }
}

#[tokio::test]
async fn desktop_web_cross_key_backups_are_atomic_and_diagnostics_are_redacted() {
    let temp = tempfile::tempdir().unwrap();
    let dist = temp.path().join("dist");
    std::fs::create_dir(&dist).unwrap();
    std::fs::write(dist.join("index.html"), "NyaTerm").unwrap();
    storage::init(&temp.path().join("desktop")).unwrap();
    crypto::set_master_password(Some(DESKTOP_PASSWORD.into()));
    let mut settings = config::AppSettings::default();
    settings.ai.proxy.password = Some(crypto::encrypt(AI_SECRET).unwrap());
    config::save_app_settings(&(), &settings).unwrap();
    config::save_passwords(&(),&serde_json::from_value(json!({"passwords":[{"id":"password","name":"fixture","password":crypto::encrypt(SECRET).unwrap()}]})).unwrap()).unwrap();
    config::save_credentials(&(),&serde_json::from_value(json!({"credentials":[{"id":"credential","name":"fixture","username":"test","password":crypto::encrypt(SECRET).unwrap()}]})).unwrap()).unwrap();
    config::save_keys(&(),&serde_json::from_value(json!({"keys":[{"id":"key","name":"fixture","key":crypto::encrypt("private-key-fixture").unwrap()}]})).unwrap()).unwrap();
    config::save_otp_entries(&(),&serde_json::from_value(json!({"entries":[{"id":"otp","otp_type":"totp","issuer":"fixture","username":"test","secret":crypto::encrypt("JBSWY3DPEHPK3PXP").unwrap()}]})).unwrap()).unwrap();
    config::save_sessions(&(), &serde_json::from_value(json!({"connections":[
        {"id":"connection","name":"fixture","type":"ssh","host":"private.example","auth":{"mode":"password","password":crypto::encrypt(SECRET).unwrap()}},
        {"id":"legacy-short","name":"legacy short","type":"ssh","host":"private.example","auth":{"mode":"password","password":LEGACY_SHORT_PASSWORD}},
        {"id":"legacy-long","name":"legacy long","type":"ssh","host":"private.example","auth":{"mode":"password","password":LEGACY_LONG_PASSWORD}},
        {"id":"legacy-empty","name":"legacy empty","type":"ssh","host":"private.example","auth":{"mode":"password"}},
        {"id":"desktop-only","name":"serial preserved","type":"serial","port_name":"COM5"}
    ]})).unwrap()).unwrap();
    let original_key = storage::load_master_key_token().unwrap();
    // This is exactly Desktop's existing snapshot + password encryption path.
    let mut desktop_snapshot = backup::build_backup("1.2.12").unwrap();
    // Desktop's codec can preserve Some("") even though storage normally clears it.
    desktop_snapshot
        .sessions
        .connections
        .iter_mut()
        .find(|connection| connection.id == "legacy-empty")
        .unwrap()
        .auth
        .as_mut()
        .unwrap()
        .password = Some(String::new());
    desktop_snapshot.payload_hash = calculate_payload_hash(&desktop_snapshot).unwrap();
    let desktop_bytes = backup_crypto::encrypt_snapshot_bytes(
        &encode_portable_snapshot(&desktop_snapshot).unwrap(),
        DESKTOP_PASSWORD,
    )
    .unwrap();
    let source_data = temp.path().join("source");
    let target_data = temp.path().join("target");
    let source = Server::start(&source_data, &dist, 41, "").await;
    assert_eq!(
        source
            .import(&desktop_bytes, DESKTOP_PASSWORD)
            .await
            .status(),
        200
    );
    source.verify_secrets().await;
    let exported = source.export().await;
    assert!(
        !exported
            .windows(SECRET.len())
            .any(|bytes| bytes == SECRET.as_bytes())
    );
    // A Web backup decodes through Desktop's wire codec and its credentials decrypt with the wrapped backup key.
    let decoded = decode_portable_snapshot(
        &backup_crypto::decrypt_snapshot_bytes(&exported, WEB_PASSWORD).unwrap(),
    )
    .unwrap();
    assert_eq!(
        decoded.settings.ai.proxy.password.as_deref(),
        Some(AI_SECRET)
    );
    storage::save_master_key_token(decoded.master_key_token.as_deref().unwrap()).unwrap();
    crypto::set_master_password(Some(WEB_PASSWORD.into()));
    assert_eq!(
        crypto::decrypt(decoded.passwords.passwords[0].password.as_deref().unwrap()).unwrap(),
        SECRET
    );
    assert_eq!(
        crypto::decrypt(decoded.keys.keys[0].key.as_deref().unwrap()).unwrap(),
        "private-key-fixture"
    );
    storage::save_master_key_token(original_key.as_deref().unwrap()).unwrap();
    crypto::set_master_password(Some(DESKTOP_PASSWORD.into()));
    let target = Server::start(&target_data, &dist, 42, "terminal/").await;
    assert_eq!(target.import(&exported, WEB_PASSWORD).await.status(), 200);
    target.verify_secrets().await;
    let saved = target.command("get_saved_connections", json!({})).await;
    assert!(
        saved
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["id"] == "desktop-only")
    );
    let response = target.import(&exported, "wrong-password").await;
    assert_eq!(response.status(), 400);
    let id = response.headers()["x-nyaterm-request-id"]
        .to_str()
        .unwrap()
        .to_owned();
    assert_eq!(response.json::<Value>().await.unwrap()["request_id"], id);
    target.verify_secrets().await;
    let mut corrupt = exported.clone();
    corrupt[20] ^= 1;
    assert_eq!(target.import(&corrupt, WEB_PASSWORD).await.status(), 400);
    target.verify_secrets().await;
    // Authenticated archives must still reject tampered inner encrypted passwords
    // and plaintext in credential types outside the inline-password compatibility path.
    for plaintext_key in [false, true] {
        let mut invalid = desktop_snapshot.clone();
        if plaintext_key {
            invalid.keys.keys[0].key = Some("invalid-plaintext-private-key".into());
        } else {
            let token = invalid
                .sessions
                .connections
                .iter_mut()
                .find(|connection| connection.id == "connection")
                .unwrap()
                .auth
                .as_mut()
                .unwrap()
                .password
                .as_mut()
                .unwrap();
            let mut bytes = STANDARD.decode(&*token).unwrap();
            bytes[12] ^= 1;
            *token = STANDARD.encode(bytes);
        }
        invalid.payload_hash = calculate_payload_hash(&invalid).unwrap();
        let bytes = backup_crypto::encrypt_snapshot_bytes(
            &encode_portable_snapshot(&invalid).unwrap(),
            DESKTOP_PASSWORD,
        )
        .unwrap();
        assert_eq!(target.import(&bytes, DESKTOP_PASSWORD).await.status(), 400);
        target.verify_secrets().await;
    }
    // Valid encrypted snapshots with invalid note relationships must roll back all preceding writes.
    let mut invalid = desktop_snapshot.clone();
    invalid.sessions.connections.clear();
    invalid.notes=serde_json::from_value(json!({"folders":[{"id":"cycle","name":"bad","parent_id":"cycle","sort_order":0,"created_at_ms":0,"updated_at_ms":0}],"notes":[]})).unwrap();
    invalid.payload_hash = calculate_payload_hash(&invalid).unwrap();
    let invalid = backup_crypto::encrypt_snapshot_bytes(
        &encode_portable_snapshot(&invalid).unwrap(),
        DESKTOP_PASSWORD,
    )
    .unwrap();
    assert_eq!(
        target.import(&invalid, DESKTOP_PASSWORD).await.status(),
        400
    );
    target.verify_secrets().await;
    assert_eq!(
        target
            .client
            .get(format!("{}api/diagnostics/export", target.base))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        target
            .client
            .post(format!("{}api/backups/export", target.base))
            .header("origin", target.origin())
            .header("cookie", &target.cookie)
            .json(&json!({"password":WEB_PASSWORD}))
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    let frontend=target.request(reqwest::Method::POST,"logs/frontend").json(&json!({"entries":[{"level":"error","domain":"ui.error","event":"fixture.failure","message":SECRET,"data":{"password":SECRET,"host":"private.example","content":SECRET}}]})).send().await.unwrap();
    assert_eq!(frontend.status(), 200);
    let zip = target
        .request(reqwest::Method::GET, "diagnostics/export")
        .send()
        .await
        .unwrap();
    assert_eq!(zip.status(), 200);
    let mut archive = zip::ZipArchive::new(Cursor::new(zip.bytes().await.unwrap())).unwrap();
    let mut logs = String::new();
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).unwrap();
        let mut text = String::new();
        file.read_to_string(&mut text).unwrap();
        logs.push_str(&text);
        assert!(!file.name().ends_with("redb"));
    }
    for secret in [
        SECRET,
        LOGIN,
        DESKTOP_PASSWORD,
        WEB_PASSWORD,
        LEGACY_LONG_PASSWORD,
        AI_SECRET,
        "private.example",
        "private-key-fixture",
    ] {
        assert!(!logs.contains(secret), "Diagnostics leaked a secret");
    }
    assert!(logs.contains("backup.import"), "{logs}");
    assert!(logs.contains("fixture.failure"), "{logs}");
    assert!(logs.contains(&id));
    let oversized_batch = target
        .request(reqwest::Method::POST, "logs/frontend")
        .json(&json!({"entries":vec![json!({"level":"info"});51]}))
        .send()
        .await
        .unwrap();
    assert_eq!(oversized_batch.status(), 400);
    let oversized_body = target
        .request(reqwest::Method::POST, "logs/frontend")
        .json(&json!({"entries":[{"level":"info","message":"x".repeat(256*1024)}]}))
        .send()
        .await
        .unwrap();
    assert_eq!(oversized_body.status(), 413);
    for _ in 0..119 {
        assert_eq!(
            target
                .request(reqwest::Method::POST, "logs/frontend")
                .json(&json!({"entries":[]}))
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
    }
    assert_eq!(
        target
            .request(reqwest::Method::POST, "logs/frontend")
            .json(&json!({"entries":[]}))
            .send()
            .await
            .unwrap()
            .status(),
        429
    );
    let mut settings = target.command("get_app_settings", json!({})).await;
    settings["diagnostics"]["level"] = json!("warn");
    settings["diagnostics"]["retention_days"] = json!(1);
    target
        .command("save_app_settings", json!({"settings":settings}))
        .await;
    let suppressed_id = uuid::Uuid::new_v4().to_string();
    assert_eq!(
        target
            .request(reqwest::Method::POST, "commands/get_saved_connections")
            .header("x-nyaterm-request-id", &suppressed_id)
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let response = target
        .request(reqwest::Method::GET, "diagnostics/export")
        .send()
        .await
        .unwrap();
    let mut archive = zip::ZipArchive::new(Cursor::new(response.bytes().await.unwrap())).unwrap();
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).unwrap();
        let mut content = String::new();
        file.read_to_string(&mut content).unwrap();
        assert!(
            !content.contains(&suppressed_id),
            "Level changes must apply without a restart"
        );
    }
    settings["diagnostics"]["level"] = json!("info");
    target
        .command("save_app_settings", json!({"settings":settings}))
        .await;
    drop(target);
    let restarted = Server::start(&target_data, &dist, 42, "terminal/").await;
    restarted.verify_secrets().await;
    let import_json=br#"{"version":1,"sessions":[{"name":"imported","type":"ssh","host":"host.example","username":"root","auth":{"mode":"password","password":"fixture"}}]}"#;
    let response = restarted
        .upload("imports/connections", "source", "nyaterm_json", import_json)
        .await;
    assert_eq!(response.status(), 200);
    assert_eq!(response.json::<Value>().await.unwrap()["imported"], 1);
    // Exercise every browser-supported source using uploaded bytes, with no server paths.
    let mut xts = zip::ZipWriter::new(Cursor::new(Vec::new()));
    xts.start_file("fixture.xsh", zip::write::SimpleFileOptions::default())
        .unwrap();
    xts.write_all(b"[CONNECTION]\nProtocol=SSH\nHost=xshell.example\nPort=22\n[CONNECTION:AUTHENTICATION]\nUserName=root\n").unwrap();
    let xts = xts.finish().unwrap().into_inner();
    for (source, bytes) in [
        ("xshell", xts.as_slice()),
        ("mobaxterm", b"[Bookmarks]\nMoba=#109#0%mobaxterm.example%22%root%\n".as_slice()),
        ("securecrt", br#"<VanDyke><key name="Sessions"><key name="Fixture"><string name="Hostname">securecrt.example</string><string name="Protocol Name">SSH2</string><string name="Username">root</string></key></key></VanDyke>"#.as_slice()),
        ("electerm", br#"{"bookmarks":[{"id":"fixture","title":"Electerm","host":"electerm.example","username":"root","type":"ssh"}]}"#.as_slice()),
        ("windterm", br#"[{"session.label":"WindTerm","session.protocol":"SSH","session.target":"windterm.example","session.autoLogin":"{\"Password\":\"windterm-secret\",\"PasswordEnabled\":true,\"session.user\":\"root\"}"}]"#.as_slice()),
    ] {
        let response=restarted.upload("imports/connections","source",source,bytes).await;
        assert_eq!(response.status(),200,"{source}");
        assert_eq!(response.json::<Value>().await.unwrap()["imported"],1,"{source}");
    }
    let saved = restarted.command("get_saved_connections", json!({})).await;
    for (name, password) in [("WindTerm", "windterm-secret"), ("imported", "fixture")] {
        let id = saved
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["name"] == name)
            .unwrap()["id"]
            .as_str()
            .unwrap();
        assert_eq!(
            restarted
                .command("get_connection_password_value", json!({"id":id}))
                .await,
            json!(password)
        );
    }
    let over_limit = vec![b'x'; 10 * 1024 * 1024 + 1];
    assert_eq!(
        restarted
            .upload("imports/connections", "source", "nyaterm_json", &over_limit)
            .await
            .status(),
        413
    );
    assert_eq!(
        restarted
            .import(&vec![b'x'; 50 * 1024 * 1024 + 1], WEB_PASSWORD)
            .await
            .status(),
        413
    );
    restarted.verify_secrets().await;
    // Unknown source, encrypted profiles and Unix as well as Windows key paths are refused.
    assert_eq!(
        restarted
            .upload("imports/connections", "source", "finalshell", b"{}")
            .await
            .status(),
        400
    );
    let external_key=br#"[{"session.protocol":"SSH","session.target":"test","ssh.identityFilePath.unix":"/server/private/key"}]"#;
    assert_eq!(
        restarted
            .upload("imports/connections", "source", "windterm", external_key)
            .await
            .status(),
        400
    );
    let windterm =
        br#"[{"session.protocol":"SSH","session.target":"test","session.autoLogin":"encrypted"}]"#;
    assert_eq!(
        restarted
            .upload("imports/connections", "source", "windterm", windterm)
            .await
            .status(),
        400
    );
}
