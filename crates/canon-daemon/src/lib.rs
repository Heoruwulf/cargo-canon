//! # canon-daemon
//!
//! Agent sidecar and background indexing daemon.
//!
//! Provides high-throughput Unix domain socket IPC for sub-millisecond
//! agent tool hook responses and non-destructive advisory context injection.

#![forbid(unsafe_code)]

pub mod error;
pub mod hook;
pub mod protocol;
pub mod server;
pub mod watcher;

pub use error::DaemonError;
pub use hook::execute_post_read_hook;
pub use protocol::{DaemonRequest, DaemonResponse, read_message, write_message};
pub use server::{SocketGuard, run_server};
pub use watcher::start_watcher;

use std::path::Path;
use tokio::net::UnixStream;

/// Sends a request to a running daemon over Unix domain socket.
pub async fn send_daemon_request(
    socket_path: &Path,
    request: DaemonRequest,
) -> Result<DaemonResponse, DaemonError> {
    let mut stream =
        UnixStream::connect(socket_path)
            .await
            .map_err(|source| DaemonError::Socket {
                path: socket_path.to_path_buf(),
                source,
            })?;

    write_message(&mut stream, &request).await?;
    let response = read_message(&mut stream).await?;
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use canon_slice::SymbolIndex;
    use std::sync::Arc;
    use tempfile::NamedTempFile;
    use tokio::sync::RwLock;
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn test_daemon_ping_pong() {
        let tmp = NamedTempFile::new().unwrap();
        let socket_path = tmp.path().to_path_buf();
        // Remove temp file so UnixListener can bind
        drop(tmp);

        let index = Arc::new(RwLock::new(SymbolIndex::new()));
        let token = CancellationToken::new();

        let s_path = socket_path.clone();
        let s_token = token.clone();
        let server_handle = tokio::spawn(async move {
            let _ = run_server(&s_path, index, s_token).await;
        });

        // Wait briefly for socket creation
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let response = send_daemon_request(&socket_path, DaemonRequest::Ping)
            .await
            .unwrap();
        assert!(matches!(response, DaemonResponse::Pong));

        let shutdown_resp = send_daemon_request(&socket_path, DaemonRequest::Shutdown)
            .await
            .unwrap();
        assert!(matches!(shutdown_resp, DaemonResponse::ShutdownAck));

        let _ = server_handle.await;
    }
}
