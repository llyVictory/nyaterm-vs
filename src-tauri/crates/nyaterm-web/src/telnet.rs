use crate::{
    error::{Result, WebError},
    session::{self, SessionMetadata, SessionProtocol, WebSession},
    state::State,
};
use nyaterm_core::{
    config,
    ssh::terminal::{Command, Output},
    telnet::*,
    terminal_encoding::*,
};
use serde_json::Value;
use std::{
    sync::{Arc, atomic::Ordering},
    time::Instant,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::mpsc,
};

pub async fn create(state: Arc<State>, owner: &str, args: Value) -> Result<String> {
    let (conn, saved) = session::connection(&args, "telnet")?;
    let config::ConnectionType::Telnet {
        host,
        port,
        username,
        backspace_mode,
        raw_tcp_cli,
        enter_mode,
        local_echo,
        local_line_edit,
        force_character_at_a_time,
        send_naws,
        send_sga,
        auto_login,
        encoding,
        ..
    } = conn.config
    else {
        return Err(WebError::bad("Connection is not Telnet"));
    };
    let account = conn
        .auth
        .as_ref()
        .map(|a| config::load_saved_account(&(), a.account_id.as_deref(), a.password_id.as_deref()))
        .transpose()?
        .flatten();
    let username = config::resolve_account_username(account.as_ref(), &username);
    let password = match conn.auth.as_ref() {
        Some(a) if a.mode == "password" => {
            if saved {
                match &a.password {
                    Some(secret) => Some(nyaterm_core::utils::crypto::decrypt(secret)?),
                    None if a.password_source.as_deref() != Some("connection") => {
                        config::decrypt_account_password(account.as_ref())?
                    }
                    _ => None,
                }
            } else {
                match &a.password {
                    Some(password) => Some(password.clone()),
                    None => config::decrypt_account_password(account.as_ref())?,
                }
            }
        }
        _ => None,
    };
    let encoding = if encoding.is_empty() || encoding.eq_ignore_ascii_case("global") {
        config::load_app_settings(&())?.interaction.default_encoding
    } else {
        encoding
    };
    let config = TelnetSessionConfig {
        network: conn.network.clone(),
        host: host.clone(),
        port,
        name: conn.name.clone(),
        username: username.clone(),
        password,
        backspace_mode,
        raw_tcp_cli,
        enter_mode: TelnetEnterMode::from_config_value(&enter_mode),
        local_echo,
        local_line_edit,
        force_character_at_a_time,
        send_naws,
        send_sga,
        auto_login: auto_login.into(),
        encoding,
    };
    let cancel = state.login(owner).await?.cancel.child_token();
    let (session, commands, sender) = session::register(
        &state,
        owner,
        &args,
        SessionProtocol::Telnet,
        SessionMetadata {
            host,
            port,
            username,
            name: conn.name,
            connection_id: saved.then_some(conn.id),
            sftp: None,
        },
        cancel,
    )
    .await?;
    let id = session.id.clone();
    let startup = args["startupCommand"].clone();
    tokio::spawn(async move {
        let result = async {
            let transport = crate::network::open(
                state.clone(),
                session.owner.clone(),
                session.cancel.clone(),
                session.host.clone(),
                session.port,
                conn.network,
            )
            .await?;
            session.ready.store(true, Ordering::Release);
            if let Some(id) = &session.connection_id {
                nyaterm_core::storage::mark_connection_used(id)?;
            }
            run(
                transport.stream,
                config,
                commands,
                sender.clone(),
                &session,
                startup,
            )
            .await
        }
        .await;
        let end = if let Err(e) = result {
            state
                .event(
                    &session.owner,
                    &format!("connection-error-{}", session.id),
                    serde_json::json!({"sessionId":session.id,"error":e.1}),
                )
                .await;
            Output::Failure(e.1)
        } else {
            Output::Closed
        };
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), sender.send(end)).await;
        session::finish(&state, &session).await;
    });
    Ok(id)
}
async fn run(
    mut stream: nyaterm_core::network::BoxedTransportStream,
    config: TelnetSessionConfig,
    mut input: mpsc::Receiver<Command>,
    output: mpsc::Sender<Output>,
    session: &Arc<WebSession>,
    startup: Value,
) -> Result<()> {
    let mut decoder = TelnetDecoder::default();
    let mut text_decoder = TerminalOutputDecoder::new(&config.encoding);
    let mut editor = TelnetLineEditor::default();
    let mut auto_login = TelnetAutoLogin::new(
        config.auto_login.clone(),
        TelnetAutoLoginCredentials {
            username: config.username.clone(),
            password: config.password.clone(),
        },
        config.enter_mode,
        Instant::now(),
    );
    let mut buf = [0u8; 16 * 1024];
    let mut startup_input = startup["command"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .map(|command| normalize_enter_bytes(format!("{command}\r").as_bytes(), config.enter_mode));
    let startup_delay = std::time::Duration::from_millis(
        startup["delayMs"]
            .as_u64()
            .or(startup["delay_ms"].as_u64())
            .unwrap_or(0)
            .min(300_000),
    );
    let mut startup_deadline = if auto_login.is_none() && startup_input.is_some() {
        Some(tokio::time::Instant::now() + startup_delay)
    } else {
        None
    };
    let mut login_tick = tokio::time::interval(std::time::Duration::from_millis(250));
    let transfer = async {
        loop {
            tokio::select! {
                _ = async { tokio::time::sleep_until(startup_deadline.unwrap()).await }, if startup_deadline.is_some() => {
                    startup_deadline = None;
                    if let Some(data) = startup_input.take() {
                        stream.write_all(&escape_telnet_application_data(&encode_terminal_input(&data, &config.encoding), config.raw_tcp_cli)).await.map_err(|_| WebError::bad("Telnet startup write failed"))?;
                    }
                },
                _ = login_tick.tick() => {
                    if let Some(login) = &mut auto_login {
                        if login.tick(Instant::now()).is_some() { startup_input = None; startup_deadline = None; }
                    }
                },
                read = stream.read(&mut buf) => {
                    let n = read.map_err(|_| WebError::bad("Telnet read failed"))?;
                    if n == 0 { break; }
                    let mut replies = Vec::new();
                    let visible = if config.raw_tcp_cli { buf[..n].to_vec() } else { decoder.decode(&buf[..n], &mut |command, option| {
                        replies.extend(negotiate_response(command, option, config.send_naws, config.send_sga));
                    }) };
                    if !replies.is_empty() { stream.write_all(&replies).await.map_err(|_| WebError::bad("Telnet negotiation failed"))?; }
                    let text = text_decoder.decode(&visible);
                    if let Some(login) = &mut auto_login {
                        for action in login.handle_text(&text, Instant::now()) {
                            match action {
                                TelnetAutoLoginAction::Send(bytes) => stream.write_all(&escape_telnet_application_data(&encode_terminal_input(&bytes, &config.encoding), config.raw_tcp_cli)).await.map_err(|_| WebError::bad("Telnet login failed"))?,
                                TelnetAutoLoginAction::Complete => if startup_input.is_some() { startup_deadline = Some(tokio::time::Instant::now() + startup_delay); },
                                TelnetAutoLoginAction::Disable => { startup_input = None; startup_deadline = None; },
                            }
                        }
                    }
                    if !text.is_empty() { output.send(Output::Data(text.into_bytes())).await.map_err(|_| WebError::bad("Terminal detached"))?; }
                },
                command = input.recv() => match command {
                    Some(Command::Input(mut bytes)) => {
                        if let Some(login) = &mut auto_login {
                            if login.handle_user_input(false).is_some() { startup_input = None; startup_deadline = None; }
                        }
                        let chunks = if config.raw_tcp_cli && config.local_line_edit {
                            let edited = editor.process(&bytes, config.enter_mode);
                            if !edited.display.is_empty() { output.send(Output::Data(edited.display.into_bytes())).await.map_err(|_| WebError::bad("Terminal detached"))?; }
                            edited.writes
                        } else {
                            if config.backspace_mode == "ctrl_h" { for byte in &mut bytes { if *byte == 0x7f { *byte = 8; } } }
                            let data = normalize_enter_bytes(&bytes, config.enter_mode);
                            if config.local_echo { let text = local_echo_text(&data); if !text.is_empty() { output.send(Output::Data(text.into_bytes())).await.map_err(|_| WebError::bad("Terminal detached"))?; } }
                            split_write_chunks(&data, config.force_character_at_a_time)
                        };
                        for chunk in chunks { stream.write_all(&escape_telnet_application_data(&encode_terminal_input(&chunk, &config.encoding), config.raw_tcp_cli)).await.map_err(|_| WebError::bad("Telnet write failed"))?; }
                    },
                    Some(Command::RawInput(bytes)) => {
                        if let Some(login) = &mut auto_login { if login.handle_user_input(false).is_some() { startup_input = None; startup_deadline = None; } }
                        stream.write_all(&escape_telnet_application_data(&bytes, config.raw_tcp_cli)).await.map_err(|_| WebError::bad("Telnet write failed"))?;
                    },
                    Some(Command::Resize(cols, rows)) => if let Some(bytes) = maybe_build_naws(cols as u16, rows as u16, &config) { stream.write_all(&bytes).await.map_err(|_| WebError::bad("Telnet resize failed"))?; },
                    Some(Command::Close) | None => break,
                }
            }
        }
        Ok(())
    };
    tokio::select! { _ = session.cancel.cancelled() => Ok(()), result = transfer => result }
}
