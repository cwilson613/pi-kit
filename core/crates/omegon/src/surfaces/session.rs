//! Frozen schema-v1 session projection DTOs.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    session_authority::{
        AssistantContentKind, AssistantContentManifest, CompactionContextItem,
        CompactionOwnerScope, ModelContextItem, ModelRequestPurpose, ModelSchemaSet, PromptContent,
        ToolResultDisposition,
    },
    session_blob_store::ContentRef,
};

pub(crate) const PROJECTOR_VERSION: u16 = 1;
pub(crate) const PROJECTION_SCHEMA_VERSION: u16 = 1;
pub(crate) const MAX_CHUNK_ITEMS: usize = 4_096;
pub(crate) const MAX_CHUNK_BYTES: usize = 8 * 1024 * 1024;
pub(crate) const MAX_OUTPUT_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub(crate) enum ProjectionValidationError {
    #[error("projection JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("projection is invalid: {0}")]
    Invalid(String),
}

pub(crate) type ProjectionResult<T> = Result<T, ProjectionValidationError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProjectorIdV1 {
    #[serde(rename = "session.provider-history")]
    ProviderHistory,
    #[serde(rename = "session.transcript")]
    Transcript,
    #[serde(rename = "session.frontend-snapshot")]
    FrontendSnapshot,
    #[serde(rename = "session.compaction-checkpoint")]
    CompactionCheckpoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProjectionLineageV1 {
    Legacy,
    Mixed,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProjectionAvailabilityV1 {
    Unavailable,
    Available,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProjectionExactnessV1 {
    None,
    ExactSuffix,
    ExactFull,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProjectionScopeV1 {
    None,
    FullSpineSuffix,
    FullSession,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FullSessionExportV1 {
    Unavailable,
    Available,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProjectionUnavailableReasonV1 {
    LegacyLineage,
    PreBoundaryContentNotAuthoritative,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceEventV1 {
    pub(crate) sequence: u64,
    pub(crate) event_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProjectionUnavailableV1 {
    pub(crate) reason: ProjectionUnavailableReasonV1,
    pub(crate) first_sequence: Option<u64>,
    pub(crate) content_digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ProjectionPayloadV1 {
    None,
    ChunkManifest { manifest: ChunkManifestV1 },
    FrontendSnapshot { snapshot: FrontendSnapshotV1 },
    CompactionCheckpoint { checkpoint: CompactionCheckpointV1 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProjectionEnvelopeV1 {
    pub(crate) envelope_schema_version: u16,
    pub(crate) projector_id: ProjectorIdV1,
    pub(crate) projector_version: u16,
    pub(crate) projection_schema_version: u16,
    pub(crate) session_id: String,
    pub(crate) stream_id: Option<Uuid>,
    pub(crate) lineage_level: ProjectionLineageV1,
    pub(crate) availability: ProjectionAvailabilityV1,
    pub(crate) exactness: ProjectionExactnessV1,
    pub(crate) scope: ProjectionScopeV1,
    pub(crate) full_spine_boundary: Option<SourceEventV1>,
    pub(crate) source_frontier: Option<SourceEventV1>,
    pub(crate) full_session_export: FullSessionExportV1,
    pub(crate) unavailable: Option<ProjectionUnavailableV1>,
    pub(crate) payload: ProjectionPayloadV1,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChunkManifestV1 {
    pub(crate) manifest_schema_version: u16,
    pub(crate) projector_id: ProjectorIdV1,
    pub(crate) session_id: String,
    pub(crate) stream_id: Uuid,
    pub(crate) source_frontier: SourceEventV1,
    pub(crate) chunk_count: u32,
    pub(crate) item_count: u64,
    pub(crate) chunks: Vec<ChunkManifestEntryV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChunkManifestEntryV1 {
    pub(crate) chunk_ordinal: u32,
    pub(crate) chunk_id: String,
    pub(crate) first_item_ordinal: u64,
    pub(crate) last_item_ordinal: u64,
    pub(crate) item_count: u32,
    pub(crate) byte_length: u64,
    pub(crate) digest_algorithm: DigestAlgorithmV1,
    pub(crate) digest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DigestAlgorithmV1 {
    Sha256,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProjectionChunkV1 {
    pub(crate) chunk_schema_version: u16,
    pub(crate) projector_id: ProjectorIdV1,
    pub(crate) session_id: String,
    pub(crate) stream_id: Uuid,
    pub(crate) chunk_ordinal: u32,
    pub(crate) first_item_ordinal: u64,
    pub(crate) last_item_ordinal: u64,
    pub(crate) items: ProjectionChunkItemsV1,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(crate) enum ProjectionChunkItemsV1 {
    ProviderRequests(Vec<ProviderRequestInputV1>),
    TranscriptMessages(Vec<TranscriptMessageV1>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProviderRequestInputV1 {
    pub(crate) item_ordinal: u64,
    pub(crate) request_id: Uuid,
    pub(crate) step_id: Uuid,
    pub(crate) turn_id: Uuid,
    pub(crate) request_ordinal: u32,
    pub(crate) purpose: ModelRequestPurpose,
    pub(crate) replaces_request_id: Option<Uuid>,
    pub(crate) prepared_event: SourceEventV1,
    pub(crate) route_join_event: SourceEventV1,
    pub(crate) lease_event: SourceEventV1,
    pub(crate) lease_id: Uuid,
    pub(crate) selected_provider_id: String,
    pub(crate) selected_model_id: String,
    pub(crate) serving_provider_id: String,
    pub(crate) serving_model_id: String,
    pub(crate) schema_dialect: String,
    pub(crate) credential_source_class: String,
    pub(crate) fallback_reason: Option<String>,
    pub(crate) contribution_generation_id: String,
    pub(crate) route_policy: String,
    pub(crate) continuity_ids: Vec<Uuid>,
    pub(crate) context_manifest_id: String,
    pub(crate) context_items: Vec<ModelContextItem>,
    pub(crate) schema_set_id: String,
    pub(crate) schema_set: ModelSchemaSet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TranscriptMessageKindV1 {
    Prompt,
    Assistant,
    ToolResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TranscriptRoleV1 {
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TranscriptStatusV1 {
    Normal,
    AbandonedAfterCommit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(crate) enum TranscriptContentV1 {
    Prompt {
        prompt_content: PromptContent,
    },
    Assistant {
        assistant_channels: Vec<AssistantContentManifest>,
    },
    ToolResult {
        tool_result_id: Uuid,
        tool_call_id: Uuid,
        call_id: String,
        content_ref: ContentRef,
        is_error: bool,
        disposition: ToolResultDisposition,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TranscriptMessageV1 {
    pub(crate) item_ordinal: u64,
    pub(crate) message_kind: TranscriptMessageKindV1,
    pub(crate) role: TranscriptRoleV1,
    pub(crate) message_id: Uuid,
    pub(crate) turn_id: Option<Uuid>,
    pub(crate) step_id: Option<Uuid>,
    pub(crate) request_id: Option<Uuid>,
    pub(crate) source_event: SourceEventV1,
    pub(crate) content: TranscriptContentV1,
    pub(crate) status: TranscriptStatusV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ActiveTurnStatusV1 {
    Active,
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FrontendSnapshotV1 {
    pub(crate) snapshot_schema_version: u16,
    pub(crate) queued_prompts: Vec<QueuedPromptV1>,
    pub(crate) active_turn: Option<FrontendActiveTurnV1>,
    pub(crate) context: FrontendContextV1,
    pub(crate) conversation: Vec<FrontendConversationItemV1>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct QueuedPromptV1 {
    pub(crate) queue_ordinal: u64,
    pub(crate) prompt_id: Uuid,
    pub(crate) submission_id: Uuid,
    pub(crate) content: PromptContent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FrontendActiveTurnV1 {
    pub(crate) turn_id: Uuid,
    pub(crate) prompt_id: Uuid,
    pub(crate) status: ActiveTurnStatusV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FrontendContextV1 {
    pub(crate) context_revision: u64,
    pub(crate) context_manifest_id: String,
    pub(crate) items: Vec<CompactionContextItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FrontendConversationKindV1 {
    CommittedMessage,
    AssistantEvidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FrontendConversationStatusV1 {
    Committed,
    Partial,
    Abandoned,
    AbandonedAfterCommit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FrontendConversationItemV1 {
    pub(crate) item_ordinal: u64,
    pub(crate) kind: FrontendConversationKindV1,
    pub(crate) turn_id: Option<Uuid>,
    pub(crate) step_id: Option<Uuid>,
    pub(crate) request_id: Option<Uuid>,
    pub(crate) message_id: Option<Uuid>,
    pub(crate) response_attempt_ordinal: Option<u32>,
    pub(crate) content_kind: Option<AssistantContentKind>,
    pub(crate) chunk_ordinal: Option<u32>,
    pub(crate) content_ref: Option<ContentRef>,
    pub(crate) transcript_message: Option<TranscriptMessageV1>,
    pub(crate) status: FrontendConversationStatusV1,
    pub(crate) source_event: SourceEventV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CompactionStateV1 {
    Never,
    Idle,
    InProgress,
    Applied,
    Abandoned,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ActiveCompactionV1 {
    pub(crate) compaction_id: Uuid,
    pub(crate) owner_scope: CompactionOwnerScope,
    pub(crate) source_frontier: crate::session_authority::AuthorityFrontierRef,
    pub(crate) source_context_revision: u64,
    pub(crate) target_context_revision: u64,
    pub(crate) input_manifest_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CompactionTerminalV1 {
    Applied,
    Abandoned,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LastCompactionTerminalV1 {
    pub(crate) compaction_id: Uuid,
    pub(crate) terminal: CompactionTerminalV1,
    pub(crate) terminal_event: SourceEventV1,
    pub(crate) source_context_revision: u64,
    pub(crate) target_context_revision: Option<u64>,
    pub(crate) replacement_manifest_id: Option<String>,
    pub(crate) compaction_summary_id: Option<Uuid>,
    pub(crate) reason_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CompactionCheckpointV1 {
    pub(crate) checkpoint_schema_version: u16,
    pub(crate) context_revision: u64,
    pub(crate) context_manifest_id: String,
    pub(crate) context_items: Vec<CompactionContextItem>,
    pub(crate) compaction_state: CompactionStateV1,
    pub(crate) active_compaction: Option<ActiveCompactionV1>,
    pub(crate) last_terminal: Option<LastCompactionTerminalV1>,
}

pub(crate) fn canonical_json_bytes(value: &(impl Serialize + ?Sized)) -> ProjectionResult<Vec<u8>> {
    fn canonicalize(value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(values) => serde_json::Value::Object(
                values
                    .into_iter()
                    .map(|(key, value)| (key, canonicalize(value)))
                    .collect::<std::collections::BTreeMap<_, _>>()
                    .into_iter()
                    .collect(),
            ),
            serde_json::Value::Array(values) => {
                serde_json::Value::Array(values.into_iter().map(canonicalize).collect())
            }
            value => value,
        }
    }
    Ok(serde_json::to_vec(&canonicalize(serde_json::to_value(
        value,
    )?))?)
}

pub(crate) fn canonical_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

impl ProjectionEnvelopeV1 {
    pub(crate) fn canonical_bytes(&self) -> ProjectionResult<Vec<u8>> {
        self.validate()?;
        let bytes = canonical_json_bytes(self)?;
        if bytes.len() > MAX_OUTPUT_BYTES {
            return Err(ProjectionValidationError::Invalid(
                "projection envelope exceeds 16 MiB".into(),
            ));
        }
        Ok(bytes)
    }

    pub(crate) fn validate(&self) -> ProjectionResult<()> {
        if self.envelope_schema_version != 1
            || self.projector_version != PROJECTOR_VERSION
            || self.projection_schema_version != PROJECTION_SCHEMA_VERSION
            || self.session_id.is_empty()
            || self.session_id.len() > 256
        {
            return Err(ProjectionValidationError::Invalid(
                "unsupported or unbounded projection envelope".into(),
            ));
        }
        if self
            .source_frontier
            .as_ref()
            .is_some_and(|frontier| frontier.sequence == 0)
        {
            return Err(ProjectionValidationError::Invalid(
                "source frontier sequence must be nonzero".into(),
            ));
        }
        let valid = match self.lineage_level {
            ProjectionLineageV1::Legacy => {
                self.availability == ProjectionAvailabilityV1::Unavailable
                    && self.exactness == ProjectionExactnessV1::None
                    && self.scope == ProjectionScopeV1::None
                    && self.full_spine_boundary.is_none()
                    && self.full_session_export == FullSessionExportV1::Unavailable
                    && matches!(
                        self.unavailable,
                        Some(ProjectionUnavailableV1 {
                            reason: ProjectionUnavailableReasonV1::LegacyLineage,
                            first_sequence: None,
                            content_digest: None,
                        })
                    )
                    && matches!(self.payload, ProjectionPayloadV1::None)
            }
            ProjectionLineageV1::Mixed => {
                self.availability == ProjectionAvailabilityV1::Available
                    && self.exactness == ProjectionExactnessV1::ExactSuffix
                    && self.scope == ProjectionScopeV1::FullSpineSuffix
                    && self.full_spine_boundary.is_some()
                    && self.source_frontier.is_some()
                    && self.full_session_export == FullSessionExportV1::Unavailable
                    && matches!(
                        self.unavailable,
                        Some(ProjectionUnavailableV1 {
                            reason:
                                ProjectionUnavailableReasonV1::PreBoundaryContentNotAuthoritative,
                            first_sequence: Some(_),
                            content_digest: None,
                        })
                    )
                    && !matches!(self.payload, ProjectionPayloadV1::None)
            }
            ProjectionLineageV1::Full => {
                self.availability == ProjectionAvailabilityV1::Available
                    && self.exactness == ProjectionExactnessV1::ExactFull
                    && self.scope == ProjectionScopeV1::FullSession
                    && self.full_spine_boundary.is_none()
                    && self.source_frontier.is_some()
                    && self.full_session_export == FullSessionExportV1::Available
                    && self.unavailable.is_none()
                    && !matches!(self.payload, ProjectionPayloadV1::None)
            }
        };
        if !valid {
            return Err(ProjectionValidationError::Invalid(
                "projection availability envelope is contradictory".into(),
            ));
        }
        Ok(())
    }
}

impl ChunkManifestV1 {
    pub(crate) fn validate(&self) -> ProjectionResult<()> {
        if self.manifest_schema_version != 1
            || !matches!(
                self.projector_id,
                ProjectorIdV1::ProviderHistory | ProjectorIdV1::Transcript
            )
            || self.source_frontier.sequence == 0
            || usize::try_from(self.chunk_count).ok() != Some(self.chunks.len())
        {
            return Err(ProjectionValidationError::Invalid(
                "invalid chunk manifest header".into(),
            ));
        }
        let mut next_item = 0_u64;
        let mut total = 0_u64;
        for (ordinal, chunk) in self.chunks.iter().enumerate() {
            if chunk.chunk_ordinal as usize != ordinal
                || chunk.first_item_ordinal != next_item
                || chunk.item_count == 0
                || chunk.item_count as usize > MAX_CHUNK_ITEMS
                || chunk.last_item_ordinal
                    != chunk.first_item_ordinal + u64::from(chunk.item_count) - 1
                || chunk.byte_length as usize > MAX_CHUNK_BYTES
                || chunk.chunk_id != chunk.digest
                || !valid_digest(&chunk.digest)
            {
                return Err(ProjectionValidationError::Invalid(
                    "invalid chunk manifest range or digest".into(),
                ));
            }
            next_item = chunk.last_item_ordinal + 1;
            total += u64::from(chunk.item_count);
        }
        if total != self.item_count {
            return Err(ProjectionValidationError::Invalid(
                "chunk manifest item count disagrees".into(),
            ));
        }
        if canonical_json_bytes(self)?.len() > MAX_OUTPUT_BYTES {
            return Err(ProjectionValidationError::Invalid(
                "chunk manifest exceeds 16 MiB".into(),
            ));
        }
        Ok(())
    }

    pub(crate) fn validate_chunks(
        &self,
        chunks: &[(ProjectionChunkV1, Vec<u8>)],
    ) -> ProjectionResult<()> {
        self.validate()?;
        if chunks.len() != self.chunks.len() {
            return Err(ProjectionValidationError::Invalid(
                "manifested chunk count disagrees with supplied chunks".into(),
            ));
        }
        for ((chunk, bytes), entry) in chunks.iter().zip(&self.chunks) {
            chunk.validate()?;
            let canonical = canonical_json_bytes(chunk)?;
            let digest = canonical_sha256(&canonical);
            if bytes != &canonical
                || chunk.projector_id != self.projector_id
                || chunk.session_id != self.session_id
                || chunk.stream_id != self.stream_id
                || chunk.chunk_ordinal != entry.chunk_ordinal
                || chunk.first_item_ordinal != entry.first_item_ordinal
                || chunk.last_item_ordinal != entry.last_item_ordinal
                || bytes.len() as u64 != entry.byte_length
                || digest != entry.chunk_id
                || digest != entry.digest
            {
                return Err(ProjectionValidationError::Invalid(
                    "manifest does not match canonical chunk bytes".into(),
                ));
            }
        }
        Ok(())
    }
}

impl ProjectionChunkV1 {
    pub(crate) fn validate(&self) -> ProjectionResult<()> {
        let item_ordinals = match (&self.projector_id, &self.items) {
            (ProjectorIdV1::ProviderHistory, ProjectionChunkItemsV1::ProviderRequests(values)) => {
                values
                    .iter()
                    .map(|value| value.item_ordinal)
                    .collect::<Vec<_>>()
            }
            (ProjectorIdV1::Transcript, ProjectionChunkItemsV1::TranscriptMessages(values)) => {
                values
                    .iter()
                    .map(|value| value.item_ordinal)
                    .collect::<Vec<_>>()
            }
            _ => {
                return Err(ProjectionValidationError::Invalid(
                    "chunk projector and item type disagree".into(),
                ));
            }
        };
        if self.chunk_schema_version != 1
            || item_ordinals.is_empty()
            || item_ordinals.len() > MAX_CHUNK_ITEMS
            || item_ordinals.first().copied() != Some(self.first_item_ordinal)
            || item_ordinals.last().copied() != Some(self.last_item_ordinal)
            || item_ordinals
                .windows(2)
                .any(|pair| pair[1] != pair[0].saturating_add(1))
            || canonical_json_bytes(self)?.len() > MAX_CHUNK_BYTES
        {
            return Err(ProjectionValidationError::Invalid(
                "invalid projection chunk ordering or bound".into(),
            ));
        }
        Ok(())
    }
}

impl FrontendSnapshotV1 {
    pub(crate) fn validate(&self) -> ProjectionResult<()> {
        if self.snapshot_schema_version != 1 {
            return Err(ProjectionValidationError::Invalid(
                "unsupported frontend snapshot schema".into(),
            ));
        }
        for (ordinal, prompt) in self.queued_prompts.iter().enumerate() {
            if prompt.queue_ordinal != ordinal as u64 {
                return Err(ProjectionValidationError::Invalid(
                    "queued prompt ordinals are not contiguous".into(),
                ));
            }
        }
        for (ordinal, item) in self.conversation.iter().enumerate() {
            if item.item_ordinal != ordinal as u64
                || (item.kind == FrontendConversationKindV1::CommittedMessage
                    && (item.transcript_message.is_none()
                        || item.content_ref.is_some()
                        || item.content_kind.is_some()
                        || item.chunk_ordinal.is_some()))
                || (item.kind == FrontendConversationKindV1::AssistantEvidence
                    && (item.transcript_message.is_some()
                        || item.content_ref.is_none()
                        || item.content_kind.is_none()
                        || item.chunk_ordinal.is_none()))
            {
                return Err(ProjectionValidationError::Invalid(
                    "invalid frontend conversation item".into(),
                ));
            }
        }
        if self
            .context
            .items
            .iter()
            .enumerate()
            .any(|(ordinal, item)| item.ordinal as usize != ordinal)
        {
            return Err(ProjectionValidationError::Invalid(
                "frontend context ordinals are not contiguous".into(),
            ));
        }
        if canonical_json_bytes(self)?.len() > MAX_OUTPUT_BYTES {
            return Err(ProjectionValidationError::Invalid(
                "frontend snapshot exceeds 16 MiB".into(),
            ));
        }
        Ok(())
    }
}

impl CompactionCheckpointV1 {
    pub(crate) fn validate(&self) -> ProjectionResult<()> {
        let state_valid = match self.compaction_state {
            CompactionStateV1::Never => {
                self.active_compaction.is_none() && self.last_terminal.is_none()
            }
            CompactionStateV1::InProgress => self.active_compaction.is_some(),
            CompactionStateV1::Applied => matches!(
                self.last_terminal,
                Some(LastCompactionTerminalV1 {
                    terminal: CompactionTerminalV1::Applied,
                    ..
                })
            ),
            CompactionStateV1::Abandoned => matches!(
                self.last_terminal,
                Some(LastCompactionTerminalV1 {
                    terminal: CompactionTerminalV1::Abandoned,
                    ..
                })
            ),
            CompactionStateV1::Idle => self.active_compaction.is_none(),
        };
        if self.checkpoint_schema_version != 1 || !state_valid {
            return Err(ProjectionValidationError::Invalid(
                "invalid compaction checkpoint state".into(),
            ));
        }
        if canonical_json_bytes(self)?.len() > MAX_OUTPUT_BYTES {
            return Err(ProjectionValidationError::Invalid(
                "compaction checkpoint exceeds 16 MiB".into(),
            ));
        }
        Ok(())
    }
}
