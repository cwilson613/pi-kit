use std::collections::BTreeMap;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};

use crate::{AuthorityKey, ContractError, Result, SCHEMA_VERSION, derive_key, path_key};

pub trait Record {
    const RECORD_KIND: &'static str;

    fn schema_version(&self) -> u32;
    fn record_kind(&self) -> &str;
    fn validate(&self) -> Result<()>;
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathIdentityV1 {
    pub dialect: PathDialect,
    pub path_bytes: String,
    pub device: u64,
    pub inode: u64,
    pub key: AuthorityKey,
}

impl PathIdentityV1 {
    pub fn unix(path_bytes: &[u8], device: u64, inode: u64) -> Result<Self> {
        if !path_bytes.starts_with(b"/") || path_bytes.contains(&0) {
            return Err(ContractError::InvalidValue(
                "Unix path identity must contain absolute NUL-free bytes".into(),
            ));
        }
        validate_absolute_unix_path(path_bytes)?;
        Ok(Self {
            dialect: PathDialect::Unix,
            path_bytes: URL_SAFE_NO_PAD.encode(path_bytes),
            device,
            inode,
            key: path_key("unix", path_bytes),
        })
    }

    pub fn decoded_path(&self) -> Result<Vec<u8>> {
        URL_SAFE_NO_PAD
            .decode(&self.path_bytes)
            .map_err(|error| ContractError::InvalidValue(format!("invalid path bytes: {error}")))
    }

    pub fn validate(&self) -> Result<()> {
        let path = self.decoded_path()?;
        if self.dialect != PathDialect::Unix || !path.starts_with(b"/") || path.contains(&0) {
            return Err(ContractError::InvalidValue(
                "unsupported or invalid path identity".into(),
            ));
        }
        validate_absolute_unix_path(&path)?;
        let expected = path_key("unix", &path);
        if self.key != expected {
            return Err(ContractError::InvalidValue(
                "path identity key does not match path bytes".into(),
            ));
        }
        Ok(())
    }
}

pub fn normalize_workspace_path(path: &[u8]) -> Result<Vec<u8>> {
    if !path.starts_with(b"/") || path.contains(&0) {
        return Err(ContractError::InvalidValue(
            "workspace path must be absolute and NUL-free".into(),
        ));
    }
    let mut components: Vec<&[u8]> = Vec::new();
    for component in path.split(|byte| *byte == b'/').skip(1) {
        match component {
            b"" | b"." => {}
            b".." => {
                if components.pop().is_none() {
                    return Err(ContractError::InvalidValue(
                        "workspace path escapes root".into(),
                    ));
                }
            }
            value => components.push(value),
        }
    }
    let mut normalized = Vec::with_capacity(path.len());
    normalized.push(b'/');
    for (index, component) in components.iter().enumerate() {
        if index > 0 {
            normalized.push(b'/');
        }
        normalized.extend_from_slice(component);
    }
    Ok(normalized)
}

pub fn validate_child_name(name: &[u8]) -> Result<()> {
    if name.is_empty()
        || name.contains(&0)
        || name.contains(&b'/')
        || matches!(name, b"." | b"..")
        || name.get(1) == Some(&b':')
    {
        return Err(ContractError::InvalidValue(
            "child path is absolute, traversing, or platform-prefixed".into(),
        ));
    }
    Ok(())
}

fn validate_absolute_unix_path(path: &[u8]) -> Result<()> {
    if normalize_workspace_path(path)? != path {
        return Err(ContractError::InvalidValue(
            "path identity bytes are not lexically canonical".into(),
        ));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathDialect {
    Unix,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallationStateV1 {
    pub schema_version: u32,
    pub record_kind: String,
    pub record_id: AuthorityKey,
    pub installation_uuid: String,
    pub home: PathIdentityV1,
    pub next_audit_sequence: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DenyRecordV1 {
    pub schema_version: u32,
    pub record_kind: String,
    pub record_id: AuthorityKey,
    pub scope_key: AuthorityKey,
    pub contribution_kind: ContributionKind,
    pub entry_key: AuthorityKey,
    pub raw_name_digest: AuthorityKey,
    pub generation: u64,
    pub state: DenyState,
    pub request_id: String,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DenyStateV1 {
    pub schema_version: u32,
    pub record_kind: String,
    pub record_id: AuthorityKey,
    pub scope_key: AuthorityKey,
    pub generation: u64,
    pub entries: BTreeMap<String, DenyRecordV1>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContributionKind {
    Extension,
    Plugin,
    Skill,
    Prompt,
    Catalog,
    Workflow,
}

impl ContributionKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Extension => "extension",
            Self::Plugin => "plugin",
            Self::Skill => "skill",
            Self::Prompt => "prompt",
            Self::Catalog => "catalog",
            Self::Workflow => "workflow",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DenyState {
    Denied,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionDenyRecordV1 {
    pub schema_version: u32,
    pub record_kind: String,
    pub record_id: AuthorityKey,
    pub session_key: AuthorityKey,
    pub session_id: String,
    pub workspace_key: AuthorityKey,
    pub state: SessionDenyState,
    pub request_id: String,
    pub created_at: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionDenyState {
    ResumeDenied,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnershipRecordV1 {
    pub schema_version: u32,
    pub record_kind: String,
    pub record_id: AuthorityKey,
    pub runtime_id: String,
    pub generation_id: String,
    pub workspace_key: AuthorityKey,
    pub boot_id: String,
    pub pid: u32,
    pub process_group: Option<i32>,
    pub process_start_token: String,
    pub lifecycle_boundary: LifecycleBoundary,
    pub cleanup_capability: CleanupCapability,
    pub writer: ArtifactIdentityV1,
    pub heartbeat_utc: String,
    pub heartbeat_monotonic_ticks: u64,
    pub expires_after_seconds: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleBoundary {
    OwnedProcessTree,
    CrossBoundary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanupCapability {
    Strict,
    BestEffort,
    Unverifiable,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransactionV1 {
    pub schema_version: u32,
    pub record_kind: String,
    pub record_id: AuthorityKey,
    pub request_id: String,
    pub command_fingerprint: AuthorityKey,
    pub domain_key: AuthorityKey,
    pub roots: Vec<PathIdentityV1>,
    pub steps: Vec<TransactionStepV1>,
    pub state: TransactionState,
    pub created_at: String,
    pub updated_at: String,
    pub audit_sequence: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransactionStepV1 {
    pub kind: TransactionStepKind,
    pub parent: PathIdentityV1,
    pub basename_digest: AuthorityKey,
    pub expected_existing: Option<FileIdentityV1>,
    pub expected_absence: bool,
    pub intended_content_digest: Option<AuthorityKey>,
    pub state: TransactionStepState,
    pub observed: Option<PostStateV1>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionStepKind {
    DenyStateReplace,
    SessionDenyCreate,
    QuarantineDetach,
    ResourceRecordPrune,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionStepState {
    Prepared,
    Dispatched,
    Settled,
    Aborted,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionState {
    Prepared,
    StepDispatched,
    StepSettled,
    Settled,
    Aborted,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileIdentityV1 {
    pub device: u64,
    pub inode: u64,
    pub size: u64,
    pub modified_ns: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PostStateV1 {
    pub source_present: bool,
    pub destination: Option<FileIdentityV1>,
    pub destination_content_digest: Option<AuthorityKey>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FenceV1 {
    pub schema_version: u32,
    pub record_kind: String,
    pub record_id: AuthorityKey,
    pub domain_key: AuthorityKey,
    pub transaction_record_id: AuthorityKey,
    pub state: FenceState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FenceState {
    Active,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditRecordV1 {
    pub schema_version: u32,
    pub record_kind: String,
    pub record_id: AuthorityKey,
    pub installation_uuid: String,
    pub sequence: u64,
    pub previous_digest: Option<AuthorityKey>,
    pub request_id: String,
    pub command: String,
    pub outcome: ResultStatus,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditCheckpointV1 {
    pub schema_version: u32,
    pub record_kind: String,
    pub record_id: AuthorityKey,
    pub installation_uuid: String,
    pub last_sequence: u64,
    pub last_digest: AuthorityKey,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageManifestV1 {
    pub schema_version: u32,
    pub record_kind: String,
    pub record_id: AuthorityKey,
    pub repository: String,
    pub workflow_identity: String,
    pub issuer: String,
    pub git_ref: String,
    pub tag: String,
    pub commit: String,
    pub version: String,
    pub target: String,
    pub archive_filename: String,
    pub archive_digest: AuthorityKey,
    pub members: Vec<PackageMemberV1>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageMemberV1 {
    pub path: String,
    pub mode: u32,
    pub size: u64,
    pub digest: AuthorityKey,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactIdentityV1 {
    pub version: String,
    pub commit: String,
    pub target: String,
    pub digest: AuthorityKey,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaintenanceResultV1 {
    pub schema_version: u32,
    pub command: String,
    pub status: ResultStatus,
    pub request_id: String,
    pub artifact: ArtifactIdentityV1,
    pub composition: CompositionIdentityV1,
    pub deadline: DeadlineEvidenceV1,
    pub diagnostics: Vec<DiagnosticV1>,
    pub mutations: Vec<MutationResultV1>,
    pub errors: Vec<ErrorV1>,
    pub truncated: bool,
    pub next_cursor: Option<String>,
}

pub mod error_code {
    pub const ROOT_UNSAFE: &str = "root_unsafe";
    pub const RECORD_INVALID: &str = "record_invalid";
    pub const TRANSACTION_UNKNOWN: &str = "transaction_unknown";
    pub const DEADLINE_AFTER_DISPATCH: &str = "deadline_after_dispatch";
    pub const OUTPUT_FAILED: &str = "output_failed";
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultStatus {
    Success,
    Failure,
    Degraded,
}

impl ResultStatus {
    pub const fn exit_code(self) -> u8 {
        match self {
            Self::Success => 0,
            Self::Failure => 1,
            Self::Degraded => 2,
        }
    }
}

impl MaintenanceResultV1 {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(ContractError::UnsupportedSchema(self.schema_version));
        }
        for diagnostic in &self.diagnostics {
            validate_code(&diagnostic.code)?;
            validate_message(&diagnostic.message)?;
            if diagnostic
                .evidence
                .as_ref()
                .is_some_and(|value| value.len() > 4096)
            {
                return Err(ContractError::InvalidValue(
                    "diagnostic evidence exceeds 4096 bytes".into(),
                ));
            }
        }
        for error in &self.errors {
            validate_code(&error.code)?;
            validate_message(&error.message)?;
        }
        if self.status == ResultStatus::Success && !self.errors.is_empty() {
            return Err(ContractError::InvalidValue(
                "successful result cannot contain errors".into(),
            ));
        }
        if self.status == ResultStatus::Success
            && self.mutations.iter().any(|mutation| {
                !matches!(
                    mutation.state,
                    MutationState::Planned | MutationState::Settled
                )
            })
        {
            return Err(ContractError::InvalidValue(
                "successful result contains an unsettled mutation".into(),
            ));
        }
        let paginated = matches!(
            self.command.as_str(),
            "contribution.list" | "session.list" | "resource.list" | "audit.inspect"
        );
        if self.truncated && (!paginated || self.next_cursor.is_none()) {
            return Err(ContractError::InvalidValue(
                "only paginated list results may truncate and they require a cursor".into(),
            ));
        }
        if !self.truncated && self.next_cursor.is_some() {
            return Err(ContractError::InvalidValue(
                "complete results cannot contain a continuation cursor".into(),
            ));
        }
        if self.truncated && (!self.mutations.is_empty() || !self.errors.is_empty()) {
            return Err(ContractError::InvalidValue(
                "mutation and error outcomes cannot be truncated".into(),
            ));
        }
        ensure_sorted(&self.diagnostics, |value| {
            (
                value.severity,
                value.code.clone(),
                value.scope.clone(),
                value.message.clone(),
                value.evidence.clone(),
            )
        })?;
        ensure_sorted(&self.errors, |value| {
            (
                value.code.clone(),
                value.phase.clone(),
                value.retry_safe,
                value.message.clone(),
            )
        })?;
        ensure_sorted(&self.mutations, |value| {
            (
                value.domain_key,
                value.kind.clone(),
                value.state,
                value.retry_safe,
            )
        })?;
        if crate::canonical_json(self)?.len() > crate::MAX_RESULT_BYTES {
            return Err(ContractError::InvalidValue(
                "result exceeds the 4 MiB envelope".into(),
            ));
        }
        Ok(())
    }
}

fn ensure_sorted<T, K: Ord>(values: &[T], key: impl Fn(&T) -> K) -> Result<()> {
    if values.windows(2).any(|pair| key(&pair[0]) > key(&pair[1])) {
        return Err(ContractError::InvalidValue(
            "result arrays must use deterministic order".into(),
        ));
    }
    Ok(())
}

fn validate_code(code: &str) -> Result<()> {
    const FAMILIES: &[&str] = &[
        "cli_",
        "root_",
        "path_",
        "limit_",
        "lock_",
        "record_",
        "deny_",
        "session_",
        "resource_",
        "release_",
        "transaction_",
        "audit_",
        "deadline_",
        "output_",
    ];
    if code.is_empty()
        || !code
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        || !FAMILIES.iter().any(|family| code.starts_with(family))
    {
        return Err(ContractError::InvalidValue(
            "diagnostic/error code is outside stable v1 families".into(),
        ));
    }
    Ok(())
}

fn validate_message(message: &str) -> Result<()> {
    if message.len() > 4096 || message.chars().any(char::is_control) {
        return Err(ContractError::InvalidValue(
            "message is unbounded or contains control characters".into(),
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompositionIdentityV1 {
    pub profile: String,
    pub generation: AuthorityKey,
    pub excluded_inputs: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeadlineEvidenceV1 {
    pub requested_ms: u64,
    pub elapsed_ms: u64,
    pub expired: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticV1 {
    pub code: String,
    pub severity: Severity,
    pub scope: String,
    pub message: String,
    pub evidence: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MutationResultV1 {
    pub domain_key: AuthorityKey,
    pub kind: String,
    pub state: MutationState,
    pub retry_safe: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationState {
    Planned,
    Prepared,
    Dispatched,
    Applied,
    Settled,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ErrorV1 {
    pub code: String,
    pub phase: String,
    pub retry_safe: bool,
    pub message: String,
}

macro_rules! impl_record {
    ($type:ty, $kind:literal, $validate:expr) => {
        impl Record for $type {
            const RECORD_KIND: &'static str = $kind;

            fn schema_version(&self) -> u32 {
                self.schema_version
            }

            fn record_kind(&self) -> &str {
                &self.record_kind
            }

            fn validate(&self) -> Result<()> {
                ($validate)(self)
            }
        }
    };
}

fn require_header<T: Record>(record: &T) -> Result<()> {
    if record.schema_version() != SCHEMA_VERSION {
        return Err(ContractError::UnsupportedSchema(record.schema_version()));
    }
    if record.record_kind() != T::RECORD_KIND {
        return Err(ContractError::RecordKind {
            expected: T::RECORD_KIND,
            actual: record.record_kind().to_owned(),
        });
    }
    Ok(())
}

fn require_record_id(actual: AuthorityKey, expected: AuthorityKey) -> Result<()> {
    if actual != expected {
        return Err(ContractError::InvalidValue(
            "record ID does not match authority fields".into(),
        ));
    }
    Ok(())
}

fn validate_uuid(value: &str) -> Result<()> {
    if value.len() != 36
        || value.bytes().enumerate().any(|(index, byte)| match index {
            8 | 13 | 18 | 23 => byte != b'-',
            _ => !byte.is_ascii_hexdigit() || byte.is_ascii_uppercase(),
        })
    {
        return Err(ContractError::InvalidValue(
            "request ID must be a lowercase UUID".into(),
        ));
    }
    Ok(())
}

fn validate_timestamp(value: &str) -> Result<(u32, u32, u32, u32, u32, u32)> {
    let bytes = value.as_bytes();
    if bytes.len() != 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || bytes[19] != b'Z'
        || bytes.iter().enumerate().any(|(index, byte)| {
            !matches!(index, 4 | 7 | 10 | 13 | 16 | 19) && !byte.is_ascii_digit()
        })
    {
        return Err(ContractError::InvalidValue(
            "timestamp must use UTC second precision".into(),
        ));
    }
    let parse = |range: std::ops::Range<usize>| -> u32 {
        std::str::from_utf8(&bytes[range])
            .expect("timestamp digits are ASCII")
            .parse()
            .expect("timestamp fields contain only digits")
    };
    let year = parse(0..4);
    let month = parse(5..7);
    let day = parse(8..10);
    let hour = parse(11..13);
    let minute = parse(14..16);
    let second = parse(17..19);
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    if day == 0 || day > days_in_month || hour > 23 || minute > 59 || second > 59 {
        return Err(ContractError::InvalidValue(
            "timestamp contains an out-of-range field".into(),
        ));
    }
    Ok((year, month, day, hour, minute, second))
}

fn validate_transaction_state(value: &TransactionV1) -> Result<()> {
    let states: Vec<_> = value.steps.iter().map(|step| step.state).collect();
    let prefix_then = |frontier: TransactionStepState, require_settled: bool| {
        let Some(index) = states
            .iter()
            .position(|state| *state != TransactionStepState::Settled)
        else {
            return false;
        };
        (!require_settled || index > 0)
            && states[index] == frontier
            && states[index + 1..]
                .iter()
                .all(|state| *state == TransactionStepState::Prepared)
    };
    let valid = match value.state {
        TransactionState::Prepared => states
            .iter()
            .all(|state| *state == TransactionStepState::Prepared),
        TransactionState::StepDispatched => prefix_then(TransactionStepState::Dispatched, false),
        TransactionState::StepSettled => prefix_then(TransactionStepState::Prepared, true),
        TransactionState::Settled => states
            .iter()
            .all(|state| *state == TransactionStepState::Settled),
        TransactionState::Aborted => prefix_then(TransactionStepState::Aborted, false),
        TransactionState::Unknown => {
            prefix_then(TransactionStepState::Unknown, false)
                || prefix_then(TransactionStepState::Dispatched, false)
        }
    };
    if !valid {
        return Err(ContractError::InvalidValue(
            "transaction state contradicts its step states".into(),
        ));
    }
    let audit_valid = match value.state {
        TransactionState::Settled | TransactionState::Aborted => value.audit_sequence.is_some(),
        TransactionState::Prepared
        | TransactionState::StepDispatched
        | TransactionState::StepSettled => value.audit_sequence.is_none(),
        TransactionState::Unknown => true,
    };
    if !audit_valid {
        return Err(ContractError::InvalidValue(
            "settled/aborted transactions require audit sequence; active transactions forbid it"
                .into(),
        ));
    }
    Ok(())
}

fn validate_step_evidence(step: &TransactionStepV1) -> Result<()> {
    match step.state {
        TransactionStepState::Prepared
        | TransactionStepState::Dispatched
        | TransactionStepState::Aborted
            if step.observed.is_some() =>
        {
            return Err(ContractError::InvalidValue(
                "undetermined transaction step cannot contain post-state evidence".into(),
            ));
        }
        TransactionStepState::Settled => {
            let observed = step.observed.as_ref().ok_or_else(|| {
                ContractError::InvalidValue("settled step requires post-state evidence".into())
            })?;
            match step.kind {
                TransactionStepKind::DenyStateReplace | TransactionStepKind::SessionDenyCreate => {
                    if observed.destination.is_none()
                        || observed.destination_content_digest != step.intended_content_digest
                    {
                        return Err(ContractError::InvalidValue(
                            "settled record write does not prove intended canonical bytes".into(),
                        ));
                    }
                }
                TransactionStepKind::QuarantineDetach => {
                    if observed.source_present
                        || observed.destination.is_none()
                        || observed.destination_content_digest.is_none()
                    {
                        return Err(ContractError::InvalidValue(
                            "settled detach requires source absence and destination identity/content"
                                .into(),
                        ));
                    }
                }
                TransactionStepKind::ResourceRecordPrune => {
                    if observed.source_present
                        || observed.destination.is_some()
                        || observed.destination_content_digest.is_some()
                    {
                        return Err(ContractError::InvalidValue(
                            "settled prune requires proven source absence only".into(),
                        ));
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}

impl_record!(
    InstallationStateV1,
    "installation_state",
    |value: &InstallationStateV1| {
        require_header(value)?;
        validate_uuid(&value.installation_uuid)?;
        value.home.validate()?;
        require_record_id(
            value.record_id,
            derive_key("installation", &[value.installation_uuid.as_bytes()]),
        )
    }
);
impl_record!(DenyRecordV1, "deny", |value: &DenyRecordV1| {
    require_header(value)?;
    validate_uuid(&value.request_id)?;
    validate_timestamp(&value.created_at)?;
    require_record_id(
        value.record_id,
        derive_key(
            "deny",
            &[
                value.scope_key.as_bytes(),
                value.entry_key.as_bytes(),
                value.request_id.as_bytes(),
            ],
        ),
    )
});
impl_record!(DenyStateV1, "deny_state", |value: &DenyStateV1| {
    require_header(value)?;
    require_record_id(
        value.record_id,
        derive_key(
            "deny-state",
            &[value.scope_key.as_bytes(), &value.generation.to_be_bytes()],
        ),
    )?;
    if value.entries.len() > 10_000 {
        return Err(ContractError::InvalidValue(
            "deny state exceeds entry limit".into(),
        ));
    }
    for (key, entry) in &value.entries {
        entry.validate()?;
        if key != &entry.entry_key.to_hex()
            || entry.scope_key != value.scope_key
            || entry.generation != value.generation
        {
            return Err(ContractError::InvalidValue(
                "deny entry does not match container".into(),
            ));
        }
    }
    Ok(())
});
impl_record!(
    SessionDenyRecordV1,
    "session_deny",
    |value: &SessionDenyRecordV1| {
        require_header(value)?;
        validate_uuid(&value.request_id)?;
        validate_timestamp(&value.created_at)?;
        require_record_id(
            value.session_key,
            crate::session_key(&value.session_id, value.workspace_key),
        )?;
        require_record_id(
            value.record_id,
            derive_key(
                "session-deny",
                &[value.session_key.as_bytes(), value.request_id.as_bytes()],
            ),
        )
    }
);
impl_record!(
    OwnershipRecordV1,
    "ownership",
    |value: &OwnershipRecordV1| {
        require_header(value)?;
        if value.expires_after_seconds != 300 {
            return Err(ContractError::InvalidValue(
                "ownership expiry must be 300 seconds".into(),
            ));
        }
        validate_timestamp(&value.heartbeat_utc)?;
        require_record_id(
            value.record_id,
            derive_key(
                "ownership",
                &[
                    value.workspace_key.as_bytes(),
                    value.runtime_id.as_bytes(),
                    value.generation_id.as_bytes(),
                ],
            ),
        )
    }
);
impl_record!(TransactionV1, "transaction", |value: &TransactionV1| {
    require_header(value)?;
    require_record_id(
        value.record_id,
        derive_key("transaction", &[value.request_id.as_bytes()]),
    )?;
    if value.steps.is_empty() {
        return Err(ContractError::InvalidValue(
            "transaction must contain a step".into(),
        ));
    }
    if value.roots.is_empty() {
        return Err(ContractError::InvalidValue(
            "transaction must contain an opened root identity".into(),
        ));
    }
    for root in &value.roots {
        root.validate()?;
    }
    for step in &value.steps {
        step.parent.validate()?;
        if step.expected_absence == step.expected_existing.is_some() {
            return Err(ContractError::InvalidValue(
                "transaction step must expect exactly absence or an existing identity".into(),
            ));
        }
        if step.observed.as_ref().is_some_and(|observed| {
            observed.destination.is_none() && observed.destination_content_digest.is_some()
        }) {
            return Err(ContractError::InvalidValue(
                "destination digest requires a destination identity".into(),
            ));
        }
        let kind_valid = match step.kind {
            TransactionStepKind::DenyStateReplace => {
                !step.expected_absence
                    && step.expected_existing.is_some()
                    && step.intended_content_digest.is_some()
            }
            TransactionStepKind::SessionDenyCreate => {
                step.expected_absence
                    && step.expected_existing.is_none()
                    && step.intended_content_digest.is_some()
            }
            TransactionStepKind::QuarantineDetach | TransactionStepKind::ResourceRecordPrune => {
                !step.expected_absence
                    && step.expected_existing.is_some()
                    && step.intended_content_digest.is_none()
            }
        };
        if !kind_valid {
            return Err(ContractError::InvalidValue(
                "transaction step evidence does not match its kind".into(),
            ));
        }
        validate_step_evidence(step)?;
    }
    validate_transaction_state(value)?;
    let created_at = validate_timestamp(&value.created_at)?;
    let updated_at = validate_timestamp(&value.updated_at)?;
    if created_at > updated_at {
        return Err(ContractError::InvalidValue(
            "transaction updated_at precedes created_at".into(),
        ));
    }
    validate_uuid(&value.request_id)?;
    Ok(())
});
impl_record!(FenceV1, "fence", |value: &FenceV1| {
    require_header(value)?;
    require_record_id(
        value.record_id,
        derive_key(
            "fence",
            &[
                value.domain_key.as_bytes(),
                value.transaction_record_id.as_bytes(),
            ],
        ),
    )
});
impl_record!(AuditRecordV1, "audit", |value: &AuditRecordV1| {
    require_header(value)?;
    validate_uuid(&value.installation_uuid)?;
    validate_uuid(&value.request_id)?;
    require_record_id(
        value.record_id,
        derive_key(
            "audit",
            &[
                value.installation_uuid.as_bytes(),
                &value.sequence.to_be_bytes(),
            ],
        ),
    )
});
impl_record!(
    AuditCheckpointV1,
    "audit_checkpoint",
    |value: &AuditCheckpointV1| {
        require_header(value)?;
        validate_uuid(&value.installation_uuid)?;
        require_record_id(
            value.record_id,
            derive_key(
                "audit-checkpoint",
                &[
                    value.installation_uuid.as_bytes(),
                    &value.last_sequence.to_be_bytes(),
                    value.last_digest.as_bytes(),
                ],
            ),
        )
    }
);
impl_record!(
    PackageManifestV1,
    "package_manifest",
    |value: &PackageManifestV1| {
        require_header(value)?;
        require_record_id(
            value.record_id,
            derive_key(
                "package",
                &[
                    value.archive_digest.as_bytes(),
                    value.target.as_bytes(),
                    value.version.as_bytes(),
                ],
            ),
        )?;
        if value.members.len() < 2
            || !value.members.iter().any(|member| member.path == "omegon")
            || !value
                .members
                .iter()
                .any(|member| member.path == "omegon-maintain")
        {
            return Err(ContractError::InvalidValue(
                "package manifest must contain both executables".into(),
            ));
        }
        Ok(())
    }
);
