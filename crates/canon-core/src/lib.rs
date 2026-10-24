//! # canon-core
//!
//! Shared foundation crate for cargo-canon.
//!
//! Provides CLI configuration discovery, structured error models, logging
//! initialization, and output formatting.

#![forbid(unsafe_code)]

pub mod config;
pub mod diagnostics;
pub mod error;
pub mod logging;
pub mod model;
pub mod output;
pub mod span;
pub mod workspace;

pub use config::{
    CanonConfig, ConfigProvenance, ProjectConfig, STANDARD_IGNORED_DIRS,
    WORKSPACE_CONFIG_CANDIDATES, find_config_in_dir, resolve_system_config_path,
};
pub use error::CoreError;
pub use logging::init_logging;
pub use model::SliceMode;
pub use output::OutputFormat;
#[allow(deprecated)]
pub use span::{
    AgentVendor, CLAUDE_WHOLE_FILE_MAX_BYTES, DEFAULT_AGY_BYTE_LIMIT, DEFAULT_AGY_END_LINE,
    DEFAULT_AGY_START_LINE, DEFAULT_CLAUDE_LIMIT, DEFAULT_CLAUDE_MAX_READ_TOKENS,
    DEFAULT_CLAUDE_OFFSET, EST_BYTES_PER_TOKEN, ParsedHookInput, ReadBudgetEnv, ReadSpan,
    detect_budget_env, parse_hook_stdin, resolve_read_budget,
};
pub use workspace::{
    derive_socket_path, resolve_active_socket, resolve_workspace_root, splitmix64, splitmix64_str,
};
