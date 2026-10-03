use base64::{Engine, engine::general_purpose::STANDARD};
use nyaterm_web::{auth, state::State};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tokio::sync::Mutex;

fn secret(name: &str) -> Result<zeroize::Zeroizing<String>, Box<dyn std::error::Error>> {
    let value = if let Ok(path) = std::env::var(format!("{name}_FILE")) {
        std::fs::read_to_string(path)?
            .trim_end_matches(['\r', '\n'])
            .to_owned()
    } else {
        std::env::var(name).map_err(|_| format!("{name} or {name}_FILE is required"))?
    };
    Ok(zeroize::Zeroizing::new(value))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let password = secret("NYATERM_WEB_PASSWORD")?;
    if password.len() < 32 {
        return Err("NYATERM_WEB_PASSWORD must contain at least 32 random characters".into());
    }
    let encoded = secret("NYATERM_WEB_ENCRYPTION_KEY")?;
    let bytes = zeroize::Zeroizing::new(
        STANDARD
            .decode(encoded.as_bytes())
            .map_err(|_| "Invalid encryption key encoding")?,
    );
    let key: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| "Encryption key must decode to 32 bytes")?;
    nyaterm_core::utils::crypto::set_server_key_material(key);
    let public =
        std::env::var("NYATERM_WEB_PUBLIC_URL").unwrap_or_else(|_| "http://localhost:8080/".into());
    let url = url::Url::parse(&public)?;
    let origin = url.origin().ascii_serialization();
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("Invalid public URL".into());
    }
    let loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    if url.scheme() != "https" && !loopback {
        return Err("Non-loopback public URLs require HTTPS".into());
    }
    let base_path = url.path().trim_end_matches('/').to_owned();
    if base_path.contains('%') || base_path.contains("//") {
        return Err("Invalid base path".into());
    }
    let host = url[url::Position::BeforeHost..url::Position::AfterPort].to_owned();
    let data = PathBuf::from(
        std::env::var("NYATERM_WEB_DATA_DIR").unwrap_or_else(|_| "./nyaterm-web-data".into()),
    );
    std::fs::create_dir_all(&data)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o700))?;
    }
    nyaterm_core::storage::init(&data)?;
    // Existing encrypted master-password data is not silently discarded. Web
    // unwraps it with the external server key, preserving the existing key hierarchy.
    if let Some(secret) = nyaterm_core::storage::load_settings_doc::<
        nyaterm_core::config::AppSettings,
    >(nyaterm_core::storage::SettingsDocKey::AppSettings)?
    .security
    .master_password
    {
        let plain = nyaterm_core::utils::crypto::decrypt_settings_secret(&secret)?;
        nyaterm_core::utils::crypto::set_master_password(Some(plain));
        nyaterm_core::utils::crypto::verify_master_key_token()?;
    }
    nyaterm_core::utils::crypto::verify_master_key_token()?;
    let dist = PathBuf::from(std::env::var("NYATERM_WEB_DIST").unwrap_or_else(|_| "dist".into()));
    if !dist.join("index.html").is_file() {
        return Err("React dist/index.html is missing; run pnpm build:web".into());
    }
    let state = Arc::new(State {
        origin,
        host,
        base_path,
        secure_cookie: url.scheme() == "https",
        password_hash: auth::digest(&password),
        logins: Mutex::new(HashMap::new()),
        sessions: Mutex::new(HashMap::new()),
        prompts: Mutex::new(HashMap::new()),
        ai_streams: Mutex::new(HashMap::new()),
        login_attempts: Mutex::new(Vec::new()),
        mutation: Mutex::new(()),
        shutdown: tokio_util::sync::CancellationToken::new(),
    });
    tokio::spawn(nyaterm_web::reap(state.clone()));
    let bind = std::env::var("NYATERM_WEB_BIND").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    eprintln!("NyaTerm Web listening on {bind}");
    let shutdown = state.shutdown.clone();
    tokio::spawn(async move {
        #[cfg(unix)]
        {
            let mut terminate =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .expect("SIGTERM handler");
            tokio::select! {_ = tokio::signal::ctrl_c()=>{},_ = terminate.recv()=>{}}
        }
        #[cfg(not(unix))]
        {
            let _ = tokio::signal::ctrl_c().await;
        }
        shutdown.cancel();
    });
    use std::future::IntoFuture;
    let stop = state.shutdown.clone();
    let server = axum::serve(listener, nyaterm_web::router(state.clone(), dist))
        .with_graceful_shutdown(async move { stop.cancelled().await })
        .into_future();
    tokio::pin!(server);
    tokio::select! {
        result=&mut server=>result?,
        _ = state.shutdown.cancelled()=> {let _=tokio::time::timeout(std::time::Duration::from_secs(5),&mut server).await;}
    }
    for session in state.sessions.lock().await.values() {
        session.cancel.cancel();
    }
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while !state.sessions.lock().await.is_empty() {
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    })
    .await;
    Ok(())
}
