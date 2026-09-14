use super::uuid_v4;
use crate::error::{AppError, AppResult};
use crate::storage;
use crate::utils::crypto;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

/// Managed account entry. The password field is AES-256-GCM encrypted on disk.
///
/// The legacy `SavedPassword` name and storage namespace are retained for
/// backwards compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedPassword {
    #[serde(default = "uuid_v4")]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub username: String,
    /// Encrypted password on disk; plaintext only after `load_password_by_id`.
    #[serde(default)]
    pub password: Option<String>,
    /// Transient: true when encrypted password data exists on disk.
    #[serde(default, skip_serializing)]
    pub has_password: bool,
}

/// Returns the active saved-account reference for a connection auth block.
/// New `account_id` references always win over the legacy `password_id` field.
pub fn effective_account_id<'a>(
    account_id: Option<&'a str>,
    legacy_password_id: Option<&'a str>,
) -> Option<&'a str> {
    account_id
        .filter(|id| !id.is_empty())
        .or_else(|| legacy_password_id.filter(|id| !id.is_empty()))
}

/// Loads account metadata without decrypting its password. Missing references
/// are treated as unavailable so callers can apply protocol-specific fallback.
pub fn load_saved_account(
    app: &AppHandle,
    account_id: Option<&str>,
    legacy_password_id: Option<&str>,
) -> AppResult<Option<SavedPassword>> {
    let Some(id) = effective_account_id(account_id, legacy_password_id) else {
        return Ok(None);
    };
    Ok(load_passwords(app)?
        .passwords
        .into_iter()
        .find(|entry| entry.id == id))
}

pub fn resolve_account_username(account: Option<&SavedPassword>, fallback: &str) -> String {
    account
        .map(|entry| entry.username.trim())
        .filter(|username| !username.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

pub fn decrypt_account_password(account: Option<&SavedPassword>) -> AppResult<Option<String>> {
    let Some(ciphertext) = account.and_then(|entry| entry.password.as_deref()) else {
        return Ok(None);
    };
    crypto::decrypt(ciphertext).map(Some)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PasswordsConfig {
    #[serde(default)]
    pub passwords: Vec<SavedPassword>,
}

pub fn load_passwords(app: &AppHandle) -> AppResult<PasswordsConfig> {
    let _ = app;
    let mut config = PasswordsConfig {
        passwords: storage::list_passwords()?,
    };
    for p in &mut config.passwords {
        p.has_password = p.password.is_some();
    }
    Ok(config)
}

pub fn save_passwords(app: &AppHandle, config: &PasswordsConfig) -> AppResult<()> {
    let _ = app;
    storage::replace_passwords(config)
}

pub fn load_password_by_id(app: &AppHandle, id: &str) -> AppResult<SavedPassword> {
    let cfg = load_passwords(app)?;
    let mut entry = cfg
        .passwords
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| AppError::Config(format!("Password '{}' not found", id)))?;
    if let Some(ct) = entry.password.clone() {
        entry.password = crypto::decrypt(&ct).ok();
    }
    Ok(entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_saved_password_defaults_to_empty_username() {
        let entry: SavedPassword = serde_json::from_value(serde_json::json!({
            "id": "legacy",
            "name": "Legacy",
            "password": null
        }))
        .expect("legacy saved password");

        assert!(entry.username.is_empty());
    }

    #[test]
    fn account_id_takes_precedence_over_legacy_password_id() {
        assert_eq!(
            effective_account_id(Some("account"), Some("legacy")),
            Some("account")
        );
        assert_eq!(effective_account_id(None, Some("legacy")), Some("legacy"));
        assert_eq!(
            effective_account_id(Some(""), Some("legacy")),
            Some("legacy")
        );
    }

    #[test]
    fn account_username_falls_back_when_empty() {
        let mut entry: SavedPassword = serde_json::from_value(serde_json::json!({
            "id": "account",
            "name": "Account",
            "username": "",
            "password": null
        }))
        .expect("account");

        assert_eq!(resolve_account_username(Some(&entry), "root"), "root");
        entry.username = "admin".to_string();
        assert_eq!(resolve_account_username(Some(&entry), "root"), "admin");
    }
}
