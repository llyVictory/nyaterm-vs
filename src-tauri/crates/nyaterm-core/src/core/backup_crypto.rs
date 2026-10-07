use crate::error::{AppError, AppResult};

use aes_gcm::aead::{Aead, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Key, KeyInit};
use sha2::{Digest, Sha256};

const CLOUD_SNAPSHOT_KEY_PREFIX: &[u8] = b"nyaterm-cloud-snapshot-v1:";
const LEGACY_CLOUD_SNAPSHOT_KEY_PREFIX: &[u8] = b"dragonfly-cloud-snapshot-v1:";

fn derive_snapshot_key_with_prefix(prefix: &[u8], master_password: &str) -> Key<Aes256Gcm> {
    let mut hasher = Sha256::new();
    hasher.update(prefix);
    hasher.update(master_password.as_bytes());
    let digest = hasher.finalize();
    *Key::<Aes256Gcm>::from_slice(&digest)
}

fn derive_snapshot_key(master_password: &str) -> Key<Aes256Gcm> {
    derive_snapshot_key_with_prefix(CLOUD_SNAPSHOT_KEY_PREFIX, master_password)
}

fn derive_legacy_snapshot_key(master_password: &str) -> Key<Aes256Gcm> {
    derive_snapshot_key_with_prefix(LEGACY_CLOUD_SNAPSHOT_KEY_PREFIX, master_password)
}

pub fn encrypt_snapshot_bytes(plaintext: &[u8], master_password: &str) -> AppResult<Vec<u8>> {
    let key = derive_snapshot_key(&master_password);
    let cipher = Aes256Gcm::new(&key);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|error| AppError::Crypto(format!("cloud snapshot encryption failed: {error}")))?;

    let mut combined = nonce.to_vec();
    combined.extend_from_slice(&ciphertext);
    Ok(combined)
}

pub fn decrypt_snapshot_bytes(ciphertext: &[u8], master_password: &str) -> AppResult<Vec<u8>> {
    if ciphertext.len() < 13 {
        return Err(AppError::Crypto(
            "cloud snapshot ciphertext is too short".to_string(),
        ));
    }

    match decrypt_snapshot_bytes_with_key(ciphertext, &derive_snapshot_key(&master_password)) {
        Ok(plaintext) => Ok(plaintext),
        Err(new_error) => decrypt_snapshot_bytes_with_key(
            ciphertext,
            &derive_legacy_snapshot_key(&master_password),
        )
        .map_err(|legacy_error| {
            AppError::Crypto(format!(
                "cloud snapshot decryption failed: NyaTerm key prefix failed ({new_error}); legacy Dragonfly key prefix failed ({legacy_error})"
            ))
        }),
    }
}

fn decrypt_snapshot_bytes_with_key(ciphertext: &[u8], key: &Key<Aes256Gcm>) -> AppResult<Vec<u8>> {
    let cipher = Aes256Gcm::new(key);
    let (nonce_bytes, payload) = ciphertext.split_at(12);
    let nonce = aes_gcm::Nonce::from_slice(nonce_bytes);
    cipher
        .decrypt(nonce, payload)
        .map_err(|error| AppError::Crypto(format!("cloud snapshot decryption failed: {error}")))
}
