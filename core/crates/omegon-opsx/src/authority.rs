//! Canonical lifecycle authority and artifact-health contracts.
//!
//! Git-native design and OpenSpec artifacts own semantic lifecycle content.
//! The opsx state store is an enforcement and audit ledger; it must not become
//! a second content authority. These contracts make disagreement explicit
//! instead of collapsing artifact state and ledger state into one enum.

use crate::{ChangeState, NodeState};

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
