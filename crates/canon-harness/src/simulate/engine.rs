//! Discrete-event deterministic simulation testing (DST) engine.

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// Simulated network or actor event scheduled for deterministic execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimEvent {
    /// Scheduled virtual tick for event execution.
    pub deliver_at_tick: u64,
    /// Tie-breaker sequence identifier.
    pub sequence_id: u64,
    /// Destination actor ID.
    pub destination: u32,
    /// Event name or payload tag.
    pub payload: String,
}

impl Ord for SimEvent {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering so BinaryHeap acts as a min-heap by deliver_at_tick
        other
            .deliver_at_tick
            .cmp(&self.deliver_at_tick)
            .then_with(|| other.sequence_id.cmp(&self.sequence_id))
    }
}

impl PartialOrd for SimEvent {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Execution status of a simulation seed.
#[derive(Debug, Clone)]
pub struct SimOutcome {
    /// Total virtual ticks elapsed.
    pub ticks: u64,
    /// Total events dispatched.
    pub events_dispatched: u64,
    /// Total simulated network faults injected.
    pub faults_injected: u64,
    /// Complete ordered event trace.
    pub trace: Vec<String>,
    /// Failure description if the seed violated an invariant.
    pub failure: Option<String>,
}

/// Deterministic Simulation Testing (DST) engine.
pub struct DstEngine {
    seed: u64,
    rng: StdRng,
    current_tick: u64,
    max_ticks: u64,
    fault_rate: f64,
    next_sequence_id: u64,
    event_queue: BinaryHeap<SimEvent>,
    trace: Vec<String>,
    events_dispatched: u64,
    faults_injected: u64,
}

impl DstEngine {
    /// Initializes simulation engine with seed and runtime bounds.
    pub fn new(seed: u64, max_ticks: u64, fault_rate: f64) -> Self {
        Self {
            seed,
            rng: StdRng::seed_from_u64(seed),
            current_tick: 0,
            max_ticks,
            fault_rate,
            next_sequence_id: 0,
            event_queue: BinaryHeap::new(),
            trace: Vec::new(),
            events_dispatched: 0,
            faults_injected: 0,
        }
    }

    /// Schedules an event for delivery after `delay_ticks`.
    pub fn schedule(&mut self, destination: u32, payload: String, delay_ticks: u64) {
        let deliver_at_tick = self.current_tick + delay_ticks;
        let event = SimEvent {
            deliver_at_tick,
            sequence_id: self.next_sequence_id,
            destination,
            payload,
        };
        self.next_sequence_id += 1;
        self.event_queue.push(event);
    }

    /// Advances the simulation clock to process events until queue is empty or max_ticks reached.
    pub fn run_until_quiescent<H>(&mut self, mut handler: H) -> SimOutcome
    where
        H: FnMut(&mut Self, SimEvent) -> Result<(), String>,
    {
        while let Some(event) = self.event_queue.pop() {
            if event.deliver_at_tick > self.max_ticks {
                return self.finish_with_failure(format!(
                    "simulation exceeded max_ticks bound ({}) without quiescing (deadlock detected)",
                    self.max_ticks
                ));
            }

            self.current_tick = event.deliver_at_tick;

            // Check if network fault should be injected
            if self.fault_rate > 0.0 && self.rng.gen_bool(self.fault_rate.clamp(0.0, 1.0)) {
                self.faults_injected += 1;
                self.trace.push(format!(
                    "[tick {:05}] FAULT INJECTED: dropped message '{}' destined for actor {}",
                    self.current_tick, event.payload, event.destination
                ));
                continue;
            }

            self.events_dispatched += 1;
            self.trace.push(format!(
                "[tick {:05}] DELIVERED: '{}' to actor {}",
                self.current_tick, event.payload, event.destination
            ));

            if let Err(err) = handler(self, event) {
                return self.finish_with_failure(err);
            }
        }

        SimOutcome {
            ticks: self.current_tick,
            events_dispatched: self.events_dispatched,
            faults_injected: self.faults_injected,
            trace: self.trace.clone(),
            failure: None,
        }
    }

    fn finish_with_failure(&self, reason: String) -> SimOutcome {
        SimOutcome {
            ticks: self.current_tick,
            events_dispatched: self.events_dispatched,
            faults_injected: self.faults_injected,
            trace: self.trace.clone(),
            failure: Some(reason),
        }
    }

    /// Accessor for current simulated tick.
    pub fn current_tick(&self) -> u64 {
        self.current_tick
    }

    /// Generates a pseudo-random integer within [0, bound).
    pub fn gen_range(&mut self, bound: u64) -> u64 {
        if bound == 0 {
            0
        } else {
            self.rng.gen_range(0..bound)
        }
    }

    /// Seed used for this simulation engine.
    pub fn seed(&self) -> u64 {
        self.seed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dst_deterministic_replay() {
        let run = |seed: u64| {
            let mut engine = DstEngine::new(seed, 1000, 0.0);
            engine.schedule(1, "MsgA".into(), 10);
            engine.schedule(2, "MsgB".into(), 5);
            engine.run_until_quiescent(|_, _| Ok(()))
        };

        let outcome1 = run(0xCAFE);
        let outcome2 = run(0xCAFE);
        assert_eq!(outcome1.trace, outcome2.trace);
        assert_eq!(outcome1.events_dispatched, 2);
    }
}
