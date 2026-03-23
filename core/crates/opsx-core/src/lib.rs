//! opsx-core — OpenSpec lifecycle FSM.
//!
//! Enforced state transitions for design nodes, OpenSpec changes,
//! and release milestones. JSON file state store for git-native
//! persistence (jj/git IS the transaction log).

pub mod types;
pub mod error;
pub mod store;
pub mod fsm;

// Re-exports for convenience
pub use types::{
    NodeState, ChangeState, MilestoneState,
    DesignNode, Change, Milestone, Decision, DecisionStatus,
    Priority, IssueType,
};
pub use error::OpsxError;
pub use store::{StateStore, JsonFileStore, MemoryStore, LifecycleState};
pub use fsm::Lifecycle;
