pub fn build_portable_snapshot(
    app: &AppHandle,
    snapshot_kind: PortableSnapshotKind,
    device_id: &str,
) -> AppResult<PortableSnapshot> {
    let _ = config::load_config(app)?;
    let settings = config::load_app_settings(app)?;
    let history = if snapshot_kind == PortableSnapshotKind::Backup {
        crate::storage::list_command_history_entries(usize::MAX)?
    } else {
        Vec::new()
    };
    let mut sessions = config::load_sessions(app)?;
    if snapshot_kind == PortableSnapshotKind::Sync {
        strip_device_local_sessions(&mut sessions);
    }
    let portable_settings = PortableAppSettings::from_app_settings(&settings, &snapshot_kind);

    let mut snapshot = PortableSnapshot {
        schema_version: PORTABLE_SNAPSHOT_SCHEMA_VERSION,
        snapshot_kind: snapshot_kind.clone(),
        revision_id: uuid::Uuid::new_v4().to_string(),
        device_id: device_id.to_string(),
        created_at_ms: current_time_ms(),
        payload_hash: String::new(),
        app_version: app.package_info().version.to_string(),
        settings: portable_settings,
        sessions,
        keys: config::load_keys(app)?,
        passwords: config::load_passwords(app)?,
        credentials: config::load_credentials(app)?,
        otp: config::load_otp_entries(app)?,
        proxies: config::load_proxies(app)?,
        proxy_groups: config::load_proxy_groups(app)?,
        tunnels: config::load_tunnels(app)?,
        tunnel_groups: config::load_tunnel_groups(app)?,
        quick_commands: config::load_quick_commands(app)?,
        history,
        master_key_token: crate::storage::load_master_key_token()?,
        known_hosts: crate::storage::render_known_hosts_export()?,
        notes: crate::storage::load_notes_snapshot()?,
    };
    snapshot.payload_hash = calculate_payload_hash(&snapshot)?;
    Ok(snapshot)
}
