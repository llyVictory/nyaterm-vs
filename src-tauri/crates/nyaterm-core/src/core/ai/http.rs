use genai::WebConfig;
use reqwest::{Client, ClientBuilder, NoProxy, Proxy, Url};

use crate::config::{AiProxyMode, AiProxyProtocol, AiProxySettings, AiSettings};
use crate::error::{AppError, AppResult};

use super::model::ai_request_headers;

/// Every built-in AI transport uses this builder. Keep timeouts at the request /
/// stream level: a total client timeout would interrupt long-running streams.
pub fn build_http_client(settings: &AiSettings) -> AppResult<Client> {
    let web_config = WebConfig::default().with_default_headers(ai_request_headers(settings)?);
    let builder = web_config.apply_to_builder(Client::builder());
    apply_proxy(builder, &settings.proxy)?
        .build()
        .map_err(|_| AppError::Config("Failed to build AI HTTP client".to_string()))
}

fn apply_proxy(builder: ClientBuilder, settings: &AiProxySettings) -> AppResult<ClientBuilder> {
    match settings.mode {
        AiProxyMode::System => Ok(builder),
        AiProxyMode::Direct => Ok(builder.no_proxy()),
        AiProxyMode::Custom => {
            let url = proxy_url(settings)?;
            let username = settings
                .username
                .as_deref()
                .filter(|value| !value.is_empty());
            let password = settings
                .password
                .as_deref()
                .filter(|value| !value.is_empty());
            if password.is_some() && username.is_none() {
                return Err(AppError::Config(
                    "AI proxy password requires a username".to_string(),
                ));
            }
            if settings.protocol == AiProxyProtocol::Socks5
                && (username.is_some_and(|value| value.len() > 255)
                    || password.is_some_and(|value| value.len() > 255))
            {
                return Err(AppError::Config(
                    "SOCKS5 proxy credentials must be at most 255 bytes".to_string(),
                ));
            }
            let mut proxy = Proxy::all(url)
                .map_err(|_| AppError::Config("Invalid AI proxy configuration".to_string()))?
                .no_proxy(NoProxy::from_string(&settings.no_proxy));
            if let Some(username) = username {
                proxy = proxy.basic_auth(username, password.unwrap_or_default());
            }
            // Explicitly disable automatic proxies so bypassed requests connect
            // directly, even when environment / system proxies are configured.
            Ok(builder.no_proxy().proxy(proxy))
        }
    }
}

fn proxy_url(settings: &AiProxySettings) -> AppResult<Url> {
    let invalid = || AppError::Config("Invalid AI proxy host or port".to_string());
    let host = settings.host.trim();
    if host.is_empty()
        || settings.port == 0
        || host
            .chars()
            .any(|value| value.is_whitespace() || "/\\@?#%".contains(value))
    {
        return Err(invalid());
    }
    let unbracketed = host
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(host);
    let host = if unbracketed.contains(':') {
        let address = unbracketed
            .parse::<std::net::Ipv6Addr>()
            .map_err(|_| invalid())?;
        format!("[{address}]")
    } else {
        host.to_string()
    };
    let scheme = match settings.protocol {
        AiProxyProtocol::Http => "http",
        AiProxyProtocol::Socks5 => "socks5h",
    };
    // Validate DNS names using a special URL scheme too: SOCKS URLs otherwise
    // allow opaque hosts with characters that cannot be used as a DNS name.
    Url::parse(&format!("http://{host}:{}", settings.port)).map_err(|_| invalid())?;
    Url::parse(&format!("{scheme}://{host}:{}", settings.port)).map_err(|_| invalid())
}

#[cfg(test)]
mod tests;
