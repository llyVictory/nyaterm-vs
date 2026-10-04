use crate::{
    error::{Result, WebError},
    state::{Login, State},
};
use axum::{
    Json,
    body::Body,
    extract::{Request, State as ExtractState},
    http::{HeaderMap, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngCore;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

pub const COOKIE: &str = "nyaterm_session";
#[derive(Clone)]
pub struct Owner(pub String);
pub fn random_secret() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}
pub fn digest(value: &str) -> [u8; 32] {
    Sha256::digest(value.as_bytes()).into()
}
fn equal(a: &[u8], b: &[u8]) -> bool {
    use subtle::ConstantTimeEq;
    bool::from(a.ct_eq(b))
}
/// UI secret reveal uses real verification without replacing the login or
/// changing the server's encryption key hierarchy.
pub async fn verify_unlock(state: &State, args: &serde_json::Value) -> Result<bool> {
    let password = zeroize::Zeroizing::new(crate::commands::text(args, "password")?.to_owned());
    let mut attempts = state.login_attempts.lock().await;
    attempts.retain(|instant| instant.elapsed() < Duration::from_secs(60));
    if attempts.len() >= 20 {
        return Err(WebError(
            StatusCode::TOO_MANY_REQUESTS,
            "Try again later".into(),
        ));
    }
    attempts.push(Instant::now());
    drop(attempts);
    let settings = nyaterm_core::config::load_app_settings(&())?;
    let expected = match settings.security.master_password {
        Some(ciphertext) => {
            let stored = zeroize::Zeroizing::new(
                nyaterm_core::utils::crypto::decrypt_settings_secret(&ciphertext)?,
            );
            digest(&stored)
        }
        None => state.password_hash,
    };
    Ok(equal(&digest(&password), &expected))
}
pub fn cookie_owner(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|part| {
            let (name, value) = part.trim().split_once('=')?;
            (name == COOKIE && value.len() == 43).then(|| value.to_owned())
        })
}
pub fn validate_boundary(state: &State, headers: &HeaderMap, require_origin: bool) -> Result<()> {
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if host != state.host {
        return Err(WebError::forbidden());
    }
    match headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) {
        Some(origin) if origin != state.origin => Err(WebError::forbidden()),
        None if require_origin => Err(WebError::forbidden()),
        _ => Ok(()),
    }
}
pub async fn guard(
    ExtractState(state): ExtractState<Arc<State>>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let unsafe_method = !matches!(
        *request.method(),
        axum::http::Method::GET | axum::http::Method::HEAD
    );
    let websocket = request.headers().contains_key(header::UPGRADE);
    let result = async {
        validate_boundary(&state, request.headers(), unsafe_method || websocket)?;
        let token = cookie_owner(request.headers()).ok_or(WebError(
            StatusCode::UNAUTHORIZED,
            "Sign in required".into(),
        ))?;
        let login = state.login(&token).await?;
        if unsafe_method {
            let csrf = request
                .headers()
                .get("x-nyaterm-csrf")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if !equal(csrf.as_bytes(), login.csrf.as_bytes()) {
                return Err(WebError::forbidden());
            }
        }
        request.extensions_mut().insert(Owner(token));
        Ok(())
    }
    .await;
    match result {
        Ok(()) => next.run(request).await,
        Err(error) => error.into_response(),
    }
}
pub async fn site_guard(
    ExtractState(state): ExtractState<Arc<State>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    match validate_boundary(&state, request.headers(), false) {
        Ok(()) => next.run(request).await,
        Err(error) => error.into_response(),
    }
}
#[derive(Deserialize)]
pub struct LoginRequest {
    password: String,
}
pub async fn sign_in(
    ExtractState(state): ExtractState<Arc<State>>,
    headers: HeaderMap,
    Json(mut payload): Json<LoginRequest>,
) -> Result<Response> {
    use zeroize::Zeroize;
    validate_boundary(&state, &headers, true)?;
    if headers
        .get("x-nyaterm-request")
        .and_then(|v| v.to_str().ok())
        != Some("1")
    {
        return Err(WebError::forbidden());
    }
    let mut attempts = state.login_attempts.lock().await;
    attempts.retain(|instant| instant.elapsed() < Duration::from_secs(60));
    if attempts.len() >= 20 {
        return Err(WebError(
            StatusCode::TOO_MANY_REQUESTS,
            "Try again later".into(),
        ));
    }
    attempts.push(Instant::now());
    drop(attempts);
    let valid = equal(&digest(&payload.password), &state.password_hash);
    payload.password.zeroize();
    if !valid {
        return Err(WebError(
            StatusCode::UNAUTHORIZED,
            "Invalid credentials".into(),
        ));
    }
    // Re-authentication revokes the old owner's sessions, avoiding abandoned leases.
    if let Some(old) = cookie_owner(&headers) {
        state.close_owner(&old).await;
    }
    let owner = random_secret();
    let csrf = random_secret();
    let (events, _) = tokio::sync::broadcast::channel(256);
    let mut logins = state.logins.lock().await;
    if logins.len() >= 128 {
        return Err(WebError(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many active logins".into(),
        ));
    }
    logins.insert(
        owner.clone(),
        Arc::new(Login {
            csrf: csrf.clone(),
            expires: Instant::now() + Duration::from_secs(8 * 3600),
            events,
            cancel: state.shutdown.child_token(),
        }),
    );
    Ok((
        [(header::SET_COOKIE, cookie(&state, &owner, 8 * 3600))],
        Json(json!({"csrf":csrf})),
    )
        .into_response())
}
fn cookie(state: &State, token: &str, max_age: u32) -> String {
    format!(
        "{COOKIE}={token}; Path={}/; HttpOnly; SameSite=Strict; Max-Age={max_age}{}",
        state.base_path,
        if state.secure_cookie { "; Secure" } else { "" }
    )
}
pub async fn current(
    ExtractState(state): ExtractState<Arc<State>>,
    axum::Extension(owner): axum::Extension<Owner>,
) -> Result<Json<serde_json::Value>> {
    Ok(Json(json!({"csrf":state.login(&owner.0).await?.csrf})))
}
pub async fn sign_out(
    ExtractState(state): ExtractState<Arc<State>>,
    axum::Extension(owner): axum::Extension<Owner>,
) -> Response {
    state.close_owner(&owner.0).await;
    (
        [(header::SET_COOKIE, cookie(&state, "", 0))],
        Json(json!({})),
    )
        .into_response()
}
