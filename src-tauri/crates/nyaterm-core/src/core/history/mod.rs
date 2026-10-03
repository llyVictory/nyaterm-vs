use crate::error::{AppError, AppResult};
use crate::utils::fuzzy::{FuzzyResult, fuzzy_search_items};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_HISTORY: usize = 5000;
const HISTORY_STORE_VERSION: u32 = 2;

include!("types.rs");
include!("store.rs");
include!("migration.rs");
include!("sanitize.rs");
include!("util.rs");
include!("tests.rs");
