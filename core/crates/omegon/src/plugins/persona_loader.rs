//! Persona and tone loader — scan installed plugins and build
//! `LoadedPersona` / `LoadedTone` instances for the PluginRegistry.

use std::path::{Path, PathBuf};

use super::armory::{ArmoryManifest, PluginType};
use super::registry::{LoadedPersona, LoadedTone, MindFact, ToneIntensity};

/// A discovered persona or tone available for activation.
#[derive(Debug, Clone)]
pub struct AvailablePlugin {
    pub id: String,
    pub name: String,
    pub plugin_type: PluginType,
    pub description: String,
    pub path: PathBuf,
}

/// Scan installed plugins and return available personas and tones.
pub fn scan_available() -> (Vec<AvailablePlugin>, Vec<AvailablePlugin>) {
    let mut personas = Vec::new();
    let mut tones = Vec::new();

    for dir in super::plugin_search_paths() {
        if !dir.is_dir() {
            continue;
        }

        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let plugin_dir = entry.path();
            // Follow symlinks
            let resolved = if plugin_dir.is_symlink() {
                std::fs::read_link(&plugin_dir).unwrap_or(plugin_dir.clone())
            } else {
                plugin_dir.clone()
            };

            let manifest_path = resolved.join("plugin.toml");
            if !manifest_path.exists() {
                continue;
            }

            let content = match std::fs::read_to_string(&manifest_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let manifest = match ArmoryManifest::parse(&content) {
                Ok(m) => m,
                Err(_) => continue,
            };

            let info = AvailablePlugin {
                id: manifest.plugin.id.clone(),
                name: manifest.plugin.name.clone(),
                plugin_type: manifest.plugin.plugin_type,
                description: manifest.plugin.description.clone(),
                path: resolved,
            };

            match manifest.plugin.plugin_type {
                PluginType::Persona => personas.push(info),
                PluginType::Tone => tones.push(info),
                _ => {} // skills and extensions don't go in persona/tone lists
            }
        }
    }

    (personas, tones)
}

/// Load a persona from its plugin directory into a `LoadedPersona`.
pub fn load_persona(plugin_dir: &Path) -> anyhow::Result<LoadedPersona> {
    let content = std::fs::read_to_string(plugin_dir.join("plugin.toml"))?;
    let manifest = ArmoryManifest::parse(&content)?;

    if manifest.plugin.plugin_type != PluginType::Persona {
        anyhow::bail!(
            "plugin '{}' is not a persona (type: {})",
            manifest.plugin.name,
            manifest.plugin.plugin_type
        );
    }

    let persona_config = manifest.persona.ok_or_else(|| {
        anyhow::anyhow!(
            "persona plugin '{}' has no [persona] section",
            manifest.plugin.name
        )
    })?;

    // Load directive (PERSONA.md)
    let directive = if let Some(ref identity) = persona_config.identity {
        std::fs::read_to_string(plugin_dir.join(&identity.directive))
            .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", identity.directive))?
    } else {
        String::new()
    };

    // Load seed facts
    let mind_facts = if let Some(ref mind) = persona_config.mind {
        if let Some(ref seed_path) = mind.seed_facts {
            load_mind_facts(&plugin_dir.join(seed_path))?
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    // Skills
    let activated_skills = persona_config
        .skills
        .as_ref()
        .map(|s| s.activate.clone())
        .unwrap_or_default();

    // Tool overrides
    let disabled_tools = persona_config
        .tools
        .as_ref()
        .map(|t| t.disable.clone())
        .unwrap_or_default();

    // Badge
    let badge = persona_config.style.as_ref().and_then(|s| s.badge.clone());

    Ok(LoadedPersona {
        id: manifest.plugin.id,
        name: manifest.plugin.name,
        directive,
        mind_facts,
        activated_skills,
        disabled_tools,
        badge,
    })
}

/// Load a tone from its plugin directory into a `LoadedTone`.
pub fn load_tone(plugin_dir: &Path) -> anyhow::Result<LoadedTone> {
    let content = std::fs::read_to_string(plugin_dir.join("plugin.toml"))?;
    let manifest = ArmoryManifest::parse(&content)?;

    if manifest.plugin.plugin_type != PluginType::Tone {
        anyhow::bail!(
            "plugin '{}' is not a tone (type: {})",
            manifest.plugin.name,
            manifest.plugin.plugin_type
        );
    }

    let tone_config = manifest.tone.ok_or_else(|| {
        anyhow::anyhow!(
            "tone plugin '{}' has no [tone] section",
            manifest.plugin.name
        )
    })?;

    // Load directive (TONE.md)
    let directive = std::fs::read_to_string(plugin_dir.join(&tone_config.directive))
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", tone_config.directive))?;

    // Load exemplars
    let exemplars = if let Some(ref exemplar_dir) = tone_config.exemplars {
        load_exemplars(&plugin_dir.join(exemplar_dir))?
    } else {
        vec![]
    };

    // Intensity
    let intensity = tone_config
        .intensity
        .map(|i| ToneIntensity {
            design: i.design,
            coding: i.coding,
        })
        .unwrap_or_default();

    Ok(LoadedTone {
        id: manifest.plugin.id,
        name: manifest.plugin.name,
        directive,
        exemplars,
        intensity,
    })
}

/// Load mind facts from a JSONL file.
fn load_mind_facts(path: &Path) -> anyhow::Result<Vec<MindFact>> {
    if !path.exists() {
        return Ok(vec![]);
    }
    let content = std::fs::read_to_string(path)?;
    let mut facts = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match serde_json::from_str::<MindFact>(line) {
            Ok(fact) => facts.push(fact),
            Err(e) => tracing::warn!(line = line, error = %e, "skipping invalid mind fact"),
        }
    }
    Ok(facts)
}

/// Load exemplar markdown files from a directory.
fn load_exemplars(dir: &Path) -> anyhow::Result<Vec<String>> {
    if !dir.is_dir() {
        return Ok(vec![]);
    }
    let mut exemplars = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        if let Ok(content) = std::fs::read_to_string(entry.path()) {
            exemplars.push(content);
        }
    }
    Ok(exemplars)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_persona_from_directory() {
        let dir = tempfile::tempdir().unwrap();

        std::fs::write(dir.path().join("PERSONA.md"), "You are a test persona.\n").unwrap();

        let mind_dir = dir.path().join("mind");
        std::fs::create_dir_all(&mind_dir).unwrap();
        std::fs::write(
            mind_dir.join("facts.jsonl"),
            r#"{"section":"Architecture","content":"test fact","confidence":1.0}
{"section":"Decisions","content":"another fact","confidence":0.9,"tags":["test"]}
"#,
        )
        .unwrap();

        std::fs::write(
            dir.path().join("plugin.toml"),
            r#"
[plugin]
type = "persona"
id = "dev.test.tester"
name = "Test Persona"
version = "1.0.0"
description = "A test"

[persona.identity]
directive = "PERSONA.md"

[persona.mind]
seed_facts = "mind/facts.jsonl"

[persona.skills]
activate = ["rust", "testing"]

[persona.style]
badge = "🧪"
"#,
        )
        .unwrap();

        let persona = load_persona(dir.path()).unwrap();
        assert_eq!(persona.id, "dev.test.tester");
        assert_eq!(persona.name, "Test Persona");
        assert!(persona.directive.contains("test persona"));
        assert_eq!(persona.mind_facts.len(), 2);
        assert_eq!(persona.mind_facts[1].tags, vec!["test"]);
        assert_eq!(persona.activated_skills, vec!["rust", "testing"]);
        assert_eq!(persona.badge, Some("🧪".into()));
    }

    #[test]
    fn load_tone_from_directory() {
        let dir = tempfile::tempdir().unwrap();

        std::fs::write(dir.path().join("TONE.md"), "Speak concisely.\n").unwrap();

        let exemplar_dir = dir.path().join("exemplars");
        std::fs::create_dir_all(&exemplar_dir).unwrap();
        std::fs::write(exemplar_dir.join("01-brevity.md"), "Short and sharp.\n").unwrap();
        std::fs::write(exemplar_dir.join("02-clarity.md"), "Clear, not clever.\n").unwrap();

        std::fs::write(
            dir.path().join("plugin.toml"),
            r#"
[plugin]
type = "tone"
id = "dev.test.concise"
name = "Concise"
version = "1.0.0"
description = "Brevity tone"

[tone]
directive = "TONE.md"
exemplars = "exemplars"

[tone.intensity]
design = "full"
coding = "muted"
"#,
        )
        .unwrap();

        let tone = load_tone(dir.path()).unwrap();
        assert_eq!(tone.id, "dev.test.concise");
        assert_eq!(tone.name, "Concise");
        assert!(tone.directive.contains("concisely"));
        assert_eq!(tone.exemplars.len(), 2);
        assert_eq!(tone.intensity.design, "full");
        assert_eq!(tone.intensity.coding, "muted");
    }

    #[test]
    fn load_persona_wrong_type() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("plugin.toml"),
            r#"
[plugin]
type = "tone"
id = "dev.test.not-persona"
name = "Not A Persona"
version = "1.0.0"
description = "wrong type"

[tone]
directive = "TONE.md"
"#,
        )
        .unwrap();

        let result = load_persona(dir.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not a persona"));
    }

    #[test]
    fn load_mind_facts_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("facts.jsonl"), "").unwrap();
        let facts = load_mind_facts(&dir.path().join("facts.jsonl")).unwrap();
        assert!(facts.is_empty());
    }

    #[test]
    fn load_mind_facts_with_comments() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("facts.jsonl"),
            "# This is a comment\n{\"section\":\"Architecture\",\"content\":\"real fact\",\"confidence\":1.0}\n\n"
        ).unwrap();
        let facts = load_mind_facts(&dir.path().join("facts.jsonl")).unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].content, "real fact");
    }

    #[test]
    fn load_mind_facts_missing_file() {
        let facts = load_mind_facts(Path::new("/nonexistent/facts.jsonl")).unwrap();
        assert!(facts.is_empty());
    }
}
