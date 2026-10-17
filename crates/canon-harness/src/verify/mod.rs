//! Formal model checking and verification module.

pub mod runner;

pub use runner::{VerifyOptions, VerifyReport, parse_kani_json, run_verify, scaffold_kani_proof};
