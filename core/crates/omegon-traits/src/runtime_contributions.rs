//! Renderer-neutral declarations for runtime contribution graph candidates.
//!
//! These contracts describe requested composition and lifecycle policy. They do
//! not grant trust, confinement, admission, readiness, or invocation authority.

use crate::{RuntimeCapabilityId, RuntimeCapabilityKind, RuntimeInvocationKind};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

macro_rules! scoped_id {
    ($name:ident, $description:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, &'static str> {
                let value = value.into();
                let Some((namespace, name)) = value.split_once(':') else {
                    return Err(concat!($description, " must contain a namespace separator"));
                };
                if namespace.is_empty()
                    || name.is_empty()
                    || value
                        .split('/')
                        .any(|segment| segment.is_empty() || segment == "..")
                    || !value.chars().all(|ch| {
                        ch.is_ascii_alphanumeric() || matches!(ch, ':' | '-' | '_' | '.' | '/')
                    })
                {
                    return Err(concat!($description, " contains invalid characters"));
                }
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::new(value).map_err(serde::de::Error::custom)
            }
        }
    };
}

scoped_id!(RuntimeContributionId, "contribution id");
scoped_id!(RuntimeCompositionGenerationId, "composition generation id");
scoped_id!(
    RuntimeContributionGenerationId,
    "contribution generation id"
);
scoped_id!(RuntimeDiagnosticCode, "diagnostic code");
scoped_id!(RuntimeCapabilityGroupId, "capability group id");
scoped_id!(RuntimeContributionResourceId, "contribution resource id");
scoped_id!(RuntimeMutationDomainId, "mutation domain id");
scoped_id!(RuntimeMutationFenceKey, "mutation fence key");

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RuntimeContributionSchemaVersion {
    V1,
}

pub const RUNTIME_CONTRIBUTION_SCHEMA_VERSION: RuntimeContributionSchemaVersion =
    RuntimeContributionSchemaVersion::V1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RuntimeDynamicPreflightSchemaVersion {
    V1,
}

pub const RUNTIME_DYNAMIC_PREFLIGHT_SCHEMA_VERSION: RuntimeDynamicPreflightSchemaVersion =
    RuntimeDynamicPreflightSchemaVersion::V1;

impl Serialize for RuntimeContributionSchemaVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u16(match self {
            Self::V1 => 1,
        })
    }
}

impl<'de> Deserialize<'de> for RuntimeContributionSchemaVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match u16::deserialize(deserializer)? {
            1 => Ok(Self::V1),
            version => Err(serde::de::Error::custom(format!(
                "unsupported runtime contribution schema version: {version}"
            ))),
        }
    }
}

impl Serialize for RuntimeDynamicPreflightSchemaVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u16(match self {
            Self::V1 => 1,
        })
    }
}

impl<'de> Deserialize<'de> for RuntimeDynamicPreflightSchemaVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match u16::deserialize(deserializer)? {
            1 => Ok(Self::V1),
            version => Err(serde::de::Error::custom(format!(
                "unsupported runtime dynamic preflight schema version: {version}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeOwnerTier {
    ConstitutionalKernel,
    System,
    Vendor,
    Operator,
    Project,
    External,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeTrustRequest {
    KernelBuiltIn,
    ReleaseArtifact,
    OperatorManaged,
    UntrustedDynamic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeConfinementRequest {
    None,
    HostProcess,
    OsSandbox,
    Oci,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeDynamicSourceKind {
    NativeExtension,
    OciExtension,
    PluginManifestEvaluation,
    PluginScript,
    PluginHttp,
    McpProcess,
    McpHttp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeProbeOperation {
    EvaluateManifest,
    Initialize,
    DiscoverCapabilities,
    GenerateContext,
    Connect,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeProbeRequirements {
    pub operations: Vec<RuntimeProbeOperation>,
    pub timeout_ms: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requested_effects: Vec<RuntimeEffect>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeDynamicContributionPreflight {
    pub schema_version: RuntimeDynamicPreflightSchemaVersion,
    pub id: RuntimeContributionId,
    /// Digest of the immutable bytes from which code or connection parameters
    /// will be evaluated. This binds admission to a specific discovered source.
    pub source_digest: String,
    pub source_kind: RuntimeDynamicSourceKind,
    pub protocol: RuntimeProtocolRange,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub minimum_dependencies: Vec<RuntimeContributionDependency>,
    pub requested_trust: RuntimeTrustRequest,
    pub requested_confinement: RuntimeConfinementRequest,
    pub probe: RuntimeProbeRequirements,
}

impl RuntimeDynamicContributionPreflight {
    pub fn validate(&self) -> Result<(), &'static str> {
        self.protocol.validate()?;
        if self.source_digest.trim().is_empty() {
            return Err("dynamic preflight source digest must not be empty");
        }
        if self.probe.operations.is_empty() {
            return Err("dynamic preflight must declare at least one probe operation");
        }
        if self.probe.timeout_ms == 0 {
            return Err("dynamic preflight probe timeout must be non-zero");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeTrustedCodeAuthority {
    KernelRelease,
    OperatorPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeTrustAdmissionEvidence {
    TrustedCode {
        authority: RuntimeTrustedCodeAuthority,
        policy_id: String,
    },
    VerifiedConfinement {
        boundary: RuntimeConfinementRequest,
        verifier: String,
        profile: String,
        prevented_effects: Vec<RuntimeEffect>,
        brokered_effects_only: bool,
    },
}

impl RuntimeTrustAdmissionEvidence {
    pub fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::TrustedCode { policy_id, .. } if policy_id.trim().is_empty() => {
                Err("trusted-code admission policy id must not be empty")
            }
            Self::VerifiedConfinement {
                boundary,
                verifier,
                profile,
                prevented_effects,
                brokered_effects_only,
            } => {
                if !matches!(
                    boundary,
                    RuntimeConfinementRequest::OsSandbox | RuntimeConfinementRequest::Oci
                ) {
                    return Err("verified confinement requires an OS sandbox or OCI boundary");
                }
                if verifier.trim().is_empty() || profile.trim().is_empty() {
                    return Err("verified confinement requires verifier and profile identity");
                }
                if !brokered_effects_only {
                    return Err(
                        "verified confinement must force privileged effects through brokers",
                    );
                }
                const REQUIRED: [RuntimeEffect; 4] = [
                    RuntimeEffect::FilesystemRead,
                    RuntimeEffect::ProcessSpawn,
                    RuntimeEffect::NetworkAccess,
                    RuntimeEffect::SecretDelivery,
                ];
                if REQUIRED
                    .iter()
                    .any(|effect| !prevented_effects.contains(effect))
                {
                    return Err(
                        "verified confinement must prevent direct filesystem, process, network, and secret access",
                    );
                }
                Ok(())
            }
            Self::TrustedCode { .. } => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeTrustAdmission {
    pub schema_version: RuntimeDynamicPreflightSchemaVersion,
    pub contribution_id: RuntimeContributionId,
    pub source_digest: String,
    pub evidence: RuntimeTrustAdmissionEvidence,
}

impl RuntimeTrustAdmission {
    pub fn validate_for(
        &self,
        preflight: &RuntimeDynamicContributionPreflight,
    ) -> Result<(), &'static str> {
        preflight.validate()?;
        self.evidence.validate()?;
        if self.contribution_id != preflight.id || self.source_digest != preflight.source_digest {
            return Err("trust admission is not bound to this preflight source");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeProtocolRange {
    pub minimum: u16,
    pub maximum: u16,
}

impl<'de> Deserialize<'de> for RuntimeProtocolRange {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireRange {
            minimum: u16,
            maximum: u16,
        }

        let range = WireRange::deserialize(deserializer)?;
        Self::new(range.minimum, range.maximum).map_err(serde::de::Error::custom)
    }
}

impl RuntimeProtocolRange {
    pub fn new(minimum: u16, maximum: u16) -> Result<Self, &'static str> {
        if minimum == 0 || minimum > maximum {
            return Err("protocol range must be non-zero and ordered");
        }
        Ok(Self { minimum, maximum })
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        Self::new(self.minimum, self.maximum).map(|_| ())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimePlatformRequirements {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub operating_systems: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub architectures: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_substrates: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeEffect {
    FilesystemRead,
    FilesystemWrite,
    ProcessSpawn,
    NetworkAccess,
    SecretDelivery,
    TerminalAccess,
    DurableStateWrite,
    RuntimeControl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeEffectEvidenceKind {
    Requested,
    Observed,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RuntimeEffectEvidence {
    pub contribution_id: RuntimeContributionId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_id: Option<RuntimeCapabilityId>,
    pub effect: RuntimeEffect,
    pub kind: RuntimeEffectEvidenceKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeTimeoutClass {
    Immediate,
    Interactive,
    Background,
    LongRunning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeRetryClass {
    Never,
    TransientFailure,
    IdempotentFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeIdempotency {
    NonIdempotent,
    Idempotent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeDeduplication {
    Unsupported,
    OwnerEnforcedStableCallId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimePrincipalClass {
    Model,
    Operator,
    Service,
    Internal,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeParallelism {
    #[default]
    Serial,
    ParallelSafe,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeTransactionBehavior {
    #[default]
    None,
    IndependentMutation,
    BestEffortRollback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeMutationFence {
    pub domain: RuntimeMutationDomainId,
    pub key: RuntimeMutationFenceKey,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeExecutionPolicy {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub principals: Vec<RuntimePrincipalClass>,
    pub timeout_class: RuntimeTimeoutClass,
    pub retry_class: RuntimeRetryClass,
    pub idempotency: RuntimeIdempotency,
    pub deduplication: RuntimeDeduplication,
    #[serde(default, skip_serializing_if = "is_serial_execution")]
    pub parallelism: RuntimeParallelism,
    #[serde(default, skip_serializing_if = "is_non_transactional")]
    pub transaction: RuntimeTransactionBehavior,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mutation_fence: Option<Box<RuntimeMutationFence>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_attempts: Option<u16>,
}

fn is_serial_execution(value: &RuntimeParallelism) -> bool {
    *value == RuntimeParallelism::Serial
}

fn is_non_transactional(value: &RuntimeTransactionBehavior) -> bool {
    *value == RuntimeTransactionBehavior::None
}

impl RuntimeExecutionPolicy {
    pub fn validate(&self, effects: &[RuntimeEffect]) -> Result<(), &'static str> {
        if self.principals.is_empty() {
            return Err("execution policy must admit at least one principal class");
        }
        if self.max_attempts == Some(0) {
            return Err("execution policy max_attempts must be non-zero");
        }
        if self.retry_class != RuntimeRetryClass::Never
            && self.idempotency == RuntimeIdempotency::NonIdempotent
            && self.deduplication == RuntimeDeduplication::Unsupported
        {
            return Err("retryable non-idempotent execution requires owner-enforced deduplication");
        }
        if self.parallelism == RuntimeParallelism::ParallelSafe
            && self.transaction == RuntimeTransactionBehavior::BestEffortRollback
        {
            return Err("best-effort rollback execution must be serial");
        }

        let mutates = runtime_effects_mutate(effects);
        if mutates && self.transaction == RuntimeTransactionBehavior::None {
            return Err("mutating execution must declare transaction behavior");
        }
        if mutates && self.mutation_fence.is_none() {
            return Err("mutating execution must declare a mutation fence");
        }
        if !mutates && self.transaction != RuntimeTransactionBehavior::None {
            return Err("non-mutating execution cannot declare mutation transaction behavior");
        }
        if !mutates && self.mutation_fence.is_some() {
            return Err("non-mutating execution cannot declare a mutation fence");
        }
        Ok(())
    }
}

pub fn runtime_effects_mutate(effects: &[RuntimeEffect]) -> bool {
    effects.iter().any(|effect| {
        matches!(
            effect,
            RuntimeEffect::FilesystemWrite
                | RuntimeEffect::DurableStateWrite
                | RuntimeEffect::RuntimeControl
        )
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeToolPolicy {
    pub effects: Vec<RuntimeEffect>,
    pub execution: RuntimeExecutionPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeSurface {
    Tui,
    Cli,
    Acp,
    Ipc,
    Web,
    Daemon,
    Headless,
    Model,
    Internal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeInvocationBindingRole {
    Canonical,
    Alias,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RuntimeInvocationBinding {
    pub kind: RuntimeInvocationKind,
    pub name: String,
    pub role: RuntimeInvocationBindingRole,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeContributionCapabilityDeclaration {
    pub id: RuntimeCapabilityId,
    pub kind: RuntimeCapabilityKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bindings: Vec<RuntimeInvocationBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<RuntimeEffect>,
    pub execution: RuntimeExecutionPolicy,
    pub transition: RuntimeCapabilityTransitionPolicy,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub surfaces: Vec<RuntimeSurface>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeContributionCapabilityGroup {
    pub id: RuntimeCapabilityGroupId,
    pub members: Vec<RuntimeCapabilityId>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeDependencyTarget {
    Contribution { id: RuntimeContributionId },
    Capability { id: RuntimeCapabilityId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeDependencyRequirement {
    Required,
    Optional,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RuntimeContributionDependency {
    pub target: RuntimeDependencyTarget,
    pub requirement: RuntimeDependencyRequirement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeLifecycleRequirement {
    Required,
    Optional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeFailureDisposition {
    FailComposition,
    DegradeLocally,
    Quarantine,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeLifecyclePolicy {
    pub requirement: RuntimeLifecycleRequirement,
    pub failure_disposition: RuntimeFailureDisposition,
    pub readiness_timeout_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heartbeat_timeout_ms: Option<u64>,
    pub restart_limit: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeActivationBoundary {
    Boot,
    QuiescentSession,
    ProjectionBoundary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeAuthorityNarrowing {
    RevokeImmediately,
    DrainExisting,
    CompleteExisting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeCleanupRequirement {
    Strict,
    BestEffort,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeCapabilityTransitionPolicy {
    pub authority_narrowing: RuntimeAuthorityNarrowing,
    pub active_call_timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeCompositionTransitionPolicy {
    pub activation_boundary: RuntimeActivationBoundary,
    pub cleanup: RuntimeCleanupRequirement,
    pub cleanup_timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeContributionDeclaration {
    pub schema_version: RuntimeContributionSchemaVersion,
    pub id: RuntimeContributionId,
    pub generation_id: RuntimeContributionGenerationId,
    pub owner_tier: RuntimeOwnerTier,
    pub requested_trust: RuntimeTrustRequest,
    pub requested_confinement: RuntimeConfinementRequest,
    pub protocol: RuntimeProtocolRange,
    #[serde(default)]
    pub platform: RuntimePlatformRequirements,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<RuntimeContributionDependency>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<RuntimeContributionId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replaces: Vec<RuntimeContributionId>,
    pub lifecycle: RuntimeLifecyclePolicy,
    pub transition: RuntimeCompositionTransitionPolicy,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<RuntimeContributionCapabilityDeclaration>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<RuntimeContributionCapabilityGroup>,
}

impl RuntimeContributionDeclaration {
    pub fn validate(&self) -> Result<(), &'static str> {
        self.protocol.validate()?;
        for capability in &self.capabilities {
            capability.execution.validate(&capability.effects)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeContributionLifecycleState {
    Discovered,
    Candidate,
    Quarantined,
    Ready,
    Active,
    Degraded,
    Draining,
    Failed,
    Retired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeLifecycleBoundary {
    Discovered,
    TrustAdmitted,
    ProbeStarted,
    DeclarationsFrozen,
    GraphValidated,
    ReadinessSatisfied,
    PublicationPrepared,
    Promoted,
    DrainStarted,
    CleanupStarted,
    CleanupSettled,
    Retired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeCleanupAssurance {
    Strict,
    BestEffort,
    Unverified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeCleanupState {
    NotRequired,
    Pending,
    Settled,
    Degraded,
    Unverified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeOwnedResourceKind {
    ProcessTree,
    Task,
    Socket,
    Subscription,
    TemporaryDirectory,
    DurableWriter,
    RemoteService,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeContributionLifecycleRecord {
    pub schema_version: RuntimeContributionSchemaVersion,
    pub composition_generation_id: RuntimeCompositionGenerationId,
    pub contribution_id: RuntimeContributionId,
    pub generation_id: RuntimeContributionGenerationId,
    pub state: RuntimeContributionLifecycleState,
    pub last_completed_boundary: RuntimeLifecycleBoundary,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<RuntimeDiagnosticCode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub restart_attempts: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_restart_not_before_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_heartbeat_ms: Option<u64>,
    pub cleanup_assurance: RuntimeCleanupAssurance,
    pub cleanup_state: RuntimeCleanupState,
}

impl RuntimeContributionLifecycleRecord {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self
            .reason
            .as_ref()
            .is_some_and(|reason| reason.len() > 512)
        {
            return Err("lifecycle reason exceeds 512 bytes");
        }
        if self.reason.is_some() != self.reason_code.is_some() {
            return Err("lifecycle reason and reason code must be present together");
        }
        if self.cleanup_assurance == RuntimeCleanupAssurance::Strict
            && self.cleanup_state == RuntimeCleanupState::Unverified
        {
            return Err("strict cleanup assurance cannot report unverified cleanup");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeOwnedResourceRecord {
    pub schema_version: RuntimeContributionSchemaVersion,
    pub id: RuntimeContributionResourceId,
    pub composition_generation_id: RuntimeCompositionGenerationId,
    pub contribution_id: RuntimeContributionId,
    pub generation_id: RuntimeContributionGenerationId,
    pub kind: RuntimeOwnedResourceKind,
    pub cleanup_assurance: RuntimeCleanupAssurance,
    pub cleanup_state: RuntimeCleanupState,
}

impl RuntimeOwnedResourceRecord {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.cleanup_assurance == RuntimeCleanupAssurance::Strict
            && self.cleanup_state == RuntimeCleanupState::Unverified
        {
            return Err("strict resource cleanup cannot be unverified");
        }
        if self.kind == RuntimeOwnedResourceKind::RemoteService
            && self.cleanup_assurance == RuntimeCleanupAssurance::Strict
        {
            return Err("a remote service cannot claim strict host cleanup assurance");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeContributionGenerationRef {
    pub contribution_id: RuntimeContributionId,
    pub generation_id: RuntimeContributionGenerationId,
    pub lifecycle_state: RuntimeContributionLifecycleState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeCompositionGenerationState {
    Candidate,
    Active,
    Rejected,
    Retired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeCompositionGeneration {
    pub schema_version: RuntimeContributionSchemaVersion,
    pub id: RuntimeCompositionGenerationId,
    pub state: RuntimeCompositionGenerationState,
    pub contributions: Vec<RuntimeContributionGenerationRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<RuntimeContributionDiagnostic>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeDiagnosticSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeDiagnosticSubject {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contribution_id: Option<RuntimeContributionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_id: Option<RuntimeCapabilityId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invocation: Option<RuntimeInvocationBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeContributionDiagnostic {
    pub code: RuntimeDiagnosticCode,
    pub severity: RuntimeDiagnosticSeverity,
    pub subject: RuntimeDiagnosticSubject,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_contributions: Vec<RuntimeContributionId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_capabilities: Vec<RuntimeCapabilityId>,
    pub message: String,
}

impl RuntimeContributionDiagnostic {
    pub fn stable_order_key(&self) -> String {
        let encoded = serde_json::to_string(self)
            .expect("runtime contribution diagnostics contain only serializable values");
        format!("{}\0{}", self.code.as_str(), encoded)
    }
}
