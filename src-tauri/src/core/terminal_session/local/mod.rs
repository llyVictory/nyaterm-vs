//! Local PTY (pseudo-terminal) session creation and management.
//!
//! Spawns the user's shell (PowerShell on Windows, $SHELL elsewhere) and bridges I/O to Tauri.

use crate::config::AiExecutionProfile;
use crate::core::SessionOutputCoalescer;
use crate::core::capture::OutputCaptureProcessor;
use crate::core::recording::RecordingManager;
use crate::core::session::{
    SessionCommand, SessionCommandReceiver, SessionCommandSender, SessionHandle, SessionInfo,
    SessionManager, SessionReadyHook, SessionType, SharedCwd, session_command_channel,
};
use crate::core::ssh::osc::{OscStripper, build_ready_marker};
use crate::core::terminal_session::{TerminalOutputDecoder, encode_terminal_input};
use crate::core::update_cwd_if_changed;
use crate::core::zmodem::{
    ZmodemAction, ZmodemDetectResult, ZmodemDetector, ZmodemDirection, ZmodemEvent, ZmodemTransfer,
    start_zmodem_transfer,
};
use crate::error::AppResult;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::path::Path;
#[cfg(target_os = "windows")]
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex, mpsc as std_mpsc};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc;

include!("config.rs");
include!("shell.rs");
include!("windows_terminal.rs");
include!("args.rs");
include!("environment.rs");
include!("startup.rs");
include!("session.rs");
include!("tests.rs");
