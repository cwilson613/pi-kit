//! Runtime resources and per-turn execution configuration for interactive work.
//!
//! Frontends own command ingress. This module owns the stable resources used to
//! construct an agent-loop execution for each promoted runtime turn.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::bridge::LlmBridge;
use crate::runtime_turn::{ActiveTurnMeta, RuntimeTurnLifecycle};
use crate::{
    AgentEvent, InteractiveAgentState, bootstrap, r#loop, ollama, providers, route, settings, tui,
};
use tokio::sync::{RwLock, broadcast};

#[derive(Clone)]
pub(crate) struct InteractiveRuntimeResources {
    pub(crate) cwd: PathBuf,
    pub(crate) secrets: Arc<omegon_secrets::SecretsManager>,
    pub(crate) context_metrics: Arc<Mutex<crate::features::context::SharedContextMetrics>>,
    pub(crate) bridge_model: Arc<Mutex<Option<String>>>,
    pub(crate) route_controller: Arc<route::RouteController>,
}

pub(crate) struct InteractiveTurnExecution {
    pub(crate) loop_config: r#loop::LoopConfig,
    pub(crate) shared_settings: Arc<Mutex<settings::Settings>>,
    pub(crate) shared_cancel: tui::SharedCancel,
    pub(crate) context_metrics: Arc<Mutex<crate::features::context::SharedContextMetrics>>,
}

impl InteractiveTurnExecution {
    pub(crate) fn spawn(
        self,
        state: InteractiveAgentState,
        bridge: Arc<RwLock<Box<dyn LlmBridge>>>,
        events_tx: broadcast::Sender<AgentEvent>,
        active: ActiveTurnMeta,
        lifecycle: RuntimeTurnLifecycle,
    ) -> tokio::task::JoinHandle<InteractiveAgentState> {
        tokio::task::spawn_local(crate::runtime_turn_execution::execute(
            state, self, bridge, events_tx, active, lifecycle,
        ))
    }

    pub(crate) fn new(
        runtime: &InteractiveRuntimeResources,
        shared_settings: Arc<Mutex<settings::Settings>>,
        shared_cancel: tui::SharedCancel,
        pending_compact: &Arc<std::sync::atomic::AtomicBool>,
    ) -> Self {
        Self {
            loop_config: runtime.build_loop_config(&shared_settings, pending_compact),
            shared_settings,
            shared_cancel,
            context_metrics: runtime.context_metrics.clone(),
        }
    }
}

impl InteractiveRuntimeResources {
    pub(crate) fn build_loop_config(
        &self,
        shared_settings: &Arc<Mutex<settings::Settings>>,
        pending_compact: &Arc<std::sync::atomic::AtomicBool>,
    ) -> r#loop::LoopConfig {
        let model = shared_settings
            .lock()
            .map(|settings| settings.model.clone())
            .unwrap_or_default();

        let ollama_manager = if providers::infer_provider_id(&model) == "ollama" {
            Some(ollama::OllamaManager::new())
        } else {
            None
        };

        bootstrap::build_loop_config(
            shared_settings,
            &self.cwd,
            &model,
            bootstrap::LoopConfigOverrides {
                secrets: Some(self.secrets.clone()),
                force_compact: Some(pending_compact.clone()),
                allow_commit_nudge: true,
                ollama_manager,
                bridge_model: self
                    .bridge_model
                    .lock()
                    .ok()
                    .and_then(|guard| guard.clone()),
                route_controller: Some(self.route_controller.clone()),
                ..Default::default()
            },
        )
    }
}
