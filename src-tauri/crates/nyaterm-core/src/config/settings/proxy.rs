use super::super::default_false;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxySettings {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_proxy_protocol")]
    pub protocol: String,
    #[serde(default = "default_proxy_host")]
    pub host: String,
    #[serde(default = "default_proxy_port")]
    pub port: u16,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default, skip_serializing)]
    pub password: Option<String>,
}

fn default_proxy_protocol() -> String {
    "socks5".to_string()
}

fn default_proxy_host() -> String {
    "127.0.0.1".to_string()
}

fn default_proxy_port() -> u16 {
    1080
}

impl Default for ProxySettings {
    fn default() -> Self {
        Self {
            enabled: false,
            protocol: default_proxy_protocol(),
            host: default_proxy_host(),
            port: default_proxy_port(),
            command: None,
            username: None,
            password: None,
        }
    }
}
