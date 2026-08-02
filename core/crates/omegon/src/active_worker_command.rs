use crate::runtime_lifecycle_command;
use crate::runtime_prompt::RuntimePromptSubmission;
use crate::tui;

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
