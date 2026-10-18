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
    /// Configuration for Go context slicing and verification.
    #[serde(default)]
    pub go: GoConfig,
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
    /// Unix domain socket path override (None = auto-derived per workspace/worktree).
    #[serde(default)]
    pub socket_path: Option<String>,
    /// Whether background filesystem watching is enabled.
    #[serde(default = "default_true")]
    pub enable_file_watcher: bool,
    /// Paths to monitor for source code modifications.
    #[serde(default = "default_watch_paths")]
    pub watch_paths: Vec<String>,
}

fn default_watch_paths() -> Vec<String> {
    vec!["crates".to_string(), "frontend/src".to_string()]
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            socket_path: None,
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
    /// Directory containing Go backend models.
    #[serde(default = "default_go_models")]
    pub go_models: String,
    /// Enforce camelCase vs snake_case consistency checks.
    #[serde(default = "default_true")]
    pub strict_casing_check: bool,
    /// Enforce omitempty zero-value discrepancy validation.
    #[serde(default = "default_true")]
    pub strict_omitempty_check: bool,
    /// Enforce nil slice vs empty slice serialization consistency.
    #[serde(default = "default_true")]
    pub strict_nil_slice_check: bool,
}

fn default_rust_models() -> String {
    "crates/shared-types/src".to_string()
}

fn default_ts_types() -> String {
    "frontend/src/types".to_string()
}

fn default_go_models() -> String {
    "backend/pkg/models".to_string()
}

impl Default for CrossBoundaryConfig {
    fn default() -> Self {
        Self {
            rust_models: default_rust_models(),
            ts_types: default_ts_types(),
            go_models: default_go_models(),
            strict_casing_check: true,
            strict_omitempty_check: true,
            strict_nil_slice_check: true,
        }
    }
}

/// Configuration for Go context slicing and verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoConfig {
    /// Enable Go parsing and slicing.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Root directory for Go modules or monorepo root.
    #[serde(default = "default_go_module_root")]
    pub module_root: String,
    /// Supported Go file extensions.
    #[serde(default = "default_go_extensions")]
    pub extensions: Vec<String>,
    /// Strip function and method bodies.
    #[serde(default = "default_true")]
    pub strip_function_bodies: bool,
    /// Compute implicit interface satisfaction matrix.
    #[serde(default = "default_true")]
    pub index_implicit_interfaces: bool,
    /// Include unexported (lowercase) types and helpers.
    #[serde(default)]
    pub include_private_types: bool,
    /// Tier 1: Concurrency and race verification settings.
    #[serde(default)]
    pub concurrency: GoConcurrencyConfig,
    /// Tier 2: Pure Sans-I/O state machine settings.
    #[serde(default)]
    pub state_machine: GoStateMachineConfig,
}

fn default_go_module_root() -> String {
    "backend".to_string()
}

fn default_go_extensions() -> Vec<String> {
    vec!["go".to_string()]
}

impl Default for GoConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            module_root: default_go_module_root(),
            extensions: default_go_extensions(),
            strip_function_bodies: true,
            index_implicit_interfaces: true,
            include_private_types: false,
            concurrency: GoConcurrencyConfig::default(),
            state_machine: GoStateMachineConfig::default(),
        }
    }
}

/// Tier 1: Go concurrency and goroutine leak options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoConcurrencyConfig {
    /// Enable Go race detector (TSan).
    #[serde(default = "default_true")]
    pub race_detector: bool,
    /// Enable uber-go/goleak inspection.
    #[serde(default = "default_true")]
    pub goleak_check: bool,
    /// GOMAXPROCS randomized sweep values.
    #[serde(default = "default_gomaxprocs")]
    pub gomaxprocs_sweep: Vec<usize>,
    /// Default number of passes.
    #[serde(default = "default_go_passes")]
    pub default_passes: u64,
}

fn default_gomaxprocs() -> Vec<usize> {
    vec![1, 2, 4, 8]
}

fn default_go_passes() -> u64 {
    100
}

impl Default for GoConcurrencyConfig {
    fn default() -> Self {
        Self {
            race_detector: true,
            goleak_check: true,
            gomaxprocs_sweep: default_gomaxprocs(),
            default_passes: default_go_passes(),
        }
    }
}

/// Tier 2: Pure Sans-I/O Go state machine verification options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoStateMachineConfig {
    /// Number of transitions to fuzz.
    #[serde(default = "default_fuzz_transitions")]
    pub fuzz_transitions: usize,
    /// Deterministic 64-bit seed (hex string).
    #[serde(default = "default_go_seed")]
    pub seed: String,
}

fn default_fuzz_transitions() -> usize {
    100_000
}

fn default_go_seed() -> String {
    "0x1337C0DECAFEE00D".to_string()
}

impl Default for GoStateMachineConfig {
    fn default() -> Self {
        Self {
            fuzz_transitions: default_fuzz_transitions(),
            seed: default_go_seed(),
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
        assert_eq!(cfg.daemon.socket_path, None);
    }

    #[test]
    fn test_daemon_config_socket_override() {
        use std::io::Write;
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
[daemon]
socket_path = "/tmp/custom-test.sock"
"#
        )
        .unwrap();

        let cfg = CanonConfig::load(file.path()).unwrap();
        assert_eq!(
            cfg.daemon.socket_path,
            Some("/tmp/custom-test.sock".to_string())
        );
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

    #[test]
    fn test_load_go_config() {
        use std::io::Write;
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
[general]
codebase_scale = "massive"

[go]
enabled = true
module_root = "backend"
extensions = ["go"]
strip_function_bodies = true
index_implicit_interfaces = true
include_private_types = false

[go.concurrency]
race_detector = true
goleak_check = true
gomaxprocs_sweep = [1, 2, 4, 8]
default_passes = 100

[go.state_machine]
fuzz_transitions = 100000
seed = "0x1337C0DECAFEE00D"

[cross_boundary]
rust_models = "crates/core/src/types"
ts_types = "frontend/src/types"
go_models = "backend/pkg/models"
strict_casing_check = true
strict_omitempty_check = true
strict_nil_slice_check = true
"#
        )
        .unwrap();

        let cfg = CanonConfig::load(file.path()).unwrap();
        assert!(cfg.go.enabled);
        assert_eq!(cfg.go.module_root, "backend");
        assert_eq!(cfg.go.extensions, vec!["go"]);
        assert!(cfg.go.strip_function_bodies);
        assert!(cfg.go.index_implicit_interfaces);
        assert!(!cfg.go.include_private_types);
        assert!(cfg.go.concurrency.race_detector);
        assert_eq!(cfg.go.concurrency.gomaxprocs_sweep, vec![1, 2, 4, 8]);
        assert_eq!(cfg.go.state_machine.fuzz_transitions, 100000);
        assert_eq!(cfg.cross_boundary.go_models, "backend/pkg/models");
        assert!(cfg.cross_boundary.strict_omitempty_check);
        assert!(cfg.cross_boundary.strict_nil_slice_check);
    }
}
