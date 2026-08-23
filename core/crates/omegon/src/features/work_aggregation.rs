//! Read-only repository work aggregation published as an in-process service.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use omegon_traits::{
    Feature, RuntimeActivationBoundary, RuntimeCapabilityId, RuntimeCleanupRequirement,
    RuntimeCompositionTransitionPolicy, RuntimeContributionGenerationId, RuntimeFailureDisposition,
    RuntimeInProcessService, RuntimeLifecyclePolicy, RuntimeLifecycleRequirement,
    RuntimeServiceInterfaceId,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use styrene_work_model::{
    ExternalRef, Priority, RelationKind, SourceId, SourceKind, WorkAuthority, WorkCapabilities,
    WorkFacets, WorkId, WorkItem, WorkKind, WorkLifecycle, WorkOrigin, WorkProvenance,
    WorkRelation, WorkState,
};
use styrene_work_runtime::{
    RefreshContext, SourceRefresh, SourceSnapshot, WorkRuntime, WorkSnapshot, WorkSource,
    WorkSourceDescriptor,
};

pub(crate) const WORK_SNAPSHOT_CAPABILITY: &str = "service:work-snapshot";
pub(crate) const WORK_SNAPSHOT_INTERFACE: &str = "interface:styrene-work-snapshot-v1";
const WORK_AGGREGATION_GENERATION: &str = "work-aggregation:v1";

pub(crate) fn work_snapshot_capability_id() -> RuntimeCapabilityId {
    RuntimeCapabilityId::new(WORK_SNAPSHOT_CAPABILITY).expect("static capability id is valid")
}

pub(crate) fn work_snapshot_interface_id() -> RuntimeServiceInterfaceId {
    RuntimeServiceInterfaceId::new(WORK_SNAPSHOT_INTERFACE).expect("static interface id is valid")
}

pub(crate) struct WorkAggregationFeature {
    snapshot: Arc<WorkSnapshot>,
}

impl WorkAggregationFeature {
    pub(crate) async fn from_repository(repo_root: &Path) -> Self {
        let sources: Vec<Arc<dyn WorkSource>> = vec![
            Arc::new(OpenSpecWorkSource::new(repo_root)),
            Arc::new(OpenSpecDiagnosticsSource::new(repo_root)),
            Arc::new(DesignWorkSource::new(repo_root)),
            Arc::new(DesignDiagnosticsSource::new(repo_root)),
        ];
        let mut runtime = WorkRuntime::new(sources);
        let snapshot = match runtime.refresh().await {
            Ok(snapshot) => snapshot.clone(),
            Err(error) => {
                tracing::warn!(%error, "repository work aggregation refresh failed");
                runtime.snapshot().clone()
            }
        };
        Self {
            snapshot: Arc::new(snapshot),
        }
    }

    #[cfg(test)]
    fn snapshot(&self) -> Arc<WorkSnapshot> {
        Arc::clone(&self.snapshot)
    }
}

#[async_trait]
impl Feature for WorkAggregationFeature {
    fn name(&self) -> &str {
        "work-aggregation"
    }

    fn runtime_contribution_generation_id(&self) -> Option<RuntimeContributionGenerationId> {
        Some(
            RuntimeContributionGenerationId::new(WORK_AGGREGATION_GENERATION)
                .expect("static generation id is valid"),
        )
    }

    fn runtime_in_process_services(&self) -> Vec<RuntimeInProcessService> {
        vec![RuntimeInProcessService::no_resource_read_service(
            work_snapshot_capability_id(),
            work_snapshot_interface_id(),
            Arc::clone(&self.snapshot),
        )]
    }

    fn runtime_lifecycle_policy(&self) -> Option<RuntimeLifecyclePolicy> {
        Some(RuntimeLifecyclePolicy {
            requirement: RuntimeLifecycleRequirement::Optional,
            failure_disposition: RuntimeFailureDisposition::DegradeLocally,
            readiness_timeout_ms: 0,
            heartbeat_timeout_ms: None,
            restart_limit: 0,
        })
    }

    fn runtime_transition_policy(&self) -> Option<RuntimeCompositionTransitionPolicy> {
        Some(RuntimeCompositionTransitionPolicy {
            activation_boundary: RuntimeActivationBoundary::Boot,
            cleanup: RuntimeCleanupRequirement::Strict,
            cleanup_timeout_ms: 0,
        })
    }
}

struct OpenSpecWorkSource {
    repo_root: PathBuf,
}

impl OpenSpecWorkSource {
    fn new(repo_root: &Path) -> Self {
        Self {
            repo_root: repo_root.to_path_buf(),
        }
    }
}

#[async_trait]
impl WorkSource for OpenSpecWorkSource {
    fn descriptor(&self) -> WorkSourceDescriptor {
        WorkSourceDescriptor {
            id: SourceId::new("omegon.openspec").expect("static source id is valid"),
            kind: SourceKind::Lifecycle,
            authority: WorkAuthority::OpenSpec,
            capabilities: WorkCapabilities::default(),
            schema_version: 1,
        }
    }

    async fn refresh(
        &self,
        _previous: Option<&SourceSnapshot>,
        context: &RefreshContext,
    ) -> styrene_work_model::Result<SourceRefresh> {
        let descriptor = self.descriptor();
        let changes_dir = self.repo_root.join("openspec/changes");
        if !changes_dir.is_dir() {
            return Ok(SourceRefresh::Unavailable {
                descriptor,
                reason: "openspec/changes is absent".into(),
            });
        }
        std::fs::read_dir(&changes_dir)?;

        let mut items = Vec::new();
        for change in crate::lifecycle::spec::list_changes(&self.repo_root) {
            let change_id = WorkId::new("openspec", &change.name)?;
            let change_state = openspec_state(change.state);
            items.push(WorkItem {
                id: change_id.clone(),
                kind: WorkKind::Change,
                authority: WorkAuthority::OpenSpec,
                title: change.name.clone(),
                lifecycle: WorkLifecycle {
                    category: change_state,
                    native_state: change.state.as_str().into(),
                    workflow: Some("openspec".into()),
                    terminal: matches!(change_state, WorkState::Completed | WorkState::Archived),
                    inferred: false,
                },
                priority: Priority::Unspecified,
                body: String::new(),
                tags: Vec::new(),
                assignee: None,
                relations: Vec::new(),
                refs: vec![ExternalRef::new(
                    "repository_path",
                    format!("openspec/changes/{}", change.name),
                )],
                capabilities: WorkCapabilities::default(),
                provenance: provenance(&descriptor, context),
                facets: WorkFacets {
                    openspec: Some(json!({
                        "change_name": change.name,
                        "done_tasks": change.done_tasks,
                        "total_tasks": change.total_tasks,
                    })),
                    ..WorkFacets::default()
                },
            });

            let mut seen_task_ids = std::collections::HashSet::new();
            for group in change.task_groups {
                for task in group.tasks {
                    let stable_id = task
                        .stable_id
                        .clone()
                        .unwrap_or_else(|| format!("openspec:{}:task:{}", change.name, task.id));
                    let item_id =
                        WorkId::new("openspec-task", &format!("{}:{}", change.name, stable_id))?;
                    if !seen_task_ids.insert(item_id.clone()) {
                        continue;
                    }
                    let (category, native_state, terminal) = child_state(
                        change_state,
                        if task.done {
                            WorkState::Completed
                        } else {
                            WorkState::Planned
                        },
                    );
                    items.push(WorkItem {
                        id: item_id,
                        kind: WorkKind::Task,
                        authority: WorkAuthority::OpenSpec,
                        title: task.description.clone(),
                        lifecycle: WorkLifecycle {
                            category,
                            native_state,
                            workflow: Some("openspec_task".into()),
                            terminal,
                            inferred: false,
                        },
                        priority: Priority::Unspecified,
                        body: String::new(),
                        tags: Vec::new(),
                        assignee: None,
                        relations: vec![WorkRelation {
                            kind: RelationKind::Implements,
                            target: change_id.clone(),
                        }],
                        refs: vec![ExternalRef::new(
                            "repository_path",
                            format!("openspec/changes/{}/tasks.md#{}", change.name, task.id),
                        )],
                        capabilities: WorkCapabilities::default(),
                        provenance: provenance(&descriptor, context),
                        facets: WorkFacets {
                            openspec: Some(json!({
                                "change_name": change.name,
                                "group": group.title,
                                "task_id": task.id,
                                "stable_id": stable_id,
                                "stable_id_explicit": task.stable_id.is_some(),
                            })),
                            ..WorkFacets::default()
                        },
                    });
                }
            }
        }

        Ok(SourceRefresh::Current(SourceSnapshot {
            descriptor,
            observed_at: context.now,
            items,
            stale: false,
        }))
    }
}

struct OpenSpecDiagnosticsSource {
    repo_root: PathBuf,
}

impl OpenSpecDiagnosticsSource {
    fn new(repo_root: &Path) -> Self {
        Self {
            repo_root: repo_root.to_path_buf(),
        }
    }
}

#[async_trait]
impl WorkSource for OpenSpecDiagnosticsSource {
    fn descriptor(&self) -> WorkSourceDescriptor {
        diagnostic_descriptor("omegon.openspec-diagnostics", SourceKind::Lifecycle)
    }

    async fn refresh(
        &self,
        _previous: Option<&SourceSnapshot>,
        context: &RefreshContext,
    ) -> styrene_work_model::Result<SourceRefresh> {
        let descriptor = self.descriptor();
        let changes_dir = self.repo_root.join("openspec/changes");
        if !changes_dir.is_dir() {
            return Ok(empty_diagnostic_snapshot(descriptor, context));
        }

        let mut findings = Vec::new();
        let entries = std::fs::read_dir(&changes_dir)?;
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    findings.push(format!("cannot inspect change entry: {error}"));
                    continue;
                }
            };
            if !entry.path().is_dir() {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                findings.push("change directory name is not UTF-8".into());
                continue;
            };
            let Some(change) = crate::lifecycle::spec::get_change(&self.repo_root, &name) else {
                findings.push(format!("openspec/changes/{name}: cannot parse change"));
                continue;
            };
            match &change.artifact_health {
                omegon_opsx::ArtifactHealth::Healthy => {}
                omegon_opsx::ArtifactHealth::Incomplete { missing } => findings.push(format!(
                    "openspec/changes/{name}: incomplete ({})",
                    missing.join(", ")
                )),
                omegon_opsx::ArtifactHealth::Malformed { detail } => {
                    findings.push(format!("openspec/changes/{name}: malformed ({detail})"))
                }
            }
            if change.has_tasks {
                let tasks_path = entry.path().join("tasks.md");
                if let Err(error) = std::fs::read_to_string(&tasks_path) {
                    findings.push(format!(
                        "openspec/changes/{name}/tasks.md: unreadable ({error})"
                    ));
                    continue;
                }
                match omegon_opsx::OpenSpecRepository::new(&self.repo_root)
                    .validate_task_stable_ids(&name)
                {
                    Ok(report) => findings.extend(report.findings.into_iter().map(|finding| {
                        format!(
                            "openspec/changes/{name}/tasks.md:{}: {}",
                            finding.line, finding.message
                        )
                    })),
                    Err(error) => findings.push(format!(
                        "openspec/changes/{name}/tasks.md: cannot validate identities ({error})"
                    )),
                }
            }
        }

        diagnostic_refresh(descriptor, context, findings)
    }
}

struct DesignWorkSource {
    repo_root: PathBuf,
}

impl DesignWorkSource {
    fn new(repo_root: &Path) -> Self {
        Self {
            repo_root: repo_root.to_path_buf(),
        }
    }
}

#[async_trait]
impl WorkSource for DesignWorkSource {
    fn descriptor(&self) -> WorkSourceDescriptor {
        WorkSourceDescriptor {
            id: SourceId::new("omegon.design").expect("static source id is valid"),
            kind: SourceKind::Repository,
            authority: WorkAuthority::Repository,
            capabilities: WorkCapabilities::default(),
            schema_version: 1,
        }
    }

    async fn refresh(
        &self,
        _previous: Option<&SourceSnapshot>,
        context: &RefreshContext,
    ) -> styrene_work_model::Result<SourceRefresh> {
        let descriptor = self.descriptor();
        let docs_dir = self.repo_root.join("docs");
        if !docs_dir.is_dir() {
            return Ok(SourceRefresh::Unavailable {
                descriptor,
                reason: "docs is absent".into(),
            });
        }
        std::fs::read_dir(&docs_dir)?;

        let scan = crate::lifecycle::design::scan_design_docs_with_findings(&docs_dir);
        let mut items = Vec::new();
        for node in scan.nodes.values() {
            let node_id = WorkId::new("design", &node.id)?;
            let node_state = design_state(node.status);
            let mut relations = Vec::new();
            if let Some(parent) = &node.parent {
                relations.push(WorkRelation {
                    kind: RelationKind::Contains,
                    target: WorkId::new("design", parent)?,
                });
            }
            for dependency in &node.dependencies {
                relations.push(WorkRelation {
                    kind: RelationKind::DependsOn,
                    target: WorkId::new("design", dependency)?,
                });
            }
            if let Some(change) = &node.openspec_change {
                relations.push(WorkRelation {
                    kind: RelationKind::Specifies,
                    target: WorkId::new("openspec", change)?,
                });
            }
            let relative_path = node
                .file_path
                .strip_prefix(&self.repo_root)
                .unwrap_or(&node.file_path)
                .to_string_lossy()
                .replace('\\', "/");
            items.push(WorkItem {
                id: node_id.clone(),
                kind: design_kind(node.issue_type),
                authority: WorkAuthority::Repository,
                title: node.title.clone(),
                lifecycle: WorkLifecycle {
                    category: node_state,
                    native_state: node.status.as_str().into(),
                    workflow: Some("design".into()),
                    terminal: matches!(node_state, WorkState::Completed | WorkState::Archived),
                    inferred: false,
                },
                priority: design_priority(node.priority),
                body: String::new(),
                tags: node.tags.clone(),
                assignee: None,
                relations,
                refs: vec![ExternalRef::new("repository_path", relative_path.clone())],
                capabilities: WorkCapabilities::default(),
                provenance: provenance(&descriptor, context),
                facets: WorkFacets {
                    planning: Some(json!({
                        "design_node_id": node.id,
                        "openspec_change": node.openspec_change,
                        "open_question_count": node.open_questions.len(),
                        "source_path": relative_path,
                    })),
                    ..WorkFacets::default()
                },
            });

            let mut duplicate_questions = std::collections::HashMap::<String, usize>::new();
            for (index, question) in node.open_questions.iter().enumerate() {
                let content_id = content_key(question);
                let occurrence = duplicate_questions.entry(content_id.clone()).or_default();
                *occurrence += 1;
                let question_id = if *occurrence == 1 {
                    format!("{}:{content_id}", node.id)
                } else {
                    format!("{}:{content_id}:{}", node.id, occurrence)
                };
                let (category, native_state, terminal) =
                    child_state(node_state, WorkState::Planned);
                items.push(WorkItem {
                    id: WorkId::new("design-question", &question_id)?,
                    kind: WorkKind::Task,
                    authority: WorkAuthority::Repository,
                    title: question.clone(),
                    lifecycle: WorkLifecycle {
                        category,
                        native_state,
                        workflow: Some("design_question".into()),
                        terminal,
                        inferred: false,
                    },
                    priority: design_priority(node.priority),
                    body: String::new(),
                    tags: node.tags.clone(),
                    assignee: None,
                    relations: vec![WorkRelation {
                        kind: RelationKind::Implements,
                        target: node_id.clone(),
                    }],
                    refs: vec![ExternalRef::new(
                        "repository_path",
                        format!("{}#question:{}", relative_path, index + 1),
                    )],
                    capabilities: WorkCapabilities::default(),
                    provenance: provenance(&descriptor, context),
                    facets: WorkFacets {
                        planning: Some(json!({
                            "design_node_id": node.id,
                            "question_index": index + 1,
                            "stable_id_explicit": false,
                        })),
                        ..WorkFacets::default()
                    },
                });
            }
        }

        Ok(SourceRefresh::Current(SourceSnapshot {
            descriptor,
            observed_at: context.now,
            items,
            stale: false,
        }))
    }
}

struct DesignDiagnosticsSource {
    repo_root: PathBuf,
}

impl DesignDiagnosticsSource {
    fn new(repo_root: &Path) -> Self {
        Self {
            repo_root: repo_root.to_path_buf(),
        }
    }
}

#[async_trait]
impl WorkSource for DesignDiagnosticsSource {
    fn descriptor(&self) -> WorkSourceDescriptor {
        diagnostic_descriptor("omegon.design-diagnostics", SourceKind::Repository)
    }

    async fn refresh(
        &self,
        _previous: Option<&SourceSnapshot>,
        context: &RefreshContext,
    ) -> styrene_work_model::Result<SourceRefresh> {
        let descriptor = self.descriptor();
        let docs_dir = self.repo_root.join("docs");
        if !docs_dir.is_dir() {
            return Ok(empty_diagnostic_snapshot(descriptor, context));
        }
        std::fs::read_dir(&docs_dir)?;
        let findings = crate::lifecycle::design::scan_design_docs_with_findings(&docs_dir)
            .findings
            .into_iter()
            .map(|finding| {
                let path = finding
                    .path
                    .strip_prefix(&self.repo_root)
                    .unwrap_or(&finding.path)
                    .to_string_lossy()
                    .replace('\\', "/");
                format!("{path}: {}", finding.message)
            })
            .collect();
        diagnostic_refresh(descriptor, context, findings)
    }
}

fn diagnostic_descriptor(id: &str, kind: SourceKind) -> WorkSourceDescriptor {
    WorkSourceDescriptor {
        id: SourceId::new(id).expect("static diagnostic source id is valid"),
        kind,
        authority: WorkAuthority::Repository,
        capabilities: WorkCapabilities::default(),
        schema_version: 1,
    }
}

fn empty_diagnostic_snapshot(
    descriptor: WorkSourceDescriptor,
    context: &RefreshContext,
) -> SourceRefresh {
    SourceRefresh::Current(SourceSnapshot {
        descriptor,
        observed_at: context.now,
        items: Vec::new(),
        stale: false,
    })
}

fn diagnostic_refresh(
    descriptor: WorkSourceDescriptor,
    context: &RefreshContext,
    findings: Vec<String>,
) -> styrene_work_model::Result<SourceRefresh> {
    if findings.is_empty() {
        return Ok(empty_diagnostic_snapshot(descriptor, context));
    }
    const FINDING_LIMIT: usize = 8;
    const MESSAGE_LIMIT: usize = 512;
    let total = findings.len();
    let mut message = findings
        .into_iter()
        .take(FINDING_LIMIT)
        .collect::<Vec<_>>()
        .join("; ");
    if total > FINDING_LIMIT {
        message.push_str(&format!(
            "; {} more finding(s) omitted",
            total - FINDING_LIMIT
        ));
    }
    message = message.chars().take(MESSAGE_LIMIT).collect();
    Ok(SourceRefresh::Invalid {
        descriptor,
        reason: message,
    })
}

fn provenance(descriptor: &WorkSourceDescriptor, context: &RefreshContext) -> WorkProvenance {
    WorkProvenance {
        origin: WorkOrigin {
            source_id: descriptor.id.clone(),
            source_kind: descriptor.kind,
        },
        observed_at: context.now,
        revision: None,
        projection_version: 1,
        inferred_fields: Vec::new(),
    }
}

fn child_state(parent: WorkState, own: WorkState) -> (WorkState, String, bool) {
    if matches!(
        own,
        WorkState::Completed | WorkState::Archived | WorkState::Cancelled
    ) {
        let native_state = match own {
            WorkState::Completed => "done",
            WorkState::Archived => "archived",
            WorkState::Cancelled => "cancelled",
            _ => unreachable!(),
        };
        return (own, native_state.into(), true);
    }
    match parent {
        WorkState::Archived => (WorkState::Archived, "parent_archived".into(), true),
        WorkState::Cancelled => (WorkState::Cancelled, "parent_cancelled".into(), true),
        WorkState::Completed => (WorkState::Cancelled, "parent_completed".into(), true),
        WorkState::Blocked => (WorkState::Blocked, "parent_blocked".into(), false),
        _ => {
            let terminal = matches!(
                own,
                WorkState::Completed | WorkState::Archived | WorkState::Cancelled
            );
            let native_state = match own {
                WorkState::Completed => "done",
                WorkState::Planned => "pending",
                _ => "inherited",
            };
            (own, native_state.into(), terminal)
        }
    }
}

fn content_key(content: &str) -> String {
    let digest = Sha256::digest(content.as_bytes());
    digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn openspec_state(state: omegon_opsx::ChangeState) -> WorkState {
    match state {
        omegon_opsx::ChangeState::Proposed => WorkState::Draft,
        omegon_opsx::ChangeState::Specced | omegon_opsx::ChangeState::Planned => WorkState::Planned,
        omegon_opsx::ChangeState::Implementing
        | omegon_opsx::ChangeState::Testing
        | omegon_opsx::ChangeState::Verifying => WorkState::Active,
        omegon_opsx::ChangeState::Archived => WorkState::Archived,
        omegon_opsx::ChangeState::Abandoned => WorkState::Cancelled,
    }
}

fn design_state(state: crate::lifecycle::types::NodeStatus) -> WorkState {
    use crate::lifecycle::types::NodeStatus;
    match state {
        NodeStatus::Seed => WorkState::Draft,
        NodeStatus::Exploring | NodeStatus::Resolved | NodeStatus::Decided => WorkState::Planned,
        NodeStatus::Implementing => WorkState::Active,
        NodeStatus::Implemented => WorkState::Completed,
        NodeStatus::Blocked => WorkState::Blocked,
        NodeStatus::Deferred => WorkState::Backlog,
        NodeStatus::Archived => WorkState::Archived,
    }
}

fn design_kind(issue_type: Option<crate::lifecycle::types::IssueType>) -> WorkKind {
    use crate::lifecycle::types::IssueType;
    match issue_type {
        Some(IssueType::Epic | IssueType::Feature) => WorkKind::Initiative,
        Some(IssueType::Task | IssueType::Bug | IssueType::Chore) => WorkKind::Task,
        None => WorkKind::Initiative,
    }
}

fn design_priority(priority: Option<u8>) -> Priority {
    match priority {
        Some(0 | 1) => Priority::Critical,
        Some(2) => Priority::High,
        Some(3) => Priority::Medium,
        Some(4) => Priority::Low,
        Some(_) => Priority::Someday,
        None => Priority::Unspecified,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn repository_sources_publish_one_normalized_snapshot_service() {
        let repo = tempfile::tempdir().unwrap();
        let change = repo.path().join("openspec/changes/demo");
        std::fs::create_dir_all(&change).unwrap();
        std::fs::write(change.join("proposal.md"), "# Demo\n").unwrap();
        std::fs::write(change.join("design.md"), "# Design\n").unwrap();
        std::fs::create_dir_all(change.join("specs/demo")).unwrap();
        std::fs::write(change.join("specs/demo/spec.md"), "# Requirements\n").unwrap();
        std::fs::write(
            change.join("tasks.md"),
            "## 1. Group\n\n- [x] 1.1 Done <!-- task-id: stable-done -->\n- [ ] 1.2 Pending <!-- task-id: stable-pending -->\n",
        )
        .unwrap();
        let docs = repo.path().join("docs");
        std::fs::create_dir_all(&docs).unwrap();
        std::fs::write(
            docs.join("node.md"),
            "---\nid: plan-node\ntitle: Plan Node\nstatus: exploring\nopen_questions:\n  - What evidence is needed?\n---\n\n# Plan Node\n",
        )
        .unwrap();

        let mut bus = crate::bus::EventBus::new();
        crate::setup::register_work_aggregation(&mut bus, repo.path()).await;
        bus.try_finalize().unwrap();
        let handle = bus
            .in_process_service::<WorkSnapshot>(
                &work_snapshot_capability_id(),
                &work_snapshot_interface_id(),
            )
            .unwrap()
            .expect("work snapshot service");
        let snapshot = Arc::clone(&handle.service);
        assert_eq!(snapshot.sources.len(), 4);
        assert!(snapshot.warnings.is_empty());
        for id in [
            "openspec:demo",
            "openspec-task:demo:stable-done",
            "design:plan-node",
        ] {
            assert!(
                snapshot.items.iter().any(|item| item.id.as_str() == id),
                "missing {id}"
            );
        }
        let question = snapshot
            .items
            .iter()
            .find(|item| item.id.as_str().starts_with("design-question:plan-node:"))
            .expect("content-addressed design question");
        assert_eq!(question.title, "What evidence is needed?");

        assert_eq!(handle.owner.as_str(), "feature:work-aggregation");
        assert_eq!(handle.generation_id.as_str(), WORK_AGGREGATION_GENERATION);
        assert_eq!(handle.service.generation, 1);
    }

    #[tokio::test]
    async fn malformed_design_is_a_local_warning_and_keeps_valid_work() {
        let repo = tempfile::tempdir().unwrap();
        let docs = repo.path().join("docs");
        std::fs::create_dir_all(&docs).unwrap();
        std::fs::write(
            docs.join("valid.md"),
            "---\nid: valid-node\ntitle: Valid Node\nstatus: exploring\n---\n",
        )
        .unwrap();
        std::fs::write(docs.join("invalid.md"), "---\nid: [invalid\n---\n").unwrap();

        let snapshot = WorkAggregationFeature::from_repository(repo.path())
            .await
            .snapshot();
        assert!(
            snapshot
                .items
                .iter()
                .any(|item| item.id.as_str() == "design:valid-node")
        );
        assert!(snapshot.warnings.iter().any(|warning| {
            warning.source_id.as_str() == "omegon.design-diagnostics"
                && warning.code == "source_invalid"
        }));
        let design_warning = snapshot
            .warnings
            .iter()
            .find(|warning| warning.source_id.as_str() == "omegon.design-diagnostics")
            .unwrap();
        assert!(
            !design_warning
                .message
                .contains(&repo.path().to_string_lossy().to_string())
        );
        assert!(!design_warning.message.contains('\\'));
        assert!(snapshot.warnings.iter().any(|warning| {
            warning.source_id.as_str() == "omegon.openspec" && warning.code == "source_unavailable"
        }));
    }

    #[tokio::test]
    async fn absent_repository_sources_publish_an_empty_degraded_snapshot() {
        let repo = tempfile::tempdir().unwrap();
        let snapshot = WorkAggregationFeature::from_repository(repo.path())
            .await
            .snapshot();
        assert!(snapshot.items.is_empty());
        assert_eq!(snapshot.sources.len(), 2);
        assert_eq!(snapshot.warnings.len(), 2);
        assert!(
            snapshot
                .warnings
                .iter()
                .all(|warning| warning.code == "source_unavailable")
        );
    }

    #[tokio::test]
    async fn explicit_task_and_question_identity_survive_reordering() {
        let repo = tempfile::tempdir().unwrap();
        let change = repo.path().join("openspec/changes/demo");
        std::fs::create_dir_all(&change).unwrap();
        std::fs::write(change.join("proposal.md"), "# Demo\n").unwrap();
        std::fs::write(
            change.join("tasks.md"),
            "## 1. Group\n\n- [ ] 9.9 Stable task <!-- task-id: stable-task -->\n",
        )
        .unwrap();
        let docs = repo.path().join("docs");
        std::fs::create_dir_all(&docs).unwrap();
        std::fs::write(
            docs.join("node.md"),
            "---\nid: node\ntitle: Node\nstatus: exploring\nopen_questions:\n  - Stable question\n---\n",
        )
        .unwrap();
        let first = WorkAggregationFeature::from_repository(repo.path())
            .await
            .snapshot();
        let task_id = WorkId::new("openspec-task", "demo:stable-task").unwrap();
        assert!(first.items.iter().any(|item| item.id == task_id));
        let question_id = first
            .items
            .iter()
            .find(|item| item.title == "Stable question")
            .unwrap()
            .id
            .clone();

        std::fs::write(
            change.join("tasks.md"),
            "## 1. Group\n\n- [ ] 1.1 Stable task <!-- task-id: stable-task -->\n",
        )
        .unwrap();
        std::fs::write(
            docs.join("node.md"),
            "---\nid: node\ntitle: Node\nstatus: exploring\nopen_questions:\n  - New question\n  - Stable question\n---\n",
        )
        .unwrap();
        let second = WorkAggregationFeature::from_repository(repo.path())
            .await
            .snapshot();
        assert!(second.items.iter().any(|item| item.id == task_id));
        assert_eq!(
            second
                .items
                .iter()
                .find(|item| item.title == "Stable question")
                .unwrap()
                .id,
            question_id
        );
    }

    #[test]
    fn terminal_parents_terminalize_child_work() {
        assert_eq!(
            child_state(WorkState::Cancelled, WorkState::Planned),
            (WorkState::Cancelled, "parent_cancelled".into(), true)
        );
        assert_eq!(
            child_state(WorkState::Completed, WorkState::Planned),
            (WorkState::Cancelled, "parent_completed".into(), true)
        );
        assert_eq!(
            child_state(WorkState::Blocked, WorkState::Planned),
            (WorkState::Blocked, "parent_blocked".into(), false)
        );
        assert_eq!(
            child_state(WorkState::Cancelled, WorkState::Completed),
            (WorkState::Completed, "done".into(), true)
        );
    }

    #[test]
    fn optional_work_contribution_can_be_omitted_from_composition() {
        let mut bus = crate::bus::EventBus::new();
        bus.try_finalize().unwrap();
        assert!(
            bus.in_process_service::<WorkSnapshot>(
                &work_snapshot_capability_id(),
                &work_snapshot_interface_id(),
            )
            .unwrap()
            .is_none()
        );
    }

    #[tokio::test]
    async fn terminal_artifacts_preserve_done_evidence_without_actionable_children() {
        let repo = tempfile::tempdir().unwrap();
        let change = repo.path().join("openspec/changes/abandoned");
        std::fs::create_dir_all(&change).unwrap();
        std::fs::write(
            change.join("proposal.md"),
            "---\nstate: abandoned\n---\n\n# Abandoned\n",
        )
        .unwrap();
        std::fs::write(
            change.join("tasks.md"),
            "## 1. Group\n\n- [x] 1.1 Finished <!-- task-id: finished -->\n- [ ] 1.2 Unfinished <!-- task-id: unfinished -->\n",
        )
        .unwrap();
        let docs = repo.path().join("docs");
        std::fs::create_dir_all(&docs).unwrap();
        std::fs::write(
            docs.join("implemented.md"),
            "---\nid: implemented\ntitle: Implemented\nstatus: implemented\nopen_questions:\n  - Still unresolved\n---\n",
        )
        .unwrap();

        let snapshot = WorkAggregationFeature::from_repository(repo.path())
            .await
            .snapshot();
        let finished = snapshot
            .items
            .iter()
            .find(|item| item.id.as_str() == "openspec-task:abandoned:finished")
            .unwrap();
        assert_eq!(finished.lifecycle.category, WorkState::Completed);
        assert!(finished.lifecycle.terminal);
        let unfinished = snapshot
            .items
            .iter()
            .find(|item| item.id.as_str() == "openspec-task:abandoned:unfinished")
            .unwrap();
        assert_eq!(unfinished.lifecycle.category, WorkState::Cancelled);
        assert!(unfinished.lifecycle.terminal);
        let question = snapshot
            .items
            .iter()
            .find(|item| item.title == "Still unresolved")
            .unwrap();
        assert_eq!(question.lifecycle.category, WorkState::Cancelled);
        assert!(question.lifecycle.terminal);
    }

    #[tokio::test]
    async fn openspec_diagnostics_are_source_local_and_bounded() {
        let repo = tempfile::tempdir().unwrap();
        let changes = repo.path().join("openspec/changes");
        for index in 0..12 {
            std::fs::create_dir_all(changes.join(format!("broken-{index}"))).unwrap();
        }

        let snapshot = WorkAggregationFeature::from_repository(repo.path())
            .await
            .snapshot();
        let warning = snapshot
            .warnings
            .iter()
            .find(|warning| warning.source_id.as_str() == "omegon.openspec-diagnostics")
            .expect("bounded OpenSpec warning");
        assert_eq!(warning.code, "source_invalid");
        assert!(warning.message.chars().count() <= 512);
        assert!(warning.message.contains("more finding(s) omitted"));
    }
}
