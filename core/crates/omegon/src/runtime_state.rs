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
    Cleave,
    Delegate,
    Harness,
    RuntimeLifecycle,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObserveError {
    Poisoned(ObservationDomain),
}

/// Invocation-owned session state. Clones share one invocation's counters;
/// separately constructed handles remain isolated.
#[derive(Clone, Default)]
pub struct SessionStateHandle {
    state: Arc<Mutex<SessionObservation>>,
}

impl SessionStateHandle {
    /// Copy the session domain under one short synchronous lock.
    pub fn observe(&self) -> Result<SessionObservation, ObserveError> {
        self.state
            .lock()
            .map(|session| *session)
            .map_err(|_| ObserveError::Poisoned(ObservationDomain::Session))
    }

    pub fn try_update_counters(
        &self,
        turns: u32,
        tool_calls: u32,
        compactions: u32,
    ) -> Result<(), ObserveError> {
        let mut session = self
            .state
            .lock()
            .map_err(|_| ObserveError::Poisoned(ObservationDomain::Session))?;
        session.turns = turns;
        session.tool_calls = tool_calls;
        session.compactions = compactions;
        Ok(())
    }

    pub fn update_counters(&self, turns: u32, tool_calls: u32, compactions: u32) {
        let _ = self.try_update_counters(turns, tool_calls, compactions);
    }

    /// Mark whether this invocation currently has interactive work in flight.
    pub fn try_set_busy(&self, busy: bool) -> Result<(), ObserveError> {
        let mut session = self
            .state
            .lock()
            .map_err(|_| ObserveError::Poisoned(ObservationDomain::Session))?;
        session.busy = busy;
        Ok(())
    }

    pub fn set_busy(&self, busy: bool) {
        let _ = self.try_set_busy(busy);
    }
}

/// Shared handles to live runtime state.
#[derive(Clone, Default)]
pub struct RuntimeStateHandles {
    pub lifecycle: Option<LifecycleReadHandle>,
    pub(crate) cleave: Arc<Mutex<Option<Arc<Mutex<CleaveProgress>>>>>,
    pub(crate) delegate: Arc<Mutex<Option<Arc<Mutex<DelegateProgress>>>>>,
    pub delegate_tasks: Option<Arc<DelegateResultStore>>,
    session: SessionStateHandle,
    pub(crate) harness: Arc<Mutex<Option<Arc<Mutex<HarnessStatus>>>>>,
    pub(crate) runtime_lifecycle: Arc<Mutex<Option<omegon_traits::RuntimeLifecycleSnapshot>>>,
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
            cleave: Arc::new(Mutex::new(cleave)),
            delegate: Arc::new(Mutex::new(delegate)),
            delegate_tasks,
            session: SessionStateHandle::default(),
            harness: Arc::new(Mutex::new(harness)),
            runtime_lifecycle: Arc::default(),
        }
    }

    pub fn session(&self) -> &SessionStateHandle {
        &self.session
    }

    /// Copy the complete cleave source domain under one short lock.
    ///
    /// `None` means this invocation has no cleave source installed. Adapters
    /// remain responsible for projecting the owned source into their own wire
    /// or renderer contracts.
    pub fn observe_cleave(&self) -> Result<Option<CleaveProgress>, ObserveError> {
        let cleave = self
            .cleave
            .lock()
            .map_err(|_| ObserveError::Poisoned(ObservationDomain::Cleave))?
            .clone();
        cleave
            .map(|cleave| {
                cleave
                    .lock()
                    .map(|progress| progress.clone())
                    .map_err(|_| ObserveError::Poisoned(ObservationDomain::Cleave))
            })
            .transpose()
    }

    pub fn cleave_available(&self) -> bool {
        self.cleave.lock().is_ok_and(|cleave| cleave.is_some())
    }

    /// Install or replace the cleave progress source for this invocation.
    pub fn install_cleave(&self, progress: Arc<Mutex<CleaveProgress>>) {
        if let Ok(mut cleave) = self.cleave.lock() {
            *cleave = Some(progress);
        }
    }

    /// Remove the cleave progress source for this invocation.
    pub fn clear_cleave(&self) {
        if let Ok(mut cleave) = self.cleave.lock() {
            *cleave = None;
        }
    }

    /// Copy the complete delegate source domain under one short lock.
    ///
    /// `None` means this invocation has no delegate source installed. Surface
    /// adapters retain ownership of their wire and rendering projections.
    pub fn observe_delegate(&self) -> Result<Option<DelegateProgress>, ObserveError> {
        let delegate = self
            .delegate
            .lock()
            .map_err(|_| ObserveError::Poisoned(ObservationDomain::Delegate))?
            .clone();
        delegate
            .map(|delegate| {
                delegate
                    .lock()
                    .map(|progress| progress.clone())
                    .map_err(|_| ObserveError::Poisoned(ObservationDomain::Delegate))
            })
            .transpose()
    }

    pub fn delegate_available(&self) -> bool {
        self.delegate
            .lock()
            .is_ok_and(|delegate| delegate.is_some())
    }

    /// Install or replace the delegate progress source for this invocation.
    pub fn install_delegate(&self, progress: Arc<Mutex<DelegateProgress>>) {
        if let Ok(mut delegate) = self.delegate.lock() {
            *delegate = Some(progress);
        }
    }

    /// Remove the delegate progress source for this invocation.
    pub fn clear_delegate(&self) {
        if let Ok(mut delegate) = self.delegate.lock() {
            *delegate = None;
        }
    }

    /// Copy the current harness source without retaining synchronization guards.
    pub fn observe_harness(&self) -> Result<Option<HarnessStatus>, ObserveError> {
        let harness = self
            .harness
            .lock()
            .map_err(|_| ObserveError::Poisoned(ObservationDomain::Harness))?
            .clone();
        harness
            .map(|harness| {
                harness
                    .lock()
                    .map(|status| status.clone())
                    .map_err(|_| ObserveError::Poisoned(ObservationDomain::Harness))
            })
            .transpose()
    }

    pub fn harness_available(&self) -> bool {
        self.harness.lock().is_ok_and(|harness| harness.is_some())
    }

    pub fn try_install_harness(
        &self,
        status: Arc<Mutex<HarnessStatus>>,
    ) -> Result<(), ObserveError> {
        let mut harness = self
            .harness
            .lock()
            .map_err(|_| ObserveError::Poisoned(ObservationDomain::Harness))?;
        *harness = Some(status);
        Ok(())
    }

    pub fn install_harness(&self, status: Arc<Mutex<HarnessStatus>>) {
        let _ = self.try_install_harness(status);
    }

    pub fn try_clear_harness(&self) -> Result<(), ObserveError> {
        let mut harness = self
            .harness
            .lock()
            .map_err(|_| ObserveError::Poisoned(ObservationDomain::Harness))?;
        *harness = None;
        Ok(())
    }

    pub fn clear_harness(&self) {
        let _ = self.try_clear_harness();
    }

    pub fn mutate_harness<R>(
        &self,
        mutate: impl FnOnce(&mut HarnessStatus) -> R,
    ) -> Result<Option<(R, HarnessStatus)>, ObserveError> {
        let harness = self
            .harness
            .lock()
            .map_err(|_| ObserveError::Poisoned(ObservationDomain::Harness))?
            .clone();
        harness
            .map(|harness| {
                let mut status = harness
                    .lock()
                    .map_err(|_| ObserveError::Poisoned(ObservationDomain::Harness))?;
                let result = mutate(&mut status);
                Ok((result, status.clone()))
            })
            .transpose()
    }

    pub fn observe_runtime_lifecycle(
        &self,
    ) -> Result<Option<omegon_traits::RuntimeLifecycleSnapshot>, ObserveError> {
        self.runtime_lifecycle
            .lock()
            .map(|snapshot| snapshot.clone())
            .map_err(|_| ObserveError::Poisoned(ObservationDomain::RuntimeLifecycle))
    }

    /// Store replay state before publishing it, and never publish while locked.
    pub fn publish_runtime_lifecycle(
        &self,
        snapshot: omegon_traits::RuntimeLifecycleSnapshot,
        publish: impl FnOnce(&omegon_traits::RuntimeLifecycleSnapshot),
    ) -> Result<(), ObserveError> {
        {
            let mut current = self
                .runtime_lifecycle
                .lock()
                .map_err(|_| ObserveError::Poisoned(ObservationDomain::RuntimeLifecycle))?;
            *current = Some(snapshot.clone());
        }
        publish(&snapshot);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_empty_and_session_state_is_shared_across_clones() {
        let handles = RuntimeStateHandles::default();
        assert!(handles.lifecycle.is_none());
        assert!(!handles.cleave_available());
        assert!(!handles.delegate_available());
        assert!(handles.delegate_tasks.is_none());
        assert!(!handles.harness_available());
        assert!(handles.observe_runtime_lifecycle().unwrap().is_none());

        let clone = handles.clone();
        {
            let mut session = handles.session.state.lock().unwrap();
            session.turns = 3;
            session.tool_calls = 7;
            session.compactions = 1;
            session.busy = true;
        }

        let session = clone.session.observe().unwrap();
        assert_eq!(session.turns, 3);
        assert_eq!(session.tool_calls, 7);
        assert_eq!(session.compactions, 1);
        assert!(session.busy);
    }

    #[test]
    fn session_observations_are_isolated_by_runtime_instance() {
        let first = RuntimeStateHandles::default();
        let second = RuntimeStateHandles::default();
        first.session.state.lock().unwrap().turns = 11;
        second.session.state.lock().unwrap().turns = 29;

        assert_eq!(first.session.observe().unwrap().turns, 11);
        assert_eq!(second.session.observe().unwrap().turns, 29);
    }

    #[test]
    fn absent_cleave_is_distinct_from_inactive_cleave() {
        let handles = RuntimeStateHandles::default();
        assert!(!handles.cleave_available());
        assert!(handles.observe_cleave().unwrap().is_none());

        handles.install_cleave(Arc::new(Mutex::new(CleaveProgress::default())));
        assert!(handles.cleave_available());
        assert!(!handles.observe_cleave().unwrap().unwrap().active);
    }

    #[test]
    fn cleave_observations_are_owned_and_isolated_by_runtime_instance() {
        let first = RuntimeStateHandles::default();
        let second = RuntimeStateHandles::default();
        first.install_cleave(Arc::new(Mutex::new(CleaveProgress {
            active: true,
            run_id: "first".into(),
            ..Default::default()
        })));
        second.install_cleave(Arc::new(Mutex::new(CleaveProgress {
            active: true,
            run_id: "second".into(),
            ..Default::default()
        })));

        let mut observed = first.observe_cleave().unwrap().unwrap();
        observed.run_id = "detached-copy".into();

        assert_eq!(first.observe_cleave().unwrap().unwrap().run_id, "first");
        assert_eq!(second.observe_cleave().unwrap().unwrap().run_id, "second");
    }

    #[test]
    fn cleave_install_and_clear_are_visible_across_handle_clones() {
        let handles = RuntimeStateHandles::default();
        let clone = handles.clone();

        handles.install_cleave(Arc::new(Mutex::new(CleaveProgress {
            active: true,
            run_id: "shared".into(),
            ..Default::default()
        })));
        assert_eq!(clone.observe_cleave().unwrap().unwrap().run_id, "shared");

        clone.clear_cleave();
        assert!(!handles.cleave_available());
        assert!(handles.observe_cleave().unwrap().is_none());
    }

    #[test]
    fn poisoned_cleave_state_is_explicit() {
        let handles = RuntimeStateHandles::default();
        let cleave = Arc::new(Mutex::new(CleaveProgress::default()));
        handles.install_cleave(cleave.clone());
        let _ = std::thread::spawn(move || {
            let _guard = cleave.lock().unwrap();
            panic!("poison cleave fixture");
        })
        .join();

        assert!(matches!(
            handles.observe_cleave(),
            Err(ObserveError::Poisoned(ObservationDomain::Cleave))
        ));
    }

    #[test]
    fn delegate_install_and_clear_are_visible_across_handle_clones() {
        let handles = RuntimeStateHandles::default();
        let clone = handles.clone();

        handles.install_delegate(Arc::new(Mutex::new(DelegateProgress {
            active: true,
            running: 1,
            ..Default::default()
        })));
        assert_eq!(clone.observe_delegate().unwrap().unwrap().running, 1);

        clone.clear_delegate();
        assert!(!handles.delegate_available());
        assert!(handles.observe_delegate().unwrap().is_none());
    }

    #[test]
    fn poisoned_delegate_state_is_explicit() {
        let handles = RuntimeStateHandles::default();
        let delegate = Arc::new(Mutex::new(DelegateProgress::default()));
        handles.install_delegate(delegate.clone());
        let _ = std::thread::spawn(move || {
            let _guard = delegate.lock().unwrap();
            panic!("poison delegate fixture");
        })
        .join();

        assert!(matches!(
            handles.observe_delegate(),
            Err(ObserveError::Poisoned(ObservationDomain::Delegate))
        ));
    }

    #[test]
    fn harness_install_mutate_and_clear_are_visible_across_clones() {
        let handles = RuntimeStateHandles::default();
        let clone = handles.clone();
        handles.install_harness(Arc::new(Mutex::new(HarnessStatus::default())));

        let (_, mutated) = handles
            .mutate_harness(|status| status.context_class = "Massive".into())
            .unwrap()
            .unwrap();
        assert_eq!(mutated.context_class, "Massive");
        assert_eq!(
            clone.observe_harness().unwrap().unwrap().context_class,
            "Massive"
        );

        clone.clear_harness();
        assert!(!handles.harness_available());
    }

    #[test]
    fn poisoned_harness_state_is_explicit() {
        let handles = RuntimeStateHandles::default();
        let harness = Arc::new(Mutex::new(HarnessStatus::default()));
        handles.install_harness(harness.clone());
        let _ = std::thread::spawn(move || {
            let _guard = harness.lock().unwrap();
            panic!("poison harness fixture");
        })
        .join();

        assert!(matches!(
            handles.observe_harness(),
            Err(ObserveError::Poisoned(ObservationDomain::Harness))
        ));
    }

    #[test]
    fn poisoned_harness_source_slot_is_explicit() {
        let handles = RuntimeStateHandles::default();
        let harness_slot = handles.harness.clone();
        let _ = std::thread::spawn(move || {
            let _guard = harness_slot.lock().unwrap();
            panic!("poison harness source slot fixture");
        })
        .join();

        assert!(matches!(
            handles.try_install_harness(Arc::new(Mutex::new(HarnessStatus::default()))),
            Err(ObserveError::Poisoned(ObservationDomain::Harness))
        ));
        assert!(matches!(
            handles.try_clear_harness(),
            Err(ObserveError::Poisoned(ObservationDomain::Harness))
        ));
    }

    #[test]
    fn lifecycle_publication_stores_before_calling_publisher() {
        let handles = RuntimeStateHandles::default();
        let snapshot = omegon_traits::RuntimeLifecycleSnapshot {
            operation_id: "restart-1".into(),
            kind: omegon_traits::RuntimeLifecycleKind::Restart,
            phase: omegon_traits::RuntimeLifecyclePhase::Queued,
            message: "restart requested".into(),
            session_id: None,
            target_version: None,
            reconnect_required: true,
        };
        handles
            .publish_runtime_lifecycle(snapshot.clone(), |published| {
                assert_eq!(published, &snapshot);
                assert_eq!(
                    handles.observe_runtime_lifecycle().unwrap(),
                    Some(snapshot.clone())
                );
            })
            .unwrap();
    }

    #[test]
    fn poisoned_lifecycle_slot_does_not_publish() {
        let handles = RuntimeStateHandles::default();
        let lifecycle_slot = handles.runtime_lifecycle.clone();
        let _ = std::thread::spawn(move || {
            let _guard = lifecycle_slot.lock().unwrap();
            panic!("poison lifecycle slot fixture");
        })
        .join();
        let snapshot = omegon_traits::RuntimeLifecycleSnapshot {
            operation_id: "restart-poisoned".into(),
            kind: omegon_traits::RuntimeLifecycleKind::Restart,
            phase: omegon_traits::RuntimeLifecyclePhase::Queued,
            message: "restart requested".into(),
            session_id: None,
            target_version: None,
            reconnect_required: true,
        };
        let published = std::sync::atomic::AtomicBool::new(false);

        assert!(matches!(
            handles.publish_runtime_lifecycle(snapshot, |_| {
                published.store(true, std::sync::atomic::Ordering::SeqCst);
            }),
            Err(ObserveError::Poisoned(ObservationDomain::RuntimeLifecycle))
        ));
        assert!(!published.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn poisoned_session_state_is_explicit() {
        let handles = RuntimeStateHandles::default();
        let session = handles.session.state.clone();
        let _ = std::thread::spawn(move || {
            let _guard = session.lock().unwrap();
            panic!("poison session fixture");
        })
        .join();

        assert!(matches!(
            handles.session.try_update_counters(1, 2, 3),
            Err(ObserveError::Poisoned(ObservationDomain::Session))
        ));
        assert!(matches!(
            handles.session.try_set_busy(true),
            Err(ObserveError::Poisoned(ObservationDomain::Session))
        ));
        assert_eq!(
            handles.session.observe(),
            Err(ObserveError::Poisoned(ObservationDomain::Session))
        );
    }
}
