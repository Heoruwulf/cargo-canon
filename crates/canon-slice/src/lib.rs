//! # canon-slice
//!
//! AST parsing and compiler-aware context slicing engine.
//!
//! Extracts minimal type skeletons, caller/callee hierarchies, and trait contracts
//! to deliver compiler-accurate context out-of-band to AI coding agents.

#![forbid(unsafe_code)]

pub mod bundle;
pub mod error;
pub mod go_skeleton;
pub mod indexer;
pub mod skeleton;
pub mod ts_skeleton;

pub use bundle::SliceBundle;
pub use error::SliceError;
pub use go_skeleton::{
    extract_go_file_skeleton, extract_go_skeleton, extract_go_skeleton_with_options,
};
pub use indexer::{
    CallEdge, ContractInfo, CrossBoundaryImpact, RouteBinding, SymbolDef, SymbolIndex,
    SymbolIndexMetrics, is_ignored_path,
};
pub use skeleton::{SkeletonStripper, extract_file_skeleton, extract_skeleton, strip_file_ast};
pub use ts_skeleton::{extract_ts_file_skeleton, extract_ts_skeleton};

use canon_core::model::SliceMode;
use std::path::Path;

/// Slices a file or target symbol according to the requested SliceMode.
pub fn slice_target(
    target_path: &Path,
    mode: SliceMode,
    index: Option<&SymbolIndex>,
) -> Result<SliceBundle, SliceError> {
    slice_target_with_overlay(target_path, mode, index, None)
}

/// Slices a file or target symbol according to the requested SliceMode,
/// querying an optional overlay index before falling through to the base index.
pub fn slice_target_with_overlay(
    target_path: &Path,
    mode: SliceMode,
    base_index: Option<&SymbolIndex>,
    overlay_index: Option<&SymbolIndex>,
) -> Result<SliceBundle, SliceError> {
    let raw_source = std::fs::read_to_string(target_path).map_err(SliceError::Io)?;
    let raw_len = raw_source.len().max(1);

    let ext = target_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let is_ts = matches!(ext, "ts" | "tsx" | "js" | "jsx");
    let is_go = ext == "go";
    let skeleton = if is_ts {
        let is_jsx = matches!(ext, "tsx" | "jsx");
        extract_ts_skeleton(&raw_source, is_jsx)?
    } else if is_go {
        extract_go_skeleton(&raw_source)?
    } else {
        extract_skeleton(&raw_source)?
    };
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

    if overlay_index.is_some() || base_index.is_some() {
        if matches!(mode, SliceMode::Contracts | SliceMode::Bundle) {
            let ov_contracts = overlay_index
                .map(|idx| idx.get_contracts(target_name))
                .unwrap_or_default();
            contracts = if !ov_contracts.is_empty() {
                ov_contracts
            } else {
                base_index
                    .map(|idx| idx.get_contracts(target_name))
                    .unwrap_or_default()
            };
        }
        if matches!(mode, SliceMode::Hierarchy | SliceMode::Bundle) {
            let ov_callers = overlay_index
                .map(|idx| idx.get_callers(target_name, 2))
                .unwrap_or_default();
            callers = if !ov_callers.is_empty() {
                ov_callers
            } else {
                base_index
                    .map(|idx| idx.get_callers(target_name, 2))
                    .unwrap_or_default()
            };

            let ov_callees = overlay_index
                .map(|idx| idx.get_callees(target_name, 2))
                .unwrap_or_default();
            callees = if !ov_callees.is_empty() {
                ov_callees
            } else {
                base_index
                    .map(|idx| idx.get_callees(target_name, 2))
                    .unwrap_or_default()
            };
        }
    }

    let cross_boundary = overlay_index
        .and_then(|ov| resolve_cross_boundary(ov, target_path, target_name))
        .or_else(|| {
            base_index.and_then(|base| resolve_cross_boundary(base, target_path, target_name))
        });

    Ok(SliceBundle {
        target: target_path.display().to_string(),
        skeleton,
        contracts,
        callers,
        callees,
        token_reduction_pct: reduction_pct,
        cross_boundary,
    })
}

fn resolve_cross_boundary(
    idx: &SymbolIndex,
    target_path: &Path,
    target_name: &str,
) -> Option<CrossBoundaryImpact> {
    let sym_stem = target_name
        .trim_end_matches(".rs")
        .trim_end_matches(".tsx")
        .trim_end_matches(".ts")
        .trim_end_matches(".jsx")
        .trim_end_matches(".js")
        .trim_end_matches(".go");
    idx.get_cross_boundary_impact(sym_stem)
        .or_else(|| idx.get_cross_boundary_impact(target_name))
        .or_else(|| {
            let file_str = target_path.to_string_lossy();
            for (sym_name, defs) in &idx.symbols_map {
                if defs
                    .iter()
                    .any(|d| d.file == file_str || target_path.ends_with(&d.file))
                {
                    if let Some(cb) = idx.get_cross_boundary_impact(sym_name) {
                        return Some(cb);
                    }
                }
            }
            None
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

    #[test]
    fn test_slice_target_bundle_tsx() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("Component.tsx");
        std::fs::write(
            &file_path,
            r#"
            export interface Props { title: string; }
            export function Component(props: Props) {
                return <div>{props.title}</div>;
            }
            "#,
        )
        .unwrap();

        let bundle = slice_target(&file_path, SliceMode::Bundle, None).unwrap();
        assert!(bundle.skeleton.contains("export interface Props"));
        assert!(
            bundle
                .skeleton
                .contains("export function Component(props: Props): JSX.Element;")
        );
        let md = bundle.format_markdown();
        assert!(md.contains("```typescript"));
        assert!(md.contains("Token Reduction:"));
    }

    #[test]
    fn test_slice_target_bundle_with_cross_boundary() {
        let dir = tempfile::tempdir().unwrap();
        let rs_path = dir.path().join("users.rs");
        let ts_path = dir.path().join("user.ts");

        std::fs::write(&rs_path, "pub struct UserPayload { pub id: u64, }").unwrap();
        std::fs::write(&ts_path, "export interface UserPayload { id: number; }").unwrap();

        let mut index = SymbolIndex::new();
        index
            .index_any_source_str(
                &rs_path.to_string_lossy(),
                "pub struct UserPayload { pub id: u64, }",
            )
            .unwrap();
        index
            .index_any_source_str(
                &ts_path.to_string_lossy(),
                "export interface UserPayload { id: number; }",
            )
            .unwrap();

        let bundle = slice_target(&rs_path, SliceMode::Bundle, Some(&index)).unwrap();
        assert!(bundle.cross_boundary.is_some());
        let md = bundle.format_markdown();
        assert!(md.contains("## Cross-Boundary Impact"));
        assert!(md.contains("Rust Backend Definition:"));
        assert!(md.contains("TypeScript Consumer Interface:"));
    }

    #[test]
    fn test_slice_target_bundle_go() {
        let dir = tempfile::tempdir().unwrap();
        let go_path = dir.path().join("user.go");
        let ts_path = dir.path().join("user.ts");

        std::fs::write(
            &go_path,
            r#"
            package model
            type UserPayload struct {
                ID uint64 `json:"user_id"`
            }
            func (u *UserPayload) Validate() bool {
                return u.ID > 0
            }
            "#,
        )
        .unwrap();

        std::fs::write(&ts_path, "export interface UserPayload { id: number; }").unwrap();

        let mut index = SymbolIndex::new();
        index.index_directory(dir.path()).unwrap();

        let bundle = slice_target(&go_path, SliceMode::Bundle, Some(&index)).unwrap();
        assert!(bundle.skeleton.contains("type UserPayload struct"));
        assert!(
            bundle
                .skeleton
                .contains("func (u *UserPayload) Validate() bool;")
        );
        assert!(!bundle.skeleton.contains("return u.ID > 0"));

        let md = bundle.format_markdown();
        assert!(md.contains("```go"));
        assert!(md.contains("Token Reduction:"));
        assert!(md.contains("## Cross-Boundary Impact"));
        assert!(md.contains("Go Backend Model:"));
    }

    #[test]
    fn test_slice_target_with_overlay() {
        let dir = tempfile::tempdir().unwrap();
        let rs_path = dir.path().join("service.rs");

        // Write initial file and index into base
        std::fs::write(
            &rs_path,
            r#"
            pub struct Service;
            pub trait Handler { fn execute(&self); }
            impl Handler for Service { fn execute(&self) {} }
            pub fn run() { let s = Service; s.execute(); base_call(); }
            pub fn base_call() {}
            "#,
        )
        .unwrap();

        let mut base_index = SymbolIndex::new();
        base_index.index_directory(dir.path()).unwrap();

        // Modify file with new implementation and new callee
        std::fs::write(
            &rs_path,
            r#"
            pub struct Service;
            pub trait Handler { fn execute(&self); }
            impl Handler for Service { fn execute(&self) {} }
            pub fn run() { let s = Service; s.execute(); overlay_call(); }
            pub fn overlay_call() {}
            "#,
        )
        .unwrap();

        let overlay_index = base_index.create_overlay(std::slice::from_ref(&rs_path));

        // Slice with overlay
        let bundle = slice_target_with_overlay(
            &rs_path,
            SliceMode::Bundle,
            Some(&base_index),
            Some(&overlay_index),
        )
        .unwrap();

        assert!(bundle.skeleton.contains("pub struct Service"));
        assert!(bundle.skeleton.contains("pub fn overlay_call ()"));

        // Overlay contracts fallback
        let contracts = overlay_index.get_contracts_with_fallback(Some(&base_index), "Service");
        assert!(!contracts.is_empty());
        assert_eq!(contracts[0].trait_name, "Handler");

        // Overridden callers in overlay take precedence
        assert!(!overlay_index.get_callers("overlay_call", 1).is_empty());
        assert!(base_index.get_callers("overlay_call", 1).is_empty());
        assert!(
            !overlay_index
                .get_callers_with_fallback(Some(&base_index), "base_call", 1)
                .is_empty()
        );
    }
}
