//! Slice 5.4 host-owned session stores. Semantic authority remains in the event stream.

use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    conversation::{
        CompletedWorkPlan, EvidenceLedger, FailedApproach, IntentDocument, OperatorToolObservation,
        PendingAction, PlanEvent, PlanMode, PlanRegistryViewState, TaskMode, VisiblePlanState,
        WorkItem,
    },
    session::{SessionEntry, SessionMeta, SessionSource},
    session_authority::{AuthorityLineageLevel, SessionAuthorityHandle, SessionFactPayload},
    session_blob_store::{ContentRef, ProjectionClass, SessionBlobStore},
    session_replacement::ProjectionBinding,
    session_replay::{ReplayEnd, SessionReplay},
};

const HOST_SCHEMA_VERSION: u16 = 1;
const OBSERVATION_SCHEMA_VERSION: u16 = 1;
const CATALOG_SCHEMA_VERSION: u16 = 1;
const MAX_HOST_BYTES: u64 = 4 * 1024 * 1024;
const MAX_CATALOG_BYTES: u64 = 1024 * 1024;
const MAX_COMPATIBILITY_BYTES: u64 = 64 * 1024 * 1024;
const MAX_OBSERVATION_BYTES: usize = 1024 * 1024;
const OBSERVATION_MARKER: &[u8] = b"session-observations-v1\n";

#[derive(Debug, Clone)]
pub(crate) struct SessionStorageBinding {
    snapshot: PathBuf,
    session_id: String,
    workspace_identity: String,
    stream_id: Option<Uuid>,
    source_frontier: Option<SourceFrontierV1>,
}

impl SessionStorageBinding {
    pub(crate) fn from_authority(
        snapshot: &Path,
        session_id: &str,
        authority: Option<&SessionAuthorityHandle>,
        workspace: &Path,
    ) -> Self {
        let state = authority.map(SessionAuthorityHandle::state);
        Self {
            snapshot: snapshot.to_path_buf(),
            session_id: session_id.into(),
            workspace_identity: workspace.to_string_lossy().into_owned(),
            stream_id: state.as_ref().and_then(|state| state.stream_id),
            source_frontier: state.as_ref().and_then(|state| {
                Some(SourceFrontierV1 {
                    sequence: state.last_sequence,
                    event_id: state.last_event_id?,
                })
            }),
        }
    }

    pub(crate) fn from_open_authority(
        snapshot: &Path,
        session_id: &str,
        authority: &crate::session_authority::SessionAuthority,
        workspace: &Path,
    ) -> Self {
        let state = authority.state();
        Self {
            snapshot: snapshot.to_path_buf(),
            session_id: session_id.into(),
            workspace_identity: workspace.to_string_lossy().into_owned(),
            stream_id: state.stream_id,
            source_frontier: state.last_event_id.map(|event_id| SourceFrontierV1 {
                sequence: state.last_sequence,
                event_id,
            }),
        }
    }

    pub(crate) fn discover(snapshot: &Path, session_id: &str, workspace: &Path) -> Result<Self> {
        let replay = SessionReplay::replay_prefix(
            snapshot,
            session_id,
            authority_stream_id(snapshot, session_id)?,
            ReplayEnd::EndOfStream,
        )?;
        Ok(Self {
            snapshot: snapshot.to_path_buf(),
            session_id: session_id.into(),
            workspace_identity: workspace.to_string_lossy().into_owned(),
            stream_id: Some(replay.frontier().stream_id()),
            source_frontier: Some(SourceFrontierV1 {
                sequence: replay.frontier().sequence(),
                event_id: replay.frontier().event_id(),
            }),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceFrontierV1 {
    sequence: u64,
    event_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HostIntentV1 {
    current_task: Option<String>,
    approach: Option<String>,
    lifecycle_phase: omegon_traits::LifecyclePhase,
    task_mode: TaskMode,
    task_mode_pinned: bool,
    commit_nudged: bool,
    skill_completion_nudged: bool,
    plan_reconciliation_fingerprint: Option<u64>,
    plan_reconciliation_nudges: u8,
    mcq_detected: bool,
    obfuscation_detected: bool,
    operator_correction_pending: bool,
    pending_action: Option<PendingAction>,
    constraints_discovered: Vec<String>,
    failed_approaches: Vec<FailedApproach>,
    open_questions: Vec<String>,
    files_read: Vec<PathBuf>,
    files_modified: Vec<PathBuf>,
    evidence: EvidenceLedger,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HostPlansV1 {
    next_plan_index: u64,
    plan_mode: PlanMode,
    visible_plan: Option<VisiblePlanState>,
    retained: Vec<VisiblePlanState>,
    visible_work: Vec<WorkItem>,
    completed: Vec<CompletedWorkPlan>,
    registry_view: PlanRegistryViewState,
    events: Vec<PlanEvent>,
    completion_ledger: Vec<crate::conversation::CompletionLedgerEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HostStateCheckpointV1 {
    checkpoint_schema_version: u16,
    session_id: String,
    stream_id: Option<Uuid>,
    host_state_revision: u64,
    source_frontier: Option<SourceFrontierV1>,
    saved_at: String,
    intent: HostIntentV1,
    plans: HostPlansV1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HostStateCursorV1 {
    cursor_schema_version: u16,
    session_id: String,
    stream_id: Option<Uuid>,
    host_state_revision: u64,
    source_frontier: Option<SourceFrontierV1>,
    checkpoint_digest_algorithm: DigestAlgorithmV1,
    checkpoint_digest: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum DigestAlgorithmV1 {
    Sha256,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservationSourceFrontierV1 {
    stream_id: Uuid,
    sequence: u64,
    event_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservationResultV1 {
    content_refs: Vec<ContentRef>,
    is_error: bool,
    exit_code: i64,
    duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservationRecordV1 {
    record_schema_version: u16,
    observation_id: Uuid,
    session_id: String,
    ledger_sequence: u64,
    source_frontier: Option<ObservationSourceFrontierV1>,
    execution_id: String,
    tool_name: String,
    arguments_ref: ContentRef,
    cwd: PathBuf,
    result: ObservationResultV1,
    origin: omegon_traits::ToolExecutionOrigin,
    observed_at: String,
    legacy_compatibility_import: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CatalogLineageV1 {
    Legacy,
    Mixed,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CatalogAvailabilityV1 {
    LegacyCompatibility,
    ExactSuffix,
    Exact,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionCatalogRecordV1 {
    catalog_schema_version: u16,
    session_id: String,
    workspace_identity: String,
    metadata_revision: u64,
    friendly_name: Option<String>,
    description: Option<String>,
    created_at: String,
    turns: u64,
    tool_calls: u64,
    last_prompt_snippet: Option<String>,
    lineage: CatalogLineageV1,
    availability: CatalogAvailabilityV1,
    semantic_frontier: Option<ObservationSourceFrontierV1>,
    source_selection: String,
}

pub(crate) fn save_full_spine(
    binding: &SessionStorageBinding,
    conversation: &crate::conversation::ConversationState,
    legacy_meta: Option<&SessionMeta>,
) -> Result<()> {
    ensure_parent(&binding.snapshot)?;
    let current = read_host(binding, false)?;
    let unchanged = current.as_ref().is_some_and(|value| {
        value.stream_id == binding.stream_id
            && value.source_frontier == binding.source_frontier
            && serde_json::to_value(&value.intent).ok()
                == serde_json::to_value(checkpoint_intent(&conversation.intent)).ok()
            && serde_json::to_value(&value.plans).ok()
                == serde_json::to_value(checkpoint_plans(&conversation.intent)).ok()
    });
    if !unchanged {
        let revision = current
            .as_ref()
            .map_or(1, |value| value.host_state_revision + 1);
        let checkpoint = checkpoint_from_intent(binding, &conversation.intent, revision);
        let bytes = canonical_bytes(&checkpoint)?;
        if bytes.len() as u64 > MAX_HOST_BYTES {
            bail!("host-state checkpoint exceeds 4 MiB");
        }
        let digest = format!("{:x}", Sha256::digest(&bytes));
        secure_atomic_replace(&host_path(&binding.snapshot), &bytes)?;
        let cursor = HostStateCursorV1 {
            cursor_schema_version: HOST_SCHEMA_VERSION,
            session_id: binding.session_id.clone(),
            stream_id: binding.stream_id,
            host_state_revision: revision,
            source_frontier: binding.source_frontier.clone(),
            checkpoint_digest_algorithm: DigestAlgorithmV1::Sha256,
            checkpoint_digest: digest,
        };
        secure_atomic_replace(
            &host_cursor_path(&binding.snapshot),
            &canonical_bytes(&cursor)?,
        )?;
    }

    for observation in conversation.operator_tool_observations() {
        append_observation_internal(binding, observation, true)?;
    }
    refresh_catalog(binding, legacy_meta)?;
    Ok(())
}

pub(crate) fn has_authority(snapshot: &Path) -> Result<bool> {
    exists_regular(&adjacent(snapshot, "authority.jsonl"))
}

pub(crate) fn load_compatibility_pair(
    snapshot: &Path,
) -> Result<(crate::conversation::ConversationState, SessionMeta)> {
    let metadata = read_strict(&snapshot.with_extension("meta.json"), MAX_CATALOG_BYTES)?;
    let bytes = read_bounded(snapshot, MAX_COMPATIBILITY_BYTES)?;
    let conversation =
        crate::conversation::ConversationState::load_session_bytes(snapshot, &bytes)?;
    Ok((conversation, metadata))
}

pub(crate) fn compatibility_pair_required(binding: &SessionStorageBinding) -> Result<bool> {
    let replay = SessionReplay::replay_prefix(
        &binding.snapshot,
        &binding.session_id,
        binding
            .stream_id
            .context("session has no semantic stream")?,
        ReplayEnd::EndOfStream,
    )?;
    Ok(match replay.lineage_level() {
        AuthorityLineageLevel::FullSpine => false,
        AuthorityLineageLevel::Mixed => !has_materialized_legacy_base(&replay),
        AuthorityLineageLevel::LegacyOnly => true,
    })
}

pub(crate) fn append_observation(
    binding: &SessionStorageBinding,
    observation: &OperatorToolObservation,
) -> Result<()> {
    if binding.stream_id.is_none() || binding.source_frontier.is_none() {
        bail!("operator observation requires semantic session identity and frontier");
    }
    append_observation_internal(binding, observation, false)
}

fn append_observation_internal(
    binding: &SessionStorageBinding,
    observation: &OperatorToolObservation,
    legacy_import: bool,
) -> Result<()> {
    let mut bounded_observation = observation.clone();
    bounded_observation.bound_in_place();
    let observation = &bounded_observation;
    ensure_parent(&binding.snapshot)?;
    let path = observations_path(&binding.snapshot);
    let records = read_observations(binding)?;
    if let Some(existing) = records
        .iter()
        .find(|record| record.execution_id == observation.execution_id)
    {
        if observation_matches(&binding.snapshot, existing, observation)? {
            secure_atomic_replace(
                &observations_marker_path(&binding.snapshot),
                OBSERVATION_MARKER,
            )?;
            return Ok(());
        }
        bail!("observation execution identity was reused with conflicting content");
    }
    let store = SessionBlobStore::at(blob_path(&binding.snapshot));
    let arguments = canonical_bytes(&observation.arguments)?;
    let arguments_ref = store.write(&arguments, "application/json", ProjectionClass::Default)?;
    let mut content_refs = Vec::new();
    for content in &observation.content {
        let bytes = canonical_bytes(content)?;
        content_refs.push(store.write(&bytes, "application/json", ProjectionClass::Default)?);
    }
    let sequence = u64::try_from(records.len())?
        .checked_add(1)
        .context("observation sequence overflow")?;
    let observation_id = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("omegon:{}:{}", binding.session_id, observation.execution_id).as_bytes(),
    );
    let record = ObservationRecordV1 {
        record_schema_version: OBSERVATION_SCHEMA_VERSION,
        observation_id,
        session_id: binding.session_id.clone(),
        ledger_sequence: sequence,
        source_frontier: binding.stream_id.zip(binding.source_frontier.as_ref()).map(
            |(stream_id, frontier)| ObservationSourceFrontierV1 {
                stream_id,
                sequence: frontier.sequence,
                event_id: frontier.event_id,
            },
        ),
        execution_id: observation.execution_id.clone(),
        tool_name: observation.tool_name.clone(),
        arguments_ref,
        cwd: observation.cwd.clone(),
        result: ObservationResultV1 {
            content_refs,
            is_error: observation.is_error,
            exit_code: observation.exit_code,
            duration_ms: observation.duration_ms,
        },
        origin: observation.origin,
        observed_at: now(),
        legacy_compatibility_import: legacy_import,
    };
    let mut bytes = canonical_bytes(&record)?;
    bytes.push(b'\n');
    if bytes.len() > MAX_OBSERVATION_BYTES {
        bail!("observation record exceeds 1 MiB");
    }
    secure_atomic_replace(
        &observations_marker_path(&binding.snapshot),
        OBSERVATION_MARKER,
    )?;
    secure_append_sync(&path, &bytes)?;
    read_observations(binding)?;
    Ok(())
}

pub(crate) fn load_resume(
    snapshot: &Path,
    session_id: &str,
    workspace: &Path,
) -> Result<Option<(crate::conversation::ConversationState, SessionMeta)>> {
    let catalog_path = catalog_path(snapshot);
    if !exists_regular(&catalog_path)? {
        if has_authority(snapshot)? {
            if recover_pending_legacy_import(snapshot, session_id, workspace, None)? {
                return load_resume(snapshot, session_id, workspace);
            }
            let replay = SessionReplay::replay_prefix(
                snapshot,
                session_id,
                authority_stream_id(snapshot, session_id)?,
                ReplayEnd::EndOfStream,
            )?;
            if replay.first_full_spine_boundary().is_none()
                && !has_materialized_legacy_base(&replay)
                && exists_regular(snapshot)?
                && exists_regular(&snapshot.with_extension("meta.json"))?
            {
                return Ok(None);
            }
            bail!("authority-backed session is missing its required catalog");
        }
        return Ok(None);
    }
    let catalog: SessionCatalogRecordV1 = read_strict(&catalog_path, MAX_CATALOG_BYTES)?;
    validate_catalog(&catalog, session_id, workspace)?;
    let binding = SessionStorageBinding {
        snapshot: snapshot.to_path_buf(),
        session_id: session_id.into(),
        workspace_identity: workspace.to_string_lossy().into_owned(),
        stream_id: catalog
            .semantic_frontier
            .as_ref()
            .map(|value| value.stream_id),
        source_frontier: catalog
            .semantic_frontier
            .as_ref()
            .map(|value| SourceFrontierV1 {
                sequence: value.sequence,
                event_id: value.event_id,
            }),
    };
    let replay = SessionReplay::replay_prefix(
        snapshot,
        session_id,
        binding
            .stream_id
            .context("catalog has no semantic stream")?,
        ReplayEnd::EndOfStream,
    )?;
    if catalog.semantic_frontier.as_ref().is_none_or(|frontier| {
        frontier.sequence != replay.frontier().sequence()
            || frontier.event_id != replay.frontier().event_id()
    }) {
        if recover_pending_legacy_import(
            snapshot,
            session_id,
            workspace,
            catalog.semantic_frontier.as_ref(),
        )? {
            return load_resume(snapshot, session_id, workspace);
        }
        bail!("catalog semantic frontier is stale or does not identify authority EOF");
    }
    let replay_lineage = match replay.lineage_level() {
        AuthorityLineageLevel::LegacyOnly => CatalogLineageV1::Legacy,
        AuthorityLineageLevel::Mixed => CatalogLineageV1::Mixed,
        AuthorityLineageLevel::FullSpine => CatalogLineageV1::Full,
    };
    let replay_availability = match replay_lineage {
        CatalogLineageV1::Legacy => CatalogAvailabilityV1::LegacyCompatibility,
        CatalogLineageV1::Mixed => CatalogAvailabilityV1::ExactSuffix,
        CatalogLineageV1::Full => CatalogAvailabilityV1::Exact,
    };
    if catalog.lineage != replay_lineage || catalog.availability != replay_availability {
        bail!("catalog lineage or availability disagrees with semantic authority");
    }
    let has_legacy_base = has_materialized_legacy_base(&replay);
    let mut conversation = match catalog.lineage {
        CatalogLineageV1::Full => crate::conversation::ConversationState::new(),
        CatalogLineageV1::Mixed if has_legacy_base => crate::conversation::ConversationState::new(),
        CatalogLineageV1::Mixed => {
            if !exists_regular(snapshot)? {
                bail!("mixed resume requires its labeled legacy compatibility base");
            }
            load_compatibility_pair(snapshot)?.0
        }
        CatalogLineageV1::Legacy if exists_regular(snapshot)? => {
            load_compatibility_pair(snapshot)?.0
        }
        CatalogLineageV1::Legacy => crate::conversation::ConversationState::new(),
    };
    if let Some(host) = read_host(&binding, true)? {
        restore_host(&mut conversation.intent, host);
    }
    read_observations(&binding)?;
    let meta = catalog_to_meta(&catalog);
    conversation.intent.stats.turns = meta.turns;
    conversation.intent.stats.tool_calls = meta.tool_calls;
    conversation.intent.stats.tokens_consumed = 0;
    conversation.intent.stats.compactions = 0;
    Ok(Some((conversation, meta)))
}

fn has_materialized_legacy_base(replay: &SessionReplay) -> bool {
    replay.records().iter().any(|record| {
        matches!(
            record.payload(),
            SessionFactPayload::ContextSourceMaterialized(source)
                if crate::session_authority::is_legacy_compatibility_source(source)
        )
    })
}

fn recover_pending_legacy_import(
    snapshot: &Path,
    session_id: &str,
    workspace: &Path,
    catalog_frontier: Option<&ObservationSourceFrontierV1>,
) -> Result<bool> {
    if !exists_regular(snapshot)? || !exists_regular(&snapshot.with_extension("meta.json"))? {
        return Ok(false);
    }
    let replay = SessionReplay::replay_prefix(
        snapshot,
        session_id,
        authority_stream_id(snapshot, session_id)?,
        ReplayEnd::EndOfStream,
    )?;
    let Some(boundary) = replay.first_full_spine_boundary() else {
        return Ok(false);
    };
    let legacy_sources = replay
        .records()
        .iter()
        .filter(|record| {
            matches!(
                record.payload(),
                SessionFactPayload::ContextSourceMaterialized(source)
                    if crate::session_authority::is_legacy_compatibility_source(source)
            )
        })
        .collect::<Vec<_>>();
    let Some(import) = legacy_sources.first() else {
        return Ok(false);
    };
    if legacy_sources.len() != 1
        || replay.lineage_level() != AuthorityLineageLevel::Mixed
        || boundary.sequence() != import.frontier().sequence()
        || replay.frontier().sequence() != import.frontier().sequence()
    {
        return Ok(false);
    }
    if let Some(frontier) = catalog_frontier {
        let catalog_record = replay
            .records()
            .iter()
            .find(|record| record.frontier().sequence() == frontier.sequence);
        if catalog_record.is_none_or(|record| record.frontier().event_id() != frontier.event_id)
            || frontier.sequence >= import.frontier().sequence()
        {
            return Ok(false);
        }
    }
    let (conversation, metadata) = load_compatibility_pair(snapshot)?;
    let normalized_metadata =
        omegon_maintenance_contracts::normalize_workspace_path(metadata.cwd.as_bytes())
            .map_err(anyhow::Error::msg)?;
    let normalized_workspace = omegon_maintenance_contracts::normalize_workspace_path(
        workspace.as_os_str().as_encoded_bytes(),
    )
    .map_err(anyhow::Error::msg)?;
    if metadata.session_id != session_id || normalized_metadata != normalized_workspace {
        bail!("pending legacy import metadata identity mismatch");
    }
    if conversation.turn_count() != metadata.turns
        || conversation.intent.stats.tool_calls != metadata.tool_calls
    {
        bail!("pending legacy import compatibility pair is internally inconsistent");
    }
    let compatibility = conversation.build_llm_view();
    let legacy = crate::session_authority::legacy_compatibility_prefix(&replay, &compatibility);
    let expected = crate::session_authority::legacy_compatibility_base_bytes(legacy)?;
    let SessionFactPayload::ContextSourceMaterialized(source) = import.payload() else {
        unreachable!("legacy source filter selected a different event type");
    };
    if SessionBlobStore::at(blob_path(snapshot))
        .read(&source.content_ref, ProjectionClass::Default)?
        != expected
    {
        bail!("pending legacy import no longer matches its compatibility pair");
    }
    let binding = SessionStorageBinding::discover(snapshot, session_id, workspace)?;
    let host = read_host(&binding, true)?.context("host-state publication is missing")?;
    if host.source_frontier != binding.source_frontier {
        bail!("pending legacy import host-state frontier is not authority EOF");
    }
    read_observations(&binding)?;
    refresh_catalog(&binding, None)?;
    Ok(true)
}

pub(crate) fn list_catalogs(dir: &Path, workspace: &Path) -> Vec<SessionEntry> {
    let mut entries = Vec::new();
    let Ok(children) = fs::read_dir(dir) else {
        return entries;
    };
    for child in children.flatten() {
        let path = child.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(id) = name.strip_suffix(".catalog.v1.json") else {
            continue;
        };
        if !crate::session::is_canonical_session_id(id) {
            continue;
        }
        let Ok(catalog) = read_strict::<SessionCatalogRecordV1>(&path, MAX_CATALOG_BYTES) else {
            continue;
        };
        if validate_catalog(&catalog, id, workspace).is_err() {
            continue;
        }
        let source = match catalog.lineage {
            CatalogLineageV1::Full => SessionSource::FullSpine,
            CatalogLineageV1::Mixed => SessionSource::Mixed,
            CatalogLineageV1::Legacy => SessionSource::LegacyCompatibility,
        };
        entries.push(SessionEntry {
            path: dir.join(format!("{id}.json")),
            meta: catalog_to_meta(&catalog),
            source,
        });
    }
    entries
}

pub(crate) fn validate_replacement_target(
    snapshot: &Path,
    session_id: &str,
    workspace: &Path,
    projection: &ProjectionBinding,
) -> Result<()> {
    let Some((_, meta)) = load_resume(snapshot, session_id, workspace)? else {
        return Ok(()); // legacy target remains supported through Slice 5.6
    };
    if meta.session_id != projection.session_id {
        bail!("catalog and projection identities differ");
    }
    let binding = SessionStorageBinding {
        snapshot: snapshot.into(),
        session_id: session_id.into(),
        workspace_identity: workspace.to_string_lossy().into_owned(),
        stream_id: Some(projection.stream_id),
        source_frontier: Some(SourceFrontierV1 {
            sequence: projection.last_sequence,
            event_id: projection.last_event_id,
        }),
    };
    read_host(&binding, true)?;
    read_observations(&binding)?;
    Ok(())
}

fn checkpoint_from_intent(
    binding: &SessionStorageBinding,
    value: &IntentDocument,
    revision: u64,
) -> HostStateCheckpointV1 {
    HostStateCheckpointV1 {
        checkpoint_schema_version: HOST_SCHEMA_VERSION,
        session_id: binding.session_id.clone(),
        stream_id: binding.stream_id,
        host_state_revision: revision,
        source_frontier: binding.source_frontier.clone(),
        saved_at: now(),
        intent: checkpoint_intent(value),
        plans: checkpoint_plans(value),
    }
}

fn checkpoint_intent(value: &IntentDocument) -> HostIntentV1 {
    HostIntentV1 {
        current_task: value.current_task.clone(),
        approach: value.approach.clone(),
        lifecycle_phase: value.lifecycle_phase.clone(),
        task_mode: value.task_mode,
        task_mode_pinned: value.task_mode_pinned,
        commit_nudged: value.commit_nudged,
        skill_completion_nudged: value.skill_completion_nudged,
        plan_reconciliation_fingerprint: value.plan_reconciliation_fingerprint,
        plan_reconciliation_nudges: value.plan_reconciliation_nudges,
        mcq_detected: value.mcq_detected,
        obfuscation_detected: value.obfuscation_detected,
        operator_correction_pending: value.operator_correction_pending,
        pending_action: value.pending_action.clone(),
        constraints_discovered: value.constraints_discovered.clone(),
        failed_approaches: value.failed_approaches.clone(),
        open_questions: value.open_questions.clone(),
        files_read: value.files_read.iter().cloned().collect(),
        files_modified: value.files_modified.iter().cloned().collect(),
        evidence: value.evidence_ledger.clone(),
    }
}

fn checkpoint_plans(value: &IntentDocument) -> HostPlansV1 {
    HostPlansV1 {
        next_plan_index: value.next_plan_index,
        plan_mode: value.plan_mode,
        visible_plan: value.visible_plan.clone(),
        retained: value.retained_session_plans.clone(),
        visible_work: value.work_plan.clone(),
        completed: value.completed_work_plans.clone(),
        registry_view: value.plan_registry_view.clone(),
        events: value.plan_events.clone(),
        completion_ledger: value.completion_ledger.clone(),
    }
}

fn restore_host(target: &mut IntentDocument, checkpoint: HostStateCheckpointV1) {
    let intent = checkpoint.intent;
    target.current_task = intent.current_task;
    target.approach = intent.approach;
    target.lifecycle_phase = intent.lifecycle_phase;
    target.task_mode = intent.task_mode;
    target.task_mode_pinned = intent.task_mode_pinned;
    target.commit_nudged = intent.commit_nudged;
    target.skill_completion_nudged = intent.skill_completion_nudged;
    target.plan_reconciliation_fingerprint = intent.plan_reconciliation_fingerprint;
    target.plan_reconciliation_nudges = intent.plan_reconciliation_nudges;
    target.mcq_detected = intent.mcq_detected;
    target.obfuscation_detected = intent.obfuscation_detected;
    target.operator_correction_pending = intent.operator_correction_pending;
    target.pending_action = intent.pending_action;
    target.constraints_discovered = intent.constraints_discovered;
    target.failed_approaches = intent.failed_approaches;
    target.open_questions = intent.open_questions;
    target.files_read = intent.files_read.into_iter().collect();
    target.files_modified = intent.files_modified.into_iter().collect();
    target.evidence_ledger = intent.evidence;
    let plans = checkpoint.plans;
    target.next_plan_index = plans.next_plan_index;
    target.plan_mode = plans.plan_mode;
    target.visible_plan = plans.visible_plan;
    target.retained_session_plans = plans.retained;
    target.work_plan = plans.visible_work;
    target.completed_work_plans = plans.completed;
    target.plan_registry_view = plans.registry_view;
    target.plan_events = plans.events;
    target.completion_ledger = plans.completion_ledger;
    // SessionStatsAccumulator is deliberately untouched: semantic counters decay/reduce elsewhere.
}

fn read_host(
    binding: &SessionStorageBinding,
    required: bool,
) -> Result<Option<HostStateCheckpointV1>> {
    let output_path = host_path(&binding.snapshot);
    let cursor_path = host_cursor_path(&binding.snapshot);
    let output_exists = exists_regular(&output_path)?;
    let cursor_exists = exists_regular(&cursor_path)?;
    if !output_exists && !cursor_exists {
        if required {
            bail!("host-state publication is missing");
        }
        return Ok(None);
    }
    if output_exists != cursor_exists {
        bail!("host-state output/cursor publication is incomplete");
    }
    let output: HostStateCheckpointV1 = read_strict(&output_path, MAX_HOST_BYTES)?;
    let cursor: HostStateCursorV1 = read_strict(&cursor_path, MAX_CATALOG_BYTES)?;
    if output.checkpoint_schema_version != HOST_SCHEMA_VERSION
        || cursor.cursor_schema_version != HOST_SCHEMA_VERSION
        || output.session_id != binding.session_id
        || cursor.session_id != binding.session_id
        || output.stream_id != binding.stream_id
        || cursor.stream_id != binding.stream_id
        || output.host_state_revision != cursor.host_state_revision
        || output.source_frontier != cursor.source_frontier
    {
        bail!("host-state identity, version, revision, or frontier mismatch");
    }
    let digest = format!("{:x}", Sha256::digest(canonical_bytes(&output)?));
    if digest != cursor.checkpoint_digest {
        bail!("host-state checkpoint digest mismatch");
    }
    if let (Some(saved), Some(current)) = (&output.source_frontier, &binding.source_frontier)
        && (saved.sequence > current.sequence
            || saved.sequence == current.sequence && saved.event_id != current.event_id)
    {
        bail!("host-state frontier is impossible for authority");
    }
    if let (Some(stream_id), Some(saved)) = (binding.stream_id, &output.source_frontier) {
        let replay = SessionReplay::replay_prefix(
            &binding.snapshot,
            &binding.session_id,
            stream_id,
            ReplayEnd::Sequence(saved.sequence),
        )?;
        if replay.frontier().event_id() != saved.event_id {
            bail!("host-state frontier does not identify an authority event");
        }
    }
    Ok(Some(output))
}

fn read_observations(binding: &SessionStorageBinding) -> Result<Vec<ObservationRecordV1>> {
    let path = observations_path(&binding.snapshot);
    let marker_path = observations_marker_path(&binding.snapshot);
    let marker_exists = exists_regular(&marker_path)?;
    if marker_exists {
        let marker = open_regular(&marker_path)?;
        let mut bytes = Vec::new();
        marker.take(64).read_to_end(&mut bytes)?;
        if bytes != OBSERVATION_MARKER {
            bail!("observation ledger existence marker is invalid");
        }
    }
    if !exists_regular(&path)? {
        if marker_exists {
            bail!("observation ledger is missing after durable existence publication");
        }
        return Ok(Vec::new());
    }
    let mut file = open_regular(&path)?;
    if file.metadata()?.len() > 64 * 1024 * 1024 {
        bail!("observation ledger exceeds 64 MiB");
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    if !bytes.is_empty() && !bytes.ends_with(b"\n") {
        bail!("observation ledger has a torn final record");
    }
    let store = SessionBlobStore::at(blob_path(&binding.snapshot));
    let mut records = Vec::new();
    let mut observation_ids = HashSet::new();
    let mut execution_ids = HashSet::new();
    for line in bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        if line.len() > MAX_OBSERVATION_BYTES {
            bail!("observation record exceeds 1 MiB");
        }
        let record: ObservationRecordV1 = strict_json(line)?;
        let expected = u64::try_from(records.len())? + 1;
        if record.record_schema_version != OBSERVATION_SCHEMA_VERSION
            || record.session_id != binding.session_id
            || record.ledger_sequence != expected
            || !observation_ids.insert(record.observation_id)
            || !execution_ids.insert(record.execution_id.clone())
        {
            bail!("observation ledger identity or sequence is invalid");
        }
        store.validate(&record.arguments_ref, ProjectionClass::Default)?;
        for content in &record.result.content_refs {
            store.validate(content, ProjectionClass::Default)?;
        }
        records.push(record);
    }
    Ok(records)
}

fn refresh_catalog(binding: &SessionStorageBinding, legacy: Option<&SessionMeta>) -> Result<()> {
    let existing = if exists_regular(&catalog_path(&binding.snapshot))? {
        Some(read_strict::<SessionCatalogRecordV1>(
            &catalog_path(&binding.snapshot),
            MAX_CATALOG_BYTES,
        )?)
    } else {
        None
    };
    let replay = SessionReplay::replay_prefix(
        &binding.snapshot,
        &binding.session_id,
        binding
            .stream_id
            .context("full-spine catalog requires stream identity")?,
        ReplayEnd::EndOfStream,
    )?;
    let turns = replay
        .records()
        .iter()
        .filter(|record| matches!(record.payload(), SessionFactPayload::TurnStarted(_)))
        .count() as u64;
    let tool_calls = replay
        .records()
        .iter()
        .filter(|record| matches!(record.payload(), SessionFactPayload::ToolCallRecorded(_)))
        .count() as u64;
    let lineage = match replay.lineage_level() {
        AuthorityLineageLevel::LegacyOnly => CatalogLineageV1::Legacy,
        AuthorityLineageLevel::Mixed => CatalogLineageV1::Mixed,
        AuthorityLineageLevel::FullSpine => CatalogLineageV1::Full,
    };
    let availability = match lineage {
        CatalogLineageV1::Legacy => CatalogAvailabilityV1::LegacyCompatibility,
        CatalogLineageV1::Mixed => CatalogAvailabilityV1::ExactSuffix,
        CatalogLineageV1::Full => CatalogAvailabilityV1::Exact,
    };
    let last_prompt_snippet = replay.records().iter().rev().find_map(|record| {
        if let SessionFactPayload::PromptAdmitted(prompt) = record.payload() {
            Some(crate::util::truncate(&prompt.content.text, 80))
        } else {
            None
        }
    });
    let catalog = SessionCatalogRecordV1 {
        catalog_schema_version: CATALOG_SCHEMA_VERSION,
        session_id: binding.session_id.clone(),
        workspace_identity: binding.workspace_identity.clone(),
        metadata_revision: existing.as_ref().map_or(1, |value| value.metadata_revision),
        friendly_name: existing
            .as_ref()
            .and_then(|value| value.friendly_name.clone())
            .or_else(|| {
                legacy
                    .map(|value| value.friendly_name.trim().to_string())
                    .filter(|value| !value.is_empty())
            }),
        description: existing
            .as_ref()
            .and_then(|value| value.description.clone())
            .or_else(|| {
                legacy
                    .map(|value| value.description.trim().to_string())
                    .filter(|value| !value.is_empty())
            }),
        created_at: existing
            .as_ref()
            .map(|value| value.created_at.clone())
            .or_else(|| legacy.map(|value| value.created_at.clone()))
            .unwrap_or_else(now),
        turns,
        tool_calls,
        last_prompt_snippet,
        lineage,
        availability,
        semantic_frontier: Some(ObservationSourceFrontierV1 {
            stream_id: replay.frontier().stream_id(),
            sequence: replay.frontier().sequence(),
            event_id: replay.frontier().event_id(),
        }),
        source_selection: "semantic_authority_plus_host_stores".into(),
    };
    secure_atomic_replace(
        &catalog_path(&binding.snapshot),
        &canonical_bytes(&catalog)?,
    )
}

fn observation_matches(
    snapshot: &Path,
    existing: &ObservationRecordV1,
    observation: &OperatorToolObservation,
) -> Result<bool> {
    if existing.tool_name != observation.tool_name
        || existing.cwd != observation.cwd
        || existing.result.is_error != observation.is_error
        || existing.result.exit_code != observation.exit_code
        || existing.result.duration_ms != observation.duration_ms
        || existing.origin != observation.origin
        || existing.result.content_refs.len() != observation.content.len()
    {
        return Ok(false);
    }
    let store = SessionBlobStore::at(blob_path(snapshot));
    if store.read(&existing.arguments_ref, ProjectionClass::Default)?
        != canonical_bytes(&observation.arguments)?
    {
        return Ok(false);
    }
    for (content_ref, content) in existing
        .result
        .content_refs
        .iter()
        .zip(&observation.content)
    {
        if store.read(content_ref, ProjectionClass::Default)? != canonical_bytes(content)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn validate_catalog(catalog: &SessionCatalogRecordV1, id: &str, workspace: &Path) -> Result<()> {
    if catalog.catalog_schema_version != CATALOG_SCHEMA_VERSION
        || catalog.session_id != id
        || catalog.workspace_identity != workspace.to_string_lossy()
    {
        bail!("catalog version, session, or workspace identity mismatch");
    }
    if catalog.source_selection != "semantic_authority_plus_host_stores" {
        bail!("catalog source selection is unsupported");
    }
    Ok(())
}

fn catalog_to_meta(value: &SessionCatalogRecordV1) -> SessionMeta {
    SessionMeta {
        session_id: value.session_id.clone(),
        cwd: value.workspace_identity.clone(),
        created_at: value.created_at.clone(),
        turns: u32::try_from(value.turns).unwrap_or(u32::MAX),
        tool_calls: u32::try_from(value.tool_calls).unwrap_or(u32::MAX),
        description: value.description.clone().unwrap_or_default(),
        friendly_name: value.friendly_name.clone().unwrap_or_default(),
        last_prompt_snippet: value.last_prompt_snippet.clone().unwrap_or_default(),
    }
}
fn host_path(path: &Path) -> PathBuf {
    adjacent(path, "host-state.v1.json")
}
fn host_cursor_path(path: &Path) -> PathBuf {
    adjacent(path, "host-state.cursor.v1.json")
}
fn observations_path(path: &Path) -> PathBuf {
    adjacent(path, "observations.v1.jsonl")
}
fn observations_marker_path(path: &Path) -> PathBuf {
    adjacent(path, "observations.v1.exists")
}
fn catalog_path(path: &Path) -> PathBuf {
    adjacent(path, "catalog.v1.json")
}
fn blob_path(path: &Path) -> PathBuf {
    adjacent(path, "authority.blobs")
}
fn adjacent(path: &Path, suffix: &str) -> PathBuf {
    path.with_file_name(format!(
        "{}.{}",
        path.file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or_default(),
        suffix
    ))
}
fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn authority_stream_id(snapshot: &Path, session_id: &str) -> Result<Uuid> {
    let path = adjacent(snapshot, "authority.jsonl");
    let mut file = open_regular(&path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let line = bytes
        .split(|byte| *byte == b'\n')
        .find(|line| !line.is_empty())
        .context("authority stream is empty")?;
    #[derive(Deserialize)]
    struct Identity {
        session_id: String,
        stream_id: Uuid,
    }
    let identity: Identity = serde_json::from_slice(line)?;
    if identity.session_id != session_id {
        bail!("authority session identity mismatch");
    }
    Ok(identity.stream_id)
}

fn canonical_bytes(value: &(impl Serialize + ?Sized)) -> Result<Vec<u8>> {
    fn sort(value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(map) => serde_json::Value::Object(
                map.into_iter()
                    .map(|(key, value)| (key, sort(value)))
                    .collect(),
            ),
            serde_json::Value::Array(values) => {
                serde_json::Value::Array(values.into_iter().map(sort).collect())
            }
            other => other,
        }
    }
    Ok(serde_json::to_vec(&sort(serde_json::to_value(value)?))?)
}
fn strict_json<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    let mut parser = serde_json::Deserializer::from_slice(bytes);
    let value = T::deserialize(&mut parser)?;
    parser.end()?;
    Ok(value)
}
fn read_strict<T: DeserializeOwned>(path: &Path, max: u64) -> Result<T> {
    strict_json(&read_bounded(path, max)?)
}

fn read_bounded(path: &Path, max: u64) -> Result<Vec<u8>> {
    let mut file = open_regular(path)?;
    if file.metadata()?.len() > max {
        bail!("session store exceeds its byte bound");
    }
    let mut bytes = Vec::new();
    (&mut file).take(max + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max {
        bail!("session store exceeds its byte bound");
    }
    Ok(bytes)
}

fn ensure_parent(path: &Path) -> Result<()> {
    let parent = path.parent().context("session path has no parent")?;
    if !parent.exists() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(parent)?;
        }
        #[cfg(not(unix))]
        fs::create_dir_all(parent)?;
    }
    if !fs::symlink_metadata(parent)?.file_type().is_dir() {
        bail!("session parent is not a real directory");
    }
    Ok(())
}
fn exists_regular(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_file() => Ok(true),
        Ok(_) => bail!("session store is not a regular file: {}", path.display()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}
fn open_regular(path: &Path) -> Result<File> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() {
        bail!("session store is not a regular file");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            bail!(
                "session store permissions are not restrictive: {}",
                path.display()
            );
        }
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        bail!("session store changed type while opening");
    }
    Ok(file)
}
fn create_restricted(path: &Path, append: bool) -> Result<File> {
    let mut options = OpenOptions::new();
    options
        .write(true)
        .create(true)
        .append(append)
        .truncate(!append);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    Ok(options.open(path)?)
}
fn secure_append_sync(path: &Path, bytes: &[u8]) -> Result<()> {
    if path.exists() {
        exists_regular(path)?;
    }
    let mut file = create_restricted(path, true)?;
    file.write_all(bytes)?;
    file.flush()?;
    file.sync_all()?;
    sync_parent(path)
}
fn secure_atomic_replace(path: &Path, bytes: &[u8]) -> Result<()> {
    ensure_parent(path)?;
    if path.exists() {
        exists_regular(path)?;
    }
    let parent = path.parent().unwrap();
    let temporary = parent.join(format!(".session-tmp-{}", Uuid::new_v4()));
    let result = (|| {
        let mut file = create_restricted(&temporary, false)?;
        file.write_all(bytes)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)?;
        sync_parent(path)
    })();
    let _ = fs::remove_file(temporary);
    result
}
#[cfg(unix)]
fn sync_parent(path: &Path) -> Result<()> {
    File::open(path.parent().context("store has no parent")?)?.sync_all()?;
    Ok(())
}
#[cfg(not(unix))]
fn sync_parent(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, PathBuf, String, SessionAuthorityHandle) {
        let dir = tempfile::tempdir().unwrap();
        let id = "2026-08-22T12-00-00_deadbeef".to_string();
        let snapshot = dir.path().join(format!("{id}.json"));
        let authority = crate::session_authority::SessionAuthority::open(
            &snapshot,
            &id,
            crate::workspace::runtime::workspace_id_from_path(dir.path()),
            "test-generation",
            crate::session_authority::ActorIdentity {
                principal: "test".into(),
                ingress: "test".into(),
            },
            "2026-08-22T12:00:00Z",
        )
        .unwrap();
        (dir, snapshot, id, SessionAuthorityHandle::new(authority))
    }

    fn observation(execution_id: &str) -> OperatorToolObservation {
        OperatorToolObservation {
            execution_id: execution_id.into(),
            tool_name: "bash".into(),
            arguments: serde_json::json!({"command":"true"}),
            cwd: PathBuf::from("/tmp"),
            content: vec![omegon_traits::ContentBlock::Text { text: "ok".into() }],
            is_error: false,
            exit_code: 0,
            duration_ms: 1,
            origin: omegon_traits::ToolExecutionOrigin::Agent,
        }
    }

    #[test]
    fn checkpoint_catalog_and_catalog_only_listing_round_trip() {
        let (dir, snapshot, id, authority) = fixture();
        let binding =
            SessionStorageBinding::from_authority(&snapshot, &id, Some(&authority), dir.path());
        let mut conversation = crate::conversation::ConversationState::new();
        conversation.intent.current_task = Some("retain host intent".into());
        conversation.intent.stats.turns = 99;
        let legacy = SessionMeta {
            session_id: id.clone(),
            cwd: dir.path().to_string_lossy().into_owned(),
            created_at: "created".into(),
            turns: 99,
            tool_calls: 77,
            description: "operator description".into(),
            friendly_name: "operator_name".into(),
            last_prompt_snippet: "stale mirror".into(),
        };
        save_full_spine(&binding, &conversation, Some(&legacy)).unwrap();

        let entries = list_catalogs(dir.path(), dir.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].source, SessionSource::LegacyCompatibility);
        assert_eq!(entries[0].meta.friendly_name, "operator_name");
        assert_eq!(
            entries[0].meta.turns, 0,
            "authority counters override stale mirror"
        );
        assert!(
            !snapshot.exists(),
            "catalog listing must not require the mirror"
        );

        let (loaded, meta) = load_resume(&snapshot, &id, dir.path()).unwrap().unwrap();
        assert_eq!(
            loaded.intent.current_task.as_deref(),
            Some("retain host intent")
        );
        assert_eq!(loaded.intent.stats.turns, 0);
        assert_eq!(meta.description, "operator description");
    }

    #[test]
    fn catalog_refresh_preserves_operator_metadata() {
        let (dir, snapshot, id, authority) = fixture();
        let binding =
            SessionStorageBinding::from_authority(&snapshot, &id, Some(&authority), dir.path());
        let conversation = crate::conversation::ConversationState::new();
        let first = SessionMeta {
            session_id: id.clone(),
            cwd: dir.path().to_string_lossy().into_owned(),
            created_at: "created".into(),
            turns: 0,
            tool_calls: 0,
            description: "kept".into(),
            friendly_name: "kept_name".into(),
            last_prompt_snippet: String::new(),
        };
        save_full_spine(&binding, &conversation, Some(&first)).unwrap();
        let first_checkpoint: HostStateCheckpointV1 =
            read_strict(&host_path(&snapshot), MAX_HOST_BYTES).unwrap();
        let changed = SessionMeta {
            description: "derived overwrite".into(),
            friendly_name: "derived_name".into(),
            ..first
        };
        save_full_spine(&binding, &conversation, Some(&changed)).unwrap();
        let second_checkpoint: HostStateCheckpointV1 =
            read_strict(&host_path(&snapshot), MAX_HOST_BYTES).unwrap();
        assert_eq!(first_checkpoint.host_state_revision, 1);
        assert_eq!(second_checkpoint.host_state_revision, 1);
        let catalog: SessionCatalogRecordV1 =
            read_strict(&catalog_path(&snapshot), MAX_CATALOG_BYTES).unwrap();
        assert_eq!(catalog.friendly_name.as_deref(), Some("kept_name"));
        assert_eq!(catalog.description.as_deref(), Some("kept"));
    }

    #[test]
    fn full_resume_ignores_rollback_mirror_conversation_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let id = "2026-08-22T12-00-00_deadbeef";
        let snapshot = dir.path().join(format!("{id}.json"));
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/session-semantic-v1/full-spine-crash-prefix.authority.jsonl");
        secure_atomic_replace(
            &adjacent(&snapshot, "authority.jsonl"),
            fs::read_to_string(fixture)
                .unwrap()
                .replace("fixture-session", id)
                .as_bytes(),
        )
        .unwrap();
        let binding = SessionStorageBinding::discover(&snapshot, id, dir.path()).unwrap();
        let mut host = crate::conversation::ConversationState::new();
        host.intent.current_task = Some("host-owned task".into());
        save_full_spine(&binding, &host, None).unwrap();

        let mut stale_mirror = crate::conversation::ConversationState::new();
        stale_mirror.push_user("mirror must not become full-spine history".into());
        stale_mirror.save_session(&snapshot).unwrap();

        let (loaded, _) = load_resume(&snapshot, id, dir.path()).unwrap().unwrap();
        assert!(loaded.last_user_prompt().is_empty());
        assert_eq!(
            loaded.intent.current_task.as_deref(),
            Some("host-owned task")
        );
        assert!(!compatibility_pair_required(&binding).unwrap());
    }

    #[test]
    fn materialized_mixed_resume_no_longer_requires_legacy_pair() {
        let directory = tempfile::tempdir().unwrap();
        let id = "2026-08-23T06-30-00_cafebabe";
        let snapshot = directory.path().join(format!("{id}.json"));
        let mut conversation = crate::conversation::ConversationState::new();
        conversation.push_user("legacy prompt".into());
        conversation.save_session(&snapshot).unwrap();
        let metadata = SessionMeta {
            session_id: id.into(),
            cwd: directory.path().to_string_lossy().into_owned(),
            created_at: "2026-08-23T06:30:00Z".into(),
            turns: 1,
            tool_calls: 0,
            description: String::new(),
            friendly_name: String::new(),
            last_prompt_snippet: "legacy prompt".into(),
        };
        fs::write(
            snapshot.with_extension("meta.json"),
            serde_json::to_vec(&metadata).unwrap(),
        )
        .unwrap();
        let mut authority = crate::session_authority::SessionAuthority::open(
            &snapshot,
            id,
            "workspace",
            "generation",
            crate::session_authority::ActorIdentity {
                principal: "test".into(),
                ingress: "test".into(),
            },
            "2026-08-23T06:30:00Z",
        )
        .unwrap();
        assert!(
            crate::session::import_legacy_resume(
                &mut authority,
                &conversation,
                &metadata,
                &snapshot,
                directory.path(),
                "2026-08-23T06:30:00Z",
            )
            .unwrap()
        );
        let authority = crate::session_authority::SessionAuthorityHandle::new(authority);
        let binding = SessionStorageBinding::from_authority(
            &snapshot,
            id,
            Some(&authority),
            directory.path(),
        );
        assert!(!compatibility_pair_required(&binding).unwrap());

        fs::remove_file(&snapshot).unwrap();
        fs::remove_file(snapshot.with_extension("meta.json")).unwrap();
        assert!(
            load_resume(&snapshot, id, directory.path())
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn pending_legacy_import_recovers_missing_catalog_from_valid_pair() {
        let directory = tempfile::tempdir().unwrap();
        let id = "2026-08-23T06-31-00_feedface";
        let snapshot = directory.path().join(format!("{id}.json"));
        let mut conversation = crate::conversation::ConversationState::new();
        conversation.push_user("legacy prompt".into());
        conversation.save_session(&snapshot).unwrap();
        let metadata = SessionMeta {
            session_id: id.into(),
            cwd: directory.path().to_string_lossy().into_owned(),
            created_at: "2026-08-23T06:31:00Z".into(),
            turns: conversation.turn_count(),
            tool_calls: conversation.intent.stats.tool_calls,
            description: "unbound pending description".into(),
            friendly_name: "unbound pending name".into(),
            last_prompt_snippet: "legacy prompt".into(),
        };
        fs::write(
            snapshot.with_extension("meta.json"),
            serde_json::to_vec(&metadata).unwrap(),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&snapshot, fs::Permissions::from_mode(0o600)).unwrap();
            fs::set_permissions(
                snapshot.with_extension("meta.json"),
                fs::Permissions::from_mode(0o600),
            )
            .unwrap();
        }
        let authority = crate::session_authority::SessionAuthority::open(
            &snapshot,
            id,
            "workspace",
            "generation",
            crate::session_authority::ActorIdentity {
                principal: "test".into(),
                ingress: "test".into(),
            },
            "2026-08-23T06:31:00Z",
        )
        .unwrap();
        drop(authority);
        assert!(
            load_resume(&snapshot, id, directory.path())
                .unwrap()
                .is_none()
        );
        let mut authority = crate::session_authority::SessionAuthority::open(
            &snapshot,
            id,
            "workspace",
            "generation",
            crate::session_authority::ActorIdentity {
                principal: "test".into(),
                ingress: "test".into(),
            },
            "2026-08-23T06:31:00Z",
        )
        .unwrap();
        authority
            .import_legacy_compatibility_base(
                &conversation.build_llm_view(),
                "2026-08-23T06:31:00Z",
            )
            .unwrap();
        let binding =
            SessionStorageBinding::from_open_authority(&snapshot, id, &authority, directory.path());
        save_full_spine(&binding, &conversation, Some(&metadata)).unwrap();
        let host_before = fs::read(host_path(&snapshot)).unwrap();
        fs::remove_file(catalog_path(&snapshot)).unwrap();
        drop(authority);

        assert!(!catalog_path(&snapshot).exists());
        assert!(
            load_resume(&snapshot, id, directory.path())
                .unwrap()
                .is_some()
        );
        assert!(catalog_path(&snapshot).exists());
        assert_eq!(fs::read(host_path(&snapshot)).unwrap(), host_before);
        let recovered_catalog: SessionCatalogRecordV1 =
            read_strict(&catalog_path(&snapshot), MAX_CATALOG_BYTES).unwrap();
        assert!(recovered_catalog.friendly_name.is_none());
        assert!(recovered_catalog.description.is_none());

        fs::remove_file(catalog_path(&snapshot)).unwrap();
        let mut replacement = crate::conversation::ConversationState::new();
        replacement.push_user("different legacy prompt".into());
        replacement.save_session(&snapshot).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&snapshot, fs::Permissions::from_mode(0o600)).unwrap();
        }
        let error = match load_resume(&snapshot, id, directory.path()) {
            Ok(_) => panic!("changed compatibility pair unexpectedly recovered"),
            Err(error) => error,
        };
        assert!(
            error
                .to_string()
                .contains("no longer matches its compatibility pair")
        );
    }

    #[test]
    fn observations_are_contiguous_idempotent_and_torn_tail_is_corruption() {
        let (dir, snapshot, id, authority) = fixture();
        let binding =
            SessionStorageBinding::from_authority(&snapshot, &id, Some(&authority), dir.path());
        append_observation(&binding, &observation("one")).unwrap();
        append_observation(&binding, &observation("one")).unwrap();
        let mut conflicting = observation("one");
        conflicting.arguments = serde_json::json!({"command":"false"});
        assert!(append_observation(&binding, &conflicting).is_err());
        append_observation(&binding, &observation("two")).unwrap();
        let records = read_observations(&binding).unwrap();
        assert_eq!(
            records
                .iter()
                .map(|record| record.ledger_sequence)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        secure_append_sync(&observations_path(&snapshot), b"{").unwrap();
        assert!(read_observations(&binding).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn secure_store_rejects_symlink_and_uses_restrictive_mode() {
        use std::os::unix::fs::{MetadataExt, symlink};
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        fs::write(&target, b"old").unwrap();
        let link = dir.path().join("link");
        symlink(&target, &link).unwrap();
        assert!(secure_atomic_replace(&link, b"new").is_err());
        assert_eq!(fs::read(&target).unwrap(), b"old");
        let real = dir.path().join("real");
        secure_atomic_replace(&real, b"new").unwrap();
        assert_eq!(fs::metadata(real).unwrap().mode() & 0o777, 0o600);
    }

    #[test]
    fn strict_json_rejects_unknown_and_trailing_data() {
        assert!(
            strict_json::<SourceFrontierV1>(
                br#"{"sequence":1,"event_id":"00000000-0000-0000-0000-000000000001","extra":1}"#
            )
            .is_err()
        );
        assert!(
            strict_json::<SourceFrontierV1>(
                br#"{"sequence":1,"event_id":"00000000-0000-0000-0000-000000000001"} x"#
            )
            .is_err()
        );
    }
}
