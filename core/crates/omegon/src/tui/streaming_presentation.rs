use std::collections::VecDeque;

use omegon_traits::AgentEvent;

/// The kind of progressive content represented by a published delta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StreamContentKind {
    Assistant,
    Thinking,
}

const MAX_AUTHORITATIVE_BYTES: usize = 1024 * 1024;
const MAX_PENDING_DELTA_BYTES: usize = 256 * 1024;
const MAX_BLOCKED_EVENTS: usize = 256;

/// TUI-local stream publication state. Runtime events remain authoritative; this
/// controller preserves their order while deciding when progressive content
/// becomes a presentation revision and when later events may overtake it.
#[derive(Debug, Default)]
pub(super) struct StreamingPresentationController {
    generation: u64,
    accumulated_revision: u64,
    published_revision: u64,
    drawn_revision: u64,
    unpublished_content: bool,
    blocked_events: VecDeque<AgentEvent>,
    authoritative_text: String,
    published_len: usize,
    pending_deltas: Vec<StreamDelta>,
    pending_delta_bytes: usize,
    snapshot_rebuild_required: bool,
}

#[derive(Debug, Clone)]
struct StreamDelta {
    kind: StreamContentKind,
    text: String,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct StreamIngestDecision {
    pub(super) apply_now: bool,
    pub(super) publication_due: bool,
}

pub(super) struct StreamPublication {
    pub(super) generation: u64,
    pub(super) revision: u64,
    pub(super) deltas: Vec<(StreamContentKind, String)>,
}

impl StreamingPresentationController {
    pub(super) fn classify(&mut self, event: AgentEvent) -> StreamIngestDecision {
        // Session reset and a superseding message start are out-of-band
        // lifecycle boundaries. Neither may remain trapped behind presentation
        // work owned by the generation it invalidates.
        if matches!(event, AgentEvent::SessionReset) {
            self.reset_generation();
            return StreamIngestDecision {
                apply_now: true,
                publication_due: false,
            };
        }
        if matches!(event, AgentEvent::MessageStart { .. }) && !self.blocked_events.is_empty() {
            self.start_generation();
            self.blocked_events.clear();
            return StreamIngestDecision {
                apply_now: true,
                publication_due: false,
            };
        }

        // Once an event is blocked behind unpublished content, every later
        // event must remain behind it. Replaying the front event after a draw
        // re-enters this method only after it has been removed from the queue.
        if !self.blocked_events.is_empty() {
            self.queue_blocked(event);
            return StreamIngestDecision {
                apply_now: false,
                publication_due: self.unpublished_content,
            };
        }

        match event {
            AgentEvent::MessageStart { .. } => {
                self.start_generation();
                StreamIngestDecision {
                    apply_now: true,
                    publication_due: false,
                }
            }
            AgentEvent::MessageChunk { text } => {
                self.accumulate(StreamContentKind::Assistant, text);
                StreamIngestDecision {
                    apply_now: false,
                    publication_due: true,
                }
            }
            AgentEvent::ThinkingChunk { text } => {
                self.accumulate(StreamContentKind::Thinking, text);
                StreamIngestDecision {
                    apply_now: false,
                    publication_due: true,
                }
            }
            event if self.unpublished_content => {
                self.queue_blocked(event);
                StreamIngestDecision {
                    apply_now: false,
                    publication_due: true,
                }
            }
            _ => StreamIngestDecision {
                apply_now: true,
                publication_due: false,
            },
        }
    }

    fn queue_blocked(&mut self, event: AgentEvent) {
        if self.blocked_events.len() >= MAX_BLOCKED_EVENTS {
            self.snapshot_rebuild_required = true;
            return;
        }
        self.blocked_events.push_back(event);
    }

    fn reset_generation(&mut self) {
        self.start_generation();
        self.blocked_events.clear();
    }

    fn start_generation(&mut self) {
        self.generation = self
            .generation
            .checked_add(1)
            .expect("stream presentation generation exhausted");
        self.accumulated_revision = 0;
        self.published_revision = 0;
        self.drawn_revision = 0;
        self.unpublished_content = false;
        self.authoritative_text.clear();
        self.published_len = 0;
        self.pending_deltas.clear();
        self.pending_delta_bytes = 0;
        self.snapshot_rebuild_required = false;
    }

    fn accumulate(&mut self, kind: StreamContentKind, text: String) {
        if self.authoritative_text.len().saturating_add(text.len()) > MAX_AUTHORITATIVE_BYTES {
            self.snapshot_rebuild_required = true;
            self.pending_deltas.clear();
            self.pending_delta_bytes = 0;
            self.unpublished_content = true;
            return;
        }
        self.authoritative_text.push_str(&text);
        if self.pending_delta_bytes.saturating_add(text.len()) > MAX_PENDING_DELTA_BYTES {
            self.snapshot_rebuild_required = true;
            self.pending_deltas.clear();
            self.pending_delta_bytes = 0;
        } else {
            self.pending_delta_bytes += text.len();
            if let Some(last) = self.pending_deltas.last_mut()
                && last.kind == kind
            {
                last.text.push_str(&text);
            } else {
                self.pending_deltas.push(StreamDelta { kind, text });
            }
        }
        self.accumulated_revision = self
            .accumulated_revision
            .checked_add(1)
            .expect("stream presentation revision exhausted");
        self.unpublished_content = true;
    }

    pub(super) fn publish(&mut self) -> Option<StreamPublication> {
        if !self.unpublished_content {
            return None;
        }
        self.published_revision = self.accumulated_revision;
        self.published_len = self.authoritative_text.len();
        self.unpublished_content = false;
        self.pending_delta_bytes = 0;
        let deltas = if self.snapshot_rebuild_required {
            self.snapshot_rebuild_required = false;
            self.pending_deltas.clear();
            vec![(
                StreamContentKind::Assistant,
                self.authoritative_text.clone(),
            )]
        } else {
            self.pending_deltas
                .drain(..)
                .map(|delta| (delta.kind, delta.text))
                .collect()
        };
        Some(StreamPublication {
            generation: self.generation,
            revision: self.published_revision,
            deltas,
        })
    }

    pub(super) fn authoritative_text(&self) -> &str {
        &self.authoritative_text
    }

    pub(super) fn published_text(&self) -> &str {
        &self.authoritative_text[..self.published_len]
    }

    pub(super) fn acknowledge_draw(&mut self, generation: u64, revision: u64) {
        if generation == self.generation {
            self.drawn_revision = self
                .drawn_revision
                .max(revision.min(self.published_revision));
        }
    }

    pub(super) fn take_drawn_event(&mut self) -> Option<AgentEvent> {
        if self.drawn_revision == self.accumulated_revision
            && self.published_revision == self.accumulated_revision
        {
            self.blocked_events.pop_front()
        } else {
            None
        }
    }

    pub(super) fn has_blocked_events(&self) -> bool {
        !self.blocked_events.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn publish_and_draw(controller: &mut StreamingPresentationController) -> StreamPublication {
        let publication = controller.publish().expect("stream publication");
        controller.acknowledge_draw(publication.generation, publication.revision);
        publication
    }

    #[test]
    fn pending_delta_bytes_are_bounded_and_collapse_to_snapshot() {
        let mut controller = StreamingPresentationController::default();
        controller.classify(AgentEvent::MessageStart {
            role: "assistant".into(),
        });
        for _ in 0..300 {
            controller.classify(AgentEvent::MessageChunk {
                text: "x".repeat(1024),
            });
        }
        assert!(controller.pending_delta_bytes <= MAX_PENDING_DELTA_BYTES);
        let publication = controller.publish().expect("snapshot publication");
        assert_eq!(publication.deltas.len(), 1);
        assert_eq!(publication.deltas[0].1, controller.authoritative_text());
    }

    #[test]
    fn blocked_control_lane_is_count_bounded() {
        let mut controller = StreamingPresentationController::default();
        controller.classify(AgentEvent::MessageChunk { text: "x".into() });
        for index in 0..(MAX_BLOCKED_EVENTS + 50) {
            controller.classify(AgentEvent::MessageAbort {
                reason: Some(index.to_string()),
            });
        }
        assert_eq!(controller.blocked_events.len(), MAX_BLOCKED_EVENTS);
        assert!(controller.snapshot_rebuild_required);
    }

    #[test]
    fn completion_waits_for_latest_published_revision_to_be_drawn() {
        let mut controller = StreamingPresentationController::default();
        controller.classify(AgentEvent::MessageStart {
            role: "assistant".into(),
        });
        controller.classify(AgentEvent::MessageChunk { text: "one".into() });
        controller.classify(AgentEvent::MessageChunk {
            text: " two".into(),
        });
        let completion = controller.classify(AgentEvent::MessageEnd);
        assert!(!completion.apply_now);
        assert!(completion.publication_due);
        assert!(controller.take_drawn_event().is_none());

        let publication = controller.publish().expect("stream publication");
        assert!(controller.take_drawn_event().is_none());
        controller.acknowledge_draw(publication.generation, publication.revision);
        assert!(matches!(
            controller.take_drawn_event(),
            Some(AgentEvent::MessageEnd)
        ));
    }

    #[test]
    fn later_events_remain_ordered_behind_unpublished_content() {
        let mut controller = StreamingPresentationController::default();
        controller.classify(AgentEvent::MessageChunk {
            text: "done".into(),
        });
        controller.classify(AgentEvent::MessageEnd);
        controller.classify(AgentEvent::MessageAbort {
            reason: Some("after-end sentinel".into()),
        });

        publish_and_draw(&mut controller);
        assert!(matches!(
            controller.take_drawn_event(),
            Some(AgentEvent::MessageEnd)
        ));
        assert!(matches!(
            controller.take_drawn_event(),
            Some(AgentEvent::MessageAbort { reason })
                if reason.as_deref() == Some("after-end sentinel")
        ));
    }

    #[test]
    fn abort_waits_for_unpublished_content_and_preserves_reason() {
        let mut controller = StreamingPresentationController::default();
        controller.classify(AgentEvent::MessageChunk {
            text: "before abort".into(),
        });
        controller.classify(AgentEvent::MessageAbort {
            reason: Some("provider disconnected".into()),
        });
        assert!(controller.take_drawn_event().is_none());

        let publication = controller.publish().expect("publication");
        controller.acknowledge_draw(publication.generation, publication.revision);
        assert!(matches!(
            controller.take_drawn_event(),
            Some(AgentEvent::MessageAbort { reason })
                if reason.as_deref() == Some("provider disconnected")
        ));
    }

    #[test]
    fn absent_and_stale_draw_acknowledgements_do_not_release_events() {
        let mut controller = StreamingPresentationController::default();
        controller.classify(AgentEvent::MessageChunk {
            text: "final".into(),
        });
        controller.classify(AgentEvent::MessageEnd);
        let publication = controller.publish().expect("publication");

        assert!(controller.take_drawn_event().is_none());
        controller.acknowledge_draw(
            publication.generation.saturating_add(1),
            publication.revision,
        );
        assert!(controller.take_drawn_event().is_none());
        controller.acknowledge_draw(publication.generation, publication.revision);
        assert!(matches!(
            controller.take_drawn_event(),
            Some(AgentEvent::MessageEnd)
        ));
    }

    #[test]
    fn reset_generation_discards_pending_content_and_deferred_events() {
        let mut controller = StreamingPresentationController::default();
        controller.classify(AgentEvent::MessageChunk {
            text: "stale".into(),
        });
        controller.classify(AgentEvent::MessageEnd);

        let decision = controller.classify(AgentEvent::SessionReset);
        assert!(decision.apply_now);
        assert!(!decision.publication_due);
        assert!(controller.publish().is_none());
        assert!(controller.take_drawn_event().is_none());
    }

    #[test]
    fn deferred_events_release_in_exact_ingestion_order() {
        let mut controller = StreamingPresentationController::default();
        controller.classify(AgentEvent::MessageChunk {
            text: "ordered".into(),
        });
        controller.classify(AgentEvent::MessageEnd);
        controller.classify(AgentEvent::MessageAbort {
            reason: Some("second".into()),
        });
        let publication = controller.publish().expect("publication");
        controller.acknowledge_draw(publication.generation, publication.revision);

        assert!(matches!(
            controller.take_drawn_event(),
            Some(AgentEvent::MessageEnd)
        ));
        assert!(matches!(
            controller.take_drawn_event(),
            Some(AgentEvent::MessageAbort { reason }) if reason.as_deref() == Some("second")
        ));
        assert!(controller.take_drawn_event().is_none());
        assert!(!controller.has_blocked_events());
    }

    #[test]
    fn released_event_does_not_overtake_the_remaining_backlog() {
        let mut controller = StreamingPresentationController::default();
        controller.classify(AgentEvent::MessageChunk {
            text: "first".into(),
        });
        controller.classify(AgentEvent::MessageEnd);
        controller.classify(AgentEvent::MessageAbort {
            reason: Some("second".into()),
        });
        publish_and_draw(&mut controller);

        let released = controller.take_drawn_event().expect("released completion");
        assert!(matches!(released, AgentEvent::MessageEnd));
        assert!(controller.has_blocked_events());
        assert!(matches!(
            controller.take_drawn_event(),
            Some(AgentEvent::MessageAbort { reason }) if reason.as_deref() == Some("second")
        ));
        assert!(!controller.has_blocked_events());
    }

    #[test]
    fn assistant_and_thinking_deltas_coalesce_without_losing_order() {
        let mut controller = StreamingPresentationController::default();
        controller.classify(AgentEvent::ThinkingChunk { text: "a".into() });
        controller.classify(AgentEvent::ThinkingChunk { text: "b".into() });
        controller.classify(AgentEvent::MessageChunk { text: "c".into() });
        let publication = controller.publish().expect("publication");
        assert_eq!(publication.revision, 3);
        assert_eq!(
            publication.deltas,
            vec![
                (StreamContentKind::Thinking, "ab".into()),
                (StreamContentKind::Assistant, "c".into())
            ]
        );
        assert_eq!(controller.authoritative_text(), "abc");
        assert_eq!(controller.published_text(), "abc");
    }

    #[test]
    fn message_start_begins_a_bounded_generation() {
        let mut controller = StreamingPresentationController::default();
        controller.classify(AgentEvent::MessageChunk {
            text: "first".into(),
        });
        publish_and_draw(&mut controller);
        controller.classify(AgentEvent::MessageStart {
            role: "assistant".into(),
        });
        controller.classify(AgentEvent::MessageChunk {
            text: "second".into(),
        });
        let publication = controller.publish().expect("second publication");
        assert_eq!(publication.generation, 1);
        assert_eq!(controller.authoritative_text(), "second");
    }

    #[test]
    fn stale_draw_acknowledgement_cannot_release_current_generation() {
        let mut controller = StreamingPresentationController::default();
        controller.classify(AgentEvent::MessageStart {
            role: "assistant".into(),
        });
        controller.classify(AgentEvent::MessageChunk { text: "new".into() });
        controller.classify(AgentEvent::MessageEnd);
        let publication = controller.publish().expect("publication");
        controller.acknowledge_draw(
            publication.generation.saturating_sub(1),
            publication.revision,
        );
        assert!(controller.take_drawn_event().is_none());
    }

    #[test]
    fn superseding_message_start_bypasses_blocked_generation() {
        let mut controller = StreamingPresentationController::default();
        controller.classify(AgentEvent::MessageChunk {
            text: "stale".into(),
        });
        controller.classify(AgentEvent::MessageEnd);

        let decision = controller.classify(AgentEvent::MessageStart {
            role: "assistant".into(),
        });

        assert!(decision.apply_now);
        assert!(!decision.publication_due);
        assert!(!controller.has_blocked_events());
        assert!(controller.publish().is_none());
    }

    #[test]
    fn generation_increment_fails_closed_at_exhaustion() {
        let mut controller = StreamingPresentationController {
            generation: u64::MAX,
            ..Default::default()
        };

        let exhausted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            controller.classify(AgentEvent::MessageStart {
                role: "assistant".into(),
            });
        }));

        assert!(exhausted.is_err());
    }
}
