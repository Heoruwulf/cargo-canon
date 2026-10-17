//! Error primitives for pure Sans-I/O state machines.

use alloc::string::String;
use core::fmt;

/// Errors emitted during state machine transitions or fuzzing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateMachineError {
    /// Attempted an invalid transition from the current state.
    InvalidTransition {
        /// Current state description.
        from_state: String,
        /// Triggering event description.
        event: String,
        /// Reason for failure.
        reason: String,
    },
    /// Invariant violated following state transition.
    InvariantViolated {
        /// Invariant name or rule description.
        rule: String,
        /// State at the time of violation.
        state: String,
    },
    /// Generic state domain error.
    Domain(String),
}

impl fmt::Display for StateMachineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTransition {
                from_state,
                event,
                reason,
            } => {
                write!(
                    f,
                    "invalid transition from [{from_state}] on event [{event}]: {reason}"
                )
            }
            Self::InvariantViolated { rule, state } => {
                write!(f, "invariant '{rule}' violated while in state [{state}]")
            }
            Self::Domain(msg) => write!(f, "domain error: {msg}"),
        }
    }
}
