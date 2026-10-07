use crate::config::{self, QuickCommand, QuickCommandCategory, QuickCommandsConfig};
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Read;
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

use uuid::Uuid;

include!("types.rs");
include!("store.rs");
include!("parsers.rs");
include!("merge.rs");
include!("tests.rs");
