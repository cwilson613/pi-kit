//! Audit log — persistent structured event trail for postmortem and diagnostics.
//!
//! Writes a JSONL file at `.omegon/audit-log.jsonl` with every significant
//! event: tool calls (name, args summary, result, error, duration), behavioral
//! decisions (OODA phase, drift, nudge), permission decisions, errors, and
//! cost deltas. Each line is a self-contained JSON object with a timestamp.
//!
//! Unlike the agent journal (narrative markdown), this is machine-parseable
//! and complete — every tool call, every decision, every error, every turn.

use async_trait::async_trait;
use omegon_traits::{BusEvent, BusRequest, Feature};
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

pub struct AuditLog {
    path: PathBuf,
    session_id: String,
}

impl AuditLog {
    pub fn new(cwd: &std::path::Path, session_id: &str) -> Self {
        let dir = crate::setup::find_project_root(cwd).join(".omegon");
        let _ = fs::create_dir_all(&dir);
        Self {
            path: dir.join("audit-log.jsonl"),
            session_id: session_id.to_string(),
        }
    }

    fn append(&self, entry: &AuditEntry) {
        let Ok(json) = serde_json::to_string(entry) else {
            return;
        };
        let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        else {
            return;
        };
        let _ = writeln!(file, "{json}");
    }

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

#[derive(Debug, Serialize)]
struct AuditEntry {
    ts: u64,
    session: String,
    kind: String,
    #[serde(flatten)]
    data: serde_json::Value,
}

#[async_trait]
impl Feature for AuditLog {
    fn name(&self) -> &str {
        "audit-log"
    }

    fn on_event(&mut self, event: &BusEvent) -> Vec<BusRequest> {
        match event {
            BusEvent::SessionStart { session_id, cwd } => {
                self.session_id = session_id.clone();
                self.append(&AuditEntry {
                    ts: Self::now_ms(),
                    session: self.session_id.clone(),
                    kind: "session_start".into(),
                    data: serde_json::json!({
                        "cwd": cwd.display().to_string(),
                    }),
                });
            }
            BusEvent::TurnEnd {
                turn,
                model,
                provider,
                estimated_tokens,
                context_window,
                context_composition,
                actual_input_tokens,
                actual_output_tokens,
                cache_read_tokens,
                dominant_phase,
                drift_kind,
                progress_signal,
                ..
            } => {
                self.append(&AuditEntry {
                    ts: Self::now_ms(),
                    session: self.session_id.clone(),
                    kind: "turn_end".into(),
                    data: serde_json::json!({
                        "turn": turn,
                        "model": model,
                        "provider": provider,
                        "estimated_tokens": estimated_tokens,
                        "context_window": context_window,
                        "actual_input": actual_input_tokens,
                        "actual_output": actual_output_tokens,
                        "cache_read": cache_read_tokens,
                        "phase": dominant_phase.map(|p| format!("{p:?}")),
                        "drift": drift_kind.map(|d| format!("{d:?}")),
                        "progress": format!("{progress_signal:?}"),
                        "ctx": {
                            "system": context_composition.system_tokens,
                            "tools": context_composition.tool_schema_tokens,
                            "conversation": context_composition.conversation_tokens,
                            "memory": context_composition.memory_tokens,
                            "thinking": context_composition.thinking_tokens,
                            "free": context_composition.free_tokens,
                        },
                    }),
                });
            }
            BusEvent::ToolEnd {
                id,
                name,
                result,
                ..
            } => {
                let is_error = result
                    .content
                    .iter()
                    .any(|c| matches!(c, omegon_traits::ContentBlock::Text { text } if text.contains("error") || text.contains("PERMISSION REQUIRED")));
                let result_preview: String = result
                    .content
                    .iter()
                    .filter_map(|c| match c {
                        omegon_traits::ContentBlock::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
                    .chars()
                    .take(200)
                    .collect();

                self.append(&AuditEntry {
                    ts: Self::now_ms(),
                    session: self.session_id.clone(),
                    kind: "tool_end".into(),
                    data: serde_json::json!({
                        "id": id,
                        "tool": name,
                        "is_error": is_error,
                        "result_preview": result_preview,
                        "details": result.details,
                    }),
                });
            }
            _ => {}
        }
        vec![]
    }
}
