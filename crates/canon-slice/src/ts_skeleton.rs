//! TypeScript and TSX AST interface skeleton stripper using oxc_parser.

use crate::error::SliceError;
use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_parser::Parser;
use oxc_span::{GetSpan, SourceType};
use std::path::Path;

/// Strips implementation logic, JSX, and local hooks from TypeScript / TSX source,
/// preserving exported functions, classes, interfaces, type aliases, and enums.
pub fn extract_ts_skeleton(source_code: &str, is_jsx: bool) -> Result<String, SliceError> {
    let allocator = Allocator::default();
    let mut source_type = SourceType::default().with_typescript(true);
    if is_jsx {
        source_type = source_type.with_jsx(true);
    }

    let ret = Parser::new(&allocator, source_code, source_type).parse();
    if !ret.errors.is_empty() && ret.program.body.is_empty() {
        return Err(SliceError::SymbolNotFound(
            "Failed to parse TypeScript/TSX syntax tree".to_string(),
        ));
    }

    let mut output = String::with_capacity(source_code.len() / 4);
    let mut items = Vec::new();

    for stmt in &ret.program.body {
        if let Some(item_str) = format_statement(stmt, source_code) {
            let trimmed = item_str.trim();
            if !trimmed.is_empty() {
                items.push(trimmed.to_string());
            }
        }
    }

    output.push_str(&items.join("\n\n"));
    Ok(output)
}

/// Reads a TypeScript or TSX file from disk and extracts its type skeleton.
pub fn extract_ts_file_skeleton(path: &Path) -> Result<String, SliceError> {
    let content = std::fs::read_to_string(path).map_err(SliceError::Io)?;
    let is_jsx = path
        .extension()
        .is_some_and(|ext| ext == "tsx" || ext == "jsx");
    extract_ts_skeleton(&content, is_jsx)
}

fn format_statement(stmt: &Statement<'_>, src: &str) -> Option<String> {
    match stmt {
        Statement::ExportNamedDeclaration(decl) => {
            if let Some(ref d) = decl.declaration {
                format_declaration(d, src, true)
            } else {
                let span = decl.span;
                Some(src[span.start as usize..span.end as usize].to_string())
            }
        }
        Statement::ExportDefaultDeclaration(decl) => match &decl.declaration {
            ExportDefaultDeclarationKind::FunctionDeclaration(func) => {
                format_function(func, src, true, true)
            }
            ExportDefaultDeclarationKind::ClassDeclaration(cls) => {
                format_class(cls, src, true, true)
            }
            _ => {
                let span = decl.span;
                Some(src[span.start as usize..span.end as usize].to_string())
            }
        },
        Statement::TSInterfaceDeclaration(decl) => {
            let span = decl.span;
            Some(src[span.start as usize..span.end as usize].to_string())
        }
        Statement::TSTypeAliasDeclaration(decl) => {
            let span = decl.span;
            Some(src[span.start as usize..span.end as usize].to_string())
        }
        Statement::TSEnumDeclaration(decl) => {
            let span = decl.span;
            Some(src[span.start as usize..span.end as usize].to_string())
        }
        Statement::FunctionDeclaration(func) => format_function(func, src, false, false),
        Statement::ClassDeclaration(cls) => format_class(cls, src, false, false),
        _ => None,
    }
}

fn format_declaration(decl: &Declaration<'_>, src: &str, is_export: bool) -> Option<String> {
    match decl {
        Declaration::TSInterfaceDeclaration(d) => {
            let span = d.span;
            let text = &src[span.start as usize..span.end as usize];
            if is_export && !text.starts_with("export") {
                Some(format!("export {}", text))
            } else {
                Some(text.to_string())
            }
        }
        Declaration::TSTypeAliasDeclaration(d) => {
            let span = d.span;
            let text = &src[span.start as usize..span.end as usize];
            if is_export && !text.starts_with("export") {
                Some(format!("export {}", text))
            } else {
                Some(text.to_string())
            }
        }
        Declaration::TSEnumDeclaration(d) => {
            let span = d.span;
            let text = &src[span.start as usize..span.end as usize];
            if is_export && !text.starts_with("export") {
                Some(format!("export {}", text))
            } else {
                Some(text.to_string())
            }
        }
        Declaration::FunctionDeclaration(func) => format_function(func, src, is_export, false),
        Declaration::ClassDeclaration(cls) => format_class(cls, src, is_export, false),
        Declaration::VariableDeclaration(var) => {
            let span = var.span;
            let text = &src[span.start as usize..span.end as usize];
            if is_export && !text.starts_with("export") {
                Some(format!("export {}", text))
            } else {
                Some(text.to_string())
            }
        }
        _ => None,
    }
}

fn format_function(
    func: &Function<'_>,
    src: &str,
    is_export: bool,
    is_default: bool,
) -> Option<String> {
    let name = func.id.as_ref().map(|id| id.name.as_str()).unwrap_or("");
    let is_component =
        name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) || has_jsx_return(func);

    let prefix = match (is_export, is_default) {
        (true, true) => "export default ",
        (true, false) => "export ",
        (false, true) => "default ",
        (false, false) => "",
    };

    let async_str = if func.r#async { "async " } else { "" };

    // Format params
    let params_str = if is_component && func.params.items.len() == 1 {
        let first_param = &func.params.items[0];
        if let Some(type_ann) = &first_param.pattern.type_annotation {
            let span = type_ann.type_annotation.span();
            let type_name = &src[span.start as usize..span.end as usize];
            format!("props: {}", type_name)
        } else {
            let span = func.params.span;
            src[span.start as usize + 1..span.end as usize - 1]
                .trim()
                .to_string()
        }
    } else {
        let span = func.params.span;
        src[span.start as usize + 1..span.end as usize - 1]
            .trim()
            .to_string()
    };

    // Return type
    let return_str = if let Some(ref ret_type) = func.return_type {
        let span = ret_type.type_annotation.span();
        format!(": {}", &src[span.start as usize..span.end as usize])
    } else if is_component {
        ": JSX.Element".to_string()
    } else {
        String::new()
    };

    Some(format!(
        "{}function {}{}({}){};",
        prefix, async_str, name, params_str, return_str
    ))
}

fn has_jsx_return(func: &Function<'_>) -> bool {
    if let Some(body) = &func.body {
        for stmt in &body.statements {
            if let Statement::ReturnStatement(ret) = stmt {
                if let Some(ref arg) = ret.argument {
                    if matches!(arg, Expression::JSXElement(_) | Expression::JSXFragment(_)) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn format_class(cls: &Class<'_>, src: &str, is_export: bool, is_default: bool) -> Option<String> {
    let name = cls.id.as_ref().map(|id| id.name.as_str()).unwrap_or("");
    let prefix = match (is_export, is_default) {
        (true, true) => "export default ",
        (true, false) => "export ",
        (false, true) => "default ",
        (false, false) => "",
    };

    let mut out = format!("{}class {} {{\n", prefix, name);
    for element in &cls.body.body {
        match element {
            ClassElement::MethodDefinition(method) => {
                let m_name = method
                    .key
                    .name()
                    .unwrap_or(std::borrow::Cow::Borrowed("method"));
                let span = method.value.params.span;
                let params = &src[span.start as usize..span.end as usize];
                let ret_str = if let Some(ref r) = method.value.return_type {
                    let r_span = r.type_annotation.span();
                    format!(": {}", &src[r_span.start as usize..r_span.end as usize])
                } else {
                    String::new()
                };
                out.push_str(&format!("    {m_name}{params}{ret_str};\n"));
            }
            ClassElement::PropertyDefinition(prop) => {
                let p_span = prop.span;
                out.push_str(&format!(
                    "    {};\n",
                    &src[p_span.start as usize..p_span.end as usize]
                ));
            }
            _ => {}
        }
    }
    out.push('}');
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_user_dashboard_tsx() {
        let code = r#"
import React, { useState, useEffect } from "react";
import { User, fetchMetrics } from "../api";

export interface UserDashboardProps {
    userId: string;
    refreshIntervalMs?: number;
    onStatusChange?: (status: "online" | "idle" | "offline") => void;
}

export function UserDashboard({ userId, refreshIntervalMs = 5000, onStatusChange }: UserDashboardProps) {
    const [metrics, setMetrics] = useState<User | null>(null);
    useEffect(() => {
        // ... 600 lines ...
    }, [userId]);

    return (
        <div className="flex flex-col p-4 bg-gray-900 border border-gray-800 rounded-xl">
            {/* ... 200 lines of JSX DOM nodes ... */}
        </div>
    );
}
"#;

        let res = extract_ts_skeleton(code, true).unwrap();
        assert!(res.contains("export interface UserDashboardProps"));
        assert!(res.contains("userId: string;"));
        assert!(
            res.contains("export function UserDashboard(props: UserDashboardProps): JSX.Element;")
        );
        assert!(!res.contains("useState"));
        assert!(!res.contains("useEffect"));
        assert!(!res.contains("className"));
    }

    #[test]
    fn test_tsx_slicing_throughput_and_token_reduction() {
        let mut source = String::new();
        source.push_str("import React, { useState } from 'react';\n\n");
        source.push_str("export interface ItemProps { id: string; name: string; }\n\n");
        for i in 0..150 {
            source.push_str(&format!(
                "export function Component{i}(props: ItemProps) {{\n    const [val, setVal] = useState({i});\n    const [loading, setLoading] = useState(false);\n    useEffect(() => {{\n        console.log(\"Mounting component {i}\");\n    }}, []);\n    return (\n        <div className=\"flex flex-col p-4 bg-gray-900 border border-gray-800 rounded-xl shadow-lg\">\n            <h1 className=\"text-xl font-bold text-white\">Component {i}</h1>\n            <p className=\"text-sm text-gray-400\">Current value: {{val}}</p>\n            <button className=\"px-4 py-2 bg-blue-600 text-white rounded hover:bg-blue-700\" onClick={{() => setVal(val + 1)}}>\n                Increment\n            </button>\n        </div>\n    );\n}}\n\n"
            ));
        }

        let total_lines = source.lines().count();
        assert!(total_lines >= 600);

        let start = std::time::Instant::now();
        let skeleton = extract_ts_skeleton(&source, true).unwrap();
        let elapsed = start.elapsed();

        let raw_len = source.len();
        let skeleton_len = skeleton.len();
        let reduction = (raw_len - skeleton_len) as f64 / raw_len as f64 * 100.0;

        assert!(
            reduction > 75.0,
            "Expected > 75% reduction, got {:.2}%",
            reduction
        );
        // Ensure parsing is sub-millisecond in release, and well within 15ms in debug
        assert!(
            elapsed.as_millis() < 50,
            "Parsing took too long: {:?}",
            elapsed
        );
    }
}
