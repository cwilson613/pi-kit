//! Runtime-owned prompt queue and active-turn supervision.
//!
//! This module is deliberately frontend-neutral. TUI, IPC, web, and ACP submit
//! semantic runtime prompts; the supervisor owns queueing, promotion,
//! cancellation state, and authoritative queue projections.

use std::path::PathBuf;

use crate::runtime_prompt::{
    ControlSurface, PromptEnvelope, PromptQueue, QueueMode, RuntimeActor, RuntimePromptSubmission,
};
use crate::runtime_turn::{
    ActiveTurnMeta, ActiveTurnState, InterruptAdmission, RuntimeTurnIdentity, RuntimeTurnOutcome,
};
use crate::session_authority::{
    AuthorityError, InterruptionKind, PromptAdmitted, PromptContent, SessionAuthority, TurnClosed,
    TurnInterruptionRequested, TurnOutcome,
};
use crate::{AgentEvent, operator_commands};
use tokio::sync::broadcast;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum RuntimePromptSubmissionOutcome {
    Queued {
        prompt_id: u64,
        queue_depth: usize,
    },
    Promoted {
        prompt_id: u64,
        active: Box<ActiveTurnMeta>,
    },
}

#[derive(Debug, Default)]
pub(crate) struct InteractiveRuntimeSupervisor {
    queue: PromptQueue,
    turns: ActiveTurnState,
    authority: Option<SessionAuthority>,
}

impl InteractiveRuntimeSupervisor {
    pub(crate) fn with_authority(authority: SessionAuthority) -> Self {
        Self {
            authority: Some(authority),
            ..Self::default()
        }
    }

    pub(crate) fn submit(
        &mut self,
        prompt: RuntimePromptSubmission,
    ) -> RuntimePromptSubmissionOutcome {
        let prompt_id = self.enqueue_prompt(
            prompt.text,
            prompt.image_paths,
            prompt.actor,
            prompt.via,
            prompt.metadata,
            Some(prompt.queue_mode),
        );
        if self.is_busy() {
            RuntimePromptSubmissionOutcome::Queued {
                prompt_id,
                queue_depth: self.queue_depth(),
            }
        } else {
            RuntimePromptSubmissionOutcome::Promoted {
                prompt_id,
                active: Box::new(
                    self.maybe_start_next_turn()
                        .expect("newly queued prompt is promotable while idle"),
                ),
            }
        }
    }

    pub(crate) fn enqueue_prompt(
        &mut self,
        text: String,
        image_paths: Vec<PathBuf>,
        actor: RuntimeActor,
        via: ControlSurface,
        metadata: operator_commands::PromptMetadata,
        queue_mode: Option<QueueMode>,
    ) -> u64 {
        self.queue
            .enqueue(text, image_paths, actor, via, metadata, queue_mode)
    }

    pub(crate) fn admit_prompt(
        &mut self,
        text: String,
        image_paths: Vec<PathBuf>,
        actor: RuntimeActor,
        via: ControlSurface,
        metadata: operator_commands::PromptMetadata,
        queue_mode: Option<QueueMode>,
    ) -> Result<u64, AuthorityError> {
        let authority_prompt_id = if let Some(authority) = self.authority.as_mut() {
            let attachments = image_paths
                .iter()
                .map(|path| authority.stage_attachment(path))
                .collect::<Result<Vec<_>, _>>()?;
            let prompt_id = uuid::Uuid::new_v4();
            authority.admit_prompt(
                uuid::Uuid::new_v4(),
                &authority_timestamp(),
                PromptAdmitted {
                    submission_id: uuid::Uuid::new_v4(),
                    prompt_id,
                    principal: actor.display_label().to_string(),
                    ingress: via.label().to_string(),
                    queue_mode: queue_mode.unwrap_or_default().into(),
                    content: PromptContent {
                        text: text.clone(),
                        attachments,
                    },
                    metadata: serde_json::to_value(&metadata)?,
                },
            )?;
            Some(prompt_id)
        } else {
            None
        };
        Ok(self.queue.enqueue_with_authority(
            text,
            image_paths,
            actor,
            via,
            metadata,
            queue_mode,
            authority_prompt_id,
        ))
    }

    pub(crate) fn active_turn_id(&self) -> Option<u64> {
        self.turns.current().map(|active| active.runtime_turn_id)
    }

    pub(crate) fn active_turn(&self) -> Option<&ActiveTurnMeta> {
        self.turns.current()
    }

    pub(crate) fn session_epoch(&self) -> u64 {
        self.turns.session_epoch()
    }

    pub(crate) fn queued_prompt(&self, prompt_id: u64) -> Option<&PromptEnvelope> {
        self.queue.get(prompt_id)
    }

    pub(crate) fn queue_depth(&self) -> usize {
        self.queue.depth()
    }

    fn queue_preview(&self) -> Vec<String> {
        self.queue.previews()
    }

    pub(crate) fn queue_snapshot_json(&self) -> serde_json::Value {
        serde_json::json!({
            "depth": self.queue_depth(),
            "active": self.turns.current().map(|active| serde_json::json!({
                "turn_id": active.runtime_turn_id,
                "prompt_id": active.prompt.id,
                "submitted_by": active.prompt.submitted_by.display_label(),
                "via": active.prompt.via.label(),
                "phase": active.phase.label(),
                "elapsed_ms": active.started_at.elapsed().as_millis() as u64,
                "queued_wait_ms": active.started_at.saturating_duration_since(active.prompt.queued_at).as_millis() as u64,
            })),
            "items": self.queue.snapshot_items(),
            "previews": self.queue_preview(),
        })
    }

    pub(crate) fn is_busy(&self) -> bool {
        self.turns.is_busy()
    }

    pub(crate) fn maybe_start_next_turn(&mut self) -> Option<ActiveTurnMeta> {
        if self.turns.is_busy() {
            return None;
        }
        self.turns.start(self.queue.pop_front()?, None)
    }

    pub(crate) fn start_next_turn(&mut self) -> Result<Option<ActiveTurnMeta>, AuthorityError> {
        if self.turns.is_busy() {
            return Ok(None);
        }
        let Some(prompt) = self.queue.pop_front() else {
            return Ok(None);
        };
        let authority_turn_id = if let Some(authority) = self.authority.as_mut() {
            let prompt_id = prompt.authority_prompt_id.ok_or_else(|| {
                AuthorityError::Invalid("durable prompt has no authority identity".into())
            })?;
            let turn_id = uuid::Uuid::new_v4();
            if let Err(error) = authority.start_turn(
                uuid::Uuid::new_v4(),
                &authority_timestamp(),
                turn_id,
                prompt_id,
            ) {
                self.queue.push_front(prompt);
                return Err(error);
            }
            Some(turn_id)
        } else {
            None
        };
        Ok(self.turns.start(prompt, authority_turn_id))
    }

    pub(crate) fn request_cancel(
        &mut self,
        actor: RuntimeActor,
        via: ControlSurface,
    ) -> Option<&ActiveTurnMeta> {
        self.turns.request_cancel(actor, via)
    }

    pub(crate) fn current_identity(&self) -> Option<RuntimeTurnIdentity> {
        self.turns.current_identity()
    }

    pub(crate) fn admit_interrupt(
        &mut self,
        identity: RuntimeTurnIdentity,
        actor: RuntimeActor,
        via: ControlSurface,
    ) -> InterruptAdmission {
        self.turns.admit_interrupt(identity, actor, via)
    }

    pub(crate) fn request_durable_interrupt(
        &mut self,
        identity: RuntimeTurnIdentity,
        actor: RuntimeActor,
        via: ControlSurface,
    ) -> Result<InterruptAdmission, AuthorityError> {
        let Some(active) = self.turns.current() else {
            return Ok(InterruptAdmission::Idle);
        };
        if identity.session_epoch != self.turns.session_epoch()
            || identity.runtime_turn_id != active.runtime_turn_id
        {
            return Ok(InterruptAdmission::Stale);
        }
        if matches!(
            active.phase,
            crate::runtime_turn::ActiveTurnPhase::Cancelling { .. }
        ) {
            return Ok(InterruptAdmission::Duplicate);
        }
        if let Some(authority) = self.authority.as_mut() {
            let turn_id = active.authority_turn_id.ok_or_else(|| {
                AuthorityError::Invalid("durable turn has no authority identity".into())
            })?;
            authority.request_interruption(
                uuid::Uuid::new_v4(),
                &authority_timestamp(),
                TurnInterruptionRequested {
                    interruption_id: uuid::Uuid::new_v4(),
                    turn_id,
                    kind: InterruptionKind::Cancel,
                    principal: actor.display_label().to_string(),
                    ingress: via.label().to_string(),
                    reason_code: "operator_cancelled".into(),
                },
            )?;
        }
        Ok(self.turns.admit_interrupt(identity, actor, via))
    }

    pub(crate) fn finish_active_turn(
        &mut self,
        runtime_turn_id: u64,
        outcome: RuntimeTurnOutcome,
    ) -> Option<ActiveTurnMeta> {
        self.turns.finish(runtime_turn_id, outcome)
    }

    pub(crate) fn settle_active_worker(&mut self) -> Option<(ActiveTurnMeta, RuntimeTurnOutcome)> {
        self.turns.settle_worker()
    }

    pub(crate) fn settle_durable_worker(
        &mut self,
    ) -> Result<Option<(ActiveTurnMeta, RuntimeTurnOutcome)>, AuthorityError> {
        let Some(active) = self.turns.current() else {
            return Ok(None);
        };
        let outcome = if matches!(
            active.phase,
            crate::runtime_turn::ActiveTurnPhase::Cancelling { .. }
        ) {
            RuntimeTurnOutcome::Revoked
        } else {
            RuntimeTurnOutcome::Completed
        };
        if let Some(authority) = self.authority.as_mut() {
            let turn_id = active.authority_turn_id.ok_or_else(|| {
                AuthorityError::Invalid("durable turn has no authority identity".into())
            })?;
            authority.close_turn(
                uuid::Uuid::new_v4(),
                &authority_timestamp(),
                TurnClosed {
                    turn_id,
                    outcome: outcome.into(),
                    reason_code: match outcome {
                        RuntimeTurnOutcome::Completed => "worker_completed",
                        RuntimeTurnOutcome::Revoked => "worker_revoked",
                        RuntimeTurnOutcome::Failed => "worker_failed",
                    }
                    .into(),
                    recovery_rule_version: None,
                },
            )?;
        }
        Ok(self.turns.settle_worker())
    }

    pub(crate) fn complete_active_turn(&mut self) -> Option<ActiveTurnMeta> {
        self.turns.complete()
    }

    pub(crate) fn pop_front_prompt(&mut self) -> Option<PromptEnvelope> {
        self.queue.pop_front()
    }

    pub(crate) fn push_front_prompt(&mut self, prompt: PromptEnvelope) {
        self.queue.push_front(prompt);
    }

    pub(crate) fn cancel_shared_turn(shared_cancel: &operator_commands::SharedCancel) {
        if let Ok(guard) = shared_cancel.lock()
            && let Some(ref cancel) = *guard
        {
            cancel.cancel();
        }
    }

    pub(crate) fn handle_cancel_command(
        &mut self,
        shared_cancel: &operator_commands::SharedCancel,
        events_tx: &broadcast::Sender<AgentEvent>,
        submitted_by: String,
        via: &'static str,
    ) {
        let actor = RuntimeActor::from_submission(submitted_by, via);
        let surface = ControlSurface::from_via(via);
        if self.request_cancel(actor, surface).is_none() {
            let _ = events_tx.send(AgentEvent::SystemNotification {
                message: "Cancel requested, but no active turn is running.".to_string(),
            });
        }
        Self::cancel_shared_turn(shared_cancel);
    }

    pub(crate) fn emit_queue_notification(
        &self,
        events_tx: &broadcast::Sender<AgentEvent>,
        prompt_id: u64,
    ) {
        if let Some(prompt) = self.queue.get(prompt_id) {
            self.emit_queue_snapshot(events_tx);
            let _ = events_tx.send(AgentEvent::SystemNotification {
                message: format!(
                    "Queued prompt #{} from {} via {}; queue depth {}.",
                    prompt.id,
                    prompt.submitted_by.display_label(),
                    prompt.via.label(),
                    self.queue_depth()
                ),
            });
        }
    }

    pub(crate) fn emit_queue_snapshot(&self, events_tx: &broadcast::Sender<AgentEvent>) {
        let _ = events_tx.send(AgentEvent::RuntimeQueueUpdated {
            snapshot_json: self.queue_snapshot_json(),
        });
    }

    pub(crate) fn clear_queue(&mut self) {
        self.queue.clear();
    }
}

impl From<QueueMode> for crate::session_authority::QueueMode {
    fn from(value: QueueMode) -> Self {
        match value {
            QueueMode::InterruptAfterTurn => Self::InterruptAfterTurn,
            QueueMode::UntilReady => Self::UntilReady,
            QueueMode::Immediate => Self::Immediate,
        }
    }
}

impl From<RuntimeTurnOutcome> for TurnOutcome {
    fn from(value: RuntimeTurnOutcome) -> Self {
        match value {
            RuntimeTurnOutcome::Completed => Self::Completed,
            RuntimeTurnOutcome::Revoked => Self::Revoked,
            RuntimeTurnOutcome::Failed => Self::Failed,
        }
    }
}

fn authority_timestamp() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_prompt::RuntimeActorKind;
    use crate::session_authority::ActorIdentity;

    #[test]
    fn durable_supervisor_commits_before_mutating_queue_and_turn_state() {
        let temp = tempfile::tempdir().unwrap();
        let authority = SessionAuthority::open(
            &temp.path().join("session-1.json"),
            "session-1",
            "workspace-1",
            "generation-1",
            ActorIdentity {
                principal: "operator".into(),
                ingress: "tui".into(),
            },
            "2026-08-19T18:00:00Z",
        )
        .unwrap();
        let mut supervisor = InteractiveRuntimeSupervisor::with_authority(authority);

        let prompt_id = supervisor
            .admit_prompt(
                "inspect".into(),
                Vec::new(),
                RuntimeActor::tui(),
                ControlSurface::Tui,
                operator_commands::PromptMetadata::default(),
                None,
            )
            .unwrap();
        assert_eq!(prompt_id, 1);
        assert_eq!(
            supervisor.authority.as_ref().unwrap().state().last_sequence,
            2
        );
        let active = supervisor.start_next_turn().unwrap().unwrap();
        assert_eq!(active.prompt.submitted_by.kind, RuntimeActorKind::Tui);
        assert!(active.authority_turn_id.is_some());
        assert_eq!(
            supervisor.authority.as_ref().unwrap().state().last_sequence,
            3
        );

        let identity = supervisor.current_identity().unwrap();
        assert_eq!(
            supervisor
                .request_durable_interrupt(identity, RuntimeActor::auspex(), ControlSurface::Ipc,)
                .unwrap(),
            InterruptAdmission::Admitted
        );
        assert_eq!(
            supervisor.authority.as_ref().unwrap().state().last_sequence,
            4
        );
        let (_, outcome) = supervisor.settle_durable_worker().unwrap().unwrap();
        assert_eq!(outcome, RuntimeTurnOutcome::Revoked);
        assert!(!supervisor.is_busy());
        assert_eq!(
            supervisor.authority.as_ref().unwrap().state().last_sequence,
            5
        );
    }
}
