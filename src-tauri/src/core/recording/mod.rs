use crate::error::{AppError, AppResult};
use regex::RegexBuilder;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::thread;
use std::time::Instant;
use time::OffsetDateTime;

pub const DEFAULT_MEMORY_LIMIT_BYTES: usize = 5 * 1024 * 1024;
pub const DEFAULT_HISTORY_SEARCH_LINES: usize = 30_000;
pub const MAX_HISTORY_SEARCH_LINES: usize = 100_000;
pub const DEFAULT_HISTORY_SEARCH_LIMIT: usize = 100;
const DEFAULT_RECORDING_QUEUE_LIMIT_BYTES: u64 = 4 * 1024 * 1024;

include!("types.rs");
include!("transcript.rs");
include!("search_types.rs");
include!("session_capture.rs");
include!("manager.rs");
include!("format.rs");
include!("search.rs");
include!("tests.rs");
