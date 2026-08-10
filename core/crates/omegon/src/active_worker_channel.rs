//! Liveness policy for the active worker's operator-command channel.
//!
//! Tokio mpsc receivers remain immediately ready after closure. A supervisor
//! that keeps polling a closed receiver can hot-spin and starve worker joins,
//! cancellation timers, and operator-visible lifecycle updates. This state
//! object makes closure terminal and disables the receive branch thereafter.

use tokio::sync::mpsc;

#[derive(Debug, Default)]
pub(crate) struct ActiveWorkerCommandChannel {
    open: bool,
}

impl ActiveWorkerCommandChannel {
    pub(crate) fn new() -> Self {
        Self { open: true }
    }

    pub(crate) async fn recv<T>(&self, receiver: &mut mpsc::Receiver<T>) -> Option<T> {
        if self.open {
            receiver.recv().await
        } else {
            std::future::pending::<Option<T>>().await
        }
    }

    /// Records the first observed closure. Returns false for duplicate calls,
    /// which indicate a supervisor bug because a closed branch must be disabled.
    pub(crate) fn observe_closed(&mut self) -> bool {
        std::mem::replace(&mut self.open, false)
    }

    #[cfg(test)]
    fn is_open(&self) -> bool {
        self.open
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn open_channel_delivers_commands() {
        let (tx, mut rx) = mpsc::channel(1);
        tx.send("command").await.unwrap();
        let state = ActiveWorkerCommandChannel::new();

        assert_eq!(state.recv(&mut rx).await, Some("command"));
        assert!(state.is_open());
    }

    #[tokio::test]
    async fn first_closed_receive_is_observable_once() {
        let (tx, mut rx) = mpsc::channel::<()>(1);
        drop(tx);
        let mut state = ActiveWorkerCommandChannel::new();

        assert_eq!(state.recv(&mut rx).await, None);
        assert!(state.observe_closed());
        assert!(!state.observe_closed());
        assert!(!state.is_open());
    }

    #[tokio::test]
    async fn disabled_channel_branch_is_not_permanently_ready() {
        let (tx, mut rx) = mpsc::channel::<()>(1);
        drop(tx);
        let mut state = ActiveWorkerCommandChannel::new();
        assert_eq!(state.recv(&mut rx).await, None);
        assert!(state.observe_closed());

        assert!(
            tokio::time::timeout(Duration::from_millis(25), state.recv(&mut rx))
                .await
                .is_err(),
            "closed branch must become pending instead of hot-spinning"
        );
    }

    #[tokio::test]
    async fn disabled_channel_cannot_starve_ready_worker_completion() {
        let (tx, mut rx) = mpsc::channel::<()>(1);
        drop(tx);
        let mut state = ActiveWorkerCommandChannel::new();
        assert_eq!(state.recv(&mut rx).await, None);
        assert!(state.observe_closed());
        let worker = async { "worker-returned" };

        let result = tokio::select! {
            biased;
            command = state.recv(&mut rx) => panic!("disabled branch returned: {command:?}"),
            result = worker => result,
        };

        assert_eq!(result, "worker-returned");
    }

    #[tokio::test]
    async fn disabled_channel_cannot_starve_cancellation_deadline() {
        let (tx, mut rx) = mpsc::channel::<()>(1);
        drop(tx);
        let mut state = ActiveWorkerCommandChannel::new();
        assert_eq!(state.recv(&mut rx).await, None);
        assert!(state.observe_closed());

        let fired = tokio::select! {
            biased;
            command = state.recv(&mut rx) => panic!("disabled branch returned: {command:?}"),
            _ = tokio::time::sleep(Duration::from_millis(10)) => true,
        };

        assert!(fired);
    }
}
