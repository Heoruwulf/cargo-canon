//! Semantic call-graph and contract indexer for Rust codebases.

use crate::error::SliceError;
use quote::ToTokens;
use rayon::prelude::*;
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

/// Cross-boundary correlation between Rust backend, Go backend, and TypeScript frontend contracts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrossBoundaryImpact {
    /// Inspected target symbol.
    pub symbol: String,
    /// Rust backend definition site, e.g. "crates/server/src/routes/users.rs:42 (UpdateUserPayload)".
    pub rust_definition: Option<String>,
    /// TypeScript consumer interface, e.g. "frontend/src/types/user.ts:18 (UpdateUserPayload)".
    pub ts_interface: Option<String>,
    /// Go backend definition site, e.g. "internal/model/user.go:18 (User)".
    #[serde(default)]
    pub go_definition: Option<String>,
    /// Frontend client call sites.
    pub api_call_sites: Vec<String>,
}

/// Metric summary of the in-memory symbol graph.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolIndexMetrics {
    /// Number of distinct source files containing declared symbols.
    pub distinct_files_count: usize,
    /// Total number of symbols declared in index.
    pub total_symbols_count: usize,
    /// Total number of caller -> callee edges indexed.
    pub total_call_edges_count: usize,
    /// Total number of trait/interface contract implementations recorded.
    pub total_contracts_count: usize,
    /// Total number of HTTP routes and client bindings recorded.
    pub total_routes_count: usize,
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

    /// Computes summary metrics of the current semantic graph in O(N).
    pub fn metrics(&self) -> SymbolIndexMetrics {
        let mut files = HashSet::new();
        let mut total_symbols = 0;
        for defs in self.symbols_map.values() {
            total_symbols += defs.len();
            for def in defs {
                files.insert(&def.file);
            }
        }
        let total_call_edges: usize = self.callers_map.values().map(|v| v.len()).sum();
        let total_contracts: usize = self.contracts_map.values().map(|v| v.len()).sum();

        SymbolIndexMetrics {
            distinct_files_count: files.len(),
            total_symbols_count: total_symbols,
            total_call_edges_count: total_call_edges,
            total_contracts_count: total_contracts,
            total_routes_count: self.routes.len(),
        }
    }

    /// Merges another SymbolIndex into this one in-place.
    pub fn merge(&mut self, other: SymbolIndex) {
        for (k, v) in other.callers_map {
            self.callers_map.entry(k).or_default().extend(v);
        }
        for (k, v) in other.callees_map {
            self.callees_map.entry(k).or_default().extend(v);
        }
        for (k, v) in other.contracts_map {
            self.contracts_map.entry(k).or_default().extend(v);
        }
        for (k, v) in other.symbols_map {
            self.symbols_map.entry(k).or_default().extend(v);
        }
        self.routes.extend(other.routes);
    }

    /// Discovers candidate source files filtering ignored directories.
    pub fn collect_source_files(&self, root: &Path) -> Vec<PathBuf> {
        WalkDir::new(root)
            .into_iter()
            .filter_entry(|e| !is_ignored_path(e.path()))
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter(|e| {
                e.path()
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| matches!(ext, "rs" | "ts" | "tsx" | "js" | "jsx" | "go"))
            })
            .map(|e| e.into_path())
            .collect()
    }

    /// Discovers and indexes all valid source files (.rs, .ts, .tsx, .js, .jsx, .go)
    /// under root in parallel using Rayon worker threads.
    pub fn index_directory(&mut self, root: &Path) -> Result<usize, SliceError> {
        let start = std::time::Instant::now();
        let files = self.collect_source_files(root);
        let discovery_ms = start.elapsed().as_millis();
        let total_files = files.len();
        if total_files == 0 {
            tracing::info!(root = %root.display(), "No source files discovered for indexing");
            return Ok(0);
        }

        tracing::info!(
            root = %root.display(),
            files = total_files,
            discovery_ms,
            "Discovered candidate source files; starting parallel AST indexing"
        );

        let parse_start = std::time::Instant::now();
        let partial_index = files
            .par_iter()
            .fold(SymbolIndex::new, |mut acc, path| {
                if let Ok(content) = std::fs::read_to_string(path) {
                    let path_str = path.to_string_lossy().to_string();
                    let _ = acc.index_any_source_str(&path_str, &content);
                }
                acc
            })
            .reduce(SymbolIndex::new, |mut a, b| {
                a.merge(b);
                a
            });
        let parse_ms = parse_start.elapsed().as_millis();

        let merge_start = std::time::Instant::now();
        self.merge(partial_index);
        let merge_ms = merge_start.elapsed().as_millis();

        let total_elapsed = start.elapsed();
        let total_ms = total_elapsed.as_millis();
        let throughput = if total_elapsed.as_secs_f64() > 0.0 {
            (total_files as f64) / total_elapsed.as_secs_f64()
        } else {
            0.0
        };

        let metrics = self.metrics();
        tracing::info!(
            root = %root.display(),
            files = total_files,
            symbols = metrics.total_symbols_count,
            edges = metrics.total_call_edges_count,
            contracts = metrics.total_contracts_count,
            routes = metrics.total_routes_count,
            discovery_ms,
            parse_ms,
            merge_ms,
            total_ms,
            throughput_fps = format!("{:.0}", throughput),
            "Initial indexing complete"
        );
        Ok(total_files)
    }

    /// Sequential fallback for single-threaded or deterministic testing.
    pub fn index_directory_sequential(&mut self, root: &Path) -> Result<usize, SliceError> {
        let start = std::time::Instant::now();
        let files = self.collect_source_files(root);
        let total_files = files.len();
        for path in &files {
            if let Ok(content) = std::fs::read_to_string(path) {
                let path_str = path.to_string_lossy().to_string();
                let _ = self.index_any_source_str(&path_str, &content);
            }
        }
        let total_elapsed = start.elapsed();
        let metrics = self.metrics();
        tracing::info!(
            root = %root.display(),
            files = total_files,
            symbols = metrics.total_symbols_count,
            total_ms = total_elapsed.as_millis(),
            "Sequential indexing complete"
        );
        Ok(total_files)
    }

    /// Indexes source from any supported language (Rust, TypeScript, TSX, JS, Go).
    pub fn index_any_source_str(
        &mut self,
        file_path: &str,
        content: &str,
    ) -> Result<(), SliceError> {
        if file_path.ends_with(".go") {
            self.index_go_source_str(file_path, content)
        } else if file_path.ends_with(".ts")
            || file_path.ends_with(".tsx")
            || file_path.ends_with(".js")
            || file_path.ends_with(".jsx")
        {
            self.index_ts_source_str(file_path, content)
        } else {
            self.index_source_str(file_path, content)
        }
    }

    /// Indexes a single Go source string.
    pub fn index_go_source_str(
        &mut self,
        file_path: &str,
        content: &str,
    ) -> Result<(), SliceError> {
        let data = crate::go_skeleton::index_go_ast(file_path, content)?;
        for sym in data.symbols {
            self.symbols_map
                .entry(sym.name.clone())
                .or_default()
                .push(sym);
        }
        for contract in data.contracts {
            self.contracts_map
                .entry(contract.target_type.clone())
                .or_default()
                .push(contract);
        }
        for route in data.routes {
            self.routes.push(route);
        }
        Ok(())
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

        let go_def = self
            .symbols_map
            .get(symbol)
            .and_then(|defs| {
                defs.iter()
                    .find(|d| d.language == "go" || d.file.ends_with(".go"))
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
                if r.is_backend && r.file.ends_with(".go") {
                    call_sites.push(format!(
                        "Go Router: {}:{} -> `{}`",
                        r.file, r.line, r.route_path
                    ));
                } else if !r.is_backend {
                    call_sites.push(format!(
                        "{}:{} (`{}()`)",
                        r.file, r.line, r.handler_or_caller
                    ));
                }
            }
        }

        call_sites.sort();
        call_sites.dedup();

        if rust_def.is_some() || ts_def.is_some() || go_def.is_some() || !call_sites.is_empty() {
            Some(CrossBoundaryImpact {
                symbol: symbol.to_string(),
                rust_definition: rust_def,
                ts_interface: ts_def,
                go_definition: go_def,
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
        let mut list = self
            .contracts_map
            .get(target_type)
            .cloned()
            .unwrap_or_default();

        if !target_type.starts_with('*') {
            let ptr_name = format!("*{}", target_type);
            if let Some(ptr_contracts) = self.contracts_map.get(&ptr_name) {
                list.extend(ptr_contracts.clone());
            }
        }

        if list.is_empty() {
            for contracts in self.contracts_map.values() {
                for c in contracts {
                    if c.trait_name == target_type
                        || c.trait_name.ends_with(&format!(".{target_type}"))
                    {
                        list.push(c.clone());
                    }
                }
            }
        }
        list
    }

    /// Returns satisfied interfaces for a Go struct type (both value and pointer receiver sets).
    pub fn get_satisfied_interfaces(&self, struct_name: &str) -> Vec<ContractInfo> {
        self.get_contracts(struct_name)
    }

    /// Returns concrete implementor type names of an interface.
    pub fn get_interface_implementors(&self, interface_name: &str) -> Vec<String> {
        let mut implementors = Vec::new();
        for (target_type, contracts) in &self.contracts_map {
            for c in contracts {
                if c.trait_name == interface_name
                    || c.trait_name.ends_with(&format!(".{interface_name}"))
                {
                    implementors.push(target_type.clone());
                }
            }
        }
        implementors.sort();
        implementors.dedup();
        implementors
    }
}

/// Returns true if any path segment corresponds to a known dependency,
/// build artifact, VCS metadata, or cache directory.
pub fn is_ignored_path(path: &Path) -> bool {
    for component in path.components() {
        if let std::path::Component::Normal(c) = component {
            if let Some(s) = c.to_str() {
                if matches!(
                    s,
                    "target"
                        | ".git"
                        | "node_modules"
                        | "vendor"
                        | "dist"
                        | "build"
                        | "out"
                        | ".next"
                        | ".nuxt"
                        | ".turbo"
                        | ".yarn"
                        | ".pnpm-store"
                        | ".output"
                        | ".svelte-kit"
                        | "coverage"
                        | ".idea"
                        | ".vscode"
                ) {
                    return true;
                }
            }
        }
    }
    false
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

    #[test]
    fn test_metrics_empty() {
        let index = SymbolIndex::new();
        let m = index.metrics();
        assert_eq!(m.distinct_files_count, 0);
        assert_eq!(m.total_symbols_count, 0);
        assert_eq!(m.total_call_edges_count, 0);
        assert_eq!(m.total_contracts_count, 0);
        assert_eq!(m.total_routes_count, 0);
    }

    #[test]
    fn test_metrics_populated() {
        let mut index = SymbolIndex::new();
        let rust_code = r#"
            pub struct Engine {
                pub id: u64,
            }
            pub trait Runner {
                fn run(&self);
            }
            impl Runner for Engine {
                fn run(&self) {}
            }
            pub fn start_all() {
                let e = Engine { id: 1 };
                e.run();
            }
        "#;
        index
            .index_any_source_str("src/engine.rs", rust_code)
            .unwrap();
        let m = index.metrics();
        assert_eq!(m.distinct_files_count, 1);
        assert!(m.total_symbols_count >= 3);
        assert!(m.total_contracts_count >= 1);
        assert!(m.total_call_edges_count >= 1);
    }

    #[test]
    fn test_is_ignored_path() {
        // Ignored relative paths
        assert!(is_ignored_path(Path::new("target/debug/foo.rs")));
        assert!(is_ignored_path(Path::new("node_modules/pkg/index.js")));
        assert!(is_ignored_path(Path::new(".git/HEAD")));
        assert!(is_ignored_path(Path::new("vendor/pkg/foo.go")));
        assert!(is_ignored_path(Path::new("dist/bundle.js")));
        assert!(is_ignored_path(Path::new("build/output.js")));
        assert!(is_ignored_path(Path::new("out/main.js")));
        assert!(is_ignored_path(Path::new(".next/server.js")));
        assert!(is_ignored_path(Path::new(".nuxt/index.js")));
        assert!(is_ignored_path(Path::new(".turbo/cache.bin")));
        assert!(is_ignored_path(Path::new(".yarn/releases.cjs")));
        assert!(is_ignored_path(Path::new(".pnpm-store/v3.bin")));
        assert!(is_ignored_path(Path::new(".output/server/index.mjs")));
        assert!(is_ignored_path(Path::new(".svelte-kit/generated.js")));
        assert!(is_ignored_path(Path::new("coverage/lcov.info")));
        assert!(is_ignored_path(Path::new(".idea/workspace.xml")));
        assert!(is_ignored_path(Path::new(".vscode/settings.json")));

        // Ignored nested subdirectories and absolute paths
        assert!(is_ignored_path(Path::new(
            "/home/user/project/node_modules/foo/index.ts"
        )));
        assert!(is_ignored_path(Path::new(
            "/home/user/project/target/release/lib.rs"
        )));
        assert!(is_ignored_path(Path::new("frontend/nested/vendor/dep.go")));

        // Valid paths that should NOT be ignored
        assert!(!is_ignored_path(Path::new("src/main.rs")));
        assert!(!is_ignored_path(Path::new(
            "crates/canon-slice/src/indexer.rs"
        )));
        assert!(!is_ignored_path(Path::new(
            "/var/workspace/app/target_metrics.rs"
        )));
        assert!(!is_ignored_path(Path::new("src/validate_target.ts")));
        assert!(!is_ignored_path(Path::new("src/build_helper.rs")));
    }

    #[test]
    fn test_symbol_index_merge() {
        let mut idx1 = SymbolIndex::new();
        idx1.index_source_str(
            "src/a.rs",
            "pub fn alpha() { beta(); }\npub fn beta() {}\npub struct StructA;",
        )
        .unwrap();

        let mut idx2 = SymbolIndex::new();
        idx2.index_source_str(
            "src/b.rs",
            "pub fn gamma() { delta(); }\npub fn delta() {}\npub struct StructB;",
        )
        .unwrap();

        let mut merged = idx1.clone();
        merged.merge(idx2.clone());

        let m1 = idx1.metrics();
        let m2 = idx2.metrics();
        let mm = merged.metrics();

        assert_eq!(
            mm.distinct_files_count,
            m1.distinct_files_count + m2.distinct_files_count
        );
        assert_eq!(
            mm.total_symbols_count,
            m1.total_symbols_count + m2.total_symbols_count
        );
        assert_eq!(
            mm.total_call_edges_count,
            m1.total_call_edges_count + m2.total_call_edges_count
        );
        assert!(merged.symbols_map.contains_key("StructA"));
        assert!(merged.symbols_map.contains_key("StructB"));
        assert!(!merged.get_callers("beta", 1).is_empty());
        assert!(!merged.get_callers("delta", 1).is_empty());
    }

    #[test]
    fn test_parallel_indexing_equivalence() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();

        let src_dir = root.join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(
            src_dir.join("lib.rs"),
            "pub fn compute() { helper(); }\npub fn helper() {}\npub struct Engine;",
        )
        .unwrap();
        std::fs::write(
            src_dir.join("client.ts"),
            "export interface User { id: string; }\nexport function getUser() { return fetch(\"/api/user\"); }",
        )
        .unwrap();
        std::fs::write(
            src_dir.join("service.go"),
            "package service\ntype Handler struct {}\nfunc (h Handler) Serve() {}",
        )
        .unwrap();

        let nm_dir = root.join("node_modules").join("dummy");
        std::fs::create_dir_all(&nm_dir).unwrap();
        std::fs::write(nm_dir.join("index.js"), "module.exports = {};").unwrap();

        let target_dir = root.join("target").join("debug");
        std::fs::create_dir_all(&target_dir).unwrap();
        std::fs::write(target_dir.join("gen.rs"), "pub fn generated() {}").unwrap();

        let mut seq_idx = SymbolIndex::new();
        let seq_count = seq_idx.index_directory_sequential(root).unwrap();

        let mut par_idx = SymbolIndex::new();
        let par_count = par_idx.index_directory(root).unwrap();

        assert_eq!(seq_count, 3);
        assert_eq!(par_count, 3);

        let seq_metrics = seq_idx.metrics();
        let par_metrics = par_idx.metrics();

        assert_eq!(
            seq_metrics.distinct_files_count,
            par_metrics.distinct_files_count
        );
        assert_eq!(
            seq_metrics.total_symbols_count,
            par_metrics.total_symbols_count
        );
        assert_eq!(
            seq_metrics.total_call_edges_count,
            par_metrics.total_call_edges_count
        );
        assert_eq!(
            seq_metrics.total_contracts_count,
            par_metrics.total_contracts_count
        );
        assert_eq!(
            seq_metrics.total_routes_count,
            par_metrics.total_routes_count
        );

        assert!(!par_idx.symbols_map.contains_key("generated"));
        assert!(!seq_idx.symbols_map.contains_key("generated"));
    }
}
