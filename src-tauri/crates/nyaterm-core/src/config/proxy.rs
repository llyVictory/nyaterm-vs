use super::uuid_v4;
use crate::error::AppResult;
use crate::storage::{self, SettingsDocKey};
use serde::{Deserialize, Serialize};

fn default_protocol() -> String {
    "socks5".to_string()
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    1080
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    #[serde(default = "uuid_v4")]
    pub id: String,
    pub name: String,
    #[serde(default = "default_protocol")]
    pub protocol: String,
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub group_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyGroup {
    #[serde(default = "uuid_v4")]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub sort_order: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProxyGroupsConfig {
    #[serde(default)]
    pub groups: Vec<ProxyGroup>,
}

pub fn load_proxies(app: &impl Sized) -> AppResult<Vec<ProxyConfig>> {
    let _ = app;
    storage::list_proxies()
}

pub fn save_proxies(app: &impl Sized, proxies: &[ProxyConfig]) -> AppResult<()> {
    let _ = app;
    storage::replace_proxies(proxies)
}

pub fn load_proxy_by_id(app: &impl Sized, id: &str) -> AppResult<Option<ProxyConfig>> {
    let proxies = load_proxies(app)?;
    Ok(proxies.into_iter().find(|p| p.id == id))
}

pub fn load_proxy_groups(app: &impl Sized) -> AppResult<Vec<ProxyGroup>> {
    let _ = app;
    Ok(storage::load_settings_doc::<ProxyGroupsConfig>(SettingsDocKey::ProxyGroups)?.groups)
}

pub fn save_proxy_groups(app: &impl Sized, groups: &[ProxyGroup]) -> AppResult<()> {
    let _ = app;
    storage::save_settings_doc(
        SettingsDocKey::ProxyGroups,
        &ProxyGroupsConfig {
            groups: groups.to_vec(),
        },
    )
}
