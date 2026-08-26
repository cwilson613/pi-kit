//! Read-only memory/federation status projection.
//!
//! Durable-memory state comes from the latest managed-service snapshot. This
//! projection never probes the live store or its synchronization files.

use std::path::{Path, PathBuf};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryFederationObservation {
    pub cwd: PathBuf,
    pub git: Option<GitSummary>,
    pub lifecycle_signals: Vec<String>,
    pub federation_signals: Vec<String>,
    pub memory_authority: MemoryAuthority,
    pub memory_index: MemoryIndexState,
}

impl MemoryFederationStatusProjection {
    pub fn git_root_or_cwd(&self) -> &Path {
        self.git
            .as_ref()
            .map(|summary| summary.root.as_path())
            .unwrap_or(self.cwd.as_path())
    }
}

pub fn project_memory_federation_status(
    observation: MemoryFederationObservation,
) -> MemoryFederationStatusProjection {
    let MemoryFederationObservation {
        cwd,
        git,
        lifecycle_signals,
        federation_signals,
        memory_authority,
        memory_index,
    } = observation;
    let mut signals = Vec::new();

    if git.is_some() {
        signals.push("git".to_string());
    }

    signals.extend(lifecycle_signals.iter().cloned());
    signals.extend(federation_signals.iter().cloned());
    if !matches!(memory_authority, MemoryAuthority::None) {
        signals.push("memory:managed".to_string());
    }
    let mode = if !federation_signals.is_empty() {
        CoordinationMode::Federation
    } else if !lifecycle_signals.is_empty() {
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
    fn observation(
        git: bool,
        lifecycle: bool,
        authority: MemoryAuthority,
    ) -> MemoryFederationObservation {
        MemoryFederationObservation {
            cwd: PathBuf::from("/workspace"),
            git: git.then(|| GitSummary {
                root: PathBuf::from("/workspace"),
                branch: Some("main".into()),
                dirty: false,
            }),
            lifecycle_signals: lifecycle.then(|| "AGENTS.md".into()).into_iter().collect(),
            federation_signals: Vec::new(),
            memory_authority: authority,
            memory_index: MemoryIndexState::Missing,
        }
    }

    #[test]
    fn non_git_directory_is_one_off_without_memory_authority() {
        let projection =
            project_memory_federation_status(observation(false, false, MemoryAuthority::None));

        assert_eq!(projection.mode, CoordinationMode::OneOff);
        assert_eq!(projection.memory_authority, MemoryAuthority::None);
        assert!(projection.recommended_behavior.contains("local/session"));
    }

    #[test]
    fn git_repo_without_lifecycle_signals_is_ordinary_git() {
        let projection =
            project_memory_federation_status(observation(true, false, MemoryAuthority::None));

        assert_eq!(projection.mode, CoordinationMode::OrdinaryGit);
        assert!(projection.signals.contains(&"git".to_string()));
    }

    #[test]
    fn owner_observation_controls_live_memory_authority() {
        let projection =
            project_memory_federation_status(observation(true, true, MemoryAuthority::None));

        assert_eq!(projection.mode, CoordinationMode::LifecycleProject);
        assert_eq!(projection.memory_authority, MemoryAuthority::None);
        assert_eq!(projection.memory_index, MemoryIndexState::Missing);
        assert!(!projection.signals.contains(&"memory:git-jsonl".into()));
    }

    #[test]
    fn managed_observation_projects_local_index_state() {
        let projection = project_memory_federation_status(observation(
            true,
            false,
            MemoryAuthority::LocalIndexOnly,
        ));

        assert_eq!(projection.memory_authority, MemoryAuthority::LocalIndexOnly);
        assert_eq!(projection.memory_index, MemoryIndexState::Missing);
    }
}
