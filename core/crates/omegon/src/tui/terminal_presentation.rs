//! Transactional ownership of primary/fullscreen terminal modes.
//! Adapted from the neighboring inline corpus's success-ordered transition model.
use std::io;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct TerminalModes {
    pub(super) raw: bool,
    pub(super) alternate_screen: bool,
    pub(super) mouse_capture: bool,
    pub(super) bracketed_paste: bool,
    pub(super) keyboard_enhancement: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TerminalMode {
    Raw,
    AlternateScreen,
    MouseCapture,
    BracketedPaste,
    KeyboardEnhancement,
}

impl TerminalModes {
    pub(super) fn get(self, mode: TerminalMode) -> bool {
        match mode {
            TerminalMode::Raw => self.raw,
            TerminalMode::AlternateScreen => self.alternate_screen,
            TerminalMode::MouseCapture => self.mouse_capture,
            TerminalMode::BracketedPaste => self.bracketed_paste,
            TerminalMode::KeyboardEnhancement => self.keyboard_enhancement,
        }
    }
    pub(super) fn set(&mut self, mode: TerminalMode, enabled: bool) {
        match mode {
            TerminalMode::Raw => self.raw = enabled,
            TerminalMode::AlternateScreen => self.alternate_screen = enabled,
            TerminalMode::MouseCapture => self.mouse_capture = enabled,
            TerminalMode::BracketedPaste => self.bracketed_paste = enabled,
            TerminalMode::KeyboardEnhancement => self.keyboard_enhancement = enabled,
        }
    }
}

pub(super) fn transition(
    state: &mut TerminalModes,
    target: TerminalModes,
    apply: &mut impl FnMut(TerminalMode, bool) -> io::Result<()>,
) -> io::Result<()> {
    use TerminalMode::*;
    for mode in [
        KeyboardEnhancement,
        BracketedPaste,
        MouseCapture,
        Raw,
        AlternateScreen,
    ] {
        if state.get(mode) && !target.get(mode) {
            apply(mode, false)?;
            state.set(mode, false);
        }
    }
    for mode in [
        Raw,
        AlternateScreen,
        MouseCapture,
        BracketedPaste,
        KeyboardEnhancement,
    ] {
        if !state.get(mode) && target.get(mode) {
            apply(mode, true)?;
            state.set(mode, true);
        }
    }
    Ok(())
}

/// Shutdown tries every release even if one fails; failed modes remain owned
/// so a later guard/main cleanup can retry without repeating successful releases.
pub(super) fn restore(
    state: &mut TerminalModes,
    apply: &mut impl FnMut(TerminalMode, bool) -> io::Result<()>,
) -> io::Result<()> {
    let mut failure = None;
    for mode in [
        TerminalMode::KeyboardEnhancement,
        TerminalMode::BracketedPaste,
        TerminalMode::MouseCapture,
        TerminalMode::Raw,
        TerminalMode::AlternateScreen,
    ] {
        if state.get(mode) {
            match apply(mode, false) {
                Ok(()) => state.set(mode, false),
                Err(error) => {
                    failure.get_or_insert(error);
                }
            }
        }
    }
    failure.map_or(Ok(()), Err)
}

pub(super) fn primary_scope<T>(
    state: &mut TerminalModes,
    apply: &mut impl FnMut(TerminalMode, bool) -> io::Result<()>,
    operation: impl FnOnce() -> io::Result<T>,
) -> io::Result<T> {
    let saved = *state;
    if let Err(error) = transition(state, TerminalModes::default(), apply) {
        return match transition(state, saved, apply) {
            Ok(()) => Err(error),
            Err(restore) => Err(io::Error::other(format!(
                "{error}; terminal restoration failed: {restore}"
            ))),
        };
    }
    let result = operation();
    let restored = transition(state, saved, apply);
    match (result, restored) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
        (Err(operation), Err(restore)) => Err(io::Error::other(format!(
            "{operation}; terminal restoration failed: {restore}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fullscreen() -> TerminalModes {
        TerminalModes {
            raw: true,
            alternate_screen: true,
            mouse_capture: true,
            bracketed_paste: true,
            keyboard_enhancement: true,
        }
    }
    #[test]
    fn every_mode_failure_retains_only_successful_operations_for_cleanup() {
        for failed in [
            TerminalMode::Raw,
            TerminalMode::AlternateScreen,
            TerminalMode::MouseCapture,
            TerminalMode::BracketedPaste,
            TerminalMode::KeyboardEnhancement,
        ] {
            let mut state = TerminalModes::default();
            let mut acquired = Vec::new();
            assert!(
                transition(&mut state, fullscreen(), &mut |mode, enabled| {
                    assert!(enabled);
                    if mode == failed {
                        Err(io::Error::other("injected acquisition failure"))
                    } else {
                        acquired.push(mode);
                        Ok(())
                    }
                })
                .is_err()
            );
            assert!(!state.get(failed));
            let mut released = Vec::new();
            restore(&mut state, &mut |mode, enabled| {
                assert!(!enabled);
                released.push(mode);
                Ok(())
            })
            .unwrap();
            assert_eq!(acquired.len(), released.len());
            assert!(acquired.iter().all(|mode| released.contains(mode)));
            assert_eq!(state, TerminalModes::default());

            let mut state = fullscreen();
            assert!(
                restore(&mut state, &mut |mode, _| {
                    if mode == failed {
                        Err(io::Error::other("injected release failure"))
                    } else {
                        Ok(())
                    }
                })
                .is_err()
            );
            let mut expected = TerminalModes::default();
            expected.set(failed, true);
            assert_eq!(state, expected);
            let mut retried = Vec::new();
            restore(&mut state, &mut |mode, _| {
                retried.push(mode);
                Ok(())
            })
            .unwrap();
            assert_eq!(retried, vec![failed]);
        }
    }

    #[test]
    fn failed_primary_write_restores_exact_modes_including_mouse_disabled() {
        let mut state = fullscreen();
        state.mouse_capture = false;
        let saved = state;
        let mut calls = Vec::new();
        let result = primary_scope(
            &mut state,
            &mut |mode, enabled| {
                calls.push((mode, enabled));
                Ok(())
            },
            || Err::<(), _>(io::Error::other("write failed")),
        );
        assert!(result.is_err());
        assert_eq!(state, saved);
        assert!(
            !calls
                .iter()
                .any(|(mode, _)| *mode == TerminalMode::MouseCapture)
        );
    }

    #[test]
    fn failed_suspension_restores_modes_without_running_primary_operation() {
        let mut state = fullscreen();
        let mut ran = false;
        let result = primary_scope(
            &mut state,
            &mut |mode, enabled| {
                if mode == TerminalMode::AlternateScreen && !enabled {
                    Err(io::Error::other("leave failed"))
                } else {
                    Ok(())
                }
            },
            || {
                ran = true;
                Ok(())
            },
        );
        assert!(result.is_err());
        assert!(!ran);
        assert_eq!(state, fullscreen());
    }

    #[test]
    fn shutdown_releases_other_modes_and_retains_only_failed_modes_for_retry() {
        let mut state = fullscreen();
        let mut released = Vec::new();
        assert!(
            restore(&mut state, &mut |mode, _| {
                if mode == TerminalMode::KeyboardEnhancement {
                    Err(io::Error::other("pop failed"))
                } else {
                    released.push(mode);
                    Ok(())
                }
            })
            .is_err()
        );
        assert_eq!(
            state,
            TerminalModes {
                keyboard_enhancement: true,
                ..TerminalModes::default()
            }
        );
        assert!(released.contains(&TerminalMode::AlternateScreen));
        let mut retried = Vec::new();
        restore(&mut state, &mut |mode, _| {
            retried.push(mode);
            Ok(())
        })
        .unwrap();
        assert_eq!(retried, vec![TerminalMode::KeyboardEnhancement]);
        assert_eq!(state, TerminalModes::default());
    }

    #[test]
    fn failed_entry_records_only_acquired_modes_and_retry_skips_them() {
        let mut state = TerminalModes::default();
        let result = transition(&mut state, fullscreen(), &mut |mode, _| {
            if mode == TerminalMode::MouseCapture {
                Err(io::Error::other("mouse failed"))
            } else {
                Ok(())
            }
        });
        assert!(result.is_err());
        assert!(state.raw && state.alternate_screen);
        assert!(!state.mouse_capture);
        let mut retried = Vec::new();
        transition(&mut state, fullscreen(), &mut |mode, enabled| {
            retried.push((mode, enabled));
            Ok(())
        })
        .unwrap();
        assert!(
            !retried
                .iter()
                .any(|(mode, _)| matches!(mode, TerminalMode::Raw | TerminalMode::AlternateScreen))
        );
        assert_eq!(state, fullscreen());
    }
    #[test]
    fn failed_leave_keeps_alternate_screen_owned_until_retry_succeeds() {
        let mut state = fullscreen();
        assert!(
            transition(&mut state, TerminalModes::default(), &mut |mode, _| {
                if mode == TerminalMode::AlternateScreen {
                    Err(io::Error::other("leave failed"))
                } else {
                    Ok(())
                }
            })
            .is_err()
        );
        assert!(state.alternate_screen);
        assert!(
            !state.raw
                && !state.mouse_capture
                && !state.keyboard_enhancement
                && !state.bracketed_paste
        );
        let mut retried = Vec::new();
        transition(
            &mut state,
            TerminalModes::default(),
            &mut |mode, enabled| {
                retried.push((mode, enabled));
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(retried, vec![(TerminalMode::AlternateScreen, false)]);
    }
}
