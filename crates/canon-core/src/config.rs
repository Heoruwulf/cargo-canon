//! Configuration models and discovery for cargo-canon.

use crate::error::CoreError;
use crate::output::OutputFormat;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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
    /// Custom directory names to ignore during file discovery.
    #[serde(default)]
    pub custom_ignored_dirs: Vec<String>,
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
            custom_ignored_dirs: Vec::new(),
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
    /// Legacy Unix domain socket path override (None = auto-derived per workspace/worktree).
    #[serde(default)]
    pub socket_path: Option<String>,
    /// Whether this daemon operates in system-wide multi-tenant mode (default: true).
    #[serde(default = "default_true")]
    pub system_daemon: bool,
    /// Global Unix domain socket path override (None = auto-derived in XDG runtime dir).
    #[serde(default)]
    pub global_socket: Option<String>,
    /// Inactivity threshold in seconds before unindexing an idle workspace (default: 7200 = 2 hours).
    #[serde(default = "default_idle_ttl_secs")]
    pub idle_ttl_secs: u64,
    /// Directory path where linked worktrees reside (relative to workspace root).
    #[serde(default)]
    pub worktrees_dir: Option<String>,
    /// Additional directories to ignore machine-wide or project-wide.
    #[serde(default)]
    pub ignored_dirs: Vec<String>,
    /// Whether background filesystem watching is enabled across active workspaces.
    #[serde(default = "default_true")]
    pub enable_file_watcher: bool,
    /// Directory for file logging (None = default ~/.local/state/canon/logs).
    #[serde(default)]
    pub log_dir: Option<String>,
    /// Enable per-workspace isolated log files under <log_dir>/workspaces/ (default: true).
    #[serde(default = "default_true")]
    pub workspace_logging: bool,
    /// Paths to monitor for source code modifications within each registered workspace.
    #[serde(default = "default_watch_paths")]
    pub watch_paths: Vec<String>,
}

fn default_idle_ttl_secs() -> u64 {
    7200
}

fn default_watch_paths() -> Vec<String> {
    vec![
        "crates".to_string(),
        "frontend/src".to_string(),
        "backend".to_string(),
        "src".to_string(),
    ]
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            socket_path: None,
            system_daemon: true,
            global_socket: None,
            idle_ttl_secs: default_idle_ttl_secs(),
            worktrees_dir: None,
            ignored_dirs: Vec::new(),
            enable_file_watcher: true,
            log_dir: None,
            workspace_logging: true,
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

/// Resolves the global user configuration path:
/// `$XDG_CONFIG_HOME/canon/canon.toml` (default: `~/.config/canon/canon.toml`).
pub fn resolve_global_config_path() -> PathBuf {
    if let Some(xdg_config) = std::env::var_os("XDG_CONFIG_HOME") {
        if !xdg_config.is_empty() {
            return PathBuf::from(xdg_config).join("canon").join("canon.toml");
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home_path = PathBuf::from(home);
        #[cfg(target_os = "macos")]
        {
            let mac_path = home_path
                .join("Library")
                .join("Application Support")
                .join("canon")
                .join("canon.toml");
            if mac_path.exists() {
                return mac_path;
            }
        }
        return home_path.join(".config").join("canon").join("canon.toml");
    }
    std::env::temp_dir().join("canon").join("canon.toml")
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

    /// Merges an overlay configuration into `self`.
    pub fn merge(&mut self, overlay: CanonConfig) {
        let def = CanonConfig::default();
        self.merge_general_and_slice(&overlay, &def);
        self.merge_sim_and_verify(&overlay, &def);
        self.merge_daemon(&overlay, &def);
        self.merge_polyglot(&overlay, &def);
    }

    fn merge_general_and_slice(&mut self, overlay: &CanonConfig, def: &CanonConfig) {
        if overlay.general.codebase_scale != def.general.codebase_scale {
            self.general.codebase_scale = overlay.general.codebase_scale.clone();
        }
        if overlay.general.default_format != def.general.default_format {
            self.general.default_format = overlay.general.default_format;
        }
        if overlay.slice.max_call_depth != def.slice.max_call_depth {
            self.slice.max_call_depth = overlay.slice.max_call_depth;
        }
        if overlay.slice.strip_comments != def.slice.strip_comments {
            self.slice.strip_comments = overlay.slice.strip_comments;
        }
        if overlay.slice.include_private_types != def.slice.include_private_types {
            self.slice.include_private_types = overlay.slice.include_private_types;
        }
        if !overlay.slice.custom_ignored_dirs.is_empty() {
            self.slice
                .custom_ignored_dirs
                .extend(overlay.slice.custom_ignored_dirs.clone());
            self.slice.custom_ignored_dirs.sort();
            self.slice.custom_ignored_dirs.dedup();
        }
    }

    fn merge_sim_and_verify(&mut self, overlay: &CanonConfig, def: &CanonConfig) {
        if overlay.simulate.default_seeds != def.simulate.default_seeds {
            self.simulate.default_seeds = overlay.simulate.default_seeds;
        }
        if overlay.simulate.timeout_seconds != def.simulate.timeout_seconds {
            self.simulate.timeout_seconds = overlay.simulate.timeout_seconds;
        }
        if overlay.simulate.panic_hook != def.simulate.panic_hook {
            self.simulate.panic_hook = overlay.simulate.panic_hook.clone();
        }
        if overlay.verify.default_unwind != def.verify.default_unwind {
            self.verify.default_unwind = overlay.verify.default_unwind;
        }
        if overlay.verify.solver != def.verify.solver {
            self.verify.solver = overlay.verify.solver.clone();
        }
        if overlay.diff_fuzz.default_iterations != def.diff_fuzz.default_iterations {
            self.diff_fuzz.default_iterations = overlay.diff_fuzz.default_iterations;
        }
        if overlay.diff_fuzz.corpus_dir != def.diff_fuzz.corpus_dir {
            self.diff_fuzz.corpus_dir = overlay.diff_fuzz.corpus_dir.clone();
        }
    }

    fn merge_daemon(&mut self, overlay: &CanonConfig, def: &CanonConfig) {
        if overlay.daemon.socket_path.is_some() {
            self.daemon.socket_path = overlay.daemon.socket_path.clone();
        }
        if overlay.daemon.system_daemon != def.daemon.system_daemon {
            self.daemon.system_daemon = overlay.daemon.system_daemon;
        }
        if overlay.daemon.global_socket.is_some() {
            self.daemon.global_socket = overlay.daemon.global_socket.clone();
        }
        if overlay.daemon.idle_ttl_secs != def.daemon.idle_ttl_secs {
            self.daemon.idle_ttl_secs = overlay.daemon.idle_ttl_secs;
        }
        if overlay.daemon.worktrees_dir.is_some() {
            self.daemon.worktrees_dir = overlay.daemon.worktrees_dir.clone();
        }
        if !overlay.daemon.ignored_dirs.is_empty() {
            self.daemon
                .ignored_dirs
                .extend(overlay.daemon.ignored_dirs.clone());
            self.daemon.ignored_dirs.sort();
            self.daemon.ignored_dirs.dedup();
        }
        if overlay.daemon.enable_file_watcher != def.daemon.enable_file_watcher {
            self.daemon.enable_file_watcher = overlay.daemon.enable_file_watcher;
        }
        if overlay.daemon.log_dir.is_some() {
            self.daemon.log_dir = overlay.daemon.log_dir.clone();
        }
        if overlay.daemon.workspace_logging != def.daemon.workspace_logging {
            self.daemon.workspace_logging = overlay.daemon.workspace_logging;
        }
        if overlay.daemon.watch_paths != def.daemon.watch_paths {
            self.daemon.watch_paths = overlay.daemon.watch_paths.clone();
        }
    }

    fn merge_polyglot(&mut self, overlay: &CanonConfig, def: &CanonConfig) {
        if overlay.typescript.enabled != def.typescript.enabled {
            self.typescript.enabled = overlay.typescript.enabled;
        }
        if overlay.typescript.tsconfig_path != def.typescript.tsconfig_path {
            self.typescript.tsconfig_path = overlay.typescript.tsconfig_path.clone();
        }
        if overlay.typescript.extensions != def.typescript.extensions {
            self.typescript.extensions = overlay.typescript.extensions.clone();
        }
        if overlay.typescript.strip_jsx != def.typescript.strip_jsx {
            self.typescript.strip_jsx = overlay.typescript.strip_jsx;
        }
        if overlay.typescript.strip_hook_bodies != def.typescript.strip_hook_bodies {
            self.typescript.strip_hook_bodies = overlay.typescript.strip_hook_bodies;
        }
        if overlay.go.enabled != def.go.enabled {
            self.go.enabled = overlay.go.enabled;
        }
        if overlay.go.module_root != def.go.module_root {
            self.go.module_root = overlay.go.module_root.clone();
        }
        if overlay.go.extensions != def.go.extensions {
            self.go.extensions = overlay.go.extensions.clone();
        }
        if overlay.go.strip_function_bodies != def.go.strip_function_bodies {
            self.go.strip_function_bodies = overlay.go.strip_function_bodies;
        }
        if overlay.go.index_implicit_interfaces != def.go.index_implicit_interfaces {
            self.go.index_implicit_interfaces = overlay.go.index_implicit_interfaces;
        }
        if overlay.go.include_private_types != def.go.include_private_types {
            self.go.include_private_types = overlay.go.include_private_types;
        }
        if overlay.go.concurrency != def.go.concurrency {
            self.go.concurrency = overlay.go.concurrency.clone();
        }
        if overlay.go.state_machine != def.go.state_machine {
            self.go.state_machine = overlay.go.state_machine.clone();
        }
        if overlay.cross_boundary != def.cross_boundary {
            self.cross_boundary = overlay.cross_boundary.clone();
        }
    }

    /// Discovers configuration for a specific workspace root or worktree path.
    pub fn discover_for_workspace(root: &Path) -> Self {
        let mut config = Self::default();

        let global_path = resolve_global_config_path();
        if global_path.is_file() {
            if let Ok(global_cfg) = Self::load(&global_path) {
                config.merge(global_cfg);
            }
        }

        if let Some(parent_repo) = crate::workspace::detect_worktree_parent_repo(root) {
            for file_name in &[".canon.toml", ".groundtruth.toml"] {
                let parent_cfg_path = parent_repo.join(file_name);
                if parent_cfg_path.is_file() {
                    if let Ok(parent_cfg) = Self::load(&parent_cfg_path) {
                        config.merge(parent_cfg);
                        break;
                    }
                }
            }
        }

        let workspace_root = crate::workspace::resolve_workspace_root(root);
        for file_name in &[".canon.toml", ".groundtruth.toml"] {
            let local_cfg_path = workspace_root.join(file_name);
            if local_cfg_path.is_file() {
                if let Ok(local_cfg) = Self::load(&local_cfg_path) {
                    config.merge(local_cfg);
                    break;
                }
            }
        }

        config
    }

    /// Automatically discovers `.canon.toml` or `.groundtruth.toml` from the current
    /// directory or its ancestors. Falls back to default settings if neither exists.
    pub fn discover() -> Self {
        if let Ok(current_dir) = std::env::current_dir() {
            return Self::discover_for_workspace(&current_dir);
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

    #[test]
    fn test_daemon_config_multi_tenant_fields() {
        use std::io::Write;
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
[daemon]
system_daemon = true
global_socket = "/tmp/global.sock"
idle_ttl_secs = 3600
worktrees_dir = ".claude/worktrees"
ignored_dirs = [".claude", ".cursor"]
enable_file_watcher = false
log_dir = "/var/log/canon"
workspace_logging = true
watch_paths = ["crates", "src"]
"#
        )
        .unwrap();

        let cfg = CanonConfig::load(file.path()).unwrap();
        assert!(cfg.daemon.system_daemon);
        assert_eq!(
            cfg.daemon.global_socket,
            Some("/tmp/global.sock".to_string())
        );
        assert_eq!(cfg.daemon.idle_ttl_secs, 3600);
        assert_eq!(
            cfg.daemon.worktrees_dir,
            Some(".claude/worktrees".to_string())
        );
        assert_eq!(cfg.daemon.ignored_dirs, vec![".claude", ".cursor"]);
        assert!(!cfg.daemon.enable_file_watcher);
        assert_eq!(cfg.daemon.log_dir, Some("/var/log/canon".to_string()));
        assert!(cfg.daemon.workspace_logging);
        assert_eq!(cfg.daemon.watch_paths, vec!["crates", "src"]);
    }

    #[test]
    fn test_config_merge() {
        let mut base = CanonConfig::default();
        let mut overlay = CanonConfig::default();
        overlay.general.codebase_scale = "small".to_string();
        overlay.daemon.idle_ttl_secs = 1800;
        overlay.daemon.worktrees_dir = Some(".worktrees".to_string());
        overlay.daemon.ignored_dirs = vec![".tmp".to_string()];

        base.merge(overlay);
        assert_eq!(base.general.codebase_scale, "small");
        assert_eq!(base.daemon.idle_ttl_secs, 1800);
        assert_eq!(base.daemon.worktrees_dir, Some(".worktrees".to_string()));
        assert_eq!(base.daemon.ignored_dirs, vec![".tmp"]);
    }

    #[test]
    fn test_resolve_global_config_path() {
        let path = resolve_global_config_path();
        assert!(path.to_str().unwrap().contains("canon.toml"));
    }
}
