//! Git-native OpenSpec artifact repository.
//!
//! This module owns proposal lifecycle metadata and renderer-neutral change
//! evidence discovery. Higher layers may parse spec/scenario content, but they
//! must not reinterpret canonical state or rewrite proposal frontmatter.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::OpsxError;
use crate::{ArtifactHealth, ChangeArtifactEvidence, ChangeState, parse_declared_change_state};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeArtifactRecord {
    pub name: String,
    pub path: PathBuf,
    pub state: ChangeState,
    pub health: ArtifactHealth,
    pub evidence: ChangeArtifactEvidence,
}

#[derive(Debug, Clone)]
pub struct OpenSpecRepository {
    repo_root: PathBuf,
}

impl OpenSpecRepository {
    pub fn new(repo_root: impl Into<PathBuf>) -> Self {
        Self {
            repo_root: repo_root.into(),
        }
    }

    pub fn openspec_dir(&self) -> PathBuf {
        self.repo_root.join("openspec")
    }

    pub fn active_change_dir(&self, name: &str) -> PathBuf {
        self.openspec_dir().join("changes").join(name)
    }

    pub fn archived_change_dir(&self, name: &str) -> PathBuf {
        self.openspec_dir().join("archive").join(name)
    }

    pub fn discover_active(&self) -> Vec<ChangeArtifactRecord> {
        self.discover_under(&self.openspec_dir().join("changes"), false)
    }

    pub fn discover_archived(&self) -> Vec<ChangeArtifactRecord> {
        self.discover_under(&self.openspec_dir().join("archive"), true)
    }

    pub fn read_active(&self, name: &str) -> Option<ChangeArtifactRecord> {
        let path = self.active_change_dir(name);
        path.is_dir()
            .then(|| self.inspect_change_dir(&path, name, false))
    }

    pub fn write_active_state(&self, name: &str, state: ChangeState) -> Result<(), OpsxError> {
        self.write_state_at(&self.active_change_dir(name).join("proposal.md"), state)
    }

    pub fn transition_active_with<F>(
        &self,
        name: &str,
        target: ChangeState,
        transition_ledger: F,
    ) -> Result<(), OpsxError>
    where
        F: FnOnce() -> Result<(), OpsxError>,
    {
        let proposal = self.active_change_dir(name).join("proposal.md");
        let previous = fs::read(&proposal).map_err(store_error)?;
        self.write_state_at(&proposal, target)?;
        if let Err(error) = transition_ledger() {
            return match atomic_write(&proposal, &previous) {
                Ok(()) => Err(error),
                Err(rollback) => Err(OpsxError::StoreError(format!(
                    "lifecycle ledger transition failed ({error}); artifact rollback also failed ({rollback})"
                ))),
            };
        }
        Ok(())
    }

    fn discover_under(&self, root: &Path, archived: bool) -> Vec<ChangeArtifactRecord> {
        let mut records = fs::read_dir(root)
            .ok()
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                let name = path.file_name()?.to_str()?.to_string();
                path.is_dir()
                    .then(|| self.inspect_change_dir(&path, &name, archived))
            })
            .collect::<Vec<_>>();
        records.sort_by(|left, right| left.name.cmp(&right.name));
        records
    }

    pub fn inspect_change_dir(
        &self,
        path: &Path,
        name: &str,
        archived: bool,
    ) -> ChangeArtifactRecord {
        let proposal = path.join("proposal.md");
        let specs = path.join("specs");
        let tasks = path.join("tasks.md");
        let (total_tasks, done_tasks) = task_counts(&tasks);
        let evidence = ChangeArtifactEvidence {
            has_proposal: proposal.is_file(),
            has_design: path.join("design.md").is_file(),
            has_specs: contains_markdown(&specs),
            has_tasks: tasks.is_file(),
            total_tasks,
            done_tasks,
            has_registered_tests: false,
        };
        let (declared, metadata_error) = read_declared_state(&proposal);
        let state = if archived {
            ChangeState::Archived
        } else {
            evidence.derive_state(declared)
        };
        let health = metadata_error.map_or_else(
            || evidence.assess_health(state),
            |detail| ArtifactHealth::Malformed { detail },
        );
        ChangeArtifactRecord {
            name: name.into(),
            path: path.into(),
            state,
            health,
            evidence,
        }
    }

    fn write_state_at(&self, proposal: &Path, state: ChangeState) -> Result<(), OpsxError> {
        let content = fs::read_to_string(proposal).map_err(store_error)?;
        let updated = render_state_metadata(&content, state, proposal)?;
        if updated != content {
            atomic_write(proposal, updated.as_bytes())?;
        }
        Ok(())
    }
}

fn read_declared_state(path: &Path) -> (Option<ChangeState>, Option<String>) {
    let Ok(content) = fs::read_to_string(path) else {
        return (None, None);
    };
    let Some(frontmatter) = content
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n").map(|(head, _)| head))
    else {
        return (None, None);
    };
    let Some(value) = frontmatter.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        (key.trim() == "state").then(|| value.trim().trim_matches(['\'', '"']))
    }) else {
        return (None, None);
    };
    match parse_declared_change_state(value) {
        Ok(state) => (Some(state), None),
        Err(error) => (None, Some(error)),
    }
}

fn render_state_metadata(
    content: &str,
    state: ChangeState,
    path: &Path,
) -> Result<String, OpsxError> {
    let state_line = format!("state: {}", state.as_str());
    if let Some(rest) = content.strip_prefix("---\n") {
        let Some((frontmatter, body)) = rest.split_once("\n---\n") else {
            return Err(OpsxError::StoreError(format!(
                "malformed proposal frontmatter: {}",
                path.display()
            )));
        };
        let mut found = false;
        let mut lines = frontmatter
            .lines()
            .map(|line| {
                if line
                    .split_once(':')
                    .is_some_and(|(key, _)| key.trim() == "state")
                {
                    found = true;
                    state_line.clone()
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>();
        if !found {
            lines.push(state_line);
        }
        Ok(format!("---\n{}\n---\n{body}", lines.join("\n")))
    } else {
        Ok(format!("---\n{state_line}\n---\n{content}"))
    }
}

fn contains_markdown(path: &Path) -> bool {
    path.is_dir()
        && fs::read_dir(path).ok().is_some_and(|entries| {
            entries
                .flatten()
                .any(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("md"))
        })
}

fn task_counts(path: &Path) -> (usize, usize) {
    let Ok(content) = fs::read_to_string(path) else {
        return (0, 0);
    };
    content.lines().fold((0, 0), |(total, done), line| {
        let trimmed = line.trim_start();
        if trimmed.starts_with("- [ ]") {
            (total + 1, done)
        } else if trimmed.starts_with("- [x]") || trimmed.starts_with("- [X]") {
            (total + 1, done + 1)
        } else {
            (total, done)
        }
    })
}

fn atomic_write(path: &Path, content: &[u8]) -> Result<(), OpsxError> {
    let parent = path
        .parent()
        .ok_or_else(|| OpsxError::StoreError(format!("path has no parent: {}", path.display())))?;
    fs::create_dir_all(parent).map_err(store_error)?;
    let temporary = parent.join(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("artifact"),
        std::process::id()
    ));
    fs::write(&temporary, content).map_err(store_error)?;
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(store_error(error));
    }
    Ok(())
}

fn store_error(error: std::io::Error) -> OpsxError {
    OpsxError::StoreError(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_declared_state_and_task_evidence() {
        let temp = tempfile::tempdir().unwrap();
        let change = temp.path().join("openspec/changes/demo");
        fs::create_dir_all(change.join("specs")).unwrap();
        fs::write(
            change.join("proposal.md"),
            "---\nstate: testing\n---\n# Demo\n",
        )
        .unwrap();
        fs::write(change.join("specs/core.md"), "# Core\n").unwrap();
        fs::write(change.join("tasks.md"), "- [x] one\n- [ ] two\n").unwrap();
        let record = OpenSpecRepository::new(temp.path())
            .read_active("demo")
            .unwrap();
        assert_eq!(record.state, ChangeState::Testing);
        assert_eq!(
            (record.evidence.total_tasks, record.evidence.done_tasks),
            (2, 1)
        );
    }

    #[test]
    fn failed_ledger_transition_restores_exact_proposal_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let change = temp.path().join("openspec/changes/demo");
        fs::create_dir_all(&change).unwrap();
        let original = b"# Legacy proposal\r\n";
        fs::write(change.join("proposal.md"), original).unwrap();
        let repo = OpenSpecRepository::new(temp.path());
        let error = repo
            .transition_active_with("demo", ChangeState::Specced, || {
                Err(OpsxError::StoreError("nope".into()))
            })
            .unwrap_err();
        assert!(error.to_string().contains("nope"));
        assert_eq!(fs::read(change.join("proposal.md")).unwrap(), original);
    }
}
