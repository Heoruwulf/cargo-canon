//! Context bundling and prompt formatting for AI coding agents.

use crate::error::SliceError;
use crate::indexer::{CallEdge, ContractInfo, CrossBoundaryImpact};
use canon_core::output::OutputFormat;
use serde::{Deserialize, Serialize};

/// An assembled bundle of compiler-accurate context for an agent prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SliceBundle {
    /// Target file or symbol inspected.
    pub target: String,
    /// Stripped interface type skeleton.
    pub skeleton: String,
    /// Trait implementations and contracts associated with target types.
    pub contracts: Vec<ContractInfo>,
    /// Upstream callers calling into this module/symbol.
    pub callers: Vec<CallEdge>,
    /// Downstream callees invoked by this module/symbol.
    pub callees: Vec<CallEdge>,
    /// Estimated context token reduction percentage compared to raw source.
    pub token_reduction_pct: f64,
    /// Cross-boundary contract impact between Rust backend and TypeScript frontend.
    #[serde(default)]
    pub cross_boundary: Option<CrossBoundaryImpact>,
}

impl SliceBundle {
    /// Formats the bundle according to the requested OutputFormat.
    pub fn format_with(&self, format: OutputFormat) -> Result<String, SliceError> {
        match format {
            OutputFormat::Markdown => Ok(self.format_markdown()),
            OutputFormat::Json => serde_json::to_string_pretty(self).map_err(SliceError::from),
            OutputFormat::Text => Ok(self.format_text()),
        }
    }

    /// Emits structured markdown optimized for LLM system prompt context injection.
    pub fn format_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("# Canon Context Slice: `{}`\n\n", self.target));
        out.push_str(&format!(
            "> **Token Reduction:** {:.1}% context saved vs raw source.\n\n",
            self.token_reduction_pct
        ));

        let lang = if self.target.ends_with(".ts")
            || self.target.ends_with(".tsx")
            || self.target.ends_with(".js")
            || self.target.ends_with(".jsx")
        {
            "typescript"
        } else {
            "rust"
        };
        out.push_str(&format!(
            "## Type Skeleton & Interface Contracts\n\n```{lang}\n"
        ));
        out.push_str(&self.skeleton);
        out.push_str("\n```\n\n");

        if !self.contracts.is_empty() {
            out.push_str("## Trait Contracts\n\n");
            for c in &self.contracts {
                out.push_str(&format!(
                    "- `impl {} for {}` (in `{}`)\n",
                    c.trait_name, c.target_type, c.file
                ));
                for m in &c.methods {
                    out.push_str(&format!("  - `{}`\n", m));
                }
            }
            out.push('\n');
        }

        if !self.callers.is_empty() {
            out.push_str("## Upstream Callers\n\n");
            for c in &self.callers {
                out.push_str(&format!(
                    "- `{}` calls `{}` (in `{}`)\n",
                    c.caller, c.callee, c.file
                ));
            }
            out.push('\n');
        }

        if !self.callees.is_empty() {
            out.push_str("## Downstream Dependencies\n\n");
            for c in &self.callees {
                out.push_str(&format!("- `{}` calls `{}`\n", c.caller, c.callee));
            }
            out.push('\n');
        }

        if let Some(ref cb) = self.cross_boundary {
            out.push_str("## Cross-Boundary Impact\n\n");
            if let Some(ref rust_def) = cb.rust_definition {
                out.push_str(&format!("- **Rust Backend Definition:** `{}`\n", rust_def));
            }
            if let Some(ref ts_def) = cb.ts_interface {
                out.push_str(&format!(
                    "- **TypeScript Consumer Interface:** `{}`\n",
                    ts_def
                ));
            }
            if !cb.api_call_sites.is_empty() {
                out.push_str("- **Frontend API Call Sites:**\n");
                for site in &cb.api_call_sites {
                    out.push_str(&format!("  - `{}`\n", site));
                }
            }
            out.push('\n');
        }

        out
    }

    /// Emits plain-text representation.
    pub fn format_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "=== Canon Context Slice: {} (Reduction: {:.1}%) ===\n\n",
            self.target, self.token_reduction_pct
        ));
        out.push_str("--- Skeleton ---\n");
        out.push_str(&self.skeleton);
        out.push_str("\n\n");

        if !self.contracts.is_empty() {
            out.push_str("--- Trait Contracts ---\n");
            for c in &self.contracts {
                out.push_str(&format!("impl {} for {}\n", c.trait_name, c.target_type));
            }
            out.push('\n');
        }

        if !self.callers.is_empty() {
            out.push_str("--- Callers ---\n");
            for c in &self.callers {
                out.push_str(&format!("{} -> {}\n", c.caller, c.callee));
            }
            out.push('\n');
        }

        if let Some(ref cb) = self.cross_boundary {
            out.push_str("--- Cross-Boundary Impact ---\n");
            if let Some(ref rust_def) = cb.rust_definition {
                out.push_str(&format!("Rust Backend Definition: {}\n", rust_def));
            }
            if let Some(ref ts_def) = cb.ts_interface {
                out.push_str(&format!("TypeScript Consumer Interface: {}\n", ts_def));
            }
            for site in &cb.api_call_sites {
                out.push_str(&format!("Frontend Call Site: {}\n", site));
            }
            out.push('\n');
        }

        out
    }
}
