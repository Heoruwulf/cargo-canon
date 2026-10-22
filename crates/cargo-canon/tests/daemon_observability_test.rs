//! End-to-end integration test verifying daemon observability, CLI control commands,
//! and out-of-band context provenance headers.

use canon_daemon::{DaemonStatusInfo, ServerOptions, run_server_with_options};
use canon_slice::SymbolIndex;
use std::sync::Arc;
use tempfile::tempdir;
use tokio::process::Command;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_daemon_observability_lifecycle() {
    let canon_bin = env!("CARGO_BIN_EXE_canon");
    let base = tempdir().unwrap();
    let src_dir = base.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();

    let test_file = src_dir.join("lib.rs");
    let test_content = br#"pub struct Service {
    pub name: String,
    pub retries: u32,
    pub timeout_ms: u64,
}

pub trait Runner {
    fn run(&self) -> bool;
    fn validate(&self) -> Result<(), String>;
    fn execute_batch(&self, items: &[String]) -> usize;
}

impl Runner for Service {
    fn run(&self) -> bool {
        let is_valid = !self.name.is_empty();
        if is_valid {
            println!("Service {} running with timeout {}", self.name, self.timeout_ms);
            true
        } else {
            eprintln!("Invalid service name configuration");
            false
        }
    }

    fn validate(&self) -> Result<(), String> {
        if self.retries > 10 {
            return Err("Too many retries configured for service worker".to_string());
        }
        if self.timeout_ms < 100 {
            return Err("Timeout configured is too low for reliable execution".to_string());
        }
        Ok(())
    }

    fn execute_batch(&self, items: &[String]) -> usize {
        let mut count = 0;
        for item in items {
            if !item.is_empty() {
                count += 1;
                println!("Processing batch item {} on service {}", item, self.name);
            }
        }
        count
    }
}
"#;
    std::fs::write(&test_file, test_content).unwrap();

    let socket_path = base.path().join("canon_obs.sock");
    let mut initial_index = SymbolIndex::new();
    initial_index.index_directory(base.path()).unwrap();
    let index = Arc::new(RwLock::new(initial_index));
    let token = CancellationToken::new();

    let s_path = socket_path.clone();
    let s_token = token.clone();
    let s_root = base.path().to_path_buf();
    let server_handle = tokio::spawn(async move {
        let opts = ServerOptions {
            socket_path: s_path,
            workspace_root: s_root,
            watcher_active: false,
            log_dir: None,
            idle_ttl_secs: 7200,
            daemon_config: Default::default(),
            config_path: None,
        };
        let _ = run_server_with_options(opts, index, s_token).await;
    });

    // Allow server socket to bind
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // 1. Probe liveness via --ping
    let ping_out = Command::new(canon_bin)
        .args([
            "daemon",
            "--ping",
            "--socket",
            socket_path.to_str().unwrap(),
        ])
        .current_dir(base.path())
        .output()
        .await
        .expect("failed to execute canon daemon --ping");
    assert!(ping_out.status.success(), "ping must succeed");
    let ping_str = String::from_utf8_lossy(&ping_out.stdout);
    assert!(ping_str.contains("[Daemon] Daemon is responsive on"));
    assert!(ping_str.contains("round-trip:"));

    // 2. Query initial status via --status (human table)
    let status_out = Command::new(canon_bin)
        .args([
            "daemon",
            "--status",
            "--socket",
            socket_path.to_str().unwrap(),
        ])
        .current_dir(base.path())
        .output()
        .await
        .expect("failed to execute canon daemon --status");
    assert!(status_out.status.success(), "status must succeed");
    let status_str = String::from_utf8_lossy(&status_out.stdout);
    assert!(status_str.contains("Canon Daemon Status: Active"));
    assert!(status_str.contains("PID:"));
    assert!(status_str.contains("Uptime:"));
    assert!(status_str.contains("Indexed Files:  1 files"));
    assert!(status_str.contains("Total Symbols:"));
    assert!(status_str.contains("Hooks Served:   0 requests"));
    assert!(status_str.contains("Context Bloat:  0 B (no requests served yet)"));

    // 3. Query initial status via --status --json: verify 0 hooks served initially
    let status_json_out = Command::new(canon_bin)
        .args([
            "daemon",
            "--status",
            "--socket",
            socket_path.to_str().unwrap(),
            "--json",
        ])
        .current_dir(base.path())
        .output()
        .await
        .expect("failed to execute canon daemon --status --json");
    assert!(status_json_out.status.success());
    let json_str = String::from_utf8_lossy(&status_json_out.stdout);
    let status_info: DaemonStatusInfo =
        serde_json::from_str(&json_str).expect("status must be valid JSON");
    assert_eq!(status_info.indexed_files, 1);
    assert!(status_info.total_symbols >= 2);
    assert_eq!(status_info.pid, std::process::id());
    assert_eq!(status_info.hooks_served, 0);
    assert_eq!(status_info.bytes_saved, 0);
    assert_eq!(status_info.token_reduction_pct, 0.0);
    assert_eq!(status_info.est_tokens_saved, 0);

    // 4. Hook post-read with live daemon (provenance: daemon)
    let hook_daemon_out = Command::new(canon_bin)
        .args([
            "daemon",
            "--hook-post-read",
            "--file",
            test_file.to_str().unwrap(),
            "--socket",
            socket_path.to_str().unwrap(),
        ])
        .current_dir(base.path())
        .output()
        .await
        .expect("failed to execute canon daemon --hook-post-read");
    assert!(hook_daemon_out.status.success());
    let hook_daemon_str = String::from_utf8_lossy(&hook_daemon_out.stdout);
    assert!(
        hook_daemon_str.contains("<!-- canon-context-provenance: daemon (socket:"),
        "expected daemon provenance header, got:\n{}",
        hook_daemon_str
    );
    assert!(hook_daemon_str.contains("# Canon Context Slice:"));

    // 5. Query status post-hook via --status --json: verify telemetry incremented
    let status_post_json_out = Command::new(canon_bin)
        .args([
            "daemon",
            "--status",
            "--socket",
            socket_path.to_str().unwrap(),
            "--json",
        ])
        .current_dir(base.path())
        .output()
        .await
        .expect("failed to execute canon daemon --status --json post-hook");
    assert!(status_post_json_out.status.success());
    let post_json_str = String::from_utf8_lossy(&status_post_json_out.stdout);
    let post_status_info: DaemonStatusInfo =
        serde_json::from_str(&post_json_str).expect("post-hook status must be valid JSON");
    assert_eq!(post_status_info.hooks_served, 1);
    assert!(post_status_info.raw_bytes_served > 0);
    assert!(post_status_info.delivered_bytes_served > 0);
    assert!(post_status_info.bytes_saved > 0);
    assert!(post_status_info.token_reduction_pct > 0.0);
    assert!(post_status_info.est_tokens_saved > 0);

    // 6. Query status post-hook via human table: verify formatted bloat lines
    let status_post_table_out = Command::new(canon_bin)
        .args([
            "daemon",
            "--status",
            "--socket",
            socket_path.to_str().unwrap(),
        ])
        .current_dir(base.path())
        .output()
        .await
        .expect("failed to execute canon daemon --status post-hook");
    assert!(status_post_table_out.status.success());
    let post_table_str = String::from_utf8_lossy(&status_post_table_out.stdout);
    assert!(post_table_str.contains("Hooks Served:   1 requests"));
    assert!(post_table_str.contains("Context Bloat:"));
    assert!(post_table_str.contains("Tokens Saved:"));
    assert!(post_table_str.contains("reduction)"));

    // 7. Graceful shutdown via --stop
    let stop_out = Command::new(canon_bin)
        .args([
            "daemon",
            "--stop",
            "--socket",
            socket_path.to_str().unwrap(),
        ])
        .current_dir(base.path())
        .output()
        .await
        .expect("failed to execute canon daemon --stop");
    assert!(stop_out.status.success(), "stop must succeed");
    let stop_str = String::from_utf8_lossy(&stop_out.stdout);
    assert!(stop_str.contains("[Daemon] Daemon on"));
    assert!(stop_str.contains("successfully terminated."));
    assert!(!socket_path.exists(), "socket must be unlinked");
    let _ = server_handle.await;

    // 6. Hook post-read post-shutdown (provenance: in-process-fallback)
    let file_bytes_before = std::fs::read(&test_file).unwrap();
    let hook_fallback_out = Command::new(canon_bin)
        .args([
            "daemon",
            "--hook-post-read",
            "--file",
            test_file.to_str().unwrap(),
            "--socket",
            socket_path.to_str().unwrap(),
        ])
        .current_dir(base.path())
        .output()
        .await
        .expect("failed to execute canon daemon --hook-post-read after shutdown");
    assert!(hook_fallback_out.status.success());
    let hook_fallback_str = String::from_utf8_lossy(&hook_fallback_out.stdout);
    assert!(
        hook_fallback_str.contains(
            "<!-- canon-context-provenance: in-process-fallback (daemon unreachable) -->"
        ),
        "expected in-process fallback header, got:\n{}",
        hook_fallback_str
    );
    assert!(hook_fallback_str.contains("# Canon Context Slice:"));

    // Critical invariant: source file on disk is strictly byte-identical
    let file_bytes_after = std::fs::read(&test_file).unwrap();
    assert_eq!(
        file_bytes_before, file_bytes_after,
        "CRITICAL INVARIANT: target file on disk modified during hook execution"
    );

    // 7. Ping failure when daemon is offline
    let ping_offline_out = Command::new(canon_bin)
        .args([
            "daemon",
            "--ping",
            "--socket",
            socket_path.to_str().unwrap(),
        ])
        .current_dir(base.path())
        .output()
        .await
        .expect("failed to execute canon daemon --ping offline");
    assert!(
        !ping_offline_out.status.success(),
        "ping should fail when offline"
    );
}
