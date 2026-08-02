use crate::tui;

#[derive(Debug)]
pub(crate) enum ActiveWorkerCommand {
    Submit(tui::PromptSubmission),
    Cancel {
        submitted_by: String,
        via: &'static str,
    },
    Quit,
    InstallUpdate {
        info: crate::update::UpdateInfo,
        args: Vec<String>,
    },
    RestartProcess {
        binary: std::path::PathBuf,
        args: Vec<String>,
    },
    Defer(tui::TuiCommand),
}

pub(crate) fn classify(command: tui::TuiCommand) -> ActiveWorkerCommand {
    match normalize(command) {
        tui::TuiCommand::SubmitPrompt(prompt) => ActiveWorkerCommand::Submit(prompt),
        tui::TuiCommand::CancelActiveTurn { submitted_by, via } => {
            ActiveWorkerCommand::Cancel { submitted_by, via }
        }
        tui::TuiCommand::Quit => ActiveWorkerCommand::Quit,
        tui::TuiCommand::InstallUpdate { info, args } => {
            ActiveWorkerCommand::InstallUpdate { info, args }
        }
        tui::TuiCommand::RestartProcess { binary, args } => {
            ActiveWorkerCommand::RestartProcess { binary, args }
        }
        other => ActiveWorkerCommand::Defer(other),
    }
}

fn normalize(command: tui::TuiCommand) -> tui::TuiCommand {
    match command {
        tui::TuiCommand::VoicePrompt { text, metadata } => {
            tui::TuiCommand::SubmitPrompt(tui::PromptSubmission {
                text: format!("🎙 {}", text.trim()),
                image_paths: Vec::new(),
                submitted_by: "voice".to_string(),
                via: "voice",
                queue_mode: tui::PromptQueueMode::UntilReady,
                metadata: tui::PromptMetadata {
                    voice: Some(metadata),
                },
            })
        }
        other => other,
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
        assert_eq!(prompt.submitted_by, "voice");
        assert_eq!(prompt.via, "voice");
        assert_eq!(prompt.queue_mode, tui::PromptQueueMode::UntilReady);
        assert!(prompt.metadata.voice.is_some());
    }

    #[test]
    fn active_worker_control_commands_have_typed_dispositions() {
        assert!(matches!(
            classify(tui::TuiCommand::Quit),
            ActiveWorkerCommand::Quit
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
