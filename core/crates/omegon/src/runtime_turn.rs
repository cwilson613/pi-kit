use std::time::Instant;

use crate::runtime_prompt::{ControlSurface, PromptEnvelope, RuntimeActor};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ActiveTurnPhase {
    Running,
    Cancelling {
        requested_by: RuntimeActor,
        via: ControlSurface,
    },
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
    pub(crate) prompt: PromptEnvelope,
    pub(crate) phase: ActiveTurnPhase,
    pub(crate) started_at: Instant,
}

#[derive(Debug, Default)]
pub(crate) struct ActiveTurnState {
    active: Option<ActiveTurnMeta>,
    next_runtime_turn_id: u64,
}

impl ActiveTurnState {
    pub(crate) fn current(&self) -> Option<&ActiveTurnMeta> {
        self.active.as_ref()
    }

    pub(crate) fn is_busy(&self) -> bool {
        self.active.is_some()
    }

    pub(crate) fn start(&mut self, prompt: PromptEnvelope) -> Option<ActiveTurnMeta> {
        if self.active.is_some() {
            return None;
        }
        self.next_runtime_turn_id = self.next_runtime_turn_id.saturating_add(1);
        let active = ActiveTurnMeta {
            runtime_turn_id: self.next_runtime_turn_id,
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
        let active = self.active.as_mut()?;
        if matches!(active.phase, ActiveTurnPhase::Running) {
            active.phase = ActiveTurnPhase::Cancelling {
                requested_by: actor,
                via,
            };
        }
        self.active.as_ref()
    }

    pub(crate) fn complete(&mut self) -> Option<ActiveTurnMeta> {
        self.active.take()
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
            text: format!("prompt-{id}"),
            image_paths: Vec::<PathBuf>::new(),
            submitted_by: RuntimeActor::tui(),
            via: ControlSurface::Tui,
            metadata: crate::tui::PromptMetadata::default(),
            queue_mode: QueueMode::UntilReady,
            queued_at: Instant::now(),
        }
    }

    #[test]
    fn start_allocates_monotonic_turn_ids_and_blocks_overlap() {
        let mut state = ActiveTurnState::default();
        assert_eq!(state.start(prompt(1)).unwrap().runtime_turn_id, 1);
        assert!(state.start(prompt(2)).is_none());
        assert_eq!(state.complete().unwrap().prompt.id, 1);
        assert_eq!(state.start(prompt(2)).unwrap().runtime_turn_id, 2);
    }

    #[test]
    fn cancel_is_idempotent_and_preserves_first_requester() {
        let mut state = ActiveTurnState::default();
        state.start(prompt(1)).unwrap();
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
