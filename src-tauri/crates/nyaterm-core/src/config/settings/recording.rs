use crate::core::{ExistingFileBehavior, RecordingMode, RotationPolicy};
use serde::{Deserialize, Serialize};

pub const DEFAULT_RECORDING_MEMORY_LIMIT_BYTES: u64 = 5 * 1024 * 1024;
pub const DEFAULT_RECORDING_PATH_TEMPLATE: &str =
    "{group}/{session}/{yyyy}-{MM}-{dd}/{HH}-{mm}-{ss}-{SSS}-{session_short_id}.log";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingSettings {
    #[serde(default)]
    pub auto_start: bool,
    #[serde(default)]
    pub default_mode: RecordingMode,
    #[serde(default)]
    pub base_path: String,
    #[serde(default = "default_path_template")]
    pub path_template: String,
    #[serde(default = "default_true")]
    pub include_timestamps: bool,
    #[serde(default = "default_true")]
    pub include_io_labels: bool,
    #[serde(default = "default_true")]
    pub include_session_metadata: bool,
    #[serde(default)]
    pub rotation: RotationPolicy,
    #[serde(default)]
    pub existing_file_behavior: ExistingFileBehavior,
    #[serde(default = "default_memory_limit_bytes")]
    pub memory_limit_bytes: u64,
    #[serde(default)]
    pub include_binary_transfer_payloads: bool,
}

impl RecordingSettings {
    pub fn normalize(&mut self) -> bool {
        let mut changed = false;
        if self.path_template.trim().is_empty() {
            self.path_template = default_path_template();
            changed = true;
        }
        if self.memory_limit_bytes == 0 {
            self.memory_limit_bytes = default_memory_limit_bytes();
            changed = true;
        }
        changed
    }
}

impl Default for RecordingSettings {
    fn default() -> Self {
        Self {
            auto_start: false,
            default_mode: RecordingMode::Transcript,
            base_path: String::new(),
            path_template: default_path_template(),
            include_timestamps: true,
            include_io_labels: true,
            include_session_metadata: true,
            rotation: RotationPolicy::Session,
            existing_file_behavior: ExistingFileBehavior::Unique,
            memory_limit_bytes: default_memory_limit_bytes(),
            include_binary_transfer_payloads: false,
        }
    }
}

fn default_path_template() -> String {
    DEFAULT_RECORDING_PATH_TEMPLATE.to_string()
}

fn default_memory_limit_bytes() -> u64 {
    DEFAULT_RECORDING_MEMORY_LIMIT_BYTES
}

fn default_true() -> bool {
    true
}
