use super::*;
use crate::{
    session_authority::AssistantContentKind,
    session_consumers::{SemanticSessionStatus, SemanticSessionView, SessionViewKind},
    surfaces::session::{
        FrontendConversationKindV1, FrontendConversationStatusV1, TranscriptContentV1,
    },
};

impl App {
    pub(super) fn replace_conversation(&mut self, conversation: ConversationView) {
        self.conversation.replace_source(conversation);
        self.publication_boundary = self.conversation.segments().len();
        self.native_publication.automatic.source_replaced(
            self.conversation.publication_generation(),
            self.publication_boundary,
        );
    }

    pub(super) fn refresh_semantic_session_view(&mut self) {
        let Some(binding) = self.session_view_binding.clone() else {
            return;
        };
        let target = binding.snapshot();
        let loaded = SemanticSessionView::load(&target);
        if binding.snapshot().generation != target.generation {
            return;
        }

        self.project_browser = None;
        self.activity_tools.clear();
        self.last_tool_name = None;
        self.completed_tool_name = None;
        self.agent_active = false;
        self.interrupt_pending = false;
        self.runtime_turn_id = None;
        self.runtime_queue_snapshot = None;
        self.session_activity_cache = Default::default();
        self.slim_turn_state = SlimTurnState::Ready;

        let mut conversation = ConversationView::new();
        match loaded {
            Ok(view) => {
                self.session_projection_frontier = Some(view.frontier_sequence);
                self.session_view_generation = target.generation;
                if let Some(snapshot) = view.frontend.as_ref() {
                    seed_conversation(&mut conversation, &view, snapshot);
                }
                let action = match target.kind {
                    SessionViewKind::Resume => "Resumed session",
                    SessionViewKind::New => "New session",
                    SessionViewKind::ContextClear => "Context cleared into new session",
                };
                if target.kind == SessionViewKind::New && view.frontier_sequence == 1 {
                    conversation.push_system(&format!(
                        "New session {}. Ready for first turn. No semantic steps recorded yet.",
                        target.session_id
                    ));
                } else {
                    conversation.push_system(&format!(
                    "{action} {}. Semantic view: {}. Frontier {}. Queue {}; turn {}; context revision {} ({} items).",
                    target.session_id,
                    view.status.label(),
                    view.frontier_sequence,
                    view.frontend
                        .as_ref()
                        .map_or(0, |snapshot| snapshot.queued_prompts.len()),
                    view.frontend
                        .as_ref()
                        .map_or("unavailable", crate::session_consumers::active_turn_label),
                    view.frontend
                        .as_ref()
                        .map_or(0, |snapshot| snapshot.context.context_revision),
                    view.frontend
                        .as_ref()
                        .map_or(0, |snapshot| snapshot.context.items.len()),
                ));
                }
            }
            Err(error) => {
                self.session_projection_frontier = None;
                self.session_view_generation = target.generation;
                conversation.push_system(&format!(
                    "Session {} is active, but its semantic frontend is unavailable: {error}. Operator controls remain idle and authoritative runtime state will reconcile live activity.",
                    target.session_id
                ));
            }
        }
        self.replace_conversation(conversation);
    }

    pub(super) fn write_exact_semantic_transcript(
        &mut self,
        allow_suffix: bool,
    ) -> Result<std::path::PathBuf, String> {
        let binding = self
            .session_view_binding
            .as_ref()
            .ok_or("sessionless execution has no semantic transcript")?;
        let target = binding.snapshot();
        let view = SemanticSessionView::load(&target).map_err(|error| error.to_string())?;
        let body = view
            .transcript_markdown(!allow_suffix)
            .map_err(|error| error.to_string())?;
        let directory = crate::setup::find_project_root(self.cwd())
            .join(".omegon")
            .join("transcripts");
        std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S%.3f");
        let path = directory.join(format!("omegon-semantic-transcript-{timestamp}.md"));
        std::fs::write(&path, body).map_err(|error| error.to_string())?;
        Ok(path)
    }

    pub(super) fn session_export_disclosure(&self) -> String {
        let Some(binding) = self.session_view_binding.as_ref() else {
            return "Semantic source: unavailable (sessionless). Ephemeral live tool progress may be present."
                .into();
        };
        let target = binding.snapshot();
        match SemanticSessionView::load(&target) {
            Ok(view) => format!(
                "Semantic source: {}; frontier {}; presentation includes durable partial/abandoned evidence and may include separately overlaid ephemeral live tool progress.",
                view.status.label(),
                view.frontier_sequence
            ),
            Err(error) => format!(
                "Semantic source: unavailable ({error}); this is presentation/evidence output only and may include separately overlaid ephemeral live tool progress."
            ),
        }
    }
}

fn seed_conversation(
    conversation: &mut ConversationView,
    view: &SemanticSessionView,
    snapshot: &crate::surfaces::session::FrontendSnapshotV1,
) {
    for item in &snapshot.conversation {
        match item.kind {
            FrontendConversationKindV1::CommittedMessage => {
                let Some(message) = item.transcript_message.as_ref() else {
                    continue;
                };
                match &message.content {
                    TranscriptContentV1::Prompt { prompt_content } => {
                        conversation.push_user(&prompt_content.text);
                        if !prompt_content.attachments.is_empty() {
                            conversation.push_system(&format!(
                                "{} committed attachment(s)",
                                prompt_content.attachments.len()
                            ));
                        }
                    }
                    TranscriptContentV1::Assistant { assistant_channels } => {
                        for channel in assistant_channels {
                            for content_ref in &channel.chunk_refs {
                                let text = view.content_text(content_ref).unwrap_or_else(|error| {
                                    format!("[semantic content unavailable: {error}]")
                                });
                                match channel.content_kind {
                                    AssistantContentKind::Text => {
                                        conversation.append_streaming(&text)
                                    }
                                    AssistantContentKind::Thinking => {
                                        conversation.append_thinking(&text)
                                    }
                                }
                            }
                        }
                        conversation.abort_streaming();
                        if item.status == FrontendConversationStatusV1::AbandonedAfterCommit {
                            conversation.push_system(
                                "Committed assistant response was followed by abnormal abandonment.",
                            );
                        }
                    }
                    TranscriptContentV1::ToolResult {
                        content_ref,
                        disposition,
                        is_error,
                        ..
                    } => {
                        let text = view.content_text(content_ref).unwrap_or_else(|error| {
                            format!("[semantic content unavailable: {error}]")
                        });
                        conversation.push_system(&format!(
                            "Historical tool result ({disposition:?}, error={is_error}):\n{text}"
                        ));
                    }
                }
            }
            FrontendConversationKindV1::AssistantEvidence => {
                let Some(content_ref) = item.content_ref.as_ref() else {
                    continue;
                };
                let text = view
                    .content_text(content_ref)
                    .unwrap_or_else(|error| format!("[semantic content unavailable: {error}]"));
                match item.content_kind {
                    Some(AssistantContentKind::Thinking) => conversation.append_thinking(&text),
                    _ => conversation.append_streaming(&text),
                }
                conversation.abort_streaming();
                conversation
                    .push_system(&format!("Durable assistant evidence: {:?}.", item.status));
            }
        }
    }
    if view.status == SemanticSessionStatus::ExactSuffix {
        conversation.push_system(
            "Only the semantic suffix is exact; the compatibility prefix is intentionally not shown as semantic history.",
        );
    }
}
