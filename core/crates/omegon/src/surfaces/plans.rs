//! Shared repository-work projection into the existing plan/task read model.

use sha2::{Digest, Sha256};
use styrene_work_model::{SourceKind, WorkAuthority, WorkItem, WorkKind, WorkState};
use styrene_work_runtime::WorkSnapshot;

use crate::conversation::{
    PlanBinding, PlanItemProjection, PlanRegistryEntry, PlanScope, PlanSource, PlanStatus,
    PlanTaskSourceRef, PlanTaskStableIdQuality, ProgressSummary, TaskCompletionPolicy, TaskIntent,
    WorkItemStatus,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecyclePlanProjection {
    pub entries: Vec<PlanRegistryEntry>,
    pub tasks: Vec<PlanItemProjection>,
    pub task_identity_findings: Vec<crate::lifecycle::spec::TaskStableIdFinding>,
}

pub(crate) fn from_work_snapshot(snapshot: &WorkSnapshot) -> LifecyclePlanProjection {
    let mut openspec_changes = snapshot
        .items
        .iter()
        .filter(|item| {
            is_openspec_source(item)
                && item.kind == WorkKind::Change
                && item.lifecycle.workflow.as_deref() == Some("openspec")
        })
        .collect::<Vec<_>>();
    openspec_changes.sort_by_key(|item| facet_string(item.facets.openspec.as_ref(), "change_name"));

    let mut openspec_tasks = snapshot
        .items
        .iter()
        .filter(|item| {
            is_openspec_source(item)
                && item.kind == WorkKind::Task
                && item.lifecycle.workflow.as_deref() == Some("openspec_task")
        })
        .collect::<Vec<_>>();
    openspec_tasks.sort_by_key(|item| {
        let facet = item.facets.openspec.as_ref();
        (
            facet_string(facet, "change_name"),
            facet_usize(facet, "group_index"),
            facet_usize(facet, "task_index"),
        )
    });

    let mut design_nodes = snapshot
        .items
        .iter()
        .filter(|item| {
            is_design_source(item)
                && matches!(item.kind, WorkKind::Initiative | WorkKind::Task)
                && item.lifecycle.workflow.as_deref() == Some("design")
                && design_visible(item.lifecycle.native_state.as_str())
        })
        .collect::<Vec<_>>();
    design_nodes.sort_by_key(|item| facet_string(item.facets.planning.as_ref(), "design_node_id"));

    let mut entries = Vec::new();
    let mut tasks = Vec::new();
    let mut task_identity_findings = Vec::new();
    for item in openspec_changes {
        if let Some(entry) = openspec_entry(item) {
            task_identity_findings.extend(openspec_findings(item));
            entries.push(entry);
        }
    }
    for item in openspec_tasks {
        if let Some(task) = openspec_task(item) {
            tasks.push(task);
        }
    }

    for node in design_nodes {
        let Some((entry, decision)) = design_entry(node) else {
            continue;
        };
        let node_id = entry.binding.design_node_id.clone().unwrap_or_default();
        entries.push(entry);
        if let Some(decision) = decision {
            tasks.push(decision);
            continue;
        }
        let mut questions = snapshot
            .items
            .iter()
            .filter(|item| {
                is_design_source(item)
                    && item.kind == WorkKind::Task
                    && item.lifecycle.workflow.as_deref() == Some("design_question")
                    && facet_string(item.facets.planning.as_ref(), "design_node_id").as_deref()
                        == Some(node_id.as_str())
            })
            .collect::<Vec<_>>();
        questions.sort_by_key(|item| facet_usize(item.facets.planning.as_ref(), "question_index"));
        tasks.extend(
            questions
                .into_iter()
                .filter_map(|item| design_question(item, node)),
        );
    }

    LifecyclePlanProjection {
        entries,
        tasks,
        task_identity_findings,
    }
}

fn openspec_entry(item: &WorkItem) -> Option<PlanRegistryEntry> {
    let facet = item.facets.openspec.as_ref()?;
    let change_name = facet_string(Some(facet), "change_name")?;
    let has_tasks = facet_bool(Some(facet), "has_tasks")?;
    let completed = facet_usize(Some(facet), "done_tasks")?;
    let total = facet_usize(Some(facet), "total_tasks")?;
    Some(PlanRegistryEntry {
        plan_id: PlanBinding::openspec_plan_id(&change_name, None),
        title: change_name.clone(),
        scope: PlanScope::Repo,
        source: PlanSource::OpenSpec,
        status: if !has_tasks {
            PlanStatus::Stale
        } else if total > 0 && completed >= total {
            PlanStatus::Completed
        } else {
            PlanStatus::Active
        },
        binding: PlanBinding {
            openspec_change: Some(change_name),
            ..PlanBinding::default()
        },
        progress: ProgressSummary { completed, total },
        resume_hint: Some(format!("OpenSpec \u{b7} {}", item.lifecycle.native_state)),
    })
}

fn openspec_findings(item: &WorkItem) -> Vec<crate::lifecycle::spec::TaskStableIdFinding> {
    item.facets
        .openspec
        .as_ref()
        .and_then(|facet| facet.get("identity_findings"))
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter_map(|finding| {
            Some(crate::lifecycle::spec::TaskStableIdFinding {
                line: finding.get("line")?.as_u64()? as usize,
                task_id: finding.get("task_id")?.as_str()?.to_string(),
                stable_id: finding.get("stable_id")?.as_str()?.to_string(),
                message: finding.get("message")?.as_str()?.to_string(),
            })
        })
        .collect()
}

fn openspec_task(item: &WorkItem) -> Option<PlanItemProjection> {
    let facet = item.facets.openspec.as_ref()?;
    let change_name = facet_string(Some(facet), "change_name")?;
    let group = facet_string(Some(facet), "group")?;
    let task_id = facet_string(Some(facet), "task_id")?;
    let stable_id = facet_string(Some(facet), "stable_id")?;
    let explicit = facet_bool(Some(facet), "stable_id_explicit")?;
    let done = facet_bool(Some(facet), "done")?;
    facet_usize(Some(facet), "group_index")?;
    facet_usize(Some(facet), "task_index")?;
    let group_plan_id = PlanBinding::openspec_plan_id(&change_name, Some(&group));
    Some(PlanItemProjection {
        id: format!("{group_plan_id}:{task_id}"),
        stable_id,
        stable_id_quality: if explicit {
            PlanTaskStableIdQuality::Explicit
        } else {
            PlanTaskStableIdQuality::Fallback
        },
        revision: source_revision("openspec", &change_name, &task_id, &item.title),
        source: PlanTaskSourceRef {
            kind: "openspec".into(),
            path: Some(format!("openspec/changes/{change_name}/tasks.md")),
            anchor: Some(task_id),
        },
        supported_mutations: Vec::new(),
        plan_id: PlanBinding::openspec_plan_id(&change_name, None),
        label: item.title.clone(),
        status: if done {
            WorkItemStatus::Done
        } else {
            WorkItemStatus::Pending
        },
        intent: TaskIntent::Spec,
        completion_policy: TaskCompletionPolicy::LifecycleStateReached,
        evidence: Vec::new(),
        external_task_refs: Vec::new(),
        writable: false,
    })
}

fn design_entry(item: &WorkItem) -> Option<(PlanRegistryEntry, Option<PlanItemProjection>)> {
    let facet = item.facets.planning.as_ref()?;
    let node_id = facet_string(Some(facet), "design_node_id")?;
    let openspec_change = facet_string(Some(facet), "openspec_change");
    let source_path = facet_string(Some(facet), "source_path")?;
    let total_questions = facet_usize(Some(facet), "open_question_count")?;
    let total = total_questions.max(1);
    let completed = usize::from(total_questions == 0);
    let plan_id = PlanBinding::design_plan_id(&node_id);
    let binding = PlanBinding {
        design_node_id: Some(node_id.clone()),
        openspec_change: openspec_change.clone(),
        ..PlanBinding::default()
    };
    let entry = PlanRegistryEntry {
        plan_id: plan_id.clone(),
        title: item.title.clone(),
        scope: PlanScope::Repo,
        source: if openspec_change.is_some() {
            PlanSource::Hybrid
        } else {
            PlanSource::Design
        },
        status: if item.lifecycle.category == WorkState::Blocked {
            PlanStatus::Blocked
        } else if item.lifecycle.category == WorkState::Completed {
            PlanStatus::Completed
        } else {
            PlanStatus::Active
        },
        binding: binding.clone(),
        progress: ProgressSummary { completed, total },
        resume_hint: Some(format!("Design \u{b7} {}", item.lifecycle.native_state)),
    };
    let decision = (total_questions == 0).then(|| PlanItemProjection {
        id: format!("{plan_id}:decision"),
        stable_id: format!("design:{node_id}:decision"),
        stable_id_quality: PlanTaskStableIdQuality::Explicit,
        revision: source_revision(
            "design",
            &node_id,
            "decision",
            "Record or verify design decision evidence",
        ),
        source: PlanTaskSourceRef {
            kind: if openspec_change.is_some() {
                "hybrid"
            } else {
                "design"
            }
            .into(),
            path: Some(source_path),
            anchor: Some("decision".into()),
        },
        supported_mutations: Vec::new(),
        plan_id,
        label: "Record or verify design decision evidence".into(),
        status: WorkItemStatus::Pending,
        intent: TaskIntent::Design,
        completion_policy: TaskCompletionPolicy::EvidenceRequired,
        evidence: Vec::new(),
        external_task_refs: binding.external_task_refs,
        writable: false,
    });
    Some((entry, decision))
}

fn design_question(item: &WorkItem, parent: &WorkItem) -> Option<PlanItemProjection> {
    let facet = item.facets.planning.as_ref()?;
    let node_id = facet_string(Some(facet), "design_node_id")?;
    let index = facet_usize(Some(facet), "question_index")?;
    if !is_design_source(parent) {
        return None;
    }
    let parent_facet = parent.facets.planning.as_ref()?;
    let openspec_change = facet_string(Some(parent_facet), "openspec_change");
    let source_path = facet_string(Some(parent_facet), "source_path")?;
    let plan_id = PlanBinding::design_plan_id(&node_id);
    Some(PlanItemProjection {
        id: format!("{plan_id}:question:{index}"),
        stable_id: format!("design:{node_id}:question:{index}"),
        stable_id_quality: PlanTaskStableIdQuality::Fallback,
        revision: source_revision(
            "design",
            &node_id,
            &format!("question:{index}"),
            &item.title,
        ),
        source: PlanTaskSourceRef {
            kind: if openspec_change.is_some() {
                "hybrid"
            } else {
                "design"
            }
            .into(),
            path: Some(source_path),
            anchor: Some(format!("question:{index}")),
        },
        supported_mutations: Vec::new(),
        plan_id,
        label: item.title.clone(),
        status: WorkItemStatus::Pending,
        intent: TaskIntent::Design,
        completion_policy: TaskCompletionPolicy::EvidenceRequired,
        evidence: Vec::new(),
        external_task_refs: Vec::new(),
        writable: false,
    })
}

fn design_visible(native_state: &str) -> bool {
    matches!(
        native_state,
        "exploring" | "decided" | "implementing" | "blocked"
    )
}

fn is_openspec_source(item: &WorkItem) -> bool {
    item.authority == WorkAuthority::OpenSpec
        && item.provenance.origin.source_id.as_str() == "omegon.openspec"
        && item.provenance.origin.source_kind == SourceKind::Lifecycle
}

fn is_design_source(item: &WorkItem) -> bool {
    item.authority == WorkAuthority::Repository
        && item.provenance.origin.source_id.as_str() == "omegon.design"
        && item.provenance.origin.source_kind == SourceKind::Repository
}

fn facet_string(facet: Option<&serde_json::Value>, key: &str) -> Option<String> {
    facet?.get(key)?.as_str().map(str::to_string)
}

fn facet_bool(facet: Option<&serde_json::Value>, key: &str) -> Option<bool> {
    facet?.get(key)?.as_bool()
}

fn facet_usize(facet: Option<&serde_json::Value>, key: &str) -> Option<usize> {
    facet?.get(key)?.as_u64().map(|value| value as usize)
}

fn source_revision(source: &str, owner: &str, anchor: &str, label: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(label.as_bytes());
    format!(
        "source-v1:{source}:{owner}:{anchor}:sha256:{:x}",
        hasher.finalize()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn work_snapshot_projection_matches_compatibility_scanner() {
        let repo = tempfile::tempdir().unwrap();
        let alpha = repo.path().join("openspec/changes/alpha");
        std::fs::create_dir_all(&alpha).unwrap();
        std::fs::write(alpha.join("proposal.md"), "# Alpha\n").unwrap();
        std::fs::write(
            alpha.join("tasks.md"),
            "## 1. Group\n\n- [ ] 1.1 First <!-- task-id: duplicate -->\n\n\n- [ ] 1.1 Second <!-- task-id: duplicate -->\n",
        )
        .unwrap();
        let change = repo.path().join("openspec/changes/demo");
        std::fs::create_dir_all(&change).unwrap();
        std::fs::write(change.join("proposal.md"), "# Demo\n").unwrap();
        std::fs::write(
            change.join("tasks.md"),
            "## 1. Group\n\n- [x] 1.1 Done task <!-- task-id: stable-done -->\n- [ ] 1.2 Pending task\n- [ ] 1.3 Duplicate marker <!-- task-id: stable-done -->\n",
        )
        .unwrap();
        let docs = repo.path().join("docs");
        std::fs::create_dir_all(&docs).unwrap();
        std::fs::write(
            docs.join("node.md"),
            "---\nid: plan-node\ntitle: Plan Node\nstatus: exploring\nopen_questions:\n  - What evidence is needed?\n---\n\n# Plan Node\n",
        )
        .unwrap();
        std::fs::write(
            docs.join("decision.md"),
            "---\nid: a-decision\ntitle: Decision Node\nstatus: decided\nopenspec_change: demo\n---\n\n# Decision Node\n",
        )
        .unwrap();

        let feature =
            crate::features::work_aggregation::WorkAggregationFeature::from_repository(repo.path())
                .await;
        let from_snapshot = from_work_snapshot(&feature.snapshot());
        let compatibility = crate::tools::lifecycle_plan_projection(repo.path());
        assert_eq!(from_snapshot, compatibility);

        let snapshot = feature.snapshot();
        let openspec_ids = snapshot
            .items
            .iter()
            .filter(|item| item.lifecycle.workflow.as_deref() == Some("openspec_task"))
            .map(|item| item.id.as_str())
            .collect::<std::collections::HashSet<_>>();
        let openspec_count = snapshot
            .items
            .iter()
            .filter(|item| item.lifecycle.workflow.as_deref() == Some("openspec_task"))
            .count();
        assert_eq!(openspec_ids.len(), openspec_count);
    }

    #[tokio::test]
    async fn malformed_or_spoofed_work_cannot_publish_plan_dtos() {
        let repo = tempfile::tempdir().unwrap();
        let change = repo.path().join("openspec/changes/demo");
        std::fs::create_dir_all(&change).unwrap();
        std::fs::write(change.join("proposal.md"), "# Demo\n").unwrap();
        std::fs::write(
            change.join("tasks.md"),
            "## 1. Group\n\n- [ ] 1.1 Task <!-- task-id: stable -->\n",
        )
        .unwrap();
        let feature =
            crate::features::work_aggregation::WorkAggregationFeature::from_repository(repo.path())
                .await;
        let original = feature.snapshot();
        let baseline = from_work_snapshot(&original);
        let mut items = original.items.to_vec();

        let mut spoofed = items
            .iter()
            .find(|item| item.lifecycle.workflow.as_deref() == Some("openspec_task"))
            .unwrap()
            .clone();
        spoofed.id = styrene_work_model::WorkId::new("spoof", "task").unwrap();
        spoofed.authority = WorkAuthority::Repository;
        items.push(spoofed);

        let mut malformed = items
            .iter()
            .find(|item| item.lifecycle.workflow.as_deref() == Some("openspec"))
            .unwrap()
            .clone();
        malformed.id = styrene_work_model::WorkId::new("malformed", "change").unwrap();
        malformed.title = "Fabricated".into();
        malformed.facets.openspec = Some(serde_json::json!({}));
        items.push(malformed);

        let adversarial = WorkSnapshot {
            generation: original.generation,
            generated_at: original.generated_at,
            items: std::sync::Arc::from(items),
            sources: std::sync::Arc::clone(&original.sources),
            warnings: std::sync::Arc::clone(&original.warnings),
        };
        assert_eq!(from_work_snapshot(&adversarial), baseline);
    }
}
