use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use crossterm::ExecutableCommand;
use crossterm::event::{DisableBracketedPaste, DisableMouseCapture, PopKeyboardEnhancementFlags};
use crossterm::terminal::{LeaveAlternateScreen, disable_raw_mode};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct TerminalModes {
    raw: bool,
    alternate_screen: bool,
    mouse_capture: bool,
    bracketed_paste: bool,
    keyboard_enhancement: bool,
}

/// Tracks each terminal mode as it is enabled so partial initialization,
/// ordinary errors, and unwinding all restore exactly the modes Omegon owns.
pub(super) struct TerminalSessionGuard {
    modes: Arc<Mutex<TerminalModes>>,
}

impl TerminalSessionGuard {
    pub(super) fn new() -> Self {
        Self {
            modes: Arc::new(Mutex::new(TerminalModes::default())),
        }
    }

    pub(super) fn mark_raw(&self) {
        self.update(|modes| modes.raw = true);
    }

    pub(super) fn mark_alternate_screen(&self) {
        self.update(|modes| modes.alternate_screen = true);
    }

    pub(super) fn mark_mouse_capture(&self) {
        self.update(|modes| modes.mouse_capture = true);
    }

    pub(super) fn mark_bracketed_paste(&self) {
        self.update(|modes| modes.bracketed_paste = true);
    }

    pub(super) fn mark_keyboard_enhancement(&self) {
        self.update(|modes| modes.keyboard_enhancement = true);
    }

    pub(super) fn install_panic_hook(&self) -> PanicHookGuard {
        let original = Arc::<PanicHook>::from(std::panic::take_hook());
        let chained = original.clone();
        let modes = self.modes.clone();
        std::panic::set_hook(Box::new(move |info| {
            restore_snapshot(snapshot(&modes));
            chained(info);
        }));
        PanicHookGuard {
            original: Some(original),
        }
    }

    pub(super) fn restore(&self) {
        let modes = self
            .modes
            .lock()
            .map(|mut modes| std::mem::take(&mut *modes))
            .unwrap_or_default();
        restore_snapshot(modes);
    }

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

fn snapshot(modes: &Arc<Mutex<TerminalModes>>) -> TerminalModes {
    modes.lock().map(|modes| *modes).unwrap_or_default()
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

pub(super) async fn termination_signal() -> io::Result<()> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};

        let mut terminate = signal(SignalKind::terminate())?;
        let mut hangup = signal(SignalKind::hangup())?;
        let mut quit = signal(SignalKind::quit())?;
        tokio::select! {
            result = tokio::signal::ctrl_c() => result,
            _ = terminate.recv() => Ok(()),
            _ = hangup.recv() => Ok(()),
            _ = quit.recv() => Ok(()),
        }
    }

    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c().await
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
        guard.mark_mouse_capture();
        guard.mark_bracketed_paste();
        guard.mark_keyboard_enhancement();

        guard.restore();
        assert_eq!(snapshot(&guard.modes), TerminalModes::default());
        guard.restore();
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
