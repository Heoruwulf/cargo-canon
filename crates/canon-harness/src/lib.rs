//! # canon-harness
//!
//! Ground-truth verification oracles and testing harnesses.
//!
//! Provides deterministic simulation testing (DST), differential fuzzing,
//! bounded model checking integration, and metamorphic invariant verification.

#![forbid(unsafe_code)]

pub mod diff_fuzz;
pub mod error;
pub mod invariants;
pub mod simulate;
pub mod verify;

pub use diff_fuzz::{
    DiffFuzzOptions, DiffFuzzReport, FuzzOp, export_reproducer_test, run_diff_fuzz,
};
pub use error::HarnessError;
pub use invariants::{InvariantOptions, InvariantReport, run_invariants, scaffold_invariant_suite};
pub use simulate::{
    DstEngine, SimEvent, SimOutcome, SimReport, SimulateOptions, build_ai_repair_prompt,
    run_simulation,
};
pub use verify::{VerifyOptions, VerifyReport, parse_kani_json, run_verify, scaffold_kani_proof};
