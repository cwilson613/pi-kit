//! Boot-captured managed ownership for project Git and JJ operations.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
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

use crate::managed_service_bus::{ManagedGenerationCandidate, ManagedResourceRegistration};
use crate::service_generation::ManagedServiceHandle;

pub(crate) const GIT_CAPABILITY: &str = "service:git";
pub(crate) const GIT_INTERFACE: &str = "interface:omegon-git-v1";
pub(crate) const GIT_GENERATION: &str = "contribution:git-managed-v1";
const WORKER_RESOURCE: &str = "resource:git-worker";
const PROCESS_RESOURCE: &str = "resource:git-process-set";
const WRITER_RESOURCE: &str = "resource:git-writer";
const QUEUE_CAPACITY: usize = 32;

pub(crate) fn git_capability_id() -> RuntimeCapabilityId {
    RuntimeCapabilityId::new(GIT_CAPABILITY).expect("static capability id is valid")
}

pub(crate) fn git_interface_id() -> RuntimeServiceInterfaceId {
    RuntimeServiceInterfaceId::new(GIT_INTERFACE).expect("static interface id is valid")
}

#[derive(Clone, Default)]
pub(crate) struct GitBinding {
    handle: Arc<OnceLock<Option<ManagedServiceHandle<GitService>>>>,
}

impl GitBinding {
    pub(crate) fn capture(&self, bus: &crate::bus::EventBus) -> anyhow::Result<()> {
        let handle =
            bus.managed_service::<GitService>(&git_capability_id(), &git_interface_id())?;
        self.handle
            .set(handle)
            .map_err(|_| anyhow::anyhow!("Git managed handle was already captured"))
    }

    pub(crate) fn handle(&self) -> Option<ManagedServiceHandle<GitService>> {
        self.handle.get().and_then(Clone::clone)
    }

    pub(crate) async fn invoke(
        &self,
        request: GitRequest,
    ) -> Result<GitResponse, ManagedServiceCallError<GitServiceError>> {
        let Some(handle) = self.handle() else {
            return Err(ManagedServiceCallError::Operation(
                GitServiceError::unavailable(),
            ));
        };
        handle.invoke(request).await
    }
}

pub(crate) struct GitFeature;

#[async_trait]
impl Feature for GitFeature {
    fn name(&self) -> &str {
        "git"
    }

    fn runtime_contribution_generation_id(&self) -> Option<RuntimeContributionGenerationId> {
        Some(
            RuntimeContributionGenerationId::new(GIT_GENERATION)
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GitRepositorySnapshot {
    pub(crate) root: PathBuf,
    pub(crate) branch: Option<String>,
    pub(crate) head_sha: Option<String>,
    pub(crate) jj_change_id: Option<String>,
    pub(crate) is_jj: bool,
    pub(crate) submodules: Vec<String>,
    pub(crate) pending_lifecycle: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GitWorktreeMode {
    Git,
    Smart,
}

#[derive(Debug, Clone)]
pub(crate) enum GitRequest {
    Snapshot {
        cancellation: CancellationToken,
    },
    RecordEdits {
        paths: Vec<String>,
        cancellation: CancellationToken,
    },
    AuthorizeWorkspace {
        path: PathBuf,
        cancellation: CancellationToken,
    },
    Status {
        path: PathBuf,
        cancellation: CancellationToken,
    },
    Commit {
        path: PathBuf,
        message: String,
        paths: Vec<String>,
        cancellation: CancellationToken,
    },
    CreateWorktree {
        workspace_path: PathBuf,
        name: String,
        branch: String,
        mode: GitWorktreeMode,
        cancellation: CancellationToken,
    },
    RemoveWorktree {
        workspace_path: PathBuf,
        name: String,
        mode: GitWorktreeMode,
        cancellation: CancellationToken,
    },
    DeleteBranch {
        branch: String,
        cancellation: CancellationToken,
    },
    Merge {
        branch: String,
        message: String,
        squash: bool,
        cancellation: CancellationToken,
    },
    ListSubmodules {
        path: PathBuf,
        cancellation: CancellationToken,
    },
    InitSubmodules {
        path: PathBuf,
        cancellation: CancellationToken,
    },
    CommitDirtySubmodules {
        path: PathBuf,
        label: String,
        cancellation: CancellationToken,
    },
}

impl GitRequest {
    fn cancellation(&self) -> &CancellationToken {
        match self {
            Self::Snapshot { cancellation }
            | Self::RecordEdits { cancellation, .. }
            | Self::AuthorizeWorkspace { cancellation, .. }
            | Self::Status { cancellation, .. }
            | Self::Commit { cancellation, .. }
            | Self::CreateWorktree { cancellation, .. }
            | Self::RemoveWorktree { cancellation, .. }
            | Self::DeleteBranch { cancellation, .. }
            | Self::Merge { cancellation, .. }
            | Self::ListSubmodules { cancellation, .. }
            | Self::InitSubmodules { cancellation, .. }
            | Self::CommitDirtySubmodules { cancellation, .. } => cancellation,
        }
    }
}

#[derive(Debug)]
pub(crate) enum GitResponse {
    Snapshot(GitRepositorySnapshot),
    Recorded,
    Status(GitStatusSnapshot),
    Commit {
        revision: String,
        files_staged: usize,
        submodule_commits: usize,
        backend: &'static str,
        branch: Option<String>,
    },
    Worktree(GitWorktreeInfo),
    Removed,
    Merge(GitMergeOutcome),
    Submodules(Vec<String>),
    SubmodulesInitialized(usize),
    DirtySubmodulesCommitted(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GitWorktreeInfo {
    pub(crate) path: PathBuf,
    pub(crate) branch: String,
    pub(crate) backend: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GitStatusSnapshot {
    pub(crate) entries: Vec<GitStatusEntry>,
    pub(crate) is_clean: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GitStatusEntry {
    pub(crate) path: String,
    pub(crate) kind: GitStatusKind,
    pub(crate) is_submodule: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GitStatusKind {
    Modified,
    Staged,
    StagedAndModified,
    Untracked,
    Deleted,
    Renamed,
    SubmoduleModified,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GitMergeOutcome {
    Success { revision: String },
    NoChanges,
    Conflict { files: Vec<String> },
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GitServiceErrorCode {
    Unavailable,
    Cancelled,
    OutsideRepository,
    Operation,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub(crate) struct GitServiceError {
    pub(crate) code: GitServiceErrorCode,
    message: String,
}

impl GitServiceError {
    fn unavailable() -> Self {
        Self {
            code: GitServiceErrorCode::Unavailable,
            message: "managed Git service is unavailable".into(),
        }
    }

    fn cancelled() -> Self {
        Self {
            code: GitServiceErrorCode::Cancelled,
            message: "Git operation was cancelled".into(),
        }
    }

    fn outside(path: &Path) -> Self {
        Self {
            code: GitServiceErrorCode::OutsideRepository,
            message: format!(
                "Git operation path is outside the captured repository: {}",
                path.display()
            ),
        }
    }

    fn operation(error: impl std::fmt::Display) -> Self {
        Self {
            code: GitServiceErrorCode::Operation,
            message: error.to_string(),
        }
    }
}

pub(crate) struct GitService {
    commands: mpsc::Sender<WorkerCommand>,
}

struct WorkerCommand {
    request: GitRequest,
    generation_cancellation: CancellationToken,
    response: oneshot::Sender<Result<GitResponse, GitServiceError>>,
}

impl ManagedServiceContract for GitService {
    type Request = GitRequest;
    type Response = GitResponse;
    type Error = GitServiceError;

    fn execute<'a>(
        &'a self,
        request: Self::Request,
        context: ManagedCallContext,
    ) -> ManagedServiceFuture<'a, Self::Response, Self::Error> {
        Box::pin(async move {
            let caller = request.cancellation().clone();
            if caller.is_cancelled() || context.cancellation.is_cancelled() {
                return Err(GitServiceError::cancelled());
            }
            let (response, receive) = oneshot::channel();
            let command = WorkerCommand {
                request,
                generation_cancellation: context.cancellation.clone(),
                response,
            };
            tokio::select! {
                biased;
                () = caller.cancelled() => return Err(GitServiceError::cancelled()),
                () = context.cancellation.cancelled() => return Err(GitServiceError::cancelled()),
                sent = self.commands.send(command) => sent.map_err(|_| GitServiceError::unavailable())?,
            }
            let mut receive = receive;
            tokio::select! {
                biased;
                result = &mut receive => result.map_err(|_| GitServiceError::unavailable())?,
                () = caller.cancelled() => {
                    let _ = receive.await;
                    Err(GitServiceError::cancelled())
                },
                () = context.cancellation.cancelled() => {
                    let _ = receive.await;
                    Err(GitServiceError::cancelled())
                },
            }
        })
    }
}

struct WorkerState {
    stopping: AtomicBool,
    worker_joined: AtomicBool,
    process_settled: AtomicBool,
    writer_settled: AtomicBool,
    changed: Notify,
    join: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl WorkerState {
    fn request_stop(&self) {
        self.stopping.store(true, Ordering::Release);
        self.changed.notify_waiters();
    }

    fn wake(commands: &mpsc::Sender<WorkerCommand>) {
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        let (response, _) = oneshot::channel();
        let _ = commands.try_send(WorkerCommand {
            request: GitRequest::Snapshot {
                cancellation: cancellation.clone(),
            },
            generation_cancellation: cancellation,
            response,
        });
    }
}

struct ResourceController {
    state: Arc<WorkerState>,
    commands: mpsc::Sender<WorkerCommand>,
    kind: ResourceControllerKind,
}

#[derive(Clone, Copy)]
enum ResourceControllerKind {
    Worker,
    ProcessSet,
    Writer,
}

impl Drop for ResourceController {
    fn drop(&mut self) {
        self.state.request_stop();
        WorkerState::wake(&self.commands);
    }
}

impl ManagedResourceController for ResourceController {
    fn request_stop(&self) {
        self.state.request_stop();
        WorkerState::wake(&self.commands);
    }

    fn force_stop(&self) {
        self.request_stop();
    }

    fn await_settled(&self) -> ManagedResourceSettlementFuture<'_> {
        let state = Arc::clone(&self.state);
        let kind = self.kind;
        Box::pin(async move {
            if matches!(kind, ResourceControllerKind::Worker)
                && !state.worker_joined.load(Ordering::Acquire)
            {
                let join = state
                    .join
                    .lock()
                    .map_err(|_| "Git worker join lock poisoned".to_string())?
                    .take();
                if let Some(join) = join {
                    let result = tokio::task::spawn_blocking(move || join.join())
                        .await
                        .map_err(|error| format!("Git worker join task failed: {error}"))?;
                    state.worker_joined.store(true, Ordering::Release);
                    state.changed.notify_waiters();
                    if result.is_err() {
                        return Err("Git worker panicked".into());
                    }
                }
            }
            loop {
                let settled = match kind {
                    ResourceControllerKind::Worker => state.worker_joined.load(Ordering::Acquire),
                    ResourceControllerKind::ProcessSet => {
                        state.process_settled.load(Ordering::Acquire)
                    }
                    ResourceControllerKind::Writer => state.writer_settled.load(Ordering::Acquire),
                };
                if settled {
                    return Ok(());
                }
                let changed = state.changed.notified();
                let settled = match kind {
                    ResourceControllerKind::Worker => state.worker_joined.load(Ordering::Acquire),
                    ResourceControllerKind::ProcessSet => {
                        state.process_settled.load(Ordering::Acquire)
                    }
                    ResourceControllerKind::Writer => state.writer_settled.load(Ordering::Acquire),
                };
                if settled {
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
    let (commands, receiver) = mpsc::channel(QUEUE_CAPACITY);
    let state = Arc::new(WorkerState {
        stopping: AtomicBool::new(false),
        worker_joined: AtomicBool::new(false),
        process_settled: AtomicBool::new(false),
        writer_settled: AtomicBool::new(false),
        changed: Notify::new(),
        join: Mutex::new(None),
    });
    let (startup, started) = std::sync::mpsc::sync_channel(1);
    let worker_state = Arc::clone(&state);
    let join = std::thread::Builder::new()
        .name("omegon-git".into())
        .spawn(move || run_worker(repo_path, receiver, worker_state, startup))?;
    *state
        .join
        .lock()
        .map_err(|_| anyhow::anyhow!("Git worker join lock poisoned"))? = Some(join);
    let startup = tokio::task::spawn_blocking(move || started.recv())
        .await
        .map_err(|error| anyhow::anyhow!("Git readiness task failed: {error}"))?
        .map_err(|_| anyhow::anyhow!("Git worker exited before readiness"))?;
    if let Err(error) = startup {
        state.request_stop();
        WorkerState::wake(&commands);
        if let Some(join) = state.join.lock().ok().and_then(|mut join| join.take()) {
            let _ = tokio::task::spawn_blocking(move || join.join()).await;
        }
        anyhow::bail!(error);
    }

    let service = Arc::new(GitService {
        commands: commands.clone(),
    });
    build_candidate(commands, state, service)
}

fn build_candidate(
    commands: mpsc::Sender<WorkerCommand>,
    state: Arc<WorkerState>,
    service: Arc<GitService>,
) -> anyhow::Result<ManagedGenerationCandidate> {
    let writer_id =
        RuntimeContributionResourceId::new(WRITER_RESOURCE).expect("static resource id is valid");
    let process_id =
        RuntimeContributionResourceId::new(PROCESS_RESOURCE).expect("static resource id is valid");
    let controller = |kind| -> Arc<dyn ManagedResourceController> {
        Arc::new(ResourceController {
            state: Arc::clone(&state),
            commands: commands.clone(),
            kind,
        })
    };
    let resources = vec![
        ManagedResourceRegistration::new(
            writer_id.clone(),
            RuntimeOwnedResourceKind::DurableWriter,
            RuntimeCleanupAssurance::Strict,
            Vec::new(),
            controller(ResourceControllerKind::Writer),
        ),
        ManagedResourceRegistration::new(
            process_id.clone(),
            RuntimeOwnedResourceKind::ProcessTree,
            RuntimeCleanupAssurance::Strict,
            vec![writer_id],
            controller(ResourceControllerKind::ProcessSet),
        ),
        ManagedResourceRegistration::new(
            RuntimeContributionResourceId::new(WORKER_RESOURCE)
                .expect("static resource id is valid"),
            RuntimeOwnedResourceKind::Task,
            RuntimeCleanupAssurance::Strict,
            vec![process_id],
            controller(ResourceControllerKind::Worker),
        ),
    ];
    let mut candidate = ManagedGenerationCandidate::new(
        RuntimeCompositionGenerationId::new("composition:git-boot")
            .expect("static composition id is valid"),
        omegon_traits::RuntimeContributionId::new("feature:git")
            .expect("static contribution id is valid"),
        RuntimeContributionGenerationId::new(GIT_GENERATION)
            .expect("static generation id is valid"),
        Duration::from_secs(30),
        Duration::from_secs(5),
        resources,
    )?;
    candidate.add_service(git_capability_id(), git_interface_id(), service)?;
    Ok(candidate)
}

#[cfg(test)]
async fn exact_transfer_candidates(
    repo_path: PathBuf,
) -> (ManagedGenerationCandidate, ManagedGenerationCandidate) {
    let (commands, receiver) = mpsc::channel(QUEUE_CAPACITY);
    let state = Arc::new(WorkerState {
        stopping: AtomicBool::new(false),
        worker_joined: AtomicBool::new(false),
        process_settled: AtomicBool::new(false),
        writer_settled: AtomicBool::new(false),
        changed: Notify::new(),
        join: Mutex::new(None),
    });
    let (startup, started) = std::sync::mpsc::sync_channel(1);
    let worker_state = Arc::clone(&state);
    let join = std::thread::spawn(move || run_worker(repo_path, receiver, worker_state, startup));
    *state.join.lock().unwrap() = Some(join);
    tokio::task::spawn_blocking(move || started.recv())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let service = Arc::new(GitService {
        commands: commands.clone(),
    });
    let controller = |kind| -> Arc<ResourceController> {
        Arc::new(ResourceController {
            state: Arc::clone(&state),
            commands: commands.clone(),
            kind,
        })
    };
    let worker = controller(ResourceControllerKind::Worker);
    let process = controller(ResourceControllerKind::ProcessSet);
    let writer = controller(ResourceControllerKind::Writer);
    let candidate = || {
        let writer_id = RuntimeContributionResourceId::new(WRITER_RESOURCE).unwrap();
        let process_id = RuntimeContributionResourceId::new(PROCESS_RESOURCE).unwrap();
        let resources = vec![
            ManagedResourceRegistration::new(
                writer_id.clone(),
                RuntimeOwnedResourceKind::DurableWriter,
                RuntimeCleanupAssurance::Strict,
                Vec::new(),
                writer.clone(),
            ),
            ManagedResourceRegistration::new(
                process_id.clone(),
                RuntimeOwnedResourceKind::ProcessTree,
                RuntimeCleanupAssurance::Strict,
                vec![writer_id],
                process.clone(),
            ),
            ManagedResourceRegistration::new(
                RuntimeContributionResourceId::new(WORKER_RESOURCE).unwrap(),
                RuntimeOwnedResourceKind::Task,
                RuntimeCleanupAssurance::Strict,
                vec![process_id],
                worker.clone(),
            ),
        ];
        let mut candidate = ManagedGenerationCandidate::new(
            RuntimeCompositionGenerationId::new("composition:git-boot").unwrap(),
            omegon_traits::RuntimeContributionId::new("feature:git").unwrap(),
            RuntimeContributionGenerationId::new(GIT_GENERATION).unwrap(),
            Duration::from_secs(30),
            Duration::from_secs(5),
            resources,
        )
        .unwrap();
        candidate
            .add_service(
                git_capability_id(),
                git_interface_id(),
                Arc::clone(&service),
            )
            .unwrap();
        candidate
    };
    (candidate(), candidate())
}

pub(crate) async fn bounded_binding(
    repo_path: PathBuf,
) -> anyhow::Result<(crate::bus::EventBus, GitBinding)> {
    let binding = GitBinding::default();
    let mut bus = crate::bus::EventBus::new();
    bus.register(Box::new(GitFeature));
    bus.stage_managed_generation("git", start_candidate(repo_path).await?)?;
    bus.try_finalize_managed().await?;
    binding.capture(&bus)?;
    Ok((bus, binding))
}

fn run_worker(
    repo_path: PathBuf,
    mut receiver: mpsc::Receiver<WorkerCommand>,
    state: Arc<WorkerState>,
    startup: std::sync::mpsc::SyncSender<Result<(), String>>,
) {
    let model = match omegon_git::RepoModel::discover_with_cancel(&repo_path, &|| {
        state.stopping.load(Ordering::Acquire)
    }) {
        Ok(Some(model)) => {
            let _ = startup.send(Ok(()));
            model
        }
        Ok(None) => {
            let _ = startup.send(Err("selected project is not a Git repository".into()));
            settle_worker(&state);
            return;
        }
        Err(error) => {
            let _ = startup.send(Err(error.to_string()));
            settle_worker(&state);
            return;
        }
    };
    let mut approved_workspaces = HashSet::new();
    while let Some(command) = receiver.blocking_recv() {
        if state.stopping.load(Ordering::Acquire) {
            break;
        }
        let caller = command.request.cancellation().clone();
        let generation = command.generation_cancellation.clone();
        let cancelled = || {
            state.stopping.load(Ordering::Acquire)
                || caller.is_cancelled()
                || generation.is_cancelled()
        };
        let result = if cancelled() {
            Err(GitServiceError::cancelled())
        } else {
            execute_request(
                &model,
                &mut approved_workspaces,
                command.request,
                &cancelled,
            )
        };
        let _ = command.response.send(result);
    }
    drop(model);
    settle_worker(&state);
}

fn settle_worker(state: &WorkerState) {
    state.process_settled.store(true, Ordering::Release);
    state.writer_settled.store(true, Ordering::Release);
    state.changed.notify_waiters();
}

fn normalized_path(root: &Path, path: &Path) -> PathBuf {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    if path.exists() {
        std::fs::canonicalize(&path).unwrap_or(path)
    } else if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
        std::fs::canonicalize(parent)
            .map(|parent| parent.join(name))
            .unwrap_or(path)
    } else {
        path
    }
}

fn require_path(
    model: &omegon_git::RepoModel,
    approved_workspaces: &HashSet<PathBuf>,
    path: &Path,
) -> Result<PathBuf, GitServiceError> {
    let root = model.repo_path();
    let normalized_root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let normalized_path = normalized_path(root, path);
    let allowed = normalized_path.starts_with(&normalized_root)
        || approved_workspaces
            .iter()
            .any(|workspace| normalized_path.starts_with(workspace));
    if allowed {
        Ok(normalized_path)
    } else {
        Err(GitServiceError::outside(path))
    }
}

fn snapshot(model: &omegon_git::RepoModel) -> GitRepositorySnapshot {
    let mut submodules = model
        .submodules()
        .into_iter()
        .map(|entry| entry.path)
        .collect::<Vec<_>>();
    submodules.sort();
    let mut pending_lifecycle = model
        .pending_lifecycle_files()
        .into_iter()
        .collect::<Vec<_>>();
    pending_lifecycle.sort();
    GitRepositorySnapshot {
        root: model.repo_path().to_path_buf(),
        branch: model.branch(),
        head_sha: model.head_sha(),
        jj_change_id: model.jj_change_id(),
        is_jj: model.is_jj(),
        submodules,
        pending_lifecycle,
    }
}

fn execute_request(
    model: &omegon_git::RepoModel,
    approved_workspaces: &mut HashSet<PathBuf>,
    request: GitRequest,
    cancelled: &impl Fn() -> bool,
) -> Result<GitResponse, GitServiceError> {
    if cancelled() {
        return Err(GitServiceError::cancelled());
    }
    let response = match request {
        GitRequest::Snapshot { .. } => GitResponse::Snapshot(snapshot(model)),
        GitRequest::RecordEdits { paths, .. } => {
            for path in paths {
                model.record_edit(&path);
            }
            GitResponse::Recorded
        }
        GitRequest::AuthorizeWorkspace { path, .. } => {
            approved_workspaces.insert(normalized_path(model.repo_path(), &path));
            GitResponse::Recorded
        }
        GitRequest::Status { path, .. } => {
            let status =
                omegon_git::status::query_status(&require_path(model, approved_workspaces, &path)?)
                    .map_err(GitServiceError::operation)?;
            GitResponse::Status(GitStatusSnapshot {
                is_clean: status.is_clean,
                entries: status
                    .entries
                    .into_iter()
                    .map(|entry| GitStatusEntry {
                        path: entry.path,
                        is_submodule: entry.is_submodule,
                        kind: match entry.status {
                            omegon_git::status::FileStatus::Modified => GitStatusKind::Modified,
                            omegon_git::status::FileStatus::Staged => GitStatusKind::Staged,
                            omegon_git::status::FileStatus::StagedAndModified => {
                                GitStatusKind::StagedAndModified
                            }
                            omegon_git::status::FileStatus::Untracked => GitStatusKind::Untracked,
                            omegon_git::status::FileStatus::Deleted => GitStatusKind::Deleted,
                            omegon_git::status::FileStatus::Renamed => GitStatusKind::Renamed,
                            omegon_git::status::FileStatus::SubmoduleModified => {
                                GitStatusKind::SubmoduleModified
                            }
                        },
                    })
                    .collect(),
            })
        }
        GitRequest::Commit {
            path,
            message,
            paths,
            ..
        } => {
            let path = require_path(model, approved_workspaces, &path)?;
            if model.is_jj() && path == model.repo_path() {
                omegon_git::jj::describe_with_cancel(&path, &message, cancelled)
                    .map_err(GitServiceError::operation)?;
                if cancelled() {
                    return Err(GitServiceError::cancelled());
                }
                omegon_git::jj::new_change_with_cancel(&path, "", cancelled)
                    .map_err(GitServiceError::operation)?;
                omegon_git::jj::sync_to_git_main_with_cancel(&path, cancelled)
                    .map_err(GitServiceError::operation)?;
                model.clear_working_set();
                model
                    .refresh_with_cancel(cancelled)
                    .map_err(GitServiceError::operation)?;
                let current =
                    omegon_git::jj::working_copy_parent_change_id_with_cancel(&path, cancelled)
                        .map_err(GitServiceError::operation)?
                        .unwrap_or_default();
                GitResponse::Commit {
                    revision: current,
                    files_staged: 0,
                    submodule_commits: 0,
                    backend: "jj",
                    branch: model.branch(),
                }
            } else {
                let lifecycle_paths = model
                    .pending_lifecycle_files()
                    .into_iter()
                    .collect::<Vec<_>>();
                let submodules = omegon_git::submodule::list_submodule_paths(&path)
                    .map_err(GitServiceError::operation)?;
                let mut submodule_commits = 0;
                for submodule in submodules {
                    let prefix = format!("{submodule}/");
                    if (paths.is_empty() || paths.iter().any(|item| item.starts_with(&prefix)))
                        && !cancelled()
                    {
                        submodule_commits += omegon_git::commit::commit_in_submodule_with_cancel(
                            &path, &submodule, &message, cancelled,
                        )
                        .unwrap_or_default();
                    }
                }
                if cancelled() {
                    return Err(GitServiceError::cancelled());
                }
                let result = omegon_git::commit::create_commit(
                    &path,
                    &omegon_git::commit::CommitOptions {
                        message: &message,
                        paths: &paths,
                        include_lifecycle: !lifecycle_paths.is_empty(),
                        lifecycle_paths: &lifecycle_paths,
                    },
                )
                .map_err(GitServiceError::operation)?;
                model.clear_working_set();
                model
                    .refresh_with_cancel(cancelled)
                    .map_err(GitServiceError::operation)?;
                GitResponse::Commit {
                    revision: result.sha,
                    files_staged: result.files_staged,
                    submodule_commits,
                    backend: "git",
                    branch: model.branch(),
                }
            }
        }
        GitRequest::CreateWorktree {
            workspace_path,
            name,
            branch,
            mode,
            ..
        } => {
            let workspace_path = normalized_path(model.repo_path(), &workspace_path);
            let info = match mode {
                GitWorktreeMode::Git => {
                    omegon_git::worktree::create(model.repo_path(), &workspace_path, &branch)
                }
                GitWorktreeMode::Smart => omegon_git::worktree::create_smart_with_cancel(
                    model.repo_path(),
                    &workspace_path,
                    &name,
                    &branch,
                    cancelled,
                ),
            }
            .map_err(GitServiceError::operation)?;
            approved_workspaces.insert(workspace_path.clone());
            GitResponse::Worktree(GitWorktreeInfo {
                path: info.path,
                branch: info.branch,
                backend: info.backend,
            })
        }
        GitRequest::RemoveWorktree {
            workspace_path,
            name,
            mode,
            ..
        } => {
            let workspace_path = normalized_path(model.repo_path(), &workspace_path);
            match mode {
                GitWorktreeMode::Git => {
                    omegon_git::worktree::remove(model.repo_path(), &workspace_path)
                }
                GitWorktreeMode::Smart => omegon_git::worktree::remove_smart_with_cancel(
                    model.repo_path(),
                    &name,
                    &workspace_path,
                    cancelled,
                ),
            }
            .map_err(GitServiceError::operation)?;
            approved_workspaces.remove(&workspace_path);
            GitResponse::Removed
        }
        GitRequest::DeleteBranch { branch, .. } => {
            omegon_git::worktree::delete_branch(model.repo_path(), &branch)
                .map_err(GitServiceError::operation)?;
            GitResponse::Removed
        }
        GitRequest::Merge {
            branch,
            message,
            squash,
            ..
        } => {
            let result = if squash {
                omegon_git::merge::squash_merge(model.repo_path(), &branch, &message)
            } else {
                omegon_git::merge::merge_no_ff(model.repo_path(), &branch, &message)
            }
            .map_err(GitServiceError::operation)?;
            model
                .refresh_with_cancel(cancelled)
                .map_err(GitServiceError::operation)?;
            GitResponse::Merge(match result {
                omegon_git::merge::MergeResult::Success { sha } => {
                    GitMergeOutcome::Success { revision: sha }
                }
                omegon_git::merge::MergeResult::NoChanges => GitMergeOutcome::NoChanges,
                omegon_git::merge::MergeResult::Conflict { files } => {
                    GitMergeOutcome::Conflict { files }
                }
                omegon_git::merge::MergeResult::Failed(error) => GitMergeOutcome::Failed(error),
            })
        }
        GitRequest::ListSubmodules { path, .. } => GitResponse::Submodules(
            omegon_git::submodule::list_submodule_paths(&require_path(
                model,
                approved_workspaces,
                &path,
            )?)
            .map_err(GitServiceError::operation)?,
        ),
        GitRequest::InitSubmodules { path, .. } => GitResponse::SubmodulesInitialized(
            omegon_git::submodule::init_submodules_with_cancel(
                &require_path(model, approved_workspaces, &path)?,
                cancelled,
            )
            .map_err(GitServiceError::operation)?,
        ),
        GitRequest::CommitDirtySubmodules { path, label, .. } => {
            let path = require_path(model, approved_workspaces, &path)?;
            let submodules = omegon_git::submodule::list_submodule_paths(&path)
                .map_err(GitServiceError::operation)?;
            let mut committed = 0;
            let mut pointers = Vec::new();
            for submodule in submodules {
                if cancelled() {
                    return Err(GitServiceError::cancelled());
                }
                let message = format!("feat({label}): auto-commit from cleave child");
                let count = omegon_git::commit::commit_in_submodule_with_cancel(
                    &path, &submodule, &message, cancelled,
                )
                .map_err(GitServiceError::operation)?;
                if count > 0 {
                    committed += 1;
                    pointers.push(submodule);
                }
            }
            if !pointers.is_empty() {
                let message = format!("chore({label}): update submodule pointer(s)");
                omegon_git::commit::create_commit(
                    &path,
                    &omegon_git::commit::CommitOptions {
                        message: &message,
                        paths: &pointers,
                        include_lifecycle: false,
                        lifecycle_paths: &[],
                    },
                )
                .map_err(GitServiceError::operation)?;
            }
            GitResponse::DirtySubmodulesCommitted(committed)
        }
    };
    if cancelled() {
        Err(GitServiceError::cancelled())
    } else {
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::{IndexAddOption, Repository, Signature};

    fn repository() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let mut config = repo.config().unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
        config.set_str("user.name", "Test").unwrap();
        drop(config);
        std::fs::write(dir.path().join("initial.txt"), "initial").unwrap();
        let mut index = repo.index().unwrap();
        index
            .add_all(["."].iter(), IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let signature = Signature::now("Test", "test@example.com").unwrap();
        repo.commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])
            .unwrap();
        drop(tree);
        drop(repo);
        dir
    }

    async fn published(path: PathBuf) -> (crate::bus::EventBus, GitBinding) {
        let binding = GitBinding::default();
        let mut bus = crate::bus::EventBus::new();
        bus.register(Box::new(GitFeature));
        bus.stage_managed_generation("git", start_candidate(path).await.unwrap())
            .unwrap();
        bus.try_finalize_managed().await.unwrap();
        binding.capture(&bus).unwrap();
        (bus, binding)
    }

    #[tokio::test]
    async fn absence_is_typed_and_does_not_discover() {
        let error = GitBinding::default()
            .invoke(GitRequest::Snapshot {
                cancellation: CancellationToken::new(),
            })
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            ManagedServiceCallError::Operation(GitServiceError {
                code: GitServiceErrorCode::Unavailable,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn exact_generation_commits_and_stale_handle_is_denied() {
        let dir = repository();
        let (mut bus, binding) = published(dir.path().to_path_buf()).await;
        let handle = binding.handle().unwrap();
        assert_eq!(handle.generation_id.as_str(), GIT_GENERATION);
        std::fs::write(dir.path().join("change.txt"), "change").unwrap();
        let response = binding
            .invoke(GitRequest::Commit {
                path: dir.path().to_path_buf(),
                message: "test: managed commit".into(),
                paths: vec!["change.txt".into()],
                cancellation: CancellationToken::new(),
            })
            .await
            .unwrap();
        assert!(matches!(
            response,
            GitResponse::Commit { backend: "git", .. }
        ));
        let report = bus.shutdown_managed_services().await;
        assert!(report.all_resources_settled(), "{report:?}");
        let error = handle
            .invoke(GitRequest::Snapshot {
                cancellation: CancellationToken::new(),
            })
            .await
            .unwrap_err();
        assert!(matches!(error, ManagedServiceCallError::GenerationRetired));
    }

    #[tokio::test]
    async fn candidate_failure_preserves_previous_generation() {
        let dir = repository();
        let (mut bus, binding) = published(dir.path().to_path_buf()).await;
        bus.stage_managed_generation(
            "git",
            start_candidate(dir.path().to_path_buf()).await.unwrap(),
        )
        .unwrap();
        assert!(bus.try_finalize_managed().await.is_err());
        let response = binding
            .invoke(GitRequest::Snapshot {
                cancellation: CancellationToken::new(),
            })
            .await
            .unwrap();
        assert!(matches!(response, GitResponse::Snapshot(_)));
        assert!(
            bus.shutdown_managed_services()
                .await
                .all_resources_settled()
        );
    }

    #[tokio::test]
    async fn unchanged_generation_transfers_exact_owner() {
        let dir = repository();
        let (first, transferred) = exact_transfer_candidates(dir.path().to_path_buf()).await;
        let mut bus = crate::bus::EventBus::new();
        bus.register(Box::new(GitFeature));
        bus.stage_managed_generation("git", first).unwrap();
        bus.try_finalize_managed().await.unwrap();
        let original = bus
            .managed_service::<GitService>(&git_capability_id(), &git_interface_id())
            .unwrap()
            .unwrap();
        bus.stage_managed_generation("git", transferred).unwrap();
        bus.try_finalize_managed().await.unwrap();
        let response = original
            .invoke(GitRequest::Snapshot {
                cancellation: CancellationToken::new(),
            })
            .await
            .unwrap();
        assert!(matches!(response, GitResponse::Snapshot(_)));
        assert!(
            bus.shutdown_managed_services()
                .await
                .all_resources_settled()
        );
    }

    #[tokio::test]
    async fn cancellation_and_path_containment_fail_without_effect() {
        let dir = repository();
        let (mut bus, binding) = published(dir.path().to_path_buf()).await;
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        let error = binding
            .invoke(GitRequest::RecordEdits {
                paths: vec!["cancelled.txt".into()],
                cancellation,
            })
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            ManagedServiceCallError::Operation(GitServiceError {
                code: GitServiceErrorCode::Cancelled,
                ..
            })
        ));
        let outside = tempfile::tempdir().unwrap();
        let error = binding
            .invoke(GitRequest::Status {
                path: outside.path().to_path_buf(),
                cancellation: CancellationToken::new(),
            })
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            ManagedServiceCallError::Operation(GitServiceError {
                code: GitServiceErrorCode::OutsideRepository,
                ..
            })
        ));
        assert!(
            bus.shutdown_managed_services()
                .await
                .all_resources_settled()
        );
    }

    #[tokio::test]
    async fn concurrent_reads_serialize_on_one_repository_owner() {
        let dir = repository();
        let (mut bus, binding) = published(dir.path().to_path_buf()).await;
        let first = binding.invoke(GitRequest::Status {
            path: dir.path().to_path_buf(),
            cancellation: CancellationToken::new(),
        });
        let second = binding.invoke(GitRequest::Snapshot {
            cancellation: CancellationToken::new(),
        });
        let (first, second) = tokio::join!(first, second);
        assert!(matches!(first.unwrap(), GitResponse::Status(_)));
        assert!(matches!(second.unwrap(), GitResponse::Snapshot(_)));
        assert!(
            bus.shutdown_managed_services()
                .await
                .all_resources_settled()
        );
    }

    #[test]
    fn production_consumers_cannot_bypass_managed_git_owner() {
        for (name, source) in [
            ("setup.rs", include_str!("setup.rs")),
            ("tools/mod.rs", include_str!("tools/mod.rs")),
            ("main.rs", include_str!("main.rs")),
            ("workspace/control.rs", include_str!("workspace/control.rs")),
            ("cleave/worktree.rs", include_str!("cleave/worktree.rs")),
            (
                "cleave/orchestrator.rs",
                include_str!("cleave/orchestrator.rs"),
            ),
        ] {
            let production = source.split("#[cfg(test)]").next().unwrap_or(source);
            assert!(
                !production.contains("omegon_git::"),
                "{name} bypasses the managed Git owner"
            );
        }
    }
}
