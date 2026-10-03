use super::super::default_true;
use serde::{Deserialize, Serialize};

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
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            startup_restore: false,
            startup_restore_window_layout: true,
            minimize_to_tray: false,
            boss_key: None,
            confirm_on_close: true,
        }
    }
}
