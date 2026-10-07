use crate::error::AppResult;
use tauri::Emitter;
pub fn import_sessions(
    app: tauri::AppHandle,
    file_path: String,
    windterm_master_password: Option<String>,
) -> AppResult<usize> {
    let count =
        nyaterm_core::core::importer::import_sessions(&app, file_path, windterm_master_password)?;
    if count > 0 {
        let _ = app.emit("connections-changed", ());
    }
    Ok(count)
}
pub fn import_termius_sessions(
    app: tauri::AppHandle,
    indexed_db_path: Option<String>,
) -> AppResult<usize> {
    let count = nyaterm_core::core::importer::import_termius_sessions(&app, indexed_db_path)?;
    if count > 0 {
        let _ = app.emit("connections-changed", ());
    }
    Ok(count)
}
