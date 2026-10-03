use crate::error::{AppError, AppResult};
use russh::{ChannelMsg, client};
use std::{sync::Arc, time::Duration};
use tokio::time::timeout;

pub async fn connect<H: client::Handler + Send + 'static>(
    config: Arc<client::Config>,
    host: &str,
    port: u16,
    handler: H,
) -> Result<client::Handle<H>, H::Error> {
    client::connect(config, (host, port), handler).await
}

pub async fn password<H: client::Handler>(
    handle: &mut client::Handle<H>,
    username: &str,
    secret: &str,
) -> Result<client::AuthResult, russh::Error> {
    handle.authenticate_password(username, secret).await
}

/// Preserve the existing Desktop request/reply timeout and rejection semantics.
pub async fn wait_channel_request_reply(
    channel: &mut russh::Channel<client::Msg>,
    request_name: &str,
) -> AppResult<()> {
    timeout(Duration::from_secs(10), async {
        loop {
            match channel.wait().await {
                Some(ChannelMsg::Success) => return Ok(()),
                Some(ChannelMsg::Failure) => {
                    return Err(AppError::Channel(format!(
                        "{request_name} request rejected by server"
                    )));
                }
                Some(ChannelMsg::Close | ChannelMsg::Eof) | None => {
                    return Err(AppError::Channel(format!(
                        "SSH channel closed before {request_name} request completed"
                    )));
                }
                Some(_) => {}
            }
        }
    })
    .await
    .map_err(|_| AppError::Channel(format!("{request_name} request timed out")))?
}

pub async fn initialize_sftp<S>(
    stream: S,
    config: russh_sftp::client::Config,
) -> Result<russh_sftp::client::SftpSession, russh_sftp::client::error::Error>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    russh_sftp::client::SftpSession::new_with_config(stream, config).await
}

pub async fn open_shell<H: client::Handler>(
    handle: &mut client::Handle<H>,
    cols: u32,
    rows: u32,
) -> AppResult<russh::Channel<client::Msg>> {
    open_shell_with_terminal(handle, cols, rows, "xterm-256color").await
}

pub async fn open_shell_with_terminal<H: client::Handler>(
    handle: &mut client::Handle<H>,
    cols: u32,
    rows: u32,
    terminal: &str,
) -> AppResult<russh::Channel<client::Msg>> {
    let mut channel = handle.channel_open_session().await?;
    let result = async {
        channel
            .request_pty(true, terminal, cols, rows, 0, 0, &[])
            .await?;
        wait_channel_request_reply(&mut channel, "PTY").await?;
        channel.request_shell(true).await?;
        wait_channel_request_reply(&mut channel, "Shell").await
    }
    .await;
    if let Err(error) = result {
        let _ = channel.close().await;
        return Err(error);
    }
    Ok(channel)
}

pub async fn open_sftp<H: client::Handler>(
    handle: &mut client::Handle<H>,
) -> AppResult<russh_sftp::client::SftpSession> {
    open_sftp_with_config(handle, Default::default()).await
}

pub async fn open_sftp_with_config<H: client::Handler>(
    handle: &mut client::Handle<H>,
    config: russh_sftp::client::Config,
) -> AppResult<russh_sftp::client::SftpSession> {
    let mut channel = handle.channel_open_session().await?;
    channel.request_subsystem(true, "sftp").await?;
    wait_channel_request_reply(&mut channel, "SFTP").await?;
    Ok(initialize_sftp(channel.into_stream(), config).await?)
}

pub async fn keyboard_start<H: client::Handler>(
    handle: &mut client::Handle<H>,
    username: &str,
) -> Result<client::KeyboardInteractiveAuthResponse, russh::Error> {
    handle
        .authenticate_keyboard_interactive_start(username, None)
        .await
}
pub async fn keyboard_respond<H: client::Handler>(
    handle: &mut client::Handle<H>,
    responses: Vec<String>,
) -> Result<client::KeyboardInteractiveAuthResponse, russh::Error> {
    handle
        .authenticate_keyboard_interactive_respond(responses)
        .await
}
