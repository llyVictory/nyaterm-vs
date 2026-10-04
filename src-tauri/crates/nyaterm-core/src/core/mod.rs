pub mod history;
mod recording;
pub use recording::{ExistingFileBehavior, RecordingMode, RotationPolicy};
pub mod ai;
pub mod capabilities;
pub mod monitoring;
pub mod quick_commands;
pub mod translate;
