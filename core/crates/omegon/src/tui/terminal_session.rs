use std::io::{self, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crossterm::ExecutableCommand;
use crossterm::event::{DisableBracketedPaste, DisableMouseCapture, PopKeyboardEnhancementFlags};
use crossterm::terminal::{LeaveAlternateScreen, disable_raw_mode};

use super::terminal_presentation::{self, TerminalMode, TerminalModes};

#[derive(Clone)]
pub(crate) struct TerminalSessionHandle {
    modes: Arc<Mutex<TerminalModes>>,
    presentation_revision: Arc<AtomicU64>,
}

impl TerminalSessionHandle {
    pub(crate) fn new() -> Self {
        Self {
            modes: Arc::new(Mutex::new(TerminalModes::default())),
            presentation_revision: Arc::new(AtomicU64::new(0)),
        }
    }

    pub(super) fn set_mode(&self, mode: TerminalMode, enabled: bool) -> io::Result<()> {
        let mut state = self
            .modes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut target = *state;
        target.set(mode, enabled);
        terminal_presentation::transition(&mut state, target, &mut apply_mode)
    }

    pub(crate) fn with_primary_screen<T>(
        &self,
        operation: impl FnOnce() -> io::Result<T>,
    ) -> io::Result<T> {
        let mut state = self
            .modes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let result = terminal_presentation::primary_scope(&mut state, &mut apply_mode, operation);
        // Re-entering the alternate screen destroys its physical contents.
        // Invalidate the renderer even when publication or restoration failed.
        self.presentation_revision.fetch_add(1, Ordering::Release);
        result
    }

    pub(super) fn presentation_revision(&self) -> u64 {
        self.presentation_revision.load(Ordering::Acquire)
    }

    pub(super) fn with_fullscreen_io<T>(
        &self,
        operation: impl FnOnce() -> io::Result<T>,
    ) -> io::Result<T> {
        let state = self
            .modes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !state.alternate_screen || !state.raw {
            return Err(io::Error::other("fullscreen terminal ownership was lost"));
        }
        operation()
    }

    pub(crate) fn restore(&self) {
        restore_owned_modes(&self.modes);
    }
}

fn restore_owned_modes(modes: &Mutex<TerminalModes>) {
    let mut state = modes
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Err(error) = terminal_presentation::restore(&mut state, &mut apply_mode) {
        tracing::warn!(%error, "terminal restoration incomplete; failed modes remain tracked");
    }
}

fn apply_mode(mode: TerminalMode, enabled: bool) -> io::Result<()> {
    use crossterm::event::{
        EnableBracketedPaste, EnableMouseCapture, KeyboardEnhancementFlags,
        PushKeyboardEnhancementFlags,
    };
    use crossterm::terminal::{EnterAlternateScreen, enable_raw_mode};
    let mut out = io::stdout();
    match (mode, enabled) {
        (TerminalMode::Raw, true) => enable_raw_mode(),
        (TerminalMode::Raw, false) => disable_raw_mode(),
        (TerminalMode::AlternateScreen, true) => out.execute(EnterAlternateScreen).map(|_| ()),
        (TerminalMode::AlternateScreen, false) => out.execute(LeaveAlternateScreen).map(|_| ()),
        (TerminalMode::MouseCapture, true) => out.execute(EnableMouseCapture).map(|_| ()),
        (TerminalMode::MouseCapture, false) => out.execute(DisableMouseCapture).map(|_| ()),
        (TerminalMode::BracketedPaste, true) => out.execute(EnableBracketedPaste).map(|_| ()),
        (TerminalMode::BracketedPaste, false) => out.execute(DisableBracketedPaste).map(|_| ()),
        (TerminalMode::KeyboardEnhancement, true) => out
            .execute(PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES,
            ))
            .map(|_| ()),
        (TerminalMode::KeyboardEnhancement, false) => {
            out.execute(PopKeyboardEnhancementFlags).map(|_| ())
        }
    }
}

/// Tracks each terminal mode as it is enabled so partial initialization,
/// ordinary errors, and unwinding all restore exactly the modes Omegon owns.
pub(super) struct TerminalSessionGuard {
    modes: Arc<Mutex<TerminalModes>>,
}

impl TerminalSessionGuard {
    pub(super) fn with_handle(handle: TerminalSessionHandle) -> Self {
        Self {
            modes: handle.modes,
        }
    }

    #[cfg(test)]
    pub(super) fn new() -> Self {
        Self::with_handle(TerminalSessionHandle::new())
    }
    #[cfg(test)]
    pub(super) fn mark_raw(&self) {
        self.update(|modes| modes.raw = true);
    }
    #[cfg(test)]
    pub(super) fn mark_alternate_screen(&self) {
        self.update(|modes| modes.alternate_screen = true);
    }

    pub(super) fn install_panic_hook(&self) -> PanicHookGuard {
        let original = Arc::<PanicHook>::from(std::panic::take_hook());
        let chained = original.clone();
        let modes = self.modes.clone();
        std::panic::set_hook(Box::new(move |info| {
            // A panic can occur while the normal path owns this mutex. Panic
            // restoration must never wait for that owner: use the tracked
            // snapshot when immediately available, otherwise issue the full
            // idempotent emergency restore sequence.
            restore_snapshot(emergency_snapshot(&modes));
            chained(info);
        }));
        PanicHookGuard {
            original: Some(original),
        }
    }

    pub(super) fn restore(&self) {
        restore_owned_modes(&self.modes);
    }

    #[cfg(test)]
    fn update(&self, update: impl FnOnce(&mut TerminalModes)) {
        if let Ok(mut modes) = self.modes.lock() {
            update(&mut modes);
        }
    }
}

impl Drop for TerminalSessionGuard {
    fn drop(&mut self) {
        self.restore();
    }
}

type PanicHook = dyn Fn(&std::panic::PanicHookInfo<'_>) + Send + Sync + 'static;

pub(super) struct PanicHookGuard {
    original: Option<Arc<PanicHook>>,
}

impl Drop for PanicHookGuard {
    fn drop(&mut self) {
        if let Some(original) = self.original.take() {
            std::panic::set_hook(Box::new(move |info| original(info)));
        }
    }
}

#[cfg(test)]
fn snapshot(modes: &Arc<Mutex<TerminalModes>>) -> TerminalModes {
    modes.lock().map(|modes| *modes).unwrap_or_default()
}

fn emergency_snapshot(modes: &Arc<Mutex<TerminalModes>>) -> TerminalModes {
    modes
        .try_lock()
        .map(|modes| *modes)
        .unwrap_or(TerminalModes {
            raw: true,
            alternate_screen: true,
            mouse_capture: true,
            bracketed_paste: true,
            keyboard_enhancement: true,
        })
}

fn restore_snapshot(modes: TerminalModes) {
    let mut stdout = io::stdout();
    if modes.bracketed_paste {
        let _ = stdout.execute(DisableBracketedPaste);
    }
    if modes.mouse_capture {
        let _ = stdout.execute(DisableMouseCapture);
    }
    if modes.keyboard_enhancement {
        let _ = stdout.execute(PopKeyboardEnhancementFlags);
    }
    if modes.raw {
        let _ = disable_raw_mode();
    }
    if modes.alternate_screen {
        let _ = stdout.execute(LeaveAlternateScreen);
    }
    let _ = stdout.flush();
}

pub(super) struct TerminationSignals {
    #[cfg(unix)]
    terminate: tokio::signal::unix::Signal,
    #[cfg(unix)]
    hangup: tokio::signal::unix::Signal,
    #[cfg(unix)]
    quit: tokio::signal::unix::Signal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TerminationSignal {
    Interrupt,
    Terminate,
    Hangup,
    Quit,
}

impl TerminationSignals {
    pub(super) fn new() -> io::Result<Self> {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{SignalKind, signal};

            Ok(Self {
                terminate: signal(SignalKind::terminate())?,
                hangup: signal(SignalKind::hangup())?,
                quit: signal(SignalKind::quit())?,
            })
        }

        #[cfg(not(unix))]
        {
            Ok(Self {})
        }
    }

    pub(super) async fn recv(&mut self) -> io::Result<TerminationSignal> {
        #[cfg(unix)]
        {
            tokio::select! {
                result = tokio::signal::ctrl_c() => result.map(|_| TerminationSignal::Interrupt),
                _ = self.terminate.recv() => Ok(TerminationSignal::Terminate),
                _ = self.hangup.recv() => Ok(TerminationSignal::Hangup),
                _ = self.quit.recv() => Ok(TerminationSignal::Quit),
            }
        }

        #[cfg(not(unix))]
        {
            tokio::signal::ctrl_c()
                .await
                .map(|_| TerminationSignal::Interrupt)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_consumes_tracked_modes_and_is_idempotent() {
        let guard = TerminalSessionGuard::new();
        guard.mark_raw();
        guard.mark_alternate_screen();
        guard.update(|modes| {
            modes.mouse_capture = true;
            modes.bracketed_paste = true;
            modes.keyboard_enhancement = true;
        });

        guard.restore();
        assert_eq!(snapshot(&guard.modes), TerminalModes::default());
        guard.restore();
    }

    #[test]
    fn emergency_snapshot_never_waits_for_normal_mode_owner() {
        let modes = Arc::new(Mutex::new(TerminalModes {
            raw: true,
            ..TerminalModes::default()
        }));
        let _normal_owner = modes.lock().expect("normal owner");

        let emergency = emergency_snapshot(&modes);

        assert!(emergency.raw);
        assert!(emergency.alternate_screen);
        assert!(emergency.mouse_capture);
        assert!(emergency.bracketed_paste);
        assert!(emergency.keyboard_enhancement);
    }

    #[test]
    fn partial_initialization_tracks_only_completed_modes() {
        let guard = TerminalSessionGuard::new();
        guard.mark_raw();
        guard.mark_alternate_screen();

        assert_eq!(
            snapshot(&guard.modes),
            TerminalModes {
                raw: true,
                alternate_screen: true,
                ..TerminalModes::default()
            }
        );
        guard.restore();
    }
}
