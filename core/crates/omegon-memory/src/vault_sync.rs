//! Bidirectional sync between Omegon's fact store and a Codex vault directory.
//!
//! - `materialize_to_vault` writes facts as markdown files with TOML frontmatter
//! - `materialize_episodes_to_vault` writes episodes as daily-note markdown files
//! - `import_from_vault` reads Codex-authored memory facts back into the backend

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tracing;

use crate::backend::MemoryBackend;
use crate::types::*;
use crate::util::now_iso;

// ─── Report types ───────────────────────────────────────────────────────────

/// Result of materializing facts to the vault.
#[derive(Debug, Clone)]
pub struct MaterializeReport {
    pub sections_written: usize,
    pub facts_written: usize,
    pub files_written: Vec<PathBuf>,
}

/// Result of importing facts from the vault.
#[derive(Debug, Clone)]
pub struct ImportReport {
    pub facts_imported: usize,
    pub facts_skipped: usize,
}

/// Result of reinforcing facts referenced by vault documents.
#[derive(Debug, Clone)]
pub struct ReinforcementReport {
    pub facts_reinforced: usize,
    pub references_dangling: usize,
    pub references_superseded: Vec<SupersededReference>,
}

/// A note references a fact that has been superseded.
#[derive(Debug, Clone)]
pub struct SupersededReference {
    pub note_path: PathBuf,
    pub old_fact_id: String,
    pub new_fact_id: String,
}

// ─── Section helpers ────────────────────────────────────────────────────────

/// Map a Section enum variant to a filesystem-safe slug.
pub fn section_to_slug(section: &Section) -> &'static str {
    match section {
        Section::Architecture => "architecture",
        Section::Decisions => "decisions",
        Section::Constraints => "constraints",
        Section::KnownIssues => "known-issues",
        Section::PatternsConventions => "patterns-conventions",
        Section::Specs => "specs",
        Section::RecentWork => "recent-work",
    }
}

/// Map a Section to its human-readable description.
pub fn section_description(section: &Section) -> &'static str {
    match section {
        Section::Architecture => "System structure, component relationships, key abstractions",
        Section::Decisions => "Choices made and their rationale",
        Section::Constraints => "Requirements, limitations, environment details",
        Section::KnownIssues => "Bugs, flaky tests, workarounds",
        Section::PatternsConventions => "Code style, project conventions, common approaches",
        Section::Specs => "Active specifications and design contracts",
        Section::RecentWork => "Recent session activity",
    }
}

/// Map a Section to its display name (matching serde rename).
fn section_display_name(section: &Section) -> &'static str {
    match section {
        Section::Architecture => "Architecture",
        Section::Decisions => "Decisions",
        Section::Constraints => "Constraints",
        Section::KnownIssues => "Known Issues",
        Section::PatternsConventions => "Patterns & Conventions",
        Section::Specs => "Specs",
        Section::RecentWork => "Recent Work",
    }
}

/// Try to map a topic string (from Codex frontmatter) to a Section.
fn topic_to_section(topic: &str) -> Option<Section> {
    let lower = topic.to_lowercase();
    match lower.as_str() {
        "architecture" => Some(Section::Architecture),
        "decisions" => Some(Section::Decisions),
        "constraints" => Some(Section::Constraints),
        "known issues" | "known-issues" | "knownissues" => Some(Section::KnownIssues),
        "patterns & conventions" | "patterns-conventions" | "patterns" | "conventions" => {
            Some(Section::PatternsConventions)
        }
        "specs" | "specifications" => Some(Section::Specs),
        "recent work" | "recent-work" | "recentwork" => Some(Section::RecentWork),
        _ => None,
    }
}

// ─── TOML frontmatter parsing ───────────────────────────────────────────────

/// Parse TOML frontmatter delimited by `+++` lines. Returns (frontmatter, body).
/// Returns None if the file doesn't have `+++` delimiters.
fn parse_frontmatter(content: &str) -> Option<(&str, &str)> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("+++") {
        return None;
    }
    // Find the closing +++
    let after_open = &trimmed[3..];
    let after_open = after_open.trim_start_matches(['\r', '\n']);
    let close_pos = after_open.find("\n+++")?;
    let frontmatter = &after_open[..close_pos];
    let body_start = close_pos + 4; // skip "\n+++"
    let body = if body_start < after_open.len() {
        after_open[body_start..].trim_start_matches(['\r', '\n'])
    } else {
        ""
    };
    Some((frontmatter, body))
}

/// Extract a simple string value from TOML-like frontmatter (without a full TOML parser).
/// Handles `key = "value"` patterns.
fn extract_toml_value<'a>(frontmatter: &'a str, key: &str) -> Option<&'a str> {
    for line in frontmatter.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(key) {
            let rest = rest.trim();
            if let Some(rest) = rest.strip_prefix('=') {
                let rest = rest.trim();
                if rest.starts_with('"') && rest.len() > 1 {
                    // Find closing quote
                    if let Some(end) = rest[1..].find('"') {
                        return Some(&rest[1..1 + end]);
                    }
                }
            }
        }
    }
    None
}

// ─── Materialize facts ─────────────────────────────────────────────────────

/// Write all active facts from the backend to the vault as markdown files.
///
/// Creates one file per section at `{vault_path}/{subdir}/{slug}.md` plus
/// an index file at `{vault_path}/{subdir}/_index.md`.
///
/// The default subdirectory is `ai/memory` (omegon convention). Pass a
/// custom value to integrate with a different vault layout.
pub async fn materialize_to_vault(
    backend: &dyn MemoryBackend,
    vault_path: &Path,
    mind: &str,
) -> Result<MaterializeReport> {
    materialize_to_vault_with_subdir(backend, vault_path, mind, "ai/memory").await
}

/// Like [`materialize_to_vault`] but with a configurable subdirectory.
pub async fn materialize_to_vault_with_subdir(
    backend: &dyn MemoryBackend,
    vault_path: &Path,
    mind: &str,
    subdir: &str,
) -> Result<MaterializeReport> {
    let memory_dir = vault_path.join(subdir);
    tokio::fs::create_dir_all(&memory_dir)
        .await
        .context("creating vault memory directory")?;

    let now = now_iso();
    // Truncate to date for index display: "2026-04-21T18:00:00.000Z" -> "2026-04-21"
    let today = &now[..10];

    let mut sections_written = 0usize;
    let mut facts_written = 0usize;
    let mut files_written = Vec::new();

    // For the index table
    let mut index_rows: Vec<(String, String, usize)> = Vec::new(); // (slug, display_name, count)

    for section in Section::all() {
        let filter = FactFilter {
            section: Some(section.clone()),
            status: Some(FactStatus::Active),
        };
        let mut facts = backend
            .list_facts(mind, filter)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        if facts.is_empty() {
            continue;
        }

        // Sort by confidence descending
        facts.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));

        let slug = section_to_slug(section);
        let display_name = section_display_name(section);
        let desc = section_description(section);
        let fact_count = facts.len();

        // Build markdown content
        let mut content = String::new();

        // TOML frontmatter
        content.push_str("+++\n");
        content.push_str(&format!("id = \"memory-section-{slug}\"\n"));
        content.push_str("kind = \"memory_section\"\n");
        content.push_str(&format!("tags = [\"memory\", \"{slug}\"]\n"));
        content.push_str("\n[data]\n");
        content.push_str(&format!("section = \"{display_name}\"\n"));
        content.push_str(&format!("fact_count = {fact_count}\n"));
        content.push_str(&format!("last_updated = \"{now}\"\n"));
        content.push_str(&format!("mind = \"{mind}\"\n"));
        content.push_str("+++\n\n");

        // Heading and description
        content.push_str(&format!("# {display_name}\n\n"));
        content.push_str(&format!("_{desc}_\n\n"));

        // Fact bullets
        for fact in &facts {
            content.push_str(&format!(
                "- {} [confidence: {:.2}, id: {}]\n",
                fact.content, fact.confidence, fact.id
            ));
        }

        let file_path = memory_dir.join(format!("{slug}.md"));
        tokio::fs::write(&file_path, content.as_bytes())
            .await
            .with_context(|| format!("writing {}", file_path.display()))?;

        tracing::debug!(section = display_name, facts = fact_count, "materialized section");

        sections_written += 1;
        facts_written += fact_count;
        files_written.push(file_path);
        index_rows.push((slug.to_string(), display_name.to_string(), fact_count));
    }

    // Write index file
    if !index_rows.is_empty() {
        let mut idx = String::new();
        idx.push_str("# Project Memory\n\n");
        idx.push_str("| Section | Facts | Last Updated |\n");
        idx.push_str("|---------|-------|-------------|\n");
        for (slug, _display, count) in &index_rows {
            idx.push_str(&format!("| [[{slug}]] | {count} | {today} |\n"));
        }

        let index_path = memory_dir.join("_index.md");
        tokio::fs::write(&index_path, idx.as_bytes())
            .await
            .context("writing _index.md")?;
        files_written.push(index_path);
    }

    Ok(MaterializeReport {
        sections_written,
        facts_written,
        files_written,
    })
}

// ─── Materialize episodes ───────────────────────────────────────────────────

/// Write recent episodes as daily-note-style markdown files.
///
/// Files are written to `{vault_path}/{subdir}/episodes/{date}.md`
/// where subdir defaults to `ai/memory`.
/// Returns the number of episodes written.
pub async fn materialize_episodes_to_vault(
    backend: &dyn MemoryBackend,
    vault_path: &Path,
    mind: &str,
    limit: usize,
) -> Result<usize> {
    materialize_episodes_to_vault_with_subdir(backend, vault_path, mind, limit, "ai/memory").await
}

/// Like [`materialize_episodes_to_vault`] but with a configurable subdirectory.
pub async fn materialize_episodes_to_vault_with_subdir(
    backend: &dyn MemoryBackend,
    vault_path: &Path,
    mind: &str,
    limit: usize,
    subdir: &str,
) -> Result<usize> {
    let episodes_dir = vault_path.join(subdir).join("episodes");
    tokio::fs::create_dir_all(&episodes_dir)
        .await
        .context("creating episodes directory")?;

    let episodes = backend
        .list_episodes(mind, limit)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let mut written = 0usize;

    for ep in &episodes {
        let date = &ep.date;
        let tool_calls = ep.tool_calls_count.unwrap_or(0);

        let mut content = String::new();
        content.push_str("+++\n");
        content.push_str(&format!("id = \"episode-{date}\"\n"));
        content.push_str("kind = \"memory_episode\"\n");
        content.push_str(&format!(
            "tags = [\"memory\", \"episode\", \"{date}\"]\n"
        ));
        content.push_str("\n[data]\n");
        content.push_str(&format!("date = \"{date}\"\n"));
        content.push_str(&format!("mind = \"{mind}\"\n"));
        content.push_str(&format!("title = \"{}\"\n", ep.title.replace('"', "\\\"")));
        content.push_str(&format!("tool_calls = {tool_calls}\n"));
        content.push_str("+++\n\n");

        content.push_str(&ep.narrative);
        content.push('\n');

        let file_path = episodes_dir.join(format!("{date}.md"));
        tokio::fs::write(&file_path, content.as_bytes())
            .await
            .with_context(|| format!("writing episode {}", file_path.display()))?;

        tracing::debug!(date = date, title = %ep.title, "materialized episode");
        written += 1;
    }

    Ok(written)
}

// ─── Import from vault ──────────────────────────────────────────────────────

/// Scan the vault's memory directory for Codex-authored facts and import them.
///
/// Only files with `kind = "memory_fact"` in their TOML frontmatter are imported.
/// Files with `kind = "memory_section"` or `kind = "memory_episode"` (written by
/// the materializer) are skipped.
pub async fn import_from_vault(
    backend: &dyn MemoryBackend,
    vault_path: &Path,
    mind: &str,
) -> Result<ImportReport> {
    let memory_dir = vault_path.join("ai").join("memory");

    if !memory_dir.exists() {
        return Ok(ImportReport {
            facts_imported: 0,
            facts_skipped: 0,
        });
    }

    let mut facts_imported = 0usize;
    let mut facts_skipped = 0usize;

    let mut entries = tokio::fs::read_dir(&memory_dir)
        .await
        .context("reading vault memory directory")?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();

        // Only process .md files (not directories)
        if path.is_dir() || path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }

        let content = match tokio::fs::read_to_string(&path).await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "failed to read vault file");
                facts_skipped += 1;
                continue;
            }
        };

        let (frontmatter, body) = match parse_frontmatter(&content) {
            Some(pair) => pair,
            None => {
                facts_skipped += 1;
                continue;
            }
        };

        // Only import files with kind = "memory_fact" (Codex-authored)
        let kind = extract_toml_value(frontmatter, "kind");
        if kind != Some("memory_fact") {
            facts_skipped += 1;
            continue;
        }

        // Extract topic/title for section mapping
        let topic = extract_toml_value(frontmatter, "topic")
            .or_else(|| extract_toml_value(frontmatter, "title"));

        let section = topic
            .and_then(topic_to_section)
            .unwrap_or(Section::Architecture);

        // The body is the fact content. Trim and use non-empty content.
        let fact_content = body.trim();
        if fact_content.is_empty() {
            facts_skipped += 1;
            continue;
        }

        let req = StoreFact {
            mind: mind.to_string(),
            content: fact_content.to_string(),
            section,
            decay_profile: DecayProfileName::Standard,
            source: Some("codex-vault".to_string()),
        };

        match backend.store_fact(req).await {
            Ok(_result) => {
                tracing::debug!(path = %path.display(), "imported fact from vault");
                facts_imported += 1;
            }
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "failed to import fact");
                facts_skipped += 1;
            }
        }
    }

    Ok(ImportReport {
        facts_imported,
        facts_skipped,
    })
}

// ─── Fact reference reinforcement ───────────────────────────────────────────

/// Extract a TOML array of strings from frontmatter.
/// Handles `key = ["val1", "val2"]` on a single line.
fn extract_toml_string_array(frontmatter: &str, key: &str) -> Vec<String> {
    for line in frontmatter.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(key) {
            let rest = rest.trim();
            if let Some(rest) = rest.strip_prefix('=') {
                let rest = rest.trim();
                if rest.starts_with('[') && rest.ends_with(']') {
                    let inner = &rest[1..rest.len() - 1];
                    return inner
                        .split(',')
                        .filter_map(|s| {
                            let s = s.trim().trim_matches('"');
                            if s.is_empty() { None } else { Some(s.to_string()) }
                        })
                        .collect();
                }
            }
        }
    }
    Vec::new()
}

/// Scan vault documents for `related_facts` references and reinforce those facts.
///
/// Any fact referenced by an active vault document gets its decay timer reset
/// and reinforcement count incremented. Facts that are woven into documentation
/// should not silently decay away.
///
/// Also detects superseded references: if a note references fact A which was
/// superseded by fact B, the report includes the mapping so the caller can
/// notify the operator or auto-update the reference.
pub async fn reinforce_referenced_facts(
    backend: &dyn MemoryBackend,
    vault_path: &Path,
) -> Result<ReinforcementReport> {
    let mut facts_reinforced = 0usize;
    let mut references_dangling = 0usize;
    let mut references_superseded = Vec::new();

    // Walk the entire vault looking for .md files with related_facts in frontmatter
    let mut dirs_to_visit = vec![vault_path.to_path_buf()];

    while let Some(dir) = dirs_to_visit.pop() {
        let mut entries = match tokio::fs::read_dir(&dir).await {
            Ok(e) => e,
            Err(_) => continue,
        };

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            if path.is_dir() {
                // Skip .codex and .git directories
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !name.starts_with('.') {
                    dirs_to_visit.push(path);
                }
                continue;
            }

            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }

            let content = match tokio::fs::read_to_string(&path).await {
                Ok(c) => c,
                Err(_) => continue,
            };

            let (frontmatter, _body) = match parse_frontmatter(&content) {
                Some(pair) => pair,
                None => continue,
            };

            // Check for related_facts in frontmatter or [data] section
            let fact_ids = extract_toml_string_array(frontmatter, "related_facts");
            if fact_ids.is_empty() {
                continue;
            }

            let rel_path = path
                .strip_prefix(vault_path)
                .unwrap_or(&path)
                .to_path_buf();

            for fact_id in &fact_ids {
                match backend.get_fact(fact_id).await {
                    Ok(Some(fact)) => {
                        // Fact exists and is active — reinforce it
                        if fact.status == FactStatus::Active {
                            match backend.reinforce_fact(fact_id).await {
                                Ok(_) => {
                                    tracing::debug!(
                                        fact_id,
                                        note = %rel_path.display(),
                                        "reinforced fact referenced by note"
                                    );
                                    facts_reinforced += 1;
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        fact_id,
                                        error = %e,
                                        "failed to reinforce referenced fact"
                                    );
                                }
                            }
                        } else if fact.status == FactStatus::Superseded {
                            // Fact was superseded — find the replacement
                            if let Some(ref new_id) = fact.superseded_by {
                                references_superseded.push(SupersededReference {
                                    note_path: rel_path.clone(),
                                    old_fact_id: fact_id.clone(),
                                    new_fact_id: new_id.clone(),
                                });
                            } else {
                                references_dangling += 1;
                            }
                        } else {
                            // Archived without supersession — dangling
                            references_dangling += 1;
                        }
                    }
                    Ok(None) => {
                        // Fact doesn't exist — dangling reference
                        tracing::debug!(
                            fact_id,
                            note = %rel_path.display(),
                            "dangling fact reference in note"
                        );
                        references_dangling += 1;
                    }
                    Err(e) => {
                        tracing::warn!(fact_id, error = %e, "failed to look up referenced fact");
                        references_dangling += 1;
                    }
                }
            }
        }
    }

    Ok(ReinforcementReport {
        facts_reinforced,
        references_dangling,
        references_superseded,
    })
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inmemory::InMemoryBackend;
    use std::sync::Arc;

    /// Helper: store N facts in a section via the backend.
    async fn seed_facts(backend: &dyn MemoryBackend, section: Section, count: usize) {
        for i in 0..count {
            let req = StoreFact {
                mind: "default".into(),
                content: format!("Fact {i} for {}", section_display_name(&section)),
                section: section.clone(),
                decay_profile: DecayProfileName::Standard,
                source: Some("test".into()),
            };
            backend.store_fact(req).await.unwrap();
        }
    }

    #[tokio::test]
    async fn materialize_report_counts_correctly() {
        let backend = Arc::new(InMemoryBackend::new());
        let tmp = tempfile::tempdir().unwrap();
        let vault = tmp.path();

        // Seed 3 Architecture, 2 Decisions
        seed_facts(backend.as_ref(), Section::Architecture, 3).await;
        seed_facts(backend.as_ref(), Section::Decisions, 2).await;

        let report = materialize_to_vault(backend.as_ref(), vault, "default")
            .await
            .unwrap();

        assert_eq!(report.sections_written, 2, "should write 2 sections");
        assert_eq!(report.facts_written, 5, "should write 5 total facts");
        // 2 section files + 1 index file
        assert_eq!(report.files_written.len(), 3);

        // Verify section files exist and have content
        let arch_path = vault.join("ai/memory/architecture.md");
        assert!(arch_path.exists(), "architecture.md should exist");
        let arch_content = std::fs::read_to_string(&arch_path).unwrap();
        assert!(arch_content.contains("kind = \"memory_section\""));
        assert!(arch_content.contains("fact_count = 3"));
        assert!(arch_content.contains("# Architecture"));

        let dec_path = vault.join("ai/memory/decisions.md");
        assert!(dec_path.exists(), "decisions.md should exist");

        // Verify index file
        let idx_path = vault.join("ai/memory/_index.md");
        assert!(idx_path.exists(), "_index.md should exist");
        let idx_content = std::fs::read_to_string(&idx_path).unwrap();
        assert!(idx_content.contains("[[architecture]]"));
        assert!(idx_content.contains("[[decisions]]"));
    }

    #[test]
    fn section_slug_roundtrip() {
        // Every section variant produces a non-empty, filesystem-safe slug
        for section in Section::all() {
            let slug = section_to_slug(section);
            assert!(!slug.is_empty(), "slug should not be empty for {section:?}");
            assert!(
                slug.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
                "slug should be filesystem-safe: {slug}"
            );
            // Also verify description is non-empty
            let desc = section_description(section);
            assert!(!desc.is_empty(), "description should not be empty for {section:?}");
        }
    }

    #[tokio::test]
    async fn import_skips_materializer_files() {
        let backend = Arc::new(InMemoryBackend::new());
        let tmp = tempfile::tempdir().unwrap();
        let vault = tmp.path();

        let memory_dir = vault.join("ai").join("memory");
        std::fs::create_dir_all(&memory_dir).unwrap();

        // Write a materializer file (kind = "memory_section") — should be skipped
        let section_file = memory_dir.join("architecture.md");
        std::fs::write(
            &section_file,
            "+++\nkind = \"memory_section\"\n+++\n\n# Architecture\n- Some fact\n",
        )
        .unwrap();

        // Write an episode file (kind = "memory_episode") — should be skipped
        let episode_file = memory_dir.join("episode-2026-04-21.md");
        std::fs::write(
            &episode_file,
            "+++\nkind = \"memory_episode\"\n+++\n\nSome narrative\n",
        )
        .unwrap();

        // Write a Codex-authored fact (kind = "memory_fact") — should be imported
        let codex_file = memory_dir.join("codex-fact-1.md");
        std::fs::write(
            &codex_file,
            "+++\nkind = \"memory_fact\"\ntopic = \"Architecture\"\n+++\n\nThe API uses REST with JSON payloads\n",
        )
        .unwrap();

        let report = import_from_vault(backend.as_ref(), vault, "default")
            .await
            .unwrap();

        assert_eq!(report.facts_imported, 1, "should import the Codex fact");
        assert_eq!(report.facts_skipped, 2, "should skip materializer files");

        // Verify the fact was actually stored
        let facts = backend
            .list_facts(
                "default",
                FactFilter {
                    section: Some(Section::Architecture),
                    status: Some(FactStatus::Active),
                },
            )
            .await
            .unwrap();
        assert_eq!(facts.len(), 1);
        assert!(facts[0].content.contains("REST with JSON"));
        assert_eq!(facts[0].source.as_deref(), Some("codex-vault"));
    }

    #[test]
    fn parse_frontmatter_works() {
        let input = "+++\nkind = \"memory_fact\"\ntopic = \"Decisions\"\n+++\n\nThe body here\n";
        let (fm, body) = parse_frontmatter(input).unwrap();
        assert!(fm.contains("kind = \"memory_fact\""));
        assert!(body.contains("The body here"));
    }

    #[test]
    fn parse_frontmatter_returns_none_without_delimiters() {
        let input = "# Just a regular markdown file\n\nNo frontmatter here.\n";
        assert!(parse_frontmatter(input).is_none());
    }

    #[test]
    fn extract_toml_value_works() {
        let fm = "kind = \"memory_fact\"\ntopic = \"Architecture\"\nfact_count = 3";
        assert_eq!(extract_toml_value(fm, "kind"), Some("memory_fact"));
        assert_eq!(extract_toml_value(fm, "topic"), Some("Architecture"));
        assert_eq!(extract_toml_value(fm, "missing"), None);
    }

    #[test]
    fn extract_toml_string_array_works() {
        let fm = "related_facts = [\"abc123\", \"def456\"]";
        let result = extract_toml_string_array(fm, "related_facts");
        assert_eq!(result, vec!["abc123", "def456"]);
    }

    #[test]
    fn extract_toml_string_array_empty() {
        let fm = "related_facts = []";
        let result = extract_toml_string_array(fm, "related_facts");
        assert!(result.is_empty());
    }

    #[test]
    fn extract_toml_string_array_missing() {
        let fm = "kind = \"note\"";
        let result = extract_toml_string_array(fm, "related_facts");
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn reinforce_referenced_facts_reinforces_active() {
        let backend = Arc::new(InMemoryBackend::new());
        let tmp = tempfile::tempdir().unwrap();
        let vault = tmp.path();

        // Store a fact
        let result = backend
            .store_fact(StoreFact {
                mind: "default".into(),
                content: "Important architecture fact".into(),
                section: Section::Architecture,
                decay_profile: DecayProfileName::Standard,
                source: Some("test".into()),
            })
            .await
            .unwrap();
        let fact_id = result.fact.id.clone();
        let initial_reinforcement = result.fact.reinforcement_count;

        // Create a note that references this fact
        let notes_dir = vault.join("notes");
        std::fs::create_dir_all(&notes_dir).unwrap();
        std::fs::write(
            notes_dir.join("design.md"),
            format!(
                "+++\nkind = \"note\"\nrelated_facts = [\"{fact_id}\"]\n+++\n\n# Design Doc\nReferences the architecture fact.\n"
            ),
        )
        .unwrap();

        let report = reinforce_referenced_facts(backend.as_ref(), vault)
            .await
            .unwrap();

        assert_eq!(report.facts_reinforced, 1);
        assert_eq!(report.references_dangling, 0);
        assert!(report.references_superseded.is_empty());

        // Verify reinforcement count increased
        let fact = backend.get_fact(&fact_id).await.unwrap().unwrap();
        assert!(
            fact.reinforcement_count > initial_reinforcement,
            "reinforcement count should increase: {} > {}",
            fact.reinforcement_count,
            initial_reinforcement
        );
    }

    #[tokio::test]
    async fn reinforce_referenced_facts_detects_dangling() {
        let backend = Arc::new(InMemoryBackend::new());
        let tmp = tempfile::tempdir().unwrap();
        let vault = tmp.path();

        // Create a note referencing a fact that doesn't exist
        std::fs::create_dir_all(vault.join("notes")).unwrap();
        std::fs::write(
            vault.join("notes/orphan.md"),
            "+++\nrelated_facts = [\"nonexistent123\"]\n+++\n\n# Orphan note\n",
        )
        .unwrap();

        let report = reinforce_referenced_facts(backend.as_ref(), vault)
            .await
            .unwrap();

        assert_eq!(report.facts_reinforced, 0);
        assert_eq!(report.references_dangling, 1);
    }

    #[tokio::test]
    async fn reinforce_skips_dotdirs() {
        let backend = Arc::new(InMemoryBackend::new());
        let tmp = tempfile::tempdir().unwrap();
        let vault = tmp.path();

        // Create a file inside .codex/ — should be skipped
        std::fs::create_dir_all(vault.join(".codex")).unwrap();
        std::fs::write(
            vault.join(".codex/internal.md"),
            "+++\nrelated_facts = [\"shouldnotprocess\"]\n+++\n\nInternal\n",
        )
        .unwrap();

        let report = reinforce_referenced_facts(backend.as_ref(), vault)
            .await
            .unwrap();

        assert_eq!(report.facts_reinforced, 0);
        assert_eq!(report.references_dangling, 0, "should not have scanned .codex/");
    }
}
