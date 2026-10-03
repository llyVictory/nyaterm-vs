fn current_time_ms() -> u64 {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    u64::try_from(millis).unwrap_or(u64::MAX)
}

/// Writes serialized history bytes to disk. Safe to call from a blocking context.
pub fn flush_to_disk(path: &Path, bytes: &[u8]) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    write_atomic(path, bytes)
}

pub fn flush_prepared_save(pending: PreparedHistorySave) -> AppResult<()> {
    match pending {
        PreparedHistorySave::File(path, bytes) => flush_to_disk(&path, &bytes),
        PreparedHistorySave::Redb(content) => {
            let (entries, _) = load_history_entries(&content)?;
            crate::storage::replace_command_history_entries(&entries)
        }
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> AppResult<()> {
    let tmp_path = temporary_path_for(path);
    fs::write(&tmp_path, bytes)?;
    match fs::rename(&tmp_path, path) {
        Ok(()) => Ok(()),
        Err(err) => {
            let _ = fs::remove_file(&tmp_path);
            Err(err.into())
        }
    }
}

fn temporary_path_for(path: &Path) -> PathBuf {
    let mut tmp_path = path.to_path_buf();
    let next_extension = match path.extension().and_then(|ext| ext.to_str()) {
        Some(ext) if !ext.is_empty() => format!("{ext}.tmp"),
        _ => "tmp".to_string(),
    };
    tmp_path.set_extension(next_extension);
    tmp_path
}
