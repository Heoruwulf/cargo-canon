//! Unix domain socket IPC server.

use crate::error::DaemonError;
use crate::hook::execute_post_read_hook;
use crate::protocol::{
    DaemonRequest, DaemonResponse, DaemonStatusInfo, read_message, write_message,
};
use canon_slice::{SymbolIndex, slice_target};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::RwLock;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

/// Lock-free context bloat reduction telemetry counters.
#[derive(Debug, Default)]
pub struct ServerTelemetry {
    pub hooks_served: AtomicU64,
    pub raw_bytes_served: AtomicU64,
    pub delivered_bytes_served: AtomicU64,
    pub bytes_saved: AtomicU64,
}

impl ServerTelemetry {
    /// Records reduction metrics for a single request and returns the per-request reduction percentage.
    pub fn record_reduction(&self, raw_bytes: u64, delivered_bytes: u64) -> f64 {
        self.hooks_served.fetch_add(1, Ordering::Relaxed);
        self.raw_bytes_served
            .fetch_add(raw_bytes, Ordering::Relaxed);
        self.delivered_bytes_served
            .fetch_add(delivered_bytes, Ordering::Relaxed);
        let saved = raw_bytes.saturating_sub(delivered_bytes);
        self.bytes_saved.fetch_add(saved, Ordering::Relaxed);
        if raw_bytes > 0 {
            (saved as f64 / raw_bytes as f64) * 100.0
        } else {
            0.0
        }
    }

    /// Returns a snapshot of cumulative telemetry:
    /// (hooks_served, raw_bytes, delivered_bytes, bytes_saved, reduction_pct, est_tokens_saved).
    pub fn snapshot(&self) -> (u64, u64, u64, u64, f64, u64) {
        let hooks = self.hooks_served.load(Ordering::Relaxed);
        let raw = self.raw_bytes_served.load(Ordering::Relaxed);
        let delivered = self.delivered_bytes_served.load(Ordering::Relaxed);
        let saved = self.bytes_saved.load(Ordering::Relaxed);
        let pct = if raw > 0 {
            (saved as f64 / raw as f64) * 100.0
        } else {
            0.0
        };
        let est_tokens = saved / 4;
        (hooks, raw, delivered, saved, pct, est_tokens)
    }
}

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
    telemetry: Arc<ServerTelemetry>,
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
        telemetry: Arc::new(ServerTelemetry::default()),
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

async fn handle_post_read_request(
    file: &Path,
    req_start: std::time::Instant,
    state: &ServerState,
) -> DaemonResponse {
    let guard = state.index.read().await;
    match execute_post_read_hook(file, Some(&guard)) {
        Ok(advisory_block) => {
            let elapsed_us = req_start.elapsed().as_micros();
            let raw_len = std::fs::metadata(file).map(|m| m.len()).unwrap_or(0);
            let delivered_len = advisory_block.len() as u64;
            let pct = state.telemetry.record_reduction(raw_len, delivered_len);
            tracing::debug!(
                file = %file.display(),
                raw_bytes = raw_len,
                delivered_bytes = delivered_len,
                reduction_pct = format_args!("{:.1}%", pct),
                elapsed_us,
                "Served post-read hook"
            );
            DaemonResponse::AdvisoryContext {
                file: file.display().to_string(),
                advisory_block,
            }
        }
        Err(e) => DaemonResponse::Error {
            message: e.to_string(),
        },
    }
}

async fn handle_slice_request(
    file: &Path,
    mode: canon_core::model::SliceMode,
    format: canon_core::output::OutputFormat,
    state: &ServerState,
) -> DaemonResponse {
    let req_start = std::time::Instant::now();
    let guard = state.index.read().await;
    match slice_target(file, mode, Some(&guard)) {
        Ok(bundle) => {
            let elapsed_us = req_start.elapsed().as_micros();
            let content = bundle.format_with(format).unwrap_or_default();
            let raw_len = std::fs::metadata(file).map(|m| m.len()).unwrap_or(0);
            let delivered_len = content.len() as u64;
            let pct = state.telemetry.record_reduction(raw_len, delivered_len);
            tracing::debug!(
                file = %file.display(),
                raw_bytes = raw_len,
                delivered_bytes = delivered_len,
                reduction_pct = format_args!("{:.1}%", pct),
                elapsed_us,
                "Served slice request"
            );
            DaemonResponse::SliceResult { content }
        }
        Err(e) => DaemonResponse::Error {
            message: e.to_string(),
        },
    }
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
                let (hooks, raw, delivered, saved, pct, est_tokens) = state.telemetry.snapshot();
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
                    hooks_served: hooks,
                    raw_bytes_served: raw,
                    delivered_bytes_served: delivered,
                    bytes_saved: saved,
                    token_reduction_pct: pct,
                    est_tokens_saved: est_tokens,
                };
                write_message(&mut stream, &DaemonResponse::Status(status_info)).await?;
            }
            DaemonRequest::PostReadHook { file } => {
                let resp = handle_post_read_request(&file, req_start, &state).await;
                write_message(&mut stream, &resp).await?;
            }
            DaemonRequest::Slice { file, mode, format } => {
                let resp = handle_slice_request(&file, mode, format, &state).await;
                write_message(&mut stream, &resp).await?;
            }
            DaemonRequest::Shutdown => {
                let (hooks, raw, delivered, saved, pct, est_tokens) = state.telemetry.snapshot();
                tracing::info!(
                    hooks_served = hooks,
                    raw_bytes = raw,
                    delivered_bytes = delivered,
                    bytes_saved = saved,
                    reduction_pct = format_args!("{:.1}%", pct),
                    est_tokens_saved = est_tokens,
                    "Daemon stopping: cumulative context bloat reduction summary"
                );
                write_message(&mut stream, &DaemonResponse::ShutdownAck).await?;
                state.token.cancel();
                break;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_telemetry_reduction_and_bounds() {
        let telemetry = ServerTelemetry::default();
        let pct = telemetry.record_reduction(1000, 200);
        assert!((pct - 80.0).abs() < 0.001);

        // Edge case: delivered > raw (saturating_sub prevents underflow)
        let pct2 = telemetry.record_reduction(50, 100);
        assert_eq!(pct2, 0.0);

        let (hooks, raw, delivered, saved, cumulative_pct, tokens) = telemetry.snapshot();
        assert_eq!(hooks, 2);
        assert_eq!(raw, 1050);
        assert_eq!(delivered, 300);
        assert_eq!(saved, 800);
        assert!((cumulative_pct - (800.0 / 1050.0 * 100.0)).abs() < 0.001);
        assert_eq!(tokens, 200);
    }

    #[tokio::test]
    async fn test_server_telemetry_concurrent_updates() {
        let telemetry = Arc::new(ServerTelemetry::default());
        let mut handles = Vec::new();

        for _ in 0..16 {
            let t = Arc::clone(&telemetry);
            handles.push(tokio::spawn(async move {
                for _ in 0..100 {
                    t.record_reduction(100, 20);
                }
            }));
        }

        for h in handles {
            h.await.unwrap();
        }

        let (hooks, raw, delivered, saved, pct, tokens) = telemetry.snapshot();
        assert_eq!(hooks, 1600);
        assert_eq!(raw, 160_000);
        assert_eq!(delivered, 32_000);
        assert_eq!(saved, 128_000);
        assert!((pct - 80.0).abs() < 0.001);
        assert_eq!(tokens, 32_000);
    }
}
