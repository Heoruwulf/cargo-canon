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

pub use config::CanonConfig;
pub use error::CoreError;
pub use logging::init_logging;
pub use model::SliceMode;
pub use output::OutputFormat;
pub use span::{
    AgentVendor, DEFAULT_AGY_BYTE_LIMIT, DEFAULT_AGY_END_LINE, DEFAULT_AGY_START_LINE,
    DEFAULT_CLAUDE_LIMIT, DEFAULT_CLAUDE_OFFSET, ParsedHookInput, ReadSpan, parse_hook_stdin,
};
pub use workspace::{
    derive_socket_path, resolve_active_socket, resolve_workspace_root, splitmix64, splitmix64_str,
};
