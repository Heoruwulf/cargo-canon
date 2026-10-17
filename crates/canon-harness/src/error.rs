//! Error primitives for verification harnesses and oracles.

use std::path::PathBuf;
use thiserror::Error;

/// Errors produced during simulation, differential fuzzing, and formal verification.
#[derive(Error, Debug)]
pub enum HarnessError {
    /// Deterministic simulation invariant violation or failure.
    #[error("simulation failure at seed {seed:#018x} (tick {tick}): {reason}")]
    SimulationFailed {
        /// Failing random seed.
        seed: u64,
        /// Simulated tick when failure occurred.
        tick: u64,
        /// Failure reason or panic message.
        reason: String,
        /// Execution trace log.
        trace: Vec<String>,
    },

    /// Differential fuzzing detected a behavioral divergence.
    #[error(
        "differential discrepancy between legacy '{legacy}' and candidate '{candidate}': {reason}"
    )]
    DifferentialDivergence {
        /// Legacy implementation name.
        legacy: String,
        /// Candidate implementation name.
        candidate: String,
        /// Explanation of discrepancy.
        reason: String,
        /// Minimized failing input representation.
        failing_input: String,
    },

    /// Model checking / Kani proof failed.
    #[error("formal verification proof '{harness}' failed: {counterexample}")]
    VerificationFailed {
        /// Harness name.
        harness: String,
        /// Formatted counterexample struct initializer.
        counterexample: String,
    },

    /// Target harness or proof was not found.
    #[error("harness '{0}' not found")]
    HarnessNotFound(String),

    /// External process failure (e.g. running cargo kani).
    #[error("command execution failed: {0}")]
    Execution(String),

    /// I/O error writing reproducer or reading files.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// File path error.
    #[error("path error for '{path}': {message}")]
    PathError {
        /// Path that encountered an error.
        path: PathBuf,
        /// Description of error.
        message: String,
    },
}
