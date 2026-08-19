//! Workflow templates — declarative per-phase configuration for lifecycle-driven work.
//!
//! Templates live in `.omegon/workflows/<name>.toml` and define persona, model,
//! max_turns, and context_class for each lifecycle phase. The daemon dispatch bridge
//! and cleave orchestrator consult these when dispatching work.

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::contribution_loading::GuardedContributionDirectory;

const MAX_WORKFLOW_BYTES: usize = 1024 * 1024;
const MAX_WORKFLOW_ENTRIES: usize = 10_000;

/// A parsed workflow template.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkflowTemplate {
    pub workflow: WorkflowMeta,
    #[serde(default)]
    pub phases: WorkflowPhases,
}

/// Top-level metadata for a workflow template.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkflowMeta {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

/// Per-phase configuration. Each field is optional — only present phases are configured.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct WorkflowPhases {
    pub exploring: Option<PhaseConfig>,
    pub specifying: Option<PhaseConfig>,
    pub decomposing: Option<PhaseConfig>,
    pub implementing: Option<PhaseConfig>,
    pub verifying: Option<PhaseConfig>,
}

/// Configuration for a single lifecycle phase.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PhaseConfig {
    pub persona: Option<String>,
    pub model: Option<String>,
    pub max_turns: Option<u32>,
    pub context_class: Option<String>,
    pub thinking_level: Option<String>,
}

impl WorkflowTemplate {
    fn parse(bytes: &[u8]) -> anyhow::Result<Self> {
        let content = std::str::from_utf8(bytes)?;
        let template: Self = toml::from_str(content)?;
        Ok(template)
    }

    /// Get the phase config for a lifecycle phase.
    pub fn phase_config(&self, phase: &omegon_traits::LifecyclePhase) -> Option<&PhaseConfig> {
        use omegon_traits::LifecyclePhase;
        match phase {
            LifecyclePhase::Exploring { .. } => self.phases.exploring.as_ref(),
            LifecyclePhase::Specifying { .. } => self.phases.specifying.as_ref(),
            LifecyclePhase::Decomposing => self.phases.decomposing.as_ref(),
            LifecyclePhase::Implementing { .. } => self.phases.implementing.as_ref(),
            LifecyclePhase::Verifying { .. } => self.phases.verifying.as_ref(),
            LifecyclePhase::Idle => None,
        }
    }
}

/// Scan `.omegon/workflows/` for TOML templates. Returns the first valid one found
/// (sorted alphabetically by filename).
pub fn with_discovered_workflow<R>(
    cwd: &Path,
    publish: impl FnOnce(Option<WorkflowTemplate>) -> R,
) -> R {
    let home = crate::paths::omegon_home();
    let admitted = match home.and_then(|home| discover_workflow_with_home(cwd, &home)) {
        Ok(template) => template,
        Err(error) => {
            tracing::warn!(error = %error, "workflow discovery failed closed");
            None
        }
    };
    match admitted {
        Some(admitted) => admitted.publish(|template| publish(Some(template))),
        None => publish(None),
    }
}

struct AdmittedWorkflow {
    template: WorkflowTemplate,
    _admission: GuardedContributionDirectory,
}

impl AdmittedWorkflow {
    fn publish<R>(self, publish: impl FnOnce(WorkflowTemplate) -> R) -> R {
        let Self {
            template,
            _admission,
        } = self;
        publish(template)
    }
}

#[cfg(unix)]
fn discover_workflow_with_home(
    cwd: &Path,
    home_path: &Path,
) -> anyhow::Result<Option<AdmittedWorkflow>> {
    let Some(admission) = GuardedContributionDirectory::open(
        cwd,
        &[b".omegon", b"workflows"],
        home_path,
        omegon_maintenance_contracts::ContributionKind::Workflow,
        "project",
    )?
    else {
        return Ok(None);
    };
    let mut entries = admission.entry_names(MAX_WORKFLOW_ENTRIES)?;
    entries.retain(|name| name.ends_with(b".toml"));
    entries.sort();
    for raw_name in entries {
        if !admission.allows(&raw_name)? {
            tracing::info!(path = %display_workflow_path(cwd, &raw_name), "excluded denied workflow template");
            continue;
        }
        let display_path = display_workflow_path(cwd, &raw_name);
        let Some(bytes) = admission.read_file(&raw_name, MAX_WORKFLOW_BYTES)? else {
            continue;
        };
        match WorkflowTemplate::parse(&bytes) {
            Ok(t) => {
                tracing::info!(
                    workflow = %t.workflow.name,
                    path = %display_path,
                    "loaded workflow template"
                );
                return Ok(Some(AdmittedWorkflow {
                    template: t,
                    _admission: admission,
                }));
            }
            Err(e) => {
                tracing::warn!(
                    path = %display_path,
                    error = %e,
                    "skipping invalid workflow template"
                );
            }
        }
    }
    Ok(None)
}

#[cfg(not(unix))]
fn discover_workflow_with_home(
    _cwd: &Path,
    _home_path: &Path,
) -> anyhow::Result<Option<AdmittedWorkflow>> {
    anyhow::bail!("guarded workflow discovery requires Unix")
}

#[cfg(unix)]
fn display_workflow_path(cwd: &Path, raw_name: &[u8]) -> String {
    use std::os::unix::ffi::OsStrExt;

    cwd.join(".omegon")
        .join("workflows")
        .join(std::ffi::OsStr::from_bytes(raw_name))
        .display()
        .to_string()
}

/// A design-tree node that is ready for autonomous dispatch.
#[derive(Debug, Clone)]
pub struct ReadyNode {
    pub id: String,
    pub title: String,
    pub priority: Option<u8>,
}

/// Query the design tree for nodes that are ready to implement:
/// status == Decided, all dependencies Implemented, not archived.
/// Reads directly from filesystem — no bus or Feature access required.
pub fn query_ready_nodes(cwd: &Path) -> Vec<ReadyNode> {
    use crate::lifecycle::{design, types::NodeStatus};

    let docs_dir = cwd.join("docs");
    if !docs_dir.is_dir() {
        return Vec::new();
    }
    let nodes = design::scan_design_docs(&docs_dir);
    nodes
        .values()
        .filter(|n| !matches!(n.status, NodeStatus::Archived))
        .filter(|n| matches!(n.status, NodeStatus::Decided))
        .filter(|n| {
            n.dependencies.iter().all(|dep_id| {
                nodes
                    .get(dep_id)
                    .is_some_and(|d| matches!(d.status, NodeStatus::Implemented))
            })
        })
        .map(|n| ReadyNode {
            id: n.id.clone(),
            title: n.title.clone(),
            priority: n.priority,
        })
        .collect()
}

/// Build a prompt for a ready design-tree node, suitable for daemon dispatch.
pub fn build_dispatch_prompt(node: &ReadyNode) -> String {
    format!(
        "Implement design node `{}`: {}\n\n\
         This node has been marked as decided and all dependencies are satisfied. \
         Transition it to implementing, create the necessary changes, and verify \
         the implementation meets the design criteria.",
        node.id, node.title
    )
}

/// Apply workflow phase config to a LoopConfig for a given lifecycle phase.
pub fn apply_phase_config(
    loop_config: &mut crate::r#loop::LoopConfig,
    phase_config: &PhaseConfig,
    shared_settings: &crate::settings::SharedSettings,
) {
    if let Some(ref model) = phase_config.model {
        loop_config.model = model.clone();
        if let Ok(mut s) = shared_settings.lock() {
            s.set_model(model);
        }
    }
    if let Some(max_turns) = phase_config.max_turns {
        loop_config.max_turns = max_turns;
        loop_config.soft_limit_turns = if max_turns > 0 { max_turns * 2 / 3 } else { 0 };
    }
    // Persona is handled separately via OMEGON_CHILD_PERSONA env var.
    // Context class and thinking level are handled via shared settings.
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    const EXAMPLE_TOML: &str = r#"
[workflow]
name = "standard-feature"
description = "Standard feature development workflow"

[phases.exploring]
persona = "researcher"
model = "gemini-flash"
max_turns = 30

[phases.implementing]
persona = "systems-engineer"
model = "claude-sonnet-4-6"
max_turns = 60
context_class = "extended"

[phases.verifying]
persona = "security-auditor"
model = "claude-opus-4-6"
max_turns = 20
thinking_level = "high"
"#;

    #[test]
    fn parse_workflow_template() {
        let template: WorkflowTemplate = toml::from_str(EXAMPLE_TOML).unwrap();
        assert_eq!(template.workflow.name, "standard-feature");
        assert_eq!(
            template.workflow.description,
            "Standard feature development workflow"
        );
    }

    #[test]
    fn phase_configs_present() {
        let template: WorkflowTemplate = toml::from_str(EXAMPLE_TOML).unwrap();

        let exploring = template.phases.exploring.as_ref().unwrap();
        assert_eq!(exploring.persona.as_deref(), Some("researcher"));
        assert_eq!(exploring.model.as_deref(), Some("gemini-flash"));
        assert_eq!(exploring.max_turns, Some(30));

        let implementing = template.phases.implementing.as_ref().unwrap();
        assert_eq!(implementing.persona.as_deref(), Some("systems-engineer"));
        assert_eq!(implementing.context_class.as_deref(), Some("extended"));

        let verifying = template.phases.verifying.as_ref().unwrap();
        assert_eq!(verifying.thinking_level.as_deref(), Some("high"));
    }

    #[test]
    fn unconfigured_phases_are_none() {
        let template: WorkflowTemplate = toml::from_str(EXAMPLE_TOML).unwrap();
        assert!(template.phases.specifying.is_none());
        assert!(template.phases.decomposing.is_none());
    }

    #[test]
    fn phase_config_lookup() {
        let template: WorkflowTemplate = toml::from_str(EXAMPLE_TOML).unwrap();

        let result =
            template.phase_config(&omegon_traits::LifecyclePhase::Exploring { node_id: None });
        assert!(result.is_some());
        assert_eq!(result.unwrap().persona.as_deref(), Some("researcher"));

        let result = template.phase_config(&omegon_traits::LifecyclePhase::Idle);
        assert!(result.is_none());

        let result =
            template.phase_config(&omegon_traits::LifecyclePhase::Specifying { change_id: None });
        assert!(result.is_none());
    }

    #[test]
    fn minimal_template() {
        let toml_str = r#"
[workflow]
name = "bare"

[phases.implementing]
model = "ollama:llama3"
"#;
        let template: WorkflowTemplate = toml::from_str(toml_str).unwrap();
        assert_eq!(template.workflow.name, "bare");
        assert!(template.phases.implementing.is_some());
        assert!(template.phases.exploring.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn guarded_discovery_loads_first_valid_workflow() {
        let project = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        write_workflow(project.path(), "b.toml", "second");
        write_workflow(project.path(), "a.toml", "first");

        let workflow = discover_workflow_with_home(project.path(), home.path())
            .unwrap()
            .unwrap();
        assert_eq!(workflow.template.workflow.name, "first");
    }

    #[cfg(unix)]
    #[test]
    fn guarded_discovery_excludes_exact_denied_basename() {
        let project = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        write_workflow(project.path(), "a.toml", "denied");
        write_workflow(project.path(), "b.toml", "allowed");
        deny_workflow(project.path(), home.path(), b"a.toml");

        let workflow = discover_workflow_with_home(project.path(), home.path())
            .unwrap()
            .unwrap();
        assert_eq!(workflow.template.workflow.name, "allowed");
    }

    #[cfg(unix)]
    #[test]
    fn guarded_discovery_fails_closed_on_malformed_deny_state() {
        use std::io::Write;

        let project = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        write_workflow(project.path(), "a.toml", "must-not-load");
        let authority = initialize_workflow_scope(project.path(), home.path());
        let state_path = home
            .path()
            .join("maintain/v1/deny")
            .join(authority.to_hex())
            .join("state.json");
        let mut state = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(state_path)
            .unwrap();
        state.write_all(b"{not-json").unwrap();
        state.sync_all().unwrap();

        assert!(discover_workflow_with_home(project.path(), home.path()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn guarded_discovery_holds_lock_through_publication() {
        use omegon_maintenance_contracts::{LockMode, MaintenanceStateV1, ProtocolLock};

        let project = tempfile::tempdir().unwrap();
        let home_path = tempfile::tempdir().unwrap();
        write_workflow(project.path(), "a.toml", "published");
        let admitted = discover_workflow_with_home(project.path(), home_path.path())
            .unwrap()
            .unwrap();
        let authority = admitted._admission.scope_key();
        let home = omegon_maintenance_contracts::open_secure_root(home_path.path()).unwrap();
        let state = MaintenanceStateV1::bootstrap(
            &home,
            omegon_maintenance_contracts::path_identity(&home).unwrap(),
            "11111111-1111-1111-1111-111111111111",
            false,
        )
        .unwrap();
        let lock_name = format!("contribution-{authority}.lock");

        let template = admitted.publish(|template| {
            assert!(
                ProtocolLock::acquire_at(
                    &state.locks,
                    lock_name.as_bytes(),
                    LockMode::Exclusive,
                    false,
                    true,
                )
                .is_err()
            );
            template
        });
        assert_eq!(template.workflow.name, "published");
        assert!(
            ProtocolLock::acquire_at(
                &state.locks,
                lock_name.as_bytes(),
                LockMode::Exclusive,
                false,
                true,
            )
            .is_ok()
        );
    }

    #[cfg(unix)]
    fn write_workflow(project: &Path, name: &str, workflow_name: &str) {
        let directory = project.join(".omegon/workflows");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join(name),
            format!("[workflow]\nname = \"{workflow_name}\"\n"),
        )
        .unwrap();
    }

    #[cfg(unix)]
    fn initialize_workflow_scope(
        project: &Path,
        home_path: &Path,
    ) -> omegon_maintenance_contracts::AuthorityKey {
        let home = omegon_maintenance_contracts::open_secure_root(home_path).unwrap();
        let state = omegon_maintenance_contracts::MaintenanceStateV1::bootstrap(
            &home,
            omegon_maintenance_contracts::path_identity(&home).unwrap(),
            "11111111-1111-1111-1111-111111111111",
            false,
        )
        .unwrap();
        let workflows = File::open(project.join(".omegon/workflows")).unwrap();
        let parent = omegon_maintenance_contracts::path_identity(&workflows).unwrap();
        state
            .admit_contribution_scope(
                omegon_maintenance_contracts::ContributionKind::Workflow,
                "project",
                &parent,
                "initialize-test",
                false,
            )
            .unwrap()
            .scope_key
    }

    #[cfg(unix)]
    fn deny_workflow(project: &Path, home_path: &Path, raw_name: &[u8]) {
        use omegon_maintenance_contracts::{
            AuthorityKey, ContributionKind, DenyRecordV1, DenyState, DenyStateV1, SCHEMA_VERSION,
            derive_key, entry_key, open_secure_dir_at, replace_record_at,
        };
        use sha2::{Digest, Sha256};

        let authority = initialize_workflow_scope(project, home_path);
        let home = omegon_maintenance_contracts::open_secure_root(home_path).unwrap();
        let state = omegon_maintenance_contracts::MaintenanceStateV1::bootstrap(
            &home,
            omegon_maintenance_contracts::path_identity(&home).unwrap(),
            "11111111-1111-1111-1111-111111111111",
            false,
        )
        .unwrap();
        let deny_directory = open_secure_dir_at(&state.deny, authority.to_hex().as_bytes())
            .unwrap()
            .unwrap();
        let kind = ContributionKind::Workflow;
        let entry = entry_key(kind.as_str(), authority, raw_name);
        let request_id = "00000000-0000-0000-0000-000000000001";
        let record = DenyRecordV1 {
            schema_version: SCHEMA_VERSION,
            record_kind: "deny".into(),
            record_id: derive_key(
                "deny",
                &[
                    authority.as_bytes(),
                    entry.as_bytes(),
                    request_id.as_bytes(),
                ],
            ),
            scope_key: authority,
            contribution_kind: kind,
            entry_key: entry,
            raw_name_digest: AuthorityKey::from_bytes(Sha256::digest(raw_name).into()),
            generation: 1,
            state: DenyState::Denied,
            request_id: request_id.into(),
            created_at: "2026-08-17T00:00:00Z".into(),
        };
        let deny = DenyStateV1 {
            schema_version: SCHEMA_VERSION,
            record_kind: "deny_state".into(),
            record_id: derive_key("deny-state", &[authority.as_bytes(), &1_u64.to_be_bytes()]),
            scope_key: authority,
            generation: 1,
            entries: [(entry.to_hex(), record)].into(),
        };
        replace_record_at(&deny_directory, b"state.json", &deny, "deny-test").unwrap();
    }
}
