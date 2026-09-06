//! Ownership and arrival ordering for responder-backed client decisions.
use super::*;
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BlockingOwner {
    Permission,
    OperatorWait,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NavigationOwner {
    Decision,
    ExtensionAction,
    ExtensionModal,
    Copy,
    Prompt,
    Panel,
    Tutorial,
    Selector,
    Process,
    Menu,
    Project,
    Composer,
}

#[derive(Default)]
pub(super) struct InteractionState {
    deferred: VecDeque<AgentEvent>,
    return_prompt: Option<CommandPrompt>,
    pub(super) prompt: Option<CommandPrompt>,
    pub(super) wait_call_id: Option<String>,
}

fn resolve_decision<T>(
    responder: &std::sync::Mutex<Option<std::sync::mpsc::Sender<T>>>,
    response: T,
) {
    if let Ok(mut slot) = responder.lock()
        && let Some(tx) = slot.take()
    {
        let _ = tx.send(response);
    }
}

fn decline_decision(event: &AgentEvent) {
    match event {
        AgentEvent::PermissionRequest { respond, .. } => {
            resolve_decision(respond, omegon_traits::PermissionResponse::Deny);
        }
        AgentEvent::OperatorWaitRequest { respond, .. } => {
            resolve_decision(respond, omegon_traits::OperatorWaitResponse::Cancelled);
        }
        _ => unreachable!("only blocking decisions enter the interaction queue"),
    }
}

impl App {
    pub(super) fn finish_operator_wait_call(&mut self, call_id: &str) {
        // A queued wait can expire before it is displayed/acknowledged. Remove
        // only its correlated request, preserving the current owner and order.
        self.interaction.deferred.retain(|event| {
            if matches!(event, AgentEvent::OperatorWaitRequest { call_id: Some(id), .. } if id == call_id) {
                decline_decision(event);
                false
            } else {
                true
            }
        });
        if self.blocking_owner() != Some(BlockingOwner::OperatorWait)
            || self.interaction.wait_call_id.as_deref() != Some(call_id)
        {
            return;
        }
        if let Some(respond) = self.pending_operator_wait.take() {
            resolve_decision(&respond, omegon_traits::OperatorWaitResponse::Cancelled);
        }
        self.pending_operator_wait_context = None;
        self.command_prompt = None;
        self.finish_blocking_interaction();
    }

    /// Authority has closed this turn/session. Resolve every owned responder
    /// negatively without promoting abandoned requests back into the UI.
    pub(super) fn cancel_blocking_interactions(&mut self) {
        let had_decision = self.blocking_owner().is_some() || self.interaction.prompt.is_some();
        if let Some(respond) = self.pending_permission.take() {
            resolve_decision(&respond, omegon_traits::PermissionResponse::Deny);
        }
        if let Some(respond) = self.pending_operator_wait.take() {
            resolve_decision(&respond, omegon_traits::OperatorWaitResponse::Cancelled);
        }
        for event in self.interaction.deferred.drain(..) {
            decline_decision(&event);
        }
        self.pending_permission_context = None;
        self.pending_operator_wait_context = None;
        self.interaction.prompt = None;
        self.interaction.wait_call_id = None;
        if had_decision {
            self.command_prompt = self.interaction.return_prompt.take();
        }
    }

    pub(super) fn navigation_owner(&self) -> NavigationOwner {
        if self.blocking_owner().is_some() {
            NavigationOwner::Decision
        } else if self.active_action_prompt.is_some() {
            NavigationOwner::ExtensionAction
        } else if self.active_modal.is_some() {
            NavigationOwner::ExtensionModal
        } else if self.copy_text_modal.is_some() {
            NavigationOwner::Copy
        } else if self.command_prompt.is_some() {
            NavigationOwner::Prompt
        } else if self.command_panel.is_some() {
            NavigationOwner::Panel
        } else if self
            .tutorial_overlay
            .as_ref()
            .is_some_and(|overlay| overlay.active)
        {
            NavigationOwner::Tutorial
        } else if self.selector.is_some() {
            NavigationOwner::Selector
        } else if self.process_viewer.is_some() {
            NavigationOwner::Process
        } else if self.active_menu.is_some() {
            NavigationOwner::Menu
        } else if self.project_browser.is_some() {
            NavigationOwner::Project
        } else {
            NavigationOwner::Composer
        }
    }

    pub(super) fn expire_navigation_overlay(&mut self) {
        if self
            .active_modal
            .as_ref()
            .is_some_and(|(_, _, timeout, started)| {
                timeout.is_some_and(|ms| started.elapsed().as_millis() > u128::from(ms))
            })
        {
            self.active_modal = None;
        }
    }

    pub(super) fn blocking_owner(&self) -> Option<BlockingOwner> {
        if self.pending_operator_wait.is_some() {
            Some(BlockingOwner::OperatorWait)
        } else if self.pending_permission.is_some() {
            Some(BlockingOwner::Permission)
        } else {
            None
        }
    }

    pub(super) fn defer_blocking_interaction(&mut self, event: &AgentEvent) -> bool {
        if !matches!(
            event,
            AgentEvent::PermissionRequest { .. } | AgentEvent::OperatorWaitRequest { .. }
        ) {
            return false;
        }
        if self.blocking_owner().is_none() {
            self.interaction.return_prompt = self.command_prompt.take();
            return false;
        }
        if self.interaction.deferred.len() < 64 {
            self.interaction.deferred.push_back(event.clone());
        } else {
            // Fail closed without dropping a live responder or growing an
            // unbounded queue. The producer receives an explicit negative answer.
            decline_decision(event);
            self.conversation
                .push_system("Decision queue is full; incoming request declined.");
        }
        true
    }

    pub(super) fn finish_blocking_interaction(&mut self) {
        if self.blocking_owner().is_some() {
            return;
        }
        self.interaction.prompt = None;
        self.interaction.wait_call_id = None;
        self.command_prompt = self.interaction.return_prompt.take();
        if let Some(next) = self.interaction.deferred.pop_front() {
            self.handle_agent_event(next);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn wait_request(
        call_id: Option<&str>,
    ) -> (
        AgentEvent,
        std::sync::mpsc::Receiver<omegon_traits::OperatorWaitResponse>,
    ) {
        let (tx, rx) = std::sync::mpsc::channel();
        let (ack, _) = std::sync::mpsc::channel();
        (
            AgentEvent::OperatorWaitRequest {
                call_id: call_id.map(str::to_owned),
                prompt: "manual step".into(),
                timeout_secs: 60,
                acknowledge: Arc::new(Mutex::new(Some(ack))),
                respond: Arc::new(Mutex::new(Some(tx))),
            },
            rx,
        )
    }

    fn wait_end(id: &str) -> AgentEvent {
        AgentEvent::ToolEnd {
            id: id.into(),
            name: crate::tool_registry::core::WAIT_FOR_OPERATOR.into(),
            provenance: omegon_traits::ToolProvenance::BuiltIn,
            execution_origin: omegon_traits::ToolExecutionOrigin::Agent,
            is_error: true,
            result: omegon_traits::ToolResult {
                content: vec![],
                details: serde_json::json!({"status": "timed_out"}),
            },
        }
    }

    #[test]
    fn operator_wait_tool_end_promotes_only_the_next_live_decision() {
        let mut app = super::super::tests::test_app();
        let (first, first_rx) = wait_request(Some("first"));
        let (next, next_rx) = wait_request(Some("next"));
        app.handle_agent_event(first);
        app.handle_agent_event(next);
        app.handle_agent_event(wait_end("first"));
        assert_eq!(
            first_rx.try_recv().unwrap(),
            omegon_traits::OperatorWaitResponse::Cancelled
        );
        assert_eq!(app.blocking_owner(), Some(BlockingOwner::OperatorWait));
        assert!(app.interaction.deferred.is_empty());
        // A delayed duplicate for the first wait must not dismiss its successor.
        app.handle_agent_event(wait_end("first"));
        assert_eq!(app.blocking_owner(), Some(BlockingOwner::OperatorWait));
        assert!(next_rx.try_recv().is_err());
        app.handle_agent_event(wait_end("next"));
        assert_eq!(
            next_rx.try_recv().unwrap(),
            omegon_traits::OperatorWaitResponse::Cancelled
        );
        assert_eq!(app.navigation_owner(), NavigationOwner::Composer);
    }

    #[test]
    fn operator_wait_tool_end_ignores_stale_and_uncorrelated_completion() {
        for call_id in [Some("current"), None] {
            let mut app = super::super::tests::test_app();
            let (event, rx) = wait_request(call_id);
            app.handle_agent_event(event);
            app.handle_agent_event(wait_end("previous"));
            assert_eq!(app.blocking_owner(), Some(BlockingOwner::OperatorWait));
            assert!(rx.try_recv().is_err());
        }
    }

    #[test]
    fn operator_wait_tool_end_removes_expired_queued_wait_without_disturbing_owner() {
        let mut app = super::super::tests::test_app();
        let (active, active_rx) = wait_request(Some("active"));
        let (expired, expired_rx) = wait_request(Some("expired"));
        let (next, next_rx) = wait_request(Some("next"));
        app.handle_agent_event(active);
        app.handle_agent_event(expired);
        app.handle_agent_event(next);
        app.handle_agent_event(wait_end("expired"));
        assert_eq!(app.blocking_owner(), Some(BlockingOwner::OperatorWait));
        assert!(active_rx.try_recv().is_err());
        assert_eq!(
            expired_rx.try_recv().unwrap(),
            omegon_traits::OperatorWaitResponse::Cancelled
        );
        assert_eq!(app.interaction.deferred.len(), 1);
        app.handle_operator_wait_action(OperatorWaitAction {
            request_id: None,
            response: omegon_traits::OperatorWaitResponse::Completed,
        });
        assert_eq!(
            active_rx.try_recv().unwrap(),
            omegon_traits::OperatorWaitResponse::Completed
        );
        assert!(app.interaction.deferred.is_empty());
        app.handle_agent_event(wait_end("next"));
        assert_eq!(
            next_rx.try_recv().unwrap(),
            omegon_traits::OperatorWaitResponse::Cancelled
        );
        assert_eq!(app.navigation_owner(), NavigationOwner::Composer);
    }

    #[tokio::test]
    async fn authoritative_decision_cleanup_releases_queue_and_next_submission() {
        for base in [
            TerminalPresentation::Inline,
            TerminalPresentation::Fullscreen,
        ] {
            for phase in [
                "supervisor_completed",
                "supervisor_revoked",
                "supervisor_failed",
                "supervisor_timed_out",
                "idle",
                "reset",
            ] {
                for wait_first in [false, true] {
                    let mut app = super::super::tests::test_app();
                    app.base_terminal = base;
                    app.runtime_turn_id = Some(41);
                    app.agent_active = true;
                    app.editor.set_text("next turn draft");
                    app.open_settings_menu();
                    let menu = app.active_menu.clone();
                    let (permission_tx, permission_rx) = std::sync::mpsc::channel();
                    let permission = AgentEvent::PermissionRequest {
                        tool_name: "write".into(),
                        path: "outside".into(),
                        kind: omegon_traits::PermissionRequestKind::PathBoundary,
                        persistence: omegon_traits::PermissionPersistence::ProjectDirectory,
                        grant_path: None,
                        respond: Arc::new(Mutex::new(Some(permission_tx))),
                    };
                    let (wait_tx, wait_rx) = std::sync::mpsc::channel();
                    let (ack_tx, _ack_rx) = std::sync::mpsc::channel();
                    let wait = AgentEvent::OperatorWaitRequest {
                        call_id: None,
                        prompt: "abandoned wait".into(),
                        timeout_secs: 60,
                        acknowledge: Arc::new(Mutex::new(Some(ack_tx))),
                        respond: Arc::new(Mutex::new(Some(wait_tx))),
                    };
                    let events = if wait_first {
                        [wait, permission]
                    } else {
                        [permission, wait]
                    };
                    for event in events {
                        app.handle_agent_event(event);
                    }
                    assert_eq!(app.interaction.deferred.len(), 1);
                    let event = match phase {
                        "idle" => AgentEvent::RuntimeQueueUpdated {
                            snapshot_json: serde_json::json!({"active": null, "depth": 0}),
                        },
                        "reset" => AgentEvent::SessionReset,
                        _ => AgentEvent::RuntimeTurnLifecycleUpdated {
                            snapshot_json: serde_json::json!({"phase": phase, "turn_id": 41}),
                        },
                    };
                    app.handle_agent_event(event.clone());
                    assert_eq!(
                        app.blocking_owner(),
                        None,
                        "{phase}: stale decision still owns input"
                    );
                    assert!(
                        app.interaction.deferred.is_empty(),
                        "{phase}: stale queue retained"
                    );
                    assert!(app.interaction.prompt.is_none());
                    assert!(app.pending_permission_context.is_none());
                    assert!(app.pending_operator_wait_context.is_none());
                    assert_eq!(
                        permission_rx.try_recv().unwrap(),
                        omegon_traits::PermissionResponse::Deny
                    );
                    assert_eq!(
                        wait_rx.try_recv().unwrap(),
                        omegon_traits::OperatorWaitResponse::Cancelled
                    );
                    app.handle_agent_event(event); // Idempotent authority reconciliation.
                    assert_eq!(app.editor.render_text(), "next turn draft");
                    assert_eq!(app.active_menu, menu);
                    app.active_menu = None; // Operator closes the preserved menu.
                    assert_eq!(app.navigation_owner(), NavigationOwner::Composer);
                    let (tx, mut rx) = tokio::sync::mpsc::channel(16);
                    app.handle_terminal_event(
                        Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                        &tx,
                    )
                    .await;
                    assert!(
                        matches!(rx.try_recv(), Ok(TuiCommand::SubmitPrompt(PromptSubmission { text, .. })) if text == "next turn draft"),
                        "{phase}: next prompt was not submitted"
                    );
                }
            }
        }
    }

    #[test]
    fn authoritative_decision_cleanup_ignores_advisory_and_stale_completion() {
        let mut app = super::super::tests::test_app();
        app.runtime_turn_id = Some(41);
        let (tx, rx) = std::sync::mpsc::channel();
        app.handle_agent_event(AgentEvent::PermissionRequest {
            tool_name: "write".into(),
            path: "outside".into(),
            kind: omegon_traits::PermissionRequestKind::PathBoundary,
            persistence: omegon_traits::PermissionPersistence::ProjectDirectory,
            grant_path: None,
            respond: Arc::new(Mutex::new(Some(tx))),
        });
        app.handle_agent_event(AgentEvent::RuntimeTurnLifecycleUpdated {
            snapshot_json: serde_json::json!({"phase": "supervisor_completed", "turn_id": 40}),
        });
        assert_eq!(app.blocking_owner(), Some(BlockingOwner::Permission));
        app.handle_agent_event(AgentEvent::AgentEnd);
        assert_eq!(app.blocking_owner(), Some(BlockingOwner::Permission));
        assert!(rx.try_recv().is_err());
        // Idle authority still clears decisions after advisory completion released the active gate.
        app.handle_agent_event(AgentEvent::RuntimeQueueUpdated {
            snapshot_json: serde_json::json!({"active": null, "depth": 0}),
        });
        assert_eq!(app.blocking_owner(), None);
        assert_eq!(
            rx.try_recv().unwrap(),
            omegon_traits::PermissionResponse::Deny
        );
    }

    #[test]
    fn decision_queue_is_bounded_and_overflow_resolves_negatively() {
        let mut app = super::super::tests::test_app();
        let mut receivers = Vec::new();
        for _ in 0..66 {
            let (tx, rx) = std::sync::mpsc::channel();
            receivers.push(rx);
            app.handle_agent_event(AgentEvent::PermissionRequest {
                tool_name: "write".into(),
                path: "outside".into(),
                kind: omegon_traits::PermissionRequestKind::PathBoundary,
                persistence: omegon_traits::PermissionPersistence::ProjectDirectory,
                grant_path: None,
                respond: std::sync::Arc::new(std::sync::Mutex::new(Some(tx))),
            });
        }
        assert_eq!(app.interaction.deferred.len(), 64);
        assert_eq!(
            receivers[65].try_recv().unwrap(),
            omegon_traits::PermissionResponse::Deny
        );
        assert!(receivers[0].try_recv().is_err());
        assert_eq!(app.blocking_owner(), Some(BlockingOwner::Permission));
    }
}
