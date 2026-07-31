//! Editor-history persistence and recall for the native TUI.
//!
//! Input routing decides when recall should occur; this module owns the
//! history session state and its project-local persistence policy.

use super::*;

const MAX_PERSISTED_ENTRIES: usize = 500;

fn history_path(cwd: &str) -> std::path::PathBuf {
    let project_root = crate::setup::find_project_root(std::path::Path::new(cwd));
    project_root.join(".omegon").join("history")
}

impl App {
    /// Load editor history from disk.
    pub(super) fn load_history(cwd: &str) -> Vec<String> {
        let path = history_path(cwd);
        match std::fs::read_to_string(path) {
            Ok(content) => content
                .lines()
                .filter(|line| !line.is_empty())
                .map(str::to_string)
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Save editor history to disk.
    pub(super) fn save_history(&self) {
        if self.history.is_empty() {
            return;
        }

        let path = history_path(&self.footer_data.cwd);
        let start = self.history.len().saturating_sub(MAX_PERSISTED_ENTRIES);
        let content = self.history[start..].join("\n");
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(error) = std::fs::write(path, content) {
            tracing::debug!("Failed to save history: {error}");
        }
    }

    pub(super) fn history_up(&mut self) {
        if self.history.is_empty() {
            return;
        }
        if self.history_idx.is_none() {
            self.history_draft = Some(self.editor.render_text());
        }
        let idx = match self.history_idx {
            None => self.history.len().saturating_sub(1),
            Some(idx) => idx.saturating_sub(1),
        };
        self.history_idx = Some(idx);
        self.editor.set_text(&self.history[idx]);
    }

    pub(super) fn history_down(&mut self) {
        let Some(idx) = self.history_idx else {
            return;
        };
        if idx + 1 < self.history.len() {
            self.history_idx = Some(idx + 1);
            self.editor.set_text(&self.history[idx + 1]);
        } else {
            self.history_idx = None;
            let draft = self.history_draft.take().unwrap_or_default();
            self.editor.set_text(&draft);
        }
    }

    pub(super) fn exit_history_recall(&mut self) {
        self.history_idx = None;
        self.history_draft = None;
    }

    pub(super) fn history_recall_up(&mut self) {
        self.pending_history_preload = None;
        if self.history_idx.is_some() || self.editor.is_empty() {
            self.history_up();
        }
    }

    pub(super) fn history_recall_down(&mut self) {
        self.pending_history_preload = None;
        if self.history_idx.is_some() {
            self.history_down();
        }
    }
}
