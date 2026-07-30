//! Shared runtime state consumed by TUI, web, IPC, ACP, and control surfaces.
//!
//! This module owns synchronization handles and plain session counters. It is
//! intentionally renderer-neutral; frontend projection and rendering remain in
//! their respective surface modules.

use std::sync::{Arc, Mutex};

use crate::features::cleave::CleaveProgress;
use crate::features::delegate::{DelegateProgress, DelegateResultStore};
use crate::lifecycle::read_model::LifecycleReadHandle;
use crate::status::HarnessStatus;

/// Shared session counters written by the interactive runtime and read by
/// operator surfaces.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SessionObservation {
    pub turns: u32,
    pub tool_calls: u32,
    pub compactions: u32,
    pub busy: bool,
}

/// Identifies which independently synchronized domain could not be observed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObservationDomain {
    Session,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObserveError {
    Poisoned(ObservationDomain),
}

/// Shared handles to live runtime state.
#[derive(Clone, Default)]
pub struct RuntimeStateHandles {
    pub lifecycle: Option<LifecycleReadHandle>,
    pub cleave: Option<Arc<Mutex<CleaveProgress>>>,
    pub delegate: Option<Arc<Mutex<DelegateProgress>>>,
    pub delegate_tasks: Option<Arc<DelegateResultStore>>,
    pub(crate) session: Arc<Mutex<SessionObservation>>,
    pub harness: Option<Arc<Mutex<HarnessStatus>>>,
    pub runtime_lifecycle: Arc<Mutex<Option<omegon_traits::RuntimeLifecycleSnapshot>>>,
}

impl RuntimeStateHandles {
    pub fn new(
        lifecycle: Option<LifecycleReadHandle>,
        cleave: Option<Arc<Mutex<CleaveProgress>>>,
        delegate: Option<Arc<Mutex<DelegateProgress>>>,
        delegate_tasks: Option<Arc<DelegateResultStore>>,
        harness: Option<Arc<Mutex<HarnessStatus>>>,
    ) -> Self {
        Self {
            lifecycle,
            cleave,
            delegate,
            delegate_tasks,
            session: Arc::default(),
            harness,
            runtime_lifecycle: Arc::default(),
        }
    }

    /// Copy the session domain under one short synchronous lock.
    ///
    /// The returned value owns its data and carries no synchronization guard.
    /// Adapters decide how an observation failure maps to their external
    /// compatibility contract.
    pub fn observe_session(&self) -> Result<SessionObservation, ObserveError> {
        self.session
            .lock()
            .map(|session| *session)
            .map_err(|_| ObserveError::Poisoned(ObservationDomain::Session))
    }

    /// Replace the surface-owned counters as one bounded mutation.
    pub fn update_session_counters(&self, turns: u32, tool_calls: u32, compactions: u32) {
        if let Ok(mut session) = self.session.lock() {
            session.turns = turns;
            session.tool_calls = tool_calls;
            session.compactions = compactions;
        }
    }

    /// Mark whether the invocation currently has interactive work in flight.
    pub fn set_session_busy(&self, busy: bool) {
        if let Ok(mut session) = self.session.lock() {
            session.busy = busy;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_empty_and_session_state_is_shared_across_clones() {
        let handles = RuntimeStateHandles::default();
        assert!(handles.lifecycle.is_none());
        assert!(handles.cleave.is_none());
        assert!(handles.delegate.is_none());
        assert!(handles.delegate_tasks.is_none());
        assert!(handles.harness.is_none());
        assert!(handles.runtime_lifecycle.lock().unwrap().is_none());

        let clone = handles.clone();
        {
            let mut session = handles.session.lock().unwrap();
            session.turns = 3;
            session.tool_calls = 7;
            session.compactions = 1;
            session.busy = true;
        }

        let session = clone.observe_session().unwrap();
        assert_eq!(session.turns, 3);
        assert_eq!(session.tool_calls, 7);
        assert_eq!(session.compactions, 1);
        assert!(session.busy);
    }

    #[test]
    fn session_observations_are_isolated_by_runtime_instance() {
        let first = RuntimeStateHandles::default();
        let second = RuntimeStateHandles::default();
        first.session.lock().unwrap().turns = 11;
        second.session.lock().unwrap().turns = 29;

        assert_eq!(first.observe_session().unwrap().turns, 11);
        assert_eq!(second.observe_session().unwrap().turns, 29);
    }

    #[test]
    fn poisoned_session_state_is_explicit() {
        let handles = RuntimeStateHandles::default();
        let session = handles.session.clone();
        let _ = std::thread::spawn(move || {
            let _guard = session.lock().unwrap();
            panic!("poison session fixture");
        })
        .join();

        assert_eq!(
            handles.observe_session(),
            Err(ObserveError::Poisoned(ObservationDomain::Session))
        );
    }
}
