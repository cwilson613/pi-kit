//! Lifecycle FSM — enforced state transitions with operator escape hatches.
//!
//! Every state change goes through the FSM. Invalid transitions return
//! `OpsxError::InvalidTransition`. When the FSM is wrong for a specific
//! situation, `force_transition` bypasses validation but logs the override
//! in the audit trail with a mandatory reason.
//!
//! The `state` field is private — all mutations go through methods.

use crate::types::*;
use crate::store::{LifecycleState, StateStore, SCHEMA_VERSION};
use crate::error::OpsxError;

/// The lifecycle engine — validates transitions and mutates state.
pub struct Lifecycle<S: StateStore> {
    store: S,
    /// Private — all access through methods. No direct mutation.
    state: LifecycleState,
}

impl<S: StateStore> Lifecycle<S> {
    /// Load or initialize the lifecycle from the store.
    pub fn load(store: S) -> Result<Self, OpsxError> {
        let state = store.load()?;
        Ok(Self { store, state })
    }

    /// Persist the current state to the store.
    fn save(&self) -> Result<(), OpsxError> {
        self.store.save(&self.state)
    }

    /// Get the current state (read-only).
    pub fn state(&self) -> &LifecycleState {
        &self.state
    }

    /// Append an audit entry and save.
    fn audit_and_save(
        &mut self,
        entity_type: &str,
        entity_id: &str,
        from: &str,
        to: &str,
        reason: Option<&str>,
        forced: bool,
    ) -> Result<(), OpsxError> {
        self.state.audit_log.push(AuditEntry {
            timestamp: iso_now(),
            entity_type: entity_type.into(),
            entity_id: entity_id.into(),
            from_state: from.into(),
            to_state: to.into(),
            reason: reason.map(|s| s.into()),
            forced,
        });
        self.save()
    }

    // ─── Design node operations ─────────────────────────────────────

    /// Create a new design node.
    pub fn create_node(&mut self, id: &str, title: &str, parent: Option<&str>) -> Result<&DesignNode, OpsxError> {
        if self.state.nodes.iter().any(|n| n.id == id) {
            return Err(OpsxError::AlreadyExists(format!("node '{id}'")));
        }
        let now = iso_now();
        self.state.nodes.push(DesignNode {
            id: id.into(),
            title: title.into(),
            state: NodeState::Seed,
            parent: parent.map(|s| s.into()),
            tags: vec![],
            priority: None,
            issue_type: None,
            open_questions: vec![],
            decisions: vec![],
            overview: String::new(),
            bound_change: None,
            created_at: now.clone(),
            updated_at: now,
        });
        self.audit_and_save("node", id, "(new)", "seed", None, false)?;
        Ok(self.state.nodes.last().unwrap())
    }

    /// Transition a design node to a new state (FSM-validated).
    pub fn transition_node(&mut self, id: &str, target: NodeState) -> Result<(), OpsxError> {
        let node = self.state.nodes.iter().find(|n| n.id == id)
            .ok_or_else(|| OpsxError::NotFound(format!("node '{id}'")))?;

        let from = node.state;

        if !from.can_transition_to(target) {
            return Err(OpsxError::InvalidTransition {
                entity: format!("node '{id}'"),
                from: from.as_str().into(),
                to: target.as_str().into(),
            });
        }

        // Enforce preconditions for specific transitions
        match target {
            NodeState::Decided => {
                if !node.open_questions.is_empty() {
                    return Err(OpsxError::PreconditionFailed(
                        format!("node '{}' has {} open questions — resolve before deciding",
                            id, node.open_questions.len())
                    ));
                }
            }
            NodeState::Implementing => {
                // Must come from Decided or Blocked (resume)
                if from != NodeState::Decided && from != NodeState::Blocked {
                    return Err(OpsxError::PreconditionFailed(
                        format!("node '{}' must be decided (or blocked) before implementing", id)
                    ));
                }
            }
            _ => {}
        }

        // Check milestone freeze — only block regression, not forward progress
        for ms in &self.state.milestones {
            if ms.state == MilestoneState::Frozen && ms.nodes.contains(&id.to_string()) {
                if target == NodeState::Exploring || target == NodeState::Seed {
                    return Err(OpsxError::MilestoneFrozen(ms.name.clone()));
                }
            }
        }

        let from_str = from.as_str().to_string();
        let node = self.state.nodes.iter_mut().find(|n| n.id == id).unwrap();
        node.state = target;
        node.updated_at = iso_now();
        self.audit_and_save("node", id, &from_str, target.as_str(), None, false)
    }

    /// 🔓 ESCAPE HATCH: Force a state transition, bypassing all FSM validation.
    ///
    /// Use when the FSM rules are wrong for a specific situation, when state.json
    /// is corrupted, or when an agent botched a transition. The override is logged
    /// in the audit trail with a mandatory reason.
    ///
    /// This is the "break glass" operator override — it should be rare and visible.
    pub fn force_transition_node(&mut self, id: &str, target: NodeState, reason: &str) -> Result<(), OpsxError> {
        let node = self.state.nodes.iter_mut().find(|n| n.id == id)
            .ok_or_else(|| OpsxError::NotFound(format!("node '{id}'")))?;

        let from_str = node.state.as_str().to_string();
        tracing::warn!(
            node_id = id,
            from = from_str,
            to = target.as_str(),
            reason = reason,
            "FORCED state transition — bypassing FSM validation"
        );
        node.state = target;
        node.updated_at = iso_now();
        self.audit_and_save("node", id, &from_str, target.as_str(), Some(reason), true)
    }

    /// Add an open question to a node.
    pub fn add_question(&mut self, id: &str, question: &str) -> Result<(), OpsxError> {
        let node = self.state.nodes.iter_mut().find(|n| n.id == id)
            .ok_or_else(|| OpsxError::NotFound(format!("node '{id}'")))?;
        node.open_questions.push(question.into());
        node.updated_at = iso_now();
        self.save()
    }

    /// Remove an open question from a node.
    pub fn remove_question(&mut self, id: &str, question: &str) -> Result<(), OpsxError> {
        let node = self.state.nodes.iter_mut().find(|n| n.id == id)
            .ok_or_else(|| OpsxError::NotFound(format!("node '{id}'")))?;
        node.open_questions.retain(|q| q != question);
        node.updated_at = iso_now();
        self.save()
    }

    /// Get a node by ID.
    pub fn get_node(&self, id: &str) -> Option<&DesignNode> {
        self.state.nodes.iter().find(|n| n.id == id)
    }

    /// List all nodes.
    pub fn nodes(&self) -> &[DesignNode] {
        &self.state.nodes
    }

    /// Get the audit log.
    pub fn audit_log(&self) -> &[AuditEntry] {
        &self.state.audit_log
    }

    // ─── Change operations ──────────────────────────────────────────

    /// Create a new OpenSpec change.
    pub fn create_change(&mut self, name: &str, title: &str, bound_node: Option<&str>) -> Result<(), OpsxError> {
        if self.state.changes.iter().any(|c| c.name == name) {
            return Err(OpsxError::AlreadyExists(format!("change '{name}'")));
        }
        let now = iso_now();
        self.state.changes.push(Change {
            name: name.into(),
            title: title.into(),
            state: ChangeState::Proposed,
            bound_node: bound_node.map(|s| s.into()),
            specs: vec![],
            tasks_total: 0,
            tasks_done: 0,
            created_at: now.clone(),
            updated_at: now,
        });
        self.audit_and_save("change", name, "(new)", "proposed", None, false)
    }

    /// Transition a change to a new state (FSM-validated).
    pub fn transition_change(&mut self, name: &str, target: ChangeState) -> Result<(), OpsxError> {
        let change = self.state.changes.iter().find(|c| c.name == name)
            .ok_or_else(|| OpsxError::NotFound(format!("change '{name}'")))?;

        let from = change.state;
        if !from.can_transition_to(target) {
            return Err(OpsxError::InvalidTransition {
                entity: format!("change '{name}'"),
                from: from.as_str().into(),
                to: target.as_str().into(),
            });
        }

        let from_str = from.as_str().to_string();
        let change = self.state.changes.iter_mut().find(|c| c.name == name).unwrap();
        change.state = target;
        change.updated_at = iso_now();
        self.audit_and_save("change", name, &from_str, target.as_str(), None, false)
    }

    /// 🔓 ESCAPE HATCH: Force a change state transition.
    pub fn force_transition_change(&mut self, name: &str, target: ChangeState, reason: &str) -> Result<(), OpsxError> {
        let change = self.state.changes.iter_mut().find(|c| c.name == name)
            .ok_or_else(|| OpsxError::NotFound(format!("change '{name}'")))?;

        let from_str = change.state.as_str().to_string();
        tracing::warn!(
            change_name = name,
            from = from_str,
            to = target.as_str(),
            reason = reason,
            "FORCED change transition — bypassing FSM validation"
        );
        change.state = target;
        change.updated_at = iso_now();
        self.audit_and_save("change", name, &from_str, target.as_str(), Some(reason), true)
    }

    // ─── Milestone operations ───────────────────────────────────────

    /// Create a milestone.
    pub fn create_milestone(&mut self, name: &str) -> Result<(), OpsxError> {
        if self.state.milestones.iter().any(|m| m.name == name) {
            return Err(OpsxError::AlreadyExists(format!("milestone '{name}'")));
        }
        let now = iso_now();
        self.state.milestones.push(Milestone {
            name: name.into(),
            state: MilestoneState::Open,
            nodes: vec![],
            created_at: now.clone(),
            updated_at: now,
        });
        self.audit_and_save("milestone", name, "(new)", "open", None, false)
    }

    /// Add a node to a milestone (creates milestone if needed).
    pub fn milestone_add(&mut self, milestone: &str, node_id: &str) -> Result<(), OpsxError> {
        if !self.state.nodes.iter().any(|n| n.id == node_id) {
            return Err(OpsxError::NotFound(format!("node '{node_id}'")));
        }

        if !self.state.milestones.iter().any(|m| m.name == milestone) {
            self.create_milestone(milestone)?;
        }

        let ms = self.state.milestones.iter_mut().find(|m| m.name == milestone).unwrap();

        if ms.state == MilestoneState::Frozen {
            return Err(OpsxError::MilestoneFrozen(milestone.into()));
        }

        if !ms.nodes.contains(&node_id.to_string()) {
            ms.nodes.push(node_id.into());
            ms.updated_at = iso_now();
        }
        self.save()
    }

    /// Remove a node from a milestone.
    pub fn milestone_remove(&mut self, milestone: &str, node_id: &str) -> Result<(), OpsxError> {
        let ms = self.state.milestones.iter_mut().find(|m| m.name == milestone)
            .ok_or_else(|| OpsxError::NotFound(format!("milestone '{milestone}'")))?;
        ms.nodes.retain(|n| n != node_id);
        ms.updated_at = iso_now();
        self.save()
    }

    /// Freeze a milestone.
    pub fn milestone_freeze(&mut self, name: &str) -> Result<(), OpsxError> {
        let ms = self.state.milestones.iter_mut().find(|m| m.name == name)
            .ok_or_else(|| OpsxError::NotFound(format!("milestone '{name}'")))?;
        let from_str = format!("{:?}", ms.state).to_lowercase();
        ms.state = MilestoneState::Frozen;
        ms.updated_at = iso_now();
        self.audit_and_save("milestone", name, &from_str, "frozen", None, false)
    }

    /// Unfreeze a milestone.
    pub fn milestone_unfreeze(&mut self, name: &str) -> Result<(), OpsxError> {
        let ms = self.state.milestones.iter_mut().find(|m| m.name == name)
            .ok_or_else(|| OpsxError::NotFound(format!("milestone '{name}'")))?;
        let from_str = format!("{:?}", ms.state).to_lowercase();
        ms.state = MilestoneState::Open;
        ms.updated_at = iso_now();
        self.audit_and_save("milestone", name, &from_str, "open", None, false)
    }

    /// Get milestone readiness report.
    pub fn milestone_status(&self, name: &str) -> Result<MilestoneStatus, OpsxError> {
        let ms = self.state.milestones.iter().find(|m| m.name == name)
            .ok_or_else(|| OpsxError::NotFound(format!("milestone '{name}'")))?;

        let mut status = MilestoneStatus {
            name: ms.name.clone(),
            state: ms.state,
            total: ms.nodes.len(),
            implemented: 0,
            decided: 0,
            exploring: 0,
            other: 0,
        };

        for node_id in &ms.nodes {
            if let Some(node) = self.state.nodes.iter().find(|n| n.id == *node_id) {
                match node.state {
                    NodeState::Implemented => status.implemented += 1,
                    NodeState::Decided | NodeState::Implementing => status.decided += 1,
                    NodeState::Exploring | NodeState::Resolved => status.exploring += 1,
                    _ => status.other += 1,
                }
            } else {
                status.other += 1;
            }
        }

        Ok(status)
    }

    /// Get all milestones.
    pub fn milestones(&self) -> &[Milestone] {
        &self.state.milestones
    }
}

/// Milestone readiness report.
pub struct MilestoneStatus {
    pub name: String,
    pub state: MilestoneState,
    pub total: usize,
    pub implemented: usize,
    pub decided: usize,
    pub exploring: usize,
    pub other: usize,
}

impl MilestoneStatus {
    pub fn progress_pct(&self) -> usize {
        if self.total == 0 { 0 } else { self.implemented * 100 / self.total }
    }
}

/// ISO 8601 timestamp without external chrono dependency.
fn iso_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Convert epoch seconds to ISO 8601 UTC
    // Days since epoch, accounting for leap years
    let days = (secs / 86400) as i64;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Compute year/month/day from days since 1970-01-01
    let (year, month, day) = days_to_ymd(days);

    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

/// Convert days since 1970-01-01 to (year, month, day).
fn days_to_ymd(days: i64) -> (i64, u32, u32) {
    // Algorithm from Howard Hinnant's date library (public domain)
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::JsonFileStore;
    use tempfile::TempDir;

    fn test_lifecycle() -> (TempDir, Lifecycle<JsonFileStore>) {
        let tmp = TempDir::new().unwrap();
        let store = JsonFileStore::new(tmp.path());
        let lc = Lifecycle::load(store).unwrap();
        (tmp, lc)
    }

    #[test]
    fn create_node() {
        let (_tmp, mut lc) = test_lifecycle();
        lc.create_node("test", "Test Node", None).unwrap();
        assert_eq!(lc.nodes().len(), 1);
        assert_eq!(lc.nodes()[0].state, NodeState::Seed);
        // Audit log should have the creation entry
        assert_eq!(lc.audit_log().len(), 1);
        assert_eq!(lc.audit_log()[0].to_state, "seed");
        assert!(!lc.audit_log()[0].forced);
    }

    #[test]
    fn valid_transition() {
        let (_tmp, mut lc) = test_lifecycle();
        lc.create_node("test", "Test", None).unwrap();
        lc.transition_node("test", NodeState::Exploring).unwrap();
        assert_eq!(lc.get_node("test").unwrap().state, NodeState::Exploring);
        // Should have 2 audit entries: create + transition
        assert_eq!(lc.audit_log().len(), 2);
    }

    #[test]
    fn invalid_transition_rejected() {
        let (_tmp, mut lc) = test_lifecycle();
        lc.create_node("test", "Test", None).unwrap();
        let err = lc.transition_node("test", NodeState::Implemented);
        assert!(err.is_err());
        match err.unwrap_err() {
            OpsxError::InvalidTransition { .. } => {}
            other => panic!("expected InvalidTransition, got {other:?}"),
        }
    }

    #[test]
    fn force_transition_bypasses_fsm() {
        let (_tmp, mut lc) = test_lifecycle();
        lc.create_node("test", "Test", None).unwrap();
        // Seed → Implemented is normally illegal
        lc.force_transition_node("test", NodeState::Implemented, "state.json was corrupted, manually verified implementation").unwrap();
        assert_eq!(lc.get_node("test").unwrap().state, NodeState::Implemented);

        // Verify audit trail records the force
        let last = lc.audit_log().last().unwrap();
        assert!(last.forced);
        assert_eq!(last.reason.as_deref(), Some("state.json was corrupted, manually verified implementation"));
    }

    #[test]
    fn implemented_can_reopen() {
        let (_tmp, mut lc) = test_lifecycle();
        lc.create_node("test", "Test", None).unwrap();
        // Walk to implemented
        lc.transition_node("test", NodeState::Exploring).unwrap();
        lc.transition_node("test", NodeState::Decided).unwrap();
        lc.transition_node("test", NodeState::Implementing).unwrap();
        lc.transition_node("test", NodeState::Implemented).unwrap();
        // Reopen — "implementation was wrong"
        lc.transition_node("test", NodeState::Exploring).unwrap();
        assert_eq!(lc.get_node("test").unwrap().state, NodeState::Exploring);
    }

    #[test]
    fn blocked_can_resume_implementing() {
        let (_tmp, mut lc) = test_lifecycle();
        lc.create_node("test", "Test", None).unwrap();
        lc.transition_node("test", NodeState::Exploring).unwrap();
        lc.transition_node("test", NodeState::Decided).unwrap();
        lc.transition_node("test", NodeState::Implementing).unwrap();
        lc.transition_node("test", NodeState::Blocked).unwrap();
        // Unblock — resume implementing directly
        lc.transition_node("test", NodeState::Implementing).unwrap();
        assert_eq!(lc.get_node("test").unwrap().state, NodeState::Implementing);
    }

    #[test]
    fn decided_requires_no_open_questions() {
        let (_tmp, mut lc) = test_lifecycle();
        lc.create_node("test", "Test", None).unwrap();
        lc.transition_node("test", NodeState::Exploring).unwrap();
        lc.add_question("test", "Unresolved?").unwrap();

        let err = lc.transition_node("test", NodeState::Decided);
        assert!(err.is_err());
        match err.unwrap_err() {
            OpsxError::PreconditionFailed(_) => {}
            other => panic!("expected PreconditionFailed, got {other:?}"),
        }

        // Remove question and try again
        lc.remove_question("test", "Unresolved?").unwrap();
        lc.transition_node("test", NodeState::Decided).unwrap();
        assert_eq!(lc.get_node("test").unwrap().state, NodeState::Decided);
    }

    #[test]
    fn change_lifecycle() {
        let (_tmp, mut lc) = test_lifecycle();
        lc.create_change("my-change", "My Change", None).unwrap();
        lc.transition_change("my-change", ChangeState::Specced).unwrap();
        lc.transition_change("my-change", ChangeState::Planned).unwrap();
        lc.transition_change("my-change", ChangeState::Implementing).unwrap();
        lc.transition_change("my-change", ChangeState::Verifying).unwrap();
        lc.transition_change("my-change", ChangeState::Archived).unwrap();

        let change = lc.state().changes.iter().find(|c| c.name == "my-change").unwrap();
        assert_eq!(change.state, ChangeState::Archived);
    }

    #[test]
    fn change_can_be_abandoned_from_any_active_state() {
        for start_state in [ChangeState::Proposed, ChangeState::Specced, ChangeState::Planned,
                            ChangeState::Implementing, ChangeState::Verifying] {
            assert!(start_state.can_transition_to(ChangeState::Abandoned),
                    "{:?} should be able to transition to Abandoned", start_state);
        }
    }

    #[test]
    fn archived_change_can_reopen() {
        let (_tmp, mut lc) = test_lifecycle();
        lc.create_change("reopen", "Reopen Test", None).unwrap();
        // Walk to archived
        lc.transition_change("reopen", ChangeState::Specced).unwrap();
        lc.transition_change("reopen", ChangeState::Planned).unwrap();
        lc.transition_change("reopen", ChangeState::Implementing).unwrap();
        lc.transition_change("reopen", ChangeState::Verifying).unwrap();
        lc.transition_change("reopen", ChangeState::Archived).unwrap();
        // Reopen
        lc.transition_change("reopen", ChangeState::Proposed).unwrap();
        let change = lc.state().changes.iter().find(|c| c.name == "reopen").unwrap();
        assert_eq!(change.state, ChangeState::Proposed);
    }

    #[test]
    fn abandoned_change_can_revive() {
        let (_tmp, mut lc) = test_lifecycle();
        lc.create_change("abandon", "Abandon Test", None).unwrap();
        lc.transition_change("abandon", ChangeState::Abandoned).unwrap();
        lc.transition_change("abandon", ChangeState::Proposed).unwrap();
        let change = lc.state().changes.iter().find(|c| c.name == "abandon").unwrap();
        assert_eq!(change.state, ChangeState::Proposed);
    }

    #[test]
    fn milestone_freeze_prevents_additions() {
        let (_tmp, mut lc) = test_lifecycle();
        lc.create_node("a", "Node A", None).unwrap();
        lc.create_node("b", "Node B", None).unwrap();
        lc.milestone_add("v1.0", "a").unwrap();
        lc.milestone_freeze("v1.0").unwrap();

        let err = lc.milestone_add("v1.0", "b");
        assert!(err.is_err());
    }

    #[test]
    fn milestone_status_report() {
        let (_tmp, mut lc) = test_lifecycle();
        lc.create_node("a", "A", None).unwrap();
        lc.create_node("b", "B", None).unwrap();
        lc.transition_node("a", NodeState::Exploring).unwrap();
        lc.transition_node("a", NodeState::Decided).unwrap();
        lc.transition_node("a", NodeState::Implementing).unwrap();
        lc.transition_node("a", NodeState::Implemented).unwrap();
        lc.milestone_add("v1.0", "a").unwrap();
        lc.milestone_add("v1.0", "b").unwrap();

        let status = lc.milestone_status("v1.0").unwrap();
        assert_eq!(status.total, 2);
        assert_eq!(status.implemented, 1);
        assert_eq!(status.progress_pct(), 50);
    }

    #[test]
    fn state_persists_across_load() {
        let tmp = TempDir::new().unwrap();
        {
            let store = JsonFileStore::new(tmp.path());
            let mut lc = Lifecycle::load(store).unwrap();
            lc.create_node("persist", "Persisted", None).unwrap();
        }
        {
            let store = JsonFileStore::new(tmp.path());
            let lc = Lifecycle::load(store).unwrap();
            assert_eq!(lc.nodes().len(), 1);
            assert_eq!(lc.nodes()[0].id, "persist");
            // Audit log also persists
            assert!(!lc.audit_log().is_empty());
        }
    }

    #[test]
    fn iso_timestamp_format() {
        let ts = iso_now();
        // Should match YYYY-MM-DDTHH:MM:SSZ
        assert!(ts.ends_with('Z'), "timestamp should end with Z: {ts}");
        assert_eq!(ts.len(), 20, "ISO 8601 should be 20 chars: {ts}");
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[10..11], "T");
    }

    #[test]
    fn audit_trail_tracks_all_operations() {
        let (_tmp, mut lc) = test_lifecycle();
        lc.create_node("a", "A", None).unwrap();         // 1
        lc.transition_node("a", NodeState::Exploring).unwrap(); // 2
        lc.force_transition_node("a", NodeState::Implemented, "test").unwrap(); // 3

        assert_eq!(lc.audit_log().len(), 3);
        assert!(!lc.audit_log()[0].forced); // create
        assert!(!lc.audit_log()[1].forced); // normal transition
        assert!(lc.audit_log()[2].forced);  // force transition
    }
}
