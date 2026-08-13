use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use tokio::sync::mpsc;

const INPUT_QUEUE_CAPACITY: usize = 256;
const INTERRUPT_QUEUE_CAPACITY: usize = 8;
const INPUT_POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminalInterrupt {
    CtrlC,
}

pub(crate) struct TerminalInputPump {
    events: mpsc::Receiver<Event>,
    interrupts: mpsc::Receiver<TerminalInterrupt>,
    stop: Arc<AtomicBool>,
}

impl TerminalInputPump {
    pub(crate) fn spawn() -> Self {
        let (event_tx, events) = mpsc::channel(INPUT_QUEUE_CAPACITY);
        let (interrupt_tx, interrupts) = mpsc::channel(INTERRUPT_QUEUE_CAPACITY);
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);

        std::thread::Builder::new()
            .name("omegon-terminal-input".to_string())
            .spawn(move || {
                while !worker_stop.load(Ordering::Acquire) {
                    match event::poll(INPUT_POLL_INTERVAL) {
                        Ok(true) => match event::read() {
                            Ok(input) => {
                                if let Some(interrupt) = classify_interrupt(&input) {
                                    // Priority ingress is independent of ordinary input
                                    // congestion. Duplicate chords may be coalesced.
                                    let _ = interrupt_tx.try_send(interrupt);
                                } else if event_tx.blocking_send(input).is_err() {
                                    break;
                                }
                            }
                            Err(_) => break,
                        },
                        Ok(false) => {}
                        Err(_) => break,
                    }
                }
            })
            .expect("spawn terminal input pump");

        Self {
            events,
            interrupts,
            stop,
        }
    }

    pub(crate) fn try_recv(&mut self) -> Result<Event, mpsc::error::TryRecvError> {
        self.events.try_recv()
    }

    pub(crate) fn try_recv_interrupt(
        &mut self,
    ) -> Result<TerminalInterrupt, mpsc::error::TryRecvError> {
        self.interrupts.try_recv()
    }

    pub(crate) async fn recv(&mut self) -> Option<Event> {
        self.events.recv().await
    }
}

impl Drop for TerminalInputPump {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        // Never join here: bounded teardown must not wait on a native read.
    }
}

fn classify_interrupt(event: &Event) -> Option<TerminalInterrupt> {
    let Event::Key(key) = event else {
        return None;
    };
    match (key.code, key.modifiers) {
        // Escape remains in the ordered lane because overlays consume it before
        // it becomes a runtime interrupt. Ctrl+C has unambiguous active-turn
        // cancellation semantics and may bypass presentation congestion.
        (KeyCode::Char('c'), modifiers) if modifiers.contains(KeyModifiers::CONTROL) => {
            Some(TerminalInterrupt::CtrlC)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState};

    fn key(code: KeyCode, modifiers: KeyModifiers) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    #[test]
    fn ctrl_c_uses_priority_ingress_but_escape_preserves_overlay_ordering() {
        assert_eq!(
            classify_interrupt(&key(KeyCode::Esc, KeyModifiers::NONE)),
            None
        );
        assert_eq!(
            classify_interrupt(&key(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(TerminalInterrupt::CtrlC)
        );
        assert_eq!(
            classify_interrupt(&key(KeyCode::Char('c'), KeyModifiers::NONE)),
            None
        );
    }

    #[tokio::test]
    async fn interrupt_lane_is_independent_of_saturated_ordinary_lane() {
        let (event_tx, _events) = mpsc::channel::<Event>(1);
        event_tx
            .try_send(key(KeyCode::Char('x'), KeyModifiers::NONE))
            .expect("fill ordinary lane");
        assert!(
            event_tx
                .try_send(key(KeyCode::Char('y'), KeyModifiers::NONE))
                .is_err()
        );

        let (interrupt_tx, mut interrupts) = mpsc::channel(1);
        interrupt_tx
            .try_send(TerminalInterrupt::CtrlC)
            .expect("priority lane remains available");
        assert_eq!(interrupts.recv().await, Some(TerminalInterrupt::CtrlC));
    }
}
