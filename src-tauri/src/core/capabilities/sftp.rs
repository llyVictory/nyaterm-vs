use std::sync::Arc;

use crate::core::session::{SessionInfo, SessionManager, SessionType};
use crate::core::sftp::{self, FileEntry, FileProperties, RemoteTextFile, WriteRemoteTextResult};
use crate::error::{AppError, AppResult};

pub fn is_available(info: &SessionInfo) -> bool {
    info.connected && info.session_type == SessionType::SSH && info.remote_file_browser_enabled
}

pub async fn require_available(
    manager: &SessionManager,
    session_id: &str,
) -> AppResult<SessionInfo> {
    let info = manager.session_info(session_id).await?;
    if is_available(&info) {
        Ok(info)
    } else {
        Err(AppError::Config(
            "SFTP is not available for this session.".to_string(),
        ))
    }
}

pub async fn home(manager: Arc<SessionManager>, session_id: &str) -> AppResult<String> {
    require_available(&manager, session_id).await?;
    sftp::get_home_dir(manager, session_id).await
}

pub async fn list(
    manager: Arc<SessionManager>,
    session_id: &str,
    path: &str,
) -> AppResult<Vec<FileEntry>> {
    require_available(&manager, session_id).await?;
    sftp::list_remote_dir(manager, session_id, path, None).await
}

pub async fn stat(
    manager: Arc<SessionManager>,
    session_id: &str,
    path: &str,
) -> AppResult<FileProperties> {
    require_available(&manager, session_id).await?;
    sftp::get_file_properties(manager, session_id, path, None).await
}

pub async fn read_text(
    manager: Arc<SessionManager>,
    session_id: &str,
    path: &str,
    max_bytes: u64,
) -> AppResult<RemoteTextFile> {
    require_available(&manager, session_id).await?;
    sftp::read_remote_file_text(manager, session_id, path, max_bytes).await
}

#[allow(clippy::too_many_arguments)]
pub async fn write_text(
    manager: Arc<SessionManager>,
    session_id: &str,
    path: &str,
    content: &str,
    expected_mtime: Option<u64>,
    expected_size: Option<u64>,
    expected_hash: Option<&str>,
    force: bool,
) -> AppResult<WriteRemoteTextResult> {
    require_available(&manager, session_id).await?;
    sftp::write_remote_file_text(
        manager,
        session_id,
        path,
        content,
        expected_mtime,
        expected_size,
        expected_hash,
        force,
    )
    .await
}

pub async fn mkdir(
    manager: Arc<SessionManager>,
    session_id: &str,
    path: &str,
    mode: Option<String>,
) -> AppResult<()> {
    require_available(&manager, session_id).await?;
    sftp::create_remote_dir(manager, session_id, path, mode).await
}

pub async fn rename(
    manager: Arc<SessionManager>,
    session_id: &str,
    old_path: &str,
    new_path: &str,
) -> AppResult<()> {
    require_available(&manager, session_id).await?;
    sftp::rename_remote_file(manager, session_id, old_path, new_path, None, None).await
}

pub async fn delete(manager: Arc<SessionManager>, session_id: &str, path: &str) -> AppResult<()> {
    require_available(&manager, session_id).await?;
    sftp::delete_remote_file(manager, session_id, path, None).await
}

pub async fn chmod(
    manager: Arc<SessionManager>,
    session_id: &str,
    path: &str,
    mode: &str,
) -> AppResult<()> {
    require_available(&manager, session_id).await?;
    sftp::chmod_remote_file(manager, session_id, path, mode).await
}
