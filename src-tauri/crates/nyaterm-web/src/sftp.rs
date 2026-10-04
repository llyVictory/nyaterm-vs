use crate::{
    auth::Owner,
    error::{Result, WebError},
    session::WebSession,
    state::State,
};
use axum::{
    Extension, Json,
    body::Body,
    extract::{Path, Query, State as ExtractState},
    http::header,
    response::Response,
};
use nyaterm_core::ssh::{files, protocol};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::io::ReaderStream;

pub fn supports(command: &str) -> bool {
    matches!(
        command,
        "get_home_dir"
            | "list_remote_dir"
            | "list_remote_child_directories"
            | "delete_remote_file"
            | "rename_remote_file"
            | "create_remote_dir"
            | "create_remote_file"
            | "read_remote_file_text"
            | "open_remote_file_text"
            | "write_remote_file_text"
            | "get_file_properties"
            | "get_remote_file_stat"
            | "read_remote_file_bytes"
            | "create_remote_symlink"
            | "update_remote_symlink_target"
            | "update_remote_file_attributes"
            | "chmod_remote_file"
            | "find_missing_remote_entries"
            | "copy_file_entry"
            | "move_file_entry"
    )
}
pub async fn open(session: &WebSession) -> Result<russh_sftp::client::SftpSession> {
    if !matches!(session.protocol, crate::session::SessionProtocol::Ssh) || !session.sftp.enabled {
        return Err(WebError::unsupported());
    }
    if !session.ready.load(std::sync::atomic::Ordering::Acquire) {
        return Err(WebError::bad("SSH is still connecting"));
    }
    let operation = async {
        let mut handle = session.handle.lock().await;
        let handle = handle
            .as_mut()
            .ok_or(WebError::bad("SSH is disconnected"))?;
        let config = russh_sftp::client::Config {
            max_concurrent_writes: session.sftp.pipeline_depth.unwrap_or(8) as usize,
            ..Default::default()
        };
        Ok(protocol::open_sftp_with_config(handle, config).await?)
    };
    tokio::select! {
        _ = session.cancel.cancelled() => Err(WebError::bad("Session closed")),
        result = tokio::time::timeout(Duration::from_secs(30), operation) => result.map_err(|_| WebError::bad("SFTP timed out"))?,
    }
}
fn validate_path(path: &str) -> Result<&str> {
    if path.is_empty() || path.contains('\0') || path.len() > 4096 {
        Err(WebError::bad("Invalid remote path"))
    } else {
        Ok(path)
    }
}
fn path<'a>(args: &'a Value, name: &str) -> Result<&'a str> {
    validate_path(
        args[name]
            .as_str()
            .ok_or(WebError::bad("Remote path required"))?,
    )
}
fn mode(value: Option<&str>) -> Result<Option<u32>> {
    value
        .filter(|v| !v.trim().is_empty())
        .map(|v| {
            let value = v.trim();
            let parsed = u32::from_str_radix(value, 8)
                .map_err(|_| WebError::bad("Invalid octal permissions"))?;
            if parsed > 0o7777 {
                return Err(WebError::bad("Invalid octal permissions"));
            }
            Ok(parsed)
        })
        .transpose()
}
async fn apply_mode(
    sftp: &russh_sftp::client::SftpSession,
    path: &str,
    permissions: Option<u32>,
) -> Result<()> {
    if let Some(permissions) = permissions {
        sftp.set_metadata(
            path,
            russh_sftp::protocol::FileAttributes {
                permissions: Some(permissions),
                ..russh_sftp::protocol::FileAttributes::empty()
            },
        )
        .await?;
    }
    Ok(())
}
fn entry(name: String, attrs: &russh_sftp::protocol::FileAttributes) -> files::FileEntry {
    files::FileEntry {
        name,
        is_dir: attrs.is_dir(),
        is_symlink: attrs.is_symlink(),
        size: attrs.size.unwrap_or(0),
        permissions: files::describe_permissions(attrs.permissions),
        owner: files::owner_or_id(&attrs.user, attrs.uid),
        group: files::group_or_id(&attrs.group, attrs.gid),
        mtime: attrs.mtime.unwrap_or(0) as u64,
        raw_path_token: None,
    }
}
pub async fn command(
    state: &Arc<State>,
    owner: &str,
    command: &str,
    args: &Value,
) -> Result<Value> {
    // Raw filename encodings and recursive/native workflows have no Web MVP contract.
    if matches!(command, "copy_file_entry" | "move_file_entry") {
        return copy_entry(state, owner, command, args).await;
    }
    if !args["rawPathToken"].is_null() {
        return Err(WebError::unsupported());
    }
    let session = state.session(owner, path(args, "sessionId")?).await?;
    let _permit = session
        .transfers
        .clone()
        .try_acquire_owned()
        .map_err(|_| WebError::bad("Too many SFTP operations"))?;
    let sftp = open(&session).await?;
    let mut temporary_link = None;
    let operation = async {
        Ok(match command {
            "get_home_dir" => json!(sftp.canonicalize(".").await?),
            "list_remote_dir" | "list_remote_child_directories" => {
                let parent = path(args, "path")?;
                let entries = sftp.read_dir(parent).await?.collect::<Vec<_>>();
                if entries
                    .iter()
                    .any(|e| std::str::from_utf8(e.file_name_bytes()).is_err())
                {
                    return Err(WebError::bad("Web SFTP requires UTF-8 filenames"));
                }
                let entries = entries.into_iter();
                let entries = entries.filter(|e| e.file_name() != "." && e.file_name() != "..");
                if command == "list_remote_child_directories" {
                    json!(
                        entries
                            .filter(|e| e.metadata().is_dir()
                                && (args["showHiddenFiles"].as_bool() == Some(true)
                                    || !e.file_name().starts_with('.')))
                            .map(|e| files::DirectoryChild {
                                name: e.file_name(),
                                path: format!("{}/{}", parent.trim_end_matches('/'), e.file_name()),
                                is_symlink: e.metadata().is_symlink(),
                                raw_path_token: None
                            })
                            .collect::<Vec<_>>()
                    )
                } else {
                    json!(
                        entries
                            .map(|e| entry(e.file_name(), &e.metadata()))
                            .collect::<Vec<_>>()
                    )
                }
            }
            "delete_remote_file" => {
                let p = path(args, "path")?;
                if sftp.symlink_metadata(p).await?.is_dir() {
                    sftp.remove_dir(p).await?;
                } else {
                    sftp.remove_file(p).await?;
                }
                Value::Null
            }
            "rename_remote_file" => {
                sftp.rename(path(args, "oldPath")?, path(args, "newPath")?)
                    .await?;
                Value::Null
            }
            "create_remote_dir" => {
                let input_mode: Option<String> = crate::commands::argument(args, "mode")?;
                let permissions = mode(input_mode.as_deref())?;
                let p = path(args, "path")?;
                sftp.create_dir(p).await?;
                apply_mode(&sftp, p, permissions).await?;
                Value::Null
            }
            "create_remote_file" => {
                let input_mode: Option<String> = crate::commands::argument(args, "mode")?;
                let permissions = mode(input_mode.as_deref())?;
                sftp.create(path(args, "path")?)
                    .await?
                    .shutdown()
                    .await
                    .map_err(|_| WebError::bad("Remote write failed"))?;
                apply_mode(&sftp, path(args, "path")?, permissions).await?;
                Value::Null
            }
            "create_remote_symlink" => {
                sftp.symlink_openssh(path(args, "targetPath")?, path(args, "linkPath")?)
                    .await?;
                Value::Null
            }
            "update_remote_symlink_target" => {
                let p = path(args, "path")?;
                let target = path(args, "targetPath")?;
                if !sftp.symlink_metadata(p).await?.is_symlink() {
                    return Err(WebError::bad("Remote path is not a symbolic link"));
                }
                // Create the replacement before touching the original link.
                let temporary = format!("{p}.nyaterm-{}.link", uuid::Uuid::new_v4());
                temporary_link = Some(temporary.clone());
                sftp.symlink_openssh(target, &temporary).await?;
                sftp.rename_replace(&temporary, p).await?;
                temporary_link = None;
                Value::Null
            }
            "chmod_remote_file" | "update_remote_file_attributes" => {
                let p = path(args, "path")?;
                let update: files::RemoteFileAttributeUpdate = if command == "chmod_remote_file" {
                    files::RemoteFileAttributeUpdate {
                        mode: Some(crate::commands::text(args, "mode")?.into()),
                        ..Default::default()
                    }
                } else {
                    crate::commands::argument(args, "update")?
                };
                if update.recursive {
                    return Err(WebError::bad(
                        "Recursive attribute changes require a transfer task",
                    ));
                }
                let current = sftp.metadata(p).await?;
                let parse_id = |value: Option<&str>| -> Result<Option<u32>> {
                    value
                        .filter(|v| !v.trim().is_empty())
                        .map(|v| {
                            v.trim().parse().map_err(|_| {
                                WebError::bad("Web SFTP requires a numeric UID or GID")
                            })
                        })
                        .transpose()
                };
                let uid = parse_id(update.owner.as_deref())?;
                let gid = parse_id(update.group.as_deref())?;
                let permissions = mode(update.mode.as_deref())?;
                if uid.is_some() || gid.is_some() || permissions.is_some() {
                    // SFTP v3 serializes UID and GID together: preserve the
                    // unspecified half instead of silently changing it to root.
                    let ownership = uid.is_some() || gid.is_some();
                    let attrs = russh_sftp::protocol::FileAttributes {
                        permissions,
                        uid: if ownership {
                            Some(
                                uid.or(current.uid)
                                    .ok_or(WebError::bad("Remote UID unavailable"))?,
                            )
                        } else {
                            None
                        },
                        gid: if ownership {
                            Some(
                                gid.or(current.gid)
                                    .ok_or(WebError::bad("Remote GID unavailable"))?,
                            )
                        } else {
                            None
                        },
                        ..russh_sftp::protocol::FileAttributes::empty()
                    };
                    sftp.set_metadata(p, attrs).await?;
                }
                Value::Null
            }
            "find_missing_remote_entries" => {
                let paths: Vec<String> = crate::commands::argument(args, "paths")?;
                let mut missing = Vec::new();
                for p in paths {
                    validate_path(&p)?;
                    match sftp.symlink_metadata(&p).await {
                        Ok(_) => {}
                        Err(russh_sftp::client::error::Error::Status(status))
                            if status.status_code
                                == russh_sftp::protocol::StatusCode::NoSuchFile =>
                        {
                            missing.push(p)
                        }
                        Err(error) => return Err(error.into()),
                    }
                }
                json!(missing)
            }
            "read_remote_file_bytes" => {
                let p = path(args, "path")?;
                let limit = args["maxBytes"]
                    .as_u64()
                    .filter(|v| *v > 0 && *v <= 25 * 1024 * 1024)
                    .ok_or(WebError::bad(
                        "Preview limit must be between 1 byte and 25 MiB",
                    ))?;
                let attrs = sftp.metadata(p).await?;
                if attrs.is_dir() || attrs.size.is_some_and(|size| size > limit) {
                    return Err(WebError::bad("Remote file exceeds preview limit"));
                }
                let mut bytes = Vec::new();
                sftp.open(p)
                    .await?
                    .take(limit + 1)
                    .read_to_end(&mut bytes)
                    .await
                    .map_err(|_| WebError::bad("Remote read failed"))?;
                if bytes.len() as u64 > limit {
                    return Err(WebError::bad("Remote file exceeds preview limit"));
                }
                json!(files::RemoteBinaryFile {
                    path: p.into(),
                    size: bytes.len() as u64,
                    mtime: attrs.mtime.unwrap_or(0) as u64,
                    mtime_nanos: None,
                    content_bytes: bytes
                })
            }
            "read_remote_file_text" | "open_remote_file_text" => {
                let p = path(args, "path")?;
                let attrs = sftp.metadata(p).await?;
                let mut file = sftp.open(p).await?;
                let limit = args["maxBytes"]
                    .as_u64()
                    .unwrap_or(1024 * 1024)
                    .min(4 * 1024 * 1024);
                let mut bytes = Vec::new();
                (&mut file)
                    .take(limit + 1)
                    .read_to_end(&mut bytes)
                    .await
                    .map_err(|_| WebError::bad("Remote read failed"))?;
                if bytes.len() as u64 > limit {
                    return Err(WebError::bad("Remote file is too large"));
                }
                let result = files::classify_text_file(files::RemoteBinaryFile {
                    path: p.into(),
                    size: bytes.len() as u64,
                    mtime: attrs.mtime.unwrap_or(0) as u64,
                    mtime_nanos: None,
                    content_bytes: bytes,
                });
                if command == "open_remote_file_text" {
                    json!(result)
                } else {
                    match result {
                        files::TextFileOpenResult::Text { file } => json!(file),
                        _ => return Err(WebError::bad("Remote file is not UTF-8 text")),
                    }
                }
            }
            "write_remote_file_text" => {
                let p = path(args, "path")?;
                let content = args["content"]
                    .as_str()
                    .ok_or(WebError::bad("Content required"))?;
                let attrs = sftp.metadata(p).await?;
                if args["force"].as_bool() != Some(true) {
                    let changed = args["expectedMtime"]
                        .as_u64()
                        .is_some_and(|t| t != attrs.mtime.unwrap_or(0) as u64)
                        || args["expectedSize"]
                            .as_u64()
                            .is_some_and(|s| s != attrs.size.unwrap_or(0));
                    let mut hash_changed = false;
                    if let Some(expected) = args["expectedHash"].as_str() {
                        let mut old = Vec::new();
                        sftp.open(p)
                            .await?
                            .take(4 * 1024 * 1024 + 1)
                            .read_to_end(&mut old)
                            .await
                            .map_err(|_| WebError::bad("Remote read failed"))?;
                        hash_changed = files::content_hash(&old) != expected;
                    }
                    if changed || hash_changed {
                        return Ok(json!(files::WriteRemoteTextResult::conflict(
                            attrs.mtime.unwrap_or(0) as u64,
                            attrs.size.unwrap_or(0),
                            None
                        )));
                    }
                }
                let mut file = sftp.create(p).await?;
                file.write_all(content.as_bytes())
                    .await
                    .map_err(|_| WebError::bad("Remote write failed"))?;
                file.shutdown()
                    .await
                    .map_err(|_| WebError::bad("Remote write failed"))?;
                let attrs = sftp.metadata(p).await?;
                json!(files::WriteRemoteTextResult::saved(
                    attrs.mtime.unwrap_or(0) as u64,
                    content.len() as u64,
                    None,
                    files::content_hash(content.as_bytes())
                ))
            }
            "get_file_properties" | "get_remote_file_stat" => {
                let p = path(args, "path")?;
                let attrs = sftp.symlink_metadata(p).await?;
                let base = entry(p.rsplit('/').next().unwrap_or(p).into(), &attrs);
                json!(files::FileProperties {
                    name: base.name,
                    is_dir: base.is_dir,
                    is_symlink: base.is_symlink,
                    symlink_target: if attrs.is_symlink() {
                        sftp.read_link(p).await.ok()
                    } else {
                        None
                    },
                    size: base.size,
                    permissions: base.permissions,
                    owner: base.owner,
                    group: base.group,
                    uid: attrs.uid.map(|v| v.to_string()).unwrap_or_default(),
                    gid: attrs.gid.map(|v| v.to_string()).unwrap_or_default(),
                    mtime: base.mtime,
                    atime: attrs.atime.unwrap_or(0) as u64
                })
            }
            _ => return Err(WebError::unsupported()),
        })
    };
    let result = tokio::select! {_ = session.cancel.cancelled()=>Err(WebError::bad("Session closed")),result=tokio::time::timeout(Duration::from_secs(30),operation)=>result.map_err(|_|WebError::bad("SFTP timed out"))?};
    if let Some(path) = temporary_link {
        let _ = tokio::time::timeout(Duration::from_secs(2), sftp.remove_file(path)).await;
    }
    let _ = tokio::time::timeout(Duration::from_secs(2), sftp.close()).await;
    result
}
#[derive(Deserialize)]
pub struct TransferPath {
    path: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CopyEndpoint {
    session_id: String,
    kind: String,
    path: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CopyRequest {
    source: CopyEndpoint,
    target: CopyEndpoint,
    file_name: String,
    is_directory: bool,
    transfer_id: Option<String>,
    duplicate_strategy_override: Option<String>,
}
async fn copy_entry(state: &Arc<State>, owner: &str, command: &str, args: &Value) -> Result<Value> {
    let request: CopyRequest = crate::commands::argument(args, "request")?;
    if request.source.kind != "remote" || request.target.kind != "remote" {
        return Err(WebError::unsupported());
    }
    validate_path(&request.source.path)?;
    validate_path(&request.target.path)?;
    let source = state.session(owner, &request.source.session_id).await?;
    let target = state.session(owner, &request.target.session_id).await?;
    let same = source.id == target.id;
    if request.is_directory && !(same && command == "move_file_entry") {
        return Err(WebError::bad("Recursive copies require a transfer task"));
    }
    if request.file_name.is_empty()
        || matches!(request.file_name.as_str(), "." | "..")
        || request.file_name.contains(['/', '\\', '\0'])
    {
        return Err(WebError::bad("Invalid file name"));
    }
    let target_path = format!(
        "{}/{}",
        request.target.path.trim_end_matches('/'),
        request.file_name
    );
    validate_path(&target_path)?;
    if same && request.source.path == target_path {
        return Err(WebError::bad("Source and destination are identical"));
    }
    let _source_permit = source
        .transfers
        .clone()
        .try_acquire_owned()
        .map_err(|_| WebError::bad("Too many transfers"))?;
    let _target_permit = if same {
        None
    } else {
        Some(
            target
                .transfers
                .clone()
                .try_acquire_owned()
                .map_err(|_| WebError::bad("Too many transfers"))?,
        )
    };
    let source_sftp = Arc::new(open(&source).await?);
    let target_sftp = if same {
        source_sftp.clone()
    } else {
        Arc::new(open(&target).await?)
    };
    let attrs = source_sftp.symlink_metadata(&request.source.path).await?;
    let mut destination = target_path;
    let mut overwrite = false;
    if target_sftp.try_exists(&destination).await? {
        let strategy = request.duplicate_strategy_override.clone().unwrap_or(
            nyaterm_core::config::load_app_settings(&())?
                .transfer
                .duplicate_strategy,
        );
        let strategy = if strategy == "ask" {
            state.prompt(owner,"transfer-duplicate-request",json!({"sessionId":target.id,"remotePath":destination,"fileName":request.file_name,"isDirectory":attrs.is_dir()}),&target.cancel).await?.as_str().unwrap_or("skip").to_owned()
        } else {
            strategy
        };
        match strategy.as_str() {
            "skip" => return Ok(json!("skipped")),
            "overwrite" if !attrs.is_dir() => {
                overwrite = true;
            }
            "rename" => {
                destination = format!("{destination}.{}", uuid::Uuid::new_v4());
            }
            _ => {
                return Err(WebError::bad(
                    "Destination exists; choose skip, rename or overwrite",
                ));
            }
        }
    }
    let id = request
        .transfer_id
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let temporary = format!("{destination}.nyaterm-{}.part", uuid::Uuid::new_v4());
    let mut transfer = Transfer {
        state: state.clone(),
        owner: owner.into(),
        session_id: target.id.clone(),
        id,
        path: destination.clone(),
        direction: "copy",
        total: attrs.size.unwrap_or(0),
        bytes: 0,
        sftp: target_sftp.clone(),
        permits: std::iter::once(_source_permit)
            .chain(_target_permit)
            .collect(),
        temporary: None,
        done: false,
        last_progress: std::time::Instant::now(),
    };
    let operation = async {
        transfer.progress("started").await;
        if same && command == "move_file_entry" {
            if overwrite {
                source_sftp
                    .rename_replace(&request.source.path, &destination)
                    .await?;
            } else {
                source_sftp
                    .rename(&request.source.path, &destination)
                    .await?;
            }
            transfer.bytes = transfer.total;
        } else {
            if attrs.is_dir() {
                return Err(WebError::bad("Recursive copies require a transfer task"));
            }
            transfer.temporary = Some(temporary.clone());
            if attrs.is_symlink() {
                let link = source_sftp.read_link(&request.source.path).await?;
                target_sftp.symlink_openssh(link, &temporary).await?;
            } else {
                if transfer.total > 1024 * 1024 * 1024 {
                    return Err(WebError::bad("Copy exceeds 1 GiB"));
                }
                let mut reader = source_sftp.open(&request.source.path).await?;
                let mut writer = target_sftp.create(&temporary).await?;
                let mut buffer = vec![0u8; 64 * 1024];
                loop {
                    let size = reader
                        .read(&mut buffer)
                        .await
                        .map_err(|_| WebError::bad("Remote read failed"))?;
                    if size == 0 {
                        break;
                    }
                    transfer.bytes += size as u64;
                    if transfer.bytes > 1024 * 1024 * 1024 {
                        return Err(WebError::bad("Copy exceeds 1 GiB"));
                    }
                    writer
                        .write_all(&buffer[..size])
                        .await
                        .map_err(|_| WebError::bad("Remote write failed"))?;
                    transfer.progress("progress").await;
                }
                writer
                    .shutdown()
                    .await
                    .map_err(|_| WebError::bad("Remote write failed"))?;
                if transfer.bytes != transfer.total {
                    return Err(WebError::bad("Source changed during copy"));
                }
                apply_mode(
                    &target_sftp,
                    &temporary,
                    attrs.permissions.map(|v| v & 0o7777),
                )
                .await?;
            }
            if overwrite {
                target_sftp.rename_replace(&temporary, &destination).await?;
            } else {
                target_sftp.rename(&temporary, &destination).await?;
            }
            transfer.temporary = None;
            if command == "move_file_entry" {
                source_sftp.remove_file(&request.source.path).await?;
            }
        }
        Ok::<_, WebError>(())
    };
    let result = tokio::select! {_=source.cancel.cancelled()=>Err(WebError::bad("Source session closed")),_=target.cancel.cancelled()=>Err(WebError::bad("Target session closed")),result=tokio::time::timeout(Duration::from_secs(3600),operation)=>result.unwrap_or(Err(WebError::bad("Copy timed out")))};
    if let Err(error) = result {
        transfer.finish("error").await;
        return Err(error);
    }
    transfer.finish("completed").await;
    Ok(json!("copied"))
}
struct Transfer {
    state: Arc<State>,
    owner: String,
    session_id: String,
    id: String,
    path: String,
    direction: &'static str,
    total: u64,
    bytes: u64,
    sftp: Arc<russh_sftp::client::SftpSession>,
    permits: Vec<tokio::sync::OwnedSemaphorePermit>,
    temporary: Option<String>,
    done: bool,
    last_progress: std::time::Instant,
}
impl Transfer {
    async fn progress(&mut self, status: &str) {
        if status == "progress" && self.last_progress.elapsed() < Duration::from_millis(200) {
            return;
        }
        self.last_progress = std::time::Instant::now();
        self.state.event(&self.owner,"transfer-event",json!({"id":self.id,"session_id":self.session_id,"file_name":self.path.rsplit('/').next().unwrap_or("file"),"remote_path":self.path,"local_path":"","direction":self.direction,"kind":"file","bytes_transferred":self.bytes,"total_size":self.total,"size":self.total,"status":status})).await;
    }
    async fn finish(&mut self, status: &str) {
        self.progress(status).await;
        self.done = true;
    }
}
impl Drop for Transfer {
    fn drop(&mut self) {
        let sftp = self.sftp.clone();
        let permits = std::mem::take(&mut self.permits);
        let temporary = self.temporary.take();
        let state = self.state.clone();
        let owner = self.owner.clone();
        let id = self.id.clone();
        let sid = self.session_id.clone();
        let done = self.done;
        tokio::spawn(async move {
            // Session shutdown waits for permits, including asynchronous cleanup.
            let _permits = permits;
            if let Some(path) = temporary {
                let _ = tokio::time::timeout(Duration::from_secs(4), sftp.remove_file(path)).await;
            }
            // A stalled removal must still allow the SFTP close attempt.
            let _ = tokio::time::timeout(Duration::from_secs(1), sftp.close()).await;
            if !done {
                state
                    .event(
                        &owner,
                        "transfer-event",
                        json!({"id":id,"session_id":sid,"status":"cancelled"}),
                    )
                    .await;
            }
        });
    }
}
pub async fn download(
    ExtractState(state): ExtractState<Arc<State>>,
    Extension(owner): Extension<Owner>,
    Path(id): Path<String>,
    Query(query): Query<TransferPath>,
) -> Result<Response> {
    validate_path(&query.path)?;
    let session = state.session(&owner.0, &id).await?;
    let permit = session
        .transfers
        .clone()
        .try_acquire_owned()
        .map_err(|_| WebError::bad("Too many transfers"))?;
    let sftp = Arc::new(open(&session).await?);
    let total = sftp.metadata(&query.path).await?.size.unwrap_or(0);
    let file = sftp.open(&query.path).await?;
    let mut transfer = Transfer {
        state,
        owner: owner.0,
        session_id: id,
        id: uuid::Uuid::new_v4().to_string(),
        path: query.path.clone(),
        direction: "download",
        total,
        bytes: 0,
        sftp,
        permits: vec![permit],
        temporary: None,
        done: false,
        last_progress: std::time::Instant::now(),
    };
    let cancel = session.cancel.clone();
    use futures_util::StreamExt;
    let stream = async_stream::try_stream! {
        let mut chunks=ReaderStream::with_capacity(file,64*1024);transfer.progress("started").await;
        loop {
            let next=tokio::select!{_ = cancel.cancelled()=>Some(Err(std::io::Error::other("Session closed"))),result=tokio::time::timeout(Duration::from_secs(30),chunks.next())=>result.unwrap_or(Some(Err(std::io::Error::other("SFTP download timed out"))))};
            match next { Some(Ok(bytes))=>{transfer.bytes+=bytes.len() as u64;transfer.progress("progress").await;yield bytes;},Some(Err(error))=>{transfer.finish("error").await;Err(error)?;},None=>break }
        }
        if transfer.bytes!=transfer.total {transfer.finish("error").await;Err(std::io::Error::other("Remote file changed during download"))?;}
        transfer.finish("completed").await;
    };
    let safe =
        files::sanitize_download_file_name(query.path.rsplit('/').next().unwrap_or("download"))
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || ".-_%".contains(c) {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>();
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_LENGTH, total)
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{safe}\""),
        )
        .body(Body::from_stream(
            stream.map(|r: std::io::Result<axum::body::Bytes>| r),
        ))
        .unwrap())
}
pub async fn upload(
    ExtractState(state): ExtractState<Arc<State>>,
    Extension(owner): Extension<Owner>,
    Path(id): Path<String>,
    Query(query): Query<TransferPath>,
    request: axum::extract::Request,
) -> Result<Json<Value>> {
    use futures_util::StreamExt;
    validate_path(&query.path)?;
    let session = state.session(&owner.0, &id).await?;
    let _permit = session
        .transfers
        .clone()
        .try_acquire_owned()
        .map_err(|_| WebError::bad("Too many transfers"))?;
    let sftp = Arc::new(open(&session).await?);
    // Own cleanup before metadata lookup or a potentially long conflict prompt.
    let mut transfer = Transfer {
        state,
        owner: owner.0,
        session_id: id,
        id: uuid::Uuid::new_v4().to_string(),
        path: query.path.clone(),
        direction: "upload",
        total: request
            .headers()
            .get(header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
        bytes: 0,
        sftp: sftp.clone(),
        permits: vec![_permit],
        temporary: None,
        done: false,
        last_progress: std::time::Instant::now(),
    };
    let operation = async {
        let mut overwrite = false;
        match sftp.symlink_metadata(&transfer.path).await {
            Ok(existing) => {
                let strategy = nyaterm_core::config::load_app_settings(&())?
                    .transfer
                    .duplicate_strategy;
                let strategy = if strategy == "ask" {
                    transfer
                        .state
                        .prompt(
                            &transfer.owner,
                            "transfer-duplicate-request",
                            json!({"sessionId":session.id,"remotePath":transfer.path,
                            "fileName":transfer.path.rsplit('/').next().unwrap_or("file"),
                            "isDirectory":existing.is_dir()}),
                            &session.cancel,
                        )
                        .await?
                        .as_str()
                        .unwrap_or("skip")
                        .to_owned()
                } else {
                    strategy
                };
                match strategy.as_str() {
                    "skip" => {
                        transfer.finish("cancelled").await;
                        return Ok(json!({"bytes":0,"status":"skipped","path":transfer.path}));
                    }
                    "rename" => {
                        transfer.path = format!("{}.{}", transfer.path, uuid::Uuid::new_v4());
                        validate_path(&transfer.path)?;
                    }
                    "overwrite" if !existing.is_dir() => overwrite = true,
                    "overwrite" => {
                        return Err(WebError::bad("Cannot overwrite a directory with a file"));
                    }
                    _ => return Err(WebError::bad("Invalid duplicate strategy")),
                }
            }
            Err(russh_sftp::client::error::Error::Status(status))
                if status.status_code == russh_sftp::protocol::StatusCode::NoSuchFile => {}
            Err(error) => return Err(error.into()),
        }
        let temporary = format!("{}.nyaterm-{}.part", transfer.path, transfer.id);
        validate_path(&temporary)?;
        transfer.temporary = Some(temporary.clone());
        let mut file = sftp.create(&temporary).await?;
        let mut stream = request.into_body().into_data_stream();
        transfer.progress("started").await;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| WebError::bad("Upload interrupted"))?;
            transfer.bytes += chunk.len() as u64;
            if transfer.bytes > 1024 * 1024 * 1024 {
                return Err(WebError::bad("Upload exceeds 1 GiB"));
            }
            file.write_all(&chunk)
                .await
                .map_err(|_| WebError::bad("SFTP write failed"))?;
            transfer.progress("progress").await;
        }
        file.shutdown()
            .await
            .map_err(|_| WebError::bad("SFTP write failed"))?;
        if overwrite {
            sftp.rename_replace(&temporary, &transfer.path)
                .await
                .map_err(|_| {
                    WebError::bad(
                        "Atomic upload replacement failed; no non-atomic fallback was attempted",
                    )
                })?;
        } else {
            sftp.rename(&temporary, &transfer.path).await?;
        }
        transfer.temporary = None;
        transfer.total = transfer.bytes;
        transfer.finish("completed").await;
        Ok::<_, WebError>(json!({"bytes":transfer.bytes,"status":"completed","path":transfer.path}))
    };
    let result = tokio::select! {
        _ = session.cancel.cancelled() => Err(WebError::bad("Session closed")),
        result = tokio::time::timeout(Duration::from_secs(3600), operation) => result.unwrap_or(Err(WebError::bad("Upload timed out"))),
    };
    match result {
        Ok(value) => Ok(Json(value)),
        Err(error) => {
            transfer.finish("error").await;
            Err(error)
        }
    }
}
