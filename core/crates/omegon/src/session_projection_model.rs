//! Immutable semantic read model derived from one successful authority replay.

use std::collections::{BTreeMap, BTreeSet};

use uuid::Uuid;

use crate::{
    session_authority::{
        AssistantContentAppended, CompactionAbandoned, CompactionApplied, CompactionStarted,
        ModelRequestOutcome, ModelRequestPrepared, RouteLeaseRecorded, SessionFactPayload,
        StepOutcome, StepStarted, ToolCallRecorded, ToolResultRecorded, TurnOutcome, TurnStarted,
    },
    session_blob_store::{ContentRef, ProjectionClass},
    session_replay::SessionReplay,
    surfaces::session::{
        ActiveCompactionV1, ActiveTurnStatusV1, ChunkManifestEntryV1, ChunkManifestV1,
        CompactionCheckpointV1, CompactionStateV1, CompactionTerminalV1, DigestAlgorithmV1,
        FrontendActiveTurnV1, FrontendContextV1, FrontendConversationItemV1,
        FrontendConversationKindV1, FrontendConversationStatusV1, FrontendSnapshotV1,
        FullSessionExportV1, LastCompactionTerminalV1, MAX_CHUNK_BYTES, MAX_CHUNK_ITEMS,
        ProjectionAvailabilityV1, ProjectionChunkItemsV1, ProjectionChunkV1, ProjectionEnvelopeV1,
        ProjectionExactnessV1, ProjectionLineageV1, ProjectionPayloadV1, ProjectionResult,
        ProjectionScopeV1, ProjectionUnavailableReasonV1, ProjectionUnavailableV1,
        ProjectionValidationError, ProjectorIdV1, ProviderRequestInputV1, QueuedPromptV1,
        SourceEventV1, TranscriptContentV1, TranscriptMessageKindV1, TranscriptMessageV1,
        TranscriptRoleV1, TranscriptStatusV1,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectionRequestState {
    PreparedUnjoined,
    JoinedOpen,
    Closed(ModelRequestOutcome),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectionAttemptState {
    Open,
    Failed,
    Committed,
    Abandoned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectionToolResultState {
    Missing,
    Denied,
    NotDispatched,
    Settled,
    UnknownCompletion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectionLifecycleState {
    Active,
    Completed,
    Abnormal,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ProjectionTurn {
    pub(crate) start: TurnStarted,
    pub(crate) source_event: SourceEventV1,
    pub(crate) state: ProjectionLifecycleState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectionStep {
    pub(crate) start: StepStarted,
    pub(crate) source_event: SourceEventV1,
    pub(crate) state: ProjectionLifecycleState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectionRequest {
    pub(crate) request_id: Uuid,
    pub(crate) step_id: Uuid,
    pub(crate) turn_id: Uuid,
    pub(crate) source_event: SourceEventV1,
    pub(crate) state: ProjectionRequestState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectionAttempt {
    pub(crate) request_id: Uuid,
    pub(crate) response_attempt_ordinal: u32,
    pub(crate) state: ProjectionAttemptState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectionToolCall {
    pub(crate) call: ToolCallRecorded,
    pub(crate) source_event: SourceEventV1,
    pub(crate) result_state: ProjectionToolResultState,
    pub(crate) result: Option<ToolResultRecorded>,
}

#[derive(Debug, Clone)]
struct EventValue<T> {
    source: SourceEventV1,
    value: T,
}

pub(crate) type ProjectedChunksV1 = (ChunkManifestV1, Vec<(ProjectionChunkV1, Vec<u8>)>);

/// One immutable, ordered semantic index shared by all schema-v1 projectors.
#[derive(Debug, Clone)]
pub(crate) struct SessionProjectionModel {
    session_id: String,
    stream_id: Uuid,
    lineage: ProjectionLineageV1,
    boundary: Option<SourceEventV1>,
    frontier: SourceEventV1,
    pub(crate) source_events: Vec<SourceEventV1>,
    pub(crate) turns: Vec<ProjectionTurn>,
    pub(crate) steps: Vec<ProjectionStep>,
    pub(crate) requests: Vec<ProjectionRequest>,
    pub(crate) attempts: Vec<ProjectionAttempt>,
    pub(crate) tool_calls: Vec<ProjectionToolCall>,
    provider_inputs: Vec<ProviderRequestInputV1>,
    transcript: Vec<TranscriptMessageV1>,
    frontend: FrontendSnapshotV1,
    compaction: CompactionCheckpointV1,
}

impl SessionProjectionModel {
    pub(crate) fn from_replay(replay: &SessionReplay) -> ProjectionResult<Self> {
        let lineage = match replay.lineage_level() {
            crate::session_authority::AuthorityLineageLevel::LegacyOnly => {
                ProjectionLineageV1::Legacy
            }
            crate::session_authority::AuthorityLineageLevel::Mixed => ProjectionLineageV1::Mixed,
            crate::session_authority::AuthorityLineageLevel::FullSpine => ProjectionLineageV1::Full,
        };
        let boundary = (lineage == ProjectionLineageV1::Mixed)
            .then(|| {
                replay
                    .first_full_spine_boundary()
                    .map(|frontier| source_from_frontier(&frontier))
            })
            .flatten();
        if lineage == ProjectionLineageV1::Mixed && boundary.is_none() {
            return Err(ProjectionValidationError::Invalid(
                "mixed replay has no full-spine boundary".into(),
            ));
        }
        let minimum_sequence = boundary.as_ref().map_or(1, |value| value.sequence);
        let frontier = source_from_frontier(replay.frontier());
        let records = replay
            .records()
            .iter()
            .filter(|record| record.frontier().sequence() >= minimum_sequence)
            .collect::<Vec<_>>();
        let source_events = records
            .iter()
            .map(|record| source_from_frontier(record.frontier()))
            .collect::<Vec<_>>();

        if lineage == ProjectionLineageV1::Legacy {
            return Ok(Self {
                session_id: replay.frontier().session_id().into(),
                stream_id: replay.frontier().stream_id(),
                lineage,
                boundary: None,
                frontier,
                source_events,
                turns: Vec::new(),
                steps: Vec::new(),
                requests: Vec::new(),
                attempts: Vec::new(),
                tool_calls: Vec::new(),
                provider_inputs: Vec::new(),
                transcript: Vec::new(),
                frontend: empty_frontend(),
                compaction: empty_checkpoint(),
            });
        }

        let mut prompts = BTreeMap::new();
        let mut turn_starts = BTreeMap::new();
        let mut step_starts = BTreeMap::new();
        let mut interrupted_turns = BTreeSet::new();
        let mut closed_turns = BTreeSet::new();
        let mut completed_steps = BTreeSet::new();
        let mut abnormal_turns = BTreeSet::new();
        let mut abnormal_steps = BTreeSet::new();
        let mut preparations = BTreeMap::new();
        let mut request_closures = BTreeMap::new();
        let mut joins = BTreeMap::new();
        let mut leases = BTreeMap::new();
        let mut failures = BTreeMap::new();
        let mut chunks: BTreeMap<Uuid, Vec<EventValue<AssistantContentAppended>>> = BTreeMap::new();
        let mut commits = BTreeMap::new();
        let mut calls = BTreeMap::new();
        let mut results = BTreeMap::new();
        let mut compaction_starts = BTreeMap::new();
        let mut compaction_summaries = BTreeMap::new();
        let mut compaction_terminals: Vec<CompactionTerminalEvent> = Vec::new();

        for record in &records {
            let source = source_from_frontier(record.frontier());
            match record.payload() {
                SessionFactPayload::PromptAdmitted(value) => {
                    prompts.insert(
                        value.prompt_id,
                        EventValue {
                            source,
                            value: value.clone(),
                        },
                    );
                }
                SessionFactPayload::PromptRemoved(value) => {
                    prompts.remove(&value.prompt_id);
                }
                SessionFactPayload::TurnStarted(value) => {
                    turn_starts.insert(
                        value.turn_id,
                        EventValue {
                            source,
                            value: value.clone(),
                        },
                    );
                }
                SessionFactPayload::StepStarted(value) => {
                    step_starts.insert(
                        value.step_id,
                        EventValue {
                            source,
                            value: value.clone(),
                        },
                    );
                }
                SessionFactPayload::TurnInterruptionRequested(value) => {
                    interrupted_turns.insert(value.turn_id);
                }
                SessionFactPayload::TurnClosed(value) => {
                    closed_turns.insert(value.turn_id);
                    if value.outcome != TurnOutcome::Completed {
                        abnormal_turns.insert(value.turn_id);
                    }
                }
                SessionFactPayload::StepClosed(value) => {
                    if !matches!(
                        value.outcome,
                        StepOutcome::ContinueLoop | StepOutcome::TurnCompleted
                    ) {
                        abnormal_steps.insert(value.step_id);
                    } else {
                        completed_steps.insert(value.step_id);
                    }
                }
                SessionFactPayload::StepAbandoned(value) => {
                    abnormal_steps.insert(value.step_id);
                }
                SessionFactPayload::ModelRequestPrepared(value) => {
                    validate_request_default_content(replay, value)?;
                    preparations.insert(
                        value.request_id,
                        EventValue {
                            source,
                            value: value.clone(),
                        },
                    );
                }
                SessionFactPayload::RouteLeaseRecorded(value) => {
                    leases.insert(
                        value.lease_id,
                        EventValue {
                            source,
                            value: value.clone(),
                        },
                    );
                }
                SessionFactPayload::ModelRequestRouteJoined(value) => {
                    joins.insert(
                        value.request_id,
                        EventValue {
                            source,
                            value: value.clone(),
                        },
                    );
                }
                SessionFactPayload::ModelResponseAttemptFailed(value) => {
                    failures.insert((value.request_id, value.response_attempt_ordinal), source);
                }
                SessionFactPayload::AssistantContentAppended(value) => {
                    validate_default_ref(replay, &value.content_ref)?;
                    chunks
                        .entry(value.request_id)
                        .or_default()
                        .push(EventValue {
                            source,
                            value: value.clone(),
                        });
                }
                SessionFactPayload::AssistantMessageCommitted(value) => {
                    for channel in &value.content {
                        for content_ref in &channel.chunk_refs {
                            validate_default_ref(replay, content_ref)?;
                        }
                    }
                    commits.insert(
                        value.request_id,
                        EventValue {
                            source,
                            value: value.clone(),
                        },
                    );
                }
                SessionFactPayload::ToolCallRecorded(value) => {
                    validate_default_ref(replay, &value.arguments_ref)?;
                    calls.insert(
                        value.tool_call_id,
                        EventValue {
                            source,
                            value: value.clone(),
                        },
                    );
                }
                SessionFactPayload::ToolResultRecorded(value) => {
                    validate_default_ref(replay, &value.content_ref)?;
                    results.insert(
                        value.tool_call_id,
                        EventValue {
                            source,
                            value: value.clone(),
                        },
                    );
                }
                SessionFactPayload::ModelRequestClosed(value) => {
                    request_closures.insert(
                        value.request_id,
                        EventValue {
                            source,
                            value: value.clone(),
                        },
                    );
                }
                SessionFactPayload::CompactionStarted(value) => {
                    for item in value.input_items.iter().chain(&value.retained_items) {
                        validate_default_ref(replay, &item.content_ref)?;
                    }
                    compaction_starts.insert(
                        value.compaction_id,
                        EventValue {
                            source,
                            value: value.clone(),
                        },
                    );
                }
                SessionFactPayload::CompactionSummaryCommitted(value) => {
                    validate_default_ref(replay, &value.summary_ref)?;
                    for item in &value.replacement_items {
                        validate_default_ref(replay, &item.content_ref)?;
                    }
                    compaction_summaries.insert(
                        value.compaction_id,
                        EventValue {
                            source,
                            value: value.clone(),
                        },
                    );
                }
                SessionFactPayload::CompactionApplied(value)
                    if compaction_starts.contains_key(&value.compaction_id) =>
                {
                    compaction_terminals.push(CompactionTerminalEvent::Applied(EventValue {
                        source,
                        value: value.clone(),
                    }))
                }
                SessionFactPayload::CompactionAbandoned(value)
                    if compaction_starts.contains_key(&value.compaction_id) =>
                {
                    compaction_terminals.push(CompactionTerminalEvent::Abandoned(EventValue {
                        source,
                        value: value.clone(),
                    }))
                }
                _ => {}
            }
        }

        let mut requests = preparations
            .values()
            .map(|prepared| ProjectionRequest {
                request_id: prepared.value.request_id,
                step_id: prepared.value.step_id,
                turn_id: prepared.value.turn_id,
                source_event: prepared.source.clone(),
                state: request_closures
                    .get(&prepared.value.request_id)
                    .map_or_else(
                        || {
                            if joins.contains_key(&prepared.value.request_id) {
                                ProjectionRequestState::JoinedOpen
                            } else {
                                ProjectionRequestState::PreparedUnjoined
                            }
                        },
                        |closed| ProjectionRequestState::Closed(closed.value.outcome),
                    ),
            })
            .collect::<Vec<_>>();
        requests.sort_by_key(|request| request.source_event.sequence);
        let mut turns = turn_starts
            .values()
            .map(|turn| ProjectionTurn {
                start: turn.value.clone(),
                source_event: turn.source.clone(),
                state: if abnormal_turns.contains(&turn.value.turn_id) {
                    ProjectionLifecycleState::Abnormal
                } else if closed_turns.contains(&turn.value.turn_id) {
                    ProjectionLifecycleState::Completed
                } else {
                    ProjectionLifecycleState::Active
                },
            })
            .collect::<Vec<_>>();
        turns.sort_by_key(|turn| turn.source_event.sequence);
        let mut steps = step_starts
            .values()
            .map(|step| ProjectionStep {
                start: step.value.clone(),
                source_event: step.source.clone(),
                state: if abnormal_steps.contains(&step.value.step_id) {
                    ProjectionLifecycleState::Abnormal
                } else if completed_steps.contains(&step.value.step_id) {
                    ProjectionLifecycleState::Completed
                } else {
                    ProjectionLifecycleState::Active
                },
            })
            .collect::<Vec<_>>();
        steps.sort_by_key(|step| (step.source_event.sequence, step.start.step_ordinal));

        let mut provider_inputs = Vec::new();
        for request in &requests {
            let Some(join) = joins.get(&request.request_id) else {
                continue;
            };
            let prepared = &preparations[&request.request_id];
            let Some(lease) = leases.get(&join.value.lease_id) else {
                // A valid mixed replay may join a lease whose evidence is before the
                // exact suffix boundary; the incomplete item has no suffix claim.
                continue;
            };
            provider_inputs.push(provider_input(
                provider_inputs.len() as u64,
                prepared,
                join,
                lease,
            ));
        }

        let abandoned_request = |request_id: Uuid| {
            request_closures
                .get(&request_id)
                .is_some_and(|value| value.value.outcome == ModelRequestOutcome::Abandoned)
                || preparations.get(&request_id).is_some_and(|prepared| {
                    abnormal_steps.contains(&prepared.value.step_id)
                        || abnormal_turns.contains(&prepared.value.turn_id)
                })
        };
        let mut attempts = Vec::new();
        for prepared in preparations.values() {
            let mut ordinals = BTreeSet::new();
            ordinals.extend(failures.keys().filter_map(|(request_id, ordinal)| {
                (*request_id == prepared.value.request_id).then_some(*ordinal)
            }));
            ordinals.extend(
                chunks
                    .get(&prepared.value.request_id)
                    .into_iter()
                    .flatten()
                    .map(|chunk| chunk.value.response_attempt_ordinal),
            );
            if let Some(commit) = commits.get(&prepared.value.request_id) {
                ordinals.insert(commit.value.response_attempt_ordinal);
            }
            if ordinals.is_empty() {
                ordinals.insert(0);
            }
            for ordinal in ordinals {
                let state = if commits
                    .get(&prepared.value.request_id)
                    .is_some_and(|commit| commit.value.response_attempt_ordinal == ordinal)
                {
                    ProjectionAttemptState::Committed
                } else if failures.contains_key(&(prepared.value.request_id, ordinal)) {
                    ProjectionAttemptState::Failed
                } else if abandoned_request(prepared.value.request_id) {
                    ProjectionAttemptState::Abandoned
                } else {
                    ProjectionAttemptState::Open
                };
                attempts.push(ProjectionAttempt {
                    request_id: prepared.value.request_id,
                    response_attempt_ordinal: ordinal,
                    state,
                });
            }
        }
        attempts.sort_by_key(|attempt| {
            (
                preparations[&attempt.request_id].source.sequence,
                attempt.response_attempt_ordinal,
            )
        });

        let mut transcript_entries: Vec<(u64, u32, TranscriptMessageV1)> = Vec::new();
        for prompt in prompts.values() {
            let turn_id = turn_starts
                .values()
                .find(|turn| turn.value.prompt_id == prompt.value.prompt_id)
                .map(|turn| turn.value.turn_id);
            let status = if turn_id.is_some_and(|turn| abnormal_turns.contains(&turn)) {
                TranscriptStatusV1::AbandonedAfterCommit
            } else {
                TranscriptStatusV1::Normal
            };
            transcript_entries.push((
                prompt.source.sequence,
                0,
                TranscriptMessageV1 {
                    item_ordinal: 0,
                    message_kind: TranscriptMessageKindV1::Prompt,
                    role: TranscriptRoleV1::User,
                    message_id: prompt.value.prompt_id,
                    turn_id,
                    step_id: None,
                    request_id: None,
                    source_event: prompt.source.clone(),
                    content: TranscriptContentV1::Prompt {
                        prompt_content: prompt.value.content.clone(),
                    },
                    status,
                },
            ));
        }
        for (request_id, commit) in &commits {
            let Some(prepared) = preparations.get(request_id) else {
                continue;
            };
            transcript_entries.push((
                commit.source.sequence,
                0,
                TranscriptMessageV1 {
                    item_ordinal: 0,
                    message_kind: TranscriptMessageKindV1::Assistant,
                    role: TranscriptRoleV1::Assistant,
                    message_id: commit.value.message_id,
                    turn_id: Some(prepared.value.turn_id),
                    step_id: Some(prepared.value.step_id),
                    request_id: Some(*request_id),
                    source_event: commit.source.clone(),
                    content: TranscriptContentV1::Assistant {
                        assistant_channels: commit.value.content.clone(),
                    },
                    status: if abandoned_request(*request_id) {
                        TranscriptStatusV1::AbandonedAfterCommit
                    } else {
                        TranscriptStatusV1::Normal
                    },
                },
            ));
        }
        for result in results.values() {
            let Some(call) = calls.get(&result.value.tool_call_id) else {
                continue;
            };
            let Some(prepared) = preparations.get(&call.value.request_id) else {
                continue;
            };
            transcript_entries.push((
                result.source.sequence,
                result.value.result_ordinal,
                TranscriptMessageV1 {
                    item_ordinal: 0,
                    message_kind: TranscriptMessageKindV1::ToolResult,
                    role: TranscriptRoleV1::Tool,
                    message_id: result.value.tool_result_id,
                    turn_id: Some(prepared.value.turn_id),
                    step_id: Some(result.value.step_id),
                    request_id: Some(call.value.request_id),
                    source_event: result.source.clone(),
                    content: TranscriptContentV1::ToolResult {
                        tool_result_id: result.value.tool_result_id,
                        tool_call_id: result.value.tool_call_id,
                        call_id: result.value.call_id.clone(),
                        content_ref: result.value.content_ref.clone(),
                        is_error: result.value.is_error,
                        disposition: result.value.disposition,
                    },
                    status: if abandoned_request(call.value.request_id) {
                        TranscriptStatusV1::AbandonedAfterCommit
                    } else {
                        TranscriptStatusV1::Normal
                    },
                },
            ));
        }
        transcript_entries.sort_by_key(|(sequence, ordinal, _)| (*sequence, *ordinal));
        let mut transcript = transcript_entries
            .into_iter()
            .map(|(_, _, message)| message)
            .collect::<Vec<_>>();
        for (ordinal, message) in transcript.iter_mut().enumerate() {
            message.item_ordinal = ordinal as u64;
        }

        let mut conversation = transcript
            .iter()
            .cloned()
            .map(|message| {
                let status = if message.status == TranscriptStatusV1::AbandonedAfterCommit {
                    FrontendConversationStatusV1::AbandonedAfterCommit
                } else {
                    FrontendConversationStatusV1::Committed
                };
                (
                    message.source_event.sequence,
                    0,
                    FrontendConversationItemV1 {
                        item_ordinal: 0,
                        kind: FrontendConversationKindV1::CommittedMessage,
                        turn_id: message.turn_id,
                        step_id: message.step_id,
                        request_id: message.request_id,
                        message_id: Some(message.message_id),
                        response_attempt_ordinal: None,
                        content_kind: None,
                        chunk_ordinal: None,
                        content_ref: None,
                        transcript_message: Some(message),
                        status,
                        source_event: SourceEventV1 {
                            sequence: 0,
                            event_id: Uuid::nil(),
                        },
                    },
                )
            })
            .collect::<Vec<_>>();
        for (_, _, item) in &mut conversation {
            item.source_event = item
                .transcript_message
                .as_ref()
                .expect("committed conversation message")
                .source_event
                .clone();
        }
        for (request_id, request_chunks) in &chunks {
            let committed_attempt = commits
                .get(request_id)
                .map(|commit| commit.value.response_attempt_ordinal);
            let Some(prepared) = preparations.get(request_id) else {
                continue;
            };
            for chunk in request_chunks {
                if committed_attempt == Some(chunk.value.response_attempt_ordinal) {
                    continue;
                }
                let status = if failures
                    .contains_key(&(*request_id, chunk.value.response_attempt_ordinal))
                    || abandoned_request(*request_id)
                {
                    FrontendConversationStatusV1::Abandoned
                } else {
                    FrontendConversationStatusV1::Partial
                };
                conversation.push((
                    chunk.source.sequence,
                    chunk.value.chunk_ordinal,
                    FrontendConversationItemV1 {
                        item_ordinal: 0,
                        kind: FrontendConversationKindV1::AssistantEvidence,
                        turn_id: Some(prepared.value.turn_id),
                        step_id: Some(prepared.value.step_id),
                        request_id: Some(*request_id),
                        message_id: Some(chunk.value.message_id),
                        response_attempt_ordinal: Some(chunk.value.response_attempt_ordinal),
                        content_kind: Some(chunk.value.content_kind),
                        chunk_ordinal: Some(chunk.value.chunk_ordinal),
                        content_ref: Some(chunk.value.content_ref.clone()),
                        transcript_message: None,
                        status,
                        source_event: chunk.source.clone(),
                    },
                ));
            }
        }
        conversation.sort_by_key(|(sequence, ordinal, _)| (*sequence, *ordinal));
        let mut conversation = conversation
            .into_iter()
            .map(|(_, _, item)| item)
            .collect::<Vec<_>>();
        for (ordinal, item) in conversation.iter_mut().enumerate() {
            item.item_ordinal = ordinal as u64;
        }

        let active_turn = turn_starts
            .values()
            .filter(|turn| !closed_turns.contains(&turn.value.turn_id))
            .max_by_key(|turn| turn.source.sequence)
            .map(|turn| FrontendActiveTurnV1 {
                turn_id: turn.value.turn_id,
                prompt_id: turn.value.prompt_id,
                status: if interrupted_turns.contains(&turn.value.turn_id) {
                    ActiveTurnStatusV1::Interrupted
                } else {
                    ActiveTurnStatusV1::Active
                },
            });
        let active_prompt_ids = turn_starts
            .values()
            .map(|turn| turn.value.prompt_id)
            .collect::<BTreeSet<_>>();
        let mut queued = prompts
            .values()
            .filter(|prompt| !active_prompt_ids.contains(&prompt.value.prompt_id))
            .collect::<Vec<_>>();
        queued.sort_by_key(|prompt| prompt.source.sequence);
        let queued_prompts = queued
            .into_iter()
            .enumerate()
            .map(|(ordinal, prompt)| QueuedPromptV1 {
                queue_ordinal: ordinal as u64,
                prompt_id: prompt.value.prompt_id,
                submission_id: prompt.value.submission_id,
                content: prompt.value.content.clone(),
            })
            .collect();

        let (context, compaction) = compaction_projection(
            &frontier,
            &compaction_starts,
            &compaction_summaries,
            &compaction_terminals,
        );
        let frontend = FrontendSnapshotV1 {
            snapshot_schema_version: 1,
            queued_prompts,
            active_turn,
            context: context.clone(),
            conversation,
        };
        frontend.validate()?;
        compaction.validate()?;

        let mut tool_calls = calls
            .values()
            .map(|call| {
                let result = results
                    .get(&call.value.tool_call_id)
                    .map(|value| value.value.clone());
                let result_state = result.as_ref().map_or(
                    ProjectionToolResultState::Missing,
                    |result| match result.disposition {
                        crate::session_authority::ToolResultDisposition::Denied => {
                            ProjectionToolResultState::Denied
                        }
                        crate::session_authority::ToolResultDisposition::NotDispatched => {
                            ProjectionToolResultState::NotDispatched
                        }
                        crate::session_authority::ToolResultDisposition::Settled => {
                            ProjectionToolResultState::Settled
                        }
                        crate::session_authority::ToolResultDisposition::UnknownCompletion => {
                            ProjectionToolResultState::UnknownCompletion
                        }
                    },
                );
                ProjectionToolCall {
                    call: call.value.clone(),
                    source_event: call.source.clone(),
                    result_state,
                    result,
                }
            })
            .collect::<Vec<_>>();
        tool_calls.sort_by_key(|call| (call.source_event.sequence, call.call.call_ordinal));

        Ok(Self {
            session_id: replay.frontier().session_id().into(),
            stream_id: replay.frontier().stream_id(),
            lineage,
            boundary,
            frontier,
            source_events,
            turns,
            steps,
            requests,
            attempts,
            tool_calls,
            provider_inputs,
            transcript,
            frontend,
            compaction,
        })
    }

    pub(crate) fn provider_history(&self) -> &[ProviderRequestInputV1] {
        &self.provider_inputs
    }

    pub(crate) fn lineage(&self) -> ProjectionLineageV1 {
        self.lineage
    }

    pub(crate) fn transcript(&self) -> &[TranscriptMessageV1] {
        &self.transcript
    }

    pub(crate) fn frontend_snapshot(&self) -> &FrontendSnapshotV1 {
        &self.frontend
    }

    pub(crate) fn compaction_checkpoint(&self) -> &CompactionCheckpointV1 {
        &self.compaction
    }

    pub(crate) fn provider_history_chunks(&self) -> ProjectionResult<ProjectedChunksV1> {
        self.chunks(
            ProjectorIdV1::ProviderHistory,
            self.provider_inputs
                .iter()
                .cloned()
                .map(|item| ChunkItem::Provider(Box::new(item)))
                .collect(),
        )
    }

    pub(crate) fn transcript_chunks(&self) -> ProjectionResult<ProjectedChunksV1> {
        self.chunks(
            ProjectorIdV1::Transcript,
            self.transcript
                .iter()
                .cloned()
                .map(|item| ChunkItem::Transcript(Box::new(item)))
                .collect(),
        )
    }

    pub(crate) fn envelope(
        &self,
        projector_id: ProjectorIdV1,
        payload: ProjectionPayloadV1,
    ) -> ProjectionResult<ProjectionEnvelopeV1> {
        let envelope = match self.lineage {
            ProjectionLineageV1::Legacy => ProjectionEnvelopeV1 {
                envelope_schema_version: 1,
                projector_id,
                projector_version: 1,
                projection_schema_version: 1,
                session_id: self.session_id.clone(),
                stream_id: Some(self.stream_id),
                lineage_level: self.lineage,
                availability: ProjectionAvailabilityV1::Unavailable,
                exactness: ProjectionExactnessV1::None,
                scope: ProjectionScopeV1::None,
                full_spine_boundary: None,
                source_frontier: Some(self.frontier.clone()),
                full_session_export: FullSessionExportV1::Unavailable,
                unavailable: Some(ProjectionUnavailableV1 {
                    reason: ProjectionUnavailableReasonV1::LegacyLineage,
                    first_sequence: None,
                    content_digest: None,
                }),
                payload: ProjectionPayloadV1::None,
            },
            ProjectionLineageV1::Mixed => ProjectionEnvelopeV1 {
                envelope_schema_version: 1,
                projector_id,
                projector_version: 1,
                projection_schema_version: 1,
                session_id: self.session_id.clone(),
                stream_id: Some(self.stream_id),
                lineage_level: self.lineage,
                availability: ProjectionAvailabilityV1::Available,
                exactness: ProjectionExactnessV1::ExactSuffix,
                scope: ProjectionScopeV1::FullSpineSuffix,
                full_spine_boundary: self.boundary.clone(),
                source_frontier: Some(self.frontier.clone()),
                full_session_export: FullSessionExportV1::Unavailable,
                unavailable: Some(ProjectionUnavailableV1 {
                    reason: ProjectionUnavailableReasonV1::PreBoundaryContentNotAuthoritative,
                    first_sequence: self.boundary.as_ref().map(|value| value.sequence),
                    content_digest: None,
                }),
                payload,
            },
            ProjectionLineageV1::Full => ProjectionEnvelopeV1 {
                envelope_schema_version: 1,
                projector_id,
                projector_version: 1,
                projection_schema_version: 1,
                session_id: self.session_id.clone(),
                stream_id: Some(self.stream_id),
                lineage_level: self.lineage,
                availability: ProjectionAvailabilityV1::Available,
                exactness: ProjectionExactnessV1::ExactFull,
                scope: ProjectionScopeV1::FullSession,
                full_spine_boundary: None,
                source_frontier: Some(self.frontier.clone()),
                full_session_export: FullSessionExportV1::Available,
                unavailable: None,
                payload,
            },
        };
        envelope.validate()?;
        Ok(envelope)
    }

    fn chunks(
        &self,
        projector_id: ProjectorIdV1,
        items: Vec<ChunkItem>,
    ) -> ProjectionResult<ProjectedChunksV1> {
        if self.lineage == ProjectionLineageV1::Legacy {
            return Err(ProjectionValidationError::Invalid(
                "legacy lineage has no projection chunks".into(),
            ));
        }
        let mut chunks = Vec::new();
        let mut offset = 0;
        while offset < items.len() {
            let chunk_ordinal = u32::try_from(chunks.len())
                .map_err(|_| ProjectionValidationError::Invalid("chunk ordinal overflow".into()))?;
            let mut end = offset;
            let mut accepted = None;
            while end < items.len() && end - offset < MAX_CHUNK_ITEMS {
                end += 1;
                let chunk = projection_chunk(
                    projector_id,
                    &self.session_id,
                    self.stream_id,
                    chunk_ordinal,
                    &items[offset..end],
                )?;
                let bytes = crate::surfaces::session::canonical_json_bytes(&chunk)?;
                if bytes.len() > MAX_CHUNK_BYTES {
                    break;
                }
                accepted = Some((chunk, bytes));
            }
            let Some(chunk) = accepted else {
                return Err(ProjectionValidationError::Invalid(
                    "projection item exceeds 8 MiB chunk limit".into(),
                ));
            };
            offset += chunk.0.items_len();
            chunks.push(chunk);
        }
        let entries = chunks
            .iter()
            .map(|(chunk, bytes)| {
                let digest = crate::surfaces::session::canonical_sha256(bytes);
                ChunkManifestEntryV1 {
                    chunk_ordinal: chunk.chunk_ordinal,
                    chunk_id: digest.clone(),
                    first_item_ordinal: chunk.first_item_ordinal,
                    last_item_ordinal: chunk.last_item_ordinal,
                    item_count: chunk.items_len() as u32,
                    byte_length: bytes.len() as u64,
                    digest_algorithm: DigestAlgorithmV1::Sha256,
                    digest,
                }
            })
            .collect::<Vec<_>>();
        let manifest = ChunkManifestV1 {
            manifest_schema_version: 1,
            projector_id,
            session_id: self.session_id.clone(),
            stream_id: self.stream_id,
            source_frontier: self.frontier.clone(),
            chunk_count: entries.len() as u32,
            item_count: items.len() as u64,
            chunks: entries,
        };
        manifest.validate()?;
        manifest.validate_chunks(&chunks)?;
        Ok((manifest, chunks))
    }
}

#[derive(Clone)]
enum ChunkItem {
    Provider(Box<ProviderRequestInputV1>),
    Transcript(Box<TranscriptMessageV1>),
}

fn projection_chunk(
    projector_id: ProjectorIdV1,
    session_id: &str,
    stream_id: Uuid,
    chunk_ordinal: u32,
    items: &[ChunkItem],
) -> ProjectionResult<ProjectionChunkV1> {
    let first_item_ordinal = match items.first() {
        Some(ChunkItem::Provider(value)) => value.item_ordinal,
        Some(ChunkItem::Transcript(value)) => value.item_ordinal,
        None => return Err(ProjectionValidationError::Invalid("empty chunk".into())),
    };
    let last_item_ordinal = match items.last().expect("non-empty chunk") {
        ChunkItem::Provider(value) => value.item_ordinal,
        ChunkItem::Transcript(value) => value.item_ordinal,
    };
    let items = match projector_id {
        ProjectorIdV1::ProviderHistory => ProjectionChunkItemsV1::ProviderRequests(
            items
                .iter()
                .map(|item| match item {
                    ChunkItem::Provider(value) => Ok(value.as_ref().clone()),
                    ChunkItem::Transcript(_) => Err(ProjectionValidationError::Invalid(
                        "provider chunk contains transcript item".into(),
                    )),
                })
                .collect::<ProjectionResult<Vec<_>>>()?,
        ),
        ProjectorIdV1::Transcript => ProjectionChunkItemsV1::TranscriptMessages(
            items
                .iter()
                .map(|item| match item {
                    ChunkItem::Transcript(value) => Ok(value.as_ref().clone()),
                    ChunkItem::Provider(_) => Err(ProjectionValidationError::Invalid(
                        "transcript chunk contains provider item".into(),
                    )),
                })
                .collect::<ProjectionResult<Vec<_>>>()?,
        ),
        _ => {
            return Err(ProjectionValidationError::Invalid(
                "inline projector cannot create chunks".into(),
            ));
        }
    };
    Ok(ProjectionChunkV1 {
        chunk_schema_version: 1,
        projector_id,
        session_id: session_id.into(),
        stream_id,
        chunk_ordinal,
        first_item_ordinal,
        last_item_ordinal,
        items,
    })
}

impl ProjectionChunkV1 {
    fn items_len(&self) -> usize {
        match &self.items {
            ProjectionChunkItemsV1::ProviderRequests(values) => values.len(),
            ProjectionChunkItemsV1::TranscriptMessages(values) => values.len(),
        }
    }
}

fn provider_input(
    item_ordinal: u64,
    prepared: &EventValue<ModelRequestPrepared>,
    join: &EventValue<crate::session_authority::ModelRequestRouteJoined>,
    lease: &EventValue<RouteLeaseRecorded>,
) -> ProviderRequestInputV1 {
    ProviderRequestInputV1 {
        item_ordinal,
        request_id: prepared.value.request_id,
        step_id: prepared.value.step_id,
        turn_id: prepared.value.turn_id,
        request_ordinal: prepared.value.request_ordinal,
        purpose: prepared.value.purpose,
        replaces_request_id: prepared.value.replaces_request_id,
        prepared_event: prepared.source.clone(),
        route_join_event: join.source.clone(),
        lease_event: lease.source.clone(),
        lease_id: lease.value.lease_id,
        selected_provider_id: lease.value.selected_provider_id.clone(),
        selected_model_id: lease.value.selected_model_id.clone(),
        serving_provider_id: lease.value.serving_provider_id.clone(),
        serving_model_id: lease.value.serving_model_id.clone(),
        schema_dialect: lease.value.schema_dialect.clone(),
        credential_source_class: lease.value.credential_source_class.clone(),
        fallback_reason: lease.value.fallback_reason.clone(),
        contribution_generation_id: lease.value.contribution_generation_id.clone(),
        route_policy: lease.value.route_policy.clone(),
        continuity_ids: prepared.value.continuity_refs.clone(),
        context_manifest_id: prepared.value.context_manifest_id.clone(),
        context_items: prepared.value.context_items.clone(),
        schema_set_id: prepared.value.schema_set_id.clone(),
        schema_set: prepared.value.schema_set.clone(),
    }
}

enum CompactionTerminalEvent {
    Applied(EventValue<CompactionApplied>),
    Abandoned(EventValue<CompactionAbandoned>),
}

fn compaction_projection(
    frontier: &SourceEventV1,
    starts: &BTreeMap<Uuid, EventValue<CompactionStarted>>,
    summaries: &BTreeMap<Uuid, EventValue<crate::session_authority::CompactionSummaryCommitted>>,
    terminals: &[CompactionTerminalEvent],
) -> (FrontendContextV1, CompactionCheckpointV1) {
    let latest_terminal = terminals.last();
    let latest_start = starts.values().max_by_key(|value| value.source.sequence);
    let terminal_ids = terminals
        .iter()
        .map(|terminal| match terminal {
            CompactionTerminalEvent::Applied(value) => value.value.compaction_id,
            CompactionTerminalEvent::Abandoned(value) => value.value.compaction_id,
        })
        .collect::<BTreeSet<_>>();
    let active = latest_start.filter(|start| !terminal_ids.contains(&start.value.compaction_id));
    let applied = terminals.iter().rev().find_map(|terminal| match terminal {
        CompactionTerminalEvent::Applied(value) => Some(value),
        CompactionTerminalEvent::Abandoned(_) => None,
    });
    let (context_revision, context_manifest_id, context_items) = if let Some(applied) = applied {
        let summary = summaries.get(&applied.value.compaction_id);
        (
            applied.value.target_context_revision,
            applied.value.replacement_manifest_id.clone(),
            summary
                .map(|summary| {
                    summary
                        .value
                        .replacement_items
                        .iter()
                        .map(|item| crate::session_authority::CompactionContextItem {
                            ordinal: item.ordinal,
                            source_event_id: item.source_event_id,
                            source_identity: item.source_identity.clone(),
                            content_ref: item.content_ref.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        )
    } else if let Some(start) = latest_start {
        (
            start.value.source_context_revision,
            start.value.input_manifest_id.clone(),
            start.value.input_items.clone(),
        )
    } else {
        (0, empty_list_digest(), Vec::new())
    };
    let last_terminal = latest_terminal.map(|terminal| match terminal {
        CompactionTerminalEvent::Applied(value) => LastCompactionTerminalV1 {
            compaction_id: value.value.compaction_id,
            terminal: CompactionTerminalV1::Applied,
            terminal_event: value.source.clone(),
            source_context_revision: value.value.source_context_revision,
            target_context_revision: Some(value.value.target_context_revision),
            replacement_manifest_id: Some(value.value.replacement_manifest_id.clone()),
            compaction_summary_id: Some(value.value.compaction_summary_id),
            reason_code: None,
        },
        CompactionTerminalEvent::Abandoned(value) => {
            let source_revision = starts
                .get(&value.value.compaction_id)
                .map_or(0, |start| start.value.source_context_revision);
            LastCompactionTerminalV1 {
                compaction_id: value.value.compaction_id,
                terminal: CompactionTerminalV1::Abandoned,
                terminal_event: value.source.clone(),
                source_context_revision: source_revision,
                target_context_revision: None,
                replacement_manifest_id: None,
                compaction_summary_id: None,
                reason_code: Some(value.value.reason_code.clone()),
            }
        }
    });
    let compaction_state = if active.is_some() {
        CompactionStateV1::InProgress
    } else if let Some(terminal) = latest_terminal {
        let source = match terminal {
            CompactionTerminalEvent::Applied(value) => &value.source,
            CompactionTerminalEvent::Abandoned(value) => &value.source,
        };
        if source == frontier {
            match terminal {
                CompactionTerminalEvent::Applied(_) => CompactionStateV1::Applied,
                CompactionTerminalEvent::Abandoned(_) => CompactionStateV1::Abandoned,
            }
        } else {
            CompactionStateV1::Idle
        }
    } else if starts.is_empty() {
        CompactionStateV1::Never
    } else {
        CompactionStateV1::Idle
    };
    let context = FrontendContextV1 {
        context_revision,
        context_manifest_id: context_manifest_id.clone(),
        items: context_items.clone(),
    };
    let checkpoint = CompactionCheckpointV1 {
        checkpoint_schema_version: 1,
        context_revision,
        context_manifest_id,
        context_items,
        compaction_state,
        active_compaction: active.map(|start| ActiveCompactionV1 {
            compaction_id: start.value.compaction_id,
            owner_scope: start.value.owner_scope.clone(),
            source_frontier: start.value.source_frontier.clone(),
            source_context_revision: start.value.source_context_revision,
            target_context_revision: start.value.target_context_revision,
            input_manifest_id: start.value.input_manifest_id.clone(),
        }),
        last_terminal,
    };
    (context, checkpoint)
}

fn validate_request_default_content(
    replay: &SessionReplay,
    request: &ModelRequestPrepared,
) -> ProjectionResult<()> {
    for item in &request.context_items {
        validate_default_ref(replay, &item.content_ref)?;
    }
    for schema in &request.schema_set.schemas {
        validate_default_ref(replay, &schema.schema_content_ref)?;
    }
    Ok(())
}

fn validate_default_ref(replay: &SessionReplay, content_ref: &ContentRef) -> ProjectionResult<()> {
    if content_ref.projection_class() != ProjectionClass::Default {
        return Err(ProjectionValidationError::Invalid(
            "restricted content reference reached semantic projection".into(),
        ));
    }
    replay.read_default_content(content_ref).map_err(|error| {
        ProjectionValidationError::Invalid(format!("default content validation failed: {error}"))
    })?;
    Ok(())
}

fn source_from_frontier(frontier: &crate::session_replay::AuthorityFrontier) -> SourceEventV1 {
    SourceEventV1 {
        sequence: frontier.sequence(),
        event_id: frontier.event_id(),
    }
}

fn empty_list_digest() -> String {
    crate::surfaces::session::canonical_sha256(b"[]")
}

fn empty_frontend() -> FrontendSnapshotV1 {
    FrontendSnapshotV1 {
        snapshot_schema_version: 1,
        queued_prompts: Vec::new(),
        active_turn: None,
        context: FrontendContextV1 {
            context_revision: 0,
            context_manifest_id: empty_list_digest(),
            items: Vec::new(),
        },
        conversation: Vec::new(),
    }
}

fn empty_checkpoint() -> CompactionCheckpointV1 {
    CompactionCheckpointV1 {
        checkpoint_schema_version: 1,
        context_revision: 0,
        context_manifest_id: empty_list_digest(),
        context_items: Vec::new(),
        compaction_state: CompactionStateV1::Never,
        active_compaction: None,
        last_terminal: None,
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use super::*;
    use crate::{
        session_replay::ReplayEnd,
        surfaces::session::{ProjectorIdV1, canonical_json_bytes},
    };

    const FIXTURES: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/session-semantic-v1"
    );
    const SESSION_ID: &str = "fixture-session";
    const STREAM_ID: Uuid = Uuid::from_u128(0x10000000_0000_4000_8000_000000000001);

    fn replay_fixture(name: &str) -> (tempfile::TempDir, SessionReplay) {
        let directory = tempfile::tempdir().unwrap();
        let snapshot = directory.path().join("session.json");
        fs::write(
            directory.path().join("session.authority.jsonl"),
            fs::read(Path::new(FIXTURES).join(name)).unwrap(),
        )
        .unwrap();
        let replay =
            SessionReplay::replay_prefix(&snapshot, SESSION_ID, STREAM_ID, ReplayEnd::EndOfStream)
                .unwrap();
        (directory, replay)
    }

    #[test]
    fn legacy_fixture_yields_availability_only() {
        let (_directory, replay) = replay_fixture("slice-1-closed.authority.jsonl");
        let model = SessionProjectionModel::from_replay(&replay).unwrap();
        let envelope = model
            .envelope(ProjectorIdV1::Transcript, ProjectionPayloadV1::None)
            .unwrap();

        assert_eq!(envelope.lineage_level, ProjectionLineageV1::Legacy);
        assert_eq!(envelope.availability, ProjectionAvailabilityV1::Unavailable);
        assert!(matches!(envelope.payload, ProjectionPayloadV1::None));
        assert!(model.transcript().is_empty());
    }

    #[test]
    fn mixed_fixture_excludes_every_pre_boundary_item() {
        let (_directory, replay) = replay_fixture("mixed-legacy-full.authority.jsonl");
        let model = SessionProjectionModel::from_replay(&replay).unwrap();
        let snapshot = model.frontend_snapshot().clone();
        let envelope = model
            .envelope(
                ProjectorIdV1::FrontendSnapshot,
                ProjectionPayloadV1::FrontendSnapshot { snapshot },
            )
            .unwrap();

        assert_eq!(envelope.exactness, ProjectionExactnessV1::ExactSuffix);
        assert_eq!(
            envelope.full_session_export,
            FullSessionExportV1::Unavailable
        );
        assert_eq!(envelope.full_spine_boundary.as_ref().unwrap().sequence, 8);
        assert_eq!(model.source_events.len(), 1);
        assert!(model.transcript().is_empty());
        assert!(model.frontend_snapshot().conversation.is_empty());
    }

    #[test]
    fn full_fixture_is_exact_and_canonical_bytes_are_stable() {
        let (_directory, replay) = replay_fixture("full-spine-crash-prefix.authority.jsonl");
        let first = SessionProjectionModel::from_replay(&replay).unwrap();
        let second = SessionProjectionModel::from_replay(&replay).unwrap();
        let first_snapshot = first.frontend_snapshot().clone();
        let second_snapshot = second.frontend_snapshot().clone();
        let first = first
            .envelope(
                ProjectorIdV1::FrontendSnapshot,
                ProjectionPayloadV1::FrontendSnapshot {
                    snapshot: first_snapshot,
                },
            )
            .unwrap();
        let second = second
            .envelope(
                ProjectorIdV1::FrontendSnapshot,
                ProjectionPayloadV1::FrontendSnapshot {
                    snapshot: second_snapshot,
                },
            )
            .unwrap();

        assert_eq!(first.exactness, ProjectionExactnessV1::ExactFull);
        assert_eq!(first.full_session_export, FullSessionExportV1::Available);
        assert_eq!(first.full_spine_boundary, None);
        assert_eq!(
            first.canonical_bytes().unwrap(),
            second.canonical_bytes().unwrap()
        );
    }

    #[test]
    fn empty_chunk_manifests_and_digests_are_deterministic() {
        let (_directory, replay) = replay_fixture("full-spine-crash-prefix.authority.jsonl");
        let model = SessionProjectionModel::from_replay(&replay).unwrap();
        let (provider, provider_chunks) = model.provider_history_chunks().unwrap();
        let (transcript, transcript_chunks) = model.transcript_chunks().unwrap();

        assert!(provider_chunks.is_empty());
        assert_eq!(provider.chunk_count, 0);
        assert_eq!(provider.item_count, 0);
        assert_eq!(transcript.item_count, 1);
        assert_eq!(transcript_chunks.len(), 1);
        let bytes = &transcript_chunks[0].1;
        assert_eq!(
            transcript.chunks[0].digest,
            crate::surfaces::session::canonical_sha256(bytes)
        );
        assert_eq!(canonical_json_bytes(&transcript).unwrap()[0], b'{');
    }

    #[test]
    fn frozen_dtos_reject_unknown_fields_and_contradictory_availability() {
        let (_directory, replay) = replay_fixture("slice-1-closed.authority.jsonl");
        let model = SessionProjectionModel::from_replay(&replay).unwrap();
        let envelope = model
            .envelope(ProjectorIdV1::Transcript, ProjectionPayloadV1::None)
            .unwrap();
        let mut value = serde_json::to_value(&envelope).unwrap();
        value["timestamp"] = serde_json::json!("forbidden");
        assert!(serde_json::from_value::<ProjectionEnvelopeV1>(value).is_err());

        let mut contradictory = envelope;
        contradictory.availability = ProjectionAvailabilityV1::Available;
        assert!(contradictory.validate().is_err());
    }
}
