use std::path::Path;
use std::sync::Arc;

use tokio::sync::{broadcast, oneshot};

use crate::auth;
use crate::bridge::LlmBridge;
use crate::providers;
use crate::session;
use crate::settings;
use crate::{CliRuntimeView, InteractiveAgentHost, InteractiveAgentState};
use omegon_traits::{AgentEvent, SlashCommandResponse};

pub struct ControlContext<'a> {
    pub runtime_state: &'a mut InteractiveAgentState,
    pub agent: &'a mut InteractiveAgentHost,
    pub shared_settings: &'a settings::SharedSettings,
    pub bridge: &'a Arc<tokio::sync::RwLock<Box<dyn LlmBridge>>>,
    pub login_prompt_tx:
        &'a std::sync::Arc<tokio::sync::Mutex<Option<oneshot::Sender<String>>>>,
    pub events_tx: &'a broadcast::Sender<AgentEvent>,
    pub cli: &'a CliRuntimeView<'a>,
}

pub enum ControlRequest {
    ModelList,
    SetModel { requested_model: String },
    SetThinking { level: crate::settings::ThinkingLevel },
    ContextStatus,
    NewSession,
    ListSessions,
    AuthStatus,
    AuthUnlock,
    AuthLogin { provider: String },
    AuthLogout { provider: String },
}

pub async fn execute_control(
    ctx: &mut ControlContext<'_>,
    request: ControlRequest,
) -> SlashCommandResponse {
    match request {
        ControlRequest::ModelList => model_list_response().await,
        ControlRequest::SetModel { requested_model } => {
            set_model_response(ctx.agent, ctx.shared_settings, ctx.bridge, &requested_model).await
        }
        ControlRequest::SetThinking { level } => {
            set_thinking_response(ctx.shared_settings, level).await
        }
        ControlRequest::ContextStatus => {
            context_status_response(ctx.runtime_state, ctx.shared_settings).await
        }
        ControlRequest::NewSession => {
            new_session_response(ctx.runtime_state, ctx.agent, ctx.cli, ctx.events_tx).await
        }
        ControlRequest::ListSessions => list_sessions_response(ctx.agent).await,
        ControlRequest::AuthStatus => auth_status_response().await,
        ControlRequest::AuthUnlock => auth_unlock_response().await,
        ControlRequest::AuthLogin { provider } => {
            auth_login_response(
                ctx.shared_settings,
                ctx.bridge,
                ctx.login_prompt_tx,
                ctx.events_tx,
                ctx.cli,
                &provider,
            )
            .await
        }
        ControlRequest::AuthLogout { provider } => auth_logout_response(&provider).await,
    }
}

pub fn list_sessions_message(cwd: &Path) -> String {
    let sessions = session::list_sessions(cwd);
    if sessions.is_empty() {
        "No saved sessions for this directory.".to_string()
    } else {
        let lines: Vec<String> = sessions
            .iter()
            .take(10)
            .map(|s| {
                format!(
                    "  {} — {} turns, {} tools — {}",
                    s.meta.session_id, s.meta.turns, s.meta.tool_calls, s.meta.last_prompt_snippet
                )
            })
            .collect();
        format!("Recent sessions:\n{}", lines.join("\n"))
    }
}

pub async fn model_view_response(
    shared_settings: &settings::SharedSettings,
) -> SlashCommandResponse {
    let s = shared_settings.lock().unwrap().clone();
    let provider = s.provider().to_string();
    let connected = if s.provider_connected { "Yes" } else { "No" };
    let thinking = {
        let raw = s.thinking.as_str();
        let mut chars = raw.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            None => String::new(),
        }
    };
    SlashCommandResponse {
        accepted: true,
        output: Some(format!(
            "Model\n  Current Model:   {}\n  Provider:        {}\n  Connected:       {}\n  Context Window:  {} tokens\n  Context Class:   {}\n  Thinking Level:  {}\n\nActions\n  /model list                Show available models\n  /model <provider:model>    Switch model\n  /think <level>             Change reasoning depth\n  /context                   Show context posture",
            s.model,
            provider,
            connected,
            s.context_window,
            s.context_class.label(),
            thinking,
        )),
    }
}

pub async fn model_list_response() -> SlashCommandResponse {
    let catalog = crate::tui::model_catalog::ModelCatalog::discover();
    let mut output = String::from("Available Models\n");
    for (provider_name, models) in &catalog.providers {
        output.push_str(&format!("\n{}\n", provider_name));
        for model in models {
            output.push_str(&format!("  {} ({})\n", model.name, model.id));
        }
    }
    SlashCommandResponse {
        accepted: true,
        output: Some(output),
    }
}

pub async fn set_model_response(
    agent: &mut InteractiveAgentHost,
    shared_settings: &settings::SharedSettings,
    bridge: &Arc<tokio::sync::RwLock<Box<dyn LlmBridge>>>,
    requested_model: &str,
) -> SlashCommandResponse {
    let effective_model = providers::resolve_execution_model_spec(requested_model)
        .await
        .unwrap_or_else(|| requested_model.to_string());
    let (old_model, old_provider) = shared_settings
        .lock()
        .ok()
        .map(|s| {
            (
                s.model.clone(),
                crate::providers::infer_provider_id(&s.model),
            )
        })
        .unwrap_or_else(|| (String::new(), String::new()));
    let new_provider = crate::providers::infer_provider_id(&effective_model);
    if let Ok(mut s) = shared_settings.lock() {
        s.set_model(&effective_model);
        let mut profile = settings::Profile::load(&agent.cwd);
        profile.capture_from(&s);
        let _ = profile.save(&agent.cwd);
    }
    let mut messages = Vec::new();
    if effective_model != requested_model {
        let provider_label = crate::auth::provider_by_id(&new_provider)
            .map(|p| p.display_name)
            .unwrap_or(new_provider.as_str());
        messages.push(format!(
            "Requested {requested_model}; using executable route {effective_model} via {provider_label}."
        ));
    }
    if old_provider != new_provider {
        let provider = crate::providers::infer_provider_id(&effective_model);
        if let Some(new_bridge) = providers::auto_detect_bridge(&effective_model).await {
            let mut guard = bridge.write().await;
            *guard = new_bridge;
            if let Ok(mut s) = shared_settings.lock() {
                s.provider_connected = true;
            }
            let provider_label = crate::auth::provider_by_id(&provider)
                .map(|p| p.display_name)
                .unwrap_or(provider.as_str());
            messages.push(format!(
                "Provider switched to {provider_label} ({effective_model})."
            ));
        } else {
            if let Ok(mut s) = shared_settings.lock() {
                s.provider_connected = false;
            }
            let provider_label = crate::auth::provider_by_id(&provider)
                .map(|p| p.display_name)
                .unwrap_or(provider.as_str());
            messages.push(format!(
                "⚠ No credentials for {provider_label}. Use /login to authenticate."
            ));
        }
    } else if old_model != effective_model {
        let provider_label = crate::auth::provider_by_id(&new_provider)
            .map(|p| p.display_name)
            .unwrap_or(new_provider.as_str());
        messages.push(format!(
            "Model switched to {effective_model} via {provider_label}."
        ));
    }
    SlashCommandResponse {
        accepted: true,
        output: Some(if messages.is_empty() {
            format!("Model unchanged: {effective_model}")
        } else {
            messages.join("\n")
        }),
    }
}

pub async fn set_thinking_response(
    shared_settings: &settings::SharedSettings,
    level: crate::settings::ThinkingLevel,
) -> SlashCommandResponse {
    if let Ok(mut s) = shared_settings.lock() {
        s.thinking = level;
    }
    SlashCommandResponse {
        accepted: true,
        output: Some(format!("Thinking → {} {}", level.icon(), level.as_str())),
    }
}

pub async fn context_status_response(
    runtime_state: &InteractiveAgentState,
    shared_settings: &settings::SharedSettings,
) -> SlashCommandResponse {
    let est = runtime_state.conversation.estimate_tokens();
    let settings = shared_settings.lock().unwrap();
    let ctx_window = settings.context_window;
    let pct = if ctx_window > 0 {
        ((est as f64 / ctx_window as f64) * 100.0).min(100.0) as u32
    } else {
        0
    };
    SlashCommandResponse {
        accepted: true,
        output: Some(format!(
            "Context: {}/{} tokens ({}%)\nPolicy: {}\nModel: {}\nThinking: {}",
            est,
            ctx_window,
            pct,
            settings.effective_requested_class().label(),
            settings.context_class.label(),
            settings.thinking.as_str()
        )),
    }
}

pub async fn new_session_response(
    runtime_state: &mut InteractiveAgentState,
    agent: &mut InteractiveAgentHost,
    cli: &CliRuntimeView<'_>,
    events_tx: &broadcast::Sender<AgentEvent>,
) -> SlashCommandResponse {
    if !cli.no_session {
        let _ = session::save_session(
            &runtime_state.conversation,
            &agent.cwd,
            Some(agent.session_id.as_str()),
        );
    }
    runtime_state.conversation = crate::conversation::ConversationState::new();
    agent.session_id = crate::session::allocate_session_id();
    agent.resume_info = None;
    let _ = events_tx.send(AgentEvent::SessionReset);
    SlashCommandResponse {
        accepted: true,
        output: Some("Started a fresh session.".to_string()),
    }
}

pub async fn list_sessions_response(agent: &InteractiveAgentHost) -> SlashCommandResponse {
    SlashCommandResponse {
        accepted: true,
        output: Some(list_sessions_message(&agent.cwd)),
    }
}

pub async fn auth_status_response() -> SlashCommandResponse {
    let status = auth::probe_all_providers().await;
    SlashCommandResponse {
        accepted: true,
        output: Some(format_auth_status(&status)),
    }
}

pub async fn auth_unlock_response() -> SlashCommandResponse {
    SlashCommandResponse {
        accepted: true,
        output: Some("🔒 Secrets store unlock not yet implemented".to_string()),
    }
}

pub async fn auth_login_response(
    shared_settings: &settings::SharedSettings,
    bridge: &Arc<tokio::sync::RwLock<Box<dyn LlmBridge>>>,
    login_prompt_tx: &std::sync::Arc<tokio::sync::Mutex<Option<oneshot::Sender<String>>>>,
    events_tx: &broadcast::Sender<AgentEvent>,
    cli: &CliRuntimeView<'_>,
    provider: &str,
) -> SlashCommandResponse {
    let provider = provider.trim();
    let provider = if provider.is_empty() { "anthropic" } else { provider };
    if provider == "openai" {
        return SlashCommandResponse {
            accepted: false,
            output: Some(
                "OpenAI API login is interactive-only in the TUI. Use /login in the terminal session or set OPENAI_API_KEY."
                    .to_string(),
            ),
        };
    }
    if login_prompt_tx.lock().await.is_some() {
        return SlashCommandResponse {
            accepted: false,
            output: Some("Login is already waiting for interactive input in the TUI.".to_string()),
        };
    }
    let events_tx_clone = events_tx.clone();
    let progress_tx = events_tx.clone();
    let prompt_tx_for_login = events_tx.clone();
    let login_prompt_slot = login_prompt_tx.clone();
    let provider_clone = provider.to_string();
    let bridge_clone = bridge.clone();
    let model_for_redetect = shared_settings
        .lock()
        .ok()
        .map(|s| s.model.clone())
        .unwrap_or_else(|| cli.model.to_string());
    let settings_for_login = shared_settings.clone();
    tokio::spawn(async move {
        let progress: auth::LoginProgress = Box::new(move |msg| {
            let _ = progress_tx.send(AgentEvent::SystemNotification {
                message: msg.to_string(),
            });
        });
        let prompt: auth::LoginPrompt = Box::new(move |msg| {
            let slot = login_prompt_slot.clone();
            let tx = prompt_tx_for_login.clone();
            Box::pin(async move {
                let (otx, orx) = tokio::sync::oneshot::channel();
                {
                    let mut guard = slot.lock().await;
                    *guard = Some(otx);
                }
                let _ = tx.send(AgentEvent::SystemNotification { message: msg });
                orx.await
                    .map_err(|_| anyhow::anyhow!("Login prompt cancelled"))
            })
        });
        let result = match provider_clone.as_str() {
            "anthropic" | "claude" => {
                auth::login_anthropic_with_callbacks(progress, prompt).await
            }
            "openai-codex" | "chatgpt" | "codex" => {
                auth::login_openai_with_callbacks(progress, prompt).await
            }
            "openai" => Err(anyhow::anyhow!(
                "OpenAI API login in the TUI uses hidden API-key entry. Run /login and choose OpenAI API, or set OPENAI_API_KEY."
            )),
            "openrouter" => Err(anyhow::anyhow!(
                "OpenRouter login in the TUI uses hidden API-key entry. Run /login and choose OpenRouter, or set OPENROUTER_API_KEY."
            )),
            "ollama-cloud" => Err(anyhow::anyhow!(
                "Ollama Cloud login in the TUI uses hidden API-key entry. Run /login and choose Ollama Cloud, or set OLLAMA_API_KEY."
            )),
            _ => Err(anyhow::anyhow!(
                "Unknown provider: {}. Use: anthropic, openai, openai-codex, openrouter, ollama-cloud",
                provider_clone
            )),
        };
        let provider_label = crate::auth::provider_by_id(&provider_clone)
            .map(|p| p.display_name)
            .unwrap_or(provider_clone.as_str())
            .to_string();
        let message = match &result {
            Ok(_) => format!("✓ Successfully logged in to {provider_label}"),
            Err(e) => format!("❌ Login failed: {}", e),
        };
        let _ = events_tx_clone.send(AgentEvent::SystemNotification { message });
        if result.is_ok() {
            let effective_model = providers::resolve_execution_model_spec(&model_for_redetect)
                .await
                .unwrap_or(model_for_redetect.clone());
            if let Some(new_bridge) = providers::auto_detect_bridge(&effective_model).await {
                let mut guard = bridge_clone.write().await;
                *guard = new_bridge;
                if let Ok(mut s) = settings_for_login.lock() {
                    s.set_model(&effective_model);
                    s.provider_connected = true;
                }
                let _ = events_tx_clone.send(AgentEvent::SystemNotification {
                    message: format!("Provider connected — active route {}.", effective_model),
                });
            }
        }
    });
    SlashCommandResponse {
        accepted: true,
        output: Some(format!(
            "Login started for {provider}. Complete any interactive prompts in the TUI."
        )),
    }
}

pub async fn auth_logout_response(provider: &str) -> SlashCommandResponse {
    if provider.trim().is_empty() {
        return SlashCommandResponse {
            accepted: false,
            output: Some(
                "Provider required for logout. Use: anthropic, openai, openai-codex, openrouter, ollama-cloud".to_string(),
            ),
        };
    }
    let message = match auth::logout_provider(provider) {
        Ok(()) => format!("✓ Logged out from {}", provider),
        Err(e) => format!("❌ Logout failed: {}", e),
    };
    SlashCommandResponse {
        accepted: true,
        output: Some(message),
    }
}

pub(crate) fn format_auth_status(status: &auth::AuthStatus) -> String {
    let mut lines = vec!["Authentication Status:".to_string()];

    for provider in &status.providers {
        let state = match provider.status {
            auth::ProviderAuthStatus::Authenticated => {
                if provider.is_oauth {
                    "✓ authenticated (oauth)".to_string()
                } else {
                    "✓ authenticated".to_string()
                }
            }
            auth::ProviderAuthStatus::Expired => "⚠ expired".to_string(),
            auth::ProviderAuthStatus::Missing => "✗ not authenticated".to_string(),
            auth::ProviderAuthStatus::Error => provider
                .details
                .as_ref()
                .map(|d| format!("✗ error ({d})"))
                .unwrap_or_else(|| "✗ error".to_string()),
        };
        lines.push(format!("  {}: {}", provider.name, state));
    }

    lines.join("\n")
}
