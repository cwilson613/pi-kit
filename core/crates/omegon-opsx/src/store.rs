//! State store — trait abstraction with JSON file implementation.
//!
//! Omegon uses JsonFileStore (git-native, diffable).
//! Omega would use a SledStore (ACID, fleet-scale).

use crate::error::OpsxError;
use crate::types::*;
use fs2::FileExt;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Current schema version. Bump when LifecycleState shape changes.
pub const SCHEMA_VERSION: u32 = 1;

/// The full lifecycle state — all nodes, changes, milestones, and audit log.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct LifecycleState {
    /// Schema version for forward-compatible deserialization.
    #[serde(default = "default_version")]
    pub version: u32,
    /// Monotonic compare-and-swap revision for whole-state persistence.
    #[serde(default)]
    pub revision: u64,
    pub nodes: Vec<DesignNode>,
    pub changes: Vec<Change>,
    pub milestones: Vec<Milestone>,
    /// Append-only audit log of all state transitions.
    #[serde(default)]
    pub audit_log: Vec<AuditEntry>,
}

fn default_version() -> u32 {
    1
}

/// Trait for state persistence. Implementations determine storage backend.
pub trait StateStore: Send + Sync {
    /// Load the full lifecycle state.
    fn load(&self) -> Result<LifecycleState, OpsxError>;

    /// Save only if the durable state still has `expected_revision`.
    fn save(&self, state: &LifecycleState, expected_revision: u64) -> Result<(), OpsxError>;
}

/// JSON file store — writes to `ai/lifecycle/state.json` (or legacy `.omegon/lifecycle/`).
/// The file is versioned by jj/git. The VCS operation log IS the transaction log.
#[derive(Clone)]
pub struct JsonFileStore {
    path: PathBuf,
}

impl JsonFileStore {
    pub fn new(project_root: &Path) -> Self {
        // Primary: ai/lifecycle/state.json
        // Fallback: .omegon/lifecycle/state.json (pre-ai convention)
        let ai_dir = project_root.join("ai").join("lifecycle");
        let legacy_dir = project_root.join(".omegon").join("lifecycle");
        let path = if ai_dir.join("state.json").exists() {
            ai_dir.join("state.json")
        } else if legacy_dir.join("state.json").exists() {
            // Legacy exists but ai/ doesn't — use legacy to avoid data loss
            legacy_dir.join("state.json")
        } else {
            // New project — write to ai/lifecycle/
            ai_dir.join("state.json")
        };
        Self { path }
    }

    /// Construct a store for an authority path already selected by the caller.
    pub fn from_path(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Hold the same exclusive lock used by compatibility `save` calls across
    /// a larger repository transaction.
    pub fn lock_transaction(&self) -> Result<JsonFileStoreTransaction, OpsxError> {
        Ok(JsonFileStoreTransaction {
            path: self.path.clone(),
            _guard: lock_exclusive(&self.path)?,
        })
    }
}

/// Exclusive selected-ledger transaction. Callers must acquire any broader
/// repository lock before this lock.
pub struct JsonFileStoreTransaction {
    path: PathBuf,
    _guard: FileLockGuard,
}

impl JsonFileStoreTransaction {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<LifecycleState, OpsxError> {
        load_state(&self.path)
    }

    pub fn save(&self, state: &LifecycleState, expected_revision: u64) -> Result<(), OpsxError> {
        validate_next_revision(state, expected_revision)?;
        let actual = load_state(&self.path)?.revision;
        if actual != expected_revision {
            return Err(OpsxError::RevisionConflict {
                expected: expected_revision,
                actual,
            });
        }
        replace_state(&self.path, state)
    }
}

impl StateStore for JsonFileStore {
    fn load(&self) -> Result<LifecycleState, OpsxError> {
        load_state(&self.path)
    }

    fn save(&self, state: &LifecycleState, expected_revision: u64) -> Result<(), OpsxError> {
        validate_next_revision(state, expected_revision)?;
        let _guard = lock_exclusive(&self.path)?;
        let actual = load_state(&self.path)?.revision;
        if actual != expected_revision {
            return Err(OpsxError::RevisionConflict {
                expected: expected_revision,
                actual,
            });
        }
        replace_state(&self.path, state)
    }
}

/// In-memory store — never persists. Used as a fallback when the filesystem
/// is unavailable (e.g. read-only directory, corrupted state).
#[derive(Default)]
pub struct MemoryStore {
    state: std::sync::Mutex<LifecycleState>,
}

impl StateStore for MemoryStore {
    fn load(&self) -> Result<LifecycleState, OpsxError> {
        Ok(self.state.lock().unwrap().clone())
    }

    fn save(&self, state: &LifecycleState, expected_revision: u64) -> Result<(), OpsxError> {
        validate_next_revision(state, expected_revision)?;
        let mut current = self.state.lock().unwrap();
        if current.revision != expected_revision {
            return Err(OpsxError::RevisionConflict {
                expected: expected_revision,
                actual: current.revision,
            });
        }
        *current = state.clone();
        Ok(())
    }
}

fn validate_next_revision(state: &LifecycleState, expected_revision: u64) -> Result<(), OpsxError> {
    let Some(next_revision) = expected_revision.checked_add(1) else {
        return Err(OpsxError::StoreError(
            "lifecycle state revision overflow".to_string(),
        ));
    };
    if state.revision != next_revision {
        return Err(OpsxError::StoreError(format!(
            "non-monotonic lifecycle revision: expected next revision after {expected_revision}, got {}",
            state.revision
        )));
    }
    Ok(())
}

fn load_state(path: &Path) -> Result<LifecycleState, OpsxError> {
    if !path.exists() {
        return Ok(LifecycleState {
            version: SCHEMA_VERSION,
            ..Default::default()
        });
    }
    let content = std::fs::read_to_string(path)
        .map_err(|error| OpsxError::StoreError(format!("read {}: {error}", path.display())))?;
    let state: LifecycleState = serde_json::from_str(&content)
        .map_err(|error| OpsxError::StoreError(format!("parse {}: {error}", path.display())))?;
    if state.version > SCHEMA_VERSION {
        return Err(OpsxError::SchemaMismatch {
            expected: SCHEMA_VERSION,
            got: state.version,
        });
    }
    Ok(state)
}

fn replace_state(path: &Path, state: &LifecycleState) -> Result<(), OpsxError> {
    let parent = path
        .parent()
        .ok_or_else(|| OpsxError::StoreError(format!("path has no parent: {}", path.display())))?;
    std::fs::create_dir_all(parent)
        .map_err(|error| OpsxError::StoreError(format!("mkdir {}: {error}", parent.display())))?;
    let json = serde_json::to_vec_pretty(state)
        .map_err(|error| OpsxError::StoreError(format!("serialize: {error}")))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|error| {
        OpsxError::StoreError(format!("create temp in {}: {error}", parent.display()))
    })?;
    temporary
        .write_all(&json)
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| OpsxError::StoreError(format!("write temporary state: {error}")))?;
    let persisted = temporary.persist(path).map_err(|error| {
        OpsxError::StoreError(format!("replace {}: {}", path.display(), error.error))
    })?;
    persisted
        .sync_all()
        .map_err(|error| OpsxError::StoreError(format!("sync {}: {error}", path.display())))?;
    sync_parent(parent)
}

#[cfg(unix)]
fn sync_parent(parent: &Path) -> Result<(), OpsxError> {
    std::fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| OpsxError::StoreError(format!("sync {}: {error}", parent.display())))
}

#[cfg(not(unix))]
fn sync_parent(_parent: &Path) -> Result<(), OpsxError> {
    Ok(())
}

struct FileLockGuard(std::fs::File);

impl Drop for FileLockGuard {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}

fn lock_exclusive(path: &Path) -> Result<FileLockGuard, OpsxError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            OpsxError::StoreError(format!("mkdir {}: {error}", parent.display()))
        })?;
    }
    let mut lock_path = path.as_os_str().to_os_string();
    lock_path.push(".lock");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(PathBuf::from(&lock_path))
        .map_err(|error| OpsxError::StoreError(format!("lock open {}: {error}", path.display())))?;
    file.lock_exclusive()
        .map_err(|error| OpsxError::StoreError(format!("lock {}: {error}", path.display())))?;
    Ok(FileLockGuard(file))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn json_store_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let store = JsonFileStore::new(tmp.path());

        let mut state = LifecycleState {
            version: SCHEMA_VERSION,
            revision: 1,
            ..Default::default()
        };
        state.nodes.push(DesignNode {
            id: "test-node".into(),
            title: "Test node".into(),
            state: NodeState::Seed,
            parent: None,
            tags: vec!["v0.15.0".into()],
            priority: Some(Priority::new(2)),
            issue_type: None,
            open_questions: vec![],
            decisions: vec![],
            overview: "A test node".into(),
            bound_change: None,
            created_at: "2026-03-23".into(),
            updated_at: "2026-03-23".into(),
        });

        store.save(&state, 0).unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded.version, SCHEMA_VERSION);
        assert_eq!(loaded.revision, 1);
        assert_eq!(loaded.nodes.len(), 1);
        assert_eq!(loaded.nodes[0].id, "test-node");
        assert_eq!(loaded.nodes[0].state, NodeState::Seed);
    }

    #[test]
    fn empty_store_returns_default_with_version() {
        let tmp = TempDir::new().unwrap();
        let store = JsonFileStore::new(tmp.path());
        let state = store.load().unwrap();
        assert_eq!(state.version, SCHEMA_VERSION);
        assert!(state.nodes.is_empty());
    }

    #[test]
    fn explicit_path_does_not_reselect_another_authority() {
        let tmp = TempDir::new().unwrap();
        let selected = tmp.path().join(".omegon/lifecycle/state.json");
        let store = JsonFileStore::from_path(&selected);
        let state = LifecycleState {
            version: SCHEMA_VERSION,
            revision: 1,
            ..Default::default()
        };

        store.save(&state, 0).unwrap();
        std::fs::create_dir_all(tmp.path().join("ai/lifecycle")).unwrap();
        std::fs::write(
            tmp.path().join("ai/lifecycle/state.json"),
            r#"{"version":1,"revision":9,"nodes":[],"changes":[],"milestones":[]}"#,
        )
        .unwrap();

        assert_eq!(store.path(), selected);
        assert_eq!(store.load().unwrap().revision, 1);
    }

    #[test]
    fn atomic_write_leaves_no_tmp_file() {
        let tmp = TempDir::new().unwrap();
        let store = JsonFileStore::new(tmp.path());
        let state = LifecycleState {
            version: SCHEMA_VERSION,
            revision: 1,
            ..Default::default()
        };
        store.save(&state, 0).unwrap();

        let tmp_path = store.path().with_extension("json.tmp");
        assert!(!tmp_path.exists(), "temp file should be renamed away");
        assert!(store.path().exists(), "final file should exist");
    }

    #[test]
    fn rejects_future_schema_version() {
        let tmp = TempDir::new().unwrap();
        let store = JsonFileStore::new(tmp.path());
        let state = LifecycleState {
            version: 999,
            ..Default::default()
        };
        // Write directly (bypassing version check on save)
        let dir = store.path().parent().unwrap();
        std::fs::create_dir_all(dir).unwrap();
        let json = serde_json::to_string_pretty(&state).unwrap();
        std::fs::write(store.path(), json).unwrap();

        let err = store.load();
        assert!(err.is_err());
        match err.unwrap_err() {
            OpsxError::SchemaMismatch { expected, got } => {
                assert_eq!(expected, SCHEMA_VERSION);
                assert_eq!(got, 999);
            }
            other => panic!("expected SchemaMismatch, got {other:?}"),
        }
    }

    #[test]
    fn legacy_state_without_revision_defaults_to_zero() {
        let tmp = TempDir::new().unwrap();
        let store = JsonFileStore::new(tmp.path());
        let dir = store.path().parent().unwrap();
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(
            store.path(),
            r#"{"version":1,"nodes":[],"changes":[],"milestones":[],"audit_log":[]}"#,
        )
        .unwrap();

        assert_eq!(store.load().unwrap().revision, 0);
    }

    #[test]
    fn store_rejects_non_monotonic_revision() {
        let tmp = TempDir::new().unwrap();
        let store = JsonFileStore::new(tmp.path());

        let error = store
            .save(
                &LifecycleState {
                    version: SCHEMA_VERSION,
                    revision: 2,
                    ..Default::default()
                },
                0,
            )
            .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("non-monotonic lifecycle revision")
        );
        assert!(!store.path().exists());
    }

    #[test]
    fn transaction_lock_blocks_compatibility_writer() {
        let tmp = TempDir::new().unwrap();
        let store = JsonFileStore::new(tmp.path());
        let transaction = store.lock_transaction().unwrap();
        let path = store.path().to_path_buf();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let writer = std::thread::spawn(move || {
            started_tx.send(()).unwrap();
            let state = LifecycleState {
                version: SCHEMA_VERSION,
                revision: 1,
                ..Default::default()
            };
            JsonFileStore::from_path(path).save(&state, 0).unwrap();
            done_tx.send(()).unwrap();
        });
        started_rx.recv().unwrap();
        assert!(
            done_rx
                .recv_timeout(std::time::Duration::from_millis(100))
                .is_err()
        );
        drop(transaction);
        done_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        writer.join().unwrap();
    }
}
