use std::path::{Path, PathBuf};

use super::{SkillManifest, parse_skill_file};
use crate::contribution::{
    ContributionDeclaration, ContributionFormat, ContributionKind, InstructionScope, SourceAdapter,
};

#[derive(Debug, Clone)]
pub struct AdaptedSkill {
    pub manifest: SkillManifest,
    pub body: String,
    pub path: PathBuf,
    pub source_adapter: SourceAdapter,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdaptedInstructions {
    pub content: String,
    pub path: PathBuf,
    pub scope: InstructionScope,
    pub source_adapter: SourceAdapter,
}

#[derive(Debug, Clone)]
pub enum AdaptedContribution {
    Skill(Box<AdaptedSkill>),
    Instructions(AdaptedInstructions),
}

impl AdaptedContribution {
    pub fn adapt(declaration: &ContributionDeclaration, pack_root: &Path) -> anyhow::Result<Self> {
        let resolved = declaration.validate(pack_root)?;
        let source_adapter = declaration
            .source
            .as_ref()
            .map(|source| source.adapter)
            .unwrap_or(SourceAdapter::Native);
        match declaration.format {
            ContributionFormat::AgentSkillsV1 => {
                let path = resolved.join("SKILL.md");
                let content = std::fs::read_to_string(&path)?;
                let (manifest, body) = parse_skill_file(&content);
                if manifest.name.trim().is_empty() || manifest.description.trim().is_empty() {
                    anyhow::bail!(
                        "agentskills-v1 contribution '{}' requires name and description frontmatter",
                        path.display()
                    );
                }
                Ok(Self::Skill(Box::new(AdaptedSkill {
                    manifest,
                    body,
                    path,
                    source_adapter,
                })))
            }
            ContributionFormat::InstructionsV1 => {
                let scope = declaration.scope.ok_or_else(|| {
                    anyhow::anyhow!("instructions-v1 contribution requires explicit scope")
                })?;
                let content = std::fs::read_to_string(&resolved)?;
                Ok(Self::Instructions(AdaptedInstructions {
                    content,
                    path: resolved,
                    scope,
                    source_adapter,
                }))
            }
        }
    }

    pub fn kind(&self) -> ContributionKind {
        match self {
            Self::Skill(_) => ContributionKind::Skill,
            Self::Instructions(_) => ContributionKind::Instructions,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contribution::{ContributionSource, SourceAdapter};

    #[test]
    fn ambient_instructions_cannot_be_projected_as_selective_skill() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("CLAUDE.md"), "Always use bounded reads.").unwrap();
        let declaration = ContributionDeclaration {
            kind: ContributionKind::Instructions,
            format: ContributionFormat::InstructionsV1,
            path: "CLAUDE.md".into(),
            scope: Some(InstructionScope::Project),
            source: Some(ContributionSource {
                adapter: SourceAdapter::ClaudeInstructions,
                original_path: Some("CLAUDE.md".into()),
            }),
        };

        let adapted = AdaptedContribution::adapt(&declaration, root.path()).unwrap();
        assert_eq!(adapted.kind(), ContributionKind::Instructions);
        let AdaptedContribution::Instructions(instructions) = adapted else {
            panic!("ambient instructions were conflated with a skill")
        };
        assert_eq!(instructions.scope, InstructionScope::Project);
        assert_eq!(
            instructions.source_adapter,
            SourceAdapter::ClaudeInstructions
        );
    }

    #[test]
    fn cross_agent_skill_preserves_selective_skill_semantics() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("review")).unwrap();
        std::fs::write(
            root.path().join("review/SKILL.md"),
            "---\nname: review\ndescription: Review changes\n---\n\nInspect the diff.",
        )
        .unwrap();
        let declaration = ContributionDeclaration {
            kind: ContributionKind::Skill,
            format: ContributionFormat::AgentSkillsV1,
            path: "review".into(),
            scope: None,
            source: Some(ContributionSource {
                adapter: SourceAdapter::CodexSkills,
                original_path: Some(".codex/skills/review".into()),
            }),
        };

        let adapted = AdaptedContribution::adapt(&declaration, root.path()).unwrap();
        let AdaptedContribution::Skill(skill) = adapted else {
            panic!("skill was projected as ambient instructions")
        };
        assert_eq!(skill.manifest.name, "review");
        assert_eq!(skill.source_adapter, SourceAdapter::CodexSkills);
        assert!(skill.body.contains("Inspect the diff"));
    }
}
