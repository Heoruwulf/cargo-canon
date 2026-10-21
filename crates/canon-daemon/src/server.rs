//! Unix domain socket IPC server.

use crate::error::DaemonError;
use crate::hook::{execute_post_read_hook, execute_post_read_hook_with_overlay};
use crate::logging::DaemonLogManager;
use crate::protocol::{
    DaemonRequest, DaemonResponse, DaemonStatusInfo, WorkspaceSummary, read_message, write_message,
};
use crate::registry::{WorkspaceEntry, WorkspaceRegistry};
use crate::watcher::DynamicWatcherManager;
use canon_slice::{SymbolIndex, slice_target, slice_target_with_overlay};
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
            ((raw_bytes as f64 - delivered_bytes as f64) / raw_bytes as f64) * 100.0
        } else {
            0.0
        }
    }

    /// Returns a snapshot of cumulative telemetry computed directly from `raw` vs `delivered`:
    /// (hooks_served, raw_bytes, delivered_bytes, bytes_saved, reduction_pct, est_tokens_saved).
    pub fn snapshot(&self) -> (u64, u64, u64, u64, f64, u64) {
        let hooks = self.hooks_served.load(Ordering::Relaxed);
        let raw = self.raw_bytes_served.load(Ordering::Relaxed);
        let delivered = self.delivered_bytes_served.load(Ordering::Relaxed);
        let saved = raw.saturating_sub(delivered);
        let pct = if raw > 0 {
            ((raw as f64 - delivered as f64) / raw as f64) * 100.0
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
    /// Bound socket path (if empty, defaults to the global system socket path).
    pub socket_path: PathBuf,
    /// Monitored workspace root directory.
    pub workspace_root: PathBuf,
    /// Indicates whether the background file watcher is active.
    pub watcher_active: bool,
    /// Directory for file logging (default: system state dir).
    pub log_dir: Option<PathBuf>,
    /// Inactivity threshold in seconds before unindexing an idle workspace (default: 7200).
    pub idle_ttl_secs: u64,
}

impl Default for ServerOptions {
    fn default() -> Self {
        Self {
            socket_path: PathBuf::new(),
            workspace_root: PathBuf::new(),
            watcher_active: false,
            log_dir: None,
            idle_ttl_secs: 7200,
        }
    }
}

/// Internal shared state for connection handlers.
struct ServerState {
    options: Arc<ServerOptions>,
    start_time: std::time::Instant,
    legacy_index: Arc<RwLock<SymbolIndex>>,
    registry: Arc<WorkspaceRegistry>,
    log_manager: Arc<DaemonLogManager>,
    watcher_manager: Option<Arc<DynamicWatcherManager>>,
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
    mut options: ServerOptions,
    index: Arc<RwLock<SymbolIndex>>,
    token: CancellationToken,
) -> Result<(), DaemonError> {
    if options.socket_path.as_os_str().is_empty() {
        options.socket_path = canon_core::workspace::resolve_global_socket();
    }
    let socket_path = &options.socket_path;
    if let Some(parent) = socket_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

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

    let log_dir = options
        .log_dir
        .clone()
        .unwrap_or_else(canon_core::logging::resolve_default_log_dir);
    let log_manager = Arc::new(DaemonLogManager::init(&log_dir)?);
    log_manager.log_daemon(&format!(
        "Canon daemon IPC server started: pid={}, socket='{}'",
        pid,
        socket_path.display()
    ));

    let registry = Arc::new(WorkspaceRegistry::new(Arc::clone(&log_manager)));

    let watcher_manager = if options.watcher_active {
        let mgr = Arc::new(DynamicWatcherManager::new(token.clone())?);
        Some(mgr)
    } else {
        None
    };

    let raw_root = &options.workspace_root;
    if !raw_root.as_os_str().is_empty() {
        let canonical_root = std::fs::canonicalize(raw_root).unwrap_or_else(|_| raw_root.clone());
        if let Ok(ws_logger) = log_manager.create_workspace_logger(&canonical_root) {
            let ws_entry = Arc::new(WorkspaceEntry::new(
                canonical_root.clone(),
                false,
                None,
                Arc::clone(&index),
                None,
                Arc::new(ws_logger),
            ));
            registry.register(ws_entry.clone()).await;
            if let Some(ref w) = watcher_manager {
                let _ = w.watch_workspace(
                    canonical_root,
                    Arc::clone(&index),
                    Arc::clone(&ws_entry.logger),
                );
            }
        }
    }

    let state = Arc::new(ServerState {
        options: Arc::new(options),
        start_time,
        legacy_index: index,
        registry: Arc::clone(&registry),
        log_manager,
        watcher_manager,
        token: token.clone(),
        telemetry: Arc::new(ServerTelemetry::default()),
    });

    let mut prune_interval = tokio::time::interval(Duration::from_secs(60));
    prune_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    prune_interval.tick().await;

    loop {
        tokio::select! {
            _ = token.cancelled() => {
                break;
            }
            _ = prune_interval.tick() => {
                let idle_ttl = Duration::from_secs(state.options.idle_ttl_secs);
                let pruned = state.registry.prune_idle_workspaces(idle_ttl).await;
                if let Some(ref watcher) = state.watcher_manager {
                    for p in &pruned {
                        let _ = watcher.unwatch_workspace(p);
                    }
                }
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
        log_dir: None,
        idle_ttl_secs: 7200,
    };
    run_server_with_options(opts, index, token).await
}

struct PostReadParams<'a> {
    file: &'a Path,
    start_line: Option<usize>,
    end_line: Option<usize>,
    raw_read_bytes: Option<u64>,
}

async fn handle_post_read_request(
    params: PostReadParams<'_>,
    req_start: std::time::Instant,
    state: &ServerState,
) -> DaemonResponse {
    let file = params.file;
    let entry = match state.registry.get_or_init(file).await {
        Ok(e) => {
            if let Some(ref watcher) = state.watcher_manager {
                if !watcher.is_watching(&e.root) {
                    let _ = watcher.watch_workspace(
                        e.root.clone(),
                        Arc::clone(&e.index),
                        Arc::clone(&e.logger),
                    );
                }
            }
            Some(e)
        }
        Err(e) => {
            tracing::debug!(
                file = %file.display(),
                error = %e,
                "Workspace could not be auto-initialized; falling back to default index"
            );
            None
        }
    };

    let raw_len = params
        .raw_read_bytes
        .unwrap_or_else(|| std::fs::metadata(file).map(|m| m.len()).unwrap_or(0));
    let advisory_res = if let Some(ref ws) = entry {
        if let Some(ref base) = ws.base_index {
            let base_guard = base.read().await;
            let ov_guard = ws.index.read().await;
            execute_post_read_hook_with_overlay(file, Some(&*base_guard), Some(&*ov_guard))
        } else {
            let guard = ws.index.read().await;
            execute_post_read_hook_with_overlay(file, Some(&*guard), None)
        }
    } else {
        let guard = state.legacy_index.read().await;
        execute_post_read_hook(file, Some(&*guard))
    };

    match advisory_res {
        Ok(mut advisory_block) => {
            let elapsed_us = req_start.elapsed().as_micros();
            let candidate_len = advisory_block.len() as u64;
            // Skip injecting full-file advisory context when a bounded read is smaller than the skeleton.
            let (delivered_len, skipped) =
                if params.raw_read_bytes.is_some() && raw_len > 0 && candidate_len >= raw_len {
                    advisory_block.clear();
                    (raw_len, true)
                } else {
                    (candidate_len, false)
                };
            let pct = state.telemetry.record_reduction(raw_len, delivered_len);
            let range_str = match (params.start_line, params.end_line) {
                (Some(s), Some(e)) => format!(" (lines {}..{})", s, e),
                (Some(s), None) => format!(" (lines {}..)", s),
                (None, Some(e)) => format!(" (lines ..{})", e),
                (None, None) => String::new(),
            };
            let action = if skipped {
                "PostReadHook skipped (micro-read < skeleton)"
            } else {
                "PostReadHook served"
            };
            let log_msg = format!(
                "{action}: '{}'{range_str} | raw: {} B | delivered: {} B | reduction: {:.1}% | latency: {:.1}ms",
                file.display(),
                raw_len,
                delivered_len,
                pct,
                elapsed_us as f64 / 1000.0,
            );
            state.log_manager.log_daemon(&log_msg);
            if let Some(ref ws) = entry {
                ws.logger.log(&log_msg);
            }
            DaemonResponse::AdvisoryContext {
                file: file.display().to_string(),
                advisory_block,
            }
        }
        Err(e) => {
            let err_msg = format!("PostReadHook failed for '{}': {}", file.display(), e);
            state.log_manager.log_daemon(&err_msg);
            if let Some(ref ws) = entry {
                ws.logger.log(&err_msg);
            }
            DaemonResponse::Error {
                message: e.to_string(),
            }
        }
    }
}

async fn handle_slice_request(
    file: &Path,
    mode: canon_core::model::SliceMode,
    format: canon_core::output::OutputFormat,
    state: &ServerState,
) -> DaemonResponse {
    let req_start = std::time::Instant::now();
    let entry = match state.registry.get_or_init(file).await {
        Ok(e) => {
            if let Some(ref watcher) = state.watcher_manager {
                if !watcher.is_watching(&e.root) {
                    let _ = watcher.watch_workspace(
                        e.root.clone(),
                        Arc::clone(&e.index),
                        Arc::clone(&e.logger),
                    );
                }
            }
            Some(e)
        }
        Err(e) => {
            tracing::debug!(
                file = %file.display(),
                error = %e,
                "Workspace could not be auto-initialized; falling back to default index"
            );
            None
        }
    };

    let raw_len = std::fs::metadata(file).map(|m| m.len()).unwrap_or(0);
    let slice_res = if let Some(ref ws) = entry {
        if let Some(ref base) = ws.base_index {
            let base_guard = base.read().await;
            let ov_guard = ws.index.read().await;
            slice_target_with_overlay(file, mode, Some(&*base_guard), Some(&*ov_guard))
        } else {
            let guard = ws.index.read().await;
            slice_target_with_overlay(file, mode, Some(&*guard), None)
        }
    } else {
        let guard = state.legacy_index.read().await;
        slice_target(file, mode, Some(&*guard))
    };

    match slice_res {
        Ok(bundle) => {
            let elapsed_us = req_start.elapsed().as_micros();
            let content = bundle.format_with(format).unwrap_or_default();
            let delivered_len = content.len() as u64;
            let pct = state.telemetry.record_reduction(raw_len, delivered_len);
            let log_msg = format!(
                "Slice served: '{}' (mode: {:?}) | raw: {} B | delivered: {} B | reduction: {:.1}% | latency: {:.1}ms",
                file.display(),
                mode,
                raw_len,
                delivered_len,
                pct,
                elapsed_us as f64 / 1000.0,
            );
            state.log_manager.log_daemon(&log_msg);
            if let Some(ref ws) = entry {
                ws.logger.log(&log_msg);
            }
            DaemonResponse::SliceResult { content }
        }
        Err(e) => {
            let err_msg = format!("Slice failed for '{}': {}", file.display(), e);
            state.log_manager.log_daemon(&err_msg);
            if let Some(ref ws) = entry {
                ws.logger.log(&err_msg);
            }
            DaemonResponse::Error {
                message: e.to_string(),
            }
        }
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
                let memory_rss = canon_core::logging::get_process_rss_mb();
                let entries = state.registry.entries().await;
                let mut ws_summaries = Vec::with_capacity(entries.len());
                let mut agg_files = 0;
                let mut agg_symbols = 0;
                let mut agg_call_edges = 0;
                let mut agg_contracts = 0;
                let mut agg_routes = 0;

                for entry in &entries {
                    let (metrics, mem_mb) = {
                        let guard = entry.index.read().await;
                        (guard.metrics(), guard.estimate_memory_mb())
                    };
                    agg_files += metrics.distinct_files_count;
                    agg_symbols += metrics.total_symbols_count;
                    agg_call_edges += metrics.total_call_edges_count;
                    agg_contracts += metrics.total_contracts_count;
                    agg_routes += metrics.total_routes_count;

                    ws_summaries.push(WorkspaceSummary {
                        root: entry.root.clone(),
                        is_worktree: entry.is_worktree,
                        parent_repo: entry.parent_repo.clone(),
                        indexed_files: metrics.distinct_files_count,
                        total_symbols: metrics.total_symbols_count,
                        memory_mb: mem_mb,
                        log_path: entry.logger.log_path().to_path_buf(),
                        last_accessed_secs_ago: entry.last_accessed_secs_ago(),
                    });
                }

                if entries.is_empty() {
                    let guard = state.legacy_index.read().await;
                    let m = guard.metrics();
                    agg_files = m.distinct_files_count;
                    agg_symbols = m.total_symbols_count;
                    agg_call_edges = m.total_call_edges_count;
                    agg_contracts = m.total_contracts_count;
                    agg_routes = m.total_routes_count;
                }

                let (hooks, raw, delivered, saved, pct, est_tokens) = state.telemetry.snapshot();
                let status_info = DaemonStatusInfo {
                    pid: std::process::id(),
                    uptime_secs: state.start_time.elapsed().as_secs(),
                    socket_path: state.options.socket_path.clone(),
                    workspace_root: state.options.workspace_root.clone(),
                    main_log_path: state.log_manager.main_log_path().to_path_buf(),
                    memory_rss_mb: memory_rss,
                    active_workspaces_count: entries.len(),
                    workspaces: ws_summaries,
                    indexed_files: agg_files,
                    total_symbols: agg_symbols,
                    total_call_edges: agg_call_edges,
                    total_contracts: agg_contracts,
                    total_routes: agg_routes,
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
            DaemonRequest::PostReadHook {
                file,
                start_line,
                end_line,
                raw_read_bytes,
            } => {
                let params = PostReadParams {
                    file: &file,
                    start_line,
                    end_line,
                    raw_read_bytes,
                };
                let resp = handle_post_read_request(params, req_start, &state).await;
                write_message(&mut stream, &resp).await?;
            }
            DaemonRequest::Slice { file, mode, format } => {
                let resp = handle_slice_request(&file, mode, format, &state).await;
                write_message(&mut stream, &resp).await?;
            }
            DaemonRequest::Shutdown => {
                let (hooks, raw, delivered, saved, pct, est_tokens) = state.telemetry.snapshot();
                state.log_manager.log_daemon(&format!(
                    "Daemon stopping: hooks_served={}, raw_bytes={}, delivered_bytes={}, bytes_saved={} ({:.1}%), est_tokens_saved={}",
                    hooks, raw, delivered, saved, pct, est_tokens
                ));
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

        // Edge case: delivered > raw reflects negative reduction and deducts from net savings
        let pct2 = telemetry.record_reduction(50, 100);
        assert!((pct2 - (-100.0)).abs() < 0.001);

        let (hooks, raw, delivered, saved, cumulative_pct, tokens) = telemetry.snapshot();
        assert_eq!(hooks, 2);
        assert_eq!(raw, 1050);
        assert_eq!(delivered, 300);
        assert_eq!(saved, 750);
        assert!((cumulative_pct - (750.0 / 1050.0 * 100.0)).abs() < 0.001);
        assert_eq!(tokens, 187);
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
