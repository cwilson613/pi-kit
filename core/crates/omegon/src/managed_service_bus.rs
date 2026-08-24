//! EventBus-owned lifecycle for resource-bearing in-process services.

use std::any::{Any, TypeId};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use omegon_traits::{
    ManagedResourceController, ManagedServiceContract, ManagedServiceGenerationState,
    RuntimeCapabilityId, RuntimeCleanupAssurance, RuntimeCompositionGenerationId,
    RuntimeContributionGenerationId, RuntimeContributionId, RuntimeContributionResourceId,
    RuntimeOwnedResourceKind, RuntimeServiceInterfaceId,
};

use crate::service_generation::{
    ManagedAdmissionRegistry, ManagedAdmissionTable, ManagedGenerationCleanupReport,
    ManagedGenerationKey, ManagedGenerationRuntime, ManagedResourceCleanupReport,
    ManagedResourceOwner, ManagedServiceHandle,
};

pub(crate) struct ManagedResourceRegistration {
    id: RuntimeContributionResourceId,
    kind: RuntimeOwnedResourceKind,
    assurance: RuntimeCleanupAssurance,
    dependencies: Vec<RuntimeContributionResourceId>,
    controller: Arc<dyn ManagedResourceController>,
    controller_identity: usize,
}

impl ManagedResourceRegistration {
    pub(crate) fn new(
        id: RuntimeContributionResourceId,
        kind: RuntimeOwnedResourceKind,
        assurance: RuntimeCleanupAssurance,
        dependencies: Vec<RuntimeContributionResourceId>,
        controller: Arc<dyn ManagedResourceController>,
    ) -> Self {
        let controller_identity = Arc::as_ptr(&controller) as *const () as usize;
        Self {
            id,
            kind,
            assurance,
            dependencies,
            controller,
            controller_identity,
        }
    }
}

pub(crate) struct ManagedServiceCandidate<S>
where
    S: ManagedServiceContract + ?Sized,
{
    composition_generation_id: RuntimeCompositionGenerationId,
    capability_id: RuntimeCapabilityId,
    interface_id: RuntimeServiceInterfaceId,
    owner: RuntimeContributionId,
    generation_id: RuntimeContributionGenerationId,
    active_call_duration: Duration,
    cleanup_duration: Duration,
    resources: Vec<ManagedResourceRegistration>,
    implementation: Arc<S>,
}

impl<S> ManagedServiceCandidate<S>
where
    S: ManagedServiceContract + ?Sized,
{
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        composition_generation_id: RuntimeCompositionGenerationId,
        capability_id: RuntimeCapabilityId,
        interface_id: RuntimeServiceInterfaceId,
        owner: RuntimeContributionId,
        generation_id: RuntimeContributionGenerationId,
        active_call_duration: Duration,
        cleanup_duration: Duration,
        resources: Vec<ManagedResourceRegistration>,
        implementation: Arc<S>,
    ) -> anyhow::Result<Self> {
        if active_call_duration.is_zero() {
            anyhow::bail!("managed service active-call duration must be nonzero");
        }
        if cleanup_duration.is_zero() {
            anyhow::bail!("managed service cleanup duration must be nonzero");
        }
        if resources.is_empty() {
            anyhow::bail!("managed service requires at least one resource");
        }
        let mut resource_ids = BTreeSet::new();
        let mut controller_identities = BTreeSet::new();
        for resource in &resources {
            if !resource_ids.insert(resource.id.clone()) {
                anyhow::bail!("duplicate managed resource id {}", resource.id.as_str());
            }
            if resource.dependencies.iter().collect::<BTreeSet<_>>().len()
                != resource.dependencies.len()
            {
                anyhow::bail!("duplicate managed resource dependency");
            }
            if !controller_identities.insert(resource.controller_identity) {
                anyhow::bail!("managed resources cannot share one controller");
            }
            if resource.kind == RuntimeOwnedResourceKind::RemoteService
                && resource.assurance == RuntimeCleanupAssurance::Strict
            {
                anyhow::bail!("a remote service cannot claim strict host cleanup assurance");
            }
        }
        Ok(Self {
            composition_generation_id,
            capability_id,
            interface_id,
            owner,
            generation_id,
            active_call_duration,
            cleanup_duration,
            resources,
            implementation,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ManagedResourceSignature {
    id: RuntimeContributionResourceId,
    kind: RuntimeOwnedResourceKind,
    assurance: RuntimeCleanupAssurance,
    dependencies: Vec<RuntimeContributionResourceId>,
    controller_identity: usize,
}

struct ManagedServiceHolder<S: ?Sized> {
    service: Arc<S>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ManagedPublishedServiceMetadata {
    pub(crate) capability_id: RuntimeCapabilityId,
    pub(crate) interface_id: RuntimeServiceInterfaceId,
    pub(crate) owner: RuntimeContributionId,
    pub(crate) generation_id: RuntimeContributionGenerationId,
}

struct ManagedPublishedService {
    metadata: ManagedPublishedServiceMetadata,
    implementation: Arc<dyn Any + Send + Sync>,
    implementation_identity: usize,
    implementation_type_id: TypeId,
    resource_signature: Vec<ManagedResourceSignature>,
    active_call_duration: Duration,
    cleanup_duration: Duration,
    runtime: Arc<ManagedGenerationRuntime>,
    state: ManagedServiceGenerationState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ManagedCleanupLaunch {
    NotRequired,
    Started,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ManagedServicePublicationOutcome {
    Rejected {
        reason: String,
        cleanup: Option<ManagedResourceCleanupReport>,
    },
    Published {
        cleanup: ManagedCleanupLaunch,
    },
    Unchanged {
        cleanup: ManagedCleanupLaunch,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ManagedShutdownGenerationResult {
    pub(crate) owner: RuntimeContributionId,
    pub(crate) generation_id: RuntimeContributionGenerationId,
    pub(crate) result: Result<ManagedGenerationCleanupReport, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ManagedServiceShutdownReport {
    pub(crate) generations: Vec<ManagedShutdownGenerationResult>,
    pub(crate) rejected_candidates: Vec<ManagedResourceCleanupReport>,
}

struct RejectedResourceOwner {
    owner: Arc<ManagedResourceOwner>,
    cleanup_duration: Duration,
}

pub(crate) struct ManagedServiceBus {
    registry: Arc<ManagedAdmissionRegistry>,
    generations: BTreeMap<ManagedGenerationKey, ManagedPublishedService>,
    active: BTreeMap<RuntimeCapabilityId, ManagedGenerationKey>,
    rejected_resource_owners: Vec<RejectedResourceOwner>,
    shutdown: bool,
}

impl Default for ManagedServiceBus {
    fn default() -> Self {
        Self {
            registry: Arc::new(ManagedAdmissionRegistry::default()),
            generations: BTreeMap::new(),
            active: BTreeMap::new(),
            rejected_resource_owners: Vec::new(),
            shutdown: false,
        }
    }
}

impl ManagedServiceBus {
    pub(crate) async fn publish<S>(
        &mut self,
        candidate: ManagedServiceCandidate<S>,
    ) -> ManagedServicePublicationOutcome
    where
        S: ManagedServiceContract + ?Sized,
    {
        let key = ManagedGenerationKey {
            owner: candidate.owner.clone(),
            generation_id: candidate.generation_id.clone(),
        };
        let implementation_identity = Arc::as_ptr(&candidate.implementation) as *const () as usize;
        let implementation_type_id = TypeId::of::<ManagedServiceHolder<S>>();
        let resource_signature = resource_signature(&candidate.resources);

        if let Some(existing) = self.generations.get(&key) {
            let exact = existing.metadata.capability_id == candidate.capability_id
                && existing.metadata.interface_id == candidate.interface_id
                && existing.implementation_identity == implementation_identity
                && existing.implementation_type_id == implementation_type_id
                && existing.resource_signature == resource_signature;
            let exact = exact
                && existing.active_call_duration == candidate.active_call_duration
                && existing.cleanup_duration == candidate.cleanup_duration;
            if exact
                && existing.state == ManagedServiceGenerationState::Accepting
                && self.active.get(&candidate.capability_id) == Some(&key)
                && !self.shutdown
            {
                let table = self.admission_table(None);
                self.registry
                    .replace(table)
                    .expect("exact managed generation transfer must preserve a valid table");
                return ManagedServicePublicationOutcome::Unchanged {
                    cleanup: ManagedCleanupLaunch::NotRequired,
                };
            }
        }

        let retained_controller_identities = self
            .generations
            .values()
            .filter(|service| service.runtime.retains_resources())
            .flat_map(|service| &service.resource_signature)
            .map(|resource| resource.controller_identity)
            .collect::<BTreeSet<_>>();
        if candidate
            .resources
            .iter()
            .any(|resource| retained_controller_identities.contains(&resource.controller_identity))
        {
            let resources =
                ManagedResourceOwner::new(candidate.composition_generation_id.clone(), key.clone());
            let mut registered = false;
            for resource in &candidate.resources {
                if retained_controller_identities.contains(&resource.controller_identity) {
                    continue;
                }
                registered = true;
                resources
                    .register(
                        resource.id.clone(),
                        resource.kind,
                        resource.assurance,
                        resource.dependencies.clone(),
                        Arc::clone(&resource.controller),
                    )
                    .expect("validated candidate resource registration must succeed");
            }
            let cleanup = if registered {
                let _ = resources.validate_and_freeze();
                resources.freeze_for_rejected_candidate_cleanup();
                Some(
                    self.cleanup_rejected_owner(resources, candidate.cleanup_duration)
                        .await,
                )
            } else {
                None
            };
            return ManagedServicePublicationOutcome::Rejected {
                reason: "managed candidate aliases a retained resource controller".into(),
                cleanup,
            };
        }

        let resources =
            ManagedResourceOwner::new(candidate.composition_generation_id.clone(), key.clone());
        let mut preparation_error = None;
        for resource in &candidate.resources {
            if let Err(error) = resources.register(
                resource.id.clone(),
                resource.kind,
                resource.assurance,
                resource.dependencies.clone(),
                Arc::clone(&resource.controller),
            ) {
                preparation_error = Some(error.to_string());
                break;
            }
        }
        if preparation_error.is_none()
            && let Err(error) = resources.validate_and_freeze()
        {
            preparation_error = Some(error.to_string());
        }
        resources.freeze_for_rejected_candidate_cleanup();

        let rejection = preparation_error.or_else(|| {
            if self.shutdown {
                Some("managed service publication is closed after shutdown".into())
            } else if self.generations.contains_key(&key) {
                Some(format!(
                    "managed service contract changed without changing generation: {}",
                    candidate.owner.as_str()
                ))
            } else {
                None
            }
        });
        if let Some(reason) = rejection {
            let cleanup = self
                .cleanup_rejected_owner(resources, candidate.cleanup_duration)
                .await;
            return ManagedServicePublicationOutcome::Rejected {
                reason,
                cleanup: Some(cleanup),
            };
        }

        let runtime = ManagedGenerationRuntime::new(key.clone());
        if let Err(error) = runtime.attach_resources(Arc::clone(&resources)) {
            let cleanup = self
                .cleanup_rejected_owner(resources, candidate.cleanup_duration)
                .await;
            return ManagedServicePublicationOutcome::Rejected {
                reason: error.to_string(),
                cleanup: Some(cleanup),
            };
        }

        let replacing = self.active.get(&candidate.capability_id).cloned();
        let mut table = self.admission_table(replacing.as_ref());
        table.insert(
            key.clone(),
            ManagedServiceGenerationState::Accepting,
            Arc::clone(&runtime),
        );
        let publication_point = match self.registry.replace(table) {
            Ok(publication_point) => publication_point,
            Err(error) => {
                let cleanup = self
                    .cleanup_rejected_owner(resources, candidate.cleanup_duration)
                    .await;
                return ManagedServicePublicationOutcome::Rejected {
                    reason: error.to_string(),
                    cleanup: Some(cleanup),
                };
            }
        };

        if let Some(old_key) = &replacing {
            self.generations
                .get_mut(old_key)
                .expect("active managed service generation must exist")
                .state = ManagedServiceGenerationState::Draining;
        }
        let implementation: Arc<dyn Any + Send + Sync> = Arc::new(ManagedServiceHolder {
            service: candidate.implementation,
        });
        self.generations.insert(
            key.clone(),
            ManagedPublishedService {
                metadata: ManagedPublishedServiceMetadata {
                    capability_id: candidate.capability_id.clone(),
                    interface_id: candidate.interface_id,
                    owner: candidate.owner,
                    generation_id: candidate.generation_id,
                },
                implementation,
                implementation_identity,
                implementation_type_id,
                resource_signature,
                active_call_duration: candidate.active_call_duration,
                cleanup_duration: candidate.cleanup_duration,
                runtime,
                state: ManagedServiceGenerationState::Accepting,
            },
        );
        self.active.insert(candidate.capability_id, key);

        let cleanup = replacing.map_or(ManagedCleanupLaunch::NotRequired, |old_key| {
            let old = self
                .generations
                .get(&old_key)
                .expect("replaced managed generation must remain retained");
            match old.runtime.start_generation_cleanup(
                publication_point + old.active_call_duration,
                old.cleanup_duration,
            ) {
                Ok(()) => ManagedCleanupLaunch::Started,
                Err(error) => ManagedCleanupLaunch::Failed(error.to_string()),
            }
        });
        ManagedServicePublicationOutcome::Published { cleanup }
    }

    pub(crate) fn service<S>(
        &self,
        capability_id: &RuntimeCapabilityId,
        interface_id: &RuntimeServiceInterfaceId,
    ) -> anyhow::Result<Option<ManagedServiceHandle<S>>>
    where
        S: ManagedServiceContract + ?Sized,
    {
        let Some(key) = self.active.get(capability_id) else {
            return Ok(None);
        };
        let published = self
            .generations
            .get(key)
            .expect("active managed generation must be retained");
        if &published.metadata.interface_id != interface_id {
            anyhow::bail!(
                "managed service {} exposes interface {}, not {}",
                capability_id.as_str(),
                published.metadata.interface_id.as_str(),
                interface_id.as_str()
            );
        }
        let service = published
            .implementation
            .downcast_ref::<ManagedServiceHolder<S>>()
            .map(|holder| Arc::clone(&holder.service))
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "managed service {} has an incompatible implementation type for interface {}",
                    capability_id.as_str(),
                    interface_id.as_str()
                )
            })?;
        Ok(Some(ManagedServiceHandle::new(
            capability_id.clone(),
            published.metadata.owner.clone(),
            published.metadata.generation_id.clone(),
            Arc::clone(&self.registry),
            Arc::clone(&published.runtime),
            service,
        )))
    }

    pub(crate) fn published_metadata(&self) -> Vec<ManagedPublishedServiceMetadata> {
        self.active
            .values()
            .filter_map(|key| self.generations.get(key))
            .map(|service| service.metadata.clone())
            .collect()
    }

    pub(crate) async fn shutdown(&mut self) -> ManagedServiceShutdownReport {
        if !self.shutdown {
            let table = self.admission_table_with_all_accepting_closed();
            self.registry
                .replace(table)
                .expect("closing valid managed admission gates must succeed");
            self.shutdown = true;
            self.active.clear();
            for service in self.generations.values_mut() {
                if service.state == ManagedServiceGenerationState::Accepting {
                    service.state = ManagedServiceGenerationState::Draining;
                }
                if service.state == ManagedServiceGenerationState::Draining
                    && !service.runtime.generation_cleanup_started()
                {
                    let _ = service.runtime.start_generation_cleanup(
                        tokio::time::Instant::now() + service.active_call_duration,
                        service.cleanup_duration,
                    );
                }
            }
        }

        let keys = self.generations.keys().cloned().collect::<Vec<_>>();
        let mut report = ManagedServiceShutdownReport::default();
        for key in keys {
            let (runtime, cleanup_duration) = {
                let service = self.generations.get(&key).expect("generation exists");
                (Arc::clone(&service.runtime), service.cleanup_duration)
            };
            let joined_running_retry = runtime.generation_retry_running();
            let mut result = if joined_running_retry {
                runtime.join_generation_resource_retry().await
            } else if runtime.generation_cleanup_complete() {
                runtime
                    .generation_cleanup_result()
                    .expect("completed managed cleanup must have a result")
                    .map_err(anyhow::Error::msg)
            } else {
                runtime.join_generation_cleanup().await
            };
            if result
                .as_ref()
                .is_ok_and(|cleanup| !cleanup.resources.strict_resources_settled())
                && !joined_running_retry
            {
                result = runtime
                    .retry_generation_resource_cleanup(
                        tokio::time::Instant::now() + cleanup_duration,
                    )
                    .await;
            }
            let next_state = match &result {
                Ok(cleanup) if cleanup.resources.strict_resources_settled() => {
                    ManagedServiceGenerationState::Retired
                }
                _ => ManagedServiceGenerationState::Degraded,
            };
            let service = self.generations.get_mut(&key).expect("generation exists");
            if service.state != next_state {
                if let Err(error) = self.registry.transition(&key, next_state) {
                    result = Err(error);
                } else {
                    service.state = next_state;
                }
            }
            report.generations.push(ManagedShutdownGenerationResult {
                owner: key.owner.clone(),
                generation_id: key.generation_id.clone(),
                result: result.map_err(|error| error.to_string()),
            });
        }

        let releasable = self
            .generations
            .iter()
            .filter(|(_, service)| {
                service.state == ManagedServiceGenerationState::Retired
                    && !service.runtime.retains_resources()
            })
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        if !releasable.is_empty() {
            let mut table = ManagedAdmissionTable::default();
            for (key, service) in &self.generations {
                if !releasable.contains(key) {
                    table.insert(key.clone(), service.state, Arc::clone(&service.runtime));
                }
            }
            self.registry
                .replace(table)
                .expect("settled retired managed generations must be removable");
            for key in releasable {
                self.generations.remove(&key);
            }
        }

        let mut retained = Vec::new();
        for rejected in self.rejected_resource_owners.drain(..) {
            let joined = rejected.owner.join_running_cleanup().await;
            let cleanup = if joined.all_resources_settled() {
                joined
            } else {
                rejected
                    .owner
                    .retry_cleanup_until(tokio::time::Instant::now() + rejected.cleanup_duration)
                    .await
                    .unwrap_or_else(|_| rejected.owner.report())
            };
            if !cleanup.all_resources_settled() {
                retained.push(rejected);
            }
            report.rejected_candidates.push(cleanup);
        }
        self.rejected_resource_owners = retained;
        report
    }

    fn admission_table(&self, draining: Option<&ManagedGenerationKey>) -> ManagedAdmissionTable {
        let mut table = ManagedAdmissionTable::default();
        for (key, service) in &self.generations {
            let state = if draining == Some(key) {
                ManagedServiceGenerationState::Draining
            } else {
                service.state
            };
            table.insert(key.clone(), state, Arc::clone(&service.runtime));
        }
        table
    }

    fn admission_table_with_all_accepting_closed(&self) -> ManagedAdmissionTable {
        let mut table = ManagedAdmissionTable::default();
        for (key, service) in &self.generations {
            let state = if service.state == ManagedServiceGenerationState::Accepting {
                ManagedServiceGenerationState::Draining
            } else {
                service.state
            };
            table.insert(key.clone(), state, Arc::clone(&service.runtime));
        }
        table
    }

    async fn cleanup_rejected_owner(
        &mut self,
        owner: Arc<ManagedResourceOwner>,
        cleanup_duration: Duration,
    ) -> ManagedResourceCleanupReport {
        self.rejected_resource_owners.push(RejectedResourceOwner {
            owner: Arc::clone(&owner),
            cleanup_duration,
        });
        let cleanup = owner
            .cleanup_candidate_until(tokio::time::Instant::now() + cleanup_duration)
            .await
            .unwrap_or_else(|_| owner.report());
        if cleanup.all_resources_settled() {
            self.rejected_resource_owners
                .retain(|retained| !Arc::ptr_eq(&retained.owner, &owner));
        }
        cleanup
    }
}

fn resource_signature(resources: &[ManagedResourceRegistration]) -> Vec<ManagedResourceSignature> {
    let mut signature = resources
        .iter()
        .map(|resource| {
            let mut dependencies = resource.dependencies.clone();
            dependencies.sort();
            ManagedResourceSignature {
                id: resource.id.clone(),
                kind: resource.kind,
                assurance: resource.assurance,
                dependencies,
                controller_identity: resource.controller_identity,
            }
        })
        .collect::<Vec<_>>();
    signature.sort_by(|left, right| left.id.cmp(&right.id));
    signature
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use omegon_traits::{
        ManagedCallContext, ManagedResourceSettlementFuture, ManagedServiceCallError,
        ManagedServiceFuture, RuntimeCleanupState,
    };
    use tokio::sync::Notify;

    use super::*;

    struct TestService {
        value: &'static str,
    }

    enum Request {
        Read,
        Wait,
    }

    impl ManagedServiceContract for TestService {
        type Request = Request;
        type Response = &'static str;
        type Error = String;

        fn execute<'a>(
            &'a self,
            request: Self::Request,
            context: ManagedCallContext,
        ) -> ManagedServiceFuture<'a, Self::Response, Self::Error> {
            Box::pin(async move {
                match request {
                    Request::Read => Ok(self.value),
                    Request::Wait => {
                        context.cancellation.cancelled().await;
                        Ok(self.value)
                    }
                }
            })
        }
    }

    struct TestResource {
        stops: AtomicUsize,
        forces: AtomicUsize,
        settled: AtomicBool,
        settle_on_stop: bool,
        changed: Notify,
    }

    impl TestResource {
        fn new(settle_on_stop: bool) -> Arc<Self> {
            Arc::new(Self {
                stops: AtomicUsize::new(0),
                forces: AtomicUsize::new(0),
                settled: AtomicBool::new(false),
                settle_on_stop,
                changed: Notify::new(),
            })
        }

        fn settle(&self) {
            self.settled.store(true, Ordering::Release);
            self.changed.notify_waiters();
        }
    }

    impl ManagedResourceController for TestResource {
        fn request_stop(&self) {
            self.stops.fetch_add(1, Ordering::AcqRel);
            if self.settle_on_stop {
                self.settle();
            }
        }

        fn force_stop(&self) {
            self.forces.fetch_add(1, Ordering::AcqRel);
        }

        fn await_settled(&self) -> ManagedResourceSettlementFuture<'_> {
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

    fn id(value: &str) -> RuntimeCapabilityId {
        RuntimeCapabilityId::new(value).unwrap()
    }

    fn interface() -> RuntimeServiceInterfaceId {
        RuntimeServiceInterfaceId::new("interface:test-managed-v1").unwrap()
    }

    fn candidate(
        generation: &str,
        service: Arc<TestService>,
        resource: Arc<TestResource>,
    ) -> ManagedServiceCandidate<TestService> {
        let controller: Arc<dyn ManagedResourceController> = resource;
        ManagedServiceCandidate::new(
            RuntimeCompositionGenerationId::new(format!("composition:{generation}")).unwrap(),
            id("service:test-managed"),
            interface(),
            RuntimeContributionId::new("feature:test-managed").unwrap(),
            RuntimeContributionGenerationId::new(generation).unwrap(),
            Duration::from_millis(10),
            Duration::from_millis(10),
            vec![ManagedResourceRegistration::new(
                RuntimeContributionResourceId::new(format!("resource:{generation}")).unwrap(),
                RuntimeOwnedResourceKind::Task,
                RuntimeCleanupAssurance::Strict,
                Vec::new(),
                controller,
            )],
            service,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn rg01_rejected_candidate_preserves_prior_managed_service() {
        let mut bus = ManagedServiceBus::default();
        let original_resource = TestResource::new(true);
        bus.publish(candidate(
            "managed:v1",
            Arc::new(TestService { value: "old" }),
            Arc::clone(&original_resource),
        ))
        .await;
        let handle = bus
            .service::<TestService>(&id("service:test-managed"), &interface())
            .unwrap()
            .unwrap();

        let rejected_resource = TestResource::new(true);
        let outcome = bus
            .publish(candidate(
                "managed:v1",
                Arc::new(TestService { value: "changed" }),
                Arc::clone(&rejected_resource),
            ))
            .await;
        assert!(matches!(
            outcome,
            ManagedServicePublicationOutcome::Rejected { .. }
        ));
        assert_eq!(handle.invoke(Request::Read).await.unwrap(), "old");
        assert_eq!(original_resource.stops.load(Ordering::Acquire), 0);
        assert_eq!(rejected_resource.stops.load(Ordering::Acquire), 1);

        let aliased = bus
            .publish(candidate(
                "managed:v1",
                Arc::new(TestService { value: "aliased" }),
                Arc::clone(&original_resource),
            ))
            .await;
        assert!(matches!(
            aliased,
            ManagedServicePublicationOutcome::Rejected { cleanup: None, .. }
        ));
        assert_eq!(handle.invoke(Request::Read).await.unwrap(), "old");
        assert_eq!(original_resource.stops.load(Ordering::Acquire), 0);
    }

    #[tokio::test]
    async fn rg05_exact_generation_transfer_reuses_runtime_and_owner_without_cleanup() {
        let mut bus = ManagedServiceBus::default();
        let service = Arc::new(TestService { value: "same" });
        let resource = TestResource::new(true);
        bus.publish(candidate(
            "managed:v1",
            Arc::clone(&service),
            Arc::clone(&resource),
        ))
        .await;
        let before = bus
            .service::<TestService>(&id("service:test-managed"), &interface())
            .unwrap()
            .unwrap();
        let runtime = Arc::clone(
            &bus.generations
                .values()
                .next()
                .expect("published generation")
                .runtime,
        );
        let outcome = bus
            .publish(candidate(
                "managed:v1",
                Arc::clone(&service),
                Arc::clone(&resource),
            ))
            .await;
        let after = bus
            .service::<TestService>(&id("service:test-managed"), &interface())
            .unwrap()
            .unwrap();

        assert_eq!(
            outcome,
            ManagedServicePublicationOutcome::Unchanged {
                cleanup: ManagedCleanupLaunch::NotRequired
            }
        );
        assert_eq!(before.invoke(Request::Read).await.unwrap(), "same");
        assert_eq!(after.invoke(Request::Read).await.unwrap(), "same");
        assert!(Arc::ptr_eq(
            &runtime,
            &bus.generations
                .values()
                .next()
                .expect("transferred generation")
                .runtime
        ));
        assert_eq!(resource.stops.load(Ordering::Acquire), 0);

        let mut changed_deadline = candidate("managed:v1", service, Arc::clone(&resource));
        changed_deadline.cleanup_duration = Duration::from_millis(20);
        assert!(matches!(
            bus.publish(changed_deadline).await,
            ManagedServicePublicationOutcome::Rejected { .. }
        ));
    }

    #[tokio::test]
    async fn rg10_changed_cleanup_and_shutdown_are_caller_independent_and_idempotent() {
        let bus = Arc::new(tokio::sync::Mutex::new(ManagedServiceBus::default()));
        let old_resource = TestResource::new(false);
        let old_handle = {
            let mut bus = bus.lock().await;
            bus.publish(candidate(
                "managed:v1",
                Arc::new(TestService { value: "old" }),
                Arc::clone(&old_resource),
            ))
            .await;
            bus.service::<TestService>(&id("service:test-managed"), &interface())
                .unwrap()
                .unwrap()
        };
        let call = tokio::spawn(async move { old_handle.invoke(Request::Wait).await });
        tokio::task::yield_now().await;
        let new_resource = TestResource::new(false);
        {
            let mut bus = bus.lock().await;
            let outcome = bus
                .publish(candidate(
                    "managed:v2",
                    Arc::new(TestService { value: "new" }),
                    Arc::clone(&new_resource),
                ))
                .await;
            assert!(matches!(
                outcome,
                ManagedServicePublicationOutcome::Published {
                    cleanup: ManagedCleanupLaunch::Started
                }
            ));
        }
        assert!(matches!(
            call.await.unwrap(),
            Err(ManagedServiceCallError::Cancelled)
        ));
        old_resource.settle();

        let cancelled_shutdown = tokio::spawn({
            let bus = Arc::clone(&bus);
            async move { bus.lock().await.shutdown().await }
        });
        while new_resource.stops.load(Ordering::Acquire) == 0 {
            tokio::task::yield_now().await;
        }
        cancelled_shutdown.abort();
        assert!(cancelled_shutdown.await.unwrap_err().is_cancelled());
        new_resource.settle();

        let first = bus.lock().await.shutdown().await;
        let second = bus.lock().await.shutdown().await;
        assert!(first.generations.iter().all(|generation| {
            generation
                .result
                .as_ref()
                .is_ok_and(|cleanup| cleanup.resources.strict_resources_settled())
        }));
        assert!(second.generations.is_empty());
    }

    #[tokio::test]
    async fn shutdown_retains_unresolved_strict_candidate_owner() {
        let mut bus = ManagedServiceBus::default();
        let resource = TestResource::new(false);
        bus.shutdown().await;
        let outcome = bus
            .publish(candidate(
                "managed:v1",
                Arc::new(TestService { value: "rejected" }),
                Arc::clone(&resource),
            ))
            .await;
        let ManagedServicePublicationOutcome::Rejected {
            cleanup: Some(cleanup),
            ..
        } = outcome
        else {
            panic!("publication after shutdown must reject");
        };
        assert_eq!(
            cleanup.records[0].cleanup_state,
            RuntimeCleanupState::Degraded
        );
        assert_eq!(bus.rejected_resource_owners.len(), 1);
        resource.settle();
        let report = bus.shutdown().await;
        assert!(report.rejected_candidates[0].strict_resources_settled());
        assert!(bus.rejected_resource_owners.is_empty());
    }

    #[tokio::test]
    async fn event_bus_owns_managed_publication_lookup_and_shutdown() {
        let mut bus = crate::bus::EventBus::new();
        let resource = TestResource::new(true);
        assert!(matches!(
            bus.publish_managed_service(candidate(
                "managed:event-bus-v1",
                Arc::new(TestService { value: "owned" }),
                resource,
            ))
            .await,
            ManagedServicePublicationOutcome::Published { .. }
        ));
        let handle = bus
            .managed_service::<TestService>(&id("service:test-managed"), &interface())
            .unwrap()
            .unwrap();
        assert_eq!(handle.invoke(Request::Read).await.unwrap(), "owned");
        assert_eq!(bus.managed_service_metadata().len(), 1);

        let report = bus.shutdown_managed_services().await;
        assert!(
            report.generations[0]
                .result
                .as_ref()
                .is_ok_and(|cleanup| cleanup.resources.all_resources_settled())
        );
        assert!(
            bus.managed_service::<TestService>(&id("service:test-managed"), &interface())
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn rg12_empty_managed_sidecar_does_not_change_no_resource_storage() {
        let bus = ManagedServiceBus::default();
        assert!(bus.active.is_empty());
        assert!(bus.generations.is_empty());
        assert!(bus.published_metadata().is_empty());
    }
}
