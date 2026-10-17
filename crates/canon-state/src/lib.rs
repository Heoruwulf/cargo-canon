//! # canon-state
//!
//! Sans-I/O pure deterministic state machine framework.
//!
//! Confines domain logic strictly to pure synchronous state transformations
//! isolated from external side effects, enabling exhaustive simulation
//! and formal verification.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod error;
pub mod fuzzer;
pub mod matrix;

pub use error::StateMachineError;
pub use fuzzer::{FuzzConfig, FuzzReport, fuzz_transitions};
pub use matrix::{MatrixReport, TransitionEdge, TransitionMatrix};

use alloc::vec::Vec;

/// Pure Sans-I/O state machine contract.
pub trait StateMachine {
    /// Domain state type.
    type State: Clone + Eq + core::fmt::Debug;
    /// Event type consumed by state transitions.
    type Event: Clone + core::fmt::Debug;
    /// Action or side-effect instruction produced by state transitions.
    type Action: Clone + core::fmt::Debug;
    /// Error type emitted when an invalid transition occurs.
    type Error: core::fmt::Debug;

    /// Consumes an event and advances the internal state, returning triggered actions.
    fn step(&mut self, event: Self::Event) -> Result<Vec<Self::Action>, Self::Error>;

    /// Returns a reference to the active state.
    fn current_state(&self) -> &Self::State;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ConnectionState {
        Disconnected,
        Connecting,
        Connected,
        Disconnecting,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ConnectionEvent {
        Connect,
        Ack,
        Disconnect,
        Closed,
        Reset,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ConnectionAction {
        SendSyn,
        NotifyConnected,
        SendFin,
        Cleanup,
    }

    pub struct ConnectionMachine {
        state: ConnectionState,
        reconnect_attempts: u32,
    }

    impl ConnectionMachine {
        pub const fn new() -> Self {
            Self {
                state: ConnectionState::Disconnected,
                reconnect_attempts: 0,
            }
        }
    }

    impl StateMachine for ConnectionMachine {
        type State = ConnectionState;
        type Event = ConnectionEvent;
        type Action = ConnectionAction;
        type Error = StateMachineError;

        fn step(&mut self, event: Self::Event) -> Result<Vec<Self::Action>, Self::Error> {
            match (self.state, event) {
                (ConnectionState::Disconnected, ConnectionEvent::Connect) => {
                    self.state = ConnectionState::Connecting;
                    self.reconnect_attempts += 1;
                    Ok(alloc::vec![ConnectionAction::SendSyn])
                }
                (ConnectionState::Connecting, ConnectionEvent::Ack) => {
                    self.state = ConnectionState::Connected;
                    self.reconnect_attempts = 0;
                    Ok(alloc::vec![ConnectionAction::NotifyConnected])
                }
                (ConnectionState::Connected, ConnectionEvent::Disconnect) => {
                    self.state = ConnectionState::Disconnecting;
                    Ok(alloc::vec![ConnectionAction::SendFin])
                }
                (ConnectionState::Disconnecting, ConnectionEvent::Closed) => {
                    self.state = ConnectionState::Disconnected;
                    Ok(alloc::vec![ConnectionAction::Cleanup])
                }
                (_, ConnectionEvent::Reset) => {
                    self.state = ConnectionState::Disconnected;
                    self.reconnect_attempts = 0;
                    Ok(alloc::vec![ConnectionAction::Cleanup])
                }
                _ => Err(StateMachineError::InvalidTransition {
                    from_state: alloc::format!("{:?}", self.state),
                    event: alloc::format!("{event:?}"),
                    reason: alloc::string::String::from("unsupported state event pairing"),
                }),
            }
        }

        fn current_state(&self) -> &Self::State {
            &self.state
        }
    }

    #[test]
    fn test_connection_state_machine_flow() {
        let mut machine = ConnectionMachine::new();
        assert_eq!(*machine.current_state(), ConnectionState::Disconnected);

        let actions = machine.step(ConnectionEvent::Connect).unwrap();
        assert_eq!(actions, alloc::vec![ConnectionAction::SendSyn]);
        assert_eq!(*machine.current_state(), ConnectionState::Connecting);

        let actions = machine.step(ConnectionEvent::Ack).unwrap();
        assert_eq!(actions, alloc::vec![ConnectionAction::NotifyConnected]);
        assert_eq!(*machine.current_state(), ConnectionState::Connected);
    }

    #[test]
    fn test_high_throughput_fuzzing() {
        let mut machine = ConnectionMachine::new();
        let events = [
            ConnectionEvent::Connect,
            ConnectionEvent::Ack,
            ConnectionEvent::Disconnect,
            ConnectionEvent::Closed,
            ConnectionEvent::Reset,
        ];

        let config = FuzzConfig {
            max_transitions: 200_000,
            seed: 0x900D_BEEF,
        };

        let report = fuzz_transitions(
            &mut machine,
            config,
            |_step, bound_fn| {
                // Return an event that is either a reset or tailored to step correctly
                let idx = bound_fn(events.len());
                events[idx]
            },
            |_state| Ok(()),
        );

        // Even with random invalid events, machine either handles or halts with failure report
        assert!(report.transitions_executed > 0);
    }
}
