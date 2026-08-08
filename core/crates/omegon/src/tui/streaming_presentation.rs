use std::collections::VecDeque;

use omegon_traits::AgentEvent;

/// TUI-local stream publication state. Runtime events remain authoritative; this
/// controller only decides when accumulated progressive content becomes a
/// presentation revision and when visual completion may follow it.
#[derive(Debug, Default)]
pub(super) struct StreamingPresentationController {
    accumulated_revision: u64,
    published_revision: u64,
    drawn_revision: u64,
    unpublished_content: bool,
    pending_completions: VecDeque<AgentEvent>,
    authoritative_text: String,
    published_text: String,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct StreamIngestDecision {
    pub(super) apply_now: bool,
    pub(super) publication_due: bool,
}

pub(super) struct StreamPublication {
    pub(super) revision: u64,
    pub(super) delta: String,
}

impl StreamingPresentationController {
    pub(super) fn classify(&mut self, event: AgentEvent) -> StreamIngestDecision {
        match event {
            AgentEvent::MessageChunk { ref text } => {
                self.authoritative_text.push_str(text);
                self.accumulated_revision = self.accumulated_revision.saturating_add(1);
                self.unpublished_content = true;
                StreamIngestDecision {
                    apply_now: false,
                    publication_due: true,
                }
            }
            AgentEvent::ThinkingChunk { .. } => StreamIngestDecision {
                apply_now: true,
                publication_due: true,
            },
            AgentEvent::MessageEnd | AgentEvent::MessageAbort { .. } | AgentEvent::TurnEnd(_)
                if self.unpublished_content =>
            {
                self.pending_completions.push_back(event);
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

    pub(super) fn publish(&mut self) -> Option<StreamPublication> {
        if !self.unpublished_content {
            return None;
        }
        let published_len = self.published_text.len();
        self.published_revision = self.accumulated_revision;
        self.published_text.clone_from(&self.authoritative_text);
        self.unpublished_content = false;
        Some(StreamPublication {
            revision: self.published_revision,
            delta: self.published_text[published_len..].to_string(),
        })
    }

    pub(super) fn authoritative_text(&self) -> &str {
        &self.authoritative_text
    }

    pub(super) fn published_text(&self) -> &str {
        &self.published_text
    }

    pub(super) fn acknowledge_draw(&mut self, revision: u64) {
        self.drawn_revision = self
            .drawn_revision
            .max(revision.min(self.published_revision));
    }

    pub(super) fn take_drawn_completion(&mut self) -> Option<AgentEvent> {
        if !self.pending_completions.is_empty()
            && self.drawn_revision == self.accumulated_revision
            && self.published_revision == self.accumulated_revision
        {
            self.pending_completions.pop_front()
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_waits_for_latest_published_revision_to_be_drawn() {
        let mut controller = StreamingPresentationController::default();
        assert!(
            !controller
                .classify(AgentEvent::MessageChunk { text: "one".into() })
                .apply_now
        );
        assert!(
            !controller
                .classify(AgentEvent::MessageChunk {
                    text: " two".into(),
                })
                .apply_now
        );

        let completion = controller.classify(AgentEvent::MessageEnd);
        assert!(!completion.apply_now);
        assert!(completion.publication_due);
        assert!(controller.take_drawn_completion().is_none());

        let publication = controller.publish().expect("stream publication");
        assert!(controller.take_drawn_completion().is_none());
        controller.acknowledge_draw(publication.revision);
        assert!(matches!(
            controller.take_drawn_completion(),
            Some(AgentEvent::MessageEnd)
        ));
    }

    #[test]
    fn deferred_message_and_turn_completion_preserve_event_order() {
        let mut controller = StreamingPresentationController::default();
        controller.classify(AgentEvent::MessageChunk {
            text: "done".into(),
        });
        controller.classify(AgentEvent::MessageEnd);
        controller.classify(AgentEvent::MessageAbort {
            reason: Some("after-end sentinel".into()),
        });

        let publication = controller.publish().expect("publication");
        controller.acknowledge_draw(publication.revision);
        assert!(matches!(
            controller.take_drawn_completion(),
            Some(AgentEvent::MessageEnd)
        ));
        assert!(matches!(
            controller.take_drawn_completion(),
            Some(AgentEvent::MessageAbort { reason })
                if reason.as_deref() == Some("after-end sentinel")
        ));
    }

    #[test]
    fn chunk_burst_coalesces_into_one_publication_revision() {
        let mut controller = StreamingPresentationController::default();
        for text in ["a", "b", "c"] {
            controller.classify(AgentEvent::MessageChunk { text: text.into() });
        }
        let publication = controller.publish().expect("publication");
        assert_eq!(publication.revision, 3);
        assert_eq!(publication.delta, "abc");
        assert_eq!(controller.authoritative_text(), "abc");
        assert_eq!(controller.published_text(), "abc");
        assert!(controller.publish().is_none());
    }

    #[test]
    fn unpublished_chunks_do_not_change_the_published_projection() {
        let mut controller = StreamingPresentationController::default();
        controller.classify(AgentEvent::MessageChunk {
            text: "first".into(),
        });
        assert_eq!(controller.authoritative_text(), "first");
        assert_eq!(controller.published_text(), "");

        controller.publish();
        controller.classify(AgentEvent::MessageChunk {
            text: " second".into(),
        });
        assert_eq!(controller.authoritative_text(), "first second");
        assert_eq!(controller.published_text(), "first");
    }
}
