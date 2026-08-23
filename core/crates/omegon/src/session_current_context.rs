//! Synchronous exact-frontier model context derived only from strict authority replay.

use base64::Engine as _;
use uuid::Uuid;

use crate::{
    session_authority::{
        ContextSourceKind, ModelContextItem, ModelContextSourceKind, ModelRequestPrepared,
        ProjectionClass, SessionFactPayload,
    },
    session_replay::{AuthorityFrontier, RestrictedContinuityAuthorization, SessionReplay},
    surfaces::session::{ProjectionExactnessV1, canonical_json_bytes, canonical_sha256},
};

#[derive(Debug, Clone)]
pub(crate) struct CurrentContextDraftItemV1 {
    pub(crate) message: crate::bridge::LlmMessage,
    pub(crate) provenance: crate::session_authority::ModelContextProvenance,
}

#[derive(Debug, Clone)]
pub(crate) struct CurrentContextDraftV1 {
    pub(crate) frontier: AuthorityFrontier,
    pub(crate) exactness: ProjectionExactnessV1,
    pub(crate) items: Vec<CurrentContextDraftItemV1>,
    pub(crate) legacy_base: Option<LegacyContextBaseV1>,
    pub(crate) continuity_refs: Vec<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LegacyContextBaseV1 {
    pub(crate) context_source_id: Uuid,
    pub(crate) source_identity: String,
}

#[derive(Debug, Clone)]
pub(crate) struct CurrentContextItemV1 {
    pub(crate) item: ModelContextItem,
    pub(crate) model_visible_bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub(crate) struct CurrentContextViewV1 {
    pub(crate) frontier: AuthorityFrontier,
    pub(crate) exactness: ProjectionExactnessV1,
    pub(crate) request_id: Uuid,
    pub(crate) context_manifest_id: String,
    pub(crate) items: Vec<CurrentContextItemV1>,
    pub(crate) legacy_base: Option<LegacyContextBaseV1>,
    continuity_refs: Vec<Uuid>,
}

#[derive(Debug, Clone)]
pub(crate) enum CurrentContextReadV1 {
    ExactFull(CurrentContextViewV1),
    ExactSuffix(CurrentContextViewV1),
    LegacyUnavailable,
    SessionlessUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum CurrentContextError {
    #[error("current context is unavailable for legacy semantic lineage")]
    LegacyUnavailable,
    #[error("exact current context requires model.request_prepared at the captured frontier")]
    NoPreparedRequestAtFrontier,
    #[error("current context validation failed: {0}")]
    Invalid(String),
}

impl CurrentContextViewV1 {
    /// Reduces the exact context captured by the terminal request-prepared fact.
    /// Dispatch uses the pre-request draft reducer and verifies the durable capture here.
    pub(crate) fn derive(
        replay: &SessionReplay,
    ) -> Result<CurrentContextReadV1, CurrentContextError> {
        match replay.lineage_level() {
            crate::session_authority::AuthorityLineageLevel::LegacyOnly => {
                return Ok(CurrentContextReadV1::LegacyUnavailable);
            }
            crate::session_authority::AuthorityLineageLevel::Mixed
            | crate::session_authority::AuthorityLineageLevel::FullSpine => {}
        }
        let terminal = replay
            .records()
            .last()
            .ok_or(CurrentContextError::NoPreparedRequestAtFrontier)?;
        let SessionFactPayload::ModelRequestPrepared(prepared) = terminal.payload() else {
            return Err(CurrentContextError::NoPreparedRequestAtFrontier);
        };
        let view = Self::from_prepared(replay, prepared)?;
        Ok(match view.exactness {
            ProjectionExactnessV1::ExactFull => CurrentContextReadV1::ExactFull(view),
            ProjectionExactnessV1::ExactSuffix => CurrentContextReadV1::ExactSuffix(view),
            ProjectionExactnessV1::None => unreachable!("legacy lineage returned above"),
        })
    }

    pub(crate) fn compare_prepared_capture(
        replay: &SessionReplay,
        prepared: &ModelRequestPrepared,
    ) -> Result<Self, CurrentContextError> {
        let view = Self::from_prepared(replay, prepared)?;
        let actual = canonical_json_bytes(
            &view
                .items
                .iter()
                .map(|item| item.item.clone())
                .collect::<Vec<_>>(),
        )
        .map_err(invalid)?;
        let captured = canonical_json_bytes(&prepared.context_items).map_err(invalid)?;
        if actual != captured || canonical_sha256(&actual) != prepared.context_manifest_id {
            return Err(CurrentContextError::Invalid(
                "derived context does not byte-match request preparation manifest".into(),
            ));
        }
        Ok(view)
    }

    pub(crate) fn authorize_continuity(
        &self,
        replay: &SessionReplay,
        continuity_id: Uuid,
        serving_provider_id: &str,
        serving_model_id: &str,
        provider_generation_id: &str,
    ) -> Result<RestrictedContinuityAuthorization, CurrentContextError> {
        if !self.continuity_refs.contains(&continuity_id) {
            return Err(CurrentContextError::Invalid(
                "continuity is absent from current-context lineage".into(),
            ));
        }
        replay
            .authorize_restricted_continuity(
                self.request_id,
                continuity_id,
                serving_provider_id,
                serving_model_id,
                provider_generation_id,
            )
            .map_err(invalid)
    }

    fn from_prepared(
        replay: &SessionReplay,
        prepared: &ModelRequestPrepared,
    ) -> Result<Self, CurrentContextError> {
        let terminal = replay
            .records()
            .last()
            .ok_or(CurrentContextError::NoPreparedRequestAtFrontier)?;
        if terminal.frontier() != replay.frontier()
            || !matches!(
                terminal.payload(),
                SessionFactPayload::ModelRequestPrepared(value)
                    if value.request_id == prepared.request_id && value == prepared
            )
        {
            return Err(CurrentContextError::NoPreparedRequestAtFrontier);
        }
        if prepared
            .context_items
            .iter()
            .enumerate()
            .any(|(ordinal, item)| item.ordinal as usize != ordinal)
        {
            return Err(CurrentContextError::Invalid(
                "context item ordinals are not contiguous".into(),
            ));
        }
        let manifest = canonical_json_bytes(&prepared.context_items).map_err(invalid)?;
        if canonical_sha256(&manifest) != prepared.context_manifest_id {
            return Err(CurrentContextError::Invalid(
                "context manifest digest disagrees with ordered items".into(),
            ));
        }

        let mut items = Vec::with_capacity(prepared.context_items.len());
        let mut legacy_base = None;
        for item in &prepared.context_items {
            if item.content_ref.projection_class() != ProjectionClass::Default {
                return Err(CurrentContextError::Invalid(
                    "restricted continuity entered default model context".into(),
                ));
            }
            validate_provenance(replay, item, &mut legacy_base)?;
            let bytes = replay
                .read_default_content(&item.content_ref)
                .map_err(invalid)?;
            if bytes.is_empty() {
                return Err(CurrentContextError::Invalid(
                    "model-visible context content is empty".into(),
                ));
            }
            items.push(CurrentContextItemV1 {
                item: item.clone(),
                model_visible_bytes: bytes,
            });
        }

        let exactness = match replay.lineage_level() {
            crate::session_authority::AuthorityLineageLevel::FullSpine => {
                if legacy_base.is_some() {
                    return Err(CurrentContextError::Invalid(
                        "full lineage cannot contain a legacy compatibility base".into(),
                    ));
                }
                ProjectionExactnessV1::ExactFull
            }
            crate::session_authority::AuthorityLineageLevel::Mixed => {
                if legacy_base.is_none() {
                    return Err(CurrentContextError::Invalid(
                        "mixed context has no labeled materialized legacy base".into(),
                    ));
                }
                ProjectionExactnessV1::ExactSuffix
            }
            crate::session_authority::AuthorityLineageLevel::LegacyOnly => {
                return Err(CurrentContextError::LegacyUnavailable);
            }
        };
        Ok(Self {
            frontier: replay.frontier().clone(),
            exactness,
            request_id: prepared.request_id,
            context_manifest_id: prepared.context_manifest_id.clone(),
            items,
            legacy_base,
            continuity_refs: prepared.continuity_refs.clone(),
        })
    }
}

impl CurrentContextDraftV1 {
    pub(crate) fn derive(replay: &SessionReplay) -> Result<Self, CurrentContextError> {
        if replay.lineage_level() == crate::session_authority::AuthorityLineageLevel::LegacyOnly {
            return Err(CurrentContextError::LegacyUnavailable);
        }

        let latest_prepared = replay.records().iter().rev().find_map(|record| {
            let SessionFactPayload::ModelRequestPrepared(prepared) = record.payload() else {
                return None;
            };
            Some((record.frontier().sequence(), prepared))
        });
        let latest_applied = replay.records().iter().rev().find_map(|record| {
            let SessionFactPayload::CompactionApplied(applied) = record.payload() else {
                return None;
            };
            Some((record.frontier().sequence(), applied))
        });

        let mut items = Vec::new();
        let mut legacy_base = None;
        let baseline_sequence = if latest_applied.is_some_and(|(sequence, _)| {
            latest_prepared.is_none_or(|(prepared_sequence, _)| sequence > prepared_sequence)
        }) {
            let (sequence, applied) = latest_applied.expect("applied baseline checked");
            append_applied_baseline(replay, applied, &mut items)?;
            sequence
        } else if let Some((sequence, prepared)) = latest_prepared {
            append_prepared_baseline(replay, prepared, &mut items, &mut legacy_base)?;
            sequence
        } else {
            append_legacy_base(replay, &mut items, &mut legacy_base)?;
            0
        };

        append_semantic_suffix(replay, baseline_sequence, &mut items)?;
        let exactness = match replay.lineage_level() {
            crate::session_authority::AuthorityLineageLevel::FullSpine => {
                if legacy_base.is_some() {
                    return Err(CurrentContextError::Invalid(
                        "full lineage cannot contain a legacy compatibility base".into(),
                    ));
                }
                ProjectionExactnessV1::ExactFull
            }
            crate::session_authority::AuthorityLineageLevel::Mixed => {
                if legacy_base.is_none() {
                    return Err(CurrentContextError::Invalid(
                        "mixed context has no labeled materialized legacy base".into(),
                    ));
                }
                ProjectionExactnessV1::ExactSuffix
            }
            crate::session_authority::AuthorityLineageLevel::LegacyOnly => unreachable!(),
        };
        let continuity_refs = latest_continuity_refs(replay);
        Ok(Self {
            frontier: replay.frontier().clone(),
            exactness,
            items,
            legacy_base,
            continuity_refs,
        })
    }

    pub(crate) fn messages(&self) -> Vec<crate::bridge::LlmMessage> {
        self.items.iter().map(|item| item.message.clone()).collect()
    }
}

fn append_prepared_baseline(
    replay: &SessionReplay,
    prepared: &ModelRequestPrepared,
    output: &mut Vec<CurrentContextDraftItemV1>,
    legacy_base: &mut Option<LegacyContextBaseV1>,
) -> Result<(), CurrentContextError> {
    for item in prepared
        .context_items
        .iter()
        .filter(|item| item.role != crate::session_authority::ModelContextRole::System)
    {
        validate_provenance(replay, item, legacy_base)?;
        let bytes = replay
            .read_default_content(&item.content_ref)
            .map_err(invalid)?;
        let message = serde_json::from_slice(&bytes).map_err(invalid)?;
        output.push(CurrentContextDraftItemV1 {
            message,
            provenance: item.provenance.clone(),
        });
    }
    Ok(())
}

fn append_legacy_base(
    replay: &SessionReplay,
    output: &mut Vec<CurrentContextDraftItemV1>,
    legacy_base: &mut Option<LegacyContextBaseV1>,
) -> Result<(), CurrentContextError> {
    for record in replay.records() {
        let SessionFactPayload::ContextSourceMaterialized(source) = record.payload() else {
            continue;
        };
        if source.source_kind != ContextSourceKind::ContributionContext
            || !source.source_identity.starts_with("legacy-")
        {
            continue;
        }
        if legacy_base.is_some() {
            return Err(CurrentContextError::Invalid(
                "mixed context contains more than one legacy compatibility base".into(),
            ));
        }
        let bytes = replay
            .read_default_content(&source.content_ref)
            .map_err(invalid)?;
        let message = serde_json::from_slice(&bytes).map_err(invalid)?;
        let generation = source.owner_generation_id.clone();
        output.push(CurrentContextDraftItemV1 {
            message,
            provenance: crate::session_authority::ModelContextProvenance {
                source_kind: ModelContextSourceKind::ContributionContext,
                source_event_id: Some(record.frontier().event_id()),
                source_identity: Some(source.context_source_id.to_string()),
                owner_id: Some(source.owner_id.clone()),
                owner_generation_id: Some(generation),
            },
        });
        *legacy_base = Some(LegacyContextBaseV1 {
            context_source_id: source.context_source_id,
            source_identity: source.source_identity.clone(),
        });
    }
    Ok(())
}

fn append_applied_baseline(
    replay: &SessionReplay,
    applied: &crate::session_authority::CompactionApplied,
    output: &mut Vec<CurrentContextDraftItemV1>,
) -> Result<(), CurrentContextError> {
    let summary_record = replay
        .records()
        .iter()
        .find(|record| {
            matches!(record.payload(), SessionFactPayload::CompactionSummaryCommitted(summary)
            if summary.compaction_id == applied.compaction_id
                && summary.compaction_summary_id == applied.compaction_summary_id)
        })
        .ok_or_else(|| {
            CurrentContextError::Invalid("applied compaction summary is absent".into())
        })?;
    let SessionFactPayload::CompactionSummaryCommitted(summary) = summary_record.payload() else {
        unreachable!()
    };
    if summary.replacement_manifest_id != applied.replacement_manifest_id {
        return Err(CurrentContextError::Invalid(
            "applied compaction replacement manifest is inconsistent".into(),
        ));
    }
    for replacement in &summary.replacement_items {
        let (message, provenance) = match replacement.source_kind {
            crate::session_authority::CompactionReplacementSourceKind::CompactionSummary => {
                let bytes = replay
                    .read_default_content(&replacement.content_ref)
                    .map_err(invalid)?;
                let text = std::str::from_utf8(&bytes).map_err(invalid)?;
                (
                    crate::bridge::LlmMessage::User {
                        content: format!(
                            "[Previous conversation summary]\n{text}\n[End summary - continue from here]"
                        ),
                        images: Vec::new(),
                    },
                    crate::session_authority::ModelContextProvenance {
                        source_kind: ModelContextSourceKind::CompactionSummary,
                        source_event_id: Some(summary_record.frontier().event_id()),
                        source_identity: Some(summary.compaction_summary_id.to_string()),
                        owner_id: None,
                        owner_generation_id: None,
                    },
                )
            }
            crate::session_authority::CompactionReplacementSourceKind::Retained => {
                retained_message(replay, replacement)?
            }
        };
        output.push(CurrentContextDraftItemV1 {
            message,
            provenance,
        });
    }
    Ok(())
}

fn retained_message(
    replay: &SessionReplay,
    replacement: &crate::session_authority::CompactionReplacementItem,
) -> Result<
    (
        crate::bridge::LlmMessage,
        crate::session_authority::ModelContextProvenance,
    ),
    CurrentContextError,
> {
    let source = replay
        .records()
        .iter()
        .find(|record| record.frontier().event_id() == replacement.source_event_id)
        .ok_or_else(|| CurrentContextError::Invalid("retained context source is absent".into()))?;
    let SessionFactPayload::ModelRequestPrepared(prepared) = source.payload() else {
        return Err(CurrentContextError::Invalid(
            "retained context source is not request preparation".into(),
        ));
    };
    let (_, ordinal) = replacement
        .source_identity
        .rsplit_once(':')
        .ok_or_else(|| {
            CurrentContextError::Invalid("retained context identity is invalid".into())
        })?;
    let ordinal = ordinal.parse::<u32>().map_err(invalid)?;
    let item = prepared
        .context_items
        .iter()
        .find(|item| item.ordinal == ordinal && item.content_ref == replacement.content_ref)
        .ok_or_else(|| CurrentContextError::Invalid("retained context item is absent".into()))?;
    let bytes = replay
        .read_default_content(&replacement.content_ref)
        .map_err(invalid)?;
    Ok((
        serde_json::from_slice(&bytes).map_err(invalid)?,
        item.provenance.clone(),
    ))
}

fn append_semantic_suffix(
    replay: &SessionReplay,
    after_sequence: u64,
    output: &mut Vec<CurrentContextDraftItemV1>,
) -> Result<(), CurrentContextError> {
    for record in replay
        .records()
        .iter()
        .filter(|record| record.frontier().sequence() > after_sequence)
    {
        let (message, source_kind, source_identity) = match record.payload() {
            SessionFactPayload::PromptAdmitted(prompt) => (
                prompt_message(replay, prompt)?,
                ModelContextSourceKind::Prompt,
                prompt.prompt_id,
            ),
            SessionFactPayload::AssistantMessageCommitted(commit)
                if committed_attempt_is_usable(replay, commit) =>
            {
                (
                    assistant_message(replay, commit)?,
                    ModelContextSourceKind::AssistantMessage,
                    commit.message_id,
                )
            }
            SessionFactPayload::ToolResultRecorded(result) => (
                tool_result_message(replay, result)?,
                ModelContextSourceKind::ToolResult,
                result.tool_result_id,
            ),
            _ => continue,
        };
        output.push(CurrentContextDraftItemV1 {
            message,
            provenance: crate::session_authority::ModelContextProvenance {
                source_kind,
                source_event_id: Some(record.frontier().event_id()),
                source_identity: Some(source_identity.to_string()),
                owner_id: None,
                owner_generation_id: None,
            },
        });
    }
    Ok(())
}

fn prompt_message(
    replay: &SessionReplay,
    prompt: &crate::session_authority::PromptAdmitted,
) -> Result<crate::bridge::LlmMessage, CurrentContextError> {
    let mut images = Vec::new();
    for attachment in &prompt.content.attachments {
        if !attachment.media_type.starts_with("image/") {
            return Err(CurrentContextError::Invalid(
                "non-image prompt attachment has no provider-context encoding".into(),
            ));
        }
        images.push(crate::bridge::ImageAttachment {
            data: base64::engine::general_purpose::STANDARD
                .encode(replay.read_attachment(attachment).map_err(invalid)?),
            media_type: attachment.media_type.clone(),
            source_path: None,
        });
    }
    Ok(crate::bridge::LlmMessage::User {
        content: prompt.content.text.clone(),
        images,
    })
}

fn assistant_message(
    replay: &SessionReplay,
    commit: &crate::session_authority::AssistantMessageCommitted,
) -> Result<crate::bridge::LlmMessage, CurrentContextError> {
    let mut text = Vec::new();
    let mut thinking = Vec::new();
    for content in &commit.content {
        let mut value = String::new();
        for content_ref in &content.chunk_refs {
            value.push_str(
                std::str::from_utf8(&replay.read_default_content(content_ref).map_err(invalid)?)
                    .map_err(invalid)?,
            );
        }
        match content.content_kind {
            crate::session_authority::AssistantContentKind::Text => text.push(value),
            crate::session_authority::AssistantContentKind::Thinking => thinking.push(value),
        }
    }
    let mut calls = replay
        .records()
        .iter()
        .filter_map(|record| {
            let SessionFactPayload::ToolCallRecorded(call) = record.payload() else {
                return None;
            };
            (call.request_id == commit.request_id).then_some(call)
        })
        .collect::<Vec<_>>();
    calls.sort_by_key(|call| call.call_ordinal);
    let tool_calls = calls
        .into_iter()
        .map(|call| {
            let arguments = serde_json::from_slice(
                &replay
                    .read_default_content(&call.arguments_ref)
                    .map_err(invalid)?,
            )
            .map_err(invalid)?;
            Ok(crate::bridge::WireToolCall {
                id: call.call_id.clone(),
                name: call.invocation_name.clone(),
                arguments,
            })
        })
        .collect::<Result<Vec<_>, CurrentContextError>>()?;
    if tool_calls.len() != commit.tool_call_count as usize {
        return Err(CurrentContextError::Invalid(
            "assistant tool-call manifest is incomplete".into(),
        ));
    }
    Ok(crate::bridge::LlmMessage::Assistant {
        text,
        thinking,
        tool_calls,
        raw: None,
    })
}

fn tool_result_message(
    replay: &SessionReplay,
    result: &crate::session_authority::ToolResultRecorded,
) -> Result<crate::bridge::LlmMessage, CurrentContextError> {
    let call = replay
        .records()
        .iter()
        .find_map(|record| {
            let SessionFactPayload::ToolCallRecorded(call) = record.payload() else {
                return None;
            };
            (call.tool_call_id == result.tool_call_id).then_some(call)
        })
        .ok_or_else(|| CurrentContextError::Invalid("tool result call is absent".into()))?;
    let blocks: Vec<omegon_traits::ContentBlock> = serde_json::from_slice(
        &replay
            .read_default_content(&result.content_ref)
            .map_err(invalid)?,
    )
    .map_err(invalid)?;
    let mut text = Vec::new();
    let mut images = Vec::new();
    for block in &blocks {
        match block {
            omegon_traits::ContentBlock::Text { text: value } => text.push(value.clone()),
            omegon_traits::ContentBlock::Image { media_type, .. } => {
                if let Some(image) = crate::bridge::ImageAttachment::from_content_block(block, None)
                {
                    images.push(image);
                }
                text.push(format!("[image output: {media_type}]"));
            }
        }
    }
    Ok(crate::bridge::LlmMessage::ToolResult {
        call_id: result.call_id.clone(),
        tool_name: call.invocation_name.clone(),
        content: text.join("\n"),
        images,
        is_error: result.is_error,
        args_summary: None,
    })
}

fn latest_continuity_refs(replay: &SessionReplay) -> Vec<Uuid> {
    let latest_request = replay.records().iter().rev().find_map(|record| {
        let SessionFactPayload::AssistantMessageCommitted(commit) = record.payload() else {
            return None;
        };
        committed_attempt_is_usable(replay, commit).then_some(commit.request_id)
    });
    let Some(request_id) = latest_request else {
        return Vec::new();
    };
    replay
        .records()
        .iter()
        .filter_map(|record| {
            let SessionFactPayload::ProviderContinuityStored(value) = record.payload() else {
                return None;
            };
            (value.request_id == request_id).then_some(value.continuity_id)
        })
        .collect()
}

fn validate_provenance(
    replay: &SessionReplay,
    item: &ModelContextItem,
    legacy_base: &mut Option<LegacyContextBaseV1>,
) -> Result<(), CurrentContextError> {
    let provenance = &item.provenance;
    let event_id = provenance.source_event_id.ok_or_else(|| {
        CurrentContextError::Invalid("model context provenance has no source event".into())
    })?;
    let source = replay
        .records()
        .iter()
        .find(|record| record.frontier().event_id() == event_id)
        .ok_or_else(|| CurrentContextError::Invalid("context source event is absent".into()))?;
    if source.frontier().sequence() >= replay.frontier().sequence() {
        return Err(CurrentContextError::Invalid(
            "context source does not precede request preparation".into(),
        ));
    }
    let identity = provenance.source_identity.as_deref().ok_or_else(|| {
        CurrentContextError::Invalid("model context provenance has no source identity".into())
    })?;
    let identity_matches = |value: Uuid| identity == value.to_string();
    let valid = match (&provenance.source_kind, source.payload()) {
        (ModelContextSourceKind::Prompt, SessionFactPayload::PromptAdmitted(value)) => {
            identity_matches(value.prompt_id)
        }
        (
            ModelContextSourceKind::AssistantMessage,
            SessionFactPayload::AssistantMessageCommitted(value),
        ) => identity_matches(value.message_id) && committed_attempt_is_usable(replay, value),
        (ModelContextSourceKind::ToolResult, SessionFactPayload::ToolResultRecorded(value)) => {
            identity_matches(value.tool_result_id)
                && replay.records().iter().any(|record| {
                    matches!(record.payload(), SessionFactPayload::ToolCallRecorded(call)
                        if call.tool_call_id == value.tool_call_id && call.call_id == value.call_id)
                })
        }
        (
            ModelContextSourceKind::CompactionSummary,
            SessionFactPayload::CompactionSummaryCommitted(value),
        ) => {
            identity_matches(value.compaction_summary_id)
                && replay.records().iter().any(|record| {
                    matches!(record.payload(), SessionFactPayload::CompactionApplied(applied)
                    if applied.compaction_id == value.compaction_id
                        && applied.compaction_summary_id == value.compaction_summary_id)
                })
        }
        (
            ModelContextSourceKind::SystemInstruction
            | ModelContextSourceKind::DeveloperInstruction
            | ModelContextSourceKind::ContributionContext,
            SessionFactPayload::ContextSourceMaterialized(value),
        ) => {
            let kind_matches = matches!(
                (&provenance.source_kind, &value.source_kind),
                (
                    ModelContextSourceKind::SystemInstruction,
                    ContextSourceKind::SystemInstruction
                ) | (
                    ModelContextSourceKind::DeveloperInstruction,
                    ContextSourceKind::DeveloperInstruction
                ) | (
                    ModelContextSourceKind::ContributionContext,
                    ContextSourceKind::ContributionContext
                )
            );
            let matched = identity_matches(value.context_source_id)
                && kind_matches
                && value.content_ref == item.content_ref
                && provenance.owner_id.as_deref() == Some(&value.owner_id)
                && provenance.owner_generation_id.as_ref() == Some(&value.owner_generation_id);
            if matched
                && provenance.source_kind == ModelContextSourceKind::ContributionContext
                && value.source_identity.starts_with("legacy-")
            {
                if legacy_base.is_some() {
                    return Err(CurrentContextError::Invalid(
                        "mixed context contains more than one legacy compatibility base".into(),
                    ));
                }
                *legacy_base = Some(LegacyContextBaseV1 {
                    context_source_id: value.context_source_id,
                    source_identity: value.source_identity.clone(),
                });
            }
            matched
        }
        _ => false,
    };
    if !valid {
        return Err(CurrentContextError::Invalid(
            "context provenance does not match its authoritative source fact".into(),
        ));
    }
    Ok(())
}

fn committed_attempt_is_usable(
    replay: &SessionReplay,
    commit: &crate::session_authority::AssistantMessageCommitted,
) -> bool {
    let failed = replay.records().iter().any(|record| {
        matches!(record.payload(), SessionFactPayload::ModelResponseAttemptFailed(value)
            if value.request_id == commit.request_id
                && value.response_attempt_ordinal == commit.response_attempt_ordinal)
    });
    let abandoned = replay.records().iter().any(|record| {
        matches!(record.payload(), SessionFactPayload::ModelRequestClosed(value)
            if value.request_id == commit.request_id
                && value.outcome == crate::session_authority::ModelRequestOutcome::Abandoned)
    });
    !failed && !abandoned
}

fn invalid(error: impl std::fmt::Display) -> CurrentContextError {
    CurrentContextError::Invalid(error.to_string())
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use omegon_traits::{
        RuntimeCompositionGenerationId, RuntimeContributionGenerationId, RuntimeContributionId,
    };

    use super::*;
    use crate::session_authority::{
        ActorIdentity, ContextSourceMaterialized, ModelContextProvenance, ModelContextRole,
        ModelSchemaSet, PromptAdmitted, PromptContent, QueueMode, SessionAuthority, StepStarted,
    };

    const FIXTURES: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/session-semantic-v1"
    );
    const SESSION_ID: &str = "fixture-session";
    const STREAM_ID: Uuid = Uuid::from_u128(0x10000000_0000_4000_8000_000000000001);

    #[test]
    fn mixed_capture_requires_and_preserves_labeled_legacy_base() {
        let directory = tempfile::tempdir().unwrap();
        let snapshot = directory.path().join("session.json");
        fs::write(
            directory.path().join("session.authority.jsonl"),
            fs::read(Path::new(FIXTURES).join("mixed-legacy-full.authority.jsonl")).unwrap(),
        )
        .unwrap();
        let mut authority = SessionAuthority::open(
            &snapshot,
            SESSION_ID,
            "fixture-workspace",
            "composition:fixture",
            ActorIdentity {
                principal: "operator".into(),
                ingress: "test".into(),
            },
            "2026-08-21T00:00:00Z",
        )
        .unwrap();
        let prompt_id = Uuid::new_v4();
        authority
            .admit_prompt(
                Uuid::new_v4(),
                "2026-08-21T00:00:01Z",
                PromptAdmitted {
                    submission_id: Uuid::new_v4(),
                    prompt_id,
                    principal: "operator".into(),
                    ingress: "test".into(),
                    queue_mode: QueueMode::UntilReady,
                    content: PromptContent {
                        text: "resume mixed session".into(),
                        attachments: Vec::new(),
                    },
                    metadata: serde_json::json!({}),
                },
            )
            .unwrap();
        let turn_id = Uuid::new_v4();
        authority
            .start_turn(Uuid::new_v4(), "2026-08-21T00:00:02Z", turn_id, prompt_id)
            .unwrap();
        let step_id = Uuid::new_v4();
        authority
            .start_step(
                Uuid::new_v4(),
                "2026-08-21T00:00:03Z",
                StepStarted {
                    step_id,
                    turn_id,
                    step_ordinal: 0,
                },
            )
            .unwrap();
        let legacy_bytes =
            crate::surfaces::session::canonical_json_bytes(&crate::bridge::LlmMessage::User {
                content: "legacy compatibility transcript".into(),
                images: Vec::new(),
            })
            .unwrap();
        let content_ref = authority
            .write_content(&legacy_bytes, "application/json", ProjectionClass::Default)
            .unwrap();
        let source_id = Uuid::new_v4();
        let source_event_id = authority
            .materialize_context_source(
                Uuid::new_v4(),
                "2026-08-21T00:00:04Z",
                ContextSourceMaterialized {
                    context_source_id: source_id,
                    source_kind: ContextSourceKind::ContributionContext,
                    source_identity: "legacy-compatibility-base".into(),
                    owner_id: "compatibility:session-resume".into(),
                    owner_generation_id: RuntimeContributionGenerationId::new(
                        "session-resume:legacy-base-v1",
                    )
                    .unwrap(),
                    content_ref: content_ref.clone(),
                },
            )
            .unwrap();
        let items = vec![ModelContextItem {
            ordinal: 0,
            role: ModelContextRole::User,
            content_ref,
            provenance: ModelContextProvenance {
                source_kind: ModelContextSourceKind::ContributionContext,
                source_event_id: Some(source_event_id),
                source_identity: Some(source_id.to_string()),
                owner_id: Some("compatibility:session-resume".into()),
                owner_generation_id: Some(
                    RuntimeContributionGenerationId::new("session-resume:legacy-base-v1").unwrap(),
                ),
            },
        }];
        let schema_set = ModelSchemaSet {
            schema_set_version: 1,
            composition_generation_id: RuntimeCompositionGenerationId::new("composition:fixture")
                .unwrap(),
            normalizer_contribution_id: RuntimeContributionId::new("system:normalizer").unwrap(),
            normalizer_generation_id: RuntimeContributionGenerationId::new("normalizer:v1")
                .unwrap(),
            schemas: Vec::new(),
        };
        let prepared = ModelRequestPrepared {
            request_id: Uuid::new_v4(),
            step_id,
            turn_id,
            request_ordinal: 0,
            purpose: crate::session_authority::ModelRequestPurpose::Initial,
            replaces_request_id: None,
            continuity_refs: Vec::new(),
            context_manifest_id: canonical_sha256(&canonical_json_bytes(&items).unwrap()),
            context_items: items,
            schema_set_id: canonical_sha256(&canonical_json_bytes(&schema_set).unwrap()),
            schema_set,
        };
        authority
            .prepare_model_request(Uuid::new_v4(), "2026-08-21T00:00:05Z", prepared.clone())
            .unwrap();
        drop(authority);

        let replay = SessionReplay::replay_prefix(
            &snapshot,
            SESSION_ID,
            STREAM_ID,
            crate::session_replay::ReplayEnd::EndOfStream,
        )
        .unwrap();
        let view = CurrentContextViewV1::compare_prepared_capture(&replay, &prepared).unwrap();
        assert_eq!(view.exactness, ProjectionExactnessV1::ExactSuffix);
        assert_eq!(
            view.legacy_base.unwrap().source_identity,
            "legacy-compatibility-base"
        );
        let draft = CurrentContextDraftV1::derive(&replay).unwrap();
        assert_eq!(draft.exactness, ProjectionExactnessV1::ExactSuffix);
        assert_eq!(draft.items.len(), 1, "legacy base must enter context once");
        assert_eq!(
            draft.legacy_base.unwrap().source_identity,
            "legacy-compatibility-base"
        );
    }
}
