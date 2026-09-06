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
    pub(crate) retained_token_budget: Option<usize>,
    pub(crate) previous_summary: Option<String>,
}

impl ContextCompactionSnapshotV1 {
    pub(crate) fn with_retained_token_budget(mut self, tokens: usize) -> Self {
        self.retained_token_budget = Some(tokens);
        self
    }
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

#[derive(Debug, Clone)]
pub(crate) struct ContextCompactionPlanV1 {
    pub(crate) payload: String,
    pub(crate) evict_count: usize,
    pub(crate) reason: Option<String>,
    pub(crate) application: ContextCompactionApplicationV1,
    pub(crate) source_messages: Vec<crate::bridge::LlmMessage>,
    pub(crate) source_is_prefix: bool,
    pub(crate) previous_summary: Option<String>,
}

impl ContextCompactionPlanV1 {
    pub(crate) fn apply(
        self,
        conversation: &mut crate::conversation::ConversationState,
        summary: String,
    ) {
        match self.application {
            ContextCompactionApplicationV1::DecayWindow => conversation.apply_compaction(summary),
            ContextCompactionApplicationV1::KeepRecent(turns) => {
                conversation.apply_compaction_keeping_recent(summary, turns);
            }
        }
    }
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
                    retained_token_budget: None,
                    previous_summary: None,
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
    if is_cancelled() {
        return Err(ContextCompactionServiceErrorV1::cancelled());
    }
    if let Some(budget) = snapshot.retained_token_budget {
        let Some((mut window, mut retained_tokens)) = budgeted_window(
            snapshot,
            primary_window,
            primary_window,
            budget,
            &is_cancelled,
        )?
        else {
            return Ok(None);
        };
        let mut payload = payload_for_window(snapshot, window, &is_cancelled)?;
        if payload.is_none()
            && mode == ContextCompactionModeV1::Pressure
            && let Some(selection) = budgeted_window(
                snapshot,
                PRESSURE_KEEP_RECENT_TURNS,
                primary_window,
                budget,
                &is_cancelled,
            )?
        {
            (window, retained_tokens) = selection;
            payload = payload_for_window(snapshot, window, &is_cancelled)?;
        }
        return Ok(payload.map(|(payload, evict_count)| ContextCompactionPlanV1 {
                source_is_prefix: evictions_are_prefix(snapshot, window),
                source_messages: snapshot.messages.iter().map(crate::conversation::ConversationState::to_llm_message).collect(),
                previous_summary: snapshot.previous_summary.clone(),
                payload,
                evict_count,
                reason: Some(if retained_tokens > budget {
                    format!("protected recent turn/tool exchange exceeds retained context target: estimated {retained_tokens} tokens, target {budget}; preserved complete messages")
                } else {
                    format!("token-budgeted retention: estimated {retained_tokens} tokens, target {budget}, keep_recent_turns={window}")
                }),
                application: ContextCompactionApplicationV1::KeepRecent(window),
        }));
    }
    if let Some((payload, evict_count)) =
        payload_for_window(snapshot, primary_window, &is_cancelled)?
    {
        return Ok(Some(ContextCompactionPlanV1 {
            source_is_prefix: evictions_are_prefix(snapshot, primary_window),
            source_messages: snapshot
                .messages
                .iter()
                .map(crate::conversation::ConversationState::to_llm_message)
                .collect(),
            previous_summary: snapshot.previous_summary.clone(),
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
            source_is_prefix: evictions_are_prefix(snapshot, PRESSURE_KEEP_RECENT_TURNS),
            source_messages: snapshot.messages.iter().map(crate::conversation::ConversationState::to_llm_message).collect(),
            previous_summary: snapshot.previous_summary.clone(),
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

// A boundary is eligible only when it keeps complete numeric turns and every
// message sharing a tool-call ID on the same side. Ages match the application's
// saturating arithmetic, including restored histories whose turn IDs coincide.
fn budgeted_window(
    snapshot: &ContextCompactionSnapshotV1,
    age_window: u32,
    protection_window: u32,
    budget: usize,
    is_cancelled: &impl Fn() -> bool,
) -> Result<Option<(u32, usize)>, ContextCompactionServiceErrorV1> {
    let mut turns = std::collections::BTreeMap::<u32, usize>::new();
    let mut exchanges = std::collections::HashMap::<&str, (u32, u32)>::new();
    for message in &snapshot.messages {
        if is_cancelled() {
            return Err(ContextCompactionServiceErrorV1::cancelled());
        }
        let age = snapshot.current_turn.saturating_sub(message_turn(message));
        let wire = crate::conversation::ConversationState::to_llm_message(message);
        let mut chars = wire.char_count();
        // The shared estimator already charges tool-result images. Charge user
        // images too; base64 size is deliberately conservative, not a tokenizer.
        if let crate::bridge::LlmMessage::User { images, .. } = &wire {
            chars = images.iter().fold(chars, |sum, image| {
                sum.saturating_add(image.data.len())
                    .saturating_add(image.media_type.len())
            });
        }
        let turn_chars = turns.entry(age).or_default();
        *turn_chars = turn_chars.saturating_add(chars);
        let mut record = |id| {
            let span = exchanges.entry(id).or_insert((age, age));
            span.0 = span.0.min(age);
            span.1 = span.1.max(age);
        };
        match message {
            AgentMessage::Assistant(assistant, _) => {
                for call in &assistant.tool_calls {
                    record(call.id.as_str());
                }
            }
            AgentMessage::ToolResult(result, _) => record(result.call_id.as_str()),
            _ => {}
        }
    }
    // The loop advances current_turn before planning, so its newest populated
    // turn normally has age one. Protect that turn while it remains in the
    // primary recent window. Entirely old idle history can still be compacted.
    if turns
        .first_key_value()
        .is_none_or(|(&age, _)| age > protection_window)
    {
        turns.insert(0, 0);
    }
    let mut chars = 0usize;
    let mut protected = None;
    let mut selected = None;
    for (age, turn_chars) in turns {
        if is_cancelled() {
            return Err(ContextCompactionServiceErrorV1::cancelled());
        }
        chars = chars.saturating_add(turn_chars);
        if !evictions_are_prefix(snapshot, age)
            || exchanges
                .values()
                .any(|&(newest, oldest)| newest <= age && age < oldest)
        {
            continue;
        }
        let tokens = crate::util::estimate_chars_to_tokens(chars);
        // The first safe boundary includes the newest recent turn and linked tool
        // exchange, even if that protected group exceeds the requested budget.
        protected.get_or_insert((age, tokens));
        if age <= age_window && tokens <= budget {
            selected = Some((age, tokens));
        }
    }
    Ok(selected.or(protected))
}

// Restored operator observations can carry older turn IDs after newer messages.
// A durable replacement cuts by source order, so an age boundary must also
// select a chronological suffix. Keep later old messages with that suffix.
fn evictions_are_prefix(snapshot: &ContextCompactionSnapshotV1, keep_recent_turns: u32) -> bool {
    let mut retained_seen = false;
    for message in &snapshot.messages {
        let retained =
            snapshot.current_turn.saturating_sub(message_turn(message)) <= keep_recent_turns;
        if !retained && retained_seen {
            return false;
        }
        retained_seen |= retained;
    }
    true
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
    if let Some(summary) = &snapshot.previous_summary {
        payload.push_str("[Previous conversation summary]\n");
        payload.push_str(summary);
        payload.push_str("\n[End previous summary]\n\n");
    }
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

    fn retention_history() -> ConversationState {
        let mut conversation = ConversationState::new();
        for turn in 0..=3 {
            conversation.intent.stats.turns = turn;
            conversation.push_user(if turn == 3 {
                "newest active request".into()
            } else {
                "x".repeat(4_000)
            });
        }
        conversation
    }

    #[test]
    fn token_retention_compacts_large_recent_turns_in_every_mode() {
        let conversation = retention_history();
        for mode in [
            ContextCompactionModeV1::Pressure,
            ContextCompactionModeV1::Overflow,
            ContextCompactionModeV1::Manual,
        ] {
            let snapshot = conversation
                .context_compaction_snapshot()
                .with_retained_token_budget(100);
            let plan = plan_compaction(&snapshot, mode, || false)
                .unwrap()
                .expect("recent oversized history must compact");
            assert_eq!(plan.evict_count, 3);
            assert_eq!(
                plan.application,
                ContextCompactionApplicationV1::KeepRecent(0)
            );
        }
    }

    #[test]
    fn token_retention_protects_latest_turn_for_zero_and_small_budgets() {
        for budget in [0, 1] {
            let mut conversation = retention_history();
            let snapshot = conversation
                .context_compaction_snapshot()
                .with_retained_token_budget(budget);
            let plan = plan_compaction(&snapshot, ContextCompactionModeV1::Pressure, || false)
                .unwrap()
                .unwrap();
            assert!(
                plan.reason
                    .as_deref()
                    .unwrap()
                    .contains("exceeds retained context target")
            );
            let removed = plan.evict_count;
            plan.apply(&mut conversation, "summary".into());
            assert_eq!(
                snapshot.messages.len() - conversation.replay_messages().len(),
                removed
            );
            assert_eq!(conversation.replay_messages().len(), 1);
            assert_eq!(conversation.last_user_prompt(), "newest active request");
        }
    }

    #[test]
    fn token_retention_preserves_cross_turn_tool_exchange() {
        let mut conversation = ConversationState::new();
        conversation.push_user("old".repeat(100));
        conversation.intent.stats.turns = 1;
        conversation.push_assistant(crate::conversation::AssistantMessage {
            text: "using tool".into(),
            tool_calls: vec![crate::conversation::ToolCall {
                id: "call".into(),
                name: "read".into(),
                arguments: serde_json::json!({"path": "file"}),
            }],
            ..Default::default()
        });
        conversation.intent.stats.turns = 2;
        conversation.push_tool_result(crate::conversation::ToolResultEntry {
            call_id: "call".into(),
            tool_name: "read".into(),
            content: vec![omegon_traits::ContentBlock::Text {
                text: "large".repeat(1000),
            }],
            is_error: false,
            args_summary: None,
        });
        conversation.intent.stats.turns = 3;
        let snapshot = conversation
            .context_compaction_snapshot()
            .with_retained_token_budget(0);
        let plan = plan_compaction(&snapshot, ContextCompactionModeV1::Overflow, || false)
            .unwrap()
            .unwrap();
        assert_eq!(
            plan.application,
            ContextCompactionApplicationV1::KeepRecent(2)
        );
        assert_eq!(plan.evict_count, 1);
        assert!(
            plan.reason
                .as_deref()
                .unwrap()
                .contains("exceeds retained context target")
        );
        plan.apply(&mut conversation, "summary".into());
        assert!(matches!(
            conversation.replay_messages(),
            [
                AgentMessage::Assistant(_, 1),
                AgentMessage::ToolResult(_, 2)
            ]
        ));
    }

    #[test]
    fn token_retention_keeps_largest_complete_suffix_and_carries_summary() {
        let mut conversation = ConversationState::new();
        conversation.push_user("initial".into());
        conversation.intent.stats.turns = 20;
        conversation.apply_compaction("earlier constraints must survive".into());
        for turn in 20..=23 {
            conversation.intent.stats.turns = turn;
            conversation.push_user("abcd".repeat(10));
        }
        let snapshot = conversation
            .context_compaction_snapshot()
            .with_retained_token_budget(25);
        let plan = plan_compaction(&snapshot, ContextCompactionModeV1::Overflow, || false)
            .unwrap()
            .unwrap();
        assert_eq!(
            plan.application,
            ContextCompactionApplicationV1::KeepRecent(1)
        );
        assert_eq!(plan.evict_count, 2);
        assert!(plan.payload.contains("earlier constraints must survive"));
        plan.apply(&mut conversation, "new summary".into());
        assert_eq!(conversation.replay_messages().len(), 2);
    }

    #[test]
    fn token_retention_counts_nontext_history_and_user_images() {
        let mut conversation = ConversationState::new();
        conversation.push_assistant(crate::conversation::AssistantMessage {
            text: "short".into(),
            thinking: Some("think".repeat(100)),
            tool_calls: vec![crate::conversation::ToolCall {
                id: "call".into(),
                name: "read".into(),
                arguments: serde_json::json!({"path": "x".repeat(400)}),
            }],
            ..Default::default()
        });
        conversation.push_tool_result(crate::conversation::ToolResultEntry {
            call_id: "call".into(),
            tool_name: "read".into(),
            content: vec![omegon_traits::ContentBlock::Text {
                text: "result".repeat(100),
            }],
            is_error: false,
            args_summary: None,
        });
        conversation.intent.stats.turns = 1;
        conversation.push_user("current".into());
        let snapshot = conversation
            .context_compaction_snapshot()
            .with_retained_token_budget(20);
        let plan = plan_compaction(&snapshot, ContextCompactionModeV1::Pressure, || false)
            .unwrap()
            .unwrap();
        assert_eq!(plan.evict_count, 2);
        let mut snapshot = conversation
            .context_compaction_snapshot()
            .with_retained_token_budget(20);
        snapshot.messages = vec![
            AgentMessage::User {
                text: "image".into(),
                images: vec![crate::bridge::ImageAttachment {
                    data: "x".repeat(4000),
                    media_type: "image/png".into(),
                    source_path: None,
                }],
                turn: 0,
            },
            AgentMessage::User {
                text: "current".into(),
                images: vec![],
                turn: 1,
            },
        ];
        let plan = plan_compaction(&snapshot, ContextCompactionModeV1::Pressure, || false)
            .unwrap()
            .unwrap();
        assert_eq!(plan.evict_count, 1);
    }

    #[test]
    fn token_retention_protects_latest_populated_turn_after_loop_advances() {
        for mode in [
            ContextCompactionModeV1::Pressure,
            ContextCompactionModeV1::Overflow,
            ContextCompactionModeV1::Manual,
        ] {
            let mut conversation = retention_history();
            // The production loop increments its turn before planning context.
            conversation.intent.stats.turns += 1;
            let snapshot = conversation
                .context_compaction_snapshot()
                .with_retained_token_budget(0);
            let plan = plan_compaction(&snapshot, mode, || false).unwrap().unwrap();
            assert_eq!(
                plan.evict_count, 3,
                "latest populated turn must survive {mode:?}"
            );
            assert_eq!(
                plan.application,
                ContextCompactionApplicationV1::KeepRecent(1)
            );
            plan.apply(&mut conversation, "summary".into());
            assert_eq!(conversation.last_user_prompt(), "newest active request");
        }
    }

    #[test]
    fn token_retention_pressure_fallback_preserves_newest_within_primary_window() {
        let mut conversation = ConversationState::new();
        conversation.push_user("newest recent request".into());
        conversation.intent.stats.turns = 6;
        let snapshot = conversation
            .context_compaction_snapshot()
            .with_retained_token_budget(100);
        // Fallback age four must not override the primary recent-window protection.
        assert!(
            plan_compaction(&snapshot, ContextCompactionModeV1::Pressure, || false)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn token_retention_reordered_turns_preserve_chronological_suffix() {
        let mut conversation = ConversationState::new();
        for (turn, text) in [(0, "old"), (2, "newest"), (1, "restored observation")] {
            conversation.intent.stats.turns = turn;
            conversation.push_user(text.into());
        }
        conversation.intent.stats.turns = 2;
        let snapshot = conversation
            .context_compaction_snapshot()
            .with_retained_token_budget(0);
        let plan = plan_compaction(&snapshot, ContextCompactionModeV1::Pressure, || false)
            .unwrap()
            .unwrap();
        assert_eq!(
            plan.evict_count, 1,
            "older restored message must widen retained suffix"
        );
        assert!(plan.source_is_prefix);
        assert_eq!(
            plan.application,
            ContextCompactionApplicationV1::KeepRecent(1)
        );
        plan.apply(&mut conversation, "summary".into());
        assert!(matches!(
            conversation.replay_messages(),
            [
                AgentMessage::User { turn: 2, .. },
                AgentMessage::User { turn: 1, .. }
            ]
        ));
    }

    #[test]
    fn token_retention_legacy_plans_flag_nonprefix_eviction() {
        for current_turn in [6, 20] {
            let mut conversation = ConversationState::new();
            for (turn, text) in [
                (0, "old"),
                (current_turn, "newest"),
                (1, "restored observation"),
            ] {
                conversation.intent.stats.turns = turn;
                conversation.push_user(text.into());
            }
            conversation.intent.stats.turns = current_turn;
            let snapshot = conversation.context_compaction_snapshot();
            let plan = plan_compaction(&snapshot, ContextCompactionModeV1::Pressure, || false)
                .unwrap()
                .unwrap();
            assert!(
                !plan.source_is_prefix,
                "legacy age/fallback plans must report nonprefix selection"
            );
            assert_eq!(plan.evict_count, 2);
        }
    }

    #[test]
    fn token_retention_old_only_history_can_be_compacted() {
        let mut conversation = ConversationState::new();
        conversation.push_user("old".into());
        conversation.intent.stats.turns = 99;
        let snapshot = conversation
            .context_compaction_snapshot()
            .with_retained_token_budget(100);
        let plan = plan_compaction(&snapshot, ContextCompactionModeV1::Manual, || false)
            .unwrap()
            .unwrap();
        assert_eq!(plan.evict_count, 1);
        plan.apply(&mut conversation, "summary".into());
        assert!(conversation.replay_messages().is_empty());
    }

    #[test]
    fn token_retention_small_history_and_legacy_pressure_fallback() {
        let mut conversation = ConversationState::new();
        conversation.push_user("small".into());
        conversation.intent.stats.turns = 6;
        conversation.push_user("new".into());
        let snapshot = conversation
            .context_compaction_snapshot()
            .with_retained_token_budget(100);
        assert!(
            plan_compaction(&snapshot, ContextCompactionModeV1::Overflow, || false)
                .unwrap()
                .is_none()
        );
        let pressure = plan_compaction(&snapshot, ContextCompactionModeV1::Pressure, || false)
            .unwrap()
            .unwrap();
        assert_eq!(pressure.evict_count, 1);
        let snapshot = ConversationState::new()
            .context_compaction_snapshot()
            .with_retained_token_budget(0);
        assert!(
            plan_compaction(&snapshot, ContextCompactionModeV1::Pressure, || false)
                .unwrap()
                .is_none()
        );
        let error =
            plan_compaction(&snapshot, ContextCompactionModeV1::Pressure, || true).unwrap_err();
        assert_eq!(error.code, ContextCompactionServiceErrorCodeV1::Cancelled);
    }

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
            retained_token_budget: None,
            previous_summary: None,
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
                    retained_token_budget: None,
                    previous_summary: None,
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
                    retained_token_budget: None,
                    previous_summary: None,
                },
                mode: ContextCompactionModeV1::Pressure,
                cancellation: CancellationToken::new(),
            })
            .await
            .unwrap_err();
        assert!(matches!(error, ManagedServiceCallError::GenerationRetired));
    }
}
