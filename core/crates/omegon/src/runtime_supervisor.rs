//! Runtime-owned prompt queue and active-turn supervision.
//!
//! This module is deliberately frontend-neutral. TUI, IPC, web, and ACP submit
//! semantic runtime prompts; the supervisor owns queueing, promotion,
//! cancellation state, and authoritative queue projections.

use std::path::PathBuf;

use crate::runtime_prompt::{
    ControlSurface, PromptEnvelope, PromptQueue, QueueMode, RuntimeActor, RuntimePromptSubmission,
};
use crate::runtime_turn::{ActiveTurnMeta, ActiveTurnState};

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
    pub(crate) queue: PromptQueue,
    pub(crate) turns: ActiveTurnState,
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

    pub(crate) fn clear_queue(&mut self) {
        self.queue.clear();
    }
}
