//! Password-portable backups. Runtime key material is never replaced during an operation.
use super::{backup_crypto, portable_snapshot::*};
use crate::{
    config,
    error::{AppError, AppResult},
    storage,
    utils::crypto,
};
use aes_gcm::{Aes256Gcm, Key, KeyInit, aead::OsRng};

pub const MAX_BACKUP_BYTES: usize = 50 * 1024 * 1024;

pub fn build_backup(app_version: &str) -> AppResult<PortableSnapshot> {
    let mut settings = config::load_app_settings(&())?;
    // Loading can retain undecipherable settings for recovery; exports must reject them.
    settings.ai = config::decrypt_ai_settings(settings.ai)?;
    let mut snapshot = PortableSnapshot {
        schema_version: PORTABLE_SNAPSHOT_SCHEMA_VERSION,
        snapshot_kind: PortableSnapshotKind::Backup,
        revision_id: uuid::Uuid::new_v4().to_string(),
        device_id: config::load_cloud_sync_state(&())?.device_id,
        created_at_ms: current_time_ms(),
        payload_hash: String::new(),
        app_version: app_version.into(),
        settings: PortableAppSettings::from_app_settings(&settings, &PortableSnapshotKind::Backup),
        sessions: config::load_sessions(&())?,
        keys: config::load_keys(&())?,
        passwords: config::load_passwords(&())?,
        credentials: config::load_credentials(&())?,
        otp: config::load_otp_entries(&())?,
        proxies: config::load_proxies(&())?,
        proxy_groups: config::load_proxy_groups(&())?,
        tunnels: config::load_tunnels(&())?,
        tunnel_groups: config::load_tunnel_groups(&())?,
        quick_commands: config::load_quick_commands(&())?,
        history: storage::list_command_history_entries(usize::MAX)?,
        master_key_token: storage::load_master_key_token()?,
        known_hosts: storage::render_known_hosts_export()?,
        notes: storage::load_notes_snapshot()?,
    };
    snapshot.payload_hash = calculate_payload_hash(&snapshot)?;
    Ok(snapshot)
}

fn transcode(
    snapshot: &mut PortableSnapshot,
    source: &Key<Aes256Gcm>,
    target: &Key<Aes256Gcm>,
) -> AppResult<()> {
    let convert = |value: &mut Option<String>| -> AppResult<()> {
        if let Some(token) = value {
            *token = crypto::transcode_secret(token, source, target)?;
        }
        Ok(())
    };
    for connection in &mut snapshot.sessions.connections {
        if let Some(auth) = &mut connection.auth {
            convert(&mut auth.password)?;
        }
    }
    for key in &mut snapshot.keys.keys {
        convert(&mut key.key)?;
        convert(&mut key.cert)?;
        convert(&mut key.passphrase)?;
    }
    for account in &mut snapshot.passwords.passwords {
        convert(&mut account.password)?;
    }
    for credential in &mut snapshot.credentials.credentials {
        convert(&mut credential.password)?;
    }
    for otp in &mut snapshot.otp.entries {
        convert(&mut otp.secret)?;
    }
    for proxy in &mut snapshot.proxies {
        convert(&mut proxy.password)?;
    }
    Ok(())
}

pub fn export_backup(password: &str, app_version: &str) -> AppResult<Vec<u8>> {
    validate_password(password)?;
    let mut snapshot = build_backup(app_version)?;
    let portable_key = Aes256Gcm::generate_key(OsRng);
    transcode(&mut snapshot, &crypto::get_master_key()?, &portable_key)?;
    snapshot.master_key_token = Some(crypto::backup_key_token(&portable_key, password)?);
    snapshot.payload_hash = calculate_payload_hash(&snapshot)?;
    let encoded = zeroize::Zeroizing::new(encode_portable_snapshot(&snapshot)?);
    let encrypted = backup_crypto::encrypt_snapshot_bytes(&encoded, password)?;
    if encrypted.len() > MAX_BACKUP_BYTES {
        return Err(AppError::Config("Backup exceeds 50 MiB".into()));
    }
    Ok(encrypted)
}

/// Decode and re-encrypt everything before acquiring a database write transaction.
pub fn prepare_import(bytes: &[u8], password: &str) -> AppResult<PortableSnapshot> {
    validate_password(password)?;
    if bytes.len() > MAX_BACKUP_BYTES {
        return Err(AppError::Config("Backup exceeds 50 MiB".into()));
    }
    let decoded = zeroize::Zeroizing::new(backup_crypto::decrypt_snapshot_bytes(bytes, password)?);
    let mut snapshot = decode_portable_snapshot(&decoded)?;
    if snapshot.snapshot_kind != PortableSnapshotKind::Backup {
        return Err(AppError::Config("Expected a configuration backup".into()));
    }
    storage::backup::validate_backup_data(&snapshot)?;
    let source = crypto::backup_key(
        snapshot
            .master_key_token
            .as_deref()
            .ok_or_else(|| AppError::Crypto("Backup key missing".into()))?,
        password,
    )?;
    transcode(&mut snapshot, &source, &crypto::get_master_key()?)?;
    // Keep the destination key token and master password. AI fields in portable settings are plaintext.
    snapshot.master_key_token = storage::load_master_key_token()?;
    snapshot.payload_hash = calculate_payload_hash(&snapshot)?;
    Ok(snapshot)
}

/// Merge target-only settings immediately before the transaction, under the instance lock.
pub fn settings_for_import(snapshot: &PortableSnapshot) -> AppResult<config::AppSettings> {
    let mut settings = snapshot.settings.clone().apply_to(
        config::load_app_settings(&())?,
        &PortableSnapshotKind::Backup,
    );
    settings.ai = config::encrypt_ai_settings(settings.ai)?;
    settings.cloud_sync = config::encrypt_cloud_sync_settings(settings.cloud_sync)?;
    Ok(settings)
}

fn validate_password(password: &str) -> AppResult<()> {
    if password.is_empty() || password.len() > 1024 {
        return Err(AppError::Config(
            "Backup password must contain 1–1024 bytes".into(),
        ));
    }
    Ok(())
}
