//! AES-256-GCM encryption for SSH credentials stored in redb JSON documents.
//!
//! ## Key hierarchy
//!
//! ```text
//! wrapping_key  = SHA-256("nyaterm-key-wrap-v1:" || master_password_or_home_path)
//! master.key    = redb text doc containing base64( wrap_nonce[12] || AES-256-GCM(wrapping_key, master_key[32]) )
//! sessions doc  = { "password": base64( nonce[12] || AES-256-GCM(master_key, plaintext) ), … }
//! ```
//!
//! When a master password is configured the wrapping key is derived from it;
//! otherwise installed mode uses the user's home directory path as the key
//! material, while portable mode uses `data/config/portable.key`.
//!
//! The master password itself is stored in the settings document encrypted with the
//! home-path-derived key (via [`encrypt_settings_secret`]) to avoid a circular
//! dependency during bootstrap.

use crate::error::{AppError, AppResult};
use aes_gcm::aead::{Aead, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Key, KeyInit};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::RwLock;

const WRAPPING_KEY_PREFIX: &[u8] = b"nyaterm-key-wrap-v1:";
const LEGACY_WRAPPING_KEY_PREFIX: &[u8] = b"dragonfly-key-wrap-v1:";

static MASTER_PASSWORD: RwLock<Option<String>> = RwLock::new(None);
static PORTABLE_KEY_PATH: RwLock<Option<PathBuf>> = RwLock::new(None);
static SERVER_KEY_MATERIAL: RwLock<Option<zeroize::Zeroizing<Vec<u8>>>> = RwLock::new(None);

/// Web bootstrap supplies an external random key before accessing storage.
/// It is never persisted, and avoids Desktop's home-path / portable-key fallback.
pub fn set_server_key_material(key: [u8; 32]) {
    *SERVER_KEY_MATERIAL.write().unwrap() = Some(zeroize::Zeroizing::new(key.to_vec()));
}

/// Set the master password used for wrapping key derivation.
/// Pass `None` to revert to home-path-based derivation.
pub fn set_master_password(password: Option<String>) {
    let mut pw = MASTER_PASSWORD.write().unwrap();
    *pw = password.filter(|s| !s.is_empty());
}

/// Set the portable-mode key path used when no master password is configured.
pub fn set_portable_key_path(path: Option<PathBuf>) {
    let mut portable_key_path = PORTABLE_KEY_PATH.write().unwrap();
    *portable_key_path = path;
}

/// Returns the currently configured master password, if any.
pub fn get_master_password() -> Option<String> {
    MASTER_PASSWORD.read().unwrap().clone()
}

/// Derives a wrapping key from the given material.
///
/// When `password` is `Some`, uses the password as key material.
/// Otherwise falls back to the runtime key material.
fn derive_wrapping_key_with_prefix(
    prefix: &[u8],
    password: Option<&str>,
) -> AppResult<Key<Aes256Gcm>> {
    let mut h = Sha256::new();
    h.update(prefix);
    match password {
        Some(pw) => h.update(pw.as_bytes()),
        None => h.update(fallback_key_material()?.as_slice()),
    }
    let digest = h.finalize();
    Ok(*Key::<Aes256Gcm>::from_slice(&digest))
}

fn fallback_key_material() -> AppResult<Vec<u8>> {
    if let Some(material) = SERVER_KEY_MATERIAL.read().unwrap().as_ref() {
        return Ok(material.to_vec());
    }
    if let Some(path) = PORTABLE_KEY_PATH.read().unwrap().clone() {
        return read_or_create_portable_key_material(path);
    }

    let home = dirs::home_dir()
        .ok_or_else(|| AppError::Crypto("cannot determine home directory".into()))?;
    Ok(home.to_string_lossy().as_bytes().to_vec())
}

fn read_or_create_portable_key_material(path: PathBuf) -> AppResult<Vec<u8>> {
    if path.exists() {
        let material = std::fs::read_to_string(&path)?;
        let material = material.trim();
        if material.is_empty() {
            return Err(AppError::Crypto(format!(
                "portable key file is empty: {}",
                path.display()
            )));
        }
        return Ok(material.as_bytes().to_vec());
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    use aes_gcm::aead::rand_core::RngCore as _;
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let material = B64.encode(bytes);
    std::fs::write(&path, format!("{material}\n"))?;
    Ok(material.into_bytes())
}

fn derive_wrapping_key(password: Option<&str>) -> AppResult<Key<Aes256Gcm>> {
    derive_wrapping_key_with_prefix(WRAPPING_KEY_PREFIX, password)
}

fn derive_legacy_wrapping_key(password: Option<&str>) -> AppResult<Key<Aes256Gcm>> {
    derive_wrapping_key_with_prefix(LEGACY_WRAPPING_KEY_PREFIX, password)
}

/// Derives the wrapping key from the current master password (or home path).
fn get_wrapping_key() -> AppResult<Key<Aes256Gcm>> {
    let pw = get_master_password();
    derive_wrapping_key(pw.as_deref())
}

/// Wraps `master_key` with `wrapping_key` and writes it to redb `master.key`.
fn write_wrapped_master_key(
    master_key: &Key<Aes256Gcm>,
    wrapping_key: &Key<Aes256Gcm>,
) -> AppResult<()> {
    let cipher = Aes256Gcm::new(wrapping_key);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let wrapped = cipher
        .encrypt(&nonce, master_key.as_slice())
        .map_err(|e| AppError::Crypto(format!("wrap master.key: {e}")))?;

    let mut combined = nonce.to_vec();
    combined.extend_from_slice(&wrapped);

    crate::storage::save_master_key_token(&B64.encode(&combined))?;
    Ok(())
}

/// Unwraps the master key from the raw bytes of `master.key`.
fn unwrap_master_key_bytes(raw: &[u8], wrapping_key: &Key<Aes256Gcm>) -> AppResult<Key<Aes256Gcm>> {
    if raw.len() < 13 {
        return Err(AppError::Crypto("master.key file is malformed".into()));
    }
    let cipher = Aes256Gcm::new(wrapping_key);
    let (nonce_bytes, ciphertext) = raw.split_at(12);
    let nonce = aes_gcm::Nonce::from_slice(nonce_bytes);
    let master_key_bytes = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| AppError::Crypto(format!("unwrap master.key: {e}")))?;

    if master_key_bytes.len() != 32 {
        return Err(AppError::Crypto("master key length mismatch".into()));
    }
    Ok(*Key::<Aes256Gcm>::from_slice(&master_key_bytes))
}

fn unwrap_master_key_with_compatible_wrapping(
    raw: &[u8],
    password: Option<&str>,
) -> AppResult<(Key<Aes256Gcm>, bool)> {
    let wrapping_key = derive_wrapping_key(password)?;
    match unwrap_master_key_bytes(raw, &wrapping_key) {
        Ok(master_key) => Ok((master_key, false)),
        Err(new_error) => {
            let legacy_wrapping_key = derive_legacy_wrapping_key(password)?;
            match unwrap_master_key_bytes(raw, &legacy_wrapping_key) {
                Ok(master_key) => Ok((master_key, true)),
                Err(legacy_error) => Err(AppError::Crypto(format!(
                    "unwrap master.key: NyaTerm key prefix failed ({new_error}); legacy Dragonfly key prefix failed ({legacy_error})"
                ))),
            }
        }
    }
}

/// Loads the master key from redb `master.key`, creating it on first use.
pub(crate) fn get_master_key() -> AppResult<Key<Aes256Gcm>> {
    if let Some(encoded) = crate::storage::load_master_key_token()? {
        let raw = B64
            .decode(encoded.trim())
            .map_err(|e| AppError::Crypto(format!("decode master.key: {e}")))?;

        let password = get_master_password();
        let (master_key, used_legacy_prefix) =
            unwrap_master_key_with_compatible_wrapping(&raw, password.as_deref())?;
        if used_legacy_prefix {
            let wrapping_key = derive_wrapping_key(password.as_deref())?;
            write_wrapped_master_key(&master_key, &wrapping_key)?;
        }
        Ok(master_key)
    } else {
        let master_key = Aes256Gcm::generate_key(OsRng);
        let wrapping_key = get_wrapping_key()?;
        write_wrapped_master_key(&master_key, &wrapping_key)?;
        Ok(master_key)
    }
}

/// Portable backups wrap a key with an explicit password without changing runtime keys.
pub(crate) fn backup_key_token(key: &Key<Aes256Gcm>, password: &str) -> AppResult<String> {
    let cipher = Aes256Gcm::new(&derive_wrapping_key(Some(password))?);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let mut bytes = nonce.to_vec();
    bytes.extend(
        cipher
            .encrypt(&nonce, key.as_slice())
            .map_err(|_| AppError::Crypto("Backup key wrapping failed".into()))?,
    );
    Ok(B64.encode(bytes))
}

pub(crate) fn backup_key(token: &str, password: &str) -> AppResult<Key<Aes256Gcm>> {
    let bytes = B64
        .decode(token)
        .map_err(|_| AppError::Crypto("Invalid backup key".into()))?;
    Ok(unwrap_master_key_with_compatible_wrapping(&bytes, Some(password))?.0)
}

pub(crate) fn transcode_secret(
    token: &str,
    source: &Key<Aes256Gcm>,
    target: &Key<Aes256Gcm>,
) -> AppResult<String> {
    let bytes = B64
        .decode(token)
        .map_err(|_| AppError::Crypto("Invalid backup credential".into()))?;
    if bytes.len() < 28 {
        return Err(AppError::Crypto("Invalid backup credential".into()));
    }
    let plain = zeroize::Zeroizing::new(
        Aes256Gcm::new(source)
            .decrypt(aes_gcm::Nonce::from_slice(&bytes[..12]), &bytes[12..])
            .map_err(|_| AppError::Crypto("Backup credential decryption failed".into()))?,
    );
    std::str::from_utf8(&plain)
        .map_err(|_| AppError::Crypto("Invalid backup credential encoding".into()))?;
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let mut encrypted = nonce.to_vec();
    encrypted.extend(
        Aes256Gcm::new(target)
            .encrypt(&nonce, plain.as_slice())
            .map_err(|_| AppError::Crypto("Backup credential encryption failed".into()))?,
    );
    Ok(B64.encode(encrypted))
}

pub fn verify_master_key_token() -> AppResult<()> {
    let _ = get_master_key()?;
    Ok(())
}

/// Re-wraps the existing master key when the master password changes.
///
/// `old_password` is the previous master password (`None` = home-path-based).
/// `new_password` is the new master password (`None` = revert to home-path-based).
pub fn rewrap_master_key(old_password: Option<&str>, new_password: Option<&str>) -> AppResult<()> {
    let master_key = if let Some(encoded) = crate::storage::load_master_key_token()? {
        let raw = B64
            .decode(encoded.trim())
            .map_err(|e| AppError::Crypto(format!("decode master.key: {e}")))?;

        unwrap_master_key_with_compatible_wrapping(&raw, old_password)?.0
    } else {
        Aes256Gcm::generate_key(OsRng)
    };

    let new_wrapping = derive_wrapping_key(new_password)?;
    write_wrapped_master_key(&master_key, &new_wrapping)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Settings-secret helpers (for the master password field in the settings document)
// ---------------------------------------------------------------------------

/// Encrypts a value using the home-path-derived key directly (not via master.key).
///
/// Used exclusively for storing the master password in settings.json to avoid
/// a circular dependency: we need the master password to unwrap master.key,
/// but master.key is needed to call [`encrypt`].
pub fn encrypt_settings_secret(plaintext: &str) -> AppResult<String> {
    let key = derive_wrapping_key(None)?;
    let cipher = Aes256Gcm::new(&key);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|e| AppError::Crypto(format!("encryption failed: {e}")))?;

    let mut combined = nonce.to_vec();
    combined.extend_from_slice(&ciphertext);
    Ok(B64.encode(&combined))
}

/// Decrypts a value produced by [`encrypt_settings_secret`].
pub fn decrypt_settings_secret(token: &str) -> AppResult<String> {
    let raw = B64
        .decode(token)
        .map_err(|e| AppError::Crypto(format!("invalid base64: {e}")))?;

    if raw.len() < 13 {
        return Err(AppError::Crypto("ciphertext too short".into()));
    }

    let key = derive_wrapping_key(None)?;
    match decrypt_settings_secret_bytes(&raw, &key) {
        Ok(value) => Ok(value),
        Err(new_error) => {
            let legacy_key = derive_legacy_wrapping_key(None)?;
            decrypt_settings_secret_bytes(&raw, &legacy_key).map_err(|legacy_error| {
                AppError::Crypto(format!(
                    "settings secret decryption failed: NyaTerm key prefix failed ({new_error}); legacy Dragonfly key prefix failed ({legacy_error})"
                ))
            })
        }
    }
}

fn decrypt_settings_secret_bytes(raw: &[u8], key: &Key<Aes256Gcm>) -> AppResult<String> {
    let cipher = Aes256Gcm::new(key);
    let (nonce_bytes, ciphertext) = raw.split_at(12);
    let nonce = aes_gcm::Nonce::from_slice(nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| AppError::Crypto(format!("decryption failed: {e}")))?;
    String::from_utf8(plaintext).map_err(|e| AppError::Crypto(format!("invalid UTF-8: {e}")))
}

// ---------------------------------------------------------------------------
// Public encrypt / decrypt (use the master key from master.key)
// ---------------------------------------------------------------------------

/// Encrypts `plaintext` with AES-256-GCM.
///
/// Returns `base64( nonce[12] || ciphertext+tag )`.
pub fn encrypt(plaintext: &str) -> AppResult<String> {
    let key = get_master_key()?;
    let cipher = Aes256Gcm::new(&key);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|e| AppError::Crypto(format!("encryption failed: {e}")))?;

    let mut combined = nonce.to_vec();
    combined.extend_from_slice(&ciphertext);
    Ok(B64.encode(&combined))
}

/// Decrypts a `base64( nonce || ciphertext )` token produced by [`encrypt`].
pub fn decrypt(token: &str) -> AppResult<String> {
    let key = get_master_key()?;
    let cipher = Aes256Gcm::new(&key);
    let raw = B64
        .decode(token)
        .map_err(|e| AppError::Crypto(format!("invalid base64: {e}")))?;

    if raw.len() < 13 {
        return Err(AppError::Crypto("ciphertext too short".into()));
    }

    let (nonce_bytes, ciphertext) = raw.split_at(12);
    let nonce = aes_gcm::Nonce::from_slice(nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| AppError::Crypto(format!("decryption failed: {e}")))?;

    String::from_utf8(plaintext).map_err(|e| AppError::Crypto(format!("invalid UTF-8: {e}")))
}

/// Decrypts an optional token, returning `None` when the input is `None` or empty.
pub fn decrypt_optional(token: &Option<String>) -> AppResult<Option<String>> {
    match token {
        Some(t) if !t.is_empty() => Ok(Some(decrypt(t)?)),
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wrap_key_for_test(master_key: &Key<Aes256Gcm>, wrapping_key: &Key<Aes256Gcm>) -> Vec<u8> {
        let cipher = Aes256Gcm::new(wrapping_key);
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let wrapped = cipher
            .encrypt(&nonce, master_key.as_slice())
            .expect("wrap test key");
        let mut combined = nonce.to_vec();
        combined.extend_from_slice(&wrapped);
        combined
    }

    fn encrypt_settings_secret_with_key_for_test(plaintext: &str, key: &Key<Aes256Gcm>) -> String {
        let cipher = Aes256Gcm::new(key);
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = cipher
            .encrypt(&nonce, plaintext.as_bytes())
            .expect("encrypt settings secret");
        let mut combined = nonce.to_vec();
        combined.extend_from_slice(&ciphertext);
        B64.encode(combined)
    }

    #[test]
    fn legacy_dragonfly_master_key_prefix_can_be_unwrapped() {
        let master_key = Aes256Gcm::generate_key(OsRng);
        let legacy_wrapping_key =
            derive_legacy_wrapping_key(Some("secret")).expect("legacy wrapping key");
        let raw = wrap_key_for_test(&master_key, &legacy_wrapping_key);

        let (unwrapped, used_legacy_prefix) =
            unwrap_master_key_with_compatible_wrapping(&raw, Some("secret"))
                .expect("unwrap with compatibility");

        assert!(used_legacy_prefix);
        assert_eq!(unwrapped.as_slice(), master_key.as_slice());
    }

    #[test]
    fn legacy_dragonfly_settings_secret_prefix_can_be_decrypted() {
        let legacy_key = derive_legacy_wrapping_key(None).expect("legacy settings key");
        let token = encrypt_settings_secret_with_key_for_test("stored-password", &legacy_key);

        assert_eq!(
            decrypt_settings_secret(&token).expect("decrypt legacy settings secret"),
            "stored-password"
        );
    }
}
