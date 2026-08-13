use crate::bridge::LlmEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CodexEventClass {
    SemanticStart,
    TransportHeartbeat,
    Other,
}

pub(super) fn classify_codex_event_type(event_type: &str) -> CodexEventClass {
    match event_type {
        "response.created" | "response.in_progress" => CodexEventClass::SemanticStart,
        "response.content_part.added" | "response.reasoning_summary_part.added" => {
            CodexEventClass::TransportHeartbeat
        }
        "response.output_text.delta"
        | "response.reasoning_summary_text.delta"
        | "response.output_item.added"
        | "response.function_call_arguments.delta"
        | "response.function_call_arguments.done"
        | "response.output_item.done"
        | "response.completed"
        | "response.failed"
        | "error" => CodexEventClass::Other,
        _ => CodexEventClass::TransportHeartbeat,
    }
}

pub(super) fn liveness_event_for_codex_type(event_type: &str) -> Option<LlmEvent> {
    match classify_codex_event_type(event_type) {
        CodexEventClass::SemanticStart => Some(LlmEvent::Start),
        CodexEventClass::TransportHeartbeat => Some(LlmEvent::TransportHeartbeat),
        CodexEventClass::Other => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_events_are_semantic_start() {
        for event_type in ["response.created", "response.in_progress"] {
            assert!(matches!(
                liveness_event_for_codex_type(event_type),
                Some(LlmEvent::Start)
            ));
        }
    }

    #[test]
    fn known_noops_and_unknown_events_are_transport_only() {
        for event_type in [
            "response.content_part.added",
            "response.reasoning_summary_part.added",
            "response.future_noop",
        ] {
            assert!(matches!(
                liveness_event_for_codex_type(event_type),
                Some(LlmEvent::TransportHeartbeat)
            ));
        }
    }

    #[test]
    fn semantic_payload_events_remain_owned_by_full_parser() {
        for event_type in [
            "response.output_text.delta",
            "response.output_item.added",
            "response.completed",
            "response.failed",
        ] {
            assert!(liveness_event_for_codex_type(event_type).is_none());
        }
    }
}
