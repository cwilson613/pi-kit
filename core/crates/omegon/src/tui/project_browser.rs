//! Project navigation composes existing session and Workbench read models.
use super::theme::Theme;
use super::*;
use crate::surfaces::menu::{
    MenuGroupProjection, MenuProjection, MenuRowKind, MenuRowProjection, MenuTabProjection,
};
use ratatui::widgets::Wrap;

pub(super) struct ProjectBrowser {
    menu: ActiveMenu,
    detail: bool,
    scroll: u16,
}

impl ProjectBrowser {
    fn new(projection: MenuProjection) -> Self {
        Self {
            menu: ActiveMenu::new(projection),
            detail: false,
            scroll: 0,
        }
    }

    fn refresh(&mut self, projection: MenuProjection) {
        let selected = self
            .menu
            .state
            .selected_row(&self.menu.projection)
            .map(|row| row.row.id.clone());
        self.menu.projection = projection;
        if !selected.is_some_and(|id| self.menu.state.select_row_by_id(&self.menu.projection, &id))
        {
            self.menu.state.selected_row = 0;
            self.detail = false;
            self.scroll = 0;
        }
    }

    pub(super) fn render(&self, frame: &mut Frame, theme: &dyn Theme) {
        if !self.detail {
            menu_surface::render_menu_surface(
                frame,
                frame.area(),
                theme,
                &self.menu.projection,
                &self.menu.state,
            );
            return;
        }
        let Some(selected) = self.menu.state.selected_row(&self.menu.projection) else {
            return;
        };
        let row = selected.row;
        let area = command_surfaces::command_modal_area(frame.area());
        frame.render_widget(Clear, area);
        let body = format!(
            "{}\n\n{}\n\n{}",
            row.label,
            row.description,
            row.metadata.join("\n")
        );
        let footer = if row.primary_action.is_some() {
            " Esc back · ↑/↓ scroll · R resume · F5 refresh · F2 conversation "
        } else {
            " Esc back · ↑/↓ scroll · F5 refresh · F2 conversation "
        };
        frame.render_widget(
            Paragraph::new(body)
                .wrap(Wrap { trim: false })
                .scroll((self.scroll, 0))
                .style(Style::default().fg(theme.fg()).bg(theme.bg()))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Project browser · Details ")
                        .title_bottom(footer)
                        .border_style(theme.style_border()),
                ),
            area,
        );
    }
}

fn row(id: &str, label: &str, description: String, metadata: Vec<String>) -> MenuRowProjection {
    MenuRowProjection {
        id: id.into(),
        label: label.into(),
        description,
        metadata,
        value: None,
        kind: MenuRowKind::Object,
        badges: vec![],
        primary_action: None,
        actions: vec![],
        safety: None,
        availability: None,
    }
}

impl App {
    fn project_browser_projection(&self) -> MenuProjection {
        let context = self.current_workbench_workspace_context();
        let mut projection = MenuProjection::new("project-browser", "Project browser");
        projection.summary = Some(format!(
            "{} · {}\n{}",
            context.repo.as_deref().unwrap_or("Project"),
            context.git_branch.as_deref().unwrap_or("No branch"),
            self.cwd().display()
        ));
        projection.footer = Some(
            "↑/↓ select · Tab tabs · Enter inspect · F5 refresh · Ctrl+C cancel · Esc back".into(),
        );
        let session_id = self
            .session_view_binding
            .as_ref()
            .map(|binding| binding.snapshot().session_id)
            .unwrap_or_else(|| "Session identity unavailable".into());
        let current = row(
            "session.current",
            "Current session",
            format!(
                "{} · {} turns · {} tool calls",
                if self.agent_active {
                    "Running"
                } else {
                    "Ready"
                },
                self.turn,
                self.tool_calls
            ),
            vec![
                session_id.clone(),
                format!("Model: {}", self.settings().model),
                "F2 returns to the conversation. Ctrl+C cancels an active turn.".into(),
            ],
        );
        let mut sessions = self.sessions_menu_projection();
        let mut saved = sessions
            .tabs
            .pop()
            .map(|tab| tab.groups)
            .unwrap_or_default();
        for group in &mut saved {
            group
                .rows
                .retain(|row| row.value.as_deref() != Some(session_id.as_str()));
        }
        let mut groups = vec![MenuGroupProjection {
            id: "project.current".into(),
            label: "Current session".into(),
            description: None,
            rows: vec![current],
        }];
        groups.extend(saved);
        projection.tabs.push(MenuTabProjection {
            id: "sessions".into(),
            label: "Sessions".into(),
            groups,
        });
        let mut work = Vec::new();
        if let Some(plan) = &self.workbench_state.active {
            work.push(row(
                "plan.current",
                "Current plan",
                format!("{} · {}/{} complete", plan.mode, plan.completed, plan.total),
                plan.items
                    .iter()
                    .map(|item| format!("{:?}: {}", item.status, item.description))
                    .collect(),
            ));
        }
        for stream in &self.workbench_state.workstreams {
            work.push(row(
                &format!("workstream.{}", stream.id),
                &stream.title,
                format!(
                    "{:?} · {}/{} complete",
                    stream.status, stream.completed, stream.total
                ),
                vec![format!("Workstream: {}", stream.id)],
            ));
        }
        if work.is_empty() {
            work.push(row(
                "work.empty",
                "No active work",
                "No plan or workstream is currently published to Workbench.".into(),
                vec!["Return to the conversation to begin work.".into()],
            ));
        }
        projection.tabs.push(MenuTabProjection {
            id: "work".into(),
            label: "Work".into(),
            groups: vec![MenuGroupProjection {
                id: "project.work".into(),
                label: "Plans and workstreams".into(),
                description: None,
                rows: work,
            }],
        });
        projection
    }

    pub(super) fn open_project_browser(&mut self) {
        self.project_browser = Some(ProjectBrowser::new(self.project_browser_projection()));
    }

    pub(super) fn handle_project_browser_key(
        &mut self,
        key: KeyEvent,
        command_tx: &OperatorCommandTx,
    ) {
        if key.code == KeyCode::F(5) {
            let projection = self.project_browser_projection();
            if let Some(browser) = &mut self.project_browser {
                browser.refresh(projection);
            }
            return;
        }
        let Some(browser) = &mut self.project_browser else {
            return;
        };
        match key.code {
            KeyCode::F(2) => self.project_browser = None,
            KeyCode::Esc if browser.detail => {
                browser.detail = false;
                browser.scroll = 0;
            }
            KeyCode::Esc => self.project_browser = None,
            KeyCode::Up if browser.detail => browser.scroll = browser.scroll.saturating_sub(1),
            KeyCode::Down if browser.detail => browser.scroll = browser.scroll.saturating_add(1),
            KeyCode::Up => browser.menu.state.move_up(),
            KeyCode::Down => browser.menu.state.move_down(&browser.menu.projection),
            KeyCode::Tab if !browser.detail => {
                browser.menu.state.next_tab(&browser.menu.projection)
            }
            KeyCode::BackTab if !browser.detail => {
                browser.menu.state.previous_tab(&browser.menu.projection)
            }
            KeyCode::Enter => {
                browser.detail = true;
                browser.scroll = 0;
            }
            KeyCode::Char('r' | 'R') if browser.detail => {
                if self.agent_active {
                    self.show_toast(
                        "Cancel the active turn before resuming another session",
                        ratatui_toaster::ToastType::Warning,
                    );
                    return;
                }
                if let Some(command) = browser
                    .menu
                    .state
                    .selected_command(&browser.menu.projection)
                {
                    match self.handle_slash_command(&command, command_tx) {
                        SlashResult::Handled => self.project_browser = None,
                        SlashResult::Display(message) => {
                            self.show_slash_response(&command, &message)
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inventory(ids: &[&str]) -> MenuProjection {
        let mut projection = MenuProjection::new("project-browser", "Project browser");
        projection.tabs.push(MenuTabProjection {
            id: "work".into(),
            label: "Work".into(),
            groups: vec![MenuGroupProjection {
                id: "work".into(),
                label: "Work".into(),
                description: None,
                rows: ids
                    .iter()
                    .map(|id| row(id, id, "Active".into(), vec![]))
                    .collect(),
            }],
        });
        projection
    }

    #[test]
    fn refresh_preserves_selected_identity_and_detail_across_reordering() {
        let mut browser = ProjectBrowser::new(inventory(&["alpha", "beta"]));
        browser.menu.state.move_down(&browser.menu.projection);
        browser.detail = true;
        browser.refresh(inventory(&["new", "alpha", "beta"]));
        assert_eq!(
            browser
                .menu
                .state
                .selected_row(&browser.menu.projection)
                .unwrap()
                .row
                .id,
            "beta"
        );
        assert!(browser.detail);
    }

    #[test]
    fn saved_session_inspection_requires_explicit_idle_resume() {
        let mut app = super::super::tests::test_app();
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        let mut projection = inventory(&["saved-session"]);
        projection.tabs[0].groups[0].rows[0].primary_action =
            Some(crate::surfaces::menu::MenuActionProjection::command(
                "resume",
                "Resume",
                "/sessions resume saved-session",
            ));
        app.project_browser = Some(ProjectBrowser::new(projection));
        app.handle_project_browser_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), &tx);
        assert!(
            rx.try_recv().is_err(),
            "inspection must not resume a session"
        );
        app.agent_active = true;
        app.handle_project_browser_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE), &tx);
        assert!(
            rx.try_recv().is_err(),
            "active turns must be cancelled first"
        );
        app.agent_active = false;
        app.handle_project_browser_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE), &tx);
        assert!(matches!(
            rx.try_recv(),
            Ok(TuiCommand::ExecuteControl { .. })
        ));
    }

    #[test]
    fn disappearing_selection_closes_stale_detail() {
        let mut browser = ProjectBrowser::new(inventory(&["removed"]));
        browser.detail = true;
        browser.refresh(inventory(&["replacement"]));
        assert!(!browser.detail);
        assert_eq!(
            browser
                .menu
                .state
                .selected_row(&browser.menu.projection)
                .unwrap()
                .row
                .id,
            "replacement"
        );
    }
}
