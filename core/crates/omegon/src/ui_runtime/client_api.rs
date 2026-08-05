//! Stable external client API root for non-TUI interfaces.
//!
//! These DTOs are the versioned wire-facing contract for external clients. They
//! intentionally carry generic JSON payloads at the protocol root so Omegon can
//! stabilize transport/version semantics before freezing every internal command
//! and surface shape as a public API.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Stable external client protocol version.
pub const CLIENT_API_VERSION: u32 = 1;

/// Wire-level envelope direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClientEnvelopeDirection {
    ClientToRuntime,
    RuntimeToClient,
}

/// Wire-level envelope kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClientEnvelopeKind {
    /// Operator intent, such as prompt submission or cancellation.
    Command,
    /// Semantic control request. Payloads should name boundary commands, not
    /// backend implementation variants.
    ControlRequest,
    /// Renderer-neutral UI action from a client.
    UiAction,
    /// Client request to subscribe to one or more semantic surfaces.
    SurfaceSubscription,
    /// Semantic surface snapshot or update emitted by the runtime.
    SurfaceUpdate,
    /// Command/action/control response emitted by the runtime.
    Outcome,
    /// Capability/version negotiation between client and runtime.
    CapabilityHello,
}

/// Versioned external client envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientEnvelope {
    pub protocol_version: u32,
    pub envelope_id: String,
    pub session_id: Option<String>,
    pub client_id: String,
    pub direction: ClientEnvelopeDirection,
    pub kind: ClientEnvelopeKind,
    pub payload: Value,
}

impl ClientEnvelope {
    pub fn new(
        envelope_id: impl Into<String>,
        client_id: impl Into<String>,
        direction: ClientEnvelopeDirection,
        kind: ClientEnvelopeKind,
        payload: Value,
    ) -> Self {
        Self {
            protocol_version: CLIENT_API_VERSION,
            envelope_id: envelope_id.into(),
            session_id: None,
            client_id: client_id.into(),
            direction,
            kind,
            payload,
        }
    }

    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }
}

/// Minimal client/runtime capability negotiation payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientCapabilityHello {
    pub client_name: String,
    pub client_version: Option<String>,
    pub protocol_versions: Vec<u32>,
    pub surfaces: Vec<String>,
    pub commands: Vec<String>,
}

impl ClientCapabilityHello {
    pub fn supports_v1(client_name: impl Into<String>) -> Self {
        Self {
            client_name: client_name.into(),
            client_version: None,
            protocol_versions: vec![CLIENT_API_VERSION],
            surfaces: Vec::new(),
            commands: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_envelope_uses_stable_camel_case_wire_shape() {
        let envelope = ClientEnvelope::new(
            "env-1",
            "client-1",
            ClientEnvelopeDirection::ClientToRuntime,
            ClientEnvelopeKind::ControlRequest,
            serde_json::json!({ "name": "contextStatus" }),
        )
        .with_session("session-1");

        let value = serde_json::to_value(envelope).expect("serialize client envelope");
        assert_eq!(value["protocolVersion"], CLIENT_API_VERSION);
        assert_eq!(value["envelopeId"], "env-1");
        assert_eq!(value["sessionId"], "session-1");
        assert_eq!(value["clientId"], "client-1");
        assert_eq!(value["direction"], "clientToRuntime");
        assert_eq!(value["kind"], "controlRequest");
        assert_eq!(value["payload"]["name"], "contextStatus");
    }

    #[test]
    fn capability_hello_advertises_supported_protocol_versions() {
        let hello = ClientCapabilityHello::supports_v1("replacement-ui");
        let value = serde_json::to_value(hello).expect("serialize hello");
        assert_eq!(value["clientName"], "replacement-ui");
        assert_eq!(value["protocolVersions"], serde_json::json!([1]));
        assert_eq!(value["surfaces"], serde_json::json!([]));
    }
}
