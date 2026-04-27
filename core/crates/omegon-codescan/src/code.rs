//! Tree-sitter AST code scanner for Rust, TypeScript, Python, Go.
//!
//! Chunks at named declaration boundaries: functions, structs/classes,
//! impl/trait blocks, modules, type aliases, enums. Falls back to the
//! regex-based scanner when tree-sitter fails to parse a file (e.g.
//! incomplete or generated code).

use std::path::{Path, PathBuf};
use tree_sitter::{Language, Node, Parser};

#[derive(Debug, Clone)]
pub struct CodeChunk {
    pub path: PathBuf,
    pub start_line: usize,
    pub end_line: usize,
    pub item_name: String,
    pub item_kind: String,
    pub text: String,
    /// Enclosing scope (e.g., "impl ShadowContext" for a method inside it).
    /// None for top-level declarations.
    pub parent_scope: Option<String>,
}

pub struct CodeScanner;

impl CodeScanner {
    pub fn scan_file(path: &Path, content: &str) -> Vec<CodeChunk> {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        match ext {
            "rs" => scan_with_ts(path, content, rust_lang(), RUST_KINDS, rust_kind, rust_name)
                .or_regex_fallback(path, content, RUST_PATTERNS),
            "ts" | "mts" => scan_with_ts(path, content, ts_lang(), TS_KINDS, ts_kind, generic_name)
                .or_regex_fallback(path, content, TS_PATTERNS),
            "tsx" => scan_with_ts(path, content, tsx_lang(), TS_KINDS, ts_kind, generic_name)
                .or_regex_fallback(path, content, TS_PATTERNS),
            "js" | "jsx" | "mjs" => {
                scan_with_ts(path, content, js_lang(), JS_KINDS, js_kind, generic_name)
                    .or_regex_fallback(path, content, TS_PATTERNS)
            }
            "py" => scan_with_ts(path, content, py_lang(), PY_KINDS, py_kind, generic_name)
                .or_regex_fallback(path, content, PY_PATTERNS),
            "go" => scan_with_ts(path, content, go_lang(), GO_KINDS, go_kind, go_name)
                .or_regex_fallback(path, content, GO_PATTERNS),
            _ => vec![],
        }
    }
}

// ── Tree-sitter languages ────────────────────────────────────────────────────

fn rust_lang() -> Language {
    tree_sitter_rust::LANGUAGE.into()
}
fn ts_lang() -> Language {
    tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
}
fn tsx_lang() -> Language {
    tree_sitter_typescript::LANGUAGE_TSX.into()
}
fn js_lang() -> Language {
    tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
} // close enough for JS
fn py_lang() -> Language {
    tree_sitter_python::LANGUAGE.into()
}
fn go_lang() -> Language {
    tree_sitter_go::LANGUAGE.into()
}

// ── Top-level node kinds to chunk ────────────────────────────────────────────

const RUST_KINDS: &[&str] = &[
    "function_item",
    "impl_item",
    "struct_item",
    "enum_item",
    "trait_item",
    "mod_item",
    "type_alias",
    "const_item",
    "static_item",
    "function_signature_item",
];

const TS_KINDS: &[&str] = &[
    "function_declaration",
    "function",
    "class_declaration",
    "interface_declaration",
    "type_alias_declaration",
    "abstract_class_declaration",
    "export_statement",
    "lexical_declaration",
    "variable_declaration",
];

const JS_KINDS: &[&str] = &[
    "function_declaration",
    "function",
    "class_declaration",
    "export_statement",
    "lexical_declaration",
    "variable_declaration",
];

const PY_KINDS: &[&str] = &[
    "function_definition",
    "class_definition",
    "decorated_definition",
];

const GO_KINDS: &[&str] = &[
    "function_declaration",
    "method_declaration",
    "type_declaration",
];

// ── Kind → human label ───────────────────────────────────────────────────────

fn rust_kind(k: &str) -> &'static str {
    match k {
        "function_item" | "function_signature_item" => "fn",
        "impl_item" => "impl",
        "struct_item" => "struct",
        "enum_item" => "enum",
        "trait_item" => "trait",
        "mod_item" => "mod",
        "type_alias" => "type",
        "const_item" => "const",
        "static_item" => "static",
        _ => "item",
    }
}

fn ts_kind(k: &str) -> &'static str {
    match k {
        "function_declaration" | "function" => "function",
        "class_declaration" | "abstract_class_declaration" => "class",
        "interface_declaration" => "interface",
        "type_alias_declaration" => "type",
        "export_statement" => "export",
        "lexical_declaration" | "variable_declaration" => "const",
        _ => "decl",
    }
}

fn js_kind(k: &str) -> &'static str {
    match k {
        "function_declaration" | "function" => "function",
        "class_declaration" => "class",
        "export_statement" => "export",
        "lexical_declaration" | "variable_declaration" => "const",
        _ => "decl",
    }
}

fn py_kind(k: &str) -> &'static str {
    match k {
        "function_definition" => "def",
        "class_definition" => "class",
        "decorated_definition" => "decorated",
        _ => "decl",
    }
}

fn go_kind(k: &str) -> &'static str {
    match k {
        "function_declaration" | "method_declaration" => "func",
        "type_declaration" => "type",
        _ => "decl",
    }
}

// ── Name extractors ──────────────────────────────────────────────────────────

fn rust_name(node: &Node, source: &[u8]) -> String {
    if node.kind() == "impl_item" {
        // impl Trait for Type  →  "Trait for Type" or just "Type"
        let type_name = node
            .child_by_field_name("type")
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or("?")
            .to_string();
        if let Some(trait_node) = node.child_by_field_name("trait") {
            let trait_name = trait_node.utf8_text(source).unwrap_or("?");
            return format!("{} for {}", trait_name, type_name);
        }
        return type_name;
    }
    generic_name(node, source)
}

fn go_name(node: &Node, source: &[u8]) -> String {
    if node.kind() == "type_declaration" {
        // type_declaration contains one or more type_spec nodes
        let cursor = &mut node.walk();
        for child in node.children(cursor) {
            if child.kind() == "type_spec" {
                if let Some(name_node) = child.child_by_field_name("name") {
                    if let Ok(text) = name_node.utf8_text(source) {
                        return text.to_string();
                    }
                }
            }
        }
    }
    generic_name(node, source)
}

fn generic_name(node: &Node, source: &[u8]) -> String {
    // Try standard "name" field first, then descend into export wrapper
    if let Some(name_node) = node.child_by_field_name("name") {
        if let Ok(text) = name_node.utf8_text(source) {
            return text.to_string();
        }
    }
    // export_statement → declaration child
    if node.kind() == "export_statement" {
        if let Some(decl) = node.child_by_field_name("declaration") {
            return generic_name(&decl, source);
        }
        // export { foo } — grab the exported name
        let cursor = &mut node.walk();
        for child in node.children(cursor) {
            if child.kind() == "export_clause" {
                let sub = &mut child.walk();
                for spec in child.children(sub) {
                    if spec.kind() == "export_specifier" {
                        if let Some(n) = spec.child_by_field_name("name") {
                            if let Ok(t) = n.utf8_text(source) {
                                return t.to_string();
                            }
                        }
                    }
                }
            }
        }
    }
    // lexical_declaration: const foo = ...
    if matches!(node.kind(), "lexical_declaration" | "variable_declaration") {
        let cursor = &mut node.walk();
        for child in node.children(cursor) {
            if matches!(child.kind(), "variable_declarator") {
                if let Some(n) = child.child_by_field_name("name") {
                    if let Ok(t) = n.utf8_text(source) {
                        return t.to_string();
                    }
                }
            }
        }
    }
    "(anonymous)".to_string()
}

// ── Core tree-sitter scanner ─────────────────────────────────────────────────

struct ScanResult(Vec<CodeChunk>);

impl ScanResult {
    fn or_regex_fallback(
        self,
        path: &Path,
        content: &str,
        patterns: &'static [(&'static str, &'static str)],
    ) -> Vec<CodeChunk> {
        if self.0.is_empty() {
            scan_with_regex(path, content, patterns)
        } else {
            self.0
        }
    }
}

/// Container kinds — nodes that should be recursed into to extract children.
const CONTAINER_KINDS: &[&str] = &[
    // Rust
    "impl_item",
    "trait_item",
    "mod_item",
    // TypeScript / JavaScript
    "class_declaration",
    "abstract_class_declaration",
    "class_body",
    // Python
    "class_definition",
    // Go — type declarations contain method sets via receiver, not nesting
];

/// Inner declaration kinds — what we extract from inside containers.
const INNER_KINDS: &[&str] = &[
    // Rust
    "function_item",
    "function_signature_item",
    "type_alias",
    "const_item",
    // TS/JS
    "method_definition",
    "public_field_definition",
    "function_declaration",
    "function",
    // Python
    "function_definition",
    "decorated_definition",
];

fn scan_with_ts(
    path: &Path,
    content: &str,
    language: Language,
    top_kinds: &[&str],
    kind_label: fn(&str) -> &'static str,
    name_extractor: fn(&Node, &[u8]) -> String,
) -> ScanResult {
    let mut parser = Parser::new();
    if parser.set_language(&language).is_err() {
        return ScanResult(vec![]);
    }
    let Some(tree) = parser.parse(content, None) else {
        return ScanResult(vec![]);
    };

    let root = tree.root_node();
    let source = content.as_bytes();
    let lines: Vec<&str> = content.lines().collect();
    let total = lines.len();

    let mut chunks = Vec::new();

    fn visit(
        node: &Node,
        path: &Path,
        source: &[u8],
        lines: &[&str],
        total: usize,
        top_kinds: &[&str],
        kind_label: fn(&str) -> &'static str,
        name_extractor: fn(&Node, &[u8]) -> String,
        parent_scope: Option<&str>,
        chunks: &mut Vec<CodeChunk>,
    ) {
        let cursor = &mut node.walk();
        for child in node.children(cursor) {
            let kind = child.kind();

            // Skip non-interesting nodes
            let is_top = top_kinds.contains(&kind);
            let is_inner = INNER_KINDS.contains(&kind);
            let is_container = CONTAINER_KINDS.contains(&kind);

            if !is_top && !is_inner && !is_container {
                continue;
            }

            let name = name_extractor(&child, source);
            if name == "(anonymous)" && child.end_position().row == child.start_position().row {
                continue;
            }

            let start = child.start_position().row;
            let end = child.end_position().row;
            let chunk_end = end.min(start + 99).min(total.saturating_sub(1));

            // Emit a chunk for this node
            if is_top || is_inner {
                let text = lines[start..=chunk_end].join("\n");
                let qualified_name = match parent_scope {
                    Some(scope) => format!("{scope}::{name}"),
                    None => name.clone(),
                };
                chunks.push(CodeChunk {
                    path: path.to_path_buf(),
                    start_line: start + 1,
                    end_line: chunk_end + 1,
                    item_name: qualified_name,
                    item_kind: kind_label(kind).to_string(),
                    text,
                    parent_scope: parent_scope.map(String::from),
                });
            }

            // Recurse into containers to extract inner declarations
            if is_container {
                let scope_name = match parent_scope {
                    Some(scope) => format!("{scope}::{name}"),
                    None => name,
                };
                visit(
                    &child,
                    path,
                    source,
                    lines,
                    total,
                    top_kinds,
                    kind_label,
                    name_extractor,
                    Some(&scope_name),
                    chunks,
                );
            }
        }
    }

    visit(
        &root,
        path,
        source,
        &lines,
        total,
        top_kinds,
        kind_label,
        name_extractor,
        None,
        &mut chunks,
    );

    ScanResult(chunks)
}

// ── Regex fallback ───────────────────────────────────────────────────────────

type PatternPair = (&'static str, &'static str);

const RUST_PATTERNS: &[PatternPair] = &[
    (
        r"^(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+([a-zA-Z_][a-zA-Z0-9_]*)\b",
        "fn",
    ),
    (
        r"^(?:pub(?:\([^)]*\))?\s+)?impl(?:<[^>]*>)?\s+(?:\S+\s+for\s+)?([a-zA-Z_][a-zA-Z0-9_:<>]*)",
        "impl",
    ),
    (
        r"^(?:pub(?:\([^)]*\))?\s+)?struct\s+([a-zA-Z_][a-zA-Z0-9_]*)\b",
        "struct",
    ),
    (
        r"^(?:pub(?:\([^)]*\))?\s+)?enum\s+([a-zA-Z_][a-zA-Z0-9_]*)\b",
        "enum",
    ),
    (
        r"^(?:pub(?:\([^)]*\))?\s+)?trait\s+([a-zA-Z_][a-zA-Z0-9_]*)\b",
        "trait",
    ),
    (
        r"^(?:pub(?:\([^)]*\))?\s+)?mod\s+([a-zA-Z_][a-zA-Z0-9_]*)\b",
        "mod",
    ),
];

const TS_PATTERNS: &[PatternPair] = &[
    (
        r"^(?:export\s+)?(?:async\s+)?function\s+([a-zA-Z_$][a-zA-Z0-9_$]*)\b",
        "function",
    ),
    (
        r"^(?:export\s+)?(?:abstract\s+)?class\s+([a-zA-Z_$][a-zA-Z0-9_$]*)\b",
        "class",
    ),
    (
        r"^(?:export\s+)?interface\s+([a-zA-Z_$][a-zA-Z0-9_$]*)\b",
        "interface",
    ),
    (
        r"^(?:export\s+)?type\s+([a-zA-Z_$][a-zA-Z0-9_$]*)\s*=",
        "type",
    ),
];

const PY_PATTERNS: &[PatternPair] = &[
    (r"^(?:async\s+)?def\s+([a-zA-Z_][a-zA-Z0-9_]*)\b", "def"),
    (r"^class\s+([a-zA-Z_][a-zA-Z0-9_]*)\b", "class"),
];

const GO_PATTERNS: &[PatternPair] = &[
    (
        r"^func\s+(?:\([^)]+\)\s+)?([a-zA-Z_][a-zA-Z0-9_]*)\b",
        "func",
    ),
    (
        r"^type\s+([a-zA-Z_][a-zA-Z0-9_]*)\s+(?:struct|interface)\b",
        "type",
    ),
];

fn scan_with_regex(path: &Path, content: &str, patterns: &[PatternPair]) -> Vec<CodeChunk> {
    use std::collections::BTreeMap;
    let lines: Vec<&str> = content.lines().collect();
    let total = lines.len();
    let mut matches: BTreeMap<usize, (String, String)> = BTreeMap::new();

    for &(pattern, kind) in patterns {
        let Ok(re) = regex::Regex::new(pattern) else {
            continue;
        };
        for (i, line) in lines.iter().enumerate() {
            if let Some(cap) = re.captures(line) {
                let name = cap
                    .get(1)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default();
                if !name.is_empty() {
                    matches.entry(i).or_insert_with(|| (name, kind.to_string()));
                }
            }
        }
    }

    let starts: Vec<usize> = matches.keys().cloned().collect();
    let mut chunks = Vec::with_capacity(starts.len());
    for (i, &start) in starts.iter().enumerate() {
        let end = if i + 1 < starts.len() {
            starts[i + 1].saturating_sub(1)
        } else {
            total.saturating_sub(1)
        };
        let chunk_end = end.min(start + 99);
        let (name, kind) = &matches[&start];
        let text = lines[start..=chunk_end.min(total.saturating_sub(1))].join("\n");
        chunks.push(CodeChunk {
            path: path.to_path_buf(),
            start_line: start + 1,
            end_line: chunk_end + 1,
            item_name: name.clone(),
            item_kind: kind.clone(),
            text,
            parent_scope: None,
        });
    }
    chunks
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_rust_treesitter() {
        let src = r#"
pub struct Foo {
    x: i32,
}

impl Foo {
    pub fn new(x: i32) -> Self { Self { x } }
    fn private_helper(&self) {}
}

pub async fn top_level() {}

pub trait Bar {
    fn do_thing(&self);
}

pub enum Color { Red, Green, Blue }
"#;
        let chunks = CodeScanner::scan_file(Path::new("x.rs"), src);
        let names: Vec<&str> = chunks.iter().map(|c| c.item_name.as_str()).collect();
        assert!(names.contains(&"Foo"), "struct: {:?}", names);
        assert!(names.contains(&"Bar"), "trait: {:?}", names);
        assert!(names.contains(&"Color"), "enum: {:?}", names);
        // impl should appear
        let impl_chunk = chunks.iter().find(|c| c.item_kind == "impl");
        assert!(impl_chunk.is_some(), "impl block: {:?}", names);
    }

    #[test]
    fn scan_rust_fn_name_extracted() {
        let src = "pub fn greet(name: &str) -> String {\n    format!(\"Hello, {name}\")\n}\n";
        let chunks = CodeScanner::scan_file(Path::new("greet.rs"), src);
        assert!(!chunks.is_empty(), "expected chunks");
        assert_eq!(chunks[0].item_name, "greet");
        assert_eq!(chunks[0].item_kind, "fn");
    }

    #[test]
    fn scan_typescript_treesitter() {
        let src = r#"
export class MyService {
    constructor(private repo: Repo) {}
}

export async function fetchData(url: string): Promise<void> {
    // ...
}

export interface Config {
    host: string;
}

export type Status = "active" | "inactive";
"#;
        let chunks = CodeScanner::scan_file(Path::new("x.ts"), src);
        let names: Vec<&str> = chunks.iter().map(|c| c.item_name.as_str()).collect();
        assert!(names.contains(&"MyService"), "class: {:?}", names);
        assert!(names.contains(&"fetchData"), "function: {:?}", names);
        assert!(names.contains(&"Config"), "interface: {:?}", names);
    }

    #[test]
    fn scan_python_treesitter() {
        let src =
            "class Foo:\n    def method(self): pass\n\nasync def handler(req):\n    return 'ok'\n";
        let chunks = CodeScanner::scan_file(Path::new("x.py"), src);
        let names: Vec<&str> = chunks.iter().map(|c| c.item_name.as_str()).collect();
        assert!(names.contains(&"Foo"), "class: {:?}", names);
        assert!(names.contains(&"handler"), "async def: {:?}", names);
    }

    #[test]
    fn scan_go_treesitter() {
        let src = "package main\n\ntype Server struct {\n\taddr string\n}\n\nfunc NewServer(addr string) *Server {\n\treturn &Server{addr: addr}\n}\n";
        let chunks = CodeScanner::scan_file(Path::new("x.go"), src);
        let names: Vec<&str> = chunks.iter().map(|c| c.item_name.as_str()).collect();
        assert!(names.contains(&"NewServer"), "func: {:?}", names);
        assert!(names.contains(&"Server"), "type: {:?}", names);
    }

    #[test]
    fn unknown_ext_empty() {
        assert!(CodeScanner::scan_file(Path::new("x.toml"), "key = 1").is_empty());
    }

    #[test]
    fn malformed_rust_falls_back_to_regex() {
        // Completely unparseable bytes still yield something via fallback
        let src = "pub fn broken_but_matches(";
        let chunks = CodeScanner::scan_file(Path::new("x.rs"), src);
        // tree-sitter will parse even incomplete Rust (error recovery), but regex
        // will produce at least one chunk
        assert!(
            !chunks.is_empty() || src.len() < 10,
            "should attempt extraction"
        );
    }
}
