//! Formatting helpers for daemon status tables and telemetry output.

use canon_daemon::{DaemonStatusInfo, WorkspaceSummary};

pub(crate) fn format_with_commas(n: usize) -> String {
    let s = n.to_string();
    let first_len = s.len() % 3;
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    if first_len > 0 {
        result.push_str(&s[..first_len]);
    }
    for (i, chunk) in s.as_bytes()[first_len..].chunks(3).enumerate() {
        if first_len > 0 || i > 0 {
            result.push(',');
        }
        result.push_str(std::str::from_utf8(chunk).unwrap_or(""));
    }
    result
}

pub(crate) fn format_uptime(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{}h {}m {}s", h, m, s)
    } else if m > 0 {
        format!("{}m {}s", m, s)
    } else {
        format!("{}s", s)
    }
}

pub(crate) fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

pub(crate) fn format_time_ago(secs: u64) -> String {
    if secs < 60 {
        format!("{}s ago", secs)
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else {
        format!("{}h ago", secs / 3600)
    }
}

pub(crate) fn truncate_tail(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count > max_chars {
        let keep = max_chars.saturating_sub(3);
        let tail: String = s.chars().skip(count - keep).collect();
        format!("...{}", tail)
    } else {
        s.to_string()
    }
}

pub(crate) fn format_range(start: Option<usize>, end: Option<usize>) -> String {
    match (start, end) {
        (Some(s), Some(e)) => format!(" (lines {}..{})", s, e),
        (Some(s), None) => format!(" (lines {}..)", s),
        (None, Some(e)) => format!(" (lines ..{})", e),
        (None, None) => String::new(),
    }
}

pub(crate) fn format_hooks_served(served: u64, submissions: u64) -> String {
    if served > 0 {
        let omitted = served.saturating_sub(submissions);
        format!(
            "Hooks Served:   {} requests ({} submitted, {} omitted)",
            format_with_commas(served as usize),
            format_with_commas(submissions as usize),
            format_with_commas(omitted as usize)
        )
    } else {
        "Hooks Served:   0 requests".to_string()
    }
}

pub(crate) fn format_drop_telemetry(cg_drops: u64, ast_drops: u64) -> String {
    format!(
        "Drop Telemetry: {} call graphs pruned, {} AST skeletons dropped",
        format_with_commas(cg_drops as usize),
        format_with_commas(ast_drops as usize)
    )
}

pub(crate) fn format_workspace_registry_row(ws: &WorkspaceSummary) -> String {
    let ws_type = if ws.is_worktree { "WkTr" } else { "Repo" };
    let root_trunc = truncate_tail(&ws.root.display().to_string(), 28);
    let proj_trunc = truncate_tail(ws.project_name.as_deref().unwrap_or("-"), 16);
    let cfg_str = ws
        .config_path
        .as_ref()
        .and_then(|p| p.file_name())
        .and_then(|f| f.to_str())
        .unwrap_or("-");
    let cfg_trunc = truncate_tail(cfg_str, 24);
    let log_str = ws
        .log_path
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("");
    let log_trunc = truncate_tail(log_str, 28);
    let accessed_str = format_time_ago(ws.last_accessed_secs_ago);
    format!(
        "  {:<28} {:<16} {:<6} {:>8} {:>10} {:>7.1} MB {:>7} {:>8} {:>9}  {:<24} {:<28} {:>8}",
        root_trunc,
        proj_trunc,
        ws_type,
        format_with_commas(ws.indexed_files),
        format_with_commas(ws.total_symbols),
        ws.memory_mb,
        format_with_commas(ws.submissions as usize),
        format_with_commas(ws.callgraph_drops as usize),
        format_with_commas(ws.ast_drops as usize),
        cfg_trunc,
        log_trunc,
        accessed_str
    )
}

pub(crate) fn print_daemon_status_table(status: &DaemonStatusInfo) {
    println!("Canon Daemon Status: Active");
    println!("PID:            {}", status.pid);
    println!("Uptime:         {}", format_uptime(status.uptime_secs));
    if !status.daemon_mode.is_empty() {
        println!("Daemon Mode:    {}", status.daemon_mode);
    }
    let cfg_display = status
        .config_path
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "built-in defaults".to_string());
    println!("Config Path:    {}", cfg_display);
    println!("Declared Projects: {}", status.declared_projects_count);
    println!("Socket Path:    {}", status.socket_path.display());
    if !status.main_log_path.as_os_str().is_empty() {
        println!("Main Daemon Log: {}", status.main_log_path.display());
    }
    println!("Memory RSS:     {:.1} MB", status.memory_rss_mb);
    println!("Active Workspaces: {}", status.active_workspaces_count);
    if !status.workspace_root.as_os_str().is_empty() {
        println!("Workspace Root: {}", status.workspace_root.display());
    }
    println!(
        "Indexed Files:  {} files",
        format_with_commas(status.indexed_files)
    );
    println!(
        "Total Symbols:  {} declarations",
        format_with_commas(status.total_symbols)
    );
    println!(
        "Call Edges:     {} edges",
        format_with_commas(status.total_call_edges)
    );
    println!(
        "Contracts:      {} trait implementations",
        format_with_commas(status.total_contracts)
    );
    println!(
        "Routes:         {} API endpoints",
        format_with_commas(status.total_routes)
    );
    let watcher_str = if status.watcher_active {
        "Active (.rs, .ts, .tsx, .js, .jsx, .go)"
    } else {
        "Inactive"
    };
    println!("File Watcher:   {}", watcher_str);

    if !status.workspaces.is_empty() {
        print_workspace_registry_table(&status.workspaces);
    }

    // --- Context Bloat Section ---
    println!(
        "{}",
        format_hooks_served(status.hooks_served, status.submissions)
    );
    if status.hooks_served > 0 {
        println!(
            "{}",
            format_drop_telemetry(status.callgraph_drops, status.ast_drops)
        );
        println!(
            "Context Bloat:  {} raw -> {} delivered ({:.1}% reduction)",
            format_bytes(status.raw_bytes_served),
            format_bytes(status.delivered_bytes_served),
            status.token_reduction_pct
        );
        println!(
            "Tokens Saved:   ~{} tokens (est. 4B/token)",
            format_with_commas(status.est_tokens_saved as usize)
        );
    } else {
        println!("Context Bloat:  0 B (no requests served yet)");
    }
}

pub(crate) fn print_workspace_registry_table(workspaces: &[WorkspaceSummary]) {
    println!("\nActive Workspace Registry:");
    println!(
        "  {:<28} {:<16} {:<6} {:>8} {:>10} {:>10} {:>7} {:>8} {:>9}  {:<24} {:<28} {:>8}",
        "WORKSPACE",
        "PROJECT",
        "TYPE",
        "FILES",
        "SYMBOLS",
        "MEMORY",
        "SUBMIT",
        "CG_DROP",
        "AST_DROP",
        "CONFIG",
        "LOG FILE",
        "ACCESSED"
    );
    println!("  {}", "-".repeat(176));
    for ws in workspaces {
        println!("{}", format_workspace_registry_row(ws));
    }
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
    }

    #[test]
    fn test_format_uptime() {
        assert_eq!(format_uptime(30), "30s");
        assert_eq!(format_uptime(90), "1m 30s");
        assert_eq!(format_uptime(3665), "1h 1m 5s");
    }

    #[test]
    fn test_format_time_ago() {
        assert_eq!(format_time_ago(5), "5s ago");
        assert_eq!(format_time_ago(120), "2m ago");
        assert_eq!(format_time_ago(7200), "2h ago");
    }

    #[test]
    fn test_truncate_tail_utf8_safety() {
        let path = "🦀_crab_🦀_crab_🦀_crab_🦀";
        let truncated = truncate_tail(path, 12);
        assert!(truncated.starts_with("..."));
        assert!(truncated.chars().count() <= 12);
    }

    #[test]
    fn test_format_hooks_served_and_drop_telemetry() {
        assert_eq!(format_hooks_served(0, 0), "Hooks Served:   0 requests");
        assert_eq!(
            format_hooks_served(23, 18),
            "Hooks Served:   23 requests (18 submitted, 5 omitted)"
        );
        assert_eq!(
            format_drop_telemetry(8, 5),
            "Drop Telemetry: 8 call graphs pruned, 5 AST skeletons dropped"
        );
    }

    #[test]
    fn test_format_workspace_registry_row_includes_drop_columns() {
        let ws = WorkspaceSummary {
            root: PathBuf::from("/home/mark/projects/pgrust"),
            project_name: Some("pgrust".to_string()),
            is_worktree: false,
            parent_repo: None,
            indexed_files: 3319,
            total_symbols: 65541,
            memory_mb: 292.0,
            config_path: Some(PathBuf::from("/home/mark/.config/canon/canon.toml")),
            log_path: PathBuf::from("/tmp/pgrust-a907563c9c2c.log"),
            last_accessed_secs_ago: 10,
            submissions: 7,
            callgraph_drops: 8,
            ast_drops: 5,
        };
        let row = format_workspace_registry_row(&ws);
        assert!(row.contains("7"));
        assert!(row.contains("8"));
        assert!(row.contains("5"));
    }
}
