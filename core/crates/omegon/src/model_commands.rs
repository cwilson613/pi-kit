use std::sync::{Arc, Mutex};

use tokio::sync::{RwLock, broadcast};

use crate::InteractiveAgentHost;
use crate::bridge::LlmBridge;
use crate::{control_runtime, inference_runtime, route, settings, tui};
use omegon_traits::AgentEvent;

pub(crate) fn is_model_command(command: &tui::TuiCommand) -> bool {
    matches!(
        command,
        tui::TuiCommand::ModelView { .. }
            | tui::TuiCommand::ModelList { .. }
            | tui::TuiCommand::SetModel { .. }
            | tui::TuiCommand::SetModelGrade { .. }
            | tui::TuiCommand::SetModelProvider { .. }
            | tui::TuiCommand::SetModelPolicy { .. }
            | tui::TuiCommand::ModelUnpin { .. }
            | tui::TuiCommand::SetThinking { .. }
    )
}

pub(crate) struct ModelCommandContext<'a> {
    pub(crate) agent: &'a mut InteractiveAgentHost,
    pub(crate) shared_settings: &'a Arc<Mutex<settings::Settings>>,
    pub(crate) bridge: &'a Arc<RwLock<Box<dyn LlmBridge>>>,
    pub(crate) route_controller: &'a Arc<route::RouteController>,
    pub(crate) inference_runtime: &'a inference_runtime::InferenceRuntimeState,
    pub(crate) bridge_model: &'a Arc<Mutex<Option<String>>>,
    pub(crate) events_tx: &'a broadcast::Sender<AgentEvent>,
}

pub(crate) async fn handle(command: tui::TuiCommand, context: ModelCommandContext<'_>) {
    let ModelCommandContext {
        agent,
        shared_settings,
        bridge,
        route_controller,
        inference_runtime,
        bridge_model,
        events_tx,
    } = context;
    let (response, respond_to, split_output) = match command {
        tui::TuiCommand::ModelView { respond_to } => {
            let response = control_runtime::model_view_response(shared_settings).await;
            finish(response, respond_to, false, events_tx);
            return;
        }
        tui::TuiCommand::ModelList { respond_to } => {
            let response = control_runtime::model_list_response().await;
            finish(response, respond_to, false, events_tx);
            return;
        }
        tui::TuiCommand::SetModel { model, respond_to } => {
            let inventory = inference_runtime.snapshot().await;
            let response = control_runtime::set_model_response(
                agent,
                shared_settings,
                bridge,
                Some(route_controller.clone()),
                &model,
                &inventory,
            )
            .await;
            if response.accepted {
                if let Ok(mut current) = bridge_model.lock() {
                    *current = None;
                }
                persist_intent(agent, route_controller, events_tx).await;
            }
            (response, respond_to, true)
        }
        tui::TuiCommand::SetModelGrade { grade, respond_to } => {
            let response = control_runtime::set_model_intent_control_response(
                Some(route_controller.clone()),
                &agent.cwd,
                &grade,
            )
            .await;
            if response.accepted {
                refresh_model_intent_route(
                    route_controller,
                    inference_runtime,
                    shared_settings,
                    bridge_model,
                    events_tx,
                )
                .await;
            }
            (response, respond_to, true)
        }
        tui::TuiCommand::SetModelProvider {
            provider,
            respond_to,
        } => {
            let response = control_runtime::set_model_provider_control_response(
                Some(route_controller.clone()),
                &agent.cwd,
                &provider,
            )
            .await;
            if response.accepted {
                refresh_model_intent_route(
                    route_controller,
                    inference_runtime,
                    shared_settings,
                    bridge_model,
                    events_tx,
                )
                .await;
            }
            (response, respond_to, false)
        }
        tui::TuiCommand::SetModelPolicy { policy, respond_to } => {
            let response = control_runtime::set_model_policy_control_response(
                Some(route_controller.clone()),
                &agent.cwd,
                &policy,
            )
            .await;
            if response.accepted {
                refresh_model_intent_route(
                    route_controller,
                    inference_runtime,
                    shared_settings,
                    bridge_model,
                    events_tx,
                )
                .await;
            }
            (response, respond_to, false)
        }
        tui::TuiCommand::ModelUnpin { respond_to } => {
            let snapshot = route_controller.clear_exact_model_override().await;
            if let Err(err) = settings::persist_model_intent(&agent.cwd, &snapshot.intent) {
                notify(events_tx, format!("Failed to persist model intent: {err}"));
            }
            refresh_model_intent_route(
                route_controller,
                inference_runtime,
                shared_settings,
                bridge_model,
                events_tx,
            )
            .await;
            let response = omegon_traits::SlashCommandResponse {
                accepted: true,
                output: Some(format!(
                    "Model exact override cleared — {}. Active route unchanged: {}",
                    snapshot.intent.summary(),
                    snapshot.serving_model().unwrap_or("disconnected")
                )),
            };
            (response, respond_to, false)
        }
        tui::TuiCommand::SetThinking { level, respond_to } => {
            let response =
                control_runtime::set_thinking_response(shared_settings, &agent.cwd, level).await;
            (response, respond_to, false)
        }
        _ => unreachable!("model command handler received non-model command"),
    };
    finish(response, respond_to, split_output, events_tx);
}

async fn refresh_model_intent_route(
    route_controller: &Arc<route::RouteController>,
    inference_runtime: &inference_runtime::InferenceRuntimeState,
    shared_settings: &settings::SharedSettings,
    bridge_model: &Arc<Mutex<Option<String>>>,
    events_tx: &broadcast::Sender<AgentEvent>,
) {
    let Some(snapshot) =
        crate::resolve_current_model_intent_route(
            route_controller,
            inference_runtime,
            shared_settings,
        )
        .await
    else {
        notify(
            events_tx,
            "No available model satisfies the updated model intent; active route unchanged.".into(),
        );
        return;
    };
    let serving_model = snapshot.serving_model().map(str::to_string);
    if let Ok(mut current) = bridge_model.lock() {
        *current = serving_model;
    }
}

async fn persist_intent(
    agent: &InteractiveAgentHost,
    route_controller: &route::RouteController,
    events_tx: &broadcast::Sender<AgentEvent>,
) {
    let snapshot = route_controller.snapshot().await;
    if let Err(err) = settings::persist_model_intent(&agent.cwd, &snapshot.intent) {
        notify(events_tx, format!("Failed to persist model intent: {err}"));
    }
}

fn finish(
    response: omegon_traits::SlashCommandResponse,
    respond_to: Option<tokio::sync::oneshot::Sender<omegon_traits::ControlOutputResponse>>,
    split_lines: bool,
    events_tx: &broadcast::Sender<AgentEvent>,
) {
    if let Some(output) = response.output.clone() {
        if split_lines {
            for line in output.split('\n') {
                notify(events_tx, line.to_string());
            }
        } else {
            notify(events_tx, output);
        }
    }
    if let Some(respond_to) = respond_to {
        let _ = respond_to.send(omegon_traits::ControlOutputResponse {
            accepted: response.accepted,
            output: response.output,
        });
    }
}

fn notify(events_tx: &broadcast::Sender<AgentEvent>, message: String) {
    let _ = events_tx.send(AgentEvent::SystemNotification { message });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifier_is_exactly_the_model_command_family() {
        assert!(is_model_command(&tui::TuiCommand::ModelList {
            respond_to: None
        }));
        assert!(is_model_command(&tui::TuiCommand::SetModelGrade {
            grade: "A".into(),
            respond_to: None,
        }));
        assert!(is_model_command(&tui::TuiCommand::SetThinking {
            level: crate::settings::ThinkingLevel::Medium,
            respond_to: None,
        }));
        assert!(!is_model_command(&tui::TuiCommand::Compact));
        assert!(!is_model_command(&tui::TuiCommand::Quit { confirmed: false }));
    }
}
