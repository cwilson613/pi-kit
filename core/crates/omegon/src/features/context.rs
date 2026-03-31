//! Context management provider — handles context_status, context_compact, context_clear tools.
//!
//! Provides the harness with tools for organic context management:
//! - context_status: show current window usage, token budget
//! - context_compact: compress conversation via LLM
//! - context_clear: clear history, start fresh

use async_trait::async_trait;
use omegon_traits::{ContentBlock, Feature, ToolDefinition, ToolResult};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

use crate::tui::TuiCommand;

/// Shared context metrics — updated by main loop, read by ContextProvider
#[derive(Debug, Clone)]
pub struct SharedContextMetrics {
    pub tokens_used: usize,
    pub context_window: usize,
    pub context_class: String,
    pub thinking_level: String,
}

impl SharedContextMetrics {
    pub fn new() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {
            tokens_used: 0,
            context_window: 200000,
            context_class: "unknown".to_string(),
            thinking_level: "unknown".to_string(),
        }))
    }

    pub fn usage_percent(&self) -> u32 {
        if self.context_window > 0 {
            ((self.tokens_used as f64 / self.context_window as f64) * 100.0).min(100.0) as u32
        } else {
            0
        }
    }

    pub fn update(&mut self, tokens_used: usize, context_window: usize, context_class: &str, thinking_level: &str) {
        self.tokens_used = tokens_used;
        self.context_window = context_window;
        self.context_class = context_class.to_string();
        self.thinking_level = thinking_level.to_string();
    }
}

pub struct ContextProvider {
    command_tx: Option<mpsc::Sender<TuiCommand>>,
    metrics: Arc<Mutex<SharedContextMetrics>>,
}

impl ContextProvider {
    pub fn new(metrics: Arc<Mutex<SharedContextMetrics>>) -> Self {
        Self {
            command_tx: None,
            metrics,
        }
    }

    pub fn with_command_tx(mut self, tx: mpsc::Sender<TuiCommand>) -> Self {
        self.command_tx = Some(tx);
        self
    }
}

#[async_trait]
impl Feature for ContextProvider {
    fn name(&self) -> &str {
        "context-provider"
    }

    fn tools(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "context_status".into(),
                label: "Context Status".into(),
                description: "Show current context window usage, token count, and compression statistics.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
            ToolDefinition {
                name: "context_compact".into(),
                label: "Compact Context".into(),
                description: "Compress the conversation history via LLM summarization, freeing tokens for new work.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
            ToolDefinition {
                name: "context_clear".into(),
                label: "Clear Context".into(),
                description: "Clear all conversation history and start fresh. Archives the current session first.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
        ]
    }

    async fn execute(
        &self,
        tool_name: &str,
        _call_id: &str,
        _args: Value,
        _cancel: tokio_util::sync::CancellationToken,
    ) -> anyhow::Result<ToolResult> {
        match tool_name {
            "context_status" => {
                let metrics = self.metrics.lock().unwrap();
                let pct = metrics.usage_percent();
                let result_text = format!(
                    "Context: {}/{} tokens ({}%)\nClass: {}\nThinking: {}",
                    metrics.tokens_used,
                    metrics.context_window,
                    pct,
                    metrics.context_class,
                    metrics.thinking_level
                );

                // Also dispatch to TUI
                if let Some(ref tx) = self.command_tx {
                    let _ = tx.try_send(TuiCommand::ContextStatus);
                }

                Ok(ToolResult {
                    content: vec![ContentBlock::Text { text: result_text }],
                    details: json!({
                        "tokens_used": metrics.tokens_used,
                        "context_window": metrics.context_window,
                        "usage_percent": pct,
                        "class": metrics.context_class,
                        "thinking": metrics.thinking_level,
                    }),
                })
            }

            "context_compact" => {
                // Dispatch to TUI
                if let Some(ref tx) = self.command_tx {
                    let _ = tx.try_send(TuiCommand::ContextCompact);
                }
                Ok(ToolResult {
                    content: vec![ContentBlock::Text {
                        text: "Compression initiated. This may take a moment...".into(),
                    }],
                    details: json!({}),
                })
            }

            "context_clear" => {
                // Dispatch to TUI
                if let Some(ref tx) = self.command_tx {
                    let _ = tx.try_send(TuiCommand::ContextClear);
                }
                Ok(ToolResult {
                    content: vec![ContentBlock::Text {
                        text: "Context clear requested. You will start fresh in the next turn.".into(),
                    }],
                    details: json!({}),
                })
            }

            _ => Err(anyhow::anyhow!("unknown context tool: {}", tool_name)),
        }
    }
}
