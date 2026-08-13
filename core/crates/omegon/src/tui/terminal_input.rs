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
pub(crate) enum TerminalBoundaryFault {
    InputOverload,
    ReadFailed,
}

impl TerminalBoundaryFault {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::InputOverload => "terminal input overloaded; shutting down safely",
            Self::ReadFailed => "terminal input boundary closed; shutting down safely",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminalInterrupt {
    CtrlC,
}

fn route_input(
    input: Event,
    event_tx: &mpsc::Sender<Event>,
    interrupt_tx: &mpsc::Sender<TerminalInterrupt>,
    boundary_tx: &mpsc::Sender<TerminalBoundaryFault>,
) -> bool {
    if let Some(interrupt) = classify_interrupt(&input) {
        // Priority ingress is independent of ordinary input congestion.
        // Duplicate chords may be coalesced.
        let _ = interrupt_tx.try_send(interrupt);
        return true;
    }
    match event_tx.try_send(input) {
        Ok(()) => true,
        Err(mpsc::error::TrySendError::Full(_)) => {
            let _ = boundary_tx.try_send(TerminalBoundaryFault::InputOverload);
            false
        }
        Err(mpsc::error::TrySendError::Closed(_)) => false,
    }
}

pub(crate) struct TerminalInputPump {
    events: mpsc::Receiver<Event>,
    interrupts: mpsc::Receiver<TerminalInterrupt>,
    boundaries: mpsc::Receiver<TerminalBoundaryFault>,
    stop: Arc<AtomicBool>,
}

impl TerminalInputPump {
    pub(crate) fn spawn() -> Self {
        let (event_tx, events) = mpsc::channel(INPUT_QUEUE_CAPACITY);
        let (interrupt_tx, interrupts) = mpsc::channel(INTERRUPT_QUEUE_CAPACITY);
        let (boundary_tx, boundaries) = mpsc::channel(1);
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);

        std::thread::Builder::new()
            .name("omegon-terminal-input".to_string())
            .spawn(move || {
                while !worker_stop.load(Ordering::Acquire) {
                    match event::poll(INPUT_POLL_INTERVAL) {
                        Ok(true) => match event::read() {
                            Ok(input) => {
                                if !route_input(input, &event_tx, &interrupt_tx, &boundary_tx) {
                                    break;
                                }
                            }
                            Err(_) => {
                                let _ = boundary_tx.try_send(TerminalBoundaryFault::ReadFailed);
                                break;
                            }
                        },
                        Ok(false) => {}
                        Err(_) => {
                            let _ = boundary_tx.try_send(TerminalBoundaryFault::ReadFailed);
                            break;
                        }
                    }
                }
            })
            .expect("spawn terminal input pump");

        Self {
            events,
            interrupts,
            boundaries,
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

    pub(crate) fn try_recv_boundary(
        &mut self,
    ) -> Result<TerminalBoundaryFault, mpsc::error::TryRecvError> {
        self.boundaries.try_recv()
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
    async fn ordinary_lane_saturation_emits_explicit_boundary_fault() {
        let (event_tx, _events) = mpsc::channel::<Event>(1);
        let (interrupt_tx, _interrupts) = mpsc::channel(1);
        let (boundary_tx, mut boundaries) = mpsc::channel(1);
        assert!(route_input(
            key(KeyCode::Char('x'), KeyModifiers::NONE),
            &event_tx,
            &interrupt_tx,
            &boundary_tx,
        ));

        assert!(!route_input(
            key(KeyCode::Char('y'), KeyModifiers::NONE),
            &event_tx,
            &interrupt_tx,
            &boundary_tx,
        ));
        assert_eq!(
            boundaries.recv().await,
            Some(TerminalBoundaryFault::InputOverload)
        );
    }

    #[tokio::test]
    async fn priority_interrupt_bypasses_saturated_ordinary_lane() {
        let (event_tx, _events) = mpsc::channel::<Event>(1);
        let (interrupt_tx, mut interrupts) = mpsc::channel(1);
        let (boundary_tx, _boundaries) = mpsc::channel(1);
        assert!(route_input(
            key(KeyCode::Char('x'), KeyModifiers::NONE),
            &event_tx,
            &interrupt_tx,
            &boundary_tx,
        ));
        assert!(route_input(
            key(KeyCode::Char('c'), KeyModifiers::CONTROL),
            &event_tx,
            &interrupt_tx,
            &boundary_tx,
        ));
        assert_eq!(interrupts.recv().await, Some(TerminalInterrupt::CtrlC));
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
