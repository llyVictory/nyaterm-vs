use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_RPC_LINE_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_INLINE_OUTPUT_BYTES: usize = 64 * 1024;
pub const MAX_TEXT_READ_BYTES: u64 = 64 * 1024;
pub const MAX_TEXT_WRITE_BYTES: usize = 1024 * 1024;

pub mod capability {
    pub const ENVIRONMENT: &str = "session.environment";
    pub const SESSION_GET: &str = "session.get";
    pub const TERMINAL_EXECUTE: &str = "terminal.execute";
    pub const TERMINAL_RECENT_OUTPUT: &str = "terminal.recent_output";
    pub const SFTP_HOME: &str = "sftp.home";
    pub const SFTP_LIST: &str = "sftp.list";
    pub const SFTP_STAT: &str = "sftp.stat";
    pub const SFTP_READ: &str = "sftp.read";
    pub const SFTP_WRITE: &str = "sftp.write";
    pub const SFTP_MKDIR: &str = "sftp.mkdir";
    pub const SFTP_RENAME: &str = "sftp.rename";
    pub const SFTP_DELETE: &str = "sftp.delete";
    pub const SFTP_CHMOD: &str = "sftp.chmod";
    pub const OUTPUT_READ: &str = "tool.output.read";
}

pub mod tool {
    pub const GET_ENVIRONMENT: &str = "get_environment";
    pub const SESSION_GET: &str = "session_get";
    pub const TERMINAL_EXECUTE: &str = "terminal_execute";
    pub const TERMINAL_RECENT_OUTPUT: &str = "terminal_recent_output";
    pub const SFTP_HOME: &str = "sftp_home";
    pub const SFTP_LIST: &str = "sftp_list";
    pub const SFTP_STAT: &str = "sftp_stat";
    pub const SFTP_READ_TEXT: &str = "sftp_read_text";
    pub const SFTP_WRITE_TEXT: &str = "sftp_write_text";
    pub const SFTP_MKDIR: &str = "sftp_mkdir";
    pub const SFTP_RENAME: &str = "sftp_rename";
    pub const SFTP_DELETE: &str = "sftp_delete";
    pub const SFTP_CHMOD: &str = "sftp_chmod";
    pub const OUTPUT_READ: &str = "tool_output_read";
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryDocument {
    pub version: u32,
    pub pid: u32,
    pub host: String,
    pub port: u16,
    pub token: String,
    pub generation: String,
    pub permission_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcRequest {
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcResponse {
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthParams {
    pub token: String,
    pub generation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientIdentifyParams {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityExecuteParams {
    #[serde(default)]
    pub request_id: Option<String>,
    pub tool: String,
    #[serde(default)]
    pub arguments: Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmptyArgs {}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionArgs {
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PathArgs {
    pub session_id: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalExecuteArgs {
    #[serde(default)]
    pub session_id: Option<String>,
    pub command: String,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalRecentOutputArgs {
    pub session_id: String,
    #[serde(default)]
    pub lines: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SftpReadTextArgs {
    pub session_id: String,
    pub path: String,
    #[serde(default)]
    pub max_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SftpWriteTextArgs {
    pub session_id: String,
    pub path: String,
    pub content: String,
    #[serde(default)]
    pub expected_mtime: Option<u64>,
    #[serde(default)]
    pub expected_size: Option<u64>,
    #[serde(default)]
    pub expected_hash: Option<String>,
    #[serde(default)]
    pub force: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SftpMkdirArgs {
    pub session_id: String,
    pub path: String,
    #[serde(default)]
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SftpRenameArgs {
    pub session_id: String,
    pub old_path: String,
    pub new_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SftpChmodArgs {
    pub session_id: String,
    pub path: String,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutputReadArgs {
    pub output_id: String,
    pub offset: usize,
    #[serde(default)]
    pub max_bytes: Option<usize>,
}
