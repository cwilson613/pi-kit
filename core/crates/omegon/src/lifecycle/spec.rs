//! OpenSpec read-only parser — spec content, scenarios, change listing.
//!
//! Parses openspec/ directories to extract change info, spec files,
//! and Given/When/Then scenarios. No mutation support (Phase 1b).

use crate::filelock::atomic_write;
use std::fs;
use std::path::{Path, PathBuf};

use super::types::*;
use crate::evidence::EvidenceStore;
use crate::tdd::{self, EvidenceQuery};
use omegon_opsx::{ChangeArtifactRecord, ChangeState, OpenSpecRepository};

/// Locate the openspec/ directory in a repository.
pub fn find_openspec_dir(repo_path: &Path) -> Option<PathBuf> {
    crate::paths::openspec_dir(repo_path)
}

/// List all active OpenSpec changes (in openspec/changes/).
pub fn list_changes(repo_path: &Path) -> Vec<ChangeInfo> {
    let Some(openspec_dir) = find_openspec_dir(repo_path) else {
        return vec![];
    };
    let changes_dir = openspec_dir.join("changes");
    if !changes_dir.is_dir() {
        return vec![];
    }

    let mut changes = Vec::new();
    let entries = match fs::read_dir(&changes_dir) {
        Ok(e) => e,
        Err(_) => return changes,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        if let Some(info) = read_change(&path, &name) {
            changes.push(info);
        }
    }

    changes.sort_by(|a, b| a.name.cmp(&b.name));
    changes
}

/// List archived OpenSpec changes (in openspec/archive/).
pub fn list_archived_changes(repo_path: &Path) -> Vec<ChangeInfo> {
    let Some(openspec_dir) = find_openspec_dir(repo_path) else {
        return vec![];
    };
    let archive_dir = openspec_dir.join("archive");
    if !archive_dir.is_dir() {
        return vec![];
    }

    let mut changes = Vec::new();
    let entries = match fs::read_dir(&archive_dir) {
        Ok(e) => e,
        Err(_) => return changes,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        if let Some(mut info) = read_change(&path, &name) {
            info.state = ChangeState::Archived;
            changes.push(info);
        }
    }

    changes.sort_by(|a, b| a.name.cmp(&b.name));
    changes
}

/// Read a single change directory into a ChangeInfo.
pub fn get_change(repo_path: &Path, name: &str) -> Option<ChangeInfo> {
    let openspec_dir = find_openspec_dir(repo_path)?;
    let change_dir = openspec_dir.join("changes").join(name);
    if !change_dir.is_dir() {
        return None;
    }
    read_change(&change_dir, name)
}

fn read_change(change_dir: &Path, name: &str) -> Option<ChangeInfo> {
    let artifact = OpenSpecRepository::new(
        change_dir
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .unwrap_or(change_dir),
    )
    .inspect_change_dir(change_dir, name, false);
    read_change_from_artifact(change_dir, artifact)
}

fn read_change_from_artifact(
    change_dir: &Path,
    artifact: ChangeArtifactRecord,
) -> Option<ChangeInfo> {
    let tasks_path = change_dir.join("tasks.md");
    let task_groups = if artifact.evidence.has_tasks {
        parse_task_groups(&tasks_path)
    } else {
        Vec::new()
    };
    let specs = if artifact.evidence.has_specs {
        parse_specs_dir(&change_dir.join("specs"))
    } else {
        vec![]
    };

    Some(ChangeInfo {
        name: artifact.name,
        path: artifact.path,
        state: artifact.state,
        artifact_health: artifact.health,
        has_proposal: artifact.evidence.has_proposal,
        has_design: artifact.evidence.has_design,
        has_specs: artifact.evidence.has_specs,
        has_tasks: artifact.evidence.has_tasks,
        total_tasks: artifact.evidence.total_tasks,
        done_tasks: artifact.evidence.done_tasks,
        task_groups,
        specs,
    })
    .map(|mut change| {
        annotate_tdd_evidence(change_dir, &mut change);
        annotate_claim_evidence(change_dir, &mut change);
        change
    })
}

fn scenario_evidence_ids(
    domain: &str,
    _requirement: &str,
    scenario: &ScenarioProjection,
) -> [String; 2] {
    [
        scenario.id.clone(),
        format!("{}/{}", domain, scenario.title),
    ]
}

fn stable_scenario_id(domain: &str, requirement: &str, scenario: &str) -> String {
    format!(
        "{}/{}/{}",
        slug_component(domain),
        slug_component(requirement),
        slug_component(scenario)
    )
}

fn slug_component(input: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in input.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

fn evidence_claim_ids(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| evidence_claim_id_from_line(line.trim()))
        .collect()
}

fn evidence_claim_id_from_line(trimmed: &str) -> Option<String> {
    if let Some(body) = trimmed
        .strip_prefix("<!--")
        .and_then(|body| body.strip_suffix("-->"))
        .map(str::trim)
    {
        let claim = body.strip_prefix("evidence-claim:")?.trim();
        return (!claim.is_empty()).then(|| claim.to_string());
    }
    let claim = trimmed.strip_prefix("evidence-claim:")?.trim();
    (!claim.is_empty()).then(|| claim.to_string())
}

fn annotate_claim_evidence(change_dir: &Path, change: &mut ChangeInfo) {
    let Some(repo_path) = change_dir
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
    else {
        return;
    };
    let Ok(store) = EvidenceStore::load(repo_path) else {
        return;
    };
    for spec in &mut change.specs {
        for requirement in &mut spec.requirements {
            let requirement_claims = evidence_claim_ids(&requirement.description);
            for scenario in &mut requirement.scenarios {
                let mut claims = requirement_claims.clone();
                claims.extend(evidence_claim_ids(&scenario.given));
                claims.extend(evidence_claim_ids(&scenario.when));
                claims.extend(evidence_claim_ids(&scenario.then));
                for and_clause in &scenario.and_clauses {
                    claims.extend(evidence_claim_ids(and_clause));
                }
                claims.sort();
                claims.dedup();
                scenario.evidence_support = claims
                    .iter()
                    .map(|claim_id| {
                        let summary = store.support_summary(claim_id);
                        ClaimEvidenceSupport {
                            claim_id: claim_id.clone(),
                            status: summary.status,
                            supports: summary.supports.len(),
                            refutes: summary.refutes.len(),
                            stale: summary.stale.len(),
                            supersedes: summary.supersedes.len(),
                        }
                    })
                    .collect();
                scenario.evidence_claims = claims;
            }
        }
    }
}

fn annotate_tdd_evidence(change_dir: &Path, change: &mut ChangeInfo) {
    let Some(repo_path) = change_dir
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
    else {
        return;
    };
    for spec in &mut change.specs {
        let domain = spec.domain.clone();
        for requirement in &mut spec.requirements {
            let requirement_title = requirement.title.clone();
            for scenario in &mut requirement.scenarios {
                let ids = scenario_evidence_ids(&domain, &requirement_title, scenario);
                let status = ids
                    .iter()
                    .find_map(|id| {
                        tdd::evidence_status(
                            repo_path,
                            &EvidenceQuery {
                                change: Some(change.name.clone()),
                                scenario: Some(id.clone()),
                                ..EvidenceQuery::default()
                            },
                        )
                        .ok()
                        .filter(|status| *status != tdd::TddEvidenceStatus::NoEvidence)
                    })
                    .unwrap_or(tdd::TddEvidenceStatus::NoEvidence);
                scenario.tdd_evidence = Some(status);
            }
        }
    }
}

/// Count tasks in a tasks.md file.
/// Tasks are lines matching `- [x]` (done) or `- [ ]` (pending).
fn count_tasks(path: &Path) -> (usize, usize) {
    let task_groups = parse_task_groups(path);
    let total = task_groups.iter().map(|group| group.tasks.len()).sum();
    let done = task_groups
        .iter()
        .flat_map(|group| &group.tasks)
        .filter(|task| task.done)
        .count();
    (total, done)
}

/// Parse OpenSpec tasks.md into groups and checkbox task lines.
pub fn parse_task_groups(path: &Path) -> Vec<TaskGroup> {
    omegon_opsx::parse_task_groups(path)
        .into_iter()
        .map(|group| TaskGroup {
            title: group.title,
            specs: group.specs,
            tasks: group
                .tasks
                .into_iter()
                .map(|task| TaskLine {
                    id: task.id,
                    stable_id: task.stable_id,
                    description: task.description,
                    done: task.done,
                })
                .collect(),
        })
        .collect()
}

fn parse_task_stable_id_marker(description: &str) -> Option<String> {
    omegon_opsx::parse_task_stable_id_marker(description)
}

/// Parse all spec files in a specs/ directory.
pub fn parse_specs_dir(specs_dir: &Path) -> Vec<SpecFileProjection> {
    omegon_opsx::parse_specs_dir(specs_dir)
        .into_iter()
        .map(project_spec_file)
        .collect()
}

fn project_spec_file(content: omegon_opsx::SpecFile) -> SpecFileProjection {
    let requirements = content
        .requirements
        .iter()
        .cloned()
        .map(project_requirement)
        .collect();
    SpecFileProjection {
        content,
        requirements,
    }
}

fn project_requirement(content: omegon_opsx::Requirement) -> RequirementProjection {
    let scenarios = content
        .scenarios
        .iter()
        .cloned()
        .map(|content| ScenarioProjection {
            content,
            tdd_evidence: None,
            evidence_claims: vec![],
            evidence_support: vec![],
        })
        .collect();
    RequirementProjection { content, scenarios }
}

/// Parse spec content into canonical requirements and project application evidence.
pub fn parse_spec_content(content: &str) -> Vec<RequirementProjection> {
    parse_spec_content_with_domain("", content)
}

pub fn parse_spec_content_with_domain(domain: &str, content: &str) -> Vec<RequirementProjection> {
    omegon_opsx::parse_spec_content_with_domain(domain, content)
        .into_iter()
        .map(project_requirement)
        .collect()
}

/// Build a context injection string for relevant OpenSpec changes.
pub fn build_context_injection(changes: &[ChangeInfo]) -> String {
    if changes.is_empty() {
        return String::new();
    }

    let mut lines = Vec::new();
    lines.push("[OpenSpec — active changes]".to_string());

    for change in changes {
        let icon = match change.state {
            ChangeState::Proposed => "◌",
            ChangeState::Specced => "◐",
            ChangeState::Planned => "▸",
            ChangeState::Testing => "⊢",
            ChangeState::Implementing => "⟳",
            ChangeState::Verifying => "◉",
            ChangeState::Archived => "✓",
            ChangeState::Abandoned => "×",
        };
        let progress = if change.total_tasks > 0 {
            format!(" ({}/{})", change.done_tasks, change.total_tasks)
        } else {
            String::new()
        };
        lines.push(format!(
            "  {icon} {} — {}{progress}",
            change.name,
            change.state.as_str()
        ));

        // Include scenario summaries for implementing/verifying changes
        if matches!(
            change.state,
            ChangeState::Implementing | ChangeState::Verifying
        ) {
            for spec in &change.specs {
                let scenario_count: usize =
                    spec.requirements.iter().map(|r| r.scenarios.len()).sum();
                if scenario_count > 0 {
                    let mut evidence_counts = std::collections::BTreeMap::new();
                    for scenario in spec
                        .requirements
                        .iter()
                        .flat_map(|requirement| &requirement.scenarios)
                    {
                        if let Some(status) = scenario.tdd_evidence {
                            *evidence_counts.entry(status.as_str()).or_insert(0usize) += 1;
                        }
                    }
                    let evidence = if evidence_counts.is_empty() {
                        String::new()
                    } else {
                        format!(
                            " [{}]",
                            evidence_counts
                                .iter()
                                .map(|(status, count)| format!("{status}:{count}"))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    };
                    lines.push(format!(
                        "    specs/{}: {} scenarios{}",
                        spec.domain, scenario_count, evidence
                    ));
                }
            }
        }
    }

    lines.join("\n")
}

/// Count total scenarios across all specs in a change.
pub fn count_scenarios(change: &ChangeInfo) -> usize {
    change
        .specs
        .iter()
        .flat_map(|s| &s.requirements)
        .map(|r| r.scenarios.len())
        .sum()
}

// ═══════════════════════════════════════════════════════════════════════════
// Mutation functions — create and modify OpenSpec changes
// ═══════════════════════════════════════════════════════════════════════════

/// Scaffold a new OpenSpec change directory with a proposal.
pub fn propose_change(
    repo_path: &Path,
    name: &str,
    title: &str,
    intent: &str,
) -> anyhow::Result<ChangeInfo> {
    let openspec_dir = repo_path.join("openspec");
    let changes_dir = openspec_dir.join("changes");
    let change_dir = changes_dir.join(name);

    if change_dir.exists() {
        anyhow::bail!("Change '{name}' already exists");
    }

    fs::create_dir_all(&change_dir)?;

    // Write proposal.md
    let proposal = format!(
        "---\nstate: proposed\n---\n\n# {title}\n\n## Intent\n\n{intent}\n\n## Scope\n\n_TBD_\n\n## Constraints\n\n_None identified yet._\n"
    );
    atomic_write(&change_dir.join("proposal.md"), proposal.as_bytes())?;

    Ok(ChangeInfo {
        name: name.to_string(),
        path: change_dir,
        state: ChangeState::Proposed,
        artifact_health: omegon_opsx::ArtifactHealth::Healthy,
        has_proposal: true,
        has_design: false,
        has_specs: false,
        has_tasks: false,
        total_tasks: 0,
        done_tasks: 0,
        task_groups: vec![],
        specs: vec![],
    })
}

pub fn write_change_state(repo_path: &Path, name: &str, state: ChangeState) -> anyhow::Result<()> {
    OpenSpecRepository::new(repo_path)
        .write_active_state(name, state)
        .map_err(Into::into)
}

#[cfg(test)]
fn add_spec(
    repo_path: &Path,
    change_name: &str,
    domain: &str,
    content: &str,
) -> anyhow::Result<PathBuf> {
    OpenSpecRepository::new(repo_path)
        .add_spec(change_name, domain, content)
        .map_err(Into::into)
}

pub use omegon_opsx::{TaskCheckboxStatus, TaskStableIdFinding, TaskWriteReport};
pub fn set_task_checkbox_status(
    repo_path: &Path,
    change_name: &str,
    group_title: &str,
    task_id: &str,
    status: TaskCheckboxStatus,
) -> anyhow::Result<TaskWriteReport> {
    OpenSpecRepository::new(repo_path)
        .set_task_checkbox_status(change_name, group_title, task_id, status)
        .map_err(Into::into)
}

/// Policy decision for OpenSpec evidence claim gates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceGateDecision {
    Pass,
    Warn,
    Block,
}

/// A single evidence-claim gate finding for a scenario.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceGateFinding {
    pub scenario_id: String,
    pub claim_id: String,
    pub status: crate::evidence::ClaimSupportStatus,
    pub decision: EvidenceGateDecision,
    pub detail: String,
}

/// Evaluate provider-neutral evidence claims attached to OpenSpec scenarios.
///
/// Default policy is intentionally conservative:
/// - refuted or mixed claims block archiving;
/// - unknown or unsupported claims warn;
/// - supported claims pass.
///
/// This keeps evidence opt-in by requiring an explicit `evidence-claim` marker,
/// but prevents knowingly refuted evidence from being archived silently.
pub fn evaluate_evidence_gates(change: &ChangeInfo) -> Vec<EvidenceGateFinding> {
    evaluate_spec_evidence_gates(&change.specs)
}

pub fn evaluate_spec_evidence_gates(
    specs: &[crate::lifecycle::types::SpecFileProjection],
) -> Vec<EvidenceGateFinding> {
    let mut findings = Vec::new();
    for spec in specs {
        for requirement in &spec.requirements {
            for scenario in &requirement.scenarios {
                for support in &scenario.evidence_support {
                    let decision = match support.status {
                        crate::evidence::ClaimSupportStatus::Supported => {
                            EvidenceGateDecision::Pass
                        }
                        crate::evidence::ClaimSupportStatus::Refuted
                        | crate::evidence::ClaimSupportStatus::Mixed => EvidenceGateDecision::Block,
                        crate::evidence::ClaimSupportStatus::Unsupported
                        | crate::evidence::ClaimSupportStatus::Unknown => {
                            EvidenceGateDecision::Warn
                        }
                    };
                    if decision == EvidenceGateDecision::Pass {
                        continue;
                    }
                    findings.push(EvidenceGateFinding {
                        scenario_id: scenario.id.clone(),
                        claim_id: support.claim_id.clone(),
                        status: support.status,
                        decision,
                        detail: format!(
                            "claim {} is {:?} for scenario {} (supports={}, refutes={}, stale={}, supersedes={})",
                            support.claim_id,
                            support.status,
                            scenario.id,
                            support.supports,
                            support.refutes,
                            support.stale,
                            support.supersedes
                        ),
                    });
                }
            }
        }
    }
    findings
}

/// Archive a change by moving it to openspec/archive/.
pub fn archive_change(repo_path: &Path, change_name: &str) -> anyhow::Result<()> {
    let change_dir = repo_path.join("openspec/changes").join(change_name);

    if !change_dir.exists() {
        anyhow::bail!("Change '{change_name}' does not exist");
    }

    let archive_dir = repo_path.join("openspec/archive");
    fs::create_dir_all(&archive_dir)?;
    let dest = archive_dir.join(change_name);

    fs::rename(&change_dir, &dest)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_spec_content_accepts_explicit_scenario_id() {
        let content = r#"# auth

### Requirement: Token Validation

#### Scenario: Expired token rejected
<!-- id: auth/token-expired -->
Given expired token
When request happens
Then response is 401
"#;
        let reqs = parse_spec_content_with_domain("auth/api", content);
        assert_eq!(reqs[0].scenarios[0].id, "auth/token-expired");
    }

    #[test]
    fn parse_spec_content_with_domain_sets_stable_scenario_id() {
        let content = r#"# auth

### Requirement: Token Validation

#### Scenario: Expired token rejected
Given expired token
When request happens
Then response is 401
"#;
        let reqs = parse_spec_content_with_domain("auth/api", content);
        assert_eq!(
            reqs[0].scenarios[0].id,
            "auth-api/token-validation/expired-token-rejected"
        );
    }

    #[test]
    fn parse_spec_content_basic() {
        let content = r#"# progress

### Requirement: Events emitted on stdout

Description of the requirement.

#### Scenario: Child lifecycle events

Given a cleave run with 2 children
When the orchestrator dispatches children
Then stdout contains a wave_start event
And stdout contains a child_spawned event for each child
And each JSON line is valid

#### Scenario: Merge events

Given a cleave run where all children complete
When the orchestrator enters merge
Then stdout contains a merge_start event

### Requirement: TS wrapper maps events

#### Scenario: Dashboard shows running children

Given a cleave_run invocation
When child_spawned events arrive
Then sharedState.cleave.children[i].status becomes running
"#;

        let reqs = parse_spec_content(content);
        assert_eq!(reqs.len(), 2, "Should have 2 requirements");

        assert_eq!(reqs[0].title, "Events emitted on stdout");
        assert!(reqs[0].description.contains("Description"));
        assert_eq!(reqs[0].scenarios.len(), 2);
        assert_eq!(reqs[0].scenarios[0].title, "Child lifecycle events");
        assert!(reqs[0].scenarios[0].given.contains("2 children"));
        assert_eq!(reqs[0].scenarios[0].and_clauses.len(), 2);

        assert_eq!(reqs[1].title, "TS wrapper maps events");
        assert_eq!(reqs[1].scenarios.len(), 1);
    }

    #[test]
    fn validate_task_stable_ids_reports_duplicates_and_invalid_markers() {
        let dir = tempfile::tempdir().unwrap();
        let task_dir = dir.path().join("openspec/changes/stable-id-validation");
        std::fs::create_dir_all(&task_dir).unwrap();
        let path = task_dir.join("tasks.md");
        std::fs::write(
            &path,
            "## 1. Group

- [ ] 1.1 First <!-- task-id: stable-alpha -->
- [ ] 1.2 Second <!-- task-id: stable-alpha -->
- [ ] 1.3 Bad <!-- task-id: bad id -->
",
        )
        .unwrap();
        let report = OpenSpecRepository::new(dir.path())
            .validate_task_stable_ids("stable-id-validation")
            .unwrap();
        assert!(!report.is_ok());
        assert_eq!(report.findings.len(), 2);
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.message.contains("duplicate task-id marker"))
        );
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.message.contains("ASCII letters"))
        );
    }

    #[test]
    fn set_task_checkbox_status_preserves_stable_task_id_marker() {
        let dir = tempfile::tempdir().unwrap();
        let task_dir = dir.path().join("openspec/changes/demo");
        std::fs::create_dir_all(&task_dir).unwrap();
        std::fs::write(
            task_dir.join("tasks.md"),
            "## 1. Group

- [ ] 1.1 Pending <!-- task-id: stable-alpha -->
",
        )
        .unwrap();

        let report = set_task_checkbox_status(
            dir.path(),
            "demo",
            "1. Group",
            "1.1",
            TaskCheckboxStatus::Done,
        )
        .unwrap();
        assert_eq!(report.description, "Pending <!-- task-id: stable-alpha -->");
        let content = std::fs::read_to_string(task_dir.join("tasks.md")).unwrap();
        assert!(content.contains("- [x] 1.1 Pending <!-- task-id: stable-alpha -->"));
    }

    #[test]
    fn parse_task_groups_reads_stable_task_id_marker() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.md");
        std::fs::write(
            &path,
            "## 1. Group

- [ ] 1.1 Validate behavior <!-- task-id: stable-alpha -->
",
        )
        .unwrap();
        let groups = parse_task_groups(&path);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].tasks[0].id, "1.1");
        assert_eq!(
            groups[0].tasks[0].stable_id.as_deref(),
            Some("stable-alpha")
        );
        assert_eq!(groups[0].tasks[0].description, "1.1 Validate behavior");
    }

    #[test]
    fn count_tasks_basic() {
        let dir = std::env::temp_dir().join("omegon-test-tasks");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("tasks.md");
        fs::write(
            &path,
            "# Tasks\n\n## Group 1\n\n- [x] Done task\n- [ ] Pending task\n- [x] Another done\n",
        )
        .unwrap();

        let (total, done) = count_tasks(&path);
        assert_eq!(total, 3);
        assert_eq!(done, 2);
        let groups = parse_task_groups(&path);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].title, "Group 1");
        assert_eq!(groups[0].tasks[0].id, "done-task");
        assert_eq!(groups[0].tasks[1].description, "Pending task");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn set_task_checkbox_status_updates_single_numeric_task() {
        let dir =
            std::env::temp_dir().join(format!("omegon-test-task-write-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let change_dir = dir.join("openspec/changes/example");
        fs::create_dir_all(&change_dir).unwrap();
        fs::write(
            change_dir.join("tasks.md"),
            "# Tasks\n\n## 1. Runtime\n- [ ] 1.1 Pending task\n- [x] 1.2 Done task\n",
        )
        .unwrap();

        let report = set_task_checkbox_status(
            &dir,
            "example",
            "1. Runtime",
            "1.1",
            TaskCheckboxStatus::Done,
        )
        .unwrap();

        assert_eq!(report.line, 4);
        assert!(!report.previous_done);
        assert!(report.new_done);
        let content = fs::read_to_string(change_dir.join("tasks.md")).unwrap();
        assert!(content.contains("- [x] 1.1 Pending task"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn set_task_checkbox_status_refuses_duplicate_task_id() {
        let dir = std::env::temp_dir().join(format!(
            "omegon-test-task-write-dupe-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        let change_dir = dir.join("openspec/changes/example");
        fs::create_dir_all(&change_dir).unwrap();
        fs::write(
            change_dir.join("tasks.md"),
            "# Tasks\n\n## 1. Runtime\n- [ ] 1.1 A\n- [ ] 1.1 B\n",
        )
        .unwrap();

        let err = set_task_checkbox_status(
            &dir,
            "example",
            "1. Runtime",
            "1.1",
            TaskCheckboxStatus::Done,
        )
        .unwrap_err()
        .to_string();

        assert!(err.contains("ambiguous"), "{err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn set_task_checkbox_status_preserves_crlf_newlines() {
        let dir = std::env::temp_dir().join(format!(
            "omegon-test-task-write-crlf-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        let change_dir = dir.join("openspec/changes/example");
        fs::create_dir_all(&change_dir).unwrap();
        fs::write(
            change_dir.join("tasks.md"),
            "# Tasks\r\n\r\n## 1. Runtime\r\n- [ ] 1.1 Pending task\r\n",
        )
        .unwrap();

        set_task_checkbox_status(
            &dir,
            "example",
            "1. Runtime",
            "1.1",
            TaskCheckboxStatus::Done,
        )
        .unwrap();

        let content = fs::read_to_string(change_dir.join("tasks.md")).unwrap();
        assert!(content.contains("\r\n"));
        assert!(content.lines().count() >= 3);
        assert!(content.contains("- [x] 1.1 Pending task\r\n"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn context_injection_format() {
        let changes = vec![ChangeInfo {
            name: "test-change".into(),
            path: PathBuf::new(),
            state: ChangeState::Implementing,
            artifact_health: omegon_opsx::ArtifactHealth::Healthy,
            has_proposal: true,
            has_design: true,
            has_specs: true,
            has_tasks: true,
            total_tasks: 10,
            done_tasks: 7,
            task_groups: vec![],
            specs: vec![project_spec_file(omegon_opsx::SpecFile {
                domain: "auth".into(),
                file_path: PathBuf::new(),
                requirements: vec![omegon_opsx::Requirement {
                    title: "Auth".into(),
                    description: String::new(),
                    scenarios: vec![omegon_opsx::Scenario {
                        id: "auth/auth/login".into(),
                        title: "Login".into(),
                        given: "user".into(),
                        when: "login".into(),
                        then: "success".into(),
                        and_clauses: vec![],
                    }],
                }],
            })],
        }];

        let injection = build_context_injection(&changes);
        assert!(injection.contains("[OpenSpec"));
        assert!(injection.contains("test-change"));
        assert!(injection.contains("7/10"));
        assert!(injection.contains("specs/auth: 1 scenarios"));
    }

    #[test]
    fn parse_real_baseline_format() {
        // Test against the actual baseline format used by Omegon
        let content = r#"# progress

### Requirement: Rust orchestrator emits NDJSON progress events on stdout

#### Scenario: Child lifecycle events appear on stdout as JSON

Given a cleave run with 2 children in one wave
When the Rust orchestrator dispatches children
Then stdout contains a `wave_start` event with both child labels
And stdout contains a `child_spawned` event for each child with pid
And stdout contains a `child_status` event with status `completed` or `failed` for each child
And each JSON line is valid self-contained JSON (parseable independently)
"#;

        let reqs = parse_spec_content(content);
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].scenarios.len(), 1);
        assert_eq!(reqs[0].scenarios[0].and_clauses.len(), 3);
        assert!(reqs[0].scenarios[0].given.contains("2 children"));
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn scan_real_openspec_directory() {
        let repo_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();

        let changes = list_changes(repo_path);
        tracing::debug!("Found {} active OpenSpec changes", changes.len());

        // Verify baseline specs can be parsed
        let baseline_dir = repo_path.join("openspec/baseline");
        if baseline_dir.is_dir() {
            let specs = parse_specs_dir(&baseline_dir);
            tracing::debug!("Parsed {} baseline spec files", specs.len());
            for spec in &specs {
                let scenario_count: usize =
                    spec.requirements.iter().map(|r| r.scenarios.len()).sum();
                tracing::debug!(
                    "  {}: {} requirements, {} scenarios",
                    spec.domain,
                    spec.requirements.len(),
                    scenario_count
                );
                assert!(
                    !spec.requirements.is_empty() || scenario_count == 0,
                    "Spec {} should have requirements if it has scenarios",
                    spec.domain
                );
            }
        }
    }
}

#[cfg(test)]
mod mutation_tests {
    use super::*;

    #[test]
    fn propose_creates_directory_and_proposal() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();

        let change = propose_change(repo, "my-change", "My Change", "Do things").unwrap();
        assert_eq!(change.name, "my-change");
        assert_eq!(change.state, ChangeState::Proposed);
        assert!(change.has_proposal);
        assert!(change.path.join("proposal.md").exists());

        let content = fs::read_to_string(change.path.join("proposal.md")).unwrap();
        assert!(content.contains("My Change"));
        assert!(content.contains("Do things"));
    }

    #[test]
    fn propose_duplicate_fails() {
        let dir = tempfile::tempdir().unwrap();
        propose_change(dir.path(), "dup", "Dup", "intent").unwrap();
        assert!(propose_change(dir.path(), "dup", "Dup2", "intent2").is_err());
    }

    #[test]
    fn add_spec_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        propose_change(dir.path(), "spec-test", "Spec Test", "intent").unwrap();

        let path = add_spec(dir.path(), "spec-test", "auth", "# auth specs").unwrap();
        assert!(path.exists());
        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content, "# auth specs");
    }

    #[test]
    fn add_spec_nested_domain() {
        let dir = tempfile::tempdir().unwrap();
        propose_change(dir.path(), "nested", "Nested", "intent").unwrap();

        let path = add_spec(dir.path(), "nested", "auth/tokens", "# token specs").unwrap();
        assert!(path.exists());
        assert!(path.to_str().unwrap().contains("auth"));
    }

    #[test]
    fn add_spec_nonexistent_change_fails() {
        let dir = tempfile::tempdir().unwrap();
        assert!(add_spec(dir.path(), "nope", "auth", "specs").is_err());
    }

    #[test]
    fn archive_moves_to_archive_dir() {
        let dir = tempfile::tempdir().unwrap();
        let change = propose_change(dir.path(), "to-archive", "Archive Me", "intent").unwrap();
        assert!(change.path.exists());

        archive_change(dir.path(), "to-archive").unwrap();
        assert!(!change.path.exists(), "original should be gone");
        assert!(
            dir.path().join("openspec/archive/to-archive").exists(),
            "should be in archive"
        );
    }

    #[test]
    fn archive_nonexistent_fails() {
        let dir = tempfile::tempdir().unwrap();
        assert!(archive_change(dir.path(), "nope").is_err());
    }

    #[test]
    fn propose_then_list() {
        let dir = tempfile::tempdir().unwrap();
        propose_change(dir.path(), "listed", "Listed Change", "intent").unwrap();

        let changes = list_changes(dir.path());
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].name, "listed");
        assert_eq!(changes[0].state, ChangeState::Proposed);
    }

    #[test]
    fn evidence_gate_reports_refuted_claims_without_archiving_block() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        let evidence_dir = repo.join(".omegon/evidence");
        fs::create_dir_all(&evidence_dir).unwrap();
        fs::write(
            evidence_dir.join("claims.jsonl"),
            serde_json::json!({
                "schema": "claim-record/v1",
                "id": "claim:docs-ready",
                "kind": "documentation-quality",
                "text": "Docs are ready",
                "status": "asserted",
                "scope": [],
                "created_at_ms": 1,
                "metadata": {}
            })
            .to_string()
                + "\n",
        )
        .unwrap();
        fs::write(
            evidence_dir.join("records.jsonl"),
            serde_json::json!({
                "schema": "evidence-record/v1",
                "id": "evidence:docs-not-ready",
                "provider": "code-evidence",
                "kind": "rust-doc-coverage",
                "status": "docs-warnings",
                "subjects": [],
                "claims": ["claim:docs-ready"],
                "artifacts": [],
                "source_state": {},
                "created_at_ms": 1,
                "metadata": {}
            })
            .to_string()
                + "\n",
        )
        .unwrap();
        fs::write(
            evidence_dir.join("edges.jsonl"),
            serde_json::json!({
                "schema": "evidence-edge/v1",
                "from": "evidence:docs-not-ready",
                "to": "claim:docs-ready",
                "kind": "refutes",
                "created_at_ms": 1
            })
            .to_string()
                + "\n",
        )
        .unwrap();

        propose_change(repo, "evidence-demo", "Evidence Demo", "intent").unwrap();
        add_spec(
            repo,
            "evidence-demo",
            "docs",
            "# docs\n\n### Requirement: Docs are ready\n\nevidence-claim: claim:docs-ready\n\n#### Scenario: Documentation claim is supported\n\nGiven docs exist\nWhen evidence is evaluated\nThen the claim is supported\n",
        )
        .unwrap();

        let change = list_changes(repo).remove(0);
        let findings = evaluate_evidence_gates(&change);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].decision, EvidenceGateDecision::Block);
        assert_eq!(
            findings[0].status,
            crate::evidence::ClaimSupportStatus::Refuted
        );
        archive_change(repo, "evidence-demo").unwrap();
        assert!(repo.join("openspec/archive/evidence-demo").exists());
    }

    fn list_changes_annotates_evidence_claim_support() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        let evidence_dir = repo.join(".omegon/evidence");
        fs::create_dir_all(&evidence_dir).unwrap();
        fs::write(
            evidence_dir.join("claims.jsonl"),
            serde_json::json!({
                "schema": "claim-record/v1",
                "id": "claim:docs-ready",
                "kind": "documentation-quality",
                "text": "Docs are ready.",
                "status": "asserted",
                "scope": [],
                "created_at_ms": 1,
                "metadata": {}
            })
            .to_string()
                + "\n",
        )
        .unwrap();
        fs::write(
            evidence_dir.join("records.jsonl"),
            serde_json::json!({
                "schema": "evidence-record/v1",
                "id": "evidence:docs-ready",
                "provider": "test",
                "kind": "manual-review",
                "status": "pass",
                "subjects": ["claim:docs-ready"],
                "claims": ["claim:docs-ready"],
                "artifacts": [],
                "source_state": {},
                "created_at_ms": 1,
                "metadata": {}
            })
            .to_string()
                + "\n",
        )
        .unwrap();
        fs::write(
            evidence_dir.join("edges.jsonl"),
            serde_json::json!({
                "schema": "evidence-edge/v1",
                "from": "evidence:docs-ready",
                "to": "claim:docs-ready",
                "kind": "supports",
                "created_at_ms": 1
            })
            .to_string()
                + "\n",
        )
        .unwrap();

        propose_change(repo, "evidence-demo", "Evidence Demo", "intent").unwrap();
        add_spec(
            repo,
            "evidence-demo",
            "docs",
            "# docs\n\n### Requirement: Docs are ready\n\nevidence-claim: claim:docs-ready\n\n#### Scenario: Documentation claim is supported\n\nGiven docs exist\nWhen evidence is evaluated\nThen the claim is supported\n",
        )
        .unwrap();

        let changes = list_changes(repo);
        let scenario = &changes[0].specs[0].requirements[0].scenarios[0];
        assert_eq!(scenario.evidence_claims, vec!["claim:docs-ready"]);
        assert_eq!(scenario.evidence_support.len(), 1);
        assert_eq!(
            scenario.evidence_support[0].status,
            crate::evidence::ClaimSupportStatus::Supported
        );
        assert_eq!(scenario.evidence_support[0].supports, 1);
        assert_eq!(scenario.evidence_support[0].refutes, 0);
    }

    #[test]
    fn propose_add_spec_updates_stage() {
        let dir = tempfile::tempdir().unwrap();
        propose_change(dir.path(), "staged", "Staged", "intent").unwrap();
        add_spec(
            dir.path(),
            "staged",
            "core",
            "# core\n\n### Requirement: Works\n\n#### Scenario: Basic\n\nGiven X\nWhen Y\nThen Z\n",
        )
        .unwrap();

        let changes = list_changes(dir.path());
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].state, ChangeState::Specced);
        assert!(changes[0].has_specs);
    }
}
