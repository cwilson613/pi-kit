use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TuiDrawReason {
    OperatorInput,
    BackgroundEvent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AgentDrainBudget {
    pub max_events: usize,
    pub max_duration: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DrainOutcome {
    pub handled: usize,
    pub hit_budget: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct TuiFrameScheduler {
    min_frame_interval: Duration,
    max_idle_poll: Duration,
    agent_budget: AgentDrainBudget,
    dirty: bool,
    urgent: bool,
    last_draw: Instant,
}

impl TuiFrameScheduler {
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            min_frame_interval: Duration::from_millis(16),
            max_idle_poll: Duration::from_millis(16),
            agent_budget: AgentDrainBudget {
                max_events: 64,
                max_duration: Duration::from_millis(4),
            },
            dirty: true,
            urgent: true,
            last_draw: now.checked_sub(Duration::from_millis(16)).unwrap_or(now),
        }
    }

    pub(crate) fn mark_dirty(&mut self, reason: TuiDrawReason) {
        self.dirty = true;
        if matches!(reason, TuiDrawReason::OperatorInput) {
            self.urgent = true;
        }
    }

    pub(crate) fn agent_budget(&self) -> AgentDrainBudget {
        self.agent_budget
    }

    pub(crate) fn should_draw(&self, now: Instant) -> bool {
        self.dirty && (self.urgent || now.duration_since(self.last_draw) >= self.min_frame_interval)
    }

    pub(crate) fn after_draw(&mut self, now: Instant) {
        self.dirty = false;
        self.urgent = false;
        self.last_draw = now;
    }

    pub(crate) fn idle_poll_timeout(&self, now: Instant) -> Duration {
        if self.dirty {
            self.min_frame_interval
                .saturating_sub(now.duration_since(self.last_draw))
                .min(self.max_idle_poll)
        } else {
            self.max_idle_poll
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operator_input_forces_immediate_draw() {
        let now = Instant::now();
        let mut scheduler = TuiFrameScheduler::new(now);
        scheduler.after_draw(now);

        scheduler.mark_dirty(TuiDrawReason::OperatorInput);

        assert!(scheduler.should_draw(now + Duration::from_millis(1)));
    }

    #[test]
    fn background_events_are_coalesced_to_frame_interval() {
        let now = Instant::now();
        let mut scheduler = TuiFrameScheduler::new(now);
        scheduler.after_draw(now);

        scheduler.mark_dirty(TuiDrawReason::BackgroundEvent);

        assert!(!scheduler.should_draw(now + Duration::from_millis(1)));
        assert!(scheduler.should_draw(now + Duration::from_millis(16)));
    }

    #[test]
    fn dirty_background_frame_waits_only_until_frame_deadline() {
        let now = Instant::now();
        let mut scheduler = TuiFrameScheduler::new(now);
        scheduler.after_draw(now);
        scheduler.mark_dirty(TuiDrawReason::BackgroundEvent);

        assert_eq!(
            scheduler.idle_poll_timeout(now + Duration::from_millis(10)),
            Duration::from_millis(6)
        );
        assert_eq!(
            scheduler.idle_poll_timeout(now + Duration::from_millis(16)),
            Duration::ZERO
        );
    }

    #[test]
    fn agent_budget_is_bounded() {
        let scheduler = TuiFrameScheduler::new(Instant::now());
        let budget = scheduler.agent_budget();

        assert_eq!(budget.max_events, 64);
        assert_eq!(budget.max_duration, Duration::from_millis(4));
    }
}
