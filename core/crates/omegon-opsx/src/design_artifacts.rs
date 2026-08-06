//! Canonical design-node Markdown artifact model and codec.
//!
//! Agent-authored documents normalize to one deterministic YAML + Markdown
//! representation. Readers accept the legacy YAML/TOML forms, but rewriting is
//! blocked when content outside the owned schema would be discarded.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::PathBuf;

use crate::{NodeState, OpsxError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontmatterFormat {
    Yaml,
    Toml,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewriteSafety {
    Canonical,
    Normalizable,
    BlockedByUnknownContent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesignDiagnosticKind {
    UnknownFrontmatterField,
    UnknownSection,
    DuplicateSection,
    MalformedField,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesignDiagnostic {
    pub kind: DesignDiagnosticKind,
    pub location: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueType {
    Epic,
    Feature,
    Task,
    Bug,
    Chore,
}

impl IssueType {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "epic" => Some(Self::Epic),
            "feature" => Some(Self::Feature),
            "task" => Some(Self::Task),
            "bug" => Some(Self::Bug),
            "chore" => Some(Self::Chore),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Epic => "epic",
            Self::Feature => "feature",
            Self::Task => "task",
            Self::Bug => "bug",
            Self::Chore => "chore",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesignNodeArtifact {
    pub id: String,
    pub title: String,
    pub state: NodeState,
    pub parent: Option<String>,
    pub tags: Vec<String>,
    pub dependencies: Vec<String>,
    pub related: Vec<String>,
    pub open_questions: Vec<String>,
    pub branches: Vec<String>,
    pub openspec_change: Option<String>,
    pub issue_type: Option<IssueType>,
    pub priority: Option<u8>,
    pub archive_reason: Option<String>,
    pub superseded_by: Option<String>,
    pub archived_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DesignSections {
    pub overview: String,
    pub research: Vec<ResearchEntry>,
    pub decisions: Vec<DesignDecision>,
    pub open_questions: Vec<String>,
    pub implementation: ImplementationNotes,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResearchEntry {
    pub heading: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesignDecision {
    pub title: String,
    pub status: String,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImplementationNotes {
    pub file_scope: Vec<FileScope>,
    pub constraints: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileScope {
    pub path: String,
    pub description: String,
    pub action: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedDesignArtifact {
    pub artifact: DesignNodeArtifact,
    pub sections: DesignSections,
    pub source_path: PathBuf,
    pub source_format: FrontmatterFormat,
    pub diagnostics: Vec<DesignDiagnostic>,
    pub rewrite_safety: RewriteSafety,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Value {
    Scalar(String),
    List(Vec<String>),
}

pub fn parse_design_artifact(
    source: &str,
    source_path: impl Into<PathBuf>,
) -> Result<ParsedDesignArtifact, OpsxError> {
    let source_path = source_path.into();
    let (format, frontmatter, body) = split_frontmatter(source)
        .ok_or_else(|| OpsxError::StoreError("design artifact has no frontmatter".into()))?;
    let values = parse_frontmatter(format, frontmatter);
    let mut diagnostics = Vec::new();
    let known = [
        "id",
        "title",
        "status",
        "parent",
        "tags",
        "dependencies",
        "related",
        "open_questions",
        "branches",
        "openspec_change",
        "issue_type",
        "priority",
        "archive_reason",
        "superseded_by",
        "archived_at",
    ];
    for key in values.keys().filter(|key| !known.contains(&key.as_str())) {
        diagnostics.push(DesignDiagnostic {
            kind: DesignDiagnosticKind::UnknownFrontmatterField,
            location: format!("frontmatter.{key}"),
            message: format!("unknown design frontmatter field '{key}'"),
        });
    }
    let required = |key: &str| -> Result<String, OpsxError> {
        scalar(&values, key)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| OpsxError::StoreError(format!("design artifact missing '{key}'")))
    };
    let state_text = scalar(&values, "status").unwrap_or_else(|| "seed".into());
    let state = NodeState::parse(&state_text).ok_or_else(|| {
        OpsxError::StoreError(format!("unknown design node status: {state_text}"))
    })?;
    let issue_type = scalar(&values, "issue_type")
        .map(|value| {
            IssueType::parse(&value)
                .ok_or_else(|| OpsxError::StoreError(format!("unknown design issue type: {value}")))
        })
        .transpose()?;
    let priority = scalar(&values, "priority")
        .map(|value| {
            value
                .parse::<u8>()
                .map_err(|_| OpsxError::StoreError(format!("invalid design priority: {value}")))
        })
        .transpose()?;
    let mut sections = parse_sections(body, &mut diagnostics);
    let frontmatter_questions = list(&values, "open_questions");
    if sections.open_questions.is_empty() {
        sections.open_questions.clone_from(&frontmatter_questions);
    } else if sections.open_questions != frontmatter_questions {
        diagnostics.push(DesignDiagnostic {
            kind: DesignDiagnosticKind::MalformedField,
            location: "open_questions".into(),
            message: "frontmatter and section open questions disagree".into(),
        });
    }
    let artifact = DesignNodeArtifact {
        id: required("id")?,
        title: required("title")?,
        state,
        parent: scalar(&values, "parent"),
        tags: list(&values, "tags"),
        dependencies: list(&values, "dependencies"),
        related: list(&values, "related"),
        open_questions: sections.open_questions.clone(),
        branches: list(&values, "branches"),
        openspec_change: scalar(&values, "openspec_change"),
        issue_type,
        priority,
        archive_reason: scalar(&values, "archive_reason"),
        superseded_by: scalar(&values, "superseded_by"),
        archived_at: scalar(&values, "archived_at"),
    };
    let blocked = diagnostics.iter().any(|diagnostic| {
        matches!(
            diagnostic.kind,
            DesignDiagnosticKind::UnknownFrontmatterField
                | DesignDiagnosticKind::UnknownSection
                | DesignDiagnosticKind::DuplicateSection
        )
    });
    let rewrite_safety = if blocked {
        RewriteSafety::BlockedByUnknownContent
    } else if format == FrontmatterFormat::Yaml
        && source == render_design_artifact(&artifact, &sections)
    {
        RewriteSafety::Canonical
    } else {
        RewriteSafety::Normalizable
    };
    Ok(ParsedDesignArtifact {
        artifact,
        sections,
        source_path,
        source_format: format,
        diagnostics,
        rewrite_safety,
    })
}

pub fn render_design_artifact(node: &DesignNodeArtifact, sections: &DesignSections) -> String {
    let mut out = String::new();
    writeln!(out, "---").unwrap();
    writeln!(out, "id: {}", yaml(&node.id)).unwrap();
    writeln!(out, "title: {}", yaml(&node.title)).unwrap();
    writeln!(out, "status: {}", node.state.as_str()).unwrap();
    optional_scalar(&mut out, "parent", node.parent.as_deref());
    yaml_list(&mut out, "tags", &node.tags);
    yaml_list(&mut out, "dependencies", &node.dependencies);
    yaml_list(&mut out, "related", &node.related);
    yaml_list(&mut out, "open_questions", &sections.open_questions);
    yaml_list(&mut out, "branches", &node.branches);
    optional_scalar(&mut out, "openspec_change", node.openspec_change.as_deref());
    if let Some(issue_type) = node.issue_type {
        writeln!(out, "issue_type: {}", issue_type.as_str()).unwrap();
    }
    if let Some(priority) = node.priority {
        writeln!(out, "priority: {priority}").unwrap();
    }
    optional_scalar(&mut out, "archive_reason", node.archive_reason.as_deref());
    optional_scalar(&mut out, "superseded_by", node.superseded_by.as_deref());
    optional_scalar(&mut out, "archived_at", node.archived_at.as_deref());
    writeln!(out, "---\n").unwrap();
    writeln!(out, "# {}", node.title).unwrap();
    if !sections.overview.is_empty() {
        write!(out, "\n## Overview\n\n{}\n", sections.overview.trim()).unwrap();
    }
    if !sections.research.is_empty() {
        writeln!(out, "\n## Research").unwrap();
        for entry in &sections.research {
            write!(out, "\n### {}\n\n{}\n", entry.heading, entry.content.trim()).unwrap();
        }
    }
    if !sections.decisions.is_empty() {
        writeln!(out, "\n## Decisions").unwrap();
        for decision in &sections.decisions {
            write!(
                out,
                "\n### {}\n\n**Status:** {}\n\n**Rationale:** {}\n",
                decision.title,
                decision.status,
                decision.rationale.trim()
            )
            .unwrap();
        }
    }
    if !sections.open_questions.is_empty() {
        writeln!(out, "\n## Open Questions\n").unwrap();
        for question in &sections.open_questions {
            writeln!(out, "- {question}").unwrap();
        }
    }
    if !sections.implementation.file_scope.is_empty()
        || !sections.implementation.constraints.is_empty()
    {
        writeln!(out, "\n## Implementation Notes").unwrap();
        if !sections.implementation.file_scope.is_empty() {
            writeln!(out, "\n### File Scope\n").unwrap();
            for scope in &sections.implementation.file_scope {
                let action = scope
                    .action
                    .as_ref()
                    .map(|value| format!(" ({value})"))
                    .unwrap_or_default();
                writeln!(out, "- `{}` — {}{}", scope.path, scope.description, action).unwrap();
            }
        }
        if !sections.implementation.constraints.is_empty() {
            writeln!(out, "\n### Constraints\n").unwrap();
            for constraint in &sections.implementation.constraints {
                writeln!(out, "- {constraint}").unwrap();
            }
        }
    }
    out
}

fn split_frontmatter(source: &str) -> Option<(FrontmatterFormat, &str, &str)> {
    if let Some(rest) = source.strip_prefix("---\n")
        && let Some(end) = rest.find("\n---")
    {
        return Some((
            FrontmatterFormat::Yaml,
            &rest[..end],
            rest[end + 4..].trim_start_matches('\n'),
        ));
    }
    if let Some(rest) = source.strip_prefix("+++\n")
        && let Some(end) = rest.find("\n+++")
    {
        return Some((
            FrontmatterFormat::Toml,
            &rest[..end],
            rest[end + 4..].trim_start_matches('\n'),
        ));
    }
    None
}

fn parse_frontmatter(format: FrontmatterFormat, input: &str) -> BTreeMap<String, Value> {
    let separator = if format == FrontmatterFormat::Yaml {
        ':'
    } else {
        '='
    };
    let mut result = BTreeMap::new();
    let mut section = None;
    let mut list_key: Option<String> = None;
    for raw in input.lines() {
        let line = raw.trim();
        if format == FrontmatterFormat::Toml && line.starts_with('[') && line.ends_with(']') {
            section = Some(line.trim_matches(['[', ']']).to_string());
            continue;
        }
        if format == FrontmatterFormat::Toml
            && section.as_deref().is_some_and(|name| name != "data")
        {
            continue;
        }
        if format == FrontmatterFormat::Yaml && line.starts_with("- ") {
            if let Some(key) = &list_key
                && let Some(Value::List(values)) = result.get_mut(key)
            {
                values.push(unquote(line.trim_start_matches("- ")));
            }
            continue;
        }
        let Some((key, raw_value)) = line.split_once(separator) else {
            continue;
        };
        let key = key.trim().to_string();
        let value = raw_value.trim();
        if value.is_empty() {
            result.insert(key.clone(), Value::List(Vec::new()));
            list_key = Some(key);
        } else if value.starts_with('[') && value.ends_with(']') {
            let values = value[1..value.len() - 1]
                .split(',')
                .map(|item| unquote(item.trim()))
                .filter(|item| !item.is_empty())
                .collect();
            result.insert(key, Value::List(values));
            list_key = None;
        } else {
            result.insert(key, Value::Scalar(unquote(value)));
            list_key = None;
        }
    }
    result
}

fn parse_sections(body: &str, diagnostics: &mut Vec<DesignDiagnostic>) -> DesignSections {
    let mut sections = DesignSections::default();
    let mut blocks = Vec::<(String, String)>::new();
    let mut heading = String::new();
    let mut content = String::new();
    for line in body.lines() {
        if let Some(next) = line.strip_prefix("## ") {
            if !heading.is_empty() || !content.trim().is_empty() {
                blocks.push((std::mem::take(&mut heading), std::mem::take(&mut content)));
            }
            heading = next.trim().to_string();
        } else if !(heading.is_empty() && content.trim().is_empty() && line.starts_with("# ")) {
            content.push_str(line);
            content.push('\n');
        }
    }
    if !heading.is_empty() || !content.trim().is_empty() {
        blocks.push((heading, content));
    }
    let mut seen = std::collections::BTreeSet::new();
    for (heading, content) in blocks {
        if !seen.insert(heading.clone()) && !heading.is_empty() {
            diagnostics.push(DesignDiagnostic {
                kind: DesignDiagnosticKind::DuplicateSection,
                location: heading.clone(),
                message: format!("duplicate design section '{heading}'"),
            });
        }
        match heading.as_str() {
            "" | "Overview" => sections.overview = content.trim().to_string(),
            "Research" => sections.research = parse_research(&content),
            "Decisions" => sections.decisions = parse_decisions(&content),
            "Open Questions" => sections.open_questions = bullets(&content),
            "Implementation Notes" => sections.implementation = parse_implementation(&content),
            unknown => diagnostics.push(DesignDiagnostic {
                kind: DesignDiagnosticKind::UnknownSection,
                location: unknown.into(),
                message: format!("unknown design section '{unknown}'"),
            }),
        }
    }
    sections
}

fn parse_research(input: &str) -> Vec<ResearchEntry> {
    parse_h3(input)
        .into_iter()
        .map(|(heading, content)| ResearchEntry {
            heading,
            content: content.trim().into(),
        })
        .collect()
}

fn parse_decisions(input: &str) -> Vec<DesignDecision> {
    parse_h3(input)
        .into_iter()
        .map(|(title, content)| {
            let status = content
                .lines()
                .find_map(|line| line.strip_prefix("**Status:**"))
                .unwrap_or("")
                .trim()
                .to_string();
            let rationale = content
                .split_once("**Rationale:**")
                .map(|(_, value)| value.trim().to_string())
                .unwrap_or_default();
            DesignDecision {
                title,
                status,
                rationale,
            }
        })
        .collect()
}

fn parse_h3(input: &str) -> Vec<(String, String)> {
    let mut result = Vec::new();
    let mut heading = None;
    let mut content = String::new();
    for line in input.lines() {
        if let Some(next) = line.strip_prefix("### ") {
            if let Some(previous) = heading.replace(next.trim().to_string()) {
                result.push((previous, std::mem::take(&mut content)));
            }
        } else if heading.is_some() {
            content.push_str(line);
            content.push('\n');
        }
    }
    if let Some(heading) = heading {
        result.push((heading, content));
    }
    result
}

fn parse_implementation(input: &str) -> ImplementationNotes {
    let blocks = parse_h3(input);
    let mut result = ImplementationNotes::default();
    for (heading, content) in blocks {
        if heading.eq_ignore_ascii_case("file scope") {
            for line in bullets(&content) {
                if let Some((path, description)) =
                    line.split_once(" — ").or_else(|| line.split_once(" - "))
                {
                    let description = description.trim();
                    let (description, action) = description
                        .strip_suffix(')')
                        .and_then(|value| value.rsplit_once(" ("))
                        .map(|(description, action)| {
                            (description.trim().into(), Some(action.into()))
                        })
                        .unwrap_or_else(|| (description.into(), None));
                    result.file_scope.push(FileScope {
                        path: path.trim().trim_matches('`').into(),
                        description,
                        action,
                    });
                }
            }
        } else if heading.eq_ignore_ascii_case("constraints") {
            result.constraints = bullets(&content);
        }
    }
    result
}

fn bullets(input: &str) -> Vec<String> {
    input
        .lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("- ")
                .or_else(|| line.trim().strip_prefix("* "))
                .map(str::to_string)
        })
        .collect()
}
fn scalar(values: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    match values.get(key) {
        Some(Value::Scalar(value)) => Some(value.clone()),
        _ => None,
    }
}
fn list(values: &BTreeMap<String, Value>, key: &str) -> Vec<String> {
    match values.get(key) {
        Some(Value::List(value)) => value.clone(),
        _ => Vec::new(),
    }
}
fn unquote(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .replace("\\\"", "\"")
        .replace("\\\\", "\\")
}
fn yaml(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}
fn optional_scalar(out: &mut String, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        writeln!(out, "{key}: {}", yaml(value)).unwrap();
    }
}
fn yaml_list(out: &mut String, key: &str, values: &[String]) {
    if values.is_empty() {
        writeln!(out, "{key}: []").unwrap();
    } else {
        writeln!(out, "{key}:").unwrap();
        for value in values {
            writeln!(out, "  - {}", yaml(value)).unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEGACY: &str = "+++\n[data]\nid = \"node-a\"\ntitle = \"Agent Authored\"\nstatus = \"decided\"\nopen_questions = []\n[publication]\nenabled = false\n+++\n# Agent Authored\n\n## Overview\n\nA deterministic artifact.\n\n## Decisions\n\n### Use typed mutations\n\n**Status:** decided\n\n**Rationale:** Prevent invalid writes.\n";

    #[test]
    fn parses_legacy_toml_as_normalizable() {
        let parsed = parse_design_artifact(LEGACY, "docs/node-a.md").unwrap();
        assert_eq!(parsed.artifact.state, NodeState::Decided);
        assert_eq!(parsed.sections.decisions.len(), 1);
        assert_eq!(parsed.rewrite_safety, RewriteSafety::Normalizable);
    }

    #[test]
    fn canonical_render_round_trips_idempotently() {
        let parsed = parse_design_artifact(LEGACY, "docs/node-a.md").unwrap();
        let canonical = render_design_artifact(&parsed.artifact, &parsed.sections);
        let reparsed = parse_design_artifact(&canonical, "docs/node-a.md").unwrap();
        assert_eq!(reparsed.rewrite_safety, RewriteSafety::Canonical);
        assert_eq!(
            canonical,
            render_design_artifact(&reparsed.artifact, &reparsed.sections)
        );
    }

    #[test]
    fn unknown_section_blocks_rewrite() {
        let source = "---\nid: \"node-a\"\ntitle: \"A\"\nstatus: seed\ntags: []\ndependencies: []\nrelated: []\nopen_questions: []\nbranches: []\n---\n\n# A\n\n## Operator Notes\n\nKeep me.\n";
        let parsed = parse_design_artifact(source, "docs/node-a.md").unwrap();
        assert_eq!(
            parsed.rewrite_safety,
            RewriteSafety::BlockedByUnknownContent
        );
        assert!(
            parsed
                .diagnostics
                .iter()
                .any(|finding| finding.kind == DesignDiagnosticKind::UnknownSection)
        );
    }
}
