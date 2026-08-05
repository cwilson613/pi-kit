use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use tokio::sync::{broadcast, mpsc};

use crate::active_worker_command;
use crate::active_worker_wait::ActiveWorkerWaitTelemetry;
use crate::post_worker_completion::PostWorkerCompletionPolicy;
use crate::runtime_turn::RuntimeTurnLifecycle;
use crate::tui;
use crate::{AgentEvent, InteractiveAgentState, InteractiveRuntimeSupervisor};

pub(crate) struct ActiveWorkerRunContext<'a> {
    pub(crate) command_rx: &'a mut mpsc::Receiver<tui::TuiCommand>,
    pub(crate) runtime: &'a mut InteractiveRuntimeSupervisor,
    pub(crate) shared_cancel: &'a Arc<Mutex<Option<tokio_util::sync::CancellationToken>>>,
    pub(crate) events_tx: &'a broadcast::Sender<AgentEvent>,
    pub(crate) deferred_commands: &'a mut VecDeque<tui::TuiCommand>,
    pub(crate) lifecycle: &'a mut RuntimeTurnLifecycle,
}

pub(crate) async fn run(
    turn_task: &mut tokio::task::JoinHandle<InteractiveAgentState>,
    context: ActiveWorkerRunContext<'_>,
) -> anyhow::Result<(InteractiveAgentState, PostWorkerCompletionPolicy)> {
    let ActiveWorkerRunContext {
        command_rx,
        runtime,
        shared_cancel,
        events_tx,
        deferred_commands,
        lifecycle,
    } = context;
    let mut completion_policy = PostWorkerCompletionPolicy::default();
    let mut wait_telemetry = ActiveWorkerWaitTelemetry::new();
    let mut slow_turn_probe = Box::pin(tokio::time::sleep(wait_telemetry.probe_interval()));

    loop {
        tokio::select! {
            turn_result = &mut *turn_task => {
                let state = turn_result.map_err(worker_join_error)?;
                lifecycle.transition("worker_returned", runtime.queue_depth(), events_tx);
                return Ok((state, completion_policy));
            }
            _ = &mut slow_turn_probe => {
                let observation = wait_telemetry.observe(runtime.queue_depth());
                tracing::warn!(
                    elapsed_secs = observation.elapsed.as_secs(),
                    queued_prompts = observation.queue_depth,
                    lifecycle = %lifecycle.snapshot(observation.queue_depth),
                    slow_turn_notifications = observation.notification_count,
                    "interactive active turn worker is still running after visible turn start; queued prompts remain blocked until worker returns"
                );
                if observation.notify_blocked_queue {
                    let _ = events_tx.send(AgentEvent::RuntimeTurnLifecycleUpdated {
                        snapshot_json: lifecycle.snapshot(observation.queue_depth),
                    });
                    let _ = events_tx.send(AgentEvent::SystemNotification {
                        message: format!(
                            "Prompt queued behind active turn after {}s. Latest lifecycle state is recorded in the agent log; queued prompts will start when this turn's worker returns.",
                            observation.elapsed.as_secs()
                        ),
                    });
                }
                slow_turn_probe.as_mut().reset(
                    tokio::time::Instant::now() + observation.next_probe_after,
                );
            }
            maybe_cmd = command_rx.recv() => {
                let Some(cmd) = maybe_cmd else {
                    completion_policy.request_channel_close();
                    InteractiveRuntimeSupervisor::cancel_shared_turn(shared_cancel);
                    continue;
                };
                let effect = active_worker_command::apply(
                    active_worker_command::classify(cmd),
                    runtime,
                    &mut completion_policy,
                );
                active_worker_command::interpret(
                    effect,
                    runtime,
                    shared_cancel,
                    events_tx,
                    deferred_commands,
                );
            }
        }
    }
}

fn worker_join_error(join_err: tokio::task::JoinError) -> anyhow::Error {
    tracing::error!("interactive turn task failed: {join_err}");
    anyhow::anyhow!(crate::format_interactive_turn_task_failure(&join_err))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn worker_join_error_reports_safe_session_shutdown() {
        let handle = tokio::spawn(async { panic!("worker exploded") });
        let join_err = handle.await.expect_err("worker should panic");
        let message = worker_join_error(join_err).to_string();
        assert!(message.contains("Interactive turn worker crashed"));
        assert!(message.contains("ending session safely"));
    }
}
