//! SUR-001/SUR-002 cross-surface authority campaign.

use crate::runtime_prompt::{ControlSurface, QueueMode, RuntimeActor};
use crate::runtime_supervisor::InteractiveRuntimeSupervisor;
use crate::runtime_turn::{LoopTerminalIntent, RuntimeTurnOutcome, TerminalSubmission};
use crate::session_authority::{ActorIdentity, SessionAuthority};
use crate::surfaces::session_activity::{
    ActionDescriptorV1, ActiveTurnActivityV1, ActivityTransport, LifecycleHealthV1,
    QueuedActivityV1, ReconcileDisposition, SESSION_ACTIVITY_SCHEMA_VERSION, SessionActivityCache,
    SessionActivityLineageV1, SessionActivityProjectionV1, TerminalTurnActivityV1,
};

fn full_lineage_fixture() -> SessionActivityProjectionV1 {
    let health = LifecycleHealthV1::Healthy;
    SessionActivityProjectionV1 {
        schema_version: SESSION_ACTIVITY_SCHEMA_VERSION,
        lineage: SessionActivityLineageV1 {
            session_id: "session-sur-001".into(),
            stream_id: "d9dc4699-f915-4a19-a45f-dfb8e37a379a".into(),
            runtime_generation: "runtime-generation-7".into(),
            composition_generation: "composition-generation-11".into(),
        },
        activity_revision: 41,
        queue: vec![QueuedActivityV1 {
            prompt_id: "100285d2-a491-49e0-ac06-b2f26b9aec1f".into(),
            submission_id: "6dabd4aa-f9a7-4573-a303-7277f87698f5".into(),
        }],
        active_turn: Some(ActiveTurnActivityV1 {
            turn_id: "db79e17c-3ae4-4191-9461-afde28b9e41b".into(),
            prompt_id: "2ce51f84-a291-47f8-ab88-b5a60639b645".into(),
            phase: "running".into(),
        }),
        terminal_turn: Some(TerminalTurnActivityV1 {
            turn_id: "a22e09af-d884-49ae-a739-8a5303ebcd86".into(),
            outcome: "completed".into(),
            reason_code: "worker_completed".into(),
            authority_sequence: 37,
        }),
        lifecycle_health: health,
        lifecycle_detail: None,
        actions: SessionActivityProjectionV1::canonical_actions(true, health),
    }
}

fn normalized_actions(
    actions: &[ActionDescriptorV1],
) -> Vec<(String, bool, String, Option<String>)> {
    actions
        .iter()
        .map(|action| {
            (
                serde_json::to_value(action.action)
                    .expect("canonical action serializes")
                    .as_str()
                    .expect("canonical action has a string identity")
                    .to_string(),
                action.available,
                action.owner.clone(),
                action.denial_reason.clone(),
            )
        })
        .collect()
}

#[test]
fn sur_001_all_six_adapters_normalize_one_authoritative_fixture() {
    let fixture = full_lineage_fixture();
    let expected_actions = normalized_actions(&fixture.actions);
    for transport in ActivityTransport::ALL {
        let edge = fixture.for_transport(transport);
        assert_eq!(
            edge.activity.schema_version,
            SESSION_ACTIVITY_SCHEMA_VERSION
        );
        assert_eq!(edge.activity.lineage, fixture.lineage, "{transport:?}");
        assert_eq!(
            edge.activity.activity_revision, fixture.activity_revision,
            "{transport:?}"
        );
        assert_eq!(edge.activity.queue, fixture.queue, "{transport:?}");
        assert_eq!(
            edge.activity.active_turn, fixture.active_turn,
            "{transport:?}"
        );
        assert_eq!(
            edge.activity.terminal_turn, fixture.terminal_turn,
            "{transport:?}"
        );
        assert_eq!(
            edge.activity.lifecycle_health, fixture.lifecycle_health,
            "{transport:?}"
        );
        assert_eq!(
            normalized_actions(&edge.activity.actions),
            expected_actions,
            "{transport:?}"
        );
        if transport == ActivityTransport::Cli {
            assert!(!edge.persistent_busy_reconciliation);
            assert_eq!(edge.narrowing.len(), 1);
            assert_eq!(edge.narrowing[0].field, "persistent_busy_reconciliation");
        } else {
            assert!(edge.persistent_busy_reconciliation, "{transport:?}");
            assert!(edge.narrowing.is_empty(), "{transport:?}");
        }
    }
}

fn prompt(text: &str, surface: ControlSurface) -> (String, RuntimeActor, ControlSurface) {
    (
        text.into(),
        RuntimeActor::from_submission(surface.label().into(), surface.label()),
        surface,
    )
}

fn run_persistent_edge(edge: ActivityTransport) {
    let directory = tempfile::tempdir().expect("create SUR-002 authority directory");
    let snapshot = directory.path().join(format!("{edge:?}.json"));
    let authority = SessionAuthority::open(
        &snapshot,
        format!("sur-002-{edge:?}"),
        "workspace-sur-002",
        "composition-sur-002",
        ActorIdentity {
            principal: "campaign".into(),
            ingress: "test".into(),
        },
        "2026-09-01T00:00:00Z",
    )
    .expect("open SUR-002 authority");
    let mut supervisor = InteractiveRuntimeSupervisor::with_authority(authority)
        .expect("restore SUR-002 supervisor");
    let (text, actor, surface) = prompt("first prompt", ControlSurface::Acp);
    supervisor
        .admit_prompt(
            text,
            Vec::new(),
            actor,
            surface,
            crate::operator_commands::PromptMetadata::default(),
            Some(QueueMode::UntilReady),
        )
        .expect("admit first prompt");
    supervisor
        .start_next_turn()
        .expect("start first prompt")
        .expect("first prompt promoted");
    let first_identity = supervisor.current_identity().expect("first identity");
    let first_active = supervisor
        .session_activity_projection()
        .expect("first activity");

    assert_eq!(
        supervisor
            .submit_loop_terminal_intent(LoopTerminalIntent {
                identity: first_identity,
                outcome: RuntimeTurnOutcome::Completed,
                reason_code: "worker_completed".into(),
            })
            .expect("close first turn"),
        TerminalSubmission::Committed {
            outcome: RuntimeTurnOutcome::Completed
        }
    );
    // Terminal advice is intentionally dropped. The durable projection is the recovery input.
    let idle = supervisor
        .session_activity_projection()
        .expect("durable idle activity");
    assert!(idle.is_durably_closed());
    assert!(idle.activity_revision > first_active.activity_revision);

    let mut cache = SessionActivityCache::default();
    assert_eq!(
        cache.reconcile(first_active.clone()).unwrap(),
        ReconcileDisposition::Applied
    );
    assert_eq!(
        cache.reconcile(idle.clone()).unwrap(),
        ReconcileDisposition::Applied
    );
    assert!(cache.current().expect("idle cache").active_turn.is_none());

    let (text, actor, surface) = prompt("second prompt", ControlSurface::Acp);
    supervisor
        .admit_prompt(
            text,
            Vec::new(),
            actor,
            surface,
            crate::operator_commands::PromptMetadata::default(),
            Some(QueueMode::UntilReady),
        )
        .expect("admit second prompt through supervisor authority");
    supervisor
        .start_next_turn()
        .expect("start second prompt")
        .expect("second prompt promoted");
    let second_identity = supervisor.current_identity().expect("second identity");
    let second_active = supervisor
        .session_activity_projection()
        .expect("second activity");
    assert!(second_active.activity_revision > idle.activity_revision);
    assert_eq!(
        cache.reconcile(second_active.clone()).unwrap(),
        ReconcileDisposition::Applied
    );

    // Delayed active and terminal observations for turn one cannot clear turn two.
    assert_eq!(
        cache.reconcile(first_active).unwrap(),
        ReconcileDisposition::IgnoredStale
    );
    assert_eq!(
        cache.reconcile(idle.clone()).unwrap(),
        ReconcileDisposition::IgnoredStale
    );
    assert_eq!(
        cache.reconcile(idle).unwrap(),
        ReconcileDisposition::IgnoredStale
    );
    assert_eq!(
        supervisor
            .submit_loop_terminal_intent(LoopTerminalIntent {
                identity: first_identity,
                outcome: RuntimeTurnOutcome::Completed,
                reason_code: "delayed_duplicate".into(),
            })
            .expect("classify delayed terminal intent"),
        TerminalSubmission::Stale
    );
    assert_eq!(supervisor.current_identity(), Some(second_identity));
    assert_eq!(
        cache
            .current()
            .and_then(|activity| activity.active_turn.as_ref()),
        second_active.active_turn.as_ref(),
        "{edge:?} must retain the exactly-once second turn"
    );
    assert!(
        second_active
            .for_transport(edge)
            .persistent_busy_reconciliation
    );
}

#[test]
fn sur_002_missed_terminal_advice_reconciles_every_persistent_edge() {
    for edge in [
        ActivityTransport::Tui,
        ActivityTransport::Acp,
        ActivityTransport::Web,
        ActivityTransport::Ipc,
        ActivityTransport::Daemon,
    ] {
        run_persistent_edge(edge);
    }
    assert!(
        !full_lineage_fixture()
            .for_transport(ActivityTransport::Cli)
            .persistent_busy_reconciliation
    );
}
