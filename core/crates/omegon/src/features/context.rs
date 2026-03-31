//! Context management provider — handles context_status, context_compact, context_clear tools.
//!
//! Provides the harness with tools for organic context management:
//! - context_status: show current window usage, token budget
//! - context_compact: compress conversation via LLM
//! - context_clear: clear history, start fresh

use async_trait::async_trait;
use omegon_traits::{ContentBlock, Feature, ToolDefinition, ToolResult};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::tui::TuiCommand;

pub struct ContextProvider {
    command_tx: Option<mpsc::Sender<TuiCommand>>,
}

impl ContextProvider {
    pub fn new() -> Self {
        Self { command_tx: None }
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
                // Dispatch to TUI
                if let Some(ref tx) = self.command_tx {
                    let _ = tx.try_send(TuiCommand::ContextStatus);
                }
                Ok(ToolResult {
                    content: vec![ContentBlock::Text {
                        text: "Context status requested. Check the footer for current usage metrics.".into(),
                    }],
                    details: json!({}),
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
