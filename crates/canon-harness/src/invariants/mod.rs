//! Metamorphic property testing module.

pub mod runner;

pub use runner::{InvariantOptions, InvariantReport, run_invariants, scaffold_invariant_suite};
