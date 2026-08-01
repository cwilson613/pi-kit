//! Pure decisions used while composing the interactive runtime.
//!
//! This module deliberately owns no I/O or process-global state. Keeping these
//! transformations separate from `main.rs` makes startup wiring testable without
//! constructing providers, terminals, or an agent loop.

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InteractiveStartupModelDecision {
    pub(crate) selected_model: String,
    pub(crate) bridge_model: String,
    pub(crate) provider_connected: bool,
    pub(crate) use_null_bridge: bool,
}

pub(crate) fn decide_interactive_startup_model(
    selected_model: &str,
    resolved_model: &str,
    resolved_available: bool,
) -> InteractiveStartupModelDecision {
    InteractiveStartupModelDecision {
        selected_model: selected_model.to_string(),
        bridge_model: resolved_model.to_string(),
        provider_connected: resolved_available,
        use_null_bridge: !resolved_available,
    }
}

pub(crate) fn restart_args_for_session(
    args: impl IntoIterator<Item = String>,
    session_id: &str,
) -> Vec<String> {
    let mut filtered = Vec::new();
    let mut skip_resume_value = false;
    for arg in args {
        if skip_resume_value {
            skip_resume_value = false;
            continue;
        }
        if arg == "--fresh" {
            continue;
        }
        if arg == "--resume" {
            skip_resume_value = true;
            continue;
        }
        if arg.starts_with("--resume=") {
            continue;
        }
        filtered.push(arg);
    }
    filtered.push("--resume".to_string());
    filtered.push(session_id.to_string());
    filtered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restart_args_replace_all_session_selection_forms() {
        let args = vec![
            "omegon".into(),
            "--fresh".into(),
            "--resume=old-a".into(),
            "--resume".into(),
            "old-b".into(),
            "--model".into(),
            "test-model".into(),
        ];

        assert_eq!(
            restart_args_for_session(args, "new-session"),
            vec!["omegon", "--model", "test-model", "--resume", "new-session"]
        );
    }

    #[test]
    fn restart_args_preserve_trailing_resume_flag_without_dropping_unrelated_args() {
        assert_eq!(
            restart_args_for_session(
                vec![
                    "omegon".into(),
                    "--model".into(),
                    "test-model".into(),
                    "--resume".into()
                ],
                "new-session",
            ),
            vec!["omegon", "--model", "test-model", "--resume", "new-session"]
        );
    }

    #[test]
    fn startup_model_decision_preserves_selection_and_resolved_bridge() {
        let decision = decide_interactive_startup_model("selected", "resolved", true);
        assert_eq!(decision.selected_model, "selected");
        assert_eq!(decision.bridge_model, "resolved");
        assert!(decision.provider_connected);
        assert!(!decision.use_null_bridge);
    }

    #[test]
    fn unavailable_model_selects_null_bridge() {
        let decision = decide_interactive_startup_model("selected", "resolved", false);
        assert!(!decision.provider_connected);
        assert!(decision.use_null_bridge);
    }
}
