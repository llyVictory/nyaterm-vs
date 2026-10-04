use crate::error::{AppError, AppResult};
use crate::utils::crypto::get_master_password;
pub fn require_master_password() -> AppResult<String> {
    get_master_password()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| AppError::Config("master password is not set".into()))
}
pub fn encrypt_snapshot_bytes(plaintext: &[u8]) -> AppResult<Vec<u8>> {
    nyaterm_core::core::backup_crypto::encrypt_snapshot_bytes(
        plaintext,
        &require_master_password()?,
    )
}
pub fn decrypt_snapshot_bytes(ciphertext: &[u8]) -> AppResult<Vec<u8>> {
    nyaterm_core::core::backup_crypto::decrypt_snapshot_bytes(
        ciphertext,
        &require_master_password()?,
    )
}
