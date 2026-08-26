//! Generation-owned codescan integration.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use async_trait::async_trait;
use omegon_codescan::{BM25Index, IndexStats, Indexer, ScanCache, SearchChunk, SearchScope};
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

pub(crate) const CODESCAN_CAPABILITY: &str = "service:codescan";
pub(crate) const CODESCAN_INTERFACE: &str = "interface:omegon-codescan-v1";
pub(crate) const CODESCAN_GENERATION: &str = "contribution:codescan-managed-v1";
const WORKER_RESOURCE: &str = "resource:codescan-worker";
const WRITER_RESOURCE: &str = "resource:codescan-writer";
const QUEUE_CAPACITY: usize = 16;

pub(crate) fn codescan_capability_id() -> RuntimeCapabilityId {
    RuntimeCapabilityId::new(CODESCAN_CAPABILITY).expect("static capability id is valid")
}

pub(crate) fn codescan_interface_id() -> RuntimeServiceInterfaceId {
    RuntimeServiceInterfaceId::new(CODESCAN_INTERFACE).expect("static interface id is valid")
}

#[derive(Clone, Default)]
pub(crate) struct CodescanBinding {
    handle: Arc<OnceLock<Option<ManagedServiceHandle<CodescanService>>>>,
}

impl CodescanBinding {
    pub(crate) fn capture(&self, bus: &crate::bus::EventBus) -> anyhow::Result<()> {
        let handle = bus.managed_service::<CodescanService>(
            &codescan_capability_id(),
            &codescan_interface_id(),
        )?;
        self.handle
            .set(handle)
            .map_err(|_| anyhow::anyhow!("codescan managed handle was already captured"))
    }

    pub(crate) fn handle(&self) -> Option<ManagedServiceHandle<CodescanService>> {
        self.handle.get().and_then(Clone::clone)
    }
}

pub(crate) struct CodescanFeature {
    provider: crate::tools::codebase_search::CodescanProvider,
}

impl CodescanFeature {
    pub(crate) fn new(repo_path: PathBuf, binding: CodescanBinding) -> Self {
        Self {
            provider: crate::tools::codebase_search::CodescanProvider::new(repo_path, binding),
        }
    }
}

#[async_trait]
impl Feature for CodescanFeature {
    fn name(&self) -> &str {
        "codescan"
    }

    fn tools(&self) -> Vec<omegon_traits::ToolDefinition> {
        omegon_traits::ToolProvider::tools(&self.provider)
    }

    async fn execute(
        &self,
        tool_name: &str,
        call_id: &str,
        args: serde_json::Value,
        cancel: CancellationToken,
    ) -> anyhow::Result<omegon_traits::ToolResult> {
        omegon_traits::ToolProvider::execute(&self.provider, tool_name, call_id, args, cancel).await
    }

    fn runtime_contribution_generation_id(&self) -> Option<RuntimeContributionGenerationId> {
        Some(
            RuntimeContributionGenerationId::new(CODESCAN_GENERATION)
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

pub(crate) enum CodescanRequest {
    Search {
        query: String,
        scope: SearchScope,
        max_results: usize,
        tags: Vec<String>,
        within: Option<PathBuf>,
        cancellation: CancellationToken,
    },
    Index {
        invalidate: bool,
        cancellation: CancellationToken,
    },
    CodeContext {
        query: String,
        max_results: usize,
        cancellation: CancellationToken,
    },
    #[cfg(test)]
    TestBlock {
        started: std::sync::mpsc::SyncSender<()>,
        release: Arc<(Mutex<bool>, std::sync::Condvar)>,
        cancellation: CancellationToken,
    },
    #[cfg(test)]
    TestRecord {
        executions: Arc<std::sync::atomic::AtomicUsize>,
        cancellation: CancellationToken,
    },
    #[cfg(test)]
    TestActiveCancellation {
        started: std::sync::mpsc::SyncSender<()>,
        finished: std::sync::mpsc::SyncSender<()>,
        cancellation: CancellationToken,
    },
}

impl CodescanRequest {
    fn cancellation(&self) -> &CancellationToken {
        match self {
            Self::Search { cancellation, .. }
            | Self::Index { cancellation, .. }
            | Self::CodeContext { cancellation, .. } => cancellation,
            #[cfg(test)]
            Self::TestBlock { cancellation, .. }
            | Self::TestRecord { cancellation, .. }
            | Self::TestActiveCancellation { cancellation, .. } => cancellation,
        }
    }
}

pub(crate) enum CodescanResponse {
    Search {
        results: Vec<SearchChunk>,
        indexed_code_chunks: usize,
        indexed_knowledge_chunks: usize,
    },
    Index(IndexStats),
    CodeContext(Vec<SearchChunk>),
}

pub(crate) struct CodescanService {
    commands: mpsc::Sender<WorkerCommand>,
}

struct WorkerCommand {
    request: CodescanRequest,
    generation_cancellation: CancellationToken,
    response: oneshot::Sender<Result<CodescanResponse, String>>,
}

impl ManagedServiceContract for CodescanService {
    type Request = CodescanRequest;
    type Response = CodescanResponse;
    type Error = String;

    fn execute<'a>(
        &'a self,
        request: Self::Request,
        context: ManagedCallContext,
    ) -> ManagedServiceFuture<'a, Self::Response, Self::Error> {
        Box::pin(async move {
            let caller_cancellation = request.cancellation().clone();
            if caller_cancellation.is_cancelled() || context.cancellation.is_cancelled() {
                return Err("codebase request cancelled".into());
            }
            let (response, receive) = oneshot::channel();
            let command = WorkerCommand {
                request,
                generation_cancellation: context.cancellation.clone(),
                response,
            };
            tokio::select! {
                biased;
                () = caller_cancellation.cancelled() => return Err("codebase request cancelled".into()),
                () = context.cancellation.cancelled() => return Err("codebase request cancelled".into()),
                sent = self.commands.send(command) => sent.map_err(|_| "codescan worker is unavailable".to_string())?,
            }
            tokio::select! {
                biased;
                () = caller_cancellation.cancelled() => Err("codebase request cancelled".into()),
                () = context.cancellation.cancelled() => Err("codebase request cancelled".into()),
                result = receive => result.map_err(|_| "codescan worker dropped its response".to_string())?,
            }
        })
    }
}

struct WorkerState {
    stopping: AtomicBool,
    connection_closed: AtomicBool,
    worker_joined: AtomicBool,
    changed: Notify,
    join: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl WorkerState {
    fn request_stop(&self) {
        self.stopping.store(true, Ordering::Release);
    }

    fn wake(commands: &mpsc::Sender<WorkerCommand>) {
        // A cancelled sentinel request is enough to wake an idle blocking receiver.
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        let (response, _) = oneshot::channel();
        let _ = commands.try_send(WorkerCommand {
            request: CodescanRequest::Index {
                invalidate: false,
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
                    .map_err(|_| "codescan worker join lock poisoned".to_string())?
                    .take();
                if let Some(join) = join {
                    let join_result = tokio::task::spawn_blocking(move || join.join())
                        .await
                        .map_err(|error| format!("codescan worker join task failed: {error}"))?;
                    state.worker_joined.store(true, Ordering::Release);
                    state.changed.notify_waiters();
                    if join_result.is_err() {
                        tracing::error!("codescan worker terminated after a panic");
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
                if state.connection_closed.load(Ordering::Acquire)
                    && state.worker_joined.load(Ordering::Acquire)
                {
                    return Ok(());
                }
                let changed = state.changed.notified();
                if state.connection_closed.load(Ordering::Acquire)
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
    let (commands, receiver) = mpsc::channel(QUEUE_CAPACITY);
    let state = Arc::new(WorkerState {
        stopping: AtomicBool::new(false),
        connection_closed: AtomicBool::new(false),
        worker_joined: AtomicBool::new(false),
        changed: Notify::new(),
        join: Mutex::new(None),
    });
    let (startup, started) = std::sync::mpsc::sync_channel(1);
    let worker_state = Arc::clone(&state);
    let join = std::thread::Builder::new()
        .name("omegon-codescan".into())
        .spawn(move || run_worker(repo_path, receiver, worker_state, startup))?;
    *state
        .join
        .lock()
        .map_err(|_| anyhow::anyhow!("codescan worker join lock poisoned"))? = Some(join);
    let startup_result = tokio::task::spawn_blocking(move || started.recv())
        .await
        .map_err(|error| anyhow::anyhow!("codescan readiness task failed: {error}"))?
        .map_err(|_| anyhow::anyhow!("codescan worker exited before reporting SQLite readiness"))?;
    if let Err(error) = startup_result {
        if let Some(join) = state.join.lock().ok().and_then(|mut join| join.take()) {
            let _ = tokio::task::spawn_blocking(move || join.join()).await;
        }
        anyhow::bail!(error);
    }

    let worker_controller: Arc<dyn ManagedResourceController> = Arc::new(WorkerController {
        state: Arc::clone(&state),
        commands: commands.clone(),
    });
    let writer_controller: Arc<dyn ManagedResourceController> = Arc::new(WriterController {
        state: Arc::clone(&state),
        commands: commands.clone(),
    });
    let writer_id =
        RuntimeContributionResourceId::new(WRITER_RESOURCE).expect("static resource id is valid");
    let resources = vec![
        ManagedResourceRegistration::new(
            writer_id.clone(),
            RuntimeOwnedResourceKind::DurableWriter,
            RuntimeCleanupAssurance::Strict,
            Vec::new(),
            writer_controller,
        ),
        ManagedResourceRegistration::new(
            RuntimeContributionResourceId::new(WORKER_RESOURCE)
                .expect("static resource id is valid"),
            RuntimeOwnedResourceKind::Task,
            RuntimeCleanupAssurance::Strict,
            vec![writer_id],
            worker_controller,
        ),
    ];
    let mut candidate = ManagedGenerationCandidate::new(
        RuntimeCompositionGenerationId::new("composition:codescan-boot")
            .expect("static composition id is valid"),
        omegon_traits::RuntimeContributionId::new("feature:codescan")
            .expect("static contribution id is valid"),
        RuntimeContributionGenerationId::new(CODESCAN_GENERATION)
            .expect("static generation id is valid"),
        Duration::from_secs(30),
        Duration::from_secs(5),
        resources,
    )?;
    candidate.add_service(
        codescan_capability_id(),
        codescan_interface_id(),
        Arc::new(CodescanService { commands }),
    )?;
    Ok(candidate)
}

fn run_worker(
    repo_path: PathBuf,
    mut receiver: mpsc::Receiver<WorkerCommand>,
    state: Arc<WorkerState>,
    startup: std::sync::mpsc::SyncSender<Result<(), String>>,
) {
    struct ConnectionClosure(Arc<WorkerState>);

    impl Drop for ConnectionClosure {
        fn drop(&mut self) {
            self.0.connection_closed.store(true, Ordering::Release);
            self.0.changed.notify_waiters();
        }
    }

    let _connection_closure = ConnectionClosure(Arc::clone(&state));
    let db_path = repo_path.join(".omegon/codescan.db");
    let mut cache = match ScanCache::open(&db_path) {
        Ok(cache) => {
            let _ = startup.send(Ok(()));
            cache
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
            let _ = command
                .response
                .send(Err("codebase request cancelled".into()));
            continue;
        }
        let result = execute_request(&repo_path, &mut cache, command.request, || {
            state.stopping.load(Ordering::Acquire)
                || caller.is_cancelled()
                || generation.is_cancelled()
        })
        .map_err(|error| error.to_string());
        let _ = command.response.send(result);
    }

    drop(cache);
}

fn execute_request(
    repo_path: &Path,
    cache: &mut ScanCache,
    request: CodescanRequest,
    is_cancelled: impl Fn() -> bool,
) -> anyhow::Result<CodescanResponse> {
    match request {
        CodescanRequest::Search {
            query,
            scope,
            max_results,
            tags,
            within,
            ..
        } => {
            Indexer::run_with_cancel(repo_path, cache, &is_cancelled)?;
            let code_chunks = cache
                .all_code_chunks()?
                .into_iter()
                .filter(|chunk| path_in_within(&chunk.path, within.as_deref()))
                .collect::<Vec<_>>();
            let mut knowledge_chunks = cache
                .all_knowledge_chunks()?
                .into_iter()
                .filter(|chunk| path_in_within(&chunk.path, within.as_deref()))
                .collect::<Vec<_>>();
            if !tags.is_empty() {
                knowledge_chunks.retain(|chunk| tags.iter().any(|tag| chunk.tags.contains(tag)));
            }
            let indexed_code_chunks = code_chunks.len();
            let indexed_knowledge_chunks = knowledge_chunks.len();
            let results =
                BM25Index::build_with_cancel(&code_chunks, &knowledge_chunks, &is_cancelled)?
                    .search_with_cancel(&query, scope, max_results, is_cancelled)?;
            Ok(CodescanResponse::Search {
                results,
                indexed_code_chunks,
                indexed_knowledge_chunks,
            })
        }
        CodescanRequest::Index { invalidate, .. } => {
            if invalidate {
                cache.begin_full_rebuild()?;
            }
            Indexer::run_with_cancel(repo_path, cache, is_cancelled).map(CodescanResponse::Index)
        }
        CodescanRequest::CodeContext {
            query, max_results, ..
        } => {
            Indexer::run_with_cancel(repo_path, cache, &is_cancelled)?;
            let code_chunks = cache.all_code_chunks()?;
            let knowledge_chunks = cache.all_knowledge_chunks()?;
            let results =
                BM25Index::build_with_cancel(&code_chunks, &knowledge_chunks, &is_cancelled)?
                    .search_with_cancel(&query, SearchScope::Code, max_results, is_cancelled)?;
            Ok(CodescanResponse::CodeContext(results))
        }
        #[cfg(test)]
        CodescanRequest::TestBlock {
            started, release, ..
        } => {
            let _ = started.send(());
            let (released, changed) = &*release;
            let mut released = released
                .lock()
                .map_err(|_| anyhow::anyhow!("test release lock poisoned"))?;
            while !*released && !is_cancelled() {
                let (next, _) = changed
                    .wait_timeout(released, Duration::from_millis(5))
                    .map_err(|_| anyhow::anyhow!("test release wait poisoned"))?;
                released = next;
            }
            if is_cancelled() {
                anyhow::bail!("codebase request cancelled");
            }
            Ok(CodescanResponse::Index(IndexStats {
                code_files: 0,
                knowledge_files: 0,
                code_chunks: 0,
                knowledge_chunks: 0,
                duration_ms: 0,
            }))
        }
        #[cfg(test)]
        CodescanRequest::TestRecord { executions, .. } => {
            executions.fetch_add(1, Ordering::AcqRel);
            Ok(CodescanResponse::Index(IndexStats {
                code_files: 0,
                knowledge_files: 0,
                code_chunks: 0,
                knowledge_chunks: 0,
                duration_ms: 0,
            }))
        }
        #[cfg(test)]
        CodescanRequest::TestActiveCancellation {
            started, finished, ..
        } => {
            let _ = started.send(());
            while !is_cancelled() {
                std::thread::sleep(Duration::from_millis(1));
            }
            let _ = finished.send(());
            anyhow::bail!("codebase request cancelled")
        }
    }
}

fn path_in_within(path: &Path, within: Option<&Path>) -> bool {
    within.is_none_or(|within| path.starts_with(within))
}

pub(crate) fn unavailable_code(error: &ManagedServiceCallError<String>) -> &'static str {
    error.code()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    async fn managed_service(
        path: PathBuf,
    ) -> (crate::bus::EventBus, ManagedServiceHandle<CodescanService>) {
        let binding = CodescanBinding::default();
        let mut bus = crate::bus::EventBus::new();
        bus.register(Box::new(CodescanFeature::new(path.clone(), binding)));
        bus.stage_managed_generation("codescan", start_candidate(path).await.unwrap())
            .unwrap();
        bus.try_finalize_managed().await.unwrap();
        let handle = bus
            .managed_service::<CodescanService>(&codescan_capability_id(), &codescan_interface_id())
            .unwrap()
            .unwrap();
        (bus, handle)
    }

    #[tokio::test]
    async fn optional_absence_finalizes_and_captures_no_handle() {
        let dir = tempfile::tempdir().unwrap();
        let binding = CodescanBinding::default();
        let mut bus = crate::bus::EventBus::new();
        bus.register(Box::new(CodescanFeature::new(
            dir.path().to_path_buf(),
            binding.clone(),
        )));

        bus.try_finalize_managed().await.unwrap();
        binding.capture(&bus).unwrap();

        assert!(binding.handle().is_none());
        assert!(
            bus.tool_definitions()
                .iter()
                .any(|tool| tool.name == crate::tool_registry::codescan::CODEBASE_SEARCH)
        );
    }

    async fn wait_for_signal(receiver: std::sync::mpsc::Receiver<()>) {
        tokio::task::spawn_blocking(move || receiver.recv_timeout(Duration::from_secs(2)))
            .await
            .unwrap()
            .unwrap();
    }

    fn release_block(release: &Arc<(Mutex<bool>, std::sync::Condvar)>) {
        let (released, changed) = &**release;
        *released.lock().unwrap() = true;
        changed.notify_all();
    }

    #[test]
    fn production_codescan_ownership_is_confined_to_this_module() {
        fn visit(path: &Path, violations: &mut Vec<PathBuf>) {
            for entry in std::fs::read_dir(path).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    visit(&path, violations);
                    continue;
                }
                if path.extension().and_then(|extension| extension.to_str()) != Some("rs")
                    || path.file_name().and_then(|name| name.to_str())
                        == Some("codescan_service.rs")
                {
                    continue;
                }
                let source = std::fs::read_to_string(&path).unwrap();
                let production = source.split("#[cfg(test)]").next().unwrap_or(&source);
                if ["ScanCache::open", "Indexer::run", "BM25Index::build"]
                    .iter()
                    .any(|owned| production.contains(owned))
                {
                    violations.push(path);
                }
            }
        }

        let mut violations = Vec::new();
        visit(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut violations,
        );
        assert!(
            violations.is_empty(),
            "production codescan owners escaped the managed service: {violations:?}"
        );
    }

    #[tokio::test]
    async fn serial_worker_does_not_execute_a_second_request_concurrently() {
        let dir = tempfile::tempdir().unwrap();
        let (mut bus, handle) = managed_service(dir.path().to_path_buf()).await;
        let release = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
        let (started, started_rx) = std::sync::mpsc::sync_channel(1);
        let first = tokio::spawn({
            let handle = handle.clone();
            let release = Arc::clone(&release);
            async move {
                handle
                    .invoke(CodescanRequest::TestBlock {
                        started,
                        release,
                        cancellation: CancellationToken::new(),
                    })
                    .await
            }
        });
        wait_for_signal(started_rx).await;

        let executions = Arc::new(AtomicUsize::new(0));
        let second = tokio::spawn({
            let handle = handle.clone();
            let executions = Arc::clone(&executions);
            async move {
                handle
                    .invoke(CodescanRequest::TestRecord {
                        executions,
                        cancellation: CancellationToken::new(),
                    })
                    .await
            }
        });
        tokio::task::yield_now().await;
        assert_eq!(executions.load(Ordering::Acquire), 0);

        release_block(&release);
        first.await.unwrap().unwrap();
        second.await.unwrap().unwrap();
        assert_eq!(executions.load(Ordering::Acquire), 1);
        assert!(
            bus.shutdown_managed_services()
                .await
                .all_resources_settled()
        );
    }

    #[tokio::test]
    async fn queued_caller_cancellation_never_executes_the_request() {
        let dir = tempfile::tempdir().unwrap();
        let (mut bus, handle) = managed_service(dir.path().to_path_buf()).await;
        let release = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
        let (started, started_rx) = std::sync::mpsc::sync_channel(1);
        let first = tokio::spawn({
            let handle = handle.clone();
            let release = Arc::clone(&release);
            async move {
                handle
                    .invoke(CodescanRequest::TestBlock {
                        started,
                        release,
                        cancellation: CancellationToken::new(),
                    })
                    .await
            }
        });
        wait_for_signal(started_rx).await;

        let cancellation = CancellationToken::new();
        let executions = Arc::new(AtomicUsize::new(0));
        let queued = tokio::spawn({
            let handle = handle.clone();
            let cancellation = cancellation.clone();
            let executions = Arc::clone(&executions);
            async move {
                handle
                    .invoke(CodescanRequest::TestRecord {
                        executions,
                        cancellation,
                    })
                    .await
            }
        });
        tokio::task::yield_now().await;
        cancellation.cancel();
        assert!(matches!(
            queued.await.unwrap(),
            Err(ManagedServiceCallError::Operation(_))
        ));
        release_block(&release);
        first.await.unwrap().unwrap();

        handle
            .invoke(CodescanRequest::TestRecord {
                executions: Arc::new(AtomicUsize::new(0)),
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
    async fn active_caller_cancellation_reaches_the_worker() {
        let dir = tempfile::tempdir().unwrap();
        let (mut bus, handle) = managed_service(dir.path().to_path_buf()).await;
        let cancellation = CancellationToken::new();
        let (started, started_rx) = std::sync::mpsc::sync_channel(1);
        let (finished, finished_rx) = std::sync::mpsc::sync_channel(1);
        let invocation = tokio::spawn({
            let cancellation = cancellation.clone();
            async move {
                handle
                    .invoke(CodescanRequest::TestActiveCancellation {
                        started,
                        finished,
                        cancellation,
                    })
                    .await
            }
        });
        wait_for_signal(started_rx).await;
        cancellation.cancel();
        assert!(matches!(
            invocation.await.unwrap(),
            Err(ManagedServiceCallError::Operation(_))
        ));
        wait_for_signal(finished_rx).await;
        assert!(
            bus.shutdown_managed_services()
                .await
                .all_resources_settled()
        );
    }

    #[tokio::test]
    async fn shutdown_joins_worker_closes_sqlite_and_stales_the_handle() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join(".omegon/codescan.db");
        let (mut bus, handle) = managed_service(dir.path().to_path_buf()).await;
        handle
            .invoke(CodescanRequest::Index {
                invalidate: false,
                cancellation: CancellationToken::new(),
            })
            .await
            .unwrap();

        let report = bus.shutdown_managed_services().await;
        assert!(report.all_resources_settled(), "{report:?}");
        assert!(matches!(
            handle
                .invoke(CodescanRequest::Index {
                    invalidate: false,
                    cancellation: CancellationToken::new(),
                })
                .await,
            Err(ManagedServiceCallError::GenerationRetired)
        ));

        let reopened = ScanCache::open(&db_path).unwrap();
        drop(reopened);
        let renamed = db_path.with_extension("reopen-check");
        std::fs::rename(&db_path, &renamed).unwrap();
        std::fs::rename(&renamed, &db_path).unwrap();
        for suffix in ["codescan.db-wal", "codescan.db-shm"] {
            let sidecar = db_path.with_file_name(suffix);
            if sidecar.exists() {
                std::fs::remove_file(sidecar).unwrap();
            }
        }
        std::fs::remove_file(&db_path).unwrap();
        assert!(!db_path.exists());
    }

    #[tokio::test]
    async fn rejected_real_candidate_rolls_back_and_closes_sqlite() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let mut bus = crate::bus::EventBus::new();
        bus.register(Box::new(CodescanFeature::new(
            path.clone(),
            CodescanBinding::default(),
        )));
        bus.register(Box::new(CodescanFeature::new(
            path.clone(),
            CodescanBinding::default(),
        )));
        bus.stage_managed_generation("codescan", start_candidate(path.clone()).await.unwrap())
            .unwrap();

        assert!(bus.try_finalize_managed().await.is_err());
        let db_path = path.join(".omegon/codescan.db");
        let reopened = ScanCache::open(&db_path).unwrap();
        drop(reopened);
        std::fs::remove_file(&db_path).unwrap();
    }
}
