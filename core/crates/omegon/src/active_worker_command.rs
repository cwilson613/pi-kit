use crate::post_worker_completion::PostWorkerCompletionPolicy;
use crate::runtime_lifecycle_command;
use crate::runtime_prompt::RuntimePromptSubmission;
use crate::tui;
use crate::{InteractiveRuntimeSupervisor, RuntimePromptSubmissionOutcome};

#[derive(Debug)]
pub(crate) enum ActiveWorkerCommand {
    Submit(RuntimePromptSubmission),
    Cancel {
        submitted_by: String,
        via: &'static str,
    },
    Lifecycle(runtime_lifecycle_command::RuntimeLifecycleCommand),
    Defer(tui::TuiCommand),
}

#[derive(Debug)]
pub(crate) enum ActiveWorkerCommandEffect {
    PromptQueued {
        prompt_id: u64,
        requests_voice_close: bool,
    },
    Cancel {
        submitted_by: String,
        via: &'static str,
    },
    LifecycleRequested,
    Deferred(tui::TuiCommand),
}

pub(crate) fn apply(
    command: ActiveWorkerCommand,
    runtime: &mut InteractiveRuntimeSupervisor,
    completion_policy: &mut PostWorkerCompletionPolicy,
) -> ActiveWorkerCommandEffect {
    match command {
        ActiveWorkerCommand::Submit(prompt) => {
            let prompt_id = match runtime.submit(prompt) {
                RuntimePromptSubmissionOutcome::Queued { prompt_id, .. } => prompt_id,
                RuntimePromptSubmissionOutcome::Promoted { .. } => {
                    unreachable!("active worker submission cannot promote")
                }
            };
            ActiveWorkerCommandEffect::PromptQueued {
                prompt_id,
                requests_voice_close: runtime
                    .queue
                    .get(prompt_id)
                    .is_some_and(|prompt| prompt.requests_voice_close()),
            }
        }
        ActiveWorkerCommand::Cancel { submitted_by, via } => {
            ActiveWorkerCommandEffect::Cancel { submitted_by, via }
        }
        ActiveWorkerCommand::Lifecycle(command) => {
            completion_policy.request_lifecycle(command.for_active_worker());
            ActiveWorkerCommandEffect::LifecycleRequested
        }
        ActiveWorkerCommand::Defer(command) => ActiveWorkerCommandEffect::Deferred(command),
    }
}

pub(crate) fn classify(command: tui::TuiCommand) -> ActiveWorkerCommand {
    match command {
        tui::TuiCommand::SubmitPrompt(prompt) => {
            ActiveWorkerCommand::Submit(RuntimePromptSubmission::from_tui(prompt))
        }
        tui::TuiCommand::VoicePrompt { text, metadata } => {
            ActiveWorkerCommand::Submit(RuntimePromptSubmission::from_voice(text, metadata))
        }
        tui::TuiCommand::CancelActiveTurn { submitted_by, via } => {
            ActiveWorkerCommand::Cancel { submitted_by, via }
        }
        other => match runtime_lifecycle_command::classify(other) {
            runtime_lifecycle_command::LifecycleClassification::Lifecycle(command) => {
                ActiveWorkerCommand::Lifecycle(command)
            }
            runtime_lifecycle_command::LifecycleClassification::Other(other) => {
                ActiveWorkerCommand::Defer(other)
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_prompt_normalizes_to_queued_voice_submission() {
        let command = tui::TuiCommand::VoicePrompt {
            text: "  status report  ".into(),
            metadata: tui::VoicePromptMetadata::default(),
        };

        let ActiveWorkerCommand::Submit(prompt) = classify(command) else {
            panic!("voice prompt should become a submission");
        };
        assert_eq!(prompt.text, "🎙 status report");
        assert_eq!(prompt.actor.label, "voice");
        assert_eq!(prompt.via, crate::runtime_prompt::ControlSurface::Internal);
        assert_eq!(
            prompt.queue_mode,
            crate::runtime_prompt::QueueMode::UntilReady
        );
        assert!(prompt.metadata.voice.is_some());
    }

    #[test]
    fn apply_submission_returns_queue_effect_and_voice_close_policy() {
        let mut runtime = InteractiveRuntimeSupervisor::default();
        runtime.submit(RuntimePromptSubmission::from_tui(tui::PromptSubmission {
            text: "active".into(),
            image_paths: Vec::new(),
            submitted_by: "operator".into(),
            via: "tui",
            queue_mode: tui::PromptQueueMode::UntilReady,
            metadata: tui::PromptMetadata::default(),
        }));
        let mut completion = PostWorkerCompletionPolicy::default();
        let effect = apply(
            classify(tui::TuiCommand::VoicePrompt {
                text: "shutdown".into(),
                metadata: tui::VoicePromptMetadata {
                    close_session_requested: Some(true),
                    radio_cue: Some("over_and_out".into()),
                    ..Default::default()
                },
            }),
            &mut runtime,
            &mut completion,
        );
        assert!(matches!(
            effect,
            ActiveWorkerCommandEffect::PromptQueued {
                prompt_id: 2,
                requests_voice_close: true
            }
        ));
        assert_eq!(runtime.queue_depth(), 1);
    }

    #[test]
    fn apply_lifecycle_updates_completion_policy_and_requests_cancellation() {
        let mut runtime = InteractiveRuntimeSupervisor::default();
        let mut completion = PostWorkerCompletionPolicy::default();
        let effect = apply(
            classify(tui::TuiCommand::Quit),
            &mut runtime,
            &mut completion,
        );
        assert!(matches!(
            effect,
            ActiveWorkerCommandEffect::LifecycleRequested
        ));
        assert!(matches!(
            completion.finish(),
            crate::post_worker_completion::PostWorkerDisposition::Exit
        ));
    }

    #[test]
    fn active_worker_control_commands_have_typed_dispositions() {
        assert!(matches!(
            classify(tui::TuiCommand::Quit),
            ActiveWorkerCommand::Lifecycle(
                runtime_lifecycle_command::RuntimeLifecycleCommand::Quit
            )
        ));
        assert!(matches!(
            classify(tui::TuiCommand::CancelActiveTurn {
                submitted_by: "operator".into(),
                via: "tui",
            }),
            ActiveWorkerCommand::Cancel { submitted_by, via }
                if submitted_by == "operator" && via == "tui"
        ));
    }
}
