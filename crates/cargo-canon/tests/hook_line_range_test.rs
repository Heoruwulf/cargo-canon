//! End-to-end integration tests for Claude Code and Antigravity (AGY) post-read hook
//! line spans, vendor defaults, and daemon token-saving telemetry.

use canon_core::span::{AgentVendor, DEFAULT_AGY_BYTE_LIMIT};
use canon_daemon::{
    DaemonRequest, DaemonResponse, DaemonStatusInfo, ServerOptions, run_server_with_options,
    send_daemon_request,
};
use canon_slice::SymbolIndex;
use cargo_canon::{DaemonArgs, compute_hook_read_metrics, resolve_hook_input};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

const TOTAL_LINES: usize = 2500;
const LINE_BYTE_LEN: usize = 60;

struct TestHarness {
    _dir: TempDir,
    socket_path: PathBuf,
    sample_file: PathBuf,
    original_content: String,
    token: CancellationToken,
    server_handle: JoinHandle<()>,
}

impl TestHarness {
    async fn start() -> Self {
        let dir = TempDir::new().expect("create temp dir");
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).expect("create src dir");

        let sample_file = src_dir.join("sample.rs");
        let original_content = generate_rust_source(TOTAL_LINES, LINE_BYTE_LEN);
        std::fs::write(&sample_file, &original_content).expect("write sample.rs");

        let socket_path = dir.path().join("daemon.sock");
        let log_dir = dir.path().join("logs");

        let mut index = SymbolIndex::new();
        let _ = index.index_directory(dir.path());
        let shared_index = Arc::new(RwLock::new(index));
        let token = CancellationToken::new();

        let opts = ServerOptions {
            socket_path: socket_path.clone(),
            workspace_root: dir.path().to_path_buf(),
            watcher_active: false,
            log_dir: Some(log_dir),
            idle_ttl_secs: 60,
            daemon_config: Default::default(),
            config_path: None,
        };

        let server_token = token.clone();
        let server_handle = tokio::spawn(async move {
            let _ = run_server_with_options(opts, shared_index, server_token).await;
        });

        for _ in 0..50 {
            if matches!(
                send_daemon_request(&socket_path, DaemonRequest::Ping).await,
                Ok(DaemonResponse::Pong)
            ) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        Self {
            _dir: dir,
            socket_path,
            sample_file,
            original_content,
            token,
            server_handle,
        }
    }

    async fn status(&self) -> DaemonStatusInfo {
        match send_daemon_request(&self.socket_path, DaemonRequest::Status)
            .await
            .expect("status request")
        {
            DaemonResponse::Status(info) => info,
            other => panic!("unexpected response: {other:?}"),
        }
    }

    async fn shutdown(self) {
        let _ = send_daemon_request(&self.socket_path, DaemonRequest::Shutdown).await;
        self.token.cancel();
        let _ = self.server_handle.await;
    }
}

fn generate_rust_source(lines: usize, line_len: usize) -> String {
    let mut buf = String::with_capacity(lines * line_len);
    for i in 1..=lines {
        let prefix = format!("pub fn item_{i:04}() -> u64 {{ {i} }} // ");
        let pad = line_len.saturating_sub(prefix.len() + 1);
        let _ = writeln!(buf, "{prefix}{}", "x".repeat(pad));
    }
    assert_eq!(buf.len(), lines * line_len);
    buf
}

fn assert_file_unchanged(path: &Path, expected: &str) {
    let on_disk = std::fs::read_to_string(path).expect("read sample file");
    assert_eq!(
        on_disk, expected,
        "advisory post-read hook must keep source file byte-identical on disk"
    );
}

#[tokio::test]
async fn claude_micro_read_records_exact_slice_bytes() {
    let harness = TestHarness::start().await;
    let stdin_json = serde_json::json!({
        "hook_event_name": "PostToolUse",
        "tool_name": "Read",
        "tool_input": {
            "file_path": harness.sample_file,
            "offset": 100,
            "limit": 30
        }
    })
    .to_string();

    let args = DaemonArgs {
        hook_post_read: true,
        ..Default::default()
    };
    let (file, span) =
        resolve_hook_input(&args, Some(&stdin_json)).expect("resolve Claude micro-read");
    assert_eq!(span.vendor, Some(AgentVendor::Claude));

    let (start_line, end_line, raw_read_bytes) = compute_hook_read_metrics(&file, &span);
    assert_eq!(start_line, Some(100));
    assert_eq!(end_line, Some(129));
    assert_eq!(raw_read_bytes, Some((30 * LINE_BYTE_LEN) as u64));

    let resp = send_daemon_request(
        &harness.socket_path,
        DaemonRequest::PostReadHook {
            file: file.clone(),
            start_line,
            end_line,
            raw_read_bytes,
        },
    )
    .await
    .expect("send PostReadHook");
    match resp {
        DaemonResponse::AdvisoryContext { advisory_block, .. } => {
            assert!(
                advisory_block.is_empty(),
                "micro-read smaller than full-file skeleton should suppress advisory bloat"
            );
        }
        other => panic!("unexpected response: {other:?}"),
    }

    let status = harness.status().await;
    assert_eq!(status.hooks_served, 1);
    assert_eq!(status.raw_bytes_served, (30 * LINE_BYTE_LEN) as u64);
    assert_eq!(status.delivered_bytes_served, status.raw_bytes_served);
    assert_eq!(status.bytes_saved, 0);
    assert_file_unchanged(&harness.sample_file, &harness.original_content);

    harness.shutdown().await;
}

#[tokio::test]
async fn claude_omitted_bounds_apply_default_2000_lines() {
    let harness = TestHarness::start().await;
    let stdin_json = serde_json::json!({
        "hook_event_name": "PostToolUse",
        "tool_name": "Read",
        "tool_input": {
            "file_path": harness.sample_file
        }
    })
    .to_string();

    let args = DaemonArgs {
        hook_post_read: true,
        ..Default::default()
    };
    let (file, span) =
        resolve_hook_input(&args, Some(&stdin_json)).expect("resolve Claude default read");
    assert_eq!(span.vendor, Some(AgentVendor::Claude));

    let (start_line, end_line, raw_read_bytes) = compute_hook_read_metrics(&file, &span);
    assert_eq!(start_line, Some(1));
    assert_eq!(end_line, Some(2000));
    assert_eq!(raw_read_bytes, Some((2000 * LINE_BYTE_LEN) as u64));

    let resp = send_daemon_request(
        &harness.socket_path,
        DaemonRequest::PostReadHook {
            file,
            start_line,
            end_line,
            raw_read_bytes,
        },
    )
    .await
    .expect("send PostReadHook");
    assert!(matches!(resp, DaemonResponse::AdvisoryContext { .. }));

    let status = harness.status().await;
    assert_eq!(status.hooks_served, 1);
    assert_eq!(status.raw_bytes_served, (2000 * LINE_BYTE_LEN) as u64);
    assert_file_unchanged(&harness.sample_file, &harness.original_content);

    harness.shutdown().await;
}

#[tokio::test]
async fn agy_micro_read_records_exact_slice_bytes() {
    let harness = TestHarness::start().await;
    let stdin_json = serde_json::json!({
        "toolCall": {
            "name": "read_file",
            "args": {
                "Path": harness.sample_file,
                "StartLine": 50,
                "EndLine": 80
            }
        }
    })
    .to_string();

    let args = DaemonArgs {
        hook_post_read: true,
        ..Default::default()
    };
    let (file, span) =
        resolve_hook_input(&args, Some(&stdin_json)).expect("resolve AGY micro-read");
    assert_eq!(span.vendor, Some(AgentVendor::Agy));

    // Lines 50..=80 inclusive = 31 lines
    let expected_bytes = (31 * LINE_BYTE_LEN) as u64;
    let (start_line, end_line, raw_read_bytes) = compute_hook_read_metrics(&file, &span);
    assert_eq!(start_line, Some(50));
    assert_eq!(end_line, Some(80));
    assert_eq!(raw_read_bytes, Some(expected_bytes));

    let resp = send_daemon_request(
        &harness.socket_path,
        DaemonRequest::PostReadHook {
            file,
            start_line,
            end_line,
            raw_read_bytes,
        },
    )
    .await
    .expect("send PostReadHook");
    match resp {
        DaemonResponse::AdvisoryContext { advisory_block, .. } => {
            assert!(
                advisory_block.is_empty(),
                "micro-read smaller than full-file skeleton should suppress advisory bloat"
            );
        }
        other => panic!("unexpected response: {other:?}"),
    }

    let status = harness.status().await;
    assert_eq!(status.hooks_served, 1);
    assert_eq!(status.raw_bytes_served, expected_bytes);
    assert_eq!(status.delivered_bytes_served, expected_bytes);
    assert_eq!(status.bytes_saved, 0);
    assert_file_unchanged(&harness.sample_file, &harness.original_content);

    harness.shutdown().await;
}

#[tokio::test]
async fn agy_omitted_lines_apply_default_800_lines_and_46kb_cap() {
    let harness = TestHarness::start().await;
    let stdin_json = serde_json::json!({
        "toolCall": {
            "name": "read_file",
            "args": {
                "Path": harness.sample_file
            }
        }
    })
    .to_string();

    let args = DaemonArgs {
        hook_post_read: true,
        ..Default::default()
    };
    let (file, span) =
        resolve_hook_input(&args, Some(&stdin_json)).expect("resolve AGY default read");
    assert_eq!(span.vendor, Some(AgentVendor::Agy));

    // 800 lines * 60 bytes = 48,000 bytes, capped at DEFAULT_AGY_BYTE_LIMIT (46,080)
    let (start_line, end_line, raw_read_bytes) = compute_hook_read_metrics(&file, &span);
    assert_eq!(start_line, Some(1));
    assert_eq!(end_line, Some(800));
    assert_eq!(raw_read_bytes, Some(DEFAULT_AGY_BYTE_LIMIT));

    let resp = send_daemon_request(
        &harness.socket_path,
        DaemonRequest::PostReadHook {
            file,
            start_line,
            end_line,
            raw_read_bytes,
        },
    )
    .await
    .expect("send PostReadHook");
    assert!(matches!(resp, DaemonResponse::AdvisoryContext { .. }));

    let status = harness.status().await;
    assert_eq!(status.hooks_served, 1);
    assert_eq!(status.raw_bytes_served, DEFAULT_AGY_BYTE_LIMIT);
    assert_file_unchanged(&harness.sample_file, &harness.original_content);

    harness.shutdown().await;
}

#[tokio::test]
async fn legacy_post_read_hook_without_line_info_falls_back_to_file_size() {
    let harness = TestHarness::start().await;

    let legacy_json = serde_json::json!({
        "PostReadHook": {
            "file": harness.sample_file
        }
    })
    .to_string();
    let req: DaemonRequest =
        serde_json::from_str(&legacy_json).expect("deserialize legacy PostReadHook");

    let resp = send_daemon_request(&harness.socket_path, req)
        .await
        .expect("send legacy PostReadHook");
    assert!(matches!(resp, DaemonResponse::AdvisoryContext { .. }));

    let status = harness.status().await;
    assert_eq!(status.hooks_served, 1);
    assert_eq!(
        status.raw_bytes_served,
        (TOTAL_LINES * LINE_BYTE_LEN) as u64
    );
    assert_file_unchanged(&harness.sample_file, &harness.original_content);

    harness.shutdown().await;
}
