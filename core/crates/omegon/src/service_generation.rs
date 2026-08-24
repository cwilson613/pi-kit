//! Generation-gated invocation for resource-bearing in-process services.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use futures_util::FutureExt;
use omegon_traits::{
    ManagedCallContext, ManagedResourceController, ManagedServiceCallError, ManagedServiceContract,
    ManagedServiceGenerationState, RUNTIME_CONTRIBUTION_SCHEMA_VERSION, RuntimeCapabilityId,
    RuntimeCleanupAssurance, RuntimeCleanupState, RuntimeCompositionGenerationId,
    RuntimeContributionGenerationId, RuntimeContributionId, RuntimeContributionResourceId,
    RuntimeOwnedResourceKind, RuntimeOwnedResourceRecord,
};
use tokio::sync::{Notify, oneshot};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ManagedGenerationKey {
    pub(crate) owner: RuntimeContributionId,
    pub(crate) generation_id: RuntimeContributionGenerationId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ManagedCallDrainOutcome {
    Graceful,
    DeadlineForced,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ManagedGenerationCleanupReport {
    pub(crate) call_drain: ManagedCallDrainOutcome,
    pub(crate) resources: ManagedResourceCleanupReport,
}

pub(crate) struct ManagedGenerationRuntime {
    key: ManagedGenerationKey,
    cancellation: CancellationToken,
    accounting: Arc<CallAccounting>,
    tasks: Mutex<JoinSet<()>>,
    admission_closed: AtomicBool,
    cleanup_started: AtomicBool,
    cleanup_complete: AtomicBool,
    cleanup_error: Mutex<Option<String>>,
    cleanup_done: Notify,
    cleanup_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    generation_cleanup_started: AtomicBool,
    generation_cleanup_complete: AtomicBool,
    generation_cleanup_done: Notify,
    generation_cleanup_result: Mutex<Option<Result<ManagedGenerationCleanupReport, String>>>,
    generation_cleanup_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    resource_binding: Mutex<ManagedResourceBinding>,
    call_drain_outcome: Mutex<Option<ManagedCallDrainOutcome>>,
    generation_retry_started: AtomicBool,
    generation_retry_running: AtomicBool,
    generation_retry_done: Notify,
    generation_retry_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

#[derive(Default)]
struct ManagedResourceBinding {
    published: bool,
    cleanup_required: bool,
    resources: Option<Arc<ManagedResourceOwner>>,
}

impl ManagedGenerationRuntime {
    pub(crate) fn new(key: ManagedGenerationKey) -> Arc<Self> {
        Arc::new(Self {
            key,
            cancellation: CancellationToken::new(),
            accounting: Arc::new(CallAccounting {
                active_calls: AtomicUsize::new(0),
                idle: Notify::new(),
            }),
            tasks: Mutex::new(JoinSet::new()),
            admission_closed: AtomicBool::new(false),
            cleanup_started: AtomicBool::new(false),
            cleanup_complete: AtomicBool::new(false),
            cleanup_error: Mutex::new(None),
            cleanup_done: Notify::new(),
            cleanup_task: Mutex::new(None),
            generation_cleanup_started: AtomicBool::new(false),
            generation_cleanup_complete: AtomicBool::new(false),
            generation_cleanup_done: Notify::new(),
            generation_cleanup_result: Mutex::new(None),
            generation_cleanup_task: Mutex::new(None),
            resource_binding: Mutex::new(ManagedResourceBinding::default()),
            call_drain_outcome: Mutex::new(None),
            generation_retry_started: AtomicBool::new(false),
            generation_retry_running: AtomicBool::new(false),
            generation_retry_done: Notify::new(),
            generation_retry_task: Mutex::new(None),
        })
    }

    pub(crate) fn active_calls(&self) -> usize {
        self.accounting.active_calls.load(Ordering::Acquire)
    }

    fn retirement_ready(&self) -> bool {
        self.cleanup_complete.load(Ordering::Acquire)
            && self
                .cleanup_error
                .lock()
                .expect("managed call cleanup error lock poisoned")
                .is_none()
            && (!self
                .resource_binding
                .lock()
                .expect("managed resource binding lock poisoned")
                .cleanup_required
                || self
                    .generation_cleanup_result
                    .lock()
                    .expect("managed generation cleanup result lock poisoned")
                    .as_ref()
                    .is_some_and(|result| {
                        result
                            .as_ref()
                            .is_ok_and(|report| report.resources.strict_resources_settled())
                    }))
    }

    pub(crate) fn attach_resources(
        &self,
        resources: Arc<ManagedResourceOwner>,
    ) -> anyhow::Result<()> {
        resources.validate_and_freeze()?;
        if resources.key != self.key {
            anyhow::bail!("managed resource owner generation does not match runtime");
        }
        let mut binding = self
            .resource_binding
            .lock()
            .expect("managed resource binding lock poisoned");
        if binding.published {
            anyhow::bail!("managed resources must be attached before publication");
        }
        if binding.resources.is_some() {
            anyhow::bail!("managed generation resources are already attached");
        }
        binding.cleanup_required = true;
        binding.resources = Some(resources);
        Ok(())
    }

    fn mark_published(&self) {
        self.resource_binding
            .lock()
            .expect("managed resource binding lock poisoned")
            .published = true;
    }

    pub(crate) async fn abort_and_join_calls(
        self: &Arc<Self>,
    ) -> anyhow::Result<ManagedCallDrainOutcome> {
        self.drain_and_join_calls_until(tokio::time::Instant::now())
            .await
    }

    pub(crate) async fn drain_and_join_calls_until(
        self: &Arc<Self>,
        deadline: tokio::time::Instant,
    ) -> anyhow::Result<ManagedCallDrainOutcome> {
        if !self.admission_closed.load(Ordering::Acquire) {
            anyhow::bail!("managed generation admission is still open");
        }
        if self
            .cleanup_started
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            let mut cleanup_task = self
                .cleanup_task
                .lock()
                .expect("managed cleanup task lock poisoned");
            let runtime = Arc::clone(self);
            let task = tokio::spawn(async move {
                let operation = async {
                    let timed_out = loop {
                        if runtime.active_calls() == 0 {
                            break false;
                        }
                        let notified = runtime.accounting.idle.notified();
                        if runtime.active_calls() == 0 {
                            break false;
                        }
                        if tokio::time::timeout_at(deadline, notified).await.is_err() {
                            break true;
                        }
                    };
                    if timed_out {
                        runtime.cancellation.cancel();
                    }
                    let mut tasks = {
                        let mut owned = runtime
                            .tasks
                            .lock()
                            .expect("managed call task lock poisoned");
                        std::mem::take(&mut *owned)
                    };
                    if timed_out {
                        tasks.abort_all();
                    }
                    while tasks.join_next().await.is_some() {}
                    while runtime.active_calls() != 0 {
                        let notified = runtime.accounting.idle.notified();
                        if runtime.active_calls() == 0 {
                            break;
                        }
                        notified.await;
                    }
                    timed_out
                };
                match std::panic::AssertUnwindSafe(operation).catch_unwind().await {
                    Ok(timed_out) => {
                        *runtime
                            .call_drain_outcome
                            .lock()
                            .expect("managed call drain outcome lock poisoned") =
                            Some(if timed_out {
                                ManagedCallDrainOutcome::DeadlineForced
                            } else {
                                ManagedCallDrainOutcome::Graceful
                            });
                    }
                    Err(_) => {
                        *runtime
                            .cleanup_error
                            .lock()
                            .expect("managed call cleanup error lock poisoned") =
                            Some("managed call cleanup task panicked".into());
                    }
                }
                runtime.cleanup_complete.store(true, Ordering::Release);
                runtime.cleanup_done.notify_waiters();
            });
            *cleanup_task = Some(task);
        }

        while !self.cleanup_complete.load(Ordering::Acquire) {
            let notified = self.cleanup_done.notified();
            if self.cleanup_complete.load(Ordering::Acquire) {
                break;
            }
            notified.await;
        }
        let task = self
            .cleanup_task
            .lock()
            .expect("managed cleanup task lock poisoned")
            .take();
        if let Some(task) = task {
            let _ = task.await;
        }
        match self
            .cleanup_error
            .lock()
            .expect("managed call cleanup error lock poisoned")
            .clone()
        {
            Some(error) => Err(anyhow::Error::msg(error)),
            None => Ok(self
                .call_drain_outcome
                .lock()
                .expect("managed call drain outcome lock poisoned")
                .expect("managed call drain outcome must exist")),
        }
    }

    pub(crate) async fn cleanup_generation(
        self: &Arc<Self>,
        active_call_deadline: tokio::time::Instant,
        cleanup_budget: std::time::Duration,
    ) -> anyhow::Result<ManagedGenerationCleanupReport> {
        if self
            .generation_cleanup_started
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            anyhow::bail!("managed generation cleanup has already started");
        }
        let resources = self
            .resource_binding
            .lock()
            .expect("managed resource binding lock poisoned")
            .resources
            .as_ref()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("managed generation has no attached resources"))?;
        let runtime = Arc::clone(self);
        let task = tokio::spawn(async move {
            let operation = async {
                match runtime
                    .drain_and_join_calls_until(active_call_deadline)
                    .await
                {
                    Ok(call_drain) => resources
                        .cleanup_until(tokio::time::Instant::now() + cleanup_budget)
                        .await
                        .map(|resources| ManagedGenerationCleanupReport {
                            call_drain,
                            resources,
                        })
                        .map_err(|error| error.to_string()),
                    Err(error) => Err(error.to_string()),
                }
            };
            let result = match std::panic::AssertUnwindSafe(operation).catch_unwind().await {
                Ok(result) => result,
                Err(_) => Err("managed generation cleanup task panicked".into()),
            };
            *runtime
                .generation_cleanup_result
                .lock()
                .expect("managed generation cleanup result lock poisoned") = Some(result);
            let release_resources = runtime
                .generation_cleanup_result
                .lock()
                .expect("managed generation cleanup result lock poisoned")
                .as_ref()
                .is_some_and(|result| {
                    result
                        .as_ref()
                        .is_ok_and(|report| report.resources.strict_resources_settled())
                });
            if release_resources {
                runtime
                    .resource_binding
                    .lock()
                    .expect("managed resource binding lock poisoned")
                    .resources
                    .take();
            }
            runtime
                .generation_cleanup_complete
                .store(true, Ordering::Release);
            runtime.generation_cleanup_done.notify_waiters();
        });
        *self
            .generation_cleanup_task
            .lock()
            .expect("managed generation cleanup task lock poisoned") = Some(task);

        self.wait_for_generation_cleanup().await
    }

    pub(crate) async fn join_generation_cleanup(
        &self,
    ) -> anyhow::Result<ManagedGenerationCleanupReport> {
        if !self.generation_cleanup_started.load(Ordering::Acquire) {
            anyhow::bail!("managed generation cleanup has not started");
        }
        self.wait_for_generation_cleanup().await
    }

    pub(crate) async fn retry_generation_resource_cleanup(
        self: &Arc<Self>,
        deadline: tokio::time::Instant,
    ) -> anyhow::Result<ManagedGenerationCleanupReport> {
        if !self.generation_cleanup_complete.load(Ordering::Acquire) {
            anyhow::bail!("managed generation cleanup is still running");
        }
        let resources = self
            .resource_binding
            .lock()
            .expect("managed resource binding lock poisoned")
            .resources
            .as_ref()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("managed generation has no retained resources"))?;
        if self
            .generation_retry_running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            anyhow::bail!("managed generation resource retry is already running");
        }
        self.generation_retry_started.store(true, Ordering::Release);
        let runtime = Arc::clone(self);
        let task = tokio::spawn(async move {
            let operation = async {
                let resource_report = resources
                    .retry_cleanup_until(deadline)
                    .await
                    .map_err(|error| error.to_string())?;
                let call_drain = runtime
                    .call_drain_outcome
                    .lock()
                    .expect("managed call drain outcome lock poisoned")
                    .expect("managed call drain outcome must exist");
                Ok(ManagedGenerationCleanupReport {
                    call_drain,
                    resources: resource_report,
                })
            };
            let result = match std::panic::AssertUnwindSafe(operation).catch_unwind().await {
                Ok(result) => result,
                Err(_) => Err("managed generation resource retry task panicked".into()),
            };
            let release_resources = result
                .as_ref()
                .is_ok_and(|report| report.resources.strict_resources_settled());
            *runtime
                .generation_cleanup_result
                .lock()
                .expect("managed generation cleanup result lock poisoned") = Some(result);
            if release_resources {
                runtime
                    .resource_binding
                    .lock()
                    .expect("managed resource binding lock poisoned")
                    .resources
                    .take();
            }
            runtime
                .generation_retry_running
                .store(false, Ordering::Release);
            runtime.generation_retry_done.notify_waiters();
        });
        *self
            .generation_retry_task
            .lock()
            .expect("managed generation retry task lock poisoned") = Some(task);
        self.wait_for_generation_retry().await
    }

    pub(crate) async fn join_generation_resource_retry(
        &self,
    ) -> anyhow::Result<ManagedGenerationCleanupReport> {
        if !self.generation_retry_started.load(Ordering::Acquire) {
            anyhow::bail!("managed generation resource retry has not started");
        }
        self.wait_for_generation_retry().await
    }

    async fn wait_for_generation_retry(&self) -> anyhow::Result<ManagedGenerationCleanupReport> {
        while self.generation_retry_running.load(Ordering::Acquire) {
            let notified = self.generation_retry_done.notified();
            if !self.generation_retry_running.load(Ordering::Acquire) {
                break;
            }
            notified.await;
        }
        self.generation_cleanup_result
            .lock()
            .expect("managed generation cleanup result lock poisoned")
            .clone()
            .expect("managed generation retry result must exist")
            .map_err(anyhow::Error::msg)
    }

    async fn wait_for_generation_cleanup(&self) -> anyhow::Result<ManagedGenerationCleanupReport> {
        while !self.generation_cleanup_complete.load(Ordering::Acquire) {
            let notified = self.generation_cleanup_done.notified();
            if self.generation_cleanup_complete.load(Ordering::Acquire) {
                break;
            }
            notified.await;
        }
        self.generation_cleanup_result
            .lock()
            .expect("managed generation cleanup result lock poisoned")
            .clone()
            .expect("managed generation cleanup result must exist")
            .map_err(anyhow::Error::msg)
    }
}

impl Drop for ManagedGenerationRuntime {
    fn drop(&mut self) {
        self.cancellation.cancel();
        if let Ok(mut tasks) = self.tasks.lock() {
            tasks.abort_all();
        }
    }
}

struct CallAccounting {
    active_calls: AtomicUsize,
    idle: Notify,
}

struct ActiveCallGuard {
    accounting: Arc<CallAccounting>,
}

impl Drop for ActiveCallGuard {
    fn drop(&mut self) {
        if self.accounting.active_calls.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.accounting.idle.notify_waiters();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ManagedResourceCleanupEvidence {
    pub(crate) resource_id: RuntimeContributionResourceId,
    pub(crate) stop_attempted: bool,
    pub(crate) force_attempted: bool,
    pub(crate) reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ManagedResourceCleanupReport {
    pub(crate) records: Vec<RuntimeOwnedResourceRecord>,
    pub(crate) evidence: Vec<ManagedResourceCleanupEvidence>,
}

impl ManagedResourceCleanupReport {
    pub(crate) fn strict_resources_settled(&self) -> bool {
        self.records.iter().all(|record| {
            record.cleanup_assurance != RuntimeCleanupAssurance::Strict
                || record.cleanup_state == RuntimeCleanupState::Settled
        })
    }
}

struct ManagedResourceEntry {
    record: RuntimeOwnedResourceRecord,
    dependencies: BTreeSet<RuntimeContributionResourceId>,
    controller: Arc<dyn ManagedResourceController>,
    stop_attempted: bool,
    force_attempted: bool,
    reason: Option<String>,
}

#[derive(Default)]
struct ManagedResourceOwnerState {
    entries: BTreeMap<RuntimeContributionResourceId, ManagedResourceEntry>,
    cleanup_order: Vec<RuntimeContributionResourceId>,
    frozen: bool,
    running_attempt: Option<u64>,
    completed_attempt: u64,
}

pub(crate) struct ManagedResourceOwner {
    composition_generation_id: RuntimeCompositionGenerationId,
    key: ManagedGenerationKey,
    state: Mutex<ManagedResourceOwnerState>,
    next_attempt: AtomicUsize,
    cleanup_done: Notify,
    cleanup_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl ManagedResourceOwner {
    pub(crate) fn new(
        composition_generation_id: RuntimeCompositionGenerationId,
        key: ManagedGenerationKey,
    ) -> Arc<Self> {
        Arc::new(Self {
            composition_generation_id,
            key,
            state: Mutex::new(ManagedResourceOwnerState::default()),
            next_attempt: AtomicUsize::new(0),
            cleanup_done: Notify::new(),
            cleanup_task: Mutex::new(None),
        })
    }

    pub(crate) fn register(
        &self,
        id: RuntimeContributionResourceId,
        kind: RuntimeOwnedResourceKind,
        assurance: RuntimeCleanupAssurance,
        dependencies: impl IntoIterator<Item = RuntimeContributionResourceId>,
        controller: Arc<dyn ManagedResourceController>,
    ) -> anyhow::Result<()> {
        let mut state = self.state.lock().expect("managed resource lock poisoned");
        if state.frozen {
            anyhow::bail!("managed resource registration is frozen");
        }
        if state.entries.contains_key(&id) {
            anyhow::bail!("duplicate managed resource id {}", id.as_str());
        }
        let dependencies = dependencies.into_iter().collect::<Vec<_>>();
        let dependency_set = dependencies.iter().cloned().collect::<BTreeSet<_>>();
        if dependency_set.len() != dependencies.len() {
            anyhow::bail!("duplicate managed resource dependency");
        }
        let record = RuntimeOwnedResourceRecord {
            schema_version: RUNTIME_CONTRIBUTION_SCHEMA_VERSION,
            id: id.clone(),
            composition_generation_id: self.composition_generation_id.clone(),
            contribution_id: self.key.owner.clone(),
            generation_id: self.key.generation_id.clone(),
            kind,
            cleanup_assurance: assurance,
            cleanup_state: RuntimeCleanupState::Pending,
        };
        record.validate().map_err(anyhow::Error::msg)?;
        state.entries.insert(
            id,
            ManagedResourceEntry {
                record,
                dependencies: dependency_set,
                controller,
                stop_attempted: false,
                force_attempted: false,
                reason: None,
            },
        );
        Ok(())
    }

    pub(crate) fn validate_and_freeze(&self) -> anyhow::Result<()> {
        let mut state = self.state.lock().expect("managed resource lock poisoned");
        if state.frozen {
            return Ok(());
        }
        if state.entries.is_empty() {
            anyhow::bail!("managed generation requires at least one resource");
        }

        let mut remaining_dependencies = BTreeMap::new();
        let mut dependents = BTreeMap::<
            RuntimeContributionResourceId,
            BTreeSet<RuntimeContributionResourceId>,
        >::new();
        for (id, entry) in &state.entries {
            for dependency in &entry.dependencies {
                if !state.entries.contains_key(dependency) {
                    anyhow::bail!(
                        "managed resource {} has missing dependency {}",
                        id.as_str(),
                        dependency.as_str()
                    );
                }
                dependents
                    .entry(dependency.clone())
                    .or_default()
                    .insert(id.clone());
            }
            remaining_dependencies.insert(id.clone(), entry.dependencies.len());
        }

        let mut ready = remaining_dependencies
            .iter()
            .filter(|(_, count)| **count == 0)
            .map(|(id, _)| id.clone())
            .collect::<BTreeSet<_>>();
        let mut activation_order = Vec::with_capacity(state.entries.len());
        while let Some(id) = ready.pop_first() {
            activation_order.push(id.clone());
            if let Some(children) = dependents.get(&id) {
                for child in children {
                    let count = remaining_dependencies
                        .get_mut(child)
                        .expect("managed dependency child must exist");
                    *count -= 1;
                    if *count == 0 {
                        ready.insert(child.clone());
                    }
                }
            }
        }
        if activation_order.len() != state.entries.len() {
            anyhow::bail!("managed resource cleanup dependency cycle");
        }
        activation_order.reverse();
        state.cleanup_order = activation_order;
        state.frozen = true;
        Ok(())
    }

    async fn cleanup_until(
        self: &Arc<Self>,
        deadline: tokio::time::Instant,
    ) -> anyhow::Result<ManagedResourceCleanupReport> {
        let attempt = {
            let mut state = self.state.lock().expect("managed resource lock poisoned");
            if !state.frozen {
                anyhow::bail!("managed resources are not frozen");
            }
            match state.running_attempt {
                Some(_) => anyhow::bail!("managed resource cleanup is already running"),
                None => {
                    let attempt = self.next_attempt.fetch_add(1, Ordering::AcqRel) as u64 + 1;
                    state.running_attempt = Some(attempt);
                    attempt
                }
            }
        };

        let owner = Arc::clone(self);
        let task = tokio::spawn(async move {
            let result = std::panic::AssertUnwindSafe(owner.run_cleanup(deadline))
                .catch_unwind()
                .await;
            if result.is_err() {
                owner.mark_unresolved("managed resource cleanup task panicked");
            }
            let mut state = owner.state.lock().expect("managed resource lock poisoned");
            state.completed_attempt = attempt;
            state.running_attempt = None;
            drop(state);
            owner.cleanup_done.notify_waiters();
        });
        *self
            .cleanup_task
            .lock()
            .expect("managed resource cleanup task lock poisoned") = Some(task);

        self.wait_for_attempt(attempt).await;
        Ok(self.report())
    }

    pub(crate) async fn cleanup_candidate_until(
        self: &Arc<Self>,
        deadline: tokio::time::Instant,
    ) -> anyhow::Result<ManagedResourceCleanupReport> {
        self.cleanup_until(deadline).await
    }

    pub(crate) async fn retry_cleanup_until(
        self: &Arc<Self>,
        deadline: tokio::time::Instant,
    ) -> anyhow::Result<ManagedResourceCleanupReport> {
        if self
            .state
            .lock()
            .expect("managed resource lock poisoned")
            .completed_attempt
            == 0
        {
            anyhow::bail!("managed resource cleanup has not started");
        }
        self.cleanup_until(deadline).await
    }

    pub(crate) async fn join_running_cleanup(&self) -> ManagedResourceCleanupReport {
        let attempt = self
            .state
            .lock()
            .expect("managed resource lock poisoned")
            .running_attempt;
        if let Some(attempt) = attempt {
            self.wait_for_attempt(attempt).await;
        }
        self.report()
    }

    async fn wait_for_attempt(&self, attempt: u64) {
        loop {
            let notified = self.cleanup_done.notified();
            if self
                .state
                .lock()
                .expect("managed resource lock poisoned")
                .completed_attempt
                >= attempt
            {
                break;
            }
            notified.await;
        }
    }

    async fn run_cleanup(&self, deadline: tokio::time::Instant) {
        let order = self
            .state
            .lock()
            .expect("managed resource lock poisoned")
            .cleanup_order
            .clone();
        for id in order {
            let (controller, already_settled) = {
                let state = self.state.lock().expect("managed resource lock poisoned");
                let entry = state
                    .entries
                    .get(&id)
                    .expect("frozen managed resource must exist");
                (
                    Arc::clone(&entry.controller),
                    entry.record.cleanup_state == RuntimeCleanupState::Settled,
                )
            };
            if already_settled {
                continue;
            }

            let stop_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                controller.request_stop();
            }));
            self.record_stop(&id, stop_result.is_err());

            let cooperative = if stop_result.is_ok() {
                observe_settlement(&controller, deadline).await
            } else {
                SettlementObservation::Failed("managed resource stop request panicked".into())
            };
            if cooperative == SettlementObservation::Settled {
                self.record_settled(&id);
                continue;
            }

            let force_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                controller.force_stop();
            }));
            self.record_force(&id, force_result.is_err());
            let forced = if force_result.is_ok() {
                observe_settlement(&controller, deadline).await
            } else {
                SettlementObservation::Failed("managed resource force stop panicked".into())
            };
            if forced == SettlementObservation::Settled {
                self.record_settled(&id);
            } else {
                let reason = match (&forced, &cooperative) {
                    (SettlementObservation::Failed(reason), _)
                    | (_, SettlementObservation::Failed(reason)) => reason.clone(),
                    _ => "managed resource cleanup deadline expired".to_string(),
                };
                self.record_unresolved(&id, &reason);
            }
        }
    }

    fn record_stop(&self, id: &RuntimeContributionResourceId, panicked: bool) {
        let mut state = self.state.lock().expect("managed resource lock poisoned");
        let entry = state
            .entries
            .get_mut(id)
            .expect("managed resource must exist");
        entry.stop_attempted = true;
        if panicked {
            entry.reason = Some("managed resource stop request panicked".into());
        }
    }

    fn record_force(&self, id: &RuntimeContributionResourceId, panicked: bool) {
        let mut state = self.state.lock().expect("managed resource lock poisoned");
        let entry = state
            .entries
            .get_mut(id)
            .expect("managed resource must exist");
        entry.force_attempted = true;
        if panicked {
            entry.reason = Some("managed resource force stop panicked".into());
        }
    }

    fn record_settled(&self, id: &RuntimeContributionResourceId) {
        let mut state = self.state.lock().expect("managed resource lock poisoned");
        let entry = state
            .entries
            .get_mut(id)
            .expect("managed resource must exist");
        entry.record.cleanup_state = RuntimeCleanupState::Settled;
        entry.reason = None;
    }

    fn record_unresolved(&self, id: &RuntimeContributionResourceId, reason: &str) {
        let mut state = self.state.lock().expect("managed resource lock poisoned");
        let entry = state
            .entries
            .get_mut(id)
            .expect("managed resource must exist");
        entry.record.cleanup_state = match entry.record.cleanup_assurance {
            RuntimeCleanupAssurance::Strict => RuntimeCleanupState::Degraded,
            RuntimeCleanupAssurance::BestEffort | RuntimeCleanupAssurance::Unverified => {
                RuntimeCleanupState::Unverified
            }
        };
        entry.reason = Some(bounded_cleanup_reason(reason));
    }

    fn mark_unresolved(&self, reason: &str) {
        let ids = self
            .state
            .lock()
            .expect("managed resource lock poisoned")
            .entries
            .iter()
            .filter(|(_, entry)| entry.record.cleanup_state != RuntimeCleanupState::Settled)
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in ids {
            self.record_unresolved(&id, reason);
        }
    }

    pub(crate) fn report(&self) -> ManagedResourceCleanupReport {
        let state = self.state.lock().expect("managed resource lock poisoned");
        ManagedResourceCleanupReport {
            records: state
                .entries
                .values()
                .map(|entry| entry.record.clone())
                .collect(),
            evidence: state
                .entries
                .values()
                .map(|entry| ManagedResourceCleanupEvidence {
                    resource_id: entry.record.id.clone(),
                    stop_attempted: entry.stop_attempted,
                    force_attempted: entry.force_attempted,
                    reason: entry.reason.clone(),
                })
                .collect(),
        }
    }
}

impl Drop for ManagedResourceOwner {
    fn drop(&mut self) {
        let controllers = self
            .state
            .lock()
            .ok()
            .map(|state| {
                state
                    .entries
                    .values()
                    .filter(|entry| entry.record.cleanup_state != RuntimeCleanupState::Settled)
                    .map(|entry| Arc::clone(&entry.controller))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for controller in controllers {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                controller.request_stop();
            }));
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                controller.force_stop();
            }));
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SettlementObservation {
    Settled,
    TimedOut,
    Failed(String),
}

async fn observe_settlement(
    controller: &Arc<dyn ManagedResourceController>,
    deadline: tokio::time::Instant,
) -> SettlementObservation {
    if tokio::time::Instant::now() >= deadline {
        return SettlementObservation::TimedOut;
    }
    let future =
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| controller.await_settled()))
        {
            Ok(future) => future,
            Err(_) => {
                return SettlementObservation::Failed(
                    "managed resource settlement construction panicked".into(),
                );
            }
        };
    match tokio::time::timeout_at(
        deadline,
        std::panic::AssertUnwindSafe(future).catch_unwind(),
    )
    .await
    {
        Ok(Ok(Ok(()))) => SettlementObservation::Settled,
        Ok(Ok(Err(reason))) => SettlementObservation::Failed(bounded_cleanup_reason(&reason)),
        Ok(Err(_)) => SettlementObservation::Failed("managed resource settlement panicked".into()),
        Err(_) => SettlementObservation::TimedOut,
    }
}

fn bounded_cleanup_reason(reason: &str) -> String {
    if reason.len() <= 512 {
        return reason.to_string();
    }
    let mut end = 512;
    while !reason.is_char_boundary(end) {
        end -= 1;
    }
    reason[..end].to_string()
}

#[derive(Clone)]
struct AdmissionEntry {
    state: ManagedServiceGenerationState,
    runtime: Arc<ManagedGenerationRuntime>,
}

#[derive(Clone, Default)]
pub(crate) struct ManagedAdmissionTable {
    entries: BTreeMap<ManagedGenerationKey, AdmissionEntry>,
}

impl ManagedAdmissionTable {
    pub(crate) fn insert(
        &mut self,
        key: ManagedGenerationKey,
        state: ManagedServiceGenerationState,
        runtime: Arc<ManagedGenerationRuntime>,
    ) {
        self.entries.insert(key, AdmissionEntry { state, runtime });
    }
}

#[derive(Default)]
pub(crate) struct ManagedAdmissionRegistry {
    table: RwLock<ManagedAdmissionTable>,
}

impl ManagedAdmissionRegistry {
    pub(crate) fn replace(&self, table: ManagedAdmissionTable) -> anyhow::Result<()> {
        let mut current = self.table.write().expect("managed admission lock poisoned");
        for (key, entry) in &table.entries {
            if table.entries.iter().any(|(other_key, other)| {
                key != other_key && Arc::ptr_eq(&entry.runtime, &other.runtime)
            }) {
                anyhow::bail!("managed generation runtime is shared by multiple owners");
            }
        }
        for (key, existing) in &current.entries {
            if table.entries.contains_key(key) {
                continue;
            }
            if existing.state != ManagedServiceGenerationState::Retired
                || !existing.runtime.retirement_ready()
            {
                anyhow::bail!("managed table cannot omit a live generation");
            }
        }
        for (key, replacement) in &table.entries {
            if replacement.runtime.key != *key {
                anyhow::bail!("managed generation runtime key does not match admission key");
            }
            let Some(existing) = current.entries.get(key) else {
                if replacement.state != ManagedServiceGenerationState::Accepting {
                    anyhow::bail!("new managed generation must accept calls");
                }
                if replacement.runtime.admission_closed.load(Ordering::Acquire)
                    || replacement.runtime.cleanup_started.load(Ordering::Acquire)
                    || replacement.runtime.cleanup_complete.load(Ordering::Acquire)
                    || replacement.runtime.cancellation.is_cancelled()
                    || replacement.runtime.active_calls() != 0
                {
                    anyhow::bail!("new managed generation runtime is not fresh");
                }
                continue;
            };
            if !Arc::ptr_eq(&existing.runtime, &replacement.runtime) {
                anyhow::bail!("managed generation runtime identity changed");
            }
            if !legal_transition(existing.state, replacement.state) {
                anyhow::bail!("illegal managed generation state transition");
            }
            if replacement.state == ManagedServiceGenerationState::Retired
                && !existing.runtime.retirement_ready()
            {
                anyhow::bail!("managed generation cleanup is incomplete");
            }
        }
        for entry in table.entries.values() {
            entry.runtime.mark_published();
        }
        *current = table;
        for entry in current.entries.values() {
            if entry.state != ManagedServiceGenerationState::Accepting {
                entry
                    .runtime
                    .admission_closed
                    .store(true, Ordering::Release);
            }
        }
        Ok(())
    }

    pub(crate) fn transition(
        &self,
        key: &ManagedGenerationKey,
        state: ManagedServiceGenerationState,
    ) -> anyhow::Result<()> {
        let mut table = self.table.write().expect("managed admission lock poisoned");
        let entry = table
            .entries
            .get_mut(key)
            .ok_or_else(|| anyhow::anyhow!("unknown managed generation"))?;
        if state == ManagedServiceGenerationState::Retired && !entry.runtime.retirement_ready() {
            anyhow::bail!("managed generation cleanup is incomplete");
        }
        if !legal_transition(entry.state, state) {
            anyhow::bail!("illegal managed generation state transition");
        }
        entry.state = state;
        if state != ManagedServiceGenerationState::Accepting {
            entry
                .runtime
                .admission_closed
                .store(true, Ordering::Release);
        }
        Ok(())
    }
}

fn legal_transition(
    from: ManagedServiceGenerationState,
    to: ManagedServiceGenerationState,
) -> bool {
    matches!(
        (from, to),
        (
            ManagedServiceGenerationState::Accepting,
            ManagedServiceGenerationState::Accepting | ManagedServiceGenerationState::Draining
        ) | (
            ManagedServiceGenerationState::Draining,
            ManagedServiceGenerationState::Draining
                | ManagedServiceGenerationState::Degraded
                | ManagedServiceGenerationState::Retired
        ) | (
            ManagedServiceGenerationState::Degraded,
            ManagedServiceGenerationState::Degraded | ManagedServiceGenerationState::Retired
        ) | (
            ManagedServiceGenerationState::Retired,
            ManagedServiceGenerationState::Retired
        )
    )
}

pub(crate) struct ManagedServiceHandle<S>
where
    S: ManagedServiceContract + ?Sized,
{
    pub(crate) capability_id: RuntimeCapabilityId,
    pub(crate) owner: RuntimeContributionId,
    pub(crate) generation_id: RuntimeContributionGenerationId,
    key: ManagedGenerationKey,
    registry: Arc<ManagedAdmissionRegistry>,
    runtime: Arc<ManagedGenerationRuntime>,
    service: Arc<S>,
}

impl<S> Clone for ManagedServiceHandle<S>
where
    S: ManagedServiceContract + ?Sized,
{
    fn clone(&self) -> Self {
        Self {
            capability_id: self.capability_id.clone(),
            owner: self.owner.clone(),
            generation_id: self.generation_id.clone(),
            key: self.key.clone(),
            registry: Arc::clone(&self.registry),
            runtime: Arc::clone(&self.runtime),
            service: Arc::clone(&self.service),
        }
    }
}

impl<S> fmt::Debug for ManagedServiceHandle<S>
where
    S: ManagedServiceContract + ?Sized,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ManagedServiceHandle")
            .field("capability_id", &self.capability_id)
            .field("owner", &self.owner)
            .field("generation_id", &self.generation_id)
            .finish_non_exhaustive()
    }
}

impl<S> ManagedServiceHandle<S>
where
    S: ManagedServiceContract + ?Sized,
{
    pub(crate) fn new(
        capability_id: RuntimeCapabilityId,
        owner: RuntimeContributionId,
        generation_id: RuntimeContributionGenerationId,
        registry: Arc<ManagedAdmissionRegistry>,
        runtime: Arc<ManagedGenerationRuntime>,
        service: Arc<S>,
    ) -> Self {
        Self {
            key: ManagedGenerationKey {
                owner: owner.clone(),
                generation_id: generation_id.clone(),
            },
            capability_id,
            owner,
            generation_id,
            registry,
            runtime,
            service,
        }
    }

    pub(crate) async fn invoke(
        &self,
        request: S::Request,
    ) -> Result<S::Response, ManagedServiceCallError<S::Error>> {
        let (result_tx, result_rx) = oneshot::channel();
        {
            // Admission and task registration share the read lock so a table
            // swap cannot miss a call that was already admitted.
            let table = self
                .registry
                .table
                .read()
                .expect("managed admission lock poisoned");
            let entry = table
                .entries
                .get(&self.key)
                .ok_or(ManagedServiceCallError::GenerationRetired)?;
            match entry.state {
                ManagedServiceGenerationState::Accepting => {}
                ManagedServiceGenerationState::Draining => {
                    return Err(ManagedServiceCallError::GenerationDraining);
                }
                ManagedServiceGenerationState::Degraded => {
                    return Err(ManagedServiceCallError::GenerationDegraded);
                }
                ManagedServiceGenerationState::Retired => {
                    return Err(ManagedServiceCallError::GenerationRetired);
                }
            }
            if !Arc::ptr_eq(&entry.runtime, &self.runtime) {
                return Err(ManagedServiceCallError::GenerationDegraded);
            }

            self.runtime
                .accounting
                .active_calls
                .fetch_add(1, Ordering::AcqRel);
            let guard = ActiveCallGuard {
                accounting: Arc::clone(&self.runtime.accounting),
            };
            let service = Arc::clone(&self.service);
            let context = ManagedCallContext {
                capability_id: self.capability_id.clone(),
                owner: self.owner.clone(),
                generation_id: self.generation_id.clone(),
                cancellation: self.runtime.cancellation.child_token(),
            };
            let mut tasks = self
                .runtime
                .tasks
                .lock()
                .expect("managed call task lock poisoned");
            while tasks.try_join_next().is_some() {}
            tasks.spawn(async move {
                let _guard = guard;
                let cancellation = context.cancellation.clone();
                let operation = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    service.execute(request, context)
                })) {
                    Ok(operation) => std::panic::AssertUnwindSafe(operation).catch_unwind(),
                    Err(_) => {
                        let _ = result_tx.send(Err(ManagedServiceCallError::Panicked));
                        return;
                    }
                };
                tokio::pin!(operation);
                let outcome = tokio::select! {
                    biased;
                    () = cancellation.cancelled() => Err(ManagedServiceCallError::Cancelled),
                    outcome = &mut operation => match outcome {
                        Ok(Ok(response)) => Ok(response),
                        Ok(Err(error)) => Err(ManagedServiceCallError::Operation(error)),
                        Err(_) => Err(ManagedServiceCallError::Panicked),
                    },
                };
                let _ = result_tx.send(outcome);
            });
        }

        match result_rx.await {
            Ok(result) => result,
            Err(_) if self.runtime.cancellation.is_cancelled() => {
                Err(ManagedServiceCallError::Cancelled)
            }
            Err(_) => Err(ManagedServiceCallError::Panicked),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omegon_traits::{ManagedResourceSettlementFuture, ManagedServiceFuture};
    use std::future::pending;
    use std::time::Duration;

    struct TestService;

    enum Request {
        Echo(String),
        Wait,
        Panic,
        SynchronousPanic,
    }

    struct SyntheticResource {
        name: &'static str,
        events: Arc<Mutex<Vec<String>>>,
        settled: AtomicBool,
        settle_on_stop: bool,
        settle_on_force: bool,
        changed: Notify,
    }

    impl SyntheticResource {
        fn new(
            name: &'static str,
            events: Arc<Mutex<Vec<String>>>,
            settle_on_stop: bool,
            settle_on_force: bool,
        ) -> Arc<Self> {
            Arc::new(Self {
                name,
                events,
                settled: AtomicBool::new(false),
                settle_on_stop,
                settle_on_force,
                changed: Notify::new(),
            })
        }

        fn settle(&self) {
            self.settled.store(true, Ordering::Release);
            self.changed.notify_waiters();
        }

        fn event(&self, action: &str) {
            self.events
                .lock()
                .expect("synthetic event lock poisoned")
                .push(format!("{}:{action}", self.name));
        }
    }

    impl ManagedResourceController for SyntheticResource {
        fn request_stop(&self) {
            self.event("stop");
            if self.settle_on_stop {
                self.settle();
            }
        }

        fn force_stop(&self) {
            self.event("force");
            if self.settle_on_force {
                self.settle();
            }
        }

        fn await_settled(&self) -> ManagedResourceSettlementFuture<'_> {
            self.event("await");
            Box::pin(async move {
                while !self.settled.load(Ordering::Acquire) {
                    let changed = self.changed.notified();
                    if self.settled.load(Ordering::Acquire) {
                        break;
                    }
                    changed.await;
                }
                Ok(())
            })
        }
    }

    fn resource_id(value: &str) -> RuntimeContributionResourceId {
        RuntimeContributionResourceId::new(value).unwrap()
    }

    fn resource_owner() -> Arc<ManagedResourceOwner> {
        resource_owner_for(&ManagedGenerationKey {
            owner: RuntimeContributionId::new("feature:managed-resources").unwrap(),
            generation_id: RuntimeContributionGenerationId::new("managed-resources:v1").unwrap(),
        })
    }

    fn resource_owner_for(key: &ManagedGenerationKey) -> Arc<ManagedResourceOwner> {
        ManagedResourceOwner::new(
            RuntimeCompositionGenerationId::new("composition:test").unwrap(),
            key.clone(),
        )
    }

    fn register_resource(
        owner: &ManagedResourceOwner,
        id: &str,
        dependencies: &[&str],
        assurance: RuntimeCleanupAssurance,
        kind: RuntimeOwnedResourceKind,
        controller: Arc<SyntheticResource>,
    ) {
        owner
            .register(
                resource_id(id),
                kind,
                assurance,
                dependencies
                    .iter()
                    .map(|dependency| resource_id(dependency)),
                controller,
            )
            .unwrap();
    }

    impl ManagedServiceContract for TestService {
        type Request = Request;
        type Response = String;
        type Error = String;

        fn execute<'a>(
            &'a self,
            request: Self::Request,
            context: ManagedCallContext,
        ) -> ManagedServiceFuture<'a, Self::Response, Self::Error> {
            if matches!(request, Request::SynchronousPanic) {
                panic!("synthetic synchronous managed call panic");
            }
            Box::pin(async move {
                match request {
                    Request::Echo(value) => Ok(value),
                    Request::Wait => {
                        tokio::select! {
                            () = context.cancellation.cancelled() => Ok("cancelled".into()),
                            () = pending() => unreachable!(),
                        }
                    }
                    Request::Panic => panic!("synthetic managed call panic"),
                    Request::SynchronousPanic => unreachable!(),
                }
            })
        }
    }

    fn unpublished_fixture() -> (
        Arc<ManagedAdmissionRegistry>,
        Arc<ManagedGenerationRuntime>,
        ManagedGenerationKey,
        ManagedServiceHandle<TestService>,
    ) {
        let registry = Arc::new(ManagedAdmissionRegistry::default());
        let key = ManagedGenerationKey {
            owner: RuntimeContributionId::new("feature:test-service").unwrap(),
            generation_id: RuntimeContributionGenerationId::new("test-service:v1").unwrap(),
        };
        let runtime = ManagedGenerationRuntime::new(key.clone());
        let handle = ManagedServiceHandle::new(
            RuntimeCapabilityId::new("service:test").unwrap(),
            key.owner.clone(),
            key.generation_id.clone(),
            Arc::clone(&registry),
            Arc::clone(&runtime),
            Arc::new(TestService),
        );
        (registry, runtime, key, handle)
    }

    fn publish_fixture(
        registry: &ManagedAdmissionRegistry,
        runtime: &Arc<ManagedGenerationRuntime>,
        key: &ManagedGenerationKey,
    ) {
        let mut table = ManagedAdmissionTable::default();
        table.insert(
            key.clone(),
            ManagedServiceGenerationState::Accepting,
            Arc::clone(runtime),
        );
        registry.replace(table).unwrap();
    }

    fn fixture() -> (
        Arc<ManagedAdmissionRegistry>,
        Arc<ManagedGenerationRuntime>,
        ManagedGenerationKey,
        ManagedServiceHandle<TestService>,
    ) {
        let fixture = unpublished_fixture();
        publish_fixture(&fixture.0, &fixture.1, &fixture.2);
        fixture
    }

    #[tokio::test]
    async fn rg02_table_swap_linearizes_stale_handle_admission() {
        let (registry, runtime, key, handle) = fixture();
        assert_eq!(
            handle.invoke(Request::Echo("before".into())).await.unwrap(),
            "before"
        );

        let mut table = ManagedAdmissionTable::default();
        table.insert(
            key.clone(),
            ManagedServiceGenerationState::Draining,
            Arc::clone(&runtime),
        );
        registry.replace(table).unwrap();

        assert_eq!(
            handle.invoke(Request::Echo("after".into())).await,
            Err(ManagedServiceCallError::GenerationDraining)
        );
        assert!(registry.replace(ManagedAdmissionTable::default()).is_err());

        let mut aliased = ManagedAdmissionTable::default();
        aliased.insert(
            key,
            ManagedServiceGenerationState::Draining,
            Arc::clone(&runtime),
        );
        aliased.insert(
            ManagedGenerationKey {
                owner: RuntimeContributionId::new("feature:alias").unwrap(),
                generation_id: RuntimeContributionGenerationId::new("alias:v1").unwrap(),
            },
            ManagedServiceGenerationState::Accepting,
            Arc::clone(&runtime),
        );
        assert!(registry.replace(aliased).is_err());
    }

    #[tokio::test]
    async fn rg03_panic_and_abort_settle_owned_call_accounting() {
        let (registry, runtime, key, handle) = fixture();
        assert_eq!(
            handle.invoke(Request::Panic).await,
            Err(ManagedServiceCallError::Panicked)
        );
        assert_eq!(
            handle.invoke(Request::SynchronousPanic).await,
            Err(ManagedServiceCallError::Panicked)
        );
        assert_eq!(runtime.active_calls(), 0);

        let waiting = tokio::spawn({
            let handle = handle.clone();
            async move { handle.invoke(Request::Wait).await }
        });
        while runtime.active_calls() == 0 {
            tokio::task::yield_now().await;
        }
        waiting.abort();
        assert!(waiting.await.unwrap_err().is_cancelled());
        assert_eq!(
            runtime.active_calls(),
            1,
            "caller cancellation must not detach owned work"
        );

        let mut table = ManagedAdmissionTable::default();
        table.insert(
            key,
            ManagedServiceGenerationState::Draining,
            Arc::clone(&runtime),
        );
        registry.replace(table).unwrap();
        let cleanup_waiter = tokio::spawn({
            let runtime = Arc::clone(&runtime);
            async move { runtime.abort_and_join_calls().await }
        });
        while !runtime.cleanup_started.load(Ordering::Acquire) {
            tokio::task::yield_now().await;
        }
        cleanup_waiter.abort();
        let _ = cleanup_waiter.await;
        runtime.abort_and_join_calls().await.unwrap();
        assert_eq!(runtime.active_calls(), 0);
    }

    #[tokio::test]
    async fn cleanup_requires_closed_admission_and_drop_aborts_owned_calls() {
        let (registry, runtime, _key, handle) = fixture();
        assert!(runtime.abort_and_join_calls().await.is_err());

        let waiting = tokio::spawn({
            let handle = handle.clone();
            async move { handle.invoke(Request::Wait).await }
        });
        while runtime.active_calls() == 0 {
            tokio::task::yield_now().await;
        }
        waiting.abort();
        let _ = waiting.await;

        let weak_runtime = Arc::downgrade(&runtime);
        let weak_accounting = Arc::downgrade(&runtime.accounting);
        drop(handle);
        drop(runtime);
        drop(registry);
        while weak_accounting.upgrade().is_some() {
            tokio::task::yield_now().await;
        }
        assert!(weak_runtime.upgrade().is_none());
    }

    #[tokio::test]
    async fn rg04_stale_handle_reports_degraded_and_retired_states() {
        let (registry, runtime, key, handle) = fixture();
        registry
            .transition(&key, ManagedServiceGenerationState::Draining)
            .unwrap();
        registry
            .transition(&key, ManagedServiceGenerationState::Degraded)
            .unwrap();
        assert_eq!(
            handle.invoke(Request::Echo("degraded".into())).await,
            Err(ManagedServiceCallError::GenerationDegraded)
        );
        assert!(
            registry
                .transition(&key, ManagedServiceGenerationState::Retired)
                .is_err()
        );
        runtime.abort_and_join_calls().await.unwrap();
        registry
            .transition(&key, ManagedServiceGenerationState::Retired)
            .unwrap();
        assert_eq!(
            handle.invoke(Request::Echo("retired".into())).await,
            Err(ManagedServiceCallError::GenerationRetired)
        );
        assert!(
            registry
                .transition(&key, ManagedServiceGenerationState::Accepting)
                .is_err()
        );
        let mut reopening = ManagedAdmissionTable::default();
        reopening.insert(
            key.clone(),
            ManagedServiceGenerationState::Accepting,
            Arc::clone(&runtime),
        );
        assert!(registry.replace(reopening).is_err());

        let mut changed_identity = ManagedAdmissionTable::default();
        changed_identity.insert(
            key.clone(),
            ManagedServiceGenerationState::Retired,
            ManagedGenerationRuntime::new(key.clone()),
        );
        assert!(registry.replace(changed_identity).is_err());

        let empty = ManagedAdmissionTable::default();
        registry.replace(empty).unwrap();
        let mut recycled = ManagedAdmissionTable::default();
        recycled.insert(
            ManagedGenerationKey {
                owner: RuntimeContributionId::new("feature:recycled").unwrap(),
                generation_id: RuntimeContributionGenerationId::new("recycled:v1").unwrap(),
            },
            ManagedServiceGenerationState::Accepting,
            Arc::clone(&runtime),
        );
        assert!(registry.replace(recycled).is_err());
        assert_eq!(runtime.active_calls(), 0);
    }

    #[tokio::test]
    async fn rg07_cleanup_follows_reverse_topological_order() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let owner = resource_owner();
        register_resource(
            &owner,
            "resource:task",
            &[],
            RuntimeCleanupAssurance::Strict,
            RuntimeOwnedResourceKind::Task,
            SyntheticResource::new("task", Arc::clone(&events), true, false),
        );
        register_resource(
            &owner,
            "resource:writer",
            &["resource:subscription"],
            RuntimeCleanupAssurance::Strict,
            RuntimeOwnedResourceKind::DurableWriter,
            SyntheticResource::new("writer", Arc::clone(&events), true, false),
        );
        register_resource(
            &owner,
            "resource:subscription",
            &["resource:task"],
            RuntimeCleanupAssurance::Strict,
            RuntimeOwnedResourceKind::Subscription,
            SyntheticResource::new("subscription", Arc::clone(&events), true, false),
        );
        owner.validate_and_freeze().unwrap();

        let report = owner
            .cleanup_candidate_until(tokio::time::Instant::now() + Duration::from_secs(1))
            .await
            .unwrap();
        assert!(report.strict_resources_settled());
        assert_eq!(
            *events.lock().unwrap(),
            [
                "writer:stop",
                "writer:await",
                "subscription:stop",
                "subscription:await",
                "task:stop",
                "task:await",
            ]
        );
    }

    #[test]
    fn rg07_freeze_rejects_missing_dependencies_and_cycles() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let missing = resource_owner();
        register_resource(
            &missing,
            "resource:one",
            &["resource:missing"],
            RuntimeCleanupAssurance::Strict,
            RuntimeOwnedResourceKind::Task,
            SyntheticResource::new("one", Arc::clone(&events), false, false),
        );
        assert!(missing.validate_and_freeze().is_err());

        let cyclic = resource_owner();
        register_resource(
            &cyclic,
            "resource:one",
            &["resource:two"],
            RuntimeCleanupAssurance::Strict,
            RuntimeOwnedResourceKind::Task,
            SyntheticResource::new("one", Arc::clone(&events), false, false),
        );
        register_resource(
            &cyclic,
            "resource:two",
            &["resource:one"],
            RuntimeCleanupAssurance::Strict,
            RuntimeOwnedResourceKind::Task,
            SyntheticResource::new("two", Arc::clone(&events), false, false),
        );
        assert!(cyclic.validate_and_freeze().is_err());
        assert!(events.lock().unwrap().is_empty());
    }

    #[test]
    fn managed_resources_must_attach_before_matching_generation_publication() {
        let (registry, runtime, key, _handle) = fixture();
        let events = Arc::new(Mutex::new(Vec::new()));
        let late = resource_owner_for(&key);
        register_resource(
            &late,
            "resource:late",
            &[],
            RuntimeCleanupAssurance::Strict,
            RuntimeOwnedResourceKind::Task,
            SyntheticResource::new("late", Arc::clone(&events), false, false),
        );
        assert!(runtime.attach_resources(late).is_err());

        let (_other_registry, other_runtime, _other_key, _handle) = unpublished_fixture();
        let mismatched = resource_owner();
        register_resource(
            &mismatched,
            "resource:mismatched",
            &[],
            RuntimeCleanupAssurance::Strict,
            RuntimeOwnedResourceKind::Task,
            SyntheticResource::new("mismatched", events, false, false),
        );
        assert!(other_runtime.attach_resources(mismatched).is_err());
        assert_eq!(registry.table.read().unwrap().entries.len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn rg06_cleanup_uses_one_non_resetting_generation_deadline() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let owner = resource_owner();
        register_resource(
            &owner,
            "resource:z-slow",
            &[],
            RuntimeCleanupAssurance::Strict,
            RuntimeOwnedResourceKind::Task,
            SyntheticResource::new("slow", Arc::clone(&events), false, false),
        );
        register_resource(
            &owner,
            "resource:a-later",
            &[],
            RuntimeCleanupAssurance::Strict,
            RuntimeOwnedResourceKind::Task,
            SyntheticResource::new("later", Arc::clone(&events), false, false),
        );
        owner.validate_and_freeze().unwrap();

        let cleanup = tokio::spawn({
            let owner = Arc::clone(&owner);
            async move {
                owner
                    .cleanup_candidate_until(tokio::time::Instant::now() + Duration::from_secs(10))
                    .await
                    .unwrap()
            }
        });
        while !events.lock().unwrap().contains(&"slow:await".to_string()) {
            tokio::task::yield_now().await;
        }
        tokio::time::advance(Duration::from_secs(10)).await;
        let report = cleanup.await.unwrap();

        assert!(!report.strict_resources_settled());
        assert!(
            report
                .records
                .iter()
                .all(|record| record.cleanup_state == RuntimeCleanupState::Degraded)
        );
        assert_eq!(
            *events.lock().unwrap(),
            [
                "slow:stop",
                "slow:await",
                "slow:force",
                "later:stop",
                "later:force"
            ]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn rg06_resource_cleanup_starts_only_after_active_call_deadline_and_join() {
        let (registry, runtime, key, handle) = unpublished_fixture();
        let events = Arc::new(Mutex::new(Vec::new()));
        let resources = resource_owner_for(&key);
        register_resource(
            &resources,
            "resource:after-call",
            &[],
            RuntimeCleanupAssurance::Strict,
            RuntimeOwnedResourceKind::Task,
            SyntheticResource::new("resource", Arc::clone(&events), true, false),
        );
        runtime.attach_resources(Arc::clone(&resources)).unwrap();
        publish_fixture(&registry, &runtime, &key);

        let call = tokio::spawn({
            let handle = handle.clone();
            async move { handle.invoke(Request::Wait).await }
        });
        while runtime.active_calls() == 0 {
            tokio::task::yield_now().await;
        }
        registry
            .transition(&key, ManagedServiceGenerationState::Draining)
            .unwrap();
        let cleanup = tokio::spawn({
            let runtime = Arc::clone(&runtime);
            async move {
                runtime
                    .cleanup_generation(
                        tokio::time::Instant::now() + Duration::from_secs(10),
                        Duration::from_secs(5),
                    )
                    .await
                    .unwrap()
            }
        });

        tokio::time::advance(Duration::from_secs(9)).await;
        tokio::task::yield_now().await;
        assert_eq!(runtime.active_calls(), 1);
        assert!(events.lock().unwrap().is_empty());

        tokio::time::advance(Duration::from_secs(1)).await;
        let report = cleanup.await.unwrap();
        assert_eq!(report.call_drain, ManagedCallDrainOutcome::DeadlineForced);
        assert!(report.resources.strict_resources_settled());
        assert_eq!(runtime.active_calls(), 0);
        assert_eq!(*events.lock().unwrap(), ["resource:stop", "resource:await"]);
        assert!(matches!(
            call.await.unwrap(),
            Err(ManagedServiceCallError::Cancelled)
        ));
    }

    #[tokio::test]
    async fn rg08_strict_cleanup_is_retained_and_retryable() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let owner = resource_owner();
        let controller = SyntheticResource::new("strict", Arc::clone(&events), false, false);
        register_resource(
            &owner,
            "resource:strict",
            &[],
            RuntimeCleanupAssurance::Strict,
            RuntimeOwnedResourceKind::TemporaryDirectory,
            Arc::clone(&controller),
        );
        owner.validate_and_freeze().unwrap();

        let degraded = owner
            .cleanup_candidate_until(tokio::time::Instant::now())
            .await
            .unwrap();
        assert!(!degraded.strict_resources_settled());
        assert_eq!(
            degraded.records[0].cleanup_state,
            RuntimeCleanupState::Degraded
        );
        assert!(degraded.evidence[0].stop_attempted);
        assert!(degraded.evidence[0].force_attempted);

        controller.settle();
        let settled = owner
            .retry_cleanup_until(tokio::time::Instant::now() + Duration::from_secs(1))
            .await
            .unwrap();
        assert!(settled.strict_resources_settled());
        assert_eq!(
            settled.records[0].cleanup_state,
            RuntimeCleanupState::Settled
        );
    }

    #[tokio::test]
    async fn rg08_generation_retains_strict_owner_until_retry_settles() {
        let (registry, runtime, key, _handle) = unpublished_fixture();
        let events = Arc::new(Mutex::new(Vec::new()));
        let resources = resource_owner_for(&key);
        let controller = SyntheticResource::new("retained", events, false, false);
        register_resource(
            &resources,
            "resource:retained",
            &[],
            RuntimeCleanupAssurance::Strict,
            RuntimeOwnedResourceKind::TemporaryDirectory,
            Arc::clone(&controller),
        );
        let weak_resources = Arc::downgrade(&resources);
        runtime.attach_resources(resources).unwrap();
        publish_fixture(&registry, &runtime, &key);
        registry
            .transition(&key, ManagedServiceGenerationState::Draining)
            .unwrap();

        let degraded = runtime
            .cleanup_generation(tokio::time::Instant::now(), Duration::from_millis(0))
            .await
            .unwrap();
        assert!(!degraded.resources.strict_resources_settled());
        assert!(weak_resources.upgrade().is_some());
        assert!(
            registry
                .transition(&key, ManagedServiceGenerationState::Retired)
                .is_err()
        );

        let retry = tokio::spawn({
            let runtime = Arc::clone(&runtime);
            async move {
                runtime
                    .retry_generation_resource_cleanup(
                        tokio::time::Instant::now() + Duration::from_secs(60),
                    )
                    .await
            }
        });
        while !controller
            .events
            .lock()
            .unwrap()
            .contains(&"retained:await".to_string())
        {
            tokio::task::yield_now().await;
        }
        retry.abort();
        let _ = retry.await;
        controller.settle();
        let settled = runtime.join_generation_resource_retry().await.unwrap();
        assert!(settled.resources.strict_resources_settled());
        assert!(weak_resources.upgrade().is_none());
        registry
            .transition(&key, ManagedServiceGenerationState::Retired)
            .unwrap();
    }

    #[tokio::test]
    async fn rg09_best_effort_cross_boundary_cleanup_is_unverified() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let owner = resource_owner();
        register_resource(
            &owner,
            "resource:remote",
            &[],
            RuntimeCleanupAssurance::BestEffort,
            RuntimeOwnedResourceKind::RemoteService,
            SyntheticResource::new("remote", events, false, false),
        );
        owner.validate_and_freeze().unwrap();
        let report = owner
            .cleanup_candidate_until(tokio::time::Instant::now())
            .await
            .unwrap();
        assert_eq!(
            report.records[0].cleanup_state,
            RuntimeCleanupState::Unverified
        );
        assert!(!report.evidence[0].reason.as_deref().unwrap().is_empty());
    }

    #[tokio::test]
    async fn cleanup_caller_cancellation_does_not_detach_resource_owner() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let owner = resource_owner();
        let controller = SyntheticResource::new("owned", Arc::clone(&events), false, false);
        register_resource(
            &owner,
            "resource:owned",
            &[],
            RuntimeCleanupAssurance::Strict,
            RuntimeOwnedResourceKind::Task,
            Arc::clone(&controller),
        );
        owner.validate_and_freeze().unwrap();

        let waiter = tokio::spawn({
            let owner = Arc::clone(&owner);
            async move {
                owner
                    .cleanup_candidate_until(tokio::time::Instant::now() + Duration::from_secs(60))
                    .await
            }
        });
        while !events.lock().unwrap().contains(&"owned:await".to_string()) {
            tokio::task::yield_now().await;
        }
        waiter.abort();
        let _ = waiter.await;
        controller.settle();

        let report = owner.join_running_cleanup().await;
        assert!(report.strict_resources_settled());
    }

    #[tokio::test]
    async fn generation_cleanup_caller_cancellation_does_not_detach_sequencing() {
        let (registry, runtime, key, _handle) = unpublished_fixture();
        let events = Arc::new(Mutex::new(Vec::new()));
        let resources = resource_owner_for(&key);
        let controller = SyntheticResource::new("generation", Arc::clone(&events), false, false);
        register_resource(
            &resources,
            "resource:generation",
            &[],
            RuntimeCleanupAssurance::Strict,
            RuntimeOwnedResourceKind::Task,
            Arc::clone(&controller),
        );
        runtime.attach_resources(Arc::clone(&resources)).unwrap();
        let weak_resources = Arc::downgrade(&resources);
        drop(resources);
        publish_fixture(&registry, &runtime, &key);
        registry
            .transition(&key, ManagedServiceGenerationState::Draining)
            .unwrap();

        let waiter = tokio::spawn({
            let runtime = Arc::clone(&runtime);
            async move {
                runtime
                    .cleanup_generation(tokio::time::Instant::now(), Duration::from_secs(60))
                    .await
            }
        });
        while !events
            .lock()
            .unwrap()
            .contains(&"generation:await".to_string())
        {
            tokio::task::yield_now().await;
        }
        waiter.abort();
        let _ = waiter.await;
        controller.settle();

        let report = runtime.join_generation_cleanup().await.unwrap();
        assert!(report.resources.strict_resources_settled());
        assert!(weak_resources.upgrade().is_none());
    }
}
