use std::time::{Duration, Instant};

pub(crate) const DEFAULT_ACTIVE_WORKER_PROBE_INTERVAL: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ActiveWorkerWaitObservation {
    pub(crate) elapsed: Duration,
    pub(crate) notification_count: u32,
    pub(crate) queue_depth: usize,
    pub(crate) notify_blocked_queue: bool,
    pub(crate) next_probe_after: Duration,
}

#[derive(Debug)]
pub(crate) struct ActiveWorkerWaitTelemetry {
    started_at: Instant,
    probe_interval: Duration,
    notification_count: u32,
    blocked_queue_notified: bool,
}

impl ActiveWorkerWaitTelemetry {
    pub(crate) fn new() -> Self {
        Self::with_started_at(Instant::now(), DEFAULT_ACTIVE_WORKER_PROBE_INTERVAL)
    }

    fn with_started_at(started_at: Instant, probe_interval: Duration) -> Self {
        Self {
            started_at,
            probe_interval,
            notification_count: 0,
            blocked_queue_notified: false,
        }
    }

    pub(crate) fn probe_interval(&self) -> Duration {
        self.probe_interval
    }

    pub(crate) fn observe(&mut self, queue_depth: usize) -> ActiveWorkerWaitObservation {
        self.observe_at(queue_depth, Instant::now())
    }

    fn observe_at(
        &mut self,
        queue_depth: usize,
        observed_at: Instant,
    ) -> ActiveWorkerWaitObservation {
        self.notification_count = self.notification_count.saturating_add(1);
        let notify_blocked_queue = queue_depth > 0 && !self.blocked_queue_notified;
        if notify_blocked_queue {
            self.blocked_queue_notified = true;
        }
        ActiveWorkerWaitObservation {
            elapsed: observed_at.saturating_duration_since(self.started_at),
            notification_count: self.notification_count,
            queue_depth,
            notify_blocked_queue,
            next_probe_after: self.probe_interval,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probes_count_monotonically_and_keep_fixed_schedule() {
        let started_at = Instant::now();
        let interval = Duration::from_secs(10);
        let mut telemetry = ActiveWorkerWaitTelemetry::with_started_at(started_at, interval);

        let first = telemetry.observe_at(0, started_at + interval);
        assert_eq!(first.elapsed, interval);
        assert_eq!(first.notification_count, 1);
        assert_eq!(first.next_probe_after, interval);
        assert!(!first.notify_blocked_queue);

        let second = telemetry.observe_at(0, started_at + interval * 2);
        assert_eq!(second.notification_count, 2);
        assert_eq!(second.next_probe_after, interval);
    }

    #[test]
    fn blocked_queue_notification_is_emitted_once_when_queue_becomes_nonempty() {
        let started_at = Instant::now();
        let mut telemetry =
            ActiveWorkerWaitTelemetry::with_started_at(started_at, Duration::from_secs(10));

        assert!(
            !telemetry
                .observe_at(0, started_at + Duration::from_secs(10))
                .notify_blocked_queue
        );
        let first_blocked = telemetry.observe_at(2, started_at + Duration::from_secs(20));
        assert!(first_blocked.notify_blocked_queue);
        assert_eq!(first_blocked.queue_depth, 2);
        assert!(
            !telemetry
                .observe_at(3, started_at + Duration::from_secs(30))
                .notify_blocked_queue
        );
    }

    #[test]
    fn notification_counter_saturates_instead_of_wrapping() {
        let started_at = Instant::now();
        let mut telemetry =
            ActiveWorkerWaitTelemetry::with_started_at(started_at, Duration::from_secs(1));
        telemetry.notification_count = u32::MAX;
        let observation = telemetry.observe_at(0, started_at + Duration::from_secs(1));
        assert_eq!(observation.notification_count, u32::MAX);
    }
}
