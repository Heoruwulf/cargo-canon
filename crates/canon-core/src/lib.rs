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

pub use config::CanonConfig;
pub use error::CoreError;
pub use logging::init_logging;
pub use model::SliceMode;
pub use output::OutputFormat;
