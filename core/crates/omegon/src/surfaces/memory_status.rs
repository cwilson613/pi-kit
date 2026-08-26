//! Read-only memory/federation status projection.
//!
//! Durable-memory state comes from the latest managed-service snapshot. This
//! projection never probes the live store or its synchronization files.

use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinationMode {
    OneOff,
    OrdinaryGit,
    LifecycleProject,
    Federation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryAuthority {
    GitJsonl { paths: Vec<PathBuf> },
    LocalIndexOnly,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryIndexState {
    Fresh,
    Stale,
    Missing,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitSummary {
    pub root: PathBuf,
    pub branch: Option<String>,
    pub dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryFederationStatusProjection {
    pub cwd: PathBuf,
    pub mode: CoordinationMode,
    pub signals: Vec<String>,
    pub git: Option<GitSummary>,
    pub memory_authority: MemoryAuthority,
    pub memory_index: MemoryIndexState,
    pub recommended_behavior: String,
}

impl MemoryFederationStatusProjection {
    pub fn git_root_or_cwd(&self) -> &Path {
        self.git
            .as_ref()
            .map(|summary| summary.root.as_path())
            .unwrap_or(self.cwd.as_path())
    }
}

pub fn project_memory_federation_status(cwd: impl AsRef<Path>) -> MemoryFederationStatusProjection {
    let cwd = cwd.as_ref().to_path_buf();
    let git = git_summary(&cwd);
    let root = git
        .as_ref()
        .map(|summary| summary.root.as_path())
        .unwrap_or(cwd.as_path());
    let mut signals = Vec::new();

    if git.is_some() {
        signals.push("git".to_string());
    }

    let lifecycle = lifecycle_signals(root);
    signals.extend(lifecycle.iter().cloned());

    let federation = federation_signals(root);
    signals.extend(federation.iter().cloned());

    let managed = crate::status::managed_memory_status_snapshot_for(root);
    let memory_authority = match managed.authority {
        crate::memory_service::ManagedMemoryAuthorityV1::GitJsonl { paths } => {
            signals.push("memory:managed".to_string());
            MemoryAuthority::GitJsonl { paths }
        }
        crate::memory_service::ManagedMemoryAuthorityV1::LocalIndexOnly => {
            signals.push("memory:managed".to_string());
            MemoryAuthority::LocalIndexOnly
        }
        crate::memory_service::ManagedMemoryAuthorityV1::None => MemoryAuthority::None,
    };
    let memory_index = match managed.index_state {
        crate::memory_service::ManagedMemoryIndexStateV1::Fresh => MemoryIndexState::Fresh,
        crate::memory_service::ManagedMemoryIndexStateV1::Stale => MemoryIndexState::Stale,
        crate::memory_service::ManagedMemoryIndexStateV1::Missing => MemoryIndexState::Missing,
        crate::memory_service::ManagedMemoryIndexStateV1::Unknown => MemoryIndexState::Unknown,
    };
    let mode = if !federation.is_empty() {
        CoordinationMode::Federation
    } else if !lifecycle.is_empty() {
        CoordinationMode::LifecycleProject
    } else if git.is_some() {
        CoordinationMode::OrdinaryGit
    } else {
        CoordinationMode::OneOff
    };

    let recommended_behavior = recommendation(mode, &memory_authority, memory_index).to_string();

    MemoryFederationStatusProjection {
        cwd,
        mode,
        signals,
        git,
        memory_authority,
        memory_index,
        recommended_behavior,
    }
}

fn git_summary(cwd: &Path) -> Option<GitSummary> {
    let root = git_output(cwd, &["rev-parse", "--show-toplevel"])?;
    let root = PathBuf::from(root);
    let branch = git_output(cwd, &["branch", "--show-current"]).filter(|value| !value.is_empty());
    let dirty = git_output(cwd, &["status", "--porcelain"])
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    Some(GitSummary {
        root,
        branch,
        dirty,
    })
}

fn git_output(cwd: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn lifecycle_signals(root: &Path) -> Vec<String> {
    let mut signals = Vec::new();
    if root.join("AGENTS.md").exists() {
        signals.push("AGENTS.md".to_string());
    }
    if root.join("openspec").is_dir() {
        signals.push("openspec".to_string());
    }
    if root.join("CHANGELOG.md").exists() {
        signals.push("CHANGELOG.md".to_string());
    }
    if root.join("docs").is_dir() {
        signals.push("docs".to_string());
    }
    signals
}

fn federation_signals(root: &Path) -> Vec<String> {
    let mut signals = Vec::new();
    if let Some(worktree_list) = git_output(root, &["worktree", "list", "--porcelain"]) {
        let count = worktree_list
            .lines()
            .filter(|line| line.starts_with("worktree "))
            .count();
        if count > 1 {
            signals.push(format!("git-worktrees:{count}"));
        }
    }
    signals
}

fn recommendation(
    mode: CoordinationMode,
    authority: &MemoryAuthority,
    index: MemoryIndexState,
) -> &'static str {
    match (mode, authority, index) {
        (CoordinationMode::OneOff, MemoryAuthority::None, _) => {
            "No Git-tracked memory authority detected; treat memory as local/session scoped."
        }
        (_, MemoryAuthority::GitJsonl { .. }, MemoryIndexState::Stale) => {
            "Git-tracked JSONL facts are authoritative; rebuild the local memory index, then use normal Git fetch/merge/rebase for checkout continuity."
        }
        (_, MemoryAuthority::GitJsonl { .. }, _) => {
            "Git-tracked JSONL facts are authoritative; use normal Git fetch/merge/rebase for checkout continuity."
        }
        (_, MemoryAuthority::LocalIndexOnly, _) => {
            "Only a local memory index was detected; do not treat it as cross-checkout coordination state."
        }
        _ => "No project memory facts detected; no memory synchronization action is applicable.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn git(cwd: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .status()
            .expect("run git");
        assert!(status.success(), "git {args:?} failed");
    }

    fn init_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        git(dir.path(), &["init"]);
        git(dir.path(), &["config", "user.email", "test@example.com"]);
        git(dir.path(), &["config", "user.name", "Test"]);
        dir
    }

    #[test]
    fn non_git_directory_is_one_off_without_memory_authority() {
        let dir = tempfile::tempdir().expect("tempdir");
        let projection = project_memory_federation_status(dir.path());

        assert_eq!(projection.mode, CoordinationMode::OneOff);
        assert_eq!(projection.memory_authority, MemoryAuthority::None);
        assert!(projection.recommended_behavior.contains("local/session"));
    }

    #[test]
    fn git_repo_without_lifecycle_signals_is_ordinary_git() {
        let dir = init_repo();
        let projection = project_memory_federation_status(dir.path());

        assert_eq!(projection.mode, CoordinationMode::OrdinaryGit);
        assert!(projection.signals.contains(&"git".to_string()));
    }

    #[test]
    fn tracked_jsonl_is_not_probed_as_live_memory_authority() {
        let dir = init_repo();
        fs::create_dir_all(dir.path().join("ai/memory")).expect("memory dir");
        fs::write(
            dir.path().join("ai/memory/facts.jsonl"),
            "{\"id\":\"fact-1\"}\n",
        )
        .expect("facts");
        fs::write(dir.path().join("AGENTS.md"), "# Agent rules\n").expect("agents");
        git(dir.path(), &["add", "ai/memory/facts.jsonl", "AGENTS.md"]);
        git(dir.path(), &["commit", "-m", "seed"]);

        let projection = project_memory_federation_status(dir.path());

        assert_eq!(projection.mode, CoordinationMode::LifecycleProject);
        assert_eq!(projection.memory_authority, MemoryAuthority::None);
        assert_eq!(projection.memory_index, MemoryIndexState::Missing);
        assert!(!projection.signals.contains(&"memory:git-jsonl".into()));
    }

    #[test]
    fn local_index_file_is_not_probed_as_live_memory_state() {
        let dir = init_repo();
        fs::create_dir_all(dir.path().join(".omegon/memory")).expect("memory dir");
        fs::write(dir.path().join(".omegon/memory/facts.db"), "index").expect("index");

        let projection = project_memory_federation_status(dir.path());

        assert_eq!(projection.memory_authority, MemoryAuthority::None);
        assert_eq!(projection.memory_index, MemoryIndexState::Missing);
    }
}
