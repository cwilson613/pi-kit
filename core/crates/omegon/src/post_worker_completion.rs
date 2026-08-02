use std::path::PathBuf;

use crate::runtime_lifecycle_command::ActiveWorkerLifecycleDisposition;
use crate::tui;

#[derive(Debug, Default)]
pub(crate) struct PostWorkerCompletionPolicy {
    pending: PendingCompletion,
}

#[derive(Debug, Default)]
enum PendingCompletion {
    #[default]
    PromoteNext,
    Quit,
    InstallUpdate {
        info: crate::update::UpdateInfo,
        args: Vec<String>,
    },
    Restart {
        binary: PathBuf,
        args: Vec<String>,
    },
}

#[derive(Debug)]
pub(crate) enum PostWorkerDisposition {
    PromoteNext,
    DispatchDeferred(tui::TuiCommand),
    Exit,
    ExitForRestart { binary: PathBuf, args: Vec<String> },
}

impl PostWorkerCompletionPolicy {
    pub(crate) fn request_channel_close(&mut self) {
        self.pending = PendingCompletion::Quit;
    }

    pub(crate) fn request_lifecycle(&mut self, disposition: ActiveWorkerLifecycleDisposition) {
        self.pending = match disposition {
            ActiveWorkerLifecycleDisposition::QuitAfterTurn => PendingCompletion::Quit,
            ActiveWorkerLifecycleDisposition::DeferInstallUpdate { info, args } => {
                PendingCompletion::InstallUpdate { info, args }
            }
            ActiveWorkerLifecycleDisposition::RestartAfterTurn { binary, args } => {
                PendingCompletion::Restart { binary, args }
            }
        };
    }

    pub(crate) fn finish(self) -> PostWorkerDisposition {
        match self.pending {
            PendingCompletion::PromoteNext => PostWorkerDisposition::PromoteNext,
            PendingCompletion::Quit => PostWorkerDisposition::Exit,
            PendingCompletion::InstallUpdate { info, args } => {
                PostWorkerDisposition::DispatchDeferred(tui::TuiCommand::InstallUpdate {
                    info,
                    args,
                })
            }
            PendingCompletion::Restart { binary, args } => {
                PostWorkerDisposition::ExitForRestart { binary, args }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn update_info() -> crate::update::UpdateInfo {
        crate::update::UpdateInfo {
            current: "1.0.0".into(),
            latest: "1.1.0".into(),
            download_url: "https://example.invalid/omegon".into(),
            signature_url: "https://example.invalid/omegon.sig".into(),
            certificate_url: "https://example.invalid/omegon.pem".into(),
            release_notes: String::new(),
            is_newer: true,
        }
    }

    #[test]
    fn default_completion_promotes_next_prompt() {
        assert!(matches!(
            PostWorkerCompletionPolicy::default().finish(),
            PostWorkerDisposition::PromoteNext
        ));
    }

    #[test]
    fn quit_and_channel_close_exit_after_worker_completion() {
        let mut policy = PostWorkerCompletionPolicy::default();
        policy.request_channel_close();
        assert!(matches!(policy.finish(), PostWorkerDisposition::Exit));

        let mut policy = PostWorkerCompletionPolicy::default();
        policy.request_lifecycle(ActiveWorkerLifecycleDisposition::QuitAfterTurn);
        assert!(matches!(policy.finish(), PostWorkerDisposition::Exit));
    }

    #[test]
    fn update_is_dispatched_after_worker_instead_of_exiting_before_execution() {
        let mut policy = PostWorkerCompletionPolicy::default();
        policy.request_lifecycle(ActiveWorkerLifecycleDisposition::DeferInstallUpdate {
            info: update_info(),
            args: vec!["--resume".into()],
        });

        match policy.finish() {
            PostWorkerDisposition::DispatchDeferred(tui::TuiCommand::InstallUpdate {
                info,
                args,
            }) => {
                assert_eq!(info.latest, "1.1.0");
                assert_eq!(args, vec!["--resume"]);
            }
            other => panic!("unexpected disposition: {other:?}"),
        }
    }

    #[test]
    fn restart_payload_is_retained_for_process_exit() {
        let binary = PathBuf::from("/tmp/omegon");
        let mut policy = PostWorkerCompletionPolicy::default();
        policy.request_lifecycle(ActiveWorkerLifecycleDisposition::RestartAfterTurn {
            binary: binary.clone(),
            args: vec!["--session".into(), "abc".into()],
        });

        match policy.finish() {
            PostWorkerDisposition::ExitForRestart {
                binary: actual,
                args,
            } => {
                assert_eq!(actual, binary);
                assert_eq!(args, vec!["--session", "abc"]);
            }
            other => panic!("unexpected disposition: {other:?}"),
        }
    }
}
