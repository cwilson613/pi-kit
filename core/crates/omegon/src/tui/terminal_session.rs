use std::io::{self, Write};

use crossterm::ExecutableCommand;
use crossterm::event::{DisableBracketedPaste, DisableMouseCapture, PopKeyboardEnhancementFlags};
use crossterm::terminal::{LeaveAlternateScreen, disable_raw_mode};

/// Owns the terminal modes enabled by the TUI and restores them on every
/// unwind/early-return path. Signal-driven shutdown is handled by `run_tui`,
/// which lets this guard drop before the process exits.
pub(super) struct TerminalSessionGuard {
    keyboard_enhancement: bool,
}

impl TerminalSessionGuard {
    pub(super) fn new(keyboard_enhancement: bool) -> Self {
        Self {
            keyboard_enhancement,
        }
    }

    pub(super) fn restore(&self) {
        restore_terminal(self.keyboard_enhancement);
    }
}

impl Drop for TerminalSessionGuard {
    fn drop(&mut self) {
        self.restore();
    }
}

pub(super) fn restore_terminal(keyboard_enhancement: bool) {
    let mut stdout = io::stdout();
    let _ = stdout.execute(DisableBracketedPaste);
    let _ = stdout.execute(DisableMouseCapture);
    if keyboard_enhancement {
        let _ = stdout.execute(PopKeyboardEnhancementFlags);
    }
    let _ = disable_raw_mode();
    let _ = stdout.execute(LeaveAlternateScreen);
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
    fn guard_can_restore_repeatedly() {
        let guard = TerminalSessionGuard::new(false);
        guard.restore();
        guard.restore();
        drop(guard);
    }
}
