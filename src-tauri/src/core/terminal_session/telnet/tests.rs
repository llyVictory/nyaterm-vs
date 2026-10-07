#[cfg(test)]
mod tests {
    use super::await_telnet_connection;
    use crate::error::AppError;
    use tokio::sync::oneshot;
    #[tokio::test]
    async fn connection_wait_can_be_cancelled_before_connect_finishes() {
        let (cancel_tx, cancel_rx) = oneshot::channel();
        cancel_tx.send(()).expect("send cancellation");

        let result = await_telnet_connection(
            std::future::pending::<std::io::Result<()>>(),
            Some(cancel_rx),
        )
        .await;

        assert!(matches!(
            result,
            Err(AppError::Cancelled(message)) if message == "Session creation cancelled"
        ));
    }

    #[tokio::test]
    async fn connection_wait_returns_connect_result_without_cancellation() {
        let result =
            await_telnet_connection(std::future::ready(Ok::<_, std::io::Error>(42_u8)), None)
                .await
                .expect("connect result");

        assert_eq!(result, 42);
    }
}
