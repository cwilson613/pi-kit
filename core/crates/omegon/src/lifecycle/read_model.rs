//! Lifecycle read model — one projection surface for UI/API consumers.
//!
//! This module joins design/OpenSpec file parsing with the `omegon-opsx` FSM.
//! It is intentionally read-oriented: callers render these snapshots instead of
//! recomputing lifecycle truth from whichever backing store is convenient.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use omegon_opsx::{ChangeState, JsonFileStore, Lifecycle as OpsxLifecycle};

use super::context::{DegradedNode, LifecycleContextProvider};
use super::{doctor, spec};
use crate::lifecycle::types::{ChangeInfo, DesignNode};

#[derive(Debug, Clone, Default)]
pub struct SnapshotOptions {
    pub include_archived: bool,
    pub include_specs: bool,
}

#[derive(Debug, Clone, Default)]
pub struct LifecycleSnapshot {
    pub openspec: OpenSpecProjection,
    pub tasking: TaskingProjection,
    pub drift: Vec<LifecycleDriftFinding>,
}

#[derive(Debug, Clone, Default)]
pub struct OpenSpecProjection {
    pub available: bool,
    pub changes: Vec<OpenSpecChangeProjection>,
    pub total_tasks: usize,
    pub done_tasks: usize,
}

#[derive(Debug, Clone)]
pub struct OpenSpecChangeProjection {
    pub name: String,
    pub repository_path: String,
    pub lifecycle_state: String,
    pub file_stage: String,
    pub has_proposal: bool,
    pub has_design: bool,
    pub has_specs: bool,
    pub has_tasks: bool,
    pub total_tasks: usize,
    pub done_tasks: usize,
    pub artifact_health: omegon_opsx::ArtifactHealth,
    pub task_groups: Vec<crate::lifecycle::types::TaskGroup>,
    pub task_identity_findings: Vec<crate::lifecycle::spec::TaskStableIdFinding>,
    pub task_identity_error: Option<String>,
    pub specs: Vec<SpecSummary>,
    pub spec_documents: Vec<crate::lifecycle::types::SpecFileProjection>,
    pub archived_on_disk: bool,
    pub sentry_task_refs: Vec<String>,
    pub execution_summary: Option<ExecutionSummary>,
}

#[derive(Debug, Clone)]
pub struct SpecSummary {
    pub domain: String,
    pub requirements: usize,
    pub scenarios: usize,
}

#[derive(Debug, Clone, Default)]
pub struct TaskingProjection {
    pub linked_task_refs: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ExecutionSummary {
    pub status: String,
    pub completed: usize,
    pub failed: usize,
    pub running: usize,
}

#[derive(Debug, Clone)]
pub struct LifecycleDriftFinding {
    pub entity_id: String,
    pub kind: String,
    pub detail: String,
}

#[derive(Debug, Clone, Default)]
pub struct DesignTreeSnapshot {
    pub available: bool,
    pub nodes: std::collections::HashMap<String, DesignNode>,
    pub focused_node_id: Option<String>,
    pub degraded_nodes: Vec<DegradedNode>,
    pub findings: Vec<omegon_opsx::DesignRepositoryFinding>,
}

#[derive(Debug, Clone)]
pub struct DesignNodeObservation {
    pub node: DesignNode,
    pub sections: Option<crate::lifecycle::types::DocumentSections>,
    pub child_count: Option<usize>,
    pub children: Option<Vec<DesignNode>>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum LifecycleReadError {
    ProviderPoisoned,
}

#[derive(Clone)]
pub struct LifecycleReadHandle {
    provider: Arc<Mutex<LifecycleContextProvider>>,
    opsx: Arc<Mutex<OpsxLifecycle<JsonFileStore>>>,
    repo_path: PathBuf,
}

impl LifecycleReadHandle {
    pub fn new(
        provider: Arc<Mutex<LifecycleContextProvider>>,
        opsx: Arc<Mutex<OpsxLifecycle<JsonFileStore>>>,
        repo_path: PathBuf,
    ) -> Self {
        Self {
            provider,
            opsx,
            repo_path,
        }
    }

    /// Internal composition hook for providers that participate in lifecycle
    /// context assembly. Surface adapters must use the owned read methods.
    pub(crate) fn provider(&self) -> Arc<Mutex<LifecycleContextProvider>> {
        Arc::clone(&self.provider)
    }

    /// Return an owned design-tree snapshot without exposing the provider lock.
    pub fn design_tree_snapshot(
        &self,
        refresh: bool,
    ) -> Result<DesignTreeSnapshot, LifecycleReadError> {
        let mut provider = self
            .provider
            .lock()
            .map_err(|_| LifecycleReadError::ProviderPoisoned)?;
        if refresh {
            provider.refresh();
        }
        Ok(DesignTreeSnapshot {
            available: crate::paths::design_docs_dir(&self.repo_path).is_dir(),
            nodes: provider.all_nodes().clone(),
            focused_node_id: provider.focused_node_id().map(str::to_string),
            degraded_nodes: provider.degraded_nodes().to_vec(),
            findings: provider.design_findings().to_vec(),
        })
    }

    /// Return one owned design node without exposing the provider lock.
    pub fn design_node(&self, id: &str) -> Result<Option<DesignNode>, LifecycleReadError> {
        self.provider
            .lock()
            .map(|provider| provider.get_node(id).cloned())
            .map_err(|_| LifecycleReadError::ProviderPoisoned)
    }

    /// Return an owned node plus optional derived details. Only source cloning
    /// occurs under the provider lock; section parsing happens after release.
    pub fn design_node_observation(
        &self,
        id: &str,
        refresh: bool,
        include_sections: bool,
        include_tree_context: bool,
    ) -> Result<Option<DesignNodeObservation>, LifecycleReadError> {
        let (node, nodes) = {
            let mut provider = self
                .provider
                .lock()
                .map_err(|_| LifecycleReadError::ProviderPoisoned)?;
            if refresh {
                provider.refresh();
            }
            (
                provider.get_node(id).cloned(),
                include_tree_context.then(|| provider.all_nodes().clone()),
            )
        };
        let Some(node) = node else {
            return Ok(None);
        };
        let sections = include_sections
            .then(|| super::design::read_node_sections(&node))
            .flatten();
        let children = nodes.as_ref().map(|nodes| {
            super::design::get_children(nodes, id)
                .into_iter()
                .cloned()
                .collect::<Vec<_>>()
        });
        let child_count = children.as_ref().map(Vec::len);
        Ok(Some(DesignNodeObservation {
            node,
            sections,
            child_count,
            children,
        }))
    }

    pub fn refresh(&self) {
        if let Ok(mut provider) = self.provider.lock() {
            provider.refresh();
        }
    }

    pub fn snapshot(&self, opts: SnapshotOptions) -> anyhow::Result<LifecycleSnapshot> {
        Ok(LifecycleSnapshot {
            openspec: self.openspec_snapshot(opts.clone())?,
            tasking: TaskingProjection::default(),
            drift: self.drift_findings()?,
        })
    }

    pub fn openspec_snapshot(&self, opts: SnapshotOptions) -> anyhow::Result<OpenSpecProjection> {
        let active_changes = {
            let mut provider = self.provider.lock().unwrap();
            provider.refresh();
            provider.changes().to_vec()
        };
        let mut changes = active_changes.clone();
        if opts.include_archived {
            changes.extend(spec::list_archived_changes(&self.repo_path));
        }

        let opsx_states = self.opsx_states();
        let mut projected = Vec::new();
        for change in &changes {
            let archived_on_disk = is_archived_change(change);
            let state = opsx_states
                .iter()
                .find(|(name, _)| name == &change.name)
                .map(|(_, state)| state.as_str().to_string())
                .unwrap_or_else(|| {
                    if archived_on_disk {
                        "archived".to_string()
                    } else {
                        change.state.as_str().to_string()
                    }
                });
            if !opts.include_archived && (state == "archived" || archived_on_disk) {
                continue;
            }
            projected.push(project_change(
                change,
                &state,
                archived_on_disk,
                &opts,
                &self.repo_path,
            ));
        }

        let total_tasks = projected.iter().map(|c| c.total_tasks).sum();
        let done_tasks = projected.iter().map(|c| c.done_tasks).sum();
        Ok(OpenSpecProjection {
            available: crate::paths::openspec_dir(&self.repo_path)
                .is_some_and(|path| path.join("changes").is_dir()),
            changes: projected,
            total_tasks,
            done_tasks,
        })
    }

    pub fn drift_findings(&self) -> anyhow::Result<Vec<LifecycleDriftFinding>> {
        let changes = spec::list_changes(&self.repo_path);
        let opsx_states = self.opsx_states_map();
        let mut findings = doctor::audit_openspec_changes(&changes, &opsx_states);
        findings.extend(doctor::audit_openspec_archives(
            &self.repo_path,
            &opsx_states,
        ));
        Ok(findings
            .into_iter()
            .map(|f| LifecycleDriftFinding {
                entity_id: f.node_id,
                kind: f.kind.as_str().to_string(),
                detail: f.detail,
            })
            .collect())
    }

    fn opsx_states(&self) -> Vec<(String, ChangeState)> {
        self.opsx
            .lock()
            .unwrap()
            .state()
            .changes
            .iter()
            .map(|c| (c.name.clone(), c.state))
            .collect()
    }

    fn opsx_states_map(&self) -> std::collections::HashMap<String, ChangeState> {
        self.opsx_states().into_iter().collect()
    }
}

fn project_change(
    change: &ChangeInfo,
    lifecycle_state: &str,
    archived_on_disk: bool,
    opts: &SnapshotOptions,
    repo_path: &std::path::Path,
) -> OpenSpecChangeProjection {
    let specs = if opts.include_specs {
        change
            .specs
            .iter()
            .map(|spec| SpecSummary {
                domain: spec.domain.clone(),
                requirements: spec.requirements.len(),
                scenarios: spec
                    .requirements
                    .iter()
                    .map(|req| req.scenarios.len())
                    .sum(),
            })
            .collect()
    } else {
        vec![]
    };

    OpenSpecChangeProjection {
        name: change.name.clone(),
        repository_path: change
            .path
            .strip_prefix(repo_path)
            .unwrap_or(&change.path)
            .to_string_lossy()
            .replace('\\', "/"),
        lifecycle_state: lifecycle_state.to_string(),
        file_stage: change.state.as_str().to_string(),
        has_proposal: change.has_proposal,
        has_design: change.has_design,
        has_specs: change.has_specs,
        has_tasks: change.has_tasks,
        total_tasks: change.total_tasks,
        done_tasks: change.done_tasks,
        artifact_health: change.artifact_health.clone(),
        task_groups: change.task_groups.clone(),
        task_identity_findings: Vec::new(),
        task_identity_error: None,
        specs,
        spec_documents: if opts.include_specs {
            change.specs.clone()
        } else {
            Vec::new()
        },
        archived_on_disk,
        sentry_task_refs: vec![],
        execution_summary: None,
    }
}

fn is_archived_change(change: &ChangeInfo) -> bool {
    let mut previous_was_openspec = false;
    for component in change.path.components() {
        let current = component.as_os_str();
        if previous_was_openspec && current == std::ffi::OsStr::new("archive") {
            return true;
        }
        previous_was_openspec = current == std::ffi::OsStr::new("openspec");
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn openspec_snapshot_reports_fsm_state_and_file_stage() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        let change_dir = repo.join("openspec/changes/snapshot-change");
        fs::create_dir_all(change_dir.join("specs")).unwrap();
        fs::write(change_dir.join("proposal.md"), "# Snapshot\n").unwrap();
        fs::write(
            change_dir.join("specs/core.md"),
            "# core\n\n### Requirement: Flow works\n\n#### Scenario: Valid flow\n\nGiven setup\nWhen run\nThen success\n",
        )
        .unwrap();

        let provider = Arc::new(Mutex::new(LifecycleContextProvider::new(repo)));
        let opsx = Arc::new(Mutex::new(
            OpsxLifecycle::load(JsonFileStore::new(repo)).unwrap(),
        ));
        let handle = LifecycleReadHandle::new(provider, opsx, repo.to_path_buf());

        let snapshot = handle
            .openspec_snapshot(SnapshotOptions::default())
            .unwrap();
        assert_eq!(snapshot.changes.len(), 1);
        assert_eq!(snapshot.changes[0].name, "snapshot-change");
        assert_eq!(snapshot.changes[0].lifecycle_state, "specced");
        assert_eq!(snapshot.changes[0].file_stage, "specced");
        assert!(
            handle.opsx.lock().unwrap().state().changes.is_empty(),
            "read-model snapshots must not write opsx state"
        );
    }

    #[test]
    fn openspec_snapshot_is_read_only_when_opsx_state_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        let change_dir = repo.join("openspec/changes/read-only");
        fs::create_dir_all(change_dir.join("specs")).unwrap();
        fs::write(
            change_dir.join("proposal.md"),
            "# Read Only
",
        )
        .unwrap();
        fs::write(
            change_dir.join("tasks.md"),
            "- [ ] pending
",
        )
        .unwrap();

        let provider = Arc::new(Mutex::new(LifecycleContextProvider::new(repo)));
        let opsx = Arc::new(Mutex::new(
            OpsxLifecycle::load(JsonFileStore::new(repo)).unwrap(),
        ));
        let handle = LifecycleReadHandle::new(provider, opsx, repo.to_path_buf());

        let snapshot = handle
            .openspec_snapshot(SnapshotOptions::default())
            .unwrap();
        assert_eq!(snapshot.changes.len(), 1);
        assert_eq!(snapshot.changes[0].lifecycle_state, "implementing");
        assert!(handle.opsx.lock().unwrap().state().changes.is_empty());
    }

    #[test]
    fn openspec_snapshot_does_not_materialize_discovered_changes() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        let change_dir = repo.join("openspec/changes/discovered-change");
        fs::create_dir_all(&change_dir).unwrap();
        fs::write(change_dir.join("proposal.md"), "# Discovered\n").unwrap();
        fs::write(change_dir.join("tasks.md"), "- [ ] pending\n").unwrap();

        let provider = Arc::new(Mutex::new(LifecycleContextProvider::new(repo)));
        let opsx = Arc::new(Mutex::new(
            OpsxLifecycle::load(JsonFileStore::new(repo)).unwrap(),
        ));
        let handle = LifecycleReadHandle::new(provider, opsx, repo.to_path_buf());

        let snapshot = handle
            .openspec_snapshot(SnapshotOptions::default())
            .unwrap();
        assert_eq!(snapshot.changes.len(), 1);
        assert_eq!(snapshot.changes[0].name, "discovered-change");
        assert_eq!(snapshot.changes[0].lifecycle_state, "implementing");
        assert!(
            !repo.join("ai/lifecycle/state.json").exists(),
            "read-only snapshot must not write lifecycle state"
        );
    }

    #[test]
    fn openspec_snapshot_can_include_archived_changes() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        let change_dir = repo.join("openspec/archive/archived-change");
        fs::create_dir_all(change_dir.join("specs")).unwrap();
        fs::write(change_dir.join("proposal.md"), "# Archived\n").unwrap();
        fs::write(change_dir.join("tasks.md"), "- [x] done\n").unwrap();

        let provider = Arc::new(Mutex::new(LifecycleContextProvider::new(repo)));
        let opsx = Arc::new(Mutex::new(
            OpsxLifecycle::load(JsonFileStore::new(repo)).unwrap(),
        ));
        let handle = LifecycleReadHandle::new(provider, opsx, repo.to_path_buf());

        let default_snapshot = handle
            .openspec_snapshot(SnapshotOptions::default())
            .unwrap();
        assert!(default_snapshot.changes.is_empty());

        let snapshot = handle
            .openspec_snapshot(SnapshotOptions {
                include_archived: true,
                include_specs: false,
            })
            .unwrap();
        assert_eq!(snapshot.changes.len(), 1);
        assert_eq!(snapshot.changes[0].name, "archived-change");
        assert_eq!(snapshot.changes[0].lifecycle_state, "archived");
        assert!(snapshot.changes[0].archived_on_disk);
    }
}
