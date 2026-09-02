use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum InvocationLeaseState {
    Open = 0,
    Dispatching = 1,
    Completed = 2,
    Failed = 3,
    Revoked = 4,
}

#[derive(Debug, Clone)]
pub struct InvocationLeaseStateMachine {
    state: Arc<AtomicU8>,
}

impl Default for InvocationLeaseStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl InvocationLeaseStateMachine {
    pub fn new() -> Self {
        Self {
            state: Arc::new(AtomicU8::new(InvocationLeaseState::Open as u8)),
        }
    }

    pub fn state(&self) -> InvocationLeaseState {
        match self.state.load(Ordering::Acquire) {
            0 => InvocationLeaseState::Open,
            1 => InvocationLeaseState::Dispatching,
            2 => InvocationLeaseState::Completed,
            3 => InvocationLeaseState::Failed,
            _ => InvocationLeaseState::Revoked,
        }
    }

    pub fn is_dispatching(&self) -> bool {
        self.state.load(Ordering::Acquire) == InvocationLeaseState::Dispatching as u8
    }

    pub fn claim_dispatch(&self) -> Result<(), InvocationLeaseTransitionError> {
        self.state
            .compare_exchange(
                InvocationLeaseState::Open as u8,
                InvocationLeaseState::Dispatching as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .map(|_| ())
            .map_err(|_| InvocationLeaseTransitionError::Closed)
    }

    pub fn close(&self, terminal: InvocationLeaseState) -> bool {
        debug_assert!(matches!(
            terminal,
            InvocationLeaseState::Completed
                | InvocationLeaseState::Failed
                | InvocationLeaseState::Revoked
        ));
        self.state
            .compare_exchange(
                InvocationLeaseState::Dispatching as u8,
                terminal as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    pub fn revoke(&self) -> bool {
        loop {
            let current = self.state.load(Ordering::Acquire);
            if current >= InvocationLeaseState::Completed as u8 {
                return false;
            }
            if self
                .state
                .compare_exchange(
                    current,
                    InvocationLeaseState::Revoked as u8,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
            {
                return true;
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum InvocationLeaseTransitionError {
    #[error("invocation lease is no longer open")]
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolBudgetExhausted {
    pub admitted: u32,
    pub observed: u32,
}

impl std::fmt::Display for ToolBudgetExhausted {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "tool budget exhausted: observed {}; admitted {}",
            self.observed, self.admitted
        )
    }
}

impl std::error::Error for ToolBudgetExhausted {}

#[derive(Debug, Clone)]
pub struct ToolInvocationBudget {
    admitted: Option<u32>,
    observed: Arc<AtomicU32>,
    exhausted: Arc<AtomicBool>,
}

impl Default for ToolInvocationBudget {
    fn default() -> Self {
        Self::new(None)
    }
}

impl PartialEq for ToolInvocationBudget {
    fn eq(&self, other: &Self) -> bool {
        self.admitted == other.admitted
            && Arc::ptr_eq(&self.observed, &other.observed)
            && Arc::ptr_eq(&self.exhausted, &other.exhausted)
    }
}

impl Eq for ToolInvocationBudget {}

impl ToolInvocationBudget {
    pub fn new(admitted: Option<u32>) -> Self {
        Self {
            admitted,
            observed: Arc::new(AtomicU32::new(0)),
            exhausted: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn observed(&self) -> u32 {
        self.observed.load(Ordering::Acquire)
    }

    pub fn exhausted(&self) -> bool {
        self.exhausted.load(Ordering::Acquire)
    }

    pub fn exhaustion(&self) -> Option<ToolBudgetExhausted> {
        self.exhausted().then(|| ToolBudgetExhausted {
            admitted: self.admitted.unwrap_or(u32::MAX),
            observed: self.observed(),
        })
    }

    pub fn issue_lease(&self) -> Result<InvocationLeaseStateMachine, ToolBudgetExhausted> {
        loop {
            let observed = self.observed();
            if let Some(admitted) = self.admitted
                && observed >= admitted
            {
                self.exhausted.store(true, Ordering::Release);
                return Err(ToolBudgetExhausted { admitted, observed });
            }
            if self
                .observed
                .compare_exchange(
                    observed,
                    observed.saturating_add(1),
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
            {
                return Ok(InvocationLeaseStateMachine::new());
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BoundedToolCallError<E> {
    #[error(transparent)]
    BudgetExhausted(#[from] ToolBudgetExhausted),
    #[error(transparent)]
    Lease(#[from] InvocationLeaseTransitionError),
    #[error("tool dispatch failed: {0}")]
    Dispatch(E),
}

pub async fn execute_bounded_tool_call<T, E, Dispatch, DispatchFuture>(
    budget: &mut ToolInvocationBudget,
    dispatch: Dispatch,
) -> Result<T, BoundedToolCallError<E>>
where
    Dispatch: FnOnce() -> DispatchFuture,
    DispatchFuture: Future<Output = Result<T, E>>,
{
    let lease = budget.issue_lease()?;
    lease.claim_dispatch()?;
    match dispatch().await {
        Ok(result) => {
            lease.close(InvocationLeaseState::Completed);
            Ok(result)
        }
        Err(error) => {
            lease.close(InvocationLeaseState::Failed);
            Err(BoundedToolCallError::Dispatch(error))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lease_claim_and_terminal_close_are_exactly_once() {
        let lease = InvocationLeaseStateMachine::new();
        lease.claim_dispatch().unwrap();
        assert!(lease.close(InvocationLeaseState::Completed));
        assert!(!lease.close(InvocationLeaseState::Failed));
        assert_eq!(lease.state(), InvocationLeaseState::Completed);
    }

    #[test]
    fn tool_budget_checks_before_constructing_the_next_lease() {
        let budget = ToolInvocationBudget::new(Some(2));

        let below = budget.issue_lease().unwrap();
        assert_eq!(below.state(), InvocationLeaseState::Open);
        let exact = budget.issue_lease().unwrap();
        assert_eq!(exact.state(), InvocationLeaseState::Open);
        let above = budget.issue_lease().unwrap_err();

        assert_eq!(above.admitted, 2);
        assert_eq!(above.observed, 2);
        assert_eq!(budget.observed(), 2);
    }

    #[test]
    fn unlimited_tool_budget_keeps_issuing_leases() {
        let budget = ToolInvocationBudget::new(None);
        for _ in 0..3 {
            assert!(budget.issue_lease().is_ok());
        }
        assert_eq!(budget.observed(), 3);
    }

    #[test]
    fn cloned_budget_admits_only_the_exact_parallel_boundary() {
        let budget = ToolInvocationBudget::new(Some(1));
        let first = budget.clone();
        let second = budget.clone();
        let outcomes = [
            std::thread::spawn(move || first.issue_lease().is_ok()),
            std::thread::spawn(move || second.issue_lease().is_ok()),
        ]
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect::<Vec<_>>();

        assert_eq!(outcomes.iter().filter(|admitted| **admitted).count(), 1);
        assert_eq!(budget.observed(), 1);
        assert!(budget.exhausted());
    }
}
