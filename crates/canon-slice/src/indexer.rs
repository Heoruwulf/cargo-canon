//! Semantic call-graph and contract indexer for Rust codebases.

use crate::error::SliceError;
use quote::ToTokens;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use syn::visit::Visit;
use walkdir::WalkDir;

/// Represents a caller-to-callee relation in the indexed call graph.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CallEdge {
    /// Upstream caller symbol or function.
    pub caller: String,
    /// Downstream callee symbol or function.
    pub callee: String,
    /// Source file path where invocation occurs.
    pub file: String,
}

/// Information describing a trait contract implementation (`impl Trait for Target`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractInfo {
    /// Name of implemented trait.
    pub trait_name: String,
    /// Name of target struct or type.
    pub target_type: String,
    /// List of method signatures provided by this contract.
    pub methods: Vec<String>,
    /// Source file path where implementation resides.
    pub file: String,
}

/// Symbol definition entry in the index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolDef {
    /// Symbol name.
    pub name: String,
    /// Kind ("fn", "struct", "enum", "trait", "type").
    pub kind: String,
    /// File where symbol is declared.
    pub file: String,
}

/// In-memory semantic index across a codebase.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SymbolIndex {
    /// Call-graph edges indexed by callee -> callers.
    pub callers_map: HashMap<String, Vec<CallEdge>>,
    /// Call-graph edges indexed by caller -> callees.
    pub callees_map: HashMap<String, Vec<CallEdge>>,
    /// Trait implementations indexed by target type name.
    pub contracts_map: HashMap<String, Vec<ContractInfo>>,
    /// Declared symbols indexed by name.
    pub symbols_map: HashMap<String, Vec<SymbolDef>>,
}

impl SymbolIndex {
    /// Creates an empty symbol index.
    pub fn new() -> Self {
        Self::default()
    }

    /// Indexes all `.rs` files located under the target directory path.
    pub fn index_directory(&mut self, root: &Path) -> Result<usize, SliceError> {
        let mut indexed_count = 0;
        for entry in WalkDir::new(root)
            .into_iter()
            .filter_entry(|e| !is_ignored(e.path()))
        {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            if entry.file_type().is_file()
                && entry.path().extension().is_some_and(|ext| ext == "rs")
            {
                if let Ok(content) = std::fs::read_to_string(entry.path()) {
                    let _ =
                        self.index_source_str(entry.path().to_string_lossy().as_ref(), &content);
                    indexed_count += 1;
                }
            }
        }
        Ok(indexed_count)
    }

    /// Indexes a single Rust source string.
    pub fn index_source_str(&mut self, file_path: &str, content: &str) -> Result<(), SliceError> {
        let ast =
            syn::parse_str::<syn::File>(content).map_err(|source| SliceError::ParseFailure {
                path: PathBuf::from(file_path),
                source,
            })?;

        let mut visitor = IndexVisitor::new(file_path);
        visitor.visit_file(&ast);

        for edge in visitor.edges {
            self.callers_map
                .entry(edge.callee.clone())
                .or_default()
                .push(edge.clone());
            self.callees_map
                .entry(edge.caller.clone())
                .or_default()
                .push(edge);
        }

        for contract in visitor.contracts {
            self.contracts_map
                .entry(contract.target_type.clone())
                .or_default()
                .push(contract);
        }

        for sym in visitor.symbols {
            self.symbols_map
                .entry(sym.name.clone())
                .or_default()
                .push(sym);
        }

        Ok(())
    }

    /// Retrieves upstream callers of a symbol up to the requested depth.
    pub fn get_callers(&self, symbol: &str, depth: usize) -> Vec<CallEdge> {
        let mut results = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = vec![(symbol.to_string(), 0)];

        while let Some((curr, d)) = queue.pop() {
            if d >= depth || !visited.insert(curr.clone()) {
                continue;
            }
            if let Some(edges) = self.callers_map.get(&curr) {
                for edge in edges {
                    results.push(edge.clone());
                    queue.push((edge.caller.clone(), d + 1));
                }
            }
        }
        results
    }

    /// Retrieves downstream callees called by a symbol up to the requested depth.
    pub fn get_callees(&self, symbol: &str, depth: usize) -> Vec<CallEdge> {
        let mut results = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = vec![(symbol.to_string(), 0)];

        while let Some((curr, d)) = queue.pop() {
            if d >= depth || !visited.insert(curr.clone()) {
                continue;
            }
            if let Some(edges) = self.callees_map.get(&curr) {
                for edge in edges {
                    results.push(edge.clone());
                    queue.push((edge.callee.clone(), d + 1));
                }
            }
        }
        results
    }

    /// Retrieves trait contracts associated with a target type.
    pub fn get_contracts(&self, target_type: &str) -> Vec<ContractInfo> {
        self.contracts_map
            .get(target_type)
            .cloned()
            .unwrap_or_default()
    }
}

fn is_ignored(path: &Path) -> bool {
    let s = path.to_string_lossy();
    s.contains("/target/") || s.contains("/.git/")
}

struct IndexVisitor<'a> {
    file_path: &'a str,
    current_fn: Option<String>,
    edges: Vec<CallEdge>,
    contracts: Vec<ContractInfo>,
    symbols: Vec<SymbolDef>,
}

impl<'a> IndexVisitor<'a> {
    fn new(file_path: &'a str) -> Self {
        Self {
            file_path,
            current_fn: None,
            edges: Vec::new(),
            contracts: Vec::new(),
            symbols: Vec::new(),
        }
    }
}

impl<'a> Visit<'_> for IndexVisitor<'a> {
    fn visit_item_fn(&mut self, node: &syn::ItemFn) {
        let name = node.sig.ident.to_string();
        self.symbols.push(SymbolDef {
            name: name.clone(),
            kind: "fn".to_string(),
            file: self.file_path.to_string(),
        });

        let prev = self.current_fn.replace(name);
        syn::visit::visit_item_fn(self, node);
        self.current_fn = prev;
    }

    fn visit_item_struct(&mut self, node: &syn::ItemStruct) {
        self.symbols.push(SymbolDef {
            name: node.ident.to_string(),
            kind: "struct".to_string(),
            file: self.file_path.to_string(),
        });
        syn::visit::visit_item_struct(self, node);
    }

    fn visit_item_enum(&mut self, node: &syn::ItemEnum) {
        self.symbols.push(SymbolDef {
            name: node.ident.to_string(),
            kind: "enum".to_string(),
            file: self.file_path.to_string(),
        });
        syn::visit::visit_item_enum(self, node);
    }

    fn visit_item_trait(&mut self, node: &syn::ItemTrait) {
        self.symbols.push(SymbolDef {
            name: node.ident.to_string(),
            kind: "trait".to_string(),
            file: self.file_path.to_string(),
        });
        syn::visit::visit_item_trait(self, node);
    }

    fn visit_item_impl(&mut self, node: &syn::ItemImpl) {
        let target_type = node.self_ty.to_token_stream().to_string().replace(' ', "");
        if let Some((_, ref trait_path, _)) = node.trait_ {
            let trait_name = trait_path.to_token_stream().to_string().replace(' ', "");
            let mut methods = Vec::new();
            for item in &node.items {
                if let syn::ImplItem::Fn(m) = item {
                    methods.push(m.sig.to_token_stream().to_string());
                }
            }
            self.contracts.push(ContractInfo {
                trait_name,
                target_type,
                methods,
                file: self.file_path.to_string(),
            });
        }
        syn::visit::visit_item_impl(self, node);
    }

    fn visit_impl_item_fn(&mut self, node: &syn::ImplItemFn) {
        let name = node.sig.ident.to_string();
        let prev = self.current_fn.replace(name);
        syn::visit::visit_impl_item_fn(self, node);
        self.current_fn = prev;
    }

    fn visit_expr_call(&mut self, node: &syn::ExprCall) {
        if let Some(ref caller) = self.current_fn {
            let callee = node.func.to_token_stream().to_string();
            // Take the last path segment identifier
            let short_callee = callee
                .split("::")
                .last()
                .unwrap_or(&callee)
                .trim()
                .to_string();
            self.edges.push(CallEdge {
                caller: caller.clone(),
                callee: short_callee,
                file: self.file_path.to_string(),
            });
        }
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &syn::ExprMethodCall) {
        if let Some(ref caller) = self.current_fn {
            self.edges.push(CallEdge {
                caller: caller.clone(),
                callee: node.method.to_string(),
                file: self.file_path.to_string(),
            });
        }
        syn::visit::visit_expr_method_call(self, node);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_indexing_calls_and_contracts() {
        let code = r#"
            pub trait Engine {
                fn run(&self);
            }

            pub struct Rocket;

            impl Engine for Rocket {
                fn run(&self) {
                    ignite();
                }
            }

            fn ignite() {
                println!("Ignition");
            }
        "#;

        let mut index = SymbolIndex::new();
        index.index_source_str("rocket.rs", code).unwrap();

        let callers = index.get_callers("ignite", 1);
        assert!(!callers.is_empty());
        assert_eq!(callers[0].caller, "run");

        let contracts = index.get_contracts("Rocket");
        assert_eq!(contracts.len(), 1);
        assert_eq!(contracts[0].trait_name, "Engine");
    }
}
