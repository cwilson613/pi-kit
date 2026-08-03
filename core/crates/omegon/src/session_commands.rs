use std::path::Path;

use omegon_traits::{AgentEvent, ControlOutputResponse};
use tokio::sync::{broadcast, oneshot};

use crate::{CliRuntimeView, InteractiveAgentHost, InteractiveAgentState, control_runtime};

pub(crate) fn is_session_command(command: &crate::tui::TuiCommand) -> bool {
    matches!(
        command,
        crate::tui::TuiCommand::ListSessions { .. } | crate::tui::TuiCommand::NewSession { .. }
    )
}

pub(crate) struct SessionCommandContext<'a> {
    pub(crate) runtime_state: &'a mut InteractiveAgentState,
    pub(crate) agent: &'a mut InteractiveAgentHost,
    pub(crate) cli: CliRuntimeView<'a>,
    pub(crate) events_tx: &'a broadcast::Sender<AgentEvent>,
}

pub(crate) async fn handle(command: crate::tui::TuiCommand, context: SessionCommandContext<'_>) {
    match command {
        crate::tui::TuiCommand::ListSessions { respond_to } => {
            list_sessions(context.agent.cwd.as_path(), respond_to, context.events_tx);
        }
        crate::tui::TuiCommand::NewSession { respond_to } => {
            let response = control_runtime::new_session_response(
                context.runtime_state,
                context.agent,
                &context.cli,
                context.events_tx,
            )
            .await;
            respond(respond_to, response.accepted, response.output);
        }
        _ => unreachable!("session command classifier must guard dispatch"),
    }
}

fn list_sessions(
    cwd: &Path,
    respond_to: Option<oneshot::Sender<ControlOutputResponse>>,
    events_tx: &broadcast::Sender<AgentEvent>,
) {
    let text = control_runtime::list_sessions_message(cwd);
    let _ = events_tx.send(AgentEvent::SystemNotification {
        message: text.clone(),
    });
    let _ = events_tx.send(AgentEvent::AgentEnd);
    tracing::info!("{text}");
    respond(respond_to, true, Some(text));
}

fn respond(
    respond_to: Option<oneshot::Sender<ControlOutputResponse>>,
    accepted: bool,
    output: Option<String>,
) {
    if let Some(respond_to) = respond_to {
        let _ = respond_to.send(ControlOutputResponse { accepted, output });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifier_is_exactly_the_session_command_family() {
        assert!(is_session_command(&crate::tui::TuiCommand::ListSessions {
            respond_to: None,
        }));
        assert!(is_session_command(&crate::tui::TuiCommand::NewSession {
            respond_to: None,
        }));
        assert!(!is_session_command(
            &crate::tui::TuiCommand::ContextStatus { respond_to: None }
        ));
    }

    #[tokio::test]
    async fn list_sessions_emits_notification_end_and_response() {
        let dir = tempfile::tempdir().unwrap();
        let (events_tx, mut events_rx) = broadcast::channel(8);
        let (response_tx, response_rx) = oneshot::channel();

        list_sessions(dir.path(), Some(response_tx), &events_tx);

        assert!(matches!(
            events_rx.recv().await.unwrap(),
            AgentEvent::SystemNotification { .. }
        ));
        assert!(matches!(
            events_rx.recv().await.unwrap(),
            AgentEvent::AgentEnd
        ));
        let response = response_rx.await.unwrap();
        assert!(response.accepted);
        assert!(response.output.is_some());
    }
}
