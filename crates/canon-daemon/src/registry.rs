//! Multi-tenant workspace registry and single-flight lazy initialization coordinator.
//!
//! Tracks active repositories and linked Git worktrees, lazily indexing codebases
//! on demand on first touch, managing overlay indices for worktrees, and pruning
//! inactive workspaces after idle timeouts.

use crate::error::DaemonError;
use crate::logging::{DaemonLogManager, WorkspaceLogger};
use canon_slice::SymbolIndex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Active workspace or worktree entry in the daemon registry.
pub struct WorkspaceEntry {
    /// Canonical workspace root directory.
    pub root: PathBuf,
    /// Whether this workspace is a linked Git worktree.
    pub is_worktree: bool,
    /// Parent repository root if this workspace is a worktree.
    pub parent_repo: Option<PathBuf>,
    /// In-memory symbol index (overlay index for worktrees, full index for standard repos).
    pub index: Arc<RwLock<SymbolIndex>>,
    /// Shared base repository symbol index if this entry is a worktree overlay.
    pub base_index: Option<Arc<RwLock<SymbolIndex>>>,
    /// Dedicated file logger for this workspace.
    pub logger: Arc<WorkspaceLogger>,
    /// Timestamp of last activity or hook evaluation.
    last_accessed: std::sync::Mutex<std::time::Instant>,
}

impl WorkspaceEntry {
    /// Creates a new workspace entry with initialized last accessed time.
    pub fn new(
        root: PathBuf,
        is_worktree: bool,
        parent_repo: Option<PathBuf>,
        index: Arc<RwLock<SymbolIndex>>,
        base_index: Option<Arc<RwLock<SymbolIndex>>>,
        logger: Arc<WorkspaceLogger>,
    ) -> Self {
        Self {
            root,
            is_worktree,
            parent_repo,
            index,
            base_index,
            logger,
            last_accessed: std::sync::Mutex::new(std::time::Instant::now()),
        }
    }

    /// Touches this workspace entry, resetting its idle duration timer.
    pub fn touch(&self) {
        if let Ok(mut lock) = self.last_accessed.lock() {
            *lock = std::time::Instant::now();
        }
    }

    /// Returns the elapsed time since this workspace was last accessed.
    pub fn idle_duration(&self) -> Duration {
        self.last_accessed
            .lock()
            .map(|t| t.elapsed())
            .unwrap_or_default()
    }

    /// Returns seconds since last access.
    pub fn last_accessed_secs_ago(&self) -> u64 {
        self.idle_duration().as_secs()
    }

    /// Estimates total in-memory heap footprint of the workspace's symbol index in Megabytes.
    pub async fn estimate_memory_mb(&self) -> f64 {
        self.index.read().await.estimate_memory_mb()
    }
}

/// Detects added or modified files in a worktree relative to its parent repository.
fn detect_worktree_modified_files(worktree_root: &Path, parent_repo: &Path) -> Vec<PathBuf> {
    let dummy_idx = SymbolIndex::new();
    let wt_files = dummy_idx.collect_source_files(worktree_root);
    let mut modified = Vec::new();

    for file in wt_files {
        if let Ok(rel) = file.strip_prefix(worktree_root) {
            let parent_file = parent_repo.join(rel);
            if !parent_file.exists() {
                modified.push(file);
            } else {
                let wt_meta = std::fs::metadata(&file).ok();
                let p_meta = std::fs::metadata(&parent_file).ok();
                match (wt_meta, p_meta) {
                    (Some(wm), Some(pm)) => {
                        if wm.len() != pm.len() || wm.modified().ok() != pm.modified().ok() {
                            modified.push(file);
                        } else if let (Ok(c1), Ok(c2)) =
                            (std::fs::read(&file), std::fs::read(&parent_file))
                        {
                            if c1 != c2 {
                                modified.push(file);
                            }
                        }
                    }
                    _ => modified.push(file),
                }
            }
        }
    }
    modified
}

type InFlightMap =
    HashMap<PathBuf, tokio::sync::broadcast::Sender<Result<Arc<WorkspaceEntry>, String>>>;

/// Multi-tenant registry holding all loaded workspaces, coordinating single-flight lazy initialization.
#[derive(Clone)]
pub struct WorkspaceRegistry {
    entries: Arc<RwLock<HashMap<PathBuf, Arc<WorkspaceEntry>>>>,
    in_flight: Arc<tokio::sync::Mutex<InFlightMap>>,
    log_manager: Arc<DaemonLogManager>,
}

impl WorkspaceRegistry {
    /// Creates a new workspace registry backed by the given log manager.
    pub fn new(log_manager: Arc<DaemonLogManager>) -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            in_flight: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            log_manager,
        }
    }

    /// Retrieves an existing workspace entry or lazily indexes it on first touch (single-flight).
    pub async fn get_or_init(
        &self,
        target_path: &Path,
    ) -> Result<Arc<WorkspaceEntry>, DaemonError> {
        let raw_root = canon_core::workspace::resolve_workspace_root(target_path);
        let canonical_root = std::fs::canonicalize(&raw_root).unwrap_or(raw_root);

        // 1. Fast path: check existing entries
        {
            let guard = self.entries.read().await;
            if let Some(entry) = guard.get(&canonical_root) {
                entry.touch();
                return Ok(entry.clone());
            }
        }

        // 2. Single-flight coordination
        let (rx, is_leader) = {
            let mut inflight = self.in_flight.lock().await;
            // Double check existing entries inside lock
            let guard = self.entries.read().await;
            if let Some(entry) = guard.get(&canonical_root) {
                entry.touch();
                return Ok(entry.clone());
            }

            if let Some(sender) = inflight.get(&canonical_root) {
                (Some(sender.subscribe()), false)
            } else {
                let (tx, _) = tokio::sync::broadcast::channel(1);
                inflight.insert(canonical_root.clone(), tx);
                (None, true)
            }
        };

        if !is_leader {
            if let Some(mut subscriber) = rx {
                match subscriber.recv().await {
                    Ok(Ok(entry)) => {
                        entry.touch();
                        return Ok(entry);
                    }
                    Ok(Err(err_msg)) => {
                        return Err(DaemonError::Protocol(format!(
                            "lazy init failed for '{}': {}",
                            canonical_root.display(),
                            err_msg
                        )));
                    }
                    Err(e) => {
                        return Err(DaemonError::Protocol(format!(
                            "single-flight coordination error: {}",
                            e
                        )));
                    }
                }
            }
        }

        // 3. Leader performs indexing
        let init_result = self.init_workspace(&canonical_root).await;

        // 4. Broadcast result and clean up in-flight map
        let mut inflight = self.in_flight.lock().await;
        let sender = inflight.remove(&canonical_root);

        match init_result {
            Ok(entry) => {
                self.entries
                    .write()
                    .await
                    .insert(canonical_root.clone(), entry.clone());
                if let Some(tx) = sender {
                    let _ = tx.send(Ok(entry.clone()));
                }
                Ok(entry)
            }
            Err(e) => {
                let err_str = e.to_string();
                if let Some(tx) = sender {
                    let _ = tx.send(Err(err_str));
                }
                Err(e)
            }
        }
    }

    /// Internal cold start initialization for a canonical workspace root.
    async fn init_workspace(
        &self,
        canonical_root: &Path,
    ) -> Result<Arc<WorkspaceEntry>, DaemonError> {
        let start_time = std::time::Instant::now();
        let parent_repo = canon_core::workspace::detect_worktree_parent_repo(canonical_root);
        let is_worktree = parent_repo.is_some();

        let logger = Arc::new(self.log_manager.create_workspace_logger(canonical_root)?);
        self.log_manager.log_daemon(&format!(
            "Cold start indexing initiated for '{}' (is_worktree: {})",
            canonical_root.display(),
            is_worktree
        ));
        logger.log(&format!(
            "Initial indexing started for '{}'",
            canonical_root.display()
        ));

        let (index, base_index) = if let Some(ref parent) = parent_repo {
            // Ensure parent repo is initialized recursively
            let parent_entry = Box::pin(self.get_or_init(parent)).await?;
            let base = parent_entry.index.clone();

            let modified_files = detect_worktree_modified_files(canonical_root, parent);
            let overlay = {
                let base_guard = base.read().await;
                base_guard.create_overlay(&modified_files)
            };

            logger.log(&format!(
                "Overlay indexed {} modified files against parent '{}' in {:?}",
                modified_files.len(),
                parent.display(),
                start_time.elapsed()
            ));

            (Arc::new(RwLock::new(overlay)), Some(base))
        } else {
            let mut idx = SymbolIndex::new();
            let indexed_files = idx.index_directory(canonical_root)?;

            let metrics = idx.metrics();
            logger.log(&format!(
                "Indexed {} files ({} symbols) in {:?}",
                indexed_files,
                metrics.total_symbols_count,
                start_time.elapsed()
            ));

            (Arc::new(RwLock::new(idx)), None)
        };

        let entry = Arc::new(WorkspaceEntry::new(
            canonical_root.to_path_buf(),
            is_worktree,
            parent_repo,
            index,
            base_index,
            logger,
        ));

        Ok(entry)
    }

    /// Fast lookup of an already indexed workspace without triggering cold start.
    pub async fn get(&self, target_path: &Path) -> Option<Arc<WorkspaceEntry>> {
        let raw_root = canon_core::workspace::resolve_workspace_root(target_path);
        let canonical_root = std::fs::canonicalize(&raw_root).unwrap_or(raw_root);
        let guard = self.entries.read().await;
        guard.get(&canonical_root).cloned()
    }

    /// Returns snapshots of all currently registered workspaces.
    pub async fn entries(&self) -> Vec<Arc<WorkspaceEntry>> {
        let guard = self.entries.read().await;
        guard.values().cloned().collect()
    }

    /// Returns the number of currently active workspaces in the registry.
    pub async fn len(&self) -> usize {
        self.entries.read().await.len()
    }

    /// Returns true if no workspaces are currently registered.
    pub async fn is_empty(&self) -> bool {
        self.entries.read().await.is_empty()
    }

    /// Prunes workspaces that have been idle for longer than `max_idle` or no longer exist on disk.
    pub async fn prune_idle_workspaces(&self, max_idle: Duration) -> Vec<PathBuf> {
        let mut entries_guard = self.entries.write().await;
        let mut to_remove = Vec::new();

        for (path, entry) in entries_guard.iter() {
            if entry.idle_duration() > max_idle || !path.exists() {
                to_remove.push(path.clone());
            }
        }

        for path in &to_remove {
            if let Some(entry) = entries_guard.remove(path) {
                self.log_manager.log_daemon(&format!(
                    "Evicted idle workspace '{}' (idle for {:?})",
                    path.display(),
                    entry.idle_duration()
                ));
                entry.logger.log(&format!(
                    "Workspace index evicted due to inactivity (idle for {:?})",
                    entry.idle_duration()
                ));
            }
        }

        to_remove
    }

    /// Explicitly registers a pre-constructed workspace entry into the registry.
    pub async fn register(&self, entry: Arc<WorkspaceEntry>) {
        let canonical = std::fs::canonicalize(&entry.root).unwrap_or_else(|_| entry.root.clone());
        self.entries.write().await.insert(canonical, entry);
    }

    /// Removes an individual workspace from the registry explicitly.
    pub async fn remove(&self, root: &Path) -> Option<Arc<WorkspaceEntry>> {
        let canonical = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        let mut guard = self.entries.write().await;
        guard.remove(&canonical)
    }

    /// Reference to the underlying log manager.
    pub fn log_manager(&self) -> &Arc<DaemonLogManager> {
        &self.log_manager
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_workspace_registry_cold_start() {
        let log_dir = tempdir().unwrap();
        let repo_dir = tempdir().unwrap();

        let src_dir = repo_dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(
            src_dir.join("lib.rs"),
            "pub struct Service; pub fn serve() {}",
        )
        .unwrap();

        let log_manager = Arc::new(DaemonLogManager::init(log_dir.path()).unwrap());
        let registry = WorkspaceRegistry::new(log_manager);

        let entry = registry
            .get_or_init(repo_dir.path())
            .await
            .expect("cold start should succeed");

        assert_eq!(entry.root, std::fs::canonicalize(repo_dir.path()).unwrap());
        assert!(!entry.is_worktree);
        assert!(entry.parent_repo.is_none());
        assert!(entry.base_index.is_none());

        let idx_guard = entry.index.read().await;
        assert!(idx_guard.symbols_map.contains_key("Service"));
        drop(idx_guard);

        assert!(entry.estimate_memory_mb().await > 0.0);
        assert_eq!(registry.len().await, 1);
    }

    #[tokio::test]
    async fn test_single_flight_deduplication() {
        let log_dir = tempdir().unwrap();
        let repo_dir = tempdir().unwrap();

        let src_dir = repo_dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(src_dir.join("main.rs"), "pub fn main() {}").unwrap();

        let log_manager = Arc::new(DaemonLogManager::init(log_dir.path()).unwrap());
        let registry = WorkspaceRegistry::new(log_manager);

        let mut handles = Vec::new();
        for _ in 0..8 {
            let reg = registry.clone();
            let p = repo_dir.path().to_path_buf();
            handles.push(tokio::spawn(async move { reg.get_or_init(&p).await }));
        }

        for h in handles {
            let res = h.await.unwrap();
            assert!(res.is_ok());
            assert_eq!(
                res.unwrap().root,
                std::fs::canonicalize(repo_dir.path()).unwrap()
            );
        }

        assert_eq!(registry.len().await, 1);
    }

    #[tokio::test]
    async fn test_worktree_overlay_registration() {
        let log_dir = tempdir().unwrap();
        let parent_repo = tempdir().unwrap();

        let parent_git = parent_repo.path().join(".git");
        std::fs::create_dir_all(&parent_git).unwrap();
        let parent_src = parent_repo.path().join("src");
        std::fs::create_dir_all(&parent_src).unwrap();
        std::fs::write(
            parent_src.join("lib.rs"),
            "pub struct BaseEngine; pub fn run_base() {}",
        )
        .unwrap();

        // Create worktree directory structure
        let worktrees_git = parent_git.join("worktrees").join("feat1");
        std::fs::create_dir_all(&worktrees_git).unwrap();
        std::fs::write(worktrees_git.join("commondir"), "../..\n").unwrap();

        let worktree_dir = tempdir().unwrap();
        let wt_git_file = worktree_dir.path().join(".git");
        std::fs::write(
            &wt_git_file,
            format!("gitdir: {}\n", worktrees_git.display()),
        )
        .unwrap();

        let wt_src = worktree_dir.path().join("src");
        std::fs::create_dir_all(&wt_src).unwrap();
        std::fs::write(
            wt_src.join("lib.rs"),
            "pub struct BaseEngine; pub fn run_overlay() {}",
        )
        .unwrap();

        let log_manager = Arc::new(DaemonLogManager::init(log_dir.path()).unwrap());
        let registry = WorkspaceRegistry::new(log_manager);

        let wt_entry = registry
            .get_or_init(worktree_dir.path())
            .await
            .expect("worktree cold start should succeed");

        assert!(wt_entry.is_worktree);
        assert_eq!(
            wt_entry.parent_repo,
            Some(std::fs::canonicalize(parent_repo.path()).unwrap())
        );
        assert!(wt_entry.base_index.is_some());

        // Overlay should contain overlay function
        let wt_guard = wt_entry.index.read().await;
        assert!(wt_guard.symbols_map.contains_key("run_overlay"));
    }

    #[tokio::test]
    async fn test_prune_idle_workspaces() {
        let log_dir = tempdir().unwrap();
        let repo_dir = tempdir().unwrap();

        let src_dir = repo_dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(src_dir.join("main.rs"), "pub fn main() {}").unwrap();

        let log_manager = Arc::new(DaemonLogManager::init(log_dir.path()).unwrap());
        let registry = WorkspaceRegistry::new(log_manager);

        let _ = registry.get_or_init(repo_dir.path()).await.unwrap();
        assert_eq!(registry.len().await, 1);

        tokio::time::sleep(Duration::from_millis(20)).await;

        let pruned = registry
            .prune_idle_workspaces(Duration::from_millis(10))
            .await;
        assert_eq!(pruned.len(), 1);
        assert_eq!(registry.len().await, 0);
    }
}
