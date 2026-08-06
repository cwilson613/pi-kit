//! omegon-opsx — OpenSpec lifecycle FSM.
//!
//! Enforced state transitions for design nodes, OpenSpec changes,
//! and release milestones. JSON file state store for git-native
//! persistence (jj/git IS the transaction log).

pub mod archive;
pub mod artifacts;
pub mod authority;
pub mod content;
mod error;
pub mod fsm;
pub mod store;
pub mod types;

// Re-exports for convenience
pub use artifacts::{
    ChangeArtifactRecord, OpenSpecRepository, TaskCheckboxStatus, TaskStableIdFinding,
    TaskStableIdValidationReport, TaskWriteReport,
};
pub use authority::{
    ArtifactAuthority, ArtifactDrift, ArtifactDriftKind, ArtifactHealth, ArtifactState,
    ChangeArtifactEvidence, parse_declared_change_state,
};
pub use content::{
    Requirement, Scenario, SpecFile, TaskGroup, TaskLine, parse_spec_content,
    parse_spec_content_with_domain, parse_specs_dir, parse_task_groups, parse_task_groups_content,
    parse_task_stable_id_marker,
};
pub use error::OpsxError;
pub use fsm::Lifecycle;
pub use store::{JsonFileStore, LifecycleState, MemoryStore, StateStore};
pub use types::{
    Change, ChangeState, Decision, DecisionStatus, DesignNode, IssueType, Milestone,
    MilestoneState, NodeState, Priority,
};
