use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticsLogLevel {
    Warn,
    Info,
    Debug,
}

impl Default for DiagnosticsLogLevel {
    fn default() -> Self {
        Self::Info
    }
}

impl DiagnosticsLogLevel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticsSettings {
    #[serde(default)]
    pub level: DiagnosticsLogLevel,
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,
}

const fn default_retention_days() -> u32 {
    7
}

impl Default for DiagnosticsSettings {
    fn default() -> Self {
        Self {
            level: DiagnosticsLogLevel::Info,
            retention_days: default_retention_days(),
        }
    }
}
