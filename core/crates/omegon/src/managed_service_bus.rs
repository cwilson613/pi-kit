//! EventBus-owned lifecycle for resource-bearing in-process services.

use std::any::{Any, TypeId};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use omegon_traits::{
    ManagedResourceController, ManagedServiceContract, ManagedServiceGenerationState,
    RUNTIME_CONTRIBUTION_SCHEMA_VERSION, RuntimeCapabilityId, RuntimeCleanupAssurance,
    RuntimeCleanupRequirement, RuntimeCleanupState, RuntimeCompositionGenerationId,
    RuntimeContributionGenerationId, RuntimeContributionId, RuntimeContributionLifecycleRecord,
    RuntimeContributionLifecycleState, RuntimeContributionResourceId, RuntimeDiagnosticCode,
    RuntimeLifecycleBoundary, RuntimeOwnedResourceKind, RuntimeServiceInterfaceId,
};

use crate::service_generation::{
    ManagedAdmissionRegistry, ManagedAdmissionTable, ManagedGenerationCleanupReport,
    ManagedGenerationKey, ManagedGenerationRuntime, ManagedResourceCleanupReport,
    ManagedResourceOwner, ManagedServiceHandle,
};
use crate::surfaces::diagnostics::{
    ManagedOwnerDiagnosticProjection, ManagedOwnerDisposition, ManagedResourceDiagnosticProjection,
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
        validate_generation_candidate(active_call_duration, cleanup_duration, &resources)?;
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

    pub(crate) fn capability_id(&self) -> &RuntimeCapabilityId {
        &self.capability_id
    }
}

struct ManagedCandidateImplementation {
    interface_id: RuntimeServiceInterfaceId,
    implementation: Arc<dyn Any + Send + Sync>,
    implementation_identity: usize,
    implementation_type_id: TypeId,
}

pub(crate) struct ManagedGenerationCandidate {
    composition_generation_id: RuntimeCompositionGenerationId,
    owner: RuntimeContributionId,
    generation_id: RuntimeContributionGenerationId,
    active_call_duration: Duration,
    cleanup_duration: Duration,
    resources: Vec<ManagedResourceRegistration>,
    implementations: BTreeMap<RuntimeCapabilityId, ManagedCandidateImplementation>,
}

impl ManagedGenerationCandidate {
    pub(crate) fn new(
        composition_generation_id: RuntimeCompositionGenerationId,
        owner: RuntimeContributionId,
        generation_id: RuntimeContributionGenerationId,
        active_call_duration: Duration,
        cleanup_duration: Duration,
        resources: Vec<ManagedResourceRegistration>,
    ) -> anyhow::Result<Self> {
        validate_generation_candidate(active_call_duration, cleanup_duration, &resources)?;
        Ok(Self {
            composition_generation_id,
            owner,
            generation_id,
            active_call_duration,
            cleanup_duration,
            resources,
            implementations: BTreeMap::new(),
        })
    }

    pub(crate) fn add_service<S>(
        &mut self,
        capability_id: RuntimeCapabilityId,
        interface_id: RuntimeServiceInterfaceId,
        implementation: Arc<S>,
    ) -> anyhow::Result<()>
    where
        S: ManagedServiceContract + ?Sized,
    {
        let implementation_identity = Arc::as_ptr(&implementation) as *const () as usize;
        let implementation_type_id = TypeId::of::<ManagedServiceHolder<S>>();
        let implementation: Arc<dyn Any + Send + Sync> = Arc::new(ManagedServiceHolder {
            service: implementation,
        });
        if self.implementations.contains_key(&capability_id) {
            anyhow::bail!(
                "duplicate managed service capability {}",
                capability_id.as_str()
            );
        }
        self.implementations.insert(
            capability_id,
            ManagedCandidateImplementation {
                interface_id,
                implementation,
                implementation_identity,
                implementation_type_id,
            },
        );
        Ok(())
    }

    pub(crate) fn active_call_duration(&self) -> Duration {
        self.active_call_duration
    }

    pub(crate) fn cleanup_duration(&self) -> Duration {
        self.cleanup_duration
    }

    pub(crate) fn cleanup_requirement(&self) -> RuntimeCleanupRequirement {
        if self
            .resources
            .iter()
            .all(|resource| resource.assurance == RuntimeCleanupAssurance::Strict)
        {
            RuntimeCleanupRequirement::Strict
        } else {
            RuntimeCleanupRequirement::BestEffort
        }
    }

    pub(crate) fn services(
        &self,
    ) -> impl Iterator<Item = (&RuntimeCapabilityId, &RuntimeServiceInterfaceId)> {
        self.implementations
            .iter()
            .map(|(capability_id, implementation)| (capability_id, &implementation.interface_id))
    }

    pub(crate) fn rebind(
        &mut self,
        composition_generation_id: RuntimeCompositionGenerationId,
        owner: RuntimeContributionId,
        generation_id: RuntimeContributionGenerationId,
    ) {
        self.composition_generation_id = composition_generation_id;
        self.owner = owner;
        self.generation_id = generation_id;
    }
}

impl<S> From<ManagedServiceCandidate<S>> for ManagedGenerationCandidate
where
    S: ManagedServiceContract + ?Sized,
{
    fn from(candidate: ManagedServiceCandidate<S>) -> Self {
        let ManagedServiceCandidate {
            composition_generation_id,
            capability_id,
            interface_id,
            owner,
            generation_id,
            active_call_duration,
            cleanup_duration,
            resources,
            implementation,
        } = candidate;
        let mut generation = Self {
            composition_generation_id,
            owner,
            generation_id,
            active_call_duration,
            cleanup_duration,
            resources,
            implementations: BTreeMap::new(),
        };
        generation
            .add_service(capability_id, interface_id, implementation)
            .expect("a one-service candidate has one unique capability");
        generation
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

struct ManagedPublishedImplementation {
    metadata: ManagedPublishedServiceMetadata,
    implementation: Arc<dyn Any + Send + Sync>,
    implementation_identity: usize,
    implementation_type_id: TypeId,
}

struct ManagedPublishedGeneration {
    implementations: BTreeMap<RuntimeCapabilityId, ManagedPublishedImplementation>,
    resource_signature: Vec<ManagedResourceSignature>,
    active_call_duration: Duration,
    cleanup_duration: Duration,
    runtime: Arc<ManagedGenerationRuntime>,
    state: ManagedServiceGenerationState,
    attempt_id: u64,
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
pub(crate) enum ManagedServiceBatchPublicationOutcome {
    Rejected {
        reason: String,
        cleanup: Vec<ManagedResourceCleanupReport>,
    },
    Published {
        cleanup: Vec<ManagedCleanupLaunch>,
    },
    Unchanged,
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

impl ManagedServiceShutdownReport {
    pub(crate) fn all_resources_settled(&self) -> bool {
        self.generations.iter().all(|generation| {
            generation
                .result
                .as_ref()
                .is_ok_and(|cleanup| cleanup.resources.all_resources_settled())
        }) && self
            .rejected_candidates
            .iter()
            .all(ManagedResourceCleanupReport::all_resources_settled)
    }
}

#[derive(Clone)]
struct RejectedResourceOwner {
    owner: Arc<ManagedResourceOwner>,
    cleanup_duration: Duration,
    attempt_id: u64,
    reason: String,
}

/// Terminal managed-owner evidence is intentionally small and DTO-only.
const MANAGED_DIAGNOSTIC_HISTORY_LIMIT: usize = 128;

pub(crate) struct ManagedServiceBus {
    registry: Arc<ManagedAdmissionRegistry>,
    generations: BTreeMap<ManagedGenerationKey, ManagedPublishedGeneration>,
    active: BTreeMap<RuntimeCapabilityId, ManagedGenerationKey>,
    graph_managed: BTreeSet<ManagedGenerationKey>,
    rejected_resource_owners: Vec<RejectedResourceOwner>,
    diagnostic_history: VecDeque<ManagedOwnerDiagnosticProjection>,
    next_attempt_id: u64,
    shutdown: bool,
}

impl Default for ManagedServiceBus {
    fn default() -> Self {
        Self {
            registry: Arc::new(ManagedAdmissionRegistry::default()),
            generations: BTreeMap::new(),
            active: BTreeMap::new(),
            graph_managed: BTreeSet::new(),
            rejected_resource_owners: Vec::new(),
            diagnostic_history: VecDeque::new(),
            next_attempt_id: 1,
            shutdown: false,
        }
    }
}

impl ManagedServiceBus {
    pub(crate) fn requires_ownership_retention(&self) -> bool {
        !self.generations.is_empty() || !self.rejected_resource_owners.is_empty()
    }

    pub(crate) fn has_graph_managed_generations(&self) -> bool {
        !self.graph_managed.is_empty()
    }

    pub(crate) fn graph_managed_identities(
        &self,
    ) -> Vec<(RuntimeContributionId, RuntimeContributionGenerationId)> {
        self.graph_managed
            .iter()
            .map(|key| (key.owner.clone(), key.generation_id.clone()))
            .collect()
    }

    pub(crate) fn is_exact_generation(
        &self,
        candidate: &ManagedGenerationCandidate,
        owner: &RuntimeContributionId,
        generation_id: &RuntimeContributionGenerationId,
    ) -> bool {
        let key = ManagedGenerationKey {
            owner: owner.clone(),
            generation_id: generation_id.clone(),
        };
        self.generations.get(&key).is_some_and(|existing| {
            exact_generation(existing, candidate)
                && existing.state == ManagedServiceGenerationState::Accepting
                && existing
                    .implementations
                    .keys()
                    .all(|capability_id| self.active.get(capability_id) == Some(&key))
        })
    }

    pub(crate) fn reconcile_managed_diagnostics(&mut self) -> anyhow::Result<()> {
        let completed = self
            .generations
            .iter()
            .filter(|(_, generation)| {
                generation.state != ManagedServiceGenerationState::Accepting
                    && generation.runtime.generation_cleanup_complete()
                    && !generation.runtime.generation_retry_running()
            })
            .map(|(key, generation)| {
                generation
                    .runtime
                    .generation_cleanup_result()
                    .map(|result| (key.clone(), result))
                    .ok_or_else(|| anyhow::anyhow!("completed managed cleanup has no result"))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        let mut releasable = Vec::new();
        for (key, result) in completed {
            let settled = result
                .as_ref()
                .is_ok_and(|report| report.resources.all_resources_settled());
            let next_state = if settled {
                ManagedServiceGenerationState::Retired
            } else {
                ManagedServiceGenerationState::Degraded
            };
            let generation = self
                .generations
                .get(&key)
                .expect("sampled managed generation must remain retained");
            if generation.state != next_state {
                self.registry.transition(&key, next_state)?;
                self.generations
                    .get_mut(&key)
                    .expect("sampled managed generation must remain retained")
                    .state = next_state;
            }
            if settled {
                let report = result.expect("settled managed cleanup has a report");
                let generation = self
                    .generations
                    .get(&key)
                    .expect("settled managed generation must remain retained");
                let snapshot = published_projection(key.clone(), generation, report.resources)?;
                self.archive_diagnostic(snapshot);
                releasable.push(key);
            }
        }

        if !releasable.is_empty() {
            let releasable = releasable.into_iter().collect::<BTreeSet<_>>();
            let mut table = ManagedAdmissionTable::default();
            for (key, generation) in &self.generations {
                if !releasable.contains(key) {
                    table.insert(
                        key.clone(),
                        generation.state,
                        Arc::clone(&generation.runtime),
                    );
                }
            }
            self.registry.replace(table)?;
            for key in releasable {
                self.generations.remove(&key);
            }
        }

        let settled_rejected = self
            .rejected_resource_owners
            .iter()
            .filter(|rejected| !rejected.owner.cleanup_running())
            .filter_map(|rejected| {
                let report = rejected.owner.report();
                report
                    .all_resources_settled()
                    .then(|| (rejected.attempt_id, rejected.reason.clone(), report))
            })
            .collect::<Vec<_>>();
        for (attempt_id, reason, report) in &settled_rejected {
            self.archive_diagnostic(rejected_projection(
                *attempt_id,
                reason,
                report.clone(),
                false,
            )?);
        }
        let settled_attempts = settled_rejected
            .into_iter()
            .map(|(attempt_id, _, _)| attempt_id)
            .collect::<BTreeSet<_>>();
        self.rejected_resource_owners
            .retain(|rejected| !settled_attempts.contains(&rejected.attempt_id));
        Ok(())
    }

    pub(crate) fn managed_diagnostic_records(
        &mut self,
    ) -> anyhow::Result<Vec<ManagedOwnerDiagnosticProjection>> {
        self.reconcile_managed_diagnostics()?;
        let mut records = self.diagnostic_history.iter().cloned().collect::<Vec<_>>();
        for (key, generation) in &self.generations {
            records.push(published_projection(
                key.clone(),
                generation,
                generation
                    .runtime
                    .resource_report()
                    .ok_or_else(|| anyhow::anyhow!("managed generation has no resource report"))?,
            )?);
        }
        for rejected in &self.rejected_resource_owners {
            records.push(rejected_projection(
                rejected.attempt_id,
                &rejected.reason,
                rejected.owner.report(),
                rejected.owner.cleanup_running(),
            )?);
        }
        records.sort_by_key(|record| record.attempt_id);
        Ok(records)
    }

    fn allocate_attempt_id(&mut self) -> u64 {
        let attempt_id = self.next_attempt_id;
        self.next_attempt_id = self
            .next_attempt_id
            .checked_add(1)
            .expect("managed diagnostic attempt id exhausted");
        attempt_id
    }

    fn archive_diagnostic(&mut self, snapshot: ManagedOwnerDiagnosticProjection) {
        if self
            .diagnostic_history
            .iter()
            .any(|existing| existing.attempt_id == snapshot.attempt_id)
        {
            return;
        }
        if self.diagnostic_history.len() == MANAGED_DIAGNOSTIC_HISTORY_LIMIT {
            self.diagnostic_history.pop_front();
        }
        self.diagnostic_history.push_back(snapshot);
    }

    pub(crate) async fn publish<S>(
        &mut self,
        candidate: ManagedServiceCandidate<S>,
    ) -> ManagedServicePublicationOutcome
    where
        S: ManagedServiceContract + ?Sized,
    {
        match self.publish_candidates(vec![candidate.into()], false).await {
            ManagedServiceBatchPublicationOutcome::Rejected {
                reason,
                mut cleanup,
            } => ManagedServicePublicationOutcome::Rejected {
                reason,
                cleanup: cleanup.pop(),
            },
            ManagedServiceBatchPublicationOutcome::Published { cleanup } => {
                ManagedServicePublicationOutcome::Published {
                    cleanup: collapse_cleanup_launches(cleanup),
                }
            }
            ManagedServiceBatchPublicationOutcome::Unchanged => {
                ManagedServicePublicationOutcome::Unchanged {
                    cleanup: ManagedCleanupLaunch::NotRequired,
                }
            }
        }
    }

    pub(crate) async fn publish_composition(
        &mut self,
        candidates: Vec<ManagedGenerationCandidate>,
    ) -> ManagedServiceBatchPublicationOutcome {
        self.publish_candidates(candidates, true).await
    }

    pub(crate) async fn reject_composition_candidates(
        &mut self,
        candidates: &[ManagedGenerationCandidate],
        reason: &str,
    ) -> Vec<ManagedResourceCleanupReport> {
        let retained = self.retained_controller_identities();
        self.cleanup_rejected_candidates(candidates, &BTreeSet::new(), &retained, reason)
            .await
    }

    async fn publish_candidates(
        &mut self,
        candidates: Vec<ManagedGenerationCandidate>,
        whole_composition: bool,
    ) -> ManagedServiceBatchPublicationOutcome {
        let retained_controller_identities = self.retained_controller_identities();
        let mut candidate_keys = BTreeSet::new();
        let mut duplicate_keys = BTreeSet::new();
        let mut candidate_capabilities = BTreeSet::new();
        let mut exact_keys = BTreeSet::new();
        let mut rejection = self
            .shutdown
            .then(|| "managed service publication is closed after shutdown".to_string());
        let replaces_graph_managed = !whole_composition
            && candidates.iter().any(|candidate| {
                candidate.implementations.keys().any(|capability_id| {
                    self.active
                        .get(capability_id)
                        .is_some_and(|key| self.graph_managed.contains(key))
                })
            });
        if replaces_graph_managed && rejection.is_none() {
            rejection =
                Some("direct managed publication cannot replace a graph-managed generation".into());
        }

        for candidate in &candidates {
            let key = generation_key(candidate);
            if !candidate_keys.insert(key.clone()) {
                duplicate_keys.insert(key.clone());
                exact_keys.remove(&key);
                if rejection.is_none() {
                    rejection = Some(format!(
                        "duplicate managed owner-generation candidate: {} {}",
                        key.owner.as_str(),
                        key.generation_id.as_str()
                    ));
                }
            }
            if candidate.implementations.is_empty() && rejection.is_none() {
                rejection = Some(format!(
                    "managed generation {} requires at least one service",
                    key.generation_id.as_str()
                ));
            }
            for capability_id in candidate.implementations.keys() {
                if !candidate_capabilities.insert(capability_id.clone()) && rejection.is_none() {
                    rejection = Some(format!(
                        "duplicate managed service capability {}",
                        capability_id.as_str()
                    ));
                }
            }
            if let Some(existing) = self.generations.get(&key) {
                if !duplicate_keys.contains(&key)
                    && exact_generation(existing, candidate)
                    && existing.state == ManagedServiceGenerationState::Accepting
                    && existing
                        .implementations
                        .keys()
                        .all(|capability_id| self.active.get(capability_id) == Some(&key))
                {
                    exact_keys.insert(key);
                } else if rejection.is_none() {
                    rejection = Some(format!(
                        "managed service contract changed without changing generation: {}",
                        candidate.owner.as_str()
                    ));
                }
            }
        }

        let mut seen_candidate_controllers = BTreeSet::new();
        for candidate in &candidates {
            if exact_keys.contains(&generation_key(candidate)) {
                continue;
            }
            for resource in &candidate.resources {
                if retained_controller_identities.contains(&resource.controller_identity)
                    && rejection.is_none()
                {
                    rejection =
                        Some("managed candidate aliases a retained resource controller".into());
                }
                if !seen_candidate_controllers.insert(resource.controller_identity)
                    && rejection.is_none()
                {
                    rejection =
                        Some("managed candidates cannot share one resource controller".into());
                }
            }
        }

        if let Some(reason) = rejection {
            let cleanup = self
                .cleanup_rejected_candidates(
                    &candidates,
                    &exact_keys,
                    &retained_controller_identities,
                    &reason,
                )
                .await;
            return ManagedServiceBatchPublicationOutcome::Rejected { reason, cleanup };
        }

        let mut desired_active = self.active.clone();
        if whole_composition {
            desired_active.retain(|_, key| !self.graph_managed.contains(key));
        }
        for candidate in &candidates {
            let key = generation_key(candidate);
            if exact_keys.contains(&key) {
                continue;
            }
            let replaced = candidate
                .implementations
                .keys()
                .filter_map(|capability_id| desired_active.get(capability_id).cloned())
                .collect::<BTreeSet<_>>();
            desired_active.retain(|_, active_key| !replaced.contains(active_key));
            for capability_id in candidate.implementations.keys() {
                desired_active.insert(capability_id.clone(), key.clone());
            }
        }
        if whole_composition {
            for candidate in &candidates {
                let key = generation_key(candidate);
                for capability_id in candidate.implementations.keys() {
                    desired_active.insert(capability_id.clone(), key.clone());
                }
            }
        }

        let desired_keys = desired_active.values().cloned().collect::<BTreeSet<_>>();
        let displaced = self
            .active
            .values()
            .filter(|key| !desired_keys.contains(*key))
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut prepared: Vec<PreparedGeneration> = Vec::new();
        let mut preparation_error = None;
        for candidate in candidates {
            let key = generation_key(&candidate);
            if exact_keys.contains(&key) {
                continue;
            }
            let resources =
                ManagedResourceOwner::new(candidate.composition_generation_id.clone(), key.clone());
            let attempt_id = self.allocate_attempt_id();
            let preparation = register_candidate_resources(&resources, &candidate.resources)
                .and_then(|()| resources.validate_and_freeze());
            if let Err(error) = preparation {
                resources.freeze_for_rejected_candidate_cleanup();
                if preparation_error.is_none() {
                    preparation_error = Some(error.to_string());
                }
            }
            let runtime = ManagedGenerationRuntime::new(key);
            if preparation_error.is_none()
                && let Err(error) = runtime.attach_resources(Arc::clone(&resources))
            {
                preparation_error = Some(error.to_string());
            }
            prepared.push(PreparedGeneration {
                candidate,
                resources,
                runtime,
                attempt_id,
            });
        }
        if let Some(reason) = preparation_error {
            let cleanup = self
                .cleanup_rejected_owners(
                    prepared
                        .into_iter()
                        .map(|prepared_candidate| {
                            (
                                prepared_candidate.resources,
                                prepared_candidate.candidate.cleanup_duration,
                                prepared_candidate.attempt_id,
                                reason.clone(),
                            )
                        })
                        .collect(),
                )
                .await;
            return ManagedServiceBatchPublicationOutcome::Rejected { reason, cleanup };
        }

        let changed = !prepared.is_empty() || !displaced.is_empty();
        let mut table = self.admission_table(&displaced);
        for prepared_candidate in &prepared {
            table.insert(
                generation_key(&prepared_candidate.candidate),
                ManagedServiceGenerationState::Accepting,
                Arc::clone(&prepared_candidate.runtime),
            );
        }
        let publication_point = match self.registry.replace(table) {
            Ok(publication_point) => publication_point,
            Err(error) => {
                let cleanup = self
                    .cleanup_rejected_owners(
                        prepared
                            .into_iter()
                            .map(|prepared_candidate| {
                                (
                                    prepared_candidate.resources,
                                    prepared_candidate.candidate.cleanup_duration,
                                    prepared_candidate.attempt_id,
                                    error.to_string(),
                                )
                            })
                            .collect(),
                    )
                    .await;
                return ManagedServiceBatchPublicationOutcome::Rejected {
                    reason: error.to_string(),
                    cleanup,
                };
            }
        };

        for key in &displaced {
            self.generations
                .get_mut(key)
                .expect("active managed generation must be retained")
                .state = ManagedServiceGenerationState::Draining;
        }
        for prepared_candidate in prepared {
            let key = generation_key(&prepared_candidate.candidate);
            self.generations.insert(
                key,
                published_generation(
                    prepared_candidate.candidate,
                    prepared_candidate.runtime,
                    prepared_candidate.attempt_id,
                ),
            );
        }
        self.active = desired_active;
        if whole_composition {
            self.graph_managed = candidate_keys;
        }

        let cleanup = displaced
            .iter()
            .map(|old_key| {
                let old = self
                    .generations
                    .get(old_key)
                    .expect("replaced managed generation must remain retained");
                match old.runtime.start_generation_cleanup(
                    deadline_after(publication_point, old.active_call_duration),
                    old.cleanup_duration,
                ) {
                    Ok(()) => ManagedCleanupLaunch::Started,
                    Err(error) => ManagedCleanupLaunch::Failed(error.to_string()),
                }
            })
            .collect();
        if changed {
            ManagedServiceBatchPublicationOutcome::Published { cleanup }
        } else {
            ManagedServiceBatchPublicationOutcome::Unchanged
        }
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
        let implementation = published
            .implementations
            .get(capability_id)
            .expect("active capability must exist in its managed generation");
        if &implementation.metadata.interface_id != interface_id {
            anyhow::bail!(
                "managed service {} exposes interface {}, not {}",
                capability_id.as_str(),
                implementation.metadata.interface_id.as_str(),
                interface_id.as_str()
            );
        }
        let service = implementation
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
            implementation.metadata.owner.clone(),
            implementation.metadata.generation_id.clone(),
            Arc::clone(&self.registry),
            Arc::clone(&published.runtime),
            service,
        )))
    }

    pub(crate) fn published_metadata(&self) -> Vec<ManagedPublishedServiceMetadata> {
        self.active
            .iter()
            .filter_map(|(capability_id, key)| {
                self.generations
                    .get(key)?
                    .implementations
                    .get(capability_id)
                    .map(|implementation| implementation.metadata.clone())
            })
            .collect()
    }

    pub(crate) fn graph_managed_metadata(&self) -> Vec<ManagedPublishedServiceMetadata> {
        self.active
            .iter()
            .filter(|(_, key)| self.graph_managed.contains(*key))
            .filter_map(|(capability_id, key)| {
                self.generations
                    .get(key)?
                    .implementations
                    .get(capability_id)
                    .map(|implementation| implementation.metadata.clone())
            })
            .collect()
    }

    pub(crate) fn direct_managed_metadata(&self) -> Vec<ManagedPublishedServiceMetadata> {
        self.active
            .iter()
            .filter(|(_, key)| !self.graph_managed.contains(*key))
            .filter_map(|(capability_id, key)| {
                self.generations
                    .get(key)?
                    .implementations
                    .get(capability_id)
                    .map(|implementation| implementation.metadata.clone())
            })
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
                    let now = tokio::time::Instant::now();
                    let _ = service.runtime.start_generation_cleanup(
                        deadline_after(now, service.active_call_duration),
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
            if !joined_running_retry
                && runtime.retains_resources()
                && (result.is_err()
                    || result
                        .as_ref()
                        .is_ok_and(|cleanup| !cleanup.resources.all_resources_settled()))
            {
                result = runtime
                    .retry_generation_resource_cleanup(deadline_from_now(cleanup_duration))
                    .await;
            }
            let next_state = match &result {
                Ok(cleanup) if cleanup.resources.all_resources_settled() => {
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

        let rejected_owners = self.rejected_resource_owners.clone();
        for rejected in rejected_owners {
            let joined = rejected.owner.join_running_cleanup().await;
            let cleanup = if joined.all_resources_settled() {
                joined
            } else {
                rejected
                    .owner
                    .retry_cleanup_until(deadline_from_now(rejected.cleanup_duration))
                    .await
                    .unwrap_or_else(|_| rejected.owner.report())
            };
            report.rejected_candidates.push(cleanup);
        }
        if let Err(error) = self.reconcile_managed_diagnostics() {
            tracing::warn!(%error, "managed shutdown diagnostic reconciliation failed");
        }
        report
    }

    fn admission_table(&self, draining: &BTreeSet<ManagedGenerationKey>) -> ManagedAdmissionTable {
        let mut table = ManagedAdmissionTable::default();
        for (key, service) in &self.generations {
            let state = if draining.contains(key) {
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

    fn retained_controller_identities(&self) -> BTreeSet<usize> {
        self.generations
            .values()
            .filter(|generation| generation.runtime.retains_resources())
            .flat_map(|generation| &generation.resource_signature)
            .map(|resource| resource.controller_identity)
            .collect()
    }

    async fn cleanup_rejected_candidates(
        &mut self,
        candidates: &[ManagedGenerationCandidate],
        exact_keys: &BTreeSet<ManagedGenerationKey>,
        retained_controller_identities: &BTreeSet<usize>,
        reason: &str,
    ) -> Vec<ManagedResourceCleanupReport> {
        let mut claimed_controllers = retained_controller_identities.clone();
        let mut rejected = Vec::new();
        for candidate in candidates {
            let key = generation_key(candidate);
            if exact_keys.contains(&key) {
                continue;
            }
            let owner = ManagedResourceOwner::new(candidate.composition_generation_id.clone(), key);
            let mut registered = false;
            for resource in &candidate.resources {
                if !claimed_controllers.insert(resource.controller_identity) {
                    continue;
                }
                registered = true;
                owner
                    .register(
                        resource.id.clone(),
                        resource.kind,
                        resource.assurance,
                        resource.dependencies.clone(),
                        Arc::clone(&resource.controller),
                    )
                    .expect("validated candidate resource registration must succeed");
            }
            if registered {
                let _ = owner.validate_and_freeze();
                owner.freeze_for_rejected_candidate_cleanup();
                rejected.push((
                    owner,
                    candidate.cleanup_duration,
                    self.allocate_attempt_id(),
                    bounded_reason(reason),
                ));
            }
        }
        self.cleanup_rejected_owners(rejected).await
    }

    async fn cleanup_rejected_owner(
        &mut self,
        owner: Arc<ManagedResourceOwner>,
        cleanup_duration: Duration,
    ) -> ManagedResourceCleanupReport {
        let attempt_id = self.allocate_attempt_id();
        self.cleanup_rejected_owners(vec![(
            owner,
            cleanup_duration,
            attempt_id,
            "managed candidate rejected".into(),
        )])
        .await
        .into_iter()
        .next()
        .expect("one rejected owner produces one cleanup report")
    }

    async fn cleanup_rejected_owners(
        &mut self,
        owners: Vec<(Arc<ManagedResourceOwner>, Duration, u64, String)>,
    ) -> Vec<ManagedResourceCleanupReport> {
        self.rejected_resource_owners.extend(owners.iter().map(
            |(owner, cleanup_duration, attempt_id, reason)| RejectedResourceOwner {
                owner: Arc::clone(owner),
                cleanup_duration: *cleanup_duration,
                attempt_id: *attempt_id,
                reason: bounded_reason(reason),
            },
        ));
        for (owner, cleanup_duration, _, _) in &owners {
            let _ = owner.start_candidate_cleanup_until(deadline_from_now(*cleanup_duration));
        }
        let mut reports = Vec::with_capacity(owners.len());
        for (owner, _, attempt_id, reason) in owners {
            let cleanup = owner.join_running_cleanup().await;
            if cleanup.all_resources_settled() {
                self.archive_diagnostic(
                    rejected_projection(attempt_id, &reason, cleanup.clone(), false)
                        .expect("managed rejected diagnostic record must validate"),
                );
                self.rejected_resource_owners
                    .retain(|retained| retained.attempt_id != attempt_id);
            }
            reports.push(cleanup);
        }
        reports
    }
}

struct PreparedGeneration {
    candidate: ManagedGenerationCandidate,
    resources: Arc<ManagedResourceOwner>,
    runtime: Arc<ManagedGenerationRuntime>,
    attempt_id: u64,
}

fn validate_generation_candidate(
    active_call_duration: Duration,
    cleanup_duration: Duration,
    resources: &[ManagedResourceRegistration],
) -> anyhow::Result<()> {
    if active_call_duration.is_zero() {
        anyhow::bail!("managed service active-call duration must be nonzero");
    }
    if cleanup_duration.is_zero() {
        anyhow::bail!("managed service cleanup duration must be nonzero");
    }
    if resources.is_empty() {
        anyhow::bail!("managed service requires at least one resource");
    }
    let now = tokio::time::Instant::now();
    if now
        .checked_add(active_call_duration)
        .and_then(|deadline| deadline.checked_add(cleanup_duration))
        .is_none()
    {
        anyhow::bail!("managed service deadlines exceed the runtime clock range");
    }
    let mut resource_ids = BTreeSet::new();
    let mut controller_identities = BTreeSet::new();
    for resource in resources {
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
    Ok(())
}

fn deadline_from_now(duration: Duration) -> tokio::time::Instant {
    let now = tokio::time::Instant::now();
    deadline_after(now, duration)
}

fn deadline_after(start: tokio::time::Instant, duration: Duration) -> tokio::time::Instant {
    start.checked_add(duration).unwrap_or(start)
}

fn generation_key(candidate: &ManagedGenerationCandidate) -> ManagedGenerationKey {
    ManagedGenerationKey {
        owner: candidate.owner.clone(),
        generation_id: candidate.generation_id.clone(),
    }
}

fn exact_generation(
    existing: &ManagedPublishedGeneration,
    candidate: &ManagedGenerationCandidate,
) -> bool {
    existing.active_call_duration == candidate.active_call_duration
        && existing.cleanup_duration == candidate.cleanup_duration
        && existing.resource_signature == resource_signature(&candidate.resources)
        && existing.implementations.len() == candidate.implementations.len()
        && candidate
            .implementations
            .iter()
            .all(|(capability_id, candidate_implementation)| {
                existing
                    .implementations
                    .get(capability_id)
                    .is_some_and(|existing_implementation| {
                        existing_implementation.metadata.interface_id
                            == candidate_implementation.interface_id
                            && existing_implementation.implementation_identity
                                == candidate_implementation.implementation_identity
                            && existing_implementation.implementation_type_id
                                == candidate_implementation.implementation_type_id
                    })
            })
}

fn register_candidate_resources(
    owner: &ManagedResourceOwner,
    resources: &[ManagedResourceRegistration],
) -> anyhow::Result<()> {
    for resource in resources {
        owner.register(
            resource.id.clone(),
            resource.kind,
            resource.assurance,
            resource.dependencies.clone(),
            Arc::clone(&resource.controller),
        )?;
    }
    Ok(())
}

fn published_generation(
    candidate: ManagedGenerationCandidate,
    runtime: Arc<ManagedGenerationRuntime>,
    attempt_id: u64,
) -> ManagedPublishedGeneration {
    let owner = candidate.owner.clone();
    let generation_id = candidate.generation_id.clone();
    let implementations = candidate
        .implementations
        .into_iter()
        .map(|(capability_id, implementation)| {
            let metadata = ManagedPublishedServiceMetadata {
                capability_id: capability_id.clone(),
                interface_id: implementation.interface_id,
                owner: owner.clone(),
                generation_id: generation_id.clone(),
            };
            (
                capability_id,
                ManagedPublishedImplementation {
                    metadata,
                    implementation: implementation.implementation,
                    implementation_identity: implementation.implementation_identity,
                    implementation_type_id: implementation.implementation_type_id,
                },
            )
        })
        .collect();
    ManagedPublishedGeneration {
        implementations,
        resource_signature: resource_signature(&candidate.resources),
        active_call_duration: candidate.active_call_duration,
        cleanup_duration: candidate.cleanup_duration,
        runtime,
        state: ManagedServiceGenerationState::Accepting,
        attempt_id,
    }
}

fn published_projection(
    key: ManagedGenerationKey,
    generation: &ManagedPublishedGeneration,
    resources: ManagedResourceCleanupReport,
) -> anyhow::Result<ManagedOwnerDiagnosticProjection> {
    let unresolved_reason = generation
        .runtime
        .generation_cleanup_result()
        .and_then(Result::err)
        .map(|reason| bounded_reason(&reason))
        .or_else(|| first_resource_reason(&resources))
        .or_else(|| Some("managed resource cleanup remains unresolved".into()));
    let (state, boundary, cleanup_state) = match generation.state {
        ManagedServiceGenerationState::Accepting => (
            RuntimeContributionLifecycleState::Active,
            RuntimeLifecycleBoundary::Promoted,
            RuntimeCleanupState::Pending,
        ),
        ManagedServiceGenerationState::Draining
            if !generation.runtime.generation_cleanup_started() =>
        {
            (
                RuntimeContributionLifecycleState::Draining,
                RuntimeLifecycleBoundary::DrainStarted,
                RuntimeCleanupState::Pending,
            )
        }
        ManagedServiceGenerationState::Draining => (
            RuntimeContributionLifecycleState::Draining,
            RuntimeLifecycleBoundary::CleanupStarted,
            RuntimeCleanupState::Pending,
        ),
        ManagedServiceGenerationState::Degraded => (
            RuntimeContributionLifecycleState::Degraded,
            RuntimeLifecycleBoundary::CleanupStarted,
            aggregate_cleanup_state(&resources),
        ),
        ManagedServiceGenerationState::Retired => (
            RuntimeContributionLifecycleState::Retired,
            RuntimeLifecycleBoundary::Retired,
            RuntimeCleanupState::Settled,
        ),
    };
    owner_projection(
        generation.attempt_id,
        ManagedOwnerDisposition::Published,
        key,
        resources,
        state,
        boundary,
        cleanup_state,
        (state == RuntimeContributionLifecycleState::Degraded)
            .then_some("managed:cleanup_unresolved")
            .zip(unresolved_reason),
    )
}

fn rejected_projection(
    attempt_id: u64,
    reason: &str,
    resources: ManagedResourceCleanupReport,
    cleanup_running: bool,
) -> anyhow::Result<ManagedOwnerDiagnosticProjection> {
    let key = report_key(&resources)?;
    let cleanup_state = aggregate_cleanup_state(&resources);
    let (state, boundary, cleanup_state) = if cleanup_running {
        (
            RuntimeContributionLifecycleState::Draining,
            RuntimeLifecycleBoundary::CleanupStarted,
            RuntimeCleanupState::Pending,
        )
    } else {
        match cleanup_state {
            RuntimeCleanupState::Settled => (
                RuntimeContributionLifecycleState::Failed,
                RuntimeLifecycleBoundary::CleanupSettled,
                RuntimeCleanupState::Settled,
            ),
            RuntimeCleanupState::Degraded | RuntimeCleanupState::Unverified => (
                RuntimeContributionLifecycleState::Degraded,
                RuntimeLifecycleBoundary::CleanupStarted,
                cleanup_state,
            ),
            RuntimeCleanupState::NotRequired | RuntimeCleanupState::Pending => (
                RuntimeContributionLifecycleState::Draining,
                RuntimeLifecycleBoundary::CleanupStarted,
                RuntimeCleanupState::Pending,
            ),
        }
    };
    owner_projection(
        attempt_id,
        ManagedOwnerDisposition::RejectedCandidate,
        key,
        resources,
        state,
        boundary,
        cleanup_state,
        Some(("managed:candidate_rejected", bounded_reason(reason))),
    )
}

#[allow(clippy::too_many_arguments)]
fn owner_projection(
    attempt_id: u64,
    disposition: ManagedOwnerDisposition,
    key: ManagedGenerationKey,
    resources: ManagedResourceCleanupReport,
    state: RuntimeContributionLifecycleState,
    boundary: RuntimeLifecycleBoundary,
    cleanup_state: RuntimeCleanupState,
    reason: Option<(&str, String)>,
) -> anyhow::Result<ManagedOwnerDiagnosticProjection> {
    let composition_generation_id = resources
        .records
        .first()
        .ok_or_else(|| anyhow::anyhow!("managed diagnostic owner has no resources"))?
        .composition_generation_id
        .clone();
    let cleanup_assurance = aggregate_cleanup_assurance(&resources);
    let (reason_code, reason) = reason.map_or((None, None), |(code, reason)| {
        (
            Some(RuntimeDiagnosticCode::new(code).expect("managed diagnostic code is valid")),
            Some(bounded_reason(&reason)),
        )
    });
    let lifecycle = RuntimeContributionLifecycleRecord {
        schema_version: RUNTIME_CONTRIBUTION_SCHEMA_VERSION,
        composition_generation_id,
        contribution_id: key.owner,
        generation_id: key.generation_id,
        state,
        last_completed_boundary: boundary,
        reason_code,
        reason,
        restart_attempts: 0,
        next_restart_not_before_ms: None,
        last_heartbeat_ms: None,
        cleanup_assurance,
        cleanup_state,
    };
    lifecycle.validate().map_err(anyhow::Error::msg)?;
    let evidence = resources
        .evidence
        .into_iter()
        .map(|evidence| (evidence.resource_id.clone(), evidence))
        .collect::<BTreeMap<_, _>>();
    let resources = resources
        .records
        .into_iter()
        .map(|record| {
            record.validate().map_err(anyhow::Error::msg)?;
            let evidence = evidence
                .get(&record.id)
                .ok_or_else(|| anyhow::anyhow!("managed resource evidence is missing"))?;
            Ok(ManagedResourceDiagnosticProjection {
                record,
                stop_attempted: evidence.stop_attempted,
                force_attempted: evidence.force_attempted,
                reason: evidence.reason.as_deref().map(bounded_reason),
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(ManagedOwnerDiagnosticProjection {
        attempt_id,
        disposition,
        lifecycle,
        resources,
    })
}

fn report_key(report: &ManagedResourceCleanupReport) -> anyhow::Result<ManagedGenerationKey> {
    let record = report
        .records
        .first()
        .ok_or_else(|| anyhow::anyhow!("managed diagnostic owner has no resources"))?;
    Ok(ManagedGenerationKey {
        owner: record.contribution_id.clone(),
        generation_id: record.generation_id.clone(),
    })
}

fn aggregate_cleanup_assurance(report: &ManagedResourceCleanupReport) -> RuntimeCleanupAssurance {
    if report
        .records
        .iter()
        .all(|record| record.cleanup_assurance == RuntimeCleanupAssurance::Strict)
    {
        RuntimeCleanupAssurance::Strict
    } else if report
        .records
        .iter()
        .any(|record| record.cleanup_assurance == RuntimeCleanupAssurance::Unverified)
    {
        RuntimeCleanupAssurance::Unverified
    } else {
        RuntimeCleanupAssurance::BestEffort
    }
}

fn aggregate_cleanup_state(report: &ManagedResourceCleanupReport) -> RuntimeCleanupState {
    if report
        .records
        .iter()
        .all(|record| record.cleanup_state == RuntimeCleanupState::Settled)
    {
        RuntimeCleanupState::Settled
    } else if report
        .records
        .iter()
        .any(|record| record.cleanup_state == RuntimeCleanupState::Degraded)
    {
        RuntimeCleanupState::Degraded
    } else if report
        .records
        .iter()
        .any(|record| record.cleanup_state == RuntimeCleanupState::Unverified)
    {
        RuntimeCleanupState::Unverified
    } else {
        RuntimeCleanupState::Pending
    }
}

fn first_resource_reason(report: &ManagedResourceCleanupReport) -> Option<String> {
    report
        .evidence
        .iter()
        .find_map(|evidence| evidence.reason.as_deref().map(bounded_reason))
}

fn bounded_reason(reason: &str) -> String {
    if reason.len() <= 512 {
        return reason.to_string();
    }
    let mut end = 512;
    while !reason.is_char_boundary(end) {
        end -= 1;
    }
    reason[..end].to_string()
}

fn collapse_cleanup_launches(cleanup: Vec<ManagedCleanupLaunch>) -> ManagedCleanupLaunch {
    if let Some(reason) = cleanup.iter().find_map(|launch| match launch {
        ManagedCleanupLaunch::Failed(reason) => Some(reason.clone()),
        ManagedCleanupLaunch::NotRequired | ManagedCleanupLaunch::Started => None,
    }) {
        ManagedCleanupLaunch::Failed(reason)
    } else if cleanup.contains(&ManagedCleanupLaunch::Started) {
        ManagedCleanupLaunch::Started
    } else {
        ManagedCleanupLaunch::NotRequired
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

    struct CountService {
        value: usize,
    }

    impl ManagedServiceContract for CountService {
        type Request = ();
        type Response = usize;
        type Error = String;

        fn execute<'a>(
            &'a self,
            (): Self::Request,
            _context: ManagedCallContext,
        ) -> ManagedServiceFuture<'a, Self::Response, Self::Error> {
            Box::pin(async move { Ok(self.value) })
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

    fn count_interface() -> RuntimeServiceInterfaceId {
        RuntimeServiceInterfaceId::new("interface:test-managed-count-v1").unwrap()
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

    fn generation_candidate(
        generation: &str,
        capability: &str,
        service: Arc<TestService>,
        resource: Arc<TestResource>,
    ) -> ManagedGenerationCandidate {
        let controller: Arc<dyn ManagedResourceController> = resource;
        let mut candidate = ManagedGenerationCandidate::new(
            RuntimeCompositionGenerationId::new(format!("composition:{generation}")).unwrap(),
            RuntimeContributionId::new(format!("feature:{generation}")).unwrap(),
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
        )
        .unwrap();
        candidate
            .add_service(id(capability), interface(), service)
            .unwrap();
        candidate
    }

    #[tokio::test]
    async fn managed_diagnostic_active_owner_is_canonical_and_exact_is_unchanged() {
        let mut bus = ManagedServiceBus::default();
        let service = Arc::new(TestService { value: "active" });
        let resource = TestResource::new(true);
        bus.publish(candidate(
            "managed:diagnostic-active-v1",
            Arc::clone(&service),
            Arc::clone(&resource),
        ))
        .await;

        let first = bus.managed_diagnostic_records().unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].attempt_id, 1);
        assert_eq!(first[0].disposition, ManagedOwnerDisposition::Published);
        assert_eq!(
            first[0].lifecycle.state,
            RuntimeContributionLifecycleState::Active
        );
        assert_eq!(
            first[0].lifecycle.last_completed_boundary,
            RuntimeLifecycleBoundary::Promoted
        );
        assert_eq!(
            first[0].lifecycle.cleanup_state,
            RuntimeCleanupState::Pending
        );
        first[0].lifecycle.validate().unwrap();

        assert!(matches!(
            bus.publish(candidate(
                "managed:diagnostic-active-v1",
                service,
                Arc::clone(&resource),
            ))
            .await,
            ManagedServicePublicationOutcome::Unchanged { .. }
        ));
        assert_eq!(bus.managed_diagnostic_records().unwrap(), first);
        assert_eq!(resource.stops.load(Ordering::Acquire), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn managed_diagnostic_blocked_cleanup_is_draining_then_strict_degraded() {
        let mut bus = ManagedServiceBus::default();
        let old_resource = TestResource::new(false);
        bus.publish(candidate(
            "managed:diagnostic-blocked-v1",
            Arc::new(TestService { value: "old" }),
            Arc::clone(&old_resource),
        ))
        .await;
        bus.publish(candidate(
            "managed:diagnostic-blocked-v2",
            Arc::new(TestService { value: "new" }),
            TestResource::new(true),
        ))
        .await;
        while old_resource.stops.load(Ordering::Acquire) == 0 {
            tokio::task::yield_now().await;
        }

        let running = bus.managed_diagnostic_records().unwrap();
        let old = running
            .iter()
            .find(|record| record.lifecycle.generation_id.as_str().ends_with("v1"))
            .unwrap();
        assert_eq!(
            old.lifecycle.state,
            RuntimeContributionLifecycleState::Draining
        );
        assert_eq!(
            old.lifecycle.last_completed_boundary,
            RuntimeLifecycleBoundary::CleanupStarted
        );
        assert_eq!(old.lifecycle.cleanup_state, RuntimeCleanupState::Pending);

        tokio::time::advance(Duration::from_millis(30)).await;
        tokio::task::yield_now().await;
        let degraded = bus.managed_diagnostic_records().unwrap();
        let old = degraded
            .iter()
            .find(|record| record.lifecycle.generation_id.as_str().ends_with("v1"))
            .unwrap();
        assert_eq!(
            old.lifecycle.state,
            RuntimeContributionLifecycleState::Degraded
        );
        assert_eq!(old.lifecycle.cleanup_state, RuntimeCleanupState::Degraded);
        assert!(old.resources[0].stop_attempted);
        assert!(old.resources[0].force_attempted);
        assert!(old.resources[0].reason.is_some());
    }

    #[tokio::test(start_paused = true)]
    async fn managed_diagnostic_retry_archives_retired_and_stale_handle_stays_retired() {
        let mut bus = ManagedServiceBus::default();
        let old_resource = TestResource::new(false);
        bus.publish(candidate(
            "managed:diagnostic-retry-v1",
            Arc::new(TestService { value: "old" }),
            Arc::clone(&old_resource),
        ))
        .await;
        let stale = bus
            .service::<TestService>(&id("service:test-managed"), &interface())
            .unwrap()
            .unwrap();
        bus.publish(candidate(
            "managed:diagnostic-retry-v2",
            Arc::new(TestService { value: "new" }),
            TestResource::new(true),
        ))
        .await;
        let old_key = bus
            .generations
            .keys()
            .find(|key| key.generation_id.as_str().ends_with("v1"))
            .unwrap()
            .clone();
        let runtime = Arc::clone(&bus.generations.get(&old_key).unwrap().runtime);
        while old_resource.stops.load(Ordering::Acquire) == 0 {
            tokio::task::yield_now().await;
        }
        tokio::time::advance(Duration::from_millis(30)).await;
        while !runtime.generation_cleanup_complete() {
            tokio::task::yield_now().await;
        }
        bus.reconcile_managed_diagnostics().unwrap();

        old_resource.settle();
        runtime
            .retry_generation_resource_cleanup(tokio::time::Instant::now() + Duration::from_secs(1))
            .await
            .unwrap();
        let records = bus.managed_diagnostic_records().unwrap();
        let retired = records
            .iter()
            .find(|record| record.lifecycle.generation_id.as_str().ends_with("v1"))
            .unwrap();
        assert_eq!(
            retired.lifecycle.state,
            RuntimeContributionLifecycleState::Retired
        );
        assert_eq!(
            retired.lifecycle.cleanup_state,
            RuntimeCleanupState::Settled
        );
        assert!(!bus.generations.contains_key(&old_key));
        assert!(matches!(
            stale.invoke(Request::Read).await,
            Err(ManagedServiceCallError::GenerationRetired)
        ));
    }

    #[tokio::test]
    async fn managed_diagnostic_rejected_history_preserves_active_and_collision_attempts() {
        let mut bus = ManagedServiceBus::default();
        bus.publish(candidate(
            "managed:diagnostic-collision-v1",
            Arc::new(TestService { value: "active" }),
            TestResource::new(true),
        ))
        .await;
        let reason = "r".repeat(700);
        let rejected_resource = TestResource::new(true);
        let owner = ManagedResourceOwner::new(
            RuntimeCompositionGenerationId::new("composition:diagnostic-rejected").unwrap(),
            ManagedGenerationKey {
                owner: RuntimeContributionId::new("feature:test-managed").unwrap(),
                generation_id: RuntimeContributionGenerationId::new(
                    "managed:diagnostic-collision-v1",
                )
                .unwrap(),
            },
        );
        owner
            .register(
                RuntimeContributionResourceId::new("resource:diagnostic-rejected").unwrap(),
                RuntimeOwnedResourceKind::Task,
                RuntimeCleanupAssurance::Strict,
                Vec::new(),
                rejected_resource,
            )
            .unwrap();
        owner.validate_and_freeze().unwrap();
        let attempt_id = bus.allocate_attempt_id();
        bus.cleanup_rejected_owners(vec![(owner, Duration::from_millis(10), attempt_id, reason)])
            .await;

        let records = bus.managed_diagnostic_records().unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].attempt_id, 1);
        assert_eq!(records[1].attempt_id, 2);
        assert_eq!(records[0].disposition, ManagedOwnerDisposition::Published);
        assert_eq!(
            records[1].disposition,
            ManagedOwnerDisposition::RejectedCandidate
        );
        assert_eq!(
            records[1].lifecycle.state,
            RuntimeContributionLifecycleState::Failed
        );
        assert_eq!(records[1].lifecycle.reason.as_ref().unwrap().len(), 512);
        records[1].lifecycle.validate().unwrap();
        let serialized = serde_json::to_string(&records).unwrap();
        assert!(!serialized.contains("controller_identity"));
        assert!(!serialized.contains("implementation_identity"));
        assert!(!serialized.contains("0x"));
    }

    #[tokio::test]
    async fn generation_publishes_two_typed_services_with_one_runtime() {
        let mut bus = ManagedServiceBus::default();
        let resource = TestResource::new(true);
        let controller: Arc<dyn ManagedResourceController> = resource;
        let mut candidate = ManagedGenerationCandidate::new(
            RuntimeCompositionGenerationId::new("composition:managed:multi-v1").unwrap(),
            RuntimeContributionId::new("feature:test-managed-multi").unwrap(),
            RuntimeContributionGenerationId::new("managed:multi-v1").unwrap(),
            Duration::from_millis(10),
            Duration::from_millis(10),
            vec![ManagedResourceRegistration::new(
                RuntimeContributionResourceId::new("resource:managed:multi-v1").unwrap(),
                RuntimeOwnedResourceKind::Task,
                RuntimeCleanupAssurance::Strict,
                Vec::new(),
                controller,
            )],
        )
        .unwrap();
        candidate
            .add_service(
                id("service:test-managed"),
                interface(),
                Arc::new(TestService { value: "multi" }),
            )
            .unwrap();
        candidate
            .add_service(
                id("service:test-managed-count"),
                count_interface(),
                Arc::new(CountService { value: 7 }),
            )
            .unwrap();

        assert!(matches!(
            bus.publish_composition(vec![candidate]).await,
            ManagedServiceBatchPublicationOutcome::Published { .. }
        ));
        let text = bus
            .service::<TestService>(&id("service:test-managed"), &interface())
            .unwrap()
            .unwrap();
        let count = bus
            .service::<CountService>(&id("service:test-managed-count"), &count_interface())
            .unwrap()
            .unwrap();

        assert_eq!(text.invoke(Request::Read).await.unwrap(), "multi");
        assert_eq!(count.invoke(()).await.unwrap(), 7);
        assert_eq!(bus.generations.len(), 1);
        assert_eq!(bus.active.values().collect::<BTreeSet<_>>().len(), 1);
        assert_eq!(bus.published_metadata().len(), 2);
    }

    #[tokio::test]
    async fn batch_rejection_preserves_prior_active_services_and_cleans_candidates() {
        let mut bus = ManagedServiceBus::default();
        let original_resource = TestResource::new(true);
        bus.publish(candidate(
            "managed:v1",
            Arc::new(TestService { value: "old" }),
            Arc::clone(&original_resource),
        ))
        .await;
        let first_resource = TestResource::new(true);
        let second_resource = TestResource::new(true);
        let first = generation_candidate(
            "managed:batch-v2",
            "service:test-managed",
            Arc::new(TestService { value: "first" }),
            Arc::clone(&first_resource),
        );
        let second = generation_candidate(
            "managed:batch-v3",
            "service:test-managed",
            Arc::new(TestService { value: "second" }),
            Arc::clone(&second_resource),
        );

        let outcome = bus.publish_composition(vec![first, second]).await;
        let ManagedServiceBatchPublicationOutcome::Rejected { cleanup, .. } = outcome else {
            panic!("duplicate batch capability must reject");
        };
        let active = bus
            .service::<TestService>(&id("service:test-managed"), &interface())
            .unwrap()
            .unwrap();
        assert_eq!(cleanup.len(), 2);
        assert_eq!(active.invoke(Request::Read).await.unwrap(), "old");
        assert_eq!(original_resource.stops.load(Ordering::Acquire), 0);
        assert_eq!(first_resource.stops.load(Ordering::Acquire), 1);
        assert_eq!(second_resource.stops.load(Ordering::Acquire), 1);
    }

    #[tokio::test]
    async fn duplicate_exact_generation_does_not_skip_changed_candidate_cleanup() {
        let mut bus = ManagedServiceBus::default();
        let service = Arc::new(TestService { value: "old" });
        let original_resource = TestResource::new(true);
        bus.publish(candidate(
            "managed:v1",
            Arc::clone(&service),
            Arc::clone(&original_resource),
        ))
        .await;
        let exact = candidate(
            "managed:v1",
            Arc::clone(&service),
            Arc::clone(&original_resource),
        )
        .into();
        let changed_resource = TestResource::new(true);
        let changed = candidate(
            "managed:v1",
            Arc::new(TestService { value: "changed" }),
            Arc::clone(&changed_resource),
        )
        .into();

        assert!(matches!(
            bus.publish_composition(vec![exact, changed]).await,
            ManagedServiceBatchPublicationOutcome::Rejected { .. }
        ));
        assert_eq!(original_resource.stops.load(Ordering::Acquire), 0);
        assert_eq!(changed_resource.stops.load(Ordering::Acquire), 1);
    }

    #[tokio::test]
    async fn managed_diagnostic_unresolved_rejected_visible_across_cancellation() {
        let bus = Arc::new(tokio::sync::Mutex::new(ManagedServiceBus::default()));
        let first_resource = TestResource::new(false);
        let second_resource = TestResource::new(false);
        let mut first = generation_candidate(
            "managed:cancel-v1",
            "service:test-managed",
            Arc::new(TestService { value: "first" }),
            Arc::clone(&first_resource),
        );
        first.cleanup_duration = Duration::from_secs(10);
        let mut second = generation_candidate(
            "managed:cancel-v2",
            "service:test-managed",
            Arc::new(TestService { value: "second" }),
            Arc::clone(&second_resource),
        );
        second.cleanup_duration = Duration::from_secs(10);
        let publication = tokio::spawn({
            let bus = Arc::clone(&bus);
            async move {
                bus.lock()
                    .await
                    .publish_composition(vec![first, second])
                    .await
            }
        });
        while first_resource.stops.load(Ordering::Acquire) == 0
            || second_resource.stops.load(Ordering::Acquire) == 0
        {
            tokio::task::yield_now().await;
        }
        publication.abort();
        assert!(publication.await.unwrap_err().is_cancelled());
        {
            let mut bus = bus.lock().await;
            assert_eq!(bus.rejected_resource_owners.len(), 2);
            let records = bus.managed_diagnostic_records().unwrap();
            assert_eq!(records.len(), 2);
            assert!(records.iter().all(|record| {
                record.disposition == ManagedOwnerDisposition::RejectedCandidate
                    && record.lifecycle.state == RuntimeContributionLifecycleState::Draining
                    && record.lifecycle.cleanup_state == RuntimeCleanupState::Pending
            }));
        }

        let shutdown = tokio::spawn({
            let bus = Arc::clone(&bus);
            async move { bus.lock().await.shutdown().await }
        });
        while let Ok(guard) = bus.try_lock() {
            drop(guard);
            tokio::task::yield_now().await;
        }
        shutdown.abort();
        assert!(shutdown.await.unwrap_err().is_cancelled());
        {
            let mut bus = bus.lock().await;
            assert_eq!(bus.rejected_resource_owners.len(), 2);
            assert_eq!(bus.managed_diagnostic_records().unwrap().len(), 2);
        }

        first_resource.settle();
        second_resource.settle();
        let report = bus.lock().await.shutdown().await;
        assert_eq!(report.rejected_candidates.len(), 2);
        assert!(
            report
                .rejected_candidates
                .iter()
                .all(ManagedResourceCleanupReport::all_resources_settled)
        );
    }

    #[tokio::test]
    async fn direct_publication_cannot_replace_graph_managed_generation() {
        let mut bus = ManagedServiceBus::default();
        let graph_resource = TestResource::new(true);
        bus.publish_composition(vec![generation_candidate(
            "managed:graph-v1",
            "service:test-managed",
            Arc::new(TestService { value: "graph" }),
            Arc::clone(&graph_resource),
        )])
        .await;
        let direct_resource = TestResource::new(true);

        let outcome = bus
            .publish(candidate(
                "managed:direct-v2",
                Arc::new(TestService { value: "direct" }),
                Arc::clone(&direct_resource),
            ))
            .await;

        assert!(matches!(
            outcome,
            ManagedServicePublicationOutcome::Rejected { .. }
        ));
        assert_eq!(graph_resource.stops.load(Ordering::Acquire), 0);
        assert_eq!(direct_resource.stops.load(Ordering::Acquire), 1);
        let active = bus
            .service::<TestService>(&id("service:test-managed"), &interface())
            .unwrap()
            .unwrap();
        assert_eq!(active.invoke(Request::Read).await.unwrap(), "graph");
    }

    #[tokio::test]
    async fn non_conflicting_direct_generation_coexists_with_graph_managed_generation() {
        let mut bus = ManagedServiceBus::default();
        let direct_service = Arc::new(TestService { value: "direct" });
        let direct_resource = TestResource::new(true);
        bus.publish(candidate(
            "managed:direct-v1",
            Arc::clone(&direct_service),
            Arc::clone(&direct_resource),
        ))
        .await;
        let graph_resource = TestResource::new(true);
        bus.publish_composition(vec![generation_candidate(
            "managed:graph-v1",
            "service:test-managed-other",
            Arc::new(TestService { value: "graph" }),
            graph_resource,
        )])
        .await;

        let outcome = bus
            .publish(candidate(
                "managed:direct-v1",
                direct_service,
                Arc::clone(&direct_resource),
            ))
            .await;

        assert_eq!(
            outcome,
            ManagedServicePublicationOutcome::Unchanged {
                cleanup: ManagedCleanupLaunch::NotRequired
            }
        );
        assert_eq!(direct_resource.stops.load(Ordering::Acquire), 0);
        assert_eq!(bus.published_metadata().len(), 2);
    }

    #[test]
    fn candidate_rejects_deadlines_outside_runtime_clock_range() {
        let resource = TestResource::new(true);
        let controller: Arc<dyn ManagedResourceController> = resource;
        let result = ManagedGenerationCandidate::new(
            RuntimeCompositionGenerationId::new("composition:oversized").unwrap(),
            RuntimeContributionId::new("feature:oversized").unwrap(),
            RuntimeContributionGenerationId::new("managed:oversized-v1").unwrap(),
            Duration::MAX,
            Duration::from_millis(1),
            vec![ManagedResourceRegistration::new(
                RuntimeContributionResourceId::new("resource:oversized").unwrap(),
                RuntimeOwnedResourceKind::Task,
                RuntimeCleanupAssurance::Strict,
                Vec::new(),
                controller,
            )],
        );
        assert!(result.is_err());
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
    async fn shutdown_retries_retained_best_effort_generation_until_settled() {
        let mut bus = ManagedServiceBus::default();
        let resource = TestResource::new(false);
        let mut generation = candidate(
            "managed:best-effort-v1",
            Arc::new(TestService { value: "active" }),
            Arc::clone(&resource),
        );
        generation.resources[0].assurance = RuntimeCleanupAssurance::BestEffort;
        assert!(matches!(
            bus.publish(generation).await,
            ManagedServicePublicationOutcome::Published { .. }
        ));

        let first = bus.shutdown().await;
        assert_eq!(
            first.generations[0]
                .result
                .as_ref()
                .unwrap()
                .resources
                .records[0]
                .cleanup_state,
            RuntimeCleanupState::Unverified
        );
        assert_eq!(bus.generations.len(), 1);

        resource.settle();
        let second = bus.shutdown().await;
        assert!(
            second.generations[0]
                .result
                .as_ref()
                .is_ok_and(|cleanup| cleanup.resources.all_resources_settled())
        );
        assert!(bus.generations.is_empty());
        let records = bus.managed_diagnostic_records().unwrap();
        assert_eq!(
            records.last().unwrap().lifecycle.state,
            RuntimeContributionLifecycleState::Retired
        );
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

    #[tokio::test]
    async fn degraded_event_bus_shutdown_retains_runtime_ownership_evidence() {
        let mut bus = crate::bus::EventBus::new();
        let retention = Arc::new(AtomicBool::new(false));
        bus.bind_runtime_ownership_retention(Arc::clone(&retention));
        let resource = TestResource::new(false);
        bus.publish_managed_service(candidate(
            "managed:event-bus-degraded-v1",
            Arc::new(TestService { value: "owned" }),
            Arc::clone(&resource),
        ))
        .await;

        let report = bus.shutdown_managed_services().await;

        assert!(!report.all_resources_settled());
        assert!(retention.load(Ordering::Acquire));

        resource.settle();
        let report = bus.shutdown_managed_services().await;
        assert!(report.all_resources_settled());
        assert!(!retention.load(Ordering::Acquire));
    }

    #[test]
    fn rg12_empty_managed_sidecar_does_not_change_no_resource_storage() {
        let bus = ManagedServiceBus::default();
        assert!(bus.active.is_empty());
        assert!(bus.generations.is_empty());
        assert!(bus.published_metadata().is_empty());
    }
}
