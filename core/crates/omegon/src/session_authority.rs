use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};

use chrono::DateTime;
use omegon_traits::{
    RuntimeCapabilityId, RuntimeCapabilityTransitionPolicy, RuntimeCompositionGenerationId,
    RuntimeContributionGenerationId, RuntimeContributionId, RuntimeEffect, RuntimeExecutionPolicy,
    RuntimeInvocationKind, RuntimePrincipalClass, RuntimeSurface,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

const ENVELOPE_VERSION: u16 = 1;
const EVENT_VERSION: u16 = 1;
const SNAPSHOT_VERSION: u16 = 2;
const REDUCER_VERSION: u16 = 2;
const MAX_RECORD_BYTES: usize = 1024 * 1024;
const MAX_ATTACHMENT_BYTES: u64 = 64 * 1024 * 1024;
const RECOVERY_NAMESPACE: Uuid = Uuid::from_u128(0x5907_b852_acde_4b53_a6b1_2d1a_c964_868a);
const INVOCATION_COMMAND_NAMESPACE: Uuid =
    Uuid::from_u128(0x39b4_58e2_e917_4210_9b34_d45d_c14d_48da);

#[derive(Debug, thiserror::Error)]
pub(crate) enum AuthorityError {
    #[error("authority I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("authority JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("authority record is invalid: {0}")]
    Invalid(String),
    #[error("authority transition is invalid at sequence {sequence}: {message}")]
    Transition { sequence: u64, message: String },
}

type Result<T> = std::result::Result<T, AuthorityError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum QueueMode {
    InterruptAfterTurn,
    UntilReady,
    Immediate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum InterruptionKind {
    Cancel,
    Revoke,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PromptRemovalReason {
    Withdrawn,
    SessionClosing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum InvocationOutcome {
    Completed,
    Failed,
    Cancelled,
    TimedOut,
    Revoked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TurnOutcome {
    Completed,
    Failed,
    Cancelled,
    TimedOut,
    Revoked,
    Interrupted,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ActorIdentity {
    pub(crate) principal: String,
    pub(crate) ingress: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AttachmentRef {
    pub(crate) digest: String,
    pub(crate) media_type: String,
    pub(crate) byte_length: u64,
    pub(crate) storage_ref: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PromptContent {
    pub(crate) text: String,
    pub(crate) attachments: Vec<AttachmentRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionCreated {
    pub(crate) workspace_identity: String,
    pub(crate) created_by: ActorIdentity,
    pub(crate) runtime_generation_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PromptAdmitted {
    pub(crate) submission_id: Uuid,
    pub(crate) prompt_id: Uuid,
    pub(crate) principal: String,
    pub(crate) ingress: String,
    pub(crate) queue_mode: QueueMode,
    pub(crate) content: PromptContent,
    pub(crate) metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PromptRejected {
    pub(crate) submission_id: Uuid,
    pub(crate) principal: String,
    pub(crate) ingress: String,
    pub(crate) reason_code: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PromptRemoved {
    pub(crate) prompt_id: Uuid,
    pub(crate) reason: PromptRemovalReason,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TurnStarted {
    pub(crate) turn_id: Uuid,
    pub(crate) prompt_id: Uuid,
    pub(crate) runtime_generation_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TurnInterruptionRequested {
    pub(crate) interruption_id: Uuid,
    pub(crate) turn_id: Uuid,
    pub(crate) kind: InterruptionKind,
    pub(crate) principal: String,
    pub(crate) ingress: String,
    pub(crate) reason_code: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InvocationRegistered {
    pub(crate) invocation_id: Uuid,
    pub(crate) turn_id: Uuid,
    pub(crate) call_id: String,
    pub(crate) owner_generation_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InvocationPrepared {
    pub(crate) invocation_id: Uuid,
    pub(crate) lease_id: Uuid,
    pub(crate) turn_id: Uuid,
    pub(crate) call_id: String,
    pub(crate) deduplication_id: Option<String>,
    pub(crate) invocation_kind: RuntimeInvocationKind,
    pub(crate) invocation_name: String,
    pub(crate) capability_id: RuntimeCapabilityId,
    pub(crate) contribution_id: RuntimeContributionId,
    pub(crate) owner_generation_id: RuntimeContributionGenerationId,
    pub(crate) issue_generation_id: RuntimeCompositionGenerationId,
    pub(crate) principal: String,
    pub(crate) principal_class: RuntimePrincipalClass,
    pub(crate) surface: RuntimeSurface,
    pub(crate) admitted_effects: Vec<RuntimeEffect>,
    pub(crate) execution: RuntimeExecutionPolicy,
    pub(crate) transition: RuntimeCapabilityTransitionPolicy,
    pub(crate) surfaces: Vec<RuntimeSurface>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InvocationDispatched {
    pub(crate) invocation_id: Uuid,
    pub(crate) lease_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InvocationAcknowledged {
    pub(crate) invocation_id: Uuid,
    pub(crate) lease_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InvocationClassifiedUnknown {
    pub(crate) invocation_id: Uuid,
    pub(crate) reason_code: String,
    pub(crate) recovery_rule_version: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InvocationSettled {
    pub(crate) invocation_id: Uuid,
    pub(crate) outcome: InvocationOutcome,
    pub(crate) terminal_evidence_reference: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TurnClosed {
    pub(crate) turn_id: Uuid,
    pub(crate) outcome: TurnOutcome,
    pub(crate) reason_code: String,
    pub(crate) recovery_rule_version: Option<u16>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SessionFactPayload {
    SessionCreated(SessionCreated),
    PromptAdmitted(PromptAdmitted),
    PromptRejected(PromptRejected),
    PromptRemoved(PromptRemoved),
    TurnStarted(TurnStarted),
    TurnInterruptionRequested(TurnInterruptionRequested),
    InvocationRegistered(InvocationRegistered),
    InvocationPrepared(InvocationPrepared),
    InvocationDispatched(InvocationDispatched),
    InvocationAcknowledged(InvocationAcknowledged),
    InvocationClassifiedUnknown(InvocationClassifiedUnknown),
    InvocationSettled(InvocationSettled),
    TurnClosed(TurnClosed),
}

impl SessionFactPayload {
    fn event_type(&self) -> &'static str {
        match self {
            Self::SessionCreated(_) => "session.created",
            Self::PromptAdmitted(_) => "prompt.admitted",
            Self::PromptRejected(_) => "prompt.rejected",
            Self::PromptRemoved(_) => "prompt.removed",
            Self::TurnStarted(_) => "turn.started",
            Self::TurnInterruptionRequested(_) => "turn.interruption_requested",
            Self::InvocationRegistered(_) => "invocation.registered",
            Self::InvocationPrepared(_) => "invocation.prepared",
            Self::InvocationDispatched(_) => "invocation.dispatched",
            Self::InvocationAcknowledged(_) => "invocation.acknowledged",
            Self::InvocationClassifiedUnknown(_) => "invocation.classified_unknown",
            Self::InvocationSettled(_) => "invocation.settled",
            Self::TurnClosed(_) => "turn.closed",
        }
    }

    fn to_value(&self) -> serde_json::Result<Value> {
        match self {
            Self::SessionCreated(value) => serde_json::to_value(value),
            Self::PromptAdmitted(value) => serde_json::to_value(value),
            Self::PromptRejected(value) => serde_json::to_value(value),
            Self::PromptRemoved(value) => serde_json::to_value(value),
            Self::TurnStarted(value) => serde_json::to_value(value),
            Self::TurnInterruptionRequested(value) => serde_json::to_value(value),
            Self::InvocationRegistered(value) => serde_json::to_value(value),
            Self::InvocationPrepared(value) => serde_json::to_value(value),
            Self::InvocationDispatched(value) => serde_json::to_value(value),
            Self::InvocationAcknowledged(value) => serde_json::to_value(value),
            Self::InvocationClassifiedUnknown(value) => serde_json::to_value(value),
            Self::InvocationSettled(value) => serde_json::to_value(value),
            Self::TurnClosed(value) => serde_json::to_value(value),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SessionFact {
    pub(crate) event_id: Uuid,
    pub(crate) session_id: String,
    pub(crate) stream_id: Uuid,
    pub(crate) sequence: u64,
    pub(crate) command_id: Uuid,
    pub(crate) command_fingerprint: String,
    pub(crate) causation_event_id: Option<Uuid>,
    pub(crate) recorded_at: String,
    pub(crate) payload: SessionFactPayload,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionFactWire {
    envelope_version: u16,
    event_id: Uuid,
    session_id: String,
    stream_id: Uuid,
    sequence: u64,
    event_type: String,
    event_version: u16,
    command_id: Uuid,
    command_fingerprint: String,
    causation_event_id: Option<Uuid>,
    recorded_at: String,
    payload: Value,
}

impl SessionFact {
    pub(crate) fn new(
        session_id: impl Into<String>,
        stream_id: Uuid,
        sequence: u64,
        command_id: Uuid,
        command_fingerprint: impl Into<String>,
        recorded_at: impl Into<String>,
        payload: SessionFactPayload,
    ) -> Self {
        Self {
            event_id: Uuid::new_v4(),
            session_id: session_id.into(),
            stream_id,
            sequence,
            command_id,
            command_fingerprint: command_fingerprint.into(),
            causation_event_id: None,
            recorded_at: recorded_at.into(),
            payload,
        }
    }

    fn encode(&self) -> Result<Vec<u8>> {
        self.validate_envelope()?;
        let wire = SessionFactWire {
            envelope_version: ENVELOPE_VERSION,
            event_id: self.event_id,
            session_id: self.session_id.clone(),
            stream_id: self.stream_id,
            sequence: self.sequence,
            event_type: self.payload.event_type().to_string(),
            event_version: EVENT_VERSION,
            command_id: self.command_id,
            command_fingerprint: self.command_fingerprint.clone(),
            causation_event_id: self.causation_event_id,
            recorded_at: self.recorded_at.clone(),
            payload: self.payload.to_value()?,
        };
        Ok(serde_json::to_vec(&wire)?)
    }

    fn decode(bytes: &[u8]) -> Result<Self> {
        let wire: SessionFactWire = serde_json::from_slice(bytes)?;
        if wire.envelope_version != ENVELOPE_VERSION {
            return Err(AuthorityError::Invalid(format!(
                "unsupported envelope version {}",
                wire.envelope_version
            )));
        }
        if wire.event_version != EVENT_VERSION {
            return Err(AuthorityError::Invalid(format!(
                "unsupported {} event version {}",
                wire.event_type, wire.event_version
            )));
        }
        let payload = match wire.event_type.as_str() {
            "session.created" => {
                decode_payload(wire.payload).map(SessionFactPayload::SessionCreated)
            }
            "prompt.admitted" => {
                decode_payload(wire.payload).map(SessionFactPayload::PromptAdmitted)
            }
            "prompt.rejected" => {
                decode_payload(wire.payload).map(SessionFactPayload::PromptRejected)
            }
            "prompt.removed" => decode_payload(wire.payload).map(SessionFactPayload::PromptRemoved),
            "turn.started" => decode_payload(wire.payload).map(SessionFactPayload::TurnStarted),
            "turn.interruption_requested" => {
                decode_payload(wire.payload).map(SessionFactPayload::TurnInterruptionRequested)
            }
            "invocation.registered" => {
                decode_payload(wire.payload).map(SessionFactPayload::InvocationRegistered)
            }
            "invocation.prepared" => {
                decode_payload(wire.payload).map(SessionFactPayload::InvocationPrepared)
            }
            "invocation.dispatched" => {
                decode_payload(wire.payload).map(SessionFactPayload::InvocationDispatched)
            }
            "invocation.acknowledged" => {
                decode_payload(wire.payload).map(SessionFactPayload::InvocationAcknowledged)
            }
            "invocation.classified_unknown" => {
                decode_payload(wire.payload).map(SessionFactPayload::InvocationClassifiedUnknown)
            }
            "invocation.settled" => {
                decode_payload(wire.payload).map(SessionFactPayload::InvocationSettled)
            }
            "turn.closed" => decode_payload(wire.payload).map(SessionFactPayload::TurnClosed),
            unknown => Err(AuthorityError::Invalid(format!(
                "unsupported authority event type {unknown}"
            ))),
        }?;
        let fact = Self {
            event_id: wire.event_id,
            session_id: wire.session_id,
            stream_id: wire.stream_id,
            sequence: wire.sequence,
            command_id: wire.command_id,
            command_fingerprint: wire.command_fingerprint,
            causation_event_id: wire.causation_event_id,
            recorded_at: wire.recorded_at,
            payload,
        };
        fact.validate_envelope()?;
        Ok(fact)
    }

    fn validate_envelope(&self) -> Result<()> {
        if self.session_id.is_empty() || self.session_id.len() > 256 {
            return Err(AuthorityError::Invalid("invalid session ID".into()));
        }
        if self.sequence == 0 {
            return Err(AuthorityError::Invalid("sequence zero is invalid".into()));
        }
        if !is_sha256_hex(&self.command_fingerprint) {
            return Err(AuthorityError::Invalid(
                "command fingerprint must be lowercase SHA-256 hex".into(),
            ));
        }
        let timestamp = DateTime::parse_from_rfc3339(&self.recorded_at)
            .map_err(|error| AuthorityError::Invalid(format!("invalid recorded_at: {error}")))?;
        if timestamp.offset().local_minus_utc() != 0 {
            return Err(AuthorityError::Invalid("recorded_at must be UTC".into()));
        }
        Ok(())
    }
}

fn decode_payload<T: DeserializeOwned>(value: Value) -> Result<T> {
    Ok(serde_json::from_value(value)?)
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ActiveTurn {
    pub(crate) turn_id: Uuid,
    pub(crate) prompt: PromptAdmitted,
    pub(crate) runtime_generation_id: String,
    pub(crate) accepted_interruption: Option<TurnInterruptionRequested>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum InvocationState {
    Registered {
        registration: InvocationRegistered,
    },
    Prepared {
        preparation: InvocationPrepared,
    },
    Dispatched {
        preparation: InvocationPrepared,
        dispatch: InvocationDispatched,
    },
    Acknowledged {
        preparation: InvocationPrepared,
        dispatch: InvocationDispatched,
        acknowledgement: InvocationAcknowledged,
    },
    Unknown {
        registration: InvocationRegistered,
        classification: InvocationClassifiedUnknown,
    },
    Settled {
        registration: InvocationRegistered,
        settlement: InvocationSettled,
    },
    DurableUnknown {
        preparation: InvocationPrepared,
        dispatch: InvocationDispatched,
        acknowledgement: Option<InvocationAcknowledged>,
        classification: InvocationClassifiedUnknown,
    },
    DurableSettled {
        preparation: InvocationPrepared,
        dispatch: InvocationDispatched,
        acknowledgement: InvocationAcknowledged,
        settlement: InvocationSettled,
    },
}

impl InvocationState {
    fn turn_id(&self) -> Uuid {
        match self {
            Self::Registered { registration }
            | Self::Unknown { registration, .. }
            | Self::Settled { registration, .. } => registration.turn_id,
            Self::Prepared { preparation }
            | Self::Dispatched { preparation, .. }
            | Self::Acknowledged { preparation, .. }
            | Self::DurableUnknown { preparation, .. }
            | Self::DurableSettled { preparation, .. } => preparation.turn_id,
        }
    }

    fn call_id(&self) -> &str {
        match self {
            Self::Registered { registration }
            | Self::Unknown { registration, .. }
            | Self::Settled { registration, .. } => &registration.call_id,
            Self::Prepared { preparation }
            | Self::Dispatched { preparation, .. }
            | Self::Acknowledged { preparation, .. }
            | Self::DurableUnknown { preparation, .. }
            | Self::DurableSettled { preparation, .. } => &preparation.call_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CommandReceipt {
    pub(crate) command_id: Uuid,
    pub(crate) fingerprint: String,
    pub(crate) event_id: Uuid,
    pub(crate) sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum SubmissionDisposition {
    Admitted { prompt_id: Uuid },
    Rejected { reason_code: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionAuthorityState {
    pub(crate) session_id: Option<String>,
    pub(crate) stream_id: Option<Uuid>,
    pub(crate) workspace_identity: Option<String>,
    pub(crate) runtime_generation_id: Option<String>,
    pub(crate) last_sequence: u64,
    pub(crate) last_event_id: Option<Uuid>,
    pub(crate) submissions: BTreeMap<Uuid, SubmissionDisposition>,
    pub(crate) prompt_ids: BTreeMap<Uuid, Uuid>,
    pub(crate) queued_prompts: Vec<PromptAdmitted>,
    pub(crate) turn_starts: BTreeMap<Uuid, TurnStarted>,
    pub(crate) interruption_requests: BTreeMap<Uuid, TurnInterruptionRequested>,
    pub(crate) active_turn: Option<ActiveTurn>,
    pub(crate) invocations: BTreeMap<Uuid, InvocationState>,
    pub(crate) closed_turns: BTreeMap<Uuid, TurnClosed>,
    pub(crate) command_receipts: BTreeMap<Uuid, CommandReceipt>,
}

impl SessionAuthorityState {
    pub(crate) fn apply(&mut self, fact: &SessionFact) -> Result<()> {
        let expected_sequence =
            self.last_sequence
                .checked_add(1)
                .ok_or_else(|| AuthorityError::Transition {
                    sequence: fact.sequence,
                    message: "sequence overflow".into(),
                })?;
        if fact.sequence != expected_sequence {
            return self.transition_error(
                fact.sequence,
                format!(
                    "expected sequence {expected_sequence}, got {}",
                    fact.sequence
                ),
            );
        }
        if self.command_receipts.contains_key(&fact.command_id) {
            return self
                .transition_error(fact.sequence, "duplicate command ID in authority stream");
        }
        if self
            .command_receipts
            .values()
            .any(|receipt| receipt.event_id == fact.event_id)
        {
            return self.transition_error(fact.sequence, "duplicate event ID in authority stream");
        }

        if self.last_sequence == 0 {
            let SessionFactPayload::SessionCreated(created) = &fact.payload else {
                return self
                    .transition_error(fact.sequence, "sequence one must create the session");
            };
            if fact.sequence != 1 {
                return self
                    .transition_error(fact.sequence, "session creation must be sequence one");
            }
            self.session_id = Some(fact.session_id.clone());
            self.stream_id = Some(fact.stream_id);
            self.workspace_identity = Some(created.workspace_identity.clone());
            self.runtime_generation_id = Some(created.runtime_generation_id.clone());
        } else {
            if self.session_id.as_deref() != Some(fact.session_id.as_str())
                || self.stream_id != Some(fact.stream_id)
            {
                return self.transition_error(fact.sequence, "session or stream identity changed");
            }
            self.apply_transition(fact)?;
        }

        self.last_sequence = fact.sequence;
        self.last_event_id = Some(fact.event_id);
        self.command_receipts.insert(
            fact.command_id,
            CommandReceipt {
                command_id: fact.command_id,
                fingerprint: fact.command_fingerprint.clone(),
                event_id: fact.event_id,
                sequence: fact.sequence,
            },
        );
        Ok(())
    }

    fn apply_transition(&mut self, fact: &SessionFact) -> Result<()> {
        match &fact.payload {
            SessionFactPayload::SessionCreated(_) => {
                self.transition_error(fact.sequence, "session is already created")
            }
            SessionFactPayload::PromptAdmitted(prompt) => {
                if self.prompt_ids.contains_key(&prompt.prompt_id)
                    || self.submissions.contains_key(&prompt.submission_id)
                {
                    return self
                        .transition_error(fact.sequence, "prompt identity is already present");
                }
                self.submissions.insert(
                    prompt.submission_id,
                    SubmissionDisposition::Admitted {
                        prompt_id: prompt.prompt_id,
                    },
                );
                self.prompt_ids
                    .insert(prompt.prompt_id, prompt.submission_id);
                self.queued_prompts.push(prompt.clone());
                Ok(())
            }
            SessionFactPayload::PromptRejected(rejected) => {
                if self.submissions.contains_key(&rejected.submission_id) {
                    return self.transition_error(
                        fact.sequence,
                        "submission identity already has an outcome",
                    );
                }
                self.submissions.insert(
                    rejected.submission_id,
                    SubmissionDisposition::Rejected {
                        reason_code: rejected.reason_code.clone(),
                    },
                );
                Ok(())
            }
            SessionFactPayload::PromptRemoved(removed) => {
                let Some(index) = self
                    .queued_prompts
                    .iter()
                    .position(|prompt| prompt.prompt_id == removed.prompt_id)
                else {
                    return self.transition_error(fact.sequence, "queued prompt was not found");
                };
                self.queued_prompts.remove(index);
                Ok(())
            }
            SessionFactPayload::TurnStarted(started) => {
                if self.active_turn.is_some() {
                    return self.transition_error(fact.sequence, "a turn is already active");
                }
                if self.turn_starts.contains_key(&started.turn_id) {
                    return self
                        .transition_error(fact.sequence, "turn identity is already present");
                }
                let Some(prompt) = self.queued_prompts.first() else {
                    return self.transition_error(fact.sequence, "prompt queue is empty");
                };
                if prompt.prompt_id != started.prompt_id {
                    return self
                        .transition_error(fact.sequence, "turn must start from FIFO queue head");
                }
                let prompt = self.queued_prompts.remove(0);
                self.turn_starts.insert(started.turn_id, started.clone());
                self.active_turn = Some(ActiveTurn {
                    turn_id: started.turn_id,
                    prompt,
                    runtime_generation_id: started.runtime_generation_id.clone(),
                    accepted_interruption: None,
                });
                Ok(())
            }
            SessionFactPayload::TurnInterruptionRequested(request) => {
                let Some(active) = self.active_turn.as_mut() else {
                    return self.transition_error(fact.sequence, "there is no active turn");
                };
                if active.turn_id != request.turn_id {
                    return self
                        .transition_error(fact.sequence, "interruption targets a stale turn");
                }
                if active.accepted_interruption.is_some() {
                    return self
                        .transition_error(fact.sequence, "an interruption is already accepted");
                }
                active.accepted_interruption = Some(request.clone());
                self.interruption_requests
                    .insert(request.turn_id, request.clone());
                Ok(())
            }
            SessionFactPayload::InvocationRegistered(registration) => {
                let Some(active) = self.active_turn.as_ref() else {
                    return self.transition_error(fact.sequence, "there is no active turn");
                };
                if active.turn_id != registration.turn_id {
                    return self.transition_error(fact.sequence, "invocation targets a stale turn");
                }
                if active
                    .accepted_interruption
                    .as_ref()
                    .is_some_and(|request| request.kind == InterruptionKind::Revoke)
                {
                    return self.transition_error(
                        fact.sequence,
                        "invocation cannot register after revocation",
                    );
                }
                if self.invocations.contains_key(&registration.invocation_id) {
                    return self
                        .transition_error(fact.sequence, "invocation identity is already present");
                }
                self.invocations.insert(
                    registration.invocation_id,
                    InvocationState::Registered {
                        registration: registration.clone(),
                    },
                );
                Ok(())
            }
            SessionFactPayload::InvocationPrepared(preparation) => {
                let Some(active) = self.active_turn.as_ref() else {
                    return self.transition_error(fact.sequence, "there is no active turn");
                };
                if active.turn_id != preparation.turn_id {
                    return self.transition_error(fact.sequence, "invocation targets a stale turn");
                }
                if active
                    .accepted_interruption
                    .as_ref()
                    .is_some_and(|request| request.kind == InterruptionKind::Revoke)
                {
                    return self.transition_error(
                        fact.sequence,
                        "invocation cannot prepare after revocation",
                    );
                }
                if self.invocations.contains_key(&preparation.invocation_id) {
                    return self
                        .transition_error(fact.sequence, "invocation identity is already present");
                }
                if self.invocations.values().any(|invocation| {
                    invocation.turn_id() == preparation.turn_id
                        && invocation.call_id() == preparation.call_id
                }) {
                    return self.transition_error(
                        fact.sequence,
                        "invocation call identity is already present in this turn",
                    );
                }
                self.invocations.insert(
                    preparation.invocation_id,
                    InvocationState::Prepared {
                        preparation: preparation.clone(),
                    },
                );
                Ok(())
            }
            SessionFactPayload::InvocationDispatched(dispatch) => {
                let Some(current) = self.invocations.get(&dispatch.invocation_id).cloned() else {
                    return self.transition_error(fact.sequence, "invocation was not prepared");
                };
                let InvocationState::Prepared { preparation } = current else {
                    return self
                        .transition_error(fact.sequence, "invocation is not awaiting dispatch");
                };
                if preparation.lease_id != dispatch.lease_id {
                    return self.transition_error(
                        fact.sequence,
                        "dispatch lease does not match prepared invocation",
                    );
                }
                self.invocations.insert(
                    dispatch.invocation_id,
                    InvocationState::Dispatched {
                        preparation,
                        dispatch: dispatch.clone(),
                    },
                );
                Ok(())
            }
            SessionFactPayload::InvocationAcknowledged(acknowledgement) => {
                let Some(current) = self
                    .invocations
                    .get(&acknowledgement.invocation_id)
                    .cloned()
                else {
                    return self.transition_error(fact.sequence, "invocation was not dispatched");
                };
                let InvocationState::Dispatched {
                    preparation,
                    dispatch,
                } = current
                else {
                    return self.transition_error(
                        fact.sequence,
                        "invocation is not awaiting acknowledgement",
                    );
                };
                if preparation.lease_id != acknowledgement.lease_id {
                    return self.transition_error(
                        fact.sequence,
                        "acknowledgement lease does not match prepared invocation",
                    );
                }
                self.invocations.insert(
                    acknowledgement.invocation_id,
                    InvocationState::Acknowledged {
                        preparation,
                        dispatch,
                        acknowledgement: acknowledgement.clone(),
                    },
                );
                Ok(())
            }
            SessionFactPayload::InvocationClassifiedUnknown(classification) => {
                let Some(current) = self.invocations.get(&classification.invocation_id).cloned()
                else {
                    return self.transition_error(fact.sequence, "invocation was not registered");
                };
                let next = match current {
                    InvocationState::Registered { registration } => InvocationState::Unknown {
                        registration,
                        classification: classification.clone(),
                    },
                    InvocationState::Dispatched {
                        preparation,
                        dispatch,
                    } => InvocationState::DurableUnknown {
                        preparation,
                        dispatch,
                        acknowledgement: None,
                        classification: classification.clone(),
                    },
                    InvocationState::Acknowledged {
                        preparation,
                        dispatch,
                        acknowledgement,
                    } => InvocationState::DurableUnknown {
                        preparation,
                        dispatch,
                        acknowledgement: Some(acknowledgement),
                        classification: classification.clone(),
                    },
                    _ => {
                        return self.transition_error(
                            fact.sequence,
                            "invocation is already classified or settled",
                        );
                    }
                };
                self.invocations.insert(classification.invocation_id, next);
                Ok(())
            }
            SessionFactPayload::InvocationSettled(settlement) => {
                let Some(current) = self.invocations.get(&settlement.invocation_id).cloned() else {
                    return self.transition_error(fact.sequence, "invocation was not registered");
                };
                let next = match current {
                    InvocationState::Registered { registration }
                    | InvocationState::Unknown { registration, .. } => InvocationState::Settled {
                        registration,
                        settlement: settlement.clone(),
                    },
                    InvocationState::Acknowledged {
                        preparation,
                        dispatch,
                        acknowledgement,
                    } => InvocationState::DurableSettled {
                        preparation,
                        dispatch,
                        acknowledgement,
                        settlement: settlement.clone(),
                    },
                    InvocationState::DurableUnknown {
                        preparation,
                        dispatch,
                        acknowledgement: Some(acknowledgement),
                        ..
                    } if settlement.terminal_evidence_reference.is_some() => {
                        InvocationState::DurableSettled {
                            preparation,
                            dispatch,
                            acknowledgement,
                            settlement: settlement.clone(),
                        }
                    }
                    InvocationState::Prepared { .. }
                    | InvocationState::Dispatched { .. }
                    | InvocationState::DurableUnknown { .. }
                    | InvocationState::Settled { .. }
                    | InvocationState::DurableSettled { .. } => {
                        return self.transition_error(
                            fact.sequence,
                            "invocation cannot settle from its current state",
                        );
                    }
                };
                self.invocations.insert(settlement.invocation_id, next);
                Ok(())
            }
            SessionFactPayload::TurnClosed(closed) => {
                let Some(active) = self.active_turn.as_ref() else {
                    return self.transition_error(fact.sequence, "there is no active turn");
                };
                if active.turn_id != closed.turn_id {
                    return self.transition_error(fact.sequence, "closure targets a stale turn");
                }
                if self.invocations.values().any(|invocation| {
                    invocation.turn_id() == closed.turn_id
                        && matches!(invocation, InvocationState::Registered { .. })
                }) {
                    return self.transition_error(
                        fact.sequence,
                        "turn cannot close with unclassified invocations",
                    );
                }
                self.active_turn = None;
                self.closed_turns.insert(closed.turn_id, closed.clone());
                Ok(())
            }
        }
    }

    fn transition_error<T>(&self, sequence: u64, message: impl Into<String>) -> Result<T> {
        Err(AuthorityError::Transition {
            sequence,
            message: message.into(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionAuthoritySnapshot {
    snapshot_version: u16,
    reducer_version: u16,
    session_id: String,
    stream_id: Uuid,
    last_sequence: u64,
    last_event_id: Uuid,
    state: SessionAuthorityState,
}

impl SessionAuthoritySnapshot {
    fn from_state(state: &SessionAuthorityState) -> Result<Self> {
        Ok(Self {
            snapshot_version: SNAPSHOT_VERSION,
            reducer_version: REDUCER_VERSION,
            session_id: state
                .session_id
                .clone()
                .ok_or_else(|| AuthorityError::Invalid("snapshot session is absent".into()))?,
            stream_id: state
                .stream_id
                .ok_or_else(|| AuthorityError::Invalid("snapshot stream is absent".into()))?,
            last_sequence: state.last_sequence,
            last_event_id: state
                .last_event_id
                .ok_or_else(|| AuthorityError::Invalid("snapshot cursor is absent".into()))?,
            state: state.clone(),
        })
    }

    fn validate(&self) -> Result<()> {
        if self.snapshot_version != SNAPSHOT_VERSION || self.reducer_version != REDUCER_VERSION {
            return Err(AuthorityError::Invalid(
                "unsupported snapshot or reducer version".into(),
            ));
        }
        if self.state.session_id.as_deref() != Some(self.session_id.as_str())
            || self.state.stream_id != Some(self.stream_id)
            || self.state.last_sequence != self.last_sequence
            || self.state.last_event_id != Some(self.last_event_id)
        {
            return Err(AuthorityError::Invalid(
                "snapshot envelope does not match state".into(),
            ));
        }
        Ok(())
    }
}

pub(crate) fn reconstruct(facts: &[SessionFact]) -> Result<SessionAuthorityState> {
    let mut state = SessionAuthorityState::default();
    for fact in facts {
        state.apply(fact)?;
    }
    Ok(state)
}

#[derive(Debug)]
pub(crate) struct SessionAuthority {
    store: SessionAuthorityStore,
    _writer_lease: crate::filelock::FileLockGuard,
    state: SessionAuthorityState,
    session_id: String,
    stream_id: Uuid,
    runtime_generation_id: String,
}

#[derive(Clone)]
pub(crate) struct SessionAuthorityHandle(Arc<Mutex<SessionAuthority>>);

impl std::fmt::Debug for SessionAuthorityHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SessionAuthorityHandle")
            .field("session_id", &self.session_id())
            .finish_non_exhaustive()
    }
}

impl PartialEq for SessionAuthorityHandle {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for SessionAuthorityHandle {}

impl SessionAuthorityHandle {
    pub(crate) fn new(authority: SessionAuthority) -> Self {
        Self(Arc::new(Mutex::new(authority)))
    }

    fn lock(&self) -> MutexGuard<'_, SessionAuthority> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(crate) fn state(&self) -> SessionAuthorityState {
        self.lock().state().clone()
    }

    pub(crate) fn session_id(&self) -> String {
        self.lock().session_id.clone()
    }

    pub(crate) fn stage_attachment(&self, source: &Path) -> Result<AttachmentRef> {
        self.lock().stage_attachment(source)
    }

    pub(crate) fn validate_attachment(&self, attachment: &AttachmentRef) -> Result<PathBuf> {
        self.lock().validate_attachment(attachment)
    }

    pub(crate) fn admit_prompt(
        &self,
        command_id: Uuid,
        recorded_at: &str,
        admission: PromptAdmitted,
    ) -> Result<bool> {
        self.lock().admit_prompt(command_id, recorded_at, admission)
    }

    pub(crate) fn remove_prompt(
        &self,
        command_id: Uuid,
        recorded_at: &str,
        prompt_id: Uuid,
        reason: PromptRemovalReason,
    ) -> Result<bool> {
        self.lock()
            .remove_prompt(command_id, recorded_at, prompt_id, reason)
    }

    pub(crate) fn start_turn(
        &self,
        command_id: Uuid,
        recorded_at: &str,
        turn_id: Uuid,
        prompt_id: Uuid,
    ) -> Result<bool> {
        self.lock()
            .start_turn(command_id, recorded_at, turn_id, prompt_id)
    }

    pub(crate) fn request_interruption(
        &self,
        command_id: Uuid,
        recorded_at: &str,
        request: TurnInterruptionRequested,
    ) -> Result<bool> {
        self.lock()
            .request_interruption(command_id, recorded_at, request)
    }

    pub(crate) fn close_turn(
        &self,
        command_id: Uuid,
        recorded_at: &str,
        closure: TurnClosed,
    ) -> Result<bool> {
        self.lock().close_turn(command_id, recorded_at, closure)
    }

    pub(crate) fn prepare_invocation(
        &self,
        recorded_at: &str,
        preparation: InvocationPrepared,
    ) -> Result<bool> {
        self.lock().prepare_invocation(recorded_at, preparation)
    }

    pub(crate) fn mark_invocation_dispatched(
        &self,
        recorded_at: &str,
        dispatch: InvocationDispatched,
    ) -> Result<bool> {
        self.lock()
            .mark_invocation_dispatched(recorded_at, dispatch)
    }

    pub(crate) fn acknowledge_invocation(
        &self,
        recorded_at: &str,
        acknowledgement: InvocationAcknowledged,
    ) -> Result<bool> {
        self.lock()
            .acknowledge_invocation(recorded_at, acknowledgement)
    }

    pub(crate) fn settle_invocation(
        &self,
        recorded_at: &str,
        settlement: InvocationSettled,
    ) -> Result<bool> {
        self.lock().settle_invocation(recorded_at, settlement)
    }

    pub(crate) fn classify_invocation_unknown(
        &self,
        recorded_at: &str,
        classification: InvocationClassifiedUnknown,
    ) -> Result<bool> {
        self.lock()
            .classify_invocation_unknown(recorded_at, classification)
    }
}

impl SessionAuthority {
    pub(crate) fn open(
        session_snapshot: &Path,
        session_id: impl Into<String>,
        workspace_identity: impl Into<String>,
        runtime_generation_id: impl Into<String>,
        created_by: ActorIdentity,
        recorded_at: &str,
    ) -> Result<Self> {
        let session_id = session_id.into();
        let workspace_identity = workspace_identity.into();
        let runtime_generation_id = runtime_generation_id.into();
        let store = SessionAuthorityStore::adjacent_to(session_snapshot)?;
        let writer_lease = crate::filelock::try_acquire_lock(&store.writer_lease_path())
            .map_err(|error| AuthorityError::Invalid(error.to_string()))?
            .ok_or_else(|| {
                AuthorityError::Invalid("session authority already has an active writer".into())
            })?;
        let initial = store.load()?;

        if initial.last_sequence > 0 && initial.session_id.as_deref() != Some(session_id.as_str()) {
            return Err(AuthorityError::Invalid(
                "authority stream belongs to a different session".into(),
            ));
        }
        if initial.last_sequence > 0
            && initial.workspace_identity.as_deref() != Some(workspace_identity.as_str())
        {
            return Err(AuthorityError::Invalid(
                "authority stream belongs to a different workspace".into(),
            ));
        }

        let mut state = store.recover(recorded_at)?;

        if state.last_sequence == 0 {
            let stream_id = Uuid::new_v4();
            let payload = SessionFactPayload::SessionCreated(SessionCreated {
                workspace_identity: workspace_identity.clone(),
                created_by,
                runtime_generation_id: runtime_generation_id.clone(),
            });
            append_payload(
                &store,
                &mut state,
                &session_id,
                stream_id,
                Uuid::new_v4(),
                recorded_at,
                payload,
            )?;
        }

        if state.session_id.as_deref() != Some(session_id.as_str()) {
            return Err(AuthorityError::Invalid(
                "authority stream belongs to a different session".into(),
            ));
        }
        if state.workspace_identity.as_deref() != Some(workspace_identity.as_str()) {
            return Err(AuthorityError::Invalid(
                "authority stream belongs to a different workspace".into(),
            ));
        }
        let stream_id = state
            .stream_id
            .ok_or_else(|| AuthorityError::Invalid("authority stream has no identity".into()))?;
        let runtime_generation_id = state.runtime_generation_id.clone().ok_or_else(|| {
            AuthorityError::Invalid("authority stream has no runtime generation".into())
        })?;

        Ok(Self {
            store,
            _writer_lease: writer_lease,
            state,
            session_id,
            stream_id,
            runtime_generation_id,
        })
    }

    pub(crate) fn state(&self) -> &SessionAuthorityState {
        &self.state
    }

    pub(crate) fn stage_attachment(&self, source: &Path) -> Result<AttachmentRef> {
        self.store.stage_attachment(source)
    }

    pub(crate) fn validate_attachment(&self, attachment: &AttachmentRef) -> Result<PathBuf> {
        self.store.validate_attachment(attachment)
    }

    pub(crate) fn admit_prompt(
        &mut self,
        command_id: Uuid,
        recorded_at: &str,
        admission: PromptAdmitted,
    ) -> Result<bool> {
        self.append(
            command_id,
            recorded_at,
            SessionFactPayload::PromptAdmitted(admission),
        )
    }

    pub(crate) fn remove_prompt(
        &mut self,
        command_id: Uuid,
        recorded_at: &str,
        prompt_id: Uuid,
        reason: PromptRemovalReason,
    ) -> Result<bool> {
        self.append(
            command_id,
            recorded_at,
            SessionFactPayload::PromptRemoved(PromptRemoved { prompt_id, reason }),
        )
    }

    pub(crate) fn start_turn(
        &mut self,
        command_id: Uuid,
        recorded_at: &str,
        turn_id: Uuid,
        prompt_id: Uuid,
    ) -> Result<bool> {
        let runtime_generation_id = self.runtime_generation_id.clone();
        self.append(
            command_id,
            recorded_at,
            SessionFactPayload::TurnStarted(TurnStarted {
                turn_id,
                prompt_id,
                runtime_generation_id,
            }),
        )
    }

    pub(crate) fn request_interruption(
        &mut self,
        command_id: Uuid,
        recorded_at: &str,
        request: TurnInterruptionRequested,
    ) -> Result<bool> {
        self.append(
            command_id,
            recorded_at,
            SessionFactPayload::TurnInterruptionRequested(request),
        )
    }

    pub(crate) fn close_turn(
        &mut self,
        command_id: Uuid,
        recorded_at: &str,
        closure: TurnClosed,
    ) -> Result<bool> {
        if self.state.invocations.values().any(|invocation| {
            invocation.turn_id() == closure.turn_id
                && matches!(
                    invocation,
                    InvocationState::Dispatched { .. } | InvocationState::Acknowledged { .. }
                )
        }) {
            return Err(AuthorityError::Invalid(
                "turn cannot close with an unresolved dispatched invocation".into(),
            ));
        }
        self.append(
            command_id,
            recorded_at,
            SessionFactPayload::TurnClosed(closure),
        )
    }

    pub(crate) fn prepare_invocation(
        &mut self,
        recorded_at: &str,
        preparation: InvocationPrepared,
    ) -> Result<bool> {
        self.append(
            invocation_phase_command_id(preparation.invocation_id, "prepared"),
            recorded_at,
            SessionFactPayload::InvocationPrepared(preparation),
        )
    }

    pub(crate) fn mark_invocation_dispatched(
        &mut self,
        recorded_at: &str,
        dispatch: InvocationDispatched,
    ) -> Result<bool> {
        self.append(
            invocation_phase_command_id(dispatch.invocation_id, "dispatched"),
            recorded_at,
            SessionFactPayload::InvocationDispatched(dispatch),
        )
    }

    pub(crate) fn acknowledge_invocation(
        &mut self,
        recorded_at: &str,
        acknowledgement: InvocationAcknowledged,
    ) -> Result<bool> {
        self.append(
            invocation_phase_command_id(acknowledgement.invocation_id, "acknowledged"),
            recorded_at,
            SessionFactPayload::InvocationAcknowledged(acknowledgement),
        )
    }

    pub(crate) fn settle_invocation(
        &mut self,
        recorded_at: &str,
        settlement: InvocationSettled,
    ) -> Result<bool> {
        self.append(
            invocation_phase_command_id(settlement.invocation_id, "settled"),
            recorded_at,
            SessionFactPayload::InvocationSettled(settlement),
        )
    }

    pub(crate) fn classify_invocation_unknown(
        &mut self,
        recorded_at: &str,
        classification: InvocationClassifiedUnknown,
    ) -> Result<bool> {
        self.append(
            invocation_phase_command_id(classification.invocation_id, "unknown"),
            recorded_at,
            SessionFactPayload::InvocationClassifiedUnknown(classification),
        )
    }

    fn append(
        &mut self,
        command_id: Uuid,
        recorded_at: &str,
        payload: SessionFactPayload,
    ) -> Result<bool> {
        append_payload(
            &self.store,
            &mut self.state,
            &self.session_id,
            self.stream_id,
            command_id,
            recorded_at,
            payload,
        )
    }
}

pub(crate) fn invocation_phase_command_id(invocation_id: Uuid, phase: &str) -> Uuid {
    Uuid::new_v5(
        &INVOCATION_COMMAND_NAMESPACE,
        format!("{invocation_id}:{phase}:1").as_bytes(),
    )
}

fn append_payload(
    store: &SessionAuthorityStore,
    state: &mut SessionAuthorityState,
    session_id: &str,
    stream_id: Uuid,
    command_id: Uuid,
    recorded_at: &str,
    payload: SessionFactPayload,
) -> Result<bool> {
    let sequence = state
        .last_sequence
        .checked_add(1)
        .ok_or_else(|| AuthorityError::Invalid("authority sequence overflow".into()))?;
    let fingerprint = command_fingerprint(&payload)?;
    let mut fact = SessionFact::new(
        session_id,
        stream_id,
        sequence,
        command_id,
        fingerprint,
        recorded_at,
        payload,
    );
    fact.causation_event_id = state.last_event_id;
    store.append(state, &fact)
}

fn command_fingerprint(payload: &SessionFactPayload) -> Result<String> {
    let mut hasher = Sha256::new();
    hasher.update(b"omegon-session-command-v1\0");
    hasher.update(payload.event_type().as_bytes());
    hasher.update(b"\0");
    hasher.update(serde_json::to_vec(&payload.to_value()?)?);
    Ok(format!("{:x}", hasher.finalize()))
}

#[derive(Debug, Clone)]
pub(crate) struct SessionAuthorityStore {
    log_path: PathBuf,
    snapshot_path: PathBuf,
    attachment_dir: PathBuf,
}

impl SessionAuthorityStore {
    pub(crate) fn adjacent_to(session_snapshot: &Path) -> Result<Self> {
        let stem = session_snapshot
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or_else(|| AuthorityError::Invalid("session snapshot has no UTF-8 stem".into()))?;
        let parent = session_snapshot
            .parent()
            .ok_or_else(|| AuthorityError::Invalid("session snapshot has no parent".into()))?;
        Ok(Self {
            log_path: parent.join(format!("{stem}.authority.jsonl")),
            snapshot_path: parent.join(format!("{stem}.authority.snapshot.json")),
            attachment_dir: parent.join(format!("{stem}.authority.attachments")),
        })
    }

    fn writer_lease_path(&self) -> PathBuf {
        let mut path = self.log_path.as_os_str().to_os_string();
        path.push(".writer");
        PathBuf::from(path)
    }

    #[cfg(test)]
    fn from_paths(log_path: PathBuf, snapshot_path: PathBuf) -> Self {
        Self {
            attachment_dir: log_path.with_extension("attachments"),
            log_path,
            snapshot_path,
        }
    }

    pub(crate) fn load(&self) -> Result<SessionAuthorityState> {
        let facts = read_facts(&self.log_path)?;
        if facts.is_empty() {
            return Ok(SessionAuthorityState::default());
        }

        if let Ok(snapshot) = self.read_snapshot()
            && let Some(prefix_end) = facts.iter().position(|fact| {
                fact.sequence == snapshot.last_sequence && fact.event_id == snapshot.last_event_id
            })
            && let Ok(prefix_state) = reconstruct(&facts[..=prefix_end])
            && prefix_state == snapshot.state
        {
            let mut state = snapshot.state;
            for fact in &facts[prefix_end + 1..] {
                state.apply(fact)?;
            }
            return Ok(state);
        }

        reconstruct(&facts)
    }

    pub(crate) fn append(
        &self,
        state: &mut SessionAuthorityState,
        fact: &SessionFact,
    ) -> Result<bool> {
        let _guard = crate::filelock::acquire_lock(&self.log_path)
            .map_err(|error| AuthorityError::Invalid(error.to_string()))?;
        let durable = reconstruct(&read_facts(&self.log_path)?)?;
        if let Some(receipt) = durable.command_receipts.get(&fact.command_id) {
            if receipt.fingerprint == fact.command_fingerprint {
                *state = durable;
                return Ok(false);
            }
            return Err(AuthorityError::Invalid(
                "command ID was reused with a conflicting event or fingerprint".into(),
            ));
        }
        if state.last_sequence != durable.last_sequence
            || state.last_event_id != durable.last_event_id
        {
            return Err(AuthorityError::Invalid(
                "in-memory authority cursor is stale".into(),
            ));
        }

        let mut next = durable;
        next.apply(fact)?;
        let mut encoded = fact.encode()?;
        if encoded.len() > MAX_RECORD_BYTES {
            return Err(AuthorityError::Invalid(
                "authority record exceeds 1 MiB".into(),
            ));
        }
        encoded.push(b'\n');

        self.append_record(&encoded)?;
        *state = next;
        if let Err(error) = SessionAuthoritySnapshot::from_state(state)
            .and_then(|snapshot| write_snapshot(&self.snapshot_path, &snapshot))
        {
            tracing::warn!(%error, "session authority snapshot cache update failed after durable append");
        }
        Ok(true)
    }

    pub(crate) fn recover(&self, recorded_at: &str) -> Result<SessionAuthorityState> {
        let _guard = crate::filelock::acquire_lock(&self.log_path)
            .map_err(|error| AuthorityError::Invalid(error.to_string()))?;
        let mut state = reconstruct(&read_facts(&self.log_path)?)?;
        for fact in recovery_facts(&state, recorded_at)? {
            let mut next = state.clone();
            next.apply(&fact)?;
            let mut encoded = fact.encode()?;
            if encoded.len() > MAX_RECORD_BYTES {
                return Err(AuthorityError::Invalid(
                    "authority record exceeds 1 MiB".into(),
                ));
            }
            encoded.push(b'\n');
            self.append_record(&encoded)?;
            state = next;
        }
        if state.last_sequence > 0
            && let Err(error) = SessionAuthoritySnapshot::from_state(&state)
                .and_then(|snapshot| write_snapshot(&self.snapshot_path, &snapshot))
        {
            tracing::warn!(%error, "session authority recovery snapshot cache update failed");
        }
        Ok(state)
    }

    fn stage_attachment(&self, source: &Path) -> Result<AttachmentRef> {
        let mut source_file = File::open(source)?;
        let metadata = source_file.metadata()?;
        if !metadata.is_file() {
            return Err(AuthorityError::Invalid(
                "prompt attachment must be a regular file".into(),
            ));
        }
        if metadata.len() > MAX_ATTACHMENT_BYTES {
            return Err(AuthorityError::Invalid(format!(
                "prompt attachment exceeds {} MiB",
                MAX_ATTACHMENT_BYTES / (1024 * 1024)
            )));
        }

        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        std::io::Read::read_to_end(&mut source_file, &mut bytes)?;
        let digest = format!("{:x}", Sha256::digest(&bytes));
        fs::create_dir_all(&self.attachment_dir)?;
        let stored = self.attachment_dir.join(&digest);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&stored)
        {
            Ok(mut file) => {
                file.write_all(&bytes)?;
                file.flush()?;
                file.sync_all()?;
                sync_parent(&stored)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if fs::read(&stored)? != bytes {
                    return Err(AuthorityError::Invalid(
                        "stored attachment digest collision".into(),
                    ));
                }
            }
            Err(error) => return Err(error.into()),
        }

        Ok(AttachmentRef {
            digest,
            media_type: attachment_media_type(source).to_string(),
            byte_length: metadata.len(),
            storage_ref: stored.to_string_lossy().into_owned(),
        })
    }

    fn validate_attachment(&self, attachment: &AttachmentRef) -> Result<PathBuf> {
        let path = PathBuf::from(&attachment.storage_ref);
        if path.parent() != Some(self.attachment_dir.as_path())
            || path.file_name().and_then(|name| name.to_str()) != Some(attachment.digest.as_str())
        {
            return Err(AuthorityError::Invalid(
                "authority attachment is outside content-addressed storage".into(),
            ));
        }
        let metadata = fs::metadata(&path)?;
        if !metadata.is_file() || metadata.len() != attachment.byte_length {
            return Err(AuthorityError::Invalid(
                "authority attachment size or type changed".into(),
            ));
        }
        let bytes = fs::read(&path)?;
        let digest = format!("{:x}", Sha256::digest(&bytes));
        if digest != attachment.digest {
            return Err(AuthorityError::Invalid(
                "authority attachment digest changed".into(),
            ));
        }
        Ok(path)
    }

    fn append_record(&self, encoded: &[u8]) -> Result<()> {
        if let Some(parent) = self.log_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let created = !self.log_path.exists();
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)?;
        file.write_all(encoded)?;
        file.flush()?;
        file.sync_all()?;
        if created {
            sync_parent(&self.log_path)?;
        }
        Ok(())
    }

    fn read_snapshot(&self) -> Result<SessionAuthoritySnapshot> {
        let bytes = fs::read(&self.snapshot_path)?;
        let snapshot: SessionAuthoritySnapshot = serde_json::from_slice(&bytes)?;
        snapshot.validate()?;
        Ok(snapshot)
    }
}

fn attachment_media_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    }
}

fn read_facts(path: &Path) -> Result<Vec<SessionFact>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    if !bytes.ends_with(b"\n") {
        return Err(AuthorityError::Invalid(
            "authority stream has a truncated final record".into(),
        ));
    }
    let mut facts = Vec::new();
    for (index, line) in bytes[..bytes.len() - 1]
        .split(|byte| *byte == b'\n')
        .enumerate()
    {
        if line.is_empty() {
            return Err(AuthorityError::Invalid(format!(
                "authority stream contains blank line {}",
                index + 1
            )));
        }
        if line.len() > MAX_RECORD_BYTES {
            return Err(AuthorityError::Invalid(format!(
                "authority record {} exceeds 1 MiB",
                index + 1
            )));
        }
        facts.push(SessionFact::decode(line)?);
    }
    Ok(facts)
}

fn write_snapshot(path: &Path, snapshot: &SessionAuthoritySnapshot) -> Result<()> {
    let bytes = serde_json::to_vec(snapshot)?;
    let parent = path
        .parent()
        .ok_or_else(|| AuthorityError::Invalid("snapshot path has no parent".into()))?;
    fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("authority"),
        std::process::id()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&tmp)?;
    file.write_all(&bytes)?;
    file.flush()?;
    file.sync_all()?;
    fs::rename(&tmp, path)?;
    sync_parent(path)
}

fn sync_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        File::open(parent)?.sync_all()?;
    }
    Ok(())
}

pub(crate) fn recovery_facts(
    state: &SessionAuthorityState,
    recorded_at: &str,
) -> Result<Vec<SessionFact>> {
    let has_unsettled_durable = state.invocations.values().any(|invocation| {
        matches!(
            invocation,
            InvocationState::Dispatched { .. } | InvocationState::Acknowledged { .. }
        )
    });
    if state.active_turn.is_none() && !has_unsettled_durable {
        return Ok(Vec::new());
    }
    let session_id = state
        .session_id
        .as_ref()
        .ok_or_else(|| AuthorityError::Invalid("recoverable invocation has no session".into()))?;
    let stream_id = state
        .stream_id
        .ok_or_else(|| AuthorityError::Invalid("recoverable invocation has no stream".into()))?;
    let mut sequence = state.last_sequence;
    let mut facts = Vec::new();
    let mut classified_durable = false;
    for invocation in state.invocations.values() {
        let (invocation_id, recovery_rule_version) = match invocation {
            InvocationState::Dispatched { preparation, .. }
            | InvocationState::Acknowledged { preparation, .. } => {
                classified_durable = true;
                (preparation.invocation_id, 2)
            }
            InvocationState::Registered { registration }
                if state
                    .active_turn
                    .as_ref()
                    .is_some_and(|active| active.turn_id == registration.turn_id) =>
            {
                (registration.invocation_id, 1)
            }
            _ => continue,
        };
        sequence += 1;
        facts.push(recovery_fact(
            session_id,
            stream_id,
            sequence,
            recorded_at,
            "invocation.classified_unknown",
            invocation_id,
            SessionFactPayload::InvocationClassifiedUnknown(InvocationClassifiedUnknown {
                invocation_id,
                reason_code: "runtime_lost".into(),
                recovery_rule_version,
            }),
        )?);
    }
    if let Some(active) = state.active_turn.as_ref() {
        sequence += 1;
        facts.push(recovery_fact(
            session_id,
            stream_id,
            sequence,
            recorded_at,
            "turn.closed",
            active.turn_id,
            SessionFactPayload::TurnClosed(TurnClosed {
                turn_id: active.turn_id,
                outcome: TurnOutcome::Interrupted,
                reason_code: "runtime_lost".into(),
                recovery_rule_version: Some(if classified_durable { 2 } else { 1 }),
            }),
        )?);
    }
    Ok(facts)
}

fn recovery_fact(
    session_id: &str,
    stream_id: Uuid,
    sequence: u64,
    recorded_at: &str,
    kind: &str,
    subject_id: Uuid,
    payload: SessionFactPayload,
) -> Result<SessionFact> {
    let identity = format!("{stream_id}:{subject_id}:{kind}:1");
    let event_id = Uuid::new_v5(&RECOVERY_NAMESPACE, identity.as_bytes());
    let command_id = Uuid::new_v5(
        &RECOVERY_NAMESPACE,
        format!("command:{identity}").as_bytes(),
    );
    let mut hasher = Sha256::new();
    hasher.update(b"omegon-session-recovery-v1\0");
    hasher.update(identity.as_bytes());
    let fingerprint = format!("{:x}", hasher.finalize());
    let mut fact = SessionFact::new(
        session_id,
        stream_id,
        sequence,
        command_id,
        fingerprint,
        recorded_at,
        payload,
    );
    fact.event_id = event_id;
    fact.validate_envelope()?;
    Ok(fact)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: &str = "2026-08-19T18:00:00Z";

    fn fingerprint(label: &str) -> String {
        format!("{:x}", Sha256::digest(label.as_bytes()))
    }

    fn fact(
        session_id: &str,
        stream_id: Uuid,
        sequence: u64,
        payload: SessionFactPayload,
    ) -> SessionFact {
        SessionFact::new(
            session_id,
            stream_id,
            sequence,
            Uuid::new_v4(),
            fingerprint(&format!("command-{sequence}")),
            NOW,
            payload,
        )
    }

    fn created(session_id: &str, stream_id: Uuid) -> SessionFact {
        fact(
            session_id,
            stream_id,
            1,
            SessionFactPayload::SessionCreated(SessionCreated {
                workspace_identity: "workspace-key".into(),
                created_by: ActorIdentity {
                    principal: "operator".into(),
                    ingress: "tui".into(),
                },
                runtime_generation_id: "generation-1".into(),
            }),
        )
    }

    fn admitted(
        session_id: &str,
        stream_id: Uuid,
        sequence: u64,
        prompt_id: Uuid,
        text: &str,
    ) -> SessionFact {
        fact(
            session_id,
            stream_id,
            sequence,
            SessionFactPayload::PromptAdmitted(PromptAdmitted {
                submission_id: Uuid::new_v4(),
                prompt_id,
                principal: "operator".into(),
                ingress: "tui".into(),
                queue_mode: QueueMode::UntilReady,
                content: PromptContent {
                    text: text.into(),
                    attachments: Vec::new(),
                },
                metadata: serde_json::json!({}),
            }),
        )
    }

    fn started(
        session_id: &str,
        stream_id: Uuid,
        sequence: u64,
        prompt_id: Uuid,
        turn_id: Uuid,
    ) -> SessionFact {
        fact(
            session_id,
            stream_id,
            sequence,
            SessionFactPayload::TurnStarted(TurnStarted {
                turn_id,
                prompt_id,
                runtime_generation_id: "generation-1".into(),
            }),
        )
    }

    fn prepared(turn_id: Uuid, call_id: &str) -> InvocationPrepared {
        let effects = vec![RuntimeEffect::FilesystemRead];
        InvocationPrepared {
            invocation_id: Uuid::new_v4(),
            lease_id: Uuid::new_v4(),
            turn_id,
            call_id: call_id.into(),
            deduplication_id: Some(call_id.into()),
            invocation_kind: RuntimeInvocationKind::Tool,
            invocation_name: "read".into(),
            capability_id: RuntimeCapabilityId::new("tool:read").unwrap(),
            contribution_id: RuntimeContributionId::new("feature:reader").unwrap(),
            owner_generation_id: RuntimeContributionGenerationId::new("contribution:reader-v1")
                .unwrap(),
            issue_generation_id: RuntimeCompositionGenerationId::new("composition:test").unwrap(),
            principal: "model".into(),
            principal_class: RuntimePrincipalClass::Model,
            surface: RuntimeSurface::Model,
            admitted_effects: effects,
            execution: RuntimeExecutionPolicy {
                principals: vec![RuntimePrincipalClass::Model],
                timeout_class: omegon_traits::RuntimeTimeoutClass::Interactive,
                retry_class: omegon_traits::RuntimeRetryClass::IdempotentFailure,
                idempotency: omegon_traits::RuntimeIdempotency::Idempotent,
                deduplication: omegon_traits::RuntimeDeduplication::OwnerEnforcedStableCallId,
                parallelism: omegon_traits::RuntimeParallelism::Serial,
                transaction: omegon_traits::RuntimeTransactionBehavior::None,
                mutation_fence: None,
                max_attempts: Some(2),
            },
            transition: RuntimeCapabilityTransitionPolicy {
                authority_narrowing: omegon_traits::RuntimeAuthorityNarrowing::CompleteExisting,
                active_call_timeout_ms: 30_000,
            },
            surfaces: vec![RuntimeSurface::Model],
        }
    }

    #[test]
    fn fact_round_trip_is_strict_and_stable() {
        let fact = created("session-1", Uuid::new_v4());
        let encoded = fact.encode().unwrap();
        assert_eq!(SessionFact::decode(&encoded).unwrap(), fact);

        let mut value: Value = serde_json::from_slice(&encoded).unwrap();
        value["payload"]["unexpected"] = Value::Bool(true);
        let error = SessionFact::decode(&serde_json::to_vec(&value).unwrap()).unwrap_err();
        assert!(error.to_string().contains("unknown field"));

        value = serde_json::from_slice(&encoded).unwrap();
        value["event_version"] = Value::from(2);
        let error = SessionFact::decode(&serde_json::to_vec(&value).unwrap()).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("unsupported session.created event version")
        );
    }

    #[test]
    fn prepared_and_dispatched_facts_round_trip_and_reduce_in_order() {
        let session_id = "session-invocation";
        let stream_id = Uuid::new_v4();
        let prompt_id = Uuid::new_v4();
        let turn_id = Uuid::new_v4();
        let preparation = prepared(turn_id, "call-1");
        let dispatch = InvocationDispatched {
            invocation_id: preparation.invocation_id,
            lease_id: preparation.lease_id,
        };
        let prepared_fact = fact(
            session_id,
            stream_id,
            4,
            SessionFactPayload::InvocationPrepared(preparation.clone()),
        );
        let dispatched_fact = fact(
            session_id,
            stream_id,
            5,
            SessionFactPayload::InvocationDispatched(dispatch.clone()),
        );
        assert_eq!(
            SessionFact::decode(&prepared_fact.encode().unwrap()).unwrap(),
            prepared_fact
        );
        assert_eq!(
            SessionFact::decode(&dispatched_fact.encode().unwrap()).unwrap(),
            dispatched_fact
        );

        let state = reconstruct(&[
            created(session_id, stream_id),
            admitted(session_id, stream_id, 2, prompt_id, "run"),
            started(session_id, stream_id, 3, prompt_id, turn_id),
            prepared_fact,
            dispatched_fact,
            fact(
                session_id,
                stream_id,
                6,
                SessionFactPayload::TurnClosed(TurnClosed {
                    turn_id,
                    outcome: TurnOutcome::Completed,
                    reason_code: "worker_completed".into(),
                    recovery_rule_version: None,
                }),
            ),
        ])
        .unwrap();
        assert!(matches!(
            state.invocations.get(&preparation.invocation_id),
            Some(InvocationState::Dispatched {
                preparation: stored,
                dispatch: stored_dispatch,
            }) if stored == &preparation && stored_dispatch == &dispatch
        ));
        assert!(state.active_turn.is_none());
        let recovery = recovery_facts(&state, NOW).unwrap();
        assert_eq!(recovery.len(), 1);
        assert!(matches!(
            recovery[0].payload,
            SessionFactPayload::InvocationClassifiedUnknown(_)
        ));
    }

    #[test]
    fn dispatch_requires_matching_prepared_lease() {
        let session_id = "session-mismatch";
        let stream_id = Uuid::new_v4();
        let prompt_id = Uuid::new_v4();
        let turn_id = Uuid::new_v4();
        let preparation = prepared(turn_id, "call-1");
        let error = reconstruct(&[
            created(session_id, stream_id),
            admitted(session_id, stream_id, 2, prompt_id, "run"),
            started(session_id, stream_id, 3, prompt_id, turn_id),
            fact(
                session_id,
                stream_id,
                4,
                SessionFactPayload::InvocationPrepared(preparation.clone()),
            ),
            fact(
                session_id,
                stream_id,
                5,
                SessionFactPayload::InvocationDispatched(InvocationDispatched {
                    invocation_id: preparation.invocation_id,
                    lease_id: Uuid::new_v4(),
                }),
            ),
        ])
        .unwrap_err();
        assert!(error.to_string().contains("dispatch lease does not match"));
    }

    #[test]
    fn acknowledged_invocation_settles_before_turn_closure() {
        let session_id = "session-settlement";
        let stream_id = Uuid::new_v4();
        let prompt_id = Uuid::new_v4();
        let turn_id = Uuid::new_v4();
        let preparation = prepared(turn_id, "call-1");
        let acknowledgement = InvocationAcknowledged {
            invocation_id: preparation.invocation_id,
            lease_id: preparation.lease_id,
        };
        let settlement = InvocationSettled {
            invocation_id: preparation.invocation_id,
            outcome: InvocationOutcome::Completed,
            terminal_evidence_reference: None,
        };
        let state = reconstruct(&[
            created(session_id, stream_id),
            admitted(session_id, stream_id, 2, prompt_id, "run"),
            started(session_id, stream_id, 3, prompt_id, turn_id),
            fact(
                session_id,
                stream_id,
                4,
                SessionFactPayload::InvocationPrepared(preparation.clone()),
            ),
            fact(
                session_id,
                stream_id,
                5,
                SessionFactPayload::InvocationDispatched(InvocationDispatched {
                    invocation_id: preparation.invocation_id,
                    lease_id: preparation.lease_id,
                }),
            ),
            fact(
                session_id,
                stream_id,
                6,
                SessionFactPayload::InvocationAcknowledged(acknowledgement.clone()),
            ),
            fact(
                session_id,
                stream_id,
                7,
                SessionFactPayload::InvocationSettled(settlement.clone()),
            ),
            fact(
                session_id,
                stream_id,
                8,
                SessionFactPayload::TurnClosed(TurnClosed {
                    turn_id,
                    outcome: TurnOutcome::Completed,
                    reason_code: "completed".into(),
                    recovery_rule_version: None,
                }),
            ),
        ])
        .unwrap();
        assert!(matches!(
            state.invocations.get(&preparation.invocation_id),
            Some(InvocationState::DurableSettled {
                acknowledgement: stored_acknowledgement,
                settlement: stored_settlement,
                ..
            }) if stored_acknowledgement == &acknowledgement && stored_settlement == &settlement
        ));
        assert!(state.active_turn.is_none());
    }

    #[test]
    fn recovery_preserves_prepared_and_classifies_dispatched_unknown() {
        let session_id = "session-recovery-phases";
        let stream_id = Uuid::new_v4();
        let prompt_id = Uuid::new_v4();
        let turn_id = Uuid::new_v4();
        let first = prepared(turn_id, "prepared-call");
        let second = prepared(turn_id, "dispatched-call");
        let state = reconstruct(&[
            created(session_id, stream_id),
            admitted(session_id, stream_id, 2, prompt_id, "run"),
            started(session_id, stream_id, 3, prompt_id, turn_id),
            fact(
                session_id,
                stream_id,
                4,
                SessionFactPayload::InvocationPrepared(first.clone()),
            ),
            fact(
                session_id,
                stream_id,
                5,
                SessionFactPayload::InvocationPrepared(second.clone()),
            ),
            fact(
                session_id,
                stream_id,
                6,
                SessionFactPayload::InvocationDispatched(InvocationDispatched {
                    invocation_id: second.invocation_id,
                    lease_id: second.lease_id,
                }),
            ),
        ])
        .unwrap();
        let recovery = recovery_facts(&state, NOW).unwrap();
        assert_eq!(recovery.len(), 2);
        assert!(matches!(
            recovery[0].payload,
            SessionFactPayload::InvocationClassifiedUnknown(_)
        ));
        assert!(matches!(
            recovery[1].payload,
            SessionFactPayload::TurnClosed(_)
        ));
        let mut recovered = state;
        for fact in &recovery {
            recovered.apply(fact).unwrap();
        }
        assert!(matches!(
            recovered.invocations.get(&first.invocation_id),
            Some(InvocationState::Prepared { .. })
        ));
        assert!(matches!(
            recovered.invocations.get(&second.invocation_id),
            Some(InvocationState::DurableUnknown { .. })
        ));
    }

    #[test]
    fn minimum_vocabulary_round_trips() {
        let session = "session-1";
        let stream = Uuid::new_v4();
        let prompt = Uuid::new_v4();
        let turn = Uuid::new_v4();
        let invocation = Uuid::new_v4();
        let payloads = vec![
            SessionFactPayload::SessionCreated(SessionCreated {
                workspace_identity: "workspace".into(),
                created_by: ActorIdentity {
                    principal: "operator".into(),
                    ingress: "tui".into(),
                },
                runtime_generation_id: "generation".into(),
            }),
            SessionFactPayload::PromptAdmitted(PromptAdmitted {
                submission_id: Uuid::new_v4(),
                prompt_id: prompt,
                principal: "operator".into(),
                ingress: "tui".into(),
                queue_mode: QueueMode::UntilReady,
                content: PromptContent {
                    text: "work".into(),
                    attachments: vec![AttachmentRef {
                        digest: fingerprint("attachment"),
                        media_type: "image/png".into(),
                        byte_length: 4,
                        storage_ref: "sha256/attachment".into(),
                    }],
                },
                metadata: serde_json::json!({"voice": false}),
            }),
            SessionFactPayload::PromptRejected(PromptRejected {
                submission_id: Uuid::new_v4(),
                principal: "operator".into(),
                ingress: "ipc".into(),
                reason_code: "session_closing".into(),
            }),
            SessionFactPayload::PromptRemoved(PromptRemoved {
                prompt_id: prompt,
                reason: PromptRemovalReason::Withdrawn,
            }),
            SessionFactPayload::TurnStarted(TurnStarted {
                turn_id: turn,
                prompt_id: prompt,
                runtime_generation_id: "generation".into(),
            }),
            SessionFactPayload::TurnInterruptionRequested(TurnInterruptionRequested {
                interruption_id: Uuid::new_v4(),
                turn_id: turn,
                kind: InterruptionKind::Cancel,
                principal: "operator".into(),
                ingress: "tui".into(),
                reason_code: "operator_cancelled".into(),
            }),
            SessionFactPayload::InvocationRegistered(InvocationRegistered {
                invocation_id: invocation,
                turn_id: turn,
                call_id: "call-1".into(),
                owner_generation_id: Some("tools-1".into()),
            }),
            SessionFactPayload::InvocationAcknowledged(InvocationAcknowledged {
                invocation_id: invocation,
                lease_id: Uuid::new_v4(),
            }),
            SessionFactPayload::InvocationClassifiedUnknown(InvocationClassifiedUnknown {
                invocation_id: invocation,
                reason_code: "runtime_lost".into(),
                recovery_rule_version: 1,
            }),
            SessionFactPayload::InvocationSettled(InvocationSettled {
                invocation_id: invocation,
                outcome: InvocationOutcome::Completed,
                terminal_evidence_reference: Some("owner:receipt-1".into()),
            }),
            SessionFactPayload::TurnClosed(TurnClosed {
                turn_id: turn,
                outcome: TurnOutcome::Cancelled,
                reason_code: "cancelled".into(),
                recovery_rule_version: None,
            }),
        ];
        for (index, payload) in payloads.into_iter().enumerate() {
            let fact = fact(session, stream, index as u64 + 1, payload);
            assert_eq!(SessionFact::decode(&fact.encode().unwrap()).unwrap(), fact);
        }
    }

    #[test]
    fn reducer_reconstructs_fifo_and_exact_terminal_state() {
        let session = "session-1";
        let stream = Uuid::new_v4();
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let turn = Uuid::new_v4();
        let facts = vec![
            created(session, stream),
            admitted(session, stream, 2, first, "first"),
            admitted(session, stream, 3, second, "second"),
            started(session, stream, 4, first, turn),
            fact(
                session,
                stream,
                5,
                SessionFactPayload::TurnClosed(TurnClosed {
                    turn_id: turn,
                    outcome: TurnOutcome::Completed,
                    reason_code: "completed".into(),
                    recovery_rule_version: None,
                }),
            ),
        ];
        let state = reconstruct(&facts).unwrap();
        assert!(state.active_turn.is_none());
        assert_eq!(state.queued_prompts.len(), 1);
        assert_eq!(state.queued_prompts[0].prompt_id, second);
        assert_eq!(state.closed_turns[&turn].outcome, TurnOutcome::Completed);
        let first_submission = match &facts[1].payload {
            SessionFactPayload::PromptAdmitted(value) => value.submission_id,
            _ => unreachable!(),
        };
        assert_eq!(state.prompt_ids[&first], first_submission);
        assert_eq!(state.turn_starts[&turn].prompt_id, first);
    }

    #[test]
    fn reducer_retains_prompt_and_submission_identity_after_terminalization() {
        let session = "session-1";
        let stream = Uuid::new_v4();
        let prompt = Uuid::new_v4();
        let turn = Uuid::new_v4();
        let admission = admitted(session, stream, 2, prompt, "work");
        let submission = match &admission.payload {
            SessionFactPayload::PromptAdmitted(value) => value.submission_id,
            _ => unreachable!(),
        };
        let facts = vec![
            created(session, stream),
            admission,
            started(session, stream, 3, prompt, turn),
            fact(
                session,
                stream,
                4,
                SessionFactPayload::TurnClosed(TurnClosed {
                    turn_id: turn,
                    outcome: TurnOutcome::Completed,
                    reason_code: "completed".into(),
                    recovery_rule_version: None,
                }),
            ),
        ];
        let mut state = reconstruct(&facts).unwrap();
        let reused_prompt = PromptAdmitted {
            submission_id: Uuid::new_v4(),
            prompt_id: prompt,
            principal: "operator".into(),
            ingress: "tui".into(),
            queue_mode: QueueMode::UntilReady,
            content: PromptContent {
                text: "duplicate".into(),
                attachments: Vec::new(),
            },
            metadata: serde_json::json!({}),
        };
        assert!(
            state
                .apply(&fact(
                    session,
                    stream,
                    5,
                    SessionFactPayload::PromptAdmitted(reused_prompt),
                ))
                .unwrap_err()
                .to_string()
                .contains("already present")
        );
        let rejection = PromptRejected {
            submission_id: submission,
            principal: "operator".into(),
            ingress: "ipc".into(),
            reason_code: "late".into(),
        };
        assert!(
            state
                .apply(&fact(
                    session,
                    stream,
                    5,
                    SessionFactPayload::PromptRejected(rejection),
                ))
                .unwrap_err()
                .to_string()
                .contains("already has an outcome")
        );
    }

    #[test]
    fn reducer_rejects_gap_non_fifo_start_and_duplicate_terminal() {
        let session = "session-1";
        let stream = Uuid::new_v4();
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let mut state = SessionAuthorityState::default();
        let creation = created(session, stream);
        state.apply(&creation).unwrap();
        let gap = admitted(session, stream, 3, first, "gap");
        assert!(
            state
                .apply(&gap)
                .unwrap_err()
                .to_string()
                .contains("expected sequence 2")
        );
        state
            .apply(&admitted(session, stream, 2, first, "first"))
            .unwrap();
        let mut reused_event = admitted(session, stream, 3, second, "reused-event");
        reused_event.event_id = creation.event_id;
        assert!(
            state
                .apply(&reused_event)
                .unwrap_err()
                .to_string()
                .contains("duplicate event ID")
        );
        state
            .apply(&admitted(session, stream, 3, second, "second"))
            .unwrap();
        let turn = Uuid::new_v4();
        let non_fifo = started(session, stream, 4, second, turn);
        assert!(
            state
                .apply(&non_fifo)
                .unwrap_err()
                .to_string()
                .contains("FIFO")
        );
        state
            .apply(&started(session, stream, 4, first, turn))
            .unwrap();
        state
            .apply(&fact(
                session,
                stream,
                5,
                SessionFactPayload::TurnClosed(TurnClosed {
                    turn_id: turn,
                    outcome: TurnOutcome::Completed,
                    reason_code: "completed".into(),
                    recovery_rule_version: None,
                }),
            ))
            .unwrap();
        let duplicate = fact(
            session,
            stream,
            6,
            SessionFactPayload::TurnClosed(TurnClosed {
                turn_id: turn,
                outcome: TurnOutcome::Failed,
                reason_code: "late".into(),
                recovery_rule_version: None,
            }),
        );
        assert!(
            state
                .apply(&duplicate)
                .unwrap_err()
                .to_string()
                .contains("no active turn")
        );
    }

    #[test]
    fn recovery_is_deterministic_and_classifies_unsettled_invocations() {
        let session = "session-1";
        let stream = Uuid::new_v4();
        let prompt = Uuid::new_v4();
        let turn = Uuid::new_v4();
        let invocation = Uuid::new_v4();
        let facts = vec![
            created(session, stream),
            admitted(session, stream, 2, prompt, "work"),
            started(session, stream, 3, prompt, turn),
            fact(
                session,
                stream,
                4,
                SessionFactPayload::InvocationRegistered(InvocationRegistered {
                    invocation_id: invocation,
                    turn_id: turn,
                    call_id: "call-1".into(),
                    owner_generation_id: Some("tools-1".into()),
                }),
            ),
        ];
        let mut state = reconstruct(&facts).unwrap();
        let recovery = recovery_facts(&state, NOW).unwrap();
        let repeated = recovery_facts(&state, NOW).unwrap();
        assert_eq!(recovery, repeated);
        assert_eq!(recovery.len(), 2);
        for fact in &recovery {
            state.apply(fact).unwrap();
        }
        assert!(state.active_turn.is_none());
        assert!(matches!(
            state.invocations[&invocation],
            InvocationState::Unknown { .. }
        ));
        assert_eq!(state.closed_turns[&turn].outcome, TurnOutcome::Interrupted);
        assert!(recovery_facts(&state, NOW).unwrap().is_empty());
    }

    #[test]
    fn store_appends_syncs_replays_tail_and_ignores_bad_cache() {
        let temp = tempfile::tempdir().unwrap();
        let session_path = temp.path().join("session-1.json");
        let store = SessionAuthorityStore::adjacent_to(&session_path).unwrap();
        let session = "session-1";
        let stream = Uuid::new_v4();
        let prompt = Uuid::new_v4();
        let mut state = SessionAuthorityState::default();
        assert!(store.append(&mut state, &created(session, stream)).unwrap());
        let old_snapshot = fs::read(&store.snapshot_path).unwrap();
        let admission = admitted(session, stream, 2, prompt, "work");
        assert!(store.append(&mut state, &admission).unwrap());
        assert_eq!(store.load().unwrap(), state);

        fs::write(&store.snapshot_path, old_snapshot).unwrap();
        assert_eq!(store.load().unwrap(), state);

        fs::write(&store.snapshot_path, b"not json").unwrap();
        assert_eq!(store.load().unwrap(), state);

        assert!(!store.append(&mut state, &admission).unwrap());
        assert_eq!(state.last_sequence, 2);

        let mut retry = admission.clone();
        retry.event_id = Uuid::new_v4();
        assert!(!store.append(&mut state, &retry).unwrap());

        let mut conflict = admission.clone();
        conflict.command_fingerprint = fingerprint("different-command");
        assert!(
            store
                .append(&mut state, &conflict)
                .unwrap_err()
                .to_string()
                .contains("conflicting event or fingerprint")
        );
    }

    #[test]
    fn store_discards_valid_cache_when_cursor_or_state_mismatches_stream() {
        let temp = tempfile::tempdir().unwrap();
        let store =
            SessionAuthorityStore::adjacent_to(&temp.path().join("session-1.json")).unwrap();
        let session = "session-1";
        let stream = Uuid::new_v4();
        let prompt = Uuid::new_v4();
        let mut expected = SessionAuthorityState::default();
        store
            .append(&mut expected, &created(session, stream))
            .unwrap();
        store
            .append(&mut expected, &admitted(session, stream, 2, prompt, "work"))
            .unwrap();
        let log_before = fs::read(&store.log_path).unwrap();
        let valid: SessionAuthoritySnapshot =
            serde_json::from_slice(&fs::read(&store.snapshot_path).unwrap()).unwrap();

        let mut wrong_event = valid.clone();
        wrong_event.last_event_id = Uuid::new_v4();
        wrong_event.state.last_event_id = Some(wrong_event.last_event_id);
        write_snapshot(&store.snapshot_path, &wrong_event).unwrap();
        assert_eq!(store.load().unwrap(), expected);

        let mut wrong_state = valid;
        wrong_state.state.queued_prompts.clear();
        write_snapshot(&store.snapshot_path, &wrong_state).unwrap();
        assert_eq!(store.load().unwrap(), expected);
        assert_eq!(fs::read(&store.log_path).unwrap(), log_before);
    }

    #[test]
    fn session_authority_commits_typed_transitions_and_reopens_at_same_cursor() {
        let temp = tempfile::tempdir().unwrap();
        let session_path = temp.path().join("session-1.json");
        let actor = ActorIdentity {
            principal: "operator".into(),
            ingress: "tui".into(),
        };
        let mut authority = SessionAuthority::open(
            &session_path,
            "session-1",
            "workspace-1",
            "generation-1",
            actor,
            NOW,
        )
        .unwrap();
        assert_eq!(authority.state().last_sequence, 1);

        let prompt_id = Uuid::new_v4();
        let admission_command = Uuid::new_v4();
        let admission = PromptAdmitted {
            submission_id: Uuid::new_v4(),
            prompt_id,
            principal: "operator".into(),
            ingress: "tui".into(),
            queue_mode: QueueMode::UntilReady,
            content: PromptContent {
                text: "inspect the workspace".into(),
                attachments: Vec::new(),
            },
            metadata: serde_json::json!({}),
        };
        assert!(
            authority
                .admit_prompt(admission_command, NOW, admission.clone())
                .unwrap()
        );
        assert!(
            !authority
                .admit_prompt(admission_command, NOW, admission)
                .unwrap()
        );

        let turn_id = Uuid::new_v4();
        assert!(
            authority
                .start_turn(Uuid::new_v4(), NOW, turn_id, prompt_id)
                .unwrap()
        );
        assert!(
            authority
                .request_interruption(
                    Uuid::new_v4(),
                    NOW,
                    TurnInterruptionRequested {
                        interruption_id: Uuid::new_v4(),
                        turn_id,
                        kind: InterruptionKind::Cancel,
                        principal: "operator".into(),
                        ingress: "ipc".into(),
                        reason_code: "operator_cancelled".into(),
                    },
                )
                .unwrap()
        );
        assert!(
            authority
                .close_turn(
                    Uuid::new_v4(),
                    NOW,
                    TurnClosed {
                        turn_id,
                        outcome: TurnOutcome::Revoked,
                        reason_code: "worker_revoked".into(),
                        recovery_rule_version: None,
                    },
                )
                .unwrap()
        );
        assert_eq!(authority.state().last_sequence, 5);
        assert!(authority.state().active_turn.is_none());
        drop(authority);

        let mut reopened = SessionAuthority::open(
            &session_path,
            "session-1",
            "workspace-1",
            "generation-2",
            ActorIdentity {
                principal: "system".into(),
                ingress: "resume".into(),
            },
            "2026-08-19T19:00:00Z",
        )
        .unwrap();
        assert_eq!(reopened.state().last_sequence, 5);
        assert_eq!(
            reopened.state().closed_turns[&turn_id].outcome,
            TurnOutcome::Revoked
        );

        let resumed_prompt_id = Uuid::new_v4();
        reopened
            .admit_prompt(
                Uuid::new_v4(),
                "2026-08-19T19:00:01Z",
                PromptAdmitted {
                    submission_id: Uuid::new_v4(),
                    prompt_id: resumed_prompt_id,
                    principal: "operator".into(),
                    ingress: "tui".into(),
                    queue_mode: QueueMode::UntilReady,
                    content: PromptContent {
                        text: "continue".into(),
                        attachments: Vec::new(),
                    },
                    metadata: serde_json::json!({}),
                },
            )
            .unwrap();
        let resumed_turn_id = Uuid::new_v4();
        reopened
            .start_turn(
                Uuid::new_v4(),
                "2026-08-19T19:00:02Z",
                resumed_turn_id,
                resumed_prompt_id,
            )
            .unwrap();
        assert_eq!(
            reopened.state().turn_starts[&resumed_turn_id].runtime_generation_id,
            "generation-1"
        );
    }

    #[test]
    fn open_refuses_concurrent_writer_without_recovering_live_turn() {
        let temp = tempfile::tempdir().unwrap();
        let session_path = temp.path().join("session-1.json");
        let mut authority = SessionAuthority::open(
            &session_path,
            "session-1",
            "workspace-1",
            "generation-1",
            ActorIdentity {
                principal: "operator".into(),
                ingress: "tui".into(),
            },
            NOW,
        )
        .unwrap();
        let prompt_id = Uuid::new_v4();
        authority
            .admit_prompt(
                Uuid::new_v4(),
                NOW,
                PromptAdmitted {
                    submission_id: Uuid::new_v4(),
                    prompt_id,
                    principal: "operator".into(),
                    ingress: "tui".into(),
                    queue_mode: QueueMode::UntilReady,
                    content: PromptContent {
                        text: "still running".into(),
                        attachments: Vec::new(),
                    },
                    metadata: serde_json::json!({}),
                },
            )
            .unwrap();
        authority
            .start_turn(Uuid::new_v4(), NOW, Uuid::new_v4(), prompt_id)
            .unwrap();
        let before = authority.state().clone();

        let error = SessionAuthority::open(
            &session_path,
            "session-1",
            "workspace-1",
            "generation-2",
            ActorIdentity {
                principal: "system".into(),
                ingress: "resume".into(),
            },
            "2026-08-19T18:01:00Z",
        )
        .unwrap_err();
        assert!(error.to_string().contains("active writer"));
        assert_eq!(authority.state(), &before);
        assert_eq!(
            SessionAuthorityStore::adjacent_to(&session_path)
                .unwrap()
                .load()
                .unwrap(),
            before
        );
    }

    #[test]
    fn open_validates_stream_identity_before_recovery_mutation() {
        let temp = tempfile::tempdir().unwrap();
        let session_path = temp.path().join("session-1.json");
        let mut authority = SessionAuthority::open(
            &session_path,
            "session-1",
            "workspace-1",
            "generation-1",
            ActorIdentity {
                principal: "operator".into(),
                ingress: "tui".into(),
            },
            NOW,
        )
        .unwrap();
        let prompt_id = Uuid::new_v4();
        authority
            .admit_prompt(
                Uuid::new_v4(),
                NOW,
                PromptAdmitted {
                    submission_id: Uuid::new_v4(),
                    prompt_id,
                    principal: "operator".into(),
                    ingress: "tui".into(),
                    queue_mode: QueueMode::UntilReady,
                    content: PromptContent {
                        text: "unsettled".into(),
                        attachments: Vec::new(),
                    },
                    metadata: serde_json::json!({}),
                },
            )
            .unwrap();
        authority
            .start_turn(Uuid::new_v4(), NOW, Uuid::new_v4(), prompt_id)
            .unwrap();
        drop(authority);
        let store = SessionAuthorityStore::adjacent_to(&session_path).unwrap();
        let log_before = fs::read(&store.log_path).unwrap();

        let error = SessionAuthority::open(
            &session_path,
            "session-1",
            "wrong-workspace",
            "generation-2",
            ActorIdentity {
                principal: "system".into(),
                ingress: "resume".into(),
            },
            "2026-08-19T18:01:00Z",
        )
        .unwrap_err();
        assert!(error.to_string().contains("different workspace"));
        assert_eq!(fs::read(&store.log_path).unwrap(), log_before);
        assert!(store.load().unwrap().active_turn.is_some());
    }

    #[test]
    fn session_authority_stages_attachments_by_content_digest() {
        let temp = tempfile::tempdir().unwrap();
        let session_path = temp.path().join("session-1.json");
        let source = temp.path().join("capture.png");
        fs::write(&source, b"stable-image-bytes").unwrap();
        let authority = SessionAuthority::open(
            &session_path,
            "session-1",
            "workspace-1",
            "generation-1",
            ActorIdentity {
                principal: "operator".into(),
                ingress: "tui".into(),
            },
            NOW,
        )
        .unwrap();

        let first = authority.stage_attachment(&source).unwrap();
        let second = authority.stage_attachment(&source).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.media_type, "image/png");
        assert_eq!(first.byte_length, 18);
        assert_eq!(fs::read(&first.storage_ref).unwrap(), b"stable-image-bytes");
        assert!(
            Path::new(&first.storage_ref)
                .file_name()
                .is_some_and(|name| name == first.digest.as_str())
        );
    }

    #[test]
    fn store_recovery_holds_authority_and_is_idempotent() {
        let temp = tempfile::tempdir().unwrap();
        let store =
            SessionAuthorityStore::adjacent_to(&temp.path().join("session-1.json")).unwrap();
        let session = "session-1";
        let stream = Uuid::new_v4();
        let prompt = Uuid::new_v4();
        let turn = Uuid::new_v4();
        let invocation = Uuid::new_v4();
        let mut state = SessionAuthorityState::default();
        let initial = vec![
            created(session, stream),
            admitted(session, stream, 2, prompt, "work"),
            started(session, stream, 3, prompt, turn),
            fact(
                session,
                stream,
                4,
                SessionFactPayload::InvocationRegistered(InvocationRegistered {
                    invocation_id: invocation,
                    turn_id: turn,
                    call_id: "call-1".into(),
                    owner_generation_id: None,
                }),
            ),
        ];
        for fact in initial {
            store.append(&mut state, &fact).unwrap();
        }

        let recovered = store.recover(NOW).unwrap();
        assert!(recovered.active_turn.is_none());
        assert_eq!(recovered.last_sequence, 6);
        assert!(matches!(
            recovered.invocations[&invocation],
            InvocationState::Unknown { .. }
        ));
        assert_eq!(store.recover("2026-08-19T19:00:00Z").unwrap(), recovered);
        assert_eq!(read_facts(&store.log_path).unwrap().len(), 6);
    }

    #[test]
    fn store_rejects_truncated_log_and_preserves_state_on_append_failure() {
        let temp = tempfile::tempdir().unwrap();
        let log = temp.path().join("authority.jsonl");
        let snapshot = temp.path().join("snapshot.json");
        let store = SessionAuthorityStore::from_paths(log.clone(), snapshot);
        fs::write(&log, b"{\"truncated\":true}").unwrap();
        assert!(store.load().unwrap_err().to_string().contains("truncated"));

        fs::remove_file(&log).unwrap();
        fs::create_dir(&log).unwrap();
        let mut state = SessionAuthorityState::default();
        let before = state.clone();
        let error = store
            .append(&mut state, &created("session-1", Uuid::new_v4()))
            .unwrap_err();
        assert!(matches!(error, AuthorityError::Io(_)));
        assert_eq!(state, before);
    }
}
