//! Authenticated proxy and recursive SSH jump transports for Web sessions.
use crate::{
    error::{Result, WebError},
    session,
    state::State,
};
use futures_util::future::BoxFuture;
use nyaterm_core::{
    config::{self, ConnectionNetwork},
    network::{self, OpenedTransport, TransportRouteKind},
};
use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_util::sync::CancellationToken;

pub fn open(
    state: Arc<State>,
    owner: String,
    cancel: CancellationToken,
    host: String,
    port: u16,
    route: Option<ConnectionNetwork>,
) -> BoxFuture<'static, Result<OpenedTransport>> {
    Box::pin(async move {
        validate_target(&host, port)?;
        tokio::select! {
            _ = cancel.cancelled() => Err(WebError::bad("Connection cancelled")),
            result = tokio::time::timeout(std::time::Duration::from_secs(120), open_inner(state, owner, cancel.clone(), host, port, route, Vec::new())) => result.unwrap_or(Err(WebError::bad("Network connection timed out"))),
        }
    })
}
pub fn validate_target(host: &str, port: u16) -> Result<()> {
    if host.is_empty()
        || host.len() > 253
        || host.chars().any(|c| c.is_whitespace() || c.is_control())
        || port == 0
    {
        return Err(WebError::bad("Invalid host or port"));
    }
    Ok(())
}
fn open_inner(
    state: Arc<State>,
    owner: String,
    cancel: CancellationToken,
    host: String,
    port: u16,
    route: Option<ConnectionNetwork>,
    mut visited: Vec<String>,
) -> BoxFuture<'static, Result<OpenedTransport>> {
    Box::pin(async move {
        validate_target(&host, port)?;
        let resolved = network::resolve_transport_route(route.as_ref());
        match resolved.kind {
            TransportRouteKind::Direct => Ok(network::open_direct_transport(&host, port)
                .await
                .map_err(|error| {
                    crate::observability::record_error(&error);
                    WebError::bad(error.to_string())
                })?),
            TransportRouteKind::Proxy => {
                let proxy = network::resolve_proxy(&(), resolved.proxy_id.as_deref().unwrap())
                    .map_err(|e| {
                        crate::observability::record_error(&e);
                        match e {
                            nyaterm_core::error::AppError::Crypto(_) => {
                                WebError::bad("Proxy credentials could not be decrypted")
                            }
                            _ => WebError::bad("Proxy configuration is missing or invalid"),
                        }
                    })?;
                if !matches!(proxy.protocol.as_str(), "socks5" | "http") {
                    return Err(WebError::bad(
                        "Web supports SOCKS5 and HTTP CONNECT proxies only",
                    ));
                }
                validate_target(&proxy.host, proxy.port)?;
                Ok(network::open_proxy_transport(proxy, &host, port)
                    .await
                    .map_err(|error| {
                        crate::observability::record_error(&error);
                        WebError::bad(error.to_string())
                    })?)
            }
            TransportRouteKind::SshJump => {
                let id = resolved.proxy_jump_id.unwrap();
                if visited.contains(&id) || visited.len() >= 8 {
                    return Err(WebError::bad("SSH jump cycle or depth limit exceeded"));
                }
                visited.push(id.clone());
                let connection = config::load_connection_by_id(&(), &id)?;
                if !matches!(connection.config, config::ConnectionType::Ssh { .. }) {
                    return Err(WebError::bad("SSH jump must refer to an SSH connection"));
                }
                let target = session::jump_target(&id)?;
                let transport = open_inner(
                    state.clone(),
                    owner.clone(),
                    cancel.clone(),
                    target.host.clone(),
                    target.port,
                    target.network.clone(),
                    visited,
                )
                .await?;
                let handle =
                    session::authenticate(&state, &owner, &cancel, target, transport.stream)
                        .await?;
                let channel = handle
                    .channel_open_direct_tcpip(&host, u32::from(port), "127.0.0.1", 0)
                    .await?;
                Ok(OpenedTransport {
                    stream: Box::new(ForwardedStream {
                        stream: channel.into_stream(),
                        _handle: handle,
                    }),
                    local_addr: None,
                })
            }
        }
    })
}
struct ForwardedStream {
    stream: russh::ChannelStream<russh::client::Msg>,
    _handle: russh::client::Handle<session::Handler>,
}
impl AsyncRead for ForwardedStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stream).poll_read(cx, buf)
    }
}
impl AsyncWrite for ForwardedStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write(cx, buf)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}
