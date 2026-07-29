//! Projection of completed turn-bound tool evidence into durable outcomes.
//!
//! Tool rows remain canonical while their authoritative episode is running.
//! Once complete, compact presentation levels replace the episode with one
//! synthetic outcome row. Completed standalone operator-shell observations use
//! the same outcome representation without inventing a turn coordinate.

use std::collections::BTreeMap;

use crate::surfaces::conversation::{ConversationSegmentKind, ProjectConversationSegment};
use crate::surfaces::episodes::{OperationEpisodeProjection, OperationEpisodeState};

use super::segments::{Segment, SegmentContent, SegmentMeta};

type TurnCoordinate = (Option<u64>, u32);

#[derive(Debug)]
pub enum TurnToolRow {
    Canonical,
    Suppressed,
    Outcome(Box<Segment>),
}

pub struct TurnToolProjection {
    complete_episodes: BTreeMap<TurnCoordinate, OperationEpisodeProjection>,
    emitted_turn: Option<TurnCoordinate>,
}

impl TurnToolProjection {
    pub fn new(segments: &[Segment]) -> Self {
        let mut tools_by_turn: BTreeMap<TurnCoordinate, Vec<_>> = BTreeMap::new();
        for segment in segments {
            let Some(turn) = segment.meta.turn else {
                continue;
            };
            let coordinate = (segment.meta.runtime_turn, turn);
            let projection = segment.project_conversation_segment();
            if matches!(projection.kind, ConversationSegmentKind::Tool(_)) {
                tools_by_turn
                    .entry(coordinate)
                    .or_default()
                    .push(projection);
            }
        }

        let mut complete_episodes = BTreeMap::new();
        for ((runtime_turn, turn), tools) in &tools_by_turn {
            let episode_id = runtime_turn.map_or_else(
                || format!("turn:{turn}"),
                |runtime_turn| format!("runtime:{runtime_turn}/turn:{turn}"),
            );
            if let Some(episode) =
                OperationEpisodeProjection::from_authoritative_boundary(episode_id, tools)
                && matches!(
                    episode.state,
                    OperationEpisodeState::Complete | OperationEpisodeState::Failed
                )
            {
                complete_episodes.insert((*runtime_turn, *turn), episode);
            }
        }

        Self {
            complete_episodes,
            emitted_turn: None,
        }
    }

    pub fn project_row(&mut self, segment: &Segment) -> TurnToolRow {
        if is_completed_operator_shell(segment) {
            let semantic = segment.project_conversation_segment();
            if let Some(episode) = OperationEpisodeProjection::single_tool_fallback(&semantic) {
                return TurnToolRow::Outcome(Box::new(outcome_segment(
                    segment.meta.clone(),
                    &episode,
                )));
            }
        }

        let Some(coordinate) = segment
            .meta
            .turn
            .map(|turn| (segment.meta.runtime_turn, turn))
        else {
            return TurnToolRow::Canonical;
        };
        let Some(episode) = self.complete_episodes.get(&coordinate) else {
            return TurnToolRow::Canonical;
        };
        if !matches!(segment.content, SegmentContent::ToolCard { .. }) {
            return TurnToolRow::Canonical;
        }
        if self.emitted_turn == Some(coordinate) {
            return TurnToolRow::Suppressed;
        }

        self.emitted_turn = Some(coordinate);
        TurnToolRow::Outcome(Box::new(outcome_segment(segment.meta.clone(), episode)))
    }
}

fn is_completed_operator_shell(segment: &Segment) -> bool {
    segment.meta.turn.is_none()
        && matches!(
            &segment.content,
            SegmentContent::ToolCard {
                name,
                complete: true,
                ..
            } if name == "operator_shell"
        )
}

fn outcome_segment(mut meta: SegmentMeta, episode: &OperationEpisodeProjection) -> Segment {
    meta.duration_ms = None;
    Segment {
        meta,
        content: SegmentContent::SystemNotification {
            text: format!(
                "{} {} · {} operation{}",
                if episode.state == OperationEpisodeState::Failed {
                    "✗"
                } else {
                    "✓"
                },
                episode.outcome,
                episode.tool_count,
                if episode.tool_count == 1 { "" } else { "s" }
            ),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(
        runtime_turn: Option<u64>,
        turn: Option<u32>,
        id: &str,
        result: &str,
        complete: bool,
    ) -> Segment {
        Segment {
            meta: SegmentMeta {
                runtime_turn,
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
    fn completed_turn_emits_one_outcome_and_suppresses_remaining_tools() {
        let segments = vec![
            tool(Some(4), Some(2), "read", "read complete", true),
            tool(Some(4), Some(2), "test", "47 tests passed", true),
        ];
        let mut projection = TurnToolProjection::new(&segments);

        let TurnToolRow::Outcome(outcome) = projection.project_row(&segments[0]) else {
            panic!("expected outcome")
        };
        let SegmentContent::SystemNotification { text } = &outcome.content else {
            panic!("expected notification")
        };
        assert_eq!(text, "✓ bash · 47 tests passed · 2 operations");
        assert!(matches!(
            projection.project_row(&segments[1]),
            TurnToolRow::Suppressed
        ));
    }

    #[test]
    fn runtime_turn_is_part_of_episode_identity() {
        let mut failed = tool(Some(7), Some(1), "old", "missing node", true);
        if let SegmentContent::ToolCard { is_error, .. } = &mut failed.content {
            *is_error = true;
        }
        let later = tool(Some(8), Some(1), "later", "build active", true);
        let segments = vec![failed, later];
        let mut projection = TurnToolProjection::new(&segments);

        let TurnToolRow::Outcome(first) = projection.project_row(&segments[0]) else {
            panic!("expected first outcome")
        };
        let TurnToolRow::Outcome(second) = projection.project_row(&segments[1]) else {
            panic!("expected second outcome")
        };
        let SegmentContent::SystemNotification { text: first } = &first.content else {
            panic!("expected first notification")
        };
        let SegmentContent::SystemNotification { text: second } = &second.content else {
            panic!("expected second notification")
        };
        assert!(first.starts_with("✗ "), "{first}");
        assert_eq!(second, "✓ bash · build active · 1 operation");
    }

    #[test]
    fn running_turn_and_unbound_tool_remain_canonical() {
        let segments = vec![
            tool(None, Some(3), "running", "working", false),
            tool(None, None, "unbound", "done", true),
        ];
        let mut projection = TurnToolProjection::new(&segments);
        assert!(
            segments
                .iter()
                .all(|segment| matches!(projection.project_row(segment), TurnToolRow::Canonical))
        );
    }

    #[test]
    fn completed_operator_shell_uses_single_observation_fallback() {
        let mut shell = tool(None, None, "shell", "exit 0 · 12ms", true);
        if let SegmentContent::ToolCard { name, .. } = &mut shell.content {
            *name = "operator_shell".into();
        }
        let segments = vec![shell];
        let mut projection = TurnToolProjection::new(&segments);
        let TurnToolRow::Outcome(outcome) = projection.project_row(&segments[0]) else {
            panic!("expected shell outcome")
        };
        let SegmentContent::SystemNotification { text } = &outcome.content else {
            panic!("expected notification")
        };
        assert_eq!(text, "✓ operator_shell · exit 0 · 12ms · 1 operation");
    }
}
