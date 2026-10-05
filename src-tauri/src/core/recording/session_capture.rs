struct TranscriptState {
    records: VecDeque<TranscriptRecord>,
    record_bytes: usize,
    memory_limit_bytes: usize,
    output_buffer: Vec<char>,
    output_cursor: usize,
    submitted_line_echo: Option<String>,
    next_line_id: u64,
}

impl TranscriptState {
    fn new(memory_limit_bytes: usize) -> Self {
        Self {
            records: VecDeque::new(),
            record_bytes: 0,
            memory_limit_bytes,
            output_buffer: Vec::new(),
            output_cursor: 0,
            submitted_line_echo: None,
            next_line_id: 1,
        }
    }

    fn set_memory_limit(&mut self, memory_limit_bytes: usize) {
        self.memory_limit_bytes = memory_limit_bytes;
        self.trim_records();
    }

    fn record_command(&mut self, command: String) -> Vec<TranscriptRecord> {
        let mut records = self.flush_output_before_command();
        let command = command.trim().to_string();
        if command.is_empty() {
            self.submitted_line_echo = None;
            return records;
        }
        self.submitted_line_echo = Some(command.clone());
        records.push(self.append_record(TranscriptEventKind::Command, command));
        records
    }

    fn record_system(&mut self, message: String) -> Option<TranscriptRecord> {
        if message.trim().is_empty() {
            return None;
        }
        Some(self.append_record(TranscriptEventKind::System, message))
    }

    fn write_output(&mut self, data: &str) -> Vec<TranscriptRecord> {
        if data.is_empty() {
            return Vec::new();
        }

        let replayed = replay_terminal_output(
            data,
            &mut self.output_buffer,
            &mut self.output_cursor,
        );
        let first_forced_split = replayed.first_forced_split;
        replayed
            .lines
            .into_iter()
            .enumerate()
            .filter_map(|(index, line)| {
                if Some(index) == first_forced_split {
                    // A segment ending in the command is not proof of an echo.
                    self.submitted_line_echo = None;
                }
                self.append_output_line(line)
            })
            .collect()
    }

    fn finish(&mut self) -> Vec<TranscriptRecord> {
        self.submitted_line_echo = None;
        self.flush_output_lines(true)
    }

    fn snapshot_records(&mut self) -> Vec<TranscriptRecord> {
        self.flush_output_lines(true);
        self.records.iter().cloned().collect()
    }

    fn append_record(&mut self, kind: TranscriptEventKind, data: String) -> TranscriptRecord {
        let line_id = self.next_line_id;
        self.next_line_id = self.next_line_id.saturating_add(1);
        let record = TranscriptRecord::new(line_id, kind, data);

        self.record_bytes += record.size_bytes;
        self.records.push_back(record.clone());
        self.trim_records();
        record
    }

    fn trim_records(&mut self) {
        while self.records.len() > 1 && self.record_bytes > self.memory_limit_bytes {
            if let Some(record) = self.records.pop_front() {
                self.record_bytes = self.record_bytes.saturating_sub(record.size_bytes);
            }
        }
    }

    fn flush_output_lines(&mut self, flush_partial: bool) -> Vec<TranscriptRecord> {
        let mut flushed = Vec::new();

        if flush_partial {
            let tail = take_replay_line(&mut self.output_buffer, &mut self.output_cursor);
            if let Some(record) = self.append_output_line(tail) {
                flushed.push(record);
            }
        }

        flushed
    }

    fn append_output_line(&mut self, mut line: String) -> Option<TranscriptRecord> {
        line = line.trim_end_matches('\r').to_string();
        if line.is_empty() {
            return None;
        }

        if let Some(echo) = self.submitted_line_echo.as_deref() {
            let trimmed_line = line.trim_end();
            if line == echo || trimmed_line.ends_with(echo) {
                self.submitted_line_echo = None;
                return None;
            }

            if let Some(remainder) = line.strip_prefix(echo) {
                self.submitted_line_echo = None;
                let remainder = strip_one_leading_newline(remainder)
                    .trim_start()
                    .to_string();
                if remainder.is_empty() {
                    return None;
                }
                line = remainder;
            } else {
                self.submitted_line_echo = None;
            }
        }

        Some(self.append_record(TranscriptEventKind::Output, line))
    }

    fn flush_output_before_command(&mut self) -> Vec<TranscriptRecord> {
        let tail = take_replay_line(&mut self.output_buffer, &mut self.output_cursor);

        if looks_like_prompt_tail(&tail) {
            return Vec::new();
        }

        self.append_output_line(tail).into_iter().collect()
    }
}

fn looks_like_prompt_tail(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return true;
    }

    trimmed.contains('')
        || (trimmed.starts_with("PS ") && trimmed.ends_with('>'))
        || matches!(trimmed.chars().last(), Some('$' | '#'))
}

struct RecordingRuntime {
    sink: Option<RecordingSink>,
    context: Option<RecordingContext>,
    profile: Option<RecordingProfile>,
    size_rotation_index: u64,
    daily_key: String,
}

impl RecordingRuntime {
    fn new() -> Self {
        Self {
            sink: None,
            context: None,
            profile: None,
            size_rotation_index: 0,
            daily_key: local_day_key(
                OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc()),
            ),
        }
    }

    fn is_recording(&self) -> bool {
        self.sink.is_some()
    }
}

struct SessionRecorder {
    transcript: Mutex<TranscriptState>,
    runtime: Mutex<RecordingRuntime>,
    lifecycle: Mutex<ScopeLifecycle>,
}

#[derive(Default)]
struct ScopeLifecycle {
    active_sessions: HashSet<String>,
    finalize_pending: bool,
}

impl SessionRecorder {
    fn new(memory_limit_bytes: usize) -> Self {
        Self {
            transcript: Mutex::new(TranscriptState::new(memory_limit_bytes)),
            runtime: Mutex::new(RecordingRuntime::new()),
            lifecycle: Mutex::new(ScopeLifecycle::default()),
        }
    }

    fn set_memory_limit(&self, memory_limit_bytes: usize) {
        lock_recover(&self.transcript).set_memory_limit(memory_limit_bytes);
    }
}

enum WriterMessage {
    Data(Vec<u8>),
    Rotate {
        path: PathBuf,
        behavior: ExistingFileBehavior,
        header: Option<Vec<u8>>,
    },
    Stop(Option<Vec<u8>>),
}

struct RecordingStatusState {
    session_id: Mutex<String>,
    mode: RecordingMode,
    file_path: Mutex<String>,
    started_at: String,
    state: Mutex<RecordingState>,
    written_bytes: AtomicU64,
    queued_bytes: AtomicU64,
    dropped_bytes: AtomicU64,
    last_error: Mutex<Option<String>>,
    app_handle: OnceLock<tauri::AppHandle>,
}

impl RecordingStatusState {
    fn new(session_id: String, mode: RecordingMode, file_path: PathBuf) -> Self {
        Self {
            session_id: Mutex::new(session_id),
            mode,
            file_path: Mutex::new(file_path.to_string_lossy().to_string()),
            started_at: chrono_timestamp(),
            state: Mutex::new(RecordingState::Starting),
            written_bytes: AtomicU64::new(0),
            queued_bytes: AtomicU64::new(0),
            dropped_bytes: AtomicU64::new(0),
            last_error: Mutex::new(None),
            app_handle: OnceLock::new(),
        }
    }

    fn set_app_handle(&self, app: Option<&tauri::AppHandle>) {
        if let Some(app) = app {
            let _ = self.app_handle.set(app.clone());
        }
    }

    fn snapshot(&self) -> RecordingStatus {
        RecordingStatus {
            session_id: lock_recover(&self.session_id).clone(),
            state: *lock_recover(&self.state),
            mode: self.mode,
            file_path: lock_recover(&self.file_path).clone(),
            started_at: self.started_at.clone(),
            written_bytes: self.written_bytes.load(Ordering::Relaxed),
            queued_bytes: self.queued_bytes.load(Ordering::Relaxed),
            dropped_bytes: self.dropped_bytes.load(Ordering::Relaxed),
            last_error: lock_recover(&self.last_error).clone(),
        }
    }

    fn set_state(&self, state: RecordingState) {
        *lock_recover(&self.state) = state;
        self.emit();
    }

    fn set_file_path(&self, path: &Path) {
        *lock_recover(&self.file_path) = path.to_string_lossy().to_string();
        self.emit();
    }

    fn set_session_id(&self, session_id: &str) {
        *lock_recover(&self.session_id) = session_id.to_string();
        self.emit();
    }

    fn set_error(&self, state: RecordingState, error: impl ToString) {
        *lock_recover(&self.state) = state;
        *lock_recover(&self.last_error) = Some(error.to_string());
        self.emit();
    }

    fn add_dropped(&self, bytes: u64, reason: &str) {
        self.dropped_bytes.fetch_add(bytes, Ordering::Relaxed);
        self.set_error(RecordingState::Degraded, reason);
    }

    fn emit(&self) {
        if let Some(app) = self.app_handle.get() {
            let _ = tauri::Emitter::emit(app, "recording-status-changed", self.snapshot());
        }
    }
}

struct RecordingSink {
    tx: SyncSender<WriterMessage>,
    status: Arc<RecordingStatusState>,
    join: Option<thread::JoinHandle<()>>,
    queue_limit_bytes: u64,
}

impl RecordingSink {
    fn start(
        session_id: &str,
        mode: RecordingMode,
        path: PathBuf,
        behavior: ExistingFileBehavior,
        app_handle: Option<&tauri::AppHandle>,
        initial_bytes: Option<Vec<u8>>,
    ) -> AppResult<Self> {
        let path = open_collision_safe_path(&path, behavior)?;
        let file = open_recording_file(&path, behavior)?;
        let status = Arc::new(RecordingStatusState::new(
            session_id.to_string(),
            mode,
            path.clone(),
        ));
        status.set_app_handle(app_handle);

        let (tx, rx) = mpsc::sync_channel::<WriterMessage>(512);
        let worker_status = status.clone();
        let join = thread::spawn(move || {
            let mut writer = BufWriter::new(file);
            worker_status.set_state(RecordingState::Recording);
            while let Ok(message) = rx.recv() {
                match message {
                    WriterMessage::Data(bytes) => {
                        let len = bytes.len() as u64;
                        worker_status.queued_bytes.fetch_sub(len, Ordering::Relaxed);
                        if let Err(error) = writer.write_all(&bytes) {
                            worker_status.set_error(
                                RecordingState::Failed,
                                format!("Failed to write recording file: {error}"),
                            );
                            continue;
                        }
                        worker_status
                            .written_bytes
                            .fetch_add(len, Ordering::Relaxed);
                    }
                    WriterMessage::Rotate {
                        path,
                        behavior,
                        header,
                    } => {
                        if let Err(error) = writer.flush() {
                            worker_status.set_error(
                                RecordingState::Failed,
                                format!("Failed to flush recording file: {error}"),
                            );
                            continue;
                        }
                        match open_recording_file(&path, behavior) {
                            Ok(file) => {
                                writer = BufWriter::new(file);
                                worker_status.written_bytes.store(0, Ordering::Relaxed);
                                worker_status.set_file_path(&path);
                                worker_status.set_state(RecordingState::Recording);
                                if let Some(header) = header {
                                    let len = header.len() as u64;
                                    if let Err(error) = writer.write_all(&header) {
                                        worker_status.set_error(
                                            RecordingState::Failed,
                                            format!(
                                                "Failed to write rotated recording header: {error}"
                                            ),
                                        );
                                    } else {
                                        worker_status
                                            .written_bytes
                                            .fetch_add(len, Ordering::Relaxed);
                                    }
                                }
                            }
                            Err(error) => {
                                worker_status.set_error(RecordingState::Failed, error);
                            }
                        }
                    }
                    WriterMessage::Stop(footer) => {
                        worker_status.set_state(RecordingState::Stopping);
                        if let Some(bytes) = footer {
                            let len = bytes.len() as u64;
                            if let Err(error) = writer.write_all(&bytes) {
                                worker_status.set_error(
                                    RecordingState::Failed,
                                    format!("Failed to write recording footer: {error}"),
                                );
                            } else {
                                worker_status
                                    .written_bytes
                                    .fetch_add(len, Ordering::Relaxed);
                            }
                        }
                        if let Err(error) = writer.flush() {
                            worker_status.set_error(
                                RecordingState::Failed,
                                format!("Failed to flush recording file: {error}"),
                            );
                        }
                        break;
                    }
                }
            }
        });

        let sink = Self {
            tx,
            status,
            join: Some(join),
            queue_limit_bytes: DEFAULT_RECORDING_QUEUE_LIMIT_BYTES,
        };

        if let Some(bytes) = initial_bytes {
            sink.enqueue(bytes);
        }
        Ok(sink)
    }

    fn enqueue(&self, bytes: Vec<u8>) {
        if bytes.is_empty() {
            return;
        }

        let len = bytes.len() as u64;
        let queued = self.status.queued_bytes.load(Ordering::Relaxed);
        if queued.saturating_add(len) > self.queue_limit_bytes {
            self.status
                .add_dropped(len, "Recording queue is full; dropped log bytes");
            return;
        }

        self.status.queued_bytes.fetch_add(len, Ordering::Relaxed);
        match self.tx.try_send(WriterMessage::Data(bytes)) {
            Ok(()) => {}
            Err(TrySendError::Full(message)) => {
                self.status.queued_bytes.fetch_sub(len, Ordering::Relaxed);
                let dropped = match message {
                    WriterMessage::Data(bytes) => bytes.len() as u64,
                    WriterMessage::Rotate { .. } => 0,
                    WriterMessage::Stop(bytes) => bytes.map_or(0, |bytes| bytes.len() as u64),
                };
                self.status
                    .add_dropped(dropped, "Recording writer channel is full");
            }
            Err(TrySendError::Disconnected(message)) => {
                self.status.queued_bytes.fetch_sub(len, Ordering::Relaxed);
                let dropped = match message {
                    WriterMessage::Data(bytes) => bytes.len() as u64,
                    WriterMessage::Rotate { .. } => 0,
                    WriterMessage::Stop(bytes) => bytes.map_or(0, |bytes| bytes.len() as u64),
                };
                self.status
                    .add_dropped(dropped, "Recording writer has stopped");
            }
        }
    }

    fn rotate(&self, path: PathBuf, behavior: ExistingFileBehavior, header: Option<Vec<u8>>) {
        let _ = self.tx.send(WriterMessage::Rotate {
            path,
            behavior,
            header,
        });
    }

    fn stop(mut self, footer: Option<Vec<u8>>) -> RecordingStatus {
        let _ = self.tx.send(WriterMessage::Stop(footer));
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
        self.status.snapshot()
    }
}
