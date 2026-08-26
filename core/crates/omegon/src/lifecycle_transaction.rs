//! Shared durable repository transaction mechanics and design transactions.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use fs2::FileExt;
use omegon_opsx::design_artifacts::{
    DesignDecision, DesignNodeArtifact, DesignSections, FileScope, ImplementationNotes, IssueType,
    ResearchEntry,
};
use omegon_opsx::{
    Decision, DecisionStatus, JsonFileStoreTransaction, Lifecycle, LifecycleState, NodeState,
    Priority, RewriteSafety, StateStore, parse_design_artifact, render_design_artifact,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::lifecycle_service::{
    DesignIssueTypeV1, DesignMutationV1, LifecycleMutationOutcomeV1, LifecycleMutationReceiptV1,
    LifecycleRepositoryRevisionV1, RepositoryRoots,
};

const JOURNAL_VERSION: u32 = 1;
const RECEIPT_VERSION: u32 = 1;
const DESIGN_DOMAIN: &str = "design-v1";
const MAX_JOURNAL_RESOURCES: usize = 128;
const MAX_JOURNAL_BYTES: usize = 64 * 1024 * 1024;
const MAX_EFFECTS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TransactionErrorCode {
    Cancelled,
    StaleRevision,
    OperationConflict,
    Validation,
    RecoveryRequired,
    Persistence,
}

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub(super) struct TransactionError {
    pub code: TransactionErrorCode,
    pub message: String,
}

pub(super) struct CommitContext<'a> {
    pub repo: &'a Path,
    pub roots: &'a RepositoryRoots,
    pub operation_id: &'a str,
    pub semantic_fingerprint: &'a str,
    pub pre_revision: &'a LifecycleRepositoryRevisionV1,
    pub ledger: &'a JsonFileStoreTransaction,
}

impl TransactionError {
    pub(super) fn new(code: TransactionErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub(super) fn validation(error: impl std::fmt::Display) -> Self {
        Self::new(TransactionErrorCode::Validation, error.to_string())
    }

    pub(super) fn persistence(error: impl std::fmt::Display) -> Self {
        Self::new(TransactionErrorCode::Persistence, error.to_string())
    }
}

pub(super) struct RepositoryTransactionLock(File);

impl Drop for RepositoryTransactionLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DesignJournal {
    version: u32,
    domain: String,
    repository_id: String,
    operation_id: String,
    semantic_fingerprint: String,
    files: Vec<JournalFile>,
    effects: Vec<String>,
    checksum: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JournalFile {
    path: String,
    pre_identity: Option<String>,
    post_identity: String,
    post_bytes: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct DurableDesignReceipt {
    version: u32,
    domain: String,
    repository_id: String,
    operation_id: String,
    semantic_fingerprint: String,
    result: LifecycleMutationReceiptV1,
    checksum: String,
}

#[derive(Clone)]
pub(super) struct SeededStore {
    state: Arc<Mutex<LifecycleState>>,
}

impl SeededStore {
    fn new(state: LifecycleState) -> Self {
        Self {
            state: Arc::new(Mutex::new(state)),
        }
    }

    fn state(&self) -> anyhow::Result<LifecycleState> {
        self.state
            .lock()
            .map(|state| state.clone())
            .map_err(|_| anyhow::anyhow!("staging lifecycle store lock poisoned"))
    }
}

impl StateStore for SeededStore {
    fn load(&self) -> Result<LifecycleState, omegon_opsx::OpsxError> {
        self.state
            .lock()
            .map(|state| state.clone())
            .map_err(|_| omegon_opsx::OpsxError::StoreError("staging store lock poisoned".into()))
    }

    fn save(
        &self,
        state: &LifecycleState,
        expected_revision: u64,
    ) -> Result<(), omegon_opsx::OpsxError> {
        let mut current = self.state.lock().map_err(|_| {
            omegon_opsx::OpsxError::StoreError("staging store lock poisoned".into())
        })?;
        if current.revision != expected_revision {
            return Err(omegon_opsx::OpsxError::RevisionConflict {
                expected: expected_revision,
                actual: current.revision,
            });
        }
        *current = state.clone();
        Ok(())
    }
}

pub(super) fn stage_ledger_state(
    pre_state: &LifecycleState,
    apply: impl FnOnce(&mut Lifecycle<SeededStore>) -> anyhow::Result<()>,
) -> anyhow::Result<LifecycleState> {
    let staging = SeededStore::new(pre_state.clone());
    let mut lifecycle = Lifecycle::load(staging.clone())?;
    apply(&mut lifecycle)?;
    let mut post_state = staging.state()?;
    post_state.revision = pre_state
        .revision
        .checked_add(1)
        .ok_or_else(|| anyhow::anyhow!("lifecycle state revision overflow"))?;
    Ok(post_state)
}

pub(super) fn lock_repository(repo: &Path) -> anyhow::Result<RepositoryTransactionLock> {
    let path = repo.join("ai/lifecycle/repository-transactions.lock");
    validate_no_follow_path(repo, &path)?;
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("transaction lock has no parent"))?;
    fs::create_dir_all(parent)?;
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&path)?;
    file.lock_exclusive()
        .map_err(|error| anyhow::anyhow!("lock {}: {error}", path.display()))?;
    Ok(RepositoryTransactionLock(file))
}

pub(super) fn semantic_fingerprint(mutation: &DesignMutationV1) -> anyhow::Result<String> {
    let mut bytes = DESIGN_DOMAIN.as_bytes().to_vec();
    bytes.push(0);
    bytes.extend(serde_json::to_vec(mutation)?);
    Ok(identity(&bytes))
}

pub(super) fn preflight_mutation_repository(
    repo: &Path,
    roots: &RepositoryRoots,
) -> anyhow::Result<()> {
    validate_mutation_paths(repo, roots)?;
    read_design_artifacts(&roots.design)?;
    Ok(())
}

pub(super) fn read_receipt(
    repo: &Path,
    operation_id: &str,
) -> anyhow::Result<Option<DurableDesignReceipt>> {
    validate_operation_id(operation_id)?;
    let path = receipt_path(repo, operation_id);
    validate_no_follow_path(repo, &path)?;
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let envelope: serde_json::Value = serde_json::from_slice(&bytes).map_err(|error| {
        anyhow::anyhow!("corrupt lifecycle receipt {}: {error}", path.display())
    })?;
    if envelope.get("domain").and_then(serde_json::Value::as_str) != Some(DESIGN_DOMAIN) {
        return Err(TransactionError::new(
            TransactionErrorCode::OperationConflict,
            "lifecycle operation id is already committed in another transaction domain",
        )
        .into());
    }
    let receipt: DurableDesignReceipt = serde_json::from_slice(&bytes)
        .map_err(|error| anyhow::anyhow!("corrupt design receipt {}: {error}", path.display()))?;
    if receipt.version != RECEIPT_VERSION
        || receipt.domain != DESIGN_DOMAIN
        || receipt.repository_id != repository_id(repo)?
        || receipt.operation_id != operation_id
        || receipt.checksum != receipt_checksum(&receipt)?
    {
        anyhow::bail!("invalid design receipt: {}", path.display());
    }
    Ok(Some(receipt))
}

pub(super) fn receipt_fingerprint(receipt: &DurableDesignReceipt) -> &str {
    &receipt.semantic_fingerprint
}

pub(super) fn receipt_result(receipt: DurableDesignReceipt) -> LifecycleMutationReceiptV1 {
    receipt.result
}

pub(super) fn stage_and_commit(
    context: CommitContext<'_>,
    mutation: &DesignMutationV1,
    is_cancelled: &impl Fn() -> bool,
    mut observe_revision: impl FnMut() -> anyhow::Result<LifecycleRepositoryRevisionV1>,
) -> anyhow::Result<LifecycleMutationReceiptV1> {
    let CommitContext {
        repo,
        roots,
        operation_id,
        semantic_fingerprint,
        pre_revision,
        ledger: ledger_transaction,
    } = context;
    validate_operation_id(operation_id)?;
    validate_mutation_paths(repo, roots)?;
    check_cancellation(is_cancelled)?;
    if ledger_transaction.path() != roots.ledger {
        anyhow::bail!("selected ledger transaction does not match the frozen ledger");
    }
    let pre_state = ledger_transaction.load()?;
    if pre_state.revision != pre_revision.ledger_revision {
        return Err(TransactionError::new(
            TransactionErrorCode::StaleRevision,
            "stale lifecycle repository revision",
        )
        .into());
    }
    let (mut files, effects) = stage_mutation(repo, roots, mutation, &pre_state, is_cancelled)?;
    let artifacts = read_design_artifacts(&roots.design)?;
    let staging = SeededStore::new(reconcile_existing_nodes(pre_state.clone(), &artifacts));
    let mut lifecycle = Lifecycle::load(staging.clone())?;
    apply_fsm(&mut lifecycle, roots, mutation)?;
    let mut post_state = staging.state()?;
    post_state.revision = pre_state
        .revision
        .checked_add(1)
        .ok_or_else(|| anyhow::anyhow!("lifecycle state revision overflow"))?;
    let ledger_bytes = serde_json::to_vec_pretty(&post_state)?;
    // Canonical artifacts settle before the enforcement ledger. Recovery can
    // roll either frontier forward, but readers must not observe a ledger that
    // claims an artifact transition before that artifact is visible.
    files.push(staged_file(repo, ledger_transaction.path(), ledger_bytes)?);

    if observe_revision()? != *pre_revision {
        return Err(TransactionError::new(
            TransactionErrorCode::StaleRevision,
            "stale lifecycle repository revision after mutation staging",
        )
        .into());
    }
    check_cancellation(is_cancelled)?;

    let mut journal = DesignJournal {
        version: JOURNAL_VERSION,
        domain: DESIGN_DOMAIN.into(),
        repository_id: repository_id(repo)?,
        operation_id: operation_id.into(),
        semantic_fingerprint: semantic_fingerprint.into(),
        files,
        effects: effects.clone(),
        checksum: String::new(),
    };
    journal.checksum = journal_checksum(&journal)?;
    let journal_path = pending_path(repo, operation_id);
    validate_no_follow_path(repo, &journal_path)?;
    atomic_durable_write(&journal_path, &serde_json::to_vec_pretty(&journal)?)?;
    settle_journal(repo, &journal)?;

    let revision = observe_revision()?;
    let result = LifecycleMutationReceiptV1 {
        operation_id: operation_id.into(),
        replayed: false,
        committed_revision: revision,
        effects,
        outcome: match mutation {
            DesignMutationV1::Create { id, .. } => LifecycleMutationOutcomeV1::DesignCreated {
                path: roots.design.join(format!("{id}.md")).display().to_string(),
            },
            DesignMutationV1::ImplementOpenSpec { id } => {
                LifecycleMutationOutcomeV1::DesignImplemented {
                    node_id: id.clone(),
                    change: id.clone(),
                    path: roots
                        .openspec
                        .join("changes")
                        .join(id)
                        .display()
                        .to_string(),
                }
            }
            _ => LifecycleMutationOutcomeV1::None,
        },
    };
    let mut receipt = DurableDesignReceipt {
        version: RECEIPT_VERSION,
        domain: DESIGN_DOMAIN.into(),
        repository_id: repository_id(repo)?,
        operation_id: operation_id.into(),
        semantic_fingerprint: semantic_fingerprint.into(),
        result: result.clone(),
        checksum: String::new(),
    };
    receipt.checksum = receipt_checksum(&receipt)?;
    let receipt_path = receipt_path(repo, operation_id);
    validate_no_follow_path(repo, &receipt_path)?;
    atomic_durable_write(&receipt_path, &serde_json::to_vec_pretty(&receipt)?)?;
    remove_durable(&journal_path)?;
    Ok(result)
}

pub(super) fn recover_pending(
    repo: &Path,
    roots: &RepositoryRoots,
    mut committed_revision: impl FnMut() -> anyhow::Result<LifecycleRepositoryRevisionV1>,
) -> Vec<String> {
    let mut blockers = quarantined_findings(repo);
    let pending = transaction_root(repo).join("pending");
    if let Err(error) = validate_no_follow_path(repo, &pending) {
        blockers.push(format!("design recovery path rejected: {error}"));
        return blockers;
    }
    let entries = match fs::read_dir(&pending) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return blockers,
        Err(error) => {
            blockers.push(format!("design recovery unavailable: {error}"));
            return blockers;
        }
    };
    let mut paths = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                blockers.push(format!("scan design transaction: {error}"));
                continue;
            }
        };
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        match entry.file_type() {
            Ok(file_type) if file_type.is_symlink() => {
                let quarantine = transaction_root(repo)
                    .join("quarantine")
                    .join(entry.file_name());
                if validate_no_follow_path(repo, &quarantine).is_err() {
                    blockers.push("design quarantine path rejected".into());
                    continue;
                }
                if let Err(error) = fs::create_dir_all(quarantine.parent().unwrap_or(repo))
                    .and_then(|()| fs::rename(&path, &quarantine))
                {
                    blockers.push(format!(
                        "symbolic-link design transaction quarantine failed: {error}"
                    ));
                } else {
                    blockers.push("quarantined symbolic-link design transaction".into());
                }
            }
            Ok(file_type) if file_type.is_file() => paths.push(path),
            Ok(_) => blockers.push("invalid non-file design transaction record".into()),
            Err(error) => blockers.push(format!("inspect design transaction: {error}")),
        }
    }
    paths.sort();
    for path in paths {
        if fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .and_then(|value| {
                value
                    .get("domain")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            })
            .is_some_and(|domain| domain == "openspec-v1")
        {
            continue;
        }
        if let Err(error) = recover_one(repo, roots, &path, &mut committed_revision) {
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("invalid-journal")
                .to_string();
            let quarantine = transaction_root(repo).join("quarantine").join(name);
            if validate_no_follow_path(repo, &quarantine).is_err() {
                blockers.push(format!("design quarantine path rejected: {error}"));
                continue;
            }
            if let Err(quarantine_error) = fs::create_dir_all(quarantine.parent().unwrap_or(repo))
                .and_then(|()| fs::rename(&path, &quarantine))
            {
                blockers.push(format!(
                    "design transaction recovery failed ({error}); quarantine failed ({quarantine_error})"
                ));
            } else {
                blockers.push(format!("quarantined design transaction: {error}"));
            }
        }
    }
    blockers
}

fn quarantined_findings(repo: &Path) -> Vec<String> {
    let directory = transaction_root(repo).join("quarantine");
    if let Err(error) = validate_no_follow_path(repo, &directory) {
        return vec![format!("design quarantine path rejected: {error}")];
    }
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => return vec![format!("design quarantine unavailable: {error}")],
    };
    let mut findings = Vec::new();
    for entry in entries {
        match entry {
            Ok(entry)
                if entry.path().extension().and_then(|value| value.to_str()) == Some("json") =>
            {
                let path = entry.path();
                let name = path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("invalid-journal");
                findings.push(format!("quarantined repository transaction: {name}"));
            }
            Ok(_) => {}
            Err(error) => findings.push(format!("scan transaction quarantine: {error}")),
        }
    }
    findings.sort();
    findings
}

fn recover_one(
    repo: &Path,
    roots: &RepositoryRoots,
    path: &Path,
    committed_revision: &mut impl FnMut() -> anyhow::Result<LifecycleRepositoryRevisionV1>,
) -> anyhow::Result<()> {
    let bytes = fs::read(path)?;
    let journal: DesignJournal = serde_json::from_slice(&bytes)?;
    validate_journal(repo, roots, &journal)?;
    let receipt = read_receipt(repo, &journal.operation_id)?;
    if let Some(receipt) = &receipt
        && receipt.semantic_fingerprint != journal.semantic_fingerprint
    {
        anyhow::bail!("journal conflicts with committed operation receipt");
    }
    if receipt.is_some() {
        for file in &journal.files {
            if file_identity(&contained_path(repo, &file.path)?)?.as_deref()
                != Some(&file.post_identity)
            {
                anyhow::bail!("committed design operation has an unsettled journal resource");
            }
        }
        return remove_durable(path);
    }
    settle_journal(repo, &journal)?;
    let result = LifecycleMutationReceiptV1 {
        operation_id: journal.operation_id.clone(),
        replayed: false,
        committed_revision: committed_revision()?,
        effects: journal.effects.clone(),
        outcome: LifecycleMutationOutcomeV1::None,
    };
    let mut receipt = DurableDesignReceipt {
        version: RECEIPT_VERSION,
        domain: DESIGN_DOMAIN.into(),
        repository_id: journal.repository_id.clone(),
        operation_id: journal.operation_id.clone(),
        semantic_fingerprint: journal.semantic_fingerprint.clone(),
        result,
        checksum: String::new(),
    };
    receipt.checksum = receipt_checksum(&receipt)?;
    let receipt_path = receipt_path(repo, &journal.operation_id);
    validate_no_follow_path(repo, &receipt_path)?;
    atomic_durable_write(&receipt_path, &serde_json::to_vec_pretty(&receipt)?)?;
    remove_durable(path)
}

fn validate_journal(
    repo: &Path,
    roots: &RepositoryRoots,
    journal: &DesignJournal,
) -> anyhow::Result<()> {
    if journal.version != JOURNAL_VERSION
        || journal.domain != DESIGN_DOMAIN
        || journal.repository_id != repository_id(repo)?
        || journal.checksum != journal_checksum(journal)?
    {
        anyhow::bail!("invalid design journal identity, version, or checksum");
    }
    validate_operation_id(&journal.operation_id)?;
    if journal.files.is_empty() {
        anyhow::bail!("design journal contains no resources");
    }
    let staged_bytes = journal.files.iter().try_fold(0usize, |total, file| {
        total.checked_add(file.post_bytes.len())
    });
    if journal.files.len() > MAX_JOURNAL_RESOURCES
        || journal.effects.len() > MAX_EFFECTS
        || staged_bytes.is_none_or(|total| total > MAX_JOURNAL_BYTES)
    {
        anyhow::bail!("design journal exceeds bounded resource limits");
    }
    let mut paths = BTreeSet::new();
    for file in &journal.files {
        if !paths.insert(file.path.as_str()) {
            anyhow::bail!("design journal contains duplicate resource: {}", file.path);
        }
        let path = contained_path(repo, &file.path)?;
        let ledger = roots.ledger.clone();
        if !path.starts_with(&roots.design) && !path.starts_with(&roots.openspec) && path != ledger
        {
            anyhow::bail!("design journal path is outside frozen roots: {}", file.path);
        }
        if identity(&file.post_bytes) != file.post_identity {
            anyhow::bail!(
                "design journal staged content identity mismatch: {}",
                file.path
            );
        }
    }
    let ledger_path = relative_path(repo, &roots.ledger)?;
    if journal
        .files
        .iter()
        .filter(|file| file.path == ledger_path)
        .count()
        != 1
        || journal.files.last().map(|file| file.path.as_str()) != Some(ledger_path.as_str())
    {
        anyhow::bail!("design journal must contain exactly one ledger resource, settled last");
    }
    Ok(())
}

fn settle_journal(repo: &Path, journal: &DesignJournal) -> anyhow::Result<()> {
    let mut settlement = Vec::with_capacity(journal.files.len());
    for file in &journal.files {
        let path = contained_path(repo, &file.path)?;
        let observed = file_identity(&path)?;
        if observed.as_deref() == Some(&file.post_identity) {
            settlement.push((path, file, false));
            continue;
        }
        if observed != file.pre_identity {
            anyhow::bail!(
                "transaction resource has neither pre nor post identity: {}",
                file.path
            );
        }
        settlement.push((path, file, true));
    }
    for (path, file, needs_write) in settlement {
        if needs_write {
            atomic_durable_write(&path, &file.post_bytes)?;
        }
    }
    Ok(())
}

fn stage_mutation(
    repo: &Path,
    roots: &RepositoryRoots,
    mutation: &DesignMutationV1,
    pre_state: &LifecycleState,
    is_cancelled: &impl Fn() -> bool,
) -> anyhow::Result<(Vec<JournalFile>, Vec<String>)> {
    check_cancellation(is_cancelled)?;
    let mut artifacts = read_design_artifacts(&roots.design)?;
    let mut staged = BTreeMap::<PathBuf, Vec<u8>>::new();
    let mut effects = Vec::new();
    match mutation {
        DesignMutationV1::Create {
            id,
            title,
            parent,
            status,
            tags,
            overview,
        } => {
            validate_entity_id(id)?;
            if artifacts.contains_key(id) {
                anyhow::bail!("design node '{id}' already exists");
            }
            let target = roots.design.join(format!("{id}.md"));
            if target.exists() {
                anyhow::bail!("design node target already exists: {}", target.display());
            }
            if let Some(parent) = parent
                && !artifacts.contains_key(parent)
            {
                anyhow::bail!("parent design node '{parent}' not found");
            }
            let state = status.unwrap_or(NodeState::Seed);
            let artifact = DesignNodeArtifact {
                id: id.clone(),
                title: title.clone(),
                state,
                parent: parent.clone(),
                tags: tags.clone(),
                dependencies: Vec::new(),
                related: Vec::new(),
                open_questions: Vec::new(),
                branches: Vec::new(),
                openspec_change: None,
                issue_type: None,
                priority: None,
                archive_reason: None,
                superseded_by: None,
                archived_at: None,
            };
            let sections = DesignSections {
                overview: overview.clone(),
                ..Default::default()
            };
            staged.insert(
                target,
                render_design_artifact(&artifact, &sections).into_bytes(),
            );
            effects.push(format!("created design node {id}"));
        }
        DesignMutationV1::BranchQuestion {
            parent_id,
            question,
            child_id,
            child_title,
        } => {
            validate_entity_id(child_id)?;
            if artifacts.contains_key(child_id) {
                anyhow::bail!("design node '{child_id}' already exists");
            }
            let child_path = roots.design.join(format!("{child_id}.md"));
            if child_path.exists() {
                anyhow::bail!(
                    "design child target already exists: {}",
                    child_path.display()
                );
            }
            let parent = artifact_mut(&mut artifacts, parent_id)?;
            require_rewrite_safe(parent)?;
            if !parent.sections.open_questions.contains(question) {
                anyhow::bail!("question not found on parent design node");
            }
            parent
                .sections
                .open_questions
                .retain(|value| value != question);
            parent.artifact.open_questions = parent.sections.open_questions.clone();
            if !parent.artifact.branches.contains(child_id) {
                parent.artifact.branches.push(child_id.clone());
            }
            staged.insert(
                parent.source_path.clone(),
                render_design_artifact(&parent.artifact, &parent.sections).into_bytes(),
            );
            let child = DesignNodeArtifact {
                id: child_id.clone(),
                title: child_title.clone(),
                state: NodeState::Seed,
                parent: Some(parent_id.clone()),
                tags: Vec::new(),
                dependencies: Vec::new(),
                related: Vec::new(),
                open_questions: Vec::new(),
                branches: Vec::new(),
                openspec_change: None,
                issue_type: None,
                priority: None,
                archive_reason: None,
                superseded_by: None,
                archived_at: None,
            };
            staged.insert(
                child_path,
                render_design_artifact(&child, &DesignSections::default()).into_bytes(),
            );
            effects.extend([
                format!("created design node {child_id}"),
                format!("removed parent question from {parent_id}"),
            ]);
        }
        DesignMutationV1::ImplementOpenSpec { id } => {
            let node = artifact_mut(&mut artifacts, id)?;
            require_rewrite_safe(node)?;
            if node.artifact.state != NodeState::Decided {
                anyhow::bail!("design node '{id}' must be decided before implementation");
            }
            let change_dir = roots.openspec.join("changes").join(id);
            if change_dir.exists() {
                anyhow::bail!("OpenSpec change '{id}' already exists");
            }
            node.artifact.state = NodeState::Implementing;
            node.artifact.openspec_change = Some(id.clone());
            staged.insert(
                node.source_path.clone(),
                render_design_artifact(&node.artifact, &node.sections).into_bytes(),
            );
            let intent = if node.sections.overview.is_empty() {
                format!("Implement {}", node.artifact.title)
            } else {
                node.sections.overview.clone()
            };
            let proposal = format!(
                "---\nstate: proposed\n---\n\n# {}\n\n## Intent\n\n{}\n\n## Scope\n\n_TBD_\n\n## Constraints\n\n_None identified yet._\n",
                node.artifact.title, intent
            );
            staged.insert(change_dir.join("proposal.md"), proposal.into_bytes());
            effects.extend([
                format!("updated design node {id}"),
                format!("created OpenSpec change {id}"),
            ]);
        }
        _ => {
            let id = mutation.node_id();
            let node = artifact_mut(&mut artifacts, id)?;
            require_rewrite_safe(node)?;
            mutate_artifact(node, mutation)?;
            staged.insert(
                node.source_path.clone(),
                render_design_artifact(&node.artifact, &node.sections).into_bytes(),
            );
            effects.push(format!("updated design node {id}"));
        }
    }
    if pre_state.version > omegon_opsx::store::SCHEMA_VERSION {
        anyhow::bail!("unsupported lifecycle state version");
    }
    let mut files = Vec::with_capacity(staged.len());
    for (path, bytes) in staged {
        check_cancellation(is_cancelled)?;
        files.push(staged_file(repo, &path, bytes)?);
    }
    Ok((files, effects))
}

fn mutate_artifact(
    node: &mut omegon_opsx::ParsedDesignArtifact,
    mutation: &DesignMutationV1,
) -> anyhow::Result<()> {
    match mutation {
        DesignMutationV1::SetState {
            state,
            archive_reason,
            superseded_by,
            archived_at,
            ..
        } => {
            node.artifact.state = *state;
            if *state == NodeState::Archived {
                node.artifact.archive_reason = archive_reason.clone();
                node.artifact.superseded_by = superseded_by.clone();
                node.artifact.archived_at = archived_at.clone();
            } else {
                node.artifact.archive_reason = None;
                node.artifact.superseded_by = None;
                node.artifact.archived_at = None;
            }
        }
        DesignMutationV1::AddQuestion { question, .. } => {
            node.sections.open_questions.push(question.clone());
            node.artifact.open_questions = node.sections.open_questions.clone();
        }
        DesignMutationV1::RemoveQuestion { question, .. } => {
            node.sections
                .open_questions
                .retain(|value| value != question);
            node.artifact.open_questions = node.sections.open_questions.clone();
        }
        DesignMutationV1::AddResearch {
            heading, content, ..
        } => node.sections.research.push(ResearchEntry {
            heading: heading.clone(),
            content: content.clone(),
        }),
        DesignMutationV1::AddDecision {
            title,
            status,
            rationale,
            ..
        } => node.sections.decisions.push(DesignDecision {
            title: title.clone(),
            status: status.clone(),
            rationale: rationale.clone(),
        }),
        DesignMutationV1::AddDependency { target_id, .. } => {
            push_unique(&mut node.artifact.dependencies, target_id)
        }
        DesignMutationV1::RemoveDependency { target_id, .. } => node
            .artifact
            .dependencies
            .retain(|value| value != target_id),
        DesignMutationV1::AddRelated { target_id, .. } => {
            push_unique(&mut node.artifact.related, target_id)
        }
        DesignMutationV1::RemoveRelated { target_id, .. } => {
            node.artifact.related.retain(|value| value != target_id)
        }
        DesignMutationV1::AddImplementationNotes {
            file_scope,
            constraints,
            ..
        } => {
            node.sections.implementation = ImplementationNotes {
                file_scope: file_scope
                    .iter()
                    .map(|scope| FileScope {
                        path: scope.path.clone(),
                        description: scope.description.clone(),
                        action: scope.action.clone(),
                    })
                    .collect(),
                constraints: constraints.clone(),
            };
        }
        DesignMutationV1::SetPriority { priority, .. } => {
            if !(1..=5).contains(priority) {
                anyhow::bail!("design priority must be between 1 and 5");
            }
            node.artifact.priority = Some(*priority);
        }
        DesignMutationV1::SetIssueType { issue_type, .. } => {
            node.artifact.issue_type = Some((*issue_type).into())
        }
        DesignMutationV1::Create { .. }
        | DesignMutationV1::BranchQuestion { .. }
        | DesignMutationV1::ImplementOpenSpec { .. } => unreachable!(),
    }
    Ok(())
}

fn apply_fsm(
    lifecycle: &mut Lifecycle<SeededStore>,
    roots: &RepositoryRoots,
    mutation: &DesignMutationV1,
) -> anyhow::Result<()> {
    let artifacts = read_design_artifacts(&roots.design)?;
    let ensure = |lifecycle: &mut Lifecycle<SeededStore>, id: &str| -> anyhow::Result<()> {
        if lifecycle.get_node(id).is_some() {
            return Ok(());
        }
        let parsed = artifacts
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("design node '{id}' not found"))?;
        lifecycle.create_node(id, &parsed.artifact.title, None)?;
        if parsed.artifact.state != NodeState::Seed {
            lifecycle.force_transition_node(
                id,
                parsed.artifact.state,
                "reconcile canonical design artifact",
            )?;
        }
        for question in &parsed.sections.open_questions {
            lifecycle.add_question(id, question)?;
        }
        Ok(())
    };
    match mutation {
        DesignMutationV1::Create {
            id,
            title,
            parent,
            status,
            ..
        } => {
            if let Some(parent) = parent {
                ensure(lifecycle, parent)?;
            }
            lifecycle.create_node(id, title, parent.as_deref())?;
            if status.is_some_and(|state| state != NodeState::Seed) {
                lifecycle.force_transition_node(
                    id,
                    status.unwrap(),
                    "initial managed design state",
                )?;
            }
        }
        DesignMutationV1::SetState { id, state, .. } => {
            ensure(lifecycle, id)?;
            lifecycle.transition_node(id, *state)?;
        }
        DesignMutationV1::AddQuestion { id, question } => {
            ensure(lifecycle, id)?;
            lifecycle.add_question(id, question)?;
        }
        DesignMutationV1::RemoveQuestion { id, question } => {
            ensure(lifecycle, id)?;
            lifecycle.remove_question(id, question)?;
        }
        DesignMutationV1::AddDecision {
            id,
            title,
            status,
            rationale,
        } => {
            ensure(lifecycle, id)?;
            let status = match status.as_str() {
                "exploring" => DecisionStatus::Exploring,
                "decided" => DecisionStatus::Decided,
                "rejected" => DecisionStatus::Rejected,
                _ => anyhow::bail!("invalid decision status: {status}"),
            };
            lifecycle.add_decision(
                id,
                Decision {
                    title: title.clone(),
                    status,
                    rationale: rationale.clone(),
                },
            )?;
        }
        DesignMutationV1::SetPriority { id, priority } => {
            ensure(lifecycle, id)?;
            lifecycle.set_priority(id, Priority::new(*priority))?;
        }
        DesignMutationV1::SetIssueType { id, issue_type } => {
            ensure(lifecycle, id)?;
            lifecycle.set_issue_type(id, (*issue_type).into())?;
        }
        DesignMutationV1::BranchQuestion {
            parent_id,
            question,
            child_id,
            child_title,
        } => {
            ensure(lifecycle, parent_id)?;
            lifecycle.create_node(child_id, child_title, Some(parent_id))?;
            lifecycle.remove_question(parent_id, question)?;
        }
        DesignMutationV1::ImplementOpenSpec { id } => {
            ensure(lifecycle, id)?;
            let title = lifecycle.get_node(id).unwrap().title.clone();
            lifecycle.transition_node(id, NodeState::Implementing)?;
            lifecycle.create_change(id, &title, Some(id))?;
            lifecycle.bind_change(id, id)?;
        }
        DesignMutationV1::AddResearch { id, .. }
        | DesignMutationV1::AddDependency { id, .. }
        | DesignMutationV1::RemoveDependency { id, .. }
        | DesignMutationV1::AddRelated { id, .. }
        | DesignMutationV1::RemoveRelated { id, .. }
        | DesignMutationV1::AddImplementationNotes { id, .. } => {
            ensure(lifecycle, id)?;
            let overview = lifecycle.get_node(id).unwrap().overview.clone();
            lifecycle.set_overview(id, &overview)?;
        }
    }
    Ok(())
}

fn reconcile_existing_nodes(
    mut state: LifecycleState,
    artifacts: &BTreeMap<String, omegon_opsx::ParsedDesignArtifact>,
) -> LifecycleState {
    for node in &mut state.nodes {
        let Some(parsed) = artifacts.get(&node.id) else {
            continue;
        };
        node.title.clone_from(&parsed.artifact.title);
        node.state = parsed.artifact.state;
        node.parent.clone_from(&parsed.artifact.parent);
        node.tags.clone_from(&parsed.artifact.tags);
        node.priority = parsed.artifact.priority.map(Priority::new);
        node.issue_type = parsed.artifact.issue_type.map(|value| match value {
            IssueType::Epic => omegon_opsx::IssueType::Epic,
            IssueType::Feature => omegon_opsx::IssueType::Feature,
            IssueType::Task => omegon_opsx::IssueType::Task,
            IssueType::Bug => omegon_opsx::IssueType::Bug,
            IssueType::Chore => omegon_opsx::IssueType::Chore,
        });
        node.open_questions
            .clone_from(&parsed.sections.open_questions);
        node.overview.clone_from(&parsed.sections.overview);
        node.bound_change
            .clone_from(&parsed.artifact.openspec_change);
    }
    state
}

fn read_design_artifacts(
    root: &Path,
) -> anyhow::Result<BTreeMap<String, omegon_opsx::ParsedDesignArtifact>> {
    let mut paths = Vec::new();
    for directory in [root.to_path_buf(), root.join("design")] {
        match fs::symlink_metadata(&directory) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                anyhow::bail!(
                    "design directory must not be a symbolic link: {}",
                    directory.display()
                )
            }
            Ok(metadata) if !metadata.is_dir() => {
                anyhow::bail!("design path is not a directory: {}", directory.display())
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        }
        let entries = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        for entry in entries {
            let entry = entry?;
            let file_type = entry.file_type()?;
            let path = entry.path();
            if file_type.is_symlink() {
                anyhow::bail!(
                    "design artifact must not be a symbolic link: {}",
                    path.display()
                );
            }
            if file_type.is_file()
                && path.extension().and_then(|value| value.to_str()) == Some("md")
            {
                paths.push(path);
            }
        }
    }
    paths.sort();
    let mut artifacts = BTreeMap::new();
    for path in paths {
        let source = fs::read_to_string(&path)?;
        if !looks_like_design_artifact(&source) {
            continue;
        }
        let parsed = parse_design_artifact(&source, &path).map_err(|error| {
            anyhow::anyhow!(
                "malformed canonical design artifact {}: {error}",
                path.display()
            )
        })?;
        if artifacts
            .insert(parsed.artifact.id.clone(), parsed)
            .is_some()
        {
            anyhow::bail!("duplicate design node id");
        }
    }
    Ok(artifacts)
}

fn looks_like_design_artifact(source: &str) -> bool {
    let frontmatter = source
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---").map(|(head, _)| head))
        .or_else(|| {
            source
                .strip_prefix("+++\n")
                .and_then(|rest| rest.split_once("\n+++").map(|(head, _)| head))
        });
    frontmatter.is_some_and(|frontmatter| {
        frontmatter.lines().any(|line| {
            let line = line.trim();
            line.starts_with("id:")
                || line.starts_with("status:")
                || line.starts_with("id =")
                || line.starts_with("status =")
                || line == "[data]"
        })
    })
}

fn artifact_mut<'a>(
    artifacts: &'a mut BTreeMap<String, omegon_opsx::ParsedDesignArtifact>,
    id: &str,
) -> anyhow::Result<&'a mut omegon_opsx::ParsedDesignArtifact> {
    artifacts
        .get_mut(id)
        .ok_or_else(|| anyhow::anyhow!("design node '{id}' not found"))
}

fn require_rewrite_safe(parsed: &omegon_opsx::ParsedDesignArtifact) -> anyhow::Result<()> {
    if parsed.rewrite_safety == RewriteSafety::BlockedByUnknownContent {
        anyhow::bail!(
            "design artifact rewrite blocked by unknown content: {}",
            parsed.source_path.display()
        );
    }
    Ok(())
}

impl From<DesignIssueTypeV1> for IssueType {
    fn from(value: DesignIssueTypeV1) -> Self {
        match value {
            DesignIssueTypeV1::Epic => Self::Epic,
            DesignIssueTypeV1::Feature => Self::Feature,
            DesignIssueTypeV1::Task => Self::Task,
            DesignIssueTypeV1::Bug => Self::Bug,
            DesignIssueTypeV1::Chore => Self::Chore,
        }
    }
}

impl From<DesignIssueTypeV1> for omegon_opsx::IssueType {
    fn from(value: DesignIssueTypeV1) -> Self {
        match value {
            DesignIssueTypeV1::Epic => Self::Epic,
            DesignIssueTypeV1::Feature => Self::Feature,
            DesignIssueTypeV1::Task => Self::Task,
            DesignIssueTypeV1::Bug => Self::Bug,
            DesignIssueTypeV1::Chore => Self::Chore,
        }
    }
}

fn staged_file(repo: &Path, path: &Path, post_bytes: Vec<u8>) -> anyhow::Result<JournalFile> {
    validate_no_follow_path(repo, path)?;
    let relative = relative_path(repo, path)?;
    Ok(JournalFile {
        path: relative,
        pre_identity: file_identity(path)?,
        post_identity: identity(&post_bytes),
        post_bytes,
    })
}

fn journal_checksum(journal: &DesignJournal) -> anyhow::Result<String> {
    let mut value = journal.clone();
    value.checksum.clear();
    Ok(identity(&serde_json::to_vec(&value)?))
}

fn receipt_checksum(receipt: &DurableDesignReceipt) -> anyhow::Result<String> {
    let mut value = receipt.clone();
    value.checksum.clear();
    Ok(identity(&serde_json::to_vec(&value)?))
}

fn repository_id(repo: &Path) -> anyhow::Result<String> {
    Ok(identity(repo.canonicalize()?.to_string_lossy().as_bytes()))
}

fn identity(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn file_identity(path: &Path) -> anyhow::Result<Option<String>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(identity(&bytes))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn validate_operation_id(id: &str) -> anyhow::Result<()> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, b'-' | b'_' | b'.'))
    {
        anyhow::bail!("invalid lifecycle operation id");
    }
    Ok(())
}

fn validate_entity_id(id: &str) -> anyhow::Result<()> {
    if id.is_empty()
        || !id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, b'-' | b'_'))
    {
        anyhow::bail!("invalid design node id");
    }
    Ok(())
}

fn relative_path(repo: &Path, path: &Path) -> anyhow::Result<String> {
    if path.components().any(|component| {
        !matches!(
            component,
            Component::Normal(_) | Component::RootDir | Component::Prefix(_)
        )
    }) {
        anyhow::bail!("non-normal transaction path");
    }
    let relative = path
        .strip_prefix(repo)
        .map_err(|_| anyhow::anyhow!("transaction path escapes repository"))?;
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

fn contained_path(repo: &Path, relative: &str) -> anyhow::Result<PathBuf> {
    let relative = Path::new(relative);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        anyhow::bail!("invalid repository-relative transaction path");
    }
    let path = repo.join(relative);
    validate_no_follow_path(repo, &path)?;
    Ok(path)
}

fn validate_mutation_paths(repo: &Path, roots: &RepositoryRoots) -> anyhow::Result<()> {
    for path in [&roots.design, &roots.openspec, &roots.ledger] {
        validate_no_follow_path(repo, path)?;
    }
    Ok(())
}

fn validate_no_follow_path(repo: &Path, path: &Path) -> anyhow::Result<()> {
    let relative = path
        .strip_prefix(repo)
        .map_err(|_| anyhow::anyhow!("mutation path escapes repository: {}", path.display()))?;
    let mut current = repo.to_path_buf();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            anyhow::bail!("mutation path is not normalized: {}", path.display());
        };
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                anyhow::bail!(
                    "mutation path contains a symbolic link: {}",
                    current.display()
                )
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn atomic_durable_write(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("transaction target has no parent"))?;
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    let persisted = temporary.persist(path).map_err(|error| error.error)?;
    persisted.sync_all()?;
    sync_parent(parent)
}

fn remove_durable(path: &Path) -> anyhow::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => sync_parent(path.parent().unwrap_or(Path::new("."))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(unix)]
fn sync_parent(path: &Path) -> anyhow::Result<()> {
    File::open(path)?.sync_all().map_err(Into::into)
}

#[cfg(not(unix))]
fn sync_parent(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

fn transaction_root(repo: &Path) -> PathBuf {
    repo.join("ai/lifecycle/transactions/repository-v1")
}

fn check_cancellation(is_cancelled: &impl Fn() -> bool) -> anyhow::Result<()> {
    if is_cancelled() {
        return Err(TransactionError::new(
            TransactionErrorCode::Cancelled,
            "lifecycle request cancelled",
        )
        .into());
    }
    Ok(())
}
fn pending_path(repo: &Path, operation_id: &str) -> PathBuf {
    transaction_root(repo)
        .join("pending")
        .join(format!("{}.json", operation_record_name(operation_id)))
}
fn receipt_path(repo: &Path, operation_id: &str) -> PathBuf {
    transaction_root(repo)
        .join("receipts")
        .join(format!("{}.json", operation_record_name(operation_id)))
}

pub(super) fn operation_record_name(operation_id: &str) -> String {
    identity(operation_id.as_bytes())
}

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|existing| existing == value) {
        values.push(value.into());
    }
}

trait MutationNodeId {
    fn node_id(&self) -> &str;
}
impl MutationNodeId for DesignMutationV1 {
    fn node_id(&self) -> &str {
        match self {
            Self::Create { id, .. }
            | Self::SetState { id, .. }
            | Self::AddQuestion { id, .. }
            | Self::RemoveQuestion { id, .. }
            | Self::AddResearch { id, .. }
            | Self::AddDecision { id, .. }
            | Self::AddDependency { id, .. }
            | Self::RemoveDependency { id, .. }
            | Self::AddRelated { id, .. }
            | Self::RemoveRelated { id, .. }
            | Self::AddImplementationNotes { id, .. }
            | Self::SetPriority { id, .. }
            | Self::SetIssueType { id, .. }
            | Self::ImplementOpenSpec { id } => id,
            Self::BranchQuestion { parent_id, .. } => parent_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn revision() -> LifecycleRepositoryRevisionV1 {
        LifecycleRepositoryRevisionV1 {
            version: 1,
            design_root: "ai/docs".into(),
            openspec_root: "ai/openspec".into(),
            ledger_path: "ai/lifecycle/state.json".into(),
            ledger_identity: "absent".into(),
            ledger_revision: 1,
            artifact_digest: "artifact".into(),
            transaction_digest: "transactions".into(),
        }
    }

    fn journal_for(repo: &Path, mut files: Vec<JournalFile>) -> DesignJournal {
        files.push(
            staged_file(repo, &repo.join("ai/lifecycle/state.json"), b"{}".to_vec()).unwrap(),
        );
        let mut journal = DesignJournal {
            version: JOURNAL_VERSION,
            domain: DESIGN_DOMAIN.into(),
            repository_id: repository_id(repo).unwrap(),
            operation_id: "recover-operation".into(),
            semantic_fingerprint: "fingerprint".into(),
            files,
            effects: vec!["recovered".into()],
            checksum: String::new(),
        };
        journal.checksum = journal_checksum(&journal).unwrap();
        journal
    }

    #[test]
    fn lifecycle_service_design_partial_transaction_rolls_forward_to_exact_post_state() {
        let dir = tempfile::tempdir().unwrap();
        let roots = RepositoryRoots {
            design: dir.path().join("ai/docs"),
            openspec: dir.path().join("ai/openspec"),
            ledger: dir.path().join("ai/lifecycle/state.json"),
        };
        fs::create_dir_all(&roots.design).unwrap();
        let first = roots.design.join("first.md");
        let second = roots.design.join("second.md");
        fs::write(&first, b"first-pre").unwrap();
        fs::write(&second, b"second-pre").unwrap();
        let files = vec![
            staged_file(dir.path(), &first, b"first-post".to_vec()).unwrap(),
            staged_file(dir.path(), &second, b"second-post".to_vec()).unwrap(),
        ];
        let journal = journal_for(dir.path(), files);
        atomic_durable_write(
            &pending_path(dir.path(), &journal.operation_id),
            &serde_json::to_vec_pretty(&journal).unwrap(),
        )
        .unwrap();
        fs::write(&first, b"first-post").unwrap();

        assert!(recover_pending(dir.path(), &roots, || Ok(revision())).is_empty());
        assert_eq!(fs::read(first).unwrap(), b"first-post");
        assert_eq!(fs::read(second).unwrap(), b"second-post");
        assert!(receipt_path(dir.path(), "recover-operation").is_file());
        assert!(!pending_path(dir.path(), "recover-operation").exists());
    }

    #[test]
    fn lifecycle_service_design_corrupt_and_path_tampered_journals_fail_closed() {
        for tamper_path in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let roots = RepositoryRoots {
                design: dir.path().join("ai/docs"),
                openspec: dir.path().join("ai/openspec"),
                ledger: dir.path().join("ai/lifecycle/state.json"),
            };
            fs::create_dir_all(&roots.design).unwrap();
            let target = roots.design.join("node.md");
            fs::write(&target, b"pre").unwrap();
            let mut journal = journal_for(
                dir.path(),
                vec![staged_file(dir.path(), &target, b"post".to_vec()).unwrap()],
            );
            if tamper_path {
                journal.files[0].path = "../outside.md".into();
                journal.checksum = journal_checksum(&journal).unwrap();
            } else {
                journal.checksum = "corrupt".into();
            }
            atomic_durable_write(
                &pending_path(dir.path(), &journal.operation_id),
                &serde_json::to_vec_pretty(&journal).unwrap(),
            )
            .unwrap();

            let blockers = recover_pending(dir.path(), &roots, || Ok(revision()));
            assert_eq!(blockers.len(), 1);
            assert_eq!(fs::read(&target).unwrap(), b"pre");
            assert!(
                transaction_root(dir.path())
                    .join("quarantine")
                    .join(format!(
                        "{}.json",
                        operation_record_name("recover-operation")
                    ))
                    .is_file()
            );
            let restart_blockers = recover_pending(dir.path(), &roots, || Ok(revision()));
            assert_eq!(restart_blockers.len(), 1);
            assert!(restart_blockers[0].contains("quarantined repository transaction"));
            assert!(!dir.path().parent().unwrap().join("outside.md").exists());
        }
    }

    #[test]
    fn lifecycle_service_design_create_does_not_replace_unparseable_existing_target() {
        let dir = tempfile::tempdir().unwrap();
        let roots = RepositoryRoots {
            design: dir.path().join("ai/docs"),
            openspec: dir.path().join("ai/openspec"),
            ledger: dir.path().join("ai/lifecycle/state.json"),
        };
        fs::create_dir_all(&roots.design).unwrap();
        let target = roots.design.join("existing.md");
        fs::write(&target, "operator-owned malformed content").unwrap();
        let mutation = DesignMutationV1::Create {
            id: "existing".into(),
            title: "Existing".into(),
            parent: None,
            status: None,
            tags: Vec::new(),
            overview: String::new(),
        };

        let error = stage_mutation(
            dir.path(),
            &roots,
            &mutation,
            &LifecycleState::default(),
            &|| false,
        )
        .unwrap_err();

        assert!(error.to_string().contains("target already exists"));
        assert_eq!(
            fs::read_to_string(target).unwrap(),
            "operator-owned malformed content"
        );
    }

    #[test]
    fn lifecycle_service_design_settlement_preflights_all_resources_before_writing() {
        let dir = tempfile::tempdir().unwrap();
        let roots = RepositoryRoots {
            design: dir.path().join("ai/docs"),
            openspec: dir.path().join("ai/openspec"),
            ledger: dir.path().join("ai/lifecycle/state.json"),
        };
        fs::create_dir_all(&roots.design).unwrap();
        let first = roots.design.join("first.md");
        let second = roots.design.join("second.md");
        fs::write(&first, b"first-pre").unwrap();
        fs::write(&second, b"second-pre").unwrap();
        let journal = journal_for(
            dir.path(),
            vec![
                staged_file(dir.path(), &first, b"first-post".to_vec()).unwrap(),
                staged_file(dir.path(), &second, b"second-post".to_vec()).unwrap(),
            ],
        );
        atomic_durable_write(
            &pending_path(dir.path(), &journal.operation_id),
            &serde_json::to_vec_pretty(&journal).unwrap(),
        )
        .unwrap();
        fs::write(&second, b"external-conflict").unwrap();

        let blockers = recover_pending(dir.path(), &roots, || Ok(revision()));

        assert_eq!(blockers.len(), 1);
        assert_eq!(fs::read(first).unwrap(), b"first-pre");
        assert_eq!(fs::read(second).unwrap(), b"external-conflict");
    }

    #[test]
    fn lifecycle_service_design_revalidates_full_revision_before_journal_durability() {
        let dir = tempfile::tempdir().unwrap();
        let roots = RepositoryRoots {
            design: dir.path().join("ai/docs"),
            openspec: dir.path().join("ai/openspec"),
            ledger: dir.path().join("ai/lifecycle/state.json"),
        };
        let expected = LifecycleRepositoryRevisionV1 {
            ledger_revision: 0,
            ..revision()
        };
        let mutation = DesignMutationV1::Create {
            id: "raced".into(),
            title: "Raced".into(),
            parent: None,
            status: None,
            tags: Vec::new(),
            overview: String::new(),
        };
        let mut observed = expected.clone();
        observed.artifact_digest = "external-edit".into();
        let ledger_store = omegon_opsx::JsonFileStore::from_path(&roots.ledger);
        let ledger_transaction = ledger_store.lock_transaction().unwrap();

        let error = stage_and_commit(
            CommitContext {
                repo: dir.path(),
                roots: &roots,
                operation_id: "race-operation",
                semantic_fingerprint: "fingerprint",
                pre_revision: &expected,
                ledger: &ledger_transaction,
            },
            &mutation,
            &|| false,
            || Ok(observed.clone()),
        )
        .unwrap_err();

        assert!(error.to_string().contains("after mutation staging"));
        assert!(!roots.design.join("raced.md").exists());
        assert!(!pending_path(dir.path(), "race-operation").exists());
        assert!(!roots.ledger.exists());
    }

    #[test]
    fn lifecycle_service_design_operation_record_names_are_hashed_and_case_distinct() {
        let upper = operation_record_name("Operation-ID");
        let lower = operation_record_name("operation-id");
        assert_eq!(upper.len(), 64);
        assert!(
            upper
                .bytes()
                .all(|value| value.is_ascii_hexdigit() && !value.is_ascii_uppercase())
        );
        assert_ne!(upper, lower);
        assert!(
            !pending_path(Path::new("repo"), "CON")
                .to_string_lossy()
                .contains("CON.json")
        );
    }
}
