use crate::error::{AppError, AppResult};
use redb::{ReadableDatabase, ReadableTable, TableDefinition};
use serde::{Serialize, de::DeserializeOwned};
use std::time::{SystemTime, UNIX_EPOCH};

use super::Storage;
use super::tables::*;

impl Storage {
    pub(super) fn read_json<T>(
        &self,
        definition: TableDefinition<&str, &[u8]>,
        key: &str,
    ) -> AppResult<Option<T>>
    where
        T: DeserializeOwned,
    {
        let txn = self.db.begin_read().map_err(storage_error)?;
        let table = match txn.open_table(definition) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(error) => return Err(storage_error(error)),
        };
        let Some(raw) = table.get(key).map_err(storage_error)? else {
            return Ok(None);
        };
        deserialize_json(raw.value()).map(Some)
    }
    pub(super) fn write_json<T>(
        &self,
        definition: TableDefinition<&str, &[u8]>,
        key: &str,
        value: &T,
    ) -> AppResult<()>
    where
        T: Serialize,
    {
        let txn = self.db.begin_write().map_err(storage_error)?;
        write_json_in_txn(&txn, definition, key, value)?;
        txn.commit().map_err(storage_error)?;
        Ok(())
    }
    pub(super) fn remove_key(
        &self,
        definition: TableDefinition<&str, &[u8]>,
        key: &str,
    ) -> AppResult<()> {
        let txn = self.db.begin_write().map_err(storage_error)?;
        {
            let mut table = txn.open_table(definition).map_err(storage_error)?;
            table.remove(key).map_err(storage_error)?;
        }
        txn.commit().map_err(storage_error)?;
        Ok(())
    }
    pub(super) fn list_json_by_prefix<T>(
        &self,
        definition: TableDefinition<&str, &[u8]>,
        prefix: &str,
    ) -> AppResult<Vec<T>>
    where
        T: DeserializeOwned,
    {
        Ok(self
            .list_keyed_json_by_prefix(definition, prefix)?
            .into_iter()
            .map(|(_, value)| value)
            .collect())
    }
    pub(super) fn list_keyed_json_by_prefix<T>(
        &self,
        definition: TableDefinition<&str, &[u8]>,
        prefix: &str,
    ) -> AppResult<Vec<(String, T)>>
    where
        T: DeserializeOwned,
    {
        let txn = self.db.begin_read().map_err(storage_error)?;
        let table = match txn.open_table(definition) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(error) => return Err(storage_error(error)),
        };
        let mut values = Vec::new();
        for entry in table.iter().map_err(storage_error)? {
            let (key, value) = entry.map_err(storage_error)?;
            if key.value().starts_with(prefix) {
                values.push((key.value().to_string(), deserialize_json(value.value())?));
            }
        }
        Ok(values)
    }
    pub(super) fn read_raw_string(
        &self,
        definition: TableDefinition<&str, &[u8]>,
        key: &str,
    ) -> AppResult<Option<String>> {
        let txn = self.db.begin_read().map_err(storage_error)?;
        let table = match txn.open_table(definition) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(error) => return Err(storage_error(error)),
        };
        let Some(raw) = table.get(key).map_err(storage_error)? else {
            return Ok(None);
        };
        String::from_utf8(raw.value().to_vec())
            .map(Some)
            .map_err(|error| AppError::Storage(format!("Stored value is not UTF-8: {error}")))
    }

    pub(super) fn read_meta_string(&self, key: &str) -> AppResult<Option<String>> {
        let txn = self.db.begin_read().map_err(storage_error)?;
        let table = match txn.open_table(META_TABLE) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(error) => return Err(storage_error(error)),
        };
        Ok(table
            .get(key)
            .map_err(storage_error)?
            .map(|raw| raw.value().to_string()))
    }
    pub(super) fn hydrate_connection_passwords(
        &self,
        connections: &mut [crate::config::SavedConnection],
    ) -> AppResult<()> {
        let txn = self.db.begin_read().map_err(storage_error)?;
        let table = match txn.open_table(CREDENTIALS_TABLE) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(()),
            Err(error) => return Err(storage_error(error)),
        };
        for connection in connections {
            let Some(auth) = connection.auth.as_mut() else {
                continue;
            };
            let key = entity_key(CONNECTION_PASSWORD_PREFIX, &connection.id);
            if let Some(raw) = table.get(key.as_str()).map_err(storage_error)? {
                let record: ConnectionPasswordRecord = deserialize_json(raw.value())?;
                auth.password = Some(record.password);
                auth.has_password = true;
            }
        }
        Ok(())
    }
    pub(super) fn list_raw_by_prefix(
        &self,
        definition: TableDefinition<&str, &[u8]>,
        prefix: &str,
    ) -> AppResult<Vec<(String, Vec<u8>)>> {
        let txn = self.db.begin_read().map_err(storage_error)?;
        let table = match txn.open_table(definition) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(error) => return Err(storage_error(error)),
        };
        let mut values = Vec::new();
        for entry in table.iter().map_err(storage_error)? {
            let (key, value) = entry.map_err(storage_error)?;
            if key.value().starts_with(prefix) {
                values.push((key.value().to_string(), value.value().to_vec()));
            }
        }
        Ok(values)
    }
}

pub(super) fn parse_meta_u32(raw: &str, key: &str) -> AppResult<u32> {
    raw.parse::<u32>()
        .map_err(|error| AppError::Storage(format!("meta/{key} is not a u32: {error}")))
}
pub(super) fn read_meta_string_in_txn(
    txn: &redb::WriteTransaction,
    key: &str,
) -> AppResult<Option<String>> {
    let table = txn.open_table(META_TABLE).map_err(storage_error)?;
    let value = table
        .get(key)
        .map_err(storage_error)?
        .map(|raw| raw.value().to_string());
    Ok(value)
}
pub(super) fn read_meta_u32_in_txn(
    txn: &redb::WriteTransaction,
    key: &str,
) -> AppResult<Option<u32>> {
    read_meta_string_in_txn(txn, key)?
        .map(|raw| parse_meta_u32(&raw, key))
        .transpose()
}
pub(super) fn read_meta_u64_in_txn(
    txn: &redb::WriteTransaction,
    key: &str,
) -> AppResult<Option<u64>> {
    read_meta_string_in_txn(txn, key)?
        .map(|raw| {
            raw.parse::<u64>()
                .map_err(|error| AppError::Storage(format!("meta/{key} is not a u64: {error}")))
        })
        .transpose()
}
pub(super) fn write_meta_u32(txn: &redb::WriteTransaction, key: &str, value: u32) -> AppResult<()> {
    write_meta_string(txn, key, &value.to_string())
}
pub(super) fn write_meta_string(
    txn: &redb::WriteTransaction,
    key: &str,
    value: &str,
) -> AppResult<()> {
    let mut table = txn.open_table(META_TABLE).map_err(storage_error)?;
    table.insert(key, value).map_err(storage_error)?;
    Ok(())
}
pub(super) fn write_raw_bytes_in_txn(
    txn: &redb::WriteTransaction,
    definition: TableDefinition<&str, &[u8]>,
    key: &str,
    value: &[u8],
) -> AppResult<()> {
    let mut table = txn.open_table(definition).map_err(storage_error)?;
    table.insert(key, value).map_err(storage_error)?;
    Ok(())
}
pub(super) fn write_json_in_txn<T>(
    txn: &redb::WriteTransaction,
    definition: TableDefinition<&str, &[u8]>,
    key: &str,
    value: &T,
) -> AppResult<()>
where
    T: Serialize,
{
    let bytes = serialize_json(value)?;
    write_raw_bytes_in_txn(txn, definition, key, bytes.as_slice())
}
pub(super) fn serialize_json<T: Serialize>(value: &T) -> AppResult<Vec<u8>> {
    serde_json::to_vec(value).map_err(Into::into)
}
pub(super) fn deserialize_json<T: DeserializeOwned>(bytes: &[u8]) -> AppResult<T> {
    serde_json::from_slice(bytes).map_err(Into::into)
}
pub(super) fn entity_key(prefix: &str, id: &str) -> String {
    format!("{prefix}{id}")
}
pub(super) fn settings_doc_key(key: &str) -> String {
    format!("{SETTINGS_DOC_PREFIX}{key}")
}
pub(super) fn is_entity_json_doc(key: &str) -> bool {
    matches!(
        key,
        LEGACY_JSON_SETTINGS
            | LEGACY_JSON_SESSIONS
            | LEGACY_JSON_KEYS
            | LEGACY_JSON_PASSWORDS
            | LEGACY_JSON_CREDENTIALS
            | LEGACY_JSON_OTP
            | LEGACY_JSON_PROXIES
            | LEGACY_JSON_TUNNELS
            | LEGACY_JSON_HISTORY
    )
}
pub(super) fn current_time_ms() -> u64 {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    u64::try_from(millis).unwrap_or(u64::MAX)
}
pub(super) fn clear_prefix_in_txn(
    txn: &redb::WriteTransaction,
    definition: TableDefinition<&str, &[u8]>,
    prefix: &str,
) -> AppResult<()> {
    let table = txn.open_table(definition).map_err(storage_error)?;
    let mut keys = Vec::new();
    for entry in table.iter().map_err(storage_error)? {
        let (key, _) = entry.map_err(storage_error)?;
        if key.value().starts_with(prefix) {
            keys.push(key.value().to_string());
        }
    }
    drop(table);
    let mut table = txn.open_table(definition).map_err(storage_error)?;
    for key in keys {
        table.remove(key.as_str()).map_err(storage_error)?;
    }
    Ok(())
}
pub(super) fn clear_string_prefix_in_txn(
    txn: &redb::WriteTransaction,
    definition: TableDefinition<&str, &str>,
    prefix: &str,
) -> AppResult<()> {
    let table = txn.open_table(definition).map_err(storage_error)?;
    let mut keys = Vec::new();
    for entry in table.iter().map_err(storage_error)? {
        let (key, _) = entry.map_err(storage_error)?;
        if key.value().starts_with(prefix) {
            keys.push(key.value().to_string());
        }
    }
    drop(table);
    let mut table = txn.open_table(definition).map_err(storage_error)?;
    for key in keys {
        table.remove(key.as_str()).map_err(storage_error)?;
    }
    Ok(())
}
pub(super) fn storage_error(error: impl std::fmt::Display) -> AppError {
    AppError::Storage(format!("Storage error: {error}"))
}
