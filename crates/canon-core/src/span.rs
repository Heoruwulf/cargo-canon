//! Agent read span models, vendor defaults, byte calculation, and hook `stdin` parsing.
//!
//! Models the line and byte windows requested by AI coding agents (Claude Code,
//! Antigravity / AGY, or generic tools) during file reads so that daemon telemetry
//! and token-saving calculations reflect actual LLM context consumption rather than
//! assuming full-file reads.

use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;
use std::path::PathBuf;

/// Default 1-indexed starting line for Claude Code `Read` tool invocations.
pub const DEFAULT_CLAUDE_OFFSET: usize = 1;

/// Default line count limit for Claude Code `Read` tool invocations when omitted.
pub const DEFAULT_CLAUDE_LIMIT: usize = 2000;

/// Default 1-indexed starting line for Antigravity (AGY) `read_file` invocations.
pub const DEFAULT_AGY_START_LINE: usize = 1;

/// Default 1-indexed ending line for Antigravity (AGY) `read_file` invocations when omitted.
pub const DEFAULT_AGY_END_LINE: usize = 800;

/// Maximum byte cap per `read_file` call in Antigravity (AGY) (`46,080` bytes / 45 KiB).
pub const DEFAULT_AGY_BYTE_LIMIT: u64 = 46_080;

/// AI coding agent vendor identity used to apply default read window bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentVendor {
    /// Claude Code (`Read` tool: defaults to lines 1..=2000).
    Claude,
    /// Google Antigravity (`read_file` tool: defaults to lines 1..=800, capped at 46,080 bytes).
    Agy,
    /// Generic or unattributed tool invocation.
    Generic,
}

impl fmt::Display for AgentVendor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Claude => write!(f, "claude"),
            Self::Agy => write!(f, "agy"),
            Self::Generic => write!(f, "generic"),
        }
    }
}

/// Line and byte window parameters for an agent file read operation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadSpan {
    /// 1-indexed inclusive start line (AGY `StartLine` or CLI `--start-line`).
    pub start_line: Option<usize>,
    /// 1-indexed inclusive end line (AGY `EndLine` or CLI `--end-line`).
    pub end_line: Option<usize>,
    /// 1-indexed starting line offset (Claude `offset` or CLI `--offset`).
    pub offset: Option<usize>,
    /// Maximum number of lines to read (Claude `limit` or CLI `--limit`).
    pub limit: Option<usize>,
    /// Byte offset into the selected line range (AGY `ByteOffset`).
    pub byte_offset: Option<u64>,
    /// Detected or explicitly specified agent vendor.
    pub vendor: Option<AgentVendor>,
}

impl ReadSpan {
    /// Returns `true` if any span parameter or vendor is populated.
    #[must_use]
    pub const fn is_specified(&self) -> bool {
        self.start_line.is_some()
            || self.end_line.is_some()
            || self.offset.is_some()
            || self.limit.is_some()
            || self.byte_offset.is_some()
            || self.vendor.is_some()
    }

    /// Merges non-`None` fields from `overrides` on top of `self`.
    #[must_use]
    pub const fn with_overrides(mut self, overrides: &Self) -> Self {
        if overrides.start_line.is_some() {
            self.start_line = overrides.start_line;
        }
        if overrides.end_line.is_some() {
            self.end_line = overrides.end_line;
        }
        if overrides.offset.is_some() {
            self.offset = overrides.offset;
        }
        if overrides.limit.is_some() {
            self.limit = overrides.limit;
        }
        if overrides.byte_offset.is_some() {
            self.byte_offset = overrides.byte_offset;
        }
        if overrides.vendor.is_some() {
            self.vendor = overrides.vendor;
        }
        self
    }

    /// Resolves the unconsolidated 1-indexed `(start_line, end_line)` before clamping to file length.
    const fn raw_bounds(&self, total_lines: usize) -> (usize, usize) {
        let start = if let Some(s) = self.start_line {
            if s == 0 { 1 } else { s }
        } else if let Some(o) = self.offset {
            if o == 0 { 1 } else { o }
        } else {
            match self.vendor {
                Some(AgentVendor::Claude) => DEFAULT_CLAUDE_OFFSET,
                Some(AgentVendor::Agy) => DEFAULT_AGY_START_LINE,
                Some(AgentVendor::Generic) | None => 1,
            }
        };

        let end = if let Some(e) = self.end_line {
            if e < start { start } else { e }
        } else if let Some(l) = self.limit {
            if l == 0 {
                start
            } else {
                start.saturating_add(l.saturating_sub(1))
            }
        } else {
            match self.vendor {
                Some(AgentVendor::Claude) => {
                    start.saturating_add(DEFAULT_CLAUDE_LIMIT.saturating_sub(1))
                }
                Some(AgentVendor::Agy) => {
                    start.saturating_add(DEFAULT_AGY_END_LINE.saturating_sub(1))
                }
                Some(AgentVendor::Generic) | None => {
                    if total_lines > start {
                        total_lines
                    } else {
                        start
                    }
                }
            }
        };

        (start, end)
    }

    /// Resolves the effective 1-indexed inclusive `(start_line, end_line)` clamped to `total_lines`.
    ///
    /// Returns `(0, 0)` when `total_lines == 0` or when the requested start line exceeds `total_lines`.
    #[must_use]
    pub fn effective_range(&self, total_lines: usize) -> (usize, usize) {
        if total_lines == 0 {
            return (0, 0);
        }
        let (raw_start, raw_end) = self.raw_bounds(total_lines);
        if raw_start > total_lines {
            return (0, 0);
        }
        let end = raw_end.min(total_lines).max(raw_start);
        (raw_start, end)
    }

    /// Calculates the exact number of bytes read from `content` under this [`ReadSpan`],
    /// applying vendor-specific line defaults and byte caps.
    #[must_use]
    pub fn calculate_read_bytes(&self, content: &str) -> u64 {
        if content.is_empty() {
            return 0;
        }

        let lines: Vec<&str> = content.split_inclusive('\n').collect();
        let (start, end) = self.effective_range(lines.len());
        if start == 0 {
            return 0;
        }

        let slice_bytes: u64 = lines[start - 1..end]
            .iter()
            .map(|line| line.len() as u64)
            .sum();

        let after_offset = slice_bytes.saturating_sub(self.byte_offset.unwrap_or(0));

        if self.vendor == Some(AgentVendor::Agy) {
            after_offset.min(DEFAULT_AGY_BYTE_LIMIT)
        } else {
            after_offset
        }
    }
}

/// Parsed representation of an agent tool hook payload from `stdin`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedHookInput {
    /// Target file path extracted from the hook payload, if present.
    pub file_path: Option<PathBuf>,
    /// Extracted line/byte span and detected vendor.
    pub span: ReadSpan,
}

/// Parses a JSON hook payload from Claude Code or Antigravity (AGY) `stdin`.
///
/// Supported formats:
/// - **Claude Code**: `{"tool_name": "Read", "tool_input": {"file_path": "...", "offset": 100, "limit": 50}}`
/// - **Antigravity (AGY)**: `{"toolCall": {"name": "read_file", "args": {"Path": "...", "StartLine": 50, "EndLine": 80, "ByteOffset": 0}}}`
/// - **Flat / Hybrid JSON**: Top-level fields with `file_path`/`Path`, `offset`/`limit`, or `StartLine`/`EndLine`.
#[must_use]
pub fn parse_hook_stdin(input: &str) -> Option<ParsedHookInput> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    let root: Value = serde_json::from_str(trimmed).ok()?;
    let obj = root.as_object()?;

    if let Some(tool_call) = obj
        .get("toolCall")
        .or_else(|| obj.get("tool_call"))
        .and_then(Value::as_object)
    {
        return parse_agy_tool_call(tool_call, obj);
    }

    if let Some(tool_input) = obj
        .get("tool_input")
        .or_else(|| obj.get("toolInput"))
        .and_then(Value::as_object)
    {
        return parse_tool_input_envelope(obj, tool_input);
    }

    parse_flat_object(obj)
}

fn parse_agy_tool_call(
    tool_call: &serde_json::Map<String, Value>,
    root: &serde_json::Map<String, Value>,
) -> Option<ParsedHookInput> {
    let args = tool_call
        .get("args")
        .or_else(|| tool_call.get("arguments"))
        .or_else(|| tool_call.get("parameters"))
        .or_else(|| tool_call.get("input"))
        .and_then(Value::as_object)
        .unwrap_or(tool_call);

    let file_path = extract_path(args).or_else(|| extract_path(root));
    let mut span = extract_span_fields(args);
    span.vendor = extract_explicit_vendor(args)
        .or_else(|| extract_explicit_vendor(root))
        .or(Some(AgentVendor::Agy));

    Some(ParsedHookInput { file_path, span })
}

fn parse_tool_input_envelope(
    root: &serde_json::Map<String, Value>,
    tool_input: &serde_json::Map<String, Value>,
) -> Option<ParsedHookInput> {
    let tool_name = root
        .get("tool_name")
        .or_else(|| root.get("toolName"))
        .and_then(Value::as_str)
        .unwrap_or("");

    let is_agy_tool = tool_name.eq_ignore_ascii_case("read_file")
        || tool_name.ends_with(":read_file")
        || tool_name.eq_ignore_ascii_case("view_file")
        || tool_input.contains_key("Path")
        || tool_input.contains_key("StartLine")
        || tool_input.contains_key("EndLine")
        || tool_input.contains_key("ByteOffset");

    let file_path = extract_path(tool_input).or_else(|| extract_path(root));
    let mut span = extract_span_fields(tool_input);
    span.vendor = extract_explicit_vendor(tool_input)
        .or_else(|| extract_explicit_vendor(root))
        .or(Some(if is_agy_tool {
            AgentVendor::Agy
        } else {
            AgentVendor::Claude
        }));

    Some(ParsedHookInput { file_path, span })
}

fn parse_flat_object(obj: &serde_json::Map<String, Value>) -> Option<ParsedHookInput> {
    let file_path = extract_path(obj);
    let mut span = extract_span_fields(obj);
    let tool_name = obj
        .get("tool_name")
        .or_else(|| obj.get("toolName"))
        .or_else(|| obj.get("name"))
        .and_then(Value::as_str);

    let inferred_vendor = extract_explicit_vendor(obj).or_else(|| {
        if let Some(name) = tool_name {
            if name.eq_ignore_ascii_case("read") {
                return Some(AgentVendor::Claude);
            }
            if name.eq_ignore_ascii_case("read_file")
                || name.ends_with(":read_file")
                || name.eq_ignore_ascii_case("view_file")
            {
                return Some(AgentVendor::Agy);
            }
        }
        if obj.contains_key("Path")
            || obj.contains_key("StartLine")
            || obj.contains_key("EndLine")
            || obj.contains_key("ByteOffset")
        {
            Some(AgentVendor::Agy)
        } else if obj.contains_key("offset")
            || obj.contains_key("limit")
            || obj.contains_key("file_path")
        {
            Some(AgentVendor::Claude)
        } else {
            None
        }
    });

    span.vendor = inferred_vendor;

    if file_path.is_none() && !span.is_specified() {
        return None;
    }

    Some(ParsedHookInput { file_path, span })
}

fn extract_path(map: &serde_json::Map<String, Value>) -> Option<PathBuf> {
    ["file_path", "Path", "path", "filePath", "file"]
        .iter()
        .find_map(|key| map.get(*key).and_then(Value::as_str))
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

fn extract_usize(map: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<usize> {
    keys.iter().find_map(|key| {
        let val = map.get(*key)?;
        val.as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .or_else(|| val.as_str()?.parse::<usize>().ok())
    })
}

fn extract_u64(map: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|key| {
        let val = map.get(*key)?;
        val.as_u64().or_else(|| val.as_str()?.parse::<u64>().ok())
    })
}

fn extract_explicit_vendor(map: &serde_json::Map<String, Value>) -> Option<AgentVendor> {
    let raw = map.get("vendor").and_then(Value::as_str)?;
    if raw.eq_ignore_ascii_case("claude") {
        Some(AgentVendor::Claude)
    } else if raw.eq_ignore_ascii_case("agy") || raw.eq_ignore_ascii_case("antigravity") {
        Some(AgentVendor::Agy)
    } else if raw.eq_ignore_ascii_case("generic") {
        Some(AgentVendor::Generic)
    } else {
        None
    }
}

fn extract_span_fields(map: &serde_json::Map<String, Value>) -> ReadSpan {
    ReadSpan {
        start_line: extract_usize(map, &["StartLine", "start_line", "startLine"]),
        end_line: extract_usize(map, &["EndLine", "end_line", "endLine"]),
        offset: extract_usize(map, &["offset", "Offset"]),
        limit: extract_usize(map, &["limit", "Limit"]),
        byte_offset: extract_u64(map, &["ByteOffset", "byte_offset", "byteOffset"]),
        vendor: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write as _;

    fn build_sample_file(lines: usize, line_len: usize) -> String {
        let mut buf = String::with_capacity(lines * line_len);
        for i in 1..=lines {
            let prefix = format!("line-{i:04}: ");
            let pad_len = line_len.saturating_sub(prefix.len() + 1);
            let _ = writeln!(buf, "{prefix}{}", "x".repeat(pad_len));
        }
        buf
    }

    #[test]
    fn claude_defaults_apply_1_to_2000() {
        let span = ReadSpan {
            vendor: Some(AgentVendor::Claude),
            ..Default::default()
        };
        assert_eq!(span.effective_range(3000), (1, 2000));
        assert_eq!(span.effective_range(500), (1, 500));

        let content = build_sample_file(3000, 50);
        assert_eq!(span.calculate_read_bytes(&content), 2000 * 50);
    }

    #[test]
    fn claude_micro_read_offset_and_limit() {
        let span = ReadSpan {
            offset: Some(100),
            limit: Some(30),
            vendor: Some(AgentVendor::Claude),
            ..Default::default()
        };
        assert_eq!(span.effective_range(500), (100, 129));

        let content = build_sample_file(500, 40);
        assert_eq!(span.calculate_read_bytes(&content), 30 * 40);
    }

    #[test]
    fn agy_defaults_apply_1_to_800_and_byte_cap() {
        let span = ReadSpan {
            vendor: Some(AgentVendor::Agy),
            ..Default::default()
        };
        assert_eq!(span.effective_range(2000), (1, 800));

        // 800 lines * 30 bytes = 24,000 bytes (under 46,080 cap)
        let small_lines = build_sample_file(2000, 30);
        assert_eq!(span.calculate_read_bytes(&small_lines), 800 * 30);

        // 800 lines * 100 bytes = 80,000 bytes (capped at 46,080)
        let large_lines = build_sample_file(2000, 100);
        assert_eq!(
            span.calculate_read_bytes(&large_lines),
            DEFAULT_AGY_BYTE_LIMIT
        );
    }

    #[test]
    fn agy_micro_read_start_and_end_line() {
        let span = ReadSpan {
            start_line: Some(50),
            end_line: Some(80),
            vendor: Some(AgentVendor::Agy),
            ..Default::default()
        };
        assert_eq!(span.effective_range(500), (50, 80));

        let content = build_sample_file(500, 40);
        // 31 lines (50..=80 inclusive) * 40 bytes = 1,240 bytes
        assert_eq!(span.calculate_read_bytes(&content), 31 * 40);
    }

    #[test]
    fn parse_claude_stdin_json() {
        let json = r#"{
            "hook_event_name": "PostToolUse",
            "tool_name": "Read",
            "tool_input": {
                "file_path": "/workspace/src/lib.rs",
                "offset": 120,
                "limit": 45
            }
        }"#;
        let parsed = parse_hook_stdin(json).expect("should parse Claude payload");
        assert_eq!(
            parsed.file_path,
            Some(PathBuf::from("/workspace/src/lib.rs"))
        );
        assert_eq!(parsed.span.vendor, Some(AgentVendor::Claude));
        assert_eq!(parsed.span.offset, Some(120));
        assert_eq!(parsed.span.limit, Some(45));
        assert_eq!(parsed.span.effective_range(500), (120, 164));
    }

    #[test]
    fn parse_claude_stdin_with_omitted_bounds() {
        let json = r#"{
            "tool_name": "Read",
            "tool_input": {
                "file_path": "/workspace/src/main.rs"
            }
        }"#;
        let parsed = parse_hook_stdin(json).expect("should parse Claude payload");
        assert_eq!(parsed.span.vendor, Some(AgentVendor::Claude));
        assert_eq!(parsed.span.offset, None);
        assert_eq!(parsed.span.limit, None);
        assert_eq!(parsed.span.effective_range(5000), (1, 2000));
    }

    #[test]
    fn parse_agy_stdin_json() {
        let json = r#"{
            "toolCall": {
                "name": "read_file",
                "args": {
                    "Path": "/workspace/src/server.rs",
                    "StartLine": 50,
                    "EndLine": 80,
                    "ByteOffset": 100
                }
            }
        }"#;
        let parsed = parse_hook_stdin(json).expect("should parse AGY payload");
        assert_eq!(
            parsed.file_path,
            Some(PathBuf::from("/workspace/src/server.rs"))
        );
        assert_eq!(parsed.span.vendor, Some(AgentVendor::Agy));
        assert_eq!(parsed.span.start_line, Some(50));
        assert_eq!(parsed.span.end_line, Some(80));
        assert_eq!(parsed.span.byte_offset, Some(100));
    }

    #[test]
    fn parse_agy_stdin_with_omitted_lines() {
        let json = r#"{
            "toolCall": {
                "name": "read_file",
                "args": {
                    "Path": "/workspace/src/server.rs"
                }
            }
        }"#;
        let parsed = parse_hook_stdin(json).expect("should parse AGY payload");
        assert_eq!(parsed.span.vendor, Some(AgentVendor::Agy));
        assert_eq!(parsed.span.start_line, None);
        assert_eq!(parsed.span.end_line, None);
        assert_eq!(parsed.span.effective_range(2000), (1, 800));
    }

    #[test]
    fn cli_overrides_merge_cleanly() {
        let base = ReadSpan {
            offset: Some(10),
            limit: Some(20),
            vendor: Some(AgentVendor::Claude),
            ..Default::default()
        };
        let cli = ReadSpan {
            limit: Some(50),
            ..Default::default()
        };
        let merged = base.with_overrides(&cli);
        assert_eq!(merged.offset, Some(10));
        assert_eq!(merged.limit, Some(50));
        assert_eq!(merged.vendor, Some(AgentVendor::Claude));
    }

    #[test]
    fn empty_or_invalid_stdin_returns_none() {
        assert!(parse_hook_stdin("").is_none());
        assert!(parse_hook_stdin("   ").is_none());
        assert!(parse_hook_stdin("not json").is_none());
        assert!(parse_hook_stdin("{}").is_none());
    }

    #[test]
    fn out_of_bounds_start_returns_zero_range_and_bytes() {
        let span = ReadSpan {
            offset: Some(600),
            limit: Some(30),
            vendor: Some(AgentVendor::Claude),
            ..Default::default()
        };
        assert_eq!(span.effective_range(500), (0, 0));
        let content = build_sample_file(500, 40);
        assert_eq!(span.calculate_read_bytes(&content), 0);
    }
}
