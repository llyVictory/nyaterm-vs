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
use tracing::Instrument;

#[derive(serde::Deserialize)]
struct SortOrderUpdate {
    id: String,
    sort_order: i32,
}

pub(crate) fn argument<T: serde::de::DeserializeOwned>(args: &Value, name: &str) -> Result<T> {
    Ok(serde_json::from_value(args[name].clone())?)
}
pub(crate) fn text<'a>(args: &'a Value, name: &str) -> Result<&'a str> {
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
    let span = tracing::info_span!("web.command", operation=%command, session_id=args["sessionId"].as_str());
    async move {
    // Explicit allowlist; no dynamic reflection or Desktop invoke handler here.
    let result = match command.as_str() {
        "import_keyword_highlight_rules" => {
            let _guard = state.mutation.lock().await;
            let content = text(&args, "content")?;
            if content.len() > 1024 * 1024 { return Err(WebError::bad("Import exceeds 1 MiB")); }
            let rules = nyaterm_core::core::keyword_highlights::parse_keyword_highlight_import(content)?;
            let result = storage::update_settings_doc::<config::AppSettings,_,_>(storage::SettingsDocKey::AppSettings, |settings| {
                nyaterm_core::core::keyword_highlights::merge_keyword_highlight_rules(&mut settings.terminal.keyword_highlights, rules, || uuid::Uuid::new_v4().to_string())
            })?;
            state.broadcast("settings-changed", Value::Null).await;
            json!(result)
        },
        "get_app_runtime_info" => json!({"mode":"web","portable":false,"packageManager":null,"executableDir":"","dataDir":"","configDir":"","logDir":"","webviewDataDir":"","portableMarkerPath":null}),
        "get_support_info" => json!({"os":format!("Web server ({})",std::env::consts::OS),"architecture":std::env::consts::ARCH,"runtime":"web","packageManager":null}),
        "get_app_settings" => {
            let mut settings = config::load_app_settings(&())?;
            // Web unlocks with the stored master password, or re-authenticates
            // with the Web login password when no master password is configured.
            settings.security.master_password=Some("__SET__".into());
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
            crate::observability::configure(&next.diagnostics);
            state.broadcast("settings-changed",Value::Null).await;
            Value::Null
        },
        "save_app_ui_settings" => {
            let _guard=state.mutation.lock().await;
            let ui:config::UiConfig=argument(&args,"ui")?;
            storage::update_settings_doc::<config::AppSettings,_,_>(storage::SettingsDocKey::AppSettings,|settings| { settings.ui=ui;Ok(()) })?;
            state.broadcast("settings-changed",Value::Null).await;Value::Null
        },
        "save_app_language" => {
            let _guard=state.mutation.lock().await;
            let language:String=argument(&args,"language")?;
            storage::update_settings_doc::<config::AppSettings,_,_>(storage::SettingsDocKey::AppSettings,|settings| { settings.ui.language=Some(language);Ok(()) })?;
            state.broadcast("settings-changed",Value::Null).await;Value::Null
        },
        "verify_master_password" => json!(crate::auth::verify_unlock(&state,&args).await?),
        "get_saved_connections" => {
            let mut connections=config::load_config(&())?.connections;
            let values=connections.iter_mut().map(|conn|{let exists=conn.auth.as_ref().is_some_and(|a|a.password.is_some());if let Some(auth)=&mut conn.auth {auth.password=None;}let mut value=json!(conn);if !value["auth"].is_null(){value["auth"]["has_password"]=json!(exists);}value}).collect::<Vec<_>>();
            json!(values)
        },
        "save_connection" => {
            let _guard=state.mutation.lock().await;
            let connection:config::SavedConnection=argument(&args,"connection")?;
            if !matches!(connection.config,config::ConnectionType::Ssh { .. } | config::ConnectionType::Telnet { .. } | config::ConnectionType::Vnc { .. }) { return Err(WebError::unsupported()); }
            let id=services::save_connection(&(),connection)?;
            state.broadcast("connections-changed",Value::Null).await;json!(id)
        },
        "delete_connection" => {
            let _guard=state.mutation.lock().await;
            let mut config=config::load_config(&())?;config.connections.retain(|c|Some(c.id.as_str())!=args["id"].as_str());config::save_config(&(),&config)?;
            state.broadcast("connections-changed",Value::Null).await;Value::Null
        },
        "update_connection_icon" | "update_connection_asset_from_monitoring" => {
            let _guard=state.mutation.lock().await;let mut cfg=config::load_config(&())?;
            let id=text(&args,"connectionId")?;
            let connection=cfg.connections.iter_mut().find(|c|c.id==id).ok_or(WebError::bad("Connection not found"))?;
            if command=="update_connection_icon" {connection.icon=args["icon"].as_str().map(str::trim).filter(|v|!v.is_empty()).map(String::from);connection.icon_auto_detect=Some(argument(&args,"iconAutoDetect")?);}
            else {
                let patch:config::AssetMetadata=argument(&args,"assetPatch")?;
                let asset=connection.asset.get_or_insert_with(Default::default);
                if patch.hostname.is_some(){asset.hostname=patch.hostname;}
                if patch.os_name.is_some(){asset.os_name=patch.os_name;}
                if patch.architecture.is_some(){asset.architecture=patch.architecture;}
                if patch.cpu_model.is_some(){asset.cpu_model=patch.cpu_model;}
                if patch.cpu_cores.is_some(){asset.cpu_cores=patch.cpu_cores;}
                if patch.memory_bytes.is_some(){asset.memory_bytes=patch.memory_bytes;}
                if patch.disks.is_some(){asset.disks=patch.disks;}
                if patch.updated_at.is_some(){asset.updated_at=patch.updated_at;}
                if let Some(accelerators)=patch.accelerators {if !accelerators.is_empty(){let types=accelerators.iter().map(|a|a.r#type.clone()).collect::<Vec<_>>();let current=asset.accelerators.get_or_insert_with(Vec::new);current.retain(|a|!types.contains(&a.r#type));current.extend(accelerators);}}
            }
            config::save_config(&(),&cfg)?;state.broadcast("connections-changed",Value::Null).await;Value::Null
        },
        "get_groups" => json!(config::load_config(&())?.groups),
        "delete_group" | "clear_all_connections" | "reorder_items" => {
            let _guard=state.mutation.lock().await;
            let mut cfg=config::load_config(&())?;
            match command.as_str() {
                "delete_group" => {
                    let mut ids=std::collections::HashSet::from([text(&args,"id")?.to_owned()]);
                    loop { let before=ids.len(); for group in &cfg.groups { if group.parent_id.as_ref().is_some_and(|id|ids.contains(id)) { ids.insert(group.id.clone()); } } if before==ids.len() { break; } }
                    cfg.groups.retain(|g|!ids.contains(&g.id));
                    cfg.connections.retain(|c|c.group_id.as_ref().is_none_or(|id|!ids.contains(id)));
                },
                "clear_all_connections" => { cfg.connections.clear();cfg.groups.clear(); },
                _ => {
                    let connections:Vec<SortOrderUpdate>=argument(&args,"connections")?;
                    let groups:Vec<SortOrderUpdate>=argument(&args,"groups")?;
                    for update in connections { if let Some(c)=cfg.connections.iter_mut().find(|c|c.id==update.id) { c.sort_order=update.sort_order; } }
                    for update in groups { if let Some(g)=cfg.groups.iter_mut().find(|g|g.id==update.id) { g.sort_order=update.sort_order; } }
                }
            }
            config::save_config(&(),&cfg)?;state.broadcast("connections-changed",Value::Null).await;Value::Null
        },
        "get_proxies" => json!(config::load_proxies(&())?.into_iter().map(|mut proxy| { proxy.password=None; proxy }).collect::<Vec<_>>()),
        "get_proxy_groups" => json!(config::load_proxy_groups(&())?),
        "get_proxy_password" => {
            let proxy=config::load_proxy_by_id(&(), text(&args,"proxyId")?)?.ok_or(WebError::bad("Proxy not found"))?;
            json!(crypto::decrypt_optional(&proxy.password)?)
        },
        "save_proxy" => {
            let _guard=state.mutation.lock().await;
            let mut proxy:config::ProxyConfig=argument(&args,"proxy")?;
            if !matches!(proxy.protocol.as_str(), "socks5" | "http") { return Err(WebError::bad("Web supports SOCKS5 and HTTP CONNECT proxies only")); }
            crate::network::validate_target(&proxy.host, proxy.port)?;
            if proxy.id.is_empty() { proxy.id=uuid::Uuid::new_v4().to_string(); }
            let mut proxies=config::load_proxies(&())?;
            proxy.password=match proxy.password.as_deref() { Some("")=>None,Some(secret)=>Some(crypto::encrypt(secret)?),None=>proxies.iter().find(|p|p.id==proxy.id).and_then(|p|p.password.clone()) };
            let id=proxy.id.clone();proxies.retain(|p|p.id!=id);proxies.push(proxy);config::save_proxies(&(),&proxies)?;
            state.broadcast("proxy-saved",Value::Null).await;json!(id)
        },
        "delete_proxy" => {
            let _guard=state.mutation.lock().await;let mut proxies=config::load_proxies(&())?;proxies.retain(|p|Some(p.id.as_str())!=args["proxyId"].as_str());config::save_proxies(&(),&proxies)?;
            state.broadcast("proxy-saved",Value::Null).await;Value::Null
        },
        "set_proxy_group" => {
            let _guard=state.mutation.lock().await;let mut proxies=config::load_proxies(&())?;
            let proxy=proxies.iter_mut().find(|p|Some(p.id.as_str())==args["proxyId"].as_str()).ok_or(WebError::bad("Proxy not found"))?;
            proxy.group_id=args["groupId"].as_str().map(String::from);config::save_proxies(&(),&proxies)?;state.broadcast("proxy-saved",Value::Null).await;Value::Null
        },
        "save_proxy_group" => {
            let _guard=state.mutation.lock().await;let mut group:config::ProxyGroup=argument(&args,"group")?;
            if group.id.is_empty() { group.id=uuid::Uuid::new_v4().to_string(); }let id=group.id.clone();let mut groups=config::load_proxy_groups(&())?;groups.retain(|g|g.id!=id);groups.push(group);config::save_proxy_groups(&(),&groups)?;state.broadcast("proxy-saved",Value::Null).await;json!(id)
        },
        "delete_proxy_group" => {
            let _guard=state.mutation.lock().await;let id=text(&args,"groupId")?;
            let mut groups=config::load_proxy_groups(&())?;groups.retain(|g|g.id!=id);config::save_proxy_groups(&(),&groups)?;
            let mut proxies=config::load_proxies(&())?;proxies.retain(|p|p.group_id.as_deref()!=Some(id));config::save_proxies(&(),&proxies)?;state.broadcast("proxy-saved",Value::Null).await;Value::Null
        },
        "get_connection_custom_icons" => json!(config::load_config(&())?.custom_icons),
        "delete_connection_custom_icon" => {
            let _guard=state.mutation.lock().await;let id=text(&args,"id")?;let mut cfg=config::load_config(&())?;cfg.custom_icons.retain(|icon|icon.id!=id);config::save_config(&(),&cfg)?;state.broadcast("connections-changed",Value::Null).await;Value::Null
        },
        "import_connection_icon" => {
            let _guard=state.mutation.lock().await;
            let data=text(&args,"dataUrl")?;
            if data.len()>512*1024 {return Err(WebError::bad("Icon exceeds 512 KiB"));}
            let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64;
            let icon=config::connection_custom_icon_from_data_url(data,text(&args,"name")?,now).ok_or(WebError::bad("Invalid icon data"))?;
            let mut cfg=config::load_config(&())?;cfg.custom_icons.retain(|i|i.id!=icon.id);cfg.custom_icons.push(icon.clone());config::save_config(&(),&cfg)?;state.broadcast("connections-changed",Value::Null).await;json!(icon)
        },
        "get_supported_ssh_algorithms" => json!(nyaterm_core::ssh::algorithms::get_supported_ssh_algorithms()),
        "save_group" => {
            let _guard=state.mutation.lock().await;
            let mut group:config::Group=argument(&args,"group")?;if group.id.is_empty(){group.id=uuid::Uuid::new_v4().to_string();}
            let id=group.id.clone();let mut config=config::load_config(&())?;
            config.groups.retain(|g|g.id!=id);config.groups.push(group);config::save_config(&(),&config)?;
            state.broadcast("connections-changed",Value::Null).await;json!(id)
        },
        "get_saved_passwords" => json!(config::load_passwords(&())?.passwords.into_iter().map(|entry| json!({"id":entry.id,"sort_order":entry.sort_order,"name":entry.name,"username":entry.username,"has_password":entry.password.is_some()})).collect::<Vec<_>>()),
        "save_password" => {
            let _guard=state.mutation.lock().await;
            let mut entry:config::SavedPassword=argument(&args,"entry")?;if entry.id.is_empty(){entry.id=uuid::Uuid::new_v4().to_string();}
            let id=entry.id.clone();let mut config=config::load_passwords(&())?;
            entry.sort_order=config::password_sort_order(&config,&id);
            entry.password=match entry.password.as_deref(){Some("")=>None,Some(secret)=>Some(crypto::encrypt(secret)?),None=>config.passwords.iter().find(|p|p.id==id).and_then(|p|p.password.clone())};
            config.passwords.retain(|p|p.id!=id);config.passwords.push(entry);config::save_passwords(&(),&config)?;json!(id)
        },
        "delete_password" => { let _guard=state.mutation.lock().await;let mut config=config::load_passwords(&())?;config.passwords.retain(|p|Some(p.id.as_str())!=args["id"].as_str());config::save_passwords(&(),&config)?;Value::Null },
        "get_ssh_keys" => json!(config::load_keys(&())?.keys.into_iter().map(|entry| json!({"id":entry.id,"sort_order":entry.sort_order,"name":entry.name,"has_key_data":entry.key.is_some(),"has_cert_data":entry.cert.is_some()})).collect::<Vec<_>>()),
        "save_ssh_key" => {
            let _guard=state.mutation.lock().await;
            let mut entry:config::SshKey=argument(&args,"key")?;
            if entry.key_file_path.is_some() || entry.cert_file_path.is_some() { return Err(WebError::unsupported()); }
            if entry.id.is_empty(){entry.id=uuid::Uuid::new_v4().to_string();}let id=entry.id.clone();
            let mut config=config::load_keys(&())?;let existing=config.keys.iter().find(|p|p.id==id);
            // The client can supply plaintext only via transient key_data. It may
            // never inject an encrypted on-disk token or a server filesystem path.
            entry.sort_order=config::key_sort_order(&config,&id);
            entry.key=match entry.key_data.take(){Some(data)=>{services::validate_private_key_content(&data,entry.passphrase.as_deref())?;Some(crypto::encrypt(&data)?)},None=>existing.and_then(|k|k.key.clone())};
            entry.cert=match entry.cert_data.take(){Some(data)=>{services::validate_certificate_content(&data)?;Some(crypto::encrypt(&data)?)},None=>existing.and_then(|k|k.cert.clone())};
            if entry.key.is_none() { return Err(WebError::bad("Private key required")); }
            entry.passphrase=match entry.passphrase.as_deref(){Some("")=>None,Some(secret)=>Some(crypto::encrypt(secret)?),None=>existing.and_then(|k|k.passphrase.clone())};
            config.keys.retain(|p|p.id!=id);config.keys.push(entry);config::save_keys(&(),&config)?;json!(id)
        },
        "delete_ssh_key" => { let _guard=state.mutation.lock().await;let mut config=config::load_keys(&())?;config.keys.retain(|p|Some(p.id.as_str())!=args["id"].as_str());config::save_keys(&(),&config)?;Value::Null },
        "reorder_passwords" => { let _guard=state.mutation.lock().await;let updates:Vec<SortOrderUpdate>=argument(&args,"updates")?;let mut cfg=config::load_passwords(&())?;config::reorder_passwords(&mut cfg,&updates.into_iter().map(|u|(u.id,u.sort_order)).collect::<Vec<_>>());config::save_passwords(&(),&cfg)?;Value::Null },
        "reorder_ssh_keys" => { let _guard=state.mutation.lock().await;let updates:Vec<SortOrderUpdate>=argument(&args,"updates")?;let mut cfg=config::load_keys(&())?;config::reorder_ssh_keys(&mut cfg,&updates.into_iter().map(|u|(u.id,u.sort_order)).collect::<Vec<_>>());config::save_keys(&(),&cfg)?;Value::Null },
        "get_saved_credentials" => { let entries=config::load_credentials(&())?.credentials;json!(entries.into_iter().map(|mut e| { let exists=e.password.is_some();e.password=None;let mut value=json!(e);value["has_password"]=json!(exists);value }).collect::<Vec<_>>()) },
        "save_credential" => { let _guard=state.mutation.lock().await;let entry:config::SavedCredential=argument(&args,"entry")?;{ let mut config=config::load_credentials(&())?;let id=config::upsert_credential(&mut config,entry)?;config::save_credentials(&(),&config)?;state.broadcast("credentials-changed",Value::Null).await;json!(id) } },
        "get_connection_password_value" => {let conn=config::load_connection_by_id(&(),text(&args,"id")?)?;json!(conn.auth.as_ref().map(|a|crypto::decrypt_optional(&a.password)).transpose()?.flatten())},
        "get_saved_credential_password" => json!(config::load_credential_by_id(&(),text(&args,"id")?)?.password),
        "get_password_value" | "get_saved_password_value" => json!(config::load_password_by_id(&(),text(&args,"id")?)?.password),
        "get_ssh_key_passphrase" => json!(config::load_key_by_id(&(),text(&args,"id")?)?.passphrase),
        "get_ssh_key_private_key" => json!(config::decrypt_key_pem(&config::load_key_by_id(&(),text(&args,"id")?)?)?),
        "get_ssh_key_public_key" => { let key=config::load_key_by_id(&(),text(&args,"id")?)?;let plain=config::decrypt_key_pem(&key)?.ok_or(WebError::bad("Private key required"))?;json!(services::derive_public_key_for_copy(&plain,key.passphrase.as_deref())?) },
        "delete_credential" => { let _guard=state.mutation.lock().await;let mut config=config::load_credentials(&())?;let id=text(&args,"id")?;config.credentials.retain(|p|p.id!=id);config::save_credentials(&(),&config)?;state.broadcast("credentials-changed",Value::Null).await;Value::Null },
        "reorder_credentials" => { let _guard=state.mutation.lock().await;let updates:Vec<SortOrderUpdate>=argument(&args,"updates")?;let mut cfg=config::load_credentials(&())?;config::reorder_credentials(&mut cfg,&updates.into_iter().map(|u|(u.id,u.sort_order)).collect::<Vec<_>>());config::save_credentials(&(),&cfg)?;state.broadcast("credentials-changed",Value::Null).await;Value::Null },
        "get_known_hosts" => json!(storage::list_known_hosts()?),
        "delete_known_host" => { storage::delete_known_host(text(&args,"id")?)?;Value::Null },
        "clear_known_hosts" => { let _guard=state.mutation.lock().await;storage::clear_known_hosts()?;Value::Null },
        "respond_host_key_verify" | "submit_ssh_auth_response" | "cancel_ssh_auth_request" | "submit_otp_response" | "respond_transfer_duplicate" => {
            let id=text(&args,"requestId")?;
            if command == "respond_transfer_duplicate" && !matches!(args["action"].as_str(),Some("skip"|"overwrite")) {return Err(WebError::bad("Invalid duplicate choice"));}
            let mut prompts=state.prompts.lock().await;
            if !prompts.get(id).is_some_and(|p|p.owner==owner.0){return Err(WebError::forbidden());}
            let prompt=prompts.remove(id).unwrap();
            let reply=match command.as_str(){"respond_host_key_verify"=>args["accepted"].clone(),"cancel_ssh_auth_request"=>Value::Null,"submit_otp_response"=>args["responses"].clone(),"respond_transfer_duplicate"=>{if !matches!(args["action"].as_str(),Some("skip"|"overwrite")){return Err(WebError::bad("Invalid duplicate choice"));}args["action"].clone()},_=>args["response"].clone()};
            let _=prompt.reply.send(reply);Value::Null
        },
        "close_session" => { state.session(&owner.0,text(&args,"sessionId")?).await?.cancel.cancel();Value::Null },
        "cancel_session_creation" => { let id=text(&args,"createRequestId")?;for session in state.sessions.lock().await.values().filter(|s|s.owner==owner.0&&s.request_id.as_deref()==Some(id)){session.cancel.cancel();}Value::Null },
        "vnc_input_batch" | "vnc_set_clipboard_text" | "vnc_reconnect" | "close_vnc_session" | "respond_vnc_server_key" => crate::vnc::command(&state,&owner.0,&command,&args).await?,
        "list_sessions" | "get_sessions" => json!(state.sessions.lock().await.values().filter(|s|s.owner==owner.0).map(|s|s.info()).collect::<Vec<_>>()),
        "get_session_info" => state.session(&owner.0,text(&args,"sessionId")?).await?.info(),
        "get_terminal_cwd" | "try_get_terminal_cwd" | "get_session_cwd" => { state.session(&owner.0,text(&args,"sessionId")?).await?;Value::Null },
        "get_app_lock_state" => json!(false),
        "get_system_fonts" => json!(["JetBrains Mono","monospace"]),
        "get_system_font_infos" => json!([{"family":"JetBrains Mono","monospace":true}]),
        "get_plugin_catalog" | "list_plugins" | "get_plugins" => json!([]),
        "get_plugin_capabilities" => crate::plugins::capabilities(),
        "check_web_plugin_compatibility" => crate::plugins::check(&args["manifest"]),
        "get_plugin_marketplace" => json!({"target":"web","catalog":{"catalogVersion":1,"repository":{"id":"web","name":"Web"},"generatedAt":"","plugins":[]}}),
        command if crate::suggestions::supports(command) => crate::suggestions::command(&state,&owner.0,command,&args).await?,
        command if crate::notes::supports(command) => crate::notes::command(&state,command,&args).await?,
        command if crate::otp::supports(command) => crate::otp::command(&state,command,&args).await?,
        command if crate::monitoring::supports(command) => crate::monitoring::command(&state,&owner.0,command,&args).await?,
        "translate_text" => {
            let settings=config::load_app_settings(&())?;
            let input=text(&args,"text")?;
            if input.len()>64*1024 { return Err(WebError::bad("Translation text exceeds 64 KiB")); }
            let target=text(&args,"targetLanguage")?;
            let target=if target.is_empty() { &settings.translation.target_language } else { target };
            json!(tokio::time::timeout(std::time::Duration::from_secs(30),nyaterm_core::core::translate::translate(text(&args,"provider")?,input,target,&settings.translation)).await.map_err(|_|WebError::bad("Translation timed out"))??)
        },
        "finish_recording_scope" | "notify_mcp_session_restore_complete" => Value::Null,
        command if crate::sftp::supports(command) => crate::sftp::command(&state,&owner.0,command,&args).await?,
        command if crate::ai::supports(command) => crate::ai::command(&state,&owner.0,command,&args).await?,
        _ => return Err(WebError::unsupported()),
    };
    tracing::info!(event="command.completed", status=200, "Web command completed");
    Ok(Json(result))
    }.instrument(span).await
}
