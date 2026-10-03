use crate::error::{AppError, AppResult};
use redb::{Database, ReadableDatabase, ReadableTable};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use super::credentials::*;
use super::history::*;
use super::known_hosts::*;
use super::sessions::*;
use super::tables::*;
use super::util::*;
use super::{BackupInfo, Storage};

impl Storage {
    pub(super) fn get_schema_version_optional(&self) -> AppResult<Option<u32>> {
        let txn = self.db.begin_read().map_err(storage_error)?;
        let table = match txn.open_table(META_TABLE) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(error) => return Err(storage_error(error)),
        };
        let Some(raw) = table.get(META_SCHEMA_VERSION).map_err(storage_error)? else {
            return Ok(None);
        };
        parse_meta_u32(raw.value(), META_SCHEMA_VERSION).map(Some)
    }
    pub(super) fn initialize_v3_schema(&self) -> AppResult<()> {
        let txn = self.db.begin_write().map_err(storage_error)?;
        open_all_v3_tables(&txn)?;
        write_meta_u32(&txn, META_SCHEMA_VERSION, SCHEMA_VERSION)?;
        txn.commit().map_err(storage_error)?;
        tracing::info!("Initialized redb storage schema v3");
        Ok(())
    }
    pub(super) fn has_legacy_data(&self) -> AppResult<bool> {
        has_legacy_data(&self.db)
    }
    pub(super) fn migrate_to_v3(&self) -> AppResult<()> {
        let from_version = self.get_schema_version_optional()?.unwrap_or(1);
        let json_docs = read_legacy_docs(&self.db, JSON_DOCS_TABLE)?;
        let text_docs = read_legacy_docs(&self.db, TEXT_DOCS_TABLE)?;

        let txn = self.db.begin_write().map_err(storage_error)?;
        open_all_v3_tables(&txn)?;
        if from_version < 2 {
            Self::import_legacy_docs_in_txn(&txn, &json_docs, &text_docs)?;
        } else {
            rebuild_all_connection_indexes_in_txn(&txn)?;
        }
        if let Some(backup) = &self.pending_migration_backup {
            write_meta_string(
                &txn,
                META_MIGRATION_BACKUP_PATH,
                &backup.path.to_string_lossy(),
            )?;
            write_meta_string(
                &txn,
                META_MIGRATION_BACKUP_CREATED_AT_MS,
                &backup.created_at_ms.to_string(),
            )?;
            write_meta_u32(&txn, META_MIGRATION_SUCCESSFUL_V3_STARTUPS, 0)?;
        }
        delete_legacy_tables_in_txn(&txn)?;
        write_meta_u32(&txn, META_SCHEMA_VERSION, SCHEMA_VERSION)?;
        txn.commit().map_err(storage_error)?;
        tracing::info!(from_version, "Migrated redb storage schema to v3 entities");
        Ok(())
    }

    fn import_legacy_docs_in_txn(
        txn: &redb::WriteTransaction,
        json_docs: &BTreeMap<String, String>,
        text_docs: &BTreeMap<String, String>,
    ) -> AppResult<()> {
        if let Some(settings) = json_docs.get(LEGACY_JSON_SETTINGS) {
            write_raw_bytes_in_txn(txn, SETTINGS_TABLE, SETTINGS_DEFAULT, settings.as_bytes())?;
        }
        for (key, raw) in json_docs {
            if key == LEGACY_JSON_SETTINGS || is_entity_json_doc(key) {
                continue;
            }
            write_raw_bytes_in_txn(txn, SETTINGS_TABLE, &settings_doc_key(key), raw.as_bytes())?;
        }
        if let Some(raw) = json_docs.get(LEGACY_JSON_SESSIONS) {
            let config = parse_sessions_config(raw)?;
            replace_sessions_in_txn(txn, &config)?;
        }
        if let Some(raw) = json_docs.get(LEGACY_JSON_PASSWORDS) {
            let config: crate::config::PasswordsConfig = serde_json::from_str(raw)?;
            replace_passwords_in_txn(txn, &config)?;
        }
        if let Some(raw) = json_docs.get(LEGACY_JSON_KEYS) {
            let config: crate::config::KeysConfig = serde_json::from_str(raw)?;
            replace_ssh_keys_in_txn(txn, &config)?;
        }
        if let Some(raw) = json_docs.get(LEGACY_JSON_CREDENTIALS) {
            let config: crate::config::CredentialsConfig = serde_json::from_str(raw)?;
            replace_credentials_in_txn(txn, &config)?;
        }
        if let Some(raw) = json_docs.get(LEGACY_JSON_OTP) {
            let config: crate::config::OtpConfig = serde_json::from_str(raw)?;
            replace_otp_in_txn(txn, &config)?;
        }
        if let Some(raw) = json_docs.get(LEGACY_JSON_PROXIES) {
            let config: ProxiesConfig = serde_json::from_str(raw)?;
            replace_proxies_in_txn(txn, &config.proxies)?;
        }
        if let Some(raw) = json_docs.get(LEGACY_JSON_TUNNELS) {
            let config: crate::config::TunnelsConfig = serde_json::from_str(raw)?;
            replace_tunnels_in_txn(txn, &config.tunnels)?;
        }
        if let Some(raw) = json_docs.get(LEGACY_JSON_HISTORY) {
            let entries = parse_history_entries(raw)?;
            replace_command_history_in_txn(txn, &entries)?;
        }
        if let Some(master_key) = text_docs.get(LEGACY_TEXT_MASTER_KEY) {
            write_meta_string(txn, META_MASTER_KEY, master_key)?;
        }
        if let Some(known_hosts) = text_docs.get(LEGACY_TEXT_KNOWN_HOSTS) {
            replace_known_hosts_text_in_txn(txn, known_hosts)?;
        }
        Ok(())
    }
    pub(super) fn record_successful_v3_startup_and_cleanup_backups(&self) -> AppResult<()> {
        if self.get_schema_version()? < SCHEMA_VERSION {
            return Ok(());
        }
        self.v3_smoke_check()?;

        let txn = self.db.begin_write().map_err(storage_error)?;
        let backup_path = read_meta_string_in_txn(&txn, META_MIGRATION_BACKUP_PATH)?;
        let backup_created_at = read_meta_u64_in_txn(&txn, META_MIGRATION_BACKUP_CREATED_AT_MS)?;
        let startup_count =
            read_meta_u32_in_txn(&txn, META_MIGRATION_SUCCESSFUL_V3_STARTUPS)?.unwrap_or(0);
        let next_count = startup_count.saturating_add(1);
        write_meta_u32(&txn, META_MIGRATION_SUCCESSFUL_V3_STARTUPS, next_count)?;
        txn.commit().map_err(storage_error)?;

        if let (Some(path), Some(created_at)) = (backup_path, backup_created_at) {
            let age_ms = current_time_ms().saturating_sub(created_at);
            if age_ms >= 14 * 24 * 60 * 60 * 1000 && next_count >= 3 {
                match fs::remove_file(&path) {
                    Ok(()) => {
                        tracing::info!(backup_path = %path, "Deleted expired migration backup");
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => tracing::warn!(
                        backup_path = %path,
                        %error,
                        "Failed to delete expired migration backup"
                    ),
                }
            }
        }

        cleanup_orphan_backups(&self.db_path)?;
        Ok(())
    }
    pub(super) fn v3_smoke_check(&self) -> AppResult<()> {
        let txn = self.db.begin_read().map_err(storage_error)?;
        txn.open_table(META_TABLE).map_err(storage_error)?;
        txn.open_table(SETTINGS_TABLE).map_err(storage_error)?;
        txn.open_table(CONNECTIONS_TABLE).map_err(storage_error)?;
        txn.open_table(CREDENTIALS_TABLE).map_err(storage_error)?;
        txn.open_table(KNOWN_HOSTS_TABLE).map_err(storage_error)?;
        txn.open_table(RDP_KNOWN_HOSTS_TABLE)
            .map_err(storage_error)?;
        txn.open_table(VNC_KNOWN_HOSTS_TABLE)
            .map_err(storage_error)?;
        txn.open_table(NOTE_FOLDERS_TABLE).map_err(storage_error)?;
        txn.open_table(NOTES_TABLE).map_err(storage_error)?;
        txn.open_table(NOTE_SUMMARIES_TABLE)
            .map_err(storage_error)?;
        Ok(())
    }

    pub(super) fn ensure_current_v3_tables(&self) -> AppResult<()> {
        let txn = self.db.begin_write().map_err(storage_error)?;
        open_all_v3_tables(&txn)?;
        txn.commit().map_err(storage_error)?;
        Ok(())
    }
}

pub(super) fn open_all_v3_tables(txn: &redb::WriteTransaction) -> AppResult<()> {
    txn.open_table(META_TABLE).map_err(storage_error)?;
    txn.open_table(SETTINGS_TABLE).map_err(storage_error)?;
    txn.open_table(GROUPS_TABLE).map_err(storage_error)?;
    txn.open_table(CONNECTIONS_TABLE).map_err(storage_error)?;
    txn.open_table(CREDENTIALS_TABLE).map_err(storage_error)?;
    txn.open_table(OTP_ACCOUNTS_TABLE).map_err(storage_error)?;
    txn.open_table(PROXIES_TABLE).map_err(storage_error)?;
    txn.open_table(TUNNELS_TABLE).map_err(storage_error)?;
    txn.open_table(KNOWN_HOSTS_TABLE).map_err(storage_error)?;
    txn.open_table(RDP_KNOWN_HOSTS_TABLE)
        .map_err(storage_error)?;
    txn.open_table(VNC_KNOWN_HOSTS_TABLE)
        .map_err(storage_error)?;
    txn.open_table(COMMAND_HISTORY_TABLE)
        .map_err(storage_error)?;
    txn.open_table(NOTE_FOLDERS_TABLE).map_err(storage_error)?;
    txn.open_table(NOTES_TABLE).map_err(storage_error)?;
    txn.open_table(NOTE_SUMMARIES_TABLE)
        .map_err(storage_error)?;
    txn.open_table(IDX_CONNECTIONS_BY_GROUP_TABLE)
        .map_err(storage_error)?;
    txn.open_table(IDX_CONNECTIONS_BY_LAST_USED_TABLE)
        .map_err(storage_error)?;
    txn.open_table(IDX_CONNECTIONS_BY_PROTOCOL_TABLE)
        .map_err(storage_error)?;
    Ok(())
}
pub(super) fn delete_legacy_tables_in_txn(txn: &redb::WriteTransaction) -> AppResult<()> {
    match txn.delete_table(JSON_DOCS_TABLE) {
        Ok(_) | Err(redb::TableError::TableDoesNotExist(_)) => {}
        Err(error) => return Err(storage_error(error)),
    }
    match txn.delete_table(TEXT_DOCS_TABLE) {
        Ok(_) | Err(redb::TableError::TableDoesNotExist(_)) => {}
        Err(error) => return Err(storage_error(error)),
    }
    Ok(())
}
pub(super) fn read_legacy_docs(
    db: &Database,
    definition: redb::TableDefinition<&str, &str>,
) -> AppResult<BTreeMap<String, String>> {
    let txn = db.begin_read().map_err(storage_error)?;
    let table = match txn.open_table(definition) {
        Ok(table) => table,
        Err(redb::TableError::TableDoesNotExist(_)) => return Ok(BTreeMap::new()),
        Err(error) => return Err(storage_error(error)),
    };
    let mut values = BTreeMap::new();
    for entry in table.iter().map_err(storage_error)? {
        let (key, value) = entry.map_err(storage_error)?;
        values.insert(key.value().to_string(), value.value().to_string());
    }
    Ok(values)
}

pub(super) fn backup_database_before_migration_if_needed(
    db_path: &Path,
) -> AppResult<Option<BackupInfo>> {
    let needs_backup = {
        let db = Database::open(db_path).map_err(storage_error)?;
        match get_schema_version_optional_from_db(&db)? {
            Some(version) if version >= SCHEMA_VERSION => false,
            _ => has_legacy_data(&db)?,
        }
    };

    if !needs_backup {
        return Ok(None);
    }

    let created_at_ms = current_time_ms();
    let backup_name = format!("nyaterm.redb.bak-v1-{created_at_ms}.redb");
    let backup_path = db_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(backup_name);
    fs::copy(db_path, &backup_path).map_err(|error| {
        AppError::Storage(format!(
            "Failed to backup redb before schema migration to '{}': {error}",
            backup_path.display()
        ))
    })?;
    tracing::info!(
        backup_path = %backup_path.display(),
        "Backed up redb before storage schema migration"
    );
    Ok(Some(BackupInfo {
        path: backup_path,
        created_at_ms,
    }))
}

fn cleanup_orphan_backups(db_path: &Path) -> AppResult<()> {
    let Some(parent) = db_path.parent() else {
        return Ok(());
    };
    let now = current_time_ms();
    let mut backups = Vec::new();
    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        let Some(timestamp) = backup_timestamp_from_name(file_name) else {
            continue;
        };
        backups.push((timestamp, entry.path()));
    }
    backups.sort_by(|left, right| right.0.cmp(&left.0));

    let stale: Vec<_> = backups
        .into_iter()
        .filter(|(created_at, _)| now.saturating_sub(*created_at) >= 30 * 24 * 60 * 60 * 1000)
        .skip(2)
        .collect();
    for (_, path) in stale {
        match fs::remove_file(&path) {
            Ok(()) => {
                tracing::info!(backup_path = %path.display(), "Deleted orphaned migration backup");
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => tracing::warn!(
                backup_path = %path.display(),
                %error,
                "Failed to delete orphaned migration backup"
            ),
        }
    }
    Ok(())
}

fn backup_timestamp_from_name(file_name: &str) -> Option<u64> {
    file_name
        .strip_prefix("nyaterm.redb.bak-v1-")?
        .strip_suffix(".redb")?
        .parse()
        .ok()
}

pub(super) fn has_legacy_data(db: &Database) -> AppResult<bool> {
    Ok(!read_legacy_docs(db, JSON_DOCS_TABLE)?.is_empty()
        || !read_legacy_docs(db, TEXT_DOCS_TABLE)?.is_empty())
}

pub(super) fn get_schema_version_optional_from_db(db: &Database) -> AppResult<Option<u32>> {
    let txn = db.begin_read().map_err(storage_error)?;
    let table = match txn.open_table(META_TABLE) {
        Ok(table) => table,
        Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
        Err(error) => return Err(storage_error(error)),
    };
    let Some(raw) = table.get(META_SCHEMA_VERSION).map_err(storage_error)? else {
        return Ok(None);
    };
    parse_meta_u32(raw.value(), META_SCHEMA_VERSION).map(Some)
}
