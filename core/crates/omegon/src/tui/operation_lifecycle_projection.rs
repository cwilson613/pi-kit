//! Projection of completed operation lifecycle evidence into durable outcomes.
//!
//! Operation events remain canonical transcript evidence. Compact presentation
//! levels replace a terminal operation's event sequence with one synthetic row.

use std::collections::{BTreeMap, BTreeSet};

use super::segments::{Segment, SegmentContent, SegmentMeta};

#[derive(Debug)]
pub enum OperationLifecycleRow {
    NotOperation,
    Suppressed,
    Outcome {
        canonical_index: usize,
        segment: Box<Segment>,
    },
}

pub struct OperationLifecycleProjection<'a> {
    segments: &'a [Segment],
    evidence_by_id: BTreeMap<String, Vec<&'a Segment>>,
    emitted: BTreeSet<String>,
}

impl<'a> OperationLifecycleProjection<'a> {
    pub fn new(segments: &'a [Segment]) -> Self {
        let mut evidence_by_id: BTreeMap<String, Vec<&Segment>> = BTreeMap::new();
        for segment in segments {
            if let Some(operation_id) = operation_id(segment) {
                evidence_by_id
                    .entry(operation_id.to_string())
                    .or_default()
                    .push(segment);
            }
        }
        Self {
            segments,
            evidence_by_id,
            emitted: BTreeSet::new(),
        }
    }

    pub fn project_row(
        &mut self,
        canonical_index: usize,
        segment: &Segment,
    ) -> OperationLifecycleRow {
        let Some(operation_id) = operation_id(segment) else {
            return OperationLifecycleRow::NotOperation;
        };
        let evidence = &self.evidence_by_id[operation_id];
        let Some(terminal) = evidence.iter().rev().find(|candidate| {
            matches!(
                &candidate.content,
                SegmentContent::LifecycleEvent { text, .. } if is_terminal_text(text)
            )
        }) else {
            return OperationLifecycleRow::NotOperation;
        };
        if !self.emitted.insert(operation_id.to_string()) {
            return OperationLifecycleRow::Suppressed;
        }
        let terminal_index = self
            .segments
            .iter()
            .position(|candidate| std::ptr::eq(candidate, *terminal))
            .unwrap_or(canonical_index);
        OperationLifecycleRow::Outcome {
            canonical_index: terminal_index,
            segment: Box::new(outcome_segment(
                terminal.meta.clone(),
                operation_id,
                evidence,
            )),
        }
    }
}

fn operation_id(segment: &Segment) -> Option<&str> {
    segment
        .meta
        .source_channel
        .as_deref()?
        .strip_prefix("operation:")
}

fn is_terminal_text(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("merged")
        || lower.contains("completed (no merge)")
        || lower.contains("failed")
        || lower.contains("cancelled")
}

fn failed(evidence: &[&Segment]) -> bool {
    evidence.iter().any(|segment| {
        matches!(
            &segment.content,
            SegmentContent::LifecycleEvent { icon, text }
                if icon == "✗"
                    || text.to_ascii_lowercase().contains("failed")
                    || text.to_ascii_lowercase().contains("cancelled")
        )
    })
}

fn outcome_segment(mut meta: SegmentMeta, operation_id: &str, evidence: &[&Segment]) -> Segment {
    meta.duration_ms = None;
    let terminal_text = evidence
        .iter()
        .rev()
        .find_map(|segment| match &segment.content {
            SegmentContent::LifecycleEvent { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .unwrap_or("completed");
    let label = operation_id
        .split_once(':')
        .map(|(kind, id)| format!("{kind} {id}"))
        .unwrap_or_else(|| operation_id.to_string());
    let state = if failed(evidence) { "✗" } else { "✓" };
    Segment {
        meta,
        content: SegmentContent::SystemNotification {
            text: format!(
                "{state} {label} · {terminal_text} · {} events",
                evidence.len()
            ),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(operation_id: &str, icon: &str, text: &str) -> Segment {
        Segment {
            meta: SegmentMeta {
                source_channel: Some(format!("operation:{operation_id}")),
                ..Default::default()
            },
            content: SegmentContent::LifecycleEvent {
                icon: icon.into(),
                text: text.into(),
            },
        }
    }

    #[test]
    fn terminal_operation_emits_one_outcome_at_terminal_canonical_index() {
        let segments = vec![
            event("delegate:7", "⇉", "Delegate started"),
            event("delegate:7", "↯", "Delegate completed (no merge)"),
        ];
        let mut projection = OperationLifecycleProjection::new(&segments);

        let OperationLifecycleRow::Outcome {
            canonical_index,
            segment,
        } = projection.project_row(0, &segments[0])
        else {
            panic!("expected outcome")
        };
        assert_eq!(canonical_index, 1);
        let SegmentContent::SystemNotification { text } = segment.content else {
            panic!("expected notification")
        };
        assert_eq!(
            text,
            "✓ delegate 7 · Delegate completed (no merge) · 2 events"
        );
        assert!(matches!(
            projection.project_row(1, &segments[1]),
            OperationLifecycleRow::Suppressed
        ));
    }

    #[test]
    fn nonterminal_operation_evidence_remains_canonical() {
        let segments = vec![event("cleave:9", "⇉", "Children dispatched")];
        let mut projection = OperationLifecycleProjection::new(&segments);
        assert!(matches!(
            projection.project_row(0, &segments[0]),
            OperationLifecycleRow::NotOperation
        ));
    }
}
