use super::{QuickCommandsStore, SessionManager};
use crate::config;
use crate::error::AppResult;
pub use nyaterm_core::core::portable_snapshot::*;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
include!("build.rs");
include!("apply.rs");
