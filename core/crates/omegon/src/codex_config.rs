//! Codex–Omegon integration config loader.
//!
//! Detects and loads the integration config from either:
//! - `.codex/omegon-integration.toml` (Codex vault side)
//! - `.omegon/codex.toml` (Omegon project side)
//!
//! The config is optional — if neither file exists, Codex integration
//! is disabled and all vault sync features are skipped.

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Loaded Codex integration configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct CodexIntegration {
    /// Master switch — set to false to disable all Codex integration
    /// even when a vault is detected.
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub vault: VaultBinding,
    #[serde(default)]
    pub memory: MemorySync,
    #[serde(default)]
    pub design_tree: DesignTreeSync,
    #[serde(default)]
    pub agent: Option<AgentSettings>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VaultBinding {
    pub path: String,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MemorySync {
    #[serde(default = "default_true")]
    pub materialize_on_session_end: bool,
    #[serde(default = "default_true")]
    pub import_on_session_start: bool,
    #[serde(default = "default_true")]
    pub reinforce_references: bool,
    #[serde(default = "default_max_episodes")]
    pub max_episodes: usize,
}

impl Default for MemorySync {
    fn default() -> Self {
        Self {
            materialize_on_session_end: true,
            import_on_session_start: true,
            reinforce_references: true,
            max_episodes: 20,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct DesignTreeSync {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_vault_subdir")]
    pub vault_subdir: String,
}

impl Default for DesignTreeSync {
    fn default() -> Self {
        Self {
            enabled: true,
            vault_subdir: "design".into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentSettings {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub thinking_level: Option<String>,
    #[serde(default)]
    pub posture: Option<String>,
    #[serde(default)]
    pub max_turns: Option<u32>,
    #[serde(default)]
    pub persona: Option<String>,
}

fn default_true() -> bool {
    true
}
fn default_max_episodes() -> usize {
    20
}
fn default_vault_subdir() -> String {
    "design".into()
}

/// Detect and load the Codex integration config.
///
/// Searches (in order):
/// 1. `{project_root}/.codex/omegon-integration.toml`
/// 2. `{project_root}/.omegon/codex.toml`
///
/// Returns None if neither exists (integration disabled).
pub fn load(project_root: &Path) -> Option<CodexIntegration> {
    let candidates = [
        project_root.join(".codex/omegon-integration.toml"),
        project_root.join(".omegon/codex.toml"),
    ];

    for path in &candidates {
        if path.exists() {
            match std::fs::read_to_string(path) {
                Ok(content) => match toml::from_str::<CodexIntegration>(&content) {
                    Ok(config) => {
                        if !config.enabled {
                            tracing::info!(
                                path = %path.display(),
                                "Codex integration explicitly disabled"
                            );
                            return None;
                        }
                        tracing::info!(
                            path = %path.display(),
                            vault = %config.vault.path,
                            "Codex integration config loaded"
                        );
                        return Some(config);
                    }
                    Err(e) => {
                        tracing::warn!(
                            path = %path.display(),
                            error = %e,
                            "invalid Codex integration config — skipping"
                        );
                    }
                },
                Err(e) => {
                    tracing::warn!(
                        path = %path.display(),
                        error = %e,
                        "failed to read Codex integration config"
                    );
                }
            }
        }
    }

    // Auto-detect: if .codex/ directory exists with config.toml, treat the
    // project root itself as the vault (common case — vault IS the repo).
    let codex_config = project_root.join(".codex/config.toml");
    if codex_config.exists() {
        tracing::info!(
            "auto-detected Codex vault at project root (no explicit integration config)"
        );
        return Some(CodexIntegration {
            enabled: true,
            vault: VaultBinding {
                path: ".".into(),
                name: None,
            },
            memory: MemorySync::default(),
            design_tree: DesignTreeSync::default(),
            agent: None,
        });
    }

    None
}

/// Resolve the vault path relative to the project root.
pub fn resolve_vault_path(project_root: &Path, config: &CodexIntegration) -> PathBuf {
    let raw = Path::new(&config.vault.path);
    if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        project_root.join(raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_config() {
        let toml = r#"
            [vault]
            path = "/Users/me/vault"
        "#;
        let config: CodexIntegration = toml::from_str(toml).unwrap();
        assert_eq!(config.vault.path, "/Users/me/vault");
        assert!(config.memory.materialize_on_session_end);
        assert!(config.design_tree.enabled);
        assert_eq!(config.design_tree.vault_subdir, "design");
    }

    #[test]
    fn parse_full_config() {
        let toml = r#"
            [vault]
            path = "../my-vault"
            name = "my-vault"

            [memory]
            materialize_on_session_end = true
            import_on_session_start = false
            reinforce_references = true
            max_episodes = 10

            [design_tree]
            enabled = true
            vault_subdir = "eng/design"

            [agent]
            model = "anthropic:claude-opus-4-6"
            posture = "architect"
            max_turns = 100
        "#;
        let config: CodexIntegration = toml::from_str(toml).unwrap();
        assert_eq!(config.vault.name.as_deref(), Some("my-vault"));
        assert!(!config.memory.import_on_session_start);
        assert_eq!(config.memory.max_episodes, 10);
        assert_eq!(config.design_tree.vault_subdir, "eng/design");
        assert_eq!(
            config.agent.as_ref().unwrap().posture.as_deref(),
            Some("architect")
        );
    }

    #[test]
    fn resolve_relative_path() {
        let root = Path::new("/projects/my-repo");
        let config = CodexIntegration {
            enabled: true,
            vault: VaultBinding {
                path: "../my-vault".into(),
                name: None,
            },
            memory: MemorySync::default(),
            design_tree: DesignTreeSync::default(),
            agent: None,
        };
        let resolved = resolve_vault_path(root, &config);
        assert_eq!(resolved, PathBuf::from("/projects/my-repo/../my-vault"));
    }

    #[test]
    fn resolve_absolute_path() {
        let root = Path::new("/projects/my-repo");
        let config = CodexIntegration {
            enabled: true,
            vault: VaultBinding {
                path: "/Users/me/vault".into(),
                name: None,
            },
            memory: MemorySync::default(),
            design_tree: DesignTreeSync::default(),
            agent: None,
        };
        let resolved = resolve_vault_path(root, &config);
        assert_eq!(resolved, PathBuf::from("/Users/me/vault"));
    }

    #[test]
    fn resolve_dot_path() {
        let root = Path::new("/projects/my-repo");
        let config = CodexIntegration {
            enabled: true,
            vault: VaultBinding {
                path: ".".into(),
                name: None,
            },
            memory: MemorySync::default(),
            design_tree: DesignTreeSync::default(),
            agent: None,
        };
        let resolved = resolve_vault_path(root, &config);
        assert_eq!(resolved, PathBuf::from("/projects/my-repo/."));
    }

    #[test]
    fn auto_detect_returns_none_without_codex_dir() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(load(tmp.path()).is_none());
    }

    #[test]
    fn auto_detect_finds_codex_vault() {
        let tmp = tempfile::tempdir().unwrap();
        let codex_dir = tmp.path().join(".codex");
        std::fs::create_dir_all(&codex_dir).unwrap();
        std::fs::write(
            codex_dir.join("config.toml"),
            "vault_name = \"test\"\n[sync]\nbackend = \"none\"\n",
        )
        .unwrap();

        let config = load(tmp.path()).expect("should auto-detect");
        assert_eq!(config.vault.path, ".");
    }
}
