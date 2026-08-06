//! Canonical lifecycle authority and artifact-health contracts.
//!
//! Git-native design and OpenSpec artifacts own semantic lifecycle content.
//! The opsx state store is an enforcement and audit ledger; it must not become
//! a second content authority. These contracts make disagreement explicit
//! instead of collapsing artifact state and ledger state into one enum.

use crate::{ChangeState, NodeState};
use std::collections::BTreeSet;

/// Declares which representation owns lifecycle content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactAuthority {
    /// Markdown/frontmatter and OpenSpec directory artifacts are canonical.
    GitNativeArtifacts,
}

impl ArtifactAuthority {
    /// The authority selected for the current opsx architecture.
    pub const CANONICAL: Self = Self::GitNativeArtifacts;
}

/// Semantic state derived from a canonical lifecycle artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactState {
    Node(NodeState),
    Change(ChangeState),
}

/// Structural health of the canonical artifact representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactHealth {
    Healthy,
    /// The artifact is readable but lacks structure required for its state.
    Incomplete {
        missing: Vec<String>,
    },
    /// The artifact cannot be interpreted safely.
    Malformed {
        detail: String,
    },
}

impl ArtifactHealth {
    pub fn is_healthy(&self) -> bool {
        matches!(self, Self::Healthy)
    }
}

/// Renderer-neutral evidence discovered from one OpenSpec change directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChangeArtifactEvidence {
    pub has_proposal: bool,
    pub has_design: bool,
    pub has_specs: bool,
    pub has_tasks: bool,
    pub total_tasks: usize,
    pub done_tasks: usize,
    pub has_registered_tests: bool,
}

impl ChangeArtifactEvidence {
    /// Derive semantic lifecycle state from git-native artifact evidence.
    ///
    /// `declared_state` is explicit artifact metadata and therefore takes
    /// precedence over structural inference. This is how canonical-only states
    /// such as `testing` and `abandoned` remain representable.
    pub fn derive_state(self, declared_state: Option<ChangeState>) -> ChangeState {
        if let Some(state) = declared_state {
            return state;
        }
        if self.has_tasks && self.total_tasks > 0 && self.done_tasks >= self.total_tasks {
            return ChangeState::Verifying;
        }
        if self.has_registered_tests {
            return ChangeState::Implementing;
        }
        if self.has_tasks {
            return ChangeState::Planned;
        }
        if self.has_specs {
            return ChangeState::Specced;
        }
        ChangeState::Proposed
    }

    /// Assess missing structure required by the derived semantic state.
    pub fn assess_health(self, state: ChangeState) -> ArtifactHealth {
        let mut missing = BTreeSet::new();
        if !self.has_proposal {
            missing.insert("proposal.md".to_string());
        }
        match state {
            ChangeState::Proposed | ChangeState::Abandoned => {}
            ChangeState::Specced | ChangeState::Testing => {
                if !self.has_specs {
                    missing.insert("specs".to_string());
                }
            }
            ChangeState::Planned | ChangeState::Implementing | ChangeState::Verifying => {
                if !self.has_specs {
                    missing.insert("specs".to_string());
                }
                if !self.has_design {
                    missing.insert("design.md".to_string());
                }
                if !self.has_tasks {
                    missing.insert("tasks.md".to_string());
                }
            }
            ChangeState::Archived => {}
        }
        if missing.is_empty() {
            ArtifactHealth::Healthy
        } else {
            ArtifactHealth::Incomplete {
                missing: missing.into_iter().collect(),
            }
        }
    }
}

/// Parse explicit canonical change-state metadata without accepting legacy
/// aliases such as `specified`.
pub fn parse_declared_change_state(value: &str) -> Result<ChangeState, String> {
    ChangeState::parse(value).ok_or_else(|| format!("unknown change state: {value}"))
}

/// Why artifact-derived state and the opsx enforcement ledger disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactDriftKind {
    MissingLedgerRecord,
    StateMismatch,
    LedgerRecordWithoutArtifact,
}

/// A typed lifecycle drift finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactDrift {
    pub entity_id: String,
    pub artifact_state: Option<ArtifactState>,
    pub ledger_state: Option<ArtifactState>,
    pub kind: ArtifactDriftKind,
}

impl ArtifactDrift {
    /// Compare canonical artifact state with an optional ledger state.
    pub fn compare(
        entity_id: impl Into<String>,
        artifact_state: ArtifactState,
        ledger_state: Option<ArtifactState>,
    ) -> Option<Self> {
        let kind = match ledger_state {
            None => ArtifactDriftKind::MissingLedgerRecord,
            Some(state) if state != artifact_state => ArtifactDriftKind::StateMismatch,
            Some(_) => return None,
        };
        Some(Self {
            entity_id: entity_id.into(),
            artifact_state: Some(artifact_state),
            ledger_state,
            kind,
        })
    }

    /// Report a ledger record whose canonical artifact no longer exists.
    pub fn ledger_without_artifact(
        entity_id: impl Into<String>,
        ledger_state: ArtifactState,
    ) -> Self {
        Self {
            entity_id: entity_id.into(),
            artifact_state: None,
            ledger_state: Some(ledger_state),
            kind: ArtifactDriftKind::LedgerRecordWithoutArtifact,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_native_artifacts_are_canonical() {
        assert_eq!(
            ArtifactAuthority::CANONICAL,
            ArtifactAuthority::GitNativeArtifacts
        );
    }

    #[test]
    fn artifact_state_keeps_testing_and_abandoned_semantic() {
        assert_eq!(
            ArtifactState::Change(ChangeState::Testing),
            ArtifactState::Change(ChangeState::Testing)
        );
        assert_eq!(
            ArtifactState::Change(ChangeState::Abandoned),
            ArtifactState::Change(ChangeState::Abandoned)
        );
    }

    #[test]
    fn artifact_health_is_separate_from_semantic_state() {
        let health = ArtifactHealth::Incomplete {
            missing: vec!["tasks.md".into()],
        };
        assert!(!health.is_healthy());
        assert_eq!(
            ArtifactState::Change(ChangeState::Planned),
            ArtifactState::Change(ChangeState::Planned)
        );
    }

    #[test]
    fn explicit_testing_and_abandoned_override_structural_inference() {
        let evidence = ChangeArtifactEvidence {
            has_proposal: true,
            has_specs: true,
            has_tasks: true,
            total_tasks: 3,
            done_tasks: 3,
            ..Default::default()
        };
        assert_eq!(
            evidence.derive_state(Some(ChangeState::Testing)),
            ChangeState::Testing
        );
        assert_eq!(
            evidence.derive_state(Some(ChangeState::Abandoned)),
            ChangeState::Abandoned
        );
    }

    #[test]
    fn structural_evidence_derives_legacy_change_progression() {
        assert_eq!(
            ChangeArtifactEvidence::default().derive_state(None),
            ChangeState::Proposed
        );
        assert_eq!(
            ChangeArtifactEvidence {
                has_specs: true,
                ..Default::default()
            }
            .derive_state(None),
            ChangeState::Specced
        );
        assert_eq!(
            ChangeArtifactEvidence {
                has_tasks: true,
                ..Default::default()
            }
            .derive_state(None),
            ChangeState::Planned
        );
        assert_eq!(
            ChangeArtifactEvidence {
                has_registered_tests: true,
                ..Default::default()
            }
            .derive_state(None),
            ChangeState::Implementing
        );
        assert_eq!(
            ChangeArtifactEvidence {
                has_tasks: true,
                total_tasks: 2,
                done_tasks: 2,
                ..Default::default()
            }
            .derive_state(None),
            ChangeState::Verifying
        );
    }

    #[test]
    fn health_reports_state_required_structure_separately() {
        let health = ChangeArtifactEvidence {
            has_proposal: true,
            has_specs: true,
            ..Default::default()
        }
        .assess_health(ChangeState::Planned);
        assert_eq!(
            health,
            ArtifactHealth::Incomplete {
                missing: vec!["design.md".into(), "tasks.md".into()]
            }
        );
    }

    #[test]
    fn declared_state_parser_rejects_legacy_alias() {
        assert_eq!(
            parse_declared_change_state("testing"),
            Ok(ChangeState::Testing)
        );
        assert!(parse_declared_change_state("specified").is_err());
    }

    #[test]
    fn matching_artifact_and_ledger_have_no_drift() {
        assert_eq!(
            ArtifactDrift::compare(
                "change-a",
                ArtifactState::Change(ChangeState::Testing),
                Some(ArtifactState::Change(ChangeState::Testing)),
            ),
            None
        );
    }

    #[test]
    fn missing_and_mismatched_ledger_state_are_typed() {
        let missing = ArtifactDrift::compare(
            "change-a",
            ArtifactState::Change(ChangeState::Testing),
            None,
        )
        .unwrap();
        assert_eq!(missing.kind, ArtifactDriftKind::MissingLedgerRecord);

        let mismatch = ArtifactDrift::compare(
            "change-a",
            ArtifactState::Change(ChangeState::Testing),
            Some(ArtifactState::Change(ChangeState::Planned)),
        )
        .unwrap();
        assert_eq!(mismatch.kind, ArtifactDriftKind::StateMismatch);
    }
}
