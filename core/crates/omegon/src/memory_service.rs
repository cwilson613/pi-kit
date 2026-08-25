//! Boot-captured managed ownership of durable memory stores.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use async_trait::async_trait;
use omegon_memory::backend::MemoryStats;
use omegon_memory::{
    Edge, EmbeddingMetadata, Episode, Fact, FactFilter, MemoryBackend, MemoryError, MemoryMutation,
    MemoryMutationOutcome, ScoredFact, SqliteBackend,
};
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

use crate::managed_service_bus::{ManagedGenerationCandidate, ManagedResourceRegistration};
use crate::service_generation::ManagedServiceHandle;

pub(crate) const MEMORY_CAPABILITY: &str = "service:memory";
pub(crate) const MEMORY_INTERFACE: &str = "interface:omegon-memory-v1";
pub(crate) const MEMORY_GENERATION: &str = "contribution:memory-managed-v1";
const WORKER_RESOURCE: &str = "resource:memory-worker";
const WRITER_RESOURCE: &str = "resource:memory-writer";
const DTO_VERSION: u32 = 1;
const QUEUE_CAPACITY: usize = 16;
const MAX_RESULT_LIMIT: usize = 10_000;
const MAX_VECTOR_DIMENSIONS: usize = 16_384;

pub(crate) fn memory_capability_id() -> RuntimeCapabilityId {
    RuntimeCapabilityId::new(MEMORY_CAPABILITY).expect("static capability id is valid")
}

pub(crate) fn memory_interface_id() -> RuntimeServiceInterfaceId {
    RuntimeServiceInterfaceId::new(MEMORY_INTERFACE).expect("static interface id is valid")
}

#[derive(Clone, Default)]
pub(crate) struct MemoryBinding {
    handle: Arc<OnceLock<Option<ManagedServiceHandle<MemoryService>>>>,
}

impl MemoryBinding {
    pub(crate) fn capture(&self, bus: &crate::bus::EventBus) -> anyhow::Result<()> {
        let handle =
            bus.managed_service::<MemoryService>(&memory_capability_id(), &memory_interface_id())?;
        self.handle
            .set(handle)
            .map_err(|_| anyhow::anyhow!("memory managed handle was already captured"))
    }

    pub(crate) fn handle(&self) -> Option<ManagedServiceHandle<MemoryService>> {
        self.handle.get().and_then(Clone::clone)
    }

    pub(crate) fn available(&self) -> bool {
        self.handle().is_some()
    }

    pub(crate) async fn invoke(
        &self,
        request: MemoryRequestV1,
    ) -> Result<MemoryResponseV1, ManagedServiceCallError<MemoryServiceErrorV1>> {
        let Some(handle) = self.handle() else {
            return Err(ManagedServiceCallError::Operation(
                MemoryServiceErrorV1::new(
                    MemoryServiceErrorCodeV1::Unavailable,
                    "managed memory service is unavailable",
                ),
            ));
        };
        handle.invoke(request).await
    }
}

/// Declares optional memory ownership when no compatibility MemoryFeature exists.
pub(crate) struct MemoryDeclarationFeature;

#[async_trait]
impl Feature for MemoryDeclarationFeature {
    fn name(&self) -> &str {
        "memory"
    }

    fn runtime_contribution_generation_id(&self) -> Option<RuntimeContributionGenerationId> {
        Some(RuntimeContributionGenerationId::new(MEMORY_GENERATION).expect("static id is valid"))
    }

    fn runtime_lifecycle_policy(&self) -> Option<RuntimeLifecyclePolicy> {
        Some(memory_lifecycle_policy())
    }

    fn runtime_transition_policy(&self) -> Option<RuntimeCompositionTransitionPolicy> {
        Some(memory_transition_policy())
    }
}

pub(crate) fn memory_lifecycle_policy() -> RuntimeLifecyclePolicy {
    RuntimeLifecyclePolicy {
        requirement: RuntimeLifecycleRequirement::Optional,
        failure_disposition: RuntimeFailureDisposition::DegradeLocally,
        readiness_timeout_ms: 0,
        heartbeat_timeout_ms: None,
        restart_limit: 0,
    }
}

pub(crate) fn memory_transition_policy() -> RuntimeCompositionTransitionPolicy {
    RuntimeCompositionTransitionPolicy {
        activation_boundary: RuntimeActivationBoundary::Boot,
        cleanup: RuntimeCleanupRequirement::Strict,
        cleanup_timeout_ms: 5_000,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MemoryScopeV1 {
    Project,
    Global,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum MemoryRequestV1 {
    Status {
        scope: MemoryScopeV1,
        #[serde(skip, default)]
        cancellation: CancellationToken,
    },
    Stats {
        scope: MemoryScopeV1,
        mind: String,
        #[serde(skip, default)]
        cancellation: CancellationToken,
    },
    GetFact {
        scope: MemoryScopeV1,
        id: String,
        #[serde(skip, default)]
        cancellation: CancellationToken,
    },
    ListFacts {
        scope: MemoryScopeV1,
        mind: String,
        filter: FactFilter,
        #[serde(skip, default)]
        cancellation: CancellationToken,
    },
    FtsSearch {
        scope: MemoryScopeV1,
        mind: String,
        query: String,
        limit: usize,
        #[serde(skip, default)]
        cancellation: CancellationToken,
    },
    VectorSearch {
        scope: MemoryScopeV1,
        mind: String,
        vector: Vec<f32>,
        limit: usize,
        min_similarity: f32,
        #[serde(skip, default)]
        cancellation: CancellationToken,
    },
    EmbeddingMetadata {
        scope: MemoryScopeV1,
        mind: String,
        #[serde(skip, default)]
        cancellation: CancellationToken,
    },
    GetEdges {
        scope: MemoryScopeV1,
        mind: String,
        fact_id: String,
        #[serde(skip, default)]
        cancellation: CancellationToken,
    },
    ListEpisodes {
        scope: MemoryScopeV1,
        mind: String,
        limit: usize,
        #[serde(skip, default)]
        cancellation: CancellationToken,
    },
    SearchEpisodes {
        scope: MemoryScopeV1,
        mind: String,
        query: String,
        limit: usize,
        #[serde(skip, default)]
        cancellation: CancellationToken,
    },
    ApplyMutation {
        scope: MemoryScopeV1,
        operation_id: String,
        mutation: MemoryMutation,
        #[serde(skip, default)]
        cancellation: CancellationToken,
    },
    #[cfg(test)]
    #[serde(skip)]
    TestBlock {
        started: std::sync::mpsc::SyncSender<()>,
        release: Arc<(Mutex<bool>, std::sync::Condvar)>,
        cancellation: CancellationToken,
    },
    #[cfg(test)]
    #[serde(skip)]
    TestRecord {
        executions: Arc<std::sync::atomic::AtomicUsize>,
        cancellation: CancellationToken,
    },
    #[cfg(test)]
    #[serde(skip)]
    TestPanic { cancellation: CancellationToken },
    #[cfg(test)]
    #[serde(skip)]
    TestAtomicMutation {
        started: std::sync::mpsc::SyncSender<()>,
        release: Arc<(Mutex<bool>, std::sync::Condvar)>,
        operation_id: String,
        mutation: MemoryMutation,
        cancellation: CancellationToken,
    },
}

impl MemoryRequestV1 {
    fn cancellation(&self) -> &CancellationToken {
        match self {
            Self::Status { cancellation, .. }
            | Self::Stats { cancellation, .. }
            | Self::GetFact { cancellation, .. }
            | Self::ListFacts { cancellation, .. }
            | Self::FtsSearch { cancellation, .. }
            | Self::VectorSearch { cancellation, .. }
            | Self::EmbeddingMetadata { cancellation, .. }
            | Self::GetEdges { cancellation, .. }
            | Self::ListEpisodes { cancellation, .. }
            | Self::SearchEpisodes { cancellation, .. }
            | Self::ApplyMutation { cancellation, .. } => cancellation,
            #[cfg(test)]
            Self::TestBlock { cancellation, .. }
            | Self::TestRecord { cancellation, .. }
            | Self::TestPanic { cancellation }
            | Self::TestAtomicMutation { cancellation, .. } => cancellation,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub(crate) enum MemoryPayloadV1 {
    Status(MemoryStoreStatusV1),
    Stats(MemoryStats),
    Fact(Box<Option<Fact>>),
    Facts(Vec<Fact>),
    ScoredFacts(Vec<ScoredFact>),
    EmbeddingMetadata(Option<EmbeddingMetadata>),
    Edges(Vec<Edge>),
    Episodes(Vec<Episode>),
    Mutation(MemoryMutationOutcome),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct MemoryResponseV1 {
    pub version: u32,
    pub scope: MemoryScopeV1,
    pub payload: MemoryPayloadV1,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct MemoryStoreStatusV1 {
    pub available: bool,
    pub schema_version: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MemoryServiceErrorCodeV1 {
    Cancelled,
    Unavailable,
    StoreUnavailable,
    FactNotFound,
    EmbeddingDimensionMismatch,
    NoEmbeddings,
    OperationConflict,
    FactVersionConflict,
    InvalidMutation,
    InvalidRequest,
    Storage,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct MemoryServiceErrorV1 {
    pub version: u32,
    pub code: MemoryServiceErrorCodeV1,
    pub message: String,
}

impl MemoryServiceErrorV1 {
    fn new(code: MemoryServiceErrorCodeV1, message: impl Into<String>) -> Self {
        Self {
            version: DTO_VERSION,
            code,
            message: message.into(),
        }
    }

    fn cancelled() -> Self {
        Self::new(
            MemoryServiceErrorCodeV1::Cancelled,
            "memory request cancelled",
        )
    }

    fn from_memory(error: MemoryError) -> Self {
        let code = match &error {
            MemoryError::FactNotFound(_) => MemoryServiceErrorCodeV1::FactNotFound,
            MemoryError::EmbeddingDimensionMismatch { .. } => {
                MemoryServiceErrorCodeV1::EmbeddingDimensionMismatch
            }
            MemoryError::NoEmbeddings => MemoryServiceErrorCodeV1::NoEmbeddings,
            MemoryError::OperationConflict(_) => MemoryServiceErrorCodeV1::OperationConflict,
            MemoryError::FactVersionConflict { .. } => {
                MemoryServiceErrorCodeV1::FactVersionConflict
            }
            MemoryError::InvalidMutation(_) => MemoryServiceErrorCodeV1::InvalidMutation,
            MemoryError::Storage(_) => MemoryServiceErrorCodeV1::Storage,
        };
        Self::new(code, error.to_string())
    }
}

impl std::fmt::Display for MemoryServiceErrorV1 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for MemoryServiceErrorV1 {}

pub(crate) struct MemoryService {
    commands: mpsc::Sender<WorkerCommand>,
}

struct WorkerCommand {
    request: MemoryRequestV1,
    generation_cancellation: CancellationToken,
    response: oneshot::Sender<Result<MemoryResponseV1, MemoryServiceErrorV1>>,
}

impl ManagedServiceContract for MemoryService {
    type Request = MemoryRequestV1;
    type Response = MemoryResponseV1;
    type Error = MemoryServiceErrorV1;

    fn execute<'a>(
        &'a self,
        request: Self::Request,
        context: ManagedCallContext,
    ) -> ManagedServiceFuture<'a, Self::Response, Self::Error> {
        Box::pin(async move {
            let caller = request.cancellation().clone();
            if caller.is_cancelled() || context.cancellation.is_cancelled() {
                return Err(MemoryServiceErrorV1::cancelled());
            }
            let (response, receive) = oneshot::channel();
            let command = WorkerCommand {
                request,
                generation_cancellation: context.cancellation.clone(),
                response,
            };
            tokio::select! {
                biased;
                () = caller.cancelled() => return Err(MemoryServiceErrorV1::cancelled()),
                () = context.cancellation.cancelled() => return Err(MemoryServiceErrorV1::cancelled()),
                sent = self.commands.send(command) => sent.map_err(|_| MemoryServiceErrorV1::new(
                    MemoryServiceErrorCodeV1::Unavailable, "memory worker is unavailable"))?,
            }
            tokio::select! {
                biased;
                () = caller.cancelled() => Err(MemoryServiceErrorV1::cancelled()),
                () = context.cancellation.cancelled() => Err(MemoryServiceErrorV1::cancelled()),
                result = receive => result.map_err(|_| MemoryServiceErrorV1::new(
                    MemoryServiceErrorCodeV1::Unavailable, "memory worker dropped its response"))?,
            }
        })
    }
}

struct WorkerState {
    stopping: AtomicBool,
    stores_closed: AtomicBool,
    worker_joined: AtomicBool,
    worker_failed: AtomicBool,
    changed: Notify,
    join: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl WorkerState {
    fn request_stop(&self) {
        self.stopping.store(true, Ordering::Release);
    }

    fn wake(&self, commands: &mpsc::Sender<WorkerCommand>) {
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        let (response, _) = oneshot::channel();
        let _ = commands.try_send(WorkerCommand {
            request: MemoryRequestV1::Status {
                scope: MemoryScopeV1::Project,
                cancellation: cancellation.clone(),
            },
            generation_cancellation: cancellation,
            response,
        });
    }
}

struct WorkerController {
    state: Arc<WorkerState>,
    commands: mpsc::Sender<WorkerCommand>,
}

struct WriterController {
    state: Arc<WorkerState>,
    commands: mpsc::Sender<WorkerCommand>,
}

impl Drop for WorkerController {
    fn drop(&mut self) {
        self.state.request_stop();
        self.state.wake(&self.commands);
    }
}

impl Drop for WriterController {
    fn drop(&mut self) {
        self.state.request_stop();
        self.state.wake(&self.commands);
    }
}

impl ManagedResourceController for WorkerController {
    fn request_stop(&self) {
        self.state.request_stop();
        self.state.wake(&self.commands);
    }

    fn force_stop(&self) {
        self.request_stop();
    }

    fn await_settled(&self) -> ManagedResourceSettlementFuture<'_> {
        let state = Arc::clone(&self.state);
        Box::pin(async move {
            if !state.worker_joined.load(Ordering::Acquire) {
                let join = state
                    .join
                    .lock()
                    .map_err(|_| "memory worker join lock poisoned".to_string())?
                    .take();
                if let Some(join) = join {
                    let result = tokio::task::spawn_blocking(move || join.join())
                        .await
                        .map_err(|error| format!("memory worker join task failed: {error}"))?;
                    if result.is_err() {
                        state.worker_failed.store(true, Ordering::Release);
                    }
                    state.worker_joined.store(true, Ordering::Release);
                    state.changed.notify_waiters();
                    if result.is_err() {
                        return Err("memory worker terminated after a panic".to_string());
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
            if state.worker_failed.load(Ordering::Acquire) {
                return Err("memory worker terminated after a panic".to_string());
            }
            Ok(())
        })
    }
}

impl ManagedResourceController for WriterController {
    fn request_stop(&self) {
        self.state.request_stop();
        self.state.wake(&self.commands);
    }

    fn force_stop(&self) {
        self.request_stop();
    }

    fn await_settled(&self) -> ManagedResourceSettlementFuture<'_> {
        let state = Arc::clone(&self.state);
        Box::pin(async move {
            loop {
                if state.stores_closed.load(Ordering::Acquire)
                    && state.worker_joined.load(Ordering::Acquire)
                {
                    if state.worker_failed.load(Ordering::Acquire) {
                        return Err("memory worker terminated after a panic".to_string());
                    }
                    return Ok(());
                }
                let changed = state.changed.notified();
                if state.stores_closed.load(Ordering::Acquire)
                    && state.worker_joined.load(Ordering::Acquire)
                {
                    return Ok(());
                }
                changed.await;
            }
        })
    }
}

pub(crate) async fn start_candidate(
    project_path: PathBuf,
    global_path: Option<PathBuf>,
) -> anyhow::Result<ManagedGenerationCandidate> {
    let (commands, receiver) = mpsc::channel(QUEUE_CAPACITY);
    let state = Arc::new(WorkerState {
        stopping: AtomicBool::new(false),
        stores_closed: AtomicBool::new(false),
        worker_joined: AtomicBool::new(false),
        worker_failed: AtomicBool::new(false),
        changed: Notify::new(),
        join: Mutex::new(None),
    });
    let (startup, started) = std::sync::mpsc::sync_channel(1);
    let worker_state = Arc::clone(&state);
    let join = std::thread::Builder::new()
        .name("omegon-memory".into())
        .spawn(move || run_worker(project_path, global_path, receiver, worker_state, startup))?;
    *state
        .join
        .lock()
        .map_err(|_| anyhow::anyhow!("memory worker join lock poisoned"))? = Some(join);
    let startup_result = tokio::task::spawn_blocking(move || started.recv())
        .await
        .map_err(|error| anyhow::anyhow!("memory readiness task failed: {error}"))?
        .map_err(|_| anyhow::anyhow!("memory worker exited before reporting readiness"))?;
    if let Err(error) = startup_result {
        if let Some(join) = state.join.lock().ok().and_then(|mut join| join.take()) {
            let _ = tokio::task::spawn_blocking(move || join.join()).await;
        }
        anyhow::bail!(error);
    }

    let writer_id =
        RuntimeContributionResourceId::new(WRITER_RESOURCE).expect("static resource id is valid");
    let resources = vec![
        ManagedResourceRegistration::new(
            writer_id.clone(),
            RuntimeOwnedResourceKind::DurableWriter,
            RuntimeCleanupAssurance::Strict,
            Vec::new(),
            Arc::new(WriterController {
                state: Arc::clone(&state),
                commands: commands.clone(),
            }),
        ),
        ManagedResourceRegistration::new(
            RuntimeContributionResourceId::new(WORKER_RESOURCE)
                .expect("static resource id is valid"),
            RuntimeOwnedResourceKind::Task,
            RuntimeCleanupAssurance::Strict,
            vec![writer_id],
            Arc::new(WorkerController {
                state,
                commands: commands.clone(),
            }),
        ),
    ];
    let mut candidate = ManagedGenerationCandidate::new(
        RuntimeCompositionGenerationId::new("composition:memory-boot")
            .expect("static composition id is valid"),
        omegon_traits::RuntimeContributionId::new("feature:memory")
            .expect("static contribution id is valid"),
        RuntimeContributionGenerationId::new(MEMORY_GENERATION)
            .expect("static generation id is valid"),
        Duration::from_secs(30),
        Duration::from_secs(5),
        resources,
    )?;
    candidate.add_service(
        memory_capability_id(),
        memory_interface_id(),
        Arc::new(MemoryService { commands }),
    )?;
    Ok(candidate)
}

fn run_worker(
    project_path: PathBuf,
    global_path: Option<PathBuf>,
    mut receiver: mpsc::Receiver<WorkerCommand>,
    state: Arc<WorkerState>,
    startup: std::sync::mpsc::SyncSender<Result<(), String>>,
) {
    struct StoreClosure(Arc<WorkerState>);
    impl Drop for StoreClosure {
        fn drop(&mut self) {
            self.0.stores_closed.store(true, Ordering::Release);
            self.0.changed.notify_waiters();
        }
    }
    let _closure = StoreClosure(Arc::clone(&state));
    let project = match SqliteBackend::open(&project_path) {
        Ok(store) => store,
        Err(error) => {
            let _ = startup.send(Err(error.to_string()));
            return;
        }
    };
    let global = match global_path {
        Some(path) => match SqliteBackend::open_existing(&path) {
            Ok(store) => Some(store),
            Err(error) => {
                tracing::warn!(
                    path = %path.display(),
                    %error,
                    "global memory store is unavailable; project memory remains active"
                );
                None
            }
        },
        None => None,
    };
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            drop(global);
            drop(project);
            let _ = startup.send(Err(error.to_string()));
            return;
        }
    };
    let _ = startup.send(Ok(()));

    while let Some(command) = receiver.blocking_recv() {
        if state.stopping.load(Ordering::Acquire) {
            break;
        }
        let caller = command.request.cancellation().clone();
        let generation = command.generation_cancellation.clone();
        if caller.is_cancelled() || generation.is_cancelled() {
            let _ = command
                .response
                .send(Err(MemoryServiceErrorV1::cancelled()));
            continue;
        }
        let result = execute_request(&runtime, &project, global.as_ref(), command.request);
        let _ = command.response.send(result);
    }
    drop(runtime);
    drop(global);
    drop(project);
}

fn execute_request(
    runtime: &tokio::runtime::Runtime,
    project: &SqliteBackend,
    global: Option<&SqliteBackend>,
    request: MemoryRequestV1,
) -> Result<MemoryResponseV1, MemoryServiceErrorV1> {
    validate_request(&request)?;
    let scope = request_scope(&request);
    let backend = match scope {
        MemoryScopeV1::Project => project,
        MemoryScopeV1::Global => match global {
            Some(global) => global,
            None if matches!(request, MemoryRequestV1::Status { .. }) => {
                return Ok(MemoryResponseV1 {
                    version: DTO_VERSION,
                    scope,
                    payload: MemoryPayloadV1::Status(MemoryStoreStatusV1 {
                        available: false,
                        schema_version: 0,
                    }),
                });
            }
            None => {
                return Err(MemoryServiceErrorV1::new(
                    MemoryServiceErrorCodeV1::StoreUnavailable,
                    "global memory store is unavailable",
                ));
            }
        },
    };
    let payload = runtime
        .block_on(async {
            match request {
                MemoryRequestV1::Status { .. } => {
                    Ok(MemoryPayloadV1::Status(MemoryStoreStatusV1 {
                        available: true,
                        schema_version: omegon_memory::sqlite::MEMORY_SCHEMA_VERSION,
                    }))
                }
                MemoryRequestV1::Stats { mind, .. } => {
                    backend.stats(&mind).await.map(MemoryPayloadV1::Stats)
                }
                MemoryRequestV1::GetFact { id, .. } => backend
                    .get_fact(&id)
                    .await
                    .map(|fact| MemoryPayloadV1::Fact(Box::new(fact))),
                MemoryRequestV1::ListFacts { mind, filter, .. } => backend
                    .list_facts(&mind, filter)
                    .await
                    .map(MemoryPayloadV1::Facts),
                MemoryRequestV1::FtsSearch {
                    mind, query, limit, ..
                } => backend
                    .fts_search(&mind, &query, limit)
                    .await
                    .map(MemoryPayloadV1::ScoredFacts),
                MemoryRequestV1::VectorSearch {
                    mind,
                    vector,
                    limit,
                    min_similarity,
                    ..
                } => backend
                    .vector_search(&mind, &vector, limit, min_similarity)
                    .await
                    .map(MemoryPayloadV1::ScoredFacts),
                MemoryRequestV1::EmbeddingMetadata { mind, .. } => backend
                    .embedding_metadata(&mind)
                    .await
                    .map(MemoryPayloadV1::EmbeddingMetadata),
                MemoryRequestV1::GetEdges { mind, fact_id, .. } => backend
                    .get_edges(&mind, &fact_id)
                    .await
                    .map(MemoryPayloadV1::Edges),
                MemoryRequestV1::ListEpisodes { mind, limit, .. } => backend
                    .list_episodes(&mind, limit)
                    .await
                    .map(MemoryPayloadV1::Episodes),
                MemoryRequestV1::SearchEpisodes {
                    mind, query, limit, ..
                } => backend
                    .search_episodes(&mind, &query, limit)
                    .await
                    .map(MemoryPayloadV1::Episodes),
                MemoryRequestV1::ApplyMutation {
                    operation_id,
                    mutation,
                    ..
                } => backend
                    .apply_mutation(&operation_id, mutation)
                    .await
                    .map(MemoryPayloadV1::Mutation),
                #[cfg(test)]
                MemoryRequestV1::TestBlock {
                    started, release, ..
                } => {
                    let _ = started.send(());
                    let (released, changed) = &*release;
                    let mut released = released.lock().expect("test release lock");
                    while !*released {
                        released = changed.wait(released).expect("test release wait");
                    }
                    Ok(MemoryPayloadV1::Status(MemoryStoreStatusV1 {
                        available: true,
                        schema_version: omegon_memory::sqlite::MEMORY_SCHEMA_VERSION,
                    }))
                }
                #[cfg(test)]
                MemoryRequestV1::TestRecord { executions, .. } => {
                    executions.fetch_add(1, Ordering::AcqRel);
                    Ok(MemoryPayloadV1::Status(MemoryStoreStatusV1 {
                        available: true,
                        schema_version: omegon_memory::sqlite::MEMORY_SCHEMA_VERSION,
                    }))
                }
                #[cfg(test)]
                MemoryRequestV1::TestPanic { .. } => panic!("test memory worker panic"),
                #[cfg(test)]
                MemoryRequestV1::TestAtomicMutation {
                    started,
                    release,
                    operation_id,
                    mutation,
                    ..
                } => {
                    let _ = started.send(());
                    {
                        let (released, changed) = &*release;
                        let mut released = released.lock().expect("test release lock");
                        while !*released {
                            released = changed.wait(released).expect("test release wait");
                        }
                    }
                    backend
                        .apply_mutation(&operation_id, mutation)
                        .await
                        .map(MemoryPayloadV1::Mutation)
                }
            }
        })
        .map_err(MemoryServiceErrorV1::from_memory)?;
    Ok(MemoryResponseV1 {
        version: DTO_VERSION,
        scope,
        payload,
    })
}

fn validate_request(request: &MemoryRequestV1) -> Result<(), MemoryServiceErrorV1> {
    let invalid =
        |message| MemoryServiceErrorV1::new(MemoryServiceErrorCodeV1::InvalidRequest, message);
    let limit = match request {
        MemoryRequestV1::FtsSearch { limit, .. }
        | MemoryRequestV1::VectorSearch { limit, .. }
        | MemoryRequestV1::ListEpisodes { limit, .. }
        | MemoryRequestV1::SearchEpisodes { limit, .. } => Some(*limit),
        _ => None,
    };
    if limit.is_some_and(|limit| limit > MAX_RESULT_LIMIT) {
        return Err(invalid(format!(
            "memory result limit exceeds {MAX_RESULT_LIMIT}"
        )));
    }
    if let MemoryRequestV1::VectorSearch {
        vector,
        min_similarity,
        ..
    } = request
    {
        if vector.len() > MAX_VECTOR_DIMENSIONS {
            return Err(invalid(format!(
                "query vector exceeds {MAX_VECTOR_DIMENSIONS} dimensions"
            )));
        }
        if vector.iter().any(|value| !value.is_finite()) || !min_similarity.is_finite() {
            return Err(invalid("query vector and similarity must be finite".into()));
        }
    }
    Ok(())
}

fn request_scope(request: &MemoryRequestV1) -> MemoryScopeV1 {
    match request {
        MemoryRequestV1::Status { scope, .. }
        | MemoryRequestV1::Stats { scope, .. }
        | MemoryRequestV1::GetFact { scope, .. }
        | MemoryRequestV1::ListFacts { scope, .. }
        | MemoryRequestV1::FtsSearch { scope, .. }
        | MemoryRequestV1::VectorSearch { scope, .. }
        | MemoryRequestV1::EmbeddingMetadata { scope, .. }
        | MemoryRequestV1::GetEdges { scope, .. }
        | MemoryRequestV1::ListEpisodes { scope, .. }
        | MemoryRequestV1::SearchEpisodes { scope, .. }
        | MemoryRequestV1::ApplyMutation { scope, .. } => *scope,
        #[cfg(test)]
        MemoryRequestV1::TestBlock { .. }
        | MemoryRequestV1::TestRecord { .. }
        | MemoryRequestV1::TestPanic { .. }
        | MemoryRequestV1::TestAtomicMutation { .. } => MemoryScopeV1::Project,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omegon_memory::{
        DecayProfileName, FactPrecondition, MemoryMutationEffect, Section, StoreFact,
    };
    use std::sync::atomic::AtomicUsize;

    const MIND: &str = omegon_memory::sqlite::PRIMENSUS_MIND;

    fn store(content: &str) -> MemoryMutation {
        MemoryMutation::StoreFact {
            request: StoreFact {
                mind: MIND.into(),
                content: content.into(),
                section: Section::Architecture,
                decay_profile: DecayProfileName::Standard,
                source: None,
            },
        }
    }

    fn request(
        scope: MemoryScopeV1,
        operation_id: &str,
        mutation: MemoryMutation,
    ) -> MemoryRequestV1 {
        MemoryRequestV1::ApplyMutation {
            scope,
            operation_id: operation_id.into(),
            mutation,
            cancellation: CancellationToken::new(),
        }
    }

    async fn managed_service(
        project: PathBuf,
        global: Option<PathBuf>,
    ) -> (crate::bus::EventBus, ManagedServiceHandle<MemoryService>) {
        let mut bus = crate::bus::EventBus::new();
        bus.register(Box::new(MemoryDeclarationFeature));
        bus.stage_managed_generation("memory", start_candidate(project, global).await.unwrap())
            .unwrap();
        bus.try_finalize_managed().await.unwrap();
        let handle = bus
            .managed_service::<MemoryService>(&memory_capability_id(), &memory_interface_id())
            .unwrap()
            .unwrap();
        (bus, handle)
    }

    fn mutation(response: MemoryResponseV1) -> MemoryMutationOutcome {
        let MemoryPayloadV1::Mutation(outcome) = response.payload else {
            panic!("expected mutation response");
        };
        outcome
    }

    #[tokio::test]
    async fn project_and_global_stores_are_explicit_and_separate() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("project.db");
        let global = dir.path().join("global.db");
        drop(SqliteBackend::open(&global).unwrap());
        let (mut bus, handle) = managed_service(project, Some(global)).await;

        handle
            .invoke(request(
                MemoryScopeV1::Project,
                "project",
                store("project fact"),
            ))
            .await
            .unwrap();
        handle
            .invoke(request(
                MemoryScopeV1::Global,
                "global",
                store("global fact"),
            ))
            .await
            .unwrap();

        for (scope, expected) in [
            (MemoryScopeV1::Project, "project fact"),
            (MemoryScopeV1::Global, "global fact"),
        ] {
            let response = handle
                .invoke(MemoryRequestV1::ListFacts {
                    scope,
                    mind: MIND.into(),
                    filter: FactFilter::default(),
                    cancellation: CancellationToken::new(),
                })
                .await
                .unwrap();
            assert!(matches!(response.payload, MemoryPayloadV1::Facts(facts)
                if facts.len() == 1 && facts[0].content == expected));
        }
        assert!(
            bus.shutdown_managed_services()
                .await
                .all_resources_settled()
        );
    }

    #[tokio::test]
    async fn absent_global_store_is_typed_and_never_falls_back_to_project() {
        let dir = tempfile::tempdir().unwrap();
        let (mut bus, handle) = managed_service(dir.path().join("project.db"), None).await;
        handle
            .invoke(request(
                MemoryScopeV1::Project,
                "project",
                store("only project"),
            ))
            .await
            .unwrap();
        let error = handle
            .invoke(MemoryRequestV1::Stats {
                scope: MemoryScopeV1::Global,
                mind: MIND.into(),
                cancellation: CancellationToken::new(),
            })
            .await
            .unwrap_err();
        assert!(matches!(error, ManagedServiceCallError::Operation(error)
            if error.code == MemoryServiceErrorCodeV1::StoreUnavailable));
        let status = handle
            .invoke(MemoryRequestV1::Status {
                scope: MemoryScopeV1::Global,
                cancellation: CancellationToken::new(),
            })
            .await
            .unwrap();
        assert!(matches!(status.payload, MemoryPayloadV1::Status(status)
            if !status.available && status.schema_version == 0));
        assert!(
            bus.shutdown_managed_services()
                .await
                .all_resources_settled()
        );
    }

    #[tokio::test]
    async fn a_missing_global_path_degrades_locally_without_creation() {
        let dir = tempfile::tempdir().unwrap();
        let global = dir.path().join("missing-global.db");
        let (mut bus, handle) =
            managed_service(dir.path().join("project.db"), Some(global.clone())).await;
        let status = handle
            .invoke(MemoryRequestV1::Status {
                scope: MemoryScopeV1::Global,
                cancellation: CancellationToken::new(),
            })
            .await
            .unwrap();
        assert!(matches!(status.payload, MemoryPayloadV1::Status(status)
            if !status.available && status.schema_version == 0));
        assert!(!global.exists());
        assert!(
            bus.shutdown_managed_services()
                .await
                .all_resources_settled()
        );
    }

    #[tokio::test]
    async fn an_uninitialized_global_file_degrades_locally_without_fabrication() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("project.db");
        let global = dir.path().join("global.db");
        std::fs::File::create(&global).unwrap();
        let (mut bus, handle) = managed_service(project, Some(global.clone())).await;
        let status = handle
            .invoke(MemoryRequestV1::Status {
                scope: MemoryScopeV1::Global,
                cancellation: CancellationToken::new(),
            })
            .await
            .unwrap();
        assert!(matches!(status.payload, MemoryPayloadV1::Status(status)
            if !status.available && status.schema_version == 0));
        assert_eq!(std::fs::metadata(global).unwrap().len(), 0);
        assert!(
            bus.shutdown_managed_services()
                .await
                .all_resources_settled()
        );
    }

    #[test]
    fn version_one_dtos_serialize_and_every_backend_error_maps_typed() {
        let encoded = serde_json::to_value(MemoryRequestV1::VectorSearch {
            scope: MemoryScopeV1::Global,
            mind: MIND.into(),
            vector: vec![1.0, 0.0],
            limit: 3,
            min_similarity: 0.2,
            cancellation: CancellationToken::new(),
        })
        .unwrap();
        assert_eq!(encoded["kind"], "vector_search");
        assert_eq!(encoded["scope"], "global");
        let decoded: MemoryRequestV1 = serde_json::from_value(encoded).unwrap();
        assert!(
            matches!(decoded, MemoryRequestV1::VectorSearch { vector, .. } if vector == vec![1.0, 0.0])
        );

        let errors = [
            (
                MemoryError::FactNotFound("x".into()),
                MemoryServiceErrorCodeV1::FactNotFound,
            ),
            (
                MemoryError::EmbeddingDimensionMismatch {
                    expected: 2,
                    got: 3,
                    stored_model: "model".into(),
                },
                MemoryServiceErrorCodeV1::EmbeddingDimensionMismatch,
            ),
            (
                MemoryError::NoEmbeddings,
                MemoryServiceErrorCodeV1::NoEmbeddings,
            ),
            (
                MemoryError::OperationConflict("op".into()),
                MemoryServiceErrorCodeV1::OperationConflict,
            ),
            (
                MemoryError::FactVersionConflict {
                    id: "x".into(),
                    expected: 1,
                    actual: 2,
                },
                MemoryServiceErrorCodeV1::FactVersionConflict,
            ),
            (
                MemoryError::InvalidMutation("bad".into()),
                MemoryServiceErrorCodeV1::InvalidMutation,
            ),
            (
                MemoryError::Storage(anyhow::anyhow!("disk")),
                MemoryServiceErrorCodeV1::Storage,
            ),
        ];
        for (error, expected) in errors {
            let mapped = MemoryServiceErrorV1::from_memory(error);
            assert_eq!(mapped.version, DTO_VERSION);
            assert_eq!(mapped.code, expected);
            serde_json::to_string(&mapped).unwrap();
        }
    }

    #[tokio::test]
    async fn serial_queue_skips_a_cancelled_waiter_before_execution() {
        let dir = tempfile::tempdir().unwrap();
        let (mut bus, handle) = managed_service(dir.path().join("project.db"), None).await;
        let release = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
        let (started, started_rx) = std::sync::mpsc::sync_channel(1);
        let first = tokio::spawn({
            let handle = handle.clone();
            let release = Arc::clone(&release);
            async move {
                handle
                    .invoke(MemoryRequestV1::TestBlock {
                        started,
                        release,
                        cancellation: CancellationToken::new(),
                    })
                    .await
            }
        });
        tokio::task::spawn_blocking(move || started_rx.recv_timeout(Duration::from_secs(2)))
            .await
            .unwrap()
            .unwrap();

        let cancellation = CancellationToken::new();
        let executions = Arc::new(AtomicUsize::new(0));
        let second = tokio::spawn({
            let handle = handle.clone();
            let cancellation = cancellation.clone();
            let executions = Arc::clone(&executions);
            async move {
                handle
                    .invoke(MemoryRequestV1::TestRecord {
                        executions,
                        cancellation,
                    })
                    .await
            }
        });
        tokio::task::yield_now().await;
        cancellation.cancel();
        assert!(
            matches!(second.await.unwrap(), Err(ManagedServiceCallError::Operation(error))
            if error.code == MemoryServiceErrorCodeV1::Cancelled)
        );
        let (released, changed) = &*release;
        *released.lock().unwrap() = true;
        changed.notify_all();
        first.await.unwrap().unwrap();
        handle
            .invoke(MemoryRequestV1::Status {
                scope: MemoryScopeV1::Project,
                cancellation: CancellationToken::new(),
            })
            .await
            .unwrap();
        assert_eq!(executions.load(Ordering::Acquire), 0);
        assert!(
            bus.shutdown_managed_services()
                .await
                .all_resources_settled()
        );
    }

    #[tokio::test]
    async fn invalid_query_bounds_are_rejected_before_backend_execution() {
        let dir = tempfile::tempdir().unwrap();
        let (mut bus, handle) = managed_service(dir.path().join("project.db"), None).await;
        let error = handle
            .invoke(MemoryRequestV1::FtsSearch {
                scope: MemoryScopeV1::Project,
                mind: MIND.into(),
                query: "bounded".into(),
                limit: MAX_RESULT_LIMIT + 1,
                cancellation: CancellationToken::new(),
            })
            .await
            .unwrap_err();
        assert!(matches!(error, ManagedServiceCallError::Operation(error)
            if error.code == MemoryServiceErrorCodeV1::InvalidRequest));
        assert!(
            bus.shutdown_managed_services()
                .await
                .all_resources_settled()
        );
    }

    #[tokio::test]
    async fn cancelled_active_mutation_settles_and_replays_by_operation_id() {
        let dir = tempfile::tempdir().unwrap();
        let (mut bus, handle) = managed_service(dir.path().join("project.db"), None).await;
        let release = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
        let (started, started_rx) = std::sync::mpsc::sync_channel(1);
        let cancellation = CancellationToken::new();
        let durable_mutation = store("cancelled caller durable fact");
        let active = tokio::spawn({
            let handle = handle.clone();
            let release = Arc::clone(&release);
            let cancellation = cancellation.clone();
            let mutation = durable_mutation.clone();
            async move {
                handle
                    .invoke(MemoryRequestV1::TestAtomicMutation {
                        started,
                        release,
                        operation_id: "active-cancel".into(),
                        mutation,
                        cancellation,
                    })
                    .await
            }
        });
        tokio::task::spawn_blocking(move || started_rx.recv_timeout(Duration::from_secs(2)))
            .await
            .unwrap()
            .unwrap();
        cancellation.cancel();
        assert!(
            matches!(active.await.unwrap(), Err(ManagedServiceCallError::Operation(error))
            if error.code == MemoryServiceErrorCodeV1::Cancelled)
        );

        let (released, changed) = &*release;
        *released.lock().unwrap() = true;
        changed.notify_all();
        let replay = mutation(
            handle
                .invoke(request(
                    MemoryScopeV1::Project,
                    "active-cancel",
                    durable_mutation,
                ))
                .await
                .unwrap(),
        );
        assert!(replay.replayed);
        assert!(
            bus.shutdown_managed_services()
                .await
                .all_resources_settled()
        );
    }

    #[tokio::test]
    async fn mutations_replay_reject_payload_conflicts_and_enforce_fact_versions() {
        let dir = tempfile::tempdir().unwrap();
        let (mut bus, handle) = managed_service(dir.path().join("project.db"), None).await;
        let store_mutation = store("versioned fact");
        let first = mutation(
            handle
                .invoke(request(
                    MemoryScopeV1::Project,
                    "stable-op",
                    store_mutation.clone(),
                ))
                .await
                .unwrap(),
        );
        let replay = mutation(
            handle
                .invoke(request(MemoryScopeV1::Project, "stable-op", store_mutation))
                .await
                .unwrap(),
        );
        assert!(!first.replayed);
        assert!(replay.replayed);
        let conflict = handle
            .invoke(request(
                MemoryScopeV1::Project,
                "stable-op",
                store("different payload"),
            ))
            .await
            .unwrap_err();
        assert!(matches!(conflict, ManagedServiceCallError::Operation(error)
            if error.code == MemoryServiceErrorCodeV1::OperationConflict));

        let MemoryMutationEffect::FactStored {
            fact_id, version, ..
        } = first.effect
        else {
            panic!("expected stored fact");
        };
        let stale = handle
            .invoke(request(
                MemoryScopeV1::Project,
                "stale-op",
                MemoryMutation::ReinforceFact {
                    fact: FactPrecondition {
                        id: fact_id,
                        expected_version: version + 1,
                    },
                },
            ))
            .await
            .unwrap_err();
        assert!(matches!(stale, ManagedServiceCallError::Operation(error)
            if error.code == MemoryServiceErrorCodeV1::FactVersionConflict));
        assert!(
            bus.shutdown_managed_services()
                .await
                .all_resources_settled()
        );
    }

    #[tokio::test]
    async fn rejected_candidate_closes_both_databases_for_reopen_and_delete() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("project.db");
        let global = dir.path().join("global.db");
        drop(SqliteBackend::open(&global).unwrap());
        let mut bus = crate::bus::EventBus::new();
        bus.register(Box::new(MemoryDeclarationFeature));
        bus.register(Box::new(MemoryDeclarationFeature));
        bus.stage_managed_generation(
            "memory",
            start_candidate(project.clone(), Some(global.clone()))
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(bus.try_finalize_managed().await.is_err());
        drop(SqliteBackend::open(&project).unwrap());
        drop(SqliteBackend::open(&global).unwrap());
        for path in [&project, &global] {
            for suffix in ["-wal", "-shm"] {
                let sidecar = PathBuf::from(format!("{}{suffix}", path.display()));
                if sidecar.exists() {
                    std::fs::remove_file(sidecar).unwrap();
                }
            }
            std::fs::remove_file(path).unwrap();
        }
    }

    #[tokio::test]
    async fn shutdown_closes_stores_joins_worker_and_retires_captured_handle() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("project.db");
        let (mut bus, handle) = managed_service(project.clone(), None).await;
        let report = bus.shutdown_managed_services().await;
        assert!(report.all_resources_settled(), "{report:?}");
        assert!(matches!(
            handle
                .invoke(MemoryRequestV1::Status {
                    scope: MemoryScopeV1::Project,
                    cancellation: CancellationToken::new(),
                })
                .await,
            Err(ManagedServiceCallError::GenerationRetired)
        ));
        drop(SqliteBackend::open(&project).unwrap());
        std::fs::remove_file(&project).unwrap();
    }

    #[tokio::test]
    async fn worker_panic_is_reported_as_strict_cleanup_failure() {
        let dir = tempfile::tempdir().unwrap();
        let (mut bus, handle) = managed_service(dir.path().join("project.db"), None).await;
        assert!(
            handle
                .invoke(MemoryRequestV1::TestPanic {
                    cancellation: CancellationToken::new(),
                })
                .await
                .is_err()
        );
        let report = bus.shutdown_managed_services().await;
        assert!(!report.all_resources_settled(), "{report:?}");
    }

    #[tokio::test]
    async fn exact_generation_transfer_keeps_the_original_handle_callable() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("project.db");
        let (commands, receiver) = mpsc::channel(QUEUE_CAPACITY);
        let state = Arc::new(WorkerState {
            stopping: AtomicBool::new(false),
            stores_closed: AtomicBool::new(false),
            worker_joined: AtomicBool::new(false),
            worker_failed: AtomicBool::new(false),
            changed: Notify::new(),
            join: Mutex::new(None),
        });
        let (startup, started) = std::sync::mpsc::sync_channel(1);
        let worker_state = Arc::clone(&state);
        let join =
            std::thread::spawn(move || run_worker(project, None, receiver, worker_state, startup));
        *state.join.lock().unwrap() = Some(join);
        tokio::task::spawn_blocking(move || started.recv())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let service = Arc::new(MemoryService {
            commands: commands.clone(),
        });
        let worker = Arc::new(WorkerController {
            state: Arc::clone(&state),
            commands: commands.clone(),
        });
        let writer = Arc::new(WriterController { state, commands });
        let candidate = || {
            let writer_id = RuntimeContributionResourceId::new(WRITER_RESOURCE).unwrap();
            let resources = vec![
                ManagedResourceRegistration::new(
                    writer_id.clone(),
                    RuntimeOwnedResourceKind::DurableWriter,
                    RuntimeCleanupAssurance::Strict,
                    Vec::new(),
                    writer.clone(),
                ),
                ManagedResourceRegistration::new(
                    RuntimeContributionResourceId::new(WORKER_RESOURCE).unwrap(),
                    RuntimeOwnedResourceKind::Task,
                    RuntimeCleanupAssurance::Strict,
                    vec![writer_id],
                    worker.clone(),
                ),
            ];
            let mut candidate = ManagedGenerationCandidate::new(
                RuntimeCompositionGenerationId::new("composition:memory-transfer").unwrap(),
                omegon_traits::RuntimeContributionId::new("feature:memory").unwrap(),
                RuntimeContributionGenerationId::new(MEMORY_GENERATION).unwrap(),
                Duration::from_secs(30),
                Duration::from_secs(5),
                resources,
            )
            .unwrap();
            candidate
                .add_service(
                    memory_capability_id(),
                    memory_interface_id(),
                    service.clone(),
                )
                .unwrap();
            candidate
        };
        let mut bus = crate::bus::EventBus::new();
        bus.register(Box::new(MemoryDeclarationFeature));
        bus.stage_managed_generation("memory", candidate()).unwrap();
        bus.try_finalize_managed().await.unwrap();
        let handle = bus
            .managed_service::<MemoryService>(&memory_capability_id(), &memory_interface_id())
            .unwrap()
            .unwrap();
        bus.stage_managed_generation("memory", candidate()).unwrap();
        bus.try_finalize_managed().await.unwrap();
        handle
            .invoke(MemoryRequestV1::Status {
                scope: MemoryScopeV1::Project,
                cancellation: CancellationToken::new(),
            })
            .await
            .unwrap();
        assert!(
            bus.shutdown_managed_services()
                .await
                .all_resources_settled()
        );
    }

    #[tokio::test]
    async fn typed_binding_absence_preserves_the_optional_declaration() {
        let mut bus = crate::bus::EventBus::new();
        bus.register(Box::new(MemoryDeclarationFeature));
        bus.try_finalize_managed().await.unwrap();
        let binding = MemoryBinding::default();
        binding.capture(&bus).unwrap();
        assert!(binding.handle().is_none());
        let error = binding
            .invoke(MemoryRequestV1::Status {
                scope: MemoryScopeV1::Project,
                cancellation: CancellationToken::new(),
            })
            .await
            .unwrap_err();
        assert!(matches!(error, ManagedServiceCallError::Operation(error)
            if error.code == MemoryServiceErrorCodeV1::Unavailable));
    }
}
