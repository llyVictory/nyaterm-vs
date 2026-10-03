use russh::{Channel, ChannelMsg, client};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub enum Command {
    Input(Vec<u8>),
    Resize(u32, u32),
    Close,
}
pub enum Output {
    Data(Vec<u8>),
    Closed,
    Error,
}
pub const CHUNK_BYTES: usize = 64 * 1024;
pub const OUTPUT_CAPACITY: usize = 32;

pub async fn write(channel: &Channel<client::Msg>, bytes: &[u8]) -> Result<(), russh::Error> {
    channel.data(bytes).await
}
pub async fn resize(
    channel: &Channel<client::Msg>,
    cols: u32,
    rows: u32,
) -> Result<(), russh::Error> {
    channel.window_change(cols, rows, 0, 0).await
}

/// Bounded output (at most 2 MiB); saturation backpressures the SSH channel.
/// Cancellation interrupts output waits and channel writes, including detached peers.
pub async fn run(
    mut channel: Channel<client::Msg>,
    mut input: mpsc::Receiver<Command>,
    output: mpsc::Sender<Output>,
    cancel: CancellationToken,
) {
    let transfer = async {
        loop {
            tokio::select! {
                command = input.recv() => match command {
                    Some(Command::Input(data)) => write(&channel, &data).await?,
                    Some(Command::Resize(cols, rows)) => resize(&channel, cols, rows).await?,
                    Some(Command::Close) | None => break,
                },
                message = channel.wait() => match message {
                    Some(ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. }) => {
                        for chunk in data.chunks(CHUNK_BYTES) {
                            if output.send(Output::Data(chunk.to_vec())).await.is_err() { return Ok::<(), russh::Error>(()); }
                        }
                    }
                    Some(ChannelMsg::Close | ChannelMsg::Eof) | None => break,
                    _ => {},
                }
            }
        }
        Ok::<(), russh::Error>(())
    };
    let failed =
        tokio::select! { _ = cancel.cancelled() => false, result = transfer => result.is_err() };
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), channel.close()).await;
    // Preserve ordering after buffered data. Sender drop also signals EOF if a
    // detached consumer cannot make room before the bounded shutdown deadline.
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        output.send(if failed {
            Output::Error
        } else {
            Output::Closed
        }),
    )
    .await;
}
