//! Runtime prompt ownership and queueing vocabulary for the interactive supervisor.
//!
//! These types describe who submitted work, which control surface carried it,
//! and how the supervisor should queue it. They deliberately contain no I/O or
//! supervisor loop policy.

use crate::tui;
use std::collections::VecDeque;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RuntimeActorKind {
    Tui,
    Auspex,
    IpcClient,
    WebClient,
    DaemonEvent,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeActor {
    pub(crate) kind: RuntimeActorKind,
    pub(crate) label: String,
}

impl RuntimeActor {
    pub(crate) fn from_submission(submitted_by: String, via: &str) -> Self {
        let kind = match via {
            "tui" => RuntimeActorKind::Tui,
            "auspex" => RuntimeActorKind::Auspex,
            "ipc" => RuntimeActorKind::IpcClient,
            "websocket" | "http-event-ingress" => RuntimeActorKind::WebClient,
            _ => RuntimeActorKind::System,
        };
        Self {
            kind,
            label: submitted_by,
        }
    }

    pub(crate) fn display_label(&self) -> &str {
        if self.label.is_empty() {
            match self.kind {
                RuntimeActorKind::Tui => "tui",
                RuntimeActorKind::Auspex => "auspex",
                RuntimeActorKind::IpcClient => "ipc-client",
                RuntimeActorKind::WebClient => "web-client",
                RuntimeActorKind::DaemonEvent => "daemon-event",
                RuntimeActorKind::System => "system",
            }
        } else {
            &self.label
        }
    }

    pub(crate) fn tui() -> Self {
        Self {
            kind: RuntimeActorKind::Tui,
            label: "local-tui".to_string(),
        }
    }

    pub(crate) fn auspex() -> Self {
        Self {
            kind: RuntimeActorKind::Auspex,
            label: "auspex".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ControlSurface {
    Tui,
    Ipc,
    WebSocket,
    HttpEventIngress,
    Internal,
}

impl ControlSurface {
    pub(crate) fn from_via(via: &str) -> Self {
        match via {
            "tui" => Self::Tui,
            "ipc" | "auspex" => Self::Ipc,
            "websocket" => Self::WebSocket,
            "http-event-ingress" => Self::HttpEventIngress,
            _ => Self::Internal,
        }
    }

    pub(crate) fn label(&self) -> &'static str {
        match self {
            ControlSurface::Tui => "tui",
            ControlSurface::Ipc => "ipc",
            ControlSurface::WebSocket => "websocket",
            ControlSurface::HttpEventIngress => "http-event-ingress",
            ControlSurface::Internal => "internal",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum QueueMode {
    InterruptAfterTurn,
    #[default]
    UntilReady,
    Immediate,
}

impl QueueMode {
    pub(crate) fn from_tui(mode: tui::PromptQueueMode) -> Self {
        match mode {
            tui::PromptQueueMode::InterruptAfterTurn => Self::InterruptAfterTurn,
            tui::PromptQueueMode::UntilReady => Self::UntilReady,
            tui::PromptQueueMode::Immediate => Self::Immediate,
        }
    }

    fn preview_label(self) -> &'static str {
        match self {
            Self::InterruptAfterTurn => "after-turn",
            Self::UntilReady => "ready",
            Self::Immediate => "now",
        }
    }

    fn snapshot_label(self) -> &'static str {
        match self {
            Self::InterruptAfterTurn => "interrupt_after_turn",
            Self::UntilReady => "until_ready",
            Self::Immediate => "immediate",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RuntimePromptSubmission {
    pub(crate) text: String,
    pub(crate) image_paths: Vec<PathBuf>,
    pub(crate) actor: RuntimeActor,
    pub(crate) via: ControlSurface,
    pub(crate) metadata: tui::PromptMetadata,
    pub(crate) queue_mode: QueueMode,
}

impl RuntimePromptSubmission {
    pub(crate) fn from_tui(submission: tui::PromptSubmission) -> Self {
        Self {
            text: submission.text,
            image_paths: submission.image_paths,
            actor: RuntimeActor::from_submission(submission.submitted_by, submission.via),
            via: ControlSurface::from_via(submission.via),
            metadata: submission.metadata,
            queue_mode: QueueMode::from_tui(submission.queue_mode),
        }
    }

    pub(crate) fn from_voice(text: String, metadata: tui::VoicePromptMetadata) -> Self {
        Self::from_tui(tui::PromptSubmission {
            text: format!("🎙 {}", text.trim()),
            image_paths: Vec::new(),
            submitted_by: "voice".to_string(),
            via: "voice",
            queue_mode: tui::PromptQueueMode::UntilReady,
            metadata: tui::PromptMetadata {
                voice: Some(metadata),
            },
        })
    }

    pub(crate) fn into_tui(self) -> tui::PromptSubmission {
        tui::PromptSubmission {
            text: self.text,
            image_paths: self.image_paths,
            submitted_by: self.actor.label,
            via: self.via.label(),
            queue_mode: match self.queue_mode {
                QueueMode::InterruptAfterTurn => tui::PromptQueueMode::InterruptAfterTurn,
                QueueMode::UntilReady => tui::PromptQueueMode::UntilReady,
                QueueMode::Immediate => tui::PromptQueueMode::Immediate,
            },
            metadata: self.metadata,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PromptEnvelope {
    pub(crate) id: u64,
    pub(crate) text: String,
    pub(crate) image_paths: Vec<PathBuf>,
    pub(crate) submitted_by: RuntimeActor,
    pub(crate) via: ControlSurface,
    pub(crate) metadata: tui::PromptMetadata,
    pub(crate) queue_mode: QueueMode,
    pub(crate) queued_at: std::time::Instant,
}

impl PromptEnvelope {
    pub(crate) fn requests_voice_close(&self) -> bool {
        self.metadata.voice.as_ref().is_some_and(|voice| {
            voice.close_session_requested == Some(true)
                && voice.radio_cue.as_deref() == Some("over_and_out")
        })
    }
}

#[derive(Debug, Default)]
pub(crate) struct PromptQueue {
    prompts: VecDeque<PromptEnvelope>,
    next_prompt_id: u64,
    default_queue_mode: QueueMode,
}

impl PromptQueue {
    pub(crate) fn enqueue(
        &mut self,
        text: String,
        image_paths: Vec<PathBuf>,
        actor: RuntimeActor,
        via: ControlSurface,
        metadata: tui::PromptMetadata,
        queue_mode: Option<QueueMode>,
    ) -> u64 {
        self.next_prompt_id += 1;
        let prompt_id = self.next_prompt_id;
        self.prompts.push_back(PromptEnvelope {
            id: prompt_id,
            text,
            image_paths,
            submitted_by: actor,
            via,
            metadata,
            queue_mode: queue_mode.unwrap_or(self.default_queue_mode),
            queued_at: std::time::Instant::now(),
        });
        prompt_id
    }

    pub(crate) fn depth(&self) -> usize {
        self.prompts.len()
    }

    pub(crate) fn get(&self, prompt_id: u64) -> Option<&PromptEnvelope> {
        self.prompts.iter().find(|prompt| prompt.id == prompt_id)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &PromptEnvelope> {
        self.prompts.iter()
    }

    pub(crate) fn pop_front(&mut self) -> Option<PromptEnvelope> {
        self.prompts.pop_front()
    }

    pub(crate) fn push_front(&mut self, prompt: PromptEnvelope) {
        self.prompts.push_front(prompt);
    }

    pub(crate) fn clear(&mut self) {
        self.prompts.clear();
    }

    pub(crate) fn snapshot_items(&self) -> Vec<serde_json::Value> {
        self.prompts
            .iter()
            .map(|prompt| {
                serde_json::json!({
                    "id": prompt.id,
                    "submitted_by": prompt.submitted_by.display_label(),
                    "via": prompt.via.label(),
                    "queue_mode": prompt.queue_mode.snapshot_label(),
                    "preview": prompt.text.chars().take(80).collect::<String>(),
                    "attachments": prompt.image_paths.len(),
                    "voice": prompt.metadata.voice.is_some(),
                    "wait_ms": prompt.queued_at.elapsed().as_millis() as u64,
                })
            })
            .collect()
    }

    pub(crate) fn previews(&self) -> Vec<String> {
        self.prompts
            .iter()
            .map(|prompt| {
                let attachment_summary = if prompt.image_paths.is_empty() {
                    String::new()
                } else {
                    let names = prompt
                        .image_paths
                        .iter()
                        .take(3)
                        .filter_map(|path| path.file_name().and_then(|name| name.to_str()))
                        .collect::<Vec<_>>();
                    let suffix = if prompt.image_paths.len() > names.len() {
                        format!(" +{} more", prompt.image_paths.len() - names.len())
                    } else {
                        String::new()
                    };
                    format!(" [{}{}]", names.join(", "), suffix)
                };
                let preview = prompt.text.chars().take(48).collect::<String>();
                let mode = prompt.queue_mode.preview_label();
                format!("#{} {mode}: {}{}", prompt.id, preview, attachment_summary)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actors_use_stable_fallback_labels() {
        let actor = RuntimeActor {
            kind: RuntimeActorKind::WebClient,
            label: String::new(),
        };
        assert_eq!(actor.display_label(), "web-client");
        assert_eq!(RuntimeActor::tui().display_label(), "local-tui");
    }

    #[test]
    fn queue_mode_defaults_to_until_ready() {
        assert_eq!(QueueMode::default(), QueueMode::UntilReady);
    }

    #[test]
    fn prompt_queue_assigns_ids_defaults_modes_and_preserves_requeue() {
        let mut queue = PromptQueue::default();
        let first = queue.enqueue(
            "first".to_string(),
            vec![],
            RuntimeActor::tui(),
            ControlSurface::Tui,
            tui::PromptMetadata::default(),
            None,
        );
        let second = queue.enqueue(
            "second".to_string(),
            vec![],
            RuntimeActor::auspex(),
            ControlSurface::Ipc,
            tui::PromptMetadata::default(),
            Some(QueueMode::InterruptAfterTurn),
        );

        assert_eq!((first, second), (1, 2));
        assert_eq!(queue.depth(), 2);
        assert_eq!(queue.get(first).unwrap().queue_mode, QueueMode::UntilReady);
        assert_eq!(
            queue.get(second).unwrap().queue_mode,
            QueueMode::InterruptAfterTurn
        );

        let prompt = queue.pop_front().unwrap();
        queue.push_front(prompt);
        assert_eq!(queue.pop_front().unwrap().id, first);
        queue.clear();
        assert_eq!(queue.depth(), 0);
    }

    #[test]
    fn prompt_queue_snapshot_items_use_transport_contract_labels() {
        let mut queue = PromptQueue::default();
        queue.enqueue(
            "snapshot me".to_string(),
            vec![PathBuf::from("image.png")],
            RuntimeActor::auspex(),
            ControlSurface::Ipc,
            tui::PromptMetadata::default(),
            Some(QueueMode::InterruptAfterTurn),
        );

        let items = queue.snapshot_items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["id"], 1);
        assert_eq!(items[0]["submitted_by"], "auspex");
        assert_eq!(items[0]["via"], "ipc");
        assert_eq!(items[0]["queue_mode"], "interrupt_after_turn");
        assert_eq!(items[0]["preview"], "snapshot me");
        assert_eq!(items[0]["attachments"], 1);
    }

    #[test]
    fn prompt_queue_previews_include_mode_and_bounded_attachments() {
        let mut queue = PromptQueue::default();
        queue.enqueue(
            "inspect these images".to_string(),
            vec!["a.png", "b.png", "c.png", "d.png"]
                .into_iter()
                .map(PathBuf::from)
                .collect(),
            RuntimeActor::tui(),
            ControlSurface::Tui,
            tui::PromptMetadata::default(),
            Some(QueueMode::Immediate),
        );

        assert_eq!(
            queue.previews(),
            vec!["#1 now: inspect these images [a.png, b.png, c.png +1 more]"]
        );
    }

    #[test]
    fn control_surface_labels_are_transport_specific() {
        assert_eq!(ControlSurface::WebSocket.label(), "websocket");
        assert_eq!(
            ControlSurface::HttpEventIngress.label(),
            "http-event-ingress"
        );
    }
}
