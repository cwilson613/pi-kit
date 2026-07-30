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
#[derive(Default)]
pub struct SharedSessionStats {
    pub turns: u32,
    pub tool_calls: u32,
    pub compactions: u32,
    pub busy: bool,
}

/// Shared handles to live runtime state.
#[derive(Clone, Default)]
pub struct RuntimeStateHandles {
    pub lifecycle: Option<LifecycleReadHandle>,
    pub cleave: Option<Arc<Mutex<CleaveProgress>>>,
    pub delegate: Option<Arc<Mutex<DelegateProgress>>>,
    pub delegate_tasks: Option<Arc<DelegateResultStore>>,
    pub session: Arc<Mutex<SharedSessionStats>>,
    pub harness: Option<Arc<Mutex<HarnessStatus>>>,
    pub runtime_lifecycle: Arc<Mutex<Option<omegon_traits::RuntimeLifecycleSnapshot>>>,
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

        let session = clone.session.lock().unwrap();
        assert_eq!(session.turns, 3);
        assert_eq!(session.tool_calls, 7);
        assert_eq!(session.compactions, 1);
        assert!(session.busy);
    }
}
