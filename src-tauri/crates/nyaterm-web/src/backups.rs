use crate::{
    auth::Owner,
    error::{Result, WebError},
    state::State,
};
use axum::{
    Extension, Json,
    extract::{Multipart, State as ExtractState},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use nyaterm_core::{core::backup, storage};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
#[derive(Deserialize)]
pub struct ExportRequest {
    password: String,
}

pub async fn export(
    ExtractState(state): ExtractState<Arc<State>>,
    Extension(_owner): Extension<Owner>,
    Json(request): Json<ExportRequest>,
) -> Result<Response> {
    let _guard = state.mutation.lock().await;
    let password = zeroize::Zeroizing::new(request.password);
    let bytes =
        crate::observability::blocking(move || backup::export_backup(&password, APP_VERSION))
            .await
            .map_err(|_| WebError::bad("Backup operation failed"))??;
    tracing::info!(
        event = "backup.export",
        bytes = bytes.len(),
        "Configuration backup exported"
    );
    Ok((
        [
            (header::CONTENT_TYPE, "application/octet-stream"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"nyaterm-backup.nya\"",
            ),
        ],
        bytes,
    )
        .into_response())
}

async fn uploaded(
    mut multipart: Multipart,
    limit: usize,
) -> Result<(zeroize::Zeroizing<String>, zeroize::Zeroizing<Vec<u8>>)> {
    let mut password = None;
    let mut bytes = None;
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|_| WebError::bad("Invalid upload"))?
    {
        let name = field.name().unwrap_or("").to_owned();
        let max = if name == "file" { limit } else { 1024 };
        let mut value = zeroize::Zeroizing::new(Vec::new());
        while let Some(chunk) = field
            .chunk()
            .await
            .map_err(|_| WebError::bad("Invalid upload"))?
        {
            if value.len().saturating_add(chunk.len()) > max {
                return Err(WebError(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "Upload exceeds limit".into(),
                ));
            }
            value.extend_from_slice(&chunk);
        }
        match name.as_str() {
            "password" | "source" if password.is_none() => {
                password = Some(zeroize::Zeroizing::new(
                    String::from_utf8(value.to_vec())
                        .map_err(|_| WebError::bad("Invalid upload field"))?,
                ))
            }
            "file" if bytes.is_none() => bytes = Some(value),
            _ => return Err(WebError::bad("Unexpected or duplicate upload field")),
        }
    }
    Ok((
        password.ok_or(WebError::bad("Missing upload field"))?,
        bytes.ok_or(WebError::bad("Missing file"))?,
    ))
}

pub async fn import(
    ExtractState(state): ExtractState<Arc<State>>,
    Extension(_owner): Extension<Owner>,
    multipart: Multipart,
) -> Result<Json<Value>> {
    let (password, bytes) = uploaded(multipart, backup::MAX_BACKUP_BYTES).await?;
    let snapshot =
        crate::observability::blocking(move || backup::prepare_import(&bytes, &password))
            .await
            .map_err(|_| WebError::bad("Backup operation failed"))?
            .map_err(import_error)?;
    let _guard = state.mutation.lock().await;
    crate::observability::blocking(move || {
        let settings = backup::settings_for_import(&snapshot)?;
        storage::backup::restore_backup(&snapshot, &settings)
    })
    .await
    .map_err(|_| WebError::bad("Backup operation failed"))?
    .map_err(import_error)?;
    crate::observability::reload_settings();
    for event in [
        "connections-changed",
        "settings-changed",
        "quick-commands-changed",
        "command-history-changed",
        "proxy-saved",
        "credentials-changed",
    ] {
        state.broadcast(event, Value::Null).await;
    }
    state
        .broadcast(
            "notes-changed",
            json!({"kind":"replaced","ids":[],"folders":[],"notes":[],"tree_changed":true}),
        )
        .await;
    tracing::info!(event = "backup.import", "Configuration backup restored");
    Ok(Json(json!({"status":"completed"})))
}

pub async fn connections(
    ExtractState(state): ExtractState<Arc<State>>,
    Extension(_owner): Extension<Owner>,
    multipart: Multipart,
) -> Result<Json<Value>> {
    let (source, bytes) = uploaded(multipart, 10 * 1024 * 1024).await?;
    let _guard = state.mutation.lock().await;
    let result = crate::observability::blocking(move || nyaterm_core::core::importer::import_browser_connections(&source, &bytes)).await.map_err(|_| WebError::bad("Import operation failed"))?.map_err(|error| {
        // Only a fixed message may cross the API boundary; parser errors can contain imported secrets.
        crate::observability::record_error(&error);
        tracing::warn!(event="connections.import_failed", "Connection import failed");
        if matches!(&error, nyaterm_core::error::AppError::Config(message) if message.starts_with("WindTerm profile")) {
            WebError::bad("WindTerm profile resources required; import on Desktop and transfer a .nya backup")
        } else { WebError::bad("Invalid or unsupported connection import") }
    })?;
    state.broadcast("connections-changed", Value::Null).await;
    tracing::info!(
        event = "connections.import",
        count = result.imported,
        "Connections imported"
    );
    Ok(Json(serde_json::to_value(result)?))
}

fn import_error(error: nyaterm_core::error::AppError) -> WebError {
    crate::observability::record_error(&error);
    tracing::warn!(
        event = "backup.import_failed",
        "Backup password, integrity or data validation failed"
    );
    WebError::bad("Invalid backup, incorrect password, or unsupported backup data")
}
