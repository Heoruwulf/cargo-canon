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
    /// Kind ("fn", "struct", "enum", "trait", "type", "interface").
    pub kind: String,
    /// File where symbol is declared.
    pub file: String,
    /// Line number where declared (1-indexed).
    #[serde(default = "default_line")]
    pub line: usize,
    /// Language origin ("rust" or "typescript").
    #[serde(default = "default_rust_lang")]
    pub language: String,
}

fn default_line() -> usize {
    1
}

fn default_rust_lang() -> String {
    "rust".to_string()
}

/// Information describing an API route, IPC command, or client fetch call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteBinding {
    /// HTTP path or command name (e.g. "/api/users", "update_user").
    pub route_path: String,
    /// Handler function or calling function name.
    pub handler_or_caller: String,
    /// Source file path.
    pub file: String,
    /// Line number where route/call is defined (1-indexed).
    pub line: usize,
    /// Whether this is a backend definition (true) or frontend client call site (false).
    pub is_backend: bool,
}

/// Cross-boundary correlation between Rust backend and TypeScript frontend contracts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrossBoundaryImpact {
    /// Inspected target symbol.
    pub symbol: String,
    /// Rust backend definition site, e.g. "crates/server/src/routes/users.rs:42 (UpdateUserPayload)".
    pub rust_definition: Option<String>,
    /// TypeScript consumer interface, e.g. "frontend/src/types/user.ts:18 (UpdateUserPayload)".
    pub ts_interface: Option<String>,
    /// Frontend client call sites.
    pub api_call_sites: Vec<String>,
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
    /// Recorded API routes and client fetch calls.
    #[serde(default)]
    pub routes: Vec<RouteBinding>,
}

impl SymbolIndex {
    /// Creates an empty symbol index.
    pub fn new() -> Self {
        Self::default()
    }

    /// Indexes all source files (.rs, .ts, .tsx, .js, .jsx) located under root path.
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
            if entry.file_type().is_file() {
                let ext = entry
                    .path()
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("");
                if matches!(ext, "rs" | "ts" | "tsx" | "js" | "jsx") {
                    if let Ok(content) = std::fs::read_to_string(entry.path()) {
                        let path_str = entry.path().to_string_lossy().to_string();
                        let _ = self.index_any_source_str(&path_str, &content);
                        indexed_count += 1;
                    }
                }
            }
        }
        Ok(indexed_count)
    }

    /// Indexes source from any supported language (Rust, TypeScript, TSX, JS).
    pub fn index_any_source_str(
        &mut self,
        file_path: &str,
        content: &str,
    ) -> Result<(), SliceError> {
        let is_ts = file_path.ends_with(".ts")
            || file_path.ends_with(".tsx")
            || file_path.ends_with(".js")
            || file_path.ends_with(".jsx");
        if is_ts {
            self.index_ts_source_str(file_path, content)
        } else {
            self.index_source_str(file_path, content)
        }
    }

    /// Indexes a single Rust source string.
    pub fn index_source_str(&mut self, file_path: &str, content: &str) -> Result<(), SliceError> {
        let ast =
            syn::parse_str::<syn::File>(content).map_err(|source| SliceError::ParseFailure {
                path: PathBuf::from(file_path),
                source,
            })?;

        let mut visitor = IndexVisitor::new(file_path, content);
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

        for route in visitor.routes {
            self.routes.push(route);
        }

        Ok(())
    }

    /// Indexes a single TypeScript, TSX, or JavaScript source string.
    pub fn index_ts_source_str(
        &mut self,
        file_path: &str,
        content: &str,
    ) -> Result<(), SliceError> {
        let allocator = oxc_allocator::Allocator::default();
        let is_jsx = file_path.ends_with(".tsx") || file_path.ends_with(".jsx");
        let source_type = oxc_span::SourceType::default()
            .with_typescript(true)
            .with_jsx(is_jsx);
        let ret = oxc_parser::Parser::new(&allocator, content, source_type).parse();

        let get_line = |offset: u32| -> usize {
            1 + content[..(offset as usize).min(content.len())]
                .matches('\n')
                .count()
        };

        for stmt in &ret.program.body {
            match stmt {
                oxc_ast::ast::Statement::ExportNamedDeclaration(decl) => {
                    if let Some(ref d) = decl.declaration {
                        self.index_ts_declaration(d, file_path, &get_line);
                    }
                }
                oxc_ast::ast::Statement::TSInterfaceDeclaration(decl) => {
                    let name = decl.id.name.to_string();
                    let line = get_line(decl.span.start);
                    self.symbols_map
                        .entry(name.clone())
                        .or_default()
                        .push(SymbolDef {
                            name,
                            kind: "interface".to_string(),
                            file: file_path.to_string(),
                            line,
                            language: "typescript".to_string(),
                        });
                }
                oxc_ast::ast::Statement::TSTypeAliasDeclaration(decl) => {
                    let name = decl.id.name.to_string();
                    let line = get_line(decl.span.start);
                    self.symbols_map
                        .entry(name.clone())
                        .or_default()
                        .push(SymbolDef {
                            name,
                            kind: "type".to_string(),
                            file: file_path.to_string(),
                            line,
                            language: "typescript".to_string(),
                        });
                }
                oxc_ast::ast::Statement::TSEnumDeclaration(decl) => {
                    let name = decl.id.name.to_string();
                    let line = get_line(decl.span.start);
                    self.symbols_map
                        .entry(name.clone())
                        .or_default()
                        .push(SymbolDef {
                            name,
                            kind: "enum".to_string(),
                            file: file_path.to_string(),
                            line,
                            language: "typescript".to_string(),
                        });
                }
                oxc_ast::ast::Statement::FunctionDeclaration(func) => {
                    if let Some(ref id) = func.id {
                        let name = id.name.to_string();
                        let line = get_line(func.span.start);
                        self.symbols_map
                            .entry(name.clone())
                            .or_default()
                            .push(SymbolDef {
                                name,
                                kind: "fn".to_string(),
                                file: file_path.to_string(),
                                line,
                                language: "typescript".to_string(),
                            });
                    }
                }
                _ => {}
            }
        }

        self.extract_ts_route_calls(file_path, content, &get_line);
        Ok(())
    }

    fn index_ts_declaration(
        &mut self,
        decl: &oxc_ast::ast::Declaration<'_>,
        file_path: &str,
        get_line: &impl Fn(u32) -> usize,
    ) {
        match decl {
            oxc_ast::ast::Declaration::TSInterfaceDeclaration(d) => {
                let name = d.id.name.to_string();
                let line = get_line(d.span.start);
                self.symbols_map
                    .entry(name.clone())
                    .or_default()
                    .push(SymbolDef {
                        name,
                        kind: "interface".to_string(),
                        file: file_path.to_string(),
                        line,
                        language: "typescript".to_string(),
                    });
            }
            oxc_ast::ast::Declaration::TSTypeAliasDeclaration(d) => {
                let name = d.id.name.to_string();
                let line = get_line(d.span.start);
                self.symbols_map
                    .entry(name.clone())
                    .or_default()
                    .push(SymbolDef {
                        name,
                        kind: "type".to_string(),
                        file: file_path.to_string(),
                        line,
                        language: "typescript".to_string(),
                    });
            }
            oxc_ast::ast::Declaration::TSEnumDeclaration(d) => {
                let name = d.id.name.to_string();
                let line = get_line(d.span.start);
                self.symbols_map
                    .entry(name.clone())
                    .or_default()
                    .push(SymbolDef {
                        name,
                        kind: "enum".to_string(),
                        file: file_path.to_string(),
                        line,
                        language: "typescript".to_string(),
                    });
            }
            oxc_ast::ast::Declaration::FunctionDeclaration(func) => {
                if let Some(ref id) = func.id {
                    let name = id.name.to_string();
                    let line = get_line(func.span.start);
                    self.symbols_map
                        .entry(name.clone())
                        .or_default()
                        .push(SymbolDef {
                            name,
                            kind: "fn".to_string(),
                            file: file_path.to_string(),
                            line,
                            language: "typescript".to_string(),
                        });
                }
            }
            _ => {}
        }
    }

    fn extract_ts_route_calls(
        &mut self,
        file_path: &str,
        content: &str,
        get_line: &impl Fn(u32) -> usize,
    ) {
        let pattern = regex::Regex::new(r#"(?:fetch|invoke|axios\s*\.\s*(?:get|post|put|delete|patch))\s*\(\s*["'`]([^"'`]+)["'`]"#).unwrap();
        for cap in pattern.captures_iter(content) {
            if let Some(m) = cap.get(0) {
                let route = cap.get(1).map(|c| c.as_str()).unwrap_or("");
                let line = get_line(m.start() as u32);
                let caller = find_enclosing_fn(content, m.start());
                self.routes.push(RouteBinding {
                    route_path: route.to_string(),
                    handler_or_caller: caller,
                    file: file_path.to_string(),
                    line,
                    is_backend: false,
                });
            }
        }
    }

    /// Computes cross-boundary impact correlating Rust backend models/endpoints
    /// with TypeScript types and frontend client API call sites.
    pub fn get_cross_boundary_impact(&self, symbol: &str) -> Option<CrossBoundaryImpact> {
        let rust_def = self
            .symbols_map
            .get(symbol)
            .and_then(|defs| {
                defs.iter()
                    .find(|d| d.language == "rust" || d.file.ends_with(".rs"))
            })
            .map(|d| format!("{}:{} (`{}`)", d.file, d.line, d.name));

        let ts_def = self
            .symbols_map
            .get(symbol)
            .and_then(|defs| {
                defs.iter().find(|d| {
                    d.language == "typescript"
                        || d.file.ends_with(".ts")
                        || d.file.ends_with(".tsx")
                })
            })
            .map(|d| format!("{}:{} (`{}`)", d.file, d.line, d.name));

        let mut call_sites = Vec::new();
        let sym_trimmed = symbol
            .trim_end_matches("Payload")
            .trim_end_matches("Request")
            .trim_end_matches("Response");
        let sym_lower = symbol.to_lowercase();
        let trimmed_lower = sym_trimmed.to_lowercase();

        for r in &self.routes {
            if !r.is_backend {
                let r_lower = r.route_path.to_lowercase();
                let handler_lower = r.handler_or_caller.to_lowercase();
                let matched = r_lower.contains(&sym_lower)
                    || handler_lower.contains(&sym_lower)
                    || handler_lower.contains(&trimmed_lower)
                    || (!trimmed_lower.is_empty() && r_lower.contains(&trimmed_lower))
                    || (trimmed_lower.len() >= 4
                        && (r_lower.contains(&trimmed_lower[..4])
                            || handler_lower.contains(&trimmed_lower[..4])));
                if matched {
                    call_sites.push(format!(
                        "{}:{} (`{}()`)",
                        r.file, r.line, r.handler_or_caller
                    ));
                }
            }
        }

        call_sites.sort();
        call_sites.dedup();

        if rust_def.is_some() || ts_def.is_some() || !call_sites.is_empty() {
            Some(CrossBoundaryImpact {
                symbol: symbol.to_string(),
                rust_definition: rust_def,
                ts_interface: ts_def,
                api_call_sites: call_sites,
            })
        } else {
            None
        }
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

fn find_line(content: &str, pattern: &str) -> usize {
    if let Some(pos) = content.find(pattern) {
        1 + content[..pos].matches('\n').count()
    } else {
        1
    }
}

fn extract_string_literal(input: &str) -> Option<String> {
    let start = input.find('"')?;
    let end = input[start + 1..].find('"')?;
    Some(input[start + 1..start + 1 + end].to_string())
}

fn find_enclosing_fn(content: &str, offset: usize) -> String {
    let before = &content[..offset];
    for line in before.lines().rev() {
        let trimmed = line.trim();
        for prefix in &[
            "export async function ",
            "async function ",
            "export function ",
            "function ",
        ] {
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                if let Some(name) = rest.split(['(', ' ']).next() {
                    let cleaned = name.trim();
                    if !cleaned.is_empty() {
                        return cleaned.to_string();
                    }
                }
            }
        }
        for prefix in &["export const ", "const ", "export let ", "let "] {
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                if let Some(name) = rest.split(['=', ':', '(']).next() {
                    let cleaned = name.trim();
                    if !cleaned.is_empty() {
                        return cleaned.to_string();
                    }
                }
            }
        }
    }
    "apiCall".to_string()
}

struct IndexVisitor<'a> {
    file_path: &'a str,
    content: &'a str,
    current_fn: Option<String>,
    edges: Vec<CallEdge>,
    contracts: Vec<ContractInfo>,
    symbols: Vec<SymbolDef>,
    routes: Vec<RouteBinding>,
}

impl<'a> IndexVisitor<'a> {
    fn new(file_path: &'a str, content: &'a str) -> Self {
        Self {
            file_path,
            content,
            current_fn: None,
            edges: Vec::new(),
            contracts: Vec::new(),
            symbols: Vec::new(),
            routes: Vec::new(),
        }
    }
}

impl<'a> Visit<'_> for IndexVisitor<'a> {
    fn visit_item_fn(&mut self, node: &syn::ItemFn) {
        let name = node.sig.ident.to_string();
        let pattern = format!("fn {}", name);
        let line = find_line(self.content, &pattern);
        self.symbols.push(SymbolDef {
            name: name.clone(),
            kind: "fn".to_string(),
            file: self.file_path.to_string(),
            line,
            language: "rust".to_string(),
        });

        for attr in &node.attrs {
            let attr_str = attr.to_token_stream().to_string();
            if attr_str.contains("tauri :: command") || attr_str.contains("tauri::command") {
                self.routes.push(RouteBinding {
                    route_path: name.clone(),
                    handler_or_caller: name.clone(),
                    file: self.file_path.to_string(),
                    line,
                    is_backend: true,
                });
            } else if attr_str.contains("get")
                || attr_str.contains("post")
                || attr_str.contains("put")
                || attr_str.contains("delete")
            {
                let route_path = extract_string_literal(&attr_str).unwrap_or_else(|| name.clone());
                self.routes.push(RouteBinding {
                    route_path,
                    handler_or_caller: name.clone(),
                    file: self.file_path.to_string(),
                    line,
                    is_backend: true,
                });
            }
        }

        let prev = self.current_fn.replace(name);
        syn::visit::visit_item_fn(self, node);
        self.current_fn = prev;
    }

    fn visit_item_struct(&mut self, node: &syn::ItemStruct) {
        let name = node.ident.to_string();
        let pattern = format!("struct {}", name);
        let line = find_line(self.content, &pattern);
        self.symbols.push(SymbolDef {
            name,
            kind: "struct".to_string(),
            file: self.file_path.to_string(),
            line,
            language: "rust".to_string(),
        });
        syn::visit::visit_item_struct(self, node);
    }

    fn visit_item_enum(&mut self, node: &syn::ItemEnum) {
        let name = node.ident.to_string();
        let pattern = format!("enum {}", name);
        let line = find_line(self.content, &pattern);
        self.symbols.push(SymbolDef {
            name,
            kind: "enum".to_string(),
            file: self.file_path.to_string(),
            line,
            language: "rust".to_string(),
        });
        syn::visit::visit_item_enum(self, node);
    }

    fn visit_item_trait(&mut self, node: &syn::ItemTrait) {
        let name = node.ident.to_string();
        let pattern = format!("trait {}", name);
        let line = find_line(self.content, &pattern);
        self.symbols.push(SymbolDef {
            name,
            kind: "trait".to_string(),
            file: self.file_path.to_string(),
            line,
            language: "rust".to_string(),
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

    #[test]
    fn test_cross_boundary_impact_indexing() {
        let rust_code = r#"
            pub struct UpdateUserPayload {
                pub user_id: String,
            }

            #[tauri::command]
            pub fn update_user(payload: UpdateUserPayload) {}
        "#;

        let ts_types = r#"
            export interface UpdateUserPayload {
                userId: string;
            }
        "#;

        let ts_client = r#"
            export async function updateUser(payload: UpdateUserPayload) {
                return fetch("/api/users", { method: "POST" });
            }
        "#;

        let mut index = SymbolIndex::new();
        index
            .index_any_source_str("crates/server/src/routes/users.rs", rust_code)
            .unwrap();
        index
            .index_any_source_str("frontend/src/types/user.ts", ts_types)
            .unwrap();
        index
            .index_any_source_str("frontend/src/api/userClient.ts", ts_client)
            .unwrap();

        let impact = index
            .get_cross_boundary_impact("UpdateUserPayload")
            .expect("impact must be found");
        assert_eq!(impact.symbol, "UpdateUserPayload");
        assert!(
            impact
                .rust_definition
                .as_ref()
                .unwrap()
                .contains("crates/server/src/routes/users.rs")
        );
        assert!(
            impact
                .ts_interface
                .as_ref()
                .unwrap()
                .contains("frontend/src/types/user.ts")
        );
        assert!(!impact.api_call_sites.is_empty());
        assert!(impact.api_call_sites[0].contains("userClient.ts"));
        assert!(impact.api_call_sites[0].contains("updateUser"));
    }
}
