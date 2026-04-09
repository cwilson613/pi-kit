from pathlib import Path
p=Path('core/crates/omegon/src/tui/editor.rs')
text=p.read_text()
old = r'''/// A terminal-style text editor with history and reverse search.
pub struct Editor {
    pub textarea: TextArea<'static>,
    mode: EditorMode,
    /// Kill ring — last killed text (Ctrl+K, Ctrl+U).
    kill_ring: Option<String>,
    /// Tracked vertical scroll offset for wrapped multiline rendering.
    scroll_row: u16,
    /// Internal text model. Attachment tokens are stored as OBJECT REPLACEMENT
    /// characters and projected into visible placeholders for rendering.
    model_text: String,
    /// Attachment payloads in token order as they appear in `model_text`.
    attachments: Vec<PathBuf>,
}

impl Editor {
    const ATTACHMENT_SENTINEL: char = '\u{FFFC}';

    pub fn new() -> Self {
        let mut ta = TextArea::default();
        ta.set_cursor_line_style(Style::default());
        ta.set_cursor_style(Style::default().add_modifier(Modifier::REVERSED));
        ta.set_placeholder_text("Ask anything, or type / for commands");
        ta.set_placeholder_style(Style::default().fg(Color::from_u32(0x00405870)));
        Self {
            textarea: ta,
            mode: EditorMode::Normal,
            kill_ring: None,
            scroll_row: 0,
            model_text: String::new(),
            attachments: Vec::new(),
        }
    }

    fn attachment_placeholder(path: &Path, idx: usize) -> String {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_ascii_lowercase());
        let kind = match ext.as_deref() {
            Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tiff" | "tif") => "image",
            Some("pdf") => "pdf",
            _ => "attachment",
        };
        format!("[{kind}{idx}]")
    }
'''
new = r'''/// A terminal-style text editor with history and reverse search.
pub struct Editor {
    pub textarea: TextArea<'static>,
    mode: EditorMode,
    /// Kill ring — last killed text (Ctrl+K, Ctrl+U).
    kill_ring: Option<String>,
    /// Tracked vertical scroll offset for wrapped multiline rendering.
    scroll_row: u16,
    /// Internal text model. Inline tokens are stored as OBJECT REPLACEMENT
    /// characters and projected into visible placeholders for rendering.
    model_text: String,
    /// Inline token payloads in token order as they appear in `model_text`.
    inline_tokens: Vec<InlineToken>,
}

impl Editor {
    const INLINE_TOKEN_SENTINEL: char = '\u{FFFC}';
    const COLLAPSIBLE_PASTE_MIN_LINES: usize = 3;
    const COLLAPSIBLE_PASTE_MIN_CHARS: usize = 120;

    pub fn new() -> Self {
        let mut ta = TextArea::default();
        ta.set_cursor_line_style(Style::default());
        ta.set_cursor_style(Style::default().add_modifier(Modifier::REVERSED));
        ta.set_placeholder_text("Ask anything, or type / for commands");
        ta.set_placeholder_style(Style::default().fg(Color::from_u32(0x00405870)));
        Self {
            textarea: ta,
            mode: EditorMode::Normal,
            kill_ring: None,
            scroll_row: 0,
            model_text: String::new(),
            inline_tokens: Vec::new(),
        }
    }

    fn attachment_placeholder(path: &Path, idx: usize) -> String {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_ascii_lowercase());
        let kind = match ext.as_deref() {
            Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tiff" | "tif") => "image",
            Some("pdf") => "pdf",
            _ => "attachment",
        };
        format!("[{kind}{idx}]")
    }

    fn paste_placeholder(text: &str, idx: usize) -> String {
        let newline_count = text.chars().filter(|ch| *ch == '\n').count();
        let extra_lines = newline_count.saturating_sub(1);
        if extra_lines > 0 {
            format!("[Pasted text #{} +{} lines]", idx + 1, extra_lines)
        } else {
            format!("[Pasted text #{}]", idx + 1)
        }
    }

    fn token_placeholder(token: &InlineToken, idx: usize) -> String {
        match token {
            InlineToken::Attachment(path) => Self::attachment_placeholder(path, idx),
            InlineToken::CollapsedPaste { text } => Self::paste_placeholder(text, idx),
        }
    }

    fn should_collapse_paste(text: &str) -> bool {
        let line_count = text.split('\n').count();
        line_count >= Self::COLLAPSIBLE_PASTE_MIN_LINES
            || text.chars().count() >= Self::COLLAPSIBLE_PASTE_MIN_CHARS
    }
'''
if old not in text:
    raise SystemExit('block1 not found')
text=text.replace(old,new,1)
text=text.replace('Self::ATTACHMENT_SENTINEL','Self::INLINE_TOKEN_SENTINEL')
text=text.replace('.attachments', '.inline_tokens')
text=text.replace(' attachments = ', ' attachments = ')
p.write_text(text)
