//! RPC protocol types (JSON-RPC 2.0 over stdin/stdout).
//!
//! Protocol v2 adds:
//! - Bidirectional messaging (extensions can send requests/notifications to the host)
//! - Numeric error codes (JSON-RPC 2.0 standard) with string labels preserved
//! - `RpcIncoming` enum for unified parsing of all message types

use crate::ErrorCode;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Top-level RPC message — either a request or notification.
/// Used by the v1 serving loop for backward compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RpcMessage {
    Request(RpcRequest),
    Notification(RpcNotification),
}

/// Incoming RPC message — request, response, or notification.
/// Used by the v2 message router for bidirectional communication.
#[derive(Debug, Clone)]
pub enum RpcIncoming {
    /// A request from the other side (has `method` + `id`).
    Request(RpcRequest),
    /// A response to one of our pending requests (has `id` + `result`/`error`).
    Response(RpcResponse),
    /// A notification from the other side (has `method`, no `id`).
    Notification(RpcNotification),
}

impl RpcIncoming {
    /// Parse a line of JSON into an RpcIncoming message.
    ///
    /// Distinguishes message types by field presence:
    /// - Has `method` + `id` → Request
    /// - Has `result` or `error`, no `method` → Response
    /// - Has `method`, no `id` → Notification
    pub fn parse(line: &str) -> crate::Result<Self> {
        let value: Value = serde_json::from_str(line)?;
        let obj = value
            .as_object()
            .ok_or_else(|| crate::Error::parse_error("expected JSON object"))?;

        let has_method = obj.contains_key("method");
        let has_id = obj.contains_key("id") && !obj["id"].is_null();
        let has_result = obj.contains_key("result");
        let has_error = obj.contains_key("error");

        if has_method && has_id {
            // Request: has method + id
            let req: RpcRequest = serde_json::from_value(value)?;
            Ok(RpcIncoming::Request(req))
        } else if (has_result || has_error) && !has_method {
            // Response: has result/error, no method
            let resp: RpcResponse = serde_json::from_value(value)?;
            Ok(RpcIncoming::Response(resp))
        } else if has_method && !has_id {
            // Notification: has method, no id
            let notif: RpcNotification = serde_json::from_value(value)?;
            Ok(RpcIncoming::Notification(notif))
        } else {
            Err(crate::Error::parse_error(
                "cannot determine message type: expected request (method+id), response (result/error), or notification (method, no id)",
            ))
        }
    }
}

/// RPC request — expects a response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcRequest {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// JSON-RPC 2.0 id — may be a string, number, or null.
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

/// RPC notification — no response expected.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

impl RpcNotification {
    /// Create a new notification.
    pub fn new(method: impl Into<String>, params: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.into(),
            params,
        }
    }
}

/// RPC response (either success or error).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcResponse {
    pub jsonrpc: String,
    /// JSON-RPC 2.0 id — echoed from the request. May be string, number, or null.
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl RpcResponse {
    /// Build a success response.
    pub fn success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// Build an error response with numeric code and label.
    pub fn error(id: Option<Value>, code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(RpcError {
                code: code.numeric(),
                label: code.label().to_string(),
                message: message.into(),
                data: None,
            }),
        }
    }

    /// Build an error response from a raw numeric code (for proxying).
    pub fn error_raw(id: Option<Value>, code: i32, message: impl Into<String>) -> Self {
        let label = ErrorCode::from_numeric(code)
            .map(|c| c.label().to_string())
            .unwrap_or_else(|| format!("Error{}", code));
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(RpcError {
                code,
                label,
                message: message.into(),
                data: None,
            }),
        }
    }

    /// Check if this is an error response.
    pub fn is_error(&self) -> bool {
        self.error.is_some()
    }

    /// Extract the result value, or return the error.
    pub fn into_result(self) -> crate::Result<Value> {
        if let Some(result) = self.result {
            Ok(result)
        } else if let Some(error) = self.error {
            Err(crate::Error::new(
                ErrorCode::from_numeric(error.code).unwrap_or(ErrorCode::InternalError),
                error.message,
            ))
        } else {
            Err(crate::Error::parse_error(
                "invalid RPC response: no result or error",
            ))
        }
    }
}

/// RPC error object.
///
/// v2 format: numeric `code` + string `label`.
/// Backward compatible: deserializes from v1 format where `code` was a string.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcError {
    /// Numeric JSON-RPC error code (e.g. -32601 for MethodNotFound).
    #[serde(deserialize_with = "deserialize_error_code")]
    pub code: i32,

    /// Human-readable label (e.g. "MethodNotFound"). Omegon-specific, not in JSON-RPC spec.
    #[serde(default)]
    pub label: String,

    /// Error description.
    pub message: String,

    /// Optional additional data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// Deserialize error code from either a numeric value (v2) or string label (v1).
fn deserialize_error_code<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<i32, D::Error> {
    let value = Value::deserialize(deserializer)?;
    match &value {
        Value::Number(n) => n
            .as_i64()
            .map(|v| v as i32)
            .ok_or_else(|| serde::de::Error::custom("error code must be an integer")),
        Value::String(s) => {
            // v1 backward compat: string label → numeric code
            ErrorCode::from_label(s)
                .map(|c| c.numeric())
                .ok_or_else(|| serde::de::Error::custom(format!("unknown error code label: {s}")))
        }
        _ => Err(serde::de::Error::custom(
            "error code must be a number or string",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rpc_request_roundtrip() {
        let req = RpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::String("1".to_string())),
            method: "get_tools".to_string(),
            params: serde_json::json!({}),
        };

        let json = serde_json::to_string(&req).unwrap();
        let parsed: RpcRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, Some(Value::String("1".to_string())));
        assert_eq!(parsed.method, "get_tools");
    }

    #[test]
    fn test_rpc_request_numeric_id() {
        let json = r#"{"jsonrpc":"2.0","id":1,"method":"get_tools","params":{}}"#;
        let parsed: RpcMessage = serde_json::from_str(json).unwrap();
        match parsed {
            RpcMessage::Request(req) => {
                assert_eq!(req.id, Some(Value::Number(1.into())));
                assert_eq!(req.method, "get_tools");
            }
            _ => panic!("expected Request"),
        }
    }

    #[test]
    fn test_rpc_response_success() {
        let resp = RpcResponse::success(
            Some(Value::String("1".to_string())),
            serde_json::json!({"status": "ok"}),
        );

        assert_eq!(resp.id, Some(Value::String("1".to_string())));
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_rpc_response_error() {
        let resp = RpcResponse::error(
            Some(Value::String("1".to_string())),
            ErrorCode::MethodNotFound,
            "method not found",
        );

        assert_eq!(resp.id, Some(Value::String("1".to_string())));
        assert!(resp.result.is_none());
        assert!(resp.error.is_some());

        let error = resp.error.unwrap();
        assert_eq!(error.code, -32601);
        assert_eq!(error.label, "MethodNotFound");
    }

    #[test]
    fn test_rpc_error_v2_numeric_roundtrip() {
        let error = RpcError {
            code: -32601,
            label: "MethodNotFound".to_string(),
            message: "not found".to_string(),
            data: None,
        };

        let json = serde_json::to_string(&error).unwrap();
        let parsed: RpcError = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.code, -32601);
        assert_eq!(parsed.label, "MethodNotFound");
    }

    #[test]
    fn test_rpc_error_v1_string_compat() {
        // v1 extensions send code as string
        let json = r#"{"code":"MethodNotFound","message":"not found"}"#;
        let parsed: RpcError = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.code, -32601);
    }

    #[test]
    fn test_incoming_parse_request() {
        let line = r#"{"jsonrpc":"2.0","id":1,"method":"get_tools","params":{}}"#;
        let msg = RpcIncoming::parse(line).unwrap();
        assert!(matches!(msg, RpcIncoming::Request(_)));
    }

    #[test]
    fn test_incoming_parse_response() {
        let line = r#"{"jsonrpc":"2.0","id":1,"result":[]}"#;
        let msg = RpcIncoming::parse(line).unwrap();
        assert!(matches!(msg, RpcIncoming::Response(_)));
    }

    #[test]
    fn test_incoming_parse_error_response() {
        let line = r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"not found"}}"#;
        let msg = RpcIncoming::parse(line).unwrap();
        assert!(matches!(msg, RpcIncoming::Response(_)));
    }

    #[test]
    fn test_incoming_parse_notification() {
        let line = r#"{"jsonrpc":"2.0","method":"notifications/tools/list_changed","params":{}}"#;
        let msg = RpcIncoming::parse(line).unwrap();
        assert!(matches!(msg, RpcIncoming::Notification(_)));
    }

    #[test]
    fn test_notification_creation() {
        let notif = RpcNotification::new("notifications/tools/list_changed", serde_json::json!({}));
        assert_eq!(notif.method, "notifications/tools/list_changed");
        assert_eq!(notif.jsonrpc, "2.0");
    }

    #[test]
    fn test_response_into_result_success() {
        let resp = RpcResponse::success(Some(Value::Number(1.into())), serde_json::json!("ok"));
        let result = resp.into_result().unwrap();
        assert_eq!(result, serde_json::json!("ok"));
    }

    #[test]
    fn test_response_into_result_error() {
        let resp = RpcResponse::error(
            Some(Value::Number(1.into())),
            ErrorCode::MethodNotFound,
            "not found",
        );
        let err = resp.into_result().unwrap_err();
        assert_eq!(err.code(), ErrorCode::MethodNotFound);
    }
}
