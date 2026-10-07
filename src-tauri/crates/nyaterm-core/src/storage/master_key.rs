use crate::error::AppResult;

use super::Storage;
use super::tables::*;
use super::util::*;

impl Storage {
    pub fn load_master_key_token(&self) -> AppResult<Option<String>> {
        self.read_meta_string(META_MASTER_KEY)
    }
    pub fn save_master_key_token(&self, token: &str) -> AppResult<()> {
        let txn = self.db.begin_write().map_err(storage_error)?;
        write_meta_string(&txn, META_MASTER_KEY, token)?;
        txn.commit().map_err(storage_error)?;
        Ok(())
    }
}
