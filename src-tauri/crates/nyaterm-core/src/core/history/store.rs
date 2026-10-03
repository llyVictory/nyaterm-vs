pub struct CommandHistoryStore {
    entries: Vec<HistoryEntry>,
    dirty: bool,
    history_path: Option<PathBuf>,
}

pub enum PreparedHistorySave {
    File(PathBuf, Vec<u8>),
    Redb(String),
}

impl CommandHistoryStore {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            dirty: false,
            history_path: None,
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn set_history_path(&mut self, path: PathBuf) {
        self.history_path = Some(path);
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn load(&mut self) -> AppResult<()> {
        let content = if let Some(path) = self.history_path.clone() {
            if !path.exists() {
                return Ok(());
            }
            fs::read_to_string(&path)?
        } else {
            let entries = crate::storage::list_command_history_entries(MAX_HISTORY)?;
            if entries.is_empty() {
                return Ok(());
            }
            self.entries = entries;
            self.dirty = false;
            return Ok(());
        };

        if content.trim().is_empty() {
            return Ok(());
        }

        let (entries, changed) = load_history_entries(&content)?;
        self.entries = entries;
        self.dirty = changed;

        if self.dirty {
            self.save()?;
        }

        Ok(())
    }

    pub fn save(&mut self) -> AppResult<()> {
        if let Some(pending) = self.prepare_save() {
            flush_prepared_save(pending)?;
        }
        Ok(())
    }

    /// Serializes dirty state and marks clean. Returns a prepared write for
    /// the caller to persist (possibly via `spawn_blocking`).
    pub fn prepare_save(&mut self) -> Option<PreparedHistorySave> {
        if !self.dirty {
            return None;
        }
        let payload = HistoryStoreFileV2 {
            version: HISTORY_STORE_VERSION,
            entries: self.entries.clone(),
        };
        self.dirty = false;
        if let Some(path) = self.history_path.clone() {
            let bytes = serde_json::to_vec(&payload).ok()?;
            Some(PreparedHistorySave::File(path, bytes))
        } else {
            let content = serde_json::to_string(&payload).ok()?;
            Some(PreparedHistorySave::Redb(content))
        }
    }

    pub fn add(&mut self, command: String) -> bool {
        let Some(command) = sanitize_history_command(&command) else {
            return false;
        };

        let last_used_at_ms = current_time_ms();
        if let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.command == command)
        {
            let mut existing = self.entries.remove(index);
            existing.last_used_at_ms = last_used_at_ms;
            existing.use_count = existing.use_count.saturating_add(1);
            self.entries.push(existing);
        } else {
            self.entries.push(HistoryEntry {
                command,
                last_used_at_ms,
                use_count: 1,
            });
        }

        trim_to_max_history(&mut self.entries);
        self.dirty = true;
        true
    }

    pub fn delete_command(&mut self, command: &str) -> bool {
        let Some(command) = sanitize_history_command(command) else {
            return false;
        };

        let original_len = self.entries.len();
        self.entries.retain(|entry| entry.command != command);
        if self.entries.len() == original_len {
            return false;
        }

        self.dirty = true;
        true
    }

    pub fn list(&self) -> Vec<String> {
        self.entries
            .iter()
            .rev()
            .map(|entry| entry.command.clone())
            .collect()
    }

    pub fn search(
        &self,
        pattern_str: &str,
        limit: usize,
        min_command_length: Option<usize>,
        max_command_length: Option<usize>,
    ) -> Vec<FuzzyResult> {
        let items: Vec<(&str, &str)> = self
            .entries
            .iter()
            .map(|entry| (entry.command.as_str(), entry.command.as_str()))
            .collect();
        fuzzy_search_items(
            &items,
            pattern_str,
            "history",
            limit,
            min_command_length,
            max_command_length,
        )
    }
}

pub fn sanitize_history_command(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    let without_prompt = strip_known_prompt_prefix(strip_leading_env_prefixes(trimmed))
        .unwrap_or(trimmed)
        .trim();

    if without_prompt.is_empty() {
        None
    } else {
        Some(without_prompt.to_string())
    }
}
