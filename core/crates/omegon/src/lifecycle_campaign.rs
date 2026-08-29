//! Cross-boundary lifecycle repository regression campaign.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use omegon_opsx::{JsonFileStore, NodeState, StateStore};
use omegon_traits::{BusEvent, BusRequest, Feature, ManagedServiceCallError};
use tokio::sync::Barrier;
use tokio_util::sync::CancellationToken;

use crate::lifecycle::read_model::SnapshotOptions;
use crate::lifecycle_service::{
    DesignMutationV1, LifecycleBinding, LifecyclePayloadV1, LifecycleRequestV1,
    LifecycleResponseV1, LifecycleServiceErrorCodeV1,
};

fn create_node(id: &str) -> DesignMutationV1 {
    DesignMutationV1::Create {
        id: id.into(),
        title: format!("{id} title"),
        parent: None,
        status: Some(NodeState::Decided),
        tags: vec!["campaign".into()],
        overview: format!("Lifecycle campaign fixture for {id}."),
    }
}

async fn revision(
    binding: &LifecycleBinding,
) -> crate::lifecycle_service::LifecycleRepositoryRevisionV1 {
    binding
        .invoke(LifecycleRequestV1::Health {
            cancellation: CancellationToken::new(),
        })
        .await
        .expect("managed lifecycle health")
        .revision
}

fn mutation_request(
    operation_id: &str,
    expected_revision: crate::lifecycle_service::LifecycleRepositoryRevisionV1,
    node_id: &str,
) -> LifecycleRequestV1 {
    LifecycleRequestV1::MutateDesign {
        operation_id: operation_id.into(),
        expected_revision,
        mutation: Box::new(create_node(node_id)),
        cancellation: CancellationToken::new(),
    }
}

async fn race_mutations(
    binding: &LifecycleBinding,
    left: LifecycleRequestV1,
    right: LifecycleRequestV1,
) -> [Result<
    LifecycleResponseV1,
    ManagedServiceCallError<crate::lifecycle_service::LifecycleServiceErrorV1>,
>; 2] {
    let barrier = Arc::new(Barrier::new(3));
    let left_task = tokio::spawn({
        let binding = binding.clone();
        let barrier = Arc::clone(&barrier);
        async move {
            barrier.wait().await;
            binding.invoke(left).await
        }
    });
    let right_task = tokio::spawn({
        let binding = binding.clone();
        let barrier = Arc::clone(&barrier);
        async move {
            barrier.wait().await;
            binding.invoke(right).await
        }
    });
    barrier.wait().await;
    [
        left_task.await.expect("left managed client joined"),
        right_task.await.expect("right managed client joined"),
    ]
}

#[tokio::test]
async fn lifecycle_campaign_revision_races_commit_once_without_partial_artifacts() {
    let repo = tempfile::tempdir().expect("campaign repository");
    let (mut bus, binding) = crate::lifecycle_service::test_binding(repo.path().to_path_buf())
        .await
        .expect("managed lifecycle fixture");
    let initial = revision(&binding).await;

    let results = race_mutations(
        &binding,
        mutation_request("race-left", initial.clone(), "race-left"),
        mutation_request("race-right", initial, "race-right"),
    )
    .await;

    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(
                result,
                Err(ManagedServiceCallError::Operation(error))
                    if error.code == LifecycleServiceErrorCodeV1::StaleRevision
            ))
            .count(),
        1
    );
    let artifacts = [
        repo.path().join("ai/docs/race-left.md"),
        repo.path().join("ai/docs/race-right.md"),
    ];
    assert_eq!(artifacts.iter().filter(|path| path.is_file()).count(), 1);
    let stale_operation = if results[0].is_err() {
        "race-left"
    } else {
        "race-right"
    };
    let stale_record = crate::lifecycle_transaction::operation_record_name(stale_operation);
    let transaction_root = repo.path().join("ai/lifecycle/transactions/repository-v1");
    assert!(
        !transaction_root
            .join("pending")
            .join(format!("{stale_record}.json"))
            .exists()
    );
    assert!(
        !transaction_root
            .join("receipts")
            .join(format!("{stale_record}.json"))
            .exists()
    );
    assert_eq!(
        std::fs::read_dir(transaction_root.join("receipts"))
            .expect("committed receipt directory")
            .count(),
        1
    );
    let state = StateStore::load(&JsonFileStore::new(repo.path())).expect("committed ledger");
    assert_eq!(state.revision, 1);
    assert_eq!(state.nodes.len(), 1);

    let replay_repo = tempfile::tempdir().expect("replay repository");
    let (mut replay_bus, replay_binding) =
        crate::lifecycle_service::test_binding(replay_repo.path().to_path_buf())
            .await
            .expect("managed replay fixture");
    let replay_initial = revision(&replay_binding).await;
    let replay_results = race_mutations(
        &replay_binding,
        mutation_request("race-replay", replay_initial.clone(), "race-replay"),
        mutation_request("race-replay", replay_initial, "race-replay"),
    )
    .await;
    let receipts: Vec<_> = replay_results
        .into_iter()
        .map(|result| match result.expect("commit or replay").payload {
            LifecyclePayloadV1::DesignMutation(receipt) => receipt,
            _ => panic!("expected design mutation receipt"),
        })
        .collect();
    assert_eq!(
        receipts.iter().filter(|receipt| receipt.replayed).count(),
        1
    );
    assert_eq!(
        receipts.iter().filter(|receipt| !receipt.replayed).count(),
        1
    );
    assert_eq!(
        receipts[0].committed_revision,
        receipts[1].committed_revision
    );
    let replay_state =
        StateStore::load(&JsonFileStore::new(replay_repo.path())).expect("replayed ledger");
    assert_eq!(replay_state.revision, 1);
    assert_eq!(replay_state.nodes.len(), 1);
    assert!(replay_repo.path().join("ai/docs/race-replay.md").is_file());

    assert!(
        bus.shutdown_managed_services()
            .await
            .all_resources_settled()
    );
    assert!(
        replay_bus
            .shutdown_managed_services()
            .await
            .all_resources_settled()
    );
}

#[tokio::test]
async fn lifecycle_campaign_shutdown_closes_ledger_and_artifact_handles() {
    let repo = tempfile::tempdir().expect("campaign repository");
    let (mut bus, binding) = crate::lifecycle_service::test_binding(repo.path().to_path_buf())
        .await
        .expect("managed lifecycle fixture");
    let initial = revision(&binding).await;
    binding
        .invoke(mutation_request("shutdown-node", initial, "shutdown-node"))
        .await
        .expect("create shutdown fixture");

    let report = bus.shutdown_managed_services().await;
    assert!(report.all_resources_settled(), "{report:?}");

    for path in [
        repo.path().join("ai/lifecycle/state.json"),
        repo.path().join("ai/docs/shutdown-node.md"),
    ] {
        let contents = std::fs::read(&path).expect("reopen settled lifecycle file");
        assert!(!contents.is_empty());
        let renamed = path.with_extension("campaign-renamed");
        std::fs::rename(&path, &renamed).expect("rename settled lifecycle file");
        std::fs::remove_file(&renamed).expect("delete settled lifecycle file");
        assert!(!renamed.exists());
    }
}

struct EventContinuityFeature(Arc<AtomicUsize>);

#[async_trait]
impl Feature for EventContinuityFeature {
    fn name(&self) -> &str {
        "lifecycle-campaign-event-continuity"
    }

    fn on_event(&mut self, _event: &BusEvent) -> Vec<BusRequest> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Vec::new()
    }
}

#[tokio::test]
async fn lifecycle_campaign_absence_is_typed_and_unrelated_event_bus_continues() {
    let mut bus = crate::bus::EventBus::new();
    let events = Arc::new(AtomicUsize::new(0));
    bus.register(Box::new(EventContinuityFeature(Arc::clone(&events))));
    let binding = LifecycleBinding::default();
    let host = crate::runtime_state::LifecycleHostHandle::new(binding.clone());
    bus.register(Box::new(
        crate::features::lifecycle::LifecycleFeature::managed(
            Path::new("."),
            binding.clone(),
            host,
        ),
    ));
    bus.try_finalize_managed()
        .await
        .expect("unrelated feature composition");

    binding.capture(&bus).expect("capture absent candidate");
    assert!(!binding.available());
    let tool_names = bus
        .tool_definitions()
        .into_iter()
        .map(|definition| definition.name)
        .collect::<std::collections::HashSet<_>>();
    for expected in [
        crate::tool_registry::lifecycle::DESIGN_TREE,
        crate::tool_registry::lifecycle::DESIGN_TREE_UPDATE,
        crate::tool_registry::lifecycle::OPENSPEC_MANAGE,
        crate::tool_registry::lifecycle::LIFECYCLE_DOCTOR,
    ] {
        assert!(
            tool_names.contains(expected),
            "missing declared tool {expected}"
        );
    }
    assert!(matches!(
        binding
            .invoke(LifecycleRequestV1::Health {
                cancellation: CancellationToken::new(),
            })
            .await,
        Err(ManagedServiceCallError::Operation(error))
            if error.code == LifecycleServiceErrorCodeV1::Unavailable
    ));

    bus.emit(&BusEvent::TurnStart { turn: 1 });
    assert_eq!(events.load(Ordering::SeqCst), 1);
}

fn write_projection_fixture(repo: &Path) {
    std::fs::create_dir_all(repo.join("ai/docs")).expect("design fixture directory");
    std::fs::write(
        repo.join("ai/docs/parity-node.md"),
        "---\nid: parity-node\ntitle: Parity Node\nstatus: decided\nissue_type: feature\npriority: 2\ntags:\n  - parity\nopen_questions: []\nopenspec_change: parity-change\n---\n\n# Parity Node\n\n## Overview\n\nShared lifecycle projection fixture.\n",
    )
    .expect("design fixture");
    let change = repo.join("ai/openspec/changes/parity-change");
    std::fs::create_dir_all(change.join("specs")).expect("openspec fixture directory");
    std::fs::write(
        change.join("proposal.md"),
        "---\nstate: implementing\n---\n\n# Parity Change\n",
    )
    .expect("proposal fixture");
    std::fs::write(change.join("design.md"), "# Design\n").expect("change design fixture");
    std::fs::write(
        change.join("tasks.md"),
        "## Campaign\n\n- [x] 1.1 Completed parity task <!-- task-id:parity.completed -->\n- [ ] 1.2 Pending parity task <!-- task-id:parity.pending -->\n",
    )
    .expect("task fixture");
    std::fs::write(
        change.join("specs/parity.md"),
        "# Parity - Delta Spec\n\n## ADDED Requirements\n\n### Requirement: Parity\n\n#### Scenario: Parity\nGiven a fixture\nWhen projected\nThen values agree\n",
    )
    .expect("spec fixture");
}

#[tokio::test]
async fn lifecycle_campaign_shared_observation_projects_consistent_values() {
    let repo = tempfile::tempdir().expect("campaign repository");
    write_projection_fixture(repo.path());
    let (mut bus, binding) = crate::lifecycle_service::test_binding(repo.path().to_path_buf())
        .await
        .expect("managed lifecycle fixture");
    let host = crate::runtime_state::LifecycleHostHandle::new(binding);
    let host_observation = host
        .refresh(SnapshotOptions::default(), CancellationToken::new())
        .await
        .expect("host lifecycle refresh");
    host.set_focus(Some("parity-node".into()))
        .expect("focus fixture node");
    let repository = host_observation.repository.expect("repository observation");

    let node = repository
        .design
        .nodes
        .get("parity-node")
        .expect("host node");
    assert_eq!(node.title, "Parity Node");
    assert_eq!(node.status, crate::lifecycle::types::NodeStatus::Decided);
    let change = repository
        .lifecycle
        .openspec
        .changes
        .iter()
        .find(|change| change.name == "parity-change")
        .expect("host change");
    assert_eq!(change.lifecycle_state, "implementing");
    assert_eq!((change.done_tasks, change.total_tasks), (1, 2));

    let work =
        crate::features::work_aggregation::WorkAggregationFeature::snapshot_from_observation(
            Arc::clone(&repository),
        )
        .await;
    let design_item = work
        .items
        .iter()
        .find(|item| item.id.as_str() == "design:parity-node")
        .expect("aggregated design item");
    assert_eq!(design_item.title, node.title);
    assert_eq!(design_item.lifecycle.native_state, node.status.as_str());
    let change_item = work
        .items
        .iter()
        .find(|item| item.id.as_str() == "openspec:parity-change")
        .expect("aggregated change item");
    assert_eq!(change_item.lifecycle.native_state, change.lifecycle_state);
    assert_eq!(
        change_item
            .facets
            .openspec
            .as_ref()
            .expect("openspec facet")["done_tasks"],
        change.done_tasks
    );
    assert_eq!(
        change_item
            .facets
            .openspec
            .as_ref()
            .expect("openspec facet")["total_tasks"],
        change.total_tasks
    );

    let acp = crate::acp_plan_tasks::projection_json(Some(&work));
    let acp_change = acp["plans"]
        .as_array()
        .expect("ACP plans")
        .iter()
        .find(|plan| plan["plan_id"] == "openspec:parity-change")
        .expect("ACP OpenSpec plan");
    assert_eq!(acp_change["progress"]["completed"], change.done_tasks);
    assert_eq!(acp_change["progress"]["total"], change.total_tasks);
    assert!(
        acp["plans"]
            .as_array()
            .expect("ACP plans")
            .iter()
            .any(|plan| plan["plan_id"] == "design:parity-node")
    );

    let handles = crate::runtime_state::RuntimeStateHandles::new(host, None, None, None, None);
    let ipc_design = crate::ipc::snapshot::project_design_tree(&handles);
    let ipc_node = ipc_design
        .nodes
        .iter()
        .find(|candidate| candidate.id == node.id)
        .expect("IPC design node");
    assert_eq!(ipc_node.title, node.title);
    assert_eq!(ipc_node.status, node.status.as_str());
    assert_eq!(
        ipc_design
            .focused
            .as_ref()
            .map(|focused| focused.id.as_str()),
        Some(node.id.as_str())
    );
    let ipc_openspec = crate::ipc::snapshot::project_openspec(&handles);
    let ipc_change = ipc_openspec
        .changes
        .iter()
        .find(|candidate| candidate.name == change.name)
        .expect("IPC OpenSpec change");
    assert_eq!(ipc_change.stage, change.lifecycle_state);
    assert_eq!(
        (ipc_change.done_tasks, ipc_change.total_tasks),
        (change.done_tasks, change.total_tasks)
    );

    #[cfg(feature = "tui")]
    {
        use crate::surfaces::dashboard::ProjectDashboardSurface;
        use crate::tui::dashboard::DashboardHandleExt;
        let mut dashboard = crate::tui::dashboard::DashboardState::default();
        handles.refresh_into(&mut dashboard);
        let tui = dashboard.project_dashboard_surface();
        let tui_node = tui
            .all_nodes
            .iter()
            .find(|candidate| candidate.id == node.id)
            .expect("TUI design node");
        assert_eq!(tui_node.title, node.title);
        assert_eq!(tui_node.status, node.status.as_str());
        assert_eq!(
            tui.focused_node.as_ref().map(|focused| focused.id.as_str()),
            Some(node.id.as_str())
        );
        let tui_change = tui
            .active_changes
            .iter()
            .find(|candidate| candidate.name == change.name)
            .expect("TUI OpenSpec change");
        assert_eq!(tui_change.stage, change.lifecycle_state);
        assert_eq!(
            (tui_change.done_tasks, tui_change.total_tasks),
            (change.done_tasks, change.total_tasks)
        );
    }

    let graph = crate::web::api::build_graph_data(&handles);
    let graph_node = graph
        .nodes
        .iter()
        .find(|node| node.id == "parity-node")
        .expect("web graph node");
    assert_eq!(graph_node.title, node.title);
    assert_eq!(graph_node.status, node.status.as_str());
    assert!(graph_node.has_openspec);

    assert!(
        bus.shutdown_managed_services()
            .await
            .all_resources_settled()
    );
}
