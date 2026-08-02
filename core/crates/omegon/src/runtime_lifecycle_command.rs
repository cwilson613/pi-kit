use std::path::PathBuf;

use crate::tui;

#[derive(Debug)]
pub(crate) enum RuntimeLifecycleCommand {
    Quit,
    InstallUpdate {
        info: crate::update::UpdateInfo,
        args: Vec<String>,
    },
    RestartProcess {
        binary: PathBuf,
        args: Vec<String>,
    },
}

#[derive(Debug)]
pub(crate) enum ActiveWorkerLifecycleDisposition {
    QuitAfterTurn,
    DeferInstallUpdate {
        info: crate::update::UpdateInfo,
        args: Vec<String>,
    },
    RestartAfterTurn {
        binary: PathBuf,
        args: Vec<String>,
    },
}

pub(crate) enum LifecycleClassification {
    Lifecycle(RuntimeLifecycleCommand),
    Other(tui::TuiCommand),
}

pub(crate) fn classify(command: tui::TuiCommand) -> LifecycleClassification {
    match command {
        tui::TuiCommand::Quit => LifecycleClassification::Lifecycle(RuntimeLifecycleCommand::Quit),
        tui::TuiCommand::InstallUpdate { info, args } => {
            LifecycleClassification::Lifecycle(RuntimeLifecycleCommand::InstallUpdate {
                info,
                args,
            })
        }
        tui::TuiCommand::RestartProcess { binary, args } => {
            LifecycleClassification::Lifecycle(RuntimeLifecycleCommand::RestartProcess {
                binary,
                args,
            })
        }
        other => LifecycleClassification::Other(other),
    }
}

impl RuntimeLifecycleCommand {
    pub(crate) fn for_active_worker(self) -> ActiveWorkerLifecycleDisposition {
        match self {
            Self::Quit => ActiveWorkerLifecycleDisposition::QuitAfterTurn,
            Self::InstallUpdate { info, args } => {
                ActiveWorkerLifecycleDisposition::DeferInstallUpdate { info, args }
            }
            Self::RestartProcess { binary, args } => {
                ActiveWorkerLifecycleDisposition::RestartAfterTurn { binary, args }
            }
        }
    }
}

pub(crate) fn restart_snapshot(session_id: &str) -> omegon_traits::RuntimeLifecycleSnapshot {
    omegon_traits::RuntimeLifecycleSnapshot {
        operation_id: uuid::Uuid::new_v4().to_string(),
        kind: omegon_traits::RuntimeLifecycleKind::Restart,
        phase: omegon_traits::RuntimeLifecyclePhase::Restarting,
        message: "Saving session and restarting".into(),
        session_id: Some(session_id.to_string()),
        target_version: None,
        reconnect_required: true,
    }
}

pub(crate) fn update_download_snapshot(
    session_id: &str,
    target_version: &str,
) -> omegon_traits::RuntimeLifecycleSnapshot {
    omegon_traits::RuntimeLifecycleSnapshot {
        operation_id: uuid::Uuid::new_v4().to_string(),
        kind: omegon_traits::RuntimeLifecycleKind::UpdateInstall,
        phase: omegon_traits::RuntimeLifecyclePhase::Downloading,
        message: "Downloading and verifying update".into(),
        session_id: Some(session_id.to_string()),
        target_version: Some(target_version.to_string()),
        reconnect_required: false,
    }
}

pub(crate) fn mark_update_restarting(snapshot: &mut omegon_traits::RuntimeLifecycleSnapshot) {
    snapshot.phase = omegon_traits::RuntimeLifecyclePhase::Restarting;
    snapshot.message = "Update installed; saving session and restarting".into();
    snapshot.reconnect_required = true;
}

pub(crate) fn mark_update_failed(
    snapshot: &mut omegon_traits::RuntimeLifecycleSnapshot,
    error: &impl std::fmt::Display,
) {
    snapshot.phase = omegon_traits::RuntimeLifecyclePhase::Failed;
    snapshot.message = format!("Update failed; current version still running: {error}");
    snapshot.reconnect_required = false;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restart_snapshot_requires_same_session_reconnect() {
        let snapshot = restart_snapshot("session-1");
        assert_eq!(snapshot.kind, omegon_traits::RuntimeLifecycleKind::Restart);
        assert_eq!(
            snapshot.phase,
            omegon_traits::RuntimeLifecyclePhase::Restarting
        );
        assert_eq!(snapshot.session_id.as_deref(), Some("session-1"));
        assert!(snapshot.reconnect_required);
    }

    #[test]
    fn update_snapshot_transitions_preserve_operation_identity() {
        let mut snapshot = update_download_snapshot("session-1", "9.9.9");
        let operation_id = snapshot.operation_id.clone();
        assert_eq!(
            snapshot.phase,
            omegon_traits::RuntimeLifecyclePhase::Downloading
        );
        assert!(!snapshot.reconnect_required);

        mark_update_restarting(&mut snapshot);
        assert_eq!(snapshot.operation_id, operation_id);
        assert_eq!(
            snapshot.phase,
            omegon_traits::RuntimeLifecyclePhase::Restarting
        );
        assert!(snapshot.reconnect_required);

        mark_update_failed(&mut snapshot, &"network unavailable");
        assert_eq!(snapshot.operation_id, operation_id);
        assert_eq!(snapshot.phase, omegon_traits::RuntimeLifecyclePhase::Failed);
        assert!(!snapshot.reconnect_required);
        assert!(snapshot.message.contains("network unavailable"));
    }

    #[test]
    fn active_worker_lifecycle_commands_have_typed_dispositions() {
        let disposition = RuntimeLifecycleCommand::Quit.for_active_worker();
        assert!(matches!(
            disposition,
            ActiveWorkerLifecycleDisposition::QuitAfterTurn
        ));

        let binary = PathBuf::from("/tmp/omegon");
        let disposition = RuntimeLifecycleCommand::RestartProcess {
            binary: binary.clone(),
            args: vec!["--resume".into()],
        }
        .for_active_worker();
        match disposition {
            ActiveWorkerLifecycleDisposition::RestartAfterTurn {
                binary: actual_binary,
                args,
            } => {
                assert_eq!(actual_binary, binary);
                assert_eq!(args, vec!["--resume"]);
            }
            other => panic!("unexpected disposition: {other:?}"),
        }
    }
}
