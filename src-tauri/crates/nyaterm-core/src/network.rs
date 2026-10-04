use crate::config::{self, ConnectionNetwork, ProxySettings};
use crate::error::{AppError, AppResult};
use std::net::SocketAddr;
use tokio::io::{AsyncRead, AsyncWrite};

pub trait AsyncReadWrite: AsyncRead + AsyncWrite {}
impl<T: AsyncRead + AsyncWrite + ?Sized> AsyncReadWrite for T {}

pub type BoxedTransportStream = Box<dyn AsyncReadWrite + Unpin + Send + Sync>;

pub struct OpenedTransport {
    pub stream: BoxedTransportStream,
    pub local_addr: Option<SocketAddr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportRouteKind {
    Direct,
    Proxy,
    SshJump,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTransportRoute {
    pub kind: TransportRouteKind,
    pub proxy_id: Option<String>,
    pub proxy_jump_id: Option<String>,
}

pub fn resolve_transport_route(network: Option<&ConnectionNetwork>) -> ResolvedTransportRoute {
    let proxy_jump_id = network
        .and_then(|network| network.proxy_jump_id.as_deref())
        .filter(|id| !id.trim().is_empty())
        .map(ToOwned::to_owned);
    let proxy_id = network
        .and_then(|network| network.proxy_id.as_deref())
        .filter(|id| !id.trim().is_empty())
        .map(ToOwned::to_owned);

    if proxy_jump_id.is_some() {
        return ResolvedTransportRoute {
            kind: TransportRouteKind::SshJump,
            proxy_id,
            proxy_jump_id,
        };
    }

    if proxy_id.is_some() {
        return ResolvedTransportRoute {
            kind: TransportRouteKind::Proxy,
            proxy_id,
            proxy_jump_id: None,
        };
    }

    ResolvedTransportRoute {
        kind: TransportRouteKind::Direct,
        proxy_id: None,
        proxy_jump_id: None,
    }
}

pub async fn open_direct_transport(
    target_host: &str,
    target_port: u16,
) -> AppResult<OpenedTransport> {
    let stream = tokio::net::TcpStream::connect((target_host, target_port))
        .await
        .map_err(|error| {
            AppError::Channel(format!(
                "Direct TCP connection to {target_host}:{target_port} failed: {error}"
            ))
        })?;
    let local_addr = stream.local_addr().ok();
    Ok(OpenedTransport {
        stream: Box::new(stream),
        local_addr,
    })
}

pub async fn open_proxy_transport(
    proxy: ProxySettings,
    target_host: &str,
    target_port: u16,
) -> AppResult<OpenedTransport> {
    let proxy_addr = if proxy.host.contains(':') {
        format!("[{}]:{}", proxy.host, proxy.port)
    } else {
        format!("{}:{}", proxy.host, proxy.port)
    };
    if proxy.username.is_some() != proxy.password.is_some() {
        return Err(AppError::Config(
            "Proxy authentication requires both username and password".into(),
        ));
    }
    match proxy.protocol.as_str() {
        "socks5" => {
            let stream = match (&proxy.username, &proxy.password) {
                (Some(user), Some(pass)) => {
                    tokio_socks::tcp::Socks5Stream::connect_with_password(
                        proxy_addr.as_str(),
                        (target_host, target_port),
                        user,
                        pass,
                    )
                    .await
                }
                _ => {
                    tokio_socks::tcp::Socks5Stream::connect(
                        proxy_addr.as_str(),
                        (target_host, target_port),
                    )
                    .await
                }
            }
            .map_err(|error| AppError::Auth(format!("SOCKS5 proxy connection failed: {error}")))?
            .into_inner();
            let local_addr = stream.local_addr().ok();
            Ok(OpenedTransport {
                stream: Box::new(stream),
                local_addr,
            })
        }
        "http" => {
            let mut stream =
                tokio::net::TcpStream::connect(&proxy_addr)
                    .await
                    .map_err(|error| {
                        AppError::Channel(format!("HTTP proxy connection failed: {error}"))
                    })?;
            match (&proxy.username, &proxy.password) {
                (Some(user), Some(pass)) => {
                    async_http_proxy::http_connect_tokio_with_basic_auth(
                        &mut stream,
                        target_host,
                        target_port,
                        user,
                        pass,
                    )
                    .await
                }
                _ => {
                    async_http_proxy::http_connect_tokio(&mut stream, target_host, target_port)
                        .await
                }
            }
            .map_err(|error| AppError::Auth(format!("HTTP proxy tunnel failed: {error}")))?;
            let local_addr = stream.local_addr().ok();
            Ok(OpenedTransport {
                stream: Box::new(stream),
                local_addr,
            })
        }
        other => Err(AppError::Config(format!(
            "Unsupported proxy protocol '{other}'"
        ))),
    }
}

pub fn resolve_proxy(app: &impl Sized, proxy_id: &str) -> AppResult<ProxySettings> {
    let proxy_cfg = config::load_proxy_by_id(app, proxy_id)?
        .ok_or_else(|| AppError::Config(format!("Proxy '{proxy_id}' not found")))?;
    let password = proxy_cfg
        .password
        .as_ref()
        .map(|ciphertext| crate::utils::crypto::decrypt(ciphertext))
        .transpose()?;

    Ok(ProxySettings {
        enabled: true,
        protocol: proxy_cfg.protocol,
        host: proxy_cfg.host,
        port: proxy_cfg.port,
        command: proxy_cfg.command,
        username: proxy_cfg.username,
        password,
    })
}
