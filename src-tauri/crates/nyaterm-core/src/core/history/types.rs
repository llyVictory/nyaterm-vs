#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HistoryEntry {
    pub command: String,
    pub last_used_at_ms: u64,
    pub use_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HistoryStoreFileV2 {
    version: u32,
    entries: Vec<HistoryEntry>,
}

