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
    ActiveTurnMeta, ActiveTurnState, InterruptAdmission, LoopTerminalIntent, RuntimeTurnIdentity,
    RuntimeTurnOutcome, TerminalSubmission,
};
use crate::session_authority::{
    AuthorityError, InterruptionKind, PromptAdmitted, PromptContent, SessionAuthority,
    SessionAuthorityHandle, TurnClosed, TurnInterruptionRequested, TurnOutcome,
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

#[derive(Debug)]
pub(crate) struct InteractiveRuntimeSupervisor {
    queue: PromptQueue,
    turns: ActiveTurnState,
    authority: Option<SessionAuthorityHandle>,
    execution_owner: crate::session_execution::SessionExecutionOwner,
    active_execution: Option<crate::session_execution::SessionExecutionCapture>,
    last_settled_identity: Option<RuntimeTurnIdentity>,
    host_session_generation: u64,
    projection_binding: Option<crate::session_replacement::ProjectionBinding>,
    projection_worker: Option<crate::session_shadow_projection::SessionProjectionWorker>,
    projection_start_error: Option<crate::session_shadow_projection::SessionProjectionWorkerError>,
}

impl Default for InteractiveRuntimeSupervisor {
    fn default() -> Self {
        Self {
            queue: PromptQueue::default(),
            turns: ActiveTurnState::default(),
            authority: None,
            execution_owner: crate::session_execution::SessionExecutionOwner::immutable_at_boot(),
            active_execution: None,
            last_settled_identity: None,
            host_session_generation: 1,
            projection_binding: None,
            projection_worker: None,
            projection_start_error: None,
        }
    }
}

impl InteractiveRuntimeSupervisor {
    pub(crate) fn with_authority(authority: SessionAuthority) -> Result<Self, AuthorityError> {
        let authority = SessionAuthorityHandle::new(authority);
        let execution_owner = crate::session_execution::SessionExecutionOwner::new(
            crate::session_execution::boot_execution_binding().capture(),
            Some(authority.clone()),
        )
        .map_err(|error| match error {
            crate::session_execution::SessionExecutionOwnerError::BootBinding(error)
            | crate::session_execution::SessionExecutionOwnerError::Authority(error) => error,
        })?;
        let mut supervisor = Self {
            authority: Some(authority),
            execution_owner,
            active_execution: None,
            ..Self::default()
        };
        supervisor.start_projection_worker();
        supervisor.refresh_projection_binding();
        let queued = supervisor
            .authority
            .as_ref()
            .expect("authority was assigned")
            .state()
            .queued_prompts
            .clone();
        for prompt in queued {
            let queue_mode = match prompt.queue_mode {
                crate::session_authority::QueueMode::InterruptAfterTurn => {
                    QueueMode::InterruptAfterTurn
                }
                crate::session_authority::QueueMode::UntilReady => QueueMode::UntilReady,
                crate::session_authority::QueueMode::Immediate => QueueMode::Immediate,
            };
            supervisor.queue.enqueue_submission(
                RuntimePromptSubmission {
                    text: prompt.content.text,
                    image_paths: prompt
                        .content
                        .attachments
                        .into_iter()
                        .map(|attachment| {
                            supervisor
                                .authority
                                .as_ref()
                                .expect("authority was assigned")
                                .validate_attachment(&attachment)
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                    actor: RuntimeActor::from_submission(prompt.principal, &prompt.ingress),
                    via: ControlSurface::from_via(&prompt.ingress),
                    metadata: serde_json::from_value(prompt.metadata)?,
                    queue_mode,
                },
                Some(prompt.prompt_id),
            );
        }
        Ok(supervisor)
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
        Ok(self.queue.enqueue_submission(
            RuntimePromptSubmission {
                text,
                image_paths,
                actor,
                via,
                metadata,
                queue_mode: queue_mode.unwrap_or_default(),
            },
            authority_prompt_id,
        ))
    }

    pub(crate) fn active_turn_id(&self) -> Option<u64> {
        self.turns.current().map(|active| active.runtime_turn_id)
    }

    pub(crate) fn active_turn(&self) -> Option<&ActiveTurnMeta> {
        self.turns.current()
    }

    pub(crate) fn invocation_authority(&self) -> Option<SessionAuthorityHandle> {
        self.authority.clone()
    }

    fn start_projection_worker(&mut self) {
        let Some(authority) = &self.authority else {
            return;
        };
        match crate::session_shadow_projection::SessionProjectionWorker::start(
            authority.projection_worker_descriptor(),
        ) {
            Ok(worker) => {
                authority.set_projection_wake(worker.wake_handle());
                self.projection_worker = Some(worker);
            }
            Err(error) => {
                tracing::warn!(%error, "shadow session projection worker did not start");
                self.projection_start_error = Some(error);
            }
        }
    }

    pub(crate) fn projection_worker_snapshot(
        &self,
    ) -> Option<crate::session_shadow_projection::SessionProjectionWorkerSnapshot> {
        self.projection_worker
            .as_ref()
            .map(|worker| worker.snapshot())
    }

    pub(crate) fn projection_start_error(
        &self,
    ) -> Option<&crate::session_shadow_projection::SessionProjectionWorkerError> {
        self.projection_start_error.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn projection_wake_for_test(
        &self,
    ) -> Option<crate::session_shadow_projection::SessionProjectionWakeHandle> {
        self.projection_worker
            .as_ref()
            .map(|worker| worker.wake_handle())
    }

    #[cfg(test)]
    pub(crate) fn projection_snapshot_state_for_test(
        &self,
    ) -> Option<
        std::sync::Arc<
            std::sync::Mutex<crate::session_shadow_projection::SessionProjectionWorkerSnapshot>,
        >,
    > {
        self.projection_worker
            .as_ref()
            .map(|worker| worker.snapshot_state())
    }

    pub(crate) fn flush_shadow_projections(&self) {
        if let Some(worker) = &self.projection_worker {
            worker.flush();
        }
    }

    pub(crate) fn shutdown_shadow_projections(&mut self) {
        if let Some(authority) = &self.authority {
            authority.clear_projection_wake();
        }
        if let Some(mut worker) = self.projection_worker.take() {
            worker.shutdown();
        }
    }

    pub(crate) fn drain_shadow_projection_worker(&mut self) {
        if let Some(authority) = &self.authority {
            authority.clear_projection_wake();
        }
        if let Some(mut worker) = self.projection_worker.take() {
            worker.shutdown();
        }
    }

    pub(crate) fn host_session_generation(&self) -> u64 {
        self.host_session_generation
    }

    pub(crate) fn projection_binding(
        &self,
    ) -> Option<&crate::session_replacement::ProjectionBinding> {
        self.projection_binding.as_ref()
    }

    pub(crate) fn publish_replacement_generation(&mut self, generation: u64) {
        self.host_session_generation = generation;
        self.refresh_projection_binding();
    }

    fn refresh_projection_binding(&mut self) {
        self.projection_binding = self.authority.as_ref().and_then(|authority| {
            crate::session_replacement::ProjectionBinding::from_authority(authority).ok()
        });
    }

    pub(crate) fn replacement_quiescence(
        &self,
    ) -> Result<(), crate::session_replacement::SessionReplacementRejection> {
        use crate::session_replacement::SessionReplacementRejection as Rejection;
        if self.is_busy() || self.active_execution.is_some() {
            return Err(Rejection::ActiveTurn);
        }
        if self.queue_depth() != 0 {
            return Err(Rejection::QueuedPrompts);
        }
        if self.execution_owner.has_pending_replacement() {
            return Err(Rejection::ExecutionBindingMigration);
        }
        let Some(authority) = &self.authority else {
            return Ok(());
        };
        let state = authority.state();
        if !state.queued_prompts.is_empty() {
            return Err(Rejection::QueuedPrompts);
        }
        if state.active_turn.is_some() || state.active_step.is_some() {
            return Err(Rejection::ActiveTurn);
        }
        if state.active_compaction.is_some() {
            return Err(Rejection::ActiveCompaction);
        }
        if state
            .invocations
            .values()
            .any(crate::session_authority::invocation_blocks_session_replacement)
        {
            return Err(Rejection::UnresolvedInvocation);
        }
        Ok(())
    }

    pub(crate) fn active_execution_capture(
        &self,
    ) -> Option<crate::session_execution::SessionExecutionCapture> {
        self.active_execution.clone()
    }

    #[cfg(test)]
    pub(crate) fn execution_owner(&self) -> crate::session_execution::SessionExecutionOwner {
        self.execution_owner.clone()
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

    pub(crate) fn withdraw_recovered_prompts(&mut self) -> Result<usize, AuthorityError> {
        if self.is_busy() {
            return Err(AuthorityError::Invalid(
                "cannot withdraw recovered prompts while a turn is active".into(),
            ));
        }
        let prompt_ids = self
            .queue
            .iter()
            .filter_map(|prompt| prompt.authority_prompt_id)
            .collect::<Vec<_>>();
        let Some(authority) = self.authority.as_mut() else {
            return Ok(0);
        };
        for prompt_id in &prompt_ids {
            authority.remove_prompt(
                uuid::Uuid::new_v4(),
                &authority_timestamp(),
                *prompt_id,
                crate::session_authority::PromptRemovalReason::Withdrawn,
            )?;
        }
        self.queue.clear();
        Ok(prompt_ids.len())
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
        if self.authority.is_some() {
            return None;
        }
        let prompt = self.queue.pop_front()?;
        self.active_execution = Some(self.execution_owner.capture());
        self.turns.start(prompt, None)
    }

    pub(crate) fn start_next_turn(&mut self) -> Result<Option<ActiveTurnMeta>, AuthorityError> {
        if self.turns.is_busy() {
            return Ok(None);
        }
        let Some(prompt) = self.queue.pop_front() else {
            return Ok(None);
        };
        let authority_turn_id = if self.authority.is_some() {
            let prompt_id = prompt.authority_prompt_id.ok_or_else(|| {
                AuthorityError::Invalid("durable prompt has no authority identity".into())
            })?;
            let turn_id = uuid::Uuid::new_v4();
            let start = self
                .execution_owner
                .start_turn_and_capture(
                    uuid::Uuid::new_v4(),
                    &authority_timestamp(),
                    turn_id,
                    prompt_id,
                )
                .map_err(|error| match error {
                    crate::session_execution::SessionExecutionOwnerError::BootBinding(error)
                    | crate::session_execution::SessionExecutionOwnerError::Authority(error) => {
                        error
                    }
                });
            let start = match start {
                Ok(start) => start,
                Err(error) => {
                    self.queue.push_front(prompt);
                    return Err(error);
                }
            };
            self.active_execution = Some(start.capture);
            Some(turn_id)
        } else {
            self.active_execution = Some(self.execution_owner.capture());
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
        self.request_durable_interrupt_with_reason(
            identity,
            actor,
            via,
            InterruptionKind::Cancel,
            "operator_cancelled",
        )
    }

    pub(crate) fn request_durable_interrupt_with_reason(
        &mut self,
        identity: RuntimeTurnIdentity,
        actor: RuntimeActor,
        via: ControlSurface,
        kind: InterruptionKind,
        reason_code: &str,
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
                    kind,
                    principal: actor.display_label().to_string(),
                    ingress: via.label().to_string(),
                    reason_code: reason_code.into(),
                },
            )?;
        }
        Ok(self.turns.admit_interrupt(identity, actor, via))
    }

    #[cfg(test)]
    pub(crate) fn finish_active_turn(
        &mut self,
        runtime_turn_id: u64,
        outcome: RuntimeTurnOutcome,
    ) -> Option<ActiveTurnMeta> {
        self.turns.finish(runtime_turn_id, outcome)
    }

    #[cfg(test)]
    pub(crate) fn settle_active_worker(&mut self) -> Option<(ActiveTurnMeta, RuntimeTurnOutcome)> {
        self.turns.settle_worker()
    }

    #[cfg(test)]
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
        self.close_durable_worker(outcome)
    }

    #[cfg(test)]
    pub(crate) fn close_durable_worker(
        &mut self,
        outcome: RuntimeTurnOutcome,
    ) -> Result<Option<(ActiveTurnMeta, RuntimeTurnOutcome)>, AuthorityError> {
        let Some(identity) = self.current_identity() else {
            return Ok(None);
        };
        let reason_code = match outcome {
            RuntimeTurnOutcome::Completed => "worker_completed",
            RuntimeTurnOutcome::Revoked => "worker_revoked",
            RuntimeTurnOutcome::Failed => "worker_failed",
            RuntimeTurnOutcome::TimedOut => "worker_timed_out",
        };
        let (_, settled) = self.commit_terminal_intent(LoopTerminalIntent {
            identity,
            outcome,
            reason_code: reason_code.into(),
        })?;
        Ok(settled)
    }

    pub(crate) fn submit_loop_terminal_intent(
        &mut self,
        intent: LoopTerminalIntent,
    ) -> Result<TerminalSubmission, AuthorityError> {
        self.commit_terminal_intent(intent)
            .map(|(submission, _)| submission)
    }

    fn commit_terminal_intent(
        &mut self,
        mut intent: LoopTerminalIntent,
    ) -> Result<
        (
            TerminalSubmission,
            Option<(ActiveTurnMeta, RuntimeTurnOutcome)>,
        ),
        AuthorityError,
    > {
        let Some(active) = self.turns.current() else {
            let submission = if self.last_settled_identity == Some(intent.identity) {
                TerminalSubmission::Duplicate
            } else {
                TerminalSubmission::Stale
            };
            return Ok((submission, None));
        };
        if self.current_identity() != Some(intent.identity) {
            return Ok((TerminalSubmission::Stale, None));
        }
        if intent.outcome == RuntimeTurnOutcome::Completed
            && matches!(
                active.phase,
                crate::runtime_turn::ActiveTurnPhase::Cancelling { .. }
            )
        {
            intent.outcome = RuntimeTurnOutcome::Revoked;
            intent.reason_code = "worker_revoked".into();
        }
        if let Some(authority) = self.authority.as_mut() {
            let turn_id = active.authority_turn_id.ok_or_else(|| {
                AuthorityError::Invalid("durable turn has no authority identity".into())
            })?;
            if intent.outcome != RuntimeTurnOutcome::Completed {
                authority.terminalize_active_semantic_step(
                    &authority_timestamp(),
                    crate::session_authority::SemanticTerminalization {
                        turn_id,
                        request_outcome: match intent.outcome {
                            RuntimeTurnOutcome::TimedOut => {
                                crate::session_authority::ModelRequestOutcome::TimedOut
                            }
                            RuntimeTurnOutcome::Revoked => {
                                crate::session_authority::ModelRequestOutcome::Revoked
                            }
                            RuntimeTurnOutcome::Failed => {
                                crate::session_authority::ModelRequestOutcome::Abandoned
                            }
                            RuntimeTurnOutcome::Completed => unreachable!(),
                        },
                        reason_code: intent.reason_code.clone(),
                        rule_version: 1,
                    },
                )?;
            }
            authority.close_turn(
                uuid::Uuid::new_v4(),
                &authority_timestamp(),
                TurnClosed {
                    turn_id,
                    outcome: intent.outcome.into(),
                    reason_code: intent.reason_code,
                    recovery_rule_version: None,
                },
            )?;
        }
        let settled = self
            .turns
            .finish(active.runtime_turn_id, intent.outcome)
            .map(|active| (active, intent.outcome));
        self.active_execution = None;
        self.last_settled_identity = Some(intent.identity);
        Ok((
            TerminalSubmission::Committed {
                outcome: intent.outcome,
            },
            settled,
        ))
    }

    #[cfg(test)]
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
            RuntimeTurnOutcome::TimedOut => Self::TimedOut,
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
    fn bounded_timeout_maps_to_durable_timeout() {
        assert_eq!(
            TurnOutcome::from(RuntimeTurnOutcome::TimedOut),
            TurnOutcome::TimedOut
        );
    }

    #[test]
    fn mixed_frontend_submissions_share_one_durable_fifo() {
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
        let mut supervisor = InteractiveRuntimeSupervisor::with_authority(authority).unwrap();
        for (text, principal, ingress, surface) in [
            ("tui", "local", "tui", ControlSurface::Tui),
            ("ipc", "controller", "ipc", ControlSurface::Ipc),
            ("web", "browser", "websocket", ControlSurface::WebSocket),
            ("acp", "editor", "acp", ControlSurface::Acp),
        ] {
            supervisor
                .admit_prompt(
                    text.into(),
                    Vec::new(),
                    RuntimeActor::from_submission(principal.into(), ingress),
                    surface,
                    operator_commands::PromptMetadata::default(),
                    None,
                )
                .unwrap();
        }

        assert_eq!(supervisor.queue_depth(), 4);
        assert_eq!(
            supervisor
                .authority
                .as_ref()
                .unwrap()
                .state()
                .queued_prompts
                .iter()
                .map(|prompt| prompt.content.text.as_str())
                .collect::<Vec<_>>(),
            vec!["tui", "ipc", "web", "acp"]
        );
        for expected in ["tui", "ipc", "web", "acp"] {
            let active = supervisor.start_next_turn().unwrap().unwrap();
            assert_eq!(active.prompt.text, expected);
            supervisor
                .close_durable_worker(RuntimeTurnOutcome::Completed)
                .unwrap();
        }
        assert_eq!(supervisor.queue_depth(), 0);
        assert!(!supervisor.is_busy());
    }

    #[test]
    fn turn_close_and_next_start_do_not_auto_commit_pending_execution_replacement() {
        let temp = tempfile::tempdir().unwrap();
        let authority = SessionAuthority::open(
            &temp.path().join("session-owner.json"),
            "session-owner",
            "workspace-1",
            "generation-1",
            ActorIdentity {
                principal: "operator".into(),
                ingress: "tui".into(),
            },
            "2026-08-21T18:00:00Z",
        )
        .unwrap();
        let mut supervisor = InteractiveRuntimeSupervisor::with_authority(authority).unwrap();
        for text in ["turn-a", "turn-b", "turn-c"] {
            supervisor
                .admit_prompt(
                    text.into(),
                    Vec::new(),
                    RuntimeActor::tui(),
                    ControlSurface::Tui,
                    operator_commands::PromptMetadata::default(),
                    None,
                )
                .unwrap();
        }

        supervisor.start_next_turn().unwrap().unwrap();
        let active_a = supervisor.active_execution_capture().unwrap();
        let generation_a = active_a.generation().clone();
        let generation_b = crate::session_authority::ExecutionBindingGeneration::new(
            "loop-driver:fixture/b",
            "provider-route-service:fixture/b",
        )
        .unwrap();
        let owner = supervisor.execution_owner();
        assert_eq!(
            owner
                .request_replacement(
                    uuid::Uuid::new_v4(),
                    "2026-08-21T18:00:01Z",
                    crate::session_execution::SessionExecutionBinding::release_coupled_for_test(
                        generation_b.clone(),
                    ),
                )
                .unwrap(),
            crate::session_execution::SessionExecutionReplacementOutcome::Pending
        );
        assert_eq!(active_a.generation(), &generation_a);
        assert_eq!(
            supervisor.active_execution_capture().unwrap().generation(),
            &generation_a
        );

        supervisor
            .close_durable_worker(RuntimeTurnOutcome::Completed)
            .unwrap();
        supervisor.start_next_turn().unwrap().unwrap();
        assert_eq!(
            supervisor.active_execution_capture().unwrap().generation(),
            &generation_a
        );
        supervisor
            .close_durable_worker(RuntimeTurnOutcome::Completed)
            .unwrap();
        assert_eq!(
            owner
                .commit_pending_at_quiescence(uuid::Uuid::new_v4(), "2026-08-21T18:00:02Z")
                .unwrap(),
            crate::session_execution::SessionExecutionReplacementOutcome::Applied
        );
        supervisor.start_next_turn().unwrap().unwrap();
        assert_eq!(
            supervisor.active_execution_capture().unwrap().generation(),
            &generation_b
        );
    }

    #[test]
    fn loop_terminal_intents_are_durably_identity_fenced_and_idempotent() {
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
        let mut supervisor = InteractiveRuntimeSupervisor::with_authority(authority).unwrap();
        for text in ["first", "second"] {
            supervisor
                .admit_prompt(
                    text.into(),
                    Vec::new(),
                    RuntimeActor::tui(),
                    ControlSurface::Tui,
                    operator_commands::PromptMetadata::default(),
                    None,
                )
                .unwrap();
        }
        supervisor.start_next_turn().unwrap().unwrap();
        let first = supervisor.current_identity().unwrap();
        let failed = LoopTerminalIntent {
            identity: first,
            outcome: RuntimeTurnOutcome::Failed,
            reason_code: "loop_failed".into(),
        };
        assert_eq!(
            supervisor
                .submit_loop_terminal_intent(failed.clone())
                .unwrap(),
            TerminalSubmission::Committed {
                outcome: RuntimeTurnOutcome::Failed
            }
        );
        let first_close_sequence = supervisor.authority.as_ref().unwrap().state().last_sequence;
        assert_eq!(
            supervisor.submit_loop_terminal_intent(failed).unwrap(),
            TerminalSubmission::Duplicate
        );
        assert_eq!(
            supervisor.authority.as_ref().unwrap().state().last_sequence,
            first_close_sequence
        );

        supervisor.start_next_turn().unwrap().unwrap();
        let second = supervisor.current_identity().unwrap();
        assert_eq!(
            supervisor
                .submit_loop_terminal_intent(LoopTerminalIntent {
                    identity: first,
                    outcome: RuntimeTurnOutcome::Completed,
                    reason_code: "late_completion".into(),
                })
                .unwrap(),
            TerminalSubmission::Stale
        );
        assert_eq!(supervisor.current_identity(), Some(second));
        assert_eq!(
            supervisor.authority.as_ref().unwrap().state().last_sequence,
            first_close_sequence + 1
        );

        supervisor
            .request_durable_interrupt(second, RuntimeActor::tui(), ControlSurface::Tui)
            .unwrap();
        assert_eq!(
            supervisor
                .submit_loop_terminal_intent(LoopTerminalIntent {
                    identity: second,
                    outcome: RuntimeTurnOutcome::Completed,
                    reason_code: "late_completion".into(),
                })
                .unwrap(),
            TerminalSubmission::Committed {
                outcome: RuntimeTurnOutcome::Revoked
            }
        );
        assert!(!supervisor.is_busy());
        let state = supervisor.authority.as_ref().unwrap().state();
        assert_eq!(state.closed_turns.len(), 2);
        assert_eq!(state.last_sequence, first_close_sequence + 3);
    }

    #[test]
    fn restart_after_cancellation_recovers_once_and_starts_queued_turn() {
        let temp = tempfile::tempdir().unwrap();
        let session_path = temp.path().join("session-1.json");
        let authority = SessionAuthority::open(
            &session_path,
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
        let mut supervisor = InteractiveRuntimeSupervisor::with_authority(authority).unwrap();
        for text in ["first", "second"] {
            supervisor
                .admit_prompt(
                    text.into(),
                    Vec::new(),
                    RuntimeActor::tui(),
                    ControlSurface::Tui,
                    operator_commands::PromptMetadata::default(),
                    None,
                )
                .unwrap();
        }
        let first = supervisor.start_next_turn().unwrap().unwrap();
        let first_identity = supervisor.current_identity().unwrap();
        assert_eq!(
            supervisor
                .request_durable_interrupt(
                    first_identity,
                    RuntimeActor::tui(),
                    ControlSurface::Tui,
                )
                .unwrap(),
            InterruptAdmission::Admitted
        );
        assert_eq!(
            supervisor
                .request_durable_interrupt(
                    first_identity,
                    RuntimeActor::tui(),
                    ControlSurface::Tui,
                )
                .unwrap(),
            InterruptAdmission::Duplicate
        );
        drop(supervisor);

        let recovered = SessionAuthority::open(
            &session_path,
            "session-1",
            "workspace-1",
            "generation-2",
            ActorIdentity {
                principal: "system".into(),
                ingress: "resume".into(),
            },
            "2026-08-19T18:01:00Z",
        )
        .unwrap();
        let recovered_sequence = recovered.state().last_sequence;
        let authority_turn_id = first.authority_turn_id.unwrap();
        assert_eq!(
            recovered.state().closed_turns[&authority_turn_id].outcome,
            TurnOutcome::Interrupted
        );
        assert_eq!(recovered.state().interruption_requests.len(), 1);
        assert_eq!(recovered.state().queued_prompts.len(), 1);
        drop(recovered);

        let recovered = SessionAuthority::open(
            &session_path,
            "session-1",
            "workspace-1",
            "generation-3",
            ActorIdentity {
                principal: "system".into(),
                ingress: "resume".into(),
            },
            "2026-08-19T18:02:00Z",
        )
        .unwrap();
        assert_eq!(recovered.state().last_sequence, recovered_sequence);
        assert_eq!(recovered.state().closed_turns.len(), 1);

        let mut supervisor = InteractiveRuntimeSupervisor::with_authority(recovered).unwrap();
        let second = supervisor.start_next_turn().unwrap().unwrap();
        assert_eq!(second.prompt.text, "second");
        assert!(supervisor.is_busy());
    }

    #[test]
    fn recovered_prompts_can_be_durably_withdrawn_before_acp_accepts_work() {
        let temp = tempfile::tempdir().unwrap();
        let session_path = temp.path().join("session-1.json");
        let authority = SessionAuthority::open(
            &session_path,
            "session-1",
            "workspace-1",
            "generation-1",
            ActorIdentity {
                principal: "acp-client".into(),
                ingress: "acp".into(),
            },
            "2026-08-19T18:00:00Z",
        )
        .unwrap();
        let mut supervisor = InteractiveRuntimeSupervisor::with_authority(authority).unwrap();
        supervisor
            .admit_prompt(
                "orphaned request".into(),
                Vec::new(),
                RuntimeActor::from_submission("acp-client".into(), "acp"),
                ControlSurface::Acp,
                operator_commands::PromptMetadata::default(),
                None,
            )
            .unwrap();
        drop(supervisor);

        let authority = SessionAuthority::open(
            &session_path,
            "session-1",
            "workspace-1",
            "generation-2",
            ActorIdentity {
                principal: "acp-client".into(),
                ingress: "acp".into(),
            },
            "2026-08-19T18:01:00Z",
        )
        .unwrap();
        let mut supervisor = InteractiveRuntimeSupervisor::with_authority(authority).unwrap();
        assert_eq!(supervisor.withdraw_recovered_prompts().unwrap(), 1);
        assert_eq!(supervisor.queue_depth(), 0);
        let state = supervisor.authority.as_ref().unwrap().state();
        assert!(state.queued_prompts.is_empty());
        assert!(matches!(
            state.submissions.values().next(),
            Some(crate::session_authority::SubmissionDisposition::Admitted { .. })
        ));
    }

    #[test]
    fn recovered_prompt_rejects_tampered_attachment() {
        let temp = tempfile::tempdir().unwrap();
        let session_path = temp.path().join("session-1.json");
        let source = temp.path().join("capture.png");
        std::fs::write(&source, b"trusted-image").unwrap();
        let authority = SessionAuthority::open(
            &session_path,
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
        let mut supervisor = InteractiveRuntimeSupervisor::with_authority(authority).unwrap();
        supervisor
            .admit_prompt(
                "inspect image".into(),
                vec![source],
                RuntimeActor::tui(),
                ControlSurface::Tui,
                operator_commands::PromptMetadata::default(),
                None,
            )
            .unwrap();
        let stored = supervisor
            .authority
            .as_ref()
            .unwrap()
            .state()
            .queued_prompts[0]
            .content
            .attachments[0]
            .storage_ref
            .clone();
        std::fs::write(stored, b"forged-image!").unwrap();
        drop(supervisor);

        let authority = SessionAuthority::open(
            &session_path,
            "session-1",
            "workspace-1",
            "generation-2",
            ActorIdentity {
                principal: "system".into(),
                ingress: "resume".into(),
            },
            "2026-08-19T18:01:00Z",
        )
        .unwrap();
        let error = InteractiveRuntimeSupervisor::with_authority(authority).unwrap_err();
        assert!(error.to_string().contains("digest changed"));
    }

    #[test]
    fn durable_supervisor_commits_before_mutating_queue_and_turn_state() {
        let temp = tempfile::tempdir().unwrap();
        let session_path = temp.path().join("session-1.json");
        let authority = SessionAuthority::open(
            &session_path,
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
        let mut supervisor = InteractiveRuntimeSupervisor::with_authority(authority).unwrap();

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
        drop(supervisor);
        let authority = SessionAuthority::open(
            &session_path,
            "session-1",
            "workspace-1",
            "generation-2",
            ActorIdentity {
                principal: "system".into(),
                ingress: "resume".into(),
            },
            "2026-08-19T18:01:00Z",
        )
        .unwrap();
        let mut supervisor = InteractiveRuntimeSupervisor::with_authority(authority).unwrap();
        assert_eq!(supervisor.queue_depth(), 1);
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

        supervisor
            .admit_prompt(
                "fail explicitly".into(),
                Vec::new(),
                RuntimeActor::tui(),
                ControlSurface::Tui,
                operator_commands::PromptMetadata::default(),
                None,
            )
            .unwrap();
        supervisor.start_next_turn().unwrap().unwrap();
        let (_, outcome) = supervisor
            .close_durable_worker(RuntimeTurnOutcome::Failed)
            .unwrap()
            .unwrap();
        assert_eq!(outcome, RuntimeTurnOutcome::Failed);
        assert_eq!(
            supervisor.authority.as_ref().unwrap().state().last_sequence,
            8
        );
    }
}
