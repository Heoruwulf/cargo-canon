//! Configuration models and discovery for cargo-canon.

use crate::error::CoreError;
use crate::output::OutputFormat;
use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Standard universal build artifacts, dependency directories, and VCS metadata.
/// NOTE: `.worktrees` and `.claude` are intentionally EXCLUDED here because they may
/// contain nested Git worktree roots; those are pruned relative to each workspace root.
pub const STANDARD_IGNORED_DIRS: &[&str] = &[
    "target",
    ".git",
    "node_modules",
    "vendor",
    "dist",
    "build",
    "out",
    ".next",
    ".nuxt",
    ".turbo",
    ".yarn",
    ".pnpm-store",
    ".output",
    ".svelte-kit",
    "coverage",
    ".idea",
    ".vscode",
    ".cache",
];

/// Ordered list of workspace configuration file candidates relative to a workspace root.
pub const WORKSPACE_CONFIG_CANDIDATES: &[&str] = &[
    "config/canon.toml",
    ".canon.toml",
    "canon.toml",
    ".config/canon.toml",
    "configs/.canon.toml",
    "configs/canon.toml",
    ".groundtruth.toml",
    "groundtruth.toml",
];

/// Primary configuration structure for cargo-canon workspaces.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
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
    /// Explicitly namespaced projects: `[projects.<name>]`.
    #[serde(default)]
    pub projects: HashMap<String, ProjectConfig>,
    /// Top-level project tables: `[<project-name>]`.
    #[serde(flatten)]
    pub dynamic_projects: HashMap<String, toml::Value>,
}

impl Eq for CanonConfig {}

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
    /// Whether indexing is deferred lazily until first workspace touch (default: true).
    #[serde(default = "default_true")]
    pub lazy_indexing: bool,
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
            lazy_indexing: true,
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

/// Declarative per-project configuration for multi-repo environments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectConfig {
    /// Canonical qualified path to the project root directory.
    #[serde(default)]
    pub qualified_path: PathBuf,
    /// Whether this project employs Git worktrees.
    #[serde(default)]
    pub has_worktrees: bool,
    /// Custom path or subdirectory where worktrees reside.
    #[serde(default, alias = "has_custom_worktree_path")]
    pub custom_worktree_path: Option<PathBuf>,
    /// Languages enabled for this project (e.g. rust, typescript, go).
    #[serde(default, deserialize_with = "deserialize_lang_list")]
    pub lang: Vec<String>,

    /// Codebase scale hint: "small", "medium", "massive".
    #[serde(default = "default_codebase_scale")]
    pub codebase_scale: String,
    /// Default output format ("markdown", "json", "text").
    #[serde(default)]
    pub default_format: OutputFormat,

    /// Subdirectories to monitor for source code modifications within this project.
    /// Empty means all non-ignored paths under the workspace root are monitored.
    #[serde(default)]
    pub watch_paths: Vec<String>,
    /// Additional directory names to ignore during indexing and watching.
    #[serde(default)]
    pub ignored_dirs: Vec<String>,

    /// Slicing call-graph exploration depth bound.
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

    /// Per-project slicing options.
    #[serde(default)]
    pub slice: SliceConfig,
    /// Per-project TypeScript options.
    #[serde(default)]
    pub typescript: TypeScriptConfig,
    /// Per-project Go options.
    #[serde(default)]
    pub go: GoConfig,
    /// Per-project cross-boundary contract options.
    #[serde(default)]
    pub cross_boundary: CrossBoundaryConfig,
    /// Per-project simulation options.
    #[serde(default)]
    pub simulate: SimulateConfig,
    /// Per-project formal verification options.
    #[serde(default)]
    pub verify: VerifyConfig,
    /// Per-project differential fuzzing options.
    #[serde(default)]
    pub diff_fuzz: DiffFuzzConfig,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            qualified_path: PathBuf::new(),
            has_worktrees: false,
            custom_worktree_path: None,
            lang: Vec::new(),
            codebase_scale: default_codebase_scale(),
            default_format: OutputFormat::Markdown,
            watch_paths: Vec::new(),
            ignored_dirs: Vec::new(),
            max_call_depth: default_max_call_depth(),
            strip_comments: false,
            include_private_types: true,
            custom_ignored_dirs: Vec::new(),
            slice: SliceConfig::default(),
            typescript: TypeScriptConfig::default(),
            go: GoConfig::default(),
            cross_boundary: CrossBoundaryConfig::default(),
            simulate: SimulateConfig::default(),
            verify: VerifyConfig::default(),
            diff_fuzz: DiffFuzzConfig::default(),
        }
    }
}

fn normalize_rel_pattern(pattern: &str) -> &Path {
    let p = Path::new(pattern.trim());
    p.strip_prefix(".").unwrap_or(p)
}

impl ProjectConfig {
    /// Constructs a `ProjectConfig` synthesized from a `CanonConfig`.
    pub fn from_canon_config(root: PathBuf, cfg: &CanonConfig) -> Self {
        let mut proj = Self {
            qualified_path: root,
            has_worktrees: cfg.daemon.worktrees_dir.is_some(),
            custom_worktree_path: cfg.daemon.worktrees_dir.as_ref().map(PathBuf::from),
            lang: Vec::new(),
            codebase_scale: cfg.general.codebase_scale.clone(),
            default_format: cfg.general.default_format,
            watch_paths: Vec::new(),
            ignored_dirs: cfg.daemon.ignored_dirs.clone(),
            max_call_depth: cfg.slice.max_call_depth,
            strip_comments: cfg.slice.strip_comments,
            include_private_types: cfg.slice.include_private_types,
            custom_ignored_dirs: cfg.slice.custom_ignored_dirs.clone(),
            slice: cfg.slice.clone(),
            typescript: cfg.typescript.clone(),
            go: cfg.go.clone(),
            cross_boundary: cfg.cross_boundary.clone(),
            simulate: cfg.simulate.clone(),
            verify: cfg.verify.clone(),
            diff_fuzz: cfg.diff_fuzz.clone(),
        };
        proj.normalize();
        proj
    }

    /// Normalizes inline scalar fields with nested sub-table configurations.
    pub fn normalize(&mut self) {
        if self.max_call_depth != default_max_call_depth()
            && self.slice.max_call_depth == default_max_call_depth()
        {
            self.slice.max_call_depth = self.max_call_depth;
        }
        if self.strip_comments && !self.slice.strip_comments {
            self.slice.strip_comments = self.strip_comments;
        }
        if !self.include_private_types && self.slice.include_private_types {
            self.slice.include_private_types = self.include_private_types;
        }
        if !self.custom_ignored_dirs.is_empty() {
            self.slice
                .custom_ignored_dirs
                .extend(self.custom_ignored_dirs.clone());
            self.slice.custom_ignored_dirs.sort();
            self.slice.custom_ignored_dirs.dedup();
        }
    }

    /// Determines whether a path relative to the workspace root should be ignored,
    /// directly evaluating universal build artifacts, project-specific `ignored_dirs`,
    /// and slicing `custom_ignored_dirs`.
    pub fn is_relative_path_ignored(&self, rel: &Path) -> bool {
        let rel = rel.strip_prefix(".").unwrap_or(rel);
        if self
            .ignored_dirs
            .iter()
            .chain(&self.custom_ignored_dirs)
            .chain(&self.slice.custom_ignored_dirs)
            .any(|d| {
                let pat = normalize_rel_pattern(d);
                !pat.as_os_str().is_empty() && rel.starts_with(pat)
            })
        {
            return true;
        }
        for comp in rel.components() {
            if let std::path::Component::Normal(c) = comp {
                if let Some(s) = c.to_str() {
                    if STANDARD_IGNORED_DIRS.contains(&s) {
                        return true;
                    }
                    if self
                        .ignored_dirs
                        .iter()
                        .chain(&self.custom_ignored_dirs)
                        .chain(&self.slice.custom_ignored_dirs)
                        .any(|d| normalize_rel_pattern(d).as_os_str() == s)
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Returns `true` if a workspace-relative directory `rel_dir` is within or is an ancestor
    /// of any configured `watch_paths` entry (or if `watch_paths` is empty/unrestricted).
    pub fn is_dir_in_watch_paths(&self, rel_dir: &Path) -> bool {
        let rel_dir = rel_dir.strip_prefix(".").unwrap_or(rel_dir);
        if self.watch_paths.is_empty() || rel_dir.as_os_str().is_empty() {
            return true;
        }
        self.watch_paths.iter().any(|wp| {
            if wp.trim().is_empty() {
                return false;
            }
            let wp_path = normalize_rel_pattern(wp);
            wp_path.as_os_str().is_empty()
                || rel_dir.starts_with(wp_path)
                || wp_path.starts_with(rel_dir)
        })
    }

    /// Returns `true` if a workspace-relative file `rel_file` resides inside any configured
    /// `watch_paths` entry (or if `watch_paths` is empty/unrestricted).
    pub fn is_file_in_watch_paths(&self, rel_file: &Path) -> bool {
        let rel_file = rel_file.strip_prefix(".").unwrap_or(rel_file);
        if self.watch_paths.is_empty() {
            return true;
        }
        self.watch_paths.iter().any(|wp| {
            if wp.trim().is_empty() {
                return false;
            }
            let wp_path = normalize_rel_pattern(wp);
            wp_path.as_os_str().is_empty() || rel_file.starts_with(wp_path)
        })
    }

    /// Checks whether a file extension is enabled by this project's `lang` setting.
    pub fn is_extension_enabled(&self, ext: &str) -> bool {
        if self.lang.is_empty() {
            return matches!(ext, "rs" | "ts" | "tsx" | "js" | "jsx" | "go");
        }
        match ext {
            "rs" => self.lang.iter().any(|l| l == "rust" || l == "rs"),
            "ts" | "tsx" | "js" | "jsx" => {
                self.typescript.enabled
                    && self.lang.iter().any(|l| {
                        matches!(
                            l.as_str(),
                            "typescript" | "ts" | "tsx" | "javascript" | "js"
                        )
                    })
            }
            "go" => self.go.enabled && self.lang.iter().any(|l| l == "go" || l == "golang"),
            _ => false,
        }
    }
}

/// Flexible deserializer accepting either a comma-separated string ("rust,typescript")
/// or an array of strings (["rust", "typescript"]).
fn deserialize_lang_list<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum LangInput {
        List(Vec<String>),
        CommaString(String),
    }

    match LangInput::deserialize(deserializer)? {
        LangInput::List(list) => Ok(list
            .into_iter()
            .map(|part| part.trim().to_ascii_lowercase())
            .filter(|part| !part.is_empty())
            .collect()),
        LangInput::CommaString(s) => Ok(s
            .split(',')
            .map(|part| part.trim().to_ascii_lowercase())
            .filter(|part| !part.is_empty())
            .collect()),
    }
}

/// Tracks the canonical file paths of all configuration layers merged during discovery.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigProvenance {
    /// Explicit `--config` override path, if supplied.
    pub custom_path: Option<PathBuf>,
    /// System/global configuration file path (`~/.config/canon/canon.toml` or `/etc/canon/canon.toml`).
    pub global_path: Option<PathBuf>,
    /// Parent repository configuration file path when operating inside a linked Git worktree.
    pub parent_repo_path: Option<PathBuf>,
    /// Local workspace configuration file path (`<root>/config/canon.toml`, `<root>/.canon.toml`, etc.).
    pub local_workspace_path: Option<PathBuf>,
}

impl ConfigProvenance {
    /// Returns the most specific configuration path loaded (custom -> local -> parent -> global).
    pub fn primary_path(&self) -> Option<&Path> {
        self.custom_path
            .as_deref()
            .or(self.local_workspace_path.as_deref())
            .or(self.parent_repo_path.as_deref())
            .or(self.global_path.as_deref())
    }

    /// Emits structured tracing logs detailing all resolved configuration layers.
    pub fn log_details(&self) {
        if let Some(ref p) = self.custom_path {
            tracing::info!(path = %p.display(), layer = "custom", "Loaded custom configuration");
        }
        if let Some(ref p) = self.global_path {
            tracing::debug!(path = %p.display(), layer = "global", "Loaded global configuration");
        }
        if let Some(ref p) = self.parent_repo_path {
            tracing::debug!(path = %p.display(), layer = "parent_repo", "Loaded worktree parent repository configuration");
        }
        if let Some(ref p) = self.local_workspace_path {
            tracing::debug!(path = %p.display(), layer = "local_workspace", "Loaded workspace configuration");
        }
    }
}

/// Searches `dir` for the first existing configuration file matching `WORKSPACE_CONFIG_CANDIDATES`
/// and returns its canonicalized `PathBuf`.
pub fn find_config_in_dir(dir: &Path) -> Option<PathBuf> {
    for rel_candidate in WORKSPACE_CONFIG_CANDIDATES {
        let candidate = dir.join(rel_candidate);
        if candidate.is_file() {
            return std::fs::canonicalize(&candidate).ok().or(Some(candidate));
        }
    }
    None
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

/// Resolves candidate system-level configuration paths:
/// 1. `$XDG_CONFIG_HOME/canon/canon.toml`
/// 2. `~/.config/canon/canon.toml`
/// 3. `~/Library/Application Support/canon/canon.toml` (macOS)
/// 4. `/etc/canon/canon.toml` (Linux / Docker / K8s system path)
/// 5. `/etc/canon/config.toml` (Kubernetes ConfigMap standard)
pub fn resolve_system_config_path() -> Option<PathBuf> {
    if let Some(xdg_config) = std::env::var_os("XDG_CONFIG_HOME") {
        if !xdg_config.is_empty() {
            let p = PathBuf::from(xdg_config).join("canon").join("canon.toml");
            if p.is_file() {
                return std::fs::canonicalize(&p).ok().or(Some(p));
            }
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
            if mac_path.is_file() {
                return std::fs::canonicalize(&mac_path).ok().or(Some(mac_path));
            }
        }
        let user_path = home_path.join(".config").join("canon").join("canon.toml");
        if user_path.is_file() {
            return std::fs::canonicalize(&user_path).ok().or(Some(user_path));
        }
    }
    let etc_canon = PathBuf::from("/etc/canon/canon.toml");
    if etc_canon.is_file() {
        return std::fs::canonicalize(&etc_canon).ok().or(Some(etc_canon));
    }
    let etc_config = PathBuf::from("/etc/canon/config.toml");
    if etc_config.is_file() {
        return std::fs::canonicalize(&etc_config).ok().or(Some(etc_config));
    }
    None
}

fn canonicalize_best_effort(path: &Path) -> PathBuf {
    if let Ok(c) = std::fs::canonicalize(path) {
        return c;
    }
    for ancestor in path.ancestors() {
        if let Ok(c_anc) = std::fs::canonicalize(ancestor) {
            if let Ok(rel) = path.strip_prefix(ancestor) {
                return c_anc.join(rel);
            }
        }
    }
    path.to_path_buf()
}

impl CanonConfig {
    /// Loads configuration from a specific file path.
    pub fn load(path: &Path) -> Result<Self, CoreError> {
        let content = std::fs::read_to_string(path).map_err(|source| CoreError::ConfigRead {
            path: path.to_path_buf(),
            source,
        })?;
        let mut config: Self =
            toml::from_str(&content).map_err(|source| CoreError::ConfigParse {
                path: path.to_path_buf(),
                source,
            })?;
        config.normalize_projects();
        Ok(config)
    }

    /// Normalizes top-level custom project sections into `self.projects`.
    pub fn normalize_projects(&mut self) {
        let reserved = [
            "general",
            "slice",
            "simulate",
            "verify",
            "diff_fuzz",
            "daemon",
            "typescript",
            "go",
            "cross_boundary",
            "projects",
        ];
        let dynamic = std::mem::take(&mut self.dynamic_projects);
        for (key, val) in dynamic {
            if !reserved.contains(&key.as_str()) {
                match val.try_into::<ProjectConfig>() {
                    Ok(mut proj_cfg) => {
                        proj_cfg.normalize();
                        self.projects.entry(key).or_insert(proj_cfg);
                    }
                    Err(err) => {
                        tracing::warn!(
                            project = %key,
                            error = %err,
                            "Ignoring malformed top-level project configuration section"
                        );
                    }
                }
            }
        }
        for proj in self.projects.values_mut() {
            proj.normalize();
        }
    }

    /// Finds the most specific declared project configuration that encompasses the given path
    /// using deterministic longest-prefix matching (with lexicographical project-name tie-breaking).
    pub fn find_project_for_path(&self, target_path: &Path) -> Option<(&str, &ProjectConfig)> {
        let canonical_target = canonicalize_best_effort(target_path);
        let mut best: Option<(usize, &str, &ProjectConfig)> = None;

        for (name, proj) in &self.projects {
            if proj.qualified_path.as_os_str().is_empty() {
                continue;
            }
            let canonical_proj = canonicalize_best_effort(&proj.qualified_path);
            let mut matched_len = if canonical_target.starts_with(&canonical_proj) {
                Some(canonical_proj.as_os_str().len())
            } else {
                None
            };

            if let Some(ref wt_path) = proj.custom_worktree_path {
                let full_wt_path = if wt_path.is_relative() {
                    canonical_proj.join(wt_path)
                } else {
                    wt_path.clone()
                };
                let canonical_wt = canonicalize_best_effort(&full_wt_path);
                if canonical_target.starts_with(&canonical_wt) {
                    let wt_len = canonical_wt.as_os_str().len();
                    matched_len = Some(matched_len.map_or(wt_len, |l| l.max(wt_len)));
                }
            }

            if let Some(len) = matched_len {
                let is_better = match best {
                    None => true,
                    Some((best_len, best_name, _)) => {
                        len > best_len || (len == best_len && name.as_str() < best_name)
                    }
                };
                if is_better {
                    best = Some((len, name.as_str(), proj));
                }
            }
        }
        best.map(|(_, name, proj)| (name, proj))
    }

    /// Loads configuration specifically for the daemon runtime.
    ///
    /// If `custom_path` is provided (via `--config`), loads directly from that file.
    /// Otherwise, loads strictly from the system path. Never loads or merges CWD configuration.
    pub fn load_daemon_config(
        custom_path: Option<&Path>,
    ) -> Result<(Self, Option<PathBuf>), CoreError> {
        Self::load_daemon_config_with_system(custom_path, resolve_system_config_path())
    }

    fn load_daemon_config_with_system(
        custom_path: Option<&Path>,
        system_path: Option<PathBuf>,
    ) -> Result<(Self, Option<PathBuf>), CoreError> {
        let (mut cfg, path) = if let Some(path) = custom_path {
            let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
            let cfg = Self::load(&canonical)?;
            (cfg, Some(canonical))
        } else if let Some(sys_path) = system_path {
            let cfg = Self::load(&sys_path)?;
            (cfg, Some(sys_path))
        } else {
            (Self::default(), None)
        };

        cfg.normalize_projects();
        Ok((cfg, path))
    }

    /// Merges an overlay configuration into `self`.
    pub fn merge(&mut self, overlay: CanonConfig) {
        let def = CanonConfig::default();
        self.merge_general_and_slice(&overlay, &def);
        self.merge_sim_and_verify(&overlay, &def);
        self.merge_daemon(&overlay, &def);
        self.merge_polyglot(&overlay, &def);
        for (name, mut proj) in overlay.projects {
            proj.normalize();
            self.projects.insert(name, proj);
        }
        for (name, val) in overlay.dynamic_projects {
            self.dynamic_projects.insert(name, val);
        }
        self.normalize_projects();
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
        if overlay.daemon.lazy_indexing != def.daemon.lazy_indexing {
            self.daemon.lazy_indexing = overlay.daemon.lazy_indexing;
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

    fn try_merge_layer(&mut self, path: PathBuf, layer: &str) -> Option<PathBuf> {
        match Self::load(&path) {
            Ok(cfg) => {
                self.merge(cfg);
                Some(path)
            }
            Err(err) => {
                tracing::warn!(path = %path.display(), layer, error = %err, "Failed to load configuration layer; skipping");
                None
            }
        }
    }

    /// Discovers configuration for a specific workspace root or worktree path,
    /// returning both the merged configuration and the provenance of all loaded layers.
    pub fn discover_for_workspace_with_provenance(root: &Path) -> (Self, ConfigProvenance) {
        let mut config = Self::default();
        let mut provenance = ConfigProvenance::default();

        if let Some(global_path) = resolve_system_config_path() {
            provenance.global_path = config.try_merge_layer(global_path, "global");
        }
        if let Some(parent_repo) = crate::workspace::detect_worktree_parent_repo(root) {
            if let Some(parent_cfg_path) = find_config_in_dir(&parent_repo) {
                provenance.parent_repo_path =
                    config.try_merge_layer(parent_cfg_path, "parent_repo");
            }
        }
        let workspace_root = crate::workspace::resolve_workspace_root(root);
        if let Some(local_cfg_path) = find_config_in_dir(&workspace_root) {
            if provenance.parent_repo_path.as_ref() != Some(&local_cfg_path) {
                provenance.local_workspace_path =
                    config.try_merge_layer(local_cfg_path, "local_workspace");
            } else {
                provenance.local_workspace_path = Some(local_cfg_path);
            }
        }

        config.normalize_projects();
        (config, provenance)
    }

    /// Discovers configuration for a specific workspace root or worktree path.
    pub fn discover_for_workspace(root: &Path) -> Self {
        Self::discover_for_workspace_with_provenance(root).0
    }

    /// Automatically discovers workspace configuration from the current directory
    /// or its ancestors, returning both the configuration and its provenance.
    pub fn discover_with_provenance() -> (Self, ConfigProvenance) {
        if let Ok(current_dir) = std::env::current_dir() {
            return Self::discover_for_workspace_with_provenance(&current_dir);
        }
        (Self::default(), ConfigProvenance::default())
    }

    /// Automatically discovers `.canon.toml` or `.groundtruth.toml` from the current
    /// directory or its ancestors. Falls back to default settings if neither exists.
    pub fn discover() -> Self {
        Self::discover_with_provenance().0
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

    #[test]
    fn test_project_config_deserialization_and_lookup() {
        use std::io::Write;
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
[daemon]
system_daemon = true
lazy_indexing = true

[acme-web]
qualified_path = "/workspace/projects/acme-web"
has_worktrees = true
has_custom_worktree_path = "/workspace/projects/acme-web/.claude/worktrees"
lang = "rust,typescript"
codebase_scale = "massive"
watch_paths = ["crates", "frontend/src"]
ignored_dirs = [".claude", ".cursor"]
max_call_depth = 3
strip_comments = true
include_private_types = false
custom_ignored_dirs = ["dist"]

[acme-web.typescript]
enabled = true
tsconfig_path = "frontend/tsconfig.json"

[acme-web.cross_boundary]
rust_models = "crates/shared-types/src"
ts_types = "frontend/src/types"

[acme-web.simulate]
default_seeds = 9000

[payment-gateway]
qualified_path = "/workspace/projects/payment-gateway"
has_worktrees = false
lang = ["go"]
codebase_scale = "medium"
watch_paths = ["backend", "pkg"]
ignored_dirs = ["vendor"]

[payment-gateway.go]
enabled = true
module_root = "backend"
"#
        )
        .unwrap();

        let (cfg, loaded_path) = CanonConfig::load_daemon_config(Some(file.path())).unwrap();
        assert!(loaded_path.is_some());
        assert!(cfg.daemon.lazy_indexing);
        assert_eq!(cfg.projects.len(), 2);

        let acme = cfg.projects.get("acme-web").unwrap();
        assert!(acme.has_worktrees);
        assert_eq!(acme.lang, vec!["rust", "typescript"]);
        assert_eq!(acme.slice.max_call_depth, 3);
        assert!(acme.slice.strip_comments);
        assert!(!acme.slice.include_private_types);
        assert_eq!(acme.slice.custom_ignored_dirs, vec!["dist"]);
        assert_eq!(acme.simulate.default_seeds, 9000);
        assert!(acme.is_extension_enabled("rs"));
        assert!(acme.is_extension_enabled("tsx"));
        assert!(!acme.is_extension_enabled("go"));
        assert!(acme.is_relative_path_ignored(Path::new(".claude/logs.txt")));
        assert!(acme.is_relative_path_ignored(Path::new("target/debug/main")));
        assert!(acme.is_relative_path_ignored(Path::new("dist/bundle.js")));
        assert!(!acme.is_relative_path_ignored(Path::new("crates/core/src/lib.rs")));
        assert!(acme.is_dir_in_watch_paths(Path::new("")));
        assert!(acme.is_dir_in_watch_paths(Path::new("crates/core")));
        assert!(acme.is_dir_in_watch_paths(Path::new("frontend")));
        assert!(!acme.is_dir_in_watch_paths(Path::new("scripts")));
        assert!(acme.is_file_in_watch_paths(Path::new("crates/core/src/lib.rs")));
        assert!(acme.is_file_in_watch_paths(Path::new("frontend/src/App.tsx")));
        assert!(!acme.is_file_in_watch_paths(Path::new("frontend/public/index.html")));

        let pg = cfg.projects.get("payment-gateway").unwrap();
        assert_eq!(pg.lang, vec!["go"]);
        assert!(pg.is_extension_enabled("go"));
        assert!(!pg.is_extension_enabled("rs"));

        let (matched_name, _) = cfg
            .find_project_for_path(Path::new(
                "/workspace/projects/acme-web/.claude/worktrees/wt-1/src/lib.rs",
            ))
            .unwrap();
        assert_eq!(matched_name, "acme-web");
    }

    #[test]
    fn test_longest_prefix_project_matching_and_system_config_error() {
        use std::io::Write;
        let mut cfg = CanonConfig::default();
        let root_proj = ProjectConfig {
            qualified_path: PathBuf::from("/workspace/monorepo"),
            ..Default::default()
        };
        let sub_proj = ProjectConfig {
            qualified_path: PathBuf::from("/workspace/monorepo/services/payments"),
            watch_paths: vec!["./src".to_string()],
            ignored_dirs: vec!["./generated/proto".to_string()],
            ..Default::default()
        };
        assert!(sub_proj.is_relative_path_ignored(Path::new("generated/proto/api.rs")));
        assert!(sub_proj.is_relative_path_ignored(Path::new("./generated/proto/api.rs")));
        assert!(sub_proj.is_dir_in_watch_paths(Path::new("src/handlers")));
        assert!(sub_proj.is_file_in_watch_paths(Path::new("src/main.rs")));
        assert!(!sub_proj.is_file_in_watch_paths(Path::new("scripts/deploy.rs")));

        cfg.projects.insert("z-root".to_string(), root_proj.clone());
        cfg.projects.insert("a-payments".to_string(), sub_proj);
        assert_eq!(
            cfg.find_project_for_path(Path::new("/workspace/monorepo/services/payments/main.rs"))
                .unwrap()
                .0,
            "a-payments"
        );
        assert_eq!(
            cfg.find_project_for_path(Path::new("/workspace/monorepo/crates/core/lib.rs"))
                .unwrap()
                .0,
            "z-root"
        );

        let mut bad_sys = NamedTempFile::new().unwrap();
        writeln!(bad_sys, "[daemon\ninvalid_toml = ").unwrap();
        assert!(
            CanonConfig::load_daemon_config_with_system(None, Some(bad_sys.path().to_path_buf()))
                .is_err()
        );
    }

    #[test]
    fn test_discover_for_workspace_with_provenance() {
        let dir = tempfile::tempdir().unwrap();
        let cfg_dir = dir.path().join("config");
        std::fs::create_dir_all(&cfg_dir).unwrap();
        let cfg_file = cfg_dir.join("canon.toml");
        std::fs::write(&cfg_file, "[general]\ncodebase_scale = \"small\"\n").unwrap();

        let sub = dir.path().join("crates/core");
        std::fs::create_dir_all(&sub).unwrap();

        let (cfg, prov) = CanonConfig::discover_for_workspace_with_provenance(&sub);
        assert_eq!(cfg.general.codebase_scale, "small");
        assert_eq!(
            prov.local_workspace_path.as_deref(),
            Some(std::fs::canonicalize(&cfg_file).unwrap().as_path())
        );
        assert!(prov.primary_path().is_some());
    }
}
