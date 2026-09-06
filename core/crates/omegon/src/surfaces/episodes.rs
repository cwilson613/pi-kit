//! Renderer-neutral operation episodes derived from canonical conversation evidence.
//!
//! This first reducer deliberately uses only authoritative boundaries supplied by
//! callers. If a caller cannot name a boundary, each tool becomes its own
//! episode rather than guessing that unrelated work belongs together.

use crate::surfaces::conversation::{ConversationSegmentKind, ConversationSegmentProjection};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationEpisodeState {
    Running,
    Complete,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationEpisodeProjection {
    pub id: String,
    pub state: OperationEpisodeState,
    pub outcome: String,
    pub evidence_ids: Vec<String>,
    pub tool_count: usize,
}

impl OperationEpisodeProjection {
    pub fn from_authoritative_boundary<TText, TPath>(
        id: impl Into<String>,
        segments: &[ConversationSegmentProjection<TText, TPath>],
    ) -> Option<Self>
    where
        TText: AsRef<str>,
    {
        let tools = segments
            .iter()
            .filter_map(|segment| match &segment.kind {
                ConversationSegmentKind::Tool(tool) => Some(tool),
                _ => None,
            })
            .collect::<Vec<_>>();
        if tools.is_empty() {
            return None;
        }

        let failed = tools.iter().any(|tool| tool.is_error);
        let complete = tools.iter().all(|tool| tool.complete);
        let state = if failed {
            OperationEpisodeState::Failed
        } else if complete {
            OperationEpisodeState::Complete
        } else {
            OperationEpisodeState::Running
        };
        let evidence_ids = tools
            .iter()
            .map(|tool| tool.id.as_ref().to_string())
            .collect::<Vec<_>>();
        let outcome = deterministic_outcome(&tools, state);

        Some(Self {
            id: id.into(),
            state,
            outcome,
            tool_count: evidence_ids.len(),
            evidence_ids,
        })
    }

    pub fn single_tool_fallback<TText, TPath>(
        segment: &ConversationSegmentProjection<TText, TPath>,
    ) -> Option<Self>
    where
        TText: AsRef<str>,
    {
        let ConversationSegmentKind::Tool(tool) = &segment.kind else {
            return None;
        };
        Self::from_authoritative_boundary(
            if tool.name.as_ref() == "operator_shell" {
                format!("operator-shell:{}", tool.id.as_ref())
            } else {
                format!("tool:{}", tool.id.as_ref())
            },
            std::slice::from_ref(segment),
        )
    }
}

fn deterministic_outcome<TText>(
    tools: &[&crate::surfaces::conversation::ToolSegment<TText>],
    state: OperationEpisodeState,
) -> String
where
    TText: AsRef<str>,
{
    let mut summary = OutcomeSummary::default();
    for tool in tools {
        summary.observe(
            tool.name.as_ref(),
            tool.result_summary.as_ref().map(AsRef::as_ref),
            tool.is_error,
        );
    }
    summary.outcome(state)
}

/// Bounded scalar reducer shared by managed and incremental terminal projections.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct OutcomeSummary {
    pub(crate) count: usize,
    failure: Option<String>,
    success: Option<String>,
    last_name: String,
}

impl OutcomeSummary {
    pub(crate) fn observe(&mut self, name: &str, result: Option<&str>, failed: bool) {
        self.count += 1;
        self.last_name = bounded(name);
        let result = result.map(bounded).filter(|v| !v.is_empty());
        if failed && self.failure.is_none() {
            self.failure = Some(format!(
                "{} failed · {}",
                self.last_name,
                result.as_deref().unwrap_or("failed")
            ));
        }
        if let Some(result) = result {
            self.success = Some(format!("{} · {}", self.last_name, result));
        }
    }

    pub(crate) fn failed(&self) -> bool {
        self.failure.is_some()
    }

    pub(crate) fn outcome(&self, state: OperationEpisodeState) -> String {
        if let Some(failure) = &self.failure {
            return failure.clone();
        }
        if state == OperationEpisodeState::Running {
            return format!("Running {}", self.last_name);
        }
        if let Some(success) = &self.success {
            return success.clone();
        }
        if self.count == 1 {
            format!("{} complete", self.last_name)
        } else {
            format!("Completed {} operations", self.count)
        }
    }

    pub(crate) fn display(&self) -> String {
        format!(
            "{} {} · {} operation{}",
            if self.failed() { "✗" } else { "✓" },
            self.outcome(OperationEpisodeState::Complete),
            self.count,
            if self.count == 1 { "" } else { "s" }
        )
    }
}

fn bounded(text: &str) -> String {
    // Bound discovery as well as the output; a huge whitespace-only result must
    // not be scanned in an inline preparation cycle.
    let prefix = &text[..text.floor_char_boundary(text.len().min(512))];
    let compact = prefix.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = compact.chars();
    let mut prefix: String = chars.by_ref().take(120).collect();
    if chars.next().is_some() || text.len() > 512 {
        prefix.pop();
        format!("{prefix}…")
    } else {
        prefix
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surfaces::conversation::{ToolSegment, UserSegment};

    fn tool<'a>(
        id: &'a str,
        name: &'a str,
        result: Option<&'a str>,
        complete: bool,
        is_error: bool,
    ) -> ConversationSegmentProjection<&'a str> {
        ConversationSegmentProjection::new(ConversationSegmentKind::Tool(ToolSegment {
            id,
            name,
            args_summary: None,
            detail_args: None,
            result_summary: result,
            detail_result: result,
            is_error,
            complete,
            expanded: false,
        }))
    }

    #[test]
    fn authoritative_boundary_groups_evidence_deterministically() {
        let segments = vec![
            tool("read-1", "read", Some("86 lines"), true, false),
            tool("test-1", "bash", Some("47 tests passed"), true, false),
        ];
        let episode = OperationEpisodeProjection::from_authoritative_boundary("turn:7", &segments)
            .expect("episode");
        assert_eq!(episode.id, "turn:7");
        assert_eq!(episode.state, OperationEpisodeState::Complete);
        assert_eq!(episode.tool_count, 2);
        assert_eq!(episode.evidence_ids, ["read-1", "test-1"]);
        assert_eq!(episode.outcome, "bash · 47 tests passed");
    }

    #[test]
    fn failure_has_precedence_over_successful_later_evidence() {
        let segments = vec![
            tool("test-1", "bash", Some("exit 1"), true, true),
            tool("read-1", "read", Some("diagnostics"), true, false),
        ];
        let episode = OperationEpisodeProjection::from_authoritative_boundary("turn:8", &segments)
            .expect("episode");
        assert_eq!(episode.state, OperationEpisodeState::Failed);
        assert_eq!(episode.outcome, "bash failed · exit 1");
    }

    #[test]
    fn missing_boundary_falls_back_to_one_tool_only() {
        let segment = tool("read-1", "read", Some("12 lines"), true, false);
        let episode = OperationEpisodeProjection::single_tool_fallback(&segment).expect("episode");
        assert_eq!(episode.id, "tool:read-1");
        assert_eq!(episode.evidence_ids, ["read-1"]);
    }

    #[test]
    fn prose_does_not_become_an_episode() {
        let segment = ConversationSegmentProjection::<&str>::new(ConversationSegmentKind::User(
            UserSegment { text: "hello" },
        ));
        assert!(OperationEpisodeProjection::single_tool_fallback(&segment).is_none());
    }
}
