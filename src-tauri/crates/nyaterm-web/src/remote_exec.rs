//! Bounded private SSH exec channels, scoped to the authenticated user's session.
use crate::{
    error::{Result, WebError},
    session::{SessionProtocol, WebSession},
};
use russh::ChannelMsg;
use std::time::Duration;

struct ChannelGuard(Option<russh::Channel<russh::client::Msg>>);
impl Drop for ChannelGuard {
    fn drop(&mut self) {
        if let Some(channel) = self.0.take() {
            crate::observability::spawn(async move {
                let _ = tokio::time::timeout(Duration::from_secs(2), channel.close()).await;
            });
        }
    }
}
#[derive(serde::Serialize)]
pub struct Output {
    pub stdout: String,
    pub stderr: String,
    pub exit_status: Option<u32>,
}
pub async fn exec(session: &WebSession, script: &str) -> Result<Output> {
    if !matches!(session.protocol, SessionProtocol::Ssh)
        || !session.ready.load(std::sync::atomic::Ordering::Acquire)
    {
        return Err(WebError::bad("SSH session is not ready"));
    }
    let _permit = session
        .transfers
        .clone()
        .try_acquire_owned()
        .map_err(|_| WebError::bad("Too many remote operations"))?;
    let operation = async {
        let channel = session
            .handle
            .lock()
            .await
            .as_ref()
            .ok_or(WebError::bad("SSH disconnected"))?
            .channel_open_session()
            .await?;
        let mut guard = ChannelGuard(Some(channel));
        let channel = guard.0.as_mut().unwrap();
        channel.exec(true, script.as_bytes()).await?;
        channel.eof().await?;
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut status = None;
        while let Some(message) = channel.wait().await {
            match message {
                ChannelMsg::Data { data } => {
                    if stdout.len() + stderr.len() + data.len() > 4 * 1024 * 1024 {
                        return Err(WebError::bad("Remote output exceeds 4 MiB"));
                    }
                    stdout.extend_from_slice(&data);
                }
                ChannelMsg::ExtendedData { data, .. } => {
                    if stdout.len() + stderr.len() + data.len() > 4 * 1024 * 1024 {
                        return Err(WebError::bad("Remote output exceeds 4 MiB"));
                    }
                    stderr.extend_from_slice(&data);
                }
                ChannelMsg::ExitStatus { exit_status } => status = Some(exit_status),
                ChannelMsg::Close => break,
                _ => {}
            }
        }
        if status != Some(0) {
            return Err(WebError::bad("Remote command failed"));
        }
        Ok(Output {
            stdout: String::from_utf8_lossy(&stdout).into(),
            stderr: String::from_utf8_lossy(&stderr).into(),
            exit_status: status,
        })
    };
    tokio::select! {_=session.cancel.cancelled()=>Err(WebError::bad("Session closed")),result=tokio::time::timeout(Duration::from_secs(30),operation)=>result.map_err(|_|WebError::bad("Remote command timed out"))?}
}
pub fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
