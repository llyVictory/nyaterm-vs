use super::super::default_true;
use serde::{Deserialize, Serialize};

fn default_rdp_client_mode() -> String {
    "builtin".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralSettings {
    #[serde(default = "default_true")]
    pub startup_restore: bool,
    #[serde(default = "default_true")]
    pub startup_restore_window_layout: bool,
    #[serde(default)]
    pub minimize_to_tray: bool,
    #[serde(default)]
    pub boss_key: Option<String>,
    #[serde(default = "default_true")]
    pub confirm_on_close: bool,
    #[serde(default = "default_rdp_client_mode")]
    pub rdp_client_mode: String,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            startup_restore: false,
            startup_restore_window_layout: true,
            minimize_to_tray: false,
            boss_key: None,
            confirm_on_close: true,
            rdp_client_mode: default_rdp_client_mode(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::GeneralSettings;

    #[test]
    fn missing_rdp_client_mode_defaults_to_builtin() {
        let settings: GeneralSettings = serde_json::from_value(serde_json::json!({
            "startup_restore": true,
            "startup_restore_window_layout": true,
            "minimize_to_tray": false,
            "boss_key": null,
            "confirm_on_close": true
        }))
        .expect("general settings");

        assert_eq!(settings.rdp_client_mode, "builtin");
    }
}
