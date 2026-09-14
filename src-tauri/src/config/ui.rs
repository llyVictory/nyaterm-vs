use super::{default_false, uuid_v4};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

fn default_leaf_id() -> String {
    format!("pane-{}", uuid_v4())
}

fn default_split_id() -> String {
    format!("split-{}", uuid_v4())
}

fn default_split_ratio() -> f64 {
    0.5
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDesktopDisplayMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_height: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view_only: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clipboard_enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum RestorablePaneNode {
    #[serde(rename = "leaf")]
    Leaf {
        #[serde(default = "default_leaf_id")]
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pane_kind: Option<String>,
        title: String,
        session_type: String,
        connection_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ssh_runtime_mode: Option<crate::config::SshRuntimeMode>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display: Option<RemoteDesktopDisplayMetadata>,
    },
    #[serde(rename = "split")]
    Split {
        #[serde(default = "default_split_id")]
        id: String,
        direction: String,
        #[serde(default = "default_split_ratio")]
        ratio: f64,
        first: Box<RestorablePaneNode>,
        second: Box<RestorablePaneNode>,
    },
}

impl RestorablePaneNode {
    pub fn first_leaf_id(&self) -> Option<&str> {
        match self {
            Self::Leaf { id, .. } => Some(id.as_str()),
            Self::Split { first, second, .. } => {
                first.first_leaf_id().or_else(|| second.first_leaf_id())
            }
        }
    }

    pub fn first_leaf_summary(&self) -> Option<(&str, &str, Option<&str>)> {
        match self {
            Self::Leaf {
                title,
                session_type,
                connection_id,
                ..
            } => Some((
                title.as_str(),
                session_type.as_str(),
                connection_id.as_deref(),
            )),
            Self::Split { first, second, .. } => first
                .first_leaf_summary()
                .or_else(|| second.first_leaf_summary()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RestorableTab {
    #[serde(default)]
    pub active_pane_id: Option<String>,
    #[serde(default)]
    pub root: Option<RestorablePaneNode>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub session_type: String,
    pub connection_id: Option<String>,
    pub custom_name: Option<String>,
    pub tab_color: Option<String>,
    #[serde(default)]
    pub locked: bool,
}

impl RestorableTab {
    pub fn normalize(&mut self) -> bool {
        let mut changed = false;

        if self.root.is_none() && !self.session_type.is_empty() {
            let leaf_id = default_leaf_id();
            self.root = Some(RestorablePaneNode::Leaf {
                id: leaf_id.clone(),
                pane_kind: None,
                title: if self.title.is_empty() {
                    "Session".to_string()
                } else {
                    self.title.clone()
                },
                session_type: self.session_type.clone(),
                connection_id: self.connection_id.clone(),
                ssh_runtime_mode: None,
                display: None,
            });
            if self.active_pane_id.is_none() {
                self.active_pane_id = Some(leaf_id);
            }
            changed = true;
        }

        if let Some(root) = &self.root {
            if self.active_pane_id.is_none() {
                self.active_pane_id = root.first_leaf_id().map(|id| id.to_string());
                changed = true;
            }

            if let Some((title, session_type, connection_id)) = root.first_leaf_summary() {
                if self.title.is_empty() {
                    self.title = title.to_string();
                    changed = true;
                }
                if self.session_type.is_empty() {
                    self.session_type = session_type.to_string();
                    changed = true;
                }
                if self.connection_id.is_none() && connection_id.is_some() {
                    self.connection_id = connection_id.map(|value| value.to_string());
                    changed = true;
                }
            }
        }

        changed
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum RestorableTerminalWindowNode {
    #[serde(rename = "leaf")]
    Leaf {
        #[serde(default)]
        tab_indexes: Vec<usize>,
        #[serde(default)]
        active_tab_index: Option<usize>,
    },
    #[serde(rename = "split")]
    Split {
        direction: String,
        #[serde(default = "default_split_ratio")]
        ratio: f64,
        first: Box<RestorableTerminalWindowNode>,
        second: Box<RestorableTerminalWindowNode>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityBarLayout {
    #[serde(default = "default_left_top")]
    pub left_top: Vec<String>,
    #[serde(default = "default_left_bottom")]
    pub left_bottom: Vec<String>,
    #[serde(default = "default_right_top")]
    pub right_top: Vec<String>,
    #[serde(default = "default_right_bottom")]
    pub right_bottom: Vec<String>,
    #[serde(default)]
    pub show_labels: bool,
    #[serde(default)]
    pub hidden_items: Vec<String>,
}

impl Default for ActivityBarLayout {
    fn default() -> Self {
        Self {
            left_top: default_left_top(),
            left_bottom: default_left_bottom(),
            right_top: default_right_top(),
            right_bottom: default_right_bottom(),
            show_labels: false,
            hidden_items: Vec::new(),
        }
    }
}

fn default_left_top() -> Vec<String> {
    vec![
        "fileExplorer".to_string(),
        "notes".to_string(),
        "network".to_string(),
        "securityAuth".to_string(),
    ]
}

fn default_left_bottom() -> Vec<String> {
    vec!["syncBackupHistory".to_string(), "settings".to_string()]
}

fn default_right_top() -> Vec<String> {
    vec![
        "savedConnections".to_string(),
        "aiAssistant".to_string(),
        "activeSessions".to_string(),
        "commandHistory".to_string(),
        "resourceMonitor".to_string(),
        "gpuMonitor".to_string(),
        "ascendNpuMonitor".to_string(),
        "processManager".to_string(),
        "dockerManager".to_string(),
    ]
}

fn default_right_bottom() -> Vec<String> {
    vec![
        "quickCmdBar".to_string(),
        "serialSend".to_string(),
        "recording".to_string(),
        "lock".to_string(),
    ]
}

/// Layout and theme preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    #[serde(default)]
    pub open_tabs: Vec<RestorableTab>,
    #[serde(default)]
    pub terminal_window_layout: Option<RestorableTerminalWindowNode>,
    #[serde(default = "default_start_workspace_mode")]
    pub start_workspace_mode: String,
    #[serde(default = "default_panel_open_mode")]
    pub panel_open_mode: String,
    #[serde(default = "default_left_width")]
    pub left_width: f64,
    #[serde(default = "default_right_width")]
    pub right_width: f64,
    #[serde(default = "default_quick_cmd_height")]
    pub quick_cmd_height: f64,
    #[serde(default = "default_quick_cmd_category_width")]
    pub quick_cmd_category_width: f64,
    #[serde(default = "default_quick_cmd_view_mode")]
    pub quick_cmd_view_mode: String,
    #[serde(default = "default_quick_cmd_sort_mode")]
    pub quick_cmd_sort_mode: String,
    #[serde(default = "default_quick_cmd_selected_category")]
    pub quick_cmd_selected_category: String,
    #[serde(default = "default_active_left_panel")]
    pub active_left_panel: Option<String>,
    #[serde(default = "default_active_right_panel")]
    pub active_right_panel: Option<String>,
    #[serde(default)]
    pub left_open_panels: Vec<String>,
    #[serde(default)]
    pub right_open_panels: Vec<String>,
    #[serde(default)]
    pub panel_stack_sizes: HashMap<String, f64>,
    #[serde(default = "default_network_panel_active_tab")]
    pub network_panel_active_tab: String,
    #[serde(default = "default_security_auth_panel_active_tab")]
    pub security_auth_panel_active_tab: String,
    #[serde(default = "default_true_fn")]
    pub show_quick_cmd_bar: bool,
    #[serde(default = "default_false")]
    pub show_serial_send_panel: bool,
    #[serde(default = "default_serial_send_height")]
    pub serial_send_height: f64,
    #[serde(default = "default_false")]
    pub serial_send_clear_after_send: bool,
    #[serde(default = "default_zoom")]
    pub zoom_level: f64,
    #[serde(default = "default_language")]
    pub language: Option<String>,
    #[serde(default = "default_header_status_mode")]
    pub header_status_mode: String,
    #[serde(default = "default_true_fn")]
    pub header_status_visible: bool,
    #[serde(default = "default_true_fn")]
    pub show_notes_panel: bool,
    #[serde(default = "default_true_fn")]
    pub show_remote_stats: bool,
    #[serde(default = "default_remote_stats_interval")]
    pub remote_stats_interval: u32,
    #[serde(default = "default_false")]
    pub show_gpu_monitor: bool,
    #[serde(default = "default_gpu_monitor_interval")]
    pub gpu_monitor_interval: u32,
    #[serde(default = "default_false")]
    pub show_ascend_npu_monitor: bool,
    #[serde(default = "default_ascend_npu_monitor_interval")]
    pub ascend_npu_monitor_interval: u32,
    #[serde(default = "default_false")]
    pub show_process_manager: bool,
    #[serde(default = "default_process_manager_interval")]
    pub process_manager_interval: u32,
    #[serde(default = "default_false")]
    pub show_docker_manager: bool,
    #[serde(default = "default_docker_manager_interval")]
    pub docker_manager_interval: u32,
    #[serde(default = "default_sort_mode")]
    pub saved_connections_sort_mode: String,
    #[serde(default)]
    pub saved_connections_expanded_group_ids: Vec<String>,
    #[serde(default)]
    pub asset_sort_key: Option<String>,
    #[serde(default)]
    pub asset_sort_direction: Option<String>,
    #[serde(default)]
    pub recent_connection_ids: Vec<String>,
    #[serde(default = "default_transfer_height")]
    pub transfer_height: f64,
    #[serde(default = "default_file_explorer_view_mode")]
    pub file_explorer_view_mode: String,
    #[serde(default = "default_true_fn")]
    pub file_explorer_show_hidden_files: bool,
    #[serde(default)]
    pub file_explorer_auto_sync_cwd_connection_ids: Vec<String>,
    #[serde(default)]
    pub file_explorer_favorite_dirs_by_connection_id: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub notes_expanded_folder_ids: Vec<String>,
    #[serde(default)]
    pub notes_last_selected_node_id: Option<String>,
    #[serde(default)]
    pub activity_bar_layout: ActivityBarLayout,
}

impl UiConfig {
    pub fn normalize_quick_command_sort_mode(&mut self) -> bool {
        if matches!(
            self.quick_cmd_sort_mode.as_str(),
            "created" | "name" | "useCount" | "custom"
        ) {
            return false;
        }

        self.quick_cmd_sort_mode = default_quick_cmd_sort_mode();
        true
    }
}

fn default_left_width() -> f64 {
    256.0
}

fn default_right_width() -> f64 {
    288.0
}

fn default_quick_cmd_height() -> f64 {
    180.0
}

fn default_quick_cmd_category_width() -> f64 {
    176.0
}

fn default_quick_cmd_view_mode() -> String {
    "tile".to_string()
}

fn default_quick_cmd_sort_mode() -> String {
    "created".to_string()
}

fn default_quick_cmd_selected_category() -> String {
    "all".to_string()
}

fn default_start_workspace_mode() -> String {
    "workbench".to_string()
}

fn default_panel_open_mode() -> String {
    "docked".to_string()
}

fn default_active_left_panel() -> Option<String> {
    Some("fileExplorer".to_string())
}

fn default_active_right_panel() -> Option<String> {
    Some("savedConnections".to_string())
}

fn default_network_panel_active_tab() -> String {
    "tunnel".to_string()
}

fn default_security_auth_panel_active_tab() -> String {
    "keys".to_string()
}

fn default_true_fn() -> bool {
    true
}

fn default_zoom() -> f64 {
    1.0
}

fn default_remote_stats_interval() -> u32 {
    3
}

fn default_gpu_monitor_interval() -> u32 {
    3
}

fn default_ascend_npu_monitor_interval() -> u32 {
    3
}

fn default_process_manager_interval() -> u32 {
    5
}

fn default_docker_manager_interval() -> u32 {
    10
}

fn default_transfer_height() -> f64 {
    180.0
}

fn default_file_explorer_view_mode() -> String {
    "list".to_string()
}

fn default_serial_send_height() -> f64 {
    180.0
}

fn default_sort_mode() -> String {
    "default".to_string()
}

fn default_language() -> Option<String> {
    Some("en".to_string())
}

fn default_header_status_mode() -> String {
    "session".to_string()
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            open_tabs: vec![],
            terminal_window_layout: None,
            start_workspace_mode: default_start_workspace_mode(),
            panel_open_mode: default_panel_open_mode(),
            left_width: default_left_width(),
            right_width: default_right_width(),
            quick_cmd_height: default_quick_cmd_height(),
            quick_cmd_category_width: default_quick_cmd_category_width(),
            quick_cmd_view_mode: default_quick_cmd_view_mode(),
            quick_cmd_sort_mode: default_quick_cmd_sort_mode(),
            quick_cmd_selected_category: default_quick_cmd_selected_category(),
            active_left_panel: default_active_left_panel(),
            active_right_panel: default_active_right_panel(),
            left_open_panels: vec![],
            right_open_panels: vec![],
            panel_stack_sizes: HashMap::new(),
            network_panel_active_tab: default_network_panel_active_tab(),
            security_auth_panel_active_tab: default_security_auth_panel_active_tab(),
            show_quick_cmd_bar: true,
            show_serial_send_panel: false,
            serial_send_height: default_serial_send_height(),
            serial_send_clear_after_send: false,
            zoom_level: default_zoom(),
            language: default_language(),
            header_status_mode: default_header_status_mode(),
            header_status_visible: true,
            show_notes_panel: true,
            show_remote_stats: true,
            remote_stats_interval: default_remote_stats_interval(),
            show_gpu_monitor: false,
            gpu_monitor_interval: default_gpu_monitor_interval(),
            show_ascend_npu_monitor: false,
            ascend_npu_monitor_interval: default_ascend_npu_monitor_interval(),
            show_process_manager: false,
            process_manager_interval: default_process_manager_interval(),
            show_docker_manager: false,
            docker_manager_interval: default_docker_manager_interval(),
            saved_connections_sort_mode: default_sort_mode(),
            saved_connections_expanded_group_ids: vec![],
            asset_sort_key: None,
            asset_sort_direction: None,
            recent_connection_ids: vec![],
            transfer_height: default_transfer_height(),
            file_explorer_view_mode: default_file_explorer_view_mode(),
            file_explorer_show_hidden_files: true,
            file_explorer_auto_sync_cwd_connection_ids: vec![],
            file_explorer_favorite_dirs_by_connection_id: HashMap::new(),
            notes_expanded_folder_ids: vec![],
            notes_last_selected_node_id: None,
            activity_bar_layout: ActivityBarLayout::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ActivityBarLayout, RestorablePaneNode, RestorableTab, UiConfig};
    use serde_json::json;

    #[test]
    fn activity_bar_layout_deserializes_legacy_shape_without_hidden_items() {
        let raw = json!({
            "left_top": ["fileExplorer"],
            "left_bottom": ["settings"],
            "right_top": ["savedConnections"],
            "right_bottom": ["lock"],
            "show_labels": true
        });

        let layout: ActivityBarLayout =
            serde_json::from_value(raw).expect("legacy activity bar layout");

        assert!(layout.hidden_items.is_empty());
        assert!(layout.show_labels);
    }

    #[test]
    fn activity_bar_layout_round_trips_hidden_items() {
        let mut layout = ActivityBarLayout::default();
        layout.hidden_items = vec!["gpuMonitor".to_string(), "settings".to_string()];

        let encoded = serde_json::to_value(&layout).expect("activity layout json");
        let decoded: ActivityBarLayout =
            serde_json::from_value(encoded).expect("activity layout decode");

        assert_eq!(decoded.hidden_items, layout.hidden_items);
    }

    #[test]
    fn ui_config_defaults_panel_open_mode_to_docked() {
        let ui = UiConfig::default();
        assert_eq!(ui.panel_open_mode, "docked");
    }

    #[test]
    fn ui_config_deserializes_legacy_shape_without_panel_open_mode() {
        let raw = json!({
            "left_width": 300.0,
            "right_width": 320.0
        });

        let ui: UiConfig = serde_json::from_value(raw).expect("legacy ui config");

        assert_eq!(ui.panel_open_mode, "docked");
        assert_eq!(ui.file_explorer_view_mode, "list");
    }

    #[test]
    fn ui_config_defaults_file_explorer_view_mode_to_list() {
        assert_eq!(UiConfig::default().file_explorer_view_mode, "list");
    }

    #[test]
    fn current_leaf_schema_round_trips_terminal_fields() {
        let raw = json!({
            "kind": "leaf",
            "id": "pane-ssh",
            "title": "Linux",
            "session_type": "SSH",
            "connection_id": "ssh-1"
        });

        let pane: RestorablePaneNode = serde_json::from_value(raw.clone()).expect("terminal leaf");
        let encoded = serde_json::to_value(pane).expect("serialized terminal leaf");

        assert_eq!(encoded, raw);
    }

    #[test]
    fn current_leaf_schema_preserves_remote_desktop_metadata() {
        let raw = json!({
            "kind": "leaf",
            "id": "pane-rdp",
            "pane_kind": "remote-desktop",
            "title": "Windows",
            "session_type": "RDP",
            "connection_id": "rdp-1",
            "display": {
                "remoteWidth": 1600,
                "remoteHeight": 900,
                "scaleMode": "actual",
                "viewOnly": true,
                "clipboardEnabled": false
            }
        });
        let pane: RestorablePaneNode =
            serde_json::from_value(raw.clone()).expect("remote desktop leaf");
        let encoded = serde_json::to_value(pane).expect("serialized remote desktop leaf");

        assert_eq!(encoded, raw);
    }

    #[test]
    fn mixed_tab_round_trip_preserves_each_pane_kind() {
        let raw = json!({
            "active_pane_id": "pane-rdp",
            "root": {
                "kind": "split",
                "id": "split-1",
                "direction": "vertical",
                "ratio": 0.4,
                "first": {
                    "kind": "leaf",
                    "id": "pane-ssh",
                    "pane_kind": "terminal",
                    "title": "Linux",
                    "session_type": "SSH",
                    "connection_id": "ssh-1"
                },
                "second": {
                    "kind": "leaf",
                    "id": "pane-rdp",
                    "pane_kind": "remote-desktop",
                    "title": "Windows",
                    "session_type": "RDP",
                    "connection_id": "rdp-1"
                }
            },
            "title": "Windows",
            "session_type": "RDP",
            "connection_id": "rdp-1",
            "custom_name": null,
            "tab_color": null,
            "locked": false
        });

        let tab: RestorableTab = serde_json::from_value(raw).expect("mixed tab");
        let encoded = serde_json::to_value(tab).expect("serialized mixed tab");

        assert_eq!(encoded["active_pane_id"], "pane-rdp");
        assert_eq!(encoded["root"]["direction"], "vertical");
        assert_eq!(encoded["root"]["first"]["session_type"], "SSH");
        assert_eq!(encoded["root"]["second"]["session_type"], "RDP");
        assert_eq!(encoded["root"]["first"]["pane_kind"], "terminal");
        assert_eq!(encoded["root"]["second"]["pane_kind"], "remote-desktop");
    }
}
