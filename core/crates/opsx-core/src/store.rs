//! State store — trait abstraction with JSON file implementation.
//!
//! Omegon uses JsonFileStore (git-native, diffable).
//! Omega would use a SledStore (ACID, fleet-scale).

use std::path::{Path, PathBuf};
use crate::types::*;
use crate::error::OpsxError;

/// Current schema version. Bump when LifecycleState shape changes.
pub const SCHEMA_VERSION: u32 = 1;

/// The full lifecycle state — all nodes, changes, milestones, and audit log.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct LifecycleState {
    /// Schema version for forward-compatible deserialization.
    #[serde(default = "default_version")]
    pub version: u32,
    pub nodes: Vec<DesignNode>,
    pub changes: Vec<Change>,
    pub milestones: Vec<Milestone>,
    /// Append-only audit log of all state transitions.
    #[serde(default)]
    pub audit_log: Vec<AuditEntry>,
}

fn default_version() -> u32 { 1 }

/// Trait for state persistence. Implementations determine storage backend.
pub trait StateStore: Send + Sync {
    /// Load the full lifecycle state.
    fn load(&self) -> Result<LifecycleState, OpsxError>;

    /// Save the full lifecycle state.
    fn save(&self, state: &LifecycleState) -> Result<(), OpsxError>;
}

/// JSON file store — writes to `.omegon/lifecycle/state.json`.
/// The file is versioned by jj/git. The VCS operation log IS the transaction log.
pub struct JsonFileStore {
    path: PathBuf,
}

impl JsonFileStore {
    pub fn new(project_root: &Path) -> Self {
        Self {
            path: project_root.join(".omegon").join("lifecycle").join("state.json"),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl StateStore for JsonFileStore {
    fn load(&self) -> Result<LifecycleState, OpsxError> {
        if !self.path.exists() {
            return Ok(LifecycleState {
                version: SCHEMA_VERSION,
                ..Default::default()
            });
        }
        let content = std::fs::read_to_string(&self.path)
            .map_err(|e| OpsxError::StoreError(format!("read {}: {e}", self.path.display())))?;
        let state: LifecycleState = serde_json::from_str(&content)
            .map_err(|e| OpsxError::StoreError(format!("parse {}: {e}", self.path.display())))?;

        // Schema version check — refuse to load future versions
        if state.version > SCHEMA_VERSION {
            return Err(OpsxError::SchemaMismatch {
                expected: SCHEMA_VERSION,
                got: state.version,
            });
        }
        // TODO: migrate older versions forward when SCHEMA_VERSION > 1

        Ok(state)
    }

    fn save(&self, state: &LifecycleState) -> Result<(), OpsxError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| OpsxError::StoreError(format!("mkdir {}: {e}", parent.display())))?;
        }
        let json = serde_json::to_string_pretty(state)
            .map_err(|e| OpsxError::StoreError(format!("serialize: {e}")))?;

        // Atomic write: write to temp file, then rename.
        // rename() is atomic on POSIX — no partial writes on crash.
        let tmp_path = self.path.with_extension("json.tmp");
        std::fs::write(&tmp_path, &json)
            .map_err(|e| OpsxError::StoreError(format!("write {}: {e}", tmp_path.display())))?;
        std::fs::rename(&tmp_path, &self.path)
            .map_err(|e| OpsxError::StoreError(format!("rename {}: {e}", self.path.display())))?;

        Ok(())
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

    fn save(&self, state: &LifecycleState) -> Result<(), OpsxError> {
        *self.state.lock().unwrap() = state.clone();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn json_store_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let store = JsonFileStore::new(tmp.path());

        let mut state = LifecycleState { version: SCHEMA_VERSION, ..Default::default() };
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

        store.save(&state).unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded.version, SCHEMA_VERSION);
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
    fn atomic_write_leaves_no_tmp_file() {
        let tmp = TempDir::new().unwrap();
        let store = JsonFileStore::new(tmp.path());
        let state = LifecycleState { version: SCHEMA_VERSION, ..Default::default() };
        store.save(&state).unwrap();

        let tmp_path = store.path().with_extension("json.tmp");
        assert!(!tmp_path.exists(), "temp file should be renamed away");
        assert!(store.path().exists(), "final file should exist");
    }

    #[test]
    fn rejects_future_schema_version() {
        let tmp = TempDir::new().unwrap();
        let store = JsonFileStore::new(tmp.path());
        let mut state = LifecycleState::default();
        state.version = 999;
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
}
