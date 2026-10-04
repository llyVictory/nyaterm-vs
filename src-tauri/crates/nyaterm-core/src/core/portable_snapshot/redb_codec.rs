fn insert_entity<T>(table: &mut redb::Table<'_, &str, &str>, key: &str, value: &T) -> AppResult<()>
where
    T: Serialize,
{
    let raw = serde_json::to_string(value)?;
    table.insert(key, raw.as_str()).map_err(storage_error)?;
    Ok(())
}

fn read_entity<T>(entities: &BTreeMap<String, String>, key: &str) -> AppResult<T>
where
    T: serde::de::DeserializeOwned,
{
    let raw = entities
        .get(key)
        .ok_or_else(|| AppError::Config(format!("portable snapshot missing entity '{key}'")))?;
    serde_json::from_str(raw).map_err(Into::into)
}

fn read_raw_entity(entities: &BTreeMap<String, String>, key: &str) -> AppResult<Box<RawValue>> {
    let raw = entities
        .get(key)
        .ok_or_else(|| AppError::Config(format!("portable snapshot missing entity '{key}'")))?;
    serde_json::from_str(raw).map_err(Into::into)
}

fn read_entity_or_default<T>(entities: &BTreeMap<String, String>, key: &str) -> AppResult<T>
where
    T: serde::de::DeserializeOwned + Default,
{
    entities
        .get(key)
        .map(|raw| serde_json::from_str(raw).map_err(Into::into))
        .transpose()
        .map(|value| value.unwrap_or_default())
}

fn read_string_table(
    txn: &redb::ReadTransaction,
    definition: TableDefinition<&str, &str>,
) -> AppResult<BTreeMap<String, String>> {
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

pub fn current_time_ms() -> u64 {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    u64::try_from(millis).unwrap_or(u64::MAX)
}

fn storage_error(error: impl std::fmt::Display) -> AppError {
    AppError::Storage(format!("Storage error: {error}"))
}

fn zip_error(error: impl std::fmt::Display) -> AppError {
    AppError::Storage(format!("portable snapshot zip error: {error}"))
}
