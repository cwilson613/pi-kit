use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Versioned contribution formats understood by the portable pack loader.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ContributionFormat {
    AgentSkillsV1,
    InstructionsV1,
}

/// Semantic contribution kind. Instructions remain ambient policy and are not
/// silently promoted into selectively disclosed skills.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContributionKind {
    Skill,
    Instructions,
}

/// Scope for ambient instruction contributions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstructionScope {
    User,
    Project,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContributionPackManifest {
    pub schema: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub contributions: Vec<ContributionDeclaration>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContributionDeclaration {
    pub kind: ContributionKind,
    pub format: ContributionFormat,
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<InstructionScope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<ContributionSource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContributionSource {
    pub adapter: SourceAdapter,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceAdapter {
    Native,
    ClaudeSkills,
    CodexSkills,
    ClaudeInstructions,
    CodexInstructions,
    CursorRules,
    CopilotInstructions,
    WindsurfRules,
    ClineRules,
}

impl ContributionDeclaration {
    pub fn validate(&self, pack_root: &Path) -> anyhow::Result<PathBuf> {
        let expected_kind = match self.format {
            ContributionFormat::AgentSkillsV1 => ContributionKind::Skill,
            ContributionFormat::InstructionsV1 => ContributionKind::Instructions,
        };
        if self.kind != expected_kind {
            anyhow::bail!(
                "contribution kind {:?} is incompatible with format {:?}",
                self.kind,
                self.format
            );
        }
        if self.kind == ContributionKind::Skill && self.scope.is_some() {
            anyhow::bail!("skill contributions cannot declare ambient instruction scope");
        }
        if self.kind == ContributionKind::Instructions && self.scope.is_none() {
            anyhow::bail!("instruction contributions require an explicit scope");
        }
        if self.path.is_absolute()
            || self
                .path
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            anyhow::bail!("contribution path must remain relative to the pack root");
        }
        let root = pack_root.canonicalize()?;
        let resolved = root.join(&self.path).canonicalize()?;
        if !resolved.starts_with(&root) {
            anyhow::bail!("contribution path escapes the pack root");
        }
        match self.format {
            ContributionFormat::AgentSkillsV1 if !resolved.join("SKILL.md").is_file() => {
                anyhow::bail!("agentskills-v1 contribution must contain SKILL.md")
            }
            ContributionFormat::InstructionsV1 if !resolved.is_file() => {
                anyhow::bail!("instructions-v1 contribution must be a regular file")
            }
            _ => {}
        }
        Ok(resolved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_and_instruction_semantics_cannot_be_conflated() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("skill")).unwrap();
        std::fs::write(
            root.path().join("skill/SKILL.md"),
            "---\nname: x\ndescription: x\n---",
        )
        .unwrap();
        let declaration = ContributionDeclaration {
            kind: ContributionKind::Instructions,
            format: ContributionFormat::AgentSkillsV1,
            path: "skill".into(),
            scope: Some(InstructionScope::Project),
            source: None,
        };
        assert!(declaration.validate(root.path()).is_err());
    }

    #[test]
    fn relative_paths_are_contained_and_formats_have_expected_shape() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("skills/rust")).unwrap();
        std::fs::write(root.path().join("skills/rust/SKILL.md"), "body").unwrap();
        let declaration = ContributionDeclaration {
            kind: ContributionKind::Skill,
            format: ContributionFormat::AgentSkillsV1,
            path: "skills/rust".into(),
            scope: None,
            source: Some(ContributionSource {
                adapter: SourceAdapter::ClaudeSkills,
                original_path: Some(".claude/skills/rust".into()),
            }),
        };
        assert!(declaration.validate(root.path()).is_ok());

        let escaping = ContributionDeclaration {
            path: "../escape".into(),
            ..declaration
        };
        assert!(escaping.validate(root.path()).is_err());
    }
}
