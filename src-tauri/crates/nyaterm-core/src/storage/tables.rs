use hmac::Hmac;
use redb::TableDefinition;
use serde::{Deserialize, Serialize};
use sha1::Sha1;

pub(super) const DATABASE_FILE: &str = "nyaterm.redb";
pub(super) const SCHEMA_VERSION: u32 = 3;
pub(super) const META_SCHEMA_VERSION: &str = "schema_version";
pub(super) const META_MASTER_KEY: &str = "security/master_key";
pub(super) const META_MIGRATION_BACKUP_PATH: &str = "migration/v1_backup_path";
pub(super) const META_MIGRATION_BACKUP_CREATED_AT_MS: &str = "migration/v1_backup_created_at_ms";
pub(super) const META_MIGRATION_SUCCESSFUL_V3_STARTUPS: &str =
    "migration/v1_successful_v3_startups";
pub(super) const SETTINGS_DEFAULT: &str = "settings/default";
pub(super) const SETTINGS_DOC_PREFIX: &str = "settings/doc/";
pub(super) const GROUP_PREFIX: &str = "groups/";
pub(super) const CONNECTION_PREFIX: &str = "connections/";
pub(super) const CONNECTION_CUSTOM_ICON_PREFIX: &str = "connection_custom_icons/";
pub(super) const CREDENTIAL_PREFIX: &str = "credentials/credential/";
pub(super) const PASSWORD_PREFIX: &str = "credentials/password/";
pub(super) const SSH_KEY_PREFIX: &str = "credentials/key/";
pub(super) const CONNECTION_PASSWORD_PREFIX: &str = "credentials/connection-password/";
pub(super) const OTP_PREFIX: &str = "otp_accounts/";
pub(super) const PROXY_PREFIX: &str = "proxies/";
pub(super) const TUNNEL_PREFIX: &str = "tunnels/";
pub(super) const KNOWN_HOST_PREFIX: &str = "known_hosts/";
pub(super) const KNOWN_HOST_RAW_PREFIX: &str = "known_hosts/raw/";
pub(super) const RDP_KNOWN_HOST_PREFIX: &str = "rdp_known_hosts/";
pub(super) const VNC_KNOWN_HOST_PREFIX: &str = "vnc_known_hosts/";
pub(super) const COMMAND_HISTORY_PREFIX: &str = "command_history/";
pub(super) const NOTE_FOLDER_PREFIX: &str = "note_folders/";
pub(super) const NOTE_DOCUMENT_PREFIX: &str = "notes/";
pub(super) const NOTE_SUMMARY_PREFIX: &str = "note_summaries/";
pub(super) const META_NOTE_SUMMARY_INDEX_VERSION: &str = "notes/summary_index_version";

pub(super) const JSON_DOCS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("json_docs");
pub(super) const TEXT_DOCS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("text_docs");
pub const META_TABLE: TableDefinition<&str, &str> = TableDefinition::new("meta");
pub const SETTINGS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("settings");
pub const GROUPS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("groups");
pub const CONNECTIONS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("connections");
pub const CREDENTIALS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("credentials");
pub const OTP_ACCOUNTS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("otp_accounts");
pub const PROXIES_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("proxies");
pub const TUNNELS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("tunnels");
pub const KNOWN_HOSTS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("known_hosts");
pub const RDP_KNOWN_HOSTS_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("rdp_known_hosts");
pub const VNC_KNOWN_HOSTS_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("vnc_known_hosts");
pub const COMMAND_HISTORY_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("command_history");
pub const NOTE_FOLDERS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("note_folders");
pub const NOTES_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("notes");
pub const NOTE_SUMMARIES_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("note_summaries");
pub const IDX_CONNECTIONS_BY_GROUP_TABLE: TableDefinition<&str, &str> =
    TableDefinition::new("idx_connections_by_group");
pub const IDX_CONNECTIONS_BY_LAST_USED_TABLE: TableDefinition<&str, &str> =
    TableDefinition::new("idx_connections_by_last_used");
pub const IDX_CONNECTIONS_BY_PROTOCOL_TABLE: TableDefinition<&str, &str> =
    TableDefinition::new("idx_connections_by_protocol");

pub(super) const LEGACY_JSON_SETTINGS: &str = "settings";
pub(super) const LEGACY_JSON_SESSIONS: &str = "sessions";
pub(super) const LEGACY_JSON_KEYS: &str = "keys";
pub(super) const LEGACY_JSON_PASSWORDS: &str = "passwords";
pub(super) const LEGACY_JSON_CREDENTIALS: &str = "credentials";
pub(super) const LEGACY_JSON_OTP: &str = "otp";
pub(super) const LEGACY_JSON_PROXIES: &str = "proxies";
pub(super) const LEGACY_JSON_TUNNELS: &str = "tunnels";
pub(super) const LEGACY_JSON_QUICK_COMMAND: &str = "quick-command";
pub(super) const LEGACY_JSON_CLOUD_SYNC: &str = "cloud-sync";
pub(super) const LEGACY_JSON_CLOUD_SYNC_STATE: &str = "cloud-sync-state";
pub(super) const LEGACY_JSON_HISTORY: &str = "history";
pub(super) const LEGACY_JSON_AI_HISTORY: &str = "ai-history";
pub(super) const LEGACY_JSON_AI_AUDIT: &str = "ai-audit";
pub(super) const LEGACY_TEXT_KNOWN_HOSTS: &str = "known_hosts";
pub(super) const LEGACY_TEXT_MASTER_KEY: &str = "master.key";

pub(super) type HmacSha1 = Hmac<Sha1>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ConnectionPasswordRecord {
    pub(super) id: String,
    pub(super) connection_id: String,
    pub(super) password: String,
    pub(super) created_at_ms: u64,
    pub(super) updated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct KnownHostRecord {
    #[serde(default)]
    pub(super) marker: Option<String>,
    pub(super) host_identifier: String,
    #[serde(default)]
    pub(super) host_patterns: Vec<String>,
    pub(super) key_type: String,
    pub(super) key_base64: String,
    #[serde(default)]
    pub(super) comment: Option<String>,
    #[serde(default)]
    pub(super) raw_line: Option<String>,
    pub(super) created_at_ms: u64,
    pub(super) updated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct KnownHostRawRecord {
    pub(super) line: String,
    pub(super) created_at_ms: u64,
    pub(super) updated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct RdpKnownHostRecord {
    pub(super) host: String,
    pub(super) port: u16,
    pub(super) sha256_fingerprint: String,
    #[serde(default)]
    pub(super) subject: Option<String>,
    #[serde(default)]
    pub(super) issuer: Option<String>,
    #[serde(default)]
    pub(super) valid_from: Option<String>,
    #[serde(default)]
    pub(super) valid_to: Option<String>,
    pub(super) created_at_ms: u64,
    pub(super) updated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct VncKnownHostRecord {
    pub(super) host: String,
    pub(super) port: u16,
    pub(super) sha256_fingerprint: String,
    pub(super) created_at_ms: u64,
    pub(super) updated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct HistoryStoreFileV2 {
    pub(super) version: u32,
    pub(super) entries: Vec<crate::core::history::HistoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(super) struct ProxiesConfig {
    #[serde(default)]
    pub(super) proxies: Vec<crate::config::ProxyConfig>,
}
