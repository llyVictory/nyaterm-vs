pub mod ai;
pub mod auth;
pub mod commands;
pub mod error;
pub mod monitoring;
pub mod network;
pub mod notes;
pub mod otp;
pub mod plugins;
pub mod remote_exec;
pub mod session;
pub mod sftp;
pub mod state;
pub mod suggestions;
pub mod telnet;
pub mod vnc;

use crate::{auth::Owner, state::State};
use axum::{
    Extension, Router,
    extract::{DefaultBodyLimit, State as ExtractState},
    http::{HeaderValue, StatusCode},
    middleware,
    response::{
        Sse,
        sse::{Event, KeepAlive},
    },
    routing::{get, post},
};
use std::{convert::Infallible, sync::Arc, time::Duration};
use tower_http::{
    services::{ServeDir, ServeFile},
    set_header::SetResponseHeaderLayer,
};

async fn events(
    ExtractState(state): ExtractState<Arc<State>>,
    Extension(owner): Extension<Owner>,
) -> error::Result<Sse<impl futures_util::Stream<Item = std::result::Result<Event, Infallible>>>> {
    let login = state.login(&owner.0).await?;
    let mut receiver = login.events.subscribe();
    let stream = async_stream::stream! {
        yield Ok(Event::default().data(r#"{"event":"ready","payload":null}"#));
        loop{
            tokio::select!{
                _ = login.cancel.cancelled()=>{yield Ok(Event::default().event("expired").data("{}"));break;},
                next=receiver.recv()=>match next{
                    Ok(event)=>{if let Ok(data)=serde_json::to_string(&event){yield Ok(Event::default().data(data));}},
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_))=>{yield Ok(Event::default().event("expired").data("{}"));break;},
                    Err(_)=>break,
                }
            }
        }
    };
    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}
pub fn router(state: Arc<State>, dist: std::path::PathBuf) -> Router {
    let protected = Router::new()
        .route("/auth/session", get(auth::current))
        .route("/auth/logout", post(auth::sign_out))
        .route("/events", get(events))
        .route("/sessions", post(session::create_route))
        .route("/sessions/{id}/terminal", get(session::ws_route))
        .route("/sessions/{id}/vnc", get(vnc::ws_route))
        .route("/sessions/{id}/download", get(sftp::download))
        .route(
            "/sessions/{id}/upload",
            post(sftp::upload).layer(DefaultBodyLimit::disable()),
        )
        .route("/commands/{command}", post(commands::route))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth::guard));
    let api = protected
        .route("/auth/login", post(auth::sign_in))
        .fallback(|| async {
            (
                StatusCode::NOT_FOUND,
                axum::Json(serde_json::json!({"error":"API route not found"})),
            )
        })
        .layer(DefaultBodyLimit::max(4 * 1024 * 1024));
    let site = Router::new()
        .nest("/api", api)
        .route_service("/", ServeFile::new(dist.join("index.html")))
        .fallback_service(ServeDir::new(&dist).fallback(ServeFile::new(dist.join("index.html"))));
    let app = if state.base_path.is_empty() {
        site
    } else {
        // Axum nests an inner `/` at the exact prefix. Include the trailing
        // slash so the browser's directory URL reaches index.html.
        let directory = format!("{}/", state.base_path);
        Router::new().nest(&directory, site).route(
            &state.base_path,
            get(move || {
                let directory = directory.clone();
                async move { axum::response::Redirect::permanent(&directory) }
            }),
        )
    };
    app.layer(middleware::from_fn_with_state(state.clone(), auth::site_guard))
        .layer(SetResponseHeaderLayer::overriding(axum::http::header::CONTENT_SECURITY_POLICY,HeaderValue::from_static("default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob: https:; font-src 'self' data:; connect-src 'self'; frame-src 'self' blob:; frame-ancestors 'self'; object-src 'none'; base-uri 'self'; form-action 'self'")))
        .layer(SetResponseHeaderLayer::overriding(axum::http::header::CACHE_CONTROL,HeaderValue::from_static("no-store")))
        .layer(SetResponseHeaderLayer::overriding(axum::http::header::X_CONTENT_TYPE_OPTIONS,HeaderValue::from_static("nosniff")))
        .layer(SetResponseHeaderLayer::overriding(axum::http::header::REFERRER_POLICY,HeaderValue::from_static("same-origin")))
        .with_state(state)
}

pub async fn reap(state: Arc<State>) {
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {_ = state.shutdown.cancelled()=>break,_ = interval.tick()=>{}}
        let expired = state
            .logins
            .lock()
            .await
            .iter()
            .filter(|(_, l)| l.expires <= std::time::Instant::now())
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for owner in expired {
            state.close_owner(&owner).await;
        }
        state
            .prompts
            .lock()
            .await
            .retain(|_, prompt| !prompt.reply.is_closed());
        for session in state.sessions.lock().await.values() {
            if !session.attached.load(std::sync::atomic::Ordering::Acquire)
                && session.detached_at.lock().unwrap().elapsed() > Duration::from_secs(30)
            {
                session.cancel.cancel();
            }
        }
    }
}
