//! Durable managed OpenSpec artifact and lifecycle-ledger transactions.

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use omegon_opsx::{
    ChangeState, LifecycleState, OpenSpecRepository, TaskCheckboxStatus, plan_proposal_state,
    plan_spec_write, plan_task_checkbox_status, task_counts_content,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::lifecycle_service::{
    LifecycleMutationOutcomeV1, LifecycleMutationReceiptV1, LifecycleRepositoryRevisionV1,
    OpenSpecMutationV1, RepositoryRoots,
};
use crate::lifecycle_transaction::{CommitContext, TransactionError, TransactionErrorCode};

const JOURNAL_VERSION: u32 = 1;
const RECEIPT_VERSION: u32 = 1;
const DOMAIN: &str = "openspec-v1";
const MAX_RESOURCES: usize = 128;
const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_JOURNAL_BYTES: u64 = (MAX_BYTES as u64 * 4) + 1024 * 1024;
const MAX_RECEIPT_BYTES: u64 = 1024 * 1024;
const MAX_TREE_ENTRIES: usize = 10_000;
const MAX_TREE_DEPTH: usize = 64;
const MAX_EFFECT_BYTES: usize = 256 * 1024;

type TxResult<T> = Result<T, TransactionError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Journal {
    version: u32,
    domain: String,
    repository_id: String,
    operation_id: String,
    semantic_fingerprint: String,
    resources: Vec<Resource>,
    effects: Vec<String>,
    checksum: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Resource {
    File {
        path: String,
        initial_identity: Option<String>,
        write_identity: Option<String>,
        post_identity: String,
        post_bytes: Vec<u8>,
    },
    DirectoryMove {
        source: String,
        destination: String,
        source_identity: String,
        moved_identity: String,
        final_identity: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct DurableOpenSpecReceipt {
    version: u32,
    domain: String,
    repository_id: String,
    operation_id: String,
    semantic_fingerprint: String,
    result: LifecycleMutationReceiptV1,
    checksum: String,
}

pub(super) fn semantic_fingerprint(mutation: &OpenSpecMutationV1) -> TxResult<String> {
    let mut bytes = DOMAIN.as_bytes().to_vec();
    bytes.push(0);
    bytes.extend(serde_json::to_vec(mutation).map_err(TransactionError::validation)?);
    Ok(identity(&bytes))
}

pub(super) fn receipt_fingerprint(receipt: &DurableOpenSpecReceipt) -> &str {
    &receipt.semantic_fingerprint
}

pub(super) fn receipt_result(receipt: DurableOpenSpecReceipt) -> LifecycleMutationReceiptV1 {
    receipt.result
}

pub(super) fn read_receipt(
    repo: &Path,
    operation_id: &str,
) -> TxResult<Option<DurableOpenSpecReceipt>> {
    validate_operation_id(operation_id)?;
    let path = receipt_path(repo, operation_id);
    validate_no_follow_path(repo, &path)?;
    let bytes = match read_bounded(&path, MAX_RECEIPT_BYTES) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {
            return Err(TransactionError::new(
                TransactionErrorCode::RecoveryRequired,
                error.to_string(),
            ));
        }
        Err(error) => return Err(TransactionError::persistence(error)),
    };
    let envelope: serde_json::Value = serde_json::from_slice(&bytes).map_err(|error| {
        TransactionError::new(
            TransactionErrorCode::RecoveryRequired,
            format!("malformed OpenSpec receipt: {error}"),
        )
    })?;
    if envelope.get("domain").and_then(serde_json::Value::as_str) != Some(DOMAIN) {
        return Err(TransactionError::new(
            TransactionErrorCode::OperationConflict,
            "lifecycle operation id is already committed in another transaction domain",
        ));
    }
    let receipt: DurableOpenSpecReceipt = serde_json::from_slice(&bytes).map_err(|error| {
        TransactionError::new(
            TransactionErrorCode::RecoveryRequired,
            format!("malformed OpenSpec receipt: {error}"),
        )
    })?;
    if receipt.version != RECEIPT_VERSION
        || receipt.repository_id != repository_id(repo)?
        || receipt.operation_id != operation_id
        || receipt.checksum != receipt_checksum(&receipt)?
    {
        return Err(TransactionError::new(
            TransactionErrorCode::RecoveryRequired,
            format!("invalid OpenSpec receipt: {}", path.display()),
        ));
    }
    Ok(Some(receipt))
}

pub(super) fn stage_and_commit(
    context: CommitContext<'_>,
    mutation: &OpenSpecMutationV1,
    is_cancelled: &impl Fn() -> bool,
    mut observe_revision: impl FnMut() -> anyhow::Result<LifecycleRepositoryRevisionV1>,
) -> TxResult<LifecycleMutationReceiptV1> {
    let CommitContext {
        repo,
        roots,
        operation_id,
        semantic_fingerprint: fingerprint,
        pre_revision,
        ledger: ledger_transaction,
    } = context;
    validate_operation_id(operation_id)?;
    check_cancelled(is_cancelled)?;
    if ledger_transaction.path() != roots.ledger {
        return Err(TransactionError::new(
            TransactionErrorCode::Validation,
            "selected ledger transaction does not match the frozen ledger",
        ));
    }
    let pre_state = ledger_transaction
        .load()
        .map_err(TransactionError::persistence)?;
    if pre_state.revision != pre_revision.ledger_revision {
        return Err(TransactionError::new(
            TransactionErrorCode::StaleRevision,
            "stale lifecycle repository revision",
        ));
    }

    let (mut resources, post_state, effects, outcome) =
        stage_mutation(repo, roots, mutation, &pre_state, is_cancelled)?;
    resources.push(file_resource(
        repo,
        ledger_transaction.path(),
        serde_json::to_vec_pretty(&post_state).map_err(TransactionError::validation)?,
        None,
    )?);
    let observed = observe_revision().map_err(TransactionError::persistence)?;
    if observed != *pre_revision {
        return Err(TransactionError::new(
            TransactionErrorCode::StaleRevision,
            "stale lifecycle repository revision after mutation staging",
        ));
    }
    check_cancelled(is_cancelled)?;

    let mut journal = Journal {
        version: JOURNAL_VERSION,
        domain: DOMAIN.into(),
        repository_id: repository_id(repo)?,
        operation_id: operation_id.into(),
        semantic_fingerprint: fingerprint.into(),
        resources,
        effects: effects.clone(),
        checksum: String::new(),
    };
    journal.checksum = journal_checksum(&journal)?;
    validate_journal(repo, roots, &journal)?;
    let pending = pending_path(repo, operation_id);
    let journal_bytes =
        serde_json::to_vec_pretty(&journal).map_err(TransactionError::validation)?;
    if journal_bytes.len() as u64 > MAX_JOURNAL_BYTES {
        return Err(TransactionError::validation(
            "OpenSpec transaction journal exceeds the serialized byte limit",
        ));
    }
    atomic_durable_write(&pending, &journal_bytes)?;
    settle_journal(repo, &journal)?;
    let revision = observe_revision().map_err(TransactionError::persistence)?;
    let result = LifecycleMutationReceiptV1 {
        operation_id: operation_id.into(),
        replayed: false,
        committed_revision: revision,
        effects,
        outcome,
    };
    write_receipt(repo, operation_id, fingerprint, &result)?;
    remove_durable(&pending)?;
    Ok(result)
}

fn stage_mutation(
    repo: &Path,
    roots: &RepositoryRoots,
    mutation: &OpenSpecMutationV1,
    pre_state: &LifecycleState,
    is_cancelled: &impl Fn() -> bool,
) -> TxResult<(
    Vec<Resource>,
    LifecycleState,
    Vec<String>,
    LifecycleMutationOutcomeV1,
)> {
    check_cancelled(is_cancelled)?;
    let repository = OpenSpecRepository::from_openspec_root(&roots.openspec);
    let mut resources = Vec::new();
    let mut effects = Vec::new();
    let mut canonical_state = pre_state.clone();

    match mutation {
        OpenSpecMutationV1::Propose {
            name,
            title,
            intent,
            bound_node,
        } => {
            validate_name(name)?;
            let change_dir = repository.active_change_dir(name);
            ensure_absent(&change_dir)?;
            if repository.archived_change_dir(name).exists()
                || canonical_state
                    .changes
                    .iter()
                    .any(|change| change.name == *name)
            {
                return Err(TransactionError::validation(format!(
                    "OpenSpec change '{name}' already exists"
                )));
            }
            let proposal = format!(
                "---\nstate: proposed\n---\n\n# {title}\n\n## Intent\n\n{intent}\n\n## Scope\n\n_TBD_\n\n## Constraints\n\n_None identified yet._\n"
            );
            resources.push(file_resource(
                repo,
                &change_dir.join("proposal.md"),
                proposal.into_bytes(),
                None,
            )?);
            let post = stage_ledger(&canonical_state, |lifecycle| {
                lifecycle.create_change(name, title, bound_node.as_deref())?;
                Ok(())
            })?;
            effects.push(format!("proposed OpenSpec change {name}"));
            return Ok((
                resources,
                post,
                effects,
                LifecycleMutationOutcomeV1::OpenSpecProposed {
                    path: change_dir.display().to_string(),
                },
            ));
        }
        OpenSpecMutationV1::Reopen { change } => {
            validate_name(change)?;
            let source = repository.archived_change_dir(change);
            let destination = repository.active_change_dir(change);
            ensure_directory(&source)?;
            ensure_absent(&destination)?;
            reconcile_archived(&repository, change, &mut canonical_state)?;
            let proposal_source = source.join("proposal.md");
            let proposal_bytes = read_regular_file(&proposal_source)?;
            let proposal_text =
                std::str::from_utf8(&proposal_bytes).map_err(TransactionError::validation)?;
            let plan = plan_proposal_state(
                &destination.join("proposal.md"),
                proposal_text,
                ChangeState::Proposed,
            )
            .map_err(TransactionError::validation)?;
            let moved_identity = tree_identity(&source)?;
            let final_identity =
                tree_identity_with_override(&source, Path::new("proposal.md"), &plan.bytes)?;
            resources.push(directory_move_resource(
                repo,
                &source,
                &destination,
                &moved_identity,
                &final_identity,
            )?);
            resources.push(file_resource_with_initial(
                repo,
                &plan.path,
                plan.bytes,
                None,
                Some(identity(&proposal_bytes)),
            )?);
            let post = stage_ledger(&canonical_state, |lifecycle| {
                lifecycle.transition_change(change, ChangeState::Proposed)?;
                Ok(())
            })?;
            effects.push(format!("reopened OpenSpec change {change}"));
            return Ok((resources, post, effects, LifecycleMutationOutcomeV1::None));
        }
        _ => {}
    }

    let change = mutation.change_name();
    validate_name(change)?;
    let change_dir = repository.active_change_dir(change);
    ensure_directory(&change_dir)?;
    reconcile_active(&repository, change, &mut canonical_state)?;

    let mut outcome = LifecycleMutationOutcomeV1::None;
    let post = match mutation {
        OpenSpecMutationV1::AddSpec {
            domain, content, ..
        } => {
            let spec = plan_spec_write(&change_dir, domain, content)
                .map_err(TransactionError::validation)?;
            resources.push(file_resource(repo, &spec.path, spec.bytes, None)?);
            outcome = LifecycleMutationOutcomeV1::OpenSpecSpecAdded {
                path: spec.path.display().to_string(),
            };
            let proposal_path = change_dir.join("proposal.md");
            let proposal = read_regular_file(&proposal_path)?;
            let proposal_plan = plan_proposal_state(
                &proposal_path,
                std::str::from_utf8(&proposal).map_err(TransactionError::validation)?,
                ChangeState::Specced,
            )
            .map_err(TransactionError::validation)?;
            resources.push(file_resource(
                repo,
                &proposal_plan.path,
                proposal_plan.bytes,
                None,
            )?);
            effects.push(format!("added OpenSpec domain {domain} to {change}"));
            stage_ledger(&canonical_state, |lifecycle| {
                lifecycle.add_spec(change, domain)?;
                if lifecycle
                    .state()
                    .changes
                    .iter()
                    .find(|item| item.name == change)
                    .is_some_and(|item| item.state == ChangeState::Proposed)
                {
                    lifecycle.transition_change(change, ChangeState::Specced)?;
                }
                Ok(())
            })?
        }
        OpenSpecMutationV1::ReconcileTasks { .. } => {
            let tasks = read_regular_file(&change_dir.join("tasks.md"))?;
            let (total, done) = task_counts_content(
                std::str::from_utf8(&tasks).map_err(TransactionError::validation)?,
            );
            let current_state = canonical_state
                .changes
                .iter()
                .find(|item| item.name == change)
                .map(|item| item.state)
                .ok_or_else(|| TransactionError::validation("reconciled change is absent"))?;
            let should_plan = total > 0 && current_state == ChangeState::Specced;
            let should_verify =
                total > 0 && done >= total && current_state == ChangeState::Implementing;
            if should_plan {
                resources.push(proposal_state_resource(
                    repo,
                    &change_dir,
                    ChangeState::Planned,
                )?);
            } else if should_verify {
                resources.push(proposal_state_resource(
                    repo,
                    &change_dir,
                    ChangeState::Verifying,
                )?);
            }
            effects.push(format!("reconciled {done}/{total} tasks for {change}"));
            outcome = LifecycleMutationOutcomeV1::OpenSpecTasksReconciled {
                total_tasks: total,
                done_tasks: done,
            };
            stage_ledger(&canonical_state, |lifecycle| {
                lifecycle.update_change_progress(change, total, done)?;
                if should_plan {
                    lifecycle.transition_change(change, ChangeState::Planned)?;
                } else if should_verify {
                    lifecycle.transition_change(change, ChangeState::Verifying)?;
                }
                Ok(())
            })?
        }
        OpenSpecMutationV1::SetTaskStatus {
            group,
            task_id,
            done,
            ..
        } => {
            let tasks_path = change_dir.join("tasks.md");
            let tasks = read_regular_file(&tasks_path)?;
            let plan = plan_task_checkbox_status(
                &tasks_path,
                std::str::from_utf8(&tasks).map_err(TransactionError::validation)?,
                change,
                group,
                task_id,
                if *done {
                    TaskCheckboxStatus::Done
                } else {
                    TaskCheckboxStatus::Pending
                },
            )
            .map_err(TransactionError::validation)?;
            outcome = LifecycleMutationOutcomeV1::OpenSpecTaskStatusChanged {
                change: plan.report.change.clone(),
                group: plan.report.group.clone(),
                task_id: plan.report.task_id.clone(),
                path: plan.report.path.display().to_string(),
                line: plan.report.line,
                previous_done: plan.report.previous_done,
                new_done: plan.report.new_done,
                description: plan.report.description.clone(),
            };
            resources.push(file_resource(
                repo,
                &plan.write.path,
                plan.write.bytes,
                None,
            )?);
            effects.push(format!("set OpenSpec task {group}/{task_id} done={done}"));
            stage_ledger(&canonical_state, |lifecycle| {
                lifecycle.update_change_progress(change, plan.total_tasks, plan.done_tasks)?;
                Ok(())
            })?
        }
        OpenSpecMutationV1::RegisterTestFile { path, .. } => {
            let test = contained_path(repo, path)?;
            let metadata = fs::symlink_metadata(&test).map_err(TransactionError::validation)?;
            if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
                return Err(TransactionError::validation(format!(
                    "registered test file is not a regular file: {path}"
                )));
            }
            let current_state = canonical_state
                .changes
                .iter()
                .find(|item| item.name == change)
                .map(|item| item.state)
                .ok_or_else(|| TransactionError::validation("reconciled change is absent"))?;
            if !matches!(
                current_state,
                ChangeState::Planned | ChangeState::Testing | ChangeState::Implementing
            ) {
                return Err(TransactionError::validation(format!(
                    "OpenSpec change '{change}' must be planned before registering test files"
                )));
            }
            if current_state != ChangeState::Implementing {
                resources.push(proposal_state_resource(
                    repo,
                    &change_dir,
                    ChangeState::Implementing,
                )?);
            }
            effects.push(format!("registered test file {path} for {change}"));
            stage_ledger(&canonical_state, |lifecycle| {
                if current_state == ChangeState::Planned {
                    lifecycle.transition_change(change, ChangeState::Testing)?;
                }
                lifecycle.add_test_file(change, path)?;
                if matches!(current_state, ChangeState::Planned | ChangeState::Testing) {
                    lifecycle.transition_change(change, ChangeState::Implementing)?;
                }
                Ok(())
            })?
        }
        OpenSpecMutationV1::Transition { state, .. } => {
            if matches!(state, ChangeState::Archived | ChangeState::Abandoned) {
                return Err(TransactionError::validation(
                    "archive and abandon require their dedicated managed operations",
                ));
            }
            resources.push(proposal_state_resource(repo, &change_dir, *state)?);
            effects.push(format!(
                "transitioned OpenSpec change {change} to {}",
                state.as_str()
            ));
            stage_ledger(&canonical_state, |lifecycle| {
                lifecycle.transition_change(change, *state)?;
                Ok(())
            })?
        }
        OpenSpecMutationV1::Archive { .. } => {
            let destination = repository.archived_change_dir(change);
            ensure_absent(&destination)?;
            let tree = tree_identity(&change_dir)?;
            resources.push(directory_move_resource(
                repo,
                &change_dir,
                &destination,
                &tree,
                &tree,
            )?);
            effects.push(format!("archived OpenSpec change {change}"));
            stage_ledger(&canonical_state, |lifecycle| {
                lifecycle.transition_change(change, ChangeState::Archived)?;
                Ok(())
            })?
        }
        OpenSpecMutationV1::Abandon { .. } => {
            resources.push(proposal_state_resource(
                repo,
                &change_dir,
                ChangeState::Abandoned,
            )?);
            effects.push(format!("abandoned OpenSpec change {change}"));
            stage_ledger(&canonical_state, |lifecycle| {
                lifecycle.transition_change(change, ChangeState::Abandoned)?;
                Ok(())
            })?
        }
        OpenSpecMutationV1::Propose { .. } | OpenSpecMutationV1::Reopen { .. } => unreachable!(),
    };
    check_cancelled(is_cancelled)?;
    Ok((resources, post, effects, outcome))
}

fn reconcile_active(
    repository: &OpenSpecRepository,
    name: &str,
    state: &mut LifecycleState,
) -> TxResult<()> {
    let record = repository.read_active(name).ok_or_else(|| {
        TransactionError::validation(format!("OpenSpec change '{name}' not found"))
    })?;
    if !record.health.is_healthy() {
        return Err(TransactionError::validation(format!(
            "OpenSpec change '{name}' has malformed or incomplete canonical artifacts"
        )));
    }
    ensure_change_record(state, name)?;
    let change = state
        .changes
        .iter_mut()
        .find(|change| change.name == name)
        .expect("reconciled change exists");
    change.state = record.state;
    change.specs = spec_domains(&record.path)?;
    change.tasks_total = record.evidence.total_tasks;
    change.tasks_done = record.evidence.done_tasks;
    Ok(())
}

fn reconcile_archived(
    repository: &OpenSpecRepository,
    name: &str,
    state: &mut LifecycleState,
) -> TxResult<()> {
    let path = repository.archived_change_dir(name);
    let record = repository.inspect_change_dir(&path, name, true);
    if !record.health.is_healthy() {
        return Err(TransactionError::validation(format!(
            "archived OpenSpec change '{name}' has malformed or incomplete canonical artifacts"
        )));
    }
    ensure_change_record(state, name)?;
    let change = state
        .changes
        .iter_mut()
        .find(|change| change.name == name)
        .expect("reconciled change exists");
    change.state = ChangeState::Archived;
    change.specs = spec_domains(&record.path)?;
    change.tasks_total = record.evidence.total_tasks;
    change.tasks_done = record.evidence.done_tasks;
    Ok(())
}

fn ensure_change_record(state: &mut LifecycleState, name: &str) -> TxResult<()> {
    if state.changes.iter().any(|change| change.name == name) {
        return Ok(());
    }
    let revision = state.revision;
    let mut reconciled = stage_ledger(state, |lifecycle| {
        lifecycle.create_change(name, name, None)?;
        Ok(())
    })?;
    // Reconciliation is part of the enclosing transaction, which publishes
    // exactly one ledger revision regardless of the staged FSM calls.
    reconciled.revision = revision;
    *state = reconciled;
    Ok(())
}

fn stage_ledger(
    state: &LifecycleState,
    apply: impl FnOnce(
        &mut omegon_opsx::Lifecycle<crate::lifecycle_transaction::SeededStore>,
    ) -> anyhow::Result<()>,
) -> TxResult<LifecycleState> {
    crate::lifecycle_transaction::stage_ledger_state(state, apply)
        .map_err(TransactionError::validation)
}

fn proposal_state_resource(
    repo: &Path,
    change_dir: &Path,
    state: ChangeState,
) -> TxResult<Resource> {
    let path = change_dir.join("proposal.md");
    let bytes = read_regular_file(&path)?;
    let plan = plan_proposal_state(
        &path,
        std::str::from_utf8(&bytes).map_err(TransactionError::validation)?,
        state,
    )
    .map_err(TransactionError::validation)?;
    file_resource(repo, &plan.path, plan.bytes, None)
}

fn file_resource(
    repo: &Path,
    path: &Path,
    post_bytes: Vec<u8>,
    write_identity: Option<String>,
) -> TxResult<Resource> {
    let pre = file_identity(path)?;
    file_resource_with_initial(repo, path, post_bytes, pre.clone(), write_identity.or(pre))
}

fn file_resource_with_initial(
    repo: &Path,
    path: &Path,
    post_bytes: Vec<u8>,
    initial_identity: Option<String>,
    write_identity: Option<String>,
) -> TxResult<Resource> {
    Ok(Resource::File {
        path: relative_path(repo, path)?,
        initial_identity,
        write_identity,
        post_identity: identity(&post_bytes),
        post_bytes,
    })
}

fn directory_move_resource(
    repo: &Path,
    source: &Path,
    destination: &Path,
    moved_identity: &str,
    final_identity: &str,
) -> TxResult<Resource> {
    Ok(Resource::DirectoryMove {
        source: relative_path(repo, source)?,
        destination: relative_path(repo, destination)?,
        source_identity: moved_identity.into(),
        moved_identity: moved_identity.into(),
        final_identity: final_identity.into(),
    })
}

fn validate_journal(repo: &Path, roots: &RepositoryRoots, journal: &Journal) -> TxResult<()> {
    if journal.version != JOURNAL_VERSION
        || journal.domain != DOMAIN
        || journal.repository_id != repository_id(repo)?
        || journal.checksum != journal_checksum(journal)?
    {
        return Err(TransactionError::new(
            TransactionErrorCode::RecoveryRequired,
            "invalid OpenSpec transaction journal identity, version, or checksum",
        ));
    }
    validate_operation_id(&journal.operation_id)?;
    let effect_bytes = journal
        .effects
        .iter()
        .try_fold(0usize, |total, effect| total.checked_add(effect.len()))
        .ok_or_else(|| TransactionError::validation("transaction effect byte count overflow"))?;
    if journal.resources.is_empty()
        || journal.resources.len() > MAX_RESOURCES
        || journal.effects.len() > 256
        || effect_bytes > MAX_EFFECT_BYTES
        || journal.semantic_fingerprint.is_empty()
    {
        return Err(TransactionError::validation(
            "OpenSpec transaction journal has an invalid resource count",
        ));
    }
    let ledger = relative_path(repo, &roots.ledger)?;
    let mut ledger_count = 0;
    let mut paths = BTreeSet::new();
    let mut bytes = 0usize;
    for resource in &journal.resources {
        match resource {
            Resource::File {
                path,
                post_identity,
                post_bytes,
                ..
            } => {
                if path.len() > 1024 {
                    return Err(TransactionError::validation(
                        "transaction resource path exceeds the length limit",
                    ));
                }
                if !paths.insert(path.clone()) {
                    return Err(TransactionError::validation(format!(
                        "duplicate transaction resource: {path}"
                    )));
                }
                let absolute = contained_path(repo, path)?;
                if absolute != roots.ledger
                    && validate_openspec_change_path(&roots.openspec, &absolute).is_err()
                {
                    return Err(TransactionError::validation(format!(
                        "transaction resource is outside a selected OpenSpec change: {path}"
                    )));
                }
                if identity(post_bytes) != *post_identity {
                    return Err(TransactionError::validation(format!(
                        "transaction staged content identity mismatch: {path}"
                    )));
                }
                bytes = bytes.checked_add(post_bytes.len()).ok_or_else(|| {
                    TransactionError::validation("transaction staged byte count overflow")
                })?;
                if *path == ledger {
                    ledger_count += 1;
                }
            }
            Resource::DirectoryMove {
                source,
                destination,
                ..
            } => {
                if source == destination || source.len() > 1024 || destination.len() > 1024 {
                    return Err(TransactionError::validation(
                        "directory transaction resource has invalid paths",
                    ));
                }
                let mut locations = Vec::with_capacity(2);
                for path in [source, destination] {
                    if !paths.insert(path.clone()) {
                        return Err(TransactionError::validation(format!(
                            "duplicate transaction resource: {path}"
                        )));
                    }
                    locations.push(validate_openspec_change_path(
                        &roots.openspec,
                        &contained_path(repo, path)?,
                    )?);
                }
                if locations[0].1 != locations[1].1
                    || !matches!(
                        (locations[0].0.as_str(), locations[1].0.as_str()),
                        ("changes", "archive") | ("archive", "changes")
                    )
                {
                    return Err(TransactionError::validation(
                        "directory transaction must move one named change between changes and archive",
                    ));
                }
            }
        }
    }
    if bytes > MAX_BYTES
        || ledger_count != 1
        || !resource_is_ledger(journal.resources.last(), &ledger)
    {
        return Err(TransactionError::validation(
            "transaction must be bounded and contain exactly one ledger resource settled last",
        ));
    }
    Ok(())
}

fn validate_openspec_change_path(root: &Path, path: &Path) -> TxResult<(String, String)> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| TransactionError::validation("path is outside the frozen OpenSpec root"))?;
    let mut components = relative.components();
    let area = match components.next() {
        Some(Component::Normal(value)) => value
            .to_str()
            .ok_or_else(|| TransactionError::validation("OpenSpec path is not UTF-8"))?,
        _ => return Err(TransactionError::validation("OpenSpec path has no area")),
    };
    if !matches!(area, "changes" | "archive") {
        return Err(TransactionError::validation(
            "OpenSpec path is outside changes and archive",
        ));
    }
    let name = match components.next() {
        Some(Component::Normal(value)) => value
            .to_str()
            .ok_or_else(|| TransactionError::validation("OpenSpec change name is not UTF-8"))?,
        _ => {
            return Err(TransactionError::validation(
                "OpenSpec path has no change name",
            ));
        }
    };
    validate_name(name)?;
    Ok((area.into(), name.into()))
}

fn resource_is_ledger(resource: Option<&Resource>, ledger: &str) -> bool {
    matches!(resource, Some(Resource::File { path, .. }) if path == ledger)
}

fn settle_journal(repo: &Path, journal: &Journal) -> TxResult<()> {
    // Validate every resource before performing the first write.
    for resource in &journal.resources {
        validate_resource_frontier(repo, resource)?;
    }
    for resource in &journal.resources {
        match resource {
            Resource::File {
                path,
                write_identity,
                post_identity,
                post_bytes,
                ..
            } => {
                let path = contained_path(repo, path)?;
                let observed = file_identity(&path)?;
                if observed.as_ref() == Some(post_identity) {
                    continue;
                }
                if &observed != write_identity {
                    return Err(TransactionError::new(
                        TransactionErrorCode::RecoveryRequired,
                        format!(
                            "transaction file is not at its write frontier: {}",
                            path.display()
                        ),
                    ));
                }
                atomic_durable_write(&path, post_bytes)?;
            }
            Resource::DirectoryMove {
                source,
                destination,
                source_identity,
                ..
            } => {
                let source = contained_path(repo, source)?;
                let destination = contained_path(repo, destination)?;
                if tree_identity_optional(&source)?.as_ref() == Some(source_identity) {
                    let parent = destination.parent().ok_or_else(|| {
                        TransactionError::validation("archive destination has no parent")
                    })?;
                    fs::create_dir_all(parent).map_err(TransactionError::persistence)?;
                    fs::rename(&source, &destination).map_err(TransactionError::persistence)?;
                    sync_directory(source.parent().unwrap_or(repo))?;
                    sync_directory(parent)?;
                }
            }
        }
    }
    for resource in &journal.resources {
        verify_resource_final(repo, resource)?;
    }
    Ok(())
}

fn validate_resource_frontier(repo: &Path, resource: &Resource) -> TxResult<()> {
    match resource {
        Resource::File {
            path,
            initial_identity,
            write_identity,
            post_identity,
            ..
        } => {
            let observed = file_identity(&contained_path(repo, path)?)?;
            if observed != *initial_identity
                && observed != *write_identity
                && observed.as_ref() != Some(post_identity)
            {
                return Err(TransactionError::new(
                    TransactionErrorCode::RecoveryRequired,
                    format!("transaction file has neither pre nor post identity: {path}"),
                ));
            }
        }
        Resource::DirectoryMove {
            source,
            destination,
            source_identity,
            moved_identity,
            final_identity,
        } => {
            let source_state = tree_identity_optional(&contained_path(repo, source)?)?;
            let destination_state = tree_identity_optional(&contained_path(repo, destination)?)?;
            let before =
                source_state.as_ref() == Some(source_identity) && destination_state.is_none();
            let after = source_state.is_none()
                && matches!(destination_state.as_ref(), Some(value) if value == moved_identity || value == final_identity);
            if !before && !after {
                return Err(TransactionError::new(
                    TransactionErrorCode::RecoveryRequired,
                    format!(
                        "directory move has neither pre nor post identity: {source} -> {destination}"
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn verify_resource_final(repo: &Path, resource: &Resource) -> TxResult<()> {
    match resource {
        Resource::File {
            path,
            post_identity,
            ..
        } if file_identity(&contained_path(repo, path)?)?.as_ref() != Some(post_identity) => {
            Err(TransactionError::new(
                TransactionErrorCode::RecoveryRequired,
                format!("transaction file did not reach post identity: {path}"),
            ))
        }
        Resource::DirectoryMove {
            source,
            destination,
            final_identity,
            ..
        } => {
            let source = contained_path(repo, source)?;
            let destination = contained_path(repo, destination)?;
            if tree_identity_optional(&source)?.is_some()
                || tree_identity_optional(&destination)?.as_ref() != Some(final_identity)
            {
                return Err(TransactionError::new(
                    TransactionErrorCode::RecoveryRequired,
                    "directory move did not reach its final identity",
                ));
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

pub(super) fn recover_pending(
    repo: &Path,
    roots: &RepositoryRoots,
    mut committed_revision: impl FnMut() -> anyhow::Result<LifecycleRepositoryRevisionV1>,
) -> Vec<TransactionError> {
    let pending = transaction_root(repo).join("pending");
    let entries = match fs::read_dir(&pending) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => {
            return vec![TransactionError::persistence(format!(
                "scan transaction journals: {error}"
            ))];
        }
    };
    let mut paths = Vec::new();
    let mut errors = Vec::new();
    for entry in entries {
        match entry {
            Ok(entry)
                if entry.path().extension().and_then(|value| value.to_str()) == Some("json") =>
            {
                match entry.file_type() {
                    Ok(file_type) if file_type.is_file() && !file_type.is_symlink() => {
                        paths.push(entry.path())
                    }
                    Ok(_) => match quarantine_journal(repo, &entry.path()) {
                        Ok(()) => errors.push(TransactionError::new(
                            TransactionErrorCode::RecoveryRequired,
                            "quarantined non-file repository transaction journal",
                        )),
                        Err(error) => errors.push(error),
                    },
                    Err(error) => errors.push(TransactionError::persistence(format!(
                        "inspect transaction journal: {error}"
                    ))),
                }
            }
            Ok(_) => {}
            Err(error) => errors.push(TransactionError::persistence(format!(
                "scan transaction journal: {error}"
            ))),
        }
    }
    paths.sort();
    for path in paths {
        let bytes = match read_bounded(&path, MAX_JOURNAL_BYTES) {
            Ok(bytes) => bytes,
            Err(error) => {
                errors.push(TransactionError::persistence(error));
                continue;
            }
        };
        let envelope: serde_json::Value = match serde_json::from_slice(&bytes) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if envelope.get("domain").and_then(serde_json::Value::as_str) != Some(DOMAIN) {
            continue;
        }
        let recovered = (|| -> TxResult<()> {
            let journal: Journal =
                serde_json::from_slice(&bytes).map_err(TransactionError::validation)?;
            validate_journal(repo, roots, &journal)?;
            let receipt = read_receipt(repo, &journal.operation_id)?;
            if let Some(receipt) = &receipt
                && receipt.semantic_fingerprint != journal.semantic_fingerprint
            {
                return Err(TransactionError::new(
                    TransactionErrorCode::OperationConflict,
                    "journal conflicts with committed operation receipt",
                ));
            }
            if receipt.is_some() {
                for resource in &journal.resources {
                    verify_resource_final(repo, resource)?;
                }
                return remove_durable(&path);
            }
            settle_journal(repo, &journal)?;
            let result = LifecycleMutationReceiptV1 {
                operation_id: journal.operation_id.clone(),
                replayed: false,
                committed_revision: committed_revision().map_err(TransactionError::persistence)?,
                effects: journal.effects.clone(),
                outcome: LifecycleMutationOutcomeV1::None,
            };
            write_receipt(
                repo,
                &journal.operation_id,
                &journal.semantic_fingerprint,
                &result,
            )?;
            remove_durable(&path)
        })();
        if let Err(error) = recovered {
            match quarantine_journal(repo, &path) {
                Ok(()) => errors.push(TransactionError::new(
                    TransactionErrorCode::RecoveryRequired,
                    format!("quarantined OpenSpec transaction: {error}"),
                )),
                Err(quarantine) => errors.push(TransactionError::new(
                    TransactionErrorCode::RecoveryRequired,
                    format!("OpenSpec recovery failed ({error}); quarantine failed ({quarantine})"),
                )),
            }
        }
    }
    errors
}

fn quarantine_journal(repo: &Path, path: &Path) -> TxResult<()> {
    let name = path
        .file_name()
        .ok_or_else(|| TransactionError::validation("journal has no file name"))?;
    let destination = transaction_root(repo).join("quarantine").join(name);
    validate_no_follow_path(repo, &destination)?;
    fs::create_dir_all(destination.parent().unwrap_or(repo))
        .map_err(TransactionError::persistence)?;
    fs::rename(path, &destination).map_err(TransactionError::persistence)?;
    sync_directory(destination.parent().unwrap_or(repo))
}

fn write_receipt(
    repo: &Path,
    operation_id: &str,
    fingerprint: &str,
    result: &LifecycleMutationReceiptV1,
) -> TxResult<()> {
    let mut receipt = DurableOpenSpecReceipt {
        version: RECEIPT_VERSION,
        domain: DOMAIN.into(),
        repository_id: repository_id(repo)?,
        operation_id: operation_id.into(),
        semantic_fingerprint: fingerprint.into(),
        result: result.clone(),
        checksum: String::new(),
    };
    receipt.checksum = receipt_checksum(&receipt)?;
    let bytes = serde_json::to_vec_pretty(&receipt).map_err(TransactionError::validation)?;
    if bytes.len() as u64 > MAX_RECEIPT_BYTES {
        return Err(TransactionError::validation(
            "OpenSpec transaction receipt exceeds the serialized byte limit",
        ));
    }
    atomic_durable_write(&receipt_path(repo, operation_id), &bytes)
}

fn journal_checksum(journal: &Journal) -> TxResult<String> {
    let mut value = journal.clone();
    value.checksum.clear();
    serde_json::to_vec(&value)
        .map(|bytes| identity(&bytes))
        .map_err(TransactionError::validation)
}

fn receipt_checksum(receipt: &DurableOpenSpecReceipt) -> TxResult<String> {
    let mut value = receipt.clone();
    value.checksum.clear();
    serde_json::to_vec(&value)
        .map(|bytes| identity(&bytes))
        .map_err(TransactionError::validation)
}

fn repository_id(repo: &Path) -> TxResult<String> {
    let canonical = repo.canonicalize().map_err(TransactionError::persistence)?;
    Ok(identity(canonical.to_string_lossy().as_bytes()))
}

fn transaction_root(repo: &Path) -> PathBuf {
    repo.join("ai/lifecycle/transactions/repository-v1")
}

fn pending_path(repo: &Path, operation_id: &str) -> PathBuf {
    transaction_root(repo).join("pending").join(format!(
        "{}.json",
        crate::lifecycle_transaction::operation_record_name(operation_id)
    ))
}

fn receipt_path(repo: &Path, operation_id: &str) -> PathBuf {
    transaction_root(repo).join("receipts").join(format!(
        "{}.json",
        crate::lifecycle_transaction::operation_record_name(operation_id)
    ))
}

fn validate_operation_id(value: &str) -> TxResult<()> {
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(TransactionError::validation(
            "invalid lifecycle operation id",
        ));
    }
    Ok(())
}

fn validate_name(value: &str) -> TxResult<()> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(TransactionError::validation(format!(
            "invalid OpenSpec change name: {value}"
        )));
    }
    Ok(())
}

fn relative_path(repo: &Path, path: &Path) -> TxResult<String> {
    let relative = path
        .strip_prefix(repo)
        .map_err(TransactionError::validation)?;
    if relative
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(TransactionError::validation(
            "transaction path is not contained",
        ));
    }
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

fn contained_path(repo: &Path, relative: &str) -> TxResult<PathBuf> {
    let relative = Path::new(relative);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(TransactionError::validation(
            "transaction path is not contained",
        ));
    }
    let path = repo.join(relative);
    validate_no_follow_path(repo, &path)?;
    Ok(path)
}

fn validate_no_follow_path(repo: &Path, path: &Path) -> TxResult<()> {
    let relative = path
        .strip_prefix(repo)
        .map_err(TransactionError::validation)?;
    let mut current = repo.to_path_buf();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            return Err(TransactionError::validation(
                "path contains an unsafe component",
            ));
        };
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(TransactionError::validation(format!(
                    "symbolic-link transaction path rejected: {}",
                    current.display()
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(TransactionError::persistence(error)),
        }
    }
    Ok(())
}

fn ensure_absent(path: &Path) -> TxResult<()> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Ok(_) => Err(TransactionError::validation(format!(
            "target already exists: {}",
            path.display()
        ))),
        Err(error) => Err(TransactionError::persistence(error)),
    }
}

fn ensure_directory(path: &Path) -> TxResult<()> {
    let metadata = fs::symlink_metadata(path).map_err(TransactionError::validation)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(TransactionError::validation(format!(
            "expected regular directory: {}",
            path.display()
        )));
    }
    Ok(())
}

fn read_regular_file(path: &Path) -> TxResult<Vec<u8>> {
    let metadata = fs::symlink_metadata(path).map_err(TransactionError::validation)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(TransactionError::validation(format!(
            "expected regular file: {}",
            path.display()
        )));
    }
    fs::read(path).map_err(TransactionError::persistence)
}

fn file_identity(path: &Path) -> TxResult<Option<String>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(TransactionError::validation(format!(
                "transaction file is not regular: {}",
                path.display()
            )))
        }
        Ok(_) => fs::read(path)
            .map(|bytes| Some(identity(&bytes)))
            .map_err(TransactionError::persistence),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(TransactionError::persistence(error)),
    }
}

fn tree_identity_optional(path: &Path) -> TxResult<Option<String>> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(TransactionError::persistence(error)),
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            Err(TransactionError::validation(format!(
                "transaction tree is not a regular directory: {}",
                path.display()
            )))
        }
        Ok(_) => tree_identity(path).map(Some),
    }
}

fn tree_identity(path: &Path) -> TxResult<String> {
    tree_identity_with_override(path, Path::new(""), &[])
}

fn tree_identity_with_override(
    path: &Path,
    override_path: &Path,
    override_bytes: &[u8],
) -> TxResult<String> {
    let mut entries = Vec::new();
    let mut bytes = 0usize;
    let mut visited = 0usize;
    collect_tree(path, path, &mut entries, &mut bytes, &mut visited, 0)?;
    if !override_path.as_os_str().is_empty() {
        let key = override_path.to_string_lossy().replace('\\', "/");
        let entry = entries
            .iter_mut()
            .find(|(relative, _)| relative == &key)
            .ok_or_else(|| TransactionError::validation("tree override target is absent"))?;
        entry.1 = identity(override_bytes);
    }
    serde_json::to_vec(&entries)
        .map(|bytes| identity(&bytes))
        .map_err(TransactionError::validation)
}

fn collect_tree(
    root: &Path,
    current: &Path,
    entries: &mut Vec<(String, String)>,
    bytes: &mut usize,
    visited: &mut usize,
    depth: usize,
) -> TxResult<()> {
    if depth > MAX_TREE_DEPTH {
        return Err(TransactionError::validation(
            "transaction tree exceeds the depth limit",
        ));
    }
    let mut children = Vec::new();
    for child in fs::read_dir(current).map_err(TransactionError::persistence)? {
        *visited = visited
            .checked_add(1)
            .ok_or_else(|| TransactionError::validation("transaction tree entry overflow"))?;
        if *visited > MAX_TREE_ENTRIES {
            return Err(TransactionError::validation(
                "transaction tree exceeds the entry limit",
            ));
        }
        children.push(child.map_err(TransactionError::persistence)?);
    }
    children.sort_by_key(std::fs::DirEntry::file_name);
    for child in children {
        let path = child.path();
        let metadata = child.metadata().map_err(TransactionError::persistence)?;
        if child
            .file_type()
            .map_err(TransactionError::persistence)?
            .is_symlink()
        {
            return Err(TransactionError::validation(format!(
                "symbolic link in transaction tree: {}",
                path.display()
            )));
        }
        if metadata.is_dir() {
            collect_tree(root, &path, entries, bytes, visited, depth + 1)?;
        } else if metadata.is_file() {
            let size = usize::try_from(metadata.len()).map_err(TransactionError::validation)?;
            *bytes = bytes
                .checked_add(size)
                .ok_or_else(|| TransactionError::validation("transaction tree size overflow"))?;
            if *bytes > MAX_BYTES {
                return Err(TransactionError::validation(
                    "transaction tree exceeds the byte limit",
                ));
            }
            let relative = path
                .strip_prefix(root)
                .map_err(TransactionError::validation)?
                .to_string_lossy()
                .replace('\\', "/");
            entries.push((
                relative,
                identity(&fs::read(path).map_err(TransactionError::persistence)?),
            ));
        } else {
            return Err(TransactionError::validation(
                "special file in transaction tree",
            ));
        }
    }
    Ok(())
}

fn read_bounded(path: &Path, limit: u64) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("{} exceeds the {limit}-byte limit", path.display()),
        ));
    }
    Ok(bytes)
}

fn spec_domains(change_dir: &Path) -> TxResult<Vec<String>> {
    let specs = change_dir.join("specs");
    if !specs.exists() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    let mut visited = 0usize;
    collect_spec_domains(&specs, &specs, &mut entries, &mut visited, 0)?;
    entries.sort();
    Ok(entries)
}

fn collect_spec_domains(
    root: &Path,
    current: &Path,
    domains: &mut Vec<String>,
    visited: &mut usize,
    depth: usize,
) -> TxResult<()> {
    if depth > MAX_TREE_DEPTH {
        return Err(TransactionError::validation(
            "OpenSpec specs exceed the depth limit",
        ));
    }
    for entry in fs::read_dir(current).map_err(TransactionError::persistence)? {
        let entry = entry.map_err(TransactionError::persistence)?;
        *visited = visited
            .checked_add(1)
            .ok_or_else(|| TransactionError::validation("OpenSpec specs entry overflow"))?;
        if *visited > MAX_TREE_ENTRIES {
            return Err(TransactionError::validation(
                "OpenSpec specs exceed the entry limit",
            ));
        }
        let file_type = entry.file_type().map_err(TransactionError::persistence)?;
        if file_type.is_symlink() {
            return Err(TransactionError::validation(
                "symbolic link in OpenSpec specs",
            ));
        }
        if file_type.is_dir() {
            collect_spec_domains(root, &entry.path(), domains, visited, depth + 1)?;
        } else if file_type.is_file()
            && entry.path().extension().and_then(|value| value.to_str()) == Some("md")
        {
            let mut relative = entry
                .path()
                .strip_prefix(root)
                .map_err(TransactionError::validation)?
                .to_path_buf();
            relative.set_extension("");
            domains.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

fn atomic_durable_write(path: &Path, bytes: &[u8]) -> TxResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| TransactionError::validation("path has no parent"))?;
    fs::create_dir_all(parent).map_err(TransactionError::persistence)?;
    let mut temporary =
        tempfile::NamedTempFile::new_in(parent).map_err(TransactionError::persistence)?;
    temporary
        .write_all(bytes)
        .map_err(TransactionError::persistence)?;
    temporary
        .as_file()
        .sync_all()
        .map_err(TransactionError::persistence)?;
    temporary
        .persist(path)
        .map_err(|error| TransactionError::persistence(error.error))?;
    sync_directory(parent)
}

fn remove_durable(path: &Path) -> TxResult<()> {
    fs::remove_file(path).map_err(TransactionError::persistence)?;
    sync_directory(path.parent().unwrap_or(Path::new(".")))
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> TxResult<()> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(TransactionError::persistence)
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> TxResult<()> {
    Ok(())
}

fn identity(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn check_cancelled(is_cancelled: &impl Fn() -> bool) -> TxResult<()> {
    if is_cancelled() {
        return Err(TransactionError::new(
            TransactionErrorCode::Cancelled,
            "lifecycle request cancelled",
        ));
    }
    Ok(())
}

trait MutationChangeName {
    fn change_name(&self) -> &str;
}

impl MutationChangeName for OpenSpecMutationV1 {
    fn change_name(&self) -> &str {
        match self {
            Self::Propose { name, .. } => name,
            Self::AddSpec { change, .. }
            | Self::ReconcileTasks { change }
            | Self::SetTaskStatus { change, .. }
            | Self::RegisterTestFile { change, .. }
            | Self::Transition { change, .. }
            | Self::Archive { change }
            | Self::Abandon { change }
            | Self::Reopen { change } => change,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roots(repo: &Path) -> RepositoryRoots {
        RepositoryRoots {
            design: repo.join("ai/docs"),
            openspec: repo.join("ai/openspec"),
            ledger: repo.join("ai/lifecycle/state.json"),
        }
    }

    fn revision() -> LifecycleRepositoryRevisionV1 {
        LifecycleRepositoryRevisionV1 {
            version: 1,
            design_root: "ai/docs".into(),
            openspec_root: "ai/openspec".into(),
            ledger_path: "ai/lifecycle/state.json".into(),
            ledger_identity: "committed".into(),
            ledger_revision: 1,
            artifact_digest: "committed".into(),
            transaction_digest: "committed".into(),
        }
    }

    fn journal(repo: &Path, resources: Vec<Resource>) -> Journal {
        let mut journal = Journal {
            version: JOURNAL_VERSION,
            domain: DOMAIN.into(),
            repository_id: repository_id(repo).unwrap(),
            operation_id: "recovery-operation".into(),
            semantic_fingerprint: "fingerprint".into(),
            resources,
            effects: vec!["recovered".into()],
            checksum: String::new(),
        };
        journal.checksum = journal_checksum(&journal).unwrap();
        journal
    }

    fn persist_journal(repo: &Path, journal: &Journal) {
        atomic_durable_write(
            &pending_path(repo, &journal.operation_id),
            &serde_json::to_vec_pretty(journal).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn recovery_rolls_artifact_forward_before_ledger_and_writes_receipt() {
        let dir = tempfile::tempdir().unwrap();
        let roots = roots(dir.path());
        fs::create_dir_all(&roots.openspec).unwrap();
        let artifact = roots.openspec.join("changes/change/proposal.md");
        fs::create_dir_all(artifact.parent().unwrap()).unwrap();
        fs::write(&artifact, b"before").unwrap();
        let resources = vec![
            file_resource(dir.path(), &artifact, b"after".to_vec(), None).unwrap(),
            file_resource(dir.path(), &roots.ledger, b"ledger-after".to_vec(), None).unwrap(),
        ];
        let journal = journal(dir.path(), resources);
        persist_journal(dir.path(), &journal);
        fs::write(&artifact, b"after").unwrap();

        let errors = recover_pending(dir.path(), &roots, || Ok(revision()));

        assert!(errors.is_empty());
        assert_eq!(fs::read(artifact).unwrap(), b"after");
        assert_eq!(fs::read(&roots.ledger).unwrap(), b"ledger-after");
        assert!(receipt_path(dir.path(), "recovery-operation").is_file());
        assert!(!pending_path(dir.path(), "recovery-operation").exists());
    }

    #[test]
    fn recovery_completes_reopen_after_directory_move() {
        let dir = tempfile::tempdir().unwrap();
        let roots = roots(dir.path());
        let source = roots.openspec.join("archive/change");
        let destination = roots.openspec.join("changes/change");
        fs::create_dir_all(&source).unwrap();
        let before = b"---\nstate: archived\n---\n# Change\n";
        let after = b"---\nstate: proposed\n---\n# Change\n";
        fs::write(source.join("proposal.md"), before).unwrap();
        let moved = tree_identity(&source).unwrap();
        let final_identity =
            tree_identity_with_override(&source, Path::new("proposal.md"), after).unwrap();
        let resources = vec![
            directory_move_resource(dir.path(), &source, &destination, &moved, &final_identity)
                .unwrap(),
            file_resource_with_initial(
                dir.path(),
                &destination.join("proposal.md"),
                after.to_vec(),
                None,
                Some(identity(before)),
            )
            .unwrap(),
            file_resource(dir.path(), &roots.ledger, b"ledger-after".to_vec(), None).unwrap(),
        ];
        let journal = journal(dir.path(), resources);
        persist_journal(dir.path(), &journal);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::rename(&source, &destination).unwrap();

        let errors = recover_pending(dir.path(), &roots, || Ok(revision()));

        assert!(errors.is_empty());
        assert!(!source.exists());
        assert_eq!(fs::read(destination.join("proposal.md")).unwrap(), after);
        assert_eq!(fs::read(&roots.ledger).unwrap(), b"ledger-after");
    }

    #[test]
    fn recovery_after_ledger_settlement_only_publishes_receipt() {
        let dir = tempfile::tempdir().unwrap();
        let roots = roots(dir.path());
        fs::create_dir_all(&roots.openspec).unwrap();
        let artifact = roots.openspec.join("changes/change/proposal.md");
        fs::create_dir_all(artifact.parent().unwrap()).unwrap();
        fs::write(&artifact, b"before").unwrap();
        let journal = journal(
            dir.path(),
            vec![
                file_resource(dir.path(), &artifact, b"after".to_vec(), None).unwrap(),
                file_resource(dir.path(), &roots.ledger, b"ledger-after".to_vec(), None).unwrap(),
            ],
        );
        persist_journal(dir.path(), &journal);
        settle_journal(dir.path(), &journal).unwrap();

        let errors = recover_pending(dir.path(), &roots, || Ok(revision()));

        assert!(errors.is_empty());
        let receipt = read_receipt(dir.path(), "recovery-operation")
            .unwrap()
            .unwrap();
        assert_eq!(receipt.result.committed_revision, revision());
    }

    #[test]
    fn recovery_quarantines_unexpected_bytes_without_partial_writes() {
        let dir = tempfile::tempdir().unwrap();
        let roots = roots(dir.path());
        fs::create_dir_all(&roots.openspec).unwrap();
        let first = roots.openspec.join("changes/change/proposal.md");
        let second = roots.openspec.join("changes/change/tasks.md");
        fs::create_dir_all(first.parent().unwrap()).unwrap();
        fs::write(&first, b"first-before").unwrap();
        fs::write(&second, b"second-before").unwrap();
        let journal = journal(
            dir.path(),
            vec![
                file_resource(dir.path(), &first, b"first-after".to_vec(), None).unwrap(),
                file_resource(dir.path(), &second, b"second-after".to_vec(), None).unwrap(),
                file_resource(dir.path(), &roots.ledger, b"ledger-after".to_vec(), None).unwrap(),
            ],
        );
        persist_journal(dir.path(), &journal);
        fs::write(&second, b"operator-edit").unwrap();

        let errors = recover_pending(dir.path(), &roots, || Ok(revision()));

        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, TransactionErrorCode::RecoveryRequired);
        assert_eq!(fs::read(first).unwrap(), b"first-before");
        assert_eq!(fs::read(second).unwrap(), b"operator-edit");
        assert!(!roots.ledger.exists());
        assert!(
            transaction_root(dir.path())
                .join("quarantine")
                .join(format!(
                    "{}.json",
                    crate::lifecycle_transaction::operation_record_name("recovery-operation")
                ))
                .is_file()
        );
    }

    #[test]
    fn recovery_quarantines_rechecksummed_path_tampering() {
        let dir = tempfile::tempdir().unwrap();
        let roots = roots(dir.path());
        fs::create_dir_all(&roots.openspec).unwrap();
        let artifact = roots.openspec.join("changes/change/proposal.md");
        let mut journal = journal(
            dir.path(),
            vec![
                file_resource(dir.path(), &artifact, b"after".to_vec(), None).unwrap(),
                file_resource(dir.path(), &roots.ledger, b"ledger-after".to_vec(), None).unwrap(),
            ],
        );
        let Resource::File { path, .. } = &mut journal.resources[0] else {
            unreachable!();
        };
        *path = "ai/openspec/config.md".into();
        journal.checksum = journal_checksum(&journal).unwrap();
        persist_journal(dir.path(), &journal);

        let errors = recover_pending(dir.path(), &roots, || Ok(revision()));

        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, TransactionErrorCode::RecoveryRequired);
        assert!(!roots.openspec.join("config.md").exists());
        assert!(!roots.ledger.exists());
        assert!(!pending_path(dir.path(), "recovery-operation").exists());
    }

    #[test]
    fn recovery_rejects_conflicting_receipt_before_settlement() {
        let dir = tempfile::tempdir().unwrap();
        let roots = roots(dir.path());
        let artifact = roots.openspec.join("changes/change/proposal.md");
        fs::create_dir_all(artifact.parent().unwrap()).unwrap();
        fs::write(&artifact, b"before").unwrap();
        let journal = journal(
            dir.path(),
            vec![
                file_resource(dir.path(), &artifact, b"after".to_vec(), None).unwrap(),
                file_resource(dir.path(), &roots.ledger, b"ledger-after".to_vec(), None).unwrap(),
            ],
        );
        persist_journal(dir.path(), &journal);
        write_receipt(
            dir.path(),
            "recovery-operation",
            "different-fingerprint",
            &LifecycleMutationReceiptV1 {
                operation_id: "recovery-operation".into(),
                replayed: false,
                committed_revision: revision(),
                effects: vec!["different operation".into()],
                outcome: LifecycleMutationOutcomeV1::None,
            },
        )
        .unwrap();

        let errors = recover_pending(dir.path(), &roots, || Ok(revision()));

        assert_eq!(errors.len(), 1);
        assert_eq!(fs::read(artifact).unwrap(), b"before");
        assert!(!roots.ledger.exists());
    }

    #[test]
    fn recovery_does_not_reapply_a_receipted_operation() {
        let dir = tempfile::tempdir().unwrap();
        let roots = roots(dir.path());
        let artifact = roots.openspec.join("changes/change/proposal.md");
        fs::create_dir_all(artifact.parent().unwrap()).unwrap();
        fs::write(&artifact, b"before operation").unwrap();
        let journal = journal(
            dir.path(),
            vec![
                file_resource_with_initial(
                    dir.path(),
                    &artifact,
                    b"committed operation".to_vec(),
                    Some(identity(b"before operation")),
                    Some(identity(b"before operation")),
                )
                .unwrap(),
                file_resource(dir.path(), &roots.ledger, b"ledger-after".to_vec(), None).unwrap(),
            ],
        );
        persist_journal(dir.path(), &journal);
        write_receipt(
            dir.path(),
            "recovery-operation",
            &journal.semantic_fingerprint,
            &LifecycleMutationReceiptV1 {
                operation_id: "recovery-operation".into(),
                replayed: false,
                committed_revision: revision(),
                effects: journal.effects.clone(),
                outcome: LifecycleMutationOutcomeV1::None,
            },
        )
        .unwrap();

        let errors = recover_pending(dir.path(), &roots, || Ok(revision()));

        assert_eq!(errors.len(), 1);
        assert_eq!(fs::read(artifact).unwrap(), b"before operation");
        assert!(!roots.ledger.exists());
    }
}
