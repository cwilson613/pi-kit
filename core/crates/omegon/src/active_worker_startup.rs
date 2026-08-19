use tokio::sync::broadcast;

use crate::runtime_state::RuntimeStateHandles;
use crate::runtime_turn::{ActiveTurnMeta, RuntimeTurnLifecycle};
use crate::{AgentEvent, InteractiveRuntimeSupervisor, mark_interactive_session_busy};

/// Emit the supervisor-visible startup projections for a promoted turn.
///
/// Execution construction and task spawning remain with the caller because
/// they own runtime resources and task lifetime. This function owns only the
/// ordered state/projection contract that precedes spawning.
pub(crate) fn prepare(
    active: &ActiveTurnMeta,
    runtime: &InteractiveRuntimeSupervisor,
    events_tx: &broadcast::Sender<AgentEvent>,
    dashboard_handles: &RuntimeStateHandles,
) -> RuntimeTurnLifecycle {
    runtime.emit_queue_snapshot(events_tx);
    let mut lifecycle = RuntimeTurnLifecycle::new(active, "promoted");
    lifecycle.transition("promoted", runtime.queue_depth(), events_tx);
    let _ = events_tx.send(AgentEvent::RuntimePromptStarted {
        runtime_turn_id: active.runtime_turn_id,
        text: active.prompt.text.clone(),
        image_paths: active.prompt.image_paths.clone(),
    });
    mark_interactive_session_busy(dashboard_handles, true);
    lifecycle.transition("worker_spawned", runtime.queue_depth(), events_tx);
    lifecycle
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_prompt::RuntimePromptSubmission;
    use crate::tui;

    #[test]
    fn preparation_preserves_promoted_turn_identity() {
        let mut runtime = InteractiveRuntimeSupervisor::default();
        let active =
            match runtime.submit(RuntimePromptSubmission::from_submission(tui::PromptSubmission {
                text: "startup contract".into(),
                image_paths: vec!["image.png".into()],
                submitted_by: "operator".into(),
                via: "tui",
                queue_mode: tui::PromptQueueMode::UntilReady,
                metadata: tui::PromptMetadata::default(),
            })) {
                crate::RuntimePromptSubmissionOutcome::Promoted { active, .. } => active,
                other => panic!("expected promoted turn, got {other:?}"),
            };

        let lifecycle = RuntimeTurnLifecycle::new(&active, "promoted");
        let snapshot = lifecycle.snapshot(runtime.queue_depth());
        assert_eq!(snapshot["turn_id"], active.runtime_turn_id);
        assert_eq!(snapshot["prompt_id"], active.prompt.id);
        assert_eq!(snapshot["phase"], "promoted");
        assert_eq!(active.prompt.text, "startup contract");
        assert_eq!(
            active.prompt.image_paths,
            vec![std::path::PathBuf::from("image.png")]
        );
    }
}
