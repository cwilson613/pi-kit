//! Presentation-aware conversation projection.
//!
//! Om and Active collapse completed tool evidence under authoritative turn
//! metadata into a synthetic outcome segment. Full returns canonical segments
//! unchanged. The source transcript is never mutated.

use crate::surfaces::layout::UiPresentationLevel;

use super::operation_lifecycle_projection::{OperationLifecycleProjection, OperationLifecycleRow};
use super::segments::Segment;
use super::turn_tool_projection::{TurnToolProjection, TurnToolRow};

#[derive(Debug, Clone)]
pub struct ConversationProjection {
    pub segments: Vec<Segment>,
    /// Maps each projected row to its canonical source row. Synthetic outcome
    /// rows point at the most relevant evidence item in their episode.
    pub canonical_indices: Vec<usize>,
}

impl ConversationProjection {
    pub fn projected_index_for_canonical(&self, canonical_index: usize) -> Option<usize> {
        self.canonical_indices
            .iter()
            .position(|index| *index == canonical_index)
    }

    fn push_canonical(&mut self, index: usize, segment: &Segment) {
        self.segments.push(segment.clone());
        self.canonical_indices.push(index);
    }

    fn push_synthetic(&mut self, canonical_index: usize, segment: Segment) {
        self.segments.push(segment);
        self.canonical_indices.push(canonical_index);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversationExportPolicy {
    /// Operator/assistant prose and durable outcomes, independent of UI level.
    Semantic,
    /// Canonical evidence-inclusive export for diagnostics and audit.
    Evidence,
}

pub fn project_conversation_for_export(
    segments: &[Segment],
    policy: ConversationExportPolicy,
) -> ConversationProjection {
    match policy {
        ConversationExportPolicy::Semantic => {
            project_conversation(segments, UiPresentationLevel::Om)
        }
        ConversationExportPolicy::Evidence => {
            project_conversation(segments, UiPresentationLevel::Full)
        }
    }
}

pub fn project_conversation(
    segments: &[Segment],
    level: UiPresentationLevel,
) -> ConversationProjection {
    if level == UiPresentationLevel::Full {
        return ConversationProjection {
            segments: segments.to_vec(),
            canonical_indices: (0..segments.len()).collect(),
        };
    }

    let mut operation_lifecycle = OperationLifecycleProjection::new(segments);
    let mut turn_tools = TurnToolProjection::new(segments);

    let mut projected = ConversationProjection {
        segments: Vec::with_capacity(segments.len()),
        canonical_indices: Vec::with_capacity(segments.len()),
    };
    for (canonical_index, segment) in segments.iter().enumerate() {
        match operation_lifecycle.project_row(canonical_index, segment) {
            OperationLifecycleRow::Outcome {
                canonical_index,
                segment,
            } => {
                projected.push_synthetic(canonical_index, *segment);
                continue;
            }
            OperationLifecycleRow::Suppressed => continue,
            OperationLifecycleRow::NotOperation => {}
        }
        match turn_tools.project_row(segment) {
            TurnToolRow::Outcome(segment) => {
                projected.push_synthetic(canonical_index, *segment);
                continue;
            }
            TurnToolRow::Suppressed => continue,
            TurnToolRow::Canonical => {}
        }
        projected.push_canonical(canonical_index, segment);
    }
    projected
}

pub fn project_conversation_segments(
    segments: &[Segment],
    level: UiPresentationLevel,
) -> Vec<Segment> {
    project_conversation(segments, level).segments
}

#[cfg(test)]
mod tests {
    use super::super::segments::{SegmentContent, SegmentMeta};
    use super::*;

    fn tool(turn: Option<u32>, id: &str, result: &str, complete: bool) -> Segment {
        Segment {
            meta: SegmentMeta {
                turn,
                ..Default::default()
            },
            content: SegmentContent::ToolCard {
                id: id.into(),
                name: "bash".into(),
                provenance: omegon_traits::ToolProvenance::BuiltIn,
                args_summary: None,
                detail_args: None,
                result_summary: Some(result.into()),
                detail_result: Some(result.into()),
                is_error: false,
                complete,
                expanded: false,
                started_at: None,
                live_partial: None,
            },
        }
    }

    #[test]
    fn canonical_replay_hands_activity_to_one_outcome_without_mutating_evidence() {
        let running = vec![tool(Some(7), "a", "running", false)];
        let before = project_conversation(&running, UiPresentationLevel::Om);
        assert!(matches!(
            before.segments[0].content,
            SegmentContent::ToolCard {
                complete: false,
                ..
            }
        ));

        let completed = vec![tool(Some(7), "a", "47 tests passed", true)];
        let canonical_before = completed.clone();
        let after = project_conversation(&completed, UiPresentationLevel::Om);
        assert_eq!(after.segments.len(), 1);
        assert_eq!(after.canonical_indices, vec![0]);
        assert!(matches!(
            after.segments[0].content,
            SegmentContent::SystemNotification { .. }
        ));
        assert_eq!(completed.len(), canonical_before.len());
        assert!(matches!(
            completed[0].content,
            SegmentContent::ToolCard { complete: true, .. }
        ));
    }

    #[test]
    fn om_collapses_complete_turn_tools_without_mutating_source() {
        let source = vec![
            tool(Some(7), "a", "read complete", true),
            tool(Some(7), "b", "47 tests passed", true),
        ];
        let projected = project_conversation_segments(&source, UiPresentationLevel::Om);
        assert_eq!(source.len(), 2);
        assert_eq!(projected.len(), 1);
        let SegmentContent::SystemNotification { text } = &projected[0].content else {
            panic!("outcome")
        };
        assert_eq!(text, "✓ bash · 47 tests passed · 2 operations");
    }

    #[test]
    fn failed_turn_tools_collapse_to_one_failed_outcome() {
        let mut failed = tool(Some(7), "a", "exit 1", true);
        if let SegmentContent::ToolCard { is_error, .. } = &mut failed.content {
            *is_error = true;
        }
        let source = vec![failed, tool(Some(7), "b", "diagnostics", true)];
        let projected = project_conversation(&source, UiPresentationLevel::Om);
        assert_eq!(projected.segments.len(), 1);
        let SegmentContent::SystemNotification { text } = &projected.segments[0].content else {
            panic!("failed outcome")
        };
        assert!(text.starts_with("✗ "), "{text}");
        assert!(text.contains("bash failed · exit 1"), "{text}");
    }

    #[test]
    fn repeated_inner_turns_from_distinct_runtime_turns_do_not_share_failures() {
        let mut failed = tool(Some(1), "old", "missing node", true);
        failed.meta.runtime_turn = Some(7);
        if let SegmentContent::ToolCard { is_error, .. } = &mut failed.content {
            *is_error = true;
        }
        let mut later = tool(Some(1), "later", "build active", true);
        later.meta.runtime_turn = Some(8);

        let projected = project_conversation(&[failed, later], UiPresentationLevel::Om);
        assert_eq!(projected.segments.len(), 2);
        let outcomes = projected
            .segments
            .iter()
            .map(|segment| match &segment.content {
                SegmentContent::SystemNotification { text } => text.as_str(),
                other => panic!("expected outcome, got {other:?}"),
            })
            .collect::<Vec<_>>();
        assert!(outcomes[0].contains("missing node"), "{}", outcomes[0]);
        assert_eq!(outcomes[1], "✓ bash · build active · 1 operation");
    }

    #[test]
    fn active_uses_same_grouped_completed_history() {
        let source = vec![tool(Some(7), "a", "done", true)];
        let projected = project_conversation_segments(&source, UiPresentationLevel::Active);
        assert!(matches!(
            projected[0].content,
            SegmentContent::SystemNotification { .. }
        ));
    }

    #[test]
    fn completed_operation_lifecycle_collapses_to_one_outcome() {
        let operation = omegon_traits::OperationRef::delegate("delegate-7");
        let mut conversation = crate::tui::conversation::ConversationView::new();
        conversation.push_operation_lifecycle(&operation, "⇉", "Delegate: review started");
        conversation.push_operation_lifecycle(&operation, "✓", "Delegate: review completed");
        conversation.push_operation_lifecycle(&operation, "↯", "Delegate completed (no merge)");

        let projected =
            project_conversation_segments(conversation.segments(), UiPresentationLevel::Om);
        assert_eq!(projected.len(), 1);
        let SegmentContent::SystemNotification { text } = &projected[0].content else {
            panic!("operation outcome")
        };
        assert!(text.contains("delegate delegate-7"), "{text}");
        assert!(text.contains("3 events"), "{text}");

        let full =
            project_conversation_segments(conversation.segments(), UiPresentationLevel::Full);
        assert_eq!(full.len(), 3);
        assert!(
            full.iter()
                .all(|segment| matches!(segment.content, SegmentContent::LifecycleEvent { .. }))
        );
    }

    #[test]
    fn semantic_export_is_independent_of_display_level() {
        let source = vec![
            tool(Some(7), "a", "read complete", true),
            tool(Some(7), "b", "47 tests passed", true),
        ];
        let semantic = project_conversation_for_export(&source, ConversationExportPolicy::Semantic);
        let om = project_conversation(&source, UiPresentationLevel::Om);
        let full = project_conversation(&source, UiPresentationLevel::Full);
        assert_eq!(semantic.segments.len(), om.segments.len());
        assert_eq!(semantic.canonical_indices, om.canonical_indices);
        assert_ne!(semantic.segments.len(), full.segments.len());

        let evidence = project_conversation_for_export(&source, ConversationExportPolicy::Evidence);
        assert_eq!(evidence.canonical_indices, full.canonical_indices);
        assert_eq!(evidence.segments.len(), full.segments.len());
    }

    #[test]
    fn failed_operation_collapses_to_failed_outcome() {
        let operation = omegon_traits::OperationRef::cleave(Some("run-9".into()));
        let mut conversation = crate::tui::conversation::ConversationView::new();
        conversation.push_operation_lifecycle(&operation, "↯", "Cleave: 2 children dispatched");
        conversation.push_operation_lifecycle(&operation, "✗", "Child 'tests' failed");

        let projected =
            project_conversation_segments(conversation.segments(), UiPresentationLevel::Om);
        assert_eq!(projected.len(), 1);
        let SegmentContent::SystemNotification { text } = &projected[0].content else {
            panic!("failed operation outcome")
        };
        assert!(text.starts_with("✗ cleave run-9"), "{text}");
        assert!(text.contains("failed"), "{text}");
    }

    #[test]
    fn full_preserves_canonical_evidence_rows() {
        let source = vec![tool(Some(7), "a", "done", true)];
        let projected = project_conversation_segments(&source, UiPresentationLevel::Full);
        assert!(matches!(
            projected[0].content,
            SegmentContent::ToolCard { .. }
        ));
    }

    #[test]
    fn operator_shell_without_turn_uses_authoritative_single_observation_episode() {
        let mut source = tool(None, "shell-7", "exit 0 · 12ms", true);
        if let SegmentContent::ToolCard { name, .. } = &mut source.content {
            *name = "operator_shell".into();
        }
        let projected = project_conversation_segments(&[source], UiPresentationLevel::Om);
        assert_eq!(projected.len(), 1);
        let SegmentContent::SystemNotification { text } = &projected[0].content else {
            panic!("outcome")
        };
        assert_eq!(text, "✓ operator_shell · exit 0 · 12ms · 1 operation");
    }

    #[test]
    fn running_or_unbound_tools_remain_visible() {
        let source = vec![
            tool(Some(7), "a", "running", false),
            tool(None, "b", "done", true),
        ];
        let projected = project_conversation_segments(&source, UiPresentationLevel::Om);
        assert_eq!(projected.len(), 2);
        assert!(
            projected
                .iter()
                .all(|segment| matches!(segment.content, SegmentContent::ToolCard { .. }))
        );
    }
}
