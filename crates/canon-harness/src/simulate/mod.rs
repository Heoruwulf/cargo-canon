//! Deterministic simulation testing (DST) module.

pub mod engine;
pub mod runner;

pub use engine::{DstEngine, SimEvent, SimOutcome};
pub use runner::{SimReport, SimulateOptions, build_ai_repair_prompt, run_simulation};
