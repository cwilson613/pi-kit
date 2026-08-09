//! Read-only modal projection for managed execution sessions.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use ansi_to_tui::IntoText as _;

use super::theme::Theme;

#[derive(Debug, Clone)]
pub(crate) struct ProcessViewerState {
    pub session_id: String,
    pub scroll: u16,
    pub follow: bool,
    pub confirm_stop: bool,
}

impl ProcessViewerState {
    pub(crate) fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            scroll: 0,
            follow: true,
            confirm_stop: false,
        }
    }

    pub(crate) fn scroll_up(&mut self) {
        self.follow = false;
        self.scroll = self.scroll.saturating_sub(1);
    }

    pub(crate) fn scroll_down(&mut self) {
        self.scroll = self.scroll.saturating_add(1);
    }

    pub(crate) fn toggle_follow(&mut self) {
        self.confirm_stop = false;
        self.follow = !self.follow;
        if self.follow {
            self.scroll = 0;
        }
    }

    pub(crate) fn switch_session(&mut self, delta: isize) {
        let sessions = crate::tools::terminal::execution_session_snapshots();
        if sessions.is_empty() {
            return;
        }
        let current = sessions
            .iter()
            .position(|snapshot| snapshot.id == self.session_id)
            .unwrap_or(0);
        let next = (current as isize + delta).rem_euclid(sessions.len() as isize) as usize;
        self.session_id = sessions[next].id.clone();
        self.scroll = 0;
        self.follow = true;
        self.confirm_stop = false;
    }

    pub(crate) fn request_stop(&mut self) -> bool {
        if self.confirm_stop {
            self.confirm_stop = false;
            true
        } else {
            self.confirm_stop = true;
            false
        }
    }

    pub(crate) fn cancel_confirmation(&mut self) -> bool {
        std::mem::take(&mut self.confirm_stop)
    }
}

pub(crate) fn render_process_viewer(
    frame: &mut Frame,
    area: Rect,
    theme: &dyn Theme,
    state: &ProcessViewerState,
) {
    let popup = super::command_surfaces::command_modal_area(area);
    frame.render_widget(Clear, popup);

    let snapshot = crate::tools::terminal::execution_session_snapshot_by_id(&state.session_id);
    let (title, body, footer) = match snapshot {
        Some(snapshot) => {
            let status = match snapshot.state {
                crate::tools::terminal::ExecutionSessionState::Running => "running",
                crate::tools::terminal::ExecutionSessionState::Exited => "exited",
                crate::tools::terminal::ExecutionSessionState::Failed => "failed",
            };
            let output = terminal_output_text(&snapshot.output, theme);
            (
                format!(" Process · {} · {status} ", snapshot.name),
                process_body(&snapshot, output, theme),
                if state.confirm_stop {
                    "Press x again to stop this process · Esc cancel"
                } else if snapshot.capabilities.stop {
                    "←/→ switch · ↑/↓ scroll · f follow · x stop · Esc close · read-only"
                } else {
                    "←/→ switch · ↑/↓ scroll · Esc close · completed"
                },
            )
        }
        None => (
            " Process · unavailable ".to_string(),
            Text::styled(
                format!("Session '{}' is no longer retained.", state.session_id),
                theme.style_muted(),
            ),
            "Esc close",
        ),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(theme.style_border())
        .title(title)
        .title_bottom(Line::from(footer).style(theme.style_dim()));
    let inner_height = popup.height.saturating_sub(2);
    let body_lines = body.lines.len() as u16;
    let max_scroll = body_lines.saturating_sub(inner_height);
    let scroll = if state.follow {
        max_scroll
    } else {
        state.scroll.min(max_scroll)
    };
    frame.render_widget(
        Paragraph::new(body)
            .block(block)
            .style(theme.style_muted())
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        popup,
    );
}

fn terminal_output_text(output: &str, theme: &dyn Theme) -> Text<'static> {
    if output.is_empty() {
        return Text::styled("(no output yet)", theme.style_dim());
    }

    match output.to_string().into_text() {
        Ok(mut text) => {
            for line in &mut text.lines {
                for span in &mut line.spans {
                    if span.style.fg.is_none() {
                        span.style = span.style.fg(theme.muted());
                    }
                }
            }
            text
        }
        Err(_) => Text::styled(strip_terminal_controls(output), theme.style_muted()),
    }
}

fn process_body(
    snapshot: &crate::tools::terminal::ExecutionSessionSnapshot,
    output: Text<'static>,
    theme: &dyn Theme,
) -> Text<'static> {
    let mut lines = vec![
        Line::from(vec![
            Span::styled("$ ", theme.style_accent()),
            Span::styled(strip_terminal_controls(&snapshot.command), theme.style_fg()),
        ]),
        Line::styled(
            format!("cwd: {}", snapshot.cwd.display()),
            theme.style_dim(),
        ),
        Line::styled(
            format!(
                "pid: {} · elapsed: {}s",
                snapshot.pid, snapshot.elapsed_secs
            ),
            theme.style_dim(),
        ),
        Line::styled(
            format!(
                "transcript: {}{}",
                snapshot.transcript_path.display(),
                if snapshot.transcript_truncated {
                    " · truncated"
                } else {
                    ""
                }
            ),
            theme.style_dim(),
        ),
        Line::default(),
    ];
    lines.extend(output.lines);
    Text::from(lines)
}

fn strip_terminal_controls(input: &str) -> String {
    input
        .chars()
        .filter(|ch| matches!(ch, '\n' | '\r' | '\t') || !ch.is_control())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewer_stop_requires_confirmation_and_escape_cancels() {
        let mut state = ProcessViewerState::new("session");
        assert!(!state.request_stop());
        assert!(state.confirm_stop);
        assert!(state.cancel_confirmation());
        assert!(!state.confirm_stop);
        assert!(!state.request_stop());
        assert!(state.request_stop());
        assert!(!state.confirm_stop);
    }

    #[test]
    fn viewer_state_disables_follow_when_scrolling_up() {
        let mut state = ProcessViewerState::new("session");
        state.scroll = 3;
        state.scroll_up();
        assert!(!state.follow);
        assert_eq!(state.scroll, 2);
        state.toggle_follow();
        assert!(state.follow);
        assert_eq!(state.scroll, 0);
    }

    #[test]
    fn terminal_output_parses_ansi_without_rendering_escape_bytes() {
        let theme = super::super::theme::Alpharius;
        let text = terminal_output_text("plain\n\x1b[31mfailed\x1b[0m", &theme);

        assert_eq!(text.lines.len(), 2);
        assert_eq!(text.lines[0].spans[0].content, "plain");
        assert_eq!(text.lines[1].spans[0].content, "failed");
        assert_eq!(text.lines[1].spans[0].style.fg, Some(Color::Red));
        assert!(
            text.lines
                .iter()
                .flat_map(|line| &line.spans)
                .all(|span| !span.content.contains('\x1b'))
        );
    }

    #[test]
    fn terminal_output_uses_theme_for_plain_and_empty_output() {
        let theme = super::super::theme::Alpharius;
        let plain = terminal_output_text("ordinary", &theme);
        assert_eq!(plain.lines[0].spans[0].style.fg, Some(theme.muted()));

        let empty = terminal_output_text("", &theme);
        assert_eq!(empty.lines[0].spans[0].content, "(no output yet)");
        assert_eq!(empty.style, theme.style_dim());
    }
}
