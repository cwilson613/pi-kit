//! Renderer-neutral core component policy and runtime status.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentPackageProjection {
    pub identity: String,
    pub source: Option<String>,
    pub present: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentProcessProvenance {
    pub identity: String,
    pub source_digest: String,
    pub pid: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentRuntimeEvidence {
    NotObserved,
    Absent,
    Incompatible,
    Failed,
    Quarantined,
    Healthy(ComponentProcessProvenance),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ComponentState {
    Packaged,
    DisabledByProfile,
    DisabledByPolicy,
    Absent,
    Incompatible,
    Failed,
    Quarantined,
    Healthy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ComponentFinalDecision {
    Eligible,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ComponentDeterminingSourceProjection {
    CompositionDefault,
    SelectedProfile { profile: String, path: String },
    UserLocal { path: std::path::PathBuf },
    ChildPropagation { env: String },
    DeprecatedExtensionField { profile: String, path: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentPolicySnapshot {
    pub component_id: String,
    pub enabled: bool,
    pub determining_source: ComponentDeterminingSourceProjection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentStatusProjection {
    pub component_id: String,
    pub package: Option<ComponentPackageProjection>,
    pub state: ComponentState,
    pub final_decision: ComponentFinalDecision,
    pub determining_source: ComponentDeterminingSourceProjection,
    pub restart_bound: bool,
    pub process: Option<ComponentProcessProvenance>,
}

impl ComponentStatusProjection {
    pub fn new(
        policy: &ComponentPolicySnapshot,
        package: Option<ComponentPackageProjection>,
        runtime: ComponentRuntimeEvidence,
    ) -> Self {
        let process = match &runtime {
            ComponentRuntimeEvidence::Healthy(process) => Some(process.clone()),
            _ => None,
        };
        let state = if !policy.enabled {
            match &policy.determining_source {
                ComponentDeterminingSourceProjection::SelectedProfile { .. }
                | ComponentDeterminingSourceProjection::DeprecatedExtensionField { .. } => {
                    ComponentState::DisabledByProfile
                }
                _ => ComponentState::DisabledByPolicy,
            }
        } else {
            match runtime {
                ComponentRuntimeEvidence::NotObserved
                    if package.as_ref().is_some_and(|package| package.present) =>
                {
                    ComponentState::Packaged
                }
                ComponentRuntimeEvidence::NotObserved | ComponentRuntimeEvidence::Absent => {
                    ComponentState::Absent
                }
                ComponentRuntimeEvidence::Incompatible => ComponentState::Incompatible,
                ComponentRuntimeEvidence::Failed => ComponentState::Failed,
                ComponentRuntimeEvidence::Quarantined => ComponentState::Quarantined,
                ComponentRuntimeEvidence::Healthy(_) => ComponentState::Healthy,
            }
        };
        Self {
            component_id: policy.component_id.clone(),
            package,
            state,
            final_decision: if policy.enabled {
                ComponentFinalDecision::Eligible
            } else {
                ComponentFinalDecision::Disabled
            },
            determining_source: policy.determining_source.clone(),
            restart_bound: true,
            process,
        }
    }
}
