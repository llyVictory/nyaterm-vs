use crate::config;
use crate::core::SessionManager;
use crate::core::ai::{
    self, AgentApprovalManager, AiAuditLog, AiChatRequest, AiMessage, AiSession, AiSessionScope,
    AiStreamStart, AppendAiAuditRequest, ClaudeCodeAccountStatus, ClaudeCodeCliStatus,
    ClaudeCodeRuntime, CodexAccountStatus, CodexCliStatus, CodexLoginFlow, CodexLoginStart,
};
use crate::error::{AppError, AppResult};
use crate::storage::{self, SettingsDocKey};
use crate::utils::crypto;
use std::sync::Arc;
use tauri::Emitter;

#[tauri::command]
pub fn import_ai_provider_icon(path: String) -> AppResult<String> {
    super::connection::import_connection_icon_data_url(&path)
}

#[tauri::command]
pub fn start_ai_chat_stream(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<SessionManager>>,
    request: AiChatRequest,
) -> AppResult<AiStreamStart> {
    ai::start_chat_stream(app, state.inner().clone(), request)
}

#[tauri::command]
pub async fn list_ai_model_names(app: tauri::AppHandle) -> AppResult<Vec<ai::AiModelDiscovery>> {
    let mut models = ai::list_model_names(&app).await?;
    let settings = config::load_app_settings(&app)?;
    let manager = ai::manager_from_app(&app).await?;
    models.extend(manager.list_models(&settings.ai).await?);
    Ok(models)
}

#[tauri::command]
pub async fn refresh_ai_model_settings(
    app: tauri::AppHandle,
    ai_settings: config::AiSettings,
) -> AppResult<config::AiSettings> {
    let existing = config::load_app_settings(&app)?;
    let mut merged_ai = config::merge_masked_ai_settings(&existing.ai, ai_settings);

    let mut discoveries = ai::list_model_names_for_settings(&merged_ai).await?;
    let manager = ai::manager_from_app(&app).await?;
    discoveries.extend(manager.list_models(&merged_ai).await?);

    merged_ai.models = merge_model_discoveries(&merged_ai, discoveries);
    merged_ai.default_model_id = update_default_model_id(&merged_ai, &merged_ai.models);
    config::normalize_ai_settings(&mut merged_ai);

    let mut persisted = existing;
    persisted.cloud_sync = config::encrypt_cloud_sync_settings(persisted.cloud_sync)?;
    persisted.ai = config::encrypt_ai_settings(merged_ai.clone())?;
    config::save_app_settings(&app, &persisted)?;

    let notify_app = app.clone();
    tauri::async_runtime::spawn(async move {
        crate::core::cloud_sync::notify_config_changed(&notify_app).await;
    });
    let _ = app.emit("settings-changed", ());
    crate::tray::schedule_refresh(&app);

    Ok(config::mask_ai_settings(merged_ai))
}

#[tauri::command]
pub async fn test_ai_provider_connection(
    app: tauri::AppHandle,
    ai_settings: config::AiSettings,
    credential_id: String,
) -> AppResult<Vec<String>> {
    let existing = config::load_app_settings(&app)?;
    let merged_ai = config::merge_masked_ai_settings(&existing.ai, ai_settings);
    ai::test_provider_connection(&merged_ai, &credential_id).await
}

#[tauri::command]
pub async fn test_ai_model_connection(
    app: tauri::AppHandle,
    ai_settings: config::AiSettings,
    model_id: String,
) -> AppResult<()> {
    let existing = config::load_app_settings(&app)?;
    let merged_ai = config::merge_masked_ai_settings(&existing.ai, ai_settings);
    ai::test_model_connection(&merged_ai, &model_id).await
}

#[tauri::command]
pub fn reveal_ai_provider_api_key(credential_id: String) -> AppResult<String> {
    let settings = storage::load_settings_doc::<config::AppSettings>(SettingsDocKey::AppSettings)?;
    let encrypted_key = settings
        .ai
        .provider_credentials
        .iter()
        .find(|credential| credential.id == credential_id)
        .and_then(|credential| credential.api_key.as_ref())
        .or_else(|| {
            settings
                .ai
                .provider_profiles
                .iter()
                .find(|profile| profile.id == credential_id)
                .and_then(|profile| profile.api_key.as_ref())
        })
        .filter(|key| !key.is_empty())
        .ok_or_else(|| AppError::Config("AI provider API key is not configured".to_string()))?;
    crypto::decrypt(encrypted_key)
}

use nyaterm_core::core::ai::catalog::{merge_model_discoveries, update_default_model_id};

#[tauri::command]
pub fn cancel_ai_chat_stream(stream_id: String) -> AppResult<()> {
    ai::cancel_chat_stream(stream_id)
}

#[tauri::command]
pub async fn detect_codex_cli(app: tauri::AppHandle) -> AppResult<CodexCliStatus> {
    let settings = config::load_app_settings(&app)?;
    Ok(ai::CodexAppServerManager::detect_cli(settings.ai.codex.executable_path).await)
}

#[tauri::command]
pub async fn get_codex_account_status(app: tauri::AppHandle) -> AppResult<CodexAccountStatus> {
    let settings = config::load_app_settings(&app)?;
    let manager = ai::manager_from_app(&app).await?;
    manager.account_read(&settings.ai).await
}

#[tauri::command]
pub async fn start_codex_login(
    app: tauri::AppHandle,
    flow: CodexLoginFlow,
) -> AppResult<CodexLoginStart> {
    let settings = config::load_app_settings(&app)?;
    let manager = ai::manager_from_app(&app).await?;
    manager.login_start(&settings.ai, flow).await
}

#[tauri::command]
pub async fn cancel_codex_login(app: tauri::AppHandle, login_id: String) -> AppResult<()> {
    let settings = config::load_app_settings(&app)?;
    let manager = ai::manager_from_app(&app).await?;
    manager.login_cancel(&settings.ai, login_id).await
}

#[tauri::command]
pub async fn logout_codex(app: tauri::AppHandle) -> AppResult<()> {
    let settings = config::load_app_settings(&app)?;
    let manager = ai::manager_from_app(&app).await?;
    manager.logout(&settings.ai).await
}

#[tauri::command]
pub async fn detect_claude_code_cli(app: tauri::AppHandle) -> AppResult<ClaudeCodeCliStatus> {
    let settings = config::load_app_settings(&app)?;
    Ok(ai::ClaudeCodeRuntime::detect_cli(settings.ai.claude_code.executable_path).await)
}

#[tauri::command]
pub async fn get_claude_code_account_status(
    app: tauri::AppHandle,
) -> AppResult<ClaudeCodeAccountStatus> {
    use tauri::Manager;

    let settings = config::load_app_settings(&app)?;
    let runtime = app.state::<Arc<ClaudeCodeRuntime>>().inner().clone();
    runtime.auth_status(&settings.ai).await
}

#[tauri::command]
pub async fn respond_agent_step(
    state: tauri::State<'_, Arc<AgentApprovalManager>>,
    stream_id: String,
    step_index: u16,
    approved: bool,
) -> AppResult<()> {
    let key = format!("{stream_id}-{step_index}");
    state.respond(&key, approved).await;
    Ok(())
}

#[tauri::command]
pub fn get_ai_sessions(app: tauri::AppHandle) -> AppResult<Vec<AiSession>> {
    ai::get_ai_sessions(&app)
}

#[tauri::command]
pub fn get_ai_messages(app: tauri::AppHandle, session_id: String) -> AppResult<Vec<AiMessage>> {
    ai::get_ai_messages(&app, session_id)
}

#[tauri::command]
pub fn clear_ai_history(app: tauri::AppHandle) -> AppResult<()> {
    ai::clear_ai_history(&app)
}

#[tauri::command]
pub fn delete_ai_session(app: tauri::AppHandle, session_id: String) -> AppResult<()> {
    ai::delete_ai_session(&app, session_id)
}

#[tauri::command]
pub fn rebind_ai_session(
    app: tauri::AppHandle,
    session_id: String,
    owner_scope: AiSessionScope,
) -> AppResult<AiSession> {
    ai::rebind_ai_session(&app, session_id, owner_scope)
}

#[tauri::command]
pub fn append_ai_audit(
    app: tauri::AppHandle,
    request: AppendAiAuditRequest,
) -> AppResult<AiAuditLog> {
    ai::append_ai_audit(&app, request)
}

#[tauri::command]
pub fn get_ai_audit_logs(
    app: tauri::AppHandle,
    limit: Option<usize>,
) -> AppResult<Vec<AiAuditLog>> {
    ai::get_ai_audit_logs(&app, limit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codex_model(id: &str, name: &str, enabled: bool) -> config::AiModelConfigItem {
        config::AiModelConfigItem {
            id: id.to_string(),
            name: name.to_string(),
            backend: config::AiBackendKind::Codex,
            provider_kind: None,
            credential_id: None,
            enabled,
            source: config::AiModelSource::RustGenai,
            last_seen_at: None,
            supported_reasoning_efforts: None,
        }
    }

    fn manual_model(id: &str, name: &str) -> config::AiModelConfigItem {
        config::AiModelConfigItem {
            id: id.to_string(),
            name: name.to_string(),
            backend: config::AiBackendKind::Genai,
            provider_kind: Some(config::AiProviderKind::OpenaiCompatible),
            credential_id: Some("credential-a".to_string()),
            enabled: true,
            source: config::AiModelSource::Manual,
            last_seen_at: None,
            supported_reasoning_efforts: None,
        }
    }

    #[test]
    fn model_discovery_merge_preserves_enabled_codex_and_manual_models() {
        let mut settings = config::AiSettings {
            models: vec![
                codex_model("codex:gpt-5-codex", "gpt-5-codex", true),
                manual_model("credential-a:manual-a", "manual-a"),
            ],
            default_model_id: Some("codex:gpt-5-codex".to_string()),
            ..config::AiSettings::default()
        };
        config::normalize_ai_settings(&mut settings);

        let models = merge_model_discoveries(
            &settings,
            vec![
                ai::AiModelDiscovery {
                    id: "codex:gpt-5-codex".to_string(),
                    name: "gpt-5-codex".to_string(),
                    backend: config::AiBackendKind::Codex,
                    provider_kind: None,
                    credential_id: None,
                    source: config::AiModelSource::RustGenai,
                },
                ai::AiModelDiscovery {
                    id: "codex:gpt-6-codex".to_string(),
                    name: "gpt-6-codex".to_string(),
                    backend: config::AiBackendKind::Codex,
                    provider_kind: None,
                    credential_id: None,
                    source: config::AiModelSource::RustGenai,
                },
            ],
        );

        let existing = models
            .iter()
            .find(|model| model.id == "codex:gpt-5-codex")
            .expect("existing codex model");
        assert!(existing.enabled);
        assert!(existing.last_seen_at.is_some());

        let fresh = models
            .iter()
            .find(|model| model.id == "codex:gpt-6-codex")
            .expect("new codex model");
        assert!(!fresh.enabled);

        assert!(
            models
                .iter()
                .any(|model| model.id == "credential-a:manual-a")
        );
        assert_eq!(
            update_default_model_id(&settings, &models).as_deref(),
            Some("codex:gpt-5-codex")
        );
    }
}
