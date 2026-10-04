//! Telnet session: raw TCP with basic IAC negotiation, bridged to the session manager.

use crate::config::AiExecutionProfile;
use crate::core::capture::OutputCaptureProcessor;
use crate::core::input::remap_del_to_bs;
use crate::core::session::{
    DynamicTitleCapabilities, SessionCommand, SessionCommandReceiver, SessionCommandSender,
    SessionHandle, SessionInfo, SessionManager, SessionReadyHook, SessionType, SharedCwd,
    session_command_channel,
};
use crate::core::terminal_session::{
    TerminalOutputDecoder, encode_terminal_input, prepare_terminal_write_input,
};
use crate::core::zmodem::{
    ZmodemAction, ZmodemDetectResult, ZmodemDetector, ZmodemDirection, ZmodemDownloadOoDrain,
    ZmodemEvent, ZmodemTransfer, ZmodemUploadDrain, start_zmodem_transfer,
};
use crate::core::{RecordingManager, SessionOutputCoalescer};
use crate::error::{AppError, AppResult};
use crate::observability::{StructuredLog, StructuredLogLevel, log_event, log_rate_limited};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{Mutex as TokioMutex, mpsc, oneshot};

pub use nyaterm_core::telnet::*;

include!("tests.rs");
include!("manager.rs");
include!("session.rs");
