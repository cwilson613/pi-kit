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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RuntimeContributionSchemaVersion {
    V1,
}

pub const RUNTIME_CONTRIBUTION_SCHEMA_VERSION: RuntimeContributionSchemaVersion =
    RuntimeContributionSchemaVersion::V1;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeExecutionPolicy {
    pub timeout_class: RuntimeTimeoutClass,
    pub retry_class: RuntimeRetryClass,
    pub idempotency: RuntimeIdempotency,
    pub deduplication: RuntimeDeduplication,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_attempts: Option<u16>,
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
        self.protocol.validate()
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
