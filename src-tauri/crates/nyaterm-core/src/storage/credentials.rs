use crate::error::AppResult;

use super::Storage;
use super::tables::*;
use super::util::*;

impl Storage {
    pub fn save_credential(&self, credential: &crate::config::SavedCredential) -> AppResult<()> {
        self.write_json(
            CREDENTIALS_TABLE,
            &entity_key(CREDENTIAL_PREFIX, &credential.id),
            credential,
        )
    }
    pub fn get_credential(
        &self,
        credential_id: &str,
    ) -> AppResult<Option<crate::config::SavedCredential>> {
        self.read_json(
            CREDENTIALS_TABLE,
            &entity_key(CREDENTIAL_PREFIX, credential_id),
        )
    }
    pub fn delete_credential(&self, credential_id: &str) -> AppResult<()> {
        self.remove_key(
            CREDENTIALS_TABLE,
            &entity_key(CREDENTIAL_PREFIX, credential_id),
        )
    }
    pub fn list_credentials(&self) -> AppResult<Vec<crate::config::SavedCredential>> {
        self.list_json_by_prefix(CREDENTIALS_TABLE, CREDENTIAL_PREFIX)
    }
    pub fn replace_credentials(&self, config: &crate::config::CredentialsConfig) -> AppResult<()> {
        let txn = self.db.begin_write().map_err(storage_error)?;
        replace_credentials_in_txn(&txn, config)?;
        txn.commit().map_err(storage_error)?;
        Ok(())
    }
    pub fn list_passwords(&self) -> AppResult<Vec<crate::config::SavedPassword>> {
        self.list_json_by_prefix(CREDENTIALS_TABLE, PASSWORD_PREFIX)
    }
    pub fn replace_passwords(&self, config: &crate::config::PasswordsConfig) -> AppResult<()> {
        let txn = self.db.begin_write().map_err(storage_error)?;
        replace_passwords_in_txn(&txn, config)?;
        txn.commit().map_err(storage_error)?;
        Ok(())
    }
    pub fn list_ssh_keys(&self) -> AppResult<Vec<crate::config::SshKey>> {
        self.list_json_by_prefix(CREDENTIALS_TABLE, SSH_KEY_PREFIX)
    }
    pub fn replace_ssh_keys(&self, config: &crate::config::KeysConfig) -> AppResult<()> {
        let txn = self.db.begin_write().map_err(storage_error)?;
        replace_ssh_keys_in_txn(&txn, config)?;
        txn.commit().map_err(storage_error)?;
        Ok(())
    }
    pub fn save_otp_account(&self, account: &crate::config::OtpEntry) -> AppResult<()> {
        self.write_json(
            OTP_ACCOUNTS_TABLE,
            &entity_key(OTP_PREFIX, &account.id),
            account,
        )
    }
    pub fn get_otp_account(&self, otp_id: &str) -> AppResult<Option<crate::config::OtpEntry>> {
        self.read_json(OTP_ACCOUNTS_TABLE, &entity_key(OTP_PREFIX, otp_id))
    }
    pub fn delete_otp_account(&self, otp_id: &str) -> AppResult<()> {
        self.remove_key(OTP_ACCOUNTS_TABLE, &entity_key(OTP_PREFIX, otp_id))
    }
    pub fn list_otp_accounts(&self) -> AppResult<Vec<crate::config::OtpEntry>> {
        self.list_json_by_prefix(OTP_ACCOUNTS_TABLE, OTP_PREFIX)
    }
    pub fn replace_otp_accounts(&self, config: &crate::config::OtpConfig) -> AppResult<()> {
        let txn = self.db.begin_write().map_err(storage_error)?;
        replace_otp_in_txn(&txn, config)?;
        txn.commit().map_err(storage_error)?;
        Ok(())
    }
    pub fn save_proxy(&self, proxy: &crate::config::ProxyConfig) -> AppResult<()> {
        self.write_json(PROXIES_TABLE, &entity_key(PROXY_PREFIX, &proxy.id), proxy)
    }
    pub fn get_proxy(&self, proxy_id: &str) -> AppResult<Option<crate::config::ProxyConfig>> {
        self.read_json(PROXIES_TABLE, &entity_key(PROXY_PREFIX, proxy_id))
    }
    pub fn list_proxies(&self) -> AppResult<Vec<crate::config::ProxyConfig>> {
        self.list_json_by_prefix(PROXIES_TABLE, PROXY_PREFIX)
    }
    pub fn delete_proxy(&self, proxy_id: &str) -> AppResult<()> {
        self.remove_key(PROXIES_TABLE, &entity_key(PROXY_PREFIX, proxy_id))
    }
    pub fn replace_proxies(&self, proxies: &[crate::config::ProxyConfig]) -> AppResult<()> {
        let txn = self.db.begin_write().map_err(storage_error)?;
        replace_proxies_in_txn(&txn, proxies)?;
        txn.commit().map_err(storage_error)?;
        Ok(())
    }
    pub fn save_tunnel(&self, tunnel: &crate::config::TunnelConfig) -> AppResult<()> {
        self.write_json(
            TUNNELS_TABLE,
            &entity_key(TUNNEL_PREFIX, &tunnel.id),
            tunnel,
        )
    }
    pub fn get_tunnel(&self, tunnel_id: &str) -> AppResult<Option<crate::config::TunnelConfig>> {
        self.read_json(TUNNELS_TABLE, &entity_key(TUNNEL_PREFIX, tunnel_id))
    }
    pub fn list_tunnels(&self) -> AppResult<Vec<crate::config::TunnelConfig>> {
        self.list_json_by_prefix(TUNNELS_TABLE, TUNNEL_PREFIX)
    }
    pub fn delete_tunnel(&self, tunnel_id: &str) -> AppResult<()> {
        self.remove_key(TUNNELS_TABLE, &entity_key(TUNNEL_PREFIX, tunnel_id))
    }
    pub fn replace_tunnels(&self, tunnels: &[crate::config::TunnelConfig]) -> AppResult<()> {
        let txn = self.db.begin_write().map_err(storage_error)?;
        replace_tunnels_in_txn(&txn, tunnels)?;
        txn.commit().map_err(storage_error)?;
        Ok(())
    }
}

pub(super) fn replace_passwords_in_txn(
    txn: &redb::WriteTransaction,
    config: &crate::config::PasswordsConfig,
) -> AppResult<()> {
    clear_prefix_in_txn(txn, CREDENTIALS_TABLE, PASSWORD_PREFIX)?;
    for entry in &config.passwords {
        write_json_in_txn(
            txn,
            CREDENTIALS_TABLE,
            &entity_key(PASSWORD_PREFIX, &entry.id),
            entry,
        )?;
    }
    Ok(())
}
pub(super) fn replace_ssh_keys_in_txn(
    txn: &redb::WriteTransaction,
    config: &crate::config::KeysConfig,
) -> AppResult<()> {
    clear_prefix_in_txn(txn, CREDENTIALS_TABLE, SSH_KEY_PREFIX)?;
    for entry in &config.keys {
        write_json_in_txn(
            txn,
            CREDENTIALS_TABLE,
            &entity_key(SSH_KEY_PREFIX, &entry.id),
            entry,
        )?;
    }
    Ok(())
}
pub(super) fn replace_credentials_in_txn(
    txn: &redb::WriteTransaction,
    config: &crate::config::CredentialsConfig,
) -> AppResult<()> {
    clear_prefix_in_txn(txn, CREDENTIALS_TABLE, CREDENTIAL_PREFIX)?;
    for entry in &config.credentials {
        write_json_in_txn(
            txn,
            CREDENTIALS_TABLE,
            &entity_key(CREDENTIAL_PREFIX, &entry.id),
            entry,
        )?;
    }
    Ok(())
}
pub(super) fn replace_otp_in_txn(
    txn: &redb::WriteTransaction,
    config: &crate::config::OtpConfig,
) -> AppResult<()> {
    clear_prefix_in_txn(txn, OTP_ACCOUNTS_TABLE, OTP_PREFIX)?;
    for entry in &config.entries {
        write_json_in_txn(
            txn,
            OTP_ACCOUNTS_TABLE,
            &entity_key(OTP_PREFIX, &entry.id),
            entry,
        )?;
    }
    Ok(())
}
pub(super) fn replace_proxies_in_txn(
    txn: &redb::WriteTransaction,
    proxies: &[crate::config::ProxyConfig],
) -> AppResult<()> {
    clear_prefix_in_txn(txn, PROXIES_TABLE, PROXY_PREFIX)?;
    for proxy in proxies {
        write_json_in_txn(
            txn,
            PROXIES_TABLE,
            &entity_key(PROXY_PREFIX, &proxy.id),
            proxy,
        )?;
    }
    Ok(())
}
pub(super) fn replace_tunnels_in_txn(
    txn: &redb::WriteTransaction,
    tunnels: &[crate::config::TunnelConfig],
) -> AppResult<()> {
    clear_prefix_in_txn(txn, TUNNELS_TABLE, TUNNEL_PREFIX)?;
    for tunnel in tunnels {
        write_json_in_txn(
            txn,
            TUNNELS_TABLE,
            &entity_key(TUNNEL_PREFIX, &tunnel.id),
            tunnel,
        )?;
    }
    Ok(())
}
