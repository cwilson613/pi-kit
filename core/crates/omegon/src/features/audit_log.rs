//! Audit log — persistent structured event trail for postmortem and diagnostics.
//!
//! Writes a JSONL file at `.omegon/audit-log.jsonl` with every significant
//! event in the session. Each line is a self-contained JSON object.
//!
//! Events captured:
//! - session_start / session_end
//! - turn_end (model, tokens, OODA phase, drift, progress, context breakdown)
//! - tool_start (name, args summary)
//! - tool_end (name, result preview, error flag, details)
//! - permission_decision (path, approve/deny)
//! - nudge_injected (reason, message preview)
//! - compacted (context was compacted)
//!
//! Diagnostic queries:
//!   jq 'select(.kind=="nudge")' .omegon/audit-log.jsonl
//!   jq 'select(.kind=="tool_end" and .is_error==true)' .omegon/audit-log.jsonl
//!   jq 'select(.kind=="permission")' .omegon/audit-log.jsonl
//!   jq 'select(.kind=="turn") | {turn, phase, drift}' .omegon/audit-log.jsonl

use async_trait::async_trait;
use omegon_traits::{BusEvent, BusRequest, ContentBlock, Feature};
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

    fn text_preview(result: &omegon_traits::ToolResult, max: usize) -> String {
        result
            .content
            .iter()
            .filter_map(|c| match c {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(max)
            .collect()
    }

    fn args_summary(args: &serde_json::Value) -> serde_json::Value {
        // Keep path, command, action — drop large content fields
        let mut summary = serde_json::Map::new();
        if let Some(obj) = args.as_object() {
            for (k, v) in obj {
                match k.as_str() {
                    "content" | "old_string" | "new_string" | "source" => {
                        // Truncate large string values
                        if let Some(s) = v.as_str() {
                            summary.insert(
                                k.clone(),
                                serde_json::Value::String(
                                    s.chars().take(80).collect::<String>()
                                        + if s.len() > 80 { "…" } else { "" },
                                ),
                            );
                        } else {
                            summary.insert(k.clone(), v.clone());
                        }
                    }
                    _ => {
                        summary.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        serde_json::Value::Object(summary)
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
        let ts = Self::now_ms();
        let session = self.session_id.clone();

        match event {
            BusEvent::SessionStart { session_id, cwd } => {
                self.session_id = session_id.clone();
                self.append(&AuditEntry {
                    ts,
                    session: session_id.clone(),
                    kind: "session_start".into(),
                    data: serde_json::json!({ "cwd": cwd.display().to_string() }),
                });
            }

            BusEvent::SessionEnd {
                turns,
                tool_calls,
                duration_secs,
                initial_prompt,
                outcome_summary,
            } => {
                self.append(&AuditEntry {
                    ts,
                    session,
                    kind: "session_end".into(),
                    data: serde_json::json!({
                        "turns": turns,
                        "tool_calls": tool_calls,
                        "duration_secs": duration_secs,
                        "initial_prompt": initial_prompt.as_deref().map(|s| &s[..s.len().min(200)]),
                        "outcome": outcome_summary.as_deref().map(|s| &s[..s.len().min(200)]),
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
                provider_telemetry,
                ..
            } => {
                self.append(&AuditEntry {
                    ts,
                    session,
                    kind: "turn".into(),
                    data: serde_json::json!({
                        "turn": turn,
                        "model": model,
                        "provider": provider,
                        "est_tokens": estimated_tokens,
                        "ctx_window": context_window,
                        "in": actual_input_tokens,
                        "out": actual_output_tokens,
                        "cache": cache_read_tokens,
                        "phase": dominant_phase.map(|p| format!("{p:?}")),
                        "drift": drift_kind.map(|d| format!("{d:?}")),
                        "progress": format!("{progress_signal:?}"),
                        "ctx": {
                            "sys": context_composition.system_tokens,
                            "tools": context_composition.tool_schema_tokens,
                            "conv": context_composition.conversation_tokens,
                            "mem": context_composition.memory_tokens,
                            "think": context_composition.thinking_tokens,
                            "free": context_composition.free_tokens,
                        },
                        "quota": provider_telemetry.as_ref().map(|t| serde_json::to_value(t).unwrap_or_default()),
                    }),
                });
            }

            BusEvent::ToolStart { id, name, args } => {
                self.append(&AuditEntry {
                    ts,
                    session,
                    kind: "tool_start".into(),
                    data: serde_json::json!({
                        "id": id,
                        "tool": name,
                        "args": Self::args_summary(args),
                    }),
                });
            }

            BusEvent::ToolEnd {
                id,
                name,
                result,
                is_error,
            } => {
                self.append(&AuditEntry {
                    ts,
                    session,
                    kind: "tool_end".into(),
                    data: serde_json::json!({
                        "id": id,
                        "tool": name,
                        "error": is_error,
                        "preview": Self::text_preview(result, 200),
                        "details": result.details,
                    }),
                });
            }

            BusEvent::PermissionDecision {
                tool_name,
                path,
                decision,
            } => {
                self.append(&AuditEntry {
                    ts,
                    session,
                    kind: "permission".into(),
                    data: serde_json::json!({
                        "tool": tool_name,
                        "path": path,
                        "decision": decision,
                    }),
                });
            }

            BusEvent::NudgeInjected {
                turn,
                reason,
                message_preview,
            } => {
                self.append(&AuditEntry {
                    ts,
                    session,
                    kind: "nudge".into(),
                    data: serde_json::json!({
                        "turn": turn,
                        "reason": reason,
                        "message": message_preview,
                    }),
                });
            }

            BusEvent::Compacted => {
                self.append(&AuditEntry {
                    ts,
                    session,
                    kind: "compacted".into(),
                    data: serde_json::json!({}),
                });
            }

            _ => {}
        }
        vec![]
    }
}
