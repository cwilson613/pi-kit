//! Native Ratatui adapter for semantic frontend actions.
//!
//! This module maps the shared `UiAction` vocabulary onto local TUI state and
//! coordinator commands. Layout, rendering, and frontend-local navigation stay
//! in their owning Ratatui modules.

use super::*;

impl App {
    pub(super) async fn handle_ui_action(
        &mut self,
        action: UiAction,
        command_tx: &mpsc::Sender<TuiCommand>,
    ) -> UiActionOutcome {
        match action {
            UiAction::SubmitPrompt(action) => {
                self.submit_prefixed_prompt(action.text, action.attachments, command_tx)
                    .await;
                UiActionOutcome::accepted()
            }
            UiAction::SubmitContinuation => {
                self.awaiting_continuation = false;
                self.editor
                    .textarea
                    .set_placeholder_text("Ask anything, or type / for commands");
                self.submit_prefixed_prompt(
                    "Continue with the plan as described.".to_string(),
                    vec![],
                    command_tx,
                )
                .await;
                UiActionOutcome::accepted()
            }
            UiAction::CancelActiveTurn => {
                self.prepare_interrupt_ui();
                let _ = command_tx
                    .send(TuiCommand::CancelActiveTurn {
                        submitted_by: "local-tui".to_string(),
                        via: "tui",
                    })
                    .await;
                UiActionOutcome::accepted_message("active turn cancellation requested")
            }
            UiAction::RespondToPermission(action) => self.handle_permission_action(action),
            UiAction::RespondToOperatorWait(action) => self.handle_operator_wait_action(action),
            UiAction::RunSlashCommand(action) => {
                match self.handle_slash_command(&action.raw, command_tx) {
                    SlashResult::Display(response) => {
                        self.history.push(action.raw.clone());
                        self.exit_history_recall();
                        // Route command feedback through the same semantic surface policy as
                        // keyboard-submitted slash commands. In particular, hidden secret
                        // entry is an editor mode with a compact hint, not a command modal.
                        self.show_slash_response(&action.raw, &response);
                        UiActionOutcome::accepted_message(response)
                    }
                    SlashResult::Handled => {
                        self.history.push(action.raw);
                        self.exit_history_recall();
                        UiActionOutcome::accepted()
                    }
                    SlashResult::Quit => {
                        self.history.push(action.raw);
                        self.exit_history_recall();
                        self.should_quit = true;
                        let _ = command_tx.send(TuiCommand::Quit).await;
                        UiActionOutcome::accepted_message("quit requested")
                    }
                    SlashResult::NotACommand => UiActionOutcome::rejected("not a slash command"),
                }
            }
            UiAction::SetUiPreset(action) => self.handle_ui_preset_action(action),
            UiAction::SetSurfaceVisible(action) => self.handle_surface_visible_action(action),
            UiAction::SelectConversationSegment(action) => {
                self.handle_select_conversation_segment_action(action)
            }
            UiAction::OpenConversationSegmentDetail(action) => {
                self.handle_open_conversation_segment_detail_action(action)
            }
            UiAction::ReplaceComposerDraft(action) => {
                self.handle_replace_composer_draft_action(action)
            }
            UiAction::ClearComposerDraft => self.handle_clear_composer_draft_action(),
            UiAction::AttachComposerPath(action) => self.handle_attach_composer_path_action(action),
            UiAction::MoveComposerCursor(action) => self.handle_move_composer_cursor_action(action),
            UiAction::EditComposer(action) => self.handle_edit_composer_action(action),
            UiAction::InsertComposerText(action) => self.handle_insert_composer_text_action(action),
            UiAction::CopyConversationSegment(action) => {
                self.handle_copy_conversation_segment_action(action)
            }
            UiAction::CopyLatestAssistantResponse(action) => {
                self.handle_copy_latest_assistant_response_action(action)
            }
        }
    }

    pub(super) fn handle_replace_composer_draft_action(
        &mut self,
        action: ReplaceComposerDraftAction,
    ) -> UiActionOutcome {
        self.editor.set_text(&action.text);
        UiActionOutcome::accepted_message("composer draft replaced")
    }

    pub(super) fn handle_clear_composer_draft_action(&mut self) -> UiActionOutcome {
        if self.editor.is_empty() {
            return UiActionOutcome::noop("composer draft already empty");
        }
        self.editor.clear_line();
        UiActionOutcome::accepted_message("composer draft cleared")
    }

    pub(super) fn handle_attach_composer_path_action(
        &mut self,
        action: AttachComposerPathAction,
    ) -> UiActionOutcome {
        self.editor.insert_attachment(action.path.clone());
        UiActionOutcome::accepted_message(format!(
            "composer attachment inserted: {}",
            action.path.display()
        ))
    }

    pub(super) fn handle_move_composer_cursor_action(
        &mut self,
        action: MoveComposerCursorAction,
    ) -> UiActionOutcome {
        match (action.direction, action.unit) {
            (ComposerCursorDirection::Backward, ComposerCursorUnit::Character) => {
                self.editor.move_left();
            }
            (ComposerCursorDirection::Forward, ComposerCursorUnit::Character) => {
                self.editor.move_right();
            }
            (ComposerCursorDirection::Backward, ComposerCursorUnit::Word) => {
                self.editor.move_word_backward();
            }
            (ComposerCursorDirection::Forward, ComposerCursorUnit::Word) => {
                self.editor.move_word_forward();
            }
            (ComposerCursorDirection::Home, ComposerCursorUnit::Line) => {
                self.editor.move_home();
            }
            (ComposerCursorDirection::End, ComposerCursorUnit::Line) => {
                self.editor.move_end();
            }
            _ => return UiActionOutcome::rejected("unsupported composer cursor movement"),
        }
        UiActionOutcome::accepted_message("composer cursor moved")
    }

    pub(super) fn handle_edit_composer_action(
        &mut self,
        action: EditComposerAction,
    ) -> UiActionOutcome {
        match action.operation {
            ComposerEditOperation::DeleteBackward => self.editor.backspace(),
            ComposerEditOperation::DeleteWordBackward => self.editor.delete_word_backward(),
            ComposerEditOperation::DeleteWordForward => self.editor.delete_word_forward(),
            ComposerEditOperation::ClearLine => self.editor.clear_line(),
            ComposerEditOperation::KillToEnd => self.editor.kill_to_end(),
            ComposerEditOperation::InsertNewline => self.editor.insert_newline(),
        }
        self.exit_history_recall();
        UiActionOutcome::accepted_message("composer edited")
    }

    pub(super) fn handle_insert_composer_text_action(
        &mut self,
        action: InsertComposerTextAction,
    ) -> UiActionOutcome {
        self.editor.insert_paste(&action.text);
        self.exit_history_recall();
        UiActionOutcome::accepted_message("composer text inserted")
    }

    pub(super) fn handle_select_conversation_segment_action(
        &mut self,
        action: SelectConversationSegmentAction,
    ) -> UiActionOutcome {
        let idx = action.segment.index;
        let Some(segment) = self.conversation.segments().get(idx) else {
            return UiActionOutcome::rejected(format!(
                "conversation segment index out of range: {idx}"
            ));
        };
        if !segment.capabilities().selectable {
            return UiActionOutcome::rejected(format!(
                "conversation segment is not selectable: {idx}"
            ));
        }
        self.conversation.select_segment(idx);
        UiActionOutcome::accepted_message(format!("conversation segment selected: {idx}"))
    }

    pub(super) fn handle_open_conversation_segment_detail_action(
        &mut self,
        action: OpenConversationSegmentDetailAction,
    ) -> UiActionOutcome {
        let idx = action.segment.index;
        let Some(segment) = self.conversation.segments().get(idx) else {
            return UiActionOutcome::rejected(format!(
                "conversation segment index out of range: {idx}"
            ));
        };
        if !segment.capabilities().detail_openable {
            return UiActionOutcome::rejected(format!(
                "conversation segment detail is not openable: {idx}"
            ));
        }
        self.conversation.toggle_timeline_expanded_segment(idx);
        UiActionOutcome::accepted_message(format!("conversation segment detail toggled: {idx}"))
    }

    pub(super) fn segment_export_mode(mode: SegmentCopyMode) -> SegmentExportMode {
        match mode {
            SegmentCopyMode::Raw => SegmentExportMode::Raw,
            SegmentCopyMode::Plaintext => SegmentExportMode::Plaintext,
        }
    }

    pub(super) fn segment_copy_mode(mode: SegmentExportMode) -> SegmentCopyMode {
        match mode {
            SegmentExportMode::Raw => SegmentCopyMode::Raw,
            SegmentExportMode::Plaintext => SegmentCopyMode::Plaintext,
        }
    }

    pub(super) fn handle_copy_conversation_segment_action(
        &mut self,
        action: CopyConversationSegmentAction,
    ) -> UiActionOutcome {
        let idx = action.segment.index;
        let Some(segment) = self.conversation.segments().get(idx) else {
            return UiActionOutcome::rejected(format!(
                "conversation segment index out of range: {idx}"
            ));
        };
        let text = match Self::segment_export_mode(action.mode) {
            SegmentExportMode::Raw => segment
                .export_text(SegmentExportMode::Raw)
                .trim_end()
                .to_string(),
            SegmentExportMode::Plaintext => segment.human_plaintext_detail(),
        };
        if text.trim().is_empty() {
            return UiActionOutcome::rejected(format!(
                "conversation segment has no copyable text: {idx}"
            ));
        }
        if self.copy_text_to_clipboard(&text) {
            UiActionOutcome::accepted_message(format!("conversation segment copied: {idx}"))
        } else {
            UiActionOutcome::rejected(
                "clipboard unavailable — install pbcopy, wl-copy, xclip, or xsel",
            )
        }
    }

    pub(super) fn handle_copy_latest_assistant_response_action(
        &mut self,
        action: CopyLatestAssistantResponseAction,
    ) -> UiActionOutcome {
        let mode = Self::segment_export_mode(action.mode);
        let Some(text) = self.conversation.latest_assistant_text_with_mode(mode) else {
            return UiActionOutcome::rejected("no assistant response to copy");
        };
        if self.copy_text_to_clipboard(&text) {
            UiActionOutcome::accepted_message("latest assistant response copied")
        } else {
            UiActionOutcome::rejected(
                "clipboard unavailable — select text in your terminal or install pbcopy/wl-copy/xclip",
            )
        }
    }

    pub(super) fn handle_ui_preset_action(&mut self, action: SetUiPresetAction) -> UiActionOutcome {
        let name = action.level.name();
        self.apply_ui_presentation(UiPresentationPolicy::named(action.level));
        UiActionOutcome::accepted_message(format!("UI → {name}"))
    }

    pub(super) fn handle_surface_visible_action(
        &mut self,
        action: SetSurfaceVisibleAction,
    ) -> UiActionOutcome {
        self.toggle_ui_surface(action.surface, action.visible);
        UiActionOutcome::accepted_message(format!(
            "UI surface {}: {}",
            if action.visible {
                "enabled"
            } else {
                "disabled"
            },
            action.surface.label()
        ))
    }

    pub(super) fn handle_permission_action(&mut self, action: PermissionAction) -> UiActionOutcome {
        if self.pending_permission.is_none() {
            return UiActionOutcome::noop("no pending permission request");
        }
        let context = self.pending_permission_context.take();
        self.command_prompt = None;
        if let Some(respond) = self.pending_permission.take()
            && let Ok(mut slot) = respond.lock()
            && let Some(tx) = slot.take()
        {
            let _ = tx.send(action.response);
        }
        let label = match action.response {
            omegon_traits::PermissionResponse::Allow => "allowed for this operation",
            omegon_traits::PermissionResponse::AllowSession => "allowed - session directory grant",
            omegon_traits::PermissionResponse::AlwaysAllow => {
                match context.as_ref().map(|ctx| ctx.persistence) {
                    Some(omegon_traits::PermissionPersistence::ProjectDirectory) => {
                        "always allowed - saved directory grant"
                    }
                    Some(omegon_traits::PermissionPersistence::SessionDirectory) => {
                        "always allowed - session directory grant"
                    }
                    _ => "allowed for this operation",
                }
            }
            omegon_traits::PermissionResponse::Deny => "denied",
        };
        let message = if let Some(context) = context {
            if matches!(
                action.response,
                omegon_traits::PermissionResponse::AllowSession
                    | omegon_traits::PermissionResponse::AlwaysAllow
            ) {
                if let Some(grant_path) = context.grant_path {
                    let target = crate::tools::canonicalize_existing_parent_for_permissions(
                        std::path::Path::new(&context.target),
                    );
                    let grant = crate::tools::canonicalize_existing_parent_for_permissions(
                        std::path::Path::new(&grant_path),
                    );
                    format!(
                        "→ {label}: {} {} (canonical grant: {})",
                        context.tool_name,
                        target.display(),
                        grant.display()
                    )
                } else {
                    format!("→ {label}: {} {}", context.tool_name, context.target)
                }
            } else {
                format!("→ {label}: {} {}", context.tool_name, context.target)
            }
        } else {
            format!("→ {label}")
        };
        self.conversation.push_system(&message);
        UiActionOutcome::accepted_message(message)
    }

    pub(super) fn handle_operator_wait_action(
        &mut self,
        action: OperatorWaitAction,
    ) -> UiActionOutcome {
        if self.pending_operator_wait.is_none() {
            return UiActionOutcome::noop("no pending operator wait request");
        }
        let context = self.pending_operator_wait_context.take();
        self.command_prompt = None;
        if let Some(respond) = self.pending_operator_wait.take()
            && let Ok(mut slot) = respond.lock()
            && let Some(tx) = slot.take()
        {
            let _ = tx.send(action.response);
        }
        let label = match action.response {
            omegon_traits::OperatorWaitResponse::Completed => "manual action completed",
            omegon_traits::OperatorWaitResponse::Cancelled => "manual action cancelled",
        };
        let message = if let Some(prompt) = context {
            format!("-> {label}: {prompt}")
        } else {
            format!("-> {label}")
        };
        self.conversation.push_system(&message);
        UiActionOutcome::accepted_message(message)
    }
}
