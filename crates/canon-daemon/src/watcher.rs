//! Incremental file watcher for hot cache updates.

use crate::error::DaemonError;
use canon_slice::{SymbolIndex, is_ignored_path};
use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

/// Watches target directory for source file changes and updates index incrementally.
pub fn start_watcher(
    watch_dir: PathBuf,
    index: Arc<RwLock<SymbolIndex>>,
    token: CancellationToken,
) -> Result<(), DaemonError> {
    let canonical_root = std::fs::canonicalize(&watch_dir).unwrap_or(watch_dir);
    tracing::info!(root = %canonical_root.display(), "Incremental file watcher started");

    let (tx, mut rx) = tokio::sync::mpsc::channel::<PathBuf>(100);

    let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
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

    watcher
        .watch(Path::new(&canonical_root), RecursiveMode::Recursive)
        .map_err(|e| DaemonError::Watcher(e.to_string()))?;

    tokio::spawn(async move {
        // Keep watcher active within task lifecycle
        let _active_watcher = watcher;

        loop {
            tokio::select! {
                _ = token.cancelled() => {
                    break;
                }
                Some(path) = rx.recv() => {
                    if is_ignored_path(&path) {
                        continue;
                    }
                    tracing::debug!(path = %path.display(), "Source file changed: updating symbol index");
                    let start = std::time::Instant::now();
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        let canonical_path = std::fs::canonicalize(&path).unwrap_or(path.clone());
                        let path_str = canonical_path.to_string_lossy().to_string();
                        let mut guard = index.write().await;
                        let _ = guard.index_any_source_str(&path_str, &content);
                        let elapsed_ms = start.elapsed().as_millis();
                        tracing::debug!(path = %path.display(), elapsed_ms, "Symbol index updated");
                    }
                }
            }
        }
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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

        let index = Arc::new(RwLock::new(SymbolIndex::new()));
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
}
