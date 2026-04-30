//! Bundled skill management — list and install curated skills to ~/.omegon/skills/.
//!
//! Skills are markdown directive files injected into the system prompt at session start.
//! Bundled skills ship embedded in the binary so `omegon skills install` works regardless
//! of whether a source tree is present.
//!
//! Two-tier load order (established by PluginRegistry::load_skills):
//!   1. ~/.omegon/skills/*/SKILL.md   — bundled / user-installed
//!   2. <cwd>/.omegon/skills/*/SKILL.md — project-local (overrides bundled)

/// All skills bundled into the binary at compile time.
/// Each entry is (name, skill_markdown_content).
pub const BUNDLED: &[(&str, &str)] = &[
    ("git", include_str!("../../../../skills/git/SKILL.md")),
    ("oci", include_str!("../../../../skills/oci/SKILL.md")),
    (
        "openspec",
        include_str!("../../../../skills/openspec/SKILL.md"),
    ),
    ("python", include_str!("../../../../skills/python/SKILL.md")),
    ("rust", include_str!("../../../../skills/rust/SKILL.md")),
    (
        "security",
        include_str!("../../../../skills/security/SKILL.md"),
    ),
    ("style", include_str!("../../../../skills/style/SKILL.md")),
    (
        "typescript",
        include_str!("../../../../skills/typescript/SKILL.md"),
    ),
    ("vault", include_str!("../../../../skills/vault/SKILL.md")),
];

fn skills_dir() -> Option<std::path::PathBuf> {
    crate::paths::omegon_home().ok().map(|h| h.join("skills"))
}

/// Render bundled skills and their installation status as terminal-friendly text.
pub fn list_summary() -> anyhow::Result<String> {
    let skills_dir = skills_dir();

    let mut lines = vec![format!("Bundled skills ({})\n", BUNDLED.len())];

    for (name, content) in BUNDLED {
        // Extract description from frontmatter if present
        let description = extract_description(content).unwrap_or("(no description)");

        let installed = skills_dir
            .as_ref()
            .is_some_and(|d| d.join(name).join("SKILL.md").exists());
        let status = if installed { "✓" } else { "○" };
        lines.push(format!("  {status} {name:<14} {description}"));
    }

    let install_path = skills_dir
        .as_ref()
        .map(|d| d.display().to_string())
        .unwrap_or_else(|| "(unknown)".into());

    lines.push(format!("\nInstall location: {install_path}"));
    lines.push("  ✓ = installed    ○ = not yet installed".into());
    lines.push("\nRun `omegon skills install` to install all bundled skills.".into());

    // Show any project-local skills if cwd has them
    let cwd = std::env::current_dir()?;
    let project_skills = cwd.join(".omegon").join("skills");
    if project_skills.is_dir() {
        let mut local: Vec<String> = std::fs::read_dir(&project_skills)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().join("SKILL.md").exists())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        local.sort();
        if !local.is_empty() {
            lines.push("\nProject-local skills (.omegon/skills/):".into());
            for name in &local {
                lines.push(format!("  ● {name}"));
            }
        }
    }

    Ok(lines.join("\n"))
}

/// List bundled skills and their installation status.
pub fn cmd_list() -> anyhow::Result<()> {
    println!("{}", list_summary()?);
    Ok(())
}

/// Install all bundled skills to ~/.omegon/skills/.
/// Existing files are overwritten. Project-local skills are never touched.
pub fn cmd_install() -> anyhow::Result<()> {
    let skills_dir =
        skills_dir().ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;

    std::fs::create_dir_all(&skills_dir)?;

    let mut installed = 0;
    let mut updated = 0;

    for (name, content) in BUNDLED {
        let skill_dir = skills_dir.join(name);
        let skill_file = skill_dir.join("SKILL.md");

        std::fs::create_dir_all(&skill_dir)?;

        let already_exists = skill_file.exists();
        let existing_content = if already_exists {
            std::fs::read_to_string(&skill_file).ok()
        } else {
            None
        };

        let changed = existing_content.as_deref() != Some(content);

        std::fs::write(&skill_file, content)?;

        if !already_exists {
            println!("  + {name}");
            installed += 1;
        } else if changed {
            println!("  ↑ {name}  (updated)");
            updated += 1;
        } else {
            println!("  ✓ {name}  (unchanged)");
        }
    }

    println!(
        "\n{} skill(s) installed, {} updated → {}",
        installed,
        updated,
        skills_dir.display()
    );
    println!("Skills are active immediately in new sessions.");

    Ok(())
}

/// Extract `trusted_paths` from SKILL.md frontmatter.
///
/// Skills that need to read/write outside the workspace can declare paths
/// in their frontmatter. On session startup, these paths are auto-trusted
/// (added to settings.trusted_directories if not already present), so the
/// user isn't prompted repeatedly.
///
/// YAML format:
/// ```yaml
/// trusted_paths:
///   - ~/Documents/pastperformance/
///   - ~/Library/Mobile Documents/iCloud~md~obsidian/Documents/jaredp/evaluations/
/// ```
///
/// TOML format:
/// ```toml
/// trusted_paths = ["~/Documents/pastperformance/"]
/// ```
pub fn extract_trusted_paths(content: &str) -> Vec<String> {
    let (body, delimiter) = if let Some(b) = content.strip_prefix("---\n") {
        (b, "\n---")
    } else if let Some(b) = content.strip_prefix("+++\n") {
        (b, "\n+++")
    } else {
        return Vec::new();
    };
    let end = match body.find(delimiter) {
        Some(e) => e,
        None => return Vec::new(),
    };
    let frontmatter = &body[..end];

    let mut paths = Vec::new();

    // TOML: trusted_paths = ["path1", "path2"]
    for line in frontmatter.lines() {
        if let Some(rest) = line.strip_prefix("trusted_paths") {
            let rest = rest.trim();
            if let Some(rest) = rest.strip_prefix('=') {
                let rest = rest.trim();
                if rest.starts_with('[') {
                    // Parse simple TOML array: ["path1", "path2"]
                    let inner = rest.trim_start_matches('[').trim_end_matches(']');
                    for item in inner.split(',') {
                        let item = item.trim().trim_matches('"').trim_matches('\'');
                        if !item.is_empty() {
                            paths.push(item.to_string());
                        }
                    }
                }
            }
        }
    }

    // YAML: trusted_paths:\n  - path1\n  - path2
    let mut in_trusted_paths = false;
    for line in frontmatter.lines() {
        if line.starts_with("trusted_paths:") {
            in_trusted_paths = true;
            // Check for inline value: trusted_paths: [path1, path2]
            let rest = line.strip_prefix("trusted_paths:").unwrap().trim();
            if rest.starts_with('[') {
                let inner = rest.trim_start_matches('[').trim_end_matches(']');
                for item in inner.split(',') {
                    let item = item.trim().trim_matches('"').trim_matches('\'');
                    if !item.is_empty() {
                        paths.push(item.to_string());
                    }
                }
                in_trusted_paths = false;
            }
            continue;
        }
        if in_trusted_paths {
            let trimmed = line.trim();
            if let Some(path) = trimmed.strip_prefix("- ") {
                let path = path.trim().trim_matches('"').trim_matches('\'');
                if !path.is_empty() {
                    paths.push(path.to_string());
                }
            } else if !trimmed.is_empty() && !trimmed.starts_with('#') {
                // Not a list item — end of trusted_paths block
                in_trusted_paths = false;
            }
        }
    }

    paths.sort();
    paths.dedup();
    paths
}

/// Collect trusted_paths from all loaded skill content strings.
pub fn collect_trusted_paths(skills: &[String]) -> Vec<String> {
    let mut all_paths = Vec::new();
    for content in skills {
        all_paths.extend(extract_trusted_paths(content));
    }
    all_paths.sort();
    all_paths.dedup();
    all_paths
}

/// Extract phase/step markers from skill content for completion tracking.
///
/// Looks for markdown headings that indicate numbered phases or steps:
///   `## Phase 1:`, `## Phase 2:`, ..., `## Step 1:`, `## Step 2:`, ...
///   `## Phase 0.5:`, `## Phase 1.75:` (fractional phases like in jputman's skill)
///
/// Returns the total number of phases and the label of the final phase
/// (e.g., "Phase 10: Export to File"). Used by the loop to detect
/// premature completion — if the agent declares "done" without reaching
/// the final phase, it gets nudged.
pub fn extract_phase_info(content: &str) -> Option<SkillPhaseInfo> {
    let mut phases: Vec<(String, String)> = Vec::new(); // (number, full heading)

    for line in content.lines() {
        let trimmed = line.trim();
        // Match: ## Phase N: ... or ## Step N: ...
        // Also handles ### Phase N, # Phase N, etc.
        if let Some(rest) = trimmed
            .strip_prefix('#')
            .and_then(|s| s.trim_start_matches('#').trim_start().strip_prefix("Phase "))
            .or_else(|| {
                trimmed
                    .strip_prefix('#')
                    .and_then(|s| s.trim_start_matches('#').trim_start().strip_prefix("Step "))
            })
        {
            // Extract the phase number (may be fractional: "0.5", "1.75")
            let num_end = rest
                .find(|c: char| !c.is_ascii_digit() && c != '.')
                .unwrap_or(rest.len());
            let num = &rest[..num_end];
            if !num.is_empty() {
                let heading = trimmed
                    .trim_start_matches('#')
                    .trim()
                    .to_string();
                phases.push((num.to_string(), heading));
            }
        }
    }

    if phases.is_empty() {
        return None;
    }

    let final_phase = phases.last().unwrap();
    Some(SkillPhaseInfo {
        total_phases: phases.len(),
        final_phase_label: final_phase.1.clone(),
        final_phase_number: final_phase.0.clone(),
    })
}

/// Phase tracking info extracted from a skill's content.
#[derive(Debug, Clone)]
pub struct SkillPhaseInfo {
    /// Total number of phase/step headings found.
    pub total_phases: usize,
    /// The heading text of the final phase (e.g., "Phase 10: Export to File").
    pub final_phase_label: String,
    /// The phase number string (e.g., "10", "1.75").
    pub final_phase_number: String,
}

/// Collect phase info from all loaded skills. Returns info for any skill
/// that has numbered phases (most skills don't — only structured workflows like
/// jputman's opportunity-eval).
pub fn collect_phase_info(skills: &[String]) -> Vec<SkillPhaseInfo> {
    skills.iter().filter_map(|s| extract_phase_info(s)).collect()
}

/// Extract the `description` field from YAML frontmatter.
fn extract_description(content: &str) -> Option<&str> {
    // Support both YAML (---) and TOML (+++) frontmatter delimiters.
    let (body, delimiter) = if let Some(b) = content.strip_prefix("---\n") {
        (b, "\n---")
    } else if let Some(b) = content.strip_prefix("+++\n") {
        (b, "\n+++")
    } else {
        return None;
    };
    let end = body.find(delimiter)?;
    let frontmatter = &body[..end];

    for line in frontmatter.lines() {
        // YAML: `description: Some text`
        if let Some(rest) = line.strip_prefix("description:") {
            return Some(rest.trim());
        }
        // TOML: `description = "Some text"`
        if let Some(rest) = line.strip_prefix("description") {
            let rest = rest.trim();
            if let Some(rest) = rest.strip_prefix('=') {
                let rest = rest.trim();
                if rest.starts_with('"')
                    && rest.len() > 1
                    && let Some(end) = rest[1..].find('"')
                {
                    return Some(&rest[1..1 + end]);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_skills_all_have_content() {
        for (name, content) in BUNDLED {
            assert!(!content.is_empty(), "skill '{name}' is empty");
            assert!(content.len() > 100, "skill '{name}' seems too short");
        }
    }

    #[test]
    fn bundled_skills_all_have_descriptions() {
        for (name, content) in BUNDLED {
            assert!(
                extract_description(content).is_some(),
                "skill '{name}' missing frontmatter description"
            );
        }
    }

    #[test]
    fn bundled_count_matches_skills_directory() {
        // 9 skills: git, oci, openspec, python, rust, security, style, typescript, vault
        assert_eq!(BUNDLED.len(), 9);
    }

    #[test]
    fn extract_description_parses_frontmatter() {
        let content = "---\nname: test\ndescription: A test skill\n---\n\n# Test";
        assert_eq!(extract_description(content), Some("A test skill"));
    }

    #[test]
    fn extract_description_returns_none_without_frontmatter() {
        let content = "# No frontmatter here";
        assert_eq!(extract_description(content), None);
    }

    #[test]
    fn extract_description_parses_toml_frontmatter() {
        let content =
            "+++\nid = \"abc\"\nname = \"test\"\ndescription = \"A TOML skill\"\n+++\n\n# Test";
        assert_eq!(extract_description(content), Some("A TOML skill"));
    }

    #[test]
    fn extract_trusted_paths_yaml() {
        let content = "---\nname: test\ntrusted_paths:\n  - ~/Documents/data/\n  - ~/Library/Obsidian/vault/\n---\n\n# Test";
        let paths = extract_trusted_paths(content);
        assert_eq!(paths, vec!["~/Documents/data/", "~/Library/Obsidian/vault/"]);
    }

    #[test]
    fn extract_trusted_paths_toml() {
        let content = "+++\nname = \"test\"\ntrusted_paths = [\"~/Documents/data/\", \"~/vault/\"]\n+++\n\n# Test";
        let paths = extract_trusted_paths(content);
        assert_eq!(paths, vec!["~/Documents/data/", "~/vault/"]);
    }

    #[test]
    fn extract_trusted_paths_empty_when_absent() {
        let content = "---\nname: test\n---\n\n# No trusted paths";
        let paths = extract_trusted_paths(content);
        assert!(paths.is_empty());
    }

    #[test]
    fn extract_trusted_paths_no_frontmatter() {
        let content = "# No frontmatter at all";
        let paths = extract_trusted_paths(content);
        assert!(paths.is_empty());
    }

    #[test]
    fn extract_trusted_paths_yaml_inline() {
        let content = "---\nname: test\ntrusted_paths: [\"~/a/\", \"~/b/\"]\n---\n\n# Test";
        let paths = extract_trusted_paths(content);
        assert_eq!(paths, vec!["~/a/", "~/b/"]);
    }

    #[test]
    fn collect_trusted_paths_deduplicates() {
        let skills = vec![
            "---\ntrusted_paths:\n  - ~/shared/\n---\n".to_string(),
            "---\ntrusted_paths:\n  - ~/shared/\n  - ~/other/\n---\n".to_string(),
        ];
        let paths = collect_trusted_paths(&skills);
        assert_eq!(paths, vec!["~/other/", "~/shared/"]);
    }

    #[test]
    fn extract_phases_from_numbered_skill() {
        let content = "# My Skill\n\n## Phase 0: Setup\nDo setup.\n\n## Phase 1: Execute\nDo the thing.\n\n## Phase 2: Export\nWrite the file.\n";
        let info = extract_phase_info(content).unwrap();
        assert_eq!(info.total_phases, 3);
        assert_eq!(info.final_phase_number, "2");
        assert!(info.final_phase_label.contains("Export"));
    }

    #[test]
    fn extract_phases_fractional() {
        let content = "## Phase 0: Hard Stops\n\n## Phase 0.5: Amendment Check\n\n## Phase 1: Metadata\n\n## Phase 1.5: Response\n\n## Phase 10: Export\n";
        let info = extract_phase_info(content).unwrap();
        assert_eq!(info.total_phases, 5);
        assert_eq!(info.final_phase_number, "10");
        assert!(info.final_phase_label.contains("Export"));
    }

    #[test]
    fn extract_phases_none_when_no_phases() {
        let content = "# Just a Guide\n\nDo things.\n\n## Setup\nSetup stuff.\n";
        assert!(extract_phase_info(content).is_none());
    }

    #[test]
    fn extract_phases_steps() {
        let content = "## Step 1: First\n\n## Step 2: Second\n\n## Step 3: Third\n";
        let info = extract_phase_info(content).unwrap();
        assert_eq!(info.total_phases, 3);
        assert_eq!(info.final_phase_number, "3");
    }

    #[test]
    fn list_summary_mentions_bundled_skills() {
        let summary = list_summary().unwrap();
        assert!(summary.contains("Bundled skills"));
        assert!(summary.contains("Run `omegon skills install`"));
    }
}
