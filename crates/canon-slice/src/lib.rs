//! # canon-slice
//!
//! AST parsing and compiler-aware context slicing engine.
//!
//! Extracts minimal type skeletons, caller/callee hierarchies, and trait contracts
//! to deliver compiler-accurate context out-of-band to AI coding agents.

#![forbid(unsafe_code)]

pub mod bundle;
pub mod error;
pub mod indexer;
pub mod skeleton;

pub use bundle::SliceBundle;
pub use error::SliceError;
pub use indexer::{CallEdge, ContractInfo, SymbolDef, SymbolIndex};
pub use skeleton::{SkeletonStripper, extract_file_skeleton, extract_skeleton, strip_file_ast};

use canon_core::model::SliceMode;
use std::path::Path;

/// Slices a file or target symbol according to the requested SliceMode.
pub fn slice_target(
    target_path: &Path,
    mode: SliceMode,
    index: Option<&SymbolIndex>,
) -> Result<SliceBundle, SliceError> {
    let raw_source = std::fs::read_to_string(target_path).map_err(SliceError::Io)?;
    let raw_len = raw_source.len().max(1);

    let skeleton = extract_skeleton(&raw_source)?;
    let skeleton_len = skeleton.len();

    let reduction_pct = if raw_len > skeleton_len {
        ((raw_len - skeleton_len) as f64 / raw_len as f64) * 100.0
    } else {
        0.0
    };

    let target_name = target_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("target");

    let mut contracts = Vec::new();
    let mut callers = Vec::new();
    let mut callees = Vec::new();

    if let Some(idx) = index {
        if matches!(mode, SliceMode::Contracts | SliceMode::Bundle) {
            contracts = idx.get_contracts(target_name);
        }
        if matches!(mode, SliceMode::Hierarchy | SliceMode::Bundle) {
            callers = idx.get_callers(target_name, 2);
            callees = idx.get_callees(target_name, 2);
        }
    }

    Ok(SliceBundle {
        target: target_path.display().to_string(),
        skeleton,
        contracts,
        callers,
        callees,
        token_reduction_pct: reduction_pct,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_slice_target_bundle() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
            pub struct Worker {{
                pub count: usize,
            }}

            impl Worker {{
                pub fn run(&self) {{
                    for i in 0..1000 {{
                        println!("Heavy iteration {{}}", i);
                    }}
                }}
            }}
            "#
        )
        .unwrap();

        let bundle = slice_target(file.path(), SliceMode::Bundle, None).unwrap();
        assert!(bundle.skeleton.contains("pub struct Worker"));
        assert!(bundle.skeleton.contains("pub fn run (& self)"));
        assert!(!bundle.skeleton.contains("Heavy iteration"));
        assert!(bundle.token_reduction_pct > 30.0);
    }
}
