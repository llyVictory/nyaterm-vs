pub mod history;
mod recording;
pub use recording::{ExistingFileBehavior, RecordingMode, RotationPolicy};
pub mod ai;
pub mod capabilities;
pub mod monitoring;
pub mod quick_commands;
pub mod translate;

pub mod backup;
pub mod backup_crypto;
pub mod importer;
pub mod keyword_highlights;
pub mod portable_snapshot;
