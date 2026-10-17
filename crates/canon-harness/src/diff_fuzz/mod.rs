//! Differential fuzzing module.

pub mod runner;

pub use runner::{DiffFuzzOptions, DiffFuzzReport, FuzzOp, export_reproducer_test, run_diff_fuzz};
