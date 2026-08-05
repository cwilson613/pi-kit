//! Runtime-owned prompt queue and active-turn supervision.
//!
//! This module is deliberately frontend-neutral. TUI, IPC, web, and ACP submit
//! semantic runtime prompts; the supervisor owns queueing, promotion,
//! cancellation state, and authoritative queue projections.

use std::path::PathBuf;

use crate::AgentEvent;
use crate::runtime_prompt::{
    ControlSurface, PromptEnvelope, PromptQueue, QueueMode, RuntimeActor, RuntimePromptSubmission,
};
use crate::runtime_turn::{ActiveTurnMeta, ActiveTurnState};
use crate::tui;
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
}

impl InteractiveRuntimeSupervisor {
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
        metadata: crate::tui::PromptMetadata,
        queue_mode: Option<QueueMode>,
    ) -> u64 {
        self.queue
            .enqueue(text, image_paths, actor, via, metadata, queue_mode)
    }

    pub(crate) fn active_turn_id(&self) -> Option<u64> {
        self.turns.current().map(|active| active.runtime_turn_id)
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
        self.turns.start(self.queue.pop_front()?)
    }

    pub(crate) fn request_cancel(
        &mut self,
        actor: RuntimeActor,
        via: ControlSurface,
    ) -> Option<&ActiveTurnMeta> {
        self.turns.request_cancel(actor, via)
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

    pub(crate) fn cancel_shared_turn(shared_cancel: &tui::SharedCancel) {
        if let Ok(guard) = shared_cancel.lock()
            && let Some(ref cancel) = *guard
        {
            cancel.cancel();
        }
    }

    pub(crate) fn handle_cancel_command(
        &mut self,
        shared_cancel: &tui::SharedCancel,
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
