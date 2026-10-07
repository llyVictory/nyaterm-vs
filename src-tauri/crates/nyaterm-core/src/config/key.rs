use super::uuid_v4;
use crate::error::{AppError, AppResult};
use crate::storage;
use crate::utils::crypto;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{RwLock, RwLockReadGuard};

static SSH_KEY_CHANGE_EPOCH: AtomicU64 = AtomicU64::new(0);
static SSH_KEY_ACCESS: RwLock<()> = RwLock::new(());

/// Managed SSH private key. Key material, certificates, and passphrases are encrypted on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshKey {
    #[serde(default = "uuid_v4")]
    pub id: String,
    pub name: String,
    /// Encrypted PEM content on disk.
    #[serde(default)]
    pub key: Option<String>,
    /// Encrypted OpenSSH user certificate content on disk.
    #[serde(default)]
    pub cert: Option<String>,
    /// Encrypted passphrase on disk.
    #[serde(default)]
    pub passphrase: Option<String>,

    /// Transient: plaintext private key content pasted from the UI.
    #[serde(default, skip_serializing)]
    pub key_data: Option<String>,
    /// Transient: plaintext OpenSSH user certificate content pasted from the UI.
    #[serde(default, skip_serializing)]
    pub cert_data: Option<String>,
    /// Transient: file path from the UI file picker.
    #[serde(default, skip_serializing)]
    pub key_file_path: Option<String>,
    /// Transient: certificate file path from the UI file picker.
    #[serde(default, skip_serializing)]
    pub cert_file_path: Option<String>,
    /// Transient: true when encrypted key data exists on disk.
    #[serde(default, skip_serializing)]
    pub has_key_data: bool,
    /// Transient: true when encrypted certificate data exists on disk.
    #[serde(default, skip_serializing)]
    pub has_cert_data: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KeysConfig {
    #[serde(default)]
    pub keys: Vec<SshKey>,
}

fn apply_key_status_flags(key: &mut SshKey) {
    key.has_key_data = key.key.is_some();
    key.has_cert_data = key.cert.is_some();
}

pub fn load_keys(app: &impl Sized) -> AppResult<KeysConfig> {
    let _ = app;
    let mut config = KeysConfig {
        keys: storage::list_ssh_keys()?,
    };
    for k in &mut config.keys {
        apply_key_status_flags(k);
    }
    Ok(config)
}

pub fn save_keys(app: &impl Sized, config: &KeysConfig) -> AppResult<()> {
    let _ = app;
    let _write_guard = SSH_KEY_ACCESS
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    storage::replace_ssh_keys(config)?;
    SSH_KEY_CHANGE_EPOCH.fetch_add(1, Ordering::SeqCst);
    Ok(())
}

/// Returns the process-local epoch of the last successful saved-key update.
///
/// Forwarding brokers may cache parsed signing identities for one epoch. Signing must still
/// acquire [`ssh_key_read_guard`] and verify the epoch before using cached private
/// material, so a successful save or deletion invalidates every stale signer.
pub fn ssh_key_change_epoch() -> u64 {
    SSH_KEY_CHANGE_EPOCH.load(Ordering::SeqCst)
}

/// Serializes saved-key snapshots and signing with successful key-store replacements.
///
/// Keep this guard only while reading the persistent snapshot or producing a
/// signature. PEM decryption and parsing deliberately happen after snapshot reads
/// release the guard, preventing slow key formats from blocking saves.
pub fn ssh_key_read_guard() -> RwLockReadGuard<'static, ()> {
    SSH_KEY_ACCESS
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub fn load_key_by_id(app: &impl Sized, id: &str) -> AppResult<SshKey> {
    let cfg = load_keys(app)?;
    let mut key = cfg
        .keys
        .into_iter()
        .find(|k| k.id == id)
        .ok_or_else(|| AppError::Config(format!("SSH key '{}' not found", id)))?;
    if let Some(ct) = key.passphrase.clone() {
        key.passphrase = crypto::decrypt(&ct).ok();
    }
    Ok(key)
}

pub fn decrypt_key_pem(key: &SshKey) -> AppResult<Option<String>> {
    crypto::decrypt_optional(&key.key)
}

pub fn decrypt_key_cert(key: &SshKey) -> AppResult<Option<String>> {
    crypto::decrypt_optional(&key.cert)
}

#[cfg(test)]
mod tests {
    use super::{SshKey, apply_key_status_flags};

    #[test]
    fn key_status_flags_track_stored_key_and_certificate_data() {
        let mut key = SshKey {
            id: "key-1".to_string(),
            name: "Key 1".to_string(),
            key: Some("encrypted-key".to_string()),
            cert: Some("encrypted-cert".to_string()),
            passphrase: None,
            key_data: None,
            cert_data: None,
            key_file_path: None,
            cert_file_path: None,
            has_key_data: false,
            has_cert_data: false,
        };

        apply_key_status_flags(&mut key);

        assert!(key.has_key_data);
        assert!(key.has_cert_data);

        key.cert = None;
        apply_key_status_flags(&mut key);

        assert!(key.has_key_data);
        assert!(!key.has_cert_data);
    }
}
