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
    SymbolIndexMetrics,
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
    let mut cross_boundary = None;

    if let Some(idx) = index {
        if matches!(mode, SliceMode::Contracts | SliceMode::Bundle) {
            contracts = idx.get_contracts(target_name);
        }
        if matches!(mode, SliceMode::Hierarchy | SliceMode::Bundle) {
            callers = idx.get_callers(target_name, 2);
            callees = idx.get_callees(target_name, 2);
        }
        let sym_stem = target_name
            .trim_end_matches(".rs")
            .trim_end_matches(".tsx")
            .trim_end_matches(".ts")
            .trim_end_matches(".jsx")
            .trim_end_matches(".js")
            .trim_end_matches(".go");
        cross_boundary = idx
            .get_cross_boundary_impact(sym_stem)
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
            });
    }

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
}
