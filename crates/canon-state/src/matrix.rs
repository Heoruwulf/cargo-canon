//! Reachability and transition matrix analysis for pure state machines.

use alloc::collections::BTreeSet;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// A recorded state transition edge.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TransitionEdge {
    /// Originating state name.
    pub from: String,
    /// Event name that triggered the step.
    pub event: String,
    /// Destination state name.
    pub to: String,
}

/// Reachability and health report for a state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatrixReport {
    /// Total unique recorded transitions.
    pub total_transitions: usize,
    /// Reachable states from the initial state.
    pub reachable_states: Vec<String>,
    /// Dead or unreachable states from declared set.
    pub unreachable_states: Vec<String>,
    /// Terminal states that have no outgoing transitions.
    pub terminal_states: Vec<String>,
}

/// Transition matrix collector and analyzer.
#[derive(Debug, Default, Clone)]
pub struct TransitionMatrix {
    edges: BTreeSet<TransitionEdge>,
    known_states: BTreeSet<String>,
}

impl TransitionMatrix {
    /// Creates an empty transition matrix.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a known declared state.
    pub fn register_state(&mut self, state: &str) {
        self.known_states.insert(String::from(state));
    }

    /// Adds a transition edge between states.
    pub fn record_transition(&mut self, from: &str, event: &str, to: &str) {
        self.known_states.insert(String::from(from));
        self.known_states.insert(String::from(to));
        self.edges.insert(TransitionEdge {
            from: String::from(from),
            event: String::from(event),
            to: String::from(to),
        });
    }

    /// Computes reachability and identifies dead or terminal states.
    pub fn analyze(&self, initial_state: &str) -> MatrixReport {
        let mut visited = BTreeSet::new();
        let mut queue = Vec::new();

        if self.known_states.contains(initial_state) {
            visited.insert(String::from(initial_state));
            queue.push(String::from(initial_state));
        }

        while let Some(current) = queue.pop() {
            for edge in &self.edges {
                if edge.from == current && !visited.contains(&edge.to) {
                    visited.insert(edge.to.clone());
                    queue.push(edge.to.clone());
                }
            }
        }

        let reachable: Vec<String> = visited.into_iter().collect();
        let unreachable: Vec<String> = self
            .known_states
            .iter()
            .filter(|s| !reachable.contains(s))
            .cloned()
            .collect();

        let terminal: Vec<String> = self
            .known_states
            .iter()
            .filter(|s| !self.edges.iter().any(|e| &e.from == *s))
            .cloned()
            .collect();

        MatrixReport {
            total_transitions: self.edges.len(),
            reachable_states: reachable,
            unreachable_states: unreachable,
            terminal_states: terminal,
        }
    }

    /// Formats the matrix as a Markdown table.
    pub fn format_markdown(&self, initial_state: &str) -> String {
        let report = self.analyze(initial_state);
        let mut out = String::new();
        out.push_str("# State Machine Transition Matrix\n\n");
        out.push_str("| From State | Event | To State |\n");
        out.push_str("|------------|-------|----------|\n");

        for edge in &self.edges {
            out.push_str(&format!(
                "| `{}` | `{}` | `{}` |\n",
                edge.from, edge.event, edge.to
            ));
        }

        out.push_str("\n## Reachability Analysis\n\n");
        out.push_str(&format!("- **Initial State:** `{}`\n", initial_state));
        out.push_str(&format!(
            "- **Total Transitions:** {}\n",
            report.total_transitions
        ));
        out.push_str(&format!(
            "- **Reachable States ({}):** {}\n",
            report.reachable_states.len(),
            report.reachable_states.join(", ")
        ));

        if report.unreachable_states.is_empty() {
            out.push_str("- **Unreachable (Dead) States:** None (all states reachable)\n");
        } else {
            out.push_str(&format!(
                "- **Unreachable (Dead) States ({}):** {}\n",
                report.unreachable_states.len(),
                report.unreachable_states.join(", ")
            ));
        }

        if report.terminal_states.is_empty() {
            out.push_str("- **Terminal States:** None\n");
        } else {
            out.push_str(&format!(
                "- **Terminal States ({}):** {}\n",
                report.terminal_states.len(),
                report.terminal_states.join(", ")
            ));
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matrix_reachability() {
        let mut matrix = TransitionMatrix::new();
        matrix.register_state("Init");
        matrix.register_state("Ready");
        matrix.register_state("Processing");
        matrix.register_state("DeadState");

        matrix.record_transition("Init", "Start", "Ready");
        matrix.record_transition("Ready", "Process", "Processing");
        matrix.record_transition("Processing", "Finish", "Ready");

        let report = matrix.analyze("Init");
        assert_eq!(report.total_transitions, 3);
        assert!(report.reachable_states.contains(&"Ready".into()));
        assert!(report.unreachable_states.contains(&"DeadState".into()));
    }
}
