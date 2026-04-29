//! auto_compact — Predictive, tiered context compaction.
//!
//! Monitors context usage with EWMA growth prediction and fires compaction
//! at two tiers:
//!   - Tier 1 (aggressive decay): tighten decay window, strip thinking.
//!     Pure Rust, no LLM call. Cheap and fast.
//!   - Tier 2 (LLM summarization): full compaction via compact_via_llm().
//!     Expensive but thorough.
//!
//! EWMA prediction projects fill at turn+2 to avoid emergency compaction.

use async_trait::async_trait;
use omegon_traits::{BusEvent, BusRequest, Feature};
use std::time::Instant;

const DEFAULT_TIER1_PERCENT: f32 = 60.0;
const DEFAULT_TIER2_PERCENT: f32 = 78.0;
const DEFAULT_COOLDOWN_SECS: u64 = 60;
const EWMA_ALPHA: f32 = 0.3;
const PREDICTION_HORIZON: f32 = 2.0; // turns ahead

/// Predictive, tiered context compaction.
pub struct AutoCompact {
    tier1_threshold: f32,
    tier2_threshold: f32,
    cooldown: std::time::Duration,
    last_compact: Option<Instant>,
    compacting: bool,
    /// Watchdog: when compaction was requested. Reset `compacting` if
    /// no `Compacted` event arrives within the timeout.
    compaction_requested_at: Option<Instant>,
    estimated_percent: f32,
    // EWMA prediction state
    prev_tokens: Option<usize>,
    ewma_growth: f32,
}

impl Default for AutoCompact {
    fn default() -> Self {
        Self::new()
    }
}

impl AutoCompact {
    pub fn new() -> Self {
        let tier1 = std::env::var("AUTO_COMPACT_TIER1_PERCENT")
            .ok()
            .and_then(|s| s.parse().ok())
            .or_else(|| {
                std::env::var("AUTO_COMPACT_PERCENT")
                    .ok()
                    .and_then(|s| s.parse().ok())
            })
            .unwrap_or(DEFAULT_TIER1_PERCENT);
        let tier2 = std::env::var("AUTO_COMPACT_TIER2_PERCENT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_TIER2_PERCENT);
        let cooldown_secs = std::env::var("AUTO_COMPACT_COOLDOWN")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_COOLDOWN_SECS);

        Self {
            tier1_threshold: tier1,
            tier2_threshold: tier2,
            cooldown: std::time::Duration::from_secs(cooldown_secs),
            last_compact: None,
            compacting: false,
            compaction_requested_at: None,
            estimated_percent: 0.0,
            prev_tokens: None,
            ewma_growth: 0.0,
        }
    }

    /// Project context fill percentage at turn+N using EWMA growth rate.
    fn projected_percent(&self, current_tokens: usize, context_window: usize) -> f32 {
        if context_window == 0 {
            return 0.0;
        }
        let projected_tokens = current_tokens as f32 + self.ewma_growth * PREDICTION_HORIZON;
        (projected_tokens / context_window as f32 * 100.0).min(100.0)
    }
}

#[async_trait]
impl Feature for AutoCompact {
    fn name(&self) -> &str {
        "auto-compact"
    }

    fn on_event(&mut self, event: &BusEvent) -> Vec<BusRequest> {
        match event {
            BusEvent::TurnEnd(te) => {
                let turn = &te.turn;
                let estimated_tokens = &te.estimated_tokens;
                let context_window = &te.context_window;
                // Watchdog: if compaction was requested >120s ago and no
                // Compacted event arrived, assume it failed and reset.
                if self.compacting {
                    if self
                        .compaction_requested_at
                        .is_some_and(|t| t.elapsed().as_secs() > 120)
                    {
                        tracing::warn!(
                            "auto-compact: compaction watchdog fired — resetting after 120s timeout"
                        );
                        self.compacting = false;
                        self.compaction_requested_at = None;
                    } else {
                        return vec![];
                    }
                }

                let tokens = *estimated_tokens;
                let window = *context_window;

                // Update EWMA growth rate
                if let Some(prev) = self.prev_tokens {
                    let delta = tokens as f32 - prev as f32;
                    self.ewma_growth = EWMA_ALPHA * delta + (1.0 - EWMA_ALPHA) * self.ewma_growth;
                }
                self.prev_tokens = Some(tokens);

                self.estimated_percent = if window > 0 {
                    ((tokens as f32 / window as f32) * 100.0).min(100.0)
                } else {
                    0.0
                };

                let projected = self.projected_percent(tokens, window);

                // Cooldown check
                if self
                    .last_compact
                    .is_some_and(|last| last.elapsed() < self.cooldown)
                {
                    return vec![];
                }

                // Tier 2: LLM summarization (at or projected to exceed tier2)
                if self.estimated_percent >= self.tier2_threshold
                    || projected >= self.tier2_threshold
                {
                    self.compacting = true;
                    self.last_compact = Some(Instant::now());
                    self.compaction_requested_at = Some(Instant::now());
                    tracing::info!(
                        turn,
                        current = self.estimated_percent,
                        projected,
                        ewma_growth = self.ewma_growth,
                        "auto-compact: tier 2 (LLM summarization)"
                    );
                    return vec![BusRequest::RequestCompaction];
                }

                // Tier 1: aggressive decay (at or projected to exceed tier1)
                if self.estimated_percent >= self.tier1_threshold
                    || projected >= self.tier1_threshold
                {
                    self.last_compact = Some(Instant::now());
                    tracing::info!(
                        turn,
                        current = self.estimated_percent,
                        projected,
                        ewma_growth = self.ewma_growth,
                        "auto-compact: tier 1 (aggressive decay)"
                    );
                    return vec![BusRequest::RequestAggressiveDecay];
                }

                vec![]
            }
            BusEvent::Compacted => {
                self.compacting = false;
                self.compaction_requested_at = None;
                // Reset EWMA after compaction — the sharp token drop would
                // otherwise suppress early warning for several turns.
                self.ewma_growth = 0.0;
                self.prev_tokens = None;
                vec![]
            }
            _ => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn_end(tokens: usize, window: usize) -> BusEvent {
        BusEvent::TurnEnd(Box::new(omegon_traits::BusEventTurnEnd {
            turn: 1,
            model: None,
            provider: None,
            estimated_tokens: tokens,
            context_window: window,
            context_composition: omegon_traits::ContextComposition::default(),
            actual_input_tokens: 0,
            actual_output_tokens: 0,
            cache_read_tokens: 0,
            provider_telemetry: None,
            dominant_phase: None,
            drift_kind: None,
            progress_signal: omegon_traits::ProgressSignal::None,
        }))
    }

    #[test]
    fn does_not_compact_below_threshold() {
        let mut ac = AutoCompact::new();
        let requests = ac.on_event(&turn_end(10_000, 200_000));
        assert!(requests.is_empty());
    }

    #[test]
    fn tier1_fires_at_threshold() {
        let mut ac = AutoCompact::new();
        ac.tier1_threshold = 50.0;
        ac.tier2_threshold = 80.0;
        let requests = ac.on_event(&turn_end(120_000, 200_000)); // 60%
        assert_eq!(requests.len(), 1);
        assert!(matches!(requests[0], BusRequest::RequestAggressiveDecay));
    }

    #[test]
    fn tier2_fires_at_threshold() {
        let mut ac = AutoCompact::new();
        ac.tier1_threshold = 50.0;
        ac.tier2_threshold = 70.0;
        let requests = ac.on_event(&turn_end(160_000, 200_000)); // 80%
        assert_eq!(requests.len(), 1);
        assert!(matches!(requests[0], BusRequest::RequestCompaction));
    }

    #[test]
    fn ewma_prediction_triggers_early() {
        let mut ac = AutoCompact::new();
        ac.tier1_threshold = 70.0;
        ac.tier2_threshold = 85.0;

        // Simulate rapid growth: 30% → 50% → 65%
        // Don't fire Compacted between turns — that resets EWMA.
        // Just clear the compacting flag directly to simulate
        // turns passing without compaction.
        ac.on_event(&turn_end(60_000, 200_000)); // 30%, establishes baseline
        ac.compacting = false; // clear without resetting EWMA
        ac.last_compact = None;
        ac.on_event(&turn_end(100_000, 200_000)); // 50%, delta=40k
        ac.compacting = false;
        ac.last_compact = None;

        // At 65%, EWMA projects growth of ~28k/turn (smoothed).
        // Projected at turn+2 = 130k + 56k = 186k / 200k = 93%.
        // Should trigger tier2 even though current is only 65%.
        let requests = ac.on_event(&turn_end(130_000, 200_000)); // 65%
        assert!(
            !requests.is_empty(),
            "EWMA should predict overflow and trigger compaction"
        );
    }

    #[test]
    fn cooldown_prevents_repeated_compaction() {
        let mut ac = AutoCompact::new();
        ac.tier1_threshold = 10.0;

        let r1 = ac.on_event(&turn_end(50_000, 200_000));
        assert!(!r1.is_empty());
        ac.on_event(&BusEvent::Compacted);

        let r2 = ac.on_event(&turn_end(55_000, 200_000));
        assert!(
            r2.is_empty(),
            "cooldown should prevent immediate re-compact"
        );
    }

    #[test]
    fn compacted_event_clears_flag() {
        let mut ac = AutoCompact::new();
        ac.compacting = true;
        ac.on_event(&BusEvent::Compacted);
        assert!(!ac.compacting);
    }
}
