//! Lesson loading and persisted progress for the interactive tutorial runner.

/// A single tutorial lesson.
#[derive(Debug, Clone)]
pub(super) struct TutorialLesson {
    /// Filename (e.g. "01-cockpit.md").
    pub(super) filename: String,
    /// Title from frontmatter.
    pub(super) title: String,
    /// Lesson prompt content after frontmatter.
    pub(super) content: String,
}

/// Tutorial runner state — tracks lessons and progress.
#[derive(Debug)]
pub(super) struct TutorialState {
    lessons: Vec<TutorialLesson>,
    pub(super) current: usize,
    tutorial_dir: std::path::PathBuf,
}

/// Persisted tutorial progress.
#[derive(serde::Serialize, serde::Deserialize, Default)]
struct TutorialProgress {
    current_lesson: usize,
    completed: Vec<usize>,
}

impl TutorialState {
    /// Load tutorial lessons from a directory.
    pub(super) fn load(tutorial_dir: &std::path::Path) -> Option<Self> {
        if !tutorial_dir.is_dir() {
            return None;
        }

        let mut entries: Vec<_> = std::fs::read_dir(tutorial_dir)
            .ok()?
            .filter_map(Result::ok)
            .filter(|entry| {
                let name = entry.file_name().to_string_lossy().to_string();
                name.ends_with(".md")
                    && name
                        .chars()
                        .next()
                        .is_some_and(|character| character.is_ascii_digit())
            })
            .collect();
        entries.sort_by_key(std::fs::DirEntry::file_name);

        let mut lessons = Vec::new();
        for entry in entries {
            let filename = entry.file_name().to_string_lossy().to_string();
            let raw = std::fs::read_to_string(entry.path()).ok()?;
            let (title, content) = parse_lesson(&raw, &filename);
            lessons.push(TutorialLesson {
                filename,
                title,
                content,
            });
        }

        if lessons.is_empty() {
            return None;
        }

        let progress = load_tutorial_progress(tutorial_dir);
        let current = progress.current_lesson.min(lessons.len().saturating_sub(1));

        Some(Self {
            lessons,
            current,
            tutorial_dir: tutorial_dir.to_path_buf(),
        })
    }

    pub(super) fn current_lesson(&self) -> &TutorialLesson {
        &self.lessons[self.current]
    }

    pub(super) fn total(&self) -> usize {
        self.lessons.len()
    }

    pub(super) fn is_last(&self) -> bool {
        self.current >= self.lessons.len() - 1
    }

    pub(super) fn advance(&mut self) -> bool {
        if self.current >= self.lessons.len() - 1 {
            return false;
        }
        self.current += 1;
        self.save_progress();
        true
    }

    pub(super) fn go_back(&mut self) -> bool {
        if self.current == 0 {
            return false;
        }
        self.current -= 1;
        self.save_progress();
        true
    }

    pub(super) fn reset(&mut self) {
        self.current = 0;
        let progress_path = self.tutorial_dir.join("progress.json");
        let _ = std::fs::remove_file(progress_path);
    }

    fn save_progress(&self) {
        let progress = TutorialProgress {
            current_lesson: self.current,
            completed: (0..self.current).collect(),
        };
        let progress_path = self.tutorial_dir.join("progress.json");
        if let Ok(json) = serde_json::to_string_pretty(&progress) {
            let _ = std::fs::write(progress_path, json);
        }
    }

    pub(super) fn status_line(&self) -> String {
        let lesson = self.current_lesson();
        format!(
            "Tutorial: lesson {}/{} — \"{}\"{}",
            self.current + 1,
            self.total(),
            lesson.title,
            if self.is_last() { " (final)" } else { "" }
        )
    }
}

pub(super) fn parse_lesson(raw: &str, filename: &str) -> (String, String) {
    let mut title = filename.trim_end_matches(".md").to_string();
    let content;

    if let Some(rest) = raw.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---") {
            let frontmatter = &rest[..end];
            for line in frontmatter.lines() {
                if let Some(value) = line.strip_prefix("title:") {
                    title = value
                        .trim()
                        .trim_matches('"')
                        .trim_matches('\'')
                        .to_string();
                }
            }
            content = rest[end + 4..].trim().to_string();
        } else {
            content = raw.to_string();
        }
    } else {
        content = raw.to_string();
    }

    (title, content)
}

fn load_tutorial_progress(tutorial_dir: &std::path::Path) -> TutorialProgress {
    let path = tutorial_dir.join("progress.json");
    std::fs::read_to_string(path)
        .ok()
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or_default()
}
