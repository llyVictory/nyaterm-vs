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
        tracing::warn!(
            event = "auth.rate_limited",
            reason = "attempt_budget",
            "Web sign-in rate limited"
        );
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
pub fn validate_boundary(headers: &HeaderMap, require_origin: bool) -> Result<()> {
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    match headers.get(header::ORIGIN) {
        Some(value) => {
            let origin = value.to_str().map_err(|_| WebError::forbidden())?;
            let url = url::Url::parse(origin).map_err(|_| WebError::forbidden())?;
            if !matches!(url.scheme(), "http" | "https")
                || url.origin().ascii_serialization() != origin
                || url[url::Position::BeforeHost..url::Position::AfterPort] != *host
            {
                return Err(WebError::forbidden());
            }
            Ok(())
        }
        None if require_origin => Err(WebError::forbidden()),
        None => Ok(()),
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
        validate_boundary(request.headers(), unsafe_method || websocket)?;
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
pub async fn site_guard(request: Request<Body>, next: Next) -> Response {
    match validate_boundary(request.headers(), false) {
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
    validate_boundary(&headers, true)?;
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
        tracing::warn!(
            event = "auth.rate_limited",
            reason = "attempt_budget",
            "Web sign-in rate limited"
        );
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
        tracing::warn!(
            event = "auth.rejected",
            reason = "invalid_credentials",
            "Web sign-in rejected"
        );
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
    tracing::info!(event = "auth.accepted", "Web sign-in accepted");
    Ok((
        [(
            header::SET_COOKIE,
            cookie(&state, &headers, &owner, 8 * 3600),
        )],
        Json(json!({"csrf":csrf})),
    )
        .into_response())
}
fn cookie(state: &State, headers: &HeaderMap, token: &str, max_age: u32) -> String {
    let secure = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|origin| origin.starts_with("https://"));
    format!(
        "{COOKIE}={token}; Path={}/; HttpOnly; SameSite=Strict; Max-Age={max_age}{}",
        state.base_path,
        if secure { "; Secure" } else { "" }
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
    headers: HeaderMap,
) -> Response {
    state.close_owner(&owner.0).await;
    tracing::info!(event = "auth.signed_out", "Web administrator signed out");
    (
        [(header::SET_COOKIE, cookie(&state, &headers, "", 0))],
        Json(json!({})),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundary_accepts_any_host_but_rejects_cross_origin_requests() {
        for (host, origin, allowed) in [
            ("192.168.1.10:8080", "http://192.168.1.10:8080", true),
            ("terminal.example", "https://terminal.example", true),
            ("[2001:db8::1]:8080", "http://[2001:db8::1]:8080", true),
            ("terminal.example", "https://evil.example", false),
            (
                "terminal.example:8080",
                "http://terminal.example:8081",
                false,
            ),
            ("terminal.example", "null", false),
            ("terminal.example", "https://terminal.example/path", false),
            ("terminal.example", "https://user@terminal.example", false),
        ] {
            let mut headers = HeaderMap::new();
            headers.insert(header::HOST, host.parse().unwrap());
            headers.insert(header::ORIGIN, origin.parse().unwrap());
            assert_eq!(
                validate_boundary(&headers, true).is_ok(),
                allowed,
                "{origin}"
            );
        }
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, "any.example".parse().unwrap());
        assert!(validate_boundary(&headers, false).is_ok());
        assert!(validate_boundary(&headers, true).is_err());
        headers.insert(
            header::ORIGIN,
            axum::http::HeaderValue::from_bytes(b"\xff").unwrap(),
        );
        assert!(validate_boundary(&headers, false).is_err());
    }
}
