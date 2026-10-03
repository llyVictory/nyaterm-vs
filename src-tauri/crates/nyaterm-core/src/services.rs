use crate::config::{self, SavedConnection};
use crate::error::{AppError, AppResult};
use crate::utils::crypto;
use std::collections::HashSet;

pub fn save_connection(app: &impl Sized, mut connection: SavedConnection) -> AppResult<String> {
    let mut cfg = config::load_config(app)?;

    if connection.id.is_empty() {
        connection.id = uuid::Uuid::new_v4().to_string();
    }
    let target_id = connection.id.clone();
    let existing = cfg.connections.iter().find(|c| c.id == target_id).cloned();

    normalize_connection_for_save(&mut connection);

    config::validate_ssh_agent_settings(&connection.config)?;
    validate_proxy_jump_config(&connection, &cfg.connections)?;
    validate_ssh_algorithm_config(&connection)?;
    validate_sftp_settings_config(&connection)?;
    validate_rdp_config(&connection)?;
    validate_vnc_config(&connection)?;

    if let Some(ref mut auth) = connection.auth {
        // account_id: Some("") means explicitly cleared, None means preserve existing
        match auth.account_id.as_deref() {
            Some("") => auth.account_id = None,
            None => {
                auth.account_id = existing
                    .as_ref()
                    .and_then(|entry| entry.auth.as_ref())
                    .and_then(|entry| entry.account_id.clone());
            }
            _ => {}
        }

        // password_source: Some("") means legacy/default behavior, None means preserve existing
        match auth.password_source.as_deref() {
            Some("") => auth.password_source = None,
            None => {
                auth.password_source = existing
                    .as_ref()
                    .and_then(|entry| entry.auth.as_ref())
                    .and_then(|entry| entry.password_source.clone());
            }
            _ => {}
        }

        // password_id: Some("") means explicitly cleared, None means preserve existing
        match auth.password_id.as_deref() {
            Some("") => auth.password_id = None,
            None => {
                auth.password_id = existing
                    .as_ref()
                    .and_then(|e| e.auth.as_ref())
                    .and_then(|a| a.password_id.clone());
            }
            _ => {}
        }

        if matches!(
            connection.config,
            config::ConnectionType::Ssh { .. } | config::ConnectionType::Telnet { .. }
        ) && auth.account_id.as_deref().is_some_and(|id| !id.is_empty())
        {
            auth.password_id = None;
        }

        // password: non-empty = encrypt new value, "" = explicitly clear, None = preserve
        auth.password = match auth.password.as_deref() {
            Some(plain) if !plain.is_empty() => Some(crypto::encrypt(plain)?),
            Some("") => None,
            None => existing
                .as_ref()
                .and_then(|e| e.auth.as_ref())
                .and_then(|a| a.password.clone()),
            _ => None,
        };
        auth.has_password = false;
    }

    if let Some(existing_connection) = existing.as_ref() {
        connection.asset = existing_connection.asset.clone();
    }

    if let Some(ex) = cfg.connections.iter_mut().find(|c| c.id == target_id) {
        *ex = connection;
    } else {
        cfg.connections.push(connection);
    }
    config::save_config(app, &cfg)?;
    Ok(target_id)
}

pub fn normalize_connection_for_save(connection: &mut SavedConnection) {
    config::migrate_legacy_ssh_agent_settings(connection);
    config::migrate_legacy_asset_tags(connection);
}

pub fn validate_ssh_algorithm_config(connection: &SavedConnection) -> AppResult<()> {
    if !matches!(connection.config, config::ConnectionType::Ssh { .. }) {
        return Ok(());
    }

    let Some(preferences) = connection.ssh_algorithms.as_ref() else {
        return Ok(());
    };

    crate::ssh::algorithms::validate_ssh_algorithm_preferences(preferences)
}

pub fn validate_sftp_settings_config(connection: &SavedConnection) -> AppResult<()> {
    if !matches!(connection.config, config::ConnectionType::Ssh { .. }) {
        return Ok(());
    }

    let timeout_ms = connection.sftp.shell_detection_timeout_ms;
    if !(config::MIN_SFTP_SHELL_DETECTION_TIMEOUT_MS..=config::MAX_SFTP_SHELL_DETECTION_TIMEOUT_MS)
        .contains(&timeout_ms)
    {
        return Err(AppError::Config(format!(
            "SFTP shell detection timeout must be between {} and {} ms",
            config::MIN_SFTP_SHELL_DETECTION_TIMEOUT_MS,
            config::MAX_SFTP_SHELL_DETECTION_TIMEOUT_MS
        )));
    }

    Ok(())
}

pub fn validate_rdp_config(connection: &SavedConnection) -> AppResult<()> {
    let config::ConnectionType::Rdp {
        host,
        port,
        username,
        security,
        display,
        clipboard,
        reconnect,
        ..
    } = &connection.config
    else {
        return Ok(());
    };

    if host.trim().is_empty() {
        return Err(AppError::Config("RDP host is required".to_string()));
    }
    if *port == 0 {
        return Err(AppError::Config(
            "RDP port must be between 1 and 65535".to_string(),
        ));
    }
    if username.trim().is_empty() {
        return Err(AppError::Config("RDP username is required".to_string()));
    }
    if !matches!(
        security.certificate_policy.as_str(),
        "strict" | "prompt" | "accept-temporarily"
    ) {
        return Err(AppError::Config(
            "RDP certificate policy is invalid".to_string(),
        ));
    }
    if !matches!(display.mode.as_str(), "fit-window" | "fixed" | "native") {
        return Err(AppError::Config("RDP display mode is invalid".to_string()));
    }
    if !(640..=7680).contains(&display.width) || !(480..=4320).contains(&display.height) {
        return Err(AppError::Config(
            "RDP display size is outside the supported range".to_string(),
        ));
    }
    if !matches!(display.color_depth, 16 | 24 | 32) {
        return Err(AppError::Config("RDP color depth is invalid".to_string()));
    }
    if !matches!(
        clipboard.mode.as_str(),
        "disabled" | "text-only" | "text-and-files"
    ) {
        return Err(AppError::Config(
            "RDP clipboard mode is invalid".to_string(),
        ));
    }
    if reconnect.max_attempts > 20 {
        return Err(AppError::Config(
            "RDP reconnect attempts must be 20 or fewer".to_string(),
        ));
    }

    Ok(())
}

pub fn validate_vnc_config(connection: &SavedConnection) -> AppResult<()> {
    let config::ConnectionType::Vnc {
        host,
        port,
        security,
        display,
        reconnect,
        ..
    } = &connection.config
    else {
        return Ok(());
    };

    if host.trim().is_empty() {
        return Err(AppError::Config("VNC host is required".to_string()));
    }
    if *port == 0 {
        return Err(AppError::Config(
            "VNC port must be between 1 and 65535".to_string(),
        ));
    }
    if !matches!(security.mode.as_str(), "auto" | "vnc-auth" | "none") {
        return Err(AppError::Config("VNC security mode is invalid".to_string()));
    }
    if !matches!(display.scale_mode.as_str(), "fit" | "actual" | "stretch") {
        return Err(AppError::Config("VNC scale mode is invalid".to_string()));
    }
    if reconnect.max_attempts > 20 {
        return Err(AppError::Config(
            "VNC reconnect attempts must be 20 or fewer".to_string(),
        ));
    }

    Ok(())
}

pub fn validate_proxy_jump_config(
    connection: &SavedConnection,
    existing_connections: &[SavedConnection],
) -> AppResult<()> {
    let proxy_jump_id = connection
        .network
        .as_ref()
        .and_then(|network| network.proxy_jump_id.as_deref());

    let Some(proxy_jump_id) = proxy_jump_id else {
        return Ok(());
    };

    if !matches!(
        connection.config,
        config::ConnectionType::Ssh { .. }
            | config::ConnectionType::Rdp { .. }
            | config::ConnectionType::Vnc { .. }
    ) {
        return Err(AppError::Config(
            "ProxyJump is only supported for SSH, RDP, and VNC connections".to_string(),
        ));
    }

    let mut visited = HashSet::new();
    visited.insert(connection.id.as_str());
    let mut current_jump_id = proxy_jump_id;

    loop {
        if !visited.insert(current_jump_id) {
            if connection.id == current_jump_id {
                return Err(AppError::Config(
                    "A connection cannot use itself as a jump host".to_string(),
                ));
            }
            return Err(AppError::Config(format!(
                "ProxyJump chain contains a cycle at '{}'",
                current_jump_id
            )));
        }

        let jump_connection =
            find_connection_for_proxy_jump(connection, existing_connections, current_jump_id)
                .ok_or_else(|| {
                    AppError::Config(format!("Jump host '{}' not found", current_jump_id))
                })?;

        if !matches!(jump_connection.config, config::ConnectionType::Ssh { .. }) {
            return Err(AppError::Config(
                "Only SSH connections can be used as jump hosts".to_string(),
            ));
        }

        let Some(next_jump_id) = jump_connection
            .network
            .as_ref()
            .and_then(|network| network.proxy_jump_id.as_deref())
        else {
            break;
        };

        current_jump_id = next_jump_id;
    }

    Ok(())
}

fn find_connection_for_proxy_jump<'a>(
    edited_connection: &'a SavedConnection,
    existing_connections: &'a [SavedConnection],
    id: &str,
) -> Option<&'a SavedConnection> {
    if edited_connection.id == id {
        return Some(edited_connection);
    }

    existing_connections
        .iter()
        .find(|candidate| candidate.id == id)
}

fn missing_private_key_passphrase_error(error: &russh::keys::Error) -> bool {
    let message = error.to_string().to_lowercase();
    message.contains("encrypted")
        || message.contains("passphrase")
        || message.contains("password")
        || message.contains("cipher")
}

pub fn validate_private_key_content(content: &str, passphrase: Option<&str>) -> AppResult<()> {
    let usable_passphrase = passphrase.filter(|value| !value.is_empty());
    match russh::keys::decode_secret_key(content, usable_passphrase) {
        Ok(_) => Ok(()),
        Err(error)
            if usable_passphrase.is_none() && missing_private_key_passphrase_error(&error) =>
        {
            Ok(())
        }
        Err(error) => Err(AppError::Config(format!(
            "invalid SSH private key: {error}"
        ))),
    }
}

pub fn derive_public_key_for_copy(content: &str, passphrase: Option<&str>) -> AppResult<String> {
    let usable_passphrase = passphrase.filter(|value| !value.is_empty());
    if let Ok(private_key) = russh::keys::decode_secret_key(content, usable_passphrase) {
        return private_key.public_key().to_openssh().map_err(|error| {
            AppError::Config(format!("failed to encode SSH public key: {error}"))
        });
    }

    // OpenSSH containers keep the public key outside the encrypted private payload.
    let private_key = ssh_key::PrivateKey::from_openssh(content)
        .map_err(|error| AppError::Config(format!("failed to derive SSH public key: {error}")))?;
    private_key
        .public_key()
        .to_openssh()
        .map_err(|error| AppError::Config(format!("failed to encode SSH public key: {error}")))
}

pub fn validate_certificate_content(content: &str) -> AppResult<()> {
    russh::keys::Certificate::from_openssh(content)
        .map(|_| ())
        .map_err(|error| AppError::Config(format!("invalid OpenSSH certificate: {error}")))
}
