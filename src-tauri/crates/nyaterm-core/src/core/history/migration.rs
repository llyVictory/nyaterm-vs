fn load_history_entries(content: &str) -> AppResult<(Vec<HistoryEntry>, bool)> {
    if let Ok(store) = serde_json::from_str::<HistoryStoreFileV2>(content) {
        if store.version != HISTORY_STORE_VERSION {
            return Err(AppError::Config(format!(
                "Unsupported command history version {}",
                store.version
            )));
        }

        let (entries, changed) = normalize_v2_entries(store.entries);
        return Ok((entries, changed));
    }

    let legacy_commands: Vec<String> = serde_json::from_str(content)?;
    Ok((migrate_legacy_commands(legacy_commands), true))
}

fn normalize_v2_entries(entries: Vec<HistoryEntry>) -> (Vec<HistoryEntry>, bool) {
    let original_len = entries.len();
    let mut normalized = Vec::new();
    let mut changed = false;

    for entry in entries {
        let use_count = entry.use_count.max(1);
        let Some(command) = sanitize_history_command(&entry.command) else {
            changed = true;
            continue;
        };

        if command != entry.command || use_count != entry.use_count {
            changed = true;
        }

        merge_entry(
            &mut normalized,
            HistoryEntry {
                command,
                last_used_at_ms: entry.last_used_at_ms,
                use_count,
            },
        );
    }

    normalized.sort_by_key(|entry| entry.last_used_at_ms);
    let trimmed = trim_to_max_history(&mut normalized);
    changed |= trimmed || normalized.len() != original_len;
    (normalized, changed)
}

fn migrate_legacy_commands(commands: Vec<String>) -> Vec<HistoryEntry> {
    let mut migrated = Vec::new();
    let base_timestamp = current_time_ms().saturating_sub(commands.len() as u64);

    for (index, command) in commands.into_iter().enumerate() {
        let Some(cleaned) = sanitize_history_command(&command) else {
            continue;
        };

        merge_entry(
            &mut migrated,
            HistoryEntry {
                command: cleaned,
                last_used_at_ms: base_timestamp.saturating_add(index as u64),
                use_count: 1,
            },
        );
    }

    trim_to_max_history(&mut migrated);
    migrated
}

fn merge_entry(entries: &mut Vec<HistoryEntry>, incoming: HistoryEntry) {
    if let Some(index) = entries
        .iter()
        .position(|entry| entry.command == incoming.command)
    {
        let mut existing = entries.remove(index);
        existing.last_used_at_ms = existing.last_used_at_ms.max(incoming.last_used_at_ms);
        existing.use_count = existing.use_count.saturating_add(incoming.use_count);
        entries.push(existing);
    } else {
        entries.push(incoming);
    }
}

fn trim_to_max_history(entries: &mut Vec<HistoryEntry>) -> bool {
    if entries.len() <= MAX_HISTORY {
        return false;
    }

    let overflow = entries.len() - MAX_HISTORY;
    entries.drain(..overflow);
    true
}
