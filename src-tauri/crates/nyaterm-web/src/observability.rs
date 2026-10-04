//! Bounded, structured deployment logs. Unstructured data is hashed at the sink.
use crate::{
    auth::Owner,
    error::{Result, WebError},
    state::State,
};
use axum::{
    Extension, Json,
    extract::{MatchedPath, Request, State as ExtractState},
    http::HeaderValue,
    middleware::Next,
    response::{IntoResponse, Response},
};
use nyaterm_core::{
    config::{self, DiagnosticsSettings},
    error::AppError,
};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU8, AtomicU32, AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tracing::{
    Event, Instrument, Subscriber,
    field::{Field, Visit},
};
use tracing_subscriber::{
    Layer,
    layer::{Context, SubscriberExt},
    registry::LookupSpan,
};

const ROTATE_BYTES: u64 = 10 * 1024 * 1024;
const TOTAL_BYTES: u64 = 100 * 1024 * 1024;
static LOGS: OnceLock<Arc<Logs>> = OnceLock::new();
enum Job {
    Entry(Vec<u8>),
    Flush(mpsc::Sender<()>),
    Shutdown(mpsc::Sender<()>),
}
struct Logs {
    sender: mpsc::SyncSender<Job>,
    dir: PathBuf,
    level: AtomicU8,
    retention: AtomicU32,
    dropped: AtomicU64,
    rates: Mutex<HashMap<String, (Instant, u32)>>,
}

pub fn fingerprint(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))[..16].into()
}
fn level_number(level: &tracing::Level) -> u8 {
    match *level {
        tracing::Level::ERROR => 3,
        tracing::Level::WARN => 2,
        tracing::Level::INFO => 1,
        _ => 0,
    }
}
#[derive(Default)]
struct Fields(Map<String, Value>);
impl Visit for Fields {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().into(), json!(value));
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0.insert(field.name().into(), json!(value));
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.0.insert(field.name().into(), json!(value));
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.0.insert(field.name().into(), json!(value));
    }
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.record_str(field, &format!("{value:?}"));
    }
}
struct LogLayer(Arc<Logs>);
impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for LogLayer {
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::Id,
        ctx: Context<'_, S>,
    ) {
        let mut fields = Fields::default();
        attrs.record(&mut fields);
        if let Some(span) = ctx.span(id) {
            span.extensions_mut().insert(fields);
        }
    }
    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        if level_number(event.metadata().level()) < self.0.level.load(Ordering::Relaxed) {
            return;
        }
        let mut fields = Fields::default();
        if let Some(scope) = ctx.event_scope(event) {
            for span in scope.from_root() {
                if let Some(values) = span.extensions().get::<Fields>() {
                    fields.0.extend(values.0.clone());
                }
            }
        }
        event.record(&mut fields);
        let entry = json!({
            "timestamp":time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339).unwrap_or_default(),
            "level":event.metadata().level().as_str().to_lowercase(),
            "target":event.metadata().target(),
            "fields":sanitize_fields(fields.0),
        });
        self.0.enqueue(entry);
    }
}

// An allowlist avoids leaking arbitrary error strings, command arguments or imported content.
fn sanitize_fields(fields: Map<String, Value>) -> Map<String, Value> {
    fields
        .into_iter()
        .map(|(key, value)| {
            let safe = match key.as_str() {
                "request_id" | "session_id" | "connection_id" | "transfer_id" => value
                    .as_str()
                    .filter(|s| uuid::Uuid::parse_str(s).is_ok())
                    .map(|s| json!(s)),
                "event" | "domain" | "operation" | "error_kind" | "reason" | "method" => value
                    .as_str()
                    .filter(|s| {
                        s.len() <= 96
                            && s.chars()
                                .all(|c| c.is_ascii_alphanumeric() || "._:- ".contains(c))
                    })
                    .map(|s| json!(s)),
                "error_hash" => value
                    .as_str()
                    .filter(|s| s.len() == 16 && s.chars().all(|c| c.is_ascii_hexdigit()))
                    .map(|s| json!(s)),
                "route" => value
                    .as_str()
                    .filter(|s| s.len() <= 160 && s.starts_with('/') && !s.contains('?'))
                    .map(|s| json!(s)),
                "status" | "duration_ms" | "bytes" | "count" | "error_code" | "dropped_count"
                | "limit" => {
                    if value.is_number() {
                        Some(value.clone())
                    } else {
                        None
                    }
                }
                // Frontend data was validated and sanitized independently below.
                "frontend" => value
                    .as_str()
                    .and_then(|s| serde_json::from_str::<Value>(s).ok())
                    .map(|v| sanitize_frontend(&v, 0)),
                _ => None,
            };
            (
                key,
                safe.unwrap_or_else(|| json!({"hash":fingerprint(&value.to_string())})),
            )
        })
        .collect()
}
impl Logs {
    fn enqueue(&self, value: Value) {
        if let Ok(mut bytes) = serde_json::to_vec(&value) {
            bytes.push(b'\n');
            if self.sender.try_send(Job::Entry(bytes)).is_err() {
                self.dropped.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

pub fn init(data_dir: &Path) {
    let (sender, receiver) = mpsc::sync_channel(2048);
    let logs = Arc::new(Logs {
        sender,
        dir: data_dir.join("logs"),
        level: AtomicU8::new(1),
        retention: AtomicU32::new(7),
        dropped: AtomicU64::new(0),
        rates: Mutex::new(HashMap::new()),
    });
    if LOGS.set(logs.clone()).is_err() {
        return;
    }
    let worker = logs.clone();
    std::thread::spawn(move || run_writer(worker, receiver, std::io::stdout()));
    let _ = tracing::subscriber::set_global_default(
        tracing_subscriber::registry().with(LogLayer(logs)),
    );
}
pub fn reload_settings() {
    if let Ok(settings) = config::load_app_settings(&()) {
        configure(&settings.diagnostics);
    }
}
pub fn configure(settings: &DiagnosticsSettings) {
    if let Some(logs) = LOGS.get() {
        logs.level.store(
            match settings.level {
                config::DiagnosticsLogLevel::Debug => 0,
                config::DiagnosticsLogLevel::Info => 1,
                config::DiagnosticsLogLevel::Warn => 2,
            },
            Ordering::Relaxed,
        );
        logs.retention
            .store(settings.retention_days.clamp(1, 30), Ordering::Relaxed);
    }
}
pub fn flush() {
    if let Some(logs) = LOGS.get() {
        let (tx, rx) = mpsc::channel();
        if logs.sender.try_send(Job::Flush(tx)).is_ok() {
            let _ = rx.recv_timeout(Duration::from_secs(3));
        }
    }
}
pub fn shutdown() {
    if let Some(logs) = LOGS.get() {
        let (tx, rx) = mpsc::channel();
        if logs.sender.try_send(Job::Shutdown(tx)).is_ok() {
            let _ = rx.recv_timeout(Duration::from_secs(3));
        } else {
            flush();
        }
    }
}
fn log_files(dir: &Path) -> Vec<(PathBuf, u64, SystemTime)> {
    let mut files = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name();
            let name = name.to_str()?;
            if !name.starts_with("nyaterm-web-") || !name.ends_with(".jsonl") {
                return None;
            }
            // DirEntry metadata can cache size=0 on Windows while the writer is open.
            let meta = fs::metadata(entry.path()).ok()?;
            if !meta.is_file() || entry.file_type().ok()?.is_symlink() {
                return None;
            }
            Some((entry.path(), meta.len(), meta.modified().ok()?))
        })
        .collect::<Vec<_>>();
    files.sort_by_key(|file| file.2);
    files
}
fn prune(logs: &Logs, current: Option<&Path>) {
    let files = log_files(&logs.dir);
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    let retention = Duration::from_secs(logs.retention.load(Ordering::Relaxed) as u64 * 86400);
    for (path, size, modified) in files {
        if Some(path.as_path()) == current {
            continue;
        }
        if total > TOTAL_BYTES.saturating_sub(if current.is_some() { ROTATE_BYTES } else { 0 })
            || modified.elapsed().unwrap_or_default() > retention
        {
            if fs::remove_file(path).is_ok() {
                total = total.saturating_sub(size);
            }
        }
    }
}
fn run_writer<W: Write>(logs: Arc<Logs>, receiver: mpsc::Receiver<Job>, mut stdout: W) -> W {
    let mut file: Option<File> = None;
    let mut current: Option<PathBuf> = None;
    let mut size = 0u64;
    let mut day = 0u64;
    let mut warned = false;
    loop {
        match receiver.recv_timeout(Duration::from_secs(30)) {
            Ok(Job::Shutdown(reply)) => {
                let _ = stdout.flush();
                if let Some(file) = &mut file {
                    let _ = file.flush();
                }
                drop(file);
                let _ = reply.send(());
                break;
            }
            Ok(Job::Flush(reply)) => {
                let _ = stdout.flush();
                if let Some(file) = &mut file {
                    let _ = file.flush();
                }
                let _ = reply.send(());
            }
            Ok(Job::Entry(mut bytes)) => {
                let dropped = logs.dropped.swap(0, Ordering::Relaxed);
                if dropped > 0 {
                    let mut summary = serde_json::to_vec(&json!({"level":"warn","event":"logger.queue_overflow","dropped_count":dropped})).unwrap_or_default();
                    summary.push(b'\n');
                    summary.extend(bytes);
                    bytes = summary;
                }
                let _ = stdout.write_all(&bytes);
                let today = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
                    / 86400;
                if file.is_none() || size + bytes.len() as u64 > ROTATE_BYTES || day != today {
                    file = None;
                    day = today;
                    size = 0;
                    current = Some(logs.dir.join(format!(
                        "nyaterm-web-{today}-{}.jsonl",
                        uuid::Uuid::new_v4()
                    )));
                    if fs::create_dir_all(&logs.dir).is_ok() {
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            let _ =
                                fs::set_permissions(&logs.dir, fs::Permissions::from_mode(0o700));
                        }
                        let mut options = OpenOptions::new();
                        options.create_new(true).write(true);
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::OpenOptionsExt;
                            options.mode(0o600);
                        }
                        file = options.open(current.as_ref().unwrap()).ok();
                    }
                    prune(&logs, current.as_deref());
                }
                if file
                    .as_mut()
                    .is_none_or(|file| file.write_all(&bytes).is_err())
                {
                    file = None;
                    if !warned {
                        let _ = std::io::stderr().write_all(
                            b"NyaTerm Web log file unavailable; stdout logging continues\n",
                        );
                        warned = true;
                    }
                } else {
                    size += bytes.len() as u64;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => prune(&logs, current.as_deref()),
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    stdout
}

pub async fn request_log(mut request: Request, next: Next) -> Response {
    let request_id = request
        .headers()
        .get("x-nyaterm-request-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| uuid::Uuid::parse_str(s).ok())
        .unwrap_or_else(uuid::Uuid::new_v4)
        .to_string();
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|p| p.as_str())
        .unwrap_or("/unmatched")
        .to_owned();
    let method = request.method().as_str().to_owned();
    let operation = if route.ends_with("/commands/{command}") {
        request.uri().path().rsplit('/').next().unwrap_or("unknown")
    } else {
        route.rsplit('/').next().unwrap_or("unknown")
    }
    .to_owned();
    request
        .extensions_mut()
        .insert(RequestId(request_id.clone()));
    let session_id = request
        .uri()
        .path()
        .split("/api/sessions/")
        .nth(1)
        .and_then(|path| path.split('/').next())
        .and_then(|id| uuid::Uuid::parse_str(id).ok())
        .map(|id| id.to_string());
    let started = Instant::now();
    let span = tracing::info_span!("web.request", request_id=%request_id, route=%route, method=%method, operation=%operation, session_id=session_id.as_deref());
    async move {
        let mut response = next.run(request).await;
        let status = response.status().as_u16();
        tracing::info!(
            event = "http.response",
            status,
            duration_ms = started.elapsed().as_millis() as u64,
            "API request completed"
        );
        response.headers_mut().insert(
            "x-nyaterm-request-id",
            HeaderValue::from_str(&request_id).unwrap(),
        );
        if status >= 400
            && response
                .headers()
                .get("content-type")
                .is_some_and(|v| v.as_bytes().starts_with(b"application/json"))
        {
            let (parts, body) = response.into_parts();
            if let Ok(bytes) = axum::body::to_bytes(body, 64 * 1024).await {
                if let Ok(mut value) = serde_json::from_slice::<Value>(&bytes) {
                    if let Some(object) = value.as_object_mut() {
                        object.insert("request_id".into(), json!(request_id));
                    }
                    return (parts, Json(value)).into_response();
                }
                return Response::from_parts(parts, axum::body::Body::from(bytes));
            }
            return Response::from_parts(parts, axum::body::Body::empty());
        }
        response
    }
    .instrument(span)
    .await
}
#[derive(Clone)]
pub struct RequestId(pub String);

pub fn spawn<F>(future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: std::future::Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::spawn(future.in_current_span())
}

pub struct StreamLogGuard {
    event: &'static str,
    session_id: Option<String>,
    span: tracing::Span,
}
impl StreamLogGuard {
    pub fn new(event: &'static str, session_id: Option<String>) -> Self {
        Self {
            event,
            session_id,
            span: tracing::Span::current(),
        }
    }
}
impl Drop for StreamLogGuard {
    fn drop(&mut self) {
        let _entered = self.span.enter();
        tracing::info!(
            event = self.event,
            session_id = self.session_id.as_deref(),
            "Web stream disconnected"
        );
    }
}

pub fn blocking<F, T>(work: F) -> tokio::task::JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let span = tracing::Span::current();
    tokio::task::spawn_blocking(move || {
        let _entered = span.enter();
        work()
    })
}

pub fn error_reason(text: &str) -> &'static str {
    let text = text.to_ascii_lowercase();
    if text.contains("timed out") || text.contains("timeout") {
        "timeout"
    } else if text.contains("refused") {
        "connection_refused"
    } else if text.contains("permission") || text.contains("denied") {
        "permission_denied"
    } else if text.contains("authentication") || text.contains("auth failed") {
        "authentication_failed"
    } else if text.contains("resolve") || text.contains("dns") {
        "name_resolution"
    } else if text.contains("not found") || text.contains("no such") {
        "not_found"
    } else if text.contains("disconnect") || text.contains("closed") || text.contains("eof") {
        "connection_closed"
    } else if text.contains("cancel") {
        "cancelled"
    } else if text.contains("key") || text.contains("certificate") {
        "key_validation"
    } else if text.contains("decrypt") || text.contains("integrity") {
        "decryption_or_integrity"
    } else if text.contains("invalid") || text.contains("unsupported") {
        "validation"
    } else {
        "operation_failed"
    }
}

pub fn record_error(error: &AppError) {
    let (kind, code, reason) = match error {
        AppError::Io(error) => ("io", error.raw_os_error(), format!("{:?}", error.kind())),
        AppError::Ssh(error) => ("ssh", None, error_reason(&error.to_string()).into()),
        AppError::SshKey(_) => ("ssh_key", None, "key_validation".into()),
        AppError::Sftp(error) => (
            "sftp",
            match error {
                russh_sftp::client::error::Error::Status(status) => Some(status.status_code as i32),
                _ => None,
            },
            error_reason(&error.to_string()).into(),
        ),
        AppError::Json(_) => ("json", None, "invalid_parameters".into()),
        AppError::Crypto(_) => ("crypto", None, "decryption_or_integrity".into()),
        AppError::Config(_) => ("config", None, "validation".into()),
        AppError::Storage(_) => ("storage", None, "database_operation".into()),
        AppError::Channel(error) => ("channel", None, error_reason(error).into()),
        _ => ("backend", None, error_reason(&error.to_string()).into()),
    };
    tracing::warn!(event="backend.error", error_kind=kind, error_code=code, reason, error_hash=%fingerprint(&error.to_string()), "Backend operation failed");
}

fn sanitize_frontend(value: &Value, depth: u8) -> Value {
    if depth > 6 {
        return json!("[TRUNCATED]");
    }
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .take(40)
                .map(|(key, value)| {
                    let lower = key.to_lowercase();
                    let safe = if [
                        "password",
                        "secret",
                        "token",
                        "otp",
                        "command",
                        "content",
                        "clipboard",
                        "key",
                        "cookie",
                        "csrf",
                        "authorization",
                        "passphrase",
                    ]
                    .iter()
                    .any(|s| lower.contains(s))
                        && !lower.ends_with("_id")
                    {
                        json!("[REDACTED]")
                    } else if matches!(key.as_str(), "level" | "domain" | "event" | "name")
                        && value.as_str().is_some_and(|s| {
                            s.len() <= 96
                                && s.chars()
                                    .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
                        })
                    {
                        value.clone()
                    } else if key.ends_with("_id")
                        && value
                            .as_str()
                            .is_some_and(|s| uuid::Uuid::parse_str(s).is_ok())
                    {
                        value.clone()
                    } else {
                        sanitize_frontend(value, depth + 1)
                    };
                    // Unknown property names are untrusted too.
                    let key = if key.len() <= 64
                        && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    {
                        key.clone()
                    } else {
                        fingerprint(key)
                    };
                    (key, safe)
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .take(50)
                .map(|v| sanitize_frontend(v, depth + 1))
                .collect(),
        ),
        Value::String(text) => json!({"hash":fingerprint(text)}),
        _ => value.clone(),
    }
}
#[derive(serde::Deserialize)]
pub struct FrontendBatch {
    entries: Vec<Value>,
}
pub async fn frontend(
    ExtractState(state): ExtractState<Arc<State>>,
    Extension(owner): Extension<Owner>,
    Json(batch): Json<FrontendBatch>,
) -> Result<Json<Value>> {
    state.login(&owner.0).await?;
    if batch.entries.len() > 50 {
        return Err(WebError::bad("Log batch exceeds 50 entries"));
    }
    if let Some(logs) = LOGS.get() {
        let mut rates = logs.rates.lock().unwrap_or_else(|e| e.into_inner());
        rates.retain(|_, (start, _)| start.elapsed() < Duration::from_secs(60));
        if rates.len() >= 1024 {
            return Err(WebError(
                axum::http::StatusCode::TOO_MANY_REQUESTS,
                "Log rate limit exceeded".into(),
            ));
        }
        let (_, count) = rates
            .entry(fingerprint(&owner.0))
            .or_insert((Instant::now(), 0));
        if *count >= 120 {
            return Err(WebError(
                axum::http::StatusCode::TOO_MANY_REQUESTS,
                "Log rate limit exceeded".into(),
            ));
        }
        *count += 1;
        drop(rates);
        for entry in batch.entries {
            let level = entry["level"]
                .as_str()
                .ok_or(WebError::bad("Invalid log level"))?;
            let fields = &entry;
            match level {
                "error" => tracing::error!(event="frontend.log", frontend=%fields),
                "warn" => tracing::warn!(event="frontend.log", frontend=%fields),
                "info" => tracing::info!(event="frontend.log", frontend=%fields),
                "debug" => tracing::debug!(event="frontend.log", frontend=%fields),
                _ => return Err(WebError::bad("Invalid log level")),
            }
        }
    }
    Ok(Json(json!({"status":"accepted"})))
}

pub async fn diagnostics() -> Result<Response> {
    let dir = LOGS.get().map(|logs| logs.dir.clone());
    let bytes = tokio::task::spawn_blocking(move || -> std::io::Result<Vec<u8>> {
        flush();
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("manifest.json",options)?;
        zip.write_all(serde_json::to_string_pretty(&json!({"runtime":"web","server_version":env!("CARGO_PKG_VERSION"),"os":std::env::consts::OS,"arch":std::env::consts::ARCH,"log_budget_bytes":20*1024*1024,"capabilities":["ssh","telnet","vnc","sftp","browserFiles","backups"]}))?.as_bytes())?;
        if let Some(dir) = dir {
            let mut budget = 20*1024*1024;
            for (path,size,_) in log_files(&dir).into_iter().rev() {
                if size > budget { continue; } budget-=size;
                zip.start_file(format!("logs/{}",path.file_name().unwrap().to_string_lossy()),options)?;
                let mut bytes = Vec::new(); File::open(path)?.take(size).read_to_end(&mut bytes)?;
                zip.write_all(&bytes)?;
            }
        }
        Ok(zip.finish()?.into_inner())
    }).await.map_err(|_|WebError::bad("Diagnostics export failed"))?.map_err(|_|WebError::bad("Diagnostics export failed"))?;
    Ok((
        [
            (axum::http::header::CONTENT_TYPE, "application/zip"),
            (
                axum::http::header::CONTENT_DISPOSITION,
                "attachment; filename=\"nyaterm-diagnostics.zip\"",
            ),
        ],
        bytes,
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn test_logs(dir: PathBuf) -> (Arc<Logs>, mpsc::Receiver<Job>) {
        let (sender, receiver) = mpsc::sync_channel(2048);
        (
            Arc::new(Logs {
                sender,
                dir,
                level: AtomicU8::new(1),
                retention: AtomicU32::new(7),
                dropped: AtomicU64::new(0),
                rates: Mutex::new(HashMap::new()),
            }),
            receiver,
        )
    }
    #[test]
    fn writer_rotates_and_exports_live_file_sizes() {
        let temp = tempfile::tempdir().unwrap();
        let (logs, receiver) = test_logs(temp.path().to_owned());
        let worker = logs.clone();
        let thread = std::thread::spawn(move || run_writer(worker, receiver, std::io::sink()));
        for _ in 0..110 {
            logs.sender
                .send(Job::Entry(vec![b'x'; 100 * 1024]))
                .unwrap();
        }
        let (tx, rx) = mpsc::channel();
        logs.sender.send(Job::Flush(tx)).unwrap();
        rx.recv().unwrap();
        let files = log_files(temp.path());
        assert_eq!(files.len(), 2);
        assert!(
            files
                .iter()
                .all(|(_, size, _)| *size > 0 && *size <= ROTATE_BYTES)
        );
        assert_eq!(
            files.iter().map(|(_, size, _)| size).sum::<u64>(),
            110 * 100 * 1024
        );
        let (tx, rx) = mpsc::channel();
        logs.sender.send(Job::Shutdown(tx)).unwrap();
        rx.recv().unwrap();
        thread.join().unwrap();
    }
    #[test]
    fn pruning_respects_retention_and_total_budget() {
        let temp = tempfile::tempdir().unwrap();
        let old = temp.path().join("nyaterm-web-old.jsonl");
        File::create(&old)
            .unwrap()
            .set_modified(SystemTime::now() - Duration::from_secs(8 * 86400))
            .unwrap();
        let (logs, _) = test_logs(temp.path().to_owned());
        prune(&logs, None);
        assert!(!old.exists());
        let large = temp.path().join("nyaterm-web-large.jsonl");
        File::create(&large)
            .unwrap()
            .set_len(TOTAL_BYTES + 1)
            .unwrap();
        let unrelated = temp.path().join("database.redb");
        std::fs::write(&unrelated, b"preserve").unwrap();
        prune(&logs, None);
        assert!(!large.exists());
        assert!(unrelated.exists());
    }
    #[test]
    fn unwritable_file_sink_preserves_stdout() {
        let temp = tempfile::tempdir().unwrap();
        let blocked = temp.path().join("file");
        std::fs::write(&blocked, b"blocked").unwrap();
        let (logs, receiver) = test_logs(blocked);
        logs.sender.send(Job::Entry(b"fixture\n".to_vec())).unwrap();
        let (tx, _) = mpsc::channel();
        logs.sender.send(Job::Shutdown(tx)).unwrap();
        assert_eq!(run_writer(logs, receiver, Vec::new()), b"fixture\n");
    }
    #[test]
    fn frontend_secrets_and_arbitrary_messages_never_persist() {
        let value = sanitize_frontend(&json!({"domain":"ui.error","event":"command.error","message":"token abc123","data":{"private_key":"PEM", "password":42,"command":"cat /secret","host":"private.internal"},"error":{"message":"password abc123"}}),0).to_string();
        assert!(!value.contains("abc123"));
        assert!(!value.contains("PEM"));
        assert!(!value.contains("private.internal"));
        assert!(!value.contains("/secret"));
        assert!(value.contains("command.error"));
    }
}
