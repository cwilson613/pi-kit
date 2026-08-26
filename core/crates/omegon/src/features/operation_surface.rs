//! Producer adapters from concrete orchestration progress to semantic operation DTOs.

use omegon_traits::OperationRef;

use super::{
    cleave::{ChildProgress, CleaveChildFailureKind, CleaveProgress},
    delegate::{DelegateChildFailureKind, DelegateProgress, DelegateProgressChild},
};
use crate::surfaces::{
    conversation::ToolActivitySummary,
    operations::{
        OperationActivity, OperationActivityKind, OperationChildProgress, OperationChildRow,
        OperationChildStatus, OperationFailure, OperationFailureKind, OperationRouteDecision,
        OperationWorkbenchProjection,
    },
};

pub fn project_delegate(progress: &DelegateProgress) -> OperationWorkbenchProjection {
    let operation_id = match progress.children.as_slice() {
        [child] => child.task_id.clone(),
        [] => "delegate".to_string(),
        children => format!("delegate-set:{}", children[0].task_id),
    };
    OperationWorkbenchProjection {
        operation: OperationRef::delegate(operation_id),
        running: progress.running,
        completed: progress.completed,
        failed: progress.failed,
        pending_results: progress.pending_results,
        children: progress.children.iter().map(delegate_child).collect(),
    }
}

pub fn project_cleave(progress: &CleaveProgress) -> OperationWorkbenchProjection {
    OperationWorkbenchProjection {
        operation: OperationRef::cleave(
            (!progress.run_id.is_empty()).then_some(progress.run_id.clone()),
        ),
        running: progress
            .children
            .iter()
            .filter(|child| child.status == "running")
            .count(),
        completed: progress.completed,
        failed: progress.failed,
        pending_results: 0,
        children: progress.children.iter().map(cleave_child).collect(),
    }
}

fn cleave_child(child: &ChildProgress) -> OperationChildRow {
    let status = OperationChildStatus::from_cleave_status(&child.status);
    OperationChildRow {
        operation_kind: omegon_traits::OperationKind::Cleave,
        id: child.label.clone(),
        label: child.label.clone(),
        status,
        status_label: child.status.clone(),
        result_viewed: true,
        last_activity: activity(
            child.last_tool_activity.clone(),
            child.last_tool.as_deref(),
            child.last_turn,
        ),
        progress: (child.tasks_done > 0 || !child.tasks.is_empty()).then_some(
            OperationChildProgress {
                done: child.tasks_done,
                total: child.tasks.len(),
            },
        ),
        result_summary: None,
        failure: match status {
            OperationChildStatus::Failed | OperationChildStatus::TimedOut => {
                let kind =
                    child
                        .failure_kind
                        .map(cleave_failure_kind)
                        .unwrap_or_else(|| match child.status.as_str() {
                            "upstream_exhausted" => OperationFailureKind::ModelError,
                            _ => OperationFailureKind::Unknown,
                        });
                Some(OperationFailure::new(kind, None))
            }
            _ => None,
        },
        route_decision: child
            .runtime
            .as_ref()
            .and_then(|runtime| runtime.route_decision.as_ref())
            .map(route_decision),
    }
}

fn delegate_child(child: &DelegateProgressChild) -> OperationChildRow {
    let status = OperationChildStatus::from_delegate_status(&child.status);
    let failure = match status {
        OperationChildStatus::Failed | OperationChildStatus::TimedOut => {
            let kind = child
                .failure_kind
                .and_then(|kind| match kind {
                    DelegateChildFailureKind::Unknown => None,
                    known => Some(delegate_failure_kind(known)),
                })
                .or_else(|| {
                    child
                        .result_summary
                        .as_deref()
                        .map(OperationFailureKind::from_message)
                })
                .unwrap_or(OperationFailureKind::Unknown);
            Some(OperationFailure::new(kind, child.result_summary.clone()))
        }
        _ => None,
    };
    OperationChildRow {
        operation_kind: omegon_traits::OperationKind::Delegate,
        id: child.task_id.clone(),
        label: child.label.clone(),
        status,
        status_label: child.status.clone(),
        result_viewed: child.result_viewed,
        last_activity: activity(
            child.last_tool_activity.clone(),
            child.last_tool.as_deref(),
            child.last_turn,
        ),
        progress: (child.tasks_done > 0 || !child.tasks.is_empty()).then_some(
            OperationChildProgress {
                done: child.tasks_done,
                total: child.tasks.len(),
            },
        ),
        result_summary: child.result_summary.clone(),
        failure,
        route_decision: child.route_decision.as_ref().map(route_decision),
    }
}

fn activity(
    summary: Option<ToolActivitySummary>,
    fallback: Option<&str>,
    turn: Option<u32>,
) -> Option<OperationActivity> {
    summary
        .or_else(|| fallback.map(|tool| ToolActivitySummary::new(tool, None)))
        .map(|activity| OperationActivity {
            kind: OperationActivityKind::Tool,
            label: activity.raw_name,
            args_summary: activity.args_summary,
            turn,
        })
}

fn route_decision(
    decision: &crate::subagent_route::SubagentRouteDecision,
) -> OperationRouteDecision {
    OperationRouteDecision {
        selected_model: decision.selected_model.clone(),
        inventory_generation: decision.inventory_generation,
        source: format!("{:?}", decision.source),
        fallback_reason: decision.fallback_reason.clone(),
    }
}

fn cleave_failure_kind(kind: CleaveChildFailureKind) -> OperationFailureKind {
    match kind {
        CleaveChildFailureKind::ChildProcessExit => OperationFailureKind::ProcessExit,
        CleaveChildFailureKind::IdleTimeout => OperationFailureKind::IdleTimeout,
        CleaveChildFailureKind::WallTimeout => OperationFailureKind::TimedOut,
        CleaveChildFailureKind::MergeConflict => OperationFailureKind::MergeConflict,
        CleaveChildFailureKind::ScopeViolation => OperationFailureKind::SandboxViolation,
        CleaveChildFailureKind::UpstreamExhausted => OperationFailureKind::ModelError,
        CleaveChildFailureKind::ValidationFailed => OperationFailureKind::ToolExecutionFailed,
        CleaveChildFailureKind::Unknown => OperationFailureKind::Unknown,
    }
}

fn delegate_failure_kind(kind: DelegateChildFailureKind) -> OperationFailureKind {
    match kind {
        DelegateChildFailureKind::MissingLocalModel
        | DelegateChildFailureKind::MissingCredential
        | DelegateChildFailureKind::ProviderStartup => OperationFailureKind::ModelError,
        DelegateChildFailureKind::WorkspaceStartup => OperationFailureKind::ProcessExit,
        DelegateChildFailureKind::Unknown => OperationFailureKind::Unknown,
    }
}
