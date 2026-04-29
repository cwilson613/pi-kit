//! Sampling — extensions request LLM completions from the host.
//!
//! Extensions can ask the host's LLM to process a prompt via `sampling/create_message`.
//! This is sent as an ext→host request through the `HostProxy`.
//!
//! # Omegon-specific features
//!
//! - `route` field: `"cloud"` (default), `"local_preferred"`, `"local_only"`
//!   Integrates with Omegon's `local_inference` feature.
//! - `usage` in response: token counts for cost tracking.
//!
//! # MCP shim behavior
//!
//! The `route` field is dropped. MCP sampling has no routing concept;
//! the client picks the model. `usage` is Omegon-specific metadata.

use serde::{Deserialize, Serialize};

/// Content within a sampling message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SamplingContent {
    /// Plain text.
    #[serde(rename = "text")]
    Text { text: String },

    /// Image (base64-encoded).
    #[serde(rename = "image")]
    Image { data: String, mime_type: String },
}

/// A message in a sampling request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingMessage {
    /// Role: "user" or "assistant".
    pub role: String,

    /// Message content.
    pub content: SamplingContent,
}

/// Model selection preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPreference {
    /// Model name hints (treated as substrings).
    #[serde(default)]
    pub hints: Vec<ModelHint>,

    /// Cost priority (0.0-1.0, higher = prefer cheaper).
    #[serde(default)]
    pub cost_priority: f32,

    /// Speed priority (0.0-1.0, higher = prefer faster).
    #[serde(default)]
    pub speed_priority: f32,

    /// Intelligence priority (0.0-1.0, higher = prefer smarter).
    #[serde(default)]
    pub intelligence_priority: f32,
}

/// A model name hint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelHint {
    /// Model name or family substring (e.g. "claude-sonnet").
    pub name: String,
}

/// Routing preference for where to run the completion.
/// Omegon-specific; dropped by MCP shim.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum SamplingRoute {
    /// Use cloud provider (default).
    #[default]
    Cloud,
    /// Try local inference first, fall back to cloud.
    LocalPreferred,
    /// Fail if no local model available.
    LocalOnly,
}

/// Parameters for `sampling/create_message` request (ext → host).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMessageParams {
    /// Messages in the conversation.
    pub messages: Vec<SamplingMessage>,

    /// Model selection preferences.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_preference: Option<ModelPreference>,

    /// Maximum tokens to generate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,

    /// System prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,

    /// Temperature (0.0-1.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,

    /// Stop sequences.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stop_sequences: Vec<String>,

    // ─── Omegon-specific (lost in MCP shim) ───
    /// Where to route the completion.
    #[serde(default)]
    pub route: SamplingRoute,
}

/// Result of `sampling/create_message` (host → ext response).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMessageResult {
    /// Role of the response ("assistant").
    pub role: String,

    /// Response content.
    pub content: SamplingContent,

    /// Model that generated the response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Why generation stopped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,

    // ─── Omegon-specific ───
    /// Token usage (Omegon-specific; MCP doesn't report this).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsage>,
}

/// Token usage statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Input tokens consumed.
    pub input_tokens: u32,
    /// Output tokens generated.
    pub output_tokens: u32,
}

/// Convenience: create a sampling request from the HostProxy.
impl crate::HostProxy {
    /// Request an LLM completion from the host.
    ///
    /// Returns the assistant's response. Requires `sampling` capability.
    pub async fn create_message(
        &self,
        params: CreateMessageParams,
    ) -> crate::Result<CreateMessageResult> {
        let value = self
            .request("sampling/create_message", serde_json::to_value(&params)?)
            .await?;
        serde_json::from_value(value).map_err(|e| crate::Error::parse_error(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_message_params_roundtrip() {
        let params = CreateMessageParams {
            messages: vec![SamplingMessage {
                role: "user".to_string(),
                content: SamplingContent::Text {
                    text: "Classify this: ...".to_string(),
                },
            }],
            model_preference: Some(ModelPreference {
                hints: vec![ModelHint {
                    name: "claude-sonnet".to_string(),
                }],
                cost_priority: 0.8,
                speed_priority: 0.5,
                intelligence_priority: 0.3,
            }),
            max_tokens: Some(500),
            system_prompt: Some("You are a classifier.".to_string()),
            temperature: Some(0.0),
            stop_sequences: vec![],
            route: SamplingRoute::LocalPreferred,
        };

        let json = serde_json::to_string(&params).unwrap();
        let parsed: CreateMessageParams = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.messages.len(), 1);
        assert_eq!(parsed.max_tokens, Some(500));
        assert!(matches!(parsed.route, SamplingRoute::LocalPreferred));
    }

    #[test]
    fn test_create_message_params_minimal() {
        let json = r#"{"messages":[{"role":"user","content":{"type":"text","text":"hi"}}]}"#;
        let parsed: CreateMessageParams = serde_json::from_str(json).unwrap();

        assert_eq!(parsed.messages.len(), 1);
        assert!(parsed.model_preference.is_none());
        assert!(parsed.max_tokens.is_none());
        assert!(matches!(parsed.route, SamplingRoute::Cloud));
    }

    #[test]
    fn test_create_message_result_roundtrip() {
        let result = CreateMessageResult {
            role: "assistant".to_string(),
            content: SamplingContent::Text {
                text: "Category: B".to_string(),
            },
            model: Some("claude-sonnet-4-20250514".to_string()),
            stop_reason: Some("end_turn".to_string()),
            usage: Some(TokenUsage {
                input_tokens: 45,
                output_tokens: 8,
            }),
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: CreateMessageResult = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.role, "assistant");
        assert_eq!(parsed.model.as_deref(), Some("claude-sonnet-4-20250514"));
        assert_eq!(parsed.usage.as_ref().unwrap().input_tokens, 45);
    }

    #[test]
    fn test_sampling_content_text() {
        let content = SamplingContent::Text {
            text: "hello".to_string(),
        };
        let json = serde_json::to_string(&content).unwrap();
        let parsed: SamplingContent = serde_json::from_str(&json).unwrap();
        match parsed {
            SamplingContent::Text { text } => assert_eq!(text, "hello"),
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn test_sampling_route_default() {
        let route = SamplingRoute::default();
        assert!(matches!(route, SamplingRoute::Cloud));
    }

    #[test]
    fn test_sampling_route_serialization() {
        assert_eq!(
            serde_json::to_string(&SamplingRoute::Cloud).unwrap(),
            r#""cloud""#
        );
        assert_eq!(
            serde_json::to_string(&SamplingRoute::LocalPreferred).unwrap(),
            r#""local_preferred""#
        );
        assert_eq!(
            serde_json::to_string(&SamplingRoute::LocalOnly).unwrap(),
            r#""local_only""#
        );
    }

    #[test]
    fn test_model_preference() {
        let pref = ModelPreference {
            hints: vec![
                ModelHint {
                    name: "claude-sonnet".to_string(),
                },
                ModelHint {
                    name: "claude-haiku".to_string(),
                },
            ],
            cost_priority: 0.8,
            speed_priority: 0.5,
            intelligence_priority: 0.3,
        };

        let json = serde_json::to_string(&pref).unwrap();
        let parsed: ModelPreference = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.hints.len(), 2);
        assert_eq!(parsed.hints[0].name, "claude-sonnet");
    }
}
