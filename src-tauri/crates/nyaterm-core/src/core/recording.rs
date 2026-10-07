use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RecordingMode {
    #[default]
    Transcript,
    Raw,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExistingFileBehavior {
    #[default]
    Unique,
    Append,
    Overwrite,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum RotationPolicy {
    Session,
    Daily,
    Size { max_bytes: u64 },
}

impl Default for RotationPolicy {
    fn default() -> Self {
        Self::Session
    }
}
