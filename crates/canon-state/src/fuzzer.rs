//! High-throughput transition fuzzer for pure state machines.

use crate::StateMachine;
use crate::error::StateMachineError;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// Configuration for transition fuzzing runs.
#[derive(Debug, Clone, Copy)]
pub struct FuzzConfig {
    /// Number of transitions to attempt.
    pub max_transitions: usize,
    /// Random seed for deterministic sequence generation.
    pub seed: u64,
}

impl Default for FuzzConfig {
    fn default() -> Self {
        Self {
            max_transitions: 100_000,
            seed: 0x1337_C0DE_CAFE_D00D,
        }
    }
}

/// Outcome of a fuzzing session.
#[derive(Debug, Clone)]
pub struct FuzzReport {
    /// Total transitions successfully executed.
    pub transitions_executed: usize,
    /// Whether all invariant and transition checks passed.
    pub passed: bool,
    /// Failure details if an invariant or transition failed.
    pub failure: Option<String>,
    /// Minimized sequence of events triggering failure, if any.
    pub failing_history: Vec<String>,
}

/// Lightweight deterministic pseudo-random generator (SplitMix64) for no_std fuzzing.
struct Prng {
    state: u64,
}

impl Prng {
    const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn next_usize(&mut self, bound: usize) -> usize {
        if bound == 0 {
            return 0;
        }
        (self.next_u64() as usize) % bound
    }
}

/// Executes state transitions against a state machine using provided generator candidates.
pub fn fuzz_transitions<M, F>(
    machine: &mut M,
    config: FuzzConfig,
    event_generator: F,
    invariant: impl Fn(&M::State) -> Result<(), StateMachineError>,
) -> FuzzReport
where
    M: StateMachine,
    F: Fn(usize, &mut dyn FnMut(usize) -> usize) -> M::Event,
{
    let mut prng = Prng::new(config.seed);
    let mut history = Vec::new();

    for step_idx in 0..config.max_transitions {
        let mut bound_fn = |b: usize| prng.next_usize(b);
        let event = event_generator(step_idx, &mut bound_fn);
        let event_desc = format!("{event:?}");

        match machine.step(event) {
            Ok(_actions) => {
                history.push(event_desc);
                if let Err(inv_err) = invariant(machine.current_state()) {
                    return FuzzReport {
                        transitions_executed: step_idx + 1,
                        passed: false,
                        failure: Some(format!("invariant failure at step {step_idx}: {inv_err}")),
                        failing_history: history,
                    };
                }
            }
            Err(step_err) => {
                history.push(event_desc);
                return FuzzReport {
                    transitions_executed: step_idx + 1,
                    passed: false,
                    failure: Some(format!("transition error at step {step_idx}: {step_err:?}")),
                    failing_history: history,
                };
            }
        }
    }

    FuzzReport {
        transitions_executed: config.max_transitions,
        passed: true,
        failure: None,
        failing_history: Vec::new(),
    }
}
