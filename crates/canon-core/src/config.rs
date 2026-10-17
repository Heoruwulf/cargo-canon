//! Configuration models and discovery for cargo-canon.

use crate::error::CoreError;
use crate::output::OutputFormat;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Primary configuration structure for cargo-canon workspaces.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CanonConfig {
    /// General workspace settings.
    #[serde(default)]
    pub general: GeneralConfig,
    /// Configuration for context slicing engine.
    #[serde(default)]
    pub slice: SliceConfig,
    /// Configuration for deterministic simulation testing.
    #[serde(default)]
    pub simulate: SimulateConfig,
    /// Configuration for bounded model checking.
    #[serde(default)]
    pub verify: VerifyConfig,
    /// Configuration for differential fuzzing.
    #[serde(default)]
    pub diff_fuzz: DiffFuzzConfig,
    /// Configuration for daemon sidecar service.
    #[serde(default)]
    pub daemon: DaemonConfig,
    /// Configuration for TypeScript / TSX context slicing.
    #[serde(default)]
    pub typescript: TypeScriptConfig,
    /// Configuration for cross-boundary contract correlation.
    #[serde(default)]
    pub cross_boundary: CrossBoundaryConfig,
}

/// General engine configuration options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneralConfig {
    /// Codebase scale hint: "small", "medium", "massive" (adjusts cache/pool allocations).
    #[serde(default = "default_codebase_scale")]
    pub codebase_scale: String,
    /// Default output format for reports and slices.
    #[serde(default)]
    pub default_format: OutputFormat,
}

fn default_codebase_scale() -> String {
    "massive".to_string()
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            codebase_scale: default_codebase_scale(),
            default_format: OutputFormat::Markdown,
        }
    }
}

/// Slicing engine configuration options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SliceConfig {
    /// Maximum call-graph depth for caller/callee exploration.
    #[serde(default = "default_max_call_depth")]
    pub max_call_depth: usize,
    /// Whether to strip comments during skeleton unparsing.
    #[serde(default)]
    pub strip_comments: bool,
    /// Whether to include private/internal types in context exports.
    #[serde(default = "default_true")]
    pub include_private_types: bool,
}

fn default_max_call_depth() -> usize {
    2
}

fn default_true() -> bool {
    true
}

impl Default for SliceConfig {
    fn default() -> Self {
        Self {
            max_call_depth: default_max_call_depth(),
            strip_comments: false,
            include_private_types: true,
        }
    }
}

/// Simulation runner configuration options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimulateConfig {
    /// Default number of seeds to run.
    #[serde(default = "default_simulate_seeds")]
    pub default_seeds: u64,
    /// Execution timeout per simulation run in seconds.
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: u64,
    /// Panic hook mode: "capture_backtrace", "silent", or "abort".
    #[serde(default = "default_panic_hook")]
    pub panic_hook: String,
}

fn default_simulate_seeds() -> u64 {
    5000
}

fn default_timeout_seconds() -> u64 {
    30
}

fn default_panic_hook() -> String {
    "capture_backtrace".to_string()
}

impl Default for SimulateConfig {
    fn default() -> Self {
        Self {
            default_seeds: default_simulate_seeds(),
            timeout_seconds: default_timeout_seconds(),
            panic_hook: default_panic_hook(),
        }
    }
}

/// Formal model checking and verification options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyConfig {
    /// Default unwind depth for loops and recursions.
    #[serde(default = "default_unwind")]
    pub default_unwind: usize,
    /// SAT/SMT solver engine ("kissat", "cadical", "z3").
    #[serde(default = "default_solver")]
    pub solver: String,
}

fn default_unwind() -> usize {
    4
}

fn default_solver() -> String {
    "kissat".to_string()
}

impl Default for VerifyConfig {
    fn default() -> Self {
        Self {
            default_unwind: default_unwind(),
            solver: default_solver(),
        }
    }
}

/// Differential fuzzing options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffFuzzConfig {
    /// Default number of fuzzing iterations.
    #[serde(default = "default_diff_fuzz_iterations")]
    pub default_iterations: u64,
    /// Path to corpus directory for seed storage.
    #[serde(default = "default_corpus_dir")]
    pub corpus_dir: String,
}

fn default_diff_fuzz_iterations() -> u64 {
    50000
}

fn default_corpus_dir() -> String {
    "tests/fuzz/corpus".to_string()
}

impl Default for DiffFuzzConfig {
    fn default() -> Self {
        Self {
            default_iterations: default_diff_fuzz_iterations(),
            corpus_dir: default_corpus_dir(),
        }
    }
}

/// Background daemon configuration options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaemonConfig {
    /// Unix domain socket path for agent IPC.
    #[serde(default = "default_socket_path")]
    pub socket_path: String,
    /// Whether background filesystem watching is enabled.
    #[serde(default = "default_true")]
    pub enable_file_watcher: bool,
    /// Paths to monitor for source code modifications.
    #[serde(default = "default_watch_paths")]
    pub watch_paths: Vec<String>,
}

fn default_socket_path() -> String {
    "/tmp/canon.sock".to_string()
}

fn default_watch_paths() -> Vec<String> {
    vec!["crates".to_string(), "frontend/src".to_string()]
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            socket_path: default_socket_path(),
            enable_file_watcher: true,
            watch_paths: default_watch_paths(),
        }
    }
}

/// Configuration for TypeScript / TSX context slicing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeScriptConfig {
    /// Enable TypeScript parsing and slicing.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Root tsconfig path for symbol resolution.
    #[serde(default = "default_tsconfig_path")]
    pub tsconfig_path: String,
    /// Supported extensions for slicing and file watching.
    #[serde(default = "default_ts_extensions")]
    pub extensions: Vec<String>,
    /// Strips JSX blocks down to clean type signatures.
    #[serde(default = "default_true")]
    pub strip_jsx: bool,
    /// Strips React hook internals (useEffect, useState).
    #[serde(default = "default_true")]
    pub strip_hook_bodies: bool,
}

fn default_tsconfig_path() -> String {
    "frontend/tsconfig.json".to_string()
}

fn default_ts_extensions() -> Vec<String> {
    vec![
        "ts".to_string(),
        "tsx".to_string(),
        "js".to_string(),
        "jsx".to_string(),
    ]
}

impl Default for TypeScriptConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            tsconfig_path: default_tsconfig_path(),
            extensions: default_ts_extensions(),
            strip_jsx: true,
            strip_hook_bodies: true,
        }
    }
}

/// Configuration for cross-boundary contract correlation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrossBoundaryConfig {
    /// Directory containing shared Serde models.
    #[serde(default = "default_rust_models")]
    pub rust_models: String,
    /// Directory containing TypeScript contract definitions.
    #[serde(default = "default_ts_types")]
    pub ts_types: String,
    /// Enforce camelCase vs snake_case consistency checks.
    #[serde(default = "default_true")]
    pub strict_casing_check: bool,
}

fn default_rust_models() -> String {
    "crates/shared-types/src".to_string()
}

fn default_ts_types() -> String {
    "frontend/src/types".to_string()
}

impl Default for CrossBoundaryConfig {
    fn default() -> Self {
        Self {
            rust_models: default_rust_models(),
            ts_types: default_ts_types(),
            strict_casing_check: true,
        }
    }
}

impl CanonConfig {
    /// Loads configuration from a specific file path.
    pub fn load(path: &Path) -> Result<Self, CoreError> {
        let content = std::fs::read_to_string(path).map_err(|source| CoreError::ConfigRead {
            path: path.to_path_buf(),
            source,
        })?;
        let config: Self = toml::from_str(&content).map_err(|source| CoreError::ConfigParse {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(config)
    }

    /// Automatically discovers `.canon.toml` or `.groundtruth.toml` from the current
    /// directory or its ancestors. Falls back to default settings if neither exists.
    pub fn discover() -> Self {
        if let Ok(current_dir) = std::env::current_dir() {
            let mut search_dir: &Path = &current_dir;
            loop {
                let candidate1 = search_dir.join(".canon.toml");
                if candidate1.is_file() {
                    if let Ok(cfg) = Self::load(&candidate1) {
                        return cfg;
                    }
                }
                let candidate2 = search_dir.join(".groundtruth.toml");
                if candidate2.is_file() {
                    if let Ok(cfg) = Self::load(&candidate2) {
                        return cfg;
                    }
                }
                match search_dir.parent() {
                    Some(parent) => search_dir = parent,
                    None => break,
                }
            }
        }
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_default_config() {
        let cfg = CanonConfig::default();
        assert_eq!(cfg.general.codebase_scale, "massive");
        assert_eq!(cfg.slice.max_call_depth, 2);
        assert_eq!(cfg.simulate.default_seeds, 5000);
        assert_eq!(cfg.verify.default_unwind, 4);
        assert_eq!(cfg.diff_fuzz.default_iterations, 50000);
        assert_eq!(cfg.daemon.socket_path, "/tmp/canon.sock");
    }

    #[test]
    fn test_load_config_file() {
        use std::io::Write;
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
[general]
codebase_scale = "small"
default_format = "json"

[slice]
max_call_depth = 5
"#
        )
        .unwrap();

        let cfg = CanonConfig::load(file.path()).unwrap();
        assert_eq!(cfg.general.codebase_scale, "small");
        assert_eq!(cfg.general.default_format, OutputFormat::Json);
        assert_eq!(cfg.slice.max_call_depth, 5);
        assert_eq!(cfg.simulate.default_seeds, 5000);
    }

    #[test]
    fn test_load_polyglot_config_file() {
        use std::io::Write;
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
[general]
codebase_scale = "massive"

[typescript]
enabled = true
tsconfig_path = "frontend/tsconfig.json"
extensions = ["ts", "tsx", "js", "jsx"]
strip_jsx = true

[cross_boundary]
rust_models = "crates/shared-types/src"
ts_types = "frontend/src/types"
strict_casing_check = true

[daemon]
watch_paths = ["crates", "frontend/src"]
"#
        )
        .unwrap();

        let cfg = CanonConfig::load(file.path()).unwrap();
        assert!(cfg.typescript.enabled);
        assert_eq!(cfg.typescript.tsconfig_path, "frontend/tsconfig.json");
        assert!(cfg.typescript.strip_jsx);
        assert_eq!(cfg.cross_boundary.rust_models, "crates/shared-types/src");
        assert_eq!(cfg.cross_boundary.ts_types, "frontend/src/types");
        assert!(cfg.cross_boundary.strict_casing_check);
        assert_eq!(cfg.daemon.watch_paths.len(), 2);
    }
}
