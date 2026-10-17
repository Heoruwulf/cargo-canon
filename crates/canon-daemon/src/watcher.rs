//! Incremental file watcher for hot cache updates.

use crate::error::DaemonError;
use canon_slice::SymbolIndex;
use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

/// Watches target directory for .rs file changes and updates index incrementally.
pub fn start_watcher(
    watch_dir: PathBuf,
    index: Arc<RwLock<SymbolIndex>>,
    token: CancellationToken,
) -> Result<(), DaemonError> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<PathBuf>(100);

    let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
        if let Ok(event) = res {
            if matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                for path in event.paths {
                    if path.extension().is_some_and(|ext| ext == "rs") {
                        let _ = tx.blocking_send(path);
                    }
                }
            }
        }
    })
    .map_err(|e| DaemonError::Watcher(e.to_string()))?;

    watcher
        .watch(Path::new(&watch_dir), RecursiveMode::Recursive)
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
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        let path_str = path.to_string_lossy().to_string();
                        let mut guard = index.write().await;
                        let _ = guard.index_source_str(&path_str, &content);
                    }
                }
            }
        }
    });

    Ok(())
}
