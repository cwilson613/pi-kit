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
    Composer,
}

#[derive(Default)]
pub(super) struct InteractionState {
    deferred: VecDeque<AgentEvent>,
    return_prompt: Option<CommandPrompt>,
    pub(super) prompt: Option<CommandPrompt>,
}

impl App {
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
            match event {
                AgentEvent::PermissionRequest { respond, .. } => {
                    if let Ok(mut slot) = respond.lock()
                        && let Some(tx) = slot.take()
                    {
                        let _ = tx.send(omegon_traits::PermissionResponse::Deny);
                    }
                }
                AgentEvent::OperatorWaitRequest { respond, .. } => {
                    if let Ok(mut slot) = respond.lock()
                        && let Some(tx) = slot.take()
                    {
                        let _ = tx.send(omegon_traits::OperatorWaitResponse::Cancelled);
                    }
                }
                _ => unreachable!(),
            }
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
        self.command_prompt = self.interaction.return_prompt.take();
        if let Some(next) = self.interaction.deferred.pop_front() {
            self.handle_agent_event(next);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
