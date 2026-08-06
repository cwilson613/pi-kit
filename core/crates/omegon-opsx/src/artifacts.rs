//! Git-native OpenSpec artifact repository.
//!
//! This module owns proposal lifecycle metadata and renderer-neutral change
//! evidence discovery. Higher layers may parse spec/scenario content, but they
//! must not reinterpret canonical state or rewrite proposal frontmatter.

use std::fs;
use std::path::{Path, PathBuf};

use crate::content::parse_task_stable_id_marker;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskCheckboxStatus {
    Pending,
    Done,
}

impl TaskCheckboxStatus {
    fn marker(self) -> &'static str {
        match self {
            Self::Pending => "[ ]",
            Self::Done => "[x]",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskWriteReport {
    pub path: PathBuf,
    pub line: usize,
    pub change: String,
    pub group: String,
    pub task_id: String,
    pub previous_done: bool,
    pub new_done: bool,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskStableIdValidationReport {
    pub path: PathBuf,
    pub findings: Vec<TaskStableIdFinding>,
}

impl TaskStableIdValidationReport {
    pub fn is_ok(&self) -> bool {
        self.findings.is_empty()
    }
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct TaskStableIdFinding {
    pub line: usize,
    pub task_id: String,
    pub stable_id: String,
    pub message: String,
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

    pub fn add_spec(
        &self,
        change_name: &str,
        domain: &str,
        content: &str,
    ) -> Result<PathBuf, OpsxError> {
        let change_dir = self.active_change_dir(change_name);
        if !change_dir.is_dir() {
            return Err(OpsxError::StoreError(format!(
                "Change '{change_name}' does not exist"
            )));
        }
        let relative = safe_spec_relative_path(domain)?;
        let path = change_dir.join("specs").join(relative);
        atomic_write(&path, content.as_bytes())?;
        self.write_active_state(change_name, ChangeState::Specced)?;
        Ok(path)
    }

    pub fn validate_task_stable_ids(
        &self,
        change_name: &str,
    ) -> Result<TaskStableIdValidationReport, OpsxError> {
        let path = self.active_change_dir(change_name).join("tasks.md");
        validate_task_stable_ids_at(&path)
    }

    pub fn set_task_checkbox_status(
        &self,
        change_name: &str,
        group_title: &str,
        task_id: &str,
        status: TaskCheckboxStatus,
    ) -> Result<TaskWriteReport, OpsxError> {
        let path = self.active_change_dir(change_name).join("tasks.md");
        set_task_checkbox_status_at(&path, change_name, group_title, task_id, status)
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

fn safe_spec_relative_path(domain: &str) -> Result<PathBuf, OpsxError> {
    let path = Path::new(domain);
    if domain.is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(OpsxError::StoreError(format!(
            "invalid OpenSpec domain path: {domain}"
        )));
    }
    let mut relative = path.to_path_buf();
    relative.set_extension("md");
    Ok(relative)
}

fn validate_task_stable_ids_at(path: &Path) -> Result<TaskStableIdValidationReport, OpsxError> {
    let content = fs::read_to_string(path).map_err(store_error)?;
    let mut seen = std::collections::BTreeMap::<String, (usize, String)>::new();
    let mut findings = Vec::new();
    for (index, line) in content.lines().enumerate() {
        let Some((_, task_id, description, _, _)) = parse_task_line_for_write(line) else {
            continue;
        };
        let Some(stable_id) = parse_task_stable_id_marker(&description) else {
            continue;
        };
        if !stable_id.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | ':' | '-')
        }) {
            findings.push(TaskStableIdFinding {
                line: index + 1,
                task_id: task_id.clone(),
                stable_id: stable_id.clone(),
                message:
                    "task-id marker must contain only ASCII letters, digits, '.', '_', ':' or '-'"
                        .into(),
            });
        }
        if let Some((first_line, first_task_id)) =
            seen.insert(stable_id.clone(), (index + 1, task_id.clone()))
        {
            findings.push(TaskStableIdFinding {
                line: index + 1,
                task_id,
                stable_id,
                message: format!(
                    "duplicate task-id marker also used by task {first_task_id} on line {first_line}"
                ),
            });
        }
    }
    Ok(TaskStableIdValidationReport {
        path: path.into(),
        findings,
    })
}

fn set_task_checkbox_status_at(
    path: &Path,
    change_name: &str,
    group_title: &str,
    task_id: &str,
    status: TaskCheckboxStatus,
) -> Result<TaskWriteReport, OpsxError> {
    let content = fs::read_to_string(path).map_err(store_error)?;
    let newline = if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut lines = content
        .split_inclusive('\n')
        .map(|line| line.trim_end_matches(['\r', '\n']).to_string())
        .collect::<Vec<_>>();
    if content.ends_with(newline) && lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    let mut current_group = None;
    let mut group_matches = 0;
    let mut task_matches = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if let Some(title) = markdown_heading_title(line) {
            current_group = Some(title.to_string());
            if title == group_title {
                group_matches += 1;
            }
            continue;
        }
        if current_group.as_deref() == Some(group_title)
            && let Some((done, id, description, start, end)) = parse_task_line_for_write(line)
            && id == task_id
        {
            task_matches.push((index, done, description, start, end));
        }
    }
    if group_matches != 1 {
        let qualifier = if group_matches == 0 {
            "not found"
        } else {
            "ambiguous"
        };
        return Err(OpsxError::StoreError(format!(
            "OpenSpec task group '{group_title}' is {qualifier} in change '{change_name}'"
        )));
    }
    if task_matches.len() != 1 {
        let qualifier = if task_matches.is_empty() {
            "not found"
        } else {
            "ambiguous"
        };
        return Err(OpsxError::StoreError(format!(
            "OpenSpec task id '{task_id}' is {qualifier} in group '{group_title}'"
        )));
    }
    let (index, previous_done, description, start, end) = task_matches.remove(0);
    let new_done = status == TaskCheckboxStatus::Done;
    lines[index].replace_range(start..end, status.marker());
    atomic_write(path, (lines.join(newline) + newline).as_bytes())?;
    Ok(TaskWriteReport {
        path: path.into(),
        line: index + 1,
        change: change_name.into(),
        group: group_title.into(),
        task_id: task_id.into(),
        previous_done,
        new_done,
        description,
    })
}

fn markdown_heading_title(line: &str) -> Option<&str> {
    let rest = line.trim_start().strip_prefix("##")?;
    (!rest.starts_with('#')).then(|| rest.trim())
}

fn parse_task_line_for_write(line: &str) -> Option<(bool, String, String, usize, usize)> {
    let start = line
        .find("[ ")
        .or_else(|| line.find("[x]"))
        .or_else(|| line.find("[X]"))?;
    let marker = line.get(start..start + 3)?;
    let after = line.get(start + 3..)?.trim_start();
    let (id, description) = after.split_once(' ')?;
    if !id.contains('.')
        || !id
            .chars()
            .all(|character| character.is_ascii_digit() || character == '.')
    {
        return None;
    }
    Some((
        matches!(marker, "[x]" | "[X]"),
        id.into(),
        description.trim().into(),
        start,
        start + 3,
    ))
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
    fn adds_nested_spec_and_rejects_traversal() {
        let temp = tempfile::tempdir().unwrap();
        let change = temp.path().join("openspec/changes/demo");
        fs::create_dir_all(&change).unwrap();
        fs::write(
            change.join("proposal.md"),
            "---\nstate: proposed\n---\n# Demo\n",
        )
        .unwrap();
        let repo = OpenSpecRepository::new(temp.path());
        let path = repo.add_spec("demo", "auth/tokens", "# Tokens\n").unwrap();
        assert_eq!(path, change.join("specs/auth/tokens.md"));
        assert_eq!(fs::read_to_string(path).unwrap(), "# Tokens\n");
        assert_eq!(
            repo.read_active("demo").unwrap().state,
            ChangeState::Specced
        );
        assert!(repo.add_spec("demo", "../escape", "bad").is_err());
    }

    #[test]
    fn validates_and_updates_tasks_without_losing_crlf() {
        let temp = tempfile::tempdir().unwrap();
        let change = temp.path().join("openspec/changes/demo");
        fs::create_dir_all(&change).unwrap();
        fs::write(
            change.join("tasks.md"),
            "## Work\r\n- [ ] 1.1 First <!-- task-id: duplicate -->\r\n- [ ] 1.2 Second <!-- task-id: duplicate -->\r\n",
        )
        .unwrap();
        let repo = OpenSpecRepository::new(temp.path());
        let validation = repo.validate_task_stable_ids("demo").unwrap();
        assert_eq!(validation.findings.len(), 1);
        let report = repo
            .set_task_checkbox_status("demo", "Work", "1.1", TaskCheckboxStatus::Done)
            .unwrap();
        assert!(!report.previous_done);
        assert!(report.new_done);
        let content = fs::read_to_string(change.join("tasks.md")).unwrap();
        assert!(content.contains("- [x] 1.1 First"));
        assert!(content.contains("\r\n"));
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
