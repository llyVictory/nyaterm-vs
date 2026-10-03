use crate::error::AppResult;
use redb::ReadableTable;
use serde::{Serialize, de::DeserializeOwned};

use super::tables::*;
use super::util::*;
use super::{SettingsDocKey, Storage};

impl Storage {
    pub fn get_settings<T>(&self, key: &str) -> AppResult<Option<T>>
    where
        T: DeserializeOwned,
    {
        self.read_json(SETTINGS_TABLE, key)
    }
    pub fn save_settings<T>(&self, key: &str, value: &T) -> AppResult<()>
    where
        T: Serialize,
    {
        self.write_json(SETTINGS_TABLE, key, value)
    }
    pub fn get_settings_doc<T>(&self, key: SettingsDocKey) -> AppResult<Option<T>>
    where
        T: DeserializeOwned,
    {
        self.get_settings(key.storage_key())
    }
    pub fn save_settings_doc<T>(&self, key: SettingsDocKey, value: &T) -> AppResult<()>
    where
        T: Serialize,
    {
        self.save_settings(key.storage_key(), value)
    }
    pub fn update_settings_doc<T, R, F>(&self, key: SettingsDocKey, updater: F) -> AppResult<R>
    where
        T: DeserializeOwned + Default + Serialize,
        F: FnOnce(&mut T) -> AppResult<R>,
    {
        let txn = self.db.begin_write().map_err(storage_error)?;
        let result = {
            let settings_key = key.storage_key();
            let mut table = txn.open_table(SETTINGS_TABLE).map_err(storage_error)?;
            let mut document = match table.get(settings_key).map_err(storage_error)? {
                Some(raw) => deserialize_json::<T>(raw.value())?,
                None => T::default(),
            };
            let result = updater(&mut document)?;
            let content = serialize_json(&document)?;
            table
                .insert(settings_key, content.as_slice())
                .map_err(storage_error)?;
            result
        };
        txn.commit().map_err(storage_error)?;
        Ok(result)
    }
}
