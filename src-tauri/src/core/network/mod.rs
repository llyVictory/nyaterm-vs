use crate::config::{ConnectionNetwork, ProxySettings};
use crate::error::{AppError, AppResult};
use tauri::AppHandle;

pub use nyaterm_core::network::{BoxedTransportStream, OpenedTransport};

pub use nyaterm_core::network::{TransportRouteKind, resolve_proxy, resolve_transport_route};

pub async fn open_tcp_transport(
    app: &AppHandle,
    target_host: &str,
    target_port: u16,
    network: Option<&ConnectionNetwork>,
    owner_window_label: Option<String>,
) -> AppResult<OpenedTransport> {
    let route = resolve_transport_route(network);
    match route.kind {
        TransportRouteKind::SshJump => {
            let jump_id = route
                .proxy_jump_id
                .as_deref()
                .ok_or_else(|| AppError::Config("ProxyJump id is empty".to_string()))?;
            let stream = crate::core::ssh::open_ssh_direct_tcpip_stream(
                app,
                jump_id,
                target_host,
                target_port,
                owner_window_label,
            )
            .await
            .map_err(|error| AppError::Channel(format!("SSH jump connection failed: {error}")))?;
            Ok(OpenedTransport {
                stream: Box::new(stream),
                local_addr: None,
            })
        }
        TransportRouteKind::Proxy => {
            let proxy_id = route
                .proxy_id
                .as_deref()
                .ok_or_else(|| AppError::Config("Proxy id is empty".to_string()))?;
            let proxy = resolve_proxy(app, proxy_id)?;
            open_proxy_transport(proxy, target_host, target_port).await
        }
        TransportRouteKind::Direct => open_direct_transport(target_host, target_port).await,
    }
}

async fn open_direct_transport(host: &str, port: u16) -> AppResult<OpenedTransport> {
    let transport = nyaterm_core::network::open_direct_transport(host, port).await?;
    Ok(OpenedTransport {
        stream: transport.stream,
        local_addr: transport.local_addr,
    })
}

async fn open_proxy_transport(
    proxy: ProxySettings,
    target_host: &str,
    target_port: u16,
) -> AppResult<OpenedTransport> {
    match proxy.protocol.as_str() {
        "socks5" | "http" => {
            let transport =
                nyaterm_core::network::open_proxy_transport(proxy, target_host, target_port)
                    .await?;
            Ok(OpenedTransport {
                stream: transport.stream,
                local_addr: transport.local_addr,
            })
        }
        "proxycommand" => {
            let stream = crate::core::ssh::open_proxy_command_stream(
                proxy.command.as_deref(),
                target_host,
                target_port,
                "",
            )
            .await
            .map_err(|error| AppError::Auth(format!("ProxyCommand failed: {error}")))?;
            Ok(OpenedTransport {
                stream: Box::new(stream),
                local_addr: None,
            })
        }
        other => Err(AppError::Config(format!(
            "Unsupported proxy protocol '{other}'"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_network_resolves_direct() {
        let route = resolve_transport_route(None);
        assert_eq!(route.kind, TransportRouteKind::Direct);
    }

    #[test]
    fn proxy_id_resolves_proxy_transport() {
        let network = ConnectionNetwork {
            proxy_id: Some("proxy-1".to_string()),
            proxy_jump_id: None,
        };
        let route = resolve_transport_route(Some(&network));
        assert_eq!(route.kind, TransportRouteKind::Proxy);
        assert_eq!(route.proxy_id.as_deref(), Some("proxy-1"));
    }

    #[test]
    fn proxy_jump_id_resolves_ssh_jump_transport() {
        let network = ConnectionNetwork {
            proxy_id: Some("proxy-1".to_string()),
            proxy_jump_id: Some("jump-1".to_string()),
        };
        let route = resolve_transport_route(Some(&network));
        assert_eq!(route.kind, TransportRouteKind::SshJump);
        assert_eq!(route.proxy_jump_id.as_deref(), Some("jump-1"));
    }
}
