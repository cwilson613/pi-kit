// Interactive runtime coordination: actors, prompt queue, turn lifecycle, and worker supervision.
//
// Included by the binary composition root so this first relocation preserves existing
// item visibility while removing the coordinator implementation from `main.rs`.

#[derive(Debug, Clone, PartialEq, Eq)]
enum RuntimeActorKind {
    Tui,
    Auspex,
    IpcClient,
    WebClient,
    DaemonEvent,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeActor {
    kind: RuntimeActorKind,
    label: String,
}

impl RuntimeActor {
    fn display_label(&self) -> &str {
        if self.label.is_empty() {
            match self.kind {
                RuntimeActorKind::Tui => "tui",
                RuntimeActorKind::Auspex => "auspex",
                RuntimeActorKind::IpcClient => "ipc-client",
                RuntimeActorKind::WebClient => "web-client",
                RuntimeActorKind::DaemonEvent => "daemon-event",
                RuntimeActorKind::System => "system",
            }
        } else {
            &self.label
        }
    }

    fn tui() -> Self {
        Self {
            kind: RuntimeActorKind::Tui,
            label: "local-tui".to_string(),
        }
    }

    fn auspex() -> Self {
        Self {
            kind: RuntimeActorKind::Auspex,
            label: "auspex".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ControlSurface {
    Tui,
    Ipc,
    WebSocket,
    HttpEventIngress,
    Internal,
}

impl ControlSurface {
    fn label(&self) -> &'static str {
        match self {
            ControlSurface::Tui => "tui",
            ControlSurface::Ipc => "ipc",
            ControlSurface::WebSocket => "websocket",
            ControlSurface::HttpEventIngress => "http-event-ingress",
            ControlSurface::Internal => "internal",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum QueueMode {
    InterruptAfterTurn,
    #[default]
    UntilReady,
    Immediate,
}

#[derive(Debug, Clone, PartialEq)]
struct PromptEnvelope {
    id: u64,
    text: String,
    image_paths: Vec<PathBuf>,
    submitted_by: RuntimeActor,
    via: ControlSurface,
    metadata: operator_commands::PromptMetadata,
    queue_mode: QueueMode,
    queued_at: std::time::Instant,
}

impl PromptEnvelope {
    fn requests_voice_close(&self) -> bool {
        self.metadata.voice.as_ref().is_some_and(|voice| {
            voice.close_session_requested == Some(true)
                && voice.radio_cue.as_deref() == Some("over_and_out")
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ActiveTurnPhase {
    Running,
    Cancelling {
        requested_by: RuntimeActor,
        via: ControlSurface,
    },
}

#[derive(Debug, Clone, PartialEq)]
struct ActiveTurnMeta {
    runtime_turn_id: u64,
    prompt: PromptEnvelope,
    phase: ActiveTurnPhase,
    started_at: std::time::Instant,
}

use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone)]
struct RuntimeTurnLifecycle {
    runtime_turn_id: u64,
    prompt_id: u64,
    phase: &'static str,
    phase_started_at: std::time::Instant,
    turn_started_at: std::time::Instant,
    sequence: Arc<AtomicU64>,
}

impl RuntimeTurnLifecycle {
    fn new(active: &ActiveTurnMeta, phase: &'static str) -> Self {
        let now = std::time::Instant::now();
        Self {
            runtime_turn_id: active.runtime_turn_id,
            prompt_id: active.prompt.id,
            phase,
            phase_started_at: now,
            turn_started_at: active.started_at,
            sequence: Arc::new(AtomicU64::new(0)),
        }
    }

    fn transition(
        &mut self,
        phase: &'static str,
        queue_depth: usize,
        events_tx: &broadcast::Sender<AgentEvent>,
    ) {
        let now = std::time::Instant::now();
        self.phase = phase;
        self.phase_started_at = now;
        self.emit_phase(phase, 0, queue_depth, events_tx, "supervisor");
    }

    fn emit_phase(
        &self,
        phase: &'static str,
        phase_elapsed_ms: u64,
        queue_depth: usize,
        events_tx: &broadcast::Sender<AgentEvent>,
        source: &'static str,
    ) {
        let now = std::time::Instant::now();
        let sequence = self
            .sequence
            .fetch_add(1, Ordering::Relaxed)
            .saturating_add(1);
        let _ = events_tx.send(AgentEvent::RuntimeTurnLifecycleUpdated {
            snapshot_json: serde_json::json!({
                "turn_id": self.runtime_turn_id,
                "prompt_id": self.prompt_id,
                "phase": phase,
                "source": source,
                "sequence": sequence,
                "phase_elapsed_ms": phase_elapsed_ms,
                "turn_elapsed_ms": now.saturating_duration_since(self.turn_started_at).as_millis() as u64,
                "queue_depth": queue_depth,
            }),
        });
    }

    fn snapshot(&self, queue_depth: usize) -> serde_json::Value {
        let now = std::time::Instant::now();
        serde_json::json!({
            "turn_id": self.runtime_turn_id,
            "prompt_id": self.prompt_id,
            "phase": self.phase,
            "source": "supervisor",
            "sequence": self.sequence.load(Ordering::Relaxed),
            "phase_elapsed_ms": now.saturating_duration_since(self.phase_started_at).as_millis() as u64,
            "turn_elapsed_ms": now.saturating_duration_since(self.turn_started_at).as_millis() as u64,
            "queue_depth": queue_depth,
        })
    }
}

fn runtime_actor_kind_from_via(via: &str) -> RuntimeActorKind {
    match via {
        "tui" => RuntimeActorKind::Tui,
        "ipc" => RuntimeActorKind::IpcClient,
        "websocket" => RuntimeActorKind::WebClient,
        _ => RuntimeActorKind::System,
    }
}

fn control_surface_from_via(via: &str) -> ControlSurface {
    match via {
        "tui" => ControlSurface::Tui,
        "ipc" => ControlSurface::Ipc,
        "websocket" => ControlSurface::WebSocket,
        _ => ControlSurface::Internal,
    }
}

fn handle_runtime_cancel_command(
    runtime: &mut InteractiveRuntimeSupervisor,
    shared_cancel: &operator_commands::SharedCancel,
    events_tx: &broadcast::Sender<AgentEvent>,
    submitted_by: String,
    via: &'static str,
) {
    let actor = RuntimeActor {
        kind: runtime_actor_kind_from_via(via),
        label: submitted_by,
    };
    let surface = control_surface_from_via(via);
    let active = runtime.request_cancel(actor, surface);
    if active.is_none() {
        let _ = events_tx.send(AgentEvent::SystemNotification {
            message: "Cancel requested, but no active turn is running.".to_string(),
        });
    }
    if let Ok(guard) = shared_cancel.lock()
        && let Some(ref cancel) = *guard
    {
        cancel.cancel();
    }
}

fn emit_runtime_queue_notification(
    runtime: &InteractiveRuntimeSupervisor,
    events_tx: &broadcast::Sender<AgentEvent>,
    prompt_id: u64,
) {
    if let Some(prompt) = runtime.queue.iter().find(|prompt| prompt.id == prompt_id) {
        emit_runtime_queue_snapshot(runtime, events_tx);
        let _ = events_tx.send(AgentEvent::SystemNotification {
            message: format!(
                "Queued prompt #{} from {} via {}; queue depth {}.",
                prompt.id,
                prompt.submitted_by.display_label(),
                prompt.via.label(),
                runtime.queue_depth()
            ),
        });
    }
}

fn emit_runtime_queue_snapshot(
    runtime: &InteractiveRuntimeSupervisor,
    events_tx: &broadcast::Sender<AgentEvent>,
) {
    let snapshot_json = runtime.queue_snapshot_json();
    let _ = events_tx.send(AgentEvent::RuntimeQueueUpdated { snapshot_json });
}

pub(crate) struct InteractiveAgentState {
    pub(crate) bus: crate::bus::EventBus,
    pub(crate) context_manager: crate::context::ContextManager,
    pub(crate) conversation: crate::conversation::ConversationState,
    pub(crate) inference_runtime: crate::inference_runtime::InferenceRuntimeState,
}

pub(crate) struct InteractiveAgentHost {
    pub(crate) session_id: String,
    pub(crate) instance_id: String,
    pub(crate) context_metrics:
        std::sync::Arc<std::sync::Mutex<crate::features::context::SharedContextMetrics>>,
    pub(crate) cwd: PathBuf,
    pub(crate) secrets: std::sync::Arc<omegon_secrets::SecretsManager>,
    pub(crate) web_auth_state: crate::web::WebAuthState,
    pub(crate) dashboard_handles: crate::runtime_state::RuntimeStateHandles,
    pub(crate) resume_info: Option<setup::ResumeInfo>,
    pub(crate) workspace_state: setup::WorkspaceStartupState,
    pub(crate) runtime_generation: u64,
}

pub(crate) struct CliRuntimeView<'a> {
    pub(crate) no_session: bool,
    pub(crate) model: &'a str,
    pub(crate) dangerously_bypass_permissions: bool,
}

fn restart_args_for_session(mut args: Vec<String>, session_id: &str) -> Vec<String> {
    let mut filtered = Vec::with_capacity(args.len() + 2);
    let mut skip_resume_value = false;
    for arg in args.drain(..) {
        if skip_resume_value {
            skip_resume_value = false;
            continue;
        }
        if arg == "--fresh" {
            continue;
        }
        if arg == "--resume" {
            skip_resume_value = true;
            continue;
        }
        if arg.starts_with("--resume=") {
            continue;
        }
        filtered.push(arg);
    }
    filtered.push("--resume".to_string());
    filtered.push(session_id.to_string());
    filtered
}

fn interactive_resume_mode(cli: &Cli) -> Option<Option<&str>> {
    if cli.fresh {
        None
    } else {
        cli.resume.as_ref().map(|r| r.as_deref())
    }
}

fn split_interactive_agent(
    agent: setup::AgentSetup,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct InteractiveStartupModelDecision {
    selected_model: String,
    bridge_model: String,
    provider_connected: bool,
    use_null_bridge: bool,
}

fn decide_interactive_startup_model(
    selected_model: &str,
    resolved_model: &str,
    resolved_available: bool,
) -> InteractiveStartupModelDecision {
    InteractiveStartupModelDecision {
        selected_model: selected_model.to_string(),
        bridge_model: resolved_model.to_string(),
        provider_connected: resolved_available,
        use_null_bridge: !resolved_available,
    }
}

#[derive(Clone)]
struct InteractiveRuntimeResources {
    cwd: PathBuf,
    secrets: std::sync::Arc<omegon_secrets::SecretsManager>,
    context_metrics:
        std::sync::Arc<std::sync::Mutex<crate::features::context::SharedContextMetrics>>,
    bridge_model: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    route_controller: Arc<route::RouteController>,
}

fn build_interactive_loop_config(
    runtime: &InteractiveRuntimeResources,
    shared_settings: &Arc<std::sync::Mutex<settings::Settings>>,
    pending_compact: &Arc<std::sync::atomic::AtomicBool>,
) -> r#loop::LoopConfig {
    let model = shared_settings
        .lock()
        .map(|s| s.model.clone())
        .unwrap_or_default();

    let ollama_manager = if providers::infer_provider_id(&model) == "ollama" {
        Some(ollama::OllamaManager::new())
    } else {
        None
    };

    bootstrap::build_loop_config(
        shared_settings,
        &runtime.cwd,
        &model,
        bootstrap::LoopConfigOverrides {
            secrets: Some(runtime.secrets.clone()),
            force_compact: Some(pending_compact.clone()),
            allow_commit_nudge: true,
            ollama_manager,
            bridge_model: runtime
                .bridge_model
                .lock()
                .ok()
                .and_then(|guard| guard.clone()),
            route_controller: Some(runtime.route_controller.clone()),
            ..Default::default()
        },
    )
}

#[allow(clippy::too_many_arguments)]
#[cfg(not(feature = "tui"))]
async fn run_interactive_command(_cli: &Cli) -> anyhow::Result<()> {
    anyhow::bail!(
        "interactive support was not compiled; rebuild with the `tui` feature or use `omegon serve`"
    )
}

#[cfg(feature = "tui")]
async fn run_interactive_active_turn(
    mut runtime_state: InteractiveAgentState,
    runtime: InteractiveRuntimeResources,
    bridge: Arc<tokio::sync::RwLock<Box<dyn LlmBridge>>>,
    shared_settings: Arc<std::sync::Mutex<settings::Settings>>,
    shared_cancel: operator_commands::SharedCancel,
    pending_compact: Arc<std::sync::atomic::AtomicBool>,
    events_tx: broadcast::Sender<AgentEvent>,
    active: ActiveTurnMeta,
    lifecycle: RuntimeTurnLifecycle,
) -> InteractiveAgentState {
    let cancel_keeps_prompt = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut loop_config =
        build_interactive_loop_config(&runtime, &shared_settings, &pending_compact);
    loop_config.cancel_keeps_prompt = Some(cancel_keeps_prompt.clone());
    loop_config.drain_post_loop_requests = false;

    if active.prompt.image_paths.is_empty() {
        runtime_state
            .conversation
            .push_user(active.prompt.text.clone());
    } else {
        let mut images = Vec::new();
        for path in &active.prompt.image_paths {
            if let Ok(data) = std::fs::read(path) {
                let media_type = match image::guess_format(&data) {
                    Ok(image::ImageFormat::Png) => Some("image/png"),
                    Ok(image::ImageFormat::Jpeg) => Some("image/jpeg"),
                    Ok(image::ImageFormat::Gif) => Some("image/gif"),
                    Ok(image::ImageFormat::WebP) => Some("image/webp"),
                    _ => None,
                };
                let Some(media_type) = media_type else {
                    tracing::warn!(path = %path.display(), "skipping invalid or provider-unsupported image attachment");
                    continue;
                };
                use base64::Engine;
                let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
                images.push(crate::bridge::ImageAttachment {
                    data: b64,
                    media_type: media_type.to_string(),
                    source_path: Some(path.display().to_string()),
                });
            }
        }
        runtime_state
            .conversation
            .push_user_with_images(active.prompt.text.clone(), images);
    }

    lifecycle.emit_phase("conversation_updated", 0, 0, &events_tx, "worker");

    let cancel = CancellationToken::new();
    if let Ok(mut guard) = shared_cancel.lock() {
        *guard = Some(cancel.clone());
    }

    let loop_started_at = std::time::Instant::now();
    lifecycle.emit_phase("loop_running", 0, 0, &events_tx, "worker");
    let run_result = {
        let bridge_guard = bridge.read().await;
        let mut run = std::pin::pin!(r#loop::run(
            bridge_guard.as_ref(),
            &mut runtime_state.bus,
            &mut runtime_state.context_manager,
            &mut runtime_state.conversation,
            &events_tx,
            cancel.clone(),
            &loop_config,
        ));

        tokio::select! {
            result = &mut run => Some(result),
            _ = cancel.cancelled() => {
                let keep_prompt = cancel_keeps_prompt.load(std::sync::atomic::Ordering::Relaxed);
                let disposition = if keep_prompt { "interrupted · kept" } else { "aborted · forgotten" };
                tracing::warn!(
                    runtime_turn_id = active.runtime_turn_id,
                    "operator cancellation requested; abandoning active turn to recover operator surface"
                );
                let _ = events_tx.send(AgentEvent::SystemNotification {
                    message: format!("Interrupt requested — recovered the operator surface ({disposition}). The abandoned provider/tool request may finish in the background."),
                });
                let _ = events_tx.send(AgentEvent::AgentEnd);
                None
            }
        }
    };
    let cleanup_started_at = std::time::Instant::now();
    lifecycle.emit_phase("post_loop_cleanup", 0, 0, &events_tx, "worker");
    tracing::info!(
        runtime_turn_id = active.runtime_turn_id,
        loop_elapsed_ms = loop_started_at.elapsed().as_millis() as u64,
        cancelled = cancel.is_cancelled(),
        result = match &run_result {
            Some(Ok(_)) => "ok",
            Some(Err(_)) => "error",
            None => "abandoned",
        },
        "interactive active turn loop returned; starting post-turn cleanup"
    );

    if (matches!(run_result, Some(Ok(_))) || run_result.is_none()) && cancel.is_cancelled() {
        let keep_prompt = cancel_keeps_prompt.load(std::sync::atomic::Ordering::Relaxed);
        if !keep_prompt {
            runtime_state
                .conversation
                .rollback_last_user_if_text(&active.prompt.text);
        }
        let disposition = if keep_prompt {
            "interrupted · kept"
        } else {
            "aborted · forgotten"
        };
        let _ = events_tx.send(AgentEvent::MessageAbort {
            reason: Some(disposition.to_string()),
        });
    }

    if let Some(Err(e)) = run_result {
        let terminal_reason = if r#loop::is_upstream_exhausted(&e) {
            omegon_traits::TurnEndReason::ProviderExhausted
        } else {
            omegon_traits::TurnEndReason::WorkerFailed
        };
        let _ = events_tx.send(AgentEvent::TurnEnd(Box::new(
            omegon_traits::AgentEventTurnEnd {
                turn: runtime_state.conversation.intent.stats.turns,
                turn_end_reason: terminal_reason,
                model: loop_config
                    .bridge_model
                    .clone()
                    .or_else(|| Some(loop_config.model.clone())),
                provider: loop_config
                    .bridge_model
                    .as_deref()
                    .map(crate::providers::infer_provider_id)
                    .or_else(|| Some(crate::providers::infer_provider_id(&loop_config.model))),
                estimated_tokens: runtime_state.conversation.estimate_tokens(),
                context_window: 0,
                context_composition: omegon_traits::ContextComposition::default(),
                actual_input_tokens: 0,
                actual_output_tokens: 0,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
                provider_telemetry: None,
                dominant_phase: None,
                drift_kind: None,
                progress_nudge_reason: None,
                intent_task: runtime_state.conversation.intent.current_task.clone(),
                intent_phase: Some(format!(
                    "{:?}",
                    runtime_state.conversation.intent.lifecycle_phase
                )),
                files_read_count: runtime_state.conversation.intent.files_read.len(),
                files_modified_count: runtime_state.conversation.intent.files_modified.len(),
                stats_tool_calls: runtime_state.conversation.intent.stats.tool_calls,
                streaks: omegon_traits::ControllerStreaks::default(),
            },
        )));
        let recent_telemetry = runtime_state.conversation.last_provider_telemetry(None);
        let user_msg = format_agent_error(&e, recent_telemetry.as_ref());
        tracing::error!(
            runtime_turn_id = active.runtime_turn_id,
            "Agent loop error: {e}"
        );
        runtime_state
            .conversation
            .rollback_last_user_if_text(&active.prompt.text);
        let _ = events_tx.send(AgentEvent::SystemNotification { message: user_msg });
        let _ = events_tx.send(AgentEvent::AgentEnd);
    }

    let cancel_lock_started_at = std::time::Instant::now();
    if let Ok(mut guard) = shared_cancel.lock() {
        guard.take();
    }
    let cancel_lock_elapsed = cancel_lock_started_at.elapsed();
    lifecycle.emit_phase(
        "cleanup_cancel_token_cleared",
        cancel_lock_elapsed.as_millis() as u64,
        0,
        &events_tx,
        "worker",
    );
    if cancel_lock_elapsed > std::time::Duration::from_millis(250) {
        tracing::warn!(
            runtime_turn_id = active.runtime_turn_id,
            elapsed_ms = cancel_lock_elapsed.as_millis() as u64,
            "post-turn cleanup waited on shared cancel lock"
        );
    } else {
        tracing::debug!(
            runtime_turn_id = active.runtime_turn_id,
            elapsed_ms = cancel_lock_elapsed.as_millis() as u64,
            "post-turn cleanup cleared shared cancel token"
        );
    }

    let estimate_started_at = std::time::Instant::now();
    let est = runtime_state.conversation.estimate_tokens();
    let estimate_elapsed = estimate_started_at.elapsed();
    lifecycle.emit_phase(
        "cleanup_tokens_estimated",
        estimate_elapsed.as_millis() as u64,
        0,
        &events_tx,
        "worker",
    );
    if estimate_elapsed > std::time::Duration::from_millis(250) {
        tracing::warn!(
            runtime_turn_id = active.runtime_turn_id,
            elapsed_ms = estimate_elapsed.as_millis() as u64,
            estimated_tokens = est,
            "post-turn cleanup spent unusually long estimating transcript tokens"
        );
    } else {
        tracing::debug!(
            runtime_turn_id = active.runtime_turn_id,
            elapsed_ms = estimate_elapsed.as_millis() as u64,
            estimated_tokens = est,
            "post-turn cleanup estimated transcript tokens"
        );
    }

    let settings_lock_started_at = std::time::Instant::now();
    let settings = shared_settings.lock().unwrap();
    let settings_lock_elapsed = settings_lock_started_at.elapsed();
    lifecycle.emit_phase(
        "cleanup_settings_acquired",
        settings_lock_elapsed.as_millis() as u64,
        0,
        &events_tx,
        "worker",
    );
    if settings_lock_elapsed > std::time::Duration::from_millis(250) {
        tracing::warn!(
            runtime_turn_id = active.runtime_turn_id,
            elapsed_ms = settings_lock_elapsed.as_millis() as u64,
            "post-turn cleanup waited on shared settings lock"
        );
    } else {
        tracing::debug!(
            runtime_turn_id = active.runtime_turn_id,
            elapsed_ms = settings_lock_elapsed.as_millis() as u64,
            "post-turn cleanup acquired shared settings lock"
        );
    }
    let context_window = settings.context_window;
    let context_class = settings.effective_requested_class().label().to_string();
    let thinking_level = settings.thinking.as_str().to_string();

    let metrics_lock_started_at = std::time::Instant::now();
    if let Ok(mut metrics) = runtime.context_metrics.lock() {
        metrics.update(est, context_window, &context_class, &thinking_level);
    }
    let metrics_lock_elapsed = metrics_lock_started_at.elapsed();
    lifecycle.emit_phase(
        "cleanup_metrics_updated",
        metrics_lock_elapsed.as_millis() as u64,
        0,
        &events_tx,
        "worker",
    );
    if metrics_lock_elapsed > std::time::Duration::from_millis(250) {
        tracing::warn!(
            runtime_turn_id = active.runtime_turn_id,
            elapsed_ms = metrics_lock_elapsed.as_millis() as u64,
            "post-turn cleanup waited on context metrics lock"
        );
    } else {
        tracing::debug!(
            runtime_turn_id = active.runtime_turn_id,
            elapsed_ms = metrics_lock_elapsed.as_millis() as u64,
            "post-turn cleanup updated context metrics"
        );
    }
    let _ = events_tx.send(AgentEvent::ContextUpdated {
        tokens: est as u64,
        context_window: context_window as u64,
        context_class,
        thinking_level,
    });
    tracing::info!(
        runtime_turn_id = active.runtime_turn_id,
        cleanup_elapsed_ms = cleanup_started_at.elapsed().as_millis() as u64,
        "interactive active turn post-turn cleanup finished"
    );
    lifecycle.emit_phase(
        "worker_returning",
        cleanup_started_at.elapsed().as_millis() as u64,
        0,
        &events_tx,
        "worker",
    );

    runtime_state
}

async fn stop_voice_session_if_requested(
    prompt: &PromptEnvelope,
    bus: &crate::bus::EventBus,
    events_tx: &tokio::sync::broadcast::Sender<AgentEvent>,
) {
    if !prompt.requests_voice_close() {
        return;
    }

    if !bus.has_tool("voice_session_stop") {
        let _ = events_tx.send(AgentEvent::SystemNotification {
            message: "Voice requested shutdown after this prompt, but no voice_session_stop tool is available.".to_string(),
        });
        return;
    }

    match bus
        .execute_tool(
            "voice_session_stop",
            "voice-over-and-out-stop",
            serde_json::json!({}),
            tokio_util::sync::CancellationToken::new(),
        )
        .await
    {
        Ok(_) => {
            let _ = events_tx.send(AgentEvent::SystemNotification {
                message: "Voice session stop requested after over and out.".to_string(),
            });
        }
        Err(err) => {
            let _ = events_tx.send(AgentEvent::SystemNotification {
                message: format!("Voice requested shutdown, but voice_session_stop failed: {err}"),
            });
        }
    }
}

#[derive(Debug, Default)]
struct InteractiveRuntimeSupervisor {
    queue: VecDeque<PromptEnvelope>,
    active_turn: Option<ActiveTurnMeta>,
    next_prompt_id: u64,
    next_runtime_turn_id: u64,
    default_queue_mode: QueueMode,
}

impl InteractiveRuntimeSupervisor {
    fn enqueue_prompt(
        &mut self,
        text: String,
        image_paths: Vec<PathBuf>,
        actor: RuntimeActor,
        via: ControlSurface,
        metadata: operator_commands::PromptMetadata,
        queue_mode: Option<QueueMode>,
    ) -> u64 {
        self.next_prompt_id += 1;
        let prompt_id = self.next_prompt_id;
        self.queue.push_back(PromptEnvelope {
            id: prompt_id,
            text,
            image_paths,
            submitted_by: actor,
            via,
            metadata,
            queue_mode: queue_mode.unwrap_or(self.default_queue_mode),
            queued_at: std::time::Instant::now(),
        });
        prompt_id
    }

    fn queue_depth(&self) -> usize {
        self.queue.len()
    }

    fn queue_preview(&self) -> Vec<String> {
        self.queue
            .iter()
            .map(|prompt| {
                let attachment_summary = if prompt.image_paths.is_empty() {
                    String::new()
                } else {
                    let names = prompt
                        .image_paths
                        .iter()
                        .take(3)
                        .filter_map(|path| path.file_name().and_then(|name| name.to_str()))
                        .collect::<Vec<_>>();
                    let suffix = if prompt.image_paths.len() > names.len() {
                        format!(" +{} more", prompt.image_paths.len() - names.len())
                    } else {
                        String::new()
                    };
                    format!(" [{}{}]", names.join(", "), suffix)
                };
                let preview = prompt.text.chars().take(48).collect::<String>();
                let mode = match prompt.queue_mode {
                    QueueMode::InterruptAfterTurn => "after-turn",
                    QueueMode::UntilReady => "ready",
                    QueueMode::Immediate => "now",
                };
                format!("#{} {mode}: {}{}", prompt.id, preview, attachment_summary)
            })
            .collect()
    }

    fn queue_snapshot_json(&self) -> serde_json::Value {
        serde_json::json!({
            "depth": self.queue_depth(),
            "active": self.active_turn.as_ref().map(|active| serde_json::json!({
                "turn_id": active.runtime_turn_id,
                "prompt_id": active.prompt.id,
                "submitted_by": active.prompt.submitted_by.display_label(),
                "via": active.prompt.via.label(),
                "phase": match &active.phase {
                    ActiveTurnPhase::Running => "running",
                    ActiveTurnPhase::Cancelling { .. } => "cancelling",
                },
                "elapsed_ms": active.started_at.elapsed().as_millis() as u64,
                "queued_wait_ms": active.started_at.saturating_duration_since(active.prompt.queued_at).as_millis() as u64,
            })),
            "items": self.queue.iter().map(|prompt| serde_json::json!({
                "id": prompt.id,
                "submitted_by": prompt.submitted_by.display_label(),
                "via": prompt.via.label(),
                "queue_mode": match prompt.queue_mode {
                    QueueMode::InterruptAfterTurn => "interrupt_after_turn",
                    QueueMode::UntilReady => "until_ready",
                    QueueMode::Immediate => "immediate",
                },
                "preview": prompt.text.chars().take(80).collect::<String>(),
                "attachments": prompt.image_paths.len(),
                "voice": prompt.metadata.voice.is_some(),
                "wait_ms": prompt.queued_at.elapsed().as_millis() as u64,
            })).collect::<Vec<_>>(),
            "previews": self.queue_preview(),
        })
    }

    fn is_busy(&self) -> bool {
        self.active_turn.is_some()
    }

    fn maybe_start_next_turn(&mut self) -> Option<ActiveTurnMeta> {
        if self.active_turn.is_some() {
            return None;
        }
        let prompt = self.queue.pop_front()?;
        self.next_runtime_turn_id += 1;
        let active = ActiveTurnMeta {
            runtime_turn_id: self.next_runtime_turn_id,
            prompt,
            phase: ActiveTurnPhase::Running,
            started_at: std::time::Instant::now(),
        };
        self.active_turn = Some(active.clone());
        Some(active)
    }

    fn request_cancel(
        &mut self,
        actor: RuntimeActor,
        via: ControlSurface,
    ) -> Option<&ActiveTurnMeta> {
        let active = self.active_turn.as_mut()?;
        if matches!(active.phase, ActiveTurnPhase::Running) {
            active.phase = ActiveTurnPhase::Cancelling {
                requested_by: actor,
                via,
            };
        }
        self.active_turn.as_ref()
    }

    fn complete_active_turn(&mut self) -> Option<ActiveTurnMeta> {
        self.active_turn.take()
    }

    fn pop_front_prompt(&mut self) -> Option<PromptEnvelope> {
        self.queue.pop_front()
    }

    fn push_front_prompt(&mut self, prompt: PromptEnvelope) {
        self.queue.push_front(prompt);
    }

    fn clear_queue(&mut self) {
        self.queue.clear();
    }
}
