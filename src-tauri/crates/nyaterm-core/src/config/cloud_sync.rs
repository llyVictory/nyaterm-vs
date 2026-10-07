use super::{default_true, uuid_v4};
use crate::error::AppResult;
use crate::storage::{self, SettingsDocKey};
use crate::utils::crypto;
use serde::{Deserialize, Serialize};

pub const MASKED_SECRET_VALUE: &str = "__SET__";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WebdavSyncSettings {
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub root: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: Option<String>,
}

impl Default for WebdavSyncSettings {
    fn default() -> Self {
        Self {
            endpoint: String::new(),
            root: String::new(),
            username: String::new(),
            password: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct S3SyncSettings {
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub bucket: String,
    #[serde(default)]
    pub region: String,
    #[serde(default)]
    pub root: String,
    #[serde(default)]
    pub access_key_id: Option<String>,
    #[serde(default)]
    pub secret_access_key: Option<String>,
    #[serde(default)]
    pub session_token: Option<String>,
    #[serde(default)]
    pub virtual_host_style: bool,
}

impl Default for S3SyncSettings {
    fn default() -> Self {
        Self {
            endpoint: String::new(),
            bucket: String::new(),
            region: String::new(),
            root: String::new(),
            access_key_id: None,
            secret_access_key: None,
            session_token: None,
            virtual_host_style: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GiteeSnippetSyncSettings {
    #[serde(default = "default_gitee_api_endpoint")]
    pub api_endpoint: String,
    #[serde(default)]
    pub gist_id: String,
    #[serde(default)]
    pub access_token: Option<String>,
}

impl Default for GiteeSnippetSyncSettings {
    fn default() -> Self {
        Self {
            api_endpoint: default_gitee_api_endpoint(),
            gist_id: String::new(),
            access_token: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OAuthDriveSyncSettings {
    #[serde(default)]
    pub root: String,
    #[serde(default)]
    pub access_token: Option<String>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub client_secret: Option<String>,
}

impl Default for OAuthDriveSyncSettings {
    fn default() -> Self {
        Self {
            root: String::new(),
            access_token: None,
            refresh_token: None,
            client_id: None,
            client_secret: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AliyunDriveSyncSettings {
    #[serde(default)]
    pub root: String,
    #[serde(default)]
    pub access_token: Option<String>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub client_secret: Option<String>,
    #[serde(default = "default_aliyun_drive_type")]
    pub drive_type: String,
}

impl Default for AliyunDriveSyncSettings {
    fn default() -> Self {
        Self {
            root: String::new(),
            access_token: None,
            refresh_token: None,
            client_id: None,
            client_secret: None,
            drive_type: default_aliyun_drive_type(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GithubGistSyncSettings {
    #[serde(default)]
    pub gist_id: String,
    #[serde(default)]
    pub access_token: Option<String>,
}

impl Default for GithubGistSyncSettings {
    fn default() -> Self {
        Self {
            gist_id: String::new(),
            access_token: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CloudSyncSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default = "default_remote_root")]
    pub remote_root: String,
    #[serde(default = "default_device_name")]
    pub device_name: String,
    #[serde(default = "default_true")]
    pub auto_check_on_startup: bool,
    #[serde(default = "default_true")]
    pub auto_push_on_change: bool,
    #[serde(default = "default_true")]
    pub auto_pull_remote_changes: bool,
    #[serde(default = "default_sync_debounce_seconds")]
    pub sync_debounce_seconds: u64,
    #[serde(default)]
    pub webdav: WebdavSyncSettings,
    #[serde(default)]
    pub s3: S3SyncSettings,
    #[serde(default)]
    pub gitee_snippet: GiteeSnippetSyncSettings,
    #[serde(default)]
    pub google_drive: OAuthDriveSyncSettings,
    #[serde(default)]
    pub onedrive: OAuthDriveSyncSettings,
    #[serde(default)]
    pub aliyun_drive: AliyunDriveSyncSettings,
    #[serde(default)]
    pub github_gist: GithubGistSyncSettings,
}

impl Default for CloudSyncSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: default_provider(),
            remote_root: default_remote_root(),
            device_name: default_device_name(),
            auto_check_on_startup: true,
            auto_push_on_change: true,
            auto_pull_remote_changes: true,
            sync_debounce_seconds: default_sync_debounce_seconds(),
            webdav: WebdavSyncSettings::default(),
            s3: S3SyncSettings::default(),
            gitee_snippet: GiteeSnippetSyncSettings::default(),
            google_drive: OAuthDriveSyncSettings::default(),
            onedrive: OAuthDriveSyncSettings::default(),
            aliyun_drive: AliyunDriveSyncSettings::default(),
            github_gist: GithubGistSyncSettings::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CloudSyncState {
    #[serde(default = "uuid_v4")]
    pub device_id: String,
    #[serde(default)]
    pub last_synced_payload_hash: Option<String>,
    #[serde(default)]
    pub last_applied_remote_revision: Option<String>,
    #[serde(default)]
    pub last_checked_at_ms: Option<u64>,
    #[serde(default)]
    pub last_synced_at_ms: Option<u64>,
    #[serde(default)]
    pub last_validated_remote_revision: Option<String>,
    #[serde(default)]
    pub last_full_validation_at_ms: Option<u64>,
    #[serde(default)]
    pub last_gc_attempt_at_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudConflictPreview {
    pub detected_at_ms: u64,
    pub provider: String,
    #[serde(default = "default_conflict_kind")]
    pub kind: String,
    pub local_payload_hash: String,
    pub remote_payload_hash: String,
    pub remote_revision: String,
    pub remote_created_at_ms: u64,
    #[serde(default)]
    pub remote_device_id: String,
    #[serde(default)]
    pub recovery_revision: Option<String>,
    #[serde(default)]
    pub recovery_payload_hash: Option<String>,
    #[serde(default)]
    pub recovery_created_at_ms: Option<u64>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudSyncStatus {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub provider: String,
    #[serde(default = "default_status_state")]
    pub state: String,
    #[serde(default)]
    pub message: String,
    /// Stable machine-readable identifier for failures the UI reacts to, so the
    /// frontend never has to match prose error text.
    #[serde(default)]
    pub error_code: Option<String>,
    #[serde(default)]
    pub current_operation: Option<String>,
    #[serde(default)]
    pub last_checked_at_ms: Option<u64>,
    #[serde(default)]
    pub last_synced_at_ms: Option<u64>,
    #[serde(default)]
    pub conflict: Option<CloudConflictPreview>,
}

impl Default for CloudSyncStatus {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: default_provider(),
            state: default_status_state(),
            message: String::new(),
            error_code: None,
            current_operation: None,
            last_checked_at_ms: None,
            last_synced_at_ms: None,
            conflict: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudSyncHistoryEntry {
    #[serde(default = "uuid_v4")]
    pub id: String,
    pub timestamp_ms: u64,
    pub kind: String,
    pub status: String,
    pub trigger: String,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub revision: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    pub message: String,
}

fn default_provider() -> String {
    "webdav".to_string()
}

fn default_remote_root() -> String {
    "nyaterm".to_string()
}

fn default_gitee_api_endpoint() -> String {
    "https://gitee.com/api/v5".to_string()
}

fn default_aliyun_drive_type() -> String {
    "resource".to_string()
}

fn default_device_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "This Device".to_string())
}

fn default_sync_debounce_seconds() -> u64 {
    60
}

fn default_status_state() -> String {
    "idle".to_string()
}

fn default_conflict_kind() -> String {
    "content_conflict".to_string()
}

pub fn load_cloud_sync_settings(app: &impl Sized) -> AppResult<CloudSyncSettings> {
    let _ = app;
    storage::load_settings_doc(SettingsDocKey::CloudSyncSettings)
}

pub fn load_cloud_sync_state(app: &impl Sized) -> AppResult<CloudSyncState> {
    let _ = app;
    let mut state: CloudSyncState = storage::load_settings_doc(SettingsDocKey::CloudSyncState)?;
    if state.device_id.is_empty() {
        state.device_id = uuid_v4();
    }
    Ok(state)
}

pub fn save_cloud_sync_state(app: &impl Sized, state: &CloudSyncState) -> AppResult<()> {
    let _ = app;
    storage::save_settings_doc(SettingsDocKey::CloudSyncState, state)
}

pub fn decrypt_cloud_sync_settings(
    mut settings: CloudSyncSettings,
) -> AppResult<CloudSyncSettings> {
    settings.webdav.password = decrypt_secret(settings.webdav.password)?;
    settings.s3.access_key_id = decrypt_secret(settings.s3.access_key_id)?;
    settings.s3.secret_access_key = decrypt_secret(settings.s3.secret_access_key)?;
    settings.s3.session_token = decrypt_secret(settings.s3.session_token)?;
    settings.gitee_snippet.access_token = decrypt_secret(settings.gitee_snippet.access_token)?;
    decrypt_oauth_drive_settings(&mut settings.google_drive)?;
    decrypt_oauth_drive_settings(&mut settings.onedrive)?;
    decrypt_aliyun_drive_settings(&mut settings.aliyun_drive)?;
    settings.github_gist.access_token = decrypt_secret(settings.github_gist.access_token)?;
    Ok(settings)
}

pub fn encrypt_cloud_sync_settings(
    mut settings: CloudSyncSettings,
) -> AppResult<CloudSyncSettings> {
    settings.webdav.password = encrypt_secret(settings.webdav.password)?;
    settings.s3.access_key_id = encrypt_secret(settings.s3.access_key_id)?;
    settings.s3.secret_access_key = encrypt_secret(settings.s3.secret_access_key)?;
    settings.s3.session_token = encrypt_secret(settings.s3.session_token)?;
    settings.gitee_snippet.access_token = encrypt_secret(settings.gitee_snippet.access_token)?;
    encrypt_oauth_drive_settings(&mut settings.google_drive)?;
    encrypt_oauth_drive_settings(&mut settings.onedrive)?;
    encrypt_aliyun_drive_settings(&mut settings.aliyun_drive)?;
    settings.github_gist.access_token = encrypt_secret(settings.github_gist.access_token)?;
    Ok(settings)
}

pub fn mask_cloud_sync_settings(mut settings: CloudSyncSettings) -> CloudSyncSettings {
    settings.webdav.password = mask_secret(settings.webdav.password);
    settings.s3.access_key_id = mask_secret(settings.s3.access_key_id);
    settings.s3.secret_access_key = mask_secret(settings.s3.secret_access_key);
    settings.s3.session_token = mask_secret(settings.s3.session_token);
    settings.gitee_snippet.access_token = mask_secret(settings.gitee_snippet.access_token);
    mask_oauth_drive_settings(&mut settings.google_drive);
    mask_oauth_drive_settings(&mut settings.onedrive);
    mask_aliyun_drive_settings(&mut settings.aliyun_drive);
    settings.github_gist.access_token = mask_secret(settings.github_gist.access_token);
    settings
}

pub fn merge_masked_cloud_sync_settings(
    current: &CloudSyncSettings,
    mut next: CloudSyncSettings,
) -> CloudSyncSettings {
    next.webdav.password = merge_secret(
        current.webdav.password.as_ref(),
        next.webdav.password.as_ref(),
    );
    next.s3.access_key_id = merge_secret(
        current.s3.access_key_id.as_ref(),
        next.s3.access_key_id.as_ref(),
    );
    next.s3.secret_access_key = merge_secret(
        current.s3.secret_access_key.as_ref(),
        next.s3.secret_access_key.as_ref(),
    );
    next.s3.session_token = merge_secret(
        current.s3.session_token.as_ref(),
        next.s3.session_token.as_ref(),
    );
    next.gitee_snippet.access_token = merge_secret(
        current.gitee_snippet.access_token.as_ref(),
        next.gitee_snippet.access_token.as_ref(),
    );
    merge_oauth_drive_settings(&current.google_drive, &mut next.google_drive);
    merge_oauth_drive_settings(&current.onedrive, &mut next.onedrive);
    merge_aliyun_drive_settings(&current.aliyun_drive, &mut next.aliyun_drive);
    next.github_gist.access_token = merge_secret(
        current.github_gist.access_token.as_ref(),
        next.github_gist.access_token.as_ref(),
    );
    next
}

fn decrypt_oauth_drive_settings(settings: &mut OAuthDriveSyncSettings) -> AppResult<()> {
    settings.access_token = decrypt_secret(settings.access_token.take())?;
    settings.refresh_token = decrypt_secret(settings.refresh_token.take())?;
    settings.client_secret = decrypt_secret(settings.client_secret.take())?;
    Ok(())
}

fn encrypt_oauth_drive_settings(settings: &mut OAuthDriveSyncSettings) -> AppResult<()> {
    settings.access_token = encrypt_secret(settings.access_token.take())?;
    settings.refresh_token = encrypt_secret(settings.refresh_token.take())?;
    settings.client_secret = encrypt_secret(settings.client_secret.take())?;
    Ok(())
}

fn mask_oauth_drive_settings(settings: &mut OAuthDriveSyncSettings) {
    settings.access_token = mask_secret(settings.access_token.take());
    settings.refresh_token = mask_secret(settings.refresh_token.take());
    settings.client_secret = mask_secret(settings.client_secret.take());
}

fn merge_oauth_drive_settings(current: &OAuthDriveSyncSettings, next: &mut OAuthDriveSyncSettings) {
    next.access_token = merge_secret(current.access_token.as_ref(), next.access_token.as_ref());
    next.refresh_token = merge_secret(current.refresh_token.as_ref(), next.refresh_token.as_ref());
    next.client_secret = merge_secret(current.client_secret.as_ref(), next.client_secret.as_ref());
}

fn decrypt_aliyun_drive_settings(settings: &mut AliyunDriveSyncSettings) -> AppResult<()> {
    settings.access_token = decrypt_secret(settings.access_token.take())?;
    settings.refresh_token = decrypt_secret(settings.refresh_token.take())?;
    settings.client_secret = decrypt_secret(settings.client_secret.take())?;
    Ok(())
}

fn encrypt_aliyun_drive_settings(settings: &mut AliyunDriveSyncSettings) -> AppResult<()> {
    settings.access_token = encrypt_secret(settings.access_token.take())?;
    settings.refresh_token = encrypt_secret(settings.refresh_token.take())?;
    settings.client_secret = encrypt_secret(settings.client_secret.take())?;
    Ok(())
}

fn mask_aliyun_drive_settings(settings: &mut AliyunDriveSyncSettings) {
    settings.access_token = mask_secret(settings.access_token.take());
    settings.refresh_token = mask_secret(settings.refresh_token.take());
    settings.client_secret = mask_secret(settings.client_secret.take());
}

fn merge_aliyun_drive_settings(
    current: &AliyunDriveSyncSettings,
    next: &mut AliyunDriveSyncSettings,
) {
    next.access_token = merge_secret(current.access_token.as_ref(), next.access_token.as_ref());
    next.refresh_token = merge_secret(current.refresh_token.as_ref(), next.refresh_token.as_ref());
    next.client_secret = merge_secret(current.client_secret.as_ref(), next.client_secret.as_ref());
}

fn decrypt_secret(value: Option<String>) -> AppResult<Option<String>> {
    match value {
        Some(ciphertext) if !ciphertext.is_empty() => crypto::decrypt(&ciphertext).map(Some),
        _ => Ok(None),
    }
}

fn encrypt_secret(value: Option<String>) -> AppResult<Option<String>> {
    match value {
        Some(plaintext) if !plaintext.is_empty() => crypto::encrypt(&plaintext).map(Some),
        _ => Ok(None),
    }
}

fn mask_secret(value: Option<String>) -> Option<String> {
    value.and_then(|secret| {
        if secret.is_empty() {
            None
        } else {
            Some(MASKED_SECRET_VALUE.to_string())
        }
    })
}

fn merge_secret(current: Option<&String>, incoming: Option<&String>) -> Option<String> {
    match incoming.map(String::as_str) {
        Some(MASKED_SECRET_VALUE) | None => current.cloned(),
        Some("") => None,
        Some(value) => Some(value.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masked_cloud_sync_merge_preserves_new_provider_secrets() {
        let mut current = CloudSyncSettings::default();
        current.google_drive.access_token = Some("google-access".to_string());
        current.google_drive.refresh_token = Some("google-refresh".to_string());
        current.google_drive.client_secret = Some("google-secret".to_string());
        current.onedrive.access_token = Some("onedrive-access".to_string());
        current.aliyun_drive.refresh_token = Some("aliyun-refresh".to_string());
        current.github_gist.access_token = Some("github-token".to_string());

        let mut next = CloudSyncSettings::default();
        next.google_drive.access_token = Some(MASKED_SECRET_VALUE.to_string());
        next.google_drive.refresh_token = Some(MASKED_SECRET_VALUE.to_string());
        next.google_drive.client_secret = Some(MASKED_SECRET_VALUE.to_string());
        next.onedrive.access_token = Some(MASKED_SECRET_VALUE.to_string());
        next.aliyun_drive.refresh_token = Some(MASKED_SECRET_VALUE.to_string());
        next.github_gist.access_token = Some(MASKED_SECRET_VALUE.to_string());

        let merged = merge_masked_cloud_sync_settings(&current, next);

        assert_eq!(
            merged.google_drive.access_token.as_deref(),
            Some("google-access")
        );
        assert_eq!(
            merged.google_drive.refresh_token.as_deref(),
            Some("google-refresh")
        );
        assert_eq!(
            merged.google_drive.client_secret.as_deref(),
            Some("google-secret")
        );
        assert_eq!(
            merged.onedrive.access_token.as_deref(),
            Some("onedrive-access")
        );
        assert_eq!(
            merged.aliyun_drive.refresh_token.as_deref(),
            Some("aliyun-refresh")
        );
        assert_eq!(
            merged.github_gist.access_token.as_deref(),
            Some("github-token")
        );
    }

    #[test]
    fn legacy_cloud_sync_state_defaults_new_maintenance_fields() {
        let state: CloudSyncState = serde_json::from_value(serde_json::json!({
            "device_id": "device",
            "last_synced_payload_hash": "hash",
            "last_applied_remote_revision": "revision",
            "last_checked_at_ms": 10,
            "last_synced_at_ms": 20
        }))
        .expect("legacy cloud sync state");

        assert!(state.last_validated_remote_revision.is_none());
        assert!(state.last_full_validation_at_ms.is_none());
        assert!(state.last_gc_attempt_at_ms.is_none());
    }
}
