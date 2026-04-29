//! Codex vault export — serialize design tree nodes as TOML-frontmatter markdown.
//!
//! Produces `.md` files with `+++...+++` TOML frontmatter suitable for ingestion
//! by a Codex vault, followed by the standard design document body sections.

use std::collections::HashMap;
use std::fmt::Write;
use std::fs;
use std::path::Path;

use super::types::*;

/// Convert a `NodeStatus` variant to a Codex-style tag string.
pub fn node_status_to_tag(status: &NodeStatus) -> String {
    format!("status:{}", status.as_str())
}

/// Render a `DesignNode` + its `DocumentSections` as a TOML-frontmatter markdown
/// document in the format Codex expects (`+++...+++`).
pub fn export_node_to_codex_markdown(node: &DesignNode, sections: &DocumentSections) -> String {
    let mut out = String::new();

    // ── TOML frontmatter ────────────────────────────────────────────────
    writeln!(out, "+++").unwrap();

    // Top-level keys
    writeln!(out, "id = \"{}\"", escape_toml(&node.id)).unwrap();
    writeln!(out, "kind = \"design_node\"").unwrap();

    // Build tags array: status tag + issue type tag + user-defined tags
    let mut tags: Vec<String> = vec![node_status_to_tag(&node.status)];
    if let Some(ref it) = node.issue_type {
        let label = match it {
            IssueType::Epic => "epic",
            IssueType::Feature => "feature",
            IssueType::Task => "task",
            IssueType::Bug => "bug",
            IssueType::Chore => "chore",
        };
        tags.push(format!("issue:{label}"));
    }
    for t in &node.tags {
        let prefixed = t.clone();
        if !tags.contains(&prefixed) {
            tags.push(prefixed);
        }
    }
    let tag_literals: Vec<String> = tags
        .iter()
        .map(|t| format!("\"{}\"", escape_toml(t)))
        .collect();
    writeln!(out, "tags = [{}]", tag_literals.join(", ")).unwrap();

    // [data] table
    writeln!(out).unwrap();
    writeln!(out, "[data]").unwrap();
    writeln!(out, "title = \"{}\"", escape_toml(&node.title)).unwrap();
    writeln!(out, "status = \"{}\"", node.status.as_str()).unwrap();

    if let Some(ref parent) = node.parent {
        writeln!(out, "parent = \"{}\"", escape_toml(parent)).unwrap();
    }

    if let Some(ref it) = node.issue_type {
        let label = match it {
            IssueType::Epic => "epic",
            IssueType::Feature => "feature",
            IssueType::Task => "task",
            IssueType::Bug => "bug",
            IssueType::Chore => "chore",
        };
        writeln!(out, "issue_type = \"{label}\"").unwrap();
    }

    if let Some(priority) = node.priority {
        writeln!(out, "priority = {priority}").unwrap();
    }

    // Arrays in [data]
    write_toml_string_array(&mut out, "dependencies", &node.dependencies);
    write_toml_string_array(&mut out, "open_questions", &node.open_questions);
    write_toml_string_array(&mut out, "related", &node.related);

    writeln!(out, "+++").unwrap();

    // ── Markdown body ───────────────────────────────────────────────────
    writeln!(out).unwrap();
    writeln!(out, "# {}", node.title).unwrap();

    if !sections.overview.is_empty() {
        writeln!(out).unwrap();
        writeln!(out, "## Overview").unwrap();
        writeln!(out).unwrap();
        writeln!(out, "{}", sections.overview.trim_end()).unwrap();
    }

    if !sections.research.is_empty() {
        writeln!(out).unwrap();
        writeln!(out, "## Research").unwrap();
        for entry in &sections.research {
            writeln!(out).unwrap();
            writeln!(out, "### {}", entry.heading).unwrap();
            writeln!(out).unwrap();
            writeln!(out, "{}", entry.content.trim_end()).unwrap();
        }
    }

    if !sections.decisions.is_empty() {
        writeln!(out).unwrap();
        writeln!(out, "## Decisions").unwrap();
        for dec in &sections.decisions {
            writeln!(out).unwrap();
            writeln!(out, "### {}", dec.title).unwrap();
            writeln!(out).unwrap();
            writeln!(out, "**Status:** {}", dec.status).unwrap();
            writeln!(out).unwrap();
            writeln!(out, "**Rationale:** {}", dec.rationale).unwrap();
        }
    }

    if !sections.open_questions.is_empty() {
        writeln!(out).unwrap();
        writeln!(out, "## Open Questions").unwrap();
        writeln!(out).unwrap();
        for q in &sections.open_questions {
            writeln!(out, "- {q}").unwrap();
        }
    }

    if !sections.impl_file_scope.is_empty() || !sections.impl_constraints.is_empty() {
        writeln!(out).unwrap();
        writeln!(out, "## Implementation Notes").unwrap();

        if !sections.impl_file_scope.is_empty() {
            writeln!(out).unwrap();
            writeln!(out, "### File Scope").unwrap();
            writeln!(out).unwrap();
            for fs_entry in &sections.impl_file_scope {
                let action = fs_entry
                    .action
                    .as_deref()
                    .map(|a| format!(" ({a})"))
                    .unwrap_or_default();
                writeln!(
                    out,
                    "- `{}` — {}{}",
                    fs_entry.path, fs_entry.description, action
                )
                .unwrap();
            }
        }

        if !sections.impl_constraints.is_empty() {
            writeln!(out).unwrap();
            writeln!(out, "### Constraints").unwrap();
            writeln!(out).unwrap();
            for c in &sections.impl_constraints {
                writeln!(out, "- {c}").unwrap();
            }
        }
    }

    out
}

/// Export an entire design tree to a Codex vault directory.
///
/// Creates `{vault_path}/design/` if needed and writes one `.md` file per node.
/// Returns the number of files written.
pub fn export_design_tree_to_vault(
    vault_path: &Path,
    nodes: &[DesignNode],
    sections_cache: &HashMap<String, DocumentSections>,
) -> anyhow::Result<usize> {
    let design_dir = vault_path.join("design");
    fs::create_dir_all(&design_dir)?;

    let empty_sections = DocumentSections::default();
    let mut count = 0;

    for node in nodes {
        let safe_id = sanitize_node_id_for_path(&node.id)?;
        let sections = sections_cache.get(&node.id).unwrap_or(&empty_sections);
        let content = export_node_to_codex_markdown(node, sections);
        let file_path = design_dir.join(format!("{}.md", safe_id));
        fs::write(&file_path, &content)?;
        count += 1;
    }

    Ok(count)
}

// ─── helpers ────────────────────────────────────────────────────────────────

/// Validate that a node ID is safe to use as a filesystem path component.
fn sanitize_node_id_for_path(id: &str) -> anyhow::Result<String> {
    if id.contains('/') || id.contains('\\') || id.contains("..") || id.is_empty() {
        anyhow::bail!("Invalid node ID for filesystem: {:?}", id);
    }
    Ok(id.to_string())
}

/// Escape a string for use inside a TOML quoted string.
fn escape_toml(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Write a TOML array of strings on a single line, or multiline if non-empty.
fn write_toml_string_array(out: &mut String, key: &str, items: &[String]) {
    if items.is_empty() {
        writeln!(out, "{key} = []").unwrap();
    } else {
        let literals: Vec<String> = items
            .iter()
            .map(|s| format!("\"{}\"", escape_toml(s)))
            .collect();
        writeln!(out, "{key} = [{}]", literals.join(", ")).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample_node() -> DesignNode {
        DesignNode {
            id: "abc-123".into(),
            title: "Sample Node".into(),
            status: NodeStatus::Exploring,
            parent: Some("parent-1".into()),
            tags: vec!["rust".into(), "lifecycle".into()],
            dependencies: vec!["dep-1".into(), "dep-2".into()],
            related: vec!["related-1".into()],
            open_questions: vec!["Question 1?".into(), "[assumption] Assumed fact".into()],
            branches: vec![],
            openspec_change: None,
            issue_type: Some(IssueType::Feature),
            priority: Some(2),
            archive_reason: None,
            superseded_by: None,
            archived_at: None,
            file_path: PathBuf::from("docs/abc-123.md"),
        }
    }

    fn sample_sections() -> DocumentSections {
        DocumentSections {
            overview: "This is the overview.".into(),
            research: vec![ResearchEntry {
                heading: "Topic A".into(),
                content: "Research content.".into(),
            }],
            decisions: vec![DesignDecision {
                title: "Use approach X".into(),
                status: "decided".into(),
                rationale: "It is simpler.".into(),
            }],
            open_questions: vec!["Question 1?".into(), "[assumption] Assumed fact".into()],
            impl_file_scope: vec![FileScope {
                path: "src/foo.rs".into(),
                description: "Main impl".into(),
                action: Some("new".into()),
            }],
            impl_constraints: vec!["Must handle UTF-8".into()],
        }
    }

    #[test]
    fn export_produces_valid_toml_frontmatter() {
        let md = export_node_to_codex_markdown(&sample_node(), &sample_sections());

        // Must start and end frontmatter with +++
        assert!(md.starts_with("+++\n"), "should start with +++");
        let second_delim = md[4..].find("+++").expect("should have closing +++");
        let frontmatter = &md[4..4 + second_delim];

        // Parse as TOML — must not error
        let table: toml::Value =
            toml::from_str(frontmatter).expect("frontmatter should be valid TOML");

        assert_eq!(table["id"].as_str().unwrap(), "abc-123");
        assert_eq!(table["kind"].as_str().unwrap(), "design_node");

        let tags = table["tags"].as_array().unwrap();
        assert!(tags.iter().any(|t| t.as_str() == Some("status:exploring")));
        assert!(tags.iter().any(|t| t.as_str() == Some("issue:feature")));

        let data = &table["data"];
        assert_eq!(data["title"].as_str().unwrap(), "Sample Node");
        assert_eq!(data["status"].as_str().unwrap(), "exploring");
        assert_eq!(data["parent"].as_str().unwrap(), "parent-1");
        assert_eq!(data["issue_type"].as_str().unwrap(), "feature");
        assert_eq!(data["priority"].as_integer().unwrap(), 2);
        assert_eq!(data["dependencies"].as_array().unwrap().len(), 2);
        assert_eq!(data["open_questions"].as_array().unwrap().len(), 2);
        assert_eq!(data["related"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn export_contains_body_sections() {
        let md = export_node_to_codex_markdown(&sample_node(), &sample_sections());

        assert!(md.contains("# Sample Node"));
        assert!(md.contains("## Overview"));
        assert!(md.contains("This is the overview."));
        assert!(md.contains("## Research"));
        assert!(md.contains("### Topic A"));
        assert!(md.contains("## Decisions"));
        assert!(md.contains("### Use approach X"));
        assert!(md.contains("## Open Questions"));
        assert!(md.contains("- Question 1?"));
        assert!(md.contains("## Implementation Notes"));
        assert!(md.contains("### File Scope"));
        assert!(md.contains("`src/foo.rs`"));
        assert!(md.contains("### Constraints"));
    }

    #[test]
    fn node_status_to_tag_covers_all_variants() {
        let variants = [
            (NodeStatus::Seed, "status:seed"),
            (NodeStatus::Exploring, "status:exploring"),
            (NodeStatus::Resolved, "status:resolved"),
            (NodeStatus::Decided, "status:decided"),
            (NodeStatus::Implementing, "status:implementing"),
            (NodeStatus::Implemented, "status:implemented"),
            (NodeStatus::Blocked, "status:blocked"),
            (NodeStatus::Deferred, "status:deferred"),
            (NodeStatus::Archived, "status:archived"),
        ];
        for (status, expected) in &variants {
            assert_eq!(node_status_to_tag(status), *expected);
        }
    }

    #[test]
    fn frontmatter_roundtrips_through_toml() {
        let md = export_node_to_codex_markdown(&sample_node(), &sample_sections());

        let frontmatter = &md[4..4 + md[4..].find("+++").unwrap()];
        let parsed: toml::Value = toml::from_str(frontmatter).unwrap();

        // Re-serialize and re-parse to verify round-trip stability
        let reserialized = toml::to_string(&parsed).unwrap();
        let reparsed: toml::Value = toml::from_str(&reserialized).unwrap();
        assert_eq!(parsed, reparsed, "TOML should round-trip cleanly");
    }

    #[test]
    fn export_node_without_optional_fields() {
        let node = DesignNode {
            id: "minimal".into(),
            title: "Minimal".into(),
            status: NodeStatus::Seed,
            parent: None,
            tags: vec![],
            dependencies: vec![],
            related: vec![],
            open_questions: vec![],
            branches: vec![],
            openspec_change: None,
            issue_type: None,
            priority: None,
            archive_reason: None,
            superseded_by: None,
            archived_at: None,
            file_path: PathBuf::from("docs/minimal.md"),
        };
        let sections = DocumentSections::default();
        let md = export_node_to_codex_markdown(&node, &sections);

        // Should still produce valid TOML
        let frontmatter = &md[4..4 + md[4..].find("+++").unwrap()];
        let table: toml::Value = toml::from_str(frontmatter).unwrap();
        assert_eq!(table["id"].as_str().unwrap(), "minimal");
        assert!(table.get("data").unwrap().get("parent").is_none());
        assert!(table.get("data").unwrap().get("issue_type").is_none());
        assert!(table.get("data").unwrap().get("priority").is_none());
    }

    #[test]
    fn export_tree_to_vault_writes_files() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path();

        let nodes = vec![sample_node()];
        let mut cache = HashMap::new();
        cache.insert("abc-123".into(), sample_sections());

        let count = export_design_tree_to_vault(vault, &nodes, &cache).unwrap();
        assert_eq!(count, 1);

        let written = vault.join("design/abc-123.md");
        assert!(written.exists(), "file should be created");

        let content = fs::read_to_string(&written).unwrap();
        assert!(content.starts_with("+++\n"));
        assert!(content.contains("Sample Node"));
    }

    #[test]
    fn escape_toml_handles_newlines() {
        let escaped = escape_toml("line1\nline2\r\nline3\ttab");
        assert_eq!(escaped, "line1\\nline2\\r\\nline3\\ttab");
        assert!(!escaped.contains('\n'));
        assert!(!escaped.contains('\r'));
        assert!(!escaped.contains('\t'));
    }

    #[test]
    fn sanitize_node_id_rejects_traversal() {
        assert!(sanitize_node_id_for_path("../etc/passwd").is_err());
        assert!(sanitize_node_id_for_path("foo/bar").is_err());
        assert!(sanitize_node_id_for_path("foo\\bar").is_err());
        assert!(sanitize_node_id_for_path("foo..bar").is_err());
        assert!(sanitize_node_id_for_path("").is_err());
    }

    #[test]
    fn sanitize_node_id_accepts_valid() {
        assert!(sanitize_node_id_for_path("abc-123").is_ok());
        assert!(sanitize_node_id_for_path("my_node").is_ok());
        assert!(sanitize_node_id_for_path("node.v2").is_ok());
    }

    #[test]
    fn special_chars_escaped_in_toml() {
        let node = DesignNode {
            id: "esc-test".into(),
            title: r#"Node with "quotes" and \ backslash"#.into(),
            status: NodeStatus::Exploring,
            parent: None,
            tags: vec![],
            dependencies: vec![],
            related: vec![],
            open_questions: vec![r#"Why "quoted"?"#.into()],
            branches: vec![],
            openspec_change: None,
            issue_type: None,
            priority: None,
            archive_reason: None,
            superseded_by: None,
            archived_at: None,
            file_path: PathBuf::from("docs/esc-test.md"),
        };
        let md = export_node_to_codex_markdown(&node, &DocumentSections::default());
        let frontmatter = &md[4..4 + md[4..].find("+++").unwrap()];
        let table: toml::Value =
            toml::from_str(frontmatter).expect("escaped frontmatter should parse");
        assert_eq!(
            table["data"]["title"].as_str().unwrap(),
            r#"Node with "quotes" and \ backslash"#
        );
    }
}
