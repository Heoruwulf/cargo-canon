//! Unix domain socket IPC server.

use crate::error::DaemonError;
use crate::hook::execute_post_read_hook;
use crate::protocol::{DaemonRequest, DaemonResponse, read_message, write_message};
use canon_slice::{SymbolIndex, slice_target};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

/// RAII Socket cleaner to remove socket file on drop.
pub struct SocketGuard {
    path: PathBuf,
}

impl SocketGuard {
    /// Creates a new guard tracking socket path.
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Drop for SocketGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Runs the IPC daemon server loop until shutdown token is cancelled.
pub async fn run_server(
    socket_path: &Path,
    index: Arc<RwLock<SymbolIndex>>,
    token: CancellationToken,
) -> Result<(), DaemonError> {
    if socket_path.exists() {
        let _ = std::fs::remove_file(socket_path);
    }

    let listener = UnixListener::bind(socket_path).map_err(|source| DaemonError::Socket {
        path: socket_path.to_path_buf(),
        source,
    })?;

    let _guard = SocketGuard::new(socket_path.to_path_buf());

    loop {
        tokio::select! {
            _ = token.cancelled() => {
                break;
            }
            res = listener.accept() => {
                match res {
                    Ok((stream, _addr)) => {
                        let idx = Arc::clone(&index);
                        let tok = token.clone();
                        tokio::spawn(async move {
                            let _ = handle_client(stream, idx, tok).await;
                        });
                    }
                    Err(e) => {
                        tracing::warn!("Failed accepting connection: {}", e);
                    }
                }
            }
        }
    }

    Ok(())
}

async fn handle_client(
    mut stream: UnixStream,
    index: Arc<RwLock<SymbolIndex>>,
    token: CancellationToken,
) -> Result<(), DaemonError> {
    loop {
        let req: DaemonRequest = match read_message(&mut stream).await {
            Ok(r) => r,
            Err(_) => break, // Client disconnected cleanly
        };

        match req {
            DaemonRequest::Ping => {
                write_message(&mut stream, &DaemonResponse::Pong).await?;
            }
            DaemonRequest::PostReadHook { file } => {
                let guard = index.read().await;
                match execute_post_read_hook(&file, Some(&guard)) {
                    Ok(advisory_block) => {
                        let resp = DaemonResponse::AdvisoryContext {
                            file: file.display().to_string(),
                            advisory_block,
                        };
                        write_message(&mut stream, &resp).await?;
                    }
                    Err(e) => {
                        let resp = DaemonResponse::Error {
                            message: e.to_string(),
                        };
                        write_message(&mut stream, &resp).await?;
                    }
                }
            }
            DaemonRequest::Slice { file, mode, format } => {
                let guard = index.read().await;
                match slice_target(&file, mode, Some(&guard)) {
                    Ok(bundle) => {
                        let content = bundle.format_with(format).unwrap_or_default();
                        let resp = DaemonResponse::SliceResult { content };
                        write_message(&mut stream, &resp).await?;
                    }
                    Err(e) => {
                        let resp = DaemonResponse::Error {
                            message: e.to_string(),
                        };
                        write_message(&mut stream, &resp).await?;
                    }
                }
            }
            DaemonRequest::Shutdown => {
                write_message(&mut stream, &DaemonResponse::ShutdownAck).await?;
                token.cancel();
                break;
            }
        }
    }
    Ok(())
}
