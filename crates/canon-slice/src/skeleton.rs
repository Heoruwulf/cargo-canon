//! AST interface skeleton stripper using syn::visit_mut::VisitMut.

use crate::error::SliceError;
use quote::ToTokens;
use std::path::Path;
use syn::visit_mut::VisitMut;

/// AST visitor that clears implementation bodies while preserving interface contracts.
#[derive(Debug, Default)]
pub struct SkeletonStripper;

impl VisitMut for SkeletonStripper {
    fn visit_item_fn_mut(&mut self, item_fn: &mut syn::ItemFn) {
        item_fn.block.stmts.clear();
        syn::visit_mut::visit_item_fn_mut(self, item_fn);
    }

    fn visit_impl_item_fn_mut(&mut self, impl_fn: &mut syn::ImplItemFn) {
        impl_fn.block.stmts.clear();
        syn::visit_mut::visit_impl_item_fn_mut(self, impl_fn);
    }

    fn visit_trait_item_fn_mut(&mut self, trait_fn: &mut syn::TraitItemFn) {
        if let Some(default_block) = &mut trait_fn.default {
            default_block.stmts.clear();
        }
        syn::visit_mut::visit_trait_item_fn_mut(self, trait_fn);
    }
}

/// Strips implementation bodies in place from a parsed syn::File syntax tree.
pub fn strip_file_ast(file: &mut syn::File) {
    let mut stripper = SkeletonStripper;
    stripper.visit_file_mut(file);
}

/// Parses Rust source code, strips all function and method bodies, and formats the skeleton.
pub fn extract_skeleton(source_code: &str) -> Result<String, SliceError> {
    let mut ast: syn::File =
        syn::parse_str(source_code).map_err(|source| SliceError::ParseFailure {
            path: Path::new("<memory>").to_path_buf(),
            source,
        })?;

    strip_file_ast(&mut ast);

    // Format the token stream
    let tokens = ast.into_token_stream();
    let formatted = tokens.to_string();
    Ok(clean_token_stream_formatting(&formatted))
}

/// Reads a source file from disk, parses it, and generates its type skeleton.
pub fn extract_file_skeleton(path: &Path) -> Result<String, SliceError> {
    let content = std::fs::read_to_string(path).map_err(SliceError::Io)?;
    let mut ast: syn::File =
        syn::parse_str(&content).map_err(|source| SliceError::ParseFailure {
            path: path.to_path_buf(),
            source,
        })?;

    strip_file_ast(&mut ast);

    let tokens = ast.into_token_stream();
    let formatted = tokens.to_string();
    Ok(clean_token_stream_formatting(&formatted))
}

/// Performs lightweight formatting to re-indent and split lines for human and LLM readability.
fn clean_token_stream_formatting(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut indent_level = 0usize;

    for ch in input.chars() {
        match ch {
            '{' => {
                result.push('{');
                indent_level += 1;
                result.push('\n');
                result.push_str(&"    ".repeat(indent_level));
            }
            '}' => {
                indent_level = indent_level.saturating_sub(1);
                result.push('\n');
                result.push_str(&"    ".repeat(indent_level));
                result.push('}');
                result.push('\n');
                result.push_str(&"    ".repeat(indent_level));
            }
            ';' => {
                result.push(';');
                result.push('\n');
                result.push_str(&"    ".repeat(indent_level));
            }
            _ => {
                result.push(ch);
            }
        }
    }

    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skeleton_stripping() {
        let code = r#"
            pub struct Engine {
                pub id: u64,
                pub name: String,
            }

            impl Engine {
                pub fn new(id: u64) -> Self {
                    let name = format!("engine-{}", id);
                    Self { id, name }
                }

                pub fn start(&self) -> bool {
                    println!("Starting engine");
                    true
                }
            }
        "#;

        let skeleton = extract_skeleton(code).unwrap();
        assert!(skeleton.contains("pub struct Engine"));
        assert!(skeleton.contains("pub fn new (id : u64) -> Self"));
        assert!(skeleton.contains("pub fn start (& self) -> bool"));
        // Method bodies must be stripped
        assert!(!skeleton.contains("println !"));
        assert!(!skeleton.contains("format !"));
    }
}
