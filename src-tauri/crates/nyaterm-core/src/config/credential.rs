use super::uuid_v4;
use crate::error::{AppError, AppResult};
use crate::storage;
use crate::utils::crypto;
use serde::{Deserialize, Serialize};

fn default_enabled() -> bool {
    true
}

/// Terminal credential entry. The password field is AES-256-GCM encrypted on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedCredential {
    #[serde(default = "uuid_v4")]
    pub id: String,
    #[serde(default)]
    pub sort_order: i32,
    pub name: String,
    pub username: String,
    /// Encrypted password on disk; plaintext only after explicit decryption.
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub username_prompt_regex: Option<String>,
    #[serde(default)]
    pub password_prompt_regex: Option<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Transient: true when encrypted password data exists on disk.
    #[serde(default, skip_serializing)]
    pub has_password: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CredentialsConfig {
    #[serde(default)]
    pub credentials: Vec<SavedCredential>,
}

pub fn load_credentials(app: &impl Sized) -> AppResult<CredentialsConfig> {
    let _ = app;
    let mut config = CredentialsConfig {
        credentials: storage::list_credentials()?,
    };
    sort_credentials(&mut config.credentials);
    for credential in &mut config.credentials {
        credential.has_password = credential.password.is_some();
    }
    Ok(config)
}

pub fn save_credentials(app: &impl Sized, config: &CredentialsConfig) -> AppResult<()> {
    let _ = app;
    storage::replace_credentials(config)
}

pub fn load_credential_by_id(app: &impl Sized, id: &str) -> AppResult<SavedCredential> {
    let cfg = load_credentials(app)?;
    let mut entry = cfg
        .credentials
        .into_iter()
        .find(|credential| credential.id == id)
        .ok_or_else(|| AppError::Config(format!("Credential '{}' not found", id)))?;
    if let Some(ct) = entry.password.clone() {
        entry.password = crypto::decrypt(&ct).ok();
    }
    Ok(entry)
}

pub fn upsert_credential(
    config: &mut CredentialsConfig,
    mut entry: SavedCredential,
) -> AppResult<String> {
    if entry.id.is_empty() {
        entry.id = uuid_v4();
    }

    let target_id = entry.id.clone();
    let existing = config
        .credentials
        .iter()
        .find(|credential| credential.id == target_id);
    let existing_sort_order = existing.map(|credential| credential.sort_order);

    entry.password = match entry.password.as_deref() {
        Some(plain) if !plain.is_empty() => Some(crypto::encrypt(plain)?),
        _ => existing.and_then(|credential| credential.password.clone()),
    };
    entry.sort_order = existing_sort_order.unwrap_or_else(|| {
        config
            .credentials
            .iter()
            .map(|credential| credential.sort_order)
            .max()
            .unwrap_or(-1)
            + 1
    });

    if let Some(existing_entry) = config
        .credentials
        .iter_mut()
        .find(|credential| credential.id == target_id)
    {
        *existing_entry = entry;
    } else {
        config.credentials.push(entry);
    }

    Ok(target_id)
}

pub fn sort_credentials(credentials: &mut [SavedCredential]) {
    credentials.sort_by(|left, right| {
        left.sort_order
            .cmp(&right.sort_order)
            .then(left.id.cmp(&right.id))
    });
}

pub fn reorder_credentials(config: &mut CredentialsConfig, updates: &[(String, i32)]) {
    for (id, sort_order) in updates {
        if let Some(credential) = config
            .credentials
            .iter_mut()
            .find(|credential| credential.id == *id)
        {
            credential.sort_order = *sort_order;
        }
    }
    sort_credentials(&mut config.credentials);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_config_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("nyaterm-credential-{name}-{nanos}"))
    }

    #[test]
    fn upsert_preserves_password_and_list_hides_plaintext_after_decrypt() {
        let dir = unique_config_dir("upsert");
        fs::create_dir_all(&dir).expect("create temp dir");
        crate::storage::init(&dir).expect("init storage");
        crate::utils::crypto::set_master_password(None);

        let mut config = CredentialsConfig::default();
        let id = upsert_credential(
            &mut config,
            SavedCredential {
                id: String::new(),
                sort_order: 0,
                name: "Git".to_string(),
                username: "nyakang".to_string(),
                password: Some("secret".to_string()),
                username_prompt_regex: None,
                password_prompt_regex: None,
                enabled: true,
                has_password: false,
            },
        )
        .expect("save credential");

        assert_eq!(config.credentials.len(), 1);
        assert!(config.credentials[0].password.as_deref() != Some("secret"));
        let encrypted = config.credentials[0].password.clone();

        let updated_id = upsert_credential(
            &mut config,
            SavedCredential {
                id: id.clone(),
                sort_order: 99,
                name: "GitLab".to_string(),
                username: "nyakang".to_string(),
                password: None,
                username_prompt_regex: Some("Username:".to_string()),
                password_prompt_regex: Some("Password:".to_string()),
                enabled: true,
                has_password: false,
            },
        )
        .expect("update credential");

        assert_eq!(updated_id, id);
        assert_eq!(config.credentials[0].name, "GitLab");
        assert_eq!(config.credentials[0].sort_order, 0);
        assert_eq!(config.credentials[0].password, encrypted);
        let decrypted = config.credentials[0]
            .password
            .as_deref()
            .and_then(|ciphertext| crypto::decrypt(ciphertext).ok());
        assert_eq!(decrypted.as_deref(), Some("secret"));

        config.credentials.retain(|credential| credential.id != id);
        assert!(config.credentials.is_empty());

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn upsert_appends_new_credentials_to_end() {
        let mut config = CredentialsConfig {
            credentials: vec![
                SavedCredential {
                    id: "alpha".to_string(),
                    sort_order: 3,
                    name: "Alpha".to_string(),
                    username: String::new(),
                    password: None,
                    username_prompt_regex: None,
                    password_prompt_regex: None,
                    enabled: true,
                    has_password: false,
                },
                SavedCredential {
                    id: "beta".to_string(),
                    sort_order: 7,
                    name: "Beta".to_string(),
                    username: String::new(),
                    password: None,
                    username_prompt_regex: None,
                    password_prompt_regex: None,
                    enabled: true,
                    has_password: false,
                },
            ],
        };

        let id = upsert_credential(
            &mut config,
            SavedCredential {
                id: String::new(),
                sort_order: 0,
                name: "Gamma".to_string(),
                username: String::new(),
                password: None,
                username_prompt_regex: None,
                password_prompt_regex: None,
                enabled: true,
                has_password: false,
            },
        )
        .expect("append credential");

        let appended = config
            .credentials
            .iter()
            .find(|credential| credential.id == id)
            .expect("new credential");
        assert_eq!(appended.sort_order, 8);
    }

    #[test]
    fn sort_credentials_orders_by_sort_order_then_id() {
        let mut credentials = vec![
            SavedCredential {
                id: "b".to_string(),
                sort_order: 1,
                name: "B".to_string(),
                username: String::new(),
                password: None,
                username_prompt_regex: None,
                password_prompt_regex: None,
                enabled: true,
                has_password: false,
            },
            SavedCredential {
                id: "a".to_string(),
                sort_order: 1,
                name: "A".to_string(),
                username: String::new(),
                password: None,
                username_prompt_regex: None,
                password_prompt_regex: None,
                enabled: true,
                has_password: false,
            },
            SavedCredential {
                id: "c".to_string(),
                sort_order: 0,
                name: "C".to_string(),
                username: String::new(),
                password: None,
                username_prompt_regex: None,
                password_prompt_regex: None,
                enabled: true,
                has_password: false,
            },
        ];

        sort_credentials(&mut credentials);

        assert_eq!(
            credentials
                .iter()
                .map(|credential| credential.id.as_str())
                .collect::<Vec<_>>(),
            vec!["c", "a", "b"]
        );
    }

    #[test]
    fn reorder_credentials_updates_and_sorts_config() {
        let mut config = CredentialsConfig {
            credentials: vec![
                SavedCredential {
                    id: "first".to_string(),
                    sort_order: 0,
                    name: "First".to_string(),
                    username: String::new(),
                    password: None,
                    username_prompt_regex: None,
                    password_prompt_regex: None,
                    enabled: true,
                    has_password: false,
                },
                SavedCredential {
                    id: "second".to_string(),
                    sort_order: 1,
                    name: "Second".to_string(),
                    username: String::new(),
                    password: None,
                    username_prompt_regex: None,
                    password_prompt_regex: None,
                    enabled: true,
                    has_password: false,
                },
                SavedCredential {
                    id: "third".to_string(),
                    sort_order: 2,
                    name: "Third".to_string(),
                    username: String::new(),
                    password: None,
                    username_prompt_regex: None,
                    password_prompt_regex: None,
                    enabled: true,
                    has_password: false,
                },
            ],
        };

        reorder_credentials(
            &mut config,
            &[
                ("third".to_string(), 0),
                ("first".to_string(), 1),
                ("second".to_string(), 2),
            ],
        );

        assert_eq!(
            config
                .credentials
                .iter()
                .map(|credential| (credential.id.as_str(), credential.sort_order))
                .collect::<Vec<_>>(),
            vec![("third", 0), ("first", 1), ("second", 2)]
        );
    }
}
