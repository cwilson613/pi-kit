//! Renderer-local effects derived from semantic menu actions.
//!
//! This is the boundary between renderer-neutral `MenuActionProjection` values
//! and mutations performed by the native TUI `App` adapter.

use crate::surfaces::menu::{MenuActionClosePolicy, MenuActionDisposition, MenuActionProjection};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum SelectorTarget {
    ContextClass,
    CurrentModel,
    ModelGrade,
    ModelProvider,
    ModelPolicy,
    SecretName,
    Unknown(Option<String>),
}

impl SelectorTarget {
    fn from_id(selector_id: Option<String>) -> Self {
        match selector_id.as_deref() {
            Some("context.class") => Self::ContextClass,
            Some("model.current") => Self::CurrentModel,
            Some("model.grade") => Self::ModelGrade,
            Some("model.provider") => Self::ModelProvider,
            Some("model.policy") => Self::ModelPolicy,
            Some("secrets.name") => Self::SecretName,
            _ => Self::Unknown(selector_id),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SettingsRowAction {
    OpenModelSelector,
    OpenMaxTurnsSelector,
    ToggleSandbox,
    ToggleAutoUpdate,
    ExplainTrustedDirectories,
    ProjectedEditor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SettingsRowTarget {
    id: Option<String>,
}

impl SettingsRowTarget {
    fn from_id(id: Option<String>) -> Self {
        Self { id }
    }

    pub(super) fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    pub(super) fn action(&self) -> SettingsRowAction {
        match self.id() {
            Some("runtime.model") => SettingsRowAction::OpenModelSelector,
            Some("runtime.max_turns") => SettingsRowAction::OpenMaxTurnsSelector,
            Some("workspace.sandbox") => SettingsRowAction::ToggleSandbox,
            Some("updates.auto_update") => SettingsRowAction::ToggleAutoUpdate,
            Some("workspace.trusted_directories") => SettingsRowAction::ExplainTrustedDirectories,
            _ => SettingsRowAction::ProjectedEditor,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum MenuEffect {
    FocusRow {
        target_row_id: Option<String>,
    },
    PrimeEditor {
        text: Option<String>,
        message: Option<String>,
    },
    BeginInlineInput {
        label: String,
        command_prefix: Option<String>,
    },
    OpenSelector {
        target: SelectorTarget,
        label: String,
    },
    OpenExtensionDetail {
        target_row_id: Option<String>,
    },
    OpenSettingsRow {
        target: SettingsRowTarget,
        label: String,
    },
    RunCommand {
        command: Option<String>,
        close_policy: MenuActionClosePolicy,
    },
}

impl From<MenuActionProjection> for MenuEffect {
    fn from(action: MenuActionProjection) -> Self {
        match action.disposition {
            MenuActionDisposition::FocusRow => Self::FocusRow {
                target_row_id: action.target_row_id,
            },
            MenuActionDisposition::PrimeEditor => Self::PrimeEditor {
                text: action.editor_text,
                message: action.message,
            },
            MenuActionDisposition::InlineInput => Self::BeginInlineInput {
                label: action.label,
                command_prefix: action.editor_text,
            },
            MenuActionDisposition::OpenSelector => Self::OpenSelector {
                target: SelectorTarget::from_id(action.target_row_id),
                label: action.label,
            },
            MenuActionDisposition::OpenExtensionDetail => Self::OpenExtensionDetail {
                target_row_id: action.target_row_id,
            },
            MenuActionDisposition::OpenSettingsRow => Self::OpenSettingsRow {
                target: SettingsRowTarget::from_id(action.target_row_id),
                label: action.label,
            },
            MenuActionDisposition::RunCommand => Self::RunCommand {
                command: action.command,
                close_policy: action.close_policy,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surfaces::menu::MenuActionProjection;

    #[test]
    fn prime_editor_projection_becomes_explicit_tui_effect() {
        let action = MenuActionProjection::prime_editor(
            "profile.save",
            "Save profile",
            "/profile save --name ",
            "Type a profile name",
        );

        assert_eq!(
            MenuEffect::from(action),
            MenuEffect::PrimeEditor {
                text: Some("/profile save --name ".into()),
                message: Some("Type a profile name".into()),
            }
        );
    }

    #[test]
    fn command_effect_preserves_refresh_policy() {
        let mut action = MenuActionProjection::command("ui.toggle", "Toggle", "/ui dashboard off");
        action.close_policy = MenuActionClosePolicy::RefreshMenu;

        assert_eq!(
            MenuEffect::from(action),
            MenuEffect::RunCommand {
                command: Some("/ui dashboard off".into()),
                close_policy: MenuActionClosePolicy::RefreshMenu,
            }
        );
    }

    #[test]
    fn selector_effect_preserves_target_and_operator_label() {
        let action = MenuActionProjection::open_selector(
            "model.provider.open",
            "Choose provider",
            "model.provider",
        );

        assert_eq!(
            MenuEffect::from(action),
            MenuEffect::OpenSelector {
                target: SelectorTarget::ModelProvider,
                label: "Choose provider".into(),
            }
        );
    }

    #[test]
    fn unknown_selector_target_remains_diagnostic() {
        let action = MenuActionProjection::open_selector(
            "future.selector.open",
            "Choose future value",
            "future.selector",
        );

        assert_eq!(
            MenuEffect::from(action),
            MenuEffect::OpenSelector {
                target: SelectorTarget::Unknown(Some("future.selector".into())),
                label: "Choose future value".into(),
            }
        );
    }

    #[test]
    fn settings_row_effect_types_local_actions() {
        let action = MenuActionProjection::open_settings_row(
            "settings.sandbox.open",
            "Sandbox",
            "workspace.sandbox",
        );

        let MenuEffect::OpenSettingsRow { target, label } = MenuEffect::from(action) else {
            panic!("expected settings row effect");
        };
        assert_eq!(target.id(), Some("workspace.sandbox"));
        assert_eq!(target.action(), SettingsRowAction::ToggleSandbox);
        assert_eq!(label, "Sandbox");
    }

    #[test]
    fn settings_row_effect_preserves_projected_editor_fallback() {
        let action = MenuActionProjection::open_settings_row(
            "settings.future.open",
            "Future setting",
            "future.setting",
        );

        let MenuEffect::OpenSettingsRow { target, .. } = MenuEffect::from(action) else {
            panic!("expected settings row effect");
        };
        assert_eq!(target.id(), Some("future.setting"));
        assert_eq!(target.action(), SettingsRowAction::ProjectedEditor);
    }
}
