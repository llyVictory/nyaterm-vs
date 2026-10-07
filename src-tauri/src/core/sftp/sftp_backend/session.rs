//! Internal pieces of the SFTP backend moved out of `sftp_backend.rs`.

use super::*;

static NEXT_SFTP_SESSION_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub(super) struct SftpSessionPool {
    sessions: Arc<Vec<Arc<ManagedSftpSession>>>,
}

impl SftpSessionPool {
    pub(super) async fn new(
        backend: &SftpBackend,
        size: usize,
        config: SftpClientConfig,
        operation: &'static str,
    ) -> AppResult<Self> {
        let size = if backend.compatibility_mode() {
            1
        } else {
            size
        };
        let mut sessions = Vec::with_capacity(size);
        for _ in 0..size {
            sessions.push(Arc::new(
                backend
                    .open_sftp_with_client_config_for_operation(config.clone(), operation)
                    .await?,
            ));
        }
        Ok(Self {
            sessions: Arc::new(sessions),
        })
    }

    pub(super) fn session_for(&self, index: usize) -> Arc<ManagedSftpSession> {
        self.sessions[index % self.sessions.len()].clone()
    }

    pub(super) async fn close_all(self) {
        for session in self.sessions.iter() {
            let _ = session.close().await;
        }
    }
}

pub(super) struct SftpSessionCore {
    inner: SftpSession,
    _permit: OwnedSemaphorePermit,
    sftp_session_id: u64,
}

pub(super) struct CompatibilitySftpSession {
    core: Arc<SftpSessionCore>,
    operation_lock: Arc<Mutex<()>>,
}

impl CompatibilitySftpSession {
    fn new(core: Arc<SftpSessionCore>) -> Self {
        Self {
            core,
            operation_lock: Arc::new(Mutex::new(())),
        }
    }
}

pub(super) struct ManagedSftpSession {
    core: Arc<SftpSessionCore>,
    operation: &'static str,
    close_on_finish: bool,
    _operation_guard: Option<OwnedMutexGuard<()>>,
}

pub(super) struct ManagedSftpSessionPair {
    source: ManagedSftpSession,
    target: Option<ManagedSftpSession>,
    requires_sequential_io: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SftpSessionAcquireOrder {
    Shared,
    SourceFirst,
    TargetFirst,
}

pub(super) fn sftp_session_acquire_order(
    source_compatibility_key: Option<usize>,
    target_compatibility_key: Option<usize>,
) -> SftpSessionAcquireOrder {
    match (source_compatibility_key, target_compatibility_key) {
        (Some(source), Some(target)) if source == target => SftpSessionAcquireOrder::Shared,
        (Some(source), Some(target)) if source > target => SftpSessionAcquireOrder::TargetFirst,
        (None, Some(_)) => SftpSessionAcquireOrder::TargetFirst,
        _ => SftpSessionAcquireOrder::SourceFirst,
    }
}

impl ManagedSftpSessionPair {
    fn new(
        source: ManagedSftpSession,
        target: Option<ManagedSftpSession>,
        requires_sequential_io: bool,
    ) -> Self {
        Self {
            source,
            target,
            requires_sequential_io,
        }
    }

    pub(super) fn source(&self) -> &SftpSession {
        &self.source
    }

    pub(super) fn target(&self) -> &SftpSession {
        self.target.as_deref().unwrap_or(&self.source)
    }

    pub(super) fn requires_sequential_io(&self) -> bool {
        self.requires_sequential_io
    }

    pub(super) async fn close(self) {
        let Self { source, target, .. } = self;
        let _ = source.close().await;
        if let Some(target) = target {
            let _ = target.close().await;
        }
    }
}

impl ManagedSftpSession {
    fn fresh(core: Arc<SftpSessionCore>, operation: &'static str) -> Self {
        Self {
            core,
            operation,
            close_on_finish: true,
            _operation_guard: None,
        }
    }

    fn shared(
        core: Arc<SftpSessionCore>,
        operation: &'static str,
        operation_guard: OwnedMutexGuard<()>,
    ) -> Self {
        Self {
            core,
            operation,
            close_on_finish: false,
            _operation_guard: Some(operation_guard),
        }
    }

    pub(super) fn sftp_session_id(&self) -> u64 {
        self.core.sftp_session_id
    }

    pub(super) async fn close(&self) -> Result<(), SftpError> {
        if !self.close_on_finish {
            tracing::debug!(
                sftp_session_id = self.sftp_session_id(),
                operation = self.operation,
                stage = "release_session",
                "SFTP compatibility session retained"
            );
            return Ok(());
        }
        tracing::debug!(
            sftp_session_id = self.sftp_session_id(),
            operation = self.operation,
            stage = "close_session",
            "SFTP session closing"
        );
        let result = self.core.inner.close().await;
        match &result {
            Ok(()) => tracing::debug!(
                sftp_session_id = self.sftp_session_id(),
                operation = self.operation,
                stage = "close_session",
                "SFTP session closed"
            ),
            Err(error) => tracing::warn!(
                sftp_session_id = self.sftp_session_id(),
                operation = self.operation,
                stage = "close_session",
                error = %error,
                stream_closed = is_sftp_stream_closed_error(error),
                "SFTP session close failed"
            ),
        }
        result
    }
}

impl Deref for ManagedSftpSession {
    type Target = SftpSession;

    fn deref(&self) -> &Self::Target {
        &self.core.inner
    }
}

impl SftpBackend {
    pub(crate) async fn probe(ssh_handle: &Arc<SshConnectionHandles>) -> AppResult<()> {
        tracing::debug!(operation = "probe", "SFTP probe started");
        let core =
            Self::open_sftp_core(ssh_handle.clone(), SftpClientConfig::default(), "probe").await?;
        let sftp = ManagedSftpSession::fresh(core, "probe");
        tracing::debug!(
            sftp_session_id = sftp.sftp_session_id(),
            operation = "probe",
            "SFTP probe session opened"
        );
        tracing::debug!(
            sftp_session_id = sftp.sftp_session_id(),
            operation = "probe",
            stage = "close_session",
            "SFTP probe session close requested"
        );
        let _ = sftp.close().await;
        tracing::debug!(
            sftp_session_id = sftp.sftp_session_id(),
            operation = "probe",
            "SFTP probe succeeded"
        );
        Ok(())
    }

    pub(crate) async fn probe_and_create(
        ssh_handle: Arc<SshConnectionHandles>,
        encoding: &str,
        pipeline_depth_override: Option<u32>,
        compatibility_mode: bool,
        shell_available: bool,
        compatibility_config: SftpClientConfig,
    ) -> AppResult<Self> {
        tracing::debug!(
            compatibility_mode,
            operation = "probe",
            "SFTP probe started"
        );
        let config = if compatibility_mode {
            compatibility_config
        } else {
            SftpClientConfig::default()
        };
        let core = Self::open_sftp_core(ssh_handle.clone(), config, "probe").await?;
        if compatibility_mode {
            tracing::info!(
                sftp_session_id = core.sftp_session_id,
                "SFTP compatibility session retained after probe"
            );
            return Ok(Self::new_with_compatibility_session(
                ssh_handle,
                encoding,
                pipeline_depth_override,
                Some(Arc::new(CompatibilitySftpSession::new(core))),
                shell_available,
            ));
        }

        let sftp = ManagedSftpSession::fresh(core, "probe");
        let _ = sftp.close().await;
        Ok(Self::new(
            ssh_handle,
            encoding,
            pipeline_depth_override,
            shell_available,
        ))
    }

    pub(super) async fn open_sftp_core(
        ssh_handle: Arc<SshConnectionHandles>,
        config: SftpClientConfig,
        operation: &'static str,
    ) -> AppResult<Arc<SftpSessionCore>> {
        for attempt in 0..=SFTP_CHANNEL_OPEN_RETRY_DELAYS.len() {
            let sftp_session_id = NEXT_SFTP_SESSION_ID.fetch_add(1, Ordering::Relaxed);
            tracing::debug!(
                sftp_session_id,
                operation,
                attempt,
                stage = "acquire_permit",
                "SFTP session opening"
            );
            let permit = match ssh_handle.acquire_sftp_channel_permit().await {
                Ok(permit) => permit,
                Err(error) => {
                    tracing::warn!(
                        sftp_session_id,
                        operation,
                        attempt,
                        stage = "acquire_permit",
                        error = %error,
                        "SFTP session setup failed"
                    );
                    return Err(error);
                }
            };
            let setup_result = tokio::time::timeout(SFTP_SESSION_SETUP_TIMEOUT, async {
                let channel_result = {
                    let handle_mtx = ssh_handle.target_handle();
                    let handle = handle_mtx.lock().await;
                    handle.channel_open_session().await
                };
                let channel = match channel_result {
                    Ok(channel) => {
                        tracing::debug!(
                            sftp_session_id,
                            operation,
                            attempt,
                            stage = "channel_open",
                            "SSH session channel opened for SFTP"
                        );
                        channel
                    }
                    Err(error) => {
                        tracing::warn!(
                            sftp_session_id,
                            operation,
                            attempt,
                            stage = "channel_open",
                            error = %error,
                            "SFTP session setup failed"
                        );
                        return Ok(Err(error));
                    }
                };

                if let Err(error) = channel.request_subsystem(true, "sftp").await {
                    let error =
                        AppError::Channel(format!("Failed to start SFTP subsystem: {}", error));
                    tracing::warn!(
                        sftp_session_id,
                        operation,
                        attempt,
                        stage = "request_subsystem",
                        error = %error,
                        "SFTP session setup failed"
                    );
                    return Err(error);
                }
                tracing::debug!(
                    sftp_session_id,
                    operation,
                    attempt,
                    stage = "request_subsystem",
                    "SFTP subsystem request accepted"
                );

                let sftp = match nyaterm_core::ssh::protocol::initialize_sftp(
                    channel.into_stream(),
                    config.clone(),
                )
                .await
                {
                    Ok(sftp) => sftp,
                    Err(error) => {
                        tracing::warn!(
                            sftp_session_id,
                            operation,
                            attempt,
                            stage = "sftp_client_init",
                            error = %error,
                            stream_closed = is_sftp_stream_closed_error(&error),
                            "SFTP session setup failed"
                        );
                        return Err(error.into());
                    }
                };
                tracing::debug!(
                    sftp_session_id,
                    operation,
                    attempt,
                    stage = "sftp_client_init",
                    "SFTP client initialized"
                );
                AppResult::Ok(Ok(Arc::new(SftpSessionCore {
                    inner: sftp,
                    _permit: permit,
                    sftp_session_id,
                })))
            })
            .await;
            let setup_result = match setup_result {
                Ok(Ok(result)) => result,
                Ok(Err(error)) => return Err(error),
                Err(_) => {
                    let error = AppError::Channel(
                        "SFTP session setup timed out after 10 seconds".to_string(),
                    );
                    tracing::warn!(
                        sftp_session_id,
                        operation,
                        attempt,
                        stage = "setup_timeout",
                        error = %error,
                        "SFTP session setup failed"
                    );
                    return Err(error);
                }
            };

            match setup_result {
                Ok(session) => {
                    tracing::debug!(
                        sftp_session_id,
                        operation,
                        attempt,
                        stage = "ready",
                        "SFTP session ready"
                    );
                    return Ok(session);
                }
                Err(error)
                    if attempt < SFTP_CHANNEL_OPEN_RETRY_DELAYS.len()
                        && is_retryable_sftp_channel_open_error(&error) =>
                {
                    tokio::time::sleep(SFTP_CHANNEL_OPEN_RETRY_DELAYS[attempt]).await;
                    continue;
                }
                Err(error) => {
                    return Err(AppError::Channel(format!(
                        "Failed to open SFTP channel: {}",
                        error
                    )));
                }
            }
        }

        unreachable!("SFTP channel open retry loop always returns or continues");
    }

    pub(super) async fn open_sftp(&self) -> AppResult<ManagedSftpSession> {
        self.open_sftp_for_operation("business").await
    }

    pub(super) async fn open_sftp_for_operation(
        &self,
        operation: &'static str,
    ) -> AppResult<ManagedSftpSession> {
        self.open_sftp_with_client_config_for_operation(SftpClientConfig::default(), operation)
            .await
    }

    pub(super) async fn open_sftp_with_client_config(
        &self,
        config: SftpClientConfig,
    ) -> AppResult<ManagedSftpSession> {
        self.open_sftp_with_client_config_for_operation(config, "business")
            .await
    }

    pub(super) async fn open_sftp_with_client_config_for_operation(
        &self,
        config: SftpClientConfig,
        operation: &'static str,
    ) -> AppResult<ManagedSftpSession> {
        if let Some(compatibility_session) = &self.compatibility_session {
            let operation_guard = compatibility_session
                .operation_lock
                .clone()
                .lock_owned()
                .await;
            return Ok(ManagedSftpSession::shared(
                compatibility_session.core.clone(),
                operation,
                operation_guard,
            ));
        }

        let core = Self::open_sftp_core(self.ssh_handle.clone(), config, operation).await?;
        Ok(ManagedSftpSession::fresh(core, operation))
    }

    pub(super) async fn open_sftp_pair(
        &self,
        target: &SftpBackend,
    ) -> AppResult<ManagedSftpSessionPair> {
        let source_key = self
            .compatibility_session
            .as_ref()
            .map(|session| Arc::as_ptr(session) as usize);
        let target_key = target
            .compatibility_session
            .as_ref()
            .map(|session| Arc::as_ptr(session) as usize);
        let requires_sequential_io = source_key.is_some() || target_key.is_some();

        match sftp_session_acquire_order(source_key, target_key) {
            SftpSessionAcquireOrder::Shared => {
                let source = self.open_sftp_for_operation("remote_copy").await?;
                Ok(ManagedSftpSessionPair::new(source, None, true))
            }
            SftpSessionAcquireOrder::TargetFirst => {
                let target_session = target.open_sftp_for_operation("remote_copy_target").await?;
                let source = self.open_sftp_for_operation("remote_copy_source").await?;
                Ok(ManagedSftpSessionPair::new(
                    source,
                    Some(target_session),
                    true,
                ))
            }
            SftpSessionAcquireOrder::SourceFirst => {
                let source = self.open_sftp_for_operation("remote_copy_source").await?;
                let target_session = target.open_sftp_for_operation("remote_copy_target").await?;
                Ok(ManagedSftpSessionPair::new(
                    source,
                    Some(target_session),
                    requires_sequential_io,
                ))
            }
        }
    }

    pub(super) async fn exec(&self, command: &str) -> AppResult<ExecResult> {
        if !self.shell_available {
            return Err(AppError::Channel(
                "Remote commands are unavailable for SFTP-only sessions".to_string(),
            ));
        }

        match tokio::time::timeout(SFTP_EXEC_TIMEOUT, async {
            let handle_mtx = self.ssh_handle.target_handle();
            let mut channel = {
                let handle = handle_mtx.lock().await;
                handle
                    .channel_open_session()
                    .await
                    .map_err(|e| AppError::Channel(format!("Failed to open exec channel: {}", e)))?
            };

            channel.exec(true, command.as_bytes()).await?;

            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let mut exit_code: Option<u32> = None;

            loop {
                match channel.wait().await {
                    Some(ChannelMsg::Data { data }) => {
                        stdout.extend_from_slice(&data);
                    }
                    Some(ChannelMsg::ExtendedData { data, ext }) => {
                        if ext == 1 {
                            stderr.extend_from_slice(&data);
                        }
                    }
                    Some(ChannelMsg::ExitStatus { exit_status }) => {
                        exit_code = Some(exit_status);
                    }
                    Some(ChannelMsg::Eof) | None => {
                        if exit_code.is_none() {
                            if let Some(ChannelMsg::ExitStatus { exit_status }) =
                                channel.wait().await
                            {
                                exit_code = Some(exit_status);
                            }
                        }
                        break;
                    }
                    _ => {}
                }
            }

            Ok(ExecResult {
                exit_code: exit_code.unwrap_or(255),
                stdout,
                stderr,
            })
        })
        .await
        {
            Ok(result) => result,
            Err(_) => {
                tracing::warn!(
                    timeout_secs = SFTP_EXEC_TIMEOUT.as_secs(),
                    "Remote SFTP backend command timed out"
                );
                Err(AppError::Channel(format!(
                    "Remote command timed out after {} seconds",
                    SFTP_EXEC_TIMEOUT.as_secs()
                )))
            }
        }
    }

    pub(super) async fn exec_ok(&self, command: &str) -> AppResult<Vec<u8>> {
        let result = self.exec(command).await?;
        if result.exit_code != 0 {
            let msg = String::from_utf8_lossy(&result.stderr);
            return Err(AppError::Channel(format!(
                "Remote command failed (exit {}): {}",
                result.exit_code,
                msg.trim()
            )));
        }
        Ok(result.stdout)
    }
}
