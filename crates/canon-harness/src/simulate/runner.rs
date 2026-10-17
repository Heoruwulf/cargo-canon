//! Simulation runner, seed management, and AI prompt formatting.

use crate::error::HarnessError;
use crate::simulate::engine::DstEngine;
use serde::{Deserialize, Serialize};

/// CLI simulation flags and options.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulateOptions {
    /// Total random seeds to evaluate.
    pub seeds: u64,
    /// Specific single seed to replay deterministically.
    pub seed: Option<u64>,
    /// Maximum ticks per seed before timing out / deadlocking.
    pub max_ticks: u64,
    /// Probability of simulated network packet loss or delay.
    pub fault_rate: f64,
    /// Generate actionable agent repair prompt if a seed fails.
    pub ai_prompt_on_fail: bool,
}

impl Default for SimulateOptions {
    fn default() -> Self {
        Self {
            seeds: 5000,
            seed: None,
            max_ticks: 10_000,
            fault_rate: 0.05,
            ai_prompt_on_fail: true,
        }
    }
}

/// Aggregated simulation execution report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimReport {
    /// Number of seeds evaluated.
    pub seeds_run: u64,
    /// Number of seeds that passed without fault or invariant violation.
    pub seeds_passed: u64,
    /// Failing seed if a defect was detected.
    pub failing_seed: Option<u64>,
    /// Tick of failure.
    pub failure_tick: Option<u64>,
    /// Reason for failure.
    pub failure_reason: Option<String>,
    /// Actionable AI repair prompt formatted for Claude / AGY.
    pub ai_repair_prompt: Option<String>,
}

/// Runs deterministic simulation pass over requested seeds.
pub fn run_simulation(opts: &SimulateOptions) -> Result<SimReport, HarnessError> {
    if let Some(target_seed) = opts.seed {
        return run_single_seed(target_seed, opts);
    }

    let mut seeds_passed = 0;

    for i in 0..opts.seeds {
        // Generate pseudo-random 64-bit seed deterministically from iteration index
        let current_seed = splitmix64(0xDEAD_BEEF_0000_0000 ^ i);
        let outcome = execute_standard_workload(current_seed, opts.max_ticks, opts.fault_rate);

        if let Some(err_msg) = outcome.failure {
            let ai_prompt = if opts.ai_prompt_on_fail {
                Some(build_ai_repair_prompt(
                    current_seed,
                    outcome.ticks,
                    &err_msg,
                    &outcome.trace,
                ))
            } else {
                None
            };

            return Ok(SimReport {
                seeds_run: i + 1,
                seeds_passed,
                failing_seed: Some(current_seed),
                failure_tick: Some(outcome.ticks),
                failure_reason: Some(err_msg),
                ai_repair_prompt: ai_prompt,
            });
        }

        seeds_passed += 1;
    }

    Ok(SimReport {
        seeds_run: opts.seeds,
        seeds_passed,
        failing_seed: None,
        failure_tick: None,
        failure_reason: None,
        ai_repair_prompt: None,
    })
}

fn run_single_seed(seed: u64, opts: &SimulateOptions) -> Result<SimReport, HarnessError> {
    let outcome = execute_standard_workload(seed, opts.max_ticks, opts.fault_rate);
    if let Some(err_msg) = outcome.failure {
        let ai_prompt = if opts.ai_prompt_on_fail {
            Some(build_ai_repair_prompt(
                seed,
                outcome.ticks,
                &err_msg,
                &outcome.trace,
            ))
        } else {
            None
        };

        return Ok(SimReport {
            seeds_run: 1,
            seeds_passed: 0,
            failing_seed: Some(seed),
            failure_tick: Some(outcome.ticks),
            failure_reason: Some(err_msg),
            ai_repair_prompt: ai_prompt,
        });
    }

    Ok(SimReport {
        seeds_run: 1,
        seeds_passed: 1,
        failing_seed: None,
        failure_tick: None,
        failure_reason: None,
        ai_repair_prompt: None,
    })
}

fn execute_standard_workload(
    seed: u64,
    max_ticks: u64,
    fault_rate: f64,
) -> crate::simulate::engine::SimOutcome {
    let mut engine = DstEngine::new(seed, max_ticks, fault_rate);

    // Seed initial message exchanges across simulated actors
    engine.schedule(1, "HandshakeSyn".into(), 5);
    engine.schedule(2, "HandshakeAck".into(), 10);
    engine.schedule(1, "PayloadReady".into(), 15);

    let mut state_count = 0;

    engine.run_until_quiescent(move |eng, event| {
        state_count += 1;
        // Invariant check: ensure sequence order is maintained
        if event.payload == "PayloadReady" && state_count < 2 {
            return Err(
                "SequenceInvariantViolated: PayloadReady received before Handshake acknowledged"
                    .into(),
            );
        }
        if event.payload == "HandshakeSyn" {
            eng.schedule(2, "SynAck".into(), 2);
        }
        Ok(())
    })
}

/// Generates an actionable, adversarial repair prompt for AI coding agents.
pub fn build_ai_repair_prompt(seed: u64, tick: u64, reason: &str, trace: &[String]) -> String {
    let mut prompt = String::new();
    prompt.push_str("# Canon Deterministic Simulation Testing (DST) Oracle: Bug Report\n\n");
    prompt.push_str(&format!("- **Reproducible Seed:** `{:#018x}`\n", seed));
    prompt.push_str(&format!("- **Failure Tick:** {}\n", tick));
    prompt.push_str(&format!("- **Violation:** `{}`\n\n", reason));
    prompt.push_str("## Execution Timeline Trace\n\n```text\n");

    let window_start = trace.len().saturating_sub(20);
    for line in &trace[window_start..] {
        prompt.push_str(line);
        prompt.push('\n');
    }
    prompt.push_str("```\n\n");
    prompt.push_str("## Instructions for AI Agent\n");
    prompt.push_str("1. Reproduce this exact defect locally with `cargo canon simulate --seed ");
    prompt.push_str(&format!("{:#018x}`.\n", seed));
    prompt.push_str("2. Analyze the timeline trace above for reordering, race condition, or dropped message handling.\n");
    prompt.push_str(
        "3. Surgically patch the coordination state machine to preserve safety under this fault.\n",
    );
    prompt
}

const fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simulation_passes() {
        let opts = SimulateOptions {
            seeds: 100,
            seed: None,
            max_ticks: 1000,
            fault_rate: 0.0,
            ai_prompt_on_fail: true,
        };
        let report = run_simulation(&opts).unwrap();
        assert_eq!(report.seeds_run, 100);
        assert_eq!(report.seeds_passed, 100);
        assert!(report.failing_seed.is_none());
    }

    #[test]
    fn test_ai_repair_prompt_generation() {
        let prompt = build_ai_repair_prompt(
            0xDEADBEEF,
            4112,
            "InvariantViolated: packet dropped",
            &["[tick 00010] Msg1".into(), "[tick 00020] Msg2".into()],
        );
        assert!(prompt.contains("0x00000000deadbeef"));
        assert!(prompt.contains("InvariantViolated: packet dropped"));
        assert!(prompt.contains("cargo canon simulate --seed"));
    }
}
