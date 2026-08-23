//! Generation-gated invocation for resource-bearing in-process services.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use futures_util::FutureExt;
use omegon_traits::{
    ManagedCallContext, ManagedServiceCallError, ManagedServiceContract,
    ManagedServiceGenerationState, RuntimeCapabilityId, RuntimeContributionGenerationId,
    RuntimeContributionId,
};
use tokio::sync::{Notify, oneshot};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ManagedGenerationKey {
    pub(crate) owner: RuntimeContributionId,
    pub(crate) generation_id: RuntimeContributionGenerationId,
}

pub(crate) struct ManagedGenerationRuntime {
    cancellation: CancellationToken,
    accounting: Arc<CallAccounting>,
    tasks: Mutex<JoinSet<()>>,
    admission_closed: AtomicBool,
    cleanup_started: AtomicBool,
    cleanup_complete: AtomicBool,
    cleanup_done: Notify,
    cleanup_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl ManagedGenerationRuntime {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            cancellation: CancellationToken::new(),
            accounting: Arc::new(CallAccounting {
                active_calls: AtomicUsize::new(0),
                idle: Notify::new(),
            }),
            tasks: Mutex::new(JoinSet::new()),
            admission_closed: AtomicBool::new(false),
            cleanup_started: AtomicBool::new(false),
            cleanup_complete: AtomicBool::new(false),
            cleanup_done: Notify::new(),
            cleanup_task: Mutex::new(None),
        })
    }

    pub(crate) fn active_calls(&self) -> usize {
        self.accounting.active_calls.load(Ordering::Acquire)
    }

    pub(crate) async fn abort_and_join_calls(self: &Arc<Self>) -> anyhow::Result<()> {
        if !self.admission_closed.load(Ordering::Acquire) {
            anyhow::bail!("managed generation admission is still open");
        }
        if self
            .cleanup_started
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            self.cancellation.cancel();
            let mut cleanup_task = self
                .cleanup_task
                .lock()
                .expect("managed cleanup task lock poisoned");
            let runtime = Arc::clone(self);
            let task = tokio::spawn(async move {
                let mut tasks = {
                    let mut owned = runtime
                        .tasks
                        .lock()
                        .expect("managed call task lock poisoned");
                    std::mem::take(&mut *owned)
                };
                tasks.abort_all();
                while tasks.join_next().await.is_some() {}
                while runtime.active_calls() != 0 {
                    let notified = runtime.accounting.idle.notified();
                    if runtime.active_calls() == 0 {
                        break;
                    }
                    notified.await;
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
        Ok(())
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
                || !existing.runtime.cleanup_complete.load(Ordering::Acquire)
            {
                anyhow::bail!("managed table cannot omit a live generation");
            }
        }
        for (key, replacement) in &table.entries {
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
                && !existing.runtime.cleanup_complete.load(Ordering::Acquire)
            {
                anyhow::bail!("managed generation cleanup is incomplete");
            }
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
        if state == ManagedServiceGenerationState::Retired
            && !entry.runtime.cleanup_complete.load(Ordering::Acquire)
        {
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
    use omegon_traits::ManagedServiceFuture;
    use std::future::pending;

    struct TestService;

    enum Request {
        Echo(String),
        Wait,
        Panic,
        SynchronousPanic,
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

    fn fixture() -> (
        Arc<ManagedAdmissionRegistry>,
        Arc<ManagedGenerationRuntime>,
        ManagedGenerationKey,
        ManagedServiceHandle<TestService>,
    ) {
        let registry = Arc::new(ManagedAdmissionRegistry::default());
        let runtime = ManagedGenerationRuntime::new();
        let key = ManagedGenerationKey {
            owner: RuntimeContributionId::new("feature:test-service").unwrap(),
            generation_id: RuntimeContributionGenerationId::new("test-service:v1").unwrap(),
        };
        let mut table = ManagedAdmissionTable::default();
        table.insert(
            key.clone(),
            ManagedServiceGenerationState::Accepting,
            Arc::clone(&runtime),
        );
        registry.replace(table).unwrap();
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
            ManagedGenerationRuntime::new(),
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
}
