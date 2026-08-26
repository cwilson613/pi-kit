//! Boot-captured managed compaction planning without session authority.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use async_trait::async_trait;
use omegon_traits::{
    Feature, ManagedCallContext, ManagedResourceController, ManagedResourceSettlementFuture,
    ManagedServiceCallError, ManagedServiceContract, ManagedServiceFuture,
    RuntimeActivationBoundary, RuntimeCapabilityId, RuntimeCleanupAssurance,
    RuntimeCleanupRequirement, RuntimeCompositionGenerationId, RuntimeCompositionTransitionPolicy,
    RuntimeContributionGenerationId, RuntimeContributionResourceId, RuntimeFailureDisposition,
    RuntimeLifecyclePolicy, RuntimeLifecycleRequirement, RuntimeOwnedResourceKind,
    RuntimeServiceInterfaceId,
};
use tokio::sync::{Notify, mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::conversation::AgentMessage;
use crate::managed_service_bus::{ManagedGenerationCandidate, ManagedResourceRegistration};
use crate::service_generation::ManagedServiceHandle;

pub(crate) const CONTEXT_COMPACTION_CAPABILITY: &str = "service:context-compaction";
pub(crate) const CONTEXT_COMPACTION_INTERFACE: &str = "interface:omegon-context-compaction-v1";
pub(crate) const CONTEXT_COMPACTION_GENERATION: &str = "contribution:context-compaction-managed-v1";
const WORKER_RESOURCE: &str = "resource:context-compaction-worker";
const QUEUE_CAPACITY: usize = 16;
const PRESSURE_KEEP_RECENT_TURNS: u32 = 4;
pub(crate) const MANUAL_KEEP_RECENT_TURNS: u32 = 2;

pub(crate) fn context_compaction_capability_id() -> RuntimeCapabilityId {
    RuntimeCapabilityId::new(CONTEXT_COMPACTION_CAPABILITY).expect("static capability id is valid")
}

pub(crate) fn context_compaction_interface_id() -> RuntimeServiceInterfaceId {
    RuntimeServiceInterfaceId::new(CONTEXT_COMPACTION_INTERFACE)
        .expect("static interface id is valid")
}

#[derive(Clone, Default)]
pub(crate) struct ContextCompactionBinding {
    handle: Arc<OnceLock<Option<ManagedServiceHandle<ContextCompactionService>>>>,
    #[cfg(test)]
    direct_test_planner: bool,
}

impl ContextCompactionBinding {
    #[cfg(test)]
    pub(crate) fn direct_for_test() -> Self {
        Self {
            handle: Arc::new(OnceLock::new()),
            direct_test_planner: true,
        }
    }

    pub(crate) fn capture(&self, bus: &crate::bus::EventBus) -> anyhow::Result<()> {
        let handle = bus.managed_service::<ContextCompactionService>(
            &context_compaction_capability_id(),
            &context_compaction_interface_id(),
        )?;
        self.handle
            .set(handle)
            .map_err(|_| anyhow::anyhow!("context/compaction managed handle was already captured"))
    }

    pub(crate) fn handle(&self) -> Option<ManagedServiceHandle<ContextCompactionService>> {
        self.handle.get().and_then(Clone::clone)
    }

    pub(crate) async fn plan(
        &self,
        snapshot: ContextCompactionSnapshotV1,
        mode: ContextCompactionModeV1,
        cancellation: CancellationToken,
    ) -> Result<
        Option<ContextCompactionPlanV1>,
        ManagedServiceCallError<ContextCompactionServiceErrorV1>,
    > {
        #[cfg(test)]
        if self.direct_test_planner {
            return plan_compaction(&snapshot, mode, || cancellation.is_cancelled())
                .map_err(ManagedServiceCallError::Operation);
        }
        let Some(handle) = self.handle() else {
            return Err(ManagedServiceCallError::Operation(
                ContextCompactionServiceErrorV1::unavailable(),
            ));
        };
        match handle
            .invoke(ContextCompactionRequestV1::Plan {
                snapshot,
                mode,
                cancellation,
            })
            .await?
        {
            ContextCompactionResponseV1::Plan(plan) => Ok(plan),
        }
    }
}

pub(crate) struct ContextCompactionFeature;

#[async_trait]
impl Feature for ContextCompactionFeature {
    fn name(&self) -> &str {
        "context-compaction"
    }

    fn runtime_contribution_generation_id(&self) -> Option<RuntimeContributionGenerationId> {
        Some(
            RuntimeContributionGenerationId::new(CONTEXT_COMPACTION_GENERATION)
                .expect("static generation id is valid"),
        )
    }

    fn runtime_lifecycle_policy(&self) -> Option<RuntimeLifecyclePolicy> {
        Some(RuntimeLifecyclePolicy {
            requirement: RuntimeLifecycleRequirement::Optional,
            failure_disposition: RuntimeFailureDisposition::DegradeLocally,
            readiness_timeout_ms: 0,
            heartbeat_timeout_ms: None,
            restart_limit: 0,
        })
    }

    fn runtime_transition_policy(&self) -> Option<RuntimeCompositionTransitionPolicy> {
        Some(RuntimeCompositionTransitionPolicy {
            activation_boundary: RuntimeActivationBoundary::Boot,
            cleanup: RuntimeCleanupRequirement::Strict,
            cleanup_timeout_ms: 5_000,
        })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ContextCompactionSnapshotV1 {
    pub(crate) messages: Vec<AgentMessage>,
    pub(crate) current_turn: u32,
    pub(crate) decay_window: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContextCompactionModeV1 {
    Pressure,
    Overflow,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContextCompactionApplicationV1 {
    DecayWindow,
    KeepRecent(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ContextCompactionPlanV1 {
    pub(crate) payload: String,
    pub(crate) evict_count: usize,
    pub(crate) reason: Option<String>,
    pub(crate) application: ContextCompactionApplicationV1,
}

pub(crate) enum ContextCompactionRequestV1 {
    Plan {
        snapshot: ContextCompactionSnapshotV1,
        mode: ContextCompactionModeV1,
        cancellation: CancellationToken,
    },
}

impl ContextCompactionRequestV1 {
    fn cancellation(&self) -> &CancellationToken {
        match self {
            Self::Plan { cancellation, .. } => cancellation,
        }
    }
}

#[derive(Debug)]
pub(crate) enum ContextCompactionResponseV1 {
    Plan(Option<ContextCompactionPlanV1>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContextCompactionServiceErrorCodeV1 {
    Unavailable,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub(crate) struct ContextCompactionServiceErrorV1 {
    pub(crate) code: ContextCompactionServiceErrorCodeV1,
    message: String,
}

impl ContextCompactionServiceErrorV1 {
    fn unavailable() -> Self {
        Self {
            code: ContextCompactionServiceErrorCodeV1::Unavailable,
            message: "managed context/compaction service is unavailable".into(),
        }
    }

    fn cancelled() -> Self {
        Self {
            code: ContextCompactionServiceErrorCodeV1::Cancelled,
            message: "context compaction planning was cancelled".into(),
        }
    }
}

pub(crate) struct ContextCompactionService {
    commands: mpsc::Sender<WorkerCommand>,
}

struct WorkerCommand {
    request: ContextCompactionRequestV1,
    generation_cancellation: CancellationToken,
    response: oneshot::Sender<Result<ContextCompactionResponseV1, ContextCompactionServiceErrorV1>>,
}

impl ManagedServiceContract for ContextCompactionService {
    type Request = ContextCompactionRequestV1;
    type Response = ContextCompactionResponseV1;
    type Error = ContextCompactionServiceErrorV1;

    fn execute<'a>(
        &'a self,
        request: Self::Request,
        context: ManagedCallContext,
    ) -> ManagedServiceFuture<'a, Self::Response, Self::Error> {
        Box::pin(async move {
            let caller_cancellation = request.cancellation().clone();
            if caller_cancellation.is_cancelled() || context.cancellation.is_cancelled() {
                return Err(ContextCompactionServiceErrorV1::cancelled());
            }
            let (response, receive) = oneshot::channel();
            let command = WorkerCommand {
                request,
                generation_cancellation: context.cancellation.clone(),
                response,
            };
            tokio::select! {
                biased;
                () = caller_cancellation.cancelled() => return Err(ContextCompactionServiceErrorV1::cancelled()),
                () = context.cancellation.cancelled() => return Err(ContextCompactionServiceErrorV1::cancelled()),
                sent = self.commands.send(command) => sent.map_err(|_| ContextCompactionServiceErrorV1::unavailable())?,
            }
            tokio::select! {
                biased;
                () = caller_cancellation.cancelled() => Err(ContextCompactionServiceErrorV1::cancelled()),
                () = context.cancellation.cancelled() => Err(ContextCompactionServiceErrorV1::cancelled()),
                result = receive => result.map_err(|_| ContextCompactionServiceErrorV1::unavailable())?,
            }
        })
    }
}

struct WorkerState {
    stopping: AtomicBool,
    worker_joined: AtomicBool,
    changed: Notify,
    join: Mutex<Option<std::thread::JoinHandle<()>>>,
}

struct WorkerController {
    state: Arc<WorkerState>,
    commands: mpsc::Sender<WorkerCommand>,
}

impl WorkerController {
    fn request_worker_stop(&self) {
        self.state.stopping.store(true, Ordering::Release);
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        let (response, _) = oneshot::channel();
        let _ = self.commands.try_send(WorkerCommand {
            request: ContextCompactionRequestV1::Plan {
                snapshot: ContextCompactionSnapshotV1 {
                    messages: Vec::new(),
                    current_turn: 0,
                    decay_window: 0,
                },
                mode: ContextCompactionModeV1::Overflow,
                cancellation: cancellation.clone(),
            },
            generation_cancellation: cancellation,
            response,
        });
    }
}

impl Drop for WorkerController {
    fn drop(&mut self) {
        self.request_worker_stop();
    }
}

impl ManagedResourceController for WorkerController {
    fn request_stop(&self) {
        self.request_worker_stop();
    }

    fn force_stop(&self) {
        self.request_worker_stop();
    }

    fn await_settled(&self) -> ManagedResourceSettlementFuture<'_> {
        let state = Arc::clone(&self.state);
        Box::pin(async move {
            if !state.worker_joined.load(Ordering::Acquire) {
                let join = state
                    .join
                    .lock()
                    .map_err(|_| "context/compaction worker join lock poisoned".to_string())?
                    .take();
                if let Some(join) = join {
                    let joined = tokio::task::spawn_blocking(move || join.join())
                        .await
                        .map_err(|error| {
                            format!("context/compaction worker join task failed: {error}")
                        })?;
                    state.worker_joined.store(true, Ordering::Release);
                    state.changed.notify_waiters();
                    if joined.is_err() {
                        return Err("context/compaction worker panicked".into());
                    }
                }
            }
            while !state.worker_joined.load(Ordering::Acquire) {
                let changed = state.changed.notified();
                if state.worker_joined.load(Ordering::Acquire) {
                    break;
                }
                changed.await;
            }
            Ok(())
        })
    }
}

pub(crate) async fn start_candidate() -> anyhow::Result<ManagedGenerationCandidate> {
    let (commands, receiver) = mpsc::channel(QUEUE_CAPACITY);
    let state = Arc::new(WorkerState {
        stopping: AtomicBool::new(false),
        worker_joined: AtomicBool::new(false),
        changed: Notify::new(),
        join: Mutex::new(None),
    });
    let worker_state = Arc::clone(&state);
    let join = std::thread::Builder::new()
        .name("omegon-context-compaction".into())
        .spawn(move || run_worker(receiver, worker_state))?;
    *state
        .join
        .lock()
        .map_err(|_| anyhow::anyhow!("context/compaction worker join lock poisoned"))? = Some(join);

    let controller: Arc<dyn ManagedResourceController> = Arc::new(WorkerController {
        state,
        commands: commands.clone(),
    });
    let resources = vec![ManagedResourceRegistration::new(
        RuntimeContributionResourceId::new(WORKER_RESOURCE).expect("static resource id is valid"),
        RuntimeOwnedResourceKind::Task,
        RuntimeCleanupAssurance::Strict,
        Vec::new(),
        controller,
    )];
    let mut candidate = ManagedGenerationCandidate::new(
        RuntimeCompositionGenerationId::new("composition:context-compaction-boot")
            .expect("static composition id is valid"),
        omegon_traits::RuntimeContributionId::new("feature:context-compaction")
            .expect("static contribution id is valid"),
        RuntimeContributionGenerationId::new(CONTEXT_COMPACTION_GENERATION)
            .expect("static generation id is valid"),
        Duration::from_secs(30),
        Duration::from_secs(5),
        resources,
    )?;
    candidate.add_service(
        context_compaction_capability_id(),
        context_compaction_interface_id(),
        Arc::new(ContextCompactionService { commands }),
    )?;
    Ok(candidate)
}

fn run_worker(mut receiver: mpsc::Receiver<WorkerCommand>, state: Arc<WorkerState>) {
    while let Some(command) = receiver.blocking_recv() {
        if state.stopping.load(Ordering::Acquire) {
            break;
        }
        let caller = command.request.cancellation().clone();
        let generation = command.generation_cancellation.clone();
        let result = if caller.is_cancelled() || generation.is_cancelled() {
            Err(ContextCompactionServiceErrorV1::cancelled())
        } else {
            execute_request(command.request, || {
                state.stopping.load(Ordering::Acquire)
                    || caller.is_cancelled()
                    || generation.is_cancelled()
            })
        };
        let _ = command.response.send(result);
    }
}

fn execute_request(
    request: ContextCompactionRequestV1,
    is_cancelled: impl Fn() -> bool,
) -> Result<ContextCompactionResponseV1, ContextCompactionServiceErrorV1> {
    match request {
        ContextCompactionRequestV1::Plan { snapshot, mode, .. } => Ok(
            ContextCompactionResponseV1::Plan(plan_compaction(&snapshot, mode, is_cancelled)?),
        ),
    }
}

fn plan_compaction(
    snapshot: &ContextCompactionSnapshotV1,
    mode: ContextCompactionModeV1,
    is_cancelled: impl Fn() -> bool,
) -> Result<Option<ContextCompactionPlanV1>, ContextCompactionServiceErrorV1> {
    let primary_window = match mode {
        ContextCompactionModeV1::Pressure | ContextCompactionModeV1::Overflow => {
            snapshot.decay_window
        }
        ContextCompactionModeV1::Manual => MANUAL_KEEP_RECENT_TURNS,
    };
    if let Some((payload, evict_count)) =
        payload_for_window(snapshot, primary_window, &is_cancelled)?
    {
        return Ok(Some(ContextCompactionPlanV1 {
            payload,
            evict_count,
            reason: None,
            application: if mode == ContextCompactionModeV1::Manual {
                ContextCompactionApplicationV1::KeepRecent(MANUAL_KEEP_RECENT_TURNS)
            } else {
                ContextCompactionApplicationV1::DecayWindow
            },
        }));
    }
    if mode != ContextCompactionModeV1::Pressure {
        return Ok(None);
    }
    payload_for_window(snapshot, PRESSURE_KEEP_RECENT_TURNS, &is_cancelled).map(|plan| {
        plan.map(|(payload, evict_count)| ContextCompactionPlanV1 {
            payload,
            evict_count,
            reason: Some(format!(
                "no decay-window payload; compacting under token pressure with keep_recent_turns={PRESSURE_KEEP_RECENT_TURNS}"
            )),
            application: ContextCompactionApplicationV1::KeepRecent(
                PRESSURE_KEEP_RECENT_TURNS,
            ),
        })
    })
}

fn payload_for_window(
    snapshot: &ContextCompactionSnapshotV1,
    keep_recent_turns: u32,
    is_cancelled: &impl Fn() -> bool,
) -> Result<Option<(String, usize)>, ContextCompactionServiceErrorV1> {
    let evictable = snapshot
        .messages
        .iter()
        .filter(|message| {
            snapshot.current_turn.saturating_sub(message_turn(message)) > keep_recent_turns
        })
        .collect::<Vec<_>>();
    if evictable.is_empty() {
        return Ok(None);
    }

    let mut payload = String::from(
        "Summarize this conversation excerpt. Preserve:\n\
         - What was accomplished (files changed, decisions made)\n\
         - What failed and why\n\
         - Current task and approach\n\
         - Key constraints discovered\n\
         Be concise but preserve actionable context.\n\n---\n\n",
    );
    for message in &evictable {
        if is_cancelled() {
            return Err(ContextCompactionServiceErrorV1::cancelled());
        }
        match message {
            AgentMessage::User { text, turn, .. } => {
                payload.push_str(&format!("[Turn {turn}] User: {text}\n\n"));
            }
            AgentMessage::Assistant(assistant, turn) => {
                let text = if assistant.text.len() > 200 {
                    crate::util::truncate(&assistant.text, 200)
                } else {
                    assistant.text.clone()
                };
                payload.push_str(&format!("[Turn {turn}] Assistant: {text}\n"));
                if !assistant.tool_calls.is_empty() {
                    let tools = assistant
                        .tool_calls
                        .iter()
                        .map(|call| call.name.as_str())
                        .collect::<Vec<_>>();
                    payload.push_str(&format!("  Tools called: {}\n", tools.join(", ")));
                }
                payload.push('\n');
            }
            AgentMessage::ToolResult(result, turn) => {
                let status = if result.is_error { "ERROR" } else { "ok" };
                payload.push_str(&format!(
                    "[Turn {turn}] Tool {}: {status}\n\n",
                    result.tool_name
                ));
            }
            AgentMessage::OperatorToolObservation(observation, turn) => {
                let status = if observation.is_error { "ERROR" } else { "ok" };
                let command = observation
                    .arguments
                    .get("command")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("<unknown>");
                payload.push_str(&format!(
                    "[Turn {turn}] Operator ran {} ({status}): {command}\n\n",
                    observation.tool_name
                ));
            }
        }
    }
    Ok(Some((payload, evictable.len())))
}

fn message_turn(message: &AgentMessage) -> u32 {
    match message {
        AgentMessage::User { turn, .. }
        | AgentMessage::Assistant(_, turn)
        | AgentMessage::ToolResult(_, turn)
        | AgentMessage::OperatorToolObservation(_, turn) => *turn,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::ConversationState;

    #[test]
    fn managed_planner_preserves_direct_pressure_and_manual_payloads() {
        let mut conversation = ConversationState::new();
        conversation.push_user("old context".into());
        conversation.intent.stats.turns = 20;
        conversation.push_user("recent context".into());
        let snapshot = conversation.context_compaction_snapshot();

        let pressure = plan_compaction(&snapshot, ContextCompactionModeV1::Pressure, || false)
            .unwrap()
            .expect("pressure plan");
        let direct = conversation
            .build_compaction_payload()
            .expect("direct plan");
        assert_eq!((pressure.payload, pressure.evict_count), direct);

        let manual = plan_compaction(&snapshot, ContextCompactionModeV1::Manual, || false)
            .unwrap()
            .expect("manual plan");
        let direct = conversation
            .build_compaction_payload_keeping_recent(MANUAL_KEEP_RECENT_TURNS)
            .expect("direct manual plan");
        assert_eq!((manual.payload, manual.evict_count), direct);
    }

    #[test]
    fn managed_planner_cancellation_has_no_plan() {
        let snapshot = ContextCompactionSnapshotV1 {
            messages: vec![AgentMessage::User {
                text: "old".into(),
                images: Vec::new(),
                turn: 0,
            }],
            current_turn: 10,
            decay_window: 1,
        };
        let error =
            plan_compaction(&snapshot, ContextCompactionModeV1::Overflow, || true).unwrap_err();
        assert_eq!(error.code, ContextCompactionServiceErrorCodeV1::Cancelled);
    }

    #[tokio::test]
    async fn absent_binding_is_typed_unavailable() {
        let error = ContextCompactionBinding::default()
            .plan(
                ContextCompactionSnapshotV1 {
                    messages: Vec::new(),
                    current_turn: 0,
                    decay_window: 10,
                },
                ContextCompactionModeV1::Pressure,
                CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            ManagedServiceCallError::Operation(ContextCompactionServiceErrorV1 {
                code: ContextCompactionServiceErrorCodeV1::Unavailable,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn published_handle_keeps_generation_and_settles_strict_worker() {
        let mut bus = crate::bus::EventBus::new();
        bus.register(Box::new(ContextCompactionFeature));
        bus.stage_managed_generation("context-compaction", start_candidate().await.unwrap())
            .unwrap();
        bus.try_finalize_managed().await.unwrap();

        let binding = ContextCompactionBinding::default();
        binding.capture(&bus).unwrap();
        let handle = binding.handle().expect("published handle");
        assert_eq!(handle.capability_id.as_str(), CONTEXT_COMPACTION_CAPABILITY);
        assert_eq!(handle.owner.as_str(), "feature:context-compaction");
        assert_eq!(handle.generation_id.as_str(), CONTEXT_COMPACTION_GENERATION);

        let report = bus.shutdown_managed_services().await;
        assert!(report.all_resources_settled(), "{report:?}");
        let error = handle
            .invoke(ContextCompactionRequestV1::Plan {
                snapshot: ContextCompactionSnapshotV1 {
                    messages: Vec::new(),
                    current_turn: 0,
                    decay_window: 10,
                },
                mode: ContextCompactionModeV1::Pressure,
                cancellation: CancellationToken::new(),
            })
            .await
            .unwrap_err();
        assert!(matches!(error, ManagedServiceCallError::GenerationRetired));
    }
}
