use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Instant;

use tokio::sync::broadcast;

use crate::AgentEvent;
use crate::runtime_prompt::{ControlSurface, PromptEnvelope, RuntimeActor};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ActiveTurnPhase {
    Running,
    Cancelling {
        requested_by: RuntimeActor,
        via: ControlSurface,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RuntimeTurnIdentity {
    pub(crate) session_epoch: u64,
    pub(crate) runtime_turn_id: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InterruptAdmission {
    Admitted,
    Duplicate,
    Stale,
    Idle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeTurnOutcome {
    Completed,
    Revoked,
    Failed,
}

impl ActiveTurnPhase {
    pub(crate) fn label(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Cancelling { .. } => "cancelling",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ActiveTurnMeta {
    pub(crate) runtime_turn_id: u64,
    pub(crate) authority_turn_id: Option<uuid::Uuid>,
    pub(crate) prompt: PromptEnvelope,
    pub(crate) phase: ActiveTurnPhase,
    pub(crate) started_at: Instant,
}

#[derive(Debug, Clone)]
pub(crate) struct RuntimeTurnLifecycle {
    runtime_turn_id: u64,
    prompt_id: u64,
    phase: &'static str,
    phase_started_at: Instant,
    turn_started_at: Instant,
    sequence: Arc<AtomicU64>,
}

impl RuntimeTurnLifecycle {
    pub(crate) fn new(active: &ActiveTurnMeta, phase: &'static str) -> Self {
        let now = Instant::now();
        Self {
            runtime_turn_id: active.runtime_turn_id,
            prompt_id: active.prompt.id,
            phase,
            phase_started_at: now,
            turn_started_at: active.started_at,
            sequence: Arc::new(AtomicU64::new(0)),
        }
    }

    pub(crate) fn transition(
        &mut self,
        phase: &'static str,
        queue_depth: usize,
        events_tx: &broadcast::Sender<AgentEvent>,
    ) {
        let now = Instant::now();
        self.phase = phase;
        self.phase_started_at = now;
        self.emit_phase(phase, 0, queue_depth, events_tx, "supervisor");
    }

    pub(crate) fn emit_phase(
        &self,
        phase: &'static str,
        phase_elapsed_ms: u64,
        queue_depth: usize,
        events_tx: &broadcast::Sender<AgentEvent>,
        source: &'static str,
    ) {
        let snapshot_json = self.phase_snapshot(phase, phase_elapsed_ms, queue_depth, source);
        let _ = events_tx.send(AgentEvent::RuntimeTurnLifecycleUpdated { snapshot_json });
    }

    fn phase_snapshot(
        &self,
        phase: &'static str,
        phase_elapsed_ms: u64,
        queue_depth: usize,
        source: &'static str,
    ) -> serde_json::Value {
        let now = Instant::now();
        let sequence = self
            .sequence
            .fetch_add(1, Ordering::Relaxed)
            .saturating_add(1);
        serde_json::json!({
            "turn_id": self.runtime_turn_id,
            "prompt_id": self.prompt_id,
            "phase": phase,
            "source": source,
            "sequence": sequence,
            "phase_elapsed_ms": phase_elapsed_ms,
            "turn_elapsed_ms": now.saturating_duration_since(self.turn_started_at).as_millis() as u64,
            "queue_depth": queue_depth,
        })
    }

    pub(crate) fn snapshot(&self, queue_depth: usize) -> serde_json::Value {
        let now = Instant::now();
        serde_json::json!({
            "turn_id": self.runtime_turn_id,
            "prompt_id": self.prompt_id,
            "phase": self.phase,
            "source": "supervisor",
            "sequence": self.sequence.load(Ordering::Relaxed),
            "phase_elapsed_ms": now.saturating_duration_since(self.phase_started_at).as_millis() as u64,
            "turn_elapsed_ms": now.saturating_duration_since(self.turn_started_at).as_millis() as u64,
            "queue_depth": queue_depth,
        })
    }
}

#[derive(Debug, Default)]
pub(crate) struct ActiveTurnState {
    active: Option<ActiveTurnMeta>,
    session_epoch: u64,
    next_runtime_turn_id: u64,
}

impl ActiveTurnState {
    pub(crate) fn current(&self) -> Option<&ActiveTurnMeta> {
        self.active.as_ref()
    }

    pub(crate) fn is_busy(&self) -> bool {
        self.active.is_some()
    }

    pub(crate) fn session_epoch(&self) -> u64 {
        self.session_epoch
    }

    pub(crate) fn start(
        &mut self,
        prompt: PromptEnvelope,
        authority_turn_id: Option<uuid::Uuid>,
    ) -> Option<ActiveTurnMeta> {
        if self.active.is_some() {
            return None;
        }
        self.next_runtime_turn_id = self.next_runtime_turn_id.saturating_add(1);
        let active = ActiveTurnMeta {
            runtime_turn_id: self.next_runtime_turn_id,
            authority_turn_id,
            prompt,
            phase: ActiveTurnPhase::Running,
            started_at: Instant::now(),
        };
        self.active = Some(active.clone());
        Some(active)
    }

    pub(crate) fn request_cancel(
        &mut self,
        actor: RuntimeActor,
        via: ControlSurface,
    ) -> Option<&ActiveTurnMeta> {
        let identity = self.current_identity()?;
        let _ = self.admit_interrupt(identity, actor, via);
        self.active.as_ref()
    }

    pub(crate) fn current_identity(&self) -> Option<RuntimeTurnIdentity> {
        self.active.as_ref().map(|active| RuntimeTurnIdentity {
            session_epoch: self.session_epoch,
            runtime_turn_id: active.runtime_turn_id,
        })
    }

    pub(crate) fn admit_interrupt(
        &mut self,
        identity: RuntimeTurnIdentity,
        actor: RuntimeActor,
        via: ControlSurface,
    ) -> InterruptAdmission {
        let Some(active) = self.active.as_mut() else {
            return InterruptAdmission::Idle;
        };
        if identity.session_epoch != self.session_epoch
            || identity.runtime_turn_id != active.runtime_turn_id
        {
            return InterruptAdmission::Stale;
        }
        if matches!(active.phase, ActiveTurnPhase::Cancelling { .. }) {
            return InterruptAdmission::Duplicate;
        }
        active.phase = ActiveTurnPhase::Cancelling {
            requested_by: actor,
            via,
        };
        InterruptAdmission::Admitted
    }

    pub(crate) fn finish(
        &mut self,
        runtime_turn_id: u64,
        outcome: RuntimeTurnOutcome,
    ) -> Option<ActiveTurnMeta> {
        let active = self.active.as_ref()?;
        if active.runtime_turn_id != runtime_turn_id {
            return None;
        }
        if outcome == RuntimeTurnOutcome::Completed
            && matches!(active.phase, ActiveTurnPhase::Cancelling { .. })
        {
            return None;
        }
        self.active.take()
    }

    pub(crate) fn settle_worker(&mut self) -> Option<(ActiveTurnMeta, RuntimeTurnOutcome)> {
        let active = self.active.as_ref()?;
        let runtime_turn_id = active.runtime_turn_id;
        let outcome = if matches!(active.phase, ActiveTurnPhase::Cancelling { .. }) {
            RuntimeTurnOutcome::Revoked
        } else {
            RuntimeTurnOutcome::Completed
        };
        self.finish(runtime_turn_id, outcome)
            .map(|active| (active, outcome))
    }

    pub(crate) fn complete(&mut self) -> Option<ActiveTurnMeta> {
        let runtime_turn_id = self.active.as_ref()?.runtime_turn_id;
        self.finish(runtime_turn_id, RuntimeTurnOutcome::Completed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_prompt::{QueueMode, RuntimeActorKind};
    use std::path::PathBuf;

    fn prompt(id: u64) -> PromptEnvelope {
        PromptEnvelope {
            id,
            authority_prompt_id: None,
            text: format!("prompt-{id}"),
            image_paths: Vec::<PathBuf>::new(),
            submitted_by: RuntimeActor::tui(),
            via: ControlSurface::Tui,
            metadata: crate::operator_commands::PromptMetadata::default(),
            queue_mode: QueueMode::UntilReady,
            queued_at: Instant::now(),
        }
    }

    #[test]
    fn start_allocates_monotonic_turn_ids_and_blocks_overlap() {
        let mut state = ActiveTurnState::default();
        assert_eq!(state.start(prompt(1), None).unwrap().runtime_turn_id, 1);
        assert!(state.start(prompt(2), None).is_none());
        assert_eq!(state.complete().unwrap().prompt.id, 1);
        assert_eq!(state.start(prompt(2), None).unwrap().runtime_turn_id, 2);
    }

    #[test]
    fn cancel_is_idempotent_and_preserves_first_requester() {
        let mut state = ActiveTurnState::default();
        state.start(prompt(1), None).unwrap();
        state.request_cancel(RuntimeActor::auspex(), ControlSurface::Ipc);
        state.request_cancel(
            RuntimeActor {
                kind: RuntimeActorKind::WebClient,
                label: "later".into(),
            },
            ControlSurface::WebSocket,
        );

        let active = state.current().unwrap();
        assert_eq!(active.phase.label(), "cancelling");
        assert!(matches!(
            &active.phase,
            ActiveTurnPhase::Cancelling { requested_by, via }
                if requested_by == &RuntimeActor::auspex() && via == &ControlSurface::Ipc
        ));
    }

    #[test]
    fn lifecycle_snapshots_increment_sequence_and_preserve_contract_fields() {
        let mut state = ActiveTurnState::default();
        let active = state.start(prompt(7), None).unwrap();
        let mut lifecycle = RuntimeTurnLifecycle::new(&active, "promoted");
        let (events_tx, mut events_rx) = broadcast::channel(4);

        lifecycle.transition("worker_spawned", 3, &events_tx);
        lifecycle.emit_phase("loop_running", 12, 2, &events_tx, "worker");

        let first = events_rx.try_recv().unwrap();
        let second = events_rx.try_recv().unwrap();
        let AgentEvent::RuntimeTurnLifecycleUpdated {
            snapshot_json: first,
        } = first
        else {
            panic!("expected lifecycle event");
        };
        let AgentEvent::RuntimeTurnLifecycleUpdated {
            snapshot_json: second,
        } = second
        else {
            panic!("expected lifecycle event");
        };

        assert_eq!(first["turn_id"], 1);
        assert_eq!(first["prompt_id"], 7);
        assert_eq!(first["phase"], "worker_spawned");
        assert_eq!(first["source"], "supervisor");
        assert_eq!(first["sequence"], 1);
        assert_eq!(first["queue_depth"], 3);
        assert_eq!(second["phase"], "loop_running");
        assert_eq!(second["source"], "worker");
        assert_eq!(second["sequence"], 2);
        assert_eq!(second["phase_elapsed_ms"], 12);

        let snapshot = lifecycle.snapshot(1);
        assert_eq!(snapshot["phase"], "worker_spawned");
        assert_eq!(snapshot["sequence"], 2);
        assert_eq!(snapshot["queue_depth"], 1);
    }

    #[test]
    fn idle_cancel_and_complete_are_noops() {
        let mut state = ActiveTurnState::default();
        assert!(
            state
                .request_cancel(RuntimeActor::tui(), ControlSurface::Tui)
                .is_none()
        );
        assert!(state.complete().is_none());
        assert!(!state.is_busy());
    }
}
