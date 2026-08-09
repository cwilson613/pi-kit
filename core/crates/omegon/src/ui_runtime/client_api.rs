//! Stable external client API root for non-TUI interfaces.
//!
//! These DTOs are the versioned wire-facing contract for external clients. They
//! intentionally carry generic JSON payloads at the protocol root so Omegon can
//! stabilize transport/version semantics before freezing every internal command
//! and surface shape as a public API.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::operator_commands::InterfaceControlRequest;
use crate::surfaces::layout::UiPresentationLevel;
use crate::ui_runtime::envelope::UiSurfaceKind;

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

/// Stable v1 subset of client-addressable control requests.
///
/// V1 includes operational commands plus read-only runtime-resource inventory.
/// Mutating profile, workspace, permission, skill, extension, and package
/// operations remain intentionally excluded until their revision and approval
/// contracts are stable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "name", rename_all = "camelCase")]
pub enum ClientControlRequestDto {
    ContextStatus,
    ContextCompact,
    ContextClear,
    NewSession,
    StatusView,
    ModelView,
    ModelList,
    RuntimeInventoryStatus,
    ProfileView,
    WorkspaceStatusView,
    WorkspaceListView,
    SkillsView,
    ExtensionView,
    ArmoryBrowse,
    CatalogView,
    PluginView,
    PermissionsView,
    SetPresentationLevel { level: UiPresentationLevel },
}

impl ClientControlRequestDto {
    pub fn into_interface_request(self) -> InterfaceControlRequest {
        match self {
            Self::ContextStatus => InterfaceControlRequest::ContextStatus,
            Self::ContextCompact => InterfaceControlRequest::ContextCompact,
            Self::ContextClear => InterfaceControlRequest::ContextClear,
            Self::NewSession => InterfaceControlRequest::NewSession,
            Self::StatusView => InterfaceControlRequest::StatusView,
            Self::ModelView => InterfaceControlRequest::ModelView,
            Self::ModelList => InterfaceControlRequest::ModelList,
            Self::RuntimeInventoryStatus => InterfaceControlRequest::RuntimeInventoryStatus,
            Self::ProfileView => InterfaceControlRequest::ProfileView,
            Self::WorkspaceStatusView => InterfaceControlRequest::WorkspaceStatusView,
            Self::WorkspaceListView => InterfaceControlRequest::WorkspaceListView,
            Self::SkillsView => InterfaceControlRequest::SkillsView,
            Self::ExtensionView => InterfaceControlRequest::ExtensionView,
            Self::ArmoryBrowse => InterfaceControlRequest::ArmoryBrowse { query: None },
            Self::CatalogView => InterfaceControlRequest::CatalogView,
            Self::PluginView => InterfaceControlRequest::PluginView,
            Self::PermissionsView => InterfaceControlRequest::PermissionsView,
            Self::SetPresentationLevel { level } => {
                InterfaceControlRequest::SetPresentationLevel { level }
            }
        }
    }
}

impl From<ClientControlRequestDto> for InterfaceControlRequest {
    fn from(value: ClientControlRequestDto) -> Self {
        value.into_interface_request()
    }
}

pub fn decode_client_control_request(
    payload: Value,
) -> serde_json::Result<ClientControlRequestDto> {
    serde_json::from_value(payload)
}

pub fn encode_client_control_request(
    request: &ClientControlRequestDto,
) -> serde_json::Result<Value> {
    serde_json::to_value(request)
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ClientEnvelopeError {
    #[error("unsupported client protocol version {actual}; expected {expected}")]
    UnsupportedVersion { actual: u32, expected: u32 },
    #[error("envelope direction {actual:?} is invalid for {expected:?} dispatch")]
    InvalidDirection {
        actual: ClientEnvelopeDirection,
        expected: ClientEnvelopeDirection,
    },
    #[error("envelope kind {actual:?} is invalid for {expected:?} dispatch")]
    InvalidKind {
        actual: ClientEnvelopeKind,
        expected: ClientEnvelopeKind,
    },
    #[error("invalid control request payload: {0}")]
    InvalidControlRequest(String),
    #[error("invalid envelope payload: {0}")]
    InvalidPayload(String),
}

pub fn validate_client_envelope(
    envelope: &ClientEnvelope,
    expected_direction: ClientEnvelopeDirection,
    expected_kind: ClientEnvelopeKind,
) -> Result<(), ClientEnvelopeError> {
    if envelope.protocol_version != CLIENT_API_VERSION {
        return Err(ClientEnvelopeError::UnsupportedVersion {
            actual: envelope.protocol_version,
            expected: CLIENT_API_VERSION,
        });
    }
    if envelope.direction != expected_direction {
        return Err(ClientEnvelopeError::InvalidDirection {
            actual: envelope.direction,
            expected: expected_direction,
        });
    }
    if envelope.kind != expected_kind {
        return Err(ClientEnvelopeError::InvalidKind {
            actual: envelope.kind,
            expected: expected_kind,
        });
    }
    Ok(())
}

pub fn decode_client_control_envelope(
    envelope: ClientEnvelope,
) -> Result<ClientControlRequestDto, ClientEnvelopeError> {
    validate_client_envelope(
        &envelope,
        ClientEnvelopeDirection::ClientToRuntime,
        ClientEnvelopeKind::ControlRequest,
    )?;
    decode_client_control_request(envelope.payload)
        .map_err(|error| ClientEnvelopeError::InvalidControlRequest(error.to_string()))
}

pub fn client_control_envelope_to_interface_request(
    envelope: ClientEnvelope,
) -> Result<InterfaceControlRequest, ClientEnvelopeError> {
    decode_client_control_envelope(envelope).map(Into::into)
}

/// Transport-neutral dispatch target decoded from an external client envelope.
///
/// Transport adapters should validate client envelopes with this helper and then
/// wrap the returned request in their local response channel/envelope type. This
/// keeps protocol validation centralized without making `ui_runtime` depend on
/// Tokio channels, WebSocket state, IPC state, or the backend control runtime.
#[derive(Debug)]
pub struct ClientControlDispatch {
    pub envelope_id: String,
    pub client_id: String,
    pub session_id: Option<String>,
    pub request: InterfaceControlRequest,
}

pub fn decode_client_control_dispatch(
    envelope: ClientEnvelope,
) -> Result<ClientControlDispatch, ClientEnvelopeError> {
    let envelope_id = envelope.envelope_id.clone();
    let client_id = envelope.client_id.clone();
    let session_id = envelope.session_id.clone();
    let request = client_control_envelope_to_interface_request(envelope)?;
    Ok(ClientControlDispatch {
        envelope_id,
        client_id,
        session_id,
        request,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientSurfaceSubscriptionDto {
    pub surfaces: Vec<UiSurfaceKind>,
    pub since_revision: Option<u64>,
    pub include_snapshot: bool,
}

impl ClientSurfaceSubscriptionDto {
    pub fn snapshot(surfaces: impl IntoIterator<Item = UiSurfaceKind>) -> Self {
        Self {
            surfaces: surfaces.into_iter().collect(),
            since_revision: None,
            include_snapshot: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientSurfaceUpdateDto {
    pub surface: UiSurfaceKind,
    pub revision: u64,
    pub payload: Value,
}

pub fn decode_client_surface_subscription_envelope(
    envelope: ClientEnvelope,
) -> Result<ClientSurfaceSubscriptionDto, ClientEnvelopeError> {
    validate_client_envelope(
        &envelope,
        ClientEnvelopeDirection::ClientToRuntime,
        ClientEnvelopeKind::SurfaceSubscription,
    )?;
    serde_json::from_value(envelope.payload)
        .map_err(|error| ClientEnvelopeError::InvalidPayload(error.to_string()))
}

pub fn encode_client_surface_update_envelope(
    envelope_id: impl Into<String>,
    client_id: impl Into<String>,
    session_id: impl Into<String>,
    update: &ClientSurfaceUpdateDto,
) -> serde_json::Result<ClientEnvelope> {
    Ok(ClientEnvelope::new(
        envelope_id,
        client_id,
        ClientEnvelopeDirection::RuntimeToClient,
        ClientEnvelopeKind::SurfaceUpdate,
        serde_json::to_value(update)?,
    )
    .with_session(session_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(raw: &str) -> serde_json::Value {
        serde_json::from_str(raw).expect("valid fixture json")
    }

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

    #[test]
    fn client_control_request_dto_uses_named_wire_shape() {
        let payload =
            encode_client_control_request(&ClientControlRequestDto::SetPresentationLevel {
                level: UiPresentationLevel::Active,
            })
            .expect("encode control request");

        assert_eq!(payload["name"], "setPresentationLevel");
        assert_eq!(payload["level"], "active");

        let decoded = decode_client_control_request(payload).expect("decode control request");
        assert_eq!(
            decoded,
            ClientControlRequestDto::SetPresentationLevel {
                level: UiPresentationLevel::Active,
            }
        );
    }

    #[test]
    fn v1_read_only_inventory_requests_map_to_interface_boundary() {
        let cases = [
            (
                ClientControlRequestDto::RuntimeInventoryStatus,
                "runtimeInventoryStatus",
            ),
            (ClientControlRequestDto::ProfileView, "profileView"),
            (
                ClientControlRequestDto::WorkspaceStatusView,
                "workspaceStatusView",
            ),
            (
                ClientControlRequestDto::WorkspaceListView,
                "workspaceListView",
            ),
            (ClientControlRequestDto::SkillsView, "skillsView"),
            (ClientControlRequestDto::ExtensionView, "extensionView"),
            (ClientControlRequestDto::ArmoryBrowse, "armoryBrowse"),
            (ClientControlRequestDto::CatalogView, "catalogView"),
            (ClientControlRequestDto::PluginView, "pluginView"),
            (ClientControlRequestDto::PermissionsView, "permissionsView"),
        ];
        for (request, wire_name) in cases {
            let payload =
                encode_client_control_request(&request).expect("encode inventory request");
            assert_eq!(payload["name"], wire_name);
            let decoded = decode_client_control_request(payload).expect("decode inventory request");
            assert_eq!(decoded, request);
        }
        assert!(matches!(
            ClientControlRequestDto::RuntimeInventoryStatus.into_interface_request(),
            InterfaceControlRequest::RuntimeInventoryStatus
        ));
        assert!(matches!(
            ClientControlRequestDto::ArmoryBrowse.into_interface_request(),
            InterfaceControlRequest::ArmoryBrowse { query: None }
        ));
    }

    #[test]
    fn client_control_request_converts_to_interface_boundary_request() {
        let request = ClientControlRequestDto::ContextStatus.into_interface_request();
        assert!(matches!(request, InterfaceControlRequest::ContextStatus));

        let request = ClientControlRequestDto::SetPresentationLevel {
            level: UiPresentationLevel::Om,
        }
        .into_interface_request();
        assert!(matches!(
            request,
            InterfaceControlRequest::SetPresentationLevel {
                level: UiPresentationLevel::Om,
            }
        ));
    }

    #[test]
    fn client_control_envelope_validates_and_dispatches_to_boundary_request() {
        let envelope = ClientEnvelope::new(
            "env-2",
            "replacement-ui",
            ClientEnvelopeDirection::ClientToRuntime,
            ClientEnvelopeKind::ControlRequest,
            serde_json::json!({ "name": "contextStatus" }),
        )
        .with_session("session-1");

        let dispatch = decode_client_control_dispatch(envelope).expect("valid control dispatch");
        assert_eq!(dispatch.envelope_id, "env-2");
        assert_eq!(dispatch.client_id, "replacement-ui");
        assert_eq!(dispatch.session_id.as_deref(), Some("session-1"));
        assert!(matches!(
            dispatch.request,
            InterfaceControlRequest::ContextStatus
        ));
    }

    #[test]
    fn client_control_envelope_rejects_wrong_version_direction_and_kind() {
        let mut envelope = ClientEnvelope::new(
            "env-3",
            "replacement-ui",
            ClientEnvelopeDirection::ClientToRuntime,
            ClientEnvelopeKind::ControlRequest,
            serde_json::json!({ "name": "contextStatus" }),
        );
        envelope.protocol_version = CLIENT_API_VERSION + 1;
        assert_eq!(
            decode_client_control_envelope(envelope),
            Err(ClientEnvelopeError::UnsupportedVersion {
                actual: CLIENT_API_VERSION + 1,
                expected: CLIENT_API_VERSION,
            })
        );

        let envelope = ClientEnvelope::new(
            "env-4",
            "replacement-ui",
            ClientEnvelopeDirection::RuntimeToClient,
            ClientEnvelopeKind::ControlRequest,
            serde_json::json!({ "name": "contextStatus" }),
        );
        assert!(matches!(
            decode_client_control_envelope(envelope),
            Err(ClientEnvelopeError::InvalidDirection { .. })
        ));

        let envelope = ClientEnvelope::new(
            "env-5",
            "replacement-ui",
            ClientEnvelopeDirection::ClientToRuntime,
            ClientEnvelopeKind::SurfaceSubscription,
            serde_json::json!({ "name": "contextStatus" }),
        );
        assert!(matches!(
            decode_client_control_envelope(envelope),
            Err(ClientEnvelopeError::InvalidKind { .. })
        ));
    }

    #[test]
    fn client_surface_subscription_dto_uses_stable_surface_names() {
        let subscription = ClientSurfaceSubscriptionDto::snapshot([
            UiSurfaceKind::Conversation,
            UiSurfaceKind::Dashboard,
        ]);
        let value = serde_json::to_value(subscription).expect("serialize subscription");
        assert_eq!(
            value["surfaces"],
            serde_json::json!(["conversation", "dashboard"])
        );
        assert_eq!(value["includeSnapshot"], true);
        assert!(value["sinceRevision"].is_null());
    }

    #[test]
    fn client_surface_subscription_envelope_validates_direction_and_kind() {
        let envelope = ClientEnvelope::new(
            "env-6",
            "replacement-ui",
            ClientEnvelopeDirection::ClientToRuntime,
            ClientEnvelopeKind::SurfaceSubscription,
            serde_json::json!({
                "surfaces": ["conversation", "footer"],
                "sinceRevision": 42,
                "includeSnapshot": false,
            }),
        );

        let subscription = decode_client_surface_subscription_envelope(envelope)
            .expect("valid surface subscription");
        assert_eq!(
            subscription.surfaces,
            vec![UiSurfaceKind::Conversation, UiSurfaceKind::Footer]
        );
        assert_eq!(subscription.since_revision, Some(42));
        assert!(!subscription.include_snapshot);
    }

    #[test]
    fn client_surface_update_envelope_wraps_runtime_to_client_payload() {
        let update = ClientSurfaceUpdateDto {
            surface: UiSurfaceKind::Presentation,
            revision: 7,
            payload: serde_json::json!({ "level": "active" }),
        };

        let envelope =
            encode_client_surface_update_envelope("env-7", "replacement-ui", "session-1", &update)
                .expect("encode surface update");
        let value = serde_json::to_value(envelope).expect("serialize surface update envelope");

        assert_eq!(value["direction"], "runtimeToClient");
        assert_eq!(value["kind"], "surfaceUpdate");
        assert_eq!(value["payload"]["surface"], "presentation");
        assert_eq!(value["payload"]["revision"], 7);
        assert_eq!(value["payload"]["payload"]["level"], "active");
    }

    #[test]
    fn fixture_control_request_context_status_dispatches_to_boundary() {
        let envelope: ClientEnvelope = serde_json::from_value(fixture(include_str!(
            "../../tests/fixtures/client_api_v1/control_request_context_status.json"
        )))
        .expect("deserialize control request fixture");

        let request =
            client_control_envelope_to_interface_request(envelope).expect("valid control request");
        assert!(matches!(request, InterfaceControlRequest::ContextStatus));
    }

    #[test]
    fn fixture_surface_subscription_snapshot_decodes_stable_names() {
        let envelope: ClientEnvelope = serde_json::from_value(fixture(include_str!(
            "../../tests/fixtures/client_api_v1/surface_subscription_snapshot.json"
        )))
        .expect("deserialize subscription fixture");

        let subscription =
            decode_client_surface_subscription_envelope(envelope).expect("valid subscription");
        assert_eq!(
            subscription.surfaces,
            vec![
                UiSurfaceKind::Conversation,
                UiSurfaceKind::Dashboard,
                UiSurfaceKind::Footer,
            ]
        );
        assert_eq!(subscription.since_revision, None);
        assert!(subscription.include_snapshot);
    }

    #[test]
    fn fixture_surface_update_presentation_matches_encoder() {
        let expected = fixture(include_str!(
            "../../tests/fixtures/client_api_v1/surface_update_presentation.json"
        ));
        let update = ClientSurfaceUpdateDto {
            surface: UiSurfaceKind::Presentation,
            revision: 7,
            payload: serde_json::json!({ "level": UiPresentationLevel::Active.name() }),
        };

        let envelope = encode_client_surface_update_envelope(
            "env-surface-1",
            "replacement-ui",
            "session-1",
            &update,
        )
        .expect("encode surface update");
        let actual = serde_json::to_value(envelope).expect("serialize surface update envelope");

        assert_eq!(actual, expected);
    }
}
