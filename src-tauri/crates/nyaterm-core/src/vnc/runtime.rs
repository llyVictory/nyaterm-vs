use crate::{config::ConnectionNetwork, error::AppResult, network::OpenedTransport};
use futures_util::future::BoxFuture;
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;

type EventSink = dyn Fn(Option<&str>, &str, Value) -> AppResult<()> + Send + Sync;
type Transport = dyn Fn(
        String,
        u16,
        Option<ConnectionNetwork>,
        Option<String>,
    ) -> BoxFuture<'static, AppResult<OpenedTransport>>
    + Send
    + Sync;

/// The protocol engine owns no native application handles or IPC channels.
#[derive(Clone)]
pub struct VncContext {
    pub events: Arc<EventSink>,
    pub transport: Arc<Transport>,
}
impl VncContext {
    pub fn emit<T: Serialize>(&self, event: &str, payload: T) -> AppResult<()> {
        (self.events)(None, event, serde_json::to_value(payload)?)
    }
    pub fn emit_to<T: Serialize>(&self, target: &str, event: &str, payload: T) -> AppResult<()> {
        (self.events)(Some(target), event, serde_json::to_value(payload)?)
    }
}
#[derive(Clone)]
pub struct FrameChannel(pub Arc<dyn Fn(Vec<u8>) -> AppResult<()> + Send + Sync>);
impl FrameChannel {
    pub fn send(&self, frame: Vec<u8>) -> AppResult<()> {
        (self.0)(frame)
    }
}
pub async fn open_tcp_transport(
    context: &VncContext,
    host: &str,
    port: u16,
    network: Option<&ConnectionNetwork>,
    owner: Option<String>,
) -> AppResult<OpenedTransport> {
    (context.transport)(host.to_owned(), port, network.cloned(), owner).await
}
