use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::json;
use std::{
    path::Path,
    process::{Child, Command, Stdio},
    time::Duration,
};
struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn command(data: &Path, dist: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_nyaterm-web"));
    cmd.env_remove("NYATERM_WEB_PASSWORD")
        .env_remove("NYATERM_WEB_PASSWORD_FILE")
        .env_remove("NYATERM_WEB_ENCRYPTION_KEY")
        .env_remove("NYATERM_WEB_ENCRYPTION_KEY_FILE")
        .env("NYATERM_WEB_DATA_DIR", data)
        .env("NYATERM_WEB_DIST", dist)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    cmd
}
#[tokio::test]
async fn bootstrap_uses_external_secret_files_and_rejects_wrong_key() {
    let temporary = tempfile::tempdir().unwrap();
    let data = temporary.path().join("data");
    let dist = temporary.path().join("dist");
    std::fs::create_dir_all(&dist).unwrap();
    std::fs::write(dist.join("index.html"), "<html>NyaTerm Web</html>").unwrap();
    let password = "test-login-secret-at-least-32-characters";
    let key = STANDARD.encode([19u8; 32]);
    let password_file = temporary.path().join("password");
    let key_file = temporary.path().join("encryption");
    std::fs::write(&password_file, password).unwrap();
    std::fs::write(&key_file, &key).unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let host = listener.local_addr().unwrap();
    drop(listener);
    let origin = format!("http://{host}");
    let mut process = Process(
        command(&data, &dist)
            .env("NYATERM_WEB_PASSWORD_FILE", &password_file)
            .env("NYATERM_WEB_ENCRYPTION_KEY_FILE", &key_file)
            .env("NYATERM_WEB_PUBLIC_URL", format!("{origin}/terminal/"))
            .env("NYATERM_WEB_BIND", host.to_string())
            .spawn()
            .unwrap(),
    );
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let mut ready = false;
    for _ in 0..100 {
        if let Ok(response) = client.get(format!("{origin}/terminal/")).send().await {
            let status = response.status();
            let body = response.text().await.unwrap();
            assert_eq!(status, 200, "Unexpected static response: {body}");
            assert!(body.contains("NyaTerm"));
            ready = true;
            break;
        }
        if let Some(status) = process.0.try_wait().unwrap() {
            panic!("Server exited during startup: {status}");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(ready, "Server did not start");
    let response = client
        .post(format!("{origin}/terminal/api/auth/login"))
        .header("origin", &origin)
        .header("x-nyaterm-request", "1")
        .json(&json!({"password":password}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let cookie = response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let csrf = response.json::<serde_json::Value>().await.unwrap()["csrf"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        client
            .post(format!("{origin}/terminal/api/commands/list_sessions"))
            .header("origin", &origin)
            .header("cookie", cookie)
            .header("x-nyaterm-csrf", csrf)
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    process.0.kill().unwrap();
    process.0.wait().unwrap();
    std::fs::write(&key_file, STANDARD.encode([20u8; 32])).unwrap();
    let output = command(&data, &dist)
        .env("NYATERM_WEB_PASSWORD_FILE", &password_file)
        .env("NYATERM_WEB_ENCRYPTION_KEY_FILE", &key_file)
        .env("NYATERM_WEB_PUBLIC_URL", &origin)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let logs = String::from_utf8_lossy(&output.stderr);
    assert!(!logs.contains(password));
    assert!(!logs.contains(&key));
}
