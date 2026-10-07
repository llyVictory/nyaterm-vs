use crate::{
    error::{Result, WebError},
    session::WebSession,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, broadcast, oneshot};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Serialize, Deserialize)]
pub struct Event {
    pub event: String,
    pub payload: Value,
}
pub struct Login {
    pub csrf: String,
    pub expires: Instant,
    pub events: broadcast::Sender<Event>,
    pub cancel: CancellationToken,
}
pub struct Prompt {
    pub owner: String,
    pub reply: oneshot::Sender<Value>,
}
pub struct State {
    pub base_path: String,
    pub password_hash: [u8; 32],
    pub logins: Mutex<HashMap<String, Arc<Login>>>,
    pub sessions: Mutex<HashMap<String, Arc<WebSession>>>,
    pub prompts: Mutex<HashMap<String, Prompt>>,
    pub ai_streams: Mutex<HashMap<(String, String), CancellationToken>>,
    pub login_attempts: Mutex<Vec<Instant>>,
    pub mutation: Mutex<()>,
    pub shutdown: CancellationToken,
}
impl State {
    pub async fn login(&self, owner: &str) -> Result<Arc<Login>> {
        self.logins
            .lock()
            .await
            .get(owner)
            .filter(|login| login.expires > Instant::now() && !login.cancel.is_cancelled())
            .cloned()
            .ok_or(WebError(
                axum::http::StatusCode::UNAUTHORIZED,
                "Sign in required".into(),
            ))
    }
    pub async fn event(&self, owner: &str, event: &str, payload: Value) {
        if let Ok(login) = self.login(owner).await {
            let _ = login.events.send(Event {
                event: event.into(),
                payload,
            });
        }
    }
    pub async fn broadcast(&self, event: &str, payload: Value) {
        for login in self.logins.lock().await.values() {
            let _ = login.events.send(Event {
                event: event.into(),
                payload: payload.clone(),
            });
        }
    }
    pub async fn prompt(
        self: &Arc<Self>,
        owner: &str,
        event: &str,
        mut payload: Value,
        cancel: &CancellationToken,
    ) -> Result<Value> {
        let id = uuid::Uuid::new_v4().to_string();
        let (reply, rx) = oneshot::channel();
        payload["requestId"] = Value::String(id.clone());
        payload["targetWindowLabel"] = Value::Null;
        self.prompts.lock().await.insert(
            id.clone(),
            Prompt {
                owner: owner.into(),
                reply,
            },
        );
        // HTTP disconnects can drop this future before its normal cleanup.
        let _cleanup = PromptCleanup {
            state: self.clone(),
            id: id.clone(),
        };
        self.event(owner, event, payload).await;
        let result = tokio::select! {
            _ = cancel.cancelled() => Err(WebError::bad("Session creation cancelled")),
            response = tokio::time::timeout(Duration::from_secs(90), rx) => response.ok().and_then(|r| r.ok()).ok_or(WebError::bad("Authentication prompt expired")),
        };
        self.prompts.lock().await.remove(&id);
        result
    }
    pub async fn session(&self, owner: &str, id: &str) -> Result<Arc<WebSession>> {
        self.sessions
            .lock()
            .await
            .get(id)
            .filter(|s| s.owner == owner && !s.cancel.is_cancelled())
            .cloned()
            .ok_or(WebError(
                axum::http::StatusCode::NOT_FOUND,
                "Session not found".into(),
            ))
    }
    pub async fn close_owner(&self, owner: &str) {
        tracing::info!(
            event = "auth.session_closed",
            "Web login closed and sessions cancelled"
        );
        if let Some(login) = self.logins.lock().await.remove(owner) {
            login.cancel.cancel();
        }
        for session in self
            .sessions
            .lock()
            .await
            .values()
            .filter(|s| s.owner == owner)
        {
            session.cancel.cancel();
        }
        self.prompts
            .lock()
            .await
            .retain(|_, prompt| prompt.owner != owner);
    }
}

struct PromptCleanup {
    state: Arc<State>,
    id: String,
}
impl Drop for PromptCleanup {
    fn drop(&mut self) {
        let state = self.state.clone();
        let id = self.id.clone();
        crate::observability::spawn(async move {
            state.prompts.lock().await.remove(&id);
        });
    }
}
