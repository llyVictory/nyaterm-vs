/// Spawns a local shell in a PTY and registers the session with the manager.
pub async fn create_local_session(
    app: AppHandle,
    manager: Arc<SessionManager>,
    config: Option<LocalSessionConfig>,
    owner_window_label: Option<String>,
    session_ready_hook: Option<SessionReadyHook>,
) -> AppResult<String> {
    tracing::info!("Creating local PTY session");
    validate_working_dir_before_spawn(config.as_ref())?;

    let session_id = uuid::Uuid::new_v4().to_string();
    let (cmd_tx, cmd_rx) = session_command_channel(session_id.clone());
    let output_control_tx = cmd_tx.clone();

    let session_name = config
        .as_ref()
        .map_or("Local Terminal".to_string(), |c| c.name.clone());

    let (_, shell_name) = match &config {
        Some(cfg) if !cfg.shell_path.trim().is_empty() => {
            build_shell_command(&cfg.shell_path, &cfg.shell_args)
                .map_err(crate::error::AppError::Config)?
        }
        _ => platform_default_shell(),
    };
    let ai_execution_profile = infer_local_ai_execution_profile(&shell_name);
    let ready_marker = build_ready_marker(&session_id);
    let startup_script = build_local_startup_script(&shell_name, &ready_marker);
    let injection_active = startup_script.shell_integration_active;

    let session_info = SessionInfo {
        id: session_id.clone(),
        name: session_name,
        session_type: SessionType::Local,
        started_at: crate::core::now_session_started_at(),
        connection_id: config.as_ref().and_then(|cfg| cfg.connection_id.clone()),
        connected: true,
        owner_window_label,
        ai_execution_profile,
        injection_active,
        remote_file_browser_enabled: false,
        remote_stats_enabled: false,
        ssh_profile: None,
    };

    let cwd: SharedCwd = Arc::new(tokio::sync::Mutex::new(None));
    let session_handle = SessionHandle {
        info: session_info.clone(),
        cmd_tx,
        ssh_config: None,
        ssh_handle: None,
        cwd: cwd.clone(),
        remote_fs: None,
    };
    manager.add_session(session_handle).await;
    if let Some(hook) = session_ready_hook.as_ref() {
        hook(&session_info);
    }

    let sid = session_id.clone();
    let mgr = manager.clone();
    let rt_handle = tokio::runtime::Handle::current();
    let encoding = config
        .as_ref()
        .map(|c| c.encoding.clone())
        .unwrap_or_else(|| {
            crate::config::load_app_settings(&app)
                .map(|settings| settings.interaction.default_encoding)
                .unwrap_or_else(|_| "UTF-8".to_string())
        });

    std::thread::spawn(move || {
        pty_session_thread(
            app,
            sid,
            mgr,
            cmd_rx,
            output_control_tx,
            rt_handle,
            cwd,
            config,
            startup_script.script,
            ready_marker,
            encoding,
        );
    });

    Ok(session_id)
}

fn pty_session_thread(
    app: AppHandle,
    session_id: String,
    manager: Arc<SessionManager>,
    mut cmd_rx: SessionCommandReceiver,
    output_control_tx: SessionCommandSender,
    rt_handle: tokio::runtime::Handle,
    cwd: SharedCwd,
    config: Option<LocalSessionConfig>,
    startup_script: Option<String>,
    ready_marker: String,
    encoding: String,
) {
    let pty_system = native_pty_system();
    let pair = match pty_system.openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    }) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("Failed to open PTY: {}", e);
            let _ = app.emit(
                &format!("session-error-{}", session_id),
                format!("Failed to open PTY: {}", e),
            );
            let _ = app.emit(&format!("session-closed-{}", session_id), ());
            rt_handle.block_on(async {
                manager.remove_session(&session_id).await;
            });
            return;
        }
    };

    let (mut cmd, _) = match &config {
        Some(cfg) if !cfg.shell_path.trim().is_empty() => {
            match build_shell_command(&cfg.shell_path, &cfg.shell_args) {
                Ok(command) => command,
                Err(error) => {
                    tracing::error!("Failed to build shell command: {}", error);
                    let _ = app.emit(
                        &format!("session-error-{}", session_id),
                        format!("Failed to build shell command: {}", error),
                    );
                    let _ = app.emit(&format!("session-closed-{}", session_id), ());
                    rt_handle.block_on(async {
                        manager.remove_session(&session_id).await;
                    });
                    return;
                }
            }
        }
        _ => platform_default_shell(),
    };

    if let Some(ref cfg) = config {
        if let Some(ref dir) = cfg.working_dir {
            if apply_working_dir_to_command(&mut cmd, dir)
                == WorkingDirectoryOutcome::MissingLocalDirectory
            {
                if cfg.fail_on_missing_working_dir {
                    tracing::error!(
                        working_dir = %dir,
                        "Explicit local terminal working directory does not exist"
                    );
                    let _ = app.emit(
                        &format!("session-error-{}", session_id),
                        format!("Working directory '{}' does not exist.", dir),
                    );
                    let _ = app.emit(&format!("session-closed-{}", session_id), ());
                    rt_handle.block_on(async {
                        manager.remove_session(&session_id).await;
                    });
                    return;
                }
                tracing::warn!(
                    working_dir = %dir,
                    "Configured local terminal working directory does not exist; using default working directory"
                );
                let _ = app.emit(
                    &format!("session-warning-{}", session_id),
                    format!(
                        "Configured working directory '{}' does not exist; using the default working directory.",
                        dir
                    ),
                );
            }
        }
    }

    #[cfg(target_os = "macos")]
    ensure_macos_interactive_path(&mut cmd);
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    configure_local_pty_environment(&mut cmd);

    let mut child = match pair.slave.spawn_command(cmd) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to spawn shell: {}", e);
            let _ = app.emit(
                &format!("session-error-{}", session_id),
                format!("Failed to spawn shell: {}", e),
            );
            let _ = app.emit(&format!("session-closed-{}", session_id), ());
            rt_handle.block_on(async {
                manager.remove_session(&session_id).await;
            });
            return;
        }
    };
    drop(pair.slave);

    let mut writer = match pair.master.take_writer() {
        Ok(w) => w,
        Err(e) => {
            tracing::error!("Failed to take PTY writer: {}", e);
            let _ = app.emit(
                &format!("session-error-{}", session_id),
                format!("Failed to take PTY writer: {}", e),
            );
            let _ = app.emit(&format!("session-closed-{}", session_id), ());
            rt_handle.block_on(async {
                manager.remove_session(&session_id).await;
            });
            return;
        }
    };

    let mut reader = match pair.master.try_clone_reader() {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("Failed to clone PTY reader: {}", e);
            let _ = app.emit(
                &format!("session-error-{}", session_id),
                format!("Failed to clone PTY reader: {}", e),
            );
            let _ = app.emit(&format!("session-closed-{}", session_id), ());
            rt_handle.block_on(async {
                manager.remove_session(&session_id).await;
            });
            return;
        }
    };
    let master = pair.master;

    let output_event = format!("terminal-output-{}", session_id);
    let output =
        SessionOutputCoalescer::for_app(app.clone(), output_event.clone(), output_control_tx);

    let capture_processor = Arc::new(StdMutex::new(OutputCaptureProcessor::new()));
    let capture_for_reader = capture_processor.clone();
    let output_pause = Arc::new((StdMutex::new(false), std::sync::Condvar::new()));
    let output_pause_reader = output_pause.clone();

    let zmodem_state: Arc<StdMutex<Option<ZmodemTransfer>>> = Arc::new(StdMutex::new(None));
    let zmodem_state_reader = zmodem_state.clone();
    let zmodem_event_name = format!("zmodem-event-{session_id}");
    let zmodem_event_reader = zmodem_event_name.clone();
    let (zmodem_out_tx, mut zmodem_out_rx) = mpsc::unbounded_channel::<Vec<u8>>();

    let app_read = app.clone();
    let sid_read = session_id.clone();
    let cwd_event = format!("cwd-changed-{}", session_id);
    let rt_for_reader = rt_handle.clone();
    let recording_mgr_reader: Option<Arc<RecordingManager>> = app
        .try_state::<Arc<RecordingManager>>()
        .map(|s| s.inner().clone());
    let sid_for_rec_reader = session_id.clone();
    let output_reader = output.clone();
    let manager_reader = manager.clone();
    let suppress_startup_output = startup_script.is_some();
    let encoding_for_reader = encoding.clone();
    let (reader_done_tx, reader_done_rx) = std_mpsc::channel::<()>();
    std::thread::spawn(move || {
        let mut raw_buf = [0u8; 4096];
        let mut stripper = OscStripper::new(&ready_marker);
        let mut suppress_visible = suppress_startup_output;
        let mut zmodem_detector = ZmodemDetector::new();
        let mut output_decoder = TerminalOutputDecoder::new(&encoding_for_reader);
        loop {
            {
                let (lock, cvar) = &*output_pause_reader;
                let mut paused = lock.lock().unwrap();
                while *paused {
                    paused = cvar.wait(paused).unwrap();
                }
            }
            match reader.read(&mut raw_buf) {
                Ok(0) => break,
                Ok(n) => {
                    let raw = &raw_buf[..n];

                    // ZMODEM: if active, route raw bytes to the transfer.
                    {
                        let mut zm = zmodem_state_reader.lock().unwrap();
                        if let Some(ref mut transfer) = *zm {
                            let actions = transfer.feed_incoming(raw);
                            for action in actions {
                                match action {
                                    ZmodemAction::SendToRemote(data) => {
                                        let _ = zmodem_out_tx.send(data);
                                    }
                                    ZmodemAction::EmitEvent(event) => {
                                        let _ = app_read.emit(&zmodem_event_reader, &event);
                                    }
                                }
                            }
                            if transfer.is_done() {
                                *zm = None;
                                zmodem_detector.reset();
                            }
                            continue;
                        }
                    }

                    // ZMODEM: detect header in raw bytes.
                    let process_raw = if !suppress_visible {
                        match zmodem_detector.feed(raw) {
                            ZmodemDetectResult::Detected {
                                direction,
                                passthrough,
                                initial_bytes,
                            } => {
                                if !passthrough.is_empty() {
                                    if let Some(rec) = recording_mgr_reader.as_ref() {
                                        rec.write_raw_output(&sid_for_rec_reader, &passthrough);
                                    }
                                    let pre = output_decoder.decode(&passthrough);
                                    if !pre.is_empty() {
                                        output_reader.push_owned(pre);
                                    }
                                }
                                let prepared_upload = if direction == ZmodemDirection::Upload {
                                    rt_for_reader.block_on(async {
                                        manager_reader.take_pending_zmodem_upload(&sid_read).await
                                    })
                                } else {
                                    None
                                };
                                let (transfer, bootstrap_actions) = start_zmodem_transfer(
                                    direction,
                                    &initial_bytes,
                                    prepared_upload,
                                );
                                for action in bootstrap_actions {
                                    match action {
                                        ZmodemAction::SendToRemote(data) => {
                                            let _ = zmodem_out_tx.send(data);
                                        }
                                        ZmodemAction::EmitEvent(event) => {
                                            let _ = app_read.emit(&zmodem_event_reader, &event);
                                        }
                                    }
                                }
                                *zmodem_state_reader.lock().unwrap() = Some(transfer);
                                let _ = app_read.emit(
                                    &zmodem_event_reader,
                                    &ZmodemEvent::Detected { direction },
                                );
                                continue;
                            }
                            ZmodemDetectResult::NoMatch { passthrough } => {
                                if passthrough.is_empty() {
                                    continue;
                                }
                                if let Some(rec) = recording_mgr_reader.as_ref() {
                                    rec.write_raw_output(&sid_for_rec_reader, &passthrough);
                                }
                                passthrough
                            }
                        }
                    } else {
                        raw.to_vec()
                    };

                    let text = output_decoder.decode(&process_raw);
                    let mut result = stripper.push(&text);

                    for path in &result.cwd_paths {
                        let cwd_ev = cwd_event.clone();
                        let app_ref = app_read.clone();
                        let next_cwd = rt_for_reader
                            .block_on(async { update_cwd_if_changed(&cwd, path).await });
                        if let Some(next_cwd) = next_cwd {
                            let _ = app_ref.emit(&cwd_ev, &next_cwd);
                        }
                    }

                    for command in &result.accepted_commands {
                        rt_for_reader.block_on(
                            manager_reader
                                .confirm_command_submission(&sid_for_rec_reader, command.clone()),
                        );
                        let _ = app_read.emit(
                            "session-command-accepted",
                            serde_json::json!({
                                "sessionId": &sid_for_rec_reader,
                                "command": command,
                            }),
                        );
                    }

                    if !should_emit_visible_output(&mut suppress_visible, result.ready) {
                        continue;
                    }

                    if let Ok(mut proc) = capture_for_reader.lock() {
                        if proc.has_active() {
                            result.visible = proc.process(&result.visible);
                        }
                    }

                    if !result.visible.is_empty() {
                        if let Some(rec) = recording_mgr_reader.as_ref() {
                            rec.write_output(&sid_for_rec_reader, &result.visible);
                        }
                        output_reader.push_owned(result.visible);
                    }
                }
                Err(error) => {
                    tracing::debug!(
                        session_id = %sid_read,
                        error = %error,
                        "Local PTY reader exited"
                    );
                    break;
                }
            }
        }
        output_reader.close();
        let _ = reader_done_tx.send(());
    });

    let recording_mgr: Option<Arc<RecordingManager>> = app
        .try_state::<Arc<RecordingManager>>()
        .map(|s| s.inner().clone());
    if let Some(script) = startup_script.as_deref() {
        if let Err(error) = write_to_pty(&mut *writer, script.as_bytes()) {
            tracing::warn!(
                session_id = %session_id,
                error = %error,
                "Failed to write local PTY startup script"
            );
        }
    }
    loop {
        match reader_done_rx.try_recv() {
            Ok(()) | Err(std_mpsc::TryRecvError::Disconnected) => {
                tracing::debug!(
                    session_id = %session_id,
                    "Local PTY reader signalled session completion"
                );
                break;
            }
            Err(std_mpsc::TryRecvError::Empty) => {}
        }

        match child.try_wait() {
            Ok(Some(status)) => {
                tracing::info!(
                    session_id = %session_id,
                    exit_status = ?status,
                    "Local PTY child exited"
                );
                break;
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(
                    session_id = %session_id,
                    error = %error,
                    "Failed to query local PTY child status; closing session"
                );
                break;
            }
        }

        // Drain any ZMODEM outgoing data first (non-blocking).
        while let Ok(data) = zmodem_out_rx.try_recv() {
            let _ = write_to_pty(&mut *writer, &data);
        }

        let cmd = match cmd_rx.try_recv() {
            Ok(cmd) => cmd,
            Err(mpsc::error::TryRecvError::Disconnected) => break,
            Err(mpsc::error::TryRecvError::Empty) => {
                std::thread::sleep(Duration::from_millis(20));
                continue;
            }
        };
        match cmd {
            SessionCommand::Attach => {
                output.attach();
            }
            SessionCommand::DetachRenderer => {
                output.detach();
            }
            SessionCommand::Write { data, .. } => {
                if zmodem_state.lock().unwrap().is_some() {
                    continue;
                }
                let send_data = encode_terminal_input(&data, &encoding);
                if let Err(error) = write_to_pty(&mut *writer, &send_data) {
                    tracing::warn!(
                        session_id = %session_id,
                        error = %error,
                        "Failed to write to local PTY"
                    );
                }
            }
            SessionCommand::CaptureExec {
                marker_id,
                wrapped_command,
                result_tx,
            } => {
                if let Ok(mut proc) = capture_processor.lock() {
                    proc.register(marker_id, result_tx);
                }
                let send_command = encode_terminal_input(&wrapped_command, &encoding);
                if let Err(error) = write_to_pty(&mut *writer, &send_command) {
                    tracing::warn!(
                        session_id = %session_id,
                        error = %error,
                        "Failed to write capture command to local PTY"
                    );
                }
            }
            SessionCommand::CancelCapture { marker_id } => {
                if let Ok(mut proc) = capture_processor.lock() {
                    proc.cancel(&marker_id);
                }
            }
            SessionCommand::Resize { cols, rows } => {
                let _ = master.resize(PtySize {
                    rows: rows as u16,
                    cols: cols as u16,
                    pixel_width: 0,
                    pixel_height: 0,
                });
            }
            SessionCommand::PauseOutput => {
                let (lock, _) = &*output_pause;
                if let Ok(mut paused) = lock.lock() {
                    *paused = true;
                }
            }
            SessionCommand::ResumeOutput => {
                let (lock, cvar) = &*output_pause;
                if let Ok(mut paused) = lock.lock() {
                    *paused = false;
                    cvar.notify_all();
                }
            }
            SessionCommand::AckOutput { bytes } => {
                output.ack(bytes);
            }
            SessionCommand::ZmodemAcceptDownload { save_dir } => {
                let mut zm = zmodem_state.lock().unwrap();
                if let Some(ref mut transfer) = *zm {
                    let actions = transfer.accept_download(save_dir);
                    for action in actions {
                        match action {
                            ZmodemAction::SendToRemote(data) => {
                                let _ = write_to_pty(&mut *writer, &data);
                            }
                            ZmodemAction::EmitEvent(event) => {
                                let _ = app.emit(&zmodem_event_name, &event);
                            }
                        }
                    }
                    if transfer.is_done() {
                        *zm = None;
                    }
                }
            }
            SessionCommand::ZmodemAcceptUpload {
                files,
                conflict_mode,
                preserve_timestamps,
            } => {
                let mut zm = zmodem_state.lock().unwrap();
                if let Some(ref mut transfer) = *zm {
                    let actions =
                        transfer.accept_upload(files, conflict_mode, preserve_timestamps);
                    for action in actions {
                        match action {
                            ZmodemAction::SendToRemote(data) => {
                                let _ = write_to_pty(&mut *writer, &data);
                            }
                            ZmodemAction::EmitEvent(event) => {
                                let _ = app.emit(&zmodem_event_name, &event);
                            }
                        }
                    }
                    if transfer.is_done() {
                        *zm = None;
                    }
                }
            }
            SessionCommand::ZmodemCancel => {
                rt_handle.block_on(async {
                    manager.clear_pending_zmodem_upload(&session_id).await;
                });
                let mut zm = zmodem_state.lock().unwrap();
                if let Some(ref mut transfer) = *zm {
                    let actions = transfer.cancel();
                    for action in actions {
                        match action {
                            ZmodemAction::SendToRemote(data) => {
                                let _ = write_to_pty(&mut *writer, &data);
                            }
                            ZmodemAction::EmitEvent(event) => {
                                let _ = app.emit(&zmodem_event_name, &event);
                            }
                        }
                    }
                }
                *zm = None;
            }
            SessionCommand::Close => {
                break;
            }
        }
    }

    {
        let (lock, cvar) = &*output_pause;
        if let Ok(mut paused) = lock.lock() {
            *paused = false;
            cvar.notify_all();
        }
    }

    drop(writer);
    drop(master);
    let _ = reader_done_rx.recv_timeout(Duration::from_millis(250));
    output.close();

    if let Some(ref rec) = recording_mgr {
        rec.cleanup_session(&session_id);
    }

    rt_handle.block_on(async {
        manager.remove_session(&session_id).await;
    });
    let _ = app.emit(&format!("session-closed-{}", session_id), ());
}

fn validate_working_dir_before_spawn(config: Option<&LocalSessionConfig>) -> AppResult<()> {
    let Some(cfg) = config else {
        return Ok(());
    };
    if !cfg.fail_on_missing_working_dir {
        return Ok(());
    }
    let Some(dir) = cfg.working_dir.as_deref() else {
        return Ok(());
    };
    if dir.trim().is_empty() {
        return Err(crate::error::AppError::Config(
            "Working directory is empty.".to_string(),
        ));
    }

    let (mut cmd, _) = match cfg {
        LocalSessionConfig {
            shell_path,
            shell_args,
            ..
        } if !shell_path.trim().is_empty() => {
            build_shell_command(shell_path, shell_args).map_err(crate::error::AppError::Config)?
        }
        _ => platform_default_shell(),
    };

    if apply_working_dir_to_command(&mut cmd, dir) == WorkingDirectoryOutcome::MissingLocalDirectory
    {
        return Err(crate::error::AppError::Config(format!(
            "Working directory '{}' does not exist.",
            dir
        )));
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkingDirectoryOutcome {
    Skipped,
    AppliedLocalCwd,
    AppliedWslCd,
    WslCdAlreadyConfigured,
    MissingLocalDirectory,
}

fn apply_working_dir_to_command(
    cmd: &mut CommandBuilder,
    working_dir: &str,
) -> WorkingDirectoryOutcome {
    let working_dir = working_dir.trim();
    if working_dir.is_empty() {
        return WorkingDirectoryOutcome::Skipped;
    }

    if command_uses_wsl(cmd) {
        if command_has_wsl_cd_arg(cmd) {
            return WorkingDirectoryOutcome::WslCdAlreadyConfigured;
        }

        let argv = cmd.get_argv_mut();
        argv.insert(1, working_dir.into());
        argv.insert(1, "--cd".into());
        return WorkingDirectoryOutcome::AppliedWslCd;
    }

    if Path::new(working_dir).is_dir() {
        cmd.cwd(working_dir);
        WorkingDirectoryOutcome::AppliedLocalCwd
    } else {
        WorkingDirectoryOutcome::MissingLocalDirectory
    }
}

fn command_uses_wsl(cmd: &CommandBuilder) -> bool {
    let Some(program) = cmd.get_argv().first() else {
        return false;
    };

    let program = program.to_string_lossy();
    let normalized = program.replace('\\', "/");
    let file_name = normalized
        .rsplit('/')
        .next()
        .unwrap_or(normalized.as_str())
        .to_ascii_lowercase();

    matches!(file_name.as_str(), "wsl" | "wsl.exe")
}

fn command_has_wsl_cd_arg(cmd: &CommandBuilder) -> bool {
    cmd.get_argv()
        .iter()
        .skip(1)
        .any(|arg| {
            let arg = arg.to_string_lossy();
            arg == "--cd" || arg.starts_with("--cd=")
        })
}
