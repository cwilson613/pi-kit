//! Host-owned and worker-owned state for an interactive Omegon session.
//!
//! The host remains on the session coordinator while `InteractiveAgentState`
//! moves into and out of each active turn worker.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub(crate) struct InteractiveAgentState {
    pub(crate) bus: crate::bus::EventBus,
    pub(crate) context_manager: crate::context::ContextManager,
    pub(crate) conversation: crate::conversation::ConversationState,
    pub(crate) inference_runtime: crate::inference_runtime::InferenceRuntimeState,
}

pub(crate) struct InteractiveAgentHost {
    pub(crate) session_id: String,
    pub(crate) instance_id: String,
    pub(crate) context_metrics: Arc<Mutex<crate::features::context::SharedContextMetrics>>,
    pub(crate) cwd: PathBuf,
    pub(crate) secrets: Arc<omegon_secrets::SecretsManager>,
    pub(crate) web_auth_state: crate::web::WebAuthState,
    pub(crate) dashboard_handles: crate::runtime_state::RuntimeStateHandles,
    pub(crate) resume_info: Option<crate::setup::ResumeInfo>,
    pub(crate) workspace_state: crate::setup::WorkspaceStartupState,
    pub(crate) runtime_generation: u64,
}

pub(crate) fn split_agent(
    agent: crate::setup::AgentSetup,
) -> (InteractiveAgentHost, InteractiveAgentState) {
    let host = InteractiveAgentHost {
        session_id: agent.session_id,
        instance_id: agent.instance_id,
        context_metrics: agent.context_metrics,
        cwd: agent.cwd,
        secrets: agent.secrets,
        web_auth_state: agent.web_auth_state,
        dashboard_handles: agent.dashboard_handles,
        resume_info: agent.resume_info,
        workspace_state: agent.workspace_state,
        runtime_generation: 1,
    };
    let state = InteractiveAgentState {
        bus: agent.bus,
        context_manager: agent.context_manager,
        conversation: agent.conversation,
        inference_runtime: agent.inference_runtime,
    };
    (host, state)
}
