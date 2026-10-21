//! Incremental file watcher for hot cache updates.
//!
//! Provides a thread-safe [`DynamicWatcherManager`] supporting dynamic registration
//! and unregistration of multiple workspace roots, selective event dispatch,
//! and per-workspace mutation logging.

use crate::error::DaemonError;
use crate::logging::WorkspaceLogger;
use canon_slice::{SymbolIndex, is_ignored_path};
use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use tokio::sync::RwLock as TokioRwLock;
use tokio_util::sync::CancellationToken;

/// Workspace entry registered with the dynamic file watcher.
#[derive(Clone)]
struct WatchedWorkspace {
    index: Arc<TokioRwLock<SymbolIndex>>,
    logger: Arc<WorkspaceLogger>,
}

/// Shared internal state for the dynamic file watcher.
struct DynamicWatcherInner {
    watcher: Mutex<notify::RecommendedWatcher>,
    workspaces: RwLock<HashMap<PathBuf, WatchedWorkspace>>,
}

/// Thread-safe manager that coordinates filesystem watches across multiple active workspaces.
#[derive(Clone)]
pub struct DynamicWatcherManager {
    inner: Arc<DynamicWatcherInner>,
    token: CancellationToken,
}

impl DynamicWatcherManager {
    /// Initializes a new dynamic watcher manager supervised by the provided cancellation token.
    pub fn new(token: CancellationToken) -> Result<Self, DaemonError> {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<PathBuf>(1024);

        let watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                if matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                    for path in event.paths {
                        if is_ignored_path(&path) {
                            continue;
                        }
                        if path.extension().is_some_and(|ext| {
                            matches!(
                                ext.to_str().unwrap_or(""),
                                "rs" | "ts" | "tsx" | "js" | "jsx" | "go"
                            )
                        }) {
                            let _ = tx.blocking_send(path);
                        }
                    }
                }
            }
        })
        .map_err(|e| DaemonError::Watcher(e.to_string()))?;

        let inner = Arc::new(DynamicWatcherInner {
            watcher: Mutex::new(watcher),
            workspaces: RwLock::new(HashMap::new()),
        });

        let weak_inner = Arc::downgrade(&inner);
        let cancel_token = token.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = cancel_token.cancelled() => {
                        break;
                    }
                    maybe_path = rx.recv() => {
                        let Some(path) = maybe_path else {
                            break;
                        };
                        if is_ignored_path(&path) {
                            continue;
                        }
                        let inner = match weak_inner.upgrade() {
                            Some(i) => i,
                            None => break,
                        };
                        Self::dispatch_mutation(&inner, path).await;
                    }
                }
            }
        });

        Ok(Self { inner, token })
    }

    /// Dispatches an observed file modification event to the appropriate matching workspace index.
    async fn dispatch_mutation(inner: &DynamicWatcherInner, path: PathBuf) {
        let canonical_path = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        let target = {
            let ws_guard = match inner.workspaces.read() {
                Ok(g) => g,
                Err(_) => return,
            };
            let mut best_match: Option<(PathBuf, WatchedWorkspace)> = None;
            for (root, ws) in ws_guard.iter() {
                if canonical_path.starts_with(root) {
                    match &best_match {
                        None => best_match = Some((root.clone(), ws.clone())),
                        Some((best_root, _)) => {
                            if root.as_os_str().len() > best_root.as_os_str().len() {
                                best_match = Some((root.clone(), ws.clone()));
                            }
                        }
                    }
                }
            }
            best_match
        };

        if let Some((root, ws)) = target {
            let start = std::time::Instant::now();
            if let Ok(content) = std::fs::read_to_string(&path) {
                let path_str = canonical_path.to_string_lossy().to_string();
                let mut guard = ws.index.write().await;
                let _ = guard.index_any_source_str(&path_str, &content);
                drop(guard);
                let elapsed_ms = start.elapsed().as_millis();
                tracing::debug!(path = %path.display(), elapsed_ms, "Symbol index updated");
                let rel_display = path.strip_prefix(&root).unwrap_or(&path).display();
                ws.logger.log(&format!(
                    "[DEBUG] File modified: '{}' -> updated symbol index in {}ms",
                    rel_display, elapsed_ms
                ));
            }
        }
    }

    /// Dynamically registers a workspace root for recursive file monitoring.
    pub fn watch_workspace(
        &self,
        root: PathBuf,
        index: Arc<TokioRwLock<SymbolIndex>>,
        logger: Arc<WorkspaceLogger>,
    ) -> Result<(), DaemonError> {
        let canonical_root = std::fs::canonicalize(&root).unwrap_or(root);

        let already_watched = {
            let ws_guard = self
                .inner
                .workspaces
                .read()
                .map_err(|e| DaemonError::Watcher(e.to_string()))?;
            ws_guard.contains_key(&canonical_root)
        };

        if !already_watched {
            let mut watcher_guard = self
                .inner
                .watcher
                .lock()
                .map_err(|e| DaemonError::Watcher(e.to_string()))?;
            watcher_guard
                .watch(&canonical_root, RecursiveMode::Recursive)
                .map_err(|e| DaemonError::Watcher(e.to_string()))?;
        }

        {
            let mut ws_guard = self
                .inner
                .workspaces
                .write()
                .map_err(|e| DaemonError::Watcher(e.to_string()))?;
            ws_guard.insert(canonical_root.clone(), WatchedWorkspace { index, logger });
        }

        tracing::info!(root = %canonical_root.display(), "Workspace registered for file watching");
        Ok(())
    }

    /// Dynamically unregisters a workspace root and detaches its filesystem watch.
    pub fn unwatch_workspace(&self, root: &Path) -> Result<(), DaemonError> {
        let canonical_root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        {
            let mut watcher_guard = self
                .inner
                .watcher
                .lock()
                .map_err(|e| DaemonError::Watcher(e.to_string()))?;
            if let Err(e) = watcher_guard.unwatch(&canonical_root) {
                tracing::debug!(root = %canonical_root.display(), error = %e, "OS unwatch notice");
            }
        }
        {
            let mut ws_guard = self
                .inner
                .workspaces
                .write()
                .map_err(|e| DaemonError::Watcher(e.to_string()))?;
            ws_guard.remove(&canonical_root);
        }
        tracing::info!(root = %canonical_root.display(), "Workspace unwatched");
        Ok(())
    }

    /// Checks whether a given directory is currently being actively watched.
    pub fn is_watching(&self, root: &Path) -> bool {
        let canonical_root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        self.inner
            .workspaces
            .read()
            .map(|w| w.contains_key(&canonical_root))
            .unwrap_or(false)
    }

    /// Returns the count of currently watched workspaces.
    pub fn watched_count(&self) -> usize {
        self.inner.workspaces.read().map(|w| w.len()).unwrap_or(0)
    }

    /// Returns the list of currently registered workspace roots.
    pub fn watched_roots(&self) -> Vec<PathBuf> {
        self.inner
            .workspaces
            .read()
            .map(|w| w.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Returns a reference to the cancellation token supervising this watcher.
    pub fn cancellation_token(&self) -> &CancellationToken {
        &self.token
    }

    /// Signals cancellation to the watcher background task.
    pub fn cancel(&self) {
        self.token.cancel();
    }
}

/// Watches target directory for source file changes and updates index incrementally (legacy entrypoint).
pub fn start_watcher(
    watch_dir: PathBuf,
    index: Arc<TokioRwLock<SymbolIndex>>,
    token: CancellationToken,
) -> Result<(), DaemonError> {
    let manager = DynamicWatcherManager::new(token.clone())?;
    let logger = Arc::new(WorkspaceLogger::noop(watch_dir.clone()));
    manager.watch_workspace(watch_dir, index, logger)?;

    tokio::spawn(async move {
        token.cancelled().await;
        drop(manager);
    });

    Ok(())
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::logging::DaemonLogManager;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_watcher_ignores_dependency_dirs() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let src_dir = root.join("src");
        let nm_dir = root.join("node_modules").join("pkg");
        let target_dir = root.join("target").join("debug");

        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::create_dir_all(&nm_dir).unwrap();
        std::fs::create_dir_all(&target_dir).unwrap();

        let index = Arc::new(TokioRwLock::new(SymbolIndex::new()));
        let token = CancellationToken::new();

        start_watcher(root.to_path_buf(), Arc::clone(&index), token.clone()).unwrap();

        // Brief delay for OS filesystem watcher registration
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Write files to ignored directories
        std::fs::write(nm_dir.join("index.js"), "function ignoredJs() {}").unwrap();
        std::fs::write(target_dir.join("gen.rs"), "pub fn ignoredRust() {}").unwrap();

        // Write file to valid source directory
        std::fs::write(src_dir.join("main.rs"), "pub fn validMain() {}").unwrap();

        // Poll for valid symbol update with timeout
        let start = std::time::Instant::now();
        let mut found = false;
        while start.elapsed() < std::time::Duration::from_secs(3) {
            {
                let guard = index.read().await;
                if guard.symbols_map.contains_key("validMain") {
                    found = true;
                    assert!(!guard.symbols_map.contains_key("ignoredJs"));
                    assert!(!guard.symbols_map.contains_key("ignoredRust"));
                    break;
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        }

        token.cancel();
        assert!(
            found,
            "watcher should have indexed validMain while ignoring node_modules and target"
        );
    }

    #[tokio::test]
    async fn test_dynamic_watcher_registration_and_logging() {
        let log_tmp = tempdir().unwrap();
        let log_manager = Arc::new(DaemonLogManager::init(log_tmp.path()).unwrap());

        let dir_a = tempdir().unwrap();
        let src_a = dir_a.path().join("src");
        std::fs::create_dir_all(&src_a).unwrap();
        std::fs::write(src_a.join("lib.rs"), "pub fn alpha_init() {}").unwrap();

        let dir_b = tempdir().unwrap();
        let src_b = dir_b.path().join("src");
        std::fs::create_dir_all(&src_b).unwrap();
        std::fs::write(src_b.join("lib.rs"), "pub fn beta_init() {}").unwrap();

        let token = CancellationToken::new();
        let manager = DynamicWatcherManager::new(token.clone()).unwrap();

        let idx_a = Arc::new(TokioRwLock::new(SymbolIndex::new()));
        let logger_a = Arc::new(log_manager.create_workspace_logger(dir_a.path()).unwrap());
        manager
            .watch_workspace(
                dir_a.path().to_path_buf(),
                Arc::clone(&idx_a),
                logger_a.clone(),
            )
            .unwrap();

        let idx_b = Arc::new(TokioRwLock::new(SymbolIndex::new()));
        let logger_b = Arc::new(log_manager.create_workspace_logger(dir_b.path()).unwrap());
        manager
            .watch_workspace(dir_b.path().to_path_buf(), Arc::clone(&idx_b), logger_b)
            .unwrap();

        assert_eq!(manager.watched_count(), 2);
        assert!(manager.is_watching(dir_a.path()));
        assert!(manager.is_watching(dir_b.path()));

        // Settle watcher
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Mutate alpha
        std::fs::write(src_a.join("lib.rs"), "pub fn alpha_modified() {}").unwrap();

        let start = std::time::Instant::now();
        let mut updated = false;
        while start.elapsed() < std::time::Duration::from_secs(3) {
            {
                let guard = idx_a.read().await;
                if guard.symbols_map.contains_key("alpha_modified") {
                    updated = true;
                    break;
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        }

        assert!(updated, "alpha index should receive updated symbol");

        // Verify isolated log file in logger_a
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        let log_content = std::fs::read_to_string(logger_a.log_path()).unwrap_or_default();
        assert!(
            log_content.contains("File modified:"),
            "logger_a log file must contain mutation event, got: {}",
            log_content
        );

        token.cancel();
    }

    #[tokio::test]
    async fn test_dynamic_watcher_unregistration() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("lib.rs"), "pub fn original() {}").unwrap();

        let token = CancellationToken::new();
        let manager = DynamicWatcherManager::new(token.clone()).unwrap();

        let idx = Arc::new(TokioRwLock::new(SymbolIndex::new()));
        let logger = Arc::new(WorkspaceLogger::noop(dir.path().to_path_buf()));

        manager
            .watch_workspace(dir.path().to_path_buf(), Arc::clone(&idx), logger)
            .unwrap();
        assert!(manager.is_watching(dir.path()));
        assert_eq!(manager.watched_count(), 1);

        manager.unwatch_workspace(dir.path()).unwrap();
        assert!(!manager.is_watching(dir.path()));
        assert_eq!(manager.watched_count(), 0);

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Write new file after unwatch
        std::fs::write(src.join("lib.rs"), "pub fn unwatched_symbol() {}").unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        let guard = idx.read().await;
        assert!(
            !guard.symbols_map.contains_key("unwatched_symbol"),
            "unwatched workspace index should not update"
        );

        token.cancel();
    }
}
