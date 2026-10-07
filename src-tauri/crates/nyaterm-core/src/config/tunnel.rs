use super::{default_false, default_true, uuid_v4};
use crate::error::AppResult;
use crate::storage::{self, SettingsDocKey};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelConfig {
    #[serde(default = "uuid_v4")]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_tunnel_type")]
    pub tunnel_type: String,
    #[serde(default)]
    pub connection_id: Option<String>,
    #[serde(default)]
    pub listen_port: u16,
    #[serde(default)]
    pub target_host: String,
    #[serde(default)]
    pub target_port: u16,
    #[serde(default = "default_false")]
    pub is_open: bool,
    #[serde(default = "default_false")]
    pub auto_open: bool,
    #[serde(default = "default_true")]
    pub bind_localhost: bool,
    #[serde(default)]
    pub group_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelGroup {
    #[serde(default = "uuid_v4")]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub sort_order: u32,
}

fn default_tunnel_type() -> String {
    "local".to_string()
}

impl Default for TunnelConfig {
    fn default() -> Self {
        Self {
            id: uuid_v4(),
            name: String::new(),
            tunnel_type: default_tunnel_type(),
            connection_id: None,
            listen_port: 0,
            target_host: "127.0.0.1".to_string(),
            target_port: 0,
            is_open: false,
            auto_open: false,
            bind_localhost: true,
            group_id: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TunnelsConfig {
    #[serde(default)]
    pub tunnels: Vec<TunnelConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TunnelGroupsConfig {
    #[serde(default)]
    pub groups: Vec<TunnelGroup>,
}

pub fn load_tunnels(app: &impl Sized) -> AppResult<Vec<TunnelConfig>> {
    let _ = app;
    storage::list_tunnels()
}

pub fn save_tunnels(app: &impl Sized, tunnels: &[TunnelConfig]) -> AppResult<()> {
    let _ = app;
    storage::replace_tunnels(tunnels)
}

pub fn load_tunnel_groups(app: &impl Sized) -> AppResult<Vec<TunnelGroup>> {
    let _ = app;
    Ok(storage::load_settings_doc::<TunnelGroupsConfig>(SettingsDocKey::TunnelGroups)?.groups)
}

pub fn save_tunnel_groups(app: &impl Sized, groups: &[TunnelGroup]) -> AppResult<()> {
    let _ = app;
    storage::save_settings_doc(
        SettingsDocKey::TunnelGroups,
        &TunnelGroupsConfig {
            groups: groups.to_vec(),
        },
    )
}
