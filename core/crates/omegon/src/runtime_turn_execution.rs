use std::sync::Arc;

use tokio::sync::{RwLock, broadcast};
use tokio_util::sync::CancellationToken;

use crate::bridge::LlmBridge;
use crate::runtime_turn::{ActiveTurnMeta, RuntimeTurnLifecycle};
use crate::{AgentEvent, InteractiveAgentState, InteractiveTurnExecution};

pub(crate) async fn execute(
    mut state: InteractiveAgentState,
    execution: InteractiveTurnExecution,
    bridge: Arc<RwLock<Box<dyn LlmBridge>>>,
    events_tx: broadcast::Sender<AgentEvent>,
    active: ActiveTurnMeta,
    lifecycle: RuntimeTurnLifecycle,
) -> InteractiveAgentState {
    let cancel_keeps_prompt = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut loop_config = execution.loop_config;
    loop_config.cancel_keeps_prompt = Some(cancel_keeps_prompt.clone());
    loop_config.drain_post_loop_requests = false;

    append_prompt(&mut state, &active);
    lifecycle.emit_phase("conversation_updated", 0, 0, &events_tx, "worker");

    let cancel = CancellationToken::new();
    if let Ok(mut guard) = execution.shared_cancel.lock() {
        *guard = Some(cancel.clone());
    }

    let loop_started_at = std::time::Instant::now();
    lifecycle.emit_phase("loop_running", 0, 0, &events_tx, "worker");
    let run_result = {
        let bridge_guard = bridge.read().await;
        let mut run = std::pin::pin!(crate::r#loop::run(
            bridge_guard.as_ref(),
            &mut state.bus,
            &mut state.context_manager,
            &mut state.conversation,
            &events_tx,
            cancel.clone(),
            &loop_config,
        ));

        tokio::select! {
            result = &mut run => Some(result),
            _ = cancel.cancelled() => {
                let keep_prompt = cancel_keeps_prompt.load(std::sync::atomic::Ordering::Relaxed);
                let disposition = if keep_prompt { "interrupted · kept" } else { "aborted · forgotten" };
                tracing::warn!(runtime_turn_id = active.runtime_turn_id, "operator cancellation requested; abandoning active turn to recover operator surface");
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
            state
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

    if let Some(Err(error)) = run_result {
        emit_loop_error(&mut state, &active, &loop_config, &events_tx, error);
    }

    let cancel_lock_started_at = std::time::Instant::now();
    if let Ok(mut guard) = execution.shared_cancel.lock() {
        guard.take();
    }
    emit_cleanup_timing(
        &lifecycle,
        &events_tx,
        &active,
        "cleanup_cancel_token_cleared",
        cancel_lock_started_at.elapsed(),
        "post-turn cleanup waited on shared cancel lock",
        "post-turn cleanup cleared shared cancel token",
    );

    let estimate_started_at = std::time::Instant::now();
    let estimated_tokens = state.conversation.estimate_tokens();
    emit_cleanup_timing(
        &lifecycle,
        &events_tx,
        &active,
        "cleanup_tokens_estimated",
        estimate_started_at.elapsed(),
        "post-turn cleanup spent unusually long estimating transcript tokens",
        "post-turn cleanup estimated transcript tokens",
    );

    let settings_lock_started_at = std::time::Instant::now();
    let settings = execution.shared_settings.lock().unwrap();
    emit_cleanup_timing(
        &lifecycle,
        &events_tx,
        &active,
        "cleanup_settings_acquired",
        settings_lock_started_at.elapsed(),
        "post-turn cleanup waited on shared settings lock",
        "post-turn cleanup acquired shared settings lock",
    );
    let context_window = settings.context_window;
    let context_class = settings.effective_requested_class().label().to_string();
    let thinking_level = settings.thinking.as_str().to_string();
    drop(settings);

    let metrics_lock_started_at = std::time::Instant::now();
    if let Ok(mut metrics) = execution.context_metrics.lock() {
        metrics.update(
            estimated_tokens,
            context_window,
            &context_class,
            &thinking_level,
        );
    }
    emit_cleanup_timing(
        &lifecycle,
        &events_tx,
        &active,
        "cleanup_metrics_updated",
        metrics_lock_started_at.elapsed(),
        "post-turn cleanup waited on context metrics lock",
        "post-turn cleanup updated context metrics",
    );

    let _ = events_tx.send(AgentEvent::ContextUpdated {
        tokens: estimated_tokens as u64,
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
    state
}

fn append_prompt(state: &mut InteractiveAgentState, active: &ActiveTurnMeta) {
    if active.prompt.image_paths.is_empty() {
        state.conversation.push_user(active.prompt.text.clone());
        return;
    }

    let images = load_image_attachments(&active.prompt.image_paths);
    state
        .conversation
        .push_user_with_images(active.prompt.text.clone(), images);
}

fn load_image_attachments(paths: &[std::path::PathBuf]) -> Vec<crate::bridge::ImageAttachment> {
    paths
        .iter()
        .filter_map(|path| {
            let data = std::fs::read(path).ok()?;
            let media_type = match image::guess_format(&data) {
                Ok(image::ImageFormat::Png) => "image/png",
                Ok(image::ImageFormat::Jpeg) => "image/jpeg",
                Ok(image::ImageFormat::Gif) => "image/gif",
                Ok(image::ImageFormat::WebP) => "image/webp",
                _ => {
                    tracing::warn!(path = %path.display(), "skipping invalid or provider-unsupported image attachment");
                    return None;
                }
            };
            use base64::Engine;
            Some(crate::bridge::ImageAttachment {
                data: base64::engine::general_purpose::STANDARD.encode(data),
                media_type: media_type.to_string(),
                source_path: Some(path.display().to_string()),
            })
        })
        .collect()
}

fn emit_loop_error(
    state: &mut InteractiveAgentState,
    active: &ActiveTurnMeta,
    loop_config: &crate::r#loop::LoopConfig,
    events_tx: &broadcast::Sender<AgentEvent>,
    error: anyhow::Error,
) {
    let terminal_reason = if crate::r#loop::is_upstream_exhausted(&error) {
        omegon_traits::TurnEndReason::ProviderExhausted
    } else {
        omegon_traits::TurnEndReason::WorkerFailed
    };
    let _ = events_tx.send(AgentEvent::TurnEnd(Box::new(
        omegon_traits::AgentEventTurnEnd {
            turn: state.conversation.intent.stats.turns,
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
            estimated_tokens: state.conversation.estimate_tokens(),
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
            intent_task: state.conversation.intent.current_task.clone(),
            intent_phase: Some(format!("{:?}", state.conversation.intent.lifecycle_phase)),
            files_read_count: state.conversation.intent.files_read.len(),
            files_modified_count: state.conversation.intent.files_modified.len(),
            stats_tool_calls: state.conversation.intent.stats.tool_calls,
            streaks: omegon_traits::ControllerStreaks::default(),
        },
    )));
    let telemetry = state.conversation.last_provider_telemetry(None);
    let user_msg = crate::format_agent_error(&error, telemetry.as_ref());
    tracing::error!(
        runtime_turn_id = active.runtime_turn_id,
        "Agent loop error: {error}"
    );
    state
        .conversation
        .rollback_last_user_if_text(&active.prompt.text);
    let _ = events_tx.send(AgentEvent::SystemNotification { message: user_msg });
    let _ = events_tx.send(AgentEvent::AgentEnd);
}

fn emit_cleanup_timing(
    lifecycle: &RuntimeTurnLifecycle,
    events_tx: &broadcast::Sender<AgentEvent>,
    active: &ActiveTurnMeta,
    phase: &'static str,
    elapsed: std::time::Duration,
    slow_message: &'static str,
    fast_message: &'static str,
) {
    lifecycle.emit_phase(phase, elapsed.as_millis() as u64, 0, events_tx, "worker");
    if elapsed > std::time::Duration::from_millis(250) {
        tracing::warn!(
            runtime_turn_id = active.runtime_turn_id,
            elapsed_ms = elapsed.as_millis() as u64,
            "{slow_message}"
        );
    } else {
        tracing::debug!(
            runtime_turn_id = active.runtime_turn_id,
            elapsed_ms = elapsed.as_millis() as u64,
            "{fast_message}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG_1X1: &[u8] = &[
        0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, b'I', b'H', b'D',
        b'R', 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
        0x15, 0xc4, 0x89,
    ];

    #[test]
    fn image_loader_builds_supported_attachment_with_source_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pixel.png");
        std::fs::write(&path, PNG_1X1).unwrap();

        let images = load_image_attachments(std::slice::from_ref(&path));

        assert_eq!(images.len(), 1);
        assert_eq!(images[0].media_type, "image/png");
        assert_eq!(images[0].source_path.as_deref(), path.to_str());
        assert!(!images[0].data.is_empty());
    }

    #[test]
    fn image_loader_skips_missing_invalid_and_unsupported_attachments() {
        let dir = tempfile::tempdir().unwrap();
        let invalid = dir.path().join("invalid.png");
        let unsupported = dir.path().join("image.bmp");
        let missing = dir.path().join("missing.png");
        std::fs::write(&invalid, b"not an image").unwrap();
        std::fs::write(&unsupported, b"BMfake bitmap").unwrap();

        assert!(load_image_attachments(&[missing, invalid, unsupported]).is_empty());
    }

    #[test]
    fn cleanup_timing_emits_worker_lifecycle_contract() {
        use crate::runtime_prompt::{ControlSurface, PromptEnvelope, QueueMode, RuntimeActor};
        use crate::runtime_turn::ActiveTurnState;

        let mut turns = ActiveTurnState::default();
        let active = turns
            .start(PromptEnvelope {
                id: 42,
                text: "test".into(),
                image_paths: Vec::new(),
                submitted_by: RuntimeActor::tui(),
                via: ControlSurface::Tui,
                metadata: crate::tui::PromptMetadata::default(),
                queue_mode: QueueMode::UntilReady,
                queued_at: std::time::Instant::now(),
            })
            .unwrap();
        let lifecycle = RuntimeTurnLifecycle::new(&active, "post_loop_cleanup");
        let (events_tx, mut events_rx) = broadcast::channel(2);

        emit_cleanup_timing(
            &lifecycle,
            &events_tx,
            &active,
            "cleanup_tokens_estimated",
            std::time::Duration::from_millis(17),
            "slow",
            "fast",
        );

        let AgentEvent::RuntimeTurnLifecycleUpdated { snapshot_json } =
            events_rx.try_recv().unwrap()
        else {
            panic!("expected lifecycle event");
        };
        assert_eq!(snapshot_json["turn_id"], 1);
        assert_eq!(snapshot_json["prompt_id"], 42);
        assert_eq!(snapshot_json["phase"], "cleanup_tokens_estimated");
        assert_eq!(snapshot_json["source"], "worker");
        assert_eq!(snapshot_json["phase_elapsed_ms"], 17);
    }
}
