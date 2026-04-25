//! Elicitation — extensions request user input from the host.
//!
//! Extensions can ask the user a question via `elicitation/request`.
//! This is sent as an ext→host request through the `HostProxy`.
//!
//! # Omegon-specific features
//!
//! - `vox_eligible` — in headless/daemon mode, route through vox bridge
//! - `vox_channel_hint` — preferred vox connector (e.g. "discord")
//! - `source` in response — tells extension where the answer came from
//!
//! # MCP shim behavior
//!
//! `vox_eligible`, `vox_channel_hint`, and `source` are dropped.
//! Elicitation becomes a standard schema-driven user prompt.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Parameters for `elicitation/request` (ext → host).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElicitationParams {
    /// Message to show the user.
    pub message: String,

    /// JSON Schema defining the expected input shape.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<Value>,

    /// Timeout in milliseconds. If exceeded, result is `action: "decline"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,

    /// Default values pre-filled in the input.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,

    // ─── Omegon-specific (lost in MCP shim) ───

    /// If true, in headless/daemon mode the elicitation can be routed
    /// through a vox bridge connector instead of blocking on a TUI.
    #[serde(default)]
    pub vox_eligible: bool,

    /// Preferred vox connector for the elicitation (e.g. "discord", "slack").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vox_channel_hint: Option<String>,
}

/// User's response action.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ElicitationAction {
    /// User provided input.
    Accept,
    /// User declined / timed out.
    Decline,
}

/// Where the elicitation response came from (Omegon-specific).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ElicitationSource {
    /// TUI modal dialog.
    Tui,
    /// Vox bridge (Discord, Slack, etc.).
    Vox,
    /// API call.
    Api,
}

/// Result of `elicitation/request` (host → ext response).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElicitationResult {
    /// Whether the user accepted or declined.
    pub action: ElicitationAction,

    /// User's input (null if declined).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Value>,

    // ─── Omegon-specific ───

    /// Where the answer came from (Omegon-specific; MCP doesn't report this).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<ElicitationSource>,
}

/// Convenience: elicit user input from the HostProxy.
impl crate::HostProxy {
    /// Request user input from the host.
    ///
    /// Returns the user's response. Requires `elicitation` capability.
    pub async fn elicit(
        &self,
        params: ElicitationParams,
    ) -> crate::Result<ElicitationResult> {
        let value = self
            .request("elicitation/request", serde_json::to_value(&params)?)
            .await?;
        serde_json::from_value(value).map_err(|e| crate::Error::parse_error(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_elicitation_params_roundtrip() {
        let params = ElicitationParams {
            message: "Which Slack workspace should I connect to?".to_string(),
            schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "workspace": {
                        "type": "string",
                        "enum": ["styrene-labs", "client-workspace"]
                    },
                    "confirm": { "type": "boolean" }
                },
                "required": ["workspace"]
            })),
            timeout_ms: Some(60000),
            default: Some(serde_json::json!({"workspace": "styrene-labs"})),
            vox_eligible: true,
            vox_channel_hint: Some("discord".to_string()),
        };

        let json = serde_json::to_string(&params).unwrap();
        let parsed: ElicitationParams = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.message, "Which Slack workspace should I connect to?");
        assert!(parsed.vox_eligible);
        assert_eq!(parsed.vox_channel_hint.as_deref(), Some("discord"));
        assert_eq!(parsed.timeout_ms, Some(60000));
    }

    #[test]
    fn test_elicitation_params_minimal() {
        let json = r#"{"message":"Continue?"}"#;
        let parsed: ElicitationParams = serde_json::from_str(json).unwrap();

        assert_eq!(parsed.message, "Continue?");
        assert!(!parsed.vox_eligible);
        assert!(parsed.schema.is_none());
        assert!(parsed.default.is_none());
    }

    #[test]
    fn test_elicitation_result_accept() {
        let result = ElicitationResult {
            action: ElicitationAction::Accept,
            content: Some(serde_json::json!({"workspace": "styrene-labs", "confirm": true})),
            source: Some(ElicitationSource::Tui),
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: ElicitationResult = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.action, ElicitationAction::Accept);
        assert!(parsed.content.is_some());
        assert_eq!(parsed.source, Some(ElicitationSource::Tui));
    }

    #[test]
    fn test_elicitation_result_decline() {
        let result = ElicitationResult {
            action: ElicitationAction::Decline,
            content: None,
            source: None,
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: ElicitationResult = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.action, ElicitationAction::Decline);
        assert!(parsed.content.is_none());
        assert!(parsed.source.is_none());
    }

    #[test]
    fn test_elicitation_result_via_vox() {
        let result = ElicitationResult {
            action: ElicitationAction::Accept,
            content: Some(serde_json::json!("yes")),
            source: Some(ElicitationSource::Vox),
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: ElicitationResult = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.source, Some(ElicitationSource::Vox));
    }

    #[test]
    fn test_action_serialization() {
        assert_eq!(
            serde_json::to_string(&ElicitationAction::Accept).unwrap(),
            r#""accept""#
        );
        assert_eq!(
            serde_json::to_string(&ElicitationAction::Decline).unwrap(),
            r#""decline""#
        );
    }

    #[test]
    fn test_source_serialization() {
        assert_eq!(
            serde_json::to_string(&ElicitationSource::Tui).unwrap(),
            r#""tui""#
        );
        assert_eq!(
            serde_json::to_string(&ElicitationSource::Vox).unwrap(),
            r#""vox""#
        );
        assert_eq!(
            serde_json::to_string(&ElicitationSource::Api).unwrap(),
            r#""api""#
        );
    }
}
