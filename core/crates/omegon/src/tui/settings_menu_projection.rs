//! Pure semantic projection builder for the universal settings menu.

pub(super) struct SettingsMenuInputs {
    pub(super) settings: crate::surfaces::settings::SettingsSurfaceProjection,
    pub(super) profile_drift: crate::surfaces::profile::ProfileDriftProjection,
}

impl SettingsMenuInputs {
    pub(super) fn new(
        settings: crate::surfaces::settings::SettingsSurfaceProjection,
        profile_drift: crate::surfaces::profile::ProfileDriftProjection,
    ) -> Self {
        Self {
            settings,
            profile_drift,
        }
    }
}

pub(super) fn settings_profile_source_line(
    source: &crate::surfaces::profile::ProfileSourceProjection,
) -> String {
    match source.kind {
        crate::surfaces::profile::ProfileSourceKind::Project => format!(
            "profile: project · file: {}",
            source
                .path
                .as_deref()
                .map_or("unknown".into(), |path| path.display().to_string())
        ),
        crate::surfaces::profile::ProfileSourceKind::User => format!(
            "profile: user · file: {}",
            source
                .path
                .as_deref()
                .map_or("unknown".into(), |path| path.display().to_string())
        ),
        crate::surfaces::profile::ProfileSourceKind::BuiltInDefault => {
            "profile: built-in defaults".to_string()
        }
    }
}

pub(super) fn build_settings_menu_projection(
    inputs: SettingsMenuInputs,
) -> crate::surfaces::menu::MenuProjection {
    use crate::surfaces::menu::{
        MenuActionProjection, MenuBadgeProjection, MenuBadgeTone, MenuGroupProjection,
        MenuProjection, MenuRowKind, MenuRowProjection, MenuTabProjection,
    };
    use crate::surfaces::settings::{SettingsEditorProjection, SettingsStatusProjection};
    let SettingsMenuInputs {
        settings,
        profile_drift,
    } = inputs;
    let profile_source_line = settings_profile_source_line(&profile_drift.source);
    let drift_line = if profile_drift.changed_count > 0 {
        format!(
            "runtime drift: Δ{} · /profile save or /profile apply · {profile_source_line}",
            profile_drift.changed_count
        )
    } else {
        format!("runtime drift: clean · {profile_source_line}")
    };
    let mut menu = MenuProjection::new("settings", "Settings");
    menu.summary = Some(format!(
        "Universal configuration entrypoint for runtime and capability settings. Enter opens or edits the selected area.\n{drift_line}"
    ));
    menu.footer = Some("↑/↓ navigate · Tab switch tabs · / filter · Enter open/edit · s save · a apply · n save named · Esc close".into());
    let configuration_rows = [
        (
            "runtime",
            "Runtime",
            "Edit runtime, workspace, and inference defaults here.",
            "/settings runtime",
        ),
        (
            "model",
            "Model & inference",
            "Select model routes, providers, grades, and routing policy.",
            "/model",
        ),
        (
            "auth",
            "Authentication",
            "Manage provider credentials, login state, and vault unlock.",
            "/auth",
        ),
        (
            "skills",
            "Skills",
            "Install, inspect, refresh, and remove operator skills.",
            "/skills",
        ),
        (
            "extensions",
            "Extensions",
            "Install, enable, disable, update, and inspect extensions.",
            "/extension",
        ),
        (
            "ui",
            "UI & presentation",
            "Configure Om, Active, Full, and individual interface surfaces.",
            "/ui",
        ),
        (
            "context",
            "Context",
            "Configure context class and manage context lifecycle.",
            "/context",
        ),
        (
            "memory",
            "Memory",
            "Inspect memory configuration and current memory state.",
            "/memory",
        ),
        (
            "profile",
            "Profiles",
            "Inspect, apply, and persist project or user defaults.",
            "/profile",
        ),
        (
            "secrets",
            "Secrets",
            "Manage named secrets and credential values.",
            "/secrets",
        ),
        (
            "sandbox",
            "Sandbox & permissions",
            "Configure child isolation and workspace access policy.",
            "/sandbox",
        ),
        (
            "updates",
            "Updates",
            "Configure the release channel and install available updates.",
            "/update",
        ),
    ]
    .into_iter()
    .map(|(id, label, description, command)| MenuRowProjection {
        id: format!("settings.area.{id}"),
        label: label.into(),
        description: description.into(),
        value: Some(command.into()),
        kind: MenuRowKind::Action,
        badges: Vec::new(),
        metadata: vec!["configuration area".into(), command.into()],
        primary_action: Some(MenuActionProjection::command(
            format!("settings.area.{id}.open"),
            "Open",
            command,
        )),
        actions: Vec::new(),
        safety: None,
        availability: None,
    })
    .collect();
    let configuration_tab = MenuTabProjection {
        id: "configuration".into(),
        label: "Configuration".into(),
        groups: vec![MenuGroupProjection {
            id: "settings.configuration".into(),
            label: "Configuration areas".into(),
            description: Some(
                "Canonical entrances for every operator-configurable capability.".into(),
            ),
            rows: configuration_rows,
        }],
    };
    menu.tabs
        .extend(settings.tabs.into_iter().map(|tab| MenuTabProjection {
            id: tab.id.clone(),
            label: tab.label.clone(),
            groups: vec![MenuGroupProjection {
                    id: format!("settings.{}", tab.id),
                    label: tab.label,
                    description: None,
                    rows: tab
                        .rows
                        .into_iter()
                        .map(|row| {
                            let tone = match row.status {
                                SettingsStatusProjection::Normal => MenuBadgeTone::Neutral,
                                SettingsStatusProjection::Warning => MenuBadgeTone::Warning,
                                SettingsStatusProjection::Error => MenuBadgeTone::Danger,
                                SettingsStatusProjection::Disabled => MenuBadgeTone::Info,
                            };
                            let editor = match row.editor {
                                SettingsEditorProjection::Choice => "choice",
                                SettingsEditorProjection::Toggle => "toggle",
                                SettingsEditorProjection::Text => "text",
                                SettingsEditorProjection::Number => "number",
                                SettingsEditorProjection::Action => "action",
                                SettingsEditorProjection::ReadOnly => "read only",
                            };
                            let mut metadata =
                                vec![row.persistence.label().to_string(), editor.to_string()];
                            if let Some(profile) = row.profile {
                                metadata.push(format!("profile: {}", profile.profile_value));
                            }
                            let row_id = row.id.clone();
                            MenuRowProjection {
                                id: row.id,
                                label: row.label,
                                description: row.description,
                                value: Some(row.value),
                                kind: MenuRowKind::Object,
                                badges: vec![MenuBadgeProjection {
                                    label: format!("{:?}", row.status).to_lowercase(),
                                    tone,
                                }],
                                metadata,
                                primary_action: Some(MenuActionProjection::open_settings_row(
                                    format!("settings.{row_id}.open"),
                                    "Edit",
                                    row_id,
                                )),
                                actions: Vec::new(),
                                safety: None,
                                availability: None,
                            }
                        })
                        .collect(),
                }],
        }));
    menu.tabs.push(configuration_tab);
    menu.actions = vec![
        {
            let mut action =
                MenuActionProjection::command("settings.save", "Save profile", "/profile save");
            action.key = Some("s".into());
            action
        },
        {
            let mut action =
                MenuActionProjection::command("settings.apply", "Apply profile", "/profile apply");
            action.key = Some("a".into());
            action
        },
        {
            let mut action = MenuActionProjection::prime_editor(
                "settings.save_named_user",
                "Save as named (user)",
                "/profile save --name ",
                "Type the profile name and press Enter — saved to ~/.omegon/profiles/<name>.json",
            );
            action.key = Some("n".into());
            action
        },
        MenuActionProjection::prime_editor(
            "settings.save_named_project",
            "Save as named (project)",
            "/profile save --name ",
            "Type the profile name followed by ' --project' and press Enter — saved to .omegon/profiles/<name>.json",
        ),
    ];
    menu
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_inputs() -> SettingsMenuInputs {
        let settings = crate::settings::Settings::new("test-model");
        let profile = crate::settings::Profile::default();
        SettingsMenuInputs::new(
            crate::surfaces::settings::SettingsSurfaceProjection::from_settings(&settings),
            crate::surfaces::profile::ProfileDriftProjection::from_profile_and_settings(
                &profile,
                crate::settings::ProfileSource::BuiltInDefault,
                &settings,
            ),
        )
    }

    #[test]
    fn projection_preserves_settings_tabs_and_configuration_entrypoints() {
        let projection = build_settings_menu_projection(test_inputs());
        assert_eq!(projection.id, "settings");
        assert!(projection.tabs.iter().any(|tab| tab.id == "runtime"));
        assert!(projection.tabs.iter().any(|tab| tab.id == "configuration"));
    }

    #[test]
    fn projection_summary_reports_profile_source() {
        let projection = build_settings_menu_projection(test_inputs());
        assert!(
            projection
                .summary
                .as_deref()
                .is_some_and(|summary| summary.contains("profile: built-in defaults"))
        );
    }
}
