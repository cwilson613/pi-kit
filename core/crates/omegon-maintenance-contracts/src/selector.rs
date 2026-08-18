use std::str::FromStr;

use crate::{AuthorityKey, ContractError, ContributionKind, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContributionSelector {
    Named { kind: ContributionKind, id: String },
    Opaque(AuthorityKey),
}

impl FromStr for ContributionSelector {
    type Err = ContractError;

    fn from_str(value: &str) -> Result<Self> {
        if let Some(key) = value.strip_prefix("entry:sha256:") {
            return Ok(Self::Opaque(key.parse()?));
        }
        let Some((kind, id)) = value.split_once(':') else {
            return Err(ContractError::InvalidValue(
                "contribution selector requires kind:id".into(),
            ));
        };
        let kind = match kind {
            "extension" => ContributionKind::Extension,
            "plugin" => ContributionKind::Plugin,
            "skill" => ContributionKind::Skill,
            "prompt" => ContributionKind::Prompt,
            "catalog" => ContributionKind::Catalog,
            "workflow" => ContributionKind::Workflow,
            _ => {
                return Err(ContractError::InvalidValue(
                    "unknown contribution kind".into(),
                ));
            }
        };
        if id.is_empty()
            || id.len() > 128
            || !id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
            || !id.as_bytes()[0].is_ascii_alphanumeric()
        {
            return Err(ContractError::InvalidValue(
                "invalid contribution identifier".into(),
            ));
        }
        Ok(Self::Named {
            kind,
            id: id.to_owned(),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListScope {
    User,
    Project,
    UserAndProject,
}

pub fn resolve_list_scope(scope: Option<&str>, has_workspace: bool) -> Result<ListScope> {
    match (scope, has_workspace) {
        (None, false) | (Some("user"), false) => Ok(ListScope::User),
        (None, true) => Ok(ListScope::UserAndProject),
        (Some("project"), true) => Ok(ListScope::Project),
        (Some("user"), true) => Err(ContractError::InvalidValue(
            "--scope user cannot be combined with --workspace".into(),
        )),
        (Some("project"), false) => Err(ContractError::InvalidValue(
            "--scope project requires --workspace".into(),
        )),
        (Some(_), _) => Err(ContractError::InvalidValue(
            "scope must be user or project".into(),
        )),
    }
}
