//! Red-contract tests for bounded runtime liveness.
//!
//! These compile only after the production seams exist; the current red state
//! is intentional and proves the contracts are not already implemented.

use super::*;

#[tokio::test]
async fn terminal_boundary_fault_is_independent_of_the_ordinary_input_lane() {
    let (event_tx, _events) = tokio::sync::mpsc::channel(1);
    let (interrupt_tx, _interrupts) = tokio::sync::mpsc::channel(1);
    let (boundary_tx, mut boundaries) = tokio::sync::mpsc::channel(1);

    event_tx
        .try_send(crossterm::event::Event::FocusGained)
        .expect("ordinary input lane should be saturated");
    assert!(!terminal_input::route_input(
        crossterm::event::Event::FocusLost,
        &event_tx,
        &interrupt_tx,
        &boundary_tx,
    ));

    assert_eq!(
        boundaries.recv().await,
        Some(terminal_input::TerminalBoundaryFault::InputOverload)
    );
}
