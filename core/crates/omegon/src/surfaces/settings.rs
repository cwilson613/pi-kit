//! Renderer-neutral settings surface projection.
//!
//! TUI, ACP, CLI, web, and agent-facing tools should render settings from this
//! semantic projection instead of each surface rebuilding its own settings view.

use serde::{Deserialize, Serialize};

use crate::surfaces::profile::{ProfileDriftProjection, ProfileDriftRow};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsSurfaceSnapshot {
    pub model: String,
    pub thinking: String,
    pub thinking_choices: Vec<SettingsChoiceProjection>,
    pub context_class: String,
    pub context_window: usize,
    pub context_choices: Vec<SettingsChoiceProjection>,
    pub max_turns: u32,
    pub tool_detail: String,
    pub trusted_directory_count: usize,
    pub sandbox: bool,
    pub update_channel: String,
    pub auto_update: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsSurfaceProjection {
    pub tabs: Vec<SettingsTabProjection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsTabProjection {
    pub id: String,
    pub label: String,
    pub rows: Vec<SettingsRowProjection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsRowProjection {
    pub id: String,
    pub label: String,
    pub value: String,
    pub description: String,
    pub route: SettingsMutationRouteProjection,
    pub persistence: SettingsPersistenceProjection,
    pub profile: Option<SettingsProfileProjection>,
    pub editor: SettingsEditorProjection,
    pub status: SettingsStatusProjection,
    pub choices: Vec<SettingsChoiceProjection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsChoiceProjection {
    pub value: String,
    pub label: String,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsProfileProjection {
    pub profile_value: String,
    pub runtime_value: String,
    pub state: SettingsProfileStateProjection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingsProfileStateProjection {
    SavedDefault,
    LiveOverride,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingsEditorProjection {
    Choice,
    Toggle,
    Text,
    Number,
    Action,
    ReadOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingsStatusProjection {
    Normal,
    Warning,
    Error,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingsMutationRouteProjection {
    RuntimeCommand,
    SharedSettings,
    LocalUi,
    ExternalAction,
    ReadOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingsPersistenceProjection {
    RuntimeOnly,
    PersistedProfile,
    ProjectPolicy,
    External,
    ReadOnly,
}

impl SettingsPersistenceProjection {
    pub fn label(self) -> &'static str {
        match self {
            Self::RuntimeOnly => "runtime only",
            Self::PersistedProfile => "profile",
            Self::ProjectPolicy => "project policy",
            Self::External => "external",
            Self::ReadOnly => "read only",
        }
    }
}

impl SettingsSurfaceProjection {
    pub fn from_snapshot(
        snapshot: &SettingsSurfaceSnapshot,
        drift: Option<&ProfileDriftProjection>,
    ) -> Self {
        let profile_row = |id: &str| {
            drift
                .and_then(|projection| {
                    projection
                        .rows
                        .iter()
                        .find(|row| settings_row_matches_drift(id, row))
                })
                .map(|row| SettingsProfileProjection {
                    profile_value: row.profile_value.clone(),
                    runtime_value: row.runtime_value.clone(),
                    state: SettingsProfileStateProjection::LiveOverride,
                })
        };

        let mut projection = Self {
            tabs: vec![
                SettingsTabProjection {
                    id: "runtime".into(),
                    label: "Runtime".into(),
                    rows: vec![
                        choice_row(
                            "runtime.model",
                            "Model",
                            &snapshot.model,
                            "Active model route for future turns",
                            SettingsMutationRouteProjection::RuntimeCommand,
                            SettingsPersistenceProjection::RuntimeOnly,
                            vec![],
                        ),
                        choice_row(
                            "runtime.thinking",
                            "Thinking",
                            &snapshot.thinking,
                            "Reasoning budget requested from capable providers",
                            SettingsMutationRouteProjection::RuntimeCommand,
                            SettingsPersistenceProjection::RuntimeOnly,
                            snapshot.thinking_choices.clone(),
                        ),
                        choice_row(
                            "runtime.context_class",
                            "Context class",
                            &snapshot.context_class,
                            &format!("Context window: {} tokens", snapshot.context_window),
                            SettingsMutationRouteProjection::RuntimeCommand,
                            SettingsPersistenceProjection::RuntimeOnly,
                            snapshot.context_choices.clone(),
                        ),
                        row(
                            "runtime.max_turns",
                            "Max turns",
                            &snapshot.max_turns.to_string(),
                            "Maximum autonomous turns for the current run",
                            SettingsMutationRouteProjection::SharedSettings,
                            SettingsPersistenceProjection::RuntimeOnly,
                            SettingsEditorProjection::Number,
                        ),
                    ],
                },
                SettingsTabProjection {
                    id: "ui".into(),
                    label: "UI".into(),
                    rows: vec![choice_row(
                        "ui.tool_detail",
                        "Tool display",
                        &snapshot.tool_detail,
                        "Density of tool-call summaries in interactive surfaces",
                        SettingsMutationRouteProjection::SharedSettings,
                        SettingsPersistenceProjection::PersistedProfile,
                        ["lean", "compact", "detailed", "verbose"]
                            .into_iter()
                            .map(|value| SettingsChoiceProjection {
                                value: value.into(),
                                label: value.into(),
                                active: value == snapshot.tool_detail,
                            })
                            .collect(),
                    )],
                },
                SettingsTabProjection {
                    id: "workspace".into(),
                    label: "Workspace".into(),
                    rows: vec![
                        row(
                            "workspace.trusted_directories",
                            "Trusted dirs",
                            &if snapshot.trusted_directory_count == 0 {
                                "none".to_string()
                            } else {
                                snapshot.trusted_directory_count.to_string()
                            },
                            "Directories outside the workspace allowed without repeated prompts",
                            SettingsMutationRouteProjection::RuntimeCommand,
                            SettingsPersistenceProjection::PersistedProfile,
                            SettingsEditorProjection::Action,
                        ),
                        row(
                            "workspace.sandbox",
                            "Sandbox",
                            if snapshot.sandbox {
                                "enabled"
                            } else {
                                "disabled"
                            },
                            "Run delegate/cleave children inside OCI isolation when available",
                            SettingsMutationRouteProjection::RuntimeCommand,
                            SettingsPersistenceProjection::PersistedProfile,
                            SettingsEditorProjection::Toggle,
                        ),
                        choice_row(
                            "workspace.role",
                            "Workspace role",
                            "select…",
                            "Federation role advertised for this checkout",
                            SettingsMutationRouteProjection::RuntimeCommand,
                            SettingsPersistenceProjection::ProjectPolicy,
                            [
                                "primary",
                                "feature",
                                "cleave_child",
                                "benchmark",
                                "release",
                                "exploratory",
                                "read_only",
                            ]
                            .into_iter()
                            .map(|role| SettingsChoiceProjection {
                                value: role.into(),
                                label: role.into(),
                                active: false,
                            })
                            .collect(),
                        ),
                        choice_row(
                            "workspace.kind",
                            "Workspace kind",
                            "select…",
                            "Primary content shape for workspace/federation projections",
                            SettingsMutationRouteProjection::RuntimeCommand,
                            SettingsPersistenceProjection::ProjectPolicy,
                            ["code", "vault", "knowledge", "spec", "mixed", "generic"]
                                .into_iter()
                                .map(|kind| SettingsChoiceProjection {
                                    value: kind.into(),
                                    label: kind.into(),
                                    active: false,
                                })
                                .collect(),
                        ),
                    ],
                },
                SettingsTabProjection {
                    id: "updates".into(),
                    label: "Updates".into(),
                    rows: vec![
                        choice_row(
                            "updates.channel",
                            "Channel",
                            &snapshot.update_channel,
                            "Release stream used by update checks",
                            SettingsMutationRouteProjection::SharedSettings,
                            SettingsPersistenceProjection::PersistedProfile,
                            ["stable", "nightly"]
                                .into_iter()
                                .map(|value| SettingsChoiceProjection {
                                    value: value.into(),
                                    label: value.into(),
                                    active: value == snapshot.update_channel,
                                })
                                .collect(),
                        ),
                        row(
                            "updates.auto_update",
                            "Auto update",
                            if snapshot.auto_update { "on" } else { "off" },
                            "Install discovered updates between sessions",
                            SettingsMutationRouteProjection::SharedSettings,
                            SettingsPersistenceProjection::PersistedProfile,
                            SettingsEditorProjection::Toggle,
                        ),
                    ],
                },
            ],
        };

        for tab in &mut projection.tabs {
            for row in &mut tab.rows {
                row.profile = profile_row(&row.id);
                if row.profile.is_some() {
                    row.status = SettingsStatusProjection::Warning;
                }
            }
        }

        projection
    }

    pub fn render_markdown(&self) -> String {
        let mut out = String::from("## Current Harness Settings\n");
        for tab in &self.tabs {
            out.push_str("\n### ");
            out.push_str(&tab.label);
            out.push('\n');
            for row in &tab.rows {
                out.push_str("\n- **");
                out.push_str(&row.label);
                out.push_str("**: ");
                out.push_str(&row.value);
                if let Some(profile) = &row.profile {
                    out.push_str(" — profile: ");
                    out.push_str(&profile.profile_value);
                    out.push_str(" · live override");
                }
                if !row.description.is_empty() {
                    out.push_str(" — ");
                    out.push_str(&row.description);
                }
            }
            out.push('\n');
        }
        out
    }
}

fn row(
    id: &str,
    label: &str,
    value: &str,
    description: &str,
    route: SettingsMutationRouteProjection,
    persistence: SettingsPersistenceProjection,
    editor: SettingsEditorProjection,
) -> SettingsRowProjection {
    SettingsRowProjection {
        id: id.into(),
        label: label.into(),
        value: value.into(),
        description: description.into(),
        route,
        persistence,
        profile: None,
        editor,
        status: SettingsStatusProjection::Normal,
        choices: vec![],
    }
}

fn choice_row(
    id: &str,
    label: &str,
    value: &str,
    description: &str,
    route: SettingsMutationRouteProjection,
    persistence: SettingsPersistenceProjection,
    choices: Vec<SettingsChoiceProjection>,
) -> SettingsRowProjection {
    SettingsRowProjection {
        editor: SettingsEditorProjection::Choice,
        choices,
        ..row(
            id,
            label,
            value,
            description,
            route,
            persistence,
            SettingsEditorProjection::Choice,
        )
    }
}

fn settings_row_matches_drift(row_id: &str, drift: &ProfileDriftRow) -> bool {
    matches!(
        (row_id, drift.key),
        ("runtime.thinking", "thinking") | ("runtime.context_class", "requestedContextClass")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    #[test]
    fn projection_contains_settings_tabs() {
        let settings = Settings::new("test-model");
        let projection = SettingsSurfaceProjection::from_settings(&settings);

        assert!(projection.tabs.iter().any(|tab| tab.id == "runtime"));
        assert!(projection.tabs.iter().any(|tab| tab.id == "ui"));
        assert!(projection.tabs.iter().any(|tab| tab.id == "workspace"));
        assert!(projection.tabs.iter().any(|tab| tab.id == "updates"));
    }

    #[test]
    fn projection_contains_choice_metadata() {
        let settings = Settings::new("test-model");
        let projection = SettingsSurfaceProjection::from_settings(&settings);
        let runtime = projection
            .tabs
            .iter()
            .find(|tab| tab.id == "runtime")
            .unwrap();
        let thinking = runtime
            .rows
            .iter()
            .find(|row| row.id == "runtime.thinking")
            .unwrap();

        assert_eq!(thinking.editor, SettingsEditorProjection::Choice);
        assert!(thinking.choices.iter().any(|choice| choice.active));

        let workspace = projection
            .tabs
            .iter()
            .find(|tab| tab.id == "workspace")
            .unwrap();
        let role = workspace
            .rows
            .iter()
            .find(|row| row.id == "workspace.role")
            .unwrap();
        let kind = workspace
            .rows
            .iter()
            .find(|row| row.id == "workspace.kind")
            .unwrap();
        assert_eq!(role.editor, SettingsEditorProjection::Choice);
        assert_eq!(kind.editor, SettingsEditorProjection::Choice);
        assert!(role.choices.iter().any(|choice| choice.value == "primary"));
        assert!(kind.choices.iter().any(|choice| choice.value == "code"));
    }

    #[test]
    fn projection_marks_profile_drift_on_runtime_rows() {
        let profile = crate::settings::Profile {
            thinking_level: Some("medium".into()),
            requested_context_class: Some("extended".into()),
            ..Default::default()
        };
        let mut settings = Settings {
            thinking: crate::settings::ThinkingLevel::High,
            ..Default::default()
        };
        settings.set_requested_context_class(crate::settings::ContextClass::Massive);
        let drift = ProfileDriftProjection::from_profile_and_settings(
            &profile,
            crate::settings::ProfileSource::BuiltInDefault,
            &settings,
        );

        let projection =
            SettingsSurfaceProjection::from_settings_with_profile_drift(&settings, Some(&drift));
        let runtime = projection
            .tabs
            .iter()
            .find(|tab| tab.id == "runtime")
            .unwrap();
        let thinking = runtime
            .rows
            .iter()
            .find(|row| row.id == "runtime.thinking")
            .unwrap();
        let context = runtime
            .rows
            .iter()
            .find(|row| row.id == "runtime.context_class")
            .unwrap();

        assert_eq!(thinking.status, SettingsStatusProjection::Warning);
        assert_eq!(thinking.profile.as_ref().unwrap().profile_value, "medium");
        assert_eq!(context.profile.as_ref().unwrap().profile_value, "extended");
        assert!(
            runtime
                .rows
                .iter()
                .find(|row| row.id == "runtime.model")
                .unwrap()
                .profile
                .is_none()
        );
    }

    #[test]
    fn markdown_renders_projection_rows() {
        let settings = Settings::new("test-model");
        let markdown = SettingsSurfaceProjection::from_settings(&settings).render_markdown();

        assert!(markdown.contains("Current Harness Settings"));
        assert!(markdown.contains("**Model**: test-model"));
        assert!(markdown.contains("### UI"));
        assert!(markdown.contains("**Tool display**"));
    }
}
