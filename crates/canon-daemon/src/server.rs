//! Unix domain socket IPC server.

use crate::error::DaemonError;
use crate::hook::execute_post_read_hook;
use crate::protocol::{
    DaemonRequest, DaemonResponse, DaemonStatusInfo, read_message, write_message,
};
use canon_slice::{SymbolIndex, slice_target};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::RwLock;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

/// Configuration options for launching the daemon server.
#[derive(Debug, Clone)]
pub struct ServerOptions {
    /// Bound socket path.
    pub socket_path: PathBuf,
    /// Monitored workspace root directory.
    pub workspace_root: PathBuf,
    /// Indicates whether the background file watcher is active.
    pub watcher_active: bool,
}

/// Internal shared state for connection handlers.
struct ServerState {
    options: Arc<ServerOptions>,
    start_time: std::time::Instant,
    index: Arc<RwLock<SymbolIndex>>,
    token: CancellationToken,
}

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

/// Probes whether an active daemon is responsive on the given socket path.
async fn probe_active_socket(socket_path: &Path) -> bool {
    let connect_fut = UnixStream::connect(socket_path);
    let Ok(Ok(mut stream)) = timeout(Duration::from_millis(50), connect_fut).await else {
        return false;
    };

    let ping_fut = async {
        write_message(&mut stream, &DaemonRequest::Ping).await?;
        let resp: DaemonResponse = read_message(&mut stream).await?;
        Ok::<_, DaemonError>(resp)
    };

    matches!(
        timeout(Duration::from_millis(50), ping_fut).await,
        Ok(Ok(DaemonResponse::Pong))
    )
}

/// Runs the IPC daemon server with explicit options until shutdown token is cancelled.
pub async fn run_server_with_options(
    options: ServerOptions,
    index: Arc<RwLock<SymbolIndex>>,
    token: CancellationToken,
) -> Result<(), DaemonError> {
    let socket_path = &options.socket_path;
    if socket_path.exists() {
        if probe_active_socket(socket_path).await {
            return Err(DaemonError::SocketAlreadyBound(socket_path.clone()));
        }
        let _ = std::fs::remove_file(socket_path);
    }

    let listener = UnixListener::bind(socket_path).map_err(|source| DaemonError::Socket {
        path: socket_path.clone(),
        source,
    })?;

    let _guard = SocketGuard::new(socket_path.clone());
    let start_time = std::time::Instant::now();
    let pid = std::process::id();
    tracing::info!(socket = %socket_path.display(), pid, "Starting canon daemon IPC server");

    let state = Arc::new(ServerState {
        options: Arc::new(options),
        start_time,
        index,
        token: token.clone(),
    });

    loop {
        tokio::select! {
            _ = token.cancelled() => {
                break;
            }
            res = listener.accept() => {
                match res {
                    Ok((stream, _addr)) => {
                        let st = Arc::clone(&state);
                        tracing::debug!(socket = %st.options.socket_path.display(), "Accepted new IPC connection");
                        tokio::spawn(async move {
                            let _ = handle_client(stream, st).await;
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

/// Runs the IPC daemon server loop until shutdown token is cancelled.
pub async fn run_server(
    socket_path: &Path,
    index: Arc<RwLock<SymbolIndex>>,
    token: CancellationToken,
) -> Result<(), DaemonError> {
    let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let opts = ServerOptions {
        socket_path: socket_path.to_path_buf(),
        workspace_root: root,
        watcher_active: false,
    };
    run_server_with_options(opts, index, token).await
}

async fn handle_client(mut stream: UnixStream, state: Arc<ServerState>) -> Result<(), DaemonError> {
    loop {
        let req: DaemonRequest = match read_message(&mut stream).await {
            Ok(r) => {
                tracing::debug!(req = ?r, "Received daemon request");
                r
            }
            Err(_) => {
                tracing::debug!("Client disconnected");
                break;
            }
        };

        let req_start = std::time::Instant::now();
        match req {
            DaemonRequest::Ping => {
                write_message(&mut stream, &DaemonResponse::Pong).await?;
            }
            DaemonRequest::Status => {
                let metrics = {
                    let guard = state.index.read().await;
                    guard.metrics()
                };
                let status_info = DaemonStatusInfo {
                    pid: std::process::id(),
                    uptime_secs: state.start_time.elapsed().as_secs(),
                    socket_path: state.options.socket_path.clone(),
                    workspace_root: state.options.workspace_root.clone(),
                    indexed_files: metrics.distinct_files_count,
                    total_symbols: metrics.total_symbols_count,
                    total_call_edges: metrics.total_call_edges_count,
                    total_contracts: metrics.total_contracts_count,
                    total_routes: metrics.total_routes_count,
                    watcher_active: state.options.watcher_active,
                };
                write_message(&mut stream, &DaemonResponse::Status(status_info)).await?;
            }
            DaemonRequest::PostReadHook { file } => {
                let guard = state.index.read().await;
                match execute_post_read_hook(&file, Some(&guard)) {
                    Ok(advisory_block) => {
                        let elapsed_us = req_start.elapsed().as_micros();
                        tracing::debug!(file = %file.display(), elapsed_us, "Served post-read hook");
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
                let guard = state.index.read().await;
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
                state.token.cancel();
                break;
            }
        }
    }
    Ok(())
}
