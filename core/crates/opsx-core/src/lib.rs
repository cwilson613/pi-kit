//! opsx-core — OpenSpec lifecycle FSM.
//!
//! Enforced state transitions for design nodes, OpenSpec changes,
//! and release milestones. JSON file state store for git-native
//! persistence (jj/git IS the transaction log).

pub mod error;
pub mod fsm;
pub mod store;
pub mod types;

// Re-exports for convenience
pub use error::OpsxError;
pub use fsm::Lifecycle;
pub use store::{JsonFileStore, LifecycleState, MemoryStore, StateStore};
pub use types::{
    Change, ChangeState, Decision, DecisionStatus, DesignNode, IssueType, Milestone,
    MilestoneState, NodeState, Priority,
};
