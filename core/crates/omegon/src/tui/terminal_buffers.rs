//! Two Ratatui buffers with one active physical output owner.
use super::{TerminalSessionHandle, inline::LIVE_ROWS};
use crate::surfaces::layout::TerminalPresentation;
use ratatui::{Terminal, TerminalOptions, Viewport, backend::CrosstermBackend};
use std::io;

type NativeTerminal = Terminal<CrosstermBackend<io::Stdout>>;

pub(super) struct TerminalBuffers {
    active: NativeTerminal,
    inline: Option<NativeTerminal>,
    presentation: TerminalPresentation,
    primary_revision: u64,
}

fn create(presentation: TerminalPresentation) -> io::Result<NativeTerminal> {
    Terminal::with_options(
        CrosstermBackend::new(io::stdout()),
        TerminalOptions {
            viewport: match presentation {
                TerminalPresentation::Inline => Viewport::Inline(LIVE_ROWS),
                TerminalPresentation::Fullscreen => Viewport::Fullscreen,
            },
        },
    )
}

impl TerminalBuffers {
    pub(super) fn new(presentation: TerminalPresentation) -> io::Result<Self> {
        Ok(Self {
            active: create(presentation)?,
            inline: None,
            presentation,
            primary_revision: 0,
        })
    }

    pub(super) fn active(&mut self) -> &mut NativeTerminal {
        &mut self.active
    }

    pub(super) fn synchronize_primary(
        &mut self,
        session: &TerminalSessionHandle,
    ) -> io::Result<()> {
        let revision = session.presentation_revision();
        if revision == self.primary_revision {
            return Ok(());
        }
        // Explicit primary output moved the cursor. An old inline anchor can
        // erase that output, so acquire a new anchor after the completed scope.
        self.inline = None;
        if self.presentation == TerminalPresentation::Inline {
            session.with_presentation_io(self.presentation, || {
                self.active = create(TerminalPresentation::Inline)?;
                Ok(())
            })?;
        }
        self.primary_revision = revision;
        Ok(())
    }

    pub(super) fn select(
        &mut self,
        target: TerminalPresentation,
        mouse: bool,
        session: &TerminalSessionHandle,
    ) -> io::Result<()> {
        if target == self.presentation {
            return Ok(());
        }
        if self.presentation == TerminalPresentation::Inline {
            session.with_presentation_io(self.presentation, || self.active.clear())?;
        }
        session.track_inline_area(None);
        session.select_presentation(target, mouse)?;
        session.with_presentation_io(target, || {
            match target {
                TerminalPresentation::Fullscreen => {
                    let next = create(target)?;
                    self.inline = Some(std::mem::replace(&mut self.active, next));
                }
                TerminalPresentation::Inline => {
                    self.active = match self.inline.take() {
                        Some(inline) => inline,
                        None => create(target)?,
                    };
                    self.active.autoresize()?;
                    self.active.clear()?;
                }
            }
            self.presentation = target;
            Ok(())
        })
        // A creation/geometry failure propagates to the session guard; no draw
        // is attempted with a half-restored buffer or an invented rollback state.
    }
}
