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
    )
}
pub async fn open(session: &WebSession) -> Result<russh_sftp::client::SftpSession> {
    if !session.sftp.enabled {
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
                sftp.create_dir(path(args, "path")?).await?;
                Value::Null
            }
            "create_remote_file" => {
                sftp.create(path(args, "path")?)
                    .await?
                    .shutdown()
                    .await
                    .map_err(|_| WebError::bad("Remote write failed"))?;
                Value::Null
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
    let _ = tokio::time::timeout(Duration::from_secs(2), sftp.close()).await;
    result
}
#[derive(Deserialize)]
pub struct TransferPath {
    path: String,
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
        let temporary = self.temporary.take();
        let state = self.state.clone();
        let owner = self.owner.clone();
        let id = self.id.clone();
        let sid = self.session_id.clone();
        let done = self.done;
        tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_secs(5), async {
                if let Some(path) = temporary {
                    let _ = sftp.remove_file(path).await;
                }
                let _ = sftp.close().await;
            })
            .await;
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
        temporary: None,
        done: false,
        last_progress: std::time::Instant::now(),
    };
    let cancel = session.cancel.clone();
    use futures_util::StreamExt;
    let stream = async_stream::try_stream! {
        let _permit=permit;let mut chunks=ReaderStream::with_capacity(file,64*1024);transfer.progress("started").await;
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
    let transfer_id = uuid::Uuid::new_v4().to_string();
    let temporary = format!("{}.nyaterm-{}.part", query.path, transfer_id);
    let mut transfer = Transfer {
        state,
        owner: owner.0,
        session_id: id,
        id: transfer_id,
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
        temporary: Some(temporary.clone()),
        done: false,
        last_progress: std::time::Instant::now(),
    };
    let mut file = sftp.create(&temporary).await?;
    let mut stream = request.into_body().into_data_stream();
    let copy = async {
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
        sftp.rename(&temporary, &query.path).await?;
        Ok::<_, WebError>(())
    };
    let result = tokio::select! {_ = session.cancel.cancelled()=>Err(WebError::bad("Session closed")),result=tokio::time::timeout(Duration::from_secs(3600),copy)=>result.unwrap_or(Err(WebError::bad("Upload timed out")))};
    if let Err(error) = result {
        transfer.finish("error").await;
        return Err(error);
    }
    transfer.temporary = None;
    transfer.total = transfer.bytes;
    transfer.finish("completed").await;
    Ok(Json(json!({"bytes":transfer.bytes})))
}
