//! Candidate contribution lifecycle, readiness, rollback, and promotion ownership.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use omegon_traits::{
    RUNTIME_CONTRIBUTION_SCHEMA_VERSION, RuntimeCleanupAssurance, RuntimeCleanupState,
    RuntimeCompositionGenerationId, RuntimeContributionDeclaration, RuntimeContributionId,
    RuntimeContributionLifecycleRecord, RuntimeContributionLifecycleState,
    RuntimeContributionResourceId, RuntimeDiagnosticCode, RuntimeLifecycleBoundary,
    RuntimeOwnedResourceKind, RuntimeOwnedResourceRecord,
};

/// Static, transport-neutral evidence captured before contribution code runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DiscoveredContributionCandidate {
    pub(crate) preflight: omegon_traits::RuntimeDynamicContributionPreflight,
}

impl DiscoveredContributionCandidate {
    pub(crate) fn new(
        preflight: omegon_traits::RuntimeDynamicContributionPreflight,
    ) -> Result<Self> {
        preflight.validate().map_err(|error| anyhow!(error))?;
        Ok(Self { preflight })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiscoveredContributionState {
    Discovered,
    Absent,
    Admitted,
    Ready,
    Rejected,
    Quarantined,
    Staged,
    Published,
    Retired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DiscoveredContributionEvidence {
    pub(crate) candidate: DiscoveredContributionCandidate,
    pub(crate) state: DiscoveredContributionState,
    pub(crate) reason: Option<String>,
}

/// Shared metadata-only inventory. Adapters register static candidates here and
/// may execute only after `admit` returns a digest-bound permit.
#[derive(Clone, Default)]
pub(crate) struct DynamicContributionInventory {
    entries: std::sync::Arc<
        std::sync::Mutex<BTreeMap<RuntimeContributionId, DiscoveredContributionEvidence>>,
    >,
}

impl DynamicContributionInventory {
    pub(crate) fn discover(
        &self,
        preflight: omegon_traits::RuntimeDynamicContributionPreflight,
    ) -> Result<DiscoveredContributionCandidate> {
        let candidate = DiscoveredContributionCandidate::new(preflight)?;
        let mut entries = self
            .entries
            .lock()
            .expect("dynamic inventory lock poisoned");
        if let Some(existing) = entries.get(&candidate.preflight.id)
            && existing.candidate.preflight.source_digest != candidate.preflight.source_digest
        {
            return Err(anyhow!(
                "dynamic contribution '{}' has conflicting discovered source digests",
                candidate.preflight.id.as_str()
            ));
        }
        entries.insert(
            candidate.preflight.id.clone(),
            DiscoveredContributionEvidence {
                candidate: candidate.clone(),
                state: DiscoveredContributionState::Discovered,
                reason: None,
            },
        );
        Ok(candidate)
    }

    pub(crate) fn admit(
        &self,
        candidate: &DiscoveredContributionCandidate,
        policy: &crate::dynamic_admission::DynamicAdmissionPolicy,
    ) -> Result<crate::dynamic_admission::DynamicAdmissionPermit> {
        let result = policy.admit(candidate.preflight.clone());
        match &result {
            Ok(_) => self.transition(
                &candidate.preflight.id,
                DiscoveredContributionState::Admitted,
                None,
            ),
            Err(error) => self.transition(
                &candidate.preflight.id,
                DiscoveredContributionState::Rejected,
                Some(error.to_string()),
            ),
        }
        result
    }

    pub(crate) fn ready(&self, id: &RuntimeContributionId) {
        self.transition(id, DiscoveredContributionState::Ready, None);
    }

    pub(crate) fn absent(&self, id: &RuntimeContributionId) {
        self.transition(id, DiscoveredContributionState::Absent, None);
    }

    pub(crate) fn quarantine(&self, id: &RuntimeContributionId, reason: impl AsRef<str>) {
        self.transition(
            id,
            DiscoveredContributionState::Quarantined,
            Some(reason.as_ref().to_string()),
        );
    }

    pub(crate) fn stage_ready(&self) {
        let mut entries = self
            .entries
            .lock()
            .expect("dynamic inventory lock poisoned");
        for evidence in entries.values_mut() {
            if evidence.state == DiscoveredContributionState::Ready {
                evidence.state = DiscoveredContributionState::Staged;
            }
        }
    }

    pub(crate) fn publish_staged(&self) {
        let mut entries = self
            .entries
            .lock()
            .expect("dynamic inventory lock poisoned");
        for evidence in entries.values_mut() {
            if evidence.state == DiscoveredContributionState::Staged {
                evidence.state = DiscoveredContributionState::Published;
            }
        }
    }

    pub(crate) fn reject_staged(&self, reason: impl AsRef<str>) {
        let reason = bounded_reason(reason.as_ref());
        let mut entries = self
            .entries
            .lock()
            .expect("dynamic inventory lock poisoned");
        for evidence in entries.values_mut() {
            if evidence.state == DiscoveredContributionState::Staged {
                evidence.state = DiscoveredContributionState::Quarantined;
                evidence.reason = Some(reason.clone());
            }
        }
    }

    pub(crate) fn evidence(&self) -> Vec<DiscoveredContributionEvidence> {
        self.entries
            .lock()
            .expect("dynamic inventory lock poisoned")
            .values()
            .cloned()
            .collect()
    }

    pub(crate) fn ensure_callable(
        &self,
        id: &RuntimeContributionId,
        source_digest: &str,
    ) -> Result<()> {
        let entries = self
            .entries
            .lock()
            .expect("dynamic inventory lock poisoned");
        let evidence = entries
            .get(id)
            .ok_or_else(|| anyhow!("dynamic contribution generation is absent"))?;
        if evidence.candidate.preflight.source_digest != source_digest
            || evidence.state != DiscoveredContributionState::Published
        {
            return Err(anyhow!(
                "dynamic contribution '{}' generation is stale or not published",
                id.as_str()
            ));
        }
        Ok(())
    }

    fn transition(
        &self,
        id: &RuntimeContributionId,
        state: DiscoveredContributionState,
        reason: Option<String>,
    ) {
        if let Some(evidence) = self
            .entries
            .lock()
            .expect("dynamic inventory lock poisoned")
            .get_mut(id)
        {
            evidence.state = state;
            evidence.reason = reason.map(|reason| bounded_reason(&reason));
        }
    }
}

/// One generation owner for native-extension and MCP transport resources.
/// Protocol adapters still perform their own bounded transport shutdown.
#[derive(Default)]
pub(crate) struct DynamicContributionGenerationOwner {
    inventory: DynamicContributionInventory,
    extensions: Vec<std::sync::Arc<crate::extensions::ExtensionSupervisor>>,
    mcp: Vec<crate::plugins::mcp::McpSupervisor>,
    published: bool,
    settled: bool,
}

impl DynamicContributionGenerationOwner {
    pub(crate) fn new(inventory: DynamicContributionInventory) -> Self {
        Self {
            inventory,
            ..Self::default()
        }
    }

    pub(crate) fn inventory(&self) -> DynamicContributionInventory {
        self.inventory.clone()
    }

    pub(crate) fn own_extension(
        &mut self,
        supervisor: std::sync::Arc<crate::extensions::ExtensionSupervisor>,
    ) {
        self.extensions.push(supervisor);
    }

    pub(crate) fn own_mcp(&mut self, supervisor: crate::plugins::mcp::McpSupervisor) {
        self.mcp.push(supervisor);
    }

    pub(crate) fn stage(&self) {
        self.inventory.stage_ready();
    }

    pub(crate) fn publish(&mut self) {
        self.inventory.publish_staged();
        self.published = true;
    }

    pub(crate) fn absorb_published(&mut self, mut candidate: Self) {
        debug_assert!(candidate.published && !candidate.settled);
        self.extensions.append(&mut candidate.extensions);
        self.mcp.append(&mut candidate.mcp);
        candidate.settled = true;
    }

    pub(crate) async fn reject(&mut self, reason: impl AsRef<str>) -> Vec<String> {
        self.inventory.reject_staged(reason);
        self.shutdown_resources().await
    }

    pub(crate) async fn shutdown(&mut self) -> Vec<String> {
        let failures = self.shutdown_resources().await;
        let mut entries = self
            .inventory
            .entries
            .lock()
            .expect("dynamic inventory lock poisoned");
        for evidence in entries.values_mut() {
            if evidence.state == DiscoveredContributionState::Published {
                evidence.state = if failures.is_empty() {
                    DiscoveredContributionState::Retired
                } else {
                    DiscoveredContributionState::Quarantined
                };
                evidence.reason =
                    (!failures.is_empty()).then(|| bounded_reason(&failures.join("; ")));
            }
        }
        failures
    }

    pub(crate) fn is_published(&self) -> bool {
        self.published
    }

    async fn shutdown_resources(&mut self) -> Vec<String> {
        if self.settled {
            return Vec::new();
        }
        self.settled = true;
        let mut failures =
            crate::extensions::shutdown_supervisors(&self.extensions, Duration::from_millis(500))
                .await;
        self.extensions.clear();
        for supervisor in self.mcp.drain(..) {
            failures.extend(supervisor.shutdown(Duration::from_millis(500)).await);
        }
        failures
    }
}

#[async_trait]
pub(crate) trait CandidateResource: Send {
    async fn settle(&mut self) -> RuntimeCleanupState;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RestartDecision {
    RetryAfter(Duration),
    Quarantined,
}

pub(crate) struct RestartController {
    restart_limit: u16,
    attempts: u16,
    base_backoff: Duration,
    max_backoff: Duration,
    quarantined: bool,
}

impl RestartController {
    pub(crate) fn new(restart_limit: u16, base_backoff: Duration, max_backoff: Duration) -> Self {
        Self {
            restart_limit,
            attempts: 0,
            base_backoff,
            max_backoff,
            quarantined: false,
        }
    }

    pub(crate) fn record_failure(&mut self) -> RestartDecision {
        if self.quarantined || self.attempts >= self.restart_limit {
            self.quarantined = true;
            return RestartDecision::Quarantined;
        }
        let exponent = u32::from(self.attempts.min(31));
        self.attempts += 1;
        let delay = self
            .base_backoff
            .saturating_mul(2_u32.saturating_pow(exponent))
            .min(self.max_backoff);
        RestartDecision::RetryAfter(delay)
    }

    pub(crate) fn attempts(&self) -> u16 {
        self.attempts
    }

    pub(crate) fn is_quarantined(&self) -> bool {
        self.quarantined
    }
}

struct OwnedCandidateResource {
    record: RuntimeOwnedResourceRecord,
    cleanup_timeout: Duration,
    resource: Box<dyn CandidateResource>,
}

pub(crate) struct CandidateGeneration {
    id: RuntimeCompositionGenerationId,
    readiness_deadline: tokio::time::Instant,
    lifecycle: BTreeMap<RuntimeContributionId, RuntimeContributionLifecycleRecord>,
    resources: Vec<OwnedCandidateResource>,
}

impl CandidateGeneration {
    pub(crate) fn new(
        id: RuntimeCompositionGenerationId,
        declarations: &[RuntimeContributionDeclaration],
        readiness_deadline: tokio::time::Instant,
    ) -> Self {
        let lifecycle = declarations
            .iter()
            .map(|declaration| {
                (
                    declaration.id.clone(),
                    RuntimeContributionLifecycleRecord {
                        schema_version: RUNTIME_CONTRIBUTION_SCHEMA_VERSION,
                        composition_generation_id: id.clone(),
                        contribution_id: declaration.id.clone(),
                        generation_id: declaration.generation_id.clone(),
                        state: RuntimeContributionLifecycleState::Discovered,
                        last_completed_boundary: RuntimeLifecycleBoundary::Discovered,
                        reason_code: None,
                        reason: None,
                        restart_attempts: 0,
                        next_restart_not_before_ms: None,
                        last_heartbeat_ms: None,
                        cleanup_assurance: match declaration.transition.cleanup {
                            omegon_traits::RuntimeCleanupRequirement::Strict => {
                                RuntimeCleanupAssurance::Strict
                            }
                            omegon_traits::RuntimeCleanupRequirement::BestEffort => {
                                RuntimeCleanupAssurance::BestEffort
                            }
                        },
                        cleanup_state: RuntimeCleanupState::NotRequired,
                    },
                )
            })
            .collect();
        Self {
            id,
            readiness_deadline,
            lifecycle,
            resources: Vec::new(),
        }
    }

    pub(crate) fn id(&self) -> &RuntimeCompositionGenerationId {
        &self.id
    }

    pub(crate) fn lifecycle_records(&self) -> Vec<RuntimeContributionLifecycleRecord> {
        self.lifecycle.values().cloned().collect()
    }

    pub(crate) fn transition(
        &mut self,
        contribution_id: &RuntimeContributionId,
        state: RuntimeContributionLifecycleState,
        boundary: RuntimeLifecycleBoundary,
    ) -> Result<()> {
        let record = self.lifecycle.get_mut(contribution_id).ok_or_else(|| {
            anyhow!(
                "unknown candidate contribution {}",
                contribution_id.as_str()
            )
        })?;
        record.state = state;
        record.last_completed_boundary = boundary;
        record.reason = None;
        record.reason_code = None;
        Ok(())
    }

    pub(crate) fn register_resource(
        &mut self,
        contribution_id: &RuntimeContributionId,
        id: RuntimeContributionResourceId,
        kind: RuntimeOwnedResourceKind,
        assurance: RuntimeCleanupAssurance,
        cleanup_timeout: Duration,
        resource: Box<dyn CandidateResource>,
    ) -> Result<()> {
        let owner = self
            .lifecycle
            .get(contribution_id)
            .ok_or_else(|| anyhow!("unknown resource owner {}", contribution_id.as_str()))?;
        let record = RuntimeOwnedResourceRecord {
            schema_version: RUNTIME_CONTRIBUTION_SCHEMA_VERSION,
            id,
            composition_generation_id: self.id.clone(),
            contribution_id: contribution_id.clone(),
            generation_id: owner.generation_id.clone(),
            kind,
            cleanup_assurance: assurance,
            cleanup_state: RuntimeCleanupState::Pending,
        };
        record.validate().map_err(|error| anyhow!(error))?;
        self.resources.push(OwnedCandidateResource {
            record,
            cleanup_timeout,
            resource,
        });
        Ok(())
    }

    fn ensure_promotable(&self, now: tokio::time::Instant) -> Result<()> {
        if now >= self.readiness_deadline {
            return Err(anyhow!("candidate readiness deadline expired"));
        }
        let not_ready = self
            .lifecycle
            .values()
            .filter(|record| {
                record.last_completed_boundary < RuntimeLifecycleBoundary::ReadinessSatisfied
            })
            .map(|record| record.contribution_id.as_str())
            .collect::<Vec<_>>();
        if !not_ready.is_empty() {
            return Err(anyhow!(
                "candidate contributions are not ready: {}",
                not_ready.join(", ")
            ));
        }

        let strict_owners = self
            .lifecycle
            .values()
            .filter(|record| record.cleanup_assurance == RuntimeCleanupAssurance::Strict)
            .map(|record| record.contribution_id.clone())
            .collect::<BTreeSet<_>>();
        let ineligible = self
            .resources
            .iter()
            .filter(|resource| {
                strict_owners.contains(&resource.record.contribution_id)
                    && resource.record.cleanup_assurance != RuntimeCleanupAssurance::Strict
            })
            .map(|resource| resource.record.id.as_str())
            .collect::<Vec<_>>();
        if !ineligible.is_empty() {
            return Err(anyhow!(
                "strict cleanup cannot be verified for candidate resources: {}",
                ineligible.join(", ")
            ));
        }
        Ok(())
    }

    async fn reject(mut self, code: &str, reason: impl AsRef<str>) -> RejectedGeneration {
        let code = RuntimeDiagnosticCode::new(code).expect("lifecycle reason code is static");
        let reason = bounded_reason(reason.as_ref());
        for record in self.lifecycle.values_mut() {
            record.state = RuntimeContributionLifecycleState::Failed;
            record.reason_code = Some(code.clone());
            record.reason = Some(reason.clone());
            record.cleanup_state = if self.resources.is_empty() {
                RuntimeCleanupState::NotRequired
            } else {
                RuntimeCleanupState::Pending
            };
        }

        for owned in self.resources.iter_mut().rev() {
            owned.record.cleanup_state =
                match tokio::time::timeout(owned.cleanup_timeout, owned.resource.settle()).await {
                    Ok(state) => state,
                    Err(_) => RuntimeCleanupState::Unverified,
                };
        }

        for record in self.lifecycle.values_mut() {
            let states = self
                .resources
                .iter()
                .filter(|resource| resource.record.contribution_id == record.contribution_id)
                .map(|resource| resource.record.cleanup_state)
                .collect::<Vec<_>>();
            record.cleanup_state = aggregate_cleanup(&states);
            if record.cleanup_state == RuntimeCleanupState::Settled {
                record.last_completed_boundary = RuntimeLifecycleBoundary::CleanupSettled;
            } else if !states.is_empty() {
                record.last_completed_boundary = RuntimeLifecycleBoundary::CleanupStarted;
            }
        }

        RejectedGeneration {
            id: self.id,
            lifecycle: self.lifecycle.into_values().collect(),
            resources: self
                .resources
                .into_iter()
                .map(|resource| resource.record)
                .collect(),
        }
    }
}

pub(crate) struct ActiveGeneration {
    id: RuntimeCompositionGenerationId,
    lifecycle: Vec<RuntimeContributionLifecycleRecord>,
    resources: Vec<OwnedCandidateResource>,
}

impl ActiveGeneration {
    async fn retire(mut self) -> RetiredGeneration {
        for record in &mut self.lifecycle {
            record.state = RuntimeContributionLifecycleState::Draining;
            record.last_completed_boundary = RuntimeLifecycleBoundary::DrainStarted;
            record.cleanup_state = if self.resources.is_empty() {
                RuntimeCleanupState::NotRequired
            } else {
                RuntimeCleanupState::Pending
            };
        }
        for owned in self.resources.iter_mut().rev() {
            owned.record.cleanup_state =
                match tokio::time::timeout(owned.cleanup_timeout, owned.resource.settle()).await {
                    Ok(state) => state,
                    Err(_) => RuntimeCleanupState::Unverified,
                };
        }
        for record in &mut self.lifecycle {
            let states = self
                .resources
                .iter()
                .filter(|resource| resource.record.contribution_id == record.contribution_id)
                .map(|resource| resource.record.cleanup_state)
                .collect::<Vec<_>>();
            record.cleanup_state = aggregate_cleanup(&states);
            record.state = RuntimeContributionLifecycleState::Retired;
            record.last_completed_boundary = if matches!(
                record.cleanup_state,
                RuntimeCleanupState::Settled | RuntimeCleanupState::NotRequired
            ) {
                RuntimeLifecycleBoundary::Retired
            } else {
                RuntimeLifecycleBoundary::CleanupStarted
            };
        }
        RetiredGeneration {
            id: self.id,
            lifecycle: self.lifecycle,
            resources: self
                .resources
                .into_iter()
                .map(|resource| resource.record)
                .collect(),
        }
    }
}

pub(crate) struct RetiredGeneration {
    pub(crate) id: RuntimeCompositionGenerationId,
    pub(crate) lifecycle: Vec<RuntimeContributionLifecycleRecord>,
    pub(crate) resources: Vec<RuntimeOwnedResourceRecord>,
}

pub(crate) struct RejectedGeneration {
    pub(crate) id: RuntimeCompositionGenerationId,
    pub(crate) lifecycle: Vec<RuntimeContributionLifecycleRecord>,
    pub(crate) resources: Vec<RuntimeOwnedResourceRecord>,
}

#[derive(Default)]
pub(crate) struct CompositionLifecycleOwner {
    active: Option<ActiveGeneration>,
    last_rejected: Option<RejectedGeneration>,
    last_retired: Option<RetiredGeneration>,
}

impl CompositionLifecycleOwner {
    pub(crate) fn active_generation_id(&self) -> Option<&RuntimeCompositionGenerationId> {
        self.active.as_ref().map(|active| &active.id)
    }

    pub(crate) fn active_lifecycle(&self) -> &[RuntimeContributionLifecycleRecord] {
        self.active
            .as_ref()
            .map(|active| active.lifecycle.as_slice())
            .unwrap_or_default()
    }

    pub(crate) fn last_rejected(&self) -> Option<&RejectedGeneration> {
        self.last_rejected.as_ref()
    }

    pub(crate) fn last_retired(&self) -> Option<&RetiredGeneration> {
        self.last_retired.as_ref()
    }

    pub(crate) async fn promote(
        &mut self,
        mut candidate: CandidateGeneration,
        commit: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        if let Err(error) = candidate.ensure_promotable(tokio::time::Instant::now()) {
            self.last_rejected = Some(
                candidate
                    .reject("lifecycle:not_ready", error.to_string())
                    .await,
            );
            return Err(error);
        }
        for record in candidate.lifecycle.values_mut() {
            record.state = RuntimeContributionLifecycleState::Ready;
            record.last_completed_boundary = RuntimeLifecycleBoundary::PublicationPrepared;
        }
        if let Err(error) = commit() {
            self.last_rejected = Some(
                candidate
                    .reject("lifecycle:publication_failed", error.to_string())
                    .await,
            );
            return Err(error);
        }
        for record in candidate.lifecycle.values_mut() {
            record.state = RuntimeContributionLifecycleState::Active;
            record.last_completed_boundary = RuntimeLifecycleBoundary::Promoted;
        }
        let previous = self.active.replace(ActiveGeneration {
            id: candidate.id,
            lifecycle: candidate.lifecycle.into_values().collect(),
            resources: candidate.resources,
        });
        if let Some(previous) = previous {
            self.last_retired = Some(previous.retire().await);
        }
        Ok(())
    }

    pub(crate) async fn reject(
        &mut self,
        candidate: CandidateGeneration,
        code: &str,
        reason: impl AsRef<str>,
    ) {
        self.last_rejected = Some(candidate.reject(code, reason).await);
    }
}

fn aggregate_cleanup(states: &[RuntimeCleanupState]) -> RuntimeCleanupState {
    if states.is_empty() {
        RuntimeCleanupState::NotRequired
    } else if states.contains(&RuntimeCleanupState::Unverified) {
        RuntimeCleanupState::Unverified
    } else if states.contains(&RuntimeCleanupState::Degraded) {
        RuntimeCleanupState::Degraded
    } else if states
        .iter()
        .all(|state| *state == RuntimeCleanupState::Settled)
    {
        RuntimeCleanupState::Settled
    } else {
        RuntimeCleanupState::Pending
    }
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

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use omegon_traits::{
        RuntimeActivationBoundary, RuntimeCompositionTransitionPolicy, RuntimeConfinementRequest,
        RuntimeContributionGenerationId, RuntimeDynamicSourceKind, RuntimeFailureDisposition,
        RuntimeLifecyclePolicy, RuntimeLifecycleRequirement, RuntimeOwnerTier,
        RuntimePlatformRequirements, RuntimeProbeOperation, RuntimeProbeRequirements,
        RuntimeProtocolRange, RuntimeTrustRequest,
    };

    fn dynamic_preflight(
        id: &str,
        source_kind: RuntimeDynamicSourceKind,
    ) -> omegon_traits::RuntimeDynamicContributionPreflight {
        omegon_traits::RuntimeDynamicContributionPreflight {
            schema_version: omegon_traits::RUNTIME_DYNAMIC_PREFLIGHT_SCHEMA_VERSION,
            id: RuntimeContributionId::new(id).unwrap(),
            source_digest: crate::dynamic_admission::digest_bytes(id.as_bytes()),
            source_kind,
            protocol: RuntimeProtocolRange::new(1, 1).unwrap(),
            minimum_dependencies: Vec::new(),
            requested_trust: RuntimeTrustRequest::OperatorManaged,
            requested_confinement: match source_kind {
                RuntimeDynamicSourceKind::OciExtension => RuntimeConfinementRequest::Oci,
                _ => RuntimeConfinementRequest::HostProcess,
            },
            probe: RuntimeProbeRequirements {
                operations: vec![RuntimeProbeOperation::DiscoverCapabilities],
                timeout_ms: 50,
                requested_effects: Vec::new(),
            },
        }
    }

    struct FakeResource {
        name: &'static str,
        order: Arc<Mutex<Vec<&'static str>>>,
        result: RuntimeCleanupState,
        delay: Duration,
    }

    #[async_trait]
    impl CandidateResource for FakeResource {
        async fn settle(&mut self) -> RuntimeCleanupState {
            tokio::time::sleep(self.delay).await;
            self.order.lock().unwrap().push(self.name);
            self.result
        }
    }

    fn declaration(
        id: &str,
        cleanup: omegon_traits::RuntimeCleanupRequirement,
    ) -> RuntimeContributionDeclaration {
        RuntimeContributionDeclaration {
            schema_version: RUNTIME_CONTRIBUTION_SCHEMA_VERSION,
            id: RuntimeContributionId::new(id).unwrap(),
            generation_id: RuntimeContributionGenerationId::new(format!("generation:{id}"))
                .unwrap(),
            owner_tier: RuntimeOwnerTier::Operator,
            requested_trust: RuntimeTrustRequest::OperatorManaged,
            requested_confinement: omegon_traits::RuntimeConfinementRequest::HostProcess,
            protocol: RuntimeProtocolRange::new(1, 1).unwrap(),
            platform: RuntimePlatformRequirements::default(),
            dependencies: Vec::new(),
            conflicts: Vec::new(),
            replaces: Vec::new(),
            lifecycle: RuntimeLifecyclePolicy {
                requirement: RuntimeLifecycleRequirement::Optional,
                failure_disposition: RuntimeFailureDisposition::Quarantine,
                readiness_timeout_ms: 100,
                heartbeat_timeout_ms: None,
                restart_limit: 2,
            },
            transition: RuntimeCompositionTransitionPolicy {
                activation_boundary: RuntimeActivationBoundary::Boot,
                cleanup,
                cleanup_timeout_ms: 50,
            },
            capabilities: Vec::new(),
            groups: Vec::new(),
        }
    }

    fn candidate(declarations: &[RuntimeContributionDeclaration]) -> CandidateGeneration {
        CandidateGeneration::new(
            RuntimeCompositionGenerationId::new("composition:test").unwrap(),
            declarations,
            tokio::time::Instant::now() + Duration::from_secs(1),
        )
    }

    #[tokio::test]
    async fn rejected_candidate_rolls_resources_back_in_reverse_order() {
        let declaration = declaration(
            "extension:test",
            omegon_traits::RuntimeCleanupRequirement::BestEffort,
        );
        let owner_id = declaration.id.clone();
        let mut candidate = candidate(&[declaration]);
        let order = Arc::new(Mutex::new(Vec::new()));
        for name in ["first", "second"] {
            candidate
                .register_resource(
                    &owner_id,
                    RuntimeContributionResourceId::new(format!("resource:{name}")).unwrap(),
                    RuntimeOwnedResourceKind::Task,
                    RuntimeCleanupAssurance::BestEffort,
                    Duration::from_millis(50),
                    Box::new(FakeResource {
                        name,
                        order: Arc::clone(&order),
                        result: RuntimeCleanupState::Settled,
                        delay: Duration::ZERO,
                    }),
                )
                .unwrap();
        }
        let mut owner = CompositionLifecycleOwner::default();
        owner.reject(candidate, "lifecycle:test", "rejected").await;
        assert_eq!(*order.lock().unwrap(), vec!["second", "first"]);
        assert!(
            owner
                .last_rejected()
                .unwrap()
                .resources
                .iter()
                .all(|resource| resource.cleanup_state == RuntimeCleanupState::Settled)
        );
    }

    #[tokio::test]
    async fn failed_publication_preserves_prior_generation_and_settles_candidate() {
        let first = declaration(
            "extension:first",
            omegon_traits::RuntimeCleanupRequirement::BestEffort,
        );
        let mut active = candidate(std::slice::from_ref(&first));
        active
            .transition(
                &first.id,
                RuntimeContributionLifecycleState::Ready,
                RuntimeLifecycleBoundary::ReadinessSatisfied,
            )
            .unwrap();
        let mut owner = CompositionLifecycleOwner::default();
        owner.promote(active, || Ok(())).await.unwrap();
        let active_id = owner.active_generation_id().unwrap().clone();

        let second = declaration(
            "extension:second",
            omegon_traits::RuntimeCleanupRequirement::BestEffort,
        );
        let mut rejected = CandidateGeneration::new(
            RuntimeCompositionGenerationId::new("composition:replacement").unwrap(),
            std::slice::from_ref(&second),
            tokio::time::Instant::now() + Duration::from_secs(1),
        );
        rejected
            .transition(
                &second.id,
                RuntimeContributionLifecycleState::Ready,
                RuntimeLifecycleBoundary::ReadinessSatisfied,
            )
            .unwrap();
        assert!(
            owner
                .promote(rejected, || Err(anyhow!("graph rejected")))
                .await
                .is_err()
        );
        assert_eq!(owner.active_generation_id(), Some(&active_id));
        assert_eq!(
            owner.last_rejected().unwrap().lifecycle[0].state,
            RuntimeContributionLifecycleState::Failed
        );
    }

    #[tokio::test]
    async fn strict_cleanup_and_readiness_deadlines_fail_closed() {
        let strict = declaration(
            "extension:strict",
            omegon_traits::RuntimeCleanupRequirement::Strict,
        );
        let mut candidate = candidate(std::slice::from_ref(&strict));
        candidate
            .transition(
                &strict.id,
                RuntimeContributionLifecycleState::Ready,
                RuntimeLifecycleBoundary::ReadinessSatisfied,
            )
            .unwrap();
        candidate
            .register_resource(
                &strict.id,
                RuntimeContributionResourceId::new("resource:best-effort").unwrap(),
                RuntimeOwnedResourceKind::Task,
                RuntimeCleanupAssurance::BestEffort,
                Duration::from_millis(50),
                Box::new(FakeResource {
                    name: "best-effort",
                    order: Arc::new(Mutex::new(Vec::new())),
                    result: RuntimeCleanupState::Settled,
                    delay: Duration::ZERO,
                }),
            )
            .unwrap();
        let mut owner = CompositionLifecycleOwner::default();
        assert!(owner.promote(candidate, || Ok(())).await.is_err());

        let expired = CandidateGeneration::new(
            RuntimeCompositionGenerationId::new("composition:expired").unwrap(),
            &[strict],
            tokio::time::Instant::now(),
        );
        assert!(owner.promote(expired, || Ok(())).await.is_err());
    }

    #[tokio::test]
    async fn cleanup_timeout_is_reported_unverified() {
        let declaration = declaration(
            "extension:slow",
            omegon_traits::RuntimeCleanupRequirement::BestEffort,
        );
        let owner_id = declaration.id.clone();
        let mut candidate = candidate(&[declaration]);
        candidate
            .register_resource(
                &owner_id,
                RuntimeContributionResourceId::new("resource:slow").unwrap(),
                RuntimeOwnedResourceKind::Task,
                RuntimeCleanupAssurance::BestEffort,
                Duration::from_millis(1),
                Box::new(FakeResource {
                    name: "slow",
                    order: Arc::new(Mutex::new(Vec::new())),
                    result: RuntimeCleanupState::Settled,
                    delay: Duration::from_millis(100),
                }),
            )
            .unwrap();
        let mut owner = CompositionLifecycleOwner::default();
        owner.reject(candidate, "lifecycle:test", "timeout").await;
        assert_eq!(
            owner.last_rejected().unwrap().resources[0].cleanup_state,
            RuntimeCleanupState::Unverified
        );
    }

    #[tokio::test]
    async fn replacement_retires_previous_resources_after_commit() {
        let first = declaration(
            "extension:first",
            omegon_traits::RuntimeCleanupRequirement::BestEffort,
        );
        let mut initial = candidate(std::slice::from_ref(&first));
        initial
            .transition(
                &first.id,
                RuntimeContributionLifecycleState::Ready,
                RuntimeLifecycleBoundary::ReadinessSatisfied,
            )
            .unwrap();
        initial
            .register_resource(
                &first.id,
                RuntimeContributionResourceId::new("resource:first").unwrap(),
                RuntimeOwnedResourceKind::Task,
                RuntimeCleanupAssurance::BestEffort,
                Duration::from_millis(50),
                Box::new(FakeResource {
                    name: "first",
                    order: Arc::new(Mutex::new(Vec::new())),
                    result: RuntimeCleanupState::Settled,
                    delay: Duration::ZERO,
                }),
            )
            .unwrap();
        let mut owner = CompositionLifecycleOwner::default();
        owner.promote(initial, || Ok(())).await.unwrap();

        let second = declaration(
            "extension:second",
            omegon_traits::RuntimeCleanupRequirement::BestEffort,
        );
        let mut replacement = CandidateGeneration::new(
            RuntimeCompositionGenerationId::new("composition:second").unwrap(),
            std::slice::from_ref(&second),
            tokio::time::Instant::now() + Duration::from_secs(1),
        );
        replacement
            .transition(
                &second.id,
                RuntimeContributionLifecycleState::Ready,
                RuntimeLifecycleBoundary::ReadinessSatisfied,
            )
            .unwrap();
        owner.promote(replacement, || Ok(())).await.unwrap();

        let retired = owner.last_retired().unwrap();
        assert_eq!(retired.id.as_str(), "composition:test");
        assert_eq!(
            retired.resources[0].cleanup_state,
            RuntimeCleanupState::Settled
        );
        assert_eq!(
            retired.lifecycle[0].state,
            RuntimeContributionLifecycleState::Retired
        );
    }

    #[test]
    fn restart_budget_backs_off_and_quarantines_until_new_generation() {
        let mut controller =
            RestartController::new(3, Duration::from_millis(10), Duration::from_millis(25));
        assert_eq!(
            controller.record_failure(),
            RestartDecision::RetryAfter(Duration::from_millis(10))
        );
        assert_eq!(
            controller.record_failure(),
            RestartDecision::RetryAfter(Duration::from_millis(20))
        );
        assert_eq!(
            controller.record_failure(),
            RestartDecision::RetryAfter(Duration::from_millis(25))
        );
        assert_eq!(controller.record_failure(), RestartDecision::Quarantined);
        assert_eq!(controller.record_failure(), RestartDecision::Quarantined);
        assert_eq!(controller.attempts(), 3);
        assert!(controller.is_quarantined());

        let replacement =
            RestartController::new(3, Duration::from_millis(10), Duration::from_millis(25));
        assert_eq!(replacement.attempts(), 0);
        assert!(!replacement.is_quarantined());
    }

    #[test]
    fn dynamic_transport_matrix_is_metadata_only_until_admission() {
        let inventory = DynamicContributionInventory::default();
        let matrix = [
            (
                "extension:native",
                RuntimeDynamicSourceKind::NativeExtension,
            ),
            ("mcp:process", RuntimeDynamicSourceKind::McpProcess),
            ("mcp:http", RuntimeDynamicSourceKind::McpHttp),
            ("plugin:script", RuntimeDynamicSourceKind::PluginScript),
            ("plugin:http", RuntimeDynamicSourceKind::PluginHttp),
            ("plugin:oci", RuntimeDynamicSourceKind::OciExtension),
        ];

        for (id, kind) in matrix {
            inventory.discover(dynamic_preflight(id, kind)).unwrap();
        }

        let evidence = inventory.evidence();
        assert_eq!(evidence.len(), matrix.len());
        assert!(evidence.iter().all(|entry| {
            entry.state == DiscoveredContributionState::Discovered
                && entry
                    .candidate
                    .preflight
                    .source_digest
                    .starts_with("sha256:")
        }));
    }

    #[test]
    fn dynamic_transport_matrix_rejects_before_probe_without_trust() {
        let inventory = DynamicContributionInventory::default();
        for (id, kind) in [
            (
                "extension:native",
                RuntimeDynamicSourceKind::NativeExtension,
            ),
            ("mcp:process", RuntimeDynamicSourceKind::McpProcess),
            ("mcp:http", RuntimeDynamicSourceKind::McpHttp),
            ("plugin:script", RuntimeDynamicSourceKind::PluginScript),
            ("plugin:http", RuntimeDynamicSourceKind::PluginHttp),
            ("plugin:oci", RuntimeDynamicSourceKind::OciExtension),
        ] {
            let candidate = inventory.discover(dynamic_preflight(id, kind)).unwrap();
            assert!(
                inventory
                    .admit(
                        &candidate,
                        &crate::dynamic_admission::DynamicAdmissionPolicy::default()
                    )
                    .is_err()
            );
        }
        assert!(
            inventory
                .evidence()
                .iter()
                .all(|entry| entry.state == DiscoveredContributionState::Rejected)
        );
    }

    #[tokio::test]
    async fn publication_rejection_stale_generation_and_optional_absence_are_local() {
        let inventory = DynamicContributionInventory::default();
        let preflight = dynamic_preflight("mcp:http", RuntimeDynamicSourceKind::McpHttp);
        let digest = preflight.source_digest.clone();
        let id = preflight.id.clone();
        inventory.discover(preflight).unwrap();
        inventory.ready(&id);
        let mut owner = DynamicContributionGenerationOwner::new(inventory.clone());
        owner.stage();
        assert!(owner.reject("graph rejected").await.is_empty());
        assert!(inventory.ensure_callable(&id, &digest).is_err());

        let absent = RuntimeContributionId::new("plugin:optional-absent").unwrap();
        assert!(inventory.ensure_callable(&absent, "sha256:absent").is_err());
        assert_eq!(inventory.evidence().len(), 1);
    }

    #[test]
    fn production_dynamic_discovery_has_one_lifecycle_owner() {
        let setup = include_str!("setup.rs");
        let plugins = include_str!("plugins/mod.rs");
        let acp = include_str!("acp_worker.rs");
        let main = include_str!("main.rs");
        for (name, source) in [
            ("setup", setup),
            ("plugins", plugins),
            ("acp", acp),
            ("main", main),
        ] {
            assert!(
                !source.contains("ExtensionSupervisorSet") && !source.contains("McpSupervisorSet"),
                "{name} retains duplicate supervisor-set lifecycle authority"
            );
        }
        assert!(setup.contains("DynamicContributionGenerationOwner::new"));
        assert!(plugins.contains("inventory.discover(preflight)"));
        assert!(
            plugins.find("inventory.admit(&candidate").unwrap()
                < plugins.find("load_plugin_manifest(").unwrap()
        );
        assert!(
            setup.find("inventory.admit(&candidate").unwrap()
                < setup.find("spawn_from_admitted_snapshot(").unwrap()
        );
        assert!(acp.contains("candidate_owner.reject"));
        assert!(main.contains("candidate_owner.shutdown().await"));
    }
}
