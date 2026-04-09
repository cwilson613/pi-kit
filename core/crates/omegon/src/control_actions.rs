use crate::tui::canonical_slash_command;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlRole {
    Read,
    Edit,
    Admin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlIngress {
    Slash,
    Cli,
    Ipc,
    WebDaemon,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalAction {
    ContextView,
    ContextCompact,
    ContextClear,
    ContextRequest,
    ContextSetClass,
    SkillsView,
    SkillsInstall,
    ModelView,
    ModelList,
    ModelSetSameProvider,
    ProviderSwitch,
    ThinkingSet,
    SessionNew,
    SessionList,
    TurnCancel,
    RuntimeShutdown,
    PromptSubmit,
    AuthStatus,
    AuthLogin,
    AuthLogout,
    AuthUnlock,
    SecretsView,
    SecretsSet,
    SecretsGet,
    SecretsDelete,
    StatusView,
    PluginView,
    PluginInstall,
    PluginRemove,
    PluginUpdate,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassifiedAction {
    pub ingress: ControlIngress,
    pub action: CanonicalAction,
    pub role: ControlRole,
}

pub fn is_role_sufficient(actual: ControlRole, required: ControlRole) -> bool {
    role_rank(actual) >= role_rank(required)
}

fn role_rank(role: ControlRole) -> u8 {
    match role {
        ControlRole::Read => 0,
        ControlRole::Edit => 1,
        ControlRole::Admin => 2,
    }
}

pub fn classify_ipc_method(method: &str) -> ClassifiedAction {
    let (action, role) = match method {
        "get_state" | "get_graph" | "subscribe" | "unsubscribe" => {
            (CanonicalAction::StatusView, ControlRole::Read)
        }
        "submit_prompt" => (CanonicalAction::PromptSubmit, ControlRole::Edit),
        "cancel" => (CanonicalAction::TurnCancel, ControlRole::Edit),
        "run_slash_command" => (CanonicalAction::Unknown, ControlRole::Edit),
        "shutdown" => (CanonicalAction::RuntimeShutdown, ControlRole::Admin),
        _ => (CanonicalAction::Unknown, ControlRole::Admin),
    };
    ClassifiedAction {
        ingress: ControlIngress::Ipc,
        action,
        role,
    }
}

pub fn classify_daemon_trigger(trigger_kind: &str) -> ClassifiedAction {
    let (action, role) = match trigger_kind {
        "prompt" => (CanonicalAction::PromptSubmit, ControlRole::Edit),
        "cancel" => (CanonicalAction::TurnCancel, ControlRole::Edit),
        "new-session" => (CanonicalAction::SessionNew, ControlRole::Edit),
        "shutdown" => (CanonicalAction::RuntimeShutdown, ControlRole::Admin),
        "slash-command" => (CanonicalAction::Unknown, ControlRole::Edit),
        "cancel-cleave-child" => (CanonicalAction::Unknown, ControlRole::Edit),
        _ => (CanonicalAction::Unknown, ControlRole::Admin),
    };
    ClassifiedAction {
        ingress: ControlIngress::WebDaemon,
        action,
        role,
    }
}

pub fn classify_slash_command(name: &str, args: &str) -> ClassifiedAction {
    let classified = match name {
        "skills" => match args.trim() {
            "" | "list" => (CanonicalAction::SkillsView, ControlRole::Read),
            "install" => (CanonicalAction::SkillsInstall, ControlRole::Edit),
            _ => (CanonicalAction::Unknown, ControlRole::Admin),
        },
        "model" => {
            let trimmed = args.trim();
            if trimmed.is_empty() {
                (CanonicalAction::ModelView, ControlRole::Read)
            } else if trimmed == "list" {
                (CanonicalAction::ModelList, ControlRole::Read)
            } else {
                let requested_provider = trimmed.split(':').next().unwrap_or("");
                let current_provider = infer_provider_from_args_or_default(trimmed);
                if requested_provider == current_provider {
                    (CanonicalAction::ModelSetSameProvider, ControlRole::Edit)
                } else {
                    (CanonicalAction::ProviderSwitch, ControlRole::Admin)
                }
            }
        }
        "think" => (CanonicalAction::ThinkingSet, ControlRole::Edit),
        "context" => match canonical_slash_command("context", args) {
            Some(crate::tui::CanonicalSlashCommand::ContextStatus) | None if args.trim().is_empty() => {
                (CanonicalAction::ContextView, ControlRole::Read)
            }
            Some(crate::tui::CanonicalSlashCommand::ContextStatus) => {
                (CanonicalAction::ContextView, ControlRole::Read)
            }
            Some(crate::tui::CanonicalSlashCommand::ContextCompact) => {
                (CanonicalAction::ContextCompact, ControlRole::Edit)
            }
            Some(crate::tui::CanonicalSlashCommand::ContextClear) => {
                (CanonicalAction::ContextClear, ControlRole::Edit)
            }
            Some(crate::tui::CanonicalSlashCommand::ContextRequest { .. })
            | Some(crate::tui::CanonicalSlashCommand::ContextRequestJson(_)) => {
                (CanonicalAction::ContextRequest, ControlRole::Edit)
            }
            Some(crate::tui::CanonicalSlashCommand::SetContextClass(_)) => {
                (CanonicalAction::ContextSetClass, ControlRole::Edit)
            }
            _ => (CanonicalAction::Unknown, ControlRole::Admin),
        },
        "new" => (CanonicalAction::SessionNew, ControlRole::Edit),
        "sessions" => (CanonicalAction::SessionList, ControlRole::Read),
        "auth" => match canonical_slash_command("auth", args) {
            Some(crate::tui::CanonicalSlashCommand::AuthStatus) => {
                (CanonicalAction::AuthStatus, ControlRole::Read)
            }
            Some(crate::tui::CanonicalSlashCommand::AuthUnlock) => {
                (CanonicalAction::AuthUnlock, ControlRole::Admin)
            }
            _ => (CanonicalAction::Unknown, ControlRole::Admin),
        },
        "login" => (CanonicalAction::AuthLogin, ControlRole::Admin),
        "logout" => (CanonicalAction::AuthLogout, ControlRole::Admin),
        "secrets" => match args.trim().split_whitespace().next().unwrap_or("") {
            "" | "list" => (CanonicalAction::SecretsView, ControlRole::Edit),
            "set" => (CanonicalAction::SecretsSet, ControlRole::Edit),
            "get" => (CanonicalAction::SecretsGet, ControlRole::Edit),
            "delete" => (CanonicalAction::SecretsDelete, ControlRole::Edit),
            _ => (CanonicalAction::Unknown, ControlRole::Admin),
        },
        "status" | "stats" | "auspex" | "dash" => (CanonicalAction::StatusView, ControlRole::Read),
        "plugin" => match args.trim().split_whitespace().next().unwrap_or("") {
            "" | "list" => (CanonicalAction::PluginView, ControlRole::Read),
            "install" => (CanonicalAction::PluginInstall, ControlRole::Edit),
            "remove" => (CanonicalAction::PluginRemove, ControlRole::Edit),
            "update" => (CanonicalAction::PluginUpdate, ControlRole::Edit),
            _ => (CanonicalAction::Unknown, ControlRole::Admin),
        },
        _ => (CanonicalAction::Unknown, ControlRole::Admin),
    };

    ClassifiedAction {
        ingress: ControlIngress::Slash,
        action: classified.0,
        role: classified.1,
    }
}

fn infer_provider_from_args_or_default(raw: &str) -> &str {
    raw.split(':').next().unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_context_view_as_read() {
        let action = classify_slash_command("context", "");
        assert_eq!(action.action, CanonicalAction::ContextView);
        assert_eq!(action.role, ControlRole::Read);
    }

    #[test]
    fn classifies_context_compact_as_edit() {
        let action = classify_slash_command("context", "compact");
        assert_eq!(action.action, CanonicalAction::ContextCompact);
        assert_eq!(action.role, ControlRole::Edit);
    }

    #[test]
    fn classifies_skills_view_as_read() {
        let action = classify_slash_command("skills", "");
        assert_eq!(action.action, CanonicalAction::SkillsView);
        assert_eq!(action.role, ControlRole::Read);
    }

    #[test]
    fn classifies_skills_install_as_edit() {
        let action = classify_slash_command("skills", "install");
        assert_eq!(action.action, CanonicalAction::SkillsInstall);
        assert_eq!(action.role, ControlRole::Edit);
    }

    #[test]
    fn classifies_auth_login_as_admin() {
        let action = classify_slash_command("login", "anthropic");
        assert_eq!(action.action, CanonicalAction::AuthLogin);
        assert_eq!(action.role, ControlRole::Admin);
    }

    #[test]
    fn classifies_ipc_shutdown_as_admin() {
        let action = classify_ipc_method("shutdown");
        assert_eq!(action.action, CanonicalAction::RuntimeShutdown);
        assert_eq!(action.role, ControlRole::Admin);
    }

    #[test]
    fn classifies_daemon_new_session_as_edit() {
        let action = classify_daemon_trigger("new-session");
        assert_eq!(action.action, CanonicalAction::SessionNew);
        assert_eq!(action.role, ControlRole::Edit);
    }
}
