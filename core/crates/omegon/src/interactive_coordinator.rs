//! Interactive command scheduling and promoted-turn coordination.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tokio::sync::{RwLock, broadcast, mpsc};

use crate::bridge::LlmBridge;
use crate::interactive_turn_execution::{InteractiveRuntimeResources, InteractiveTurnExecution};
use crate::post_worker_completion::PostWorkerDisposition;
use crate::runtime_prompt::RuntimePromptSubmission;
use crate::runtime_supervisor::{InteractiveRuntimeSupervisor, RuntimePromptSubmissionOutcome};
use crate::tui;
use crate::{AgentEvent, InteractiveAgentState};

#[derive(Debug)]
pub(crate) enum TurnChainDisposition {
    Continue,
    DispatchDeferred(tui::TuiCommand),
    Exit,
    ExitForRestart { binary: PathBuf, args: Vec<String> },
}

pub(crate) async fn next_command(
    deferred: &mut VecDeque<tui::TuiCommand>,
    command_rx: &mut mpsc::Receiver<tui::TuiCommand>,
) -> Option<tui::TuiCommand> {
    if let Some(command) = deferred.pop_front() {
        Some(command)
    } else {
        command_rx.recv().await
    }
}

pub(crate) fn normalize_ingress(command: tui::TuiCommand) -> tui::TuiCommand {
    match command {
        tui::TuiCommand::VoicePrompt { text, metadata } => tui::TuiCommand::SubmitPrompt(
            RuntimePromptSubmission::from_voice(text, metadata).into_tui(),
        ),
        other => other,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn execute_submission(
    submission: RuntimePromptSubmission,
    mut state: InteractiveAgentState,
    runtime: &mut InteractiveRuntimeSupervisor,
    resources: &InteractiveRuntimeResources,
    shared_settings: Arc<Mutex<crate::settings::Settings>>,
    shared_cancel: tui::SharedCancel,
    pending_compact: &Arc<std::sync::atomic::AtomicBool>,
    bridge: Arc<RwLock<Box<dyn LlmBridge>>>,
    events_tx: &broadcast::Sender<AgentEvent>,
    dashboard_handles: &crate::runtime_state::RuntimeStateHandles,
    command_rx: &mut mpsc::Receiver<tui::TuiCommand>,
    deferred_commands: &mut VecDeque<tui::TuiCommand>,
) -> anyhow::Result<(InteractiveAgentState, TurnChainDisposition)> {
    let submitted_by = submission.actor.display_label().to_string();
    let via = submission.via.label();
    let first_active = match runtime.submit(submission) {
        RuntimePromptSubmissionOutcome::Queued {
            prompt_id,
            queue_depth,
        } => {
            tracing::info!(
                prompt_id,
                queue_depth,
                active_turn_id = runtime.turns.current().map(|active| active.runtime_turn_id),
                submitted_by,
                via,
                "prompt queued behind active interactive turn"
            );
            runtime.emit_queue_notification(events_tx, prompt_id);
            return Ok((state, TurnChainDisposition::Continue));
        }
        RuntimePromptSubmissionOutcome::Promoted { active, .. } => Some(*active),
    };

    let mut next_active = first_active;
    while let Some(active) = next_active
        .take()
        .or_else(|| runtime.maybe_start_next_turn())
    {
        let mut lifecycle =
            crate::active_worker_startup::prepare(&active, runtime, events_tx, dashboard_handles);
        crate::stop_voice_session_if_requested(&active.prompt, &state.bus, events_tx).await;

        let execution = InteractiveTurnExecution::new(
            resources,
            shared_settings.clone(),
            shared_cancel.clone(),
            pending_compact,
        );
        let mut turn_task = execution.spawn(
            state,
            bridge.clone(),
            events_tx.clone(),
            active,
            lifecycle.clone(),
        );
        let result = crate::active_worker_run::run(
            &mut turn_task,
            crate::active_worker_run::ActiveWorkerRunContext {
                command_rx,
                runtime,
                shared_cancel: &shared_cancel,
                events_tx,
                deferred_commands,
                lifecycle: &mut lifecycle,
            },
        )
        .await;
        let (returned_state, completion_policy) = match result {
            Ok(result) => result,
            Err(error) => {
                crate::mark_interactive_session_busy(dashboard_handles, false);
                let _ = events_tx.send(AgentEvent::SystemNotification {
                    message: error.to_string(),
                });
                let _ = events_tx.send(AgentEvent::AgentEnd);
                return Err(error);
            }
        };
        state = returned_state;
        crate::active_worker_completion::complete(
            runtime,
            &mut lifecycle,
            events_tx,
            dashboard_handles,
        );

        match completion_policy.finish() {
            PostWorkerDisposition::PromoteNext => {}
            PostWorkerDisposition::DispatchDeferred(command) => {
                return Ok((state, TurnChainDisposition::DispatchDeferred(command)));
            }
            PostWorkerDisposition::Exit => return Ok((state, TurnChainDisposition::Exit)),
            PostWorkerDisposition::ExitForRestart { binary, args } => {
                return Ok((state, TurnChainDisposition::ExitForRestart { binary, args }));
            }
        }
    }
    Ok((state, TurnChainDisposition::Continue))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_ingress_is_normalized_once() {
        let command = normalize_ingress(tui::TuiCommand::VoicePrompt {
            text: "  status  ".into(),
            metadata: tui::VoicePromptMetadata::default(),
        });
        let tui::TuiCommand::SubmitPrompt(prompt) = command else {
            panic!("voice ingress was not normalized");
        };
        assert_eq!(prompt.text, "🎙 status");
        assert_eq!(prompt.submitted_by, "voice");
    }
}
