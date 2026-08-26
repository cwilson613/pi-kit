//! Compatibility executor and scheduling policy for loop tool batches.

use crate::conversation::{ToolCall, ToolResultEntry};
use crate::loop_permission::PermissionRecord;
use futures_util::stream::{self, StreamExt};
use omegon_traits::{AgentEvent, ContentBlock};
use serde_json::Value;
use std::collections::HashMap;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

const MAX_PARALLEL_INVOCATIONS: usize = 4;

pub(crate) struct DispatchResult {
    pub(crate) results: Vec<ToolResultEntry>,
    pub(crate) permission_decisions: Vec<PermissionRecord>,
}

fn invocation_denial_result(
    tool_name: &str,
    denial: crate::invocation_service::InvocationDenial,
) -> (omegon_traits::ToolResult, bool) {
    let text = format!("BLOCKED: `{tool_name}`: {}", denial.message);
    (
        omegon_traits::ToolResult {
            content: vec![ContentBlock::Text { text }],
            details: serde_json::json!({
                "is_error": true,
                "blocked": true,
                "reason": denial.code.as_str(),
                "layer": denial.policy_layer.map(|layer| layer.as_str()).unwrap_or("none"),
            }),
        },
        true,
    )
}

#[allow(clippy::too_many_arguments)]
async fn execute_tool_invocation(
    invocations: &dyn crate::loop_driver::LoopInvocationContract,
    visible_call_id: &str,
    visible_tool_name: &str,
    visible_args: &Value,
    execution_tool_name: &str,
    execution_args: Value,
    events: &broadcast::Sender<AgentEvent>,
    cancel: CancellationToken,
    secrets: Option<&omegon_secrets::SecretsManager>,
    permission_log: &mut Vec<PermissionRecord>,
    emit_agent_events: bool,
    permission_policy: Option<&crate::permissions::LayeredPermissionPolicy>,
    permission_role: Option<styrene_rbac::Role>,
    invocation_scope: &crate::invocation_service::InvocationScope,
) -> (omegon_traits::ToolResult, bool) {
    let provenance = invocations
        .runtime_ref()
        .tool_provenance(execution_tool_name);
    if let Some(manager) = secrets
        && let Some(decision) = manager.check_guard(visible_tool_name, visible_args)
        && decision.is_block()
    {
        let message = match decision {
            omegon_secrets::GuardDecision::Block { reason, path } => {
                format!("Blocked: {reason} ({path})")
            }
            _ => unreachable!(),
        };
        tracing::warn!(tool = visible_tool_name, %message, "tool guard blocked");
        let result = omegon_traits::ToolResult {
            content: vec![ContentBlock::Text {
                text: message.clone(),
            }],
            details: Value::Null,
        };
        if emit_agent_events {
            publish_tool_end(
                events,
                visible_call_id,
                visible_tool_name,
                result.clone(),
                true,
                provenance,
            );
        }
        return (result, true);
    }

    let admission = invocations.admit_tool(
        execution_tool_name,
        crate::invocation_service::InvocationAdmissionRequest {
            call_id: visible_call_id,
            visible_tool_name,
            args: visible_args,
            scope: invocation_scope.clone(),
            permission_policy,
            permission_role,
        },
    );
    let lease = match admission {
        crate::invocation_service::InvocationAdmission::Lease(lease) => lease,
        crate::invocation_service::InvocationAdmission::Denied(denial) => {
            return invocation_denial_result(visible_tool_name, denial);
        }
        crate::invocation_service::InvocationAdmission::ApprovalRequired(pending) => {
            match invocations
                .acquire_tool_approval(crate::loop_driver::LoopToolApprovalRequest {
                    pending,
                    visible_call_id,
                    visible_tool_name,
                    events,
                    cancel: cancel.clone(),
                    permission_log,
                })
                .await
            {
                Ok(lease) => lease,
                Err(denial) => return invocation_denial_result(visible_tool_name, denial),
            }
        }
    };
    if let Err(denial) =
        invocations.persist_tool_dispatch(&lease, visible_call_id, execution_tool_name)
    {
        return invocation_denial_result(visible_tool_name, denial);
    }

    if emit_agent_events {
        let _ = events.send(AgentEvent::ToolStart {
            id: visible_call_id.into(),
            name: visible_tool_name.into(),
            args: visible_args.clone(),
            provenance: provenance.clone(),
        });
    }
    let sink_events = events.clone();
    let sink_call_id = visible_call_id.to_string();
    let sink = omegon_traits::ToolProgressSink::from_fn(move |partial| {
        let _ = sink_events.send(AgentEvent::ToolUpdate {
            id: sink_call_id.clone(),
            partial,
        });
    });
    let context = invocations.tool_execution_context();
    let handoff = invocations
        .handoff_tool_owner(crate::loop_driver::LoopToolOwnerRequest {
            lease: &lease,
            execution_tool_name,
            visible_call_id,
            execution_args: execution_args.clone(),
            cancel: cancel.clone(),
            sink: if emit_agent_events {
                sink.clone()
            } else {
                omegon_traits::ToolProgressSink::noop()
            },
            context: context.clone(),
        })
        .await;

    let first_result = match handoff {
        crate::loop_driver::LoopToolOwnerHandoff::Delegated(result) => {
            if let Err(error) = &result {
                match invocations.classify_tool_owner_completion(&lease, error) {
                    Ok(true) => {
                        let result = unknown_completion_result(error, secrets);
                        if emit_agent_events {
                            publish_tool_end(
                                events,
                                visible_call_id,
                                visible_tool_name,
                                result.clone(),
                                true,
                                provenance,
                            );
                        }
                        return (result, true);
                    }
                    Ok(false) => {}
                    Err(denial) => return invocation_denial_result(visible_tool_name, denial),
                }
            }
            let (mut result, is_error) = owner_result(result);
            finalize_result_content(&mut result.content, secrets);
            if let Err(denial) =
                invocations.settle_tool_owner(&lease, &result, is_error, cancel.is_cancelled())
            {
                return invocation_denial_result(visible_tool_name, denial);
            }
            if emit_agent_events {
                publish_tool_end(
                    events,
                    visible_call_id,
                    visible_tool_name,
                    result.clone(),
                    is_error,
                    provenance,
                );
            }
            return (result, is_error);
        }
        crate::loop_driver::LoopToolOwnerHandoff::Local(result) => result,
    };

    let presented = invocations
        .present_tool_owner_result(crate::loop_driver::LoopToolPresentationRequest {
            result: first_result,
            lease: &lease,
            visible_call_id,
            visible_tool_name,
            execution_tool_name,
            execution_args,
            events,
            cancel: cancel.clone(),
            sink,
            context,
            permission_log,
            invocation_scope,
        })
        .await;
    let (result, is_error) = match presented {
        crate::loop_driver::LoopToolPresentation::Resolved(result, is_error) => (result, is_error),
        crate::loop_driver::LoopToolPresentation::Unhandled(error) => {
            match invocations.classify_tool_owner_completion(&lease, &error) {
                Ok(true) => {
                    let result = unknown_completion_result(&error, secrets);
                    if emit_agent_events {
                        publish_tool_end(
                            events,
                            visible_call_id,
                            visible_tool_name,
                            result.clone(),
                            true,
                            provenance,
                        );
                    }
                    return (result, true);
                }
                Ok(false) => owner_result(Err(error)),
                Err(denial) => return invocation_denial_result(visible_tool_name, denial),
            }
        }
    };

    let mut final_content = result.content;
    finalize_result_content(&mut final_content, secrets);
    let result = omegon_traits::ToolResult {
        content: final_content,
        details: result.details,
    };
    if let Err(denial) =
        invocations.settle_tool_owner(&lease, &result, is_error, cancel.is_cancelled())
    {
        return invocation_denial_result(visible_tool_name, denial);
    }
    if emit_agent_events {
        publish_tool_end(
            events,
            visible_call_id,
            visible_tool_name,
            result.clone(),
            is_error,
            provenance,
        );
    }
    (result, is_error)
}

fn owner_result(
    result: anyhow::Result<omegon_traits::ToolResult>,
) -> (omegon_traits::ToolResult, bool) {
    match result {
        Ok(result) => {
            let is_error = result
                .details
                .get("is_error")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            (result, is_error)
        }
        Err(error) => (
            omegon_traits::ToolResult {
                content: vec![ContentBlock::Text {
                    text: error.to_string(),
                }],
                details: Value::Null,
            },
            true,
        ),
    }
}

fn unknown_completion_result(
    error: &anyhow::Error,
    secrets: Option<&omegon_secrets::SecretsManager>,
) -> omegon_traits::ToolResult {
    let mut content = vec![ContentBlock::Text {
        text: error.to_string(),
    }];
    finalize_result_content(&mut content, secrets);
    omegon_traits::ToolResult {
        content,
        details: serde_json::json!({
            "is_error": true,
            "status": "unknown_completion",
        }),
    }
}

fn finalize_result_content(
    content: &mut [ContentBlock],
    secrets: Option<&omegon_secrets::SecretsManager>,
) {
    if let Some(manager) = secrets {
        manager.redact_content(content);
    }
    const MAX_TOOL_OUTPUT_CHARS: usize = 16_000;
    crate::util::truncate_content_blocks(content, MAX_TOOL_OUTPUT_CHARS);
}

fn publish_tool_end(
    events: &broadcast::Sender<AgentEvent>,
    call_id: &str,
    tool_name: &str,
    result: omegon_traits::ToolResult,
    is_error: bool,
    provenance: omegon_traits::ToolProvenance,
) {
    let _ = events.send(AgentEvent::ToolEnd {
        id: call_id.into(),
        name: tool_name.into(),
        result,
        is_error,
        provenance,
    });
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn dispatch_tools(
    invocations: &dyn crate::loop_driver::LoopInvocationContract,
    tool_calls: &[ToolCall],
    events: &broadcast::Sender<AgentEvent>,
    cancel: CancellationToken,
    cwd: &std::path::Path,
    secrets: Option<&omegon_secrets::SecretsManager>,
    permission_policy: Option<&crate::permissions::LayeredPermissionPolicy>,
    permission_role: Option<styrene_rbac::Role>,
    invocation_scope: &crate::invocation_service::InvocationScope,
) -> DispatchResult {
    let bus = invocations.runtime_ref();
    let mut permission_decisions = Vec::new();

    let mutation_count = tool_calls
        .iter()
        .filter(|call| declaration_allows_rollback(invocations, &call.name))
        .count();
    let batch_mode = mutation_count >= 2;
    let snapshot = if batch_mode {
        snapshot_mutation_files(invocations, tool_calls, cwd).await
    } else {
        MutationSnapshot::default()
    };

    if !snapshot.originals.is_empty() {
        tracing::info!(
            files = snapshot.originals.len(),
            edits = mutation_count,
            "Auto-batch: snapshotted {} file(s) for {} mutations",
            snapshot.originals.len(),
            mutation_count
        );
    }

    let mut serial_calls = Vec::new();
    let mut parallel_calls = Vec::new();
    let allow_parallel = !batch_mode && secrets.is_none() && permission_policy.is_none();
    for (idx, call) in tool_calls.iter().cloned().enumerate() {
        if allow_parallel && declaration_allows_parallel(invocations, &call.name) {
            parallel_calls.push((idx, call));
        } else {
            serial_calls.push((idx, call));
        }
    }

    let mut indexed_results = Vec::with_capacity(tool_calls.len());
    if !parallel_calls.is_empty() {
        let outcomes = stream::iter(parallel_calls.into_iter().map(|(idx, call)| {
            let events = events.clone();
            let cancel = cancel.clone();
            async move {
                let mut ignored_permission_log = Vec::new();
                let result = dispatch_single_tool(
                    invocations,
                    &call,
                    &events,
                    cancel,
                    None,
                    &mut ignored_permission_log,
                    permission_policy,
                    permission_role,
                    invocation_scope,
                )
                .await;
                (idx, result)
            }
        }))
        .buffer_unordered(MAX_PARALLEL_INVOCATIONS)
        .collect::<Vec<_>>()
        .await;
        indexed_results.extend(outcomes);
    }

    let mut batch_failed = false;
    let mut mutated_files = Vec::new();
    let mut serial_idx = 0;
    while serial_idx < serial_calls.len() {
        if let Some(adapted) = dispatch_edit_batch(
            invocations,
            &serial_calls,
            serial_idx,
            events,
            cancel.clone(),
            cwd,
            secrets,
            &mut permission_decisions,
            permission_policy,
            permission_role,
            invocation_scope,
        )
        .await
        {
            batch_failed |= adapted.failed;
            mutated_files.extend(adapted.mutated_files);
            indexed_results.extend(adapted.results);
            serial_idx = adapted.next_idx;
            continue;
        }

        let (idx, call) = serial_calls[serial_idx].clone();
        if batch_failed && declaration_allows_rollback(invocations, &call.name) {
            indexed_results.push((idx, skipped_after_rollback(bus, events, &call)));
            serial_idx += 1;
            continue;
        }

        let dispatched = dispatch_single_tool(
            invocations,
            &call,
            events,
            cancel.clone(),
            secrets,
            &mut permission_decisions,
            permission_policy,
            permission_role,
            invocation_scope,
        )
        .await;

        if !dispatched.is_error
            && declaration_allows_rollback(invocations, &call.name)
            && let Some(path) = mutation_path(&call.arguments)
        {
            mutated_files.push(cwd.join(path));
        }

        if dispatched.is_error
            && batch_mode
            && declaration_allows_rollback(invocations, &call.name)
            && !mutated_files.is_empty()
        {
            batch_failed = true;
            let rollback_report = snapshot.rollback(&mutated_files).await;
            let result = rollback_failure_result(bus, events, &call, dispatched, rollback_report);
            indexed_results.push((idx, result));
            serial_idx += 1;
            continue;
        }

        indexed_results.push((idx, dispatched));
        serial_idx += 1;
    }

    indexed_results.sort_by_key(|(idx, _)| *idx);
    DispatchResult {
        results: indexed_results
            .into_iter()
            .map(|(_, result)| result)
            .collect(),
        permission_decisions,
    }
}

pub(crate) fn declaration_allows_parallel(
    invocations: &dyn crate::loop_driver::LoopInvocationContract,
    name: &str,
) -> bool {
    invocations
        .tool_declaration(name)
        .is_some_and(|declaration| declaration.parallel_safe)
}

fn declaration_allows_rollback(
    invocations: &dyn crate::loop_driver::LoopInvocationContract,
    name: &str,
) -> bool {
    invocations
        .tool_declaration(name)
        .is_some_and(|declaration| declaration.best_effort_rollback)
}

#[derive(Default)]
struct MutationSnapshot {
    originals: HashMap<std::path::PathBuf, String>,
    created: Vec<std::path::PathBuf>,
}

impl MutationSnapshot {
    async fn rollback(&self, mutated_files: &[std::path::PathBuf]) -> Vec<String> {
        tracing::warn!(
            mutated = mutated_files.len(),
            "Auto-batch: mutation failed — rolling back {} file(s)",
            mutated_files.len()
        );
        let mut report = Vec::new();
        for file in mutated_files {
            if let Some(original) = self.originals.get(file) {
                match tokio::fs::write(file, original).await {
                    Ok(_) => report.push(format!("  ✓ restored {}", file.display())),
                    Err(error) => {
                        report.push(format!("  ✗ rollback failed {}: {error}", file.display()))
                    }
                }
            } else if self.created.contains(file) {
                match tokio::fs::remove_file(file).await {
                    Ok(_) => report.push(format!("  ✓ removed {}", file.display())),
                    Err(error) => {
                        report.push(format!("  ✗ remove failed {}: {error}", file.display()))
                    }
                }
            }
        }
        report
    }
}

async fn snapshot_mutation_files(
    invocations: &dyn crate::loop_driver::LoopInvocationContract,
    calls: &[ToolCall],
    cwd: &std::path::Path,
) -> MutationSnapshot {
    let mut snapshot = MutationSnapshot::default();
    for call in calls {
        if declaration_allows_rollback(invocations, &call.name)
            && let Some(path) = mutation_path(&call.arguments)
        {
            let full = cwd.join(path);
            if full.exists() {
                if !snapshot.originals.contains_key(&full)
                    && let Ok(content) = tokio::fs::read_to_string(&full).await
                {
                    snapshot.originals.insert(full, content);
                }
            } else {
                snapshot.created.push(full);
            }
        }
    }
    snapshot
}

fn skipped_after_rollback(
    bus: &crate::bus::EventBus,
    events: &broadcast::Sender<AgentEvent>,
    call: &ToolCall,
) -> ToolResultEntry {
    let text = format!(
        "Skipped {} — previous edit in this turn failed and triggered rollback.",
        call.name
    );
    emit_tool_end(bus, events, call, text.clone(), Value::Null, true);
    result_entry(call, vec![ContentBlock::Text { text }], true)
}

fn rollback_failure_result(
    bus: &crate::bus::EventBus,
    events: &broadcast::Sender<AgentEvent>,
    call: &ToolCall,
    dispatched: ToolResultEntry,
    rollback_report: Vec<String>,
) -> ToolResultEntry {
    let mut text = dispatched
        .content
        .iter()
        .filter_map(ContentBlock::as_text)
        .collect::<Vec<_>>()
        .join("\n");
    text.push_str("\n\n[Auto-rollback: previous edits in this turn were reverted]\n");
    text.push_str(&rollback_report.join("\n"));
    emit_tool_end(bus, events, call, text.clone(), Value::Null, true);
    result_entry(call, vec![ContentBlock::Text { text }], true)
}

#[allow(clippy::too_many_arguments)]
async fn dispatch_single_tool(
    invocations: &dyn crate::loop_driver::LoopInvocationContract,
    call: &ToolCall,
    events: &broadcast::Sender<AgentEvent>,
    cancel: CancellationToken,
    secrets: Option<&omegon_secrets::SecretsManager>,
    permission_log: &mut Vec<PermissionRecord>,
    permission_policy: Option<&crate::permissions::LayeredPermissionPolicy>,
    permission_role: Option<styrene_rbac::Role>,
    invocation_scope: &crate::invocation_service::InvocationScope,
) -> ToolResultEntry {
    let (mut result, is_error) = execute_tool_invocation(
        invocations,
        &call.id,
        &call.name,
        &call.arguments,
        &call.name,
        call.arguments.clone(),
        events,
        cancel,
        secrets,
        permission_log,
        true,
        permission_policy,
        permission_role,
        invocation_scope,
    )
    .await;
    normalize_result_content(&call.name, &mut result, is_error);
    result_entry(call, result.content, is_error)
}

struct AdaptedBatch {
    next_idx: usize,
    results: Vec<(usize, ToolResultEntry)>,
    mutated_files: Vec<std::path::PathBuf>,
    failed: bool,
}

#[allow(clippy::too_many_arguments)]
async fn dispatch_edit_batch(
    invocations: &dyn crate::loop_driver::LoopInvocationContract,
    serial_calls: &[(usize, ToolCall)],
    start_idx: usize,
    events: &broadcast::Sender<AgentEvent>,
    cancel: CancellationToken,
    cwd: &std::path::Path,
    secrets: Option<&omegon_secrets::SecretsManager>,
    permission_log: &mut Vec<PermissionRecord>,
    permission_policy: Option<&crate::permissions::LayeredPermissionPolicy>,
    permission_role: Option<styrene_rbac::Role>,
    invocation_scope: &crate::invocation_service::InvocationScope,
) -> Option<AdaptedBatch> {
    let bus = invocations.runtime_ref();
    if secrets.is_some()
        || invocation_scope.authority.is_some()
        || !declaration_allows_rollback(invocations, &serial_calls.get(start_idx)?.1.name)
        || serial_calls.get(start_idx)?.1.name != "edit"
        || !bus.has_registered_tool("change")
        || !declaration_allows_rollback(invocations, "change")
    {
        return None;
    }

    let mut end_idx = start_idx;
    while let Some((_, call)) = serial_calls.get(end_idx) {
        if call.name != "edit" {
            break;
        }
        end_idx += 1;
    }
    if end_idx - start_idx < 2 {
        return None;
    }

    let calls = &serial_calls[start_idx..end_idx];
    let edits = calls
        .iter()
        .map(|(_, call)| {
            serde_json::json!({
                "file": call.arguments.get("path").and_then(Value::as_str).unwrap_or_default(),
                "oldText": call.arguments.get("oldText").and_then(Value::as_str).unwrap_or_default(),
                "newText": call.arguments.get("newText").and_then(Value::as_str).unwrap_or_default(),
            })
        })
        .collect::<Vec<_>>();
    for (_, call) in calls {
        let _ = events.send(AgentEvent::ToolStart {
            id: call.id.clone(),
            name: call.name.clone(),
            args: call.arguments.clone(),
            provenance: bus.tool_provenance(&call.name),
        });
    }

    let first = &calls[0].1;
    let (batch_result, failed) = execute_tool_invocation(
        invocations,
        &first.id,
        &first.name,
        &first.arguments,
        "change",
        serde_json::json!({"edits": edits, "validate": "none"}),
        events,
        cancel,
        None,
        permission_log,
        false,
        permission_policy,
        permission_role,
        invocation_scope,
    )
    .await;
    let batch_text = batch_result
        .content
        .iter()
        .filter_map(ContentBlock::as_text)
        .collect::<Vec<_>>()
        .join("\n");

    let mut results = Vec::with_capacity(calls.len());
    let mut mutated_files = Vec::new();
    for (position, (idx, call)) in calls.iter().enumerate() {
        let path = call
            .arguments
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let text = if failed {
            if position == 0 {
                batch_text.clone()
            } else {
                format!("Skipped edit in {path} — atomic edit batch failed.\n\n{batch_text}")
            }
        } else if position + 1 == calls.len() {
            format!("Applied exact-text edit to {path} as part of an atomic batch.\n\n{batch_text}")
        } else {
            format!("Applied exact-text edit to {path} as part of an atomic batch.")
        };
        if !failed && let Some(path) = mutation_path(&call.arguments) {
            mutated_files.push(cwd.join(path));
        }
        let details = if position + 1 == calls.len() {
            batch_result.details.clone()
        } else {
            Value::Null
        };
        emit_tool_end(bus, events, call, text.clone(), details, failed);
        results.push((
            *idx,
            result_entry(call, vec![ContentBlock::Text { text }], failed),
        ));
    }

    Some(AdaptedBatch {
        next_idx: end_idx,
        results,
        mutated_files,
        failed,
    })
}

fn emit_tool_end(
    bus: &crate::bus::EventBus,
    events: &broadcast::Sender<AgentEvent>,
    call: &ToolCall,
    text: String,
    details: Value,
    is_error: bool,
) {
    let _ = events.send(AgentEvent::ToolEnd {
        id: call.id.clone(),
        name: call.name.clone(),
        result: omegon_traits::ToolResult {
            content: vec![ContentBlock::Text { text }],
            details,
        },
        is_error,
        provenance: bus.tool_provenance(&call.name),
    });
}

fn result_entry(call: &ToolCall, content: Vec<ContentBlock>, is_error: bool) -> ToolResultEntry {
    ToolResultEntry {
        call_id: call.id.clone(),
        tool_name: call.name.clone(),
        content,
        is_error,
        args_summary: summarize_tool_args(&call.name, &call.arguments),
    }
}

/// Summarize tool call arguments into compact decay context.
pub(crate) fn summarize_tool_args(tool_name: &str, args: &Value) -> Option<String> {
    match tool_name {
        "read" | "edit" | "write" | "view" => {
            args.get("path").and_then(Value::as_str).map(|path| {
                let cwd = std::env::current_dir()
                    .map(|directory| directory.to_string_lossy().to_string())
                    .unwrap_or_default();
                if !cwd.is_empty() && path.starts_with(&cwd) {
                    path[cwd.len()..]
                        .strip_prefix('/')
                        .unwrap_or(&path[cwd.len()..])
                        .to_string()
                } else {
                    path.to_string()
                }
            })
        }
        "bash" => {
            let command = args.get("command").and_then(Value::as_str)?;
            let clean = command.strip_prefix("cd ").map_or(command, |rest| {
                rest.split_once(" && ").map_or(rest, |(_, command)| command)
            });
            if clean.len() > 60 {
                let mut end = 60;
                while end > 0 && !clean.is_char_boundary(end) {
                    end -= 1;
                }
                Some(format!("{}…", &clean[..end]))
            } else {
                Some(clean.to_string())
            }
        }
        "terminal" => {
            let action = args
                .get("action")
                .and_then(Value::as_str)
                .unwrap_or("status");
            match action {
                "start" => args
                    .get("command")
                    .and_then(Value::as_str)
                    .map(|command| format!("start: {}", crate::util::truncate(command, 60))),
                "send" | "read" | "stop" => args
                    .get("session_id")
                    .or_else(|| args.get("name"))
                    .and_then(Value::as_str)
                    .map(|id| format!("{action}: {id}")),
                "list" => Some("list".into()),
                other => Some(other.into()),
            }
        }
        "change" => Some(
            args.get("edits")
                .and_then(Value::as_array)?
                .iter()
                .filter_map(|edit| edit.get("file").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join(", "),
        ),
        "web_search" => args.get("query").and_then(Value::as_str).map(|query| {
            if query.len() > 60 {
                crate::util::truncate(query, 60)
            } else {
                query.to_string()
            }
        }),
        "memory_recall" | "memory_store" | "memory_query" => args
            .get("query")
            .or_else(|| args.get("content"))
            .and_then(Value::as_str)
            .map(|text| {
                if text.len() > 60 {
                    crate::util::truncate(text, 60)
                } else {
                    text.to_string()
                }
            }),
        "cleave_run" => {
            let plan = args
                .get("plan_json")
                .and_then(Value::as_str)
                .and_then(|json| serde_json::from_str::<Value>(json).ok());
            let labels = plan
                .as_ref()
                .and_then(|plan| plan.get("children"))
                .and_then(Value::as_array)
                .map(|children| {
                    children
                        .iter()
                        .filter_map(|child| child.get("label").and_then(Value::as_str))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if labels.is_empty() {
                Some("cleave".into())
            } else {
                Some(crate::util::truncate(
                    &format!("{} children: {}", labels.len(), labels.join(", ")),
                    60,
                ))
            }
        }
        "cleave_assess" => args
            .get("directive")
            .and_then(Value::as_str)
            .map(|directive| crate::util::truncate(directive, 60)),
        _ => None,
    }
}

fn mutation_path(args: &Value) -> Option<String> {
    args.get("path").and_then(Value::as_str).map(str::to_owned)
}

fn normalize_result_content(
    tool_name: &str,
    result: &mut omegon_traits::ToolResult,
    is_error: bool,
) {
    let substantive = result.content.iter().any(|block| match block {
        ContentBlock::Text { text } => !text.trim().is_empty(),
        ContentBlock::Image { .. } => true,
    });
    if substantive {
        return;
    }
    let details_are_empty = result.details.is_null()
        || matches!(&result.details, Value::Object(map) if map.is_empty())
        || matches!(&result.details, Value::Array(items) if items.is_empty());
    let text = if !details_are_empty {
        serde_json::to_string_pretty(&result.details).unwrap_or_else(|_| result.details.to_string())
    } else if is_error {
        format!("Tool `{tool_name}` failed without returning diagnostic content.")
    } else {
        format!("Tool `{tool_name}` completed successfully with no output.")
    };
    result.content = vec![ContentBlock::Text { text }];
}

#[cfg(test)]
mod tests {
    use super::*;
    use omegon_traits::{ToolCapability, ToolDefinition, ToolProvider, ToolResult};

    #[test]
    fn normalization_preserves_content_and_fills_empty_results() {
        let mut empty = omegon_traits::ToolResult {
            content: Vec::new(),
            details: Value::Null,
        };
        normalize_result_content("whoami", &mut empty, false);
        assert_eq!(
            empty.content[0].as_text(),
            Some("Tool `whoami` completed successfully with no output.")
        );

        let mut substantive = omegon_traits::ToolResult {
            content: vec![ContentBlock::Text { text: "ok".into() }],
            details: Value::Null,
        };
        normalize_result_content("whoami", &mut substantive, false);
        assert_eq!(substantive.content[0].as_text(), Some("ok"));
    }

    #[test]
    fn edit_path_adaptation_uses_only_path_arguments() {
        assert_eq!(
            mutation_path(&serde_json::json!({"path": "src/main.rs"})).as_deref(),
            Some("src/main.rs")
        );
        assert!(mutation_path(&serde_json::json!({"command": "ls"})).is_none());
    }

    #[tokio::test]
    async fn contiguous_edits_adapt_to_change_and_preserve_visible_result_order() {
        struct RecordingProvider {
            calls: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        }

        #[async_trait::async_trait]
        impl ToolProvider for RecordingProvider {
            fn tools(&self) -> Vec<ToolDefinition> {
                ["edit", "change"]
                    .into_iter()
                    .map(|name| ToolDefinition {
                        name: name.into(),
                        label: name.into(),
                        description: "mutation".into(),
                        parameters: serde_json::json!({}),
                        capabilities: vec![ToolCapability::Mutation, ToolCapability::StateChanging],
                    })
                    .collect()
            }

            async fn execute(
                &self,
                tool_name: &str,
                _call_id: &str,
                _args: Value,
                _cancel: CancellationToken,
            ) -> anyhow::Result<ToolResult> {
                self.calls.lock().unwrap().push(tool_name.into());
                Ok(ToolResult {
                    content: vec![ContentBlock::Text {
                        text: "batch applied".into(),
                    }],
                    details: serde_json::json!({"adapted": true}),
                })
            }
        }

        let observed = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut bus = crate::bus::EventBus::new();
        bus.register(Box::new(crate::features::adapter::ToolAdapter::new(
            "recording-mutations",
            Box::new(RecordingProvider {
                calls: observed.clone(),
            }),
        )));
        bus.finalize();
        let invocations = crate::loop_driver::LoopInvocationPort::new(&mut bus);
        let calls = vec![
            ToolCall {
                id: "edit-1".into(),
                name: "edit".into(),
                arguments: serde_json::json!({
                    "path": "a.rs",
                    "oldText": "a",
                    "newText": "b"
                }),
            },
            ToolCall {
                id: "edit-2".into(),
                name: "edit".into(),
                arguments: serde_json::json!({
                    "path": "b.rs",
                    "oldText": "c",
                    "newText": "d"
                }),
            },
        ];
        let (events, _) = broadcast::channel(16);
        let cwd = tempfile::tempdir().unwrap();

        let dispatch = dispatch_tools(
            &invocations,
            &calls,
            &events,
            CancellationToken::new(),
            cwd.path(),
            None,
            None,
            None,
            &crate::invocation_service::InvocationScope::default(),
        )
        .await;

        assert_eq!(&*observed.lock().unwrap(), &["change"]);
        assert_eq!(
            dispatch
                .results
                .iter()
                .map(|result| (result.call_id.as_str(), result.tool_name.as_str()))
                .collect::<Vec<_>>(),
            vec![("edit-1", "edit"), ("edit-2", "edit")]
        );
        assert!(dispatch.results.iter().all(|result| !result.is_error));
        assert!(
            dispatch.results[1].content[0]
                .as_text()
                .unwrap()
                .contains("batch applied")
        );
    }

    #[tokio::test]
    async fn parallel_completion_preserves_provider_call_result_order() {
        struct DelayedReads;

        #[async_trait::async_trait]
        impl ToolProvider for DelayedReads {
            fn tools(&self) -> Vec<ToolDefinition> {
                vec![ToolDefinition {
                    name: "parallel_read".into(),
                    label: "parallel read".into(),
                    description: "parallel-safe test tool".into(),
                    parameters: serde_json::json!({"type": "object"}),
                    capabilities: vec![ToolCapability::TargetedRepoInspection],
                }]
            }

            fn runtime_tool_policy(
                &self,
                _tool_name: &str,
            ) -> Option<omegon_traits::RuntimeToolPolicy> {
                Some(omegon_traits::RuntimeToolPolicy {
                    effects: vec![omegon_traits::RuntimeEffect::FilesystemRead],
                    execution: omegon_traits::RuntimeExecutionPolicy {
                        principals: vec![omegon_traits::RuntimePrincipalClass::Model],
                        timeout_class: omegon_traits::RuntimeTimeoutClass::Immediate,
                        retry_class: omegon_traits::RuntimeRetryClass::Never,
                        idempotency: omegon_traits::RuntimeIdempotency::Idempotent,
                        deduplication: omegon_traits::RuntimeDeduplication::Unsupported,
                        parallelism: omegon_traits::RuntimeParallelism::ParallelSafe,
                        transaction: omegon_traits::RuntimeTransactionBehavior::None,
                        mutation_fence: None,
                        max_attempts: None,
                    },
                })
            }

            async fn execute(
                &self,
                _tool_name: &str,
                call_id: &str,
                _args: Value,
                _cancel: CancellationToken,
            ) -> anyhow::Result<ToolResult> {
                if call_id == "slow-first" {
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
                Ok(ToolResult {
                    content: vec![ContentBlock::Text {
                        text: call_id.into(),
                    }],
                    details: Value::Null,
                })
            }
        }

        let mut bus = crate::bus::EventBus::new();
        bus.register(Box::new(crate::features::adapter::ToolAdapter::new(
            "parallel-reads",
            Box::new(DelayedReads),
        )));
        bus.finalize();
        let invocations = crate::loop_driver::LoopInvocationPort::new(&mut bus);
        assert!(declaration_allows_parallel(&invocations, "parallel_read"));
        let calls = [
            ToolCall {
                id: "slow-first".into(),
                name: "parallel_read".into(),
                arguments: serde_json::json!({}),
            },
            ToolCall {
                id: "fast-second".into(),
                name: "parallel_read".into(),
                arguments: serde_json::json!({}),
            },
        ];
        let (events, _) = broadcast::channel(8);
        let cwd = tempfile::tempdir().unwrap();
        let dispatch = dispatch_tools(
            &invocations,
            &calls,
            &events,
            CancellationToken::new(),
            cwd.path(),
            None,
            None,
            None,
            &crate::invocation_service::InvocationScope::default(),
        )
        .await;

        assert_eq!(
            dispatch
                .results
                .iter()
                .map(|result| result.call_id.as_str())
                .collect::<Vec<_>>(),
            ["slow-first", "fast-second"]
        );
    }
}
