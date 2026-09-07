//! Small composition over the same editor, decisions, and conversation state.
use super::*;
use crate::surfaces::layout::TerminalPresentation;
use ratatui::widgets::Wrap;

pub(super) const LIVE_ROWS: u16 = 8;

impl App {
    pub(super) fn reconcile_native_publication(&mut self) {
        let mut invalid_prune = false;
        if let Some(prune) = self.conversation.take_publication_prune() {
            if self.native_publication.automatic.apply_prune(&prune) {
                self.publication_boundary = prune.rebase_boundary(self.publication_boundary);
            } else {
                invalid_prune = true;
            }
        }
        if !self.agent_active {
            self.publication_boundary = self.conversation.segments().len();
        }
        let generation = self.conversation.publication_generation();
        if generation != self.native_publication.automatic.generation() {
            let changed_at = self.conversation.take_publication_change();
            let changed_at = if invalid_prune { 0 } else { changed_at };
            self.publication_boundary = self
                .publication_boundary
                .min(self.conversation.segments().len());
            if self.native_publication.automatic.reconcile(
                generation,
                changed_at,
                self.publication_boundary,
            ) {
                self.show_toast(
                    "Conversation boundary changed; prior history remains in the transcript",
                    ratatui_toaster::ToastType::Info,
                );
            }
        }
    }

    pub(super) fn publish_inline(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
        session: &TerminalSessionHandle,
    ) -> io::Result<bool> {
        use ratatui::{backend::Backend, widgets::Widget};
        self.reconcile_native_publication();
        let generation = self.conversation.publication_generation();
        session.with_presentation_io(TerminalPresentation::Inline, || terminal.autoresize())?;
        let size = terminal.size()?;
        if size.width == 0 || size.height == 0 {
            return Ok(false);
        }
        let Some(batch) = self.native_publication.automatic.prepare(
            generation,
            self.conversation.segments(),
            self.publication_boundary,
            self.ui_presentation.level,
            size.width,
            native_publication::PreparationBudget::default(),
        ) else {
            return Ok(false);
        };
        let mut attempted = false;
        let result = session.with_presentation_io(TerminalPresentation::Inline, || {
            if !batch.lines.is_empty() {
                attempted = true;
                terminal.insert_before(batch.lines.len() as u16, |buffer| {
                    let lines = batch
                        .lines
                        .iter()
                        .map(|line| Line::from(line.as_str()))
                        .collect::<Vec<_>>();
                    Paragraph::new(lines).render(buffer.area, buffer);
                })?;
                terminal.backend_mut().flush()?;
            }
            Ok(())
        });
        let delivery = if result.is_ok() {
            native_publication::DeliveryResult::Committed
        } else if attempted {
            native_publication::DeliveryResult::Ambiguous
        } else {
            native_publication::DeliveryResult::KnownFailure
        };
        self.native_publication.automatic.settle(batch, delivery);
        Ok(true)
    }

    pub(super) fn requires_fullscreen(&self) -> bool {
        // Mounted roots retain their space while a decision covers them. Input
        // precedence still comes exclusively from navigation_owner().
        self.base_terminal == TerminalPresentation::Fullscreen
            || self.project_browser.is_some()
            || self.active_menu.is_some()
            || self.process_viewer.is_some()
            || self.selector.is_some()
            || self.at_picker.is_some()
            || self.command_panel.is_some()
            || self.command_prompt.is_some()
            || self.copy_text_modal.is_some()
            || self.active_modal.is_some()
            || self.active_action_prompt.is_some()
            || self.tutorial_overlay.as_ref().is_some_and(|v| v.active)
            || self.blocking_owner().is_some()
    }

    pub(super) fn draw_inline(&mut self, frame: &mut Frame) {
        let area = frame.area();
        let editor_rows = editor_height_for(&self.editor, area)
            .clamp(2, 4)
            .min(area.height);
        let status = if self.native_publication.automatic.is_degraded() {
            "Scrollback delivery uncertain · /session-export or fullscreen for history"
        } else if self.agent_active {
            "Working · Ctrl+C cancel · F2 Project"
        } else if self
            .native_publication
            .automatic
            .has_pending(self.publication_boundary)
        {
            "Publishing completed output · F2 Project"
        } else {
            ""
        };
        let mut lines = Vec::new();
        if !status.is_empty() {
            lines.push(Line::styled(status, self.theme.style_dim()));
        }
        // Borrow only a bounded suffix; never format the whole transcript here.
        if self.agent_active
            && let Some(segment) = self.conversation.segments().last()
        {
            let text = match &segment.content {
                SegmentContent::AssistantText { text, .. } => text.as_str(),
                SegmentContent::SystemNotification { text } => text.as_str(),
                SegmentContent::ToolCard { name, .. } => name.as_str(),
                _ => "",
            };
            let start = text.floor_char_boundary(text.len().saturating_sub(1024));
            let clean = super::native_publication::safe_inline_text(&text[start..]);
            lines.extend(
                clean
                    .lines()
                    .rev()
                    .take(3)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .map(|v| Line::from(v.to_owned())),
            );
        }
        // Keep idle input beside the transcript. The reserved viewport remains
        // available below for streaming output and contextual controls.
        let available = area.height.saturating_sub(editor_rows + 1);
        let live_rows = if self.editor.render_text().trim_start().starts_with('/') {
            // Autocomplete opens above the editor, inside the reserved viewport.
            available
        } else {
            (lines.len() as u16).min(available)
        };
        let live = Rect::new(area.x, area.y, area.width, live_rows);
        let editor = Rect::new(
            area.x,
            area.y + live_rows + u16::from(area.height > editor_rows),
            area.width,
            editor_rows,
        );
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), live);
        self.render_shared_composer(frame, editor);
        self.render_operator_event_toast(frame);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contribution_health_notice_publishes_after_initial_attachment_exactly_once() {
        use crate::contribution_health::{ContributionKind, ScopeHealth};
        let mut app = App::new(crate::settings::shared("test"));
        app.conversation
            .push_system("Session attachment already published");
        app.native_publication.automatic.attach(
            app.conversation.publication_generation(),
            app.conversation.segments().len(),
        );
        let status = crate::status::HarnessStatus {
            contribution_loading: vec![ScopeHealth::blocked(
                ContributionKind::Skills,
                "user",
                std::path::Path::new("/fixture/skills"),
                &anyhow::anyhow!("blocked fixture"),
            )],
            ..Default::default()
        };
        let event = AgentEvent::HarnessStatusChanged {
            status_json: serde_json::to_value(status).unwrap(),
        };
        app.handle_agent_event(event.clone());
        let source = app.conversation.segments();
        let batch = app
            .native_publication
            .automatic
            .prepare(
                app.conversation.publication_generation(),
                source,
                source.len(),
                UiPresentationLevel::Active,
                120,
                native_publication::PreparationBudget::default(),
            )
            .expect(
                "new contribution notice must not merge into already-published session attachment",
            );
        assert!(
            batch
                .lines
                .concat()
                .contains("1 contribution scope could not load")
        );
        assert!(
            !batch
                .lines
                .concat()
                .contains("Session attachment already published")
        );
        app.native_publication
            .automatic
            .settle(batch, native_publication::DeliveryResult::Committed);
        app.handle_agent_event(event);
        let source = app.conversation.segments();
        assert!(
            app.native_publication
                .automatic
                .prepare(
                    app.conversation.publication_generation(),
                    source,
                    source.len(),
                    UiPresentationLevel::Active,
                    120,
                    native_publication::PreparationBudget::default(),
                )
                .is_none()
        );
    }

    #[test]
    fn contribution_health_status_output_publishes_after_startup_notice() {
        let mut app = App::new(crate::settings::shared("test"));
        app.conversation
            .push_system("3 contribution scopes could not load. /status for details.");
        app.native_publication.automatic.attach(
            app.conversation.publication_generation(),
            app.conversation.segments().len(),
        );
        app.handle_agent_event(AgentEvent::SystemNotification {
            message:
                "Harness status\nskills (user) blocked [home_identity_mismatch] — /fixture/skills"
                    .into(),
        });
        let source = app.conversation.segments();
        let batch = app
            .native_publication
            .automatic
            .prepare(
                app.conversation.publication_generation(),
                source,
                source.len(),
                UiPresentationLevel::Active,
                120,
                native_publication::PreparationBudget::default(),
            )
            .expect("status response must create a new publication");
        assert!(
            batch
                .lines
                .concat()
                .contains("skills (user) blocked [home_identity_mismatch]")
        );
    }

    #[test]
    fn persistent_local_notice_publishes_without_repeating_prior_scrollback() {
        let mut app = App::new(crate::settings::shared("test"));
        app.conversation.push_system("Already published notice");
        app.native_publication.automatic.attach(
            app.conversation.publication_generation(),
            app.conversation.segments().len(),
        );
        app.conversation
            .push_system("Local command result remains visible");
        let source = app.conversation.segments();
        let batch = app
            .native_publication
            .automatic
            .prepare(
                app.conversation.publication_generation(),
                source,
                source.len(),
                UiPresentationLevel::Active,
                120,
                native_publication::PreparationBudget::default(),
            )
            .unwrap();
        let rendered = batch.lines.join("\n");
        assert!(rendered.contains("Local command result remains visible"));
        assert!(!rendered.contains("Already published notice"));
    }

    #[test]
    fn persistent_notice_rollover_publishes_new_records_without_replay() {
        let mut app = App::new(crate::settings::shared("test"));
        for index in 0..64 {
            app.conversation.push_system(&format!("old notice {index}"));
        }
        app.publication_boundary = app.conversation.segments().len();
        app.native_publication.automatic.attach(
            app.conversation.publication_generation(),
            app.publication_boundary,
        );
        for index in 65..=66 {
            app.conversation.push_system(&format!("new notice {index}"));
            app.reconcile_native_publication();
            let source = app.conversation.segments();
            let batch = app
                .native_publication
                .automatic
                .prepare(
                    app.conversation.publication_generation(),
                    source,
                    source.len(),
                    UiPresentationLevel::Active,
                    120,
                    native_publication::PreparationBudget::default(),
                )
                .unwrap();
            let text = batch.lines.join("\n");
            assert!(text.contains(&format!("new notice {index}")), "{text}");
            assert!(!text.contains("old notice"), "{text}");
            assert_eq!(source.len(), 64);
            app.native_publication
                .automatic
                .settle(batch, native_publication::DeliveryResult::Committed);
        }
    }

    #[test]
    fn persistent_notice_burst_rebases_once_and_keeps_prune_scratch_bounded() {
        let mut app = App::new(crate::settings::shared("test"));
        for index in 0..64 {
            app.conversation.push_system(&format!("old-{index}"));
        }
        app.publication_boundary = app.conversation.segments().len();
        app.native_publication.automatic.attach(
            app.conversation.publication_generation(),
            app.publication_boundary,
        );
        let attachment = app
            .native_publication
            .automatic
            .prepare(
                app.conversation.publication_generation(),
                app.conversation.segments(),
                app.publication_boundary,
                UiPresentationLevel::Active,
                120,
                native_publication::PreparationBudget::default(),
            )
            .unwrap();
        app.native_publication
            .automatic
            .settle(attachment, native_publication::DeliveryResult::Committed);
        for index in 65..=200 {
            app.conversation.push_system(&format!("new-{index}"));
        }
        app.reconcile_native_publication();
        let source = app.conversation.segments();
        let batch = app
            .native_publication
            .automatic
            .prepare(
                app.conversation.publication_generation(),
                source,
                source.len(),
                UiPresentationLevel::Active,
                120,
                native_publication::PreparationBudget::default(),
            )
            .unwrap();
        let text = batch.lines.join("\n");
        for index in 137..=200 {
            assert_eq!(
                text.lines()
                    .filter(|line| line.trim() == format!("new-{index}"))
                    .count(),
                1,
                "{text}"
            );
        }
        assert!(!text.contains("old-") && !text.contains("new-136"));
        assert!(app.conversation.take_publication_prune().is_none());
    }

    #[test]
    fn persistent_notice_prunes_keep_active_and_new_turn_boundaries_correct() {
        let mut app = App::new(crate::settings::shared("test"));
        for index in 0..64 {
            app.conversation.push_system(&format!("old-{index}"));
        }
        app.publication_boundary = 64;
        app.native_publication
            .automatic
            .attach(app.conversation.publication_generation(), 64);
        app.agent_active = true;
        app.conversation.append_streaming("still live");
        app.conversation.push_system("new-65");
        app.reconcile_native_publication();
        assert_eq!(
            app.publication_boundary, 63,
            "live assistant must remain beyond finalized cutoff"
        );
        app.conversation.push_system("new-66");
        app.handle_agent_event(AgentEvent::AgentEnd);
        let finalized = app.conversation.segments().len();
        app.agent_active = true;
        app.conversation.push_system("new-67");
        app.reconcile_native_publication();
        assert_eq!(
            app.publication_boundary,
            finalized - 1,
            "only prunes after terminal boundary apply to next turn"
        );
    }

    #[test]
    fn session_reset_invalidates_publication_before_the_next_prompt_arrives() {
        let mut app = App::new(crate::settings::shared("test"));
        app.conversation.push_system("old session");
        app.native_publication.automatic.attach(0, 30);
        app.handle_agent_event(AgentEvent::SessionReset);
        app.conversation.push_system("NEW_SESSION_REPLY");
        let source = app.conversation.segments();
        let batch = app
            .native_publication
            .automatic
            .prepare(
                app.conversation.publication_generation(),
                source,
                source.len(),
                UiPresentationLevel::Active,
                90,
                native_publication::PreparationBudget::default(),
            )
            .unwrap();
        assert!(batch.lines.concat().contains("NEW_SESSION_REPLY"));
        assert!(!batch.lines.concat().contains("old session"));
    }

    #[test]
    fn ui_full_does_not_change_terminal_base_or_discard_a_multiline_draft() {
        let mut app = App::new(crate::settings::shared("test"));
        app.base_terminal = TerminalPresentation::Inline;
        app.editor.set_text("first 界\nsecond é");
        app.ui_presentation = UiPresentationPolicy::full();
        assert!(!app.requires_fullscreen());
        app.open_ui_menu();
        assert!(app.requires_fullscreen());
        app.base_terminal = TerminalPresentation::Inline;
        assert!(app.requires_fullscreen());
        app.active_menu = None;
        assert!(!app.requires_fullscreen());
        assert_eq!(app.editor.render_text(), "first 界\nsecond é");
    }

    #[test]
    fn covered_root_retains_fullscreen() {
        let mut app = App::new(crate::settings::shared("test"));
        app.base_terminal = TerminalPresentation::Inline;
        assert!(!app.requires_fullscreen());
        app.open_ui_menu();
        assert!(app.requires_fullscreen());
        app.base_terminal = TerminalPresentation::Fullscreen;
        app.active_menu = None;
        assert!(app.requires_fullscreen());
    }

    #[test]
    fn inline_composer_respects_nonzero_origin() {
        use ratatui::{TerminalOptions, Viewport, backend::TestBackend};
        for (width, height) in [(40, 4), (56, LIVE_ROWS), (90, LIVE_ROWS)] {
            for level in [UiPresentationLevel::Active, UiPresentationLevel::Full] {
                let mut app = App::new(crate::settings::shared("test"));
                app.inline_active = true;
                app.ui_presentation = UiPresentationPolicy::named(level);
                app.editor.set_text("first 界\nsecond é");
                let mut terminal = Terminal::with_options(
                    TestBackend::new(width, 24),
                    TerminalOptions {
                        viewport: Viewport::Fixed(Rect::new(0, 12, width, height)),
                    },
                )
                .unwrap();
                terminal.draw(|frame| app.draw(frame)).unwrap();
                let editor = app.editor_area.unwrap();
                assert!(editor.y >= 12 && editor.bottom() <= 12 + height);
                assert_eq!(terminal.backend().buffer()[(0, 0)].symbol(), " ");
                assert_eq!(app.editor.render_text(), "first 界\nsecond é");
            }
        }
    }
}
