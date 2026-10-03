use crate::{
    auth::Owner,
    error::{Result, WebError},
    state::State,
};
use axum::{
    Extension, Json,
    extract::{Path, State as ExtractState},
};
use nyaterm_core::{config, services, storage, utils::crypto};
use serde_json::{Value, json};
use std::sync::Arc;

fn argument<T: serde::de::DeserializeOwned>(args: &Value, name: &str) -> Result<T> {
    Ok(serde_json::from_value(args[name].clone())?)
}
fn text<'a>(args: &'a Value, name: &str) -> Result<&'a str> {
    args[name]
        .as_str()
        .ok_or(WebError::bad("Missing command argument"))
}
pub async fn route(
    ExtractState(state): ExtractState<Arc<State>>,
    Extension(owner): Extension<Owner>,
    Path(command): Path<String>,
    Json(args): Json<Value>,
) -> Result<Json<Value>> {
    // Explicit allowlist; no dynamic reflection or Desktop invoke handler here.
    let result = match command.as_str() {
        "get_app_runtime_info" => json!({"mode":"web","portable":false,"packageManager":null,"executableDir":"","dataDir":"","configDir":"","logDir":"","webviewDataDir":"","portableMarkerPath":null}),
        "get_app_settings" => {
            let mut settings = config::load_app_settings(&())?;
            if settings.security.master_password.is_some() { settings.security.master_password=Some("__SET__".into()); }
            settings.ai=config::mask_ai_settings(settings.ai);
            settings.cloud_sync=config::mask_cloud_sync_settings(settings.cloud_sync);
            json!(settings)
        },
        "save_app_settings" => {
            let _guard=state.mutation.lock().await;
            let mut next:config::AppSettings=argument(&args,"settings")?;
            let current=config::load_app_settings(&())?;
            // Master-password rotation remains a Desktop-only flow. Web cannot
            // disable or replace an existing master password through a settings save.
            if next.security.master_password.as_deref().is_some_and(|v| v!="__SET__") || next.security.master_password.is_none() && current.security.master_password.is_some() { return Err(WebError::unsupported()); }
            next.security.master_password=current.security.master_password;
            next.appearance.normalize_window_transparency();
            next.terminal.normalize_scrollback_lines();next.terminal.normalize_timestamp_format();next.recording.normalize();
            next.ai=config::encrypt_ai_settings(config::merge_masked_ai_settings(&current.ai,next.ai))?;
            next.cloud_sync=config::encrypt_cloud_sync_settings(config::merge_masked_cloud_sync_settings(&current.cloud_sync,next.cloud_sync))?;
            config::save_app_settings(&(),&next)?;
            state.broadcast("settings-changed",Value::Null).await;
            Value::Null
        },
        "save_app_ui_settings" => {
            let _guard=state.mutation.lock().await;
            let ui:config::UiConfig=argument(&args,"ui")?;
            storage::update_settings_doc::<config::AppSettings,_,_>(storage::SettingsDocKey::AppSettings,|settings| { settings.ui=ui;Ok(()) })?;
            state.broadcast("settings-changed",Value::Null).await;Value::Null
        },
        "get_saved_connections" => {
            let mut connections=config::load_config(&())?.connections;
            let values=connections.iter_mut().map(|conn|{let exists=conn.auth.as_ref().is_some_and(|a|a.password.is_some());if let Some(auth)=&mut conn.auth {auth.password=None;}let mut value=json!(conn);if !value["auth"].is_null(){value["auth"]["has_password"]=json!(exists);}value}).collect::<Vec<_>>();
            json!(values)
        },
        "save_connection" => {
            let _guard=state.mutation.lock().await;
            let connection:config::SavedConnection=argument(&args,"connection")?;
            if !matches!(connection.config,config::ConnectionType::Ssh { .. }) { return Err(WebError::unsupported()); }
            let id=services::save_connection(&(),connection)?;
            state.broadcast("connections-changed",Value::Null).await;json!(id)
        },
        "delete_connection" => {
            let _guard=state.mutation.lock().await;
            let mut config=config::load_config(&())?;config.connections.retain(|c|Some(c.id.as_str())!=args["id"].as_str());config::save_config(&(),&config)?;
            state.broadcast("connections-changed",Value::Null).await;Value::Null
        },
        "get_groups" => json!(config::load_config(&())?.groups),
        "get_connection_custom_icons" => json!(config::load_config(&())?.custom_icons),
        "get_supported_ssh_algorithms" => json!(nyaterm_core::ssh::algorithms::get_supported_ssh_algorithms()),
        "save_group" => {
            let _guard=state.mutation.lock().await;
            let mut group:config::Group=argument(&args,"group")?;if group.id.is_empty(){group.id=uuid::Uuid::new_v4().to_string();}
            let id=group.id.clone();let mut config=config::load_config(&())?;
            config.groups.retain(|g|g.id!=id);config.groups.push(group);config::save_config(&(),&config)?;
            state.broadcast("connections-changed",Value::Null).await;json!(id)
        },
        "get_saved_passwords" => json!(config::load_passwords(&())?.passwords.into_iter().map(|entry| json!({"id":entry.id,"name":entry.name,"username":entry.username,"has_password":entry.password.is_some()})).collect::<Vec<_>>()),
        "save_password" => {
            let _guard=state.mutation.lock().await;
            let mut entry:config::SavedPassword=argument(&args,"entry")?;if entry.id.is_empty(){entry.id=uuid::Uuid::new_v4().to_string();}
            let id=entry.id.clone();let mut config=config::load_passwords(&())?;
            entry.password=match entry.password.as_deref(){Some("")=>None,Some(secret)=>Some(crypto::encrypt(secret)?),None=>config.passwords.iter().find(|p|p.id==id).and_then(|p|p.password.clone())};
            config.passwords.retain(|p|p.id!=id);config.passwords.push(entry);config::save_passwords(&(),&config)?;json!(id)
        },
        "delete_password" => { let _guard=state.mutation.lock().await;let mut config=config::load_passwords(&())?;config.passwords.retain(|p|Some(p.id.as_str())!=args["id"].as_str());config::save_passwords(&(),&config)?;Value::Null },
        "get_ssh_keys" => json!(config::load_keys(&())?.keys.into_iter().map(|entry| json!({"id":entry.id,"name":entry.name,"has_key_data":entry.key.is_some(),"has_cert_data":entry.cert.is_some()})).collect::<Vec<_>>()),
        "save_ssh_key" => {
            let _guard=state.mutation.lock().await;
            let mut entry:config::SshKey=argument(&args,"key")?;
            if entry.key_file_path.is_some() || entry.cert_file_path.is_some() { return Err(WebError::unsupported()); }
            if entry.id.is_empty(){entry.id=uuid::Uuid::new_v4().to_string();}let id=entry.id.clone();
            let mut config=config::load_keys(&())?;let existing=config.keys.iter().find(|p|p.id==id);
            // The client can supply plaintext only via transient key_data. It may
            // never inject an encrypted on-disk token or a server filesystem path.
            entry.key=match entry.key_data.take(){Some(data)=>{services::validate_private_key_content(&data,entry.passphrase.as_deref())?;Some(crypto::encrypt(&data)?)},None=>existing.and_then(|k|k.key.clone())};
            entry.cert=match entry.cert_data.take(){Some(data)=>{services::validate_certificate_content(&data)?;Some(crypto::encrypt(&data)?)},None=>existing.and_then(|k|k.cert.clone())};
            if entry.key.is_none() { return Err(WebError::bad("Private key required")); }
            entry.passphrase=match entry.passphrase.as_deref(){Some("")=>None,Some(secret)=>Some(crypto::encrypt(secret)?),None=>existing.and_then(|k|k.passphrase.clone())};
            config.keys.retain(|p|p.id!=id);config.keys.push(entry);config::save_keys(&(),&config)?;json!(id)
        },
        "delete_ssh_key" => { let _guard=state.mutation.lock().await;let mut config=config::load_keys(&())?;config.keys.retain(|p|Some(p.id.as_str())!=args["id"].as_str());config::save_keys(&(),&config)?;Value::Null },
        "get_saved_credentials" => { let entries=config::load_credentials(&())?.credentials;json!(entries.into_iter().map(|mut e| { let exists=e.password.is_some();e.password=None;let mut value=json!(e);value["has_password"]=json!(exists);value }).collect::<Vec<_>>()) },
        "save_credential" => { let _guard=state.mutation.lock().await;let entry:config::SavedCredential=argument(&args,"entry")?;{ let mut config=config::load_credentials(&())?;let id=config::upsert_credential(&mut config,entry)?;config::save_credentials(&(),&config)?;state.broadcast("credentials-changed",Value::Null).await;json!(id) } },
        "get_connection_password_value" => {let conn=config::load_connection_by_id(&(),text(&args,"id")?)?;json!(conn.auth.as_ref().map(|a|crypto::decrypt_optional(&a.password)).transpose()?.flatten())},
        "get_saved_credential_password" => json!(config::load_credential_by_id(&(),text(&args,"id")?)?.password),
        "get_password_value" | "get_saved_password_value" => json!(config::load_password_by_id(&(),text(&args,"id")?)?.password),
        "get_ssh_key_passphrase" => json!(config::load_key_by_id(&(),text(&args,"id")?)?.passphrase),
        "get_ssh_key_private_key" => json!(config::decrypt_key_pem(&config::load_key_by_id(&(),text(&args,"id")?)?)?),
        "get_ssh_key_public_key" => { let key=config::load_key_by_id(&(),text(&args,"id")?)?;let plain=config::decrypt_key_pem(&key)?.ok_or(WebError::bad("Private key required"))?;json!(services::derive_public_key_for_copy(&plain,key.passphrase.as_deref())?) },
        "delete_credential" => { let _guard=state.mutation.lock().await;let mut config=config::load_credentials(&())?;let id=text(&args,"id")?;config.credentials.retain(|p|p.id!=id);config::save_credentials(&(),&config)?;state.broadcast("credentials-changed",Value::Null).await;Value::Null },
        "get_known_hosts" => json!(storage::list_known_hosts()?),
        "delete_known_host" => { storage::delete_known_host(text(&args,"id")?)?;Value::Null },
        "respond_host_key_verify" | "submit_ssh_auth_response" | "cancel_ssh_auth_request" | "submit_otp_response" => {
            let id=text(&args,"requestId")?;
            let mut prompts=state.prompts.lock().await;
            if !prompts.get(id).is_some_and(|p|p.owner==owner.0){return Err(WebError::forbidden());}
            let prompt=prompts.remove(id).unwrap();
            let reply=match command.as_str(){"respond_host_key_verify"=>args["accepted"].clone(),"cancel_ssh_auth_request"=>Value::Null,"submit_otp_response"=>args["responses"].clone(),_=>args["response"].clone()};
            let _=prompt.reply.send(reply);Value::Null
        },
        "close_session" => { state.session(&owner.0,text(&args,"sessionId")?).await?.cancel.cancel();Value::Null },
        "cancel_session_creation" => { let id=text(&args,"createRequestId")?;for session in state.sessions.lock().await.values().filter(|s|s.owner==owner.0&&s.request_id.as_deref()==Some(id)){session.cancel.cancel();}Value::Null },
        "list_sessions" | "get_sessions" => json!(state.sessions.lock().await.values().filter(|s|s.owner==owner.0).map(|s|s.info()).collect::<Vec<_>>()),
        "get_session_info" => state.session(&owner.0,text(&args,"sessionId")?).await?.info(),
        "try_get_terminal_cwd" | "get_session_cwd" => { state.session(&owner.0,text(&args,"sessionId")?).await?;Value::Null },
        "get_app_lock_state" => json!(false),
        "get_system_fonts" => json!(["JetBrains Mono","monospace"]),
        "get_system_font_infos" => json!([{"family":"JetBrains Mono","monospace":true}]),
        "get_plugin_catalog" | "list_plugins" | "get_plugins" => json!([]),
        "get_plugin_capabilities" => crate::plugins::capabilities(),
        "check_web_plugin_compatibility" => crate::plugins::check(&args["manifest"]),
        "get_plugin_marketplace" => json!({"target":"web","catalog":{"catalogVersion":1,"repository":{"id":"web","name":"Web"},"generatedAt":"","plugins":[]}}),
        "get_command_history" | "fuzzy_search_history" | "get_quick_commands" => json!([]),
        "register_command_submission" | "register_command_confirmation_candidate" => { state.session(&owner.0,text(&args,"sessionId")?).await?;Value::Null },
        "finish_recording_scope" | "notify_mcp_session_restore_complete" => Value::Null,
        command if crate::sftp::supports(command) => crate::sftp::command(&state,&owner.0,command,&args).await?,
        command if crate::ai::supports(command) => crate::ai::command(&state,&owner.0,command,&args).await?,
        _ => return Err(WebError::unsupported()),
    };
    Ok(Json(result))
}
