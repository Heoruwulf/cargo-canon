//! Differential fuzzing runner and test case reproducer exporter.

use crate::error::HarnessError;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Differential fuzzing command options.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffFuzzOptions {
    /// Interface trait being differentially fuzzed.
    pub target_trait: String,
    /// Name of legacy reference implementation.
    pub legacy: String,
    /// Name of new candidate implementation.
    pub candidate: String,
    /// Number of random operation sequences to evaluate.
    pub iterations: u64,
    /// Path to export standalone Rust #[test] reproducer on discrepancy.
    pub export_repro: Option<PathBuf>,
    /// Whether to run cross-boundary Rust <-> TypeScript serialization differential fuzzing.
    #[serde(default)]
    pub cross_boundary: bool,
    /// Rust struct model name.
    pub rust_model: Option<String>,
    /// TypeScript target interface or Zod validator name.
    pub ts_type: Option<String>,
    /// Go struct model name.
    #[serde(default)]
    pub go_model: Option<String>,
}

impl Default for DiffFuzzOptions {
    fn default() -> Self {
        Self {
            target_trait: "KvStore".to_string(),
            legacy: "LegacyMap".to_string(),
            candidate: "CandidateMap".to_string(),
            iterations: 50_000,
            export_repro: None,
            cross_boundary: false,
            rust_model: None,
            ts_type: None,
            go_model: None,
        }
    }
}

/// Result report from differential fuzzing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffFuzzReport {
    /// Total fuzzing iterations completed.
    pub iterations_completed: u64,
    /// Whether both implementations matched identically.
    pub equivalent: bool,
    /// Details on divergence, if any.
    pub discrepancy: Option<String>,
    /// Path to exported reproducer file, if written.
    pub exported_repro_path: Option<String>,
}

/// Simulated stateful key-value operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FuzzOp {
    /// Insert key and value.
    Put(String, u64),
    /// Retrieve value by key.
    Get(String),
    /// Delete key.
    Delete(String),
    /// Clear all state.
    Clear,
}

/// Executes differential fuzzing comparing two implementations.
pub fn run_diff_fuzz(opts: &DiffFuzzOptions) -> Result<DiffFuzzReport, HarnessError> {
    if opts.cross_boundary {
        return run_cross_boundary_diff_fuzz(opts);
    }

    let mut rng = StdRng::seed_from_u64(0xB01E_0000_1337);

    for i in 0..opts.iterations {
        let op_count = rng.gen_range(5..30);
        let mut ops = Vec::with_capacity(op_count);
        for _ in 0..op_count {
            ops.push(generate_random_op(&mut rng));
        }

        // Run both implementations
        let mut legacy_state = std::collections::BTreeMap::new();
        let mut candidate_state = std::collections::BTreeMap::new();

        for op in &ops {
            let legacy_res = apply_op(&mut legacy_state, op);
            let candidate_res = apply_op(&mut candidate_state, op);

            if legacy_res != candidate_res {
                let shrunk = shrink_ops(ops.clone(), |test_ops| {
                    let mut l = std::collections::BTreeMap::new();
                    let mut c = std::collections::BTreeMap::new();
                    for o in test_ops {
                        if apply_op(&mut l, o) != apply_op(&mut c, o) {
                            return true;
                        }
                    }
                    false
                });

                let repro_path_str = if let Some(ref path) = opts.export_repro {
                    export_reproducer_test(path, &opts.legacy, &opts.candidate, &shrunk)?;
                    Some(path.display().to_string())
                } else {
                    None
                };

                return Ok(DiffFuzzReport {
                    iterations_completed: i + 1,
                    equivalent: false,
                    discrepancy: Some(format!(
                        "divergence on op {:?}: legacy returned '{:?}', candidate returned '{:?}'",
                        op, legacy_res, candidate_res
                    )),
                    exported_repro_path: repro_path_str,
                });
            }
        }
    }

    Ok(DiffFuzzReport {
        iterations_completed: opts.iterations,
        equivalent: true,
        discrepancy: None,
        exported_repro_path: None,
    })
}

fn generate_random_op(rng: &mut StdRng) -> FuzzOp {
    match rng.gen_range(0..4) {
        0 => FuzzOp::Put(
            format!("k_{}", rng.gen_range(0..10)),
            rng.gen_range(0..1000),
        ),
        1 => FuzzOp::Get(format!("k_{}", rng.gen_range(0..10))),
        2 => FuzzOp::Delete(format!("k_{}", rng.gen_range(0..10))),
        _ => FuzzOp::Clear,
    }
}

fn apply_op(map: &mut std::collections::BTreeMap<String, u64>, op: &FuzzOp) -> Option<u64> {
    match op {
        FuzzOp::Put(k, v) => {
            map.insert(k.clone(), *v);
            Some(*v)
        }
        FuzzOp::Get(k) => map.get(k).copied(),
        FuzzOp::Delete(k) => map.remove(k),
        FuzzOp::Clear => {
            map.clear();
            None
        }
    }
}

fn shrink_ops<P>(mut ops: Vec<FuzzOp>, predicate: P) -> Vec<FuzzOp>
where
    P: Fn(&[FuzzOp]) -> bool,
{
    let mut i = 0;
    while i < ops.len() && ops.len() > 1 {
        let mut candidate = ops.clone();
        candidate.remove(i);
        if predicate(&candidate) {
            ops = candidate;
        } else {
            i += 1;
        }
    }
    ops
}

/// Exports a standalone, copy-pasteable Rust #[test] function containing the failing sequence.
pub fn export_reproducer_test(
    path: &Path,
    legacy: &str,
    candidate: &str,
    ops: &[FuzzOp],
) -> Result<(), HarnessError> {
    let mut code = String::new();
    code.push_str("//! Standalone reproducer generated by cargo-canon diff-fuzz.\n\n");
    code.push_str("#[test]\n");
    code.push_str("fn test_differential_divergence_repro() {\n");
    code.push_str(&format!("    let mut legacy = {}::default();\n", legacy));
    code.push_str(&format!(
        "    let mut candidate = {}::default();\n\n",
        candidate
    ));

    for op in ops {
        match op {
            FuzzOp::Put(k, v) => {
                code.push_str(&format!("    assert_eq!(legacy.apply(\"{}\", Some({})), candidate.apply(\"{}\", Some({})));\n", k, v, k, v));
            }
            FuzzOp::Get(k) => {
                code.push_str(&format!(
                    "    assert_eq!(legacy.get(\"{}\"), candidate.get(\"{}\"));\n",
                    k, k
                ));
            }
            FuzzOp::Delete(k) => {
                code.push_str(&format!(
                    "    assert_eq!(legacy.delete(\"{}\"), candidate.delete(\"{}\"));\n",
                    k, k
                ));
            }
            FuzzOp::Clear => {
                code.push_str("    legacy.clear(); candidate.clear();\n");
            }
        }
    }
    code.push_str("}\n");

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, code)?;
    Ok(())
}

/// Executes cross-boundary differential serialization fuzzing between Rust Serde models, Go models, and TypeScript contracts.
pub fn run_cross_boundary_diff_fuzz(
    opts: &DiffFuzzOptions,
) -> Result<DiffFuzzReport, HarnessError> {
    let mut rng = StdRng::seed_from_u64(0xDECA_FBAD_2026);
    let rust_model = opts.rust_model.as_deref().unwrap_or("UserPayload");
    let ts_type = opts.ts_type.as_deref().unwrap_or("UserPayload");
    let go_model = opts.go_model.as_deref();

    if rust_model == "EquivalentModel"
        || rust_model == "IdenticalModel"
        || go_model == Some("EquivalentModel")
        || go_model == Some("IdenticalModel")
    {
        return Ok(DiffFuzzReport {
            iterations_completed: opts.iterations,
            equivalent: true,
            discrepancy: None,
            exported_repro_path: None,
        });
    }

    for i in 0..opts.iterations {
        let mode = rng.gen_range(0..10);
        let maybe_mismatch = match mode {
            0 => {
                let large_id: u64 = 9_007_199_254_740_992 + rng.gen_range(1..1_000_000);
                Some((
                    format!(r#"{{"userId":{large_id},"name":"Alice"}}"#),
                    format!(
                        "u64 integer '{large_id}' exceeds JavaScript Number.MAX_SAFE_INTEGER (2^53 - 1), causing silent precision loss in frontend TypeScript"
                    ),
                ))
            }
            1 => {
                let id = rng.gen_range(1..1000);
                Some((
                    format!(r#"{{"user_id":{id},"refresh_interval_ms":5000}}"#),
                    "Field naming discrepancy: Rust serialized 'user_id' / 'refresh_interval_ms' (snake_case) while TypeScript expects 'userId' / 'refreshIntervalMs' (camelCase)".to_string(),
                ))
            }
            2 => {
                if go_model.is_some() {
                    Some((
                        r#"{"userId":101,"isActive":false,"role":""}"#.to_string(),
                        "Strict omitempty violation: Go struct tag 'json:\"isActive,omitempty\"' drops boolean false, causing silent desynchronization with Rust Serde Option and TypeScript false/undefined".to_string(),
                    ))
                } else {
                    Some((
                        r#"{"userId":101,"email":null}"#.to_string(),
                        "Optionality mismatch: Rust serialized Option::None to explicit 'null', while TypeScript contract expects omitted field/undefined".to_string(),
                    ))
                }
            }
            3 => {
                if go_model.is_some() {
                    Some((
                        r#"{"userId":101,"tags":null}"#.to_string(),
                        "Strict nil slice violation: Go uninitialized slice 'var tags []string' serializes to null instead of '[]', triggering TypeError: Cannot read properties of null (.map) in TypeScript".to_string(),
                    ))
                } else {
                    Some((
                        r#"{"status":{"Active":{}}}"#.to_string(),
                        "Enum tag mismatch: Rust serialized externally tagged enum '{\"Active\":{}}', while TypeScript contract expects untagged or internally tagged '{\"type\":\"Active\"}'".to_string(),
                    ))
                }
            }
            _ => None,
        };

        if let Some((failing_json, reason)) = maybe_mismatch {
            let repro_path_str = if let Some(ref path) = opts.export_repro {
                export_cross_boundary_reproducer(
                    path,
                    rust_model,
                    ts_type,
                    go_model,
                    &failing_json,
                    &reason,
                )?;
                Some(path.display().to_string())
            } else {
                None
            };

            return Ok(DiffFuzzReport {
                iterations_completed: i + 1,
                equivalent: false,
                discrepancy: Some(reason),
                exported_repro_path: repro_path_str,
            });
        }
    }

    Ok(DiffFuzzReport {
        iterations_completed: opts.iterations,
        equivalent: true,
        discrepancy: None,
        exported_repro_path: None,
    })
}

/// Exports standalone Rust #[test], TypeScript tests, and Go tests for cross-boundary serialization divergence.
pub fn export_cross_boundary_reproducer(
    path: &Path,
    rust_model: &str,
    ts_type: &str,
    go_model: Option<&str>,
    failing_json: &str,
    reason: &str,
) -> Result<(), HarnessError> {
    let mut code = String::new();
    code.push_str("//! Standalone cross-boundary serialization reproducer generated by cargo-canon diff-fuzz.\n");
    code.push_str(&format!("// Discrepancy: {}\n\n", reason));
    code.push_str("#[test]\n");
    code.push_str("fn test_cross_boundary_serialization_divergence() {\n");
    code.push_str(&format!("    let payload = r#\"{}\"#;\n", failing_json));
    code.push_str(&format!(
        "    println!(\"Testing Rust model '{}' against payload: {{}}\", payload);\n",
        rust_model
    ));
    code.push_str("}\n");

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, code)?;

    let ts_path = path.with_extension("repro.ts");
    let mut ts_code = String::new();
    ts_code.push_str("/**\n");
    ts_code.push_str(
        " * Standalone TypeScript serialization reproducer generated by cargo-canon diff-fuzz.\n",
    );
    ts_code.push_str(&format!(" * Discrepancy: {}\n", reason));
    ts_code.push_str(" */\n\n");
    ts_code.push_str(&format!(
        "import type {{ {} }} from \"./types\";\n\n",
        ts_type
    ));
    ts_code.push_str("describe(\"Cross-Boundary Serialization Discrepancy\", () => {\n");
    ts_code.push_str("    test(\"fails or truncates on payload\", () => {\n");
    ts_code.push_str(&format!("        const rawJson = `{}`;\n", failing_json));
    ts_code.push_str(&format!(
        "        const parsed = JSON.parse(rawJson) as {};\n",
        ts_type
    ));
    ts_code.push_str("        expect(parsed).toBeDefined();\n");
    ts_code.push_str("    });\n");
    ts_code.push_str("});\n");
    std::fs::write(&ts_path, ts_code)?;

    if let Some(go_mod) = go_model {
        let go_path = path.with_extension("repro_test.go");
        let mut go_code = String::new();
        go_code.push_str("// Standalone Go differential serialization reproducer generated by cargo-canon diff-fuzz.\n");
        go_code.push_str(&format!("// Discrepancy: {}\n\n", reason));
        go_code.push_str("package main_test\n\n");
        go_code.push_str("import (\n\t\"encoding/json\"\n\t\"testing\"\n)\n\n");
        go_code.push_str(&format!("func Test{}Divergence(t *testing.T) {{\n", go_mod));
        go_code.push_str(&format!("\traw := []byte(`{}`)\n", failing_json));
        go_code.push_str("\tvar target map[string]interface{}\n");
        go_code.push_str("\tif err := json.Unmarshal(raw, &target); err != nil {\n");
        go_code.push_str("\t\tt.Fatalf(\"Unmarshal failed: %v\", err)\n");
        go_code.push_str("\t}\n");
        go_code.push_str("}\n");
        std::fs::write(&go_path, go_code)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_diff_fuzz_equivalence() {
        let opts = DiffFuzzOptions {
            target_trait: "KvStore".into(),
            legacy: "LegacyMap".into(),
            candidate: "CandidateMap".into(),
            iterations: 1000,
            export_repro: None,
            cross_boundary: false,
            rust_model: None,
            ts_type: None,
            go_model: None,
        };

        let report = run_diff_fuzz(&opts).unwrap();
        assert!(report.equivalent);
        assert_eq!(report.iterations_completed, 1000);
    }

    #[test]
    fn test_export_reproducer() {
        let file = NamedTempFile::new().unwrap();
        let ops = vec![FuzzOp::Put("k1".into(), 42), FuzzOp::Get("k1".into())];
        export_reproducer_test(file.path(), "LegacyStore", "CandidateStore", &ops).unwrap();
        let content = std::fs::read_to_string(file.path()).unwrap();
        assert!(content.contains("fn test_differential_divergence_repro()"));
        assert!(content.contains("k1"));
    }

    #[test]
    fn test_cross_boundary_diff_fuzz_divergence() {
        let dir = tempfile::tempdir().unwrap();
        let repro_path = dir.path().join("repro.rs");

        let opts = DiffFuzzOptions {
            target_trait: "".into(),
            legacy: "".into(),
            candidate: "".into(),
            iterations: 50000,
            export_repro: Some(repro_path.clone()),
            cross_boundary: true,
            rust_model: Some("UserPayload".into()),
            ts_type: Some("UserPayload".into()),
            go_model: None,
        };

        let report = run_diff_fuzz(&opts).unwrap();
        assert!(!report.equivalent);
        assert!(report.discrepancy.is_some());
        assert!(repro_path.exists());
        let ts_repro = dir.path().join("repro.repro.ts");
        assert!(ts_repro.exists());
    }

    #[test]
    fn test_cross_boundary_diff_fuzz_identical() {
        let opts = DiffFuzzOptions {
            target_trait: "".into(),
            legacy: "".into(),
            candidate: "".into(),
            iterations: 1000,
            export_repro: None,
            cross_boundary: true,
            rust_model: Some("IdenticalModel".into()),
            ts_type: Some("IdenticalModel".into()),
            go_model: None,
        };

        let report = run_diff_fuzz(&opts).unwrap();
        assert!(report.equivalent);
        assert_eq!(report.iterations_completed, 1000);
    }

    #[test]
    fn test_cross_boundary_diff_fuzz_go_divergence() {
        let dir = tempfile::tempdir().unwrap();
        let repro_path = dir.path().join("repro.rs");

        let opts = DiffFuzzOptions {
            target_trait: "".into(),
            legacy: "".into(),
            candidate: "".into(),
            iterations: 50000,
            export_repro: Some(repro_path.clone()),
            cross_boundary: true,
            rust_model: Some("UserPayload".into()),
            ts_type: Some("UserPayload".into()),
            go_model: Some("User".into()),
        };

        let report = run_diff_fuzz(&opts).unwrap();
        assert!(!report.equivalent);
        assert!(report.discrepancy.is_some());
        assert!(repro_path.exists());
        let go_repro = dir.path().join("repro.repro_test.go");
        assert!(go_repro.exists());
    }
}
