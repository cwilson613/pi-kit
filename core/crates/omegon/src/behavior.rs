//! Behavioral classification layer for the agent loop.
//!
//! Contains tool-classification predicates, drift/phase classifiers,
//! continuation pressure heuristics, auto-delegation logic, and the
//! `ControllerState` streak tracker. Extracted from `loop.rs` to keep
//! the core state machine focused on turn orchestration.

use crate::conversation::{ConversationState, TaskMode, ToolCall, ToolResultEntry};
pub(crate) use omegon_traits::ProgressSignal;
use omegon_traits::{DriftKind, OodaPhase, ProgressNudgeReason, ToolCapability, ToolDefinition};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(crate) trait BehaviorPolicyService: std::any::Any + Send + Sync {
    fn infer_unpinned_task_mode(&self, prompt: &str) -> TaskMode;
    fn assess_turn(&self, input: &BehaviorTurnInput) -> BehaviorTurnAssessment;
    fn assess_pressure(&self, input: &BehaviorPressureInput) -> BehaviorPressureAssessment;
    fn assess_text(&self, text: &str) -> BehaviorTextAssessment;
    fn message(&self, kind: BehaviorMessageKind) -> String;
}

#[derive(Debug, Default)]
pub(crate) struct DefaultBehaviorPolicy;

#[derive(Clone)]
pub(crate) struct BehaviorPolicyBinding {
    pub(crate) capability_id: omegon_traits::RuntimeCapabilityId,
    pub(crate) owner: omegon_traits::RuntimeContributionId,
    pub(crate) generation_id: omegon_traits::RuntimeContributionGenerationId,
    pub(crate) service: Arc<dyn BehaviorPolicyService>,
}

impl std::fmt::Debug for BehaviorPolicyBinding {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BehaviorPolicyBinding")
            .field("capability_id", &self.capability_id)
            .field("owner", &self.owner)
            .field("generation_id", &self.generation_id)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BehaviorToolOutcome {
    Succeeded,
    Failed,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BehaviorToolView {
    pub(crate) name: String,
    pub(crate) target: Option<PathBuf>,
    pub(crate) outcome: BehaviorToolOutcome,
    pub(crate) targeted_validation: bool,
    capabilities: BTreeSet<ToolCapability>,
}

impl BehaviorToolView {
    fn has(&self, capability: ToolCapability) -> bool {
        self.capabilities.contains(&capability)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BehaviorIntentView {
    pub(crate) task_mode: TaskMode,
    pub(crate) files_read: Vec<PathBuf>,
    pub(crate) has_modified_files: bool,
    pub(crate) low_novelty_revisit_streak: u32,
}

impl BehaviorIntentView {
    fn from_conversation(conversation: &ConversationState) -> Self {
        Self {
            task_mode: conversation.intent.task_mode,
            files_read: conversation.intent.files_read.iter().cloned().collect(),
            has_modified_files: !conversation.intent.files_modified.is_empty(),
            low_novelty_revisit_streak: conversation
                .intent
                .evidence_ledger
                .low_novelty_revisit_streak(),
        }
    }

    fn has_read(&self, path: &Path) -> bool {
        self.files_read.iter().any(|read| read == path)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct BehaviorObservationView {
    pub(crate) progress_boundary: bool,
    pub(crate) file_mutated: bool,
    pub(crate) validation_run: bool,
}

impl BehaviorObservationView {
    pub(crate) fn from_events(events: &[crate::observation::ObservationEvent]) -> Self {
        Self {
            progress_boundary: events.iter().any(|event| {
                matches!(
                    event,
                    crate::observation::ObservationEvent::ProgressBoundary { .. }
                )
            }),
            file_mutated: events.iter().any(|event| {
                matches!(
                    event,
                    crate::observation::ObservationEvent::FileMutated { .. }
                )
            }),
            validation_run: events.iter().any(|event| {
                matches!(
                    event,
                    crate::observation::ObservationEvent::ValidationRun { .. }
                )
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BehaviorTurnInput {
    pub(crate) turn: u32,
    pub(crate) constraints_before: usize,
    pub(crate) constraints_after: usize,
    pub(crate) intent: BehaviorIntentView,
    pub(crate) tools: Vec<BehaviorToolView>,
    pub(crate) observations: BehaviorObservationView,
}

impl BehaviorTurnInput {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_host(
        turn: u32,
        constraints_before: usize,
        constraints_after: usize,
        conversation: &ConversationState,
        catalog: &ToolCapabilityCatalog,
        tool_calls: &[ToolCall],
        results: &[ToolResultEntry],
        observations: &[crate::observation::ObservationEvent],
    ) -> Self {
        Self {
            turn,
            constraints_before,
            constraints_after,
            intent: BehaviorIntentView::from_conversation(conversation),
            tools: tool_views(catalog, tool_calls, results),
            observations: BehaviorObservationView::from_events(observations),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BehaviorTurnAssessment {
    pub(crate) dominant_phase: Option<OodaPhase>,
    pub(crate) drift_kind: Option<DriftKind>,
    pub(crate) progress_signal: ProgressSignal,
    pub(crate) evidence: EvidenceAssessment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BehaviorConfigView {
    pub(crate) enforce_first_turn_execution_bias: bool,
    pub(crate) slim_execution_bias: bool,
    pub(crate) tier: BehavioralTier,
}

impl BehaviorConfigView {
    pub(crate) fn from_host(config: &super::r#loop::LoopConfig) -> Self {
        Self {
            enforce_first_turn_execution_bias: config.enforce_first_turn_execution_bias,
            slim_execution_bias: is_slim_execution_bias(config),
            tier: behavioral_tier(config),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BehaviorControllerView {
    pub(crate) consecutive_tool_continuations: u32,
    pub(crate) orientation_churn_streak: u32,
    pub(crate) repeated_action_failure_streak: u32,
    pub(crate) validation_thrash_streak: u32,
    pub(crate) closure_stall_streak: u32,
    pub(crate) constraint_discovery_streak: u32,
    pub(crate) local_evidence_sufficient_streak: u32,
    pub(crate) evidence_sufficient_streak: u32,
}

impl From<&ControllerState> for BehaviorControllerView {
    fn from(controller: &ControllerState) -> Self {
        Self {
            consecutive_tool_continuations: controller.consecutive_tool_continuations,
            orientation_churn_streak: controller.orientation_churn_streak,
            repeated_action_failure_streak: controller.repeated_action_failure_streak,
            validation_thrash_streak: controller.validation_thrash_streak,
            closure_stall_streak: controller.closure_stall_streak,
            constraint_discovery_streak: controller.constraint_discovery_streak,
            local_evidence_sufficient_streak: controller.local_evidence_sufficient_streak,
            evidence_sufficient_streak: controller.evidence_sufficient_streak,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BehaviorPressureInput {
    pub(crate) turn: u32,
    pub(crate) config: BehaviorConfigView,
    pub(crate) intent: BehaviorIntentView,
    pub(crate) tools: Vec<BehaviorToolView>,
    pub(crate) dominant_phase: Option<OodaPhase>,
    pub(crate) controller: BehaviorControllerView,
}

impl BehaviorPressureInput {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_host(
        turn: u32,
        config: &super::r#loop::LoopConfig,
        conversation: &ConversationState,
        catalog: &ToolCapabilityCatalog,
        tool_calls: &[ToolCall],
        results: &[ToolResultEntry],
        dominant_phase: Option<OodaPhase>,
        controller: &ControllerState,
    ) -> Self {
        Self {
            turn,
            config: BehaviorConfigView::from_host(config),
            intent: BehaviorIntentView::from_conversation(conversation),
            tools: tool_views(catalog, tool_calls, results),
            dominant_phase,
            controller: controller.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct BehaviorPressureAssessment {
    pub(crate) first_turn_orientation_churn: bool,
    pub(crate) execution_pressure: bool,
    pub(crate) continuation_tier: Option<u8>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct BehaviorTextAssessment {
    pub(crate) substantive_interleaved_prose: bool,
    pub(crate) pathological_meta_response: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BehaviorMessageKind {
    FirstTurn(BehavioralTier),
    ExecutionPressure(BehavioralTier),
    Continuation { tier: u8, behavior: BehavioralTier },
    Evidence(BehavioralTier),
    LocalFirst(BehavioralTier),
    MetaRetry,
}

// ─── Task-mode inference ────────────────────────────────────────────────────

/// Infer the guidance task mode from the operator's prompt.
///
/// Research-style prompts (questions, explain/summarize/review requests, any
/// read-oriented ask) legitimately spend many turns in read/search without
/// mutating files, so convergence pressure must relax for them. The heuristic
/// errs strongly toward `Research`: a false `Implementation` classification
/// pushes the model to invent file-writing work the user never requested,
/// which is the worse failure mode.
pub(crate) fn explicit_task_mode_from_prompt(prompt: &str) -> Option<TaskMode> {
    let normalized = prompt.trim_start().to_lowercase();
    let first_line = normalized.lines().next().unwrap_or("").trim();
    match first_line {
        "/mode research" | "/mode: research" | "[mode: research]" => Some(TaskMode::Research),
        "/mode implementation" | "/mode: implementation" | "[mode: implementation]" => {
            Some(TaskMode::Implementation)
        }
        _ => None,
    }
}

pub(crate) fn infer_task_mode_from_prompt(prompt: &str) -> TaskMode {
    if let Some(mode) = explicit_task_mode_from_prompt(prompt) {
        return mode;
    }
    let prompt = prompt.to_lowercase();
    let starts = |w: &str| prompt.trim_start().starts_with(w);
    let research = prompt.contains('?')
        || starts("explain")
        || starts("what")
        || starts("why")
        || starts("how")
        || starts("when")
        || starts("where")
        || starts("which")
        || starts("who")
        || starts("describe")
        || starts("summarize")
        || starts("summary")
        || starts("rundown")
        || starts("overview")
        || starts("review")
        || starts("assess")
        || starts("analyze")
        || starts("compare")
        || starts("contrast")
        || starts("outline")
        || starts("discuss")
        || starts("tell me")
        || starts("show me")
        || starts("give me")
        || starts("list")
        || starts("can you")
        || starts("could you")
        || starts("do you")
        || starts("is ")
        || starts("are ")
        || starts("does")
        || starts("did")
        || starts("read")
        || starts("look")
        || starts("check")
        || starts("find")
        || starts("search")
        || starts("investigate")
        || starts("research")
        || prompt.contains(" rundown")
        || prompt.contains(" summary")
        || prompt.contains(" overview");
    if research {
        TaskMode::Research
    } else {
        TaskMode::Implementation
    }
}

// ─── Tool classification predicates ────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub(crate) struct ToolCapabilityCatalog {
    capabilities_by_name: HashMap<String, BTreeSet<ToolCapability>>,
}

impl ToolCapabilityCatalog {
    pub fn from_tool_defs(tool_defs: &[ToolDefinition]) -> Self {
        let capabilities_by_name = tool_defs
            .iter()
            .map(|def| {
                (
                    def.name.clone(),
                    def.capabilities.iter().copied().collect::<BTreeSet<_>>(),
                )
            })
            .collect();
        Self {
            capabilities_by_name,
        }
    }

    fn has(&self, tool_name: &str, capability: ToolCapability) -> bool {
        self.capabilities_by_name
            .get(tool_name)
            .is_some_and(|caps| caps.contains(&capability))
    }

    pub fn capabilities_for(&self, tool_name: &str) -> Vec<ToolCapability> {
        self.capabilities_by_name
            .get(tool_name)
            .map(|caps| caps.iter().copied().collect())
            .unwrap_or_default()
    }
}

fn tool_views(
    catalog: &ToolCapabilityCatalog,
    tool_calls: &[ToolCall],
    results: &[ToolResultEntry],
) -> Vec<BehaviorToolView> {
    tool_calls
        .iter()
        .map(|call| {
            let outcome = match results.iter().find(|result| result.call_id == call.id) {
                Some(result) if result.is_error => BehaviorToolOutcome::Failed,
                Some(_) => BehaviorToolOutcome::Succeeded,
                None => BehaviorToolOutcome::Missing,
            };
            let targeted_validation = catalog.has(&call.name, ToolCapability::Validation)
                && call
                    .arguments
                    .get("level")
                    .and_then(|value| value.as_str())
                    .unwrap_or("standard")
                    != "full"
                && (call.arguments.get("path").is_some()
                    || call
                        .arguments
                        .get("paths")
                        .and_then(|value| value.as_array())
                        .is_some_and(|paths| !paths.is_empty() && paths.len() <= 2));
            BehaviorToolView {
                name: call.name.clone(),
                target: call
                    .arguments
                    .get("path")
                    .and_then(|value| value.as_str())
                    .map(PathBuf::from),
                outcome,
                targeted_validation,
                capabilities: catalog.capabilities_for(&call.name).into_iter().collect(),
            }
        })
        .collect()
}

pub(crate) fn is_orientation_tool(catalog: &ToolCapabilityCatalog, name: &str) -> bool {
    catalog.has(name, ToolCapability::Orientation)
}

pub(crate) fn is_repo_inspection_tool(catalog: &ToolCapabilityCatalog, name: &str) -> bool {
    catalog.has(name, ToolCapability::RepoInspection)
}

pub(crate) fn is_broad_orientation_tool(catalog: &ToolCapabilityCatalog, name: &str) -> bool {
    catalog.has(name, ToolCapability::BroadOrientation)
}

pub(crate) fn is_broad_repo_inspection_tool(catalog: &ToolCapabilityCatalog, name: &str) -> bool {
    catalog.has(name, ToolCapability::BroadRepoInspection)
}

pub(crate) fn is_targeted_repo_inspection_tool(
    catalog: &ToolCapabilityCatalog,
    name: &str,
) -> bool {
    catalog.has(name, ToolCapability::TargetedRepoInspection)
}

pub(crate) fn is_mutation_tool_name(catalog: &ToolCapabilityCatalog, name: &str) -> bool {
    catalog.has(name, ToolCapability::Mutation)
}

pub(crate) fn is_validation_tool_name(catalog: &ToolCapabilityCatalog, name: &str) -> bool {
    catalog.has(name, ToolCapability::Validation)
}

pub(crate) fn is_progress_boundary_tool(catalog: &ToolCapabilityCatalog, name: &str) -> bool {
    catalog.has(name, ToolCapability::ProgressBoundary)
}

pub(crate) fn mutation_targets_within_limit(
    catalog: &ToolCapabilityCatalog,
    tool_calls: &[ToolCall],
    max_files: usize,
) -> bool {
    let mut paths = std::collections::BTreeSet::new();
    for call in tool_calls {
        if !is_mutation_tool_name(catalog, &call.name) {
            continue;
        }
        let Some(path) = call.arguments.get("path").and_then(|v| v.as_str()) else {
            return false;
        };
        paths.insert(path.to_string());
        if paths.len() > max_files {
            return false;
        }
    }
    !paths.is_empty()
}

pub(crate) fn is_narrow_patch_candidate(
    catalog: &ToolCapabilityCatalog,
    tool_calls: &[ToolCall],
) -> bool {
    if !tool_calls
        .iter()
        .any(|call| is_mutation_tool_name(catalog, &call.name))
    {
        return false;
    }
    if !mutation_targets_within_limit(catalog, tool_calls, 2) {
        return false;
    }
    tool_calls.iter().all(|call| {
        is_mutation_tool_name(catalog, &call.name)
            || is_targeted_repo_inspection_tool(catalog, &call.name)
            || is_validation_tool_name(catalog, &call.name)
    })
}

// ─── Phase & drift classification ──────────────────────────────────────────

pub(crate) fn classify_turn_phase(
    catalog: &ToolCapabilityCatalog,
    tool_calls: &[ToolCall],
    results: &[ToolResultEntry],
) -> Option<OodaPhase> {
    phase_from_view(&tool_views(catalog, tool_calls, results))
}

pub(crate) fn classify_drift_kind(
    catalog: &ToolCapabilityCatalog,
    turn: u32,
    conversation: &ConversationState,
    tool_calls: &[ToolCall],
    results: &[ToolResultEntry],
) -> Option<DriftKind> {
    let input = BehaviorTurnInput {
        turn,
        constraints_before: 0,
        constraints_after: 0,
        intent: BehaviorIntentView::from_conversation(conversation),
        tools: tool_views(catalog, tool_calls, results),
        observations: BehaviorObservationView::default(),
    };
    drift_from_view(&input)
}

pub(crate) fn progress_nudge_reason_for_drift(drift: DriftKind) -> ProgressNudgeReason {
    match drift {
        DriftKind::OrientationChurn => ProgressNudgeReason::AntiOrientation,
        DriftKind::RepeatedActionFailure => ProgressNudgeReason::ActionRecovery,
        DriftKind::ValidationThrash => ProgressNudgeReason::ValidationPressure,
        DriftKind::ClosureStall => ProgressNudgeReason::ClosurePressure,
    }
}

pub(crate) fn is_first_turn_orientation_churn(
    turn: u32,
    config: &super::r#loop::LoopConfig,
    conversation: &ConversationState,
    catalog: &ToolCapabilityCatalog,
    tool_calls: &[ToolCall],
) -> bool {
    config.enforce_first_turn_execution_bias
        && turn == 1
        && !tool_calls.is_empty()
        && tool_calls
            .iter()
            .all(|call| is_orientation_tool(catalog, &call.name))
        && conversation.intent.files_read.is_empty()
        && conversation.intent.files_modified.is_empty()
}

pub(crate) fn should_inject_execution_pressure(
    turn: u32,
    _config: &super::r#loop::LoopConfig,
    conversation: &ConversationState,
    catalog: &ToolCapabilityCatalog,
    tool_calls: &[ToolCall],
    behavior: BehavioralTier,
) -> bool {
    // Research turns legitimately read/search without mutating files; do not
    // pressure them toward edits they were never asked to make.
    if conversation.intent.task_mode == TaskMode::Research {
        return false;
    }
    if tool_calls.is_empty()
        || !conversation.intent.files_modified.is_empty()
        || conversation.intent.files_read.is_empty()
        || !tool_calls
            .iter()
            .all(|call| is_repo_inspection_tool(catalog, &call.name))
    {
        return false;
    }

    let has_broad_repo_inspection = tool_calls
        .iter()
        .any(|call| is_broad_repo_inspection_tool(catalog, &call.name));

    // Give the agent time to orient before pressuring execution.
    let (broad_threshold, targeted_threshold) = match behavior {
        BehavioralTier::Constrained => (3, 4),
        BehavioralTier::Standard => (5, 6),
    };

    (turn >= broad_threshold && has_broad_repo_inspection)
        || (turn >= targeted_threshold && !has_broad_repo_inspection)
}

// ─── Progress signals & evidence ───────────────────────────────────────────

// ProgressSignal is now defined in omegon-traits and imported above.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum EvidenceSufficiency {
    #[default]
    None,
    Targeted,
    Actionable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct EvidenceAssessment {
    pub local: EvidenceSufficiency,
    pub global: EvidenceSufficiency,
}

/// Behavioral tier for loop control. Determines pressure thresholds and nudge style.
/// Frontier/Max models get standard treatment; Mid/Leaf models get a tighter leash
/// with simpler instructions, earlier pressure, and dead-mouse detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BehavioralTier {
    /// Frontier/Max models — current defaults, multi-clause nudges
    Standard,
    /// Mid/Leaf models (Ollama, Groq, etc.) — tighter thresholds, imperative nudges
    Constrained,
}

pub(crate) fn behavioral_tier(config: &super::r#loop::LoopConfig) -> BehavioralTier {
    let tier = crate::routing::infer_model_grade_band(&config.model);
    match tier {
        crate::routing::CapabilityGradeBand::Max
        | crate::routing::CapabilityGradeBand::Frontier => BehavioralTier::Standard,
        crate::routing::CapabilityGradeBand::Mid | crate::routing::CapabilityGradeBand::Leaf => {
            BehavioralTier::Constrained
        }
    }
}

// ─── Controller state (streak tracker) ─────────────────────────────────────

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ControllerState {
    pub consecutive_tool_continuations: u32,
    pub orientation_churn_streak: u32,
    pub repeated_action_failure_streak: u32,
    pub validation_thrash_streak: u32,
    pub closure_stall_streak: u32,
    pub constraint_discovery_streak: u32,
    pub targeted_evidence_streak: u32,
    pub local_evidence_sufficient_streak: u32,
    pub evidence_sufficient_streak: u32,
    /// Consecutive tool-continuation turns without mutation, validation,
    /// constraint discovery, commit, completion, or substantive visible prose.
    pub no_progress_continuation_streak: u32,
}

/// Minimum trimmed length for interleaved assistant prose to count as
/// visible output for continuation-pressure purposes. Short narration
/// ("Checking the config...") stays below this; substantive analysis
/// delivered alongside tool calls clears it.
const SUBSTANTIVE_PROSE_MIN_CHARS: usize = 240;

/// True when the assistant text emitted alongside tool calls is substantive
/// output rather than transitional narration.
pub(crate) fn is_substantive_interleaved_prose(text: &str) -> bool {
    text.trim().len() >= SUBSTANTIVE_PROSE_MIN_CHARS
}

impl ControllerState {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Snapshot the streak counters as the public `ControllerStreaks`
    /// shape that's carried on `AgentEvent::TurnEnd`.
    pub fn streaks(&self) -> omegon_traits::ControllerStreaks {
        omegon_traits::ControllerStreaks {
            orientation_churn: self.orientation_churn_streak,
            repeated_action_failure: self.repeated_action_failure_streak,
            validation_thrash: self.validation_thrash_streak,
            closure_stall: self.closure_stall_streak,
            constraint_discovery: self.constraint_discovery_streak,
            evidence_sufficient: self.evidence_sufficient_streak,
        }
    }

    pub fn observe_turn(
        &mut self,
        turn_end_reason: omegon_traits::TurnEndReason,
        drift_kind: Option<DriftKind>,
        progress_signal: ProgressSignal,
        evidence: EvidenceAssessment,
        substantive_prose: bool,
    ) {
        match progress_signal {
            ProgressSignal::Mutation | ProgressSignal::Commit | ProgressSignal::Completion => {
                self.reset();
                return;
            }
            ProgressSignal::TargetedValidation
            | ProgressSignal::BroadValidation
            | ProgressSignal::ConstraintDiscovery => {
                self.no_progress_continuation_streak = 0;
                self.consecutive_tool_continuations /= 2;
                self.orientation_churn_streak /= 2;
                self.repeated_action_failure_streak = 0;
                self.validation_thrash_streak = 0;
                self.closure_stall_streak /= 2;
            }
            ProgressSignal::None => {
                if matches!(
                    turn_end_reason,
                    omegon_traits::TurnEndReason::ToolContinuation
                ) && !substantive_prose
                {
                    self.no_progress_continuation_streak =
                        self.no_progress_continuation_streak.saturating_add(1);
                } else {
                    self.no_progress_continuation_streak = 0;
                }
            }
        }

        if matches!(
            turn_end_reason,
            omegon_traits::TurnEndReason::ToolContinuation
        ) {
            // Substantive interleaved prose IS visible output — the operator
            // is being answered while tools run. Hold the counter instead of
            // incrementing so "exploring without producing output" pressure
            // only accrues on genuinely silent tool grinding. Short narration
            // ("Let me check X...") still counts as silent.
            if !substantive_prose {
                self.consecutive_tool_continuations =
                    self.consecutive_tool_continuations.saturating_add(1);
            }
        } else {
            self.consecutive_tool_continuations = 0;
        }

        // Drift streaks: increment on match, *decay* (halve) on mismatch
        // instead of hard-resetting.
        self.orientation_churn_streak = if matches!(drift_kind, Some(DriftKind::OrientationChurn)) {
            self.orientation_churn_streak.saturating_add(1)
        } else {
            self.orientation_churn_streak / 2
        };
        self.repeated_action_failure_streak =
            if matches!(drift_kind, Some(DriftKind::RepeatedActionFailure)) {
                self.repeated_action_failure_streak.saturating_add(1)
            } else {
                self.repeated_action_failure_streak / 2
            };
        self.validation_thrash_streak = if matches!(drift_kind, Some(DriftKind::ValidationThrash)) {
            self.validation_thrash_streak.saturating_add(1)
        } else {
            self.validation_thrash_streak / 2
        };
        self.closure_stall_streak = if matches!(drift_kind, Some(DriftKind::ClosureStall)) {
            self.closure_stall_streak.saturating_add(1)
        } else {
            self.closure_stall_streak / 2
        };
        self.constraint_discovery_streak =
            if matches!(progress_signal, ProgressSignal::ConstraintDiscovery) {
                self.constraint_discovery_streak.saturating_add(1)
            } else {
                self.constraint_discovery_streak / 2
            };
        self.targeted_evidence_streak = if matches!(
            evidence.local,
            EvidenceSufficiency::Targeted | EvidenceSufficiency::Actionable
        ) {
            self.targeted_evidence_streak.saturating_add(1)
        } else {
            self.targeted_evidence_streak / 2
        };
        self.local_evidence_sufficient_streak =
            if matches!(evidence.local, EvidenceSufficiency::Actionable) {
                self.local_evidence_sufficient_streak.saturating_add(1)
            } else {
                self.local_evidence_sufficient_streak / 2
            };
        self.evidence_sufficient_streak =
            if matches!(evidence.global, EvidenceSufficiency::Actionable) {
                self.evidence_sufficient_streak.saturating_add(1)
            } else {
                self.evidence_sufficient_streak / 2
            };
    }
}

// ─── Helpers ───────────────────────────────────────────────────────────────

pub(crate) fn has_successful_tool_call<F>(
    tool_calls: &[ToolCall],
    results: &[ToolResultEntry],
    predicate: F,
) -> bool
where
    F: Fn(&ToolCall) -> bool,
{
    tool_calls.iter().any(|call| {
        predicate(call)
            && results
                .iter()
                .find(|result| result.call_id == call.id)
                .is_some_and(|result| !result.is_error)
    })
}

pub(crate) fn has_progress_boundary(
    catalog: &ToolCapabilityCatalog,
    tool_calls: &[ToolCall],
    results: &[ToolResultEntry],
) -> bool {
    has_successful_tool_call(tool_calls, results, |call| {
        is_mutation_tool_name(catalog, &call.name)
    }) || has_successful_tool_call(tool_calls, results, |call| {
        is_validation_tool_name(catalog, &call.name)
    }) || has_successful_tool_call(tool_calls, results, |call| {
        is_progress_boundary_tool(catalog, &call.name)
    })
}

pub(crate) fn classify_validation_scope(
    catalog: &ToolCapabilityCatalog,
    tool_calls: &[ToolCall],
    results: &[ToolResultEntry],
) -> ProgressSignal {
    let successful_validation_calls: Vec<&ToolCall> = tool_calls
        .iter()
        .filter(|call| {
            is_validation_tool_name(catalog, &call.name)
                && results
                    .iter()
                    .find(|result| result.call_id == call.id)
                    .is_some_and(|result| !result.is_error)
        })
        .collect();

    if successful_validation_calls.is_empty() {
        return ProgressSignal::None;
    }

    let is_targeted = successful_validation_calls.iter().any(|call| {
        let level = call
            .arguments
            .get("level")
            .and_then(|v| v.as_str())
            .unwrap_or("standard");
        if level == "full" {
            return false;
        }

        call.arguments
            .get("paths")
            .and_then(|v| v.as_array())
            .is_some_and(|paths| !paths.is_empty() && paths.len() <= 2)
            || call.arguments.get("path").is_some()
    });

    if is_targeted {
        ProgressSignal::TargetedValidation
    } else {
        ProgressSignal::BroadValidation
    }
}

pub(crate) fn detect_constraint_discovery(
    constraints_before: usize,
    constraints_after: usize,
    catalog: &ToolCapabilityCatalog,
    tool_calls: &[ToolCall],
    results: &[ToolResultEntry],
) -> bool {
    if constraints_after <= constraints_before {
        return false;
    }

    tool_calls.iter().any(|call| {
        is_repo_inspection_tool(catalog, &call.name)
            || is_validation_tool_name(catalog, &call.name)
            || (is_mutation_tool_name(catalog, &call.name)
                && results
                    .iter()
                    .find(|result| result.call_id == call.id)
                    .is_some_and(|result| result.is_error))
    })
}

pub(crate) fn classify_progress_signal(
    constraints_before: usize,
    constraints_after: usize,
    catalog: &ToolCapabilityCatalog,
    tool_calls: &[ToolCall],
    results: &[ToolResultEntry],
) -> ProgressSignal {
    let observations =
        crate::observation::ObservationNormalizer::new(catalog).normalize(tool_calls, results);
    progress_from_view(&BehaviorTurnInput {
        turn: 0,
        constraints_before,
        constraints_after,
        intent: BehaviorIntentView::from_conversation(&ConversationState::new()),
        tools: tool_views(catalog, tool_calls, results),
        observations: BehaviorObservationView::from_events(&observations),
    })
}

pub(crate) fn assess_evidence(
    conversation: &ConversationState,
    catalog: &ToolCapabilityCatalog,
    tool_calls: &[ToolCall],
    results: &[ToolResultEntry],
) -> EvidenceAssessment {
    evidence_from_view(&BehaviorTurnInput {
        turn: 0,
        constraints_before: 0,
        constraints_after: 0,
        intent: BehaviorIntentView::from_conversation(conversation),
        tools: tool_views(catalog, tool_calls, results),
        observations: BehaviorObservationView::default(),
    })
}

pub(crate) fn is_slim_execution_bias(config: &super::r#loop::LoopConfig) -> bool {
    config
        .settings
        .as_ref()
        .and_then(|settings| settings.lock().ok().map(|s| s.is_slim()))
        .unwrap_or(false)
}

pub(crate) fn has_local_target_hypothesis(conversation: &ConversationState) -> bool {
    !conversation.intent.files_read.is_empty() && conversation.intent.files_modified.is_empty()
}

// ─── Continuation pressure ─────────────────────────────────────────────────

pub(crate) fn continuation_pressure_tier(
    config: &super::r#loop::LoopConfig,
    controller: &ControllerState,
    conversation: &ConversationState,
    tool_calls: &[ToolCall],
    dominant_phase: Option<OodaPhase>,
    behavior: BehavioralTier,
) -> Option<u8> {
    if tool_calls.is_empty()
        || !matches!(dominant_phase, Some(OodaPhase::Observe | OodaPhase::Orient))
    {
        return None;
    }

    let local_evidence_sufficient = controller.local_evidence_sufficient_streak > 0;
    let evidence_sufficient = controller.evidence_sufficient_streak > 0;
    let research_mode = conversation.intent.task_mode == TaskMode::Research;
    let om_local_first_lock = !research_mode
        && is_slim_execution_bias(config)
        && local_evidence_sufficient
        && has_local_target_hypothesis(conversation);
    let constrained = behavior == BehavioralTier::Constrained;
    let (tier1, tier2, tier3) = if research_mode {
        // Research turns legitimately spend many turns in read/search.
        // Keep only a late safety net against genuinely unbounded exploration.
        if constrained {
            (8, 12, 16)
        } else {
            (16, 24, 32)
        }
    } else if om_local_first_lock {
        if constrained { (2, 3, 5) } else { (4, 6, 8) }
    } else if evidence_sufficient {
        if constrained { (3, 4, 6) } else { (6, 8, 10) }
    } else if is_slim_execution_bias(config) {
        if constrained { (4, 6, 8) } else { (8, 12, 16) }
    } else if constrained {
        (3, 5, 7)
    } else {
        (12, 16, 20)
    };

    let continuation = controller.consecutive_tool_continuations;
    let orient = controller.orientation_churn_streak;
    let closure = controller.closure_stall_streak;
    let validation = controller.validation_thrash_streak;
    let failures = controller.repeated_action_failure_streak;
    let discoveries = controller.constraint_discovery_streak;

    if om_local_first_lock && (continuation >= tier1 || orient >= tier1 || closure >= tier1) {
        return Some(3);
    }
    if evidence_sufficient && (continuation >= tier2 || orient >= tier1 || closure >= tier1) {
        return Some(3);
    }

    if discoveries >= 2 && !research_mode {
        return Some(2);
    }

    if continuation >= tier3 || orient >= tier2 || closure >= tier2 || validation >= tier2 {
        Some(3)
    } else if continuation >= tier2 || orient >= tier1 || failures >= 2 {
        Some(2)
    } else if continuation >= tier1 {
        Some(1)
    } else {
        None
    }
}

pub(crate) fn continuation_pressure_message(tier: u8, behavior: BehavioralTier) -> String {
    // IMPORTANT: A direct text reply IS valid output. Do NOT bias toward file
    // mutations — many sessions are Q&A / explanation work where writing a
    // file is wrong (e.g. answering "summarize this doc" by creating a new
    // summary file the user never asked for). File writes are listed only as
    // an option, after answering, and only when the user explicitly asked to
    // change a file.
    match (tier, behavior) {
        (1, BehavioralTier::Constrained) => "[System: You have been exploring. Produce output now — answer the user, or state what's blocking you. Do not apologize, self-criticize, mirror operator frustration, or explain your process.]".to_string(),
        (2, BehavioralTier::Constrained) => "[System: Produce output now. Answer the user, or (only if they explicitly asked you to change a file) write/edit one. Otherwise state the blocker. Do not apologize, self-criticize, mirror operator frustration, or explain your process.]".to_string(),
        (_, BehavioralTier::Constrained) => "[System: You must produce output on this turn. Answer the user, or explain why you cannot. Do not apologize, self-criticize, mirror operator frustration, or explain your process.]".to_string(),
        (1, _) => "[System: You have spent several turns exploring without producing output. You likely have enough context. Take the next concrete step toward completing the user's request — answer them directly. If — and only if — they explicitly asked you to modify a file, do that instead. Otherwise reply in chat. Do not apologize, self-criticize, mirror operator frustration, or explain your process.]".to_string(),
        (2, _) => "[System: You are still exploring. Produce a concrete result now: answer the user's question, or (only if they explicitly asked) write/edit a file. Do not invent file-writing tasks the user did not request. Do not apologize, self-criticize, mirror operator frustration, or explain your process.]".to_string(),
        _ => "[System: You have been exploring for many turns without producing output. On this turn, you must do one of: (1) answer the user directly in chat, (2) write or edit a file ONLY if the user explicitly asked for that, or (3) tell the user exactly what is preventing you from completing the task. Do not apologize, self-criticize, mirror operator frustration, or explain your process.]".to_string(),
    }
}

// ─── Auto-delegation ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AutoDelegatePlan {
    pub worker_profile: &'static str,
    pub background: bool,
}

/// Auto-delegation is DISABLED. It was an experimental feature that
/// intercepted the agent's tool calls and dispatched them to background
/// workers. In practice, the workers frequently failed silently, causing
/// "content dispatched" messages with no actual work done. Users reported
/// this as "the agent cannot perform work" — the exact opposite of what
/// auto-delegation was supposed to achieve.
///
/// The agent should always execute its own tool calls directly.
/// Delegation is still available as an explicit tool the agent can
/// choose to call — just not as an invisible interception layer.
pub(crate) fn classify_auto_delegate_plan(
    _config: &super::r#loop::LoopConfig,
    _conversation: &ConversationState,
    _tool_calls: &[ToolCall],
    _dominant_phase: Option<OodaPhase>,
    _drift_kind: Option<DriftKind>,
) -> Option<AutoDelegatePlan> {
    None
}

pub(crate) fn evidence_sufficiency_message(behavior: BehavioralTier) -> String {
    match behavior {
        BehavioralTier::Constrained => "[System: You have enough context. Produce output now — answer the user. Do not apologize, self-criticize, mirror operator frustration, or explain your process.]".to_string(),
        BehavioralTier::Standard => "[System: You have gathered enough context to act. Produce a concrete result — answer the user's question. If they explicitly asked you to modify a file, do that. Otherwise reply in chat; do not invent file-writing work. Do not apologize, self-criticize, mirror operator frustration, or explain your process.]".to_string(),
    }
}

pub(crate) fn om_local_first_message(behavior: BehavioralTier) -> String {
    match behavior {
        BehavioralTier::Constrained => "[System: Produce output now. Do not search again. Answer the user. Do not apologize, self-criticize, mirror operator frustration, or explain your process.]".to_string(),
        BehavioralTier::Standard => "[System: You have enough context. Produce the requested output — answer the user. If they explicitly asked you to modify a file, do that; otherwise reply in chat. Do not apologize, self-criticize, mirror operator frustration, or explain your process.]".to_string(),
    }
}

pub(crate) fn operator_correction_recovery_message() -> String {
    "[System: The operator corrected your behavior. Treat this as a control signal. Do not apologize, self-criticize, mirror profanity, or explain your process. Preserve the active task, stop broad exploration, and take the smallest concrete next action. If blocked, state the blocker and the exact operator decision needed.]".to_string()
}

pub(crate) fn meta_recovery_retry_message() -> String {
    "[System: Your previous response was meta-commentary rather than task progress. Retry now with no apology, self-critique, profanity mirroring, or process narration. Take the next concrete action, answer the user's request, or state the precise blocker.]".to_string()
}

pub(crate) fn is_pathological_meta_response(text: &str) -> bool {
    let normalized = text.trim().to_lowercase();
    if normalized.is_empty() {
        return false;
    }
    let self_rebuke = [
        "i'm wasting",
        "i am wasting",
        "i've been wasting",
        "i have been wasting",
        "i was wasting",
        "my mistake was",
        "my failure was",
        "i over-investigated",
        "i over investigated",
        "i over-read",
        "i over read",
        "i should have just",
        "i should stop",
    ];
    let apology = [
        "sorry",
        "i apologize",
        "apologies",
        "you're right",
        "you are right",
        "that was wrong",
    ];
    let process_only = [
        "let me stop",
        "i'll stop exploring",
        "i will stop exploring",
        "i'll just do it",
        "i will just do it",
        "just doing it",
    ];

    let has_meta_marker = self_rebuke
        .iter()
        .chain(apology.iter())
        .chain(process_only.iter())
        .any(|marker| normalized.contains(marker));
    if !has_meta_marker {
        return false;
    }

    let has_concrete_work_marker = [
        "changed ",
        "updated ",
        "fixed ",
        "implemented ",
        "added ",
        "removed ",
        "ran ",
        "verified ",
        "tested ",
        "committed ",
        "pushed ",
        "blocked:",
        "blocker:",
    ]
    .iter()
    .any(|marker| normalized.contains(marker));

    !has_concrete_work_marker
}

// ─── Auto-delegation ───────────────────────────────────────────────────────

pub(crate) fn auto_delegate_tool_call(
    conversation: &ConversationState,
    plan: AutoDelegatePlan,
) -> ToolCall {
    // Use the tracked task from conversation intent, but validate it.
    // If the tracked task is conversational or too vague, fall back to
    // a generic orientation instruction that the delegate can work with.
    let raw_task = conversation.intent.current_task.clone().unwrap_or_default();
    let task = if raw_task.trim().is_empty()
        || crate::features::delegate::is_conversational_non_task(&raw_task)
    {
        "Inspect the current bounded task and return concise findings.".to_string()
    } else {
        raw_task
    };
    ToolCall {
        id: format!(
            "auto-delegate-{}",
            conversation.turn_count().saturating_add(1)
        ),
        name: "delegate".to_string(),
        arguments: serde_json::json!({
            "task": task,
            "background": plan.background,
            "worker_profile": plan.worker_profile,
        }),
    }
}

fn phase_from_view(tools: &[BehaviorToolView]) -> Option<OodaPhase> {
    if tools.is_empty() {
        return None;
    }
    if tools
        .iter()
        .any(|tool| tool.has(ToolCapability::StateChanging) || tool.has(ToolCapability::Validation))
    {
        return Some(OodaPhase::Act);
    }
    if tools.iter().any(|tool| tool.has(ToolCapability::Mutation)) {
        return Some(OodaPhase::Act);
    }
    if tools
        .iter()
        .all(|tool| tool.has(ToolCapability::Orientation))
        || tools
            .iter()
            .all(|tool| tool.has(ToolCapability::RepoInspection))
    {
        return Some(OodaPhase::Observe);
    }
    Some(OodaPhase::Orient)
}

fn validation_signal_from_view(tools: &[BehaviorToolView]) -> ProgressSignal {
    let successful: Vec<&BehaviorToolView> = tools
        .iter()
        .filter(|tool| {
            tool.has(ToolCapability::Validation) && tool.outcome == BehaviorToolOutcome::Succeeded
        })
        .collect();
    if successful.is_empty() {
        ProgressSignal::None
    } else if successful.iter().any(|tool| tool.targeted_validation) {
        ProgressSignal::TargetedValidation
    } else {
        ProgressSignal::BroadValidation
    }
}

fn drift_from_view(input: &BehaviorTurnInput) -> Option<DriftKind> {
    let broad_orientation = input
        .tools
        .iter()
        .filter(|tool| tool.has(ToolCapability::BroadOrientation))
        .count();
    let broad_inspection = input
        .tools
        .iter()
        .filter(|tool| tool.has(ToolCapability::BroadRepoInspection))
        .count();
    let targeted_inspection = input
        .tools
        .iter()
        .filter(|tool| tool.has(ToolCapability::TargetedRepoInspection))
        .count();
    let research = input.intent.task_mode == TaskMode::Research;

    if !research
        && !input.intent.has_modified_files
        && !input.intent.files_read.is_empty()
        && input
            .tools
            .iter()
            .all(|tool| tool.has(ToolCapability::RepoInspection))
        && input.turn >= 4
        && broad_inspection > 0
        && targeted_inspection <= 1
    {
        return Some(DriftKind::OrientationChurn);
    }
    if !research
        && !input.intent.has_modified_files
        && input.intent.files_read.is_empty()
        && input.turn >= 3
        && broad_orientation == input.tools.len()
    {
        return Some(DriftKind::OrientationChurn);
    }

    let failing_mutations: Vec<&BehaviorToolView> = input
        .tools
        .iter()
        .filter(|tool| {
            tool.has(ToolCapability::Mutation) && tool.outcome == BehaviorToolOutcome::Failed
        })
        .collect();
    if failing_mutations.len() >= 2
        && failing_mutations.iter().enumerate().any(|(index, tool)| {
            failing_mutations
                .iter()
                .enumerate()
                .any(|(other_index, other)| {
                    index != other_index && tool.name == other.name && tool.target == other.target
                })
        })
    {
        return Some(DriftKind::RepeatedActionFailure);
    }

    let validation_calls = input
        .tools
        .iter()
        .filter(|tool| tool.has(ToolCapability::Validation))
        .count();
    if validation_calls >= 2
        && !input.intent.has_modified_files
        && validation_signal_from_view(&input.tools) != ProgressSignal::TargetedValidation
    {
        return Some(DriftKind::ValidationThrash);
    }
    if input.intent.has_modified_files
        && input
            .tools
            .iter()
            .all(|tool| tool.has(ToolCapability::RepoInspection))
        && broad_inspection > 0
    {
        return Some(DriftKind::ClosureStall);
    }
    None
}

fn progress_from_view(input: &BehaviorTurnInput) -> ProgressSignal {
    if input.observations.progress_boundary {
        return ProgressSignal::Commit;
    }
    if input.observations.file_mutated {
        return ProgressSignal::Mutation;
    }
    if input.observations.validation_run {
        return ProgressSignal::TargetedValidation;
    }
    let validation = validation_signal_from_view(&input.tools);
    if validation != ProgressSignal::None {
        return validation;
    }
    if input.constraints_after > input.constraints_before
        && input.tools.iter().any(|tool| {
            tool.has(ToolCapability::RepoInspection)
                || tool.has(ToolCapability::Validation)
                || (tool.has(ToolCapability::Mutation)
                    && tool.outcome == BehaviorToolOutcome::Failed)
        })
    {
        return ProgressSignal::ConstraintDiscovery;
    }
    ProgressSignal::None
}

fn evidence_from_view(input: &BehaviorTurnInput) -> EvidenceAssessment {
    if input.intent.files_read.is_empty() {
        return EvidenceAssessment::default();
    }
    if input.intent.has_modified_files {
        return EvidenceAssessment {
            local: EvidenceSufficiency::Actionable,
            global: EvidenceSufficiency::Actionable,
        };
    }

    let targeted_validation =
        validation_signal_from_view(&input.tools) == ProgressSignal::TargetedValidation;
    let failed_mutation_on_known_target = input.tools.iter().any(|tool| {
        tool.has(ToolCapability::Mutation)
            && tool.outcome == BehaviorToolOutcome::Failed
            && tool
                .target
                .as_deref()
                .is_some_and(|target| input.intent.has_read(target))
    });
    let inspection_backed_by_validation_failure = input
        .tools
        .iter()
        .any(|tool| tool.has(ToolCapability::RepoInspection))
        && input
            .tools
            .iter()
            .any(|tool| tool.outcome == BehaviorToolOutcome::Failed)
        && input
            .tools
            .iter()
            .any(|tool| tool.has(ToolCapability::Validation));
    let targeted_reads: Vec<&Path> = input
        .tools
        .iter()
        .filter(|tool| tool.has(ToolCapability::TargetedRepoInspection))
        .filter_map(|tool| tool.target.as_deref())
        .collect();
    let narrow_target_cluster = !targeted_reads.is_empty()
        && input
            .tools
            .iter()
            .all(|tool| tool.has(ToolCapability::RepoInspection))
        && !input
            .tools
            .iter()
            .any(|tool| tool.has(ToolCapability::BroadRepoInspection));
    let targeted_paths_known = narrow_target_cluster
        && targeted_reads
            .iter()
            .all(|target| input.intent.has_read(target));
    let global = if targeted_validation
        || failed_mutation_on_known_target
        || inspection_backed_by_validation_failure
    {
        EvidenceSufficiency::Actionable
    } else {
        EvidenceSufficiency::None
    };
    if input.intent.task_mode == TaskMode::Research && global != EvidenceSufficiency::Actionable {
        return EvidenceAssessment {
            local: if targeted_paths_known {
                EvidenceSufficiency::Targeted
            } else {
                EvidenceSufficiency::None
            },
            global,
        };
    }
    let local = if targeted_paths_known && input.intent.low_novelty_revisit_streak >= 2 {
        EvidenceSufficiency::Actionable
    } else if targeted_paths_known || !input.intent.files_read.is_empty() {
        EvidenceSufficiency::Targeted
    } else {
        EvidenceSufficiency::None
    };
    EvidenceAssessment { local, global }
}

fn pressure_from_view(input: &BehaviorPressureInput) -> BehaviorPressureAssessment {
    let first_turn_orientation_churn = input.config.enforce_first_turn_execution_bias
        && input.turn == 1
        && !input.tools.is_empty()
        && input
            .tools
            .iter()
            .all(|tool| tool.has(ToolCapability::Orientation))
        && input.intent.files_read.is_empty()
        && !input.intent.has_modified_files;

    let execution_pressure = if input.intent.task_mode == TaskMode::Research
        || input.tools.is_empty()
        || input.intent.has_modified_files
        || input.intent.files_read.is_empty()
        || !input
            .tools
            .iter()
            .all(|tool| tool.has(ToolCapability::RepoInspection))
    {
        false
    } else {
        let broad = input
            .tools
            .iter()
            .any(|tool| tool.has(ToolCapability::BroadRepoInspection));
        let (broad_threshold, targeted_threshold) = match input.config.tier {
            BehavioralTier::Constrained => (3, 4),
            BehavioralTier::Standard => (5, 6),
        };
        (input.turn >= broad_threshold && broad) || (input.turn >= targeted_threshold && !broad)
    };

    let continuation_tier = if input.tools.is_empty()
        || !matches!(
            input.dominant_phase,
            Some(OodaPhase::Observe | OodaPhase::Orient)
        ) {
        None
    } else {
        let controller = input.controller;
        let research = input.intent.task_mode == TaskMode::Research;
        let local_first = !research
            && input.config.slim_execution_bias
            && controller.local_evidence_sufficient_streak > 0
            && !input.intent.files_read.is_empty()
            && !input.intent.has_modified_files;
        let constrained = input.config.tier == BehavioralTier::Constrained;
        let (tier1, tier2, tier3) = if research {
            if constrained {
                (8, 12, 16)
            } else {
                (16, 24, 32)
            }
        } else if local_first {
            if constrained { (2, 3, 5) } else { (4, 6, 8) }
        } else if controller.evidence_sufficient_streak > 0 {
            if constrained { (3, 4, 6) } else { (6, 8, 10) }
        } else if input.config.slim_execution_bias {
            if constrained { (4, 6, 8) } else { (8, 12, 16) }
        } else if constrained {
            (3, 5, 7)
        } else {
            (12, 16, 20)
        };
        let c = controller;
        let force_tier_three = (local_first
            && (c.consecutive_tool_continuations >= tier1
                || c.orientation_churn_streak >= tier1
                || c.closure_stall_streak >= tier1))
            || (c.evidence_sufficient_streak > 0
                && (c.consecutive_tool_continuations >= tier2
                    || c.orientation_churn_streak >= tier1
                    || c.closure_stall_streak >= tier1));
        if force_tier_three {
            Some(3)
        } else if c.constraint_discovery_streak >= 2 && !research {
            Some(2)
        } else if c.consecutive_tool_continuations >= tier3
            || c.orientation_churn_streak >= tier2
            || c.closure_stall_streak >= tier2
            || c.validation_thrash_streak >= tier2
        {
            Some(3)
        } else if c.consecutive_tool_continuations >= tier2
            || c.orientation_churn_streak >= tier1
            || c.repeated_action_failure_streak >= 2
        {
            Some(2)
        } else if c.consecutive_tool_continuations >= tier1 {
            Some(1)
        } else {
            None
        }
    };

    BehaviorPressureAssessment {
        first_turn_orientation_churn,
        execution_pressure,
        continuation_tier,
    }
}

impl BehaviorPolicyService for DefaultBehaviorPolicy {
    fn infer_unpinned_task_mode(&self, prompt: &str) -> TaskMode {
        infer_task_mode_from_prompt(prompt)
    }

    fn assess_turn(&self, input: &BehaviorTurnInput) -> BehaviorTurnAssessment {
        BehaviorTurnAssessment {
            dominant_phase: phase_from_view(&input.tools),
            drift_kind: drift_from_view(input),
            progress_signal: progress_from_view(input),
            evidence: evidence_from_view(input),
        }
    }

    fn assess_pressure(&self, input: &BehaviorPressureInput) -> BehaviorPressureAssessment {
        pressure_from_view(input)
    }

    fn assess_text(&self, text: &str) -> BehaviorTextAssessment {
        BehaviorTextAssessment {
            substantive_interleaved_prose: is_substantive_interleaved_prose(text),
            pathological_meta_response: is_pathological_meta_response(text),
        }
    }

    fn message(&self, kind: BehaviorMessageKind) -> String {
        match kind {
            BehaviorMessageKind::FirstTurn(BehavioralTier::Constrained) => {
                "[System: Read the relevant file or answer the user. Do not use broad orientation tools.]".into()
            }
            BehaviorMessageKind::FirstTurn(BehavioralTier::Standard) => {
                "[System: Focus on the user's request. Read the most relevant file, then answer them in chat.]".into()
            }
            BehaviorMessageKind::ExecutionPressure(BehavioralTier::Constrained) => {
                "[System: You have enough context. Answer the user now.]".into()
            }
            BehaviorMessageKind::ExecutionPressure(BehavioralTier::Standard) => {
                "[System: You have enough context. Answer the user, or explain what's blocking you. Do not invent file-writing work the user didn't ask for.]".into()
            }
            BehaviorMessageKind::Continuation { tier, behavior } => {
                continuation_pressure_message(tier, behavior)
            }
            BehaviorMessageKind::Evidence(behavior) => evidence_sufficiency_message(behavior),
            BehaviorMessageKind::LocalFirst(behavior) => om_local_first_message(behavior),
            BehaviorMessageKind::MetaRetry => meta_recovery_retry_message(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_recovery_directive(message: &str) {
        assert!(message.contains("Do not apologize"));
        assert!(message.contains("self-criticize"));
        assert!(message.contains("mirror operator frustration"));
        assert!(message.contains("explain your process"));
    }

    #[test]
    fn no_progress_continuation_streak_is_bounded_and_reset_by_progress() {
        let mut controller = ControllerState::default();
        let evidence = EvidenceAssessment::default();

        for expected in 1..=8 {
            controller.observe_turn(
                omegon_traits::TurnEndReason::ToolContinuation,
                None,
                ProgressSignal::None,
                evidence,
                false,
            );
            assert_eq!(controller.no_progress_continuation_streak, expected);
        }

        controller.observe_turn(
            omegon_traits::TurnEndReason::ToolContinuation,
            None,
            ProgressSignal::TargetedValidation,
            evidence,
            false,
        );
        assert_eq!(controller.no_progress_continuation_streak, 0);

        controller.observe_turn(
            omegon_traits::TurnEndReason::ToolContinuation,
            None,
            ProgressSignal::None,
            evidence,
            true,
        );
        assert_eq!(controller.no_progress_continuation_streak, 0);
    }

    #[test]
    fn non_continuation_turn_clears_no_progress_streak() {
        let mut controller = ControllerState {
            no_progress_continuation_streak: 4,
            ..Default::default()
        };
        controller.observe_turn(
            omegon_traits::TurnEndReason::Blocked,
            None,
            ProgressSignal::None,
            EvidenceAssessment::default(),
            false,
        );
        assert_eq!(controller.no_progress_continuation_streak, 0);
    }

    #[test]
    fn continuation_pressure_messages_prohibit_meta_recovery() {
        for behavior in [BehavioralTier::Constrained, BehavioralTier::Standard] {
            for tier in [1, 2, 3] {
                let message = continuation_pressure_message(tier, behavior);
                assert_recovery_directive(&message);
            }
        }
    }

    #[test]
    fn task_mode_inference_classifies_research_prompts() {
        for prompt in [
            "what does the observation layer do?",
            "Explain the OODA loop wiring",
            "summarize the recent changes",
            "give me a rundown of loop.rs",
            "review the pressure heuristics",
            "How does compaction work",
            "investigate the flaky test",
            "can you check whether the tests pass",
        ] {
            assert_eq!(
                infer_task_mode_from_prompt(prompt),
                TaskMode::Research,
                "prompt should infer Research: {prompt}"
            );
        }
    }

    #[test]
    fn task_mode_inference_classifies_implementation_prompts() {
        for prompt in [
            "fix the bug in conversation.rs",
            "implement the observation normalizer",
            "add a regression test for orphaned tool results",
            "refactor the pressure tiers into policy rows",
            "commit the changes",
        ] {
            assert_eq!(
                infer_task_mode_from_prompt(prompt),
                TaskMode::Implementation,
                "prompt should infer Implementation: {prompt}"
            );
        }
    }

    #[test]
    fn repeated_observation_flow_makes_target_actionable_after_revisits() {
        let catalog = ToolCapabilityCatalog::from_tool_defs(&[omegon_traits::ToolDefinition {
            name: "read".into(),
            label: String::new(),
            description: String::new(),
            parameters: serde_json::json!({}),
            capabilities: vec![
                omegon_traits::ToolCapability::RepoInspection,
                omegon_traits::ToolCapability::TargetedRepoInspection,
            ],
        }]);
        let mut conversation = ConversationState::new();
        let call = ToolCall {
            id: "1".into(),
            name: "read".into(),
            arguments: serde_json::json!({"path": "core/crates/omegon/src/behavior.rs"}),
        };
        let result = ToolResultEntry {
            call_id: "1".into(),
            tool_name: "read".into(),
            content: vec![],
            is_error: false,
            args_summary: None,
        };
        conversation.intent.update_from_tools(
            &catalog,
            std::slice::from_ref(&call),
            std::slice::from_ref(&result),
        );
        conversation.intent.update_from_tools(
            &catalog,
            std::slice::from_ref(&call),
            std::slice::from_ref(&result),
        );
        conversation.intent.update_from_tools(
            &catalog,
            std::slice::from_ref(&call),
            std::slice::from_ref(&result),
        );
        let evidence = assess_evidence(&conversation, &catalog, &[call], &[result]);
        assert_eq!(evidence.local, EvidenceSufficiency::Actionable);
    }

    #[test]
    fn first_targeted_read_is_targeted_not_actionable() {
        let catalog = ToolCapabilityCatalog::from_tool_defs(&[omegon_traits::ToolDefinition {
            name: "read".into(),
            label: String::new(),
            description: String::new(),
            parameters: serde_json::json!({}),
            capabilities: vec![omegon_traits::ToolCapability::RepoInspection],
        }]);
        let mut conversation = ConversationState::new();
        conversation
            .intent
            .files_read
            .insert("core/crates/omegon/src/behavior.rs".into());
        let call = ToolCall {
            id: "1".into(),
            name: "read".into(),
            arguments: serde_json::json!({"path": "core/crates/omegon/src/behavior.rs"}),
        };
        let evidence = assess_evidence(&conversation, &catalog, &[call], &[]);
        assert_eq!(evidence.local, EvidenceSufficiency::Targeted);
        assert_eq!(evidence.global, EvidenceSufficiency::None);
    }

    #[test]
    fn repeated_low_novelty_revisits_make_known_target_actionable() {
        let catalog = ToolCapabilityCatalog::from_tool_defs(&[omegon_traits::ToolDefinition {
            name: "read".into(),
            label: String::new(),
            description: String::new(),
            parameters: serde_json::json!({}),
            capabilities: vec![
                omegon_traits::ToolCapability::RepoInspection,
                omegon_traits::ToolCapability::TargetedRepoInspection,
            ],
        }]);
        let mut conversation = ConversationState::new();
        conversation
            .intent
            .files_read
            .insert("core/crates/omegon/src/behavior.rs".into());
        conversation
            .intent
            .evidence_ledger
            .turns
            .push(crate::conversation::EvidenceTurn {
                observations: 1,
                novel_paths: 0,
                revisits: 1,
                searches: 0,
                search_roots: Vec::new(),
                mutation_or_validation: false,
            });
        conversation
            .intent
            .evidence_ledger
            .turns
            .push(crate::conversation::EvidenceTurn {
                observations: 1,
                novel_paths: 0,
                revisits: 1,
                searches: 0,
                search_roots: Vec::new(),
                mutation_or_validation: false,
            });
        let call = ToolCall {
            id: "1".into(),
            name: "read".into(),
            arguments: serde_json::json!({"path": "core/crates/omegon/src/behavior.rs"}),
        };
        let evidence = assess_evidence(&conversation, &catalog, &[call], &[]);
        assert_eq!(evidence.local, EvidenceSufficiency::Actionable);
    }

    #[test]
    fn explicit_task_mode_marker_is_recognized() {
        assert_eq!(
            explicit_task_mode_from_prompt("/mode research\nreview the loop"),
            Some(TaskMode::Research)
        );
        assert_eq!(
            infer_task_mode_from_prompt("[mode: implementation]\nwhat file should change?"),
            TaskMode::Implementation
        );
    }

    #[test]
    fn successful_shell_validation_resets_no_progress_streak() {
        let catalog = ToolCapabilityCatalog::from_tool_defs(&[omegon_traits::ToolDefinition {
            name: "bash".into(),
            label: String::new(),
            description: String::new(),
            parameters: serde_json::json!({}),
            capabilities: vec![omegon_traits::ToolCapability::StateChanging],
        }]);
        let call = ToolCall {
            id: "check".into(),
            name: "bash".into(),
            arguments: serde_json::json!({"command": "cargo check -p omegon"}),
        };
        let result = ToolResultEntry {
            call_id: "check".into(),
            tool_name: "bash".into(),
            content: vec![],
            is_error: false,
            args_summary: None,
        };
        assert_eq!(
            classify_progress_signal(0, 0, &catalog, &[call], &[result]),
            ProgressSignal::TargetedValidation
        );

        let mut controller = ControllerState {
            no_progress_continuation_streak: 7,
            ..ControllerState::default()
        };
        controller.observe_turn(
            omegon_traits::TurnEndReason::ToolContinuation,
            None,
            ProgressSignal::TargetedValidation,
            EvidenceAssessment::default(),
            false,
        );
        assert_eq!(controller.no_progress_continuation_streak, 0);
    }

    #[test]
    fn research_mode_suppresses_orientation_churn_drift() {
        let catalog = ToolCapabilityCatalog::from_tool_defs(&[omegon_traits::ToolDefinition {
            name: "codebase_search".into(),
            label: String::new(),
            description: String::new(),
            parameters: serde_json::json!({}),
            capabilities: vec![
                omegon_traits::ToolCapability::RepoInspection,
                omegon_traits::ToolCapability::BroadRepoInspection,
            ],
        }]);
        let call = ToolCall {
            id: "1".into(),
            name: "codebase_search".into(),
            arguments: serde_json::json!({"query": "loop"}),
        };
        let mut conversation = ConversationState::new();
        conversation
            .intent
            .files_read
            .insert("core/crates/omegon/src/loop.rs".into());
        assert_eq!(
            classify_drift_kind(&catalog, 4, &conversation, std::slice::from_ref(&call), &[]),
            Some(DriftKind::OrientationChurn)
        );
        conversation.intent.pin_task_mode(TaskMode::Research);
        assert_eq!(
            classify_drift_kind(&catalog, 4, &conversation, &[call], &[]),
            None
        );
    }

    #[test]
    fn observed_task_mode_does_not_override_pinned_mode() {
        let mut conversation = ConversationState::new();
        conversation.intent.pin_task_mode(TaskMode::Research);
        conversation
            .intent
            .observe_task_mode(TaskMode::Implementation);
        assert_eq!(conversation.intent.task_mode, TaskMode::Research);

        let mut unpinned = ConversationState::new();
        unpinned.intent.observe_task_mode(TaskMode::Research);
        assert_eq!(unpinned.intent.task_mode, TaskMode::Research);
        unpinned.intent.observe_task_mode(TaskMode::Implementation);
        assert_eq!(unpinned.intent.task_mode, TaskMode::Implementation);
    }

    #[test]
    fn evidence_and_local_first_messages_prohibit_meta_recovery() {
        for behavior in [BehavioralTier::Constrained, BehavioralTier::Standard] {
            assert_recovery_directive(&evidence_sufficiency_message(behavior));
            assert_recovery_directive(&om_local_first_message(behavior));
        }
    }

    #[test]
    fn operator_correction_recovery_message_preserves_task_and_forces_action() {
        let message = operator_correction_recovery_message();
        assert!(message.contains("operator corrected your behavior"));
        assert!(message.contains("Preserve the active task"));
        assert!(message.contains("smallest concrete next action"));
        assert!(message.contains("Do not apologize"));
    }

    #[test]
    fn pathological_meta_response_detects_self_rebuke_without_progress() {
        assert!(is_pathological_meta_response(
            "You're right. I'm wasting turns reading things I already know."
        ));
        assert!(is_pathological_meta_response(
            "The user is frustrated. Let me stop exploring and just do it."
        ));
        assert!(!is_pathological_meta_response(
            "Updated src/main.rs and ran cargo test -p omegon."
        ));
        assert!(!is_pathological_meta_response(
            "Blocked: ssh requires an operator-provided key."
        ));
    }

    #[test]
    fn bp01_service_preserves_unpinned_prompt_inference() {
        let service = DefaultBehaviorPolicy;
        for (prompt, expected) in [
            ("explain the loop", TaskMode::Research),
            ("fix the loop", TaskMode::Implementation),
        ] {
            assert_eq!(service.infer_unpinned_task_mode(prompt), expected);
            assert_eq!(
                service.infer_unpinned_task_mode(prompt),
                infer_task_mode_from_prompt(prompt)
            );
        }
    }

    #[test]
    fn bp02_through_bp08_service_matches_direct_turn_policy() {
        let tool = |name: &str, capabilities: &[ToolCapability], outcome| BehaviorToolView {
            name: name.into(),
            target: None,
            outcome,
            targeted_validation: false,
            capabilities: capabilities.iter().copied().collect(),
        };
        let input = |tools| BehaviorTurnInput {
            turn: 1,
            constraints_before: 0,
            constraints_after: 0,
            intent: BehaviorIntentView {
                task_mode: TaskMode::Implementation,
                files_read: Vec::new(),
                has_modified_files: false,
                low_novelty_revisit_streak: 0,
            },
            tools,
            observations: BehaviorObservationView::default(),
        };
        let service = DefaultBehaviorPolicy;
        assert_eq!(service.assess_turn(&input(Vec::new())).dominant_phase, None);
        assert_eq!(
            service
                .assess_turn(&input(vec![tool(
                    "orientation",
                    &[ToolCapability::Orientation],
                    BehaviorToolOutcome::Missing,
                )]))
                .dominant_phase,
            Some(OodaPhase::Observe)
        );
        assert_eq!(
            service
                .assess_turn(&input(vec![
                    tool(
                        "orientation",
                        &[ToolCapability::Orientation],
                        BehaviorToolOutcome::Missing,
                    ),
                    tool(
                        "inspection",
                        &[ToolCapability::RepoInspection],
                        BehaviorToolOutcome::Missing,
                    ),
                ]))
                .dominant_phase,
            Some(OodaPhase::Orient)
        );
        for outcome in [
            BehaviorToolOutcome::Succeeded,
            BehaviorToolOutcome::Failed,
            BehaviorToolOutcome::Missing,
        ] {
            assert_eq!(
                service
                    .assess_turn(&input(vec![tool(
                        "dynamic-mutation",
                        &[ToolCapability::Mutation],
                        outcome,
                    )]))
                    .dominant_phase,
                Some(OodaPhase::Act)
            );
        }

        let definition = omegon_traits::ToolDefinition {
            name: "codebase_search".into(),
            label: String::new(),
            description: String::new(),
            parameters: serde_json::json!({}),
            capabilities: vec![
                ToolCapability::RepoInspection,
                ToolCapability::BroadRepoInspection,
            ],
        };
        let catalog = ToolCapabilityCatalog::from_tool_defs(&[definition]);
        let call = ToolCall {
            id: "bp-turn".into(),
            name: "codebase_search".into(),
            arguments: serde_json::json!({"query": "loop"}),
        };
        let mut conversation = ConversationState::new();
        conversation.intent.files_read.insert("src/loop.rs".into());
        let calls = vec![call];
        let observations =
            crate::observation::ObservationNormalizer::new(&catalog).normalize(&calls, &[]);
        let input = BehaviorTurnInput::from_host(
            4,
            0,
            0,
            &conversation,
            &catalog,
            &calls,
            &[],
            &observations,
        );
        let assessment = service.assess_turn(&input);

        assert_eq!(
            assessment.dominant_phase,
            classify_turn_phase(&catalog, &calls, &[])
        );
        assert_eq!(
            assessment.drift_kind,
            classify_drift_kind(&catalog, 4, &conversation, &calls, &[])
        );
        assert_eq!(
            assessment.progress_signal,
            classify_progress_signal(0, 0, &catalog, &calls, &[])
        );
        assert_eq!(
            assessment.evidence,
            assess_evidence(&conversation, &catalog, &calls, &[])
        );

        let controller = ControllerState {
            consecutive_tool_continuations: 12,
            ..Default::default()
        };
        let config = crate::r#loop::LoopConfig::default();
        let pressure = service.assess_pressure(&BehaviorPressureInput::from_host(
            4,
            &config,
            &conversation,
            &catalog,
            &calls,
            &[],
            assessment.dominant_phase,
            &controller,
        ));
        assert_eq!(
            pressure.continuation_tier,
            continuation_pressure_tier(
                &config,
                &controller,
                &conversation,
                &calls,
                assessment.dominant_phase,
                behavioral_tier(&config),
            )
        );
        assert_eq!(
            pressure.execution_pressure,
            should_inject_execution_pressure(
                4,
                &config,
                &conversation,
                &catalog,
                &calls,
                behavioral_tier(&config),
            )
        );
    }

    #[test]
    fn bp03_through_bp08_literal_policy_matrix() {
        let service = DefaultBehaviorPolicy;
        let tool = |name: &str,
                    capabilities: &[ToolCapability],
                    outcome: BehaviorToolOutcome,
                    target: Option<&str>,
                    targeted_validation: bool| BehaviorToolView {
            name: name.into(),
            target: target.map(PathBuf::from),
            outcome,
            targeted_validation,
            capabilities: capabilities.iter().copied().collect(),
        };
        let intent = |task_mode, read: &[&str], modified| BehaviorIntentView {
            task_mode,
            files_read: read.iter().map(|path| PathBuf::from(*path)).collect(),
            has_modified_files: modified,
            low_novelty_revisit_streak: 0,
        };
        let turn = |turn, intent, tools, observations| BehaviorTurnInput {
            turn,
            constraints_before: 0,
            constraints_after: 0,
            intent,
            tools,
            observations,
        };

        let broad_inspection = || {
            tool(
                "search",
                &[
                    ToolCapability::RepoInspection,
                    ToolCapability::BroadRepoInspection,
                ],
                BehaviorToolOutcome::Succeeded,
                None,
                false,
            )
        };
        let orientation = service.assess_turn(&turn(
            4,
            intent(TaskMode::Implementation, &["src/lib.rs"], false),
            vec![broad_inspection()],
            BehaviorObservationView::default(),
        ));
        assert_eq!(orientation.drift_kind, Some(DriftKind::OrientationChurn));

        let failed_mutation = || {
            tool(
                "edit",
                &[ToolCapability::Mutation],
                BehaviorToolOutcome::Failed,
                Some("src/lib.rs"),
                false,
            )
        };
        assert_eq!(
            service
                .assess_turn(&turn(
                    2,
                    intent(TaskMode::Implementation, &[], false),
                    vec![failed_mutation(), failed_mutation()],
                    BehaviorObservationView::default(),
                ))
                .drift_kind,
            Some(DriftKind::RepeatedActionFailure)
        );
        let broad_validation = || {
            tool(
                "validate",
                &[ToolCapability::Validation],
                BehaviorToolOutcome::Missing,
                None,
                false,
            )
        };
        assert_eq!(
            service
                .assess_turn(&turn(
                    2,
                    intent(TaskMode::Implementation, &[], false),
                    vec![broad_validation(), broad_validation()],
                    BehaviorObservationView::default(),
                ))
                .drift_kind,
            Some(DriftKind::ValidationThrash)
        );
        assert_eq!(
            service
                .assess_turn(&turn(
                    2,
                    intent(TaskMode::Implementation, &["src/lib.rs"], true),
                    vec![broad_inspection()],
                    BehaviorObservationView::default(),
                ))
                .drift_kind,
            Some(DriftKind::ClosureStall)
        );

        for (observations, expected) in [
            (
                BehaviorObservationView {
                    progress_boundary: true,
                    ..Default::default()
                },
                ProgressSignal::Commit,
            ),
            (
                BehaviorObservationView {
                    file_mutated: true,
                    ..Default::default()
                },
                ProgressSignal::Mutation,
            ),
            (
                BehaviorObservationView {
                    validation_run: true,
                    ..Default::default()
                },
                ProgressSignal::TargetedValidation,
            ),
        ] {
            assert_eq!(
                service
                    .assess_turn(&turn(
                        1,
                        intent(TaskMode::Implementation, &[], false),
                        Vec::new(),
                        observations,
                    ))
                    .progress_signal,
                expected
            );
        }
        let mut constraint_input = turn(
            1,
            intent(TaskMode::Implementation, &[], false),
            vec![broad_inspection()],
            BehaviorObservationView::default(),
        );
        constraint_input.constraints_after = 1;
        assert_eq!(
            service.assess_turn(&constraint_input).progress_signal,
            ProgressSignal::ConstraintDiscovery
        );

        assert_eq!(
            service
                .assess_turn(&turn(
                    1,
                    intent(TaskMode::Implementation, &[], false),
                    Vec::new(),
                    BehaviorObservationView::default(),
                ))
                .evidence,
            EvidenceAssessment::default()
        );
        assert_eq!(
            service
                .assess_turn(&turn(
                    1,
                    intent(TaskMode::Implementation, &["src/lib.rs"], true),
                    Vec::new(),
                    BehaviorObservationView::default(),
                ))
                .evidence,
            EvidenceAssessment {
                local: EvidenceSufficiency::Actionable,
                global: EvidenceSufficiency::Actionable,
            }
        );

        let pressure_input = |turn, controller| BehaviorPressureInput {
            turn,
            config: BehaviorConfigView {
                enforce_first_turn_execution_bias: true,
                slim_execution_bias: false,
                tier: BehavioralTier::Standard,
            },
            intent: intent(TaskMode::Implementation, &["src/lib.rs"], false),
            tools: vec![broad_inspection()],
            dominant_phase: Some(OodaPhase::Observe),
            controller,
        };
        let first = service.assess_pressure(&BehaviorPressureInput {
            intent: intent(TaskMode::Implementation, &[], false),
            tools: vec![tool(
                "orientation",
                &[ToolCapability::Orientation],
                BehaviorToolOutcome::Missing,
                None,
                false,
            )],
            ..pressure_input(1, BehaviorControllerView::from(&ControllerState::default()))
        });
        assert!(first.first_turn_orientation_churn);
        assert!(
            service
                .assess_pressure(&pressure_input(
                    5,
                    BehaviorControllerView::from(&ControllerState::default())
                ))
                .execution_pressure
        );
        let pressured = service.assess_pressure(&pressure_input(
            2,
            BehaviorControllerView::from(&ControllerState {
                repeated_action_failure_streak: 2,
                ..Default::default()
            }),
        ));
        assert_eq!(pressured.continuation_tier, Some(2));

        assert!(
            !service
                .assess_text(&"x".repeat(SUBSTANTIVE_PROSE_MIN_CHARS - 1))
                .substantive_interleaved_prose
        );
        assert!(
            service
                .assess_text(&"x".repeat(SUBSTANTIVE_PROSE_MIN_CHARS))
                .substantive_interleaved_prose
        );
        assert!(
            service
                .assess_text("I'm wasting time and should stop exploring.")
                .pathological_meta_response
        );
    }

    #[test]
    fn bp09_service_messages_preserve_exact_first_execution_and_meta_bytes() {
        let service = DefaultBehaviorPolicy;
        assert_eq!(
            service.message(BehaviorMessageKind::FirstTurn(BehavioralTier::Constrained)),
            "[System: Read the relevant file or answer the user. Do not use broad orientation tools.]"
        );
        assert_eq!(
            service.message(BehaviorMessageKind::FirstTurn(BehavioralTier::Standard)),
            "[System: Focus on the user's request. Read the most relevant file, then answer them in chat.]"
        );
        assert_eq!(
            service.message(BehaviorMessageKind::ExecutionPressure(
                BehavioralTier::Constrained
            )),
            "[System: You have enough context. Answer the user now.]"
        );
        assert_eq!(
            service.message(BehaviorMessageKind::ExecutionPressure(
                BehavioralTier::Standard
            )),
            "[System: You have enough context. Answer the user, or explain what's blocking you. Do not invent file-writing work the user didn't ask for.]"
        );
        assert_eq!(
            service.message(BehaviorMessageKind::MetaRetry),
            "[System: Your previous response was meta-commentary rather than task progress. Retry now with no apology, self-critique, profanity mirroring, or process narration. Take the next concrete action, answer the user's request, or state the precise blocker.]"
        );
        for (tier, behavior, expected) in [
            (
                1,
                BehavioralTier::Constrained,
                "[System: You have been exploring. Produce output now — answer the user, or state what's blocking you. Do not apologize, self-criticize, mirror operator frustration, or explain your process.]",
            ),
            (
                2,
                BehavioralTier::Constrained,
                "[System: Produce output now. Answer the user, or (only if they explicitly asked you to change a file) write/edit one. Otherwise state the blocker. Do not apologize, self-criticize, mirror operator frustration, or explain your process.]",
            ),
            (
                3,
                BehavioralTier::Constrained,
                "[System: You must produce output on this turn. Answer the user, or explain why you cannot. Do not apologize, self-criticize, mirror operator frustration, or explain your process.]",
            ),
            (
                1,
                BehavioralTier::Standard,
                "[System: You have spent several turns exploring without producing output. You likely have enough context. Take the next concrete step toward completing the user's request — answer them directly. If — and only if — they explicitly asked you to modify a file, do that instead. Otherwise reply in chat. Do not apologize, self-criticize, mirror operator frustration, or explain your process.]",
            ),
            (
                2,
                BehavioralTier::Standard,
                "[System: You are still exploring. Produce a concrete result now: answer the user's question, or (only if they explicitly asked) write/edit a file. Do not invent file-writing tasks the user did not request. Do not apologize, self-criticize, mirror operator frustration, or explain your process.]",
            ),
            (
                3,
                BehavioralTier::Standard,
                "[System: You have been exploring for many turns without producing output. On this turn, you must do one of: (1) answer the user directly in chat, (2) write or edit a file ONLY if the user explicitly asked for that, or (3) tell the user exactly what is preventing you from completing the task. Do not apologize, self-criticize, mirror operator frustration, or explain your process.]",
            ),
        ] {
            assert_eq!(
                service.message(BehaviorMessageKind::Continuation { tier, behavior }),
                expected
            );
        }
        for (kind, expected) in [
            (
                BehaviorMessageKind::Evidence(BehavioralTier::Constrained),
                "[System: You have enough context. Produce output now — answer the user. Do not apologize, self-criticize, mirror operator frustration, or explain your process.]",
            ),
            (
                BehaviorMessageKind::Evidence(BehavioralTier::Standard),
                "[System: You have gathered enough context to act. Produce a concrete result — answer the user's question. If they explicitly asked you to modify a file, do that. Otherwise reply in chat; do not invent file-writing work. Do not apologize, self-criticize, mirror operator frustration, or explain your process.]",
            ),
            (
                BehaviorMessageKind::LocalFirst(BehavioralTier::Constrained),
                "[System: Produce output now. Do not search again. Answer the user. Do not apologize, self-criticize, mirror operator frustration, or explain your process.]",
            ),
            (
                BehaviorMessageKind::LocalFirst(BehavioralTier::Standard),
                "[System: You have enough context. Produce the requested output — answer the user. If they explicitly asked you to modify a file, do that; otherwise reply in chat. Do not apologize, self-criticize, mirror operator frustration, or explain your process.]",
            ),
        ] {
            assert_eq!(service.message(kind), expected);
        }
    }
}
