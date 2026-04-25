//! Prompt types — templated prompt expansion that extensions can provide.
//!
//! Extensions expose prompt templates that can be expanded with arguments.
//! On Omegon, prompts can be enriched with mind facts and project context.
//! On MCP, these enrichment features are silently dropped.
//!
//! # Omegon-specific metadata
//!
//! - `mind_context` — host injects relevant mind facts into expansion
//! - `inject_project_context` — host injects project-level context
//! - `PromptContent::MindFacts` — resolved by host to matched facts

use serde::{Deserialize, Serialize};

/// A prompt template exposed by the extension.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prompt {
    /// Prompt identifier.
    pub name: String,

    /// Human-readable description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Arguments this prompt accepts.
    #[serde(default)]
    pub arguments: Vec<PromptArgument>,

    // ─── Omegon-specific (lost in MCP shim) ───

    /// When true, host injects relevant mind facts into the expanded prompt.
    #[serde(default)]
    pub mind_context: bool,

    /// When true, host injects project context (branch, recent files, etc.).
    #[serde(default)]
    pub inject_project_context: bool,
}

/// A named argument for a prompt template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptArgument {
    /// Argument name.
    pub name: String,

    /// Description of the argument.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Whether this argument is required.
    #[serde(default)]
    pub required: bool,
}

/// Content within a prompt message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PromptContent {
    /// Plain text content.
    #[serde(rename = "text")]
    Text { text: String },

    /// Image content (base64-encoded).
    #[serde(rename = "image")]
    Image { data: String, mime_type: String },

    /// Mind facts query — resolved by host to matched facts.
    /// Omegon-specific; MCP shim drops this content type.
    #[serde(rename = "mind_facts")]
    MindFacts { query: String, limit: usize },
}

/// A message in a prompt expansion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptMessage {
    /// Role: "user", "assistant", or "system".
    pub role: String,

    /// Message content.
    pub content: PromptContent,
}

/// Parameters for `prompts/list` request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListPromptsParams {
    /// Pagination cursor (null for first page).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

/// Result of `prompts/list`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListPromptsResult {
    /// Prompts on this page.
    pub prompts: Vec<Prompt>,

    /// Next page cursor (null if last page).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// Parameters for `prompts/get` request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetPromptParams {
    /// Prompt name.
    pub name: String,

    /// Arguments for template expansion.
    #[serde(default)]
    pub arguments: std::collections::HashMap<String, String>,
}

/// Result of `prompts/get`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetPromptResult {
    /// Description of the expanded prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Expanded messages.
    pub messages: Vec<PromptMessage>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_roundtrip() {
        let prompt = Prompt {
            name: "engagement_summary".to_string(),
            description: Some("Summarize engagement status".to_string()),
            arguments: vec![PromptArgument {
                name: "client_name".to_string(),
                description: Some("Client to summarize".to_string()),
                required: true,
            }],
            mind_context: true,
            inject_project_context: false,
        };

        let json = serde_json::to_string(&prompt).unwrap();
        let parsed: Prompt = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.name, "engagement_summary");
        assert!(parsed.mind_context);
        assert!(!parsed.inject_project_context);
        assert_eq!(parsed.arguments.len(), 1);
        assert!(parsed.arguments[0].required);
    }

    #[test]
    fn test_prompt_minimal() {
        let json = r#"{"name":"hello"}"#;
        let parsed: Prompt = serde_json::from_str(json).unwrap();

        assert_eq!(parsed.name, "hello");
        assert!(parsed.arguments.is_empty());
        assert!(!parsed.mind_context);
    }

    #[test]
    fn test_prompt_content_text() {
        let content = PromptContent::Text {
            text: "Summarize the Recro engagement.".to_string(),
        };

        let json = serde_json::to_string(&content).unwrap();
        let parsed: PromptContent = serde_json::from_str(&json).unwrap();

        match parsed {
            PromptContent::Text { text } => {
                assert_eq!(text, "Summarize the Recro engagement.");
            }
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn test_prompt_content_image() {
        let content = PromptContent::Image {
            data: "iVBORw0KGgo=".to_string(),
            mime_type: "image/png".to_string(),
        };

        let json = serde_json::to_string(&content).unwrap();
        let parsed: PromptContent = serde_json::from_str(&json).unwrap();

        match parsed {
            PromptContent::Image { data, mime_type } => {
                assert_eq!(mime_type, "image/png");
                assert!(!data.is_empty());
            }
            _ => panic!("expected Image"),
        }
    }

    #[test]
    fn test_prompt_content_mind_facts() {
        let content = PromptContent::MindFacts {
            query: "engagement status".to_string(),
            limit: 5,
        };

        let json = serde_json::to_string(&content).unwrap();
        let parsed: PromptContent = serde_json::from_str(&json).unwrap();

        match parsed {
            PromptContent::MindFacts { query, limit } => {
                assert_eq!(query, "engagement status");
                assert_eq!(limit, 5);
            }
            _ => panic!("expected MindFacts"),
        }
    }

    #[test]
    fn test_prompt_message_roundtrip() {
        let msg = PromptMessage {
            role: "user".to_string(),
            content: PromptContent::Text {
                text: "Review this code.".to_string(),
            },
        };

        let json = serde_json::to_string(&msg).unwrap();
        let parsed: PromptMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.role, "user");
    }

    #[test]
    fn test_get_prompt_result() {
        let result = GetPromptResult {
            description: Some("Code review prompt".to_string()),
            messages: vec![
                PromptMessage {
                    role: "user".to_string(),
                    content: PromptContent::Text {
                        text: "Review the following code:".to_string(),
                    },
                },
                PromptMessage {
                    role: "user".to_string(),
                    content: PromptContent::MindFacts {
                        query: "code review patterns".to_string(),
                        limit: 3,
                    },
                },
            ],
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: GetPromptResult = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.messages.len(), 2);
    }

    #[test]
    fn test_list_prompts_result_pagination() {
        let result = ListPromptsResult {
            prompts: vec![Prompt {
                name: "review".to_string(),
                description: None,
                arguments: vec![],
                mind_context: false,
                inject_project_context: false,
            }],
            next_cursor: Some("page2".to_string()),
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: ListPromptsResult = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.prompts.len(), 1);
        assert_eq!(parsed.next_cursor.as_deref(), Some("page2"));
    }

    #[test]
    fn test_get_prompt_params() {
        let params = GetPromptParams {
            name: "engagement_summary".to_string(),
            arguments: [("client_name".to_string(), "Recro".to_string())]
                .into_iter()
                .collect(),
        };

        let json = serde_json::to_string(&params).unwrap();
        let parsed: GetPromptParams = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.name, "engagement_summary");
        assert_eq!(parsed.arguments.get("client_name").unwrap(), "Recro");
    }
}
