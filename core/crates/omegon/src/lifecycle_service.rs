//! Generation-owned lifecycle repository reads.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use omegon_opsx::{JsonFileStore, Lifecycle as OpsxLifecycle, OpenSpecRepository};
use omegon_traits::{
    ManagedCallContext, ManagedResourceController, ManagedResourceSettlementFuture,
    ManagedServiceContract, ManagedServiceFuture, RuntimeCapabilityId, RuntimeCleanupAssurance,
    RuntimeCompositionGenerationId, RuntimeContributionGenerationId, RuntimeContributionResourceId,
    RuntimeOwnedResourceKind, RuntimeServiceInterfaceId,
};
use sha2::{Digest, Sha256};
use tokio::sync::{Notify, mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::lifecycle::context::LifecycleContextProvider;
use crate::lifecycle::query::{BlockedNode, FrontierNode, ReadyNode};
use crate::lifecycle::read_model::{
    DesignNodeObservation, DesignTreeSnapshot, LifecycleSnapshot, SnapshotOptions,
};
use crate::managed_service_bus::{ManagedGenerationCandidate, ManagedResourceRegistration};
use crate::service_generation::ManagedServiceHandle;

pub(crate) const LIFECYCLE_CAPABILITY: &str = "service:lifecycle";
pub(crate) const LIFECYCLE_INTERFACE: &str = "interface:omegon-lifecycle-v1";
pub(crate) const LIFECYCLE_GENERATION: &str = "contribution:lifecycle-managed-v1";
const WORKER_RESOURCE: &str = "resource:lifecycle-worker";
const WRITER_RESOURCE: &str = "resource:lifecycle-writer";
const QUEUE_CAPACITY: usize = 16;
const DTO_VERSION: u32 = 1;

pub(crate) fn lifecycle_capability_id() -> RuntimeCapabilityId {
    RuntimeCapabilityId::new(LIFECYCLE_CAPABILITY).expect("static capability id is valid")
}

pub(crate) fn lifecycle_interface_id() -> RuntimeServiceInterfaceId {
    RuntimeServiceInterfaceId::new(LIFECYCLE_INTERFACE).expect("static interface id is valid")
}

#[derive(Clone, Default)]
pub(crate) struct LifecycleBinding {
    handle: Arc<OnceLock<Option<ManagedServiceHandle<LifecycleService>>>>,
}

impl LifecycleBinding {
    pub(crate) fn capture(&self, bus: &crate::bus::EventBus) -> anyhow::Result<()> {
        let handle = bus.managed_service::<LifecycleService>(
            &lifecycle_capability_id(),
            &lifecycle_interface_id(),
        )?;
        self.handle
            .set(handle)
            .map_err(|_| anyhow::anyhow!("lifecycle managed handle was already captured"))
    }

    pub(crate) fn handle(&self) -> Option<ManagedServiceHandle<LifecycleService>> {
        self.handle.get().and_then(Clone::clone)
    }

    pub(crate) fn available(&self) -> bool {
        self.handle().is_some()
    }

    pub(crate) async fn invoke(
        &self,
        request: LifecycleRequestV1,
    ) -> Result<LifecycleResponseV1, omegon_traits::ManagedServiceCallError<LifecycleServiceErrorV1>>
    {
        let Some(handle) = self.handle() else {
            return Err(omegon_traits::ManagedServiceCallError::Operation(
                LifecycleServiceErrorV1::new(
                    LifecycleServiceErrorCodeV1::Unavailable,
                    "managed lifecycle service is unavailable",
                ),
            ));
        };
        handle.invoke(request).await
    }
}

pub(crate) async fn observe_repository_once(
    repo_path: PathBuf,
) -> anyhow::Result<Option<Arc<crate::runtime_state::LifecycleRepositoryObservation>>> {
    let response = invoke_repository_once(
        repo_path,
        LifecycleRequestV1::RepositorySnapshot {
            options: SnapshotOptions::default(),
            cancellation: tokio_util::sync::CancellationToken::new(),
        },
    )
    .await?;
    match response.payload {
        LifecyclePayloadV1::RepositorySnapshot {
            design,
            lifecycle,
            sections,
        } => Ok(Some(Arc::new(
            crate::runtime_state::LifecycleRepositoryObservation {
                revision: response.revision,
                design,
                lifecycle,
                sections,
            },
        ))),
        _ => anyhow::bail!("managed lifecycle returned an unexpected repository payload"),
    }
}

pub(crate) async fn invoke_repository_once(
    repo_path: PathBuf,
    request: LifecycleRequestV1,
) -> anyhow::Result<LifecycleResponseV1> {
    validate_repository_roots(&repo_path)?;
    let mut bus = crate::bus::EventBus::new();
    let binding = LifecycleBinding::default();
    let host = crate::runtime_state::LifecycleHostHandle::new(binding.clone());
    bus.register(Box::new(
        crate::features::lifecycle::LifecycleFeature::managed(
            &repo_path,
            binding.clone(),
            host.clone(),
        ),
    ));
    let candidate = start_candidate(repo_path).await?;
    bus.stage_managed_generation("lifecycle", candidate)?;
    if let Err(error) = bus.try_finalize_managed().await {
        let _ = bus.shutdown_managed_services().await;
        return Err(error);
    }

    let result = async {
        binding.capture(&bus)?;
        binding
            .invoke(request)
            .await
            .map_err(|error| anyhow::anyhow!("managed lifecycle invocation failed: {error:?}"))
    }
    .await;
    let shutdown = bus.shutdown_managed_services().await;
    if !shutdown.all_resources_settled() {
        return Err(anyhow::anyhow!(
            "managed lifecycle observation cleanup did not settle: {shutdown:?}"
        ));
    }
    result
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct LifecycleRepositoryRevisionV1 {
    pub version: u32,
    pub design_root: String,
    pub openspec_root: String,
    pub ledger_path: String,
    pub ledger_identity: String,
    pub ledger_revision: u64,
    pub artifact_digest: String,
    pub transaction_digest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DesignIssueTypeV1 {
    Epic,
    Feature,
    Task,
    Bug,
    Chore,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct DesignFileScopeV1 {
    pub path: String,
    pub description: String,
    pub action: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum DesignMutationV1 {
    Create {
        id: String,
        title: String,
        parent: Option<String>,
        status: Option<omegon_opsx::NodeState>,
        tags: Vec<String>,
        overview: String,
    },
    SetState {
        id: String,
        state: omegon_opsx::NodeState,
        archive_reason: Option<String>,
        superseded_by: Option<String>,
        archived_at: Option<String>,
    },
    AddQuestion {
        id: String,
        question: String,
    },
    RemoveQuestion {
        id: String,
        question: String,
    },
    AddResearch {
        id: String,
        heading: String,
        content: String,
    },
    AddDecision {
        id: String,
        title: String,
        status: String,
        rationale: String,
    },
    AddDependency {
        id: String,
        target_id: String,
    },
    RemoveDependency {
        id: String,
        target_id: String,
    },
    AddRelated {
        id: String,
        target_id: String,
    },
    RemoveRelated {
        id: String,
        target_id: String,
    },
    AddImplementationNotes {
        id: String,
        file_scope: Vec<DesignFileScopeV1>,
        constraints: Vec<String>,
    },
    SetPriority {
        id: String,
        priority: u8,
    },
    SetIssueType {
        id: String,
        issue_type: DesignIssueTypeV1,
    },
    BranchQuestion {
        parent_id: String,
        question: String,
        child_id: String,
        child_title: String,
    },
    ImplementOpenSpec {
        id: String,
    },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum OpenSpecMutationV1 {
    Propose {
        name: String,
        title: String,
        intent: String,
        bound_node: Option<String>,
    },
    AddSpec {
        change: String,
        domain: String,
        content: String,
    },
    ReconcileTasks {
        change: String,
    },
    SetTaskStatus {
        change: String,
        group: String,
        task_id: String,
        done: bool,
    },
    RegisterTestFile {
        change: String,
        path: String,
    },
    Transition {
        change: String,
        state: omegon_opsx::ChangeState,
    },
    Archive {
        change: String,
    },
    Abandon {
        change: String,
    },
    Reopen {
        change: String,
    },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct LifecycleMutationReceiptV1 {
    pub operation_id: String,
    pub replayed: bool,
    pub committed_revision: LifecycleRepositoryRevisionV1,
    pub effects: Vec<String>,
    #[serde(default)]
    pub outcome: LifecycleMutationOutcomeV1,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum LifecycleMutationOutcomeV1 {
    #[default]
    None,
    DesignCreated {
        path: String,
    },
    DesignImplemented {
        node_id: String,
        change: String,
        path: String,
    },
    OpenSpecProposed {
        path: String,
    },
    OpenSpecSpecAdded {
        path: String,
    },
    OpenSpecTasksReconciled {
        total_tasks: usize,
        done_tasks: usize,
    },
    OpenSpecTaskStatusChanged {
        change: String,
        group: String,
        task_id: String,
        path: String,
        line: usize,
        previous_done: bool,
        new_done: bool,
        description: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LifecycleServiceErrorCodeV1 {
    Cancelled,
    Unavailable,
    StaleRevision,
    OperationConflict,
    Validation,
    RecoveryRequired,
    Persistence,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct LifecycleServiceErrorV1 {
    pub version: u32,
    pub code: LifecycleServiceErrorCodeV1,
    pub message: String,
}

impl LifecycleServiceErrorV1 {
    fn new(code: LifecycleServiceErrorCodeV1, message: impl Into<String>) -> Self {
        Self {
            version: DTO_VERSION,
            code,
            message: message.into(),
        }
    }

    fn classify(message: impl Into<String>) -> Self {
        let message = message.into();
        let code = if message.contains("cancelled") {
            LifecycleServiceErrorCodeV1::Cancelled
        } else if message.contains("unavailable") || message.contains("dropped its response") {
            LifecycleServiceErrorCodeV1::Unavailable
        } else if message.contains("stale lifecycle repository revision") {
            LifecycleServiceErrorCodeV1::StaleRevision
        } else if message.contains("operation id conflicts") {
            LifecycleServiceErrorCodeV1::OperationConflict
        } else if message.contains("recovery required")
            || message.contains("quarantined design transaction")
        {
            LifecycleServiceErrorCodeV1::RecoveryRequired
        } else if message.contains("invalid")
            || message.contains("not found")
            || message.contains("already exists")
            || message.contains("must be")
            || message.contains("blocked by unknown content")
            || message.contains("conflicting")
            || message.contains("malformed")
            || message.contains("symlink")
        {
            LifecycleServiceErrorCodeV1::Validation
        } else {
            LifecycleServiceErrorCodeV1::Internal
        };
        Self::new(code, message)
    }

    fn cancelled() -> Self {
        Self::new(
            LifecycleServiceErrorCodeV1::Cancelled,
            "lifecycle request cancelled",
        )
    }

    fn from_transaction(error: &crate::lifecycle_transaction::TransactionError) -> Self {
        use crate::lifecycle_transaction::TransactionErrorCode as Code;
        let code = match error.code {
            Code::Cancelled => LifecycleServiceErrorCodeV1::Cancelled,
            Code::StaleRevision => LifecycleServiceErrorCodeV1::StaleRevision,
            Code::OperationConflict => LifecycleServiceErrorCodeV1::OperationConflict,
            Code::Validation => LifecycleServiceErrorCodeV1::Validation,
            Code::RecoveryRequired => LifecycleServiceErrorCodeV1::RecoveryRequired,
            Code::Persistence => LifecycleServiceErrorCodeV1::Persistence,
        };
        Self::new(code, error.message.clone())
    }
}

impl std::fmt::Display for LifecycleServiceErrorV1 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for LifecycleServiceErrorV1 {}

#[derive(Debug, Clone)]
pub(crate) enum LifecycleReadQueryV1 {
    Ready,
    Blocked,
    Frontier,
}

#[derive(Debug, Clone)]
pub(crate) enum LifecycleRequestV1 {
    RepositorySnapshot {
        options: SnapshotOptions,
        cancellation: CancellationToken,
    },
    Snapshot {
        options: SnapshotOptions,
        cancellation: CancellationToken,
    },
    DesignTree {
        cancellation: CancellationToken,
    },
    ObserveDesignNode {
        id: String,
        include_sections: bool,
        include_tree_context: bool,
        cancellation: CancellationToken,
    },
    QueryDesign {
        query: LifecycleReadQueryV1,
        cancellation: CancellationToken,
    },
    ValidateTaskStableIds {
        change: String,
        cancellation: CancellationToken,
    },
    Health {
        cancellation: CancellationToken,
    },
    Doctor {
        cancellation: CancellationToken,
    },
    RecoverRepository {
        cancellation: CancellationToken,
    },
    MutateDesign {
        operation_id: String,
        expected_revision: LifecycleRepositoryRevisionV1,
        mutation: Box<DesignMutationV1>,
        cancellation: CancellationToken,
    },
    MutateOpenSpec {
        operation_id: String,
        expected_revision: LifecycleRepositoryRevisionV1,
        mutation: Box<OpenSpecMutationV1>,
        cancellation: CancellationToken,
    },
    #[cfg(test)]
    TestBlock {
        started: std::sync::mpsc::SyncSender<()>,
        cancellation: CancellationToken,
    },
}

impl LifecycleRequestV1 {
    fn cancellation(&self) -> &CancellationToken {
        match self {
            Self::RepositorySnapshot { cancellation, .. }
            | Self::Snapshot { cancellation, .. }
            | Self::DesignTree { cancellation }
            | Self::ObserveDesignNode { cancellation, .. }
            | Self::QueryDesign { cancellation, .. }
            | Self::ValidateTaskStableIds { cancellation, .. }
            | Self::Health { cancellation }
            | Self::Doctor { cancellation }
            | Self::RecoverRepository { cancellation }
            | Self::MutateDesign { cancellation, .. }
            | Self::MutateOpenSpec { cancellation, .. } => cancellation,
            #[cfg(test)]
            Self::TestBlock { cancellation, .. } => cancellation,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LifecycleHealthV1 {
    pub selected_design_root: String,
    pub selected_openspec_root: String,
    pub design_findings: Vec<String>,
    pub artifact_findings: Vec<String>,
    pub recovery_required: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct LifecycleDoctorFindingV1 {
    pub node_id: String,
    pub title: String,
    pub kind: String,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub(crate) struct LifecycleDoctorReportV1 {
    pub findings: Vec<LifecycleDoctorFindingV1>,
    pub recovered: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) enum LifecyclePayloadV1 {
    RepositorySnapshot {
        design: DesignTreeSnapshot,
        lifecycle: LifecycleSnapshot,
        sections: std::collections::HashMap<String, crate::lifecycle::types::DocumentSections>,
    },
    Snapshot(LifecycleSnapshot),
    DesignTree(DesignTreeSnapshot),
    DesignNode(Box<Option<DesignNodeObservation>>),
    Ready(Vec<ReadyNode>),
    Blocked(Vec<BlockedNode>),
    Frontier(Vec<FrontierNode>),
    TaskStableIds(omegon_opsx::TaskStableIdValidationReport),
    Health(LifecycleHealthV1),
    Doctor(LifecycleDoctorReportV1),
    Recovery {
        recovered: Vec<String>,
    },
    DesignMutation(LifecycleMutationReceiptV1),
    OpenSpecMutation(LifecycleMutationReceiptV1),
}

#[derive(Debug, Clone)]
pub(crate) struct LifecycleResponseV1 {
    pub version: u32,
    pub revision: LifecycleRepositoryRevisionV1,
    pub payload: LifecyclePayloadV1,
}

pub(crate) struct LifecycleService {
    commands: mpsc::Sender<WorkerCommand>,
}

struct WorkerCommand {
    request: LifecycleRequestV1,
    generation_cancellation: CancellationToken,
    response: oneshot::Sender<Result<LifecycleResponseV1, LifecycleServiceErrorV1>>,
}

impl ManagedServiceContract for LifecycleService {
    type Request = LifecycleRequestV1;
    type Response = LifecycleResponseV1;
    type Error = LifecycleServiceErrorV1;

    fn execute<'a>(
        &'a self,
        request: Self::Request,
        context: ManagedCallContext,
    ) -> ManagedServiceFuture<'a, Self::Response, Self::Error> {
        Box::pin(async move {
            let caller_cancellation = request.cancellation().clone();
            if caller_cancellation.is_cancelled() || context.cancellation.is_cancelled() {
                return Err(LifecycleServiceErrorV1::cancelled());
            }
            let (response, receive) = oneshot::channel();
            let command = WorkerCommand {
                request,
                generation_cancellation: context.cancellation.clone(),
                response,
            };
            tokio::select! {
                biased;
                () = caller_cancellation.cancelled() => return Err(LifecycleServiceErrorV1::cancelled()),
                () = context.cancellation.cancelled() => return Err(LifecycleServiceErrorV1::cancelled()),
                sent = self.commands.send(command) => sent.map_err(|_| LifecycleServiceErrorV1::new(
                    LifecycleServiceErrorCodeV1::Unavailable,
                    "lifecycle worker is unavailable",
                ))?,
            }
            tokio::select! {
                biased;
                () = caller_cancellation.cancelled() => Err(LifecycleServiceErrorV1::cancelled()),
                () = context.cancellation.cancelled() => Err(LifecycleServiceErrorV1::cancelled()),
                result = receive => result.map_err(|_| LifecycleServiceErrorV1::new(
                    LifecycleServiceErrorCodeV1::Unavailable,
                    "lifecycle worker dropped its response",
                ))?,
            }
        })
    }
}

struct WorkerState {
    stopping: AtomicBool,
    writer_closed: AtomicBool,
    worker_joined: AtomicBool,
    changed: Notify,
    join: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl WorkerState {
    fn request_stop(&self) {
        self.stopping.store(true, Ordering::Release);
    }

    fn wake(commands: &mpsc::Sender<WorkerCommand>) {
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        let (response, _) = oneshot::channel();
        let _ = commands.try_send(WorkerCommand {
            request: LifecycleRequestV1::Health {
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
        WorkerState::wake(&self.commands);
    }
}

impl Drop for WriterController {
    fn drop(&mut self) {
        self.state.request_stop();
        WorkerState::wake(&self.commands);
    }
}

impl ManagedResourceController for WorkerController {
    fn request_stop(&self) {
        self.state.request_stop();
        WorkerState::wake(&self.commands);
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
                    .map_err(|_| "lifecycle worker join lock poisoned".to_string())?
                    .take();
                if let Some(join) = join {
                    let result = tokio::task::spawn_blocking(move || join.join())
                        .await
                        .map_err(|error| format!("lifecycle worker join task failed: {error}"))?;
                    state.worker_joined.store(true, Ordering::Release);
                    state.changed.notify_waiters();
                    if result.is_err() {
                        tracing::error!("lifecycle worker terminated after a panic");
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

impl ManagedResourceController for WriterController {
    fn request_stop(&self) {
        self.state.request_stop();
        WorkerState::wake(&self.commands);
    }

    fn force_stop(&self) {
        self.request_stop();
    }

    fn await_settled(&self) -> ManagedResourceSettlementFuture<'_> {
        let state = Arc::clone(&self.state);
        Box::pin(async move {
            loop {
                if state.writer_closed.load(Ordering::Acquire)
                    && state.worker_joined.load(Ordering::Acquire)
                {
                    return Ok(());
                }
                let changed = state.changed.notified();
                if state.writer_closed.load(Ordering::Acquire)
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
    repo_path: PathBuf,
) -> anyhow::Result<ManagedGenerationCandidate> {
    let roots = RepositoryRoots::resolve(&repo_path)?;
    let (commands, receiver) = mpsc::channel(QUEUE_CAPACITY);
    let state = Arc::new(WorkerState {
        stopping: AtomicBool::new(false),
        writer_closed: AtomicBool::new(false),
        worker_joined: AtomicBool::new(false),
        changed: Notify::new(),
        join: Mutex::new(None),
    });
    let (startup, started) = std::sync::mpsc::sync_channel(1);
    let worker_state = Arc::clone(&state);
    let join = std::thread::Builder::new()
        .name("omegon-lifecycle".into())
        .spawn(move || run_worker(repo_path, roots, receiver, worker_state, startup))?;
    *state
        .join
        .lock()
        .map_err(|_| anyhow::anyhow!("lifecycle worker join lock poisoned"))? = Some(join);
    let startup_result = tokio::task::spawn_blocking(move || started.recv())
        .await
        .map_err(|error| anyhow::anyhow!("lifecycle readiness task failed: {error}"))?
        .map_err(|_| anyhow::anyhow!("lifecycle worker exited before reporting readiness"))?;
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
        RuntimeCompositionGenerationId::new("composition:lifecycle-boot")
            .expect("static composition id is valid"),
        omegon_traits::RuntimeContributionId::new("feature:lifecycle")
            .expect("static contribution id is valid"),
        RuntimeContributionGenerationId::new(LIFECYCLE_GENERATION)
            .expect("static generation id is valid"),
        Duration::from_secs(30),
        Duration::from_secs(5),
        resources,
    )?;
    candidate.add_service(
        lifecycle_capability_id(),
        lifecycle_interface_id(),
        Arc::new(LifecycleService { commands }),
    )?;
    Ok(candidate)
}

#[cfg(test)]
pub(crate) async fn test_binding(
    path: PathBuf,
) -> anyhow::Result<(crate::bus::EventBus, LifecycleBinding)> {
    let mut bus = crate::bus::EventBus::new();
    bus.register(Box::new(
        crate::features::lifecycle::LifecycleFeature::try_new(&path)?,
    ));
    bus.stage_managed_generation("lifecycle", start_candidate(path).await?)?;
    bus.try_finalize_managed().await?;
    let binding = LifecycleBinding::default();
    binding.capture(&bus)?;
    Ok((bus, binding))
}

#[derive(Clone)]
pub(super) struct RepositoryRoots {
    pub(super) design: PathBuf,
    pub(super) openspec: PathBuf,
    pub(super) ledger: PathBuf,
}

impl RepositoryRoots {
    fn resolve(repo_path: &Path) -> anyhow::Result<Self> {
        validate_repository_roots(repo_path)?;
        Ok(Self {
            design: crate::paths::design_docs_dir(repo_path),
            openspec: resolve_openspec_path(repo_path)?,
            ledger: resolve_ledger_path(repo_path)?,
        })
    }
}

pub(crate) fn validate_repository_roots(repo_path: &Path) -> anyhow::Result<()> {
    let primary = repo_path.join("ai/openspec");
    let legacy = repo_path.join("openspec");
    if contains_openspec_artifacts(&primary)? && contains_openspec_artifacts(&legacy)? {
        anyhow::bail!(
            "conflicting OpenSpec authorities: both {} and {} contain lifecycle artifacts",
            primary.display(),
            legacy.display()
        );
    }
    let primary_ledger = repo_path.join("ai/lifecycle/state.json");
    let legacy_ledger = repo_path.join(".omegon/lifecycle/state.json");
    if regular_file_no_follow(&primary_ledger)? && regular_file_no_follow(&legacy_ledger)? {
        anyhow::bail!(
            "conflicting lifecycle ledger authorities: both {} and {} are populated",
            primary_ledger.display(),
            legacy_ledger.display()
        );
    }
    Ok(())
}

fn resolve_ledger_path(repo_path: &Path) -> anyhow::Result<PathBuf> {
    validate_repository_roots(repo_path)?;
    let primary = repo_path.join("ai/lifecycle/state.json");
    let legacy = repo_path.join(".omegon/lifecycle/state.json");
    Ok(if regular_file_no_follow(&primary)? {
        primary
    } else if regular_file_no_follow(&legacy)? {
        legacy
    } else {
        primary
    })
}

fn resolve_openspec_path(repo_path: &Path) -> anyhow::Result<PathBuf> {
    let primary = repo_path.join("ai/openspec");
    let legacy = repo_path.join("openspec");
    Ok(
        if contains_openspec_artifacts(&legacy)? && !contains_openspec_artifacts(&primary)? {
            legacy
        } else {
            primary
        },
    )
}

struct RepositoryWorker {
    repo_path: PathBuf,
    roots: RepositoryRoots,
    provider: Arc<Mutex<LifecycleContextProvider>>,
    opsx: Arc<Mutex<OpsxLifecycle<JsonFileStore>>>,
    openspec: OpenSpecRepository,
    design_recovery_blockers: Vec<String>,
}

impl RepositoryWorker {
    fn load(repo_path: PathBuf, roots: RepositoryRoots) -> anyhow::Result<Self> {
        let provider = Arc::new(Mutex::new(LifecycleContextProvider::new(&repo_path)));
        let opsx = OpsxLifecycle::load(JsonFileStore::from_path(&roots.ledger))
            .map_err(|error| anyhow::anyhow!("lifecycle state unavailable: {error}"))?;
        let mut worker = Self {
            openspec: OpenSpecRepository::from_openspec_root(&roots.openspec),
            repo_path,
            roots,
            provider,
            opsx: Arc::new(Mutex::new(opsx)),
            design_recovery_blockers: Vec::new(),
        };
        let _lock = crate::lifecycle_transaction::lock_repository(&worker.repo_path)?;
        let ledger_store = JsonFileStore::from_path(&worker.roots.ledger);
        let _ledger_transaction = ledger_store.lock_transaction()?;
        let repo = worker.repo_path.clone();
        let roots = worker.roots.clone();
        worker.design_recovery_blockers =
            crate::lifecycle_transaction::recover_pending(&repo, &roots, || {
                worker.reload()?;
                worker.revision(&|| false)
            });
        let openspec_blockers =
            crate::lifecycle_openspec_transaction::recover_pending(&repo, &roots, || {
                worker.reload()?;
                worker.revision(&|| false)
            });
        worker
            .design_recovery_blockers
            .extend(openspec_blockers.into_iter().map(|error| error.to_string()));
        Ok(worker)
    }

    fn reload(&mut self) -> anyhow::Result<()> {
        self.provider
            .lock()
            .map_err(|_| anyhow::anyhow!("lifecycle provider lock poisoned"))?
            .refresh();
        let loaded = OpsxLifecycle::load(JsonFileStore::from_path(&self.roots.ledger))
            .map_err(|error| anyhow::anyhow!("lifecycle state unavailable: {error}"))?;
        *self
            .opsx
            .lock()
            .map_err(|_| anyhow::anyhow!("lifecycle ledger lock poisoned"))? = loaded;
        Ok(())
    }

    fn execute(
        &mut self,
        request: LifecycleRequestV1,
        is_cancelled: impl Fn() -> bool,
    ) -> anyhow::Result<LifecycleResponseV1> {
        if is_cancelled() {
            anyhow::bail!("lifecycle request cancelled");
        }
        if !matches!(
            &request,
            LifecycleRequestV1::MutateDesign { .. } | LifecycleRequestV1::MutateOpenSpec { .. }
        ) {
            self.reload()?;
            if is_cancelled() {
                anyhow::bail!("lifecycle request cancelled");
            }
        }
        let read = crate::lifecycle::read_model::LifecycleReadHandle::new(
            Arc::clone(&self.provider),
            Arc::clone(&self.opsx),
            self.repo_path.clone(),
        );
        let payload = match request {
            LifecycleRequestV1::RepositorySnapshot { options, .. } => {
                let design = read
                    .design_tree_snapshot(false)
                    .map_err(|_| anyhow::anyhow!("lifecycle provider lock poisoned"))?;
                let sections = design
                    .nodes
                    .values()
                    .filter_map(|node| {
                        crate::lifecycle::design::read_node_sections(node)
                            .map(|sections| (node.id.clone(), sections))
                    })
                    .collect();
                let mut lifecycle = read.snapshot(options)?;
                for change in &mut lifecycle.openspec.changes {
                    if !change.has_tasks {
                        continue;
                    }
                    match self.openspec.validate_task_stable_ids(&change.name) {
                        Ok(report) => change.task_identity_findings = report.findings,
                        Err(error) => change.task_identity_error = Some(error.to_string()),
                    }
                }
                LifecyclePayloadV1::RepositorySnapshot {
                    design,
                    lifecycle,
                    sections,
                }
            }
            LifecycleRequestV1::Snapshot { options, .. } => {
                LifecyclePayloadV1::Snapshot(read.snapshot(options)?)
            }
            LifecycleRequestV1::DesignTree { .. } => LifecyclePayloadV1::DesignTree(
                read.design_tree_snapshot(false)
                    .map_err(|_| anyhow::anyhow!("lifecycle provider lock poisoned"))?,
            ),
            LifecycleRequestV1::ObserveDesignNode {
                id,
                include_sections,
                include_tree_context,
                ..
            } => LifecyclePayloadV1::DesignNode(Box::new(
                read.design_node_observation(&id, false, include_sections, include_tree_context)
                    .map_err(|_| anyhow::anyhow!("lifecycle provider lock poisoned"))?,
            )),
            LifecycleRequestV1::QueryDesign { query, .. } => {
                let tree = read
                    .design_tree_snapshot(false)
                    .map_err(|_| anyhow::anyhow!("lifecycle provider lock poisoned"))?;
                match query {
                    LifecycleReadQueryV1::Ready => {
                        LifecyclePayloadV1::Ready(crate::lifecycle::query::ready(&tree.nodes))
                    }
                    LifecycleReadQueryV1::Blocked => {
                        LifecyclePayloadV1::Blocked(crate::lifecycle::query::blocked(&tree.nodes))
                    }
                    LifecycleReadQueryV1::Frontier => {
                        LifecyclePayloadV1::Frontier(crate::lifecycle::query::frontier(&tree.nodes))
                    }
                }
            }
            LifecycleRequestV1::ValidateTaskStableIds { change, .. } => {
                LifecyclePayloadV1::TaskStableIds(self.openspec.validate_task_stable_ids(&change)?)
            }
            LifecycleRequestV1::Health { .. } => LifecyclePayloadV1::Health(self.health()?),
            LifecycleRequestV1::Doctor { .. } => {
                let recovered = crate::lifecycle::archive::recover_archive_transactions(
                    &self.repo_path,
                    &self.opsx,
                )?;
                let mut findings = crate::lifecycle::doctor::audit_repo(&self.repo_path);
                let changes = crate::lifecycle::spec::list_changes(&self.repo_path);
                let opsx_states = self
                    .opsx
                    .lock()
                    .map_err(|_| anyhow::anyhow!("lifecycle ledger lock poisoned"))?
                    .state()
                    .changes
                    .iter()
                    .map(|change| (change.name.clone(), change.state))
                    .collect();
                findings.extend(crate::lifecycle::doctor::audit_openspec_changes(
                    &changes,
                    &opsx_states,
                ));
                findings.extend(crate::lifecycle::doctor::audit_openspec_archives(
                    &self.repo_path,
                    &opsx_states,
                ));
                LifecyclePayloadV1::Doctor(LifecycleDoctorReportV1 {
                    findings: findings
                        .into_iter()
                        .map(|finding| LifecycleDoctorFindingV1 {
                            node_id: finding.node_id,
                            title: finding.title,
                            kind: finding.kind.as_str().to_string(),
                            detail: finding.detail,
                        })
                        .collect(),
                    recovered,
                })
            }
            LifecycleRequestV1::RecoverRepository { .. } => {
                let _lock = crate::lifecycle_transaction::lock_repository(&self.repo_path)
                    .map_err(|error| {
                        map_transaction_error(
                            error,
                            crate::lifecycle_transaction::TransactionErrorCode::Persistence,
                        )
                    })?;
                let ledger_store = JsonFileStore::from_path(&self.roots.ledger);
                let _ledger_transaction = ledger_store.lock_transaction().map_err(|error| {
                    transaction_error(
                        crate::lifecycle_transaction::TransactionErrorCode::Persistence,
                        error,
                    )
                })?;
                self.validate_frozen_authorities()?;
                let repo = self.repo_path.clone();
                let roots = self.roots.clone();
                let mut recovered =
                    crate::lifecycle_transaction::recover_pending(&repo, &roots, || {
                        self.reload()?;
                        self.revision(&|| false)
                    });
                recovered.extend(
                    crate::lifecycle_openspec_transaction::recover_pending(&repo, &roots, || {
                        self.reload()?;
                        self.revision(&|| false)
                    })
                    .into_iter()
                    .map(|error| error.to_string()),
                );
                self.reload()?;
                LifecyclePayloadV1::Recovery { recovered }
            }
            LifecycleRequestV1::MutateDesign {
                operation_id,
                expected_revision,
                mutation,
                ..
            } => {
                check_cancellation(&is_cancelled)?;
                let _lock = crate::lifecycle_transaction::lock_repository(&self.repo_path)
                    .map_err(|error| {
                        map_transaction_error(
                            error,
                            crate::lifecycle_transaction::TransactionErrorCode::Persistence,
                        )
                    })?;
                let ledger_store = JsonFileStore::from_path(&self.roots.ledger);
                let ledger_transaction = ledger_store.lock_transaction().map_err(|error| {
                    transaction_error(
                        crate::lifecycle_transaction::TransactionErrorCode::Persistence,
                        error,
                    )
                })?;
                self.validate_frozen_authorities()?;
                let repo = self.repo_path.clone();
                let roots = self.roots.clone();
                crate::lifecycle_transaction::preflight_mutation_repository(&repo, &roots)
                    .map_err(|error| {
                        map_transaction_error(
                            error,
                            crate::lifecycle_transaction::TransactionErrorCode::Validation,
                        )
                    })?;
                let blockers = crate::lifecycle_transaction::recover_pending(&repo, &roots, || {
                    self.reload()?;
                    self.revision(&|| false)
                });
                self.design_recovery_blockers.extend(blockers);
                self.design_recovery_blockers.sort();
                self.design_recovery_blockers.dedup();
                self.reload()?;
                if !self.design_recovery_blockers.is_empty() {
                    return Err(transaction_error(
                        crate::lifecycle_transaction::TransactionErrorCode::RecoveryRequired,
                        format!(
                            "design transaction recovery required: {}",
                            self.design_recovery_blockers.join("; ")
                        ),
                    ));
                }
                let fingerprint = crate::lifecycle_transaction::semantic_fingerprint(&mutation)
                    .map_err(|error| {
                        map_transaction_error(
                            error,
                            crate::lifecycle_transaction::TransactionErrorCode::Validation,
                        )
                    })?;
                if let Some(receipt) =
                    crate::lifecycle_transaction::read_receipt(&self.repo_path, &operation_id)
                        .map_err(|error| {
                            map_transaction_error(
                        error,
                        crate::lifecycle_transaction::TransactionErrorCode::RecoveryRequired,
                    )
                        })?
                {
                    if crate::lifecycle_transaction::receipt_fingerprint(&receipt) != fingerprint {
                        return Err(transaction_error(
                            crate::lifecycle_transaction::TransactionErrorCode::OperationConflict,
                            "lifecycle operation id conflicts with a different payload",
                        ));
                    }
                    let mut result = crate::lifecycle_transaction::receipt_result(receipt);
                    result.replayed = true;
                    LifecyclePayloadV1::DesignMutation(result)
                } else {
                    let current = self.revision(&is_cancelled)?;
                    if current != expected_revision {
                        return Err(transaction_error(
                            crate::lifecycle_transaction::TransactionErrorCode::StaleRevision,
                            "stale lifecycle repository revision",
                        ));
                    }
                    check_cancellation(&is_cancelled)?;
                    let result = crate::lifecycle_transaction::stage_and_commit(
                        crate::lifecycle_transaction::CommitContext {
                            repo: &repo,
                            roots: &roots,
                            operation_id: &operation_id,
                            semantic_fingerprint: &fingerprint,
                            pre_revision: &current,
                            ledger: &ledger_transaction,
                        },
                        &mutation,
                        &is_cancelled,
                        || {
                            self.reload()?;
                            self.revision(&|| false)
                        },
                    )
                    .map_err(|error| {
                        map_transaction_error(
                            error,
                            crate::lifecycle_transaction::TransactionErrorCode::Validation,
                        )
                    })?;
                    LifecyclePayloadV1::DesignMutation(result)
                }
            }
            LifecycleRequestV1::MutateOpenSpec {
                operation_id,
                expected_revision,
                mutation,
                ..
            } => {
                check_cancellation(&is_cancelled)?;
                let _lock = crate::lifecycle_transaction::lock_repository(&self.repo_path)
                    .map_err(|error| {
                        map_transaction_error(
                            error,
                            crate::lifecycle_transaction::TransactionErrorCode::Persistence,
                        )
                    })?;
                let ledger_store = JsonFileStore::from_path(&self.roots.ledger);
                let ledger_transaction = ledger_store.lock_transaction().map_err(|error| {
                    transaction_error(
                        crate::lifecycle_transaction::TransactionErrorCode::Persistence,
                        error,
                    )
                })?;
                self.validate_frozen_authorities()?;
                let repo = self.repo_path.clone();
                let roots = self.roots.clone();
                let fingerprint =
                    crate::lifecycle_openspec_transaction::semantic_fingerprint(&mutation)
                        .map_err(anyhow::Error::new)?;
                if let Some(receipt) = crate::lifecycle_openspec_transaction::read_receipt(
                    &self.repo_path,
                    &operation_id,
                )
                .map_err(anyhow::Error::new)?
                {
                    if crate::lifecycle_openspec_transaction::receipt_fingerprint(&receipt)
                        != fingerprint
                    {
                        return Err(crate::lifecycle_transaction::TransactionError {
                            code: crate::lifecycle_transaction::TransactionErrorCode::OperationConflict,
                            message: "lifecycle operation id conflicts with a different payload".into(),
                        }
                        .into());
                    }
                    let mut result = crate::lifecycle_openspec_transaction::receipt_result(receipt);
                    result.replayed = true;
                    LifecyclePayloadV1::OpenSpecMutation(result)
                } else {
                    let mut blockers =
                        crate::lifecycle_transaction::recover_pending(&repo, &roots, || {
                            self.reload()?;
                            self.revision(&|| false)
                        });
                    blockers.extend(
                        crate::lifecycle_openspec_transaction::recover_pending(
                            &repo,
                            &roots,
                            || {
                                self.reload()?;
                                self.revision(&|| false)
                            },
                        )
                        .into_iter()
                        .map(|error| error.to_string()),
                    );
                    self.design_recovery_blockers.extend(blockers);
                    self.design_recovery_blockers.sort();
                    self.design_recovery_blockers.dedup();
                    self.reload()?;
                    if !self.design_recovery_blockers.is_empty() {
                        return Err(crate::lifecycle_transaction::TransactionError {
                            code: crate::lifecycle_transaction::TransactionErrorCode::RecoveryRequired,
                            message: format!(
                                "repository transaction recovery required: {}",
                                self.design_recovery_blockers.join("; ")
                            ),
                        }
                        .into());
                    }
                    let current = self.revision(&is_cancelled)?;
                    if current != expected_revision {
                        return Err(crate::lifecycle_transaction::TransactionError {
                            code: crate::lifecycle_transaction::TransactionErrorCode::StaleRevision,
                            message: "stale lifecycle repository revision".into(),
                        }
                        .into());
                    }
                    let result = crate::lifecycle_openspec_transaction::stage_and_commit(
                        crate::lifecycle_transaction::CommitContext {
                            repo: &repo,
                            roots: &roots,
                            operation_id: &operation_id,
                            semantic_fingerprint: &fingerprint,
                            pre_revision: &current,
                            ledger: &ledger_transaction,
                        },
                        &mutation,
                        &is_cancelled,
                        || {
                            self.reload()?;
                            self.revision(&|| false)
                        },
                    )
                    .map_err(anyhow::Error::new)?;
                    LifecyclePayloadV1::OpenSpecMutation(result)
                }
            }
            #[cfg(test)]
            LifecycleRequestV1::TestBlock { started, .. } => {
                let _ = started.send(());
                while !is_cancelled() {
                    std::thread::sleep(Duration::from_millis(1));
                }
                anyhow::bail!("lifecycle request cancelled");
            }
        };
        if is_cancelled()
            && !matches!(
                payload,
                LifecyclePayloadV1::DesignMutation(_) | LifecyclePayloadV1::OpenSpecMutation(_)
            )
        {
            anyhow::bail!("lifecycle request cancelled");
        }
        let replay_revision = match &payload {
            LifecyclePayloadV1::DesignMutation(result)
            | LifecyclePayloadV1::OpenSpecMutation(result)
                if result.replayed =>
            {
                Some(result.committed_revision.clone())
            }
            _ => None,
        };
        let revision = if let Some(revision) = replay_revision {
            revision
        } else if matches!(
            payload,
            LifecyclePayloadV1::DesignMutation(_) | LifecyclePayloadV1::OpenSpecMutation(_)
        ) {
            self.revision(&|| false)?
        } else {
            self.revision(&is_cancelled)?
        };
        Ok(LifecycleResponseV1 {
            version: DTO_VERSION,
            revision,
            payload,
        })
    }

    fn revision(
        &self,
        is_cancelled: &impl Fn() -> bool,
    ) -> anyhow::Result<LifecycleRepositoryRevisionV1> {
        let ledger_revision = self
            .opsx
            .lock()
            .map_err(|_| anyhow::anyhow!("lifecycle ledger lock poisoned"))?
            .state()
            .revision;
        Ok(LifecycleRepositoryRevisionV1 {
            version: DTO_VERSION,
            design_root: relative_display(&self.repo_path, &self.roots.design),
            openspec_root: relative_display(&self.repo_path, &self.roots.openspec),
            ledger_path: relative_display(&self.repo_path, &self.roots.ledger),
            ledger_identity: file_content_identity(&self.roots.ledger)?,
            ledger_revision,
            artifact_digest: digest_roots(
                &self.repo_path,
                [&self.roots.design, &self.roots.openspec],
                is_cancelled,
            )?,
            transaction_digest: digest_transaction_frontier(&self.repo_path, is_cancelled)?,
        })
    }

    fn validate_frozen_authorities(&self) -> anyhow::Result<()> {
        validate_repository_roots(&self.repo_path)?;
        let current_ledger = resolve_ledger_path(&self.repo_path)?;
        if current_ledger != self.roots.ledger {
            anyhow::bail!(
                "lifecycle ledger authority changed after startup: selected {}, current {}",
                self.roots.ledger.display(),
                current_ledger.display()
            );
        }
        let current_openspec = resolve_openspec_path(&self.repo_path)?;
        if current_openspec != self.roots.openspec {
            anyhow::bail!(
                "OpenSpec authority changed after startup: selected {}, current {}",
                self.roots.openspec.display(),
                current_openspec.display()
            );
        }
        Ok(())
    }

    fn health(&self) -> anyhow::Result<LifecycleHealthV1> {
        let provider = self
            .provider
            .lock()
            .map_err(|_| anyhow::anyhow!("lifecycle provider lock poisoned"))?;
        let design_findings = provider
            .design_findings()
            .iter()
            .map(|finding| format!("{:?}: {}", finding.kind, finding.message))
            .collect();
        let artifact_findings = self
            .openspec
            .discover_active()
            .into_iter()
            .chain(self.openspec.discover_archived())
            .filter_map(|record| {
                (!matches!(record.health, omegon_opsx::ArtifactHealth::Healthy))
                    .then(|| format!("{}: {:?}", record.name, record.health))
            })
            .collect();
        let mut recovery_required = recovery_findings(&self.repo_path)?;
        recovery_required.extend(self.design_recovery_blockers.iter().cloned());
        recovery_required.sort();
        recovery_required.dedup();
        Ok(LifecycleHealthV1 {
            selected_design_root: relative_display(&self.repo_path, &self.roots.design),
            selected_openspec_root: relative_display(&self.repo_path, &self.roots.openspec),
            design_findings,
            artifact_findings,
            recovery_required,
        })
    }
}

fn run_worker(
    repo_path: PathBuf,
    roots: RepositoryRoots,
    mut receiver: mpsc::Receiver<WorkerCommand>,
    state: Arc<WorkerState>,
    startup: std::sync::mpsc::SyncSender<Result<(), String>>,
) {
    struct WriterClosure(Arc<WorkerState>);
    impl Drop for WriterClosure {
        fn drop(&mut self) {
            self.0.writer_closed.store(true, Ordering::Release);
            self.0.changed.notify_waiters();
        }
    }
    let _writer_closure = WriterClosure(Arc::clone(&state));
    let mut worker = match RepositoryWorker::load(repo_path, roots) {
        Ok(worker) => {
            let _ = startup.send(Ok(()));
            worker
        }
        Err(error) => {
            let _ = startup.send(Err(error.to_string()));
            return;
        }
    };
    while let Some(command) = receiver.blocking_recv() {
        if state.stopping.load(Ordering::Acquire) {
            break;
        }
        let caller = command.request.cancellation().clone();
        let generation = command.generation_cancellation.clone();
        if caller.is_cancelled() || generation.is_cancelled() {
            let _ = command.response.send(Err(LifecycleServiceErrorV1::new(
                LifecycleServiceErrorCodeV1::Cancelled,
                "lifecycle request cancelled",
            )));
            continue;
        }
        let result = worker
            .execute(command.request, || {
                state.stopping.load(Ordering::Acquire)
                    || caller.is_cancelled()
                    || generation.is_cancelled()
            })
            .map_err(|error| {
                error
                    .downcast_ref::<crate::lifecycle_transaction::TransactionError>()
                    .map(LifecycleServiceErrorV1::from_transaction)
                    .unwrap_or_else(|| LifecycleServiceErrorV1::classify(error.to_string()))
            });
        let _ = command.response.send(result);
    }
}

fn contains_openspec_artifacts(path: &Path) -> anyhow::Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            anyhow::bail!(
                "OpenSpec authority must not be a symbolic link: {}",
                path.display()
            )
        }
        Ok(metadata) if !metadata.is_dir() => return Ok(false),
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    }
    Ok(contains_markdown(&path.join("changes"))? || contains_markdown(&path.join("archive"))?)
}

fn contains_markdown(path: &Path) -> anyhow::Result<bool> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() {
        anyhow::bail!(
            "lifecycle artifact path must not be a symbolic link: {}",
            path.display()
        );
    }
    if !metadata.is_dir() {
        return Ok(false);
    }
    let mut found = false;
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();
        if file_type.is_symlink() {
            anyhow::bail!(
                "lifecycle artifact path must not be a symbolic link: {}",
                path.display()
            );
        }
        found |= (file_type.is_dir() && contains_markdown(&path)?)
            || (file_type.is_file()
                && path.extension().and_then(|extension| extension.to_str()) == Some("md"));
    }
    Ok(found)
}

fn digest_roots<'a>(
    repo_path: &Path,
    roots: impl IntoIterator<Item = &'a PathBuf>,
    is_cancelled: &impl Fn() -> bool,
) -> anyhow::Result<String> {
    let mut files = Vec::new();
    for root in roots {
        check_cancellation(is_cancelled)?;
        collect_files(root, &mut files, is_cancelled)?;
    }
    files.sort();
    let mut digest = Sha256::new();
    for path in files {
        check_cancellation(is_cancelled)?;
        digest.update(relative_display(repo_path, &path).as_bytes());
        digest.update([0]);
        digest.update(std::fs::read(&path)?);
        digest.update([0xff]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn file_content_identity(path: &Path) -> anyhow::Result<String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            anyhow::bail!(
                "ledger authority must not be a symbolic link: {}",
                path.display()
            )
        }
        Ok(metadata) if !metadata.is_file() => {
            anyhow::bail!("ledger authority is not a regular file: {}", path.display())
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok("absent".into()),
        Err(error) => return Err(error.into()),
    }
    match std::fs::read(path) {
        Ok(bytes) => Ok(format!("sha256:{:x}", Sha256::digest(bytes))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok("absent".into()),
        Err(error) => Err(error.into()),
    }
}

fn regular_file_no_follow(path: &Path) -> anyhow::Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            anyhow::bail!(
                "ledger authority must not be a symbolic link: {}",
                path.display()
            )
        }
        Ok(metadata) => Ok(metadata.is_file()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn digest_transaction_frontier(
    repo_path: &Path,
    is_cancelled: &impl Fn() -> bool,
) -> anyhow::Result<String> {
    let root = repo_path.join("ai/lifecycle/transactions");
    let mut files = Vec::new();
    let mut design_operations = std::collections::BTreeSet::new();
    let entries = match std::fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(format!("{:x}", Sha256::digest([])));
        }
        Err(error) => return Err(error.into()),
    };
    for entry in entries {
        check_cancellation(is_cancelled)?;
        let entry = entry?;
        if entry.file_name() == "design" || entry.file_name() == "repository-v1" {
            if entry.file_type()?.is_symlink() {
                anyhow::bail!(
                    "transaction frontier must not contain a symbolic link: {}",
                    entry.path().display()
                );
            }
            let design_root = entry.path();
            for (category, directory) in [
                ("operation", design_root.join("pending")),
                ("operation", design_root.join("receipts")),
                ("quarantine", design_root.join("quarantine")),
            ] {
                let design_entries = match std::fs::read_dir(directory) {
                    Ok(entries) => entries,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(error) => return Err(error.into()),
                };
                for design_entry in design_entries {
                    check_cancellation(is_cancelled)?;
                    let design_entry = design_entry?;
                    let path = design_entry.path();
                    if path.extension().and_then(|value| value.to_str()) != Some("json") {
                        continue;
                    }
                    if design_entry.file_type()?.is_symlink() {
                        anyhow::bail!(
                            "transaction record must not be a symbolic link: {}",
                            path.display()
                        );
                    }
                    let bytes = std::fs::read(&path)?;
                    let identity = serde_json::from_slice::<serde_json::Value>(&bytes)
                        .ok()
                        .and_then(|value| {
                            Some((
                                category.to_string(),
                                value.get("operation_id")?.as_str()?.to_string(),
                                value.get("semantic_fingerprint")?.as_str()?.to_string(),
                            ))
                        })
                        .unwrap_or_else(|| {
                            (
                                category.to_string(),
                                relative_display(repo_path, &path),
                                format!("corrupt:{:x}", Sha256::digest(&bytes)),
                            )
                        });
                    design_operations.insert(identity);
                }
            }
            continue;
        }
        collect_files(&entry.path(), &mut files, is_cancelled)?;
    }
    files.sort();
    let mut digest = Sha256::new();
    for path in files {
        digest.update(relative_display(repo_path, &path).as_bytes());
        digest.update([0]);
        digest.update(std::fs::read(path)?);
        digest.update([0xff]);
    }
    for (category, operation_id, semantic_fingerprint) in design_operations {
        digest.update(b"repository-operation\0");
        digest.update(category.as_bytes());
        digest.update([0]);
        digest.update(operation_id.as_bytes());
        digest.update([0]);
        digest.update(semantic_fingerprint.as_bytes());
        digest.update([0xff]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn collect_files(
    path: &Path,
    files: &mut Vec<PathBuf>,
    is_cancelled: &impl Fn() -> bool,
) -> anyhow::Result<()> {
    check_cancellation(is_cancelled)?;
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() {
        anyhow::bail!(
            "revision path must not be a symbolic link: {}",
            path.display()
        );
    }
    if metadata.is_file() {
        files.push(path.to_path_buf());
        return Ok(());
    }
    if !metadata.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(path)? {
        check_cancellation(is_cancelled)?;
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();
        if file_type.is_symlink() {
            anyhow::bail!(
                "revision path must not be a symbolic link: {}",
                path.display()
            );
        }
        if file_type.is_dir() {
            collect_files(&path, files, is_cancelled)?;
        } else if file_type.is_file() {
            files.push(path);
        }
    }
    Ok(())
}

fn check_cancellation(is_cancelled: &impl Fn() -> bool) -> anyhow::Result<()> {
    if is_cancelled() {
        return Err(transaction_error(
            crate::lifecycle_transaction::TransactionErrorCode::Cancelled,
            "lifecycle request cancelled",
        ));
    }
    Ok(())
}

fn transaction_error(
    code: crate::lifecycle_transaction::TransactionErrorCode,
    error: impl std::fmt::Display,
) -> anyhow::Error {
    anyhow::Error::new(crate::lifecycle_transaction::TransactionError::new(
        code,
        error.to_string(),
    ))
}

fn map_transaction_error(
    error: anyhow::Error,
    fallback: crate::lifecycle_transaction::TransactionErrorCode,
) -> anyhow::Error {
    if error
        .downcast_ref::<crate::lifecycle_transaction::TransactionError>()
        .is_some()
    {
        return error;
    }
    let code = if error.downcast_ref::<std::io::Error>().is_some()
        || matches!(
            error.downcast_ref::<omegon_opsx::OpsxError>(),
            Some(
                omegon_opsx::OpsxError::StoreError(_)
                    | omegon_opsx::OpsxError::RevisionConflict { .. }
            )
        ) {
        crate::lifecycle_transaction::TransactionErrorCode::Persistence
    } else {
        fallback
    };
    transaction_error(code, error)
}

fn relative_display(repo_path: &Path, path: &Path) -> String {
    path.strip_prefix(repo_path)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn recovery_findings(repo_path: &Path) -> anyhow::Result<Vec<String>> {
    let root = repo_path.join("ai/lifecycle/transactions");
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut findings = Vec::new();
    for entry in entries {
        let path = entry?.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("invalid transaction name");
        match std::fs::read_to_string(&path)
            .ok()
            .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
        {
            Some(value) if value.get("version").and_then(|value| value.as_u64()) == Some(1) => {
                findings.push(format!("pending transaction: {name}"));
            }
            _ => findings.push(format!("corrupt or future transaction: {name}")),
        }
    }
    findings.sort();
    Ok(findings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use omegon_traits::ManagedServiceCallError;

    fn direct_worker(repo: &Path) -> RepositoryWorker {
        let roots = RepositoryRoots::resolve(repo).unwrap();
        RepositoryWorker::load(repo.to_path_buf(), roots).unwrap()
    }

    fn current_revision(worker: &mut RepositoryWorker) -> LifecycleRepositoryRevisionV1 {
        worker
            .execute(
                LifecycleRequestV1::Health {
                    cancellation: CancellationToken::new(),
                },
                || false,
            )
            .unwrap()
            .revision
    }

    fn mutate_direct(
        worker: &mut RepositoryWorker,
        operation_id: &str,
        expected_revision: LifecycleRepositoryRevisionV1,
        mutation: DesignMutationV1,
    ) -> anyhow::Result<LifecycleResponseV1> {
        worker.execute(
            LifecycleRequestV1::MutateDesign {
                operation_id: operation_id.into(),
                expected_revision,
                mutation: Box::new(mutation),
                cancellation: CancellationToken::new(),
            },
            || false,
        )
    }

    fn mutate_openspec_direct(
        worker: &mut RepositoryWorker,
        operation_id: &str,
        expected_revision: LifecycleRepositoryRevisionV1,
        mutation: OpenSpecMutationV1,
    ) -> anyhow::Result<LifecycleResponseV1> {
        worker.execute(
            LifecycleRequestV1::MutateOpenSpec {
                operation_id: operation_id.into(),
                expected_revision,
                mutation: Box::new(mutation),
                cancellation: CancellationToken::new(),
            },
            || false,
        )
    }

    fn create_mutation(id: &str, status: Option<omegon_opsx::NodeState>) -> DesignMutationV1 {
        DesignMutationV1::Create {
            id: id.into(),
            title: format!("{id} title"),
            parent: None,
            status,
            tags: vec!["managed".into()],
            overview: format!("Overview for {id}."),
        }
    }

    fn write_change(repo: &Path, name: &str, body: &str) {
        write_change_under(&repo.join("openspec"), name, body);
    }

    #[test]
    fn lifecycle_service_design_create_single_file_mutation_and_audit_are_transactional() {
        let dir = tempfile::tempdir().unwrap();
        let mut worker = direct_worker(dir.path());
        let initial = current_revision(&mut worker);
        let created = mutate_direct(
            &mut worker,
            "create-node",
            initial,
            create_mutation("node", None),
        )
        .unwrap();
        assert!(dir.path().join("ai/docs/node.md").is_file());
        assert_eq!(created.revision.ledger_revision, 1);
        assert!(matches!(
            created.payload,
            LifecyclePayloadV1::DesignMutation(LifecycleMutationReceiptV1 {
                outcome: LifecycleMutationOutcomeV1::DesignCreated { ref path },
                ..
            }) if Path::new(path).ends_with(Path::new("ai").join("docs").join("node.md"))
        ));

        let updated = mutate_direct(
            &mut worker,
            "question-node",
            created.revision,
            DesignMutationV1::AddQuestion {
                id: "node".into(),
                question: "Which path?".into(),
            },
        )
        .unwrap();
        let source = std::fs::read_to_string(dir.path().join("ai/docs/node.md")).unwrap();
        assert!(source.contains("Which path?"));
        let state = omegon_opsx::StateStore::load(&JsonFileStore::new(dir.path())).unwrap();
        assert_eq!(state.revision, 2);
        assert_eq!(state.audit_log.len(), 1, "create records one audit outcome");
        assert!(
            state.nodes[0]
                .open_questions
                .contains(&"Which path?".into())
        );
        assert!(matches!(
            updated.payload,
            LifecyclePayloadV1::DesignMutation(_)
        ));
    }

    #[test]
    fn lifecycle_service_openspec_operations_are_transactional_and_replayable() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("ai/openspec")).unwrap();
        let mut worker = direct_worker(dir.path());
        let initial = current_revision(&mut worker);
        let proposed = mutate_openspec_direct(
            &mut worker,
            "propose-change",
            initial.clone(),
            OpenSpecMutationV1::Propose {
                name: "managed-change".into(),
                title: "Managed change".into(),
                intent: "Exercise managed OpenSpec transactions.".into(),
                bound_node: None,
            },
        )
        .unwrap();
        let proposal_path = dir
            .path()
            .join("ai/openspec/changes/managed-change/proposal.md");
        assert!(proposal_path.is_file());
        assert!(matches!(
            proposed.payload,
            LifecyclePayloadV1::OpenSpecMutation(LifecycleMutationReceiptV1 {
                outcome: LifecycleMutationOutcomeV1::OpenSpecProposed { ref path },
                ..
            }) if Path::new(path).ends_with(
                Path::new("ai").join("openspec").join("changes").join("managed-change")
            )
        ));

        let replay = mutate_openspec_direct(
            &mut worker,
            "propose-change",
            initial,
            OpenSpecMutationV1::Propose {
                name: "managed-change".into(),
                title: "Managed change".into(),
                intent: "Exercise managed OpenSpec transactions.".into(),
                bound_node: None,
            },
        )
        .unwrap();
        assert!(matches!(
            replay.payload,
            LifecyclePayloadV1::OpenSpecMutation(result)
                if result.replayed
                    && matches!(result.outcome, LifecycleMutationOutcomeV1::OpenSpecProposed { .. })
        ));
        let conflict = mutate_openspec_direct(
            &mut worker,
            "propose-change",
            replay.revision.clone(),
            OpenSpecMutationV1::Propose {
                name: "managed-change".into(),
                title: "Different payload".into(),
                intent: "Exercise managed OpenSpec transactions.".into(),
                bound_node: None,
            },
        )
        .unwrap_err();
        assert_eq!(
            conflict
                .downcast_ref::<crate::lifecycle_transaction::TransactionError>()
                .unwrap()
                .code,
            crate::lifecycle_transaction::TransactionErrorCode::OperationConflict
        );

        let spec = mutate_openspec_direct(
            &mut worker,
            "add-spec",
            proposed.revision.clone(),
            OpenSpecMutationV1::AddSpec {
                change: "managed-change".into(),
                domain: "runtime/managed".into(),
                content: "# Managed\n\n## ADDED Requirements\n".into(),
            },
        )
        .unwrap();
        let replay_after_advance = mutate_openspec_direct(
            &mut worker,
            "propose-change",
            spec.revision.clone(),
            OpenSpecMutationV1::Propose {
                name: "managed-change".into(),
                title: "Managed change".into(),
                intent: "Exercise managed OpenSpec transactions.".into(),
                bound_node: None,
            },
        )
        .unwrap();
        assert_eq!(replay_after_advance.revision, proposed.revision);
        assert!(matches!(
            replay_after_advance.payload,
            LifecyclePayloadV1::OpenSpecMutation(result)
                if result.replayed && result.committed_revision == proposed.revision
        ));
        let quarantine = dir.path().join("ai/lifecycle/transactions/quarantine");
        std::fs::create_dir_all(&quarantine).unwrap();
        std::fs::write(quarantine.join("unrelated.json"), "damaged").unwrap();
        let replay_with_unrelated_damage = mutate_openspec_direct(
            &mut worker,
            "propose-change",
            spec.revision.clone(),
            OpenSpecMutationV1::Propose {
                name: "managed-change".into(),
                title: "Managed change".into(),
                intent: "Exercise managed OpenSpec transactions.".into(),
                bound_node: None,
            },
        )
        .unwrap();
        assert_eq!(replay_with_unrelated_damage.revision, proposed.revision);
        std::fs::remove_file(quarantine.join("unrelated.json")).unwrap();
        assert!(
            dir.path()
                .join("ai/openspec/changes/managed-change/specs/runtime/managed.md")
                .is_file()
        );

        std::fs::write(
            dir.path()
                .join("ai/openspec/changes/managed-change/tasks.md"),
            "## Implementation\n\n- [ ] 1.1 Build it <!-- task-id:managed.build -->\n",
        )
        .unwrap();
        std::fs::write(
            dir.path()
                .join("ai/openspec/changes/managed-change/design.md"),
            "# Design\n",
        )
        .unwrap();
        let after_tasks = current_revision(&mut worker);
        assert_ne!(after_tasks, spec.revision);
        let reconciled = mutate_openspec_direct(
            &mut worker,
            "reconcile-tasks",
            after_tasks,
            OpenSpecMutationV1::ReconcileTasks {
                change: "managed-change".into(),
            },
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("tests")).unwrap();
        std::fs::write(dir.path().join("tests/managed.rs"), "// test stub\n").unwrap();
        let after_test_file = current_revision(&mut worker);
        assert_eq!(after_test_file, reconciled.revision);
        let registered = mutate_openspec_direct(
            &mut worker,
            "register-test",
            after_test_file,
            OpenSpecMutationV1::RegisterTestFile {
                change: "managed-change".into(),
                path: "tests/managed.rs".into(),
            },
        )
        .unwrap();
        assert!(
            std::fs::read_to_string(&proposal_path)
                .unwrap()
                .contains("state: implementing")
        );
        let completed = mutate_openspec_direct(
            &mut worker,
            "complete-task",
            registered.revision,
            OpenSpecMutationV1::SetTaskStatus {
                change: "managed-change".into(),
                group: "Implementation".into(),
                task_id: "1.1".into(),
                done: true,
            },
        )
        .unwrap();
        let verifying = mutate_openspec_direct(
            &mut worker,
            "transition-verifying",
            completed.revision,
            OpenSpecMutationV1::Transition {
                change: "managed-change".into(),
                state: omegon_opsx::ChangeState::Verifying,
            },
        )
        .unwrap();
        let archived = mutate_openspec_direct(
            &mut worker,
            "archive-change",
            verifying.revision,
            OpenSpecMutationV1::Archive {
                change: "managed-change".into(),
            },
        )
        .unwrap();
        assert!(!proposal_path.exists());
        assert!(
            dir.path()
                .join("ai/openspec/archive/managed-change/proposal.md")
                .is_file()
        );

        let reopened = mutate_openspec_direct(
            &mut worker,
            "reopen-change",
            archived.revision,
            OpenSpecMutationV1::Reopen {
                change: "managed-change".into(),
            },
        )
        .unwrap();
        assert!(proposal_path.is_file());
        assert!(
            std::fs::read_to_string(&proposal_path)
                .unwrap()
                .contains("state: proposed")
        );
        let abandoned = mutate_openspec_direct(
            &mut worker,
            "abandon-change",
            reopened.revision,
            OpenSpecMutationV1::Abandon {
                change: "managed-change".into(),
            },
        )
        .unwrap();
        assert!(
            std::fs::read_to_string(proposal_path)
                .unwrap()
                .contains("state: abandoned")
        );
        assert!(matches!(
            abandoned.payload,
            LifecyclePayloadV1::OpenSpecMutation(_)
        ));
    }

    #[test]
    fn lifecycle_service_operation_ids_are_global_across_mutation_domains() {
        let dir = tempfile::tempdir().unwrap();
        let mut worker = direct_worker(dir.path());
        let initial = current_revision(&mut worker);
        let design = mutate_direct(
            &mut worker,
            "shared-operation",
            initial,
            create_mutation("node", None),
        )
        .unwrap();

        let error = mutate_openspec_direct(
            &mut worker,
            "shared-operation",
            design.revision,
            OpenSpecMutationV1::Propose {
                name: "change".into(),
                title: "Change".into(),
                intent: "Must not reuse the design operation identity.".into(),
                bound_node: None,
            },
        )
        .unwrap_err();

        assert_eq!(
            error
                .downcast_ref::<crate::lifecycle_transaction::TransactionError>()
                .unwrap()
                .code,
            crate::lifecycle_transaction::TransactionErrorCode::OperationConflict
        );
        assert!(!dir.path().join("ai/openspec/changes/change").exists());
    }

    #[test]
    fn lifecycle_service_openspec_reopen_rejects_unhealthy_archive() {
        let dir = tempfile::tempdir().unwrap();
        let archived = dir.path().join("ai/openspec/archive/broken");
        std::fs::create_dir_all(&archived).unwrap();
        std::fs::write(archived.join("tasks.md"), "- [ ] Missing proposal\n").unwrap();
        let mut worker = direct_worker(dir.path());
        let revision = current_revision(&mut worker);

        let error = mutate_openspec_direct(
            &mut worker,
            "reopen-broken",
            revision,
            OpenSpecMutationV1::Reopen {
                change: "broken".into(),
            },
        )
        .unwrap_err();

        assert_eq!(
            error
                .downcast_ref::<crate::lifecycle_transaction::TransactionError>()
                .unwrap()
                .code,
            crate::lifecycle_transaction::TransactionErrorCode::Validation
        );
        assert!(archived.is_dir());
        assert!(!dir.path().join("ai/openspec/changes/broken").exists());
    }

    #[test]
    fn lifecycle_service_design_stale_external_edit_replay_and_operation_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let mut worker = direct_worker(dir.path());
        let expected = current_revision(&mut worker);
        std::fs::create_dir_all(dir.path().join("ai/docs")).unwrap();
        std::fs::write(
            dir.path().join("ai/docs/external.md"),
            "external authoring\n",
        )
        .unwrap();
        assert!(
            mutate_direct(
                &mut worker,
                "stale-create",
                expected,
                create_mutation("stale", None),
            )
            .unwrap_err()
            .to_string()
            .contains("stale lifecycle repository revision")
        );
        assert!(!dir.path().join("ai/docs/stale.md").exists());
        assert!(
            !dir.path()
                .join("ai/lifecycle/transactions/design/receipts")
                .join(format!(
                    "{}.json",
                    crate::lifecycle_transaction::operation_record_name("stale-create")
                ))
                .exists()
        );

        let current = current_revision(&mut worker);
        let first = mutate_direct(
            &mut worker,
            "stable-create",
            current,
            create_mutation("stable", None),
        )
        .unwrap();
        let audit_count = omegon_opsx::StateStore::load(&JsonFileStore::new(dir.path()))
            .unwrap()
            .audit_log
            .len();
        let replay = mutate_direct(
            &mut worker,
            "stable-create",
            first.revision.clone(),
            create_mutation("stable", None),
        )
        .unwrap();
        assert_eq!(replay.revision, first.revision);
        assert!(
            matches!(replay.payload, LifecyclePayloadV1::DesignMutation(result) if result.replayed)
        );
        assert_eq!(
            omegon_opsx::StateStore::load(&JsonFileStore::new(dir.path()))
                .unwrap()
                .audit_log
                .len(),
            audit_count
        );
        assert!(
            mutate_direct(
                &mut worker,
                "stable-create",
                first.revision,
                create_mutation("different", None),
            )
            .unwrap_err()
            .to_string()
            .contains("conflicts with a different payload")
        );
    }

    #[test]
    fn lifecycle_service_design_branch_and_implement_commit_all_files() {
        let dir = tempfile::tempdir().unwrap();
        let mut worker = direct_worker(dir.path());
        let initial = current_revision(&mut worker);
        let parent = mutate_direct(
            &mut worker,
            "create-parent",
            initial,
            create_mutation("parent", None),
        )
        .unwrap();
        let question = mutate_direct(
            &mut worker,
            "add-parent-question",
            parent.revision,
            DesignMutationV1::AddQuestion {
                id: "parent".into(),
                question: "Split this?".into(),
            },
        )
        .unwrap();
        let branched = mutate_direct(
            &mut worker,
            "branch-parent",
            question.revision,
            DesignMutationV1::BranchQuestion {
                parent_id: "parent".into(),
                question: "Split this?".into(),
                child_id: "child".into(),
                child_title: "Child".into(),
            },
        )
        .unwrap();
        let parent_source = std::fs::read_to_string(dir.path().join("ai/docs/parent.md")).unwrap();
        assert!(!parent_source.contains("Split this?"));
        assert!(parent_source.contains("child"));
        assert!(dir.path().join("ai/docs/child.md").is_file());

        let decided = mutate_direct(
            &mut worker,
            "create-implementation",
            branched.revision,
            create_mutation("implementation", Some(omegon_opsx::NodeState::Decided)),
        )
        .unwrap();
        let implemented = mutate_direct(
            &mut worker,
            "implement-node",
            decided.revision,
            DesignMutationV1::ImplementOpenSpec {
                id: "implementation".into(),
            },
        )
        .unwrap();
        assert!(
            dir.path()
                .join("ai/openspec/changes/implementation/proposal.md")
                .is_file()
        );
        let implementation =
            std::fs::read_to_string(dir.path().join("ai/docs/implementation.md")).unwrap();
        assert!(implementation.contains("status: implementing"));
        assert!(implementation.contains("openspec_change: \"implementation\""));
        assert_eq!(implemented.revision.ledger_revision, 5);
    }

    #[test]
    fn lifecycle_service_design_rejects_unknown_content_before_writing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("ai/docs")).unwrap();
        let path = dir.path().join("ai/docs/blocked.md");
        let source = "---\nid: blocked\ntitle: Blocked\nstatus: seed\n---\n\n# Blocked\n\n## Operator Notes\n\nDo not erase.\n";
        std::fs::write(&path, source).unwrap();
        let mut worker = direct_worker(dir.path());
        let current = current_revision(&mut worker);
        let error = mutate_direct(
            &mut worker,
            "blocked-update",
            current,
            DesignMutationV1::SetPriority {
                id: "blocked".into(),
                priority: 2,
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("blocked by unknown content"));
        assert_eq!(std::fs::read_to_string(path).unwrap(), source);
        assert!(!JsonFileStore::new(dir.path()).path().exists());
    }

    #[test]
    fn lifecycle_service_design_typed_metadata_mutations_use_canonical_codec() {
        let dir = tempfile::tempdir().unwrap();
        let mut worker = direct_worker(dir.path());
        let initial = current_revision(&mut worker);
        let created = mutate_direct(
            &mut worker,
            "metadata-create",
            initial,
            create_mutation("metadata", None),
        )
        .unwrap();
        let mutations = vec![
            DesignMutationV1::AddQuestion {
                id: "metadata".into(),
                question: "Temporary question?".into(),
            },
            DesignMutationV1::RemoveQuestion {
                id: "metadata".into(),
                question: "Temporary question?".into(),
            },
            DesignMutationV1::AddResearch {
                id: "metadata".into(),
                heading: "Evidence".into(),
                content: "Observed behavior.".into(),
            },
            DesignMutationV1::AddDecision {
                id: "metadata".into(),
                title: "Choose path".into(),
                status: "decided".into(),
                rationale: "It is deterministic.".into(),
            },
            DesignMutationV1::AddDependency {
                id: "metadata".into(),
                target_id: "dependency".into(),
            },
            DesignMutationV1::RemoveDependency {
                id: "metadata".into(),
                target_id: "dependency".into(),
            },
            DesignMutationV1::AddRelated {
                id: "metadata".into(),
                target_id: "related".into(),
            },
            DesignMutationV1::RemoveRelated {
                id: "metadata".into(),
                target_id: "related".into(),
            },
            DesignMutationV1::AddImplementationNotes {
                id: "metadata".into(),
                file_scope: vec![DesignFileScopeV1 {
                    path: "src/lib.rs".into(),
                    description: "Update behavior".into(),
                    action: Some("modified".into()),
                }],
                constraints: vec!["Preserve compatibility".into()],
            },
            DesignMutationV1::SetPriority {
                id: "metadata".into(),
                priority: 2,
            },
            DesignMutationV1::SetIssueType {
                id: "metadata".into(),
                issue_type: DesignIssueTypeV1::Feature,
            },
            DesignMutationV1::SetState {
                id: "metadata".into(),
                state: omegon_opsx::NodeState::Archived,
                archive_reason: Some("Superseded".into()),
                superseded_by: Some("replacement".into()),
                archived_at: Some("2026-08-24".into()),
            },
        ];
        let mut revision = created.revision;
        for (index, mutation) in mutations.into_iter().enumerate() {
            revision = mutate_direct(
                &mut worker,
                &format!("metadata-{index}"),
                revision,
                mutation,
            )
            .unwrap()
            .revision;
        }

        let source = std::fs::read_to_string(dir.path().join("ai/docs/metadata.md")).unwrap();
        for expected in [
            "status: archived",
            "issue_type: feature",
            "priority: 2",
            "archive_reason: \"Superseded\"",
            "### Evidence",
            "### Choose path",
            "`src/lib.rs`",
            "Preserve compatibility",
        ] {
            assert!(
                source.contains(expected),
                "missing {expected:?} in {source}"
            );
        }
        assert!(!source.contains("Temporary question?"));
        assert!(source.contains("dependencies: []"));
        assert!(source.contains("related: []"));
    }

    #[test]
    fn lifecycle_service_design_reconciles_external_questions_and_state_before_policy() {
        let dir = tempfile::tempdir().unwrap();
        let mut worker = direct_worker(dir.path());
        let initial = current_revision(&mut worker);
        let created = mutate_direct(
            &mut worker,
            "drift-create",
            initial,
            create_mutation("drift", Some(omegon_opsx::NodeState::Exploring)),
        )
        .unwrap();
        let path = dir.path().join("ai/docs/drift.md");
        let source = std::fs::read_to_string(&path).unwrap();
        let mut parsed = omegon_opsx::parse_design_artifact(&source, &path).unwrap();
        parsed
            .sections
            .open_questions
            .push("External question?".into());
        parsed.artifact.open_questions = parsed.sections.open_questions.clone();
        std::fs::write(
            &path,
            omegon_opsx::render_design_artifact(&parsed.artifact, &parsed.sections),
        )
        .unwrap();
        let question_revision = current_revision(&mut worker);
        let question_error = mutate_direct(
            &mut worker,
            "drift-decide",
            question_revision,
            DesignMutationV1::SetState {
                id: "drift".into(),
                state: omegon_opsx::NodeState::Decided,
                archive_reason: None,
                superseded_by: None,
                archived_at: None,
            },
        )
        .unwrap_err();
        assert!(question_error.to_string().contains("open questions"));

        parsed.sections.open_questions.clear();
        parsed.artifact.open_questions.clear();
        parsed.artifact.state = omegon_opsx::NodeState::Seed;
        std::fs::write(
            &path,
            omegon_opsx::render_design_artifact(&parsed.artifact, &parsed.sections),
        )
        .unwrap();
        let state_revision = current_revision(&mut worker);
        let state_error = mutate_direct(
            &mut worker,
            "drift-implement",
            state_revision,
            DesignMutationV1::SetState {
                id: "drift".into(),
                state: omegon_opsx::NodeState::Implementing,
                archive_reason: None,
                superseded_by: None,
                archived_at: None,
            },
        )
        .unwrap_err();
        assert!(state_error.to_string().contains("invalid transition"));
        assert_eq!(
            omegon_opsx::StateStore::load(&JsonFileStore::from_path(
                dir.path().join("ai/lifecycle/state.json")
            ))
            .unwrap()
            .revision,
            created.revision.ledger_revision
        );
    }

    #[test]
    fn lifecycle_service_design_blocks_on_unrelated_malformed_canonical_artifact() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("ai/docs")).unwrap();
        std::fs::write(
            dir.path().join("ai/docs/malformed.md"),
            "---\nid: malformed\nstatus: seed\n---\n# Missing title\n",
        )
        .unwrap();
        let mut worker = direct_worker(dir.path());
        let revision = current_revision(&mut worker);

        let error = mutate_direct(
            &mut worker,
            "malformed-unrelated",
            revision,
            create_mutation("other", None),
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("malformed canonical design artifact")
        );
        assert!(!dir.path().join("ai/docs/other.md").exists());
    }

    #[test]
    fn lifecycle_service_design_revalidates_openspec_and_ledger_authorities() {
        let dir = tempfile::tempdir().unwrap();
        let mut worker = direct_worker(dir.path());
        let revision = current_revision(&mut worker);
        write_change_under(&dir.path().join("ai/openspec"), "primary", "# Primary\n");
        write_change_under(&dir.path().join("openspec"), "legacy", "# Legacy\n");
        let openspec_error = mutate_direct(
            &mut worker,
            "post-start-openspec-conflict",
            revision,
            create_mutation("blocked", None),
        )
        .unwrap_err();
        assert!(
            openspec_error
                .to_string()
                .contains("conflicting OpenSpec authorities")
        );

        let ledger_dir = tempfile::tempdir().unwrap();
        let mut ledger_worker = direct_worker(ledger_dir.path());
        let ledger_revision = current_revision(&mut ledger_worker);
        for path in [
            ledger_dir.path().join("ai/lifecycle/state.json"),
            ledger_dir.path().join(".omegon/lifecycle/state.json"),
        ] {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(
                path,
                r#"{"version":1,"revision":0,"nodes":[],"changes":[],"milestones":[]}"#,
            )
            .unwrap();
        }
        assert!(RepositoryRoots::resolve(ledger_dir.path()).is_err());
        let ledger_error = mutate_direct(
            &mut ledger_worker,
            "post-start-ledger-conflict",
            ledger_revision,
            create_mutation("blocked", None),
        )
        .unwrap_err();
        assert!(
            ledger_error
                .to_string()
                .contains("conflicting lifecycle ledger authorities")
        );
    }

    #[test]
    fn lifecycle_service_revision_freezes_legacy_ledger_path_and_identity() {
        let dir = tempfile::tempdir().unwrap();
        let ledger = dir.path().join(".omegon/lifecycle/state.json");
        std::fs::create_dir_all(ledger.parent().unwrap()).unwrap();
        let content = r#"{"version":1,"revision":0,"nodes":[],"changes":[],"milestones":[]}"#;
        std::fs::write(&ledger, content).unwrap();
        let mut worker = direct_worker(dir.path());

        let revision = current_revision(&mut worker);

        assert_eq!(revision.ledger_path, ".omegon/lifecycle/state.json");
        assert_eq!(
            revision.ledger_identity,
            format!("sha256:{:x}", Sha256::digest(content.as_bytes()))
        );
    }

    #[cfg(unix)]
    #[test]
    fn lifecycle_service_design_rejects_symlink_artifacts_and_design_directory() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let mut worker = direct_worker(dir.path());
        let revision = current_revision(&mut worker);
        let design_revision = revision.clone();
        std::fs::create_dir_all(dir.path().join("ai/docs")).unwrap();
        let outside = dir.path().join("outside.md");
        std::fs::write(
            &outside,
            "---\nid: linked\ntitle: Linked\nstatus: seed\n---\n# Linked\n",
        )
        .unwrap();
        symlink(&outside, dir.path().join("ai/docs/linked.md")).unwrap();
        let error = mutate_direct(
            &mut worker,
            "symlink-artifact",
            revision,
            create_mutation("safe", None),
        )
        .unwrap_err();
        assert!(error.to_string().contains("symbolic link"));
        assert!(!dir.path().join("ai/docs/safe.md").exists());

        std::fs::remove_file(dir.path().join("ai/docs/linked.md")).unwrap();
        std::fs::create_dir_all(dir.path().join("outside-design")).unwrap();
        symlink(
            dir.path().join("outside-design"),
            dir.path().join("ai/docs/design"),
        )
        .unwrap();
        let error = mutate_direct(
            &mut worker,
            "symlink-directory",
            design_revision,
            create_mutation("safe", None),
        )
        .unwrap_err();
        assert!(error.to_string().contains("symbolic link"));
    }

    fn write_change_under(openspec_root: &Path, name: &str, body: &str) {
        let root = openspec_root.join("changes").join(name);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("proposal.md"), body).unwrap();
        std::fs::write(
            root.join("tasks.md"),
            "- [ ] 1. task <!-- task-id:task.one -->\n",
        )
        .unwrap();
    }

    async fn managed_service(
        path: PathBuf,
    ) -> (crate::bus::EventBus, ManagedServiceHandle<LifecycleService>) {
        let mut bus = crate::bus::EventBus::new();
        bus.register(Box::new(
            crate::features::lifecycle::LifecycleFeature::try_new(&path).unwrap(),
        ));
        bus.stage_managed_generation("lifecycle", start_candidate(path).await.unwrap())
            .unwrap();
        bus.try_finalize_managed().await.unwrap();
        let handle = bus
            .managed_service::<LifecycleService>(
                &lifecycle_capability_id(),
                &lifecycle_interface_id(),
            )
            .unwrap()
            .unwrap();
        (bus, handle)
    }

    #[tokio::test]
    async fn publishes_and_captures_revisioned_basic_snapshots() {
        let dir = tempfile::tempdir().unwrap();
        write_change(dir.path(), "basic", "# Basic\n");
        let (mut bus, handle) = managed_service(dir.path().to_path_buf()).await;
        let response = handle
            .invoke(LifecycleRequestV1::Snapshot {
                options: SnapshotOptions::default(),
                cancellation: CancellationToken::new(),
            })
            .await
            .unwrap();
        assert_eq!(response.version, DTO_VERSION);
        assert_eq!(response.revision.ledger_revision, 0);
        assert!(matches!(response.payload, LifecyclePayloadV1::Snapshot(_)));

        let binding = LifecycleBinding::default();
        binding.capture(&bus).unwrap();
        assert!(binding.handle().is_some());
        assert!(
            bus.shutdown_managed_services()
                .await
                .all_resources_settled()
        );
    }

    #[tokio::test]
    async fn design_reads_match_the_compatibility_query_projection() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("docs")).unwrap();
        std::fs::write(
            dir.path().join("docs/managed-node.md"),
            "---\nid: managed-node\ntitle: Managed node\nstatus: decided\nopen_questions:\n  - Which path?\n---\n\n# Managed node\n\n## Overview\n\nManaged overview.\n",
        )
        .unwrap();
        let (mut bus, handle) = managed_service(dir.path().to_path_buf()).await;

        let tree = handle
            .invoke(LifecycleRequestV1::DesignTree {
                cancellation: CancellationToken::new(),
            })
            .await
            .unwrap();
        let LifecyclePayloadV1::DesignTree(tree) = tree.payload else {
            panic!("expected design tree response");
        };
        assert!(tree.nodes.contains_key("managed-node"));

        let observation = handle
            .invoke(LifecycleRequestV1::ObserveDesignNode {
                id: "managed-node".into(),
                include_sections: true,
                include_tree_context: true,
                cancellation: CancellationToken::new(),
            })
            .await
            .unwrap();
        assert!(matches!(
            observation.payload,
            LifecyclePayloadV1::DesignNode(observation)
                if observation
                    .as_ref()
                    .as_ref()
                    .is_some_and(|observation| observation.sections.is_some())
        ));

        for (query, expected) in [
            (LifecycleReadQueryV1::Ready, "ready"),
            (LifecycleReadQueryV1::Blocked, "blocked"),
            (LifecycleReadQueryV1::Frontier, "frontier"),
        ] {
            let response = handle
                .invoke(LifecycleRequestV1::QueryDesign {
                    query,
                    cancellation: CancellationToken::new(),
                })
                .await
                .unwrap();
            match (expected, response.payload) {
                ("ready", LifecyclePayloadV1::Ready(nodes)) => assert_eq!(nodes.len(), 1),
                ("blocked", LifecyclePayloadV1::Blocked(nodes)) => assert!(nodes.is_empty()),
                ("frontier", LifecyclePayloadV1::Frontier(nodes)) => assert_eq!(nodes.len(), 1),
                _ => panic!("unexpected design query response"),
            }
        }
        assert!(
            bus.shutdown_managed_services()
                .await
                .all_resources_settled()
        );
    }

    #[tokio::test]
    async fn primary_design_and_openspec_roots_drive_service_reads() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("ai/docs")).unwrap();
        std::fs::write(
            dir.path().join("ai/docs/primary-node.md"),
            "---\nid: primary-node\ntitle: Primary node\nstatus: decided\n---\n\n# Primary node\n",
        )
        .unwrap();
        write_change_under(
            &dir.path().join("ai/openspec"),
            "primary-change",
            "# Primary change\n",
        );
        let (mut bus, handle) = managed_service(dir.path().to_path_buf()).await;

        let tree = handle
            .invoke(LifecycleRequestV1::DesignTree {
                cancellation: CancellationToken::new(),
            })
            .await
            .unwrap();
        assert!(matches!(
            tree.payload,
            LifecyclePayloadV1::DesignTree(tree) if tree.nodes.contains_key("primary-node")
        ));
        let snapshot = handle
            .invoke(LifecycleRequestV1::Snapshot {
                options: SnapshotOptions::default(),
                cancellation: CancellationToken::new(),
            })
            .await
            .unwrap();
        assert!(matches!(
            snapshot.payload,
            LifecyclePayloadV1::Snapshot(snapshot)
                if snapshot.openspec.changes.iter().any(|change| change.name == "primary-change")
        ));
        let validation = handle
            .invoke(LifecycleRequestV1::ValidateTaskStableIds {
                change: "primary-change".into(),
                cancellation: CancellationToken::new(),
            })
            .await
            .unwrap();
        assert!(matches!(
            validation.payload,
            LifecyclePayloadV1::TaskStableIds(report) if report.is_ok()
        ));
        assert_eq!(validation.revision.design_root, "ai/docs");
        assert_eq!(validation.revision.openspec_root, "ai/openspec");
        let recovery = handle
            .invoke(LifecycleRequestV1::RecoverRepository {
                cancellation: CancellationToken::new(),
            })
            .await
            .unwrap();
        assert!(matches!(
            recovery.payload,
            LifecyclePayloadV1::Recovery { recovered } if recovered.is_empty()
        ));
        assert!(
            bus.shutdown_managed_services()
                .await
                .all_resources_settled()
        );
    }

    #[tokio::test]
    async fn revision_is_deterministic_and_changes_after_external_edit() {
        let dir = tempfile::tempdir().unwrap();
        write_change(dir.path(), "revision", "# First\n");
        let (mut bus, handle) = managed_service(dir.path().to_path_buf()).await;
        let request = || LifecycleRequestV1::Health {
            cancellation: CancellationToken::new(),
        };
        let first = handle.invoke(request()).await.unwrap().revision;
        let second = handle.invoke(request()).await.unwrap().revision;
        assert_eq!(first, second);

        std::fs::write(
            dir.path().join("openspec/changes/revision/proposal.md"),
            "# Externally edited\n",
        )
        .unwrap();
        let third = handle.invoke(request()).await.unwrap().revision;
        assert_ne!(first.artifact_digest, third.artifact_digest);
        assert!(
            bus.shutdown_managed_services()
                .await
                .all_resources_settled()
        );
    }

    #[tokio::test]
    async fn active_cancellation_reaches_the_serial_worker() {
        let dir = tempfile::tempdir().unwrap();
        let (mut bus, handle) = managed_service(dir.path().to_path_buf()).await;
        let cancellation = CancellationToken::new();
        let (started, receiver) = std::sync::mpsc::sync_channel(1);
        let invocation = tokio::spawn({
            let cancellation = cancellation.clone();
            async move {
                handle
                    .invoke(LifecycleRequestV1::TestBlock {
                        started,
                        cancellation,
                    })
                    .await
            }
        });
        tokio::task::spawn_blocking(move || receiver.recv_timeout(Duration::from_secs(2)))
            .await
            .unwrap()
            .unwrap();
        cancellation.cancel();
        assert!(matches!(
            invocation.await.unwrap(),
            Err(ManagedServiceCallError::Operation(_))
        ));
        assert!(
            bus.shutdown_managed_services()
                .await
                .all_resources_settled()
        );
    }

    #[tokio::test]
    async fn shutdown_joins_worker_before_writer_and_stales_handle() {
        let dir = tempfile::tempdir().unwrap();
        let (mut bus, handle) = managed_service(dir.path().to_path_buf()).await;
        let report = bus.shutdown_managed_services().await;
        assert!(report.all_resources_settled(), "{report:?}");
        assert!(matches!(
            handle
                .invoke(LifecycleRequestV1::Health {
                    cancellation: CancellationToken::new(),
                })
                .await,
            Err(ManagedServiceCallError::GenerationRetired)
        ));
    }

    #[tokio::test]
    async fn rejected_candidate_rolls_back_its_worker() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let mut bus = crate::bus::EventBus::new();
        bus.register(Box::new(
            crate::features::lifecycle::LifecycleFeature::try_new(&path).unwrap(),
        ));
        bus.register(Box::new(
            crate::features::lifecycle::LifecycleFeature::try_new(&path).unwrap(),
        ));
        bus.stage_managed_generation("lifecycle", start_candidate(path).await.unwrap())
            .unwrap();
        assert!(bus.try_finalize_managed().await.is_err());
    }

    #[tokio::test]
    async fn exact_generation_transfer_keeps_the_captured_handle_callable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let roots = RepositoryRoots::resolve(&path).unwrap();
        let (commands, receiver) = mpsc::channel(QUEUE_CAPACITY);
        let state = Arc::new(WorkerState {
            stopping: AtomicBool::new(false),
            writer_closed: AtomicBool::new(false),
            worker_joined: AtomicBool::new(false),
            changed: Notify::new(),
            join: Mutex::new(None),
        });
        let (startup, started) = std::sync::mpsc::sync_channel(1);
        let worker_state = Arc::clone(&state);
        let worker_path = path.clone();
        let join = std::thread::spawn(move || {
            run_worker(worker_path, roots, receiver, worker_state, startup)
        });
        *state.join.lock().unwrap() = Some(join);
        tokio::task::spawn_blocking(move || started.recv())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let service = Arc::new(LifecycleService {
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
                RuntimeCompositionGenerationId::new("composition:lifecycle-test").unwrap(),
                omegon_traits::RuntimeContributionId::new("feature:lifecycle").unwrap(),
                RuntimeContributionGenerationId::new(LIFECYCLE_GENERATION).unwrap(),
                Duration::from_secs(30),
                Duration::from_secs(5),
                resources,
            )
            .unwrap();
            candidate
                .add_service(
                    lifecycle_capability_id(),
                    lifecycle_interface_id(),
                    service.clone(),
                )
                .unwrap();
            candidate
        };
        let mut bus = crate::bus::EventBus::new();
        bus.register(Box::new(
            crate::features::lifecycle::LifecycleFeature::try_new(&path).unwrap(),
        ));
        bus.stage_managed_generation("lifecycle", candidate())
            .unwrap();
        bus.try_finalize_managed().await.unwrap();
        let handle = bus
            .managed_service::<LifecycleService>(
                &lifecycle_capability_id(),
                &lifecycle_interface_id(),
            )
            .unwrap()
            .unwrap();

        bus.stage_managed_generation("lifecycle", candidate())
            .unwrap();

        bus.try_finalize_managed().await.unwrap();

        let response = handle
            .invoke(LifecycleRequestV1::Health {
                cancellation: CancellationToken::new(),
            })
            .await
            .unwrap();
        assert!(matches!(response.payload, LifecyclePayloadV1::Health(_)));
        assert!(
            bus.shutdown_managed_services()
                .await
                .all_resources_settled()
        );
    }

    #[tokio::test]
    async fn conflicting_openspec_roots_reject_readiness() {
        let dir = tempfile::tempdir().unwrap();
        write_change_under(&dir.path().join("ai/openspec"), "primary", "# Primary\n");
        write_change_under(&dir.path().join("openspec"), "legacy", "# Legacy\n");
        assert!(validate_repository_roots(dir.path()).is_err());
        let error = match start_candidate(dir.path().to_path_buf()).await {
            Ok(_) => panic!("conflicting roots must reject startup"),
            Err(error) => error,
        };
        assert!(
            error
                .to_string()
                .contains("conflicting OpenSpec authorities")
        );

        let mut bus = crate::bus::EventBus::new();
        let registered = match validate_repository_roots(dir.path())
            .and_then(|()| crate::features::lifecycle::LifecycleFeature::try_new(dir.path()))
        {
            Ok(feature) => {
                bus.register(Box::new(feature));
                true
            }
            Err(_) => false,
        };
        assert!(!registered, "compatibility lifecycle must remain absent");
        bus.try_finalize_managed().await.unwrap();
        let binding = LifecycleBinding::default();
        binding.capture(&bus).unwrap();
        assert!(binding.handle().is_none());
    }

    #[test]
    fn root_metadata_files_do_not_create_a_conflict() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("ai/openspec")).unwrap();
        std::fs::write(dir.path().join("ai/openspec/.DS_Store"), "metadata").unwrap();
        write_change_under(&dir.path().join("openspec"), "legacy", "# Legacy\n");

        validate_repository_roots(dir.path()).unwrap();
        assert_eq!(
            RepositoryRoots::resolve(dir.path()).unwrap().openspec,
            dir.path().join("openspec")
        );
        let mut worker = direct_worker(dir.path());
        let revision = current_revision(&mut worker);
        mutate_openspec_direct(
            &mut worker,
            "legacy-root-proposal",
            revision,
            OpenSpecMutationV1::Propose {
                name: "managed-legacy".into(),
                title: "Managed legacy root".into(),
                intent: "Keep the populated legacy authority selected.".into(),
                bound_node: None,
            },
        )
        .unwrap();
        assert!(
            dir.path()
                .join("openspec/changes/managed-legacy/proposal.md")
                .is_file()
        );
        assert!(
            !dir.path()
                .join("ai/openspec/changes/managed-legacy")
                .exists()
        );
    }

    #[tokio::test]
    async fn corrupt_and_future_ledgers_reject_startup() {
        for content in [
            "not json",
            r#"{"version":999,"nodes":[],"changes":[],"milestones":[]}"#,
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("ai/lifecycle/state.json");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, content).unwrap();
            assert!(start_candidate(dir.path().to_path_buf()).await.is_err());
        }
    }

    #[tokio::test]
    async fn task_validation_health_and_recovery_are_owned_responses() {
        let dir = tempfile::tempdir().unwrap();
        write_change(dir.path(), "owned", "# Owned\n");
        let (mut bus, handle) = managed_service(dir.path().to_path_buf()).await;
        let validation = handle
            .invoke(LifecycleRequestV1::ValidateTaskStableIds {
                change: "owned".into(),
                cancellation: CancellationToken::new(),
            })
            .await
            .unwrap();
        assert!(matches!(
            validation.payload,
            LifecyclePayloadV1::TaskStableIds(_)
        ));
        let recovery = handle
            .invoke(LifecycleRequestV1::RecoverRepository {
                cancellation: CancellationToken::new(),
            })
            .await
            .unwrap();
        assert!(matches!(
            recovery.payload,
            LifecyclePayloadV1::Recovery { recovered } if recovered.is_empty()
        ));
        assert!(
            bus.shutdown_managed_services()
                .await
                .all_resources_settled()
        );
    }
}
