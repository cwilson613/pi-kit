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
            .map_or(false, |d| d.join(name).join("SKILL.md").exists());
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
                if rest.starts_with('"') && rest.len() > 1 {
                    if let Some(end) = rest[1..].find('"') {
                        return Some(&rest[1..1 + end]);
                    }
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
    fn list_summary_mentions_bundled_skills() {
        let summary = list_summary().unwrap();
        assert!(summary.contains("Bundled skills"));
        assert!(summary.contains("Run `omegon skills install`"));
    }
}
