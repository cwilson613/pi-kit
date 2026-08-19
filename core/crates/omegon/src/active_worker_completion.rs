use tokio::sync::broadcast;

use crate::runtime_state::RuntimeStateHandles;
use crate::{InteractiveRuntimeSupervisor, RuntimeTurnLifecycle, mark_interactive_session_busy};
use omegon_traits::AgentEvent;

/// Finalize the supervisor-visible state for a worker that has stopped.
///
/// Worker task ownership and post-completion policy remain in the caller. This
/// function owns the ordered state/projection sequence that must happen exactly
/// once before the caller decides whether to promote, defer, restart, or exit.
pub(crate) fn complete(
    runtime: &mut InteractiveRuntimeSupervisor,
    lifecycle: &mut RuntimeTurnLifecycle,
    events_tx: &broadcast::Sender<AgentEvent>,
    dashboard_handles: &RuntimeStateHandles,
) {
    lifecycle.transition("supervisor_completing", runtime.queue_depth(), events_tx);
    runtime.complete_active_turn();
    lifecycle.transition("supervisor_completed", runtime.queue_depth(), events_tx);
    runtime.emit_queue_snapshot(events_tx);
    mark_interactive_session_busy(dashboard_handles, runtime.is_busy());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_prompt::RuntimePromptSubmission;
    use crate::tui;

    #[test]
    fn completion_clears_active_turn_before_next_promotion() {
        let mut runtime = InteractiveRuntimeSupervisor::default();
        let active =
            match runtime.submit(RuntimePromptSubmission::from_submission(tui::PromptSubmission {
                text: "first".into(),
                image_paths: Vec::new(),
                submitted_by: "operator".into(),
                via: "tui",
                queue_mode: tui::PromptQueueMode::UntilReady,
                metadata: tui::PromptMetadata::default(),
            })) {
                crate::RuntimePromptSubmissionOutcome::Promoted { active, .. } => active,
                crate::RuntimePromptSubmissionOutcome::Queued { .. } => {
                    panic!("idle prompt queued")
                }
            };

        runtime.submit(RuntimePromptSubmission::from_submission(tui::PromptSubmission {
            text: "second".into(),
            image_paths: Vec::new(),
            submitted_by: "operator".into(),
            via: "tui",
            queue_mode: tui::PromptQueueMode::UntilReady,
            metadata: tui::PromptMetadata::default(),
        }));

        let completed = runtime.complete_active_turn().expect("active turn");
        assert_eq!(completed.runtime_turn_id, active.runtime_turn_id);
        assert!(!runtime.is_busy());
        assert_eq!(runtime.queue_depth(), 1);
        assert_eq!(
            runtime
                .maybe_start_next_turn()
                .expect("queued turn")
                .runtime_turn_id,
            active.runtime_turn_id + 1
        );
    }
}
