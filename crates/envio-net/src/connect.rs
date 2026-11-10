//! TCP connect and the timeouts every network step runs under.

use std::future::Future;
use std::time::Duration;

use tokio::net::TcpStream;

use crate::error::{NetError, Result};

/// How long a TCP connect may take.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
/// How long one command (handshake, login, fetch, send) may take.
pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(60);

pub(crate) async fn tcp(host: &str, port: u16) -> Result<TcpStream> {
    let stream = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect((host, port)))
        .await
        .map_err(|_| NetError::Timeout)??;
    stream.set_nodelay(true)?;
    Ok(stream)
}

/// Runs `step` under [`COMMAND_TIMEOUT`].
pub(crate) async fn within<T>(step: impl Future<Output = Result<T>>) -> Result<T> {
    within_for(COMMAND_TIMEOUT, step).await
}

pub(crate) async fn within_for<T>(
    limit: Duration,
    step: impl Future<Output = Result<T>>,
) -> Result<T> {
    tokio::time::timeout(limit, step)
        .await
        .map_err(|_| NetError::Timeout)?
}

/// How long sending a message of `len` bytes may take: the command time,
/// plus one second per 16 KiB, which allows for a 16 KiB/s uplink.
pub(crate) fn transfer_timeout(len: usize) -> Duration {
    COMMAND_TIMEOUT + Duration::from_secs((len / 16_384) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_step_that_overruns_is_a_timeout() {
        let slow = async {
            tokio::time::sleep(Duration::from_secs(5)).await;
            Ok(())
        };
        let err = within_for(Duration::from_millis(10), slow)
            .await
            .unwrap_err();
        assert!(matches!(err, NetError::Timeout));
        assert!(err.is_temporary());
    }

    #[test]
    fn an_empty_message_gets_the_command_time() {
        assert_eq!(transfer_timeout(0), COMMAND_TIMEOUT);
    }

    #[test]
    fn a_message_gets_a_second_per_16_kib_on_top() {
        assert_eq!(
            transfer_timeout(16_384 * 10),
            COMMAND_TIMEOUT + Duration::from_secs(10)
        );
    }

    #[tokio::test]
    async fn a_closed_port_is_a_temporary_io_error() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let err = tcp("127.0.0.1", port).await.unwrap_err();
        assert!(matches!(err, NetError::Io(_)), "{err:?}");
        assert!(err.is_temporary());
    }
}
