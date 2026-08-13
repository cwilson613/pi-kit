use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TuiDrawReason {
    OperatorInput,
    BackgroundEvent,
    TimerTick,
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
    requested_revision: u64,
    drawn_revision: u64,
    urgent_revision: Option<u64>,
    last_draw: Instant,
    consecutive_over_budget_draws: u8,
    presentation_retry_at: Option<Instant>,
}

impl TuiFrameScheduler {
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            min_frame_interval: Duration::from_millis(16),
            max_idle_poll: Duration::from_secs(1),
            agent_budget: AgentDrainBudget {
                max_events: 64,
                max_duration: Duration::from_millis(4),
            },
            requested_revision: 1,
            drawn_revision: 0,
            urgent_revision: Some(1),
            last_draw: now.checked_sub(Duration::from_millis(16)).unwrap_or(now),
            consecutive_over_budget_draws: 0,
            presentation_retry_at: None,
        }
    }

    pub(crate) fn mark_dirty(&mut self, reason: TuiDrawReason) {
        self.requested_revision = self
            .requested_revision
            .checked_add(1)
            .expect("TUI frame revision exhausted");
        if matches!(reason, TuiDrawReason::OperatorInput) {
            self.urgent_revision = Some(self.requested_revision);
        }
    }

    pub(crate) fn agent_budget(&self) -> AgentDrainBudget {
        self.agent_budget
    }

    pub(crate) fn receiver_budget(&self) -> AgentDrainBudget {
        self.agent_budget
    }

    pub(crate) fn should_draw(&self, now: Instant) -> bool {
        if self
            .presentation_retry_at
            .is_some_and(|retry_at| now < retry_at)
        {
            return false;
        }
        self.requested_revision > self.drawn_revision
            && (self.is_urgent() || now.duration_since(self.last_draw) >= self.min_frame_interval)
    }

    pub(crate) fn presentation_degraded(&self) -> bool {
        self.presentation_retry_at.is_some()
    }

    pub(crate) fn observe_draw_duration(&mut self, elapsed: Duration, completed_at: Instant) {
        const DRAW_BUDGET: Duration = Duration::from_millis(250);
        const OPEN_AFTER: u8 = 3;
        const RETRY_DELAY: Duration = Duration::from_millis(250);

        if elapsed > DRAW_BUDGET {
            self.consecutive_over_budget_draws = self
                .consecutive_over_budget_draws
                .saturating_add(1)
                .min(OPEN_AFTER);
            if self.consecutive_over_budget_draws >= OPEN_AFTER {
                self.presentation_retry_at = Some(completed_at + RETRY_DELAY);
                self.mark_dirty(TuiDrawReason::BackgroundEvent);
            }
        } else {
            self.consecutive_over_budget_draws = 0;
            self.presentation_retry_at = None;
        }
    }

    pub(crate) fn is_urgent(&self) -> bool {
        self.urgent_revision
            .is_some_and(|revision| revision > self.drawn_revision)
    }

    /// Capture the exact dirty revision represented by a frame. Mutations that
    /// arrive while the draw is running receive a later revision and therefore
    /// remain dirty after this frame completes.
    pub(crate) fn begin_draw(&self) -> Option<u64> {
        (self.requested_revision > self.drawn_revision).then_some(self.requested_revision)
    }

    pub(crate) fn after_draw(&mut self, drawn_revision: u64, completed_at: Instant) {
        self.drawn_revision = self.drawn_revision.max(drawn_revision);
        if self
            .urgent_revision
            .is_some_and(|revision| revision <= self.drawn_revision)
        {
            self.urgent_revision = None;
        }
        self.last_draw = completed_at;
    }

    pub(crate) fn idle_poll_timeout(&self, now: Instant) -> Duration {
        if let Some(retry_at) = self.presentation_retry_at
            && now < retry_at
        {
            return retry_at.duration_since(now).min(self.max_idle_poll);
        }
        if self.requested_revision > self.drawn_revision {
            self.min_frame_interval
                .saturating_sub(now.duration_since(self.last_draw))
                .min(self.max_idle_poll)
        } else {
            self.max_idle_poll
                .saturating_sub(now.duration_since(self.last_draw))
        }
    }

    pub(crate) fn mark_timer_due(&mut self, now: Instant) {
        if self.requested_revision == self.drawn_revision
            && now.duration_since(self.last_draw) >= self.max_idle_poll
        {
            self.mark_dirty(TuiDrawReason::TimerTick);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_over_budget_draws_open_breaker_with_bounded_backoff() {
        let now = Instant::now();
        let mut scheduler = TuiFrameScheduler::new(now);
        scheduler.after_draw(scheduler.begin_draw().unwrap(), now);

        for offset in 1..=3 {
            scheduler.observe_draw_duration(
                Duration::from_millis(300),
                now + Duration::from_millis(offset),
            );
        }

        assert!(scheduler.presentation_degraded());
        assert!(!scheduler.should_draw(now + Duration::from_millis(50)));
        assert!(scheduler.should_draw(now + Duration::from_millis(300)));
    }

    #[test]
    fn successful_half_open_probe_closes_breaker() {
        let now = Instant::now();
        let mut scheduler = TuiFrameScheduler::new(now);
        scheduler.after_draw(scheduler.begin_draw().unwrap(), now);
        for offset in 1..=3 {
            scheduler.observe_draw_duration(
                Duration::from_millis(300),
                now + Duration::from_millis(offset),
            );
        }
        let retry = now + Duration::from_millis(300);
        assert!(scheduler.should_draw(retry));
        scheduler.observe_draw_duration(Duration::from_millis(5), retry);

        assert!(!scheduler.presentation_degraded());
    }

    #[test]
    fn operator_input_remains_dirty_while_breaker_is_open() {
        let now = Instant::now();
        let mut scheduler = TuiFrameScheduler::new(now);
        scheduler.after_draw(scheduler.begin_draw().unwrap(), now);
        for offset in 1..=3 {
            scheduler.observe_draw_duration(
                Duration::from_millis(300),
                now + Duration::from_millis(offset),
            );
        }
        scheduler.mark_dirty(TuiDrawReason::OperatorInput);

        assert!(scheduler.is_urgent());
        assert!(!scheduler.should_draw(now + Duration::from_millis(10)));
        assert!(scheduler.should_draw(now + Duration::from_millis(300)));
    }

    #[test]
    fn dirty_state_raised_during_draw_survives_completion() {
        let now = Instant::now();
        let mut scheduler = TuiFrameScheduler::new(now);
        let drawn_revision = scheduler.begin_draw().expect("initial frame");

        scheduler.mark_dirty(TuiDrawReason::BackgroundEvent);
        scheduler.after_draw(drawn_revision, now + Duration::from_millis(1));

        assert!(scheduler.should_draw(now + Duration::from_millis(18)));
    }

    #[test]
    fn operator_input_forces_immediate_draw() {
        let now = Instant::now();
        let mut scheduler = TuiFrameScheduler::new(now);
        scheduler.after_draw(scheduler.begin_draw().expect("frame"), now);

        scheduler.mark_dirty(TuiDrawReason::OperatorInput);

        assert!(scheduler.should_draw(now + Duration::from_millis(1)));
    }

    #[test]
    fn operator_urgency_survives_until_the_draw() {
        let now = Instant::now();
        let mut scheduler = TuiFrameScheduler::new(now);
        scheduler.after_draw(scheduler.begin_draw().expect("frame"), now);
        scheduler.mark_dirty(TuiDrawReason::OperatorInput);
        scheduler.mark_dirty(TuiDrawReason::BackgroundEvent);

        assert!(scheduler.is_urgent());
        scheduler.after_draw(
            scheduler.begin_draw().expect("frame"),
            now + Duration::from_millis(1),
        );
        assert!(!scheduler.is_urgent());
    }

    #[test]
    fn draw_completion_restarts_idle_deadline() {
        let now = Instant::now();
        let mut scheduler = TuiFrameScheduler::new(now);
        scheduler.after_draw(
            scheduler.begin_draw().expect("frame"),
            now + Duration::from_millis(40),
        );

        assert_eq!(
            scheduler.idle_poll_timeout(now + Duration::from_millis(40)),
            Duration::from_secs(1)
        );
    }

    #[test]
    fn background_events_are_coalesced_to_frame_interval() {
        let now = Instant::now();
        let mut scheduler = TuiFrameScheduler::new(now);
        scheduler.after_draw(scheduler.begin_draw().expect("frame"), now);

        scheduler.mark_dirty(TuiDrawReason::BackgroundEvent);

        assert!(!scheduler.should_draw(now + Duration::from_millis(1)));
        assert!(scheduler.should_draw(now + Duration::from_millis(16)));
    }

    #[test]
    fn dirty_background_frame_waits_only_until_frame_deadline() {
        let now = Instant::now();
        let mut scheduler = TuiFrameScheduler::new(now);
        scheduler.after_draw(scheduler.begin_draw().expect("frame"), now);
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
    fn clean_idle_uses_bounded_background_refresh() {
        let now = Instant::now();
        let mut scheduler = TuiFrameScheduler::new(now);
        scheduler.after_draw(scheduler.begin_draw().expect("frame"), now);

        assert_eq!(scheduler.idle_poll_timeout(now), Duration::from_secs(1));
        assert_eq!(
            scheduler.idle_poll_timeout(now + Duration::from_millis(750)),
            Duration::from_millis(250)
        );
        assert_eq!(
            scheduler.idle_poll_timeout(now + Duration::from_secs(1)),
            Duration::ZERO
        );
    }

    #[test]
    fn elapsed_idle_deadline_marks_timer_frame_due() {
        let now = Instant::now();
        let mut scheduler = TuiFrameScheduler::new(now);
        scheduler.after_draw(scheduler.begin_draw().expect("frame"), now);

        scheduler.mark_timer_due(now + Duration::from_millis(999));
        assert!(!scheduler.should_draw(now + Duration::from_millis(999)));

        scheduler.mark_timer_due(now + Duration::from_secs(1));
        assert!(scheduler.should_draw(now + Duration::from_secs(1)));
    }

    #[test]
    fn agent_budget_is_bounded() {
        let scheduler = TuiFrameScheduler::new(Instant::now());
        let budget = scheduler.agent_budget();

        assert_eq!(budget.max_events, 64);
        assert_eq!(budget.max_duration, Duration::from_millis(4));
    }

    #[derive(Debug, serde::Serialize)]
    struct DeterministicScrollStreamReport {
        scenario: &'static str,
        stream_events: usize,
        input_events: usize,
        frames: usize,
        agent_budget_hits: usize,
        max_events_before_input: usize,
        input_to_frame_ms: Percentiles,
    }

    #[derive(Debug, serde::Serialize)]
    struct Percentiles {
        p50: u64,
        p95: u64,
        max: u64,
    }

    fn percentiles(mut samples: Vec<u64>) -> Percentiles {
        samples.sort_unstable();
        let at = |fraction: f64| {
            let index = ((samples.len() - 1) as f64 * fraction).ceil() as usize;
            samples[index]
        };
        Percentiles {
            p50: at(0.50),
            p95: at(0.95),
            max: *samples.last().expect("non-empty samples"),
        }
    }

    /// Deterministic virtual-time scheduler benchmark. It models a 60 Hz token
    /// stream plus operator scroll input without requiring an inaccessible TUI.
    #[test]
    #[ignore = "run with `just bench-tui-scroll-stream`"]
    fn deterministic_streaming_scroll_trace() {
        let origin = Instant::now();
        let mut scheduler = TuiFrameScheduler::new(origin);
        scheduler.after_draw(scheduler.begin_draw().expect("frame"), origin);

        let mut pending_stream_events = 0usize;
        let mut stream_events = 0usize;
        let mut input_events = 0usize;
        let mut frames = 0usize;
        let mut agent_budget_hits = 0usize;
        let max_events_before_input = 0usize;
        let mut pending_input_at: Option<Duration> = None;
        let mut input_to_frame_ms = Vec::new();

        // Ten seconds at 1 ms virtual-time resolution. Stream events arrive at
        // 60 Hz and scroll input at 20 Hz during seconds two through six.
        for elapsed_ms in 0_u64..10_000 {
            let elapsed = Duration::from_millis(elapsed_ms);
            let now = origin + elapsed;

            if elapsed_ms % 17 == 0 {
                pending_stream_events += 1;
                stream_events += 1;
            }
            if (2_000..6_000).contains(&elapsed_ms) && elapsed_ms % 50 == 0 {
                pending_input_at.get_or_insert(elapsed);
                input_events += 1;
            }

            // Operator input wins the scheduling pass before any background
            // drain. Therefore zero background events can precede ready input.
            if pending_input_at.is_some() {
                scheduler.mark_dirty(TuiDrawReason::OperatorInput);
            }

            if pending_stream_events > 0 {
                let handled = pending_stream_events.min(scheduler.agent_budget().max_events);
                pending_stream_events -= handled;
                if pending_stream_events > 0 {
                    agent_budget_hits += 1;
                }
                scheduler.mark_dirty(TuiDrawReason::BackgroundEvent);
            }

            if scheduler.should_draw(now) {
                frames += 1;
                if let Some(input_at) = pending_input_at.take() {
                    input_to_frame_ms.push(elapsed.saturating_sub(input_at).as_millis() as u64);
                }
                scheduler.after_draw(scheduler.begin_draw().expect("frame"), now);
            }
        }

        let report = DeterministicScrollStreamReport {
            scenario: "scheduler-60hz-stream-20hz-scroll",
            stream_events,
            input_events,
            frames,
            agent_budget_hits,
            max_events_before_input,
            input_to_frame_ms: percentiles(input_to_frame_ms),
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&report).expect("serialize benchmark report")
        );

        assert_eq!(report.input_events, 80);
        assert_eq!(report.max_events_before_input, 0);
        assert!(report.input_to_frame_ms.p95 <= 1, "{report:?}");
        assert!(report.input_to_frame_ms.max <= 1, "{report:?}");
    }
}
