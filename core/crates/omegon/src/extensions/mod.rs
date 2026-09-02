//! Omegon product adapters for extension spawning and process management.
//!
//! The dependency-clean `omegon-native-extension-host` crate owns manifests,
//! JSON-RPC transport, child processes, handshake, replacement, and shutdown.
//! This module retains application policy and maps the shared host into Omegon
//! features, admission, state, widgets, voice/vox, and host actions.
//! Stateful widgets stream updates via separate TCP connection.
//!
//! # Secret delivery
//!
//! Extension subprocesses are spawned with `env_clear()` — no secret inheritance
//! from the parent process environment. Declared secrets are delivered via the
//! `bootstrap_secrets` RPC method immediately after the `get_tools` handshake.
//! This prevents plain-text secrets from appearing in `/proc/<pid>/environ`,
//! `ps` output, crash dumps, or child processes of the extension.

use anyhow::{Context, Result, anyhow};
use omegon_traits::{ContentBlock, Feature, ToolDefinition, ToolResult};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{Mutex, broadcast, mpsc};
use tokio_util::sync::CancellationToken;

pub(crate) mod approval;
pub mod config_store;
#[cfg(test)]
mod conformance_tests;
pub(crate) mod host_actions;
pub mod manifest {
    pub use omegon_native_extension_host::*;
}
pub mod mind;
pub mod sdk_compat {
    pub use omegon_native_extension_host::{
        MIN_COMPATIBLE_SDK_CONTRACT_VERSION, SUPPORTED_SDK_CONTRACT_VERSION,
        SdkCompatibilityDiagnostic, SdkCompatibilityStatus, classify_initialize_metadata,
        classify_sdk_version,
    };
}
pub mod state;
mod tool_result;
pub mod voice_bridge;
pub mod vox_bridge;
pub mod widgets;
pub use manifest::{
    ConnectionMode, ExtensionManifest, McpConfig, McpTransport, RuntimeConfig, WidgetConfig,
};
pub use mind::{ExtensionMind, MindStats};
pub use omegon_native_extension_host::{
    ExtensionNotification, ExtensionProcessHealth, ExtensionProcessState, ExtensionSupervisor,
    shutdown_supervisors,
};
pub use sdk_compat::SdkCompatibilityDiagnostic;
pub use state::{ExtensionState, StabilityMetrics};
pub use widgets::{ExtensionTabWidget, WidgetDeclaration, WidgetEvent};

const EXTENSION_TOOL_RPC_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);
const EXTENSION_POLL_RPC_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

fn host_rpc_response_for_extension_request(
    manifest: &ExtensionManifest,
    extension_name: &str,
    request: &omegon_extension::RpcRequest,
) -> Option<Value> {
    match request.method.as_str() {
        "actions/execute" => {
            let action = request.params.get("action").cloned().unwrap_or(Value::Null);
            let outcome = host_actions::process_native_extension_action_execute(
                action,
                manifest,
                extension_name,
            );
            let result = serde_json::to_value(outcome).unwrap_or_else(|err| {
                json!({
                    "action_id": "<serialization-error>",
                    "status": "invalid",
                    "error": {
                        "code": "serialization_error",
                        "message": err.to_string()
                    }
                })
            });
            Some(json!({
                "jsonrpc": "2.0",
                "id": request.id.clone(),
                "result": result
            }))
        }
        _ => Some(json!({
            "jsonrpc": "2.0",
            "id": request.id.clone(),
            "error": {
                "code": -32601,
                "message": format!("unknown host request method '{}'", request.method)
            }
        })),
    }
}

struct OmegonHostRequestHandler {
    manifest: ExtensionManifest,
    extension_name: String,
}

impl omegon_native_extension_host::HostRequestHandler for OmegonHostRequestHandler {
    fn handle(&self, request: &omegon_extension::RpcRequest) -> Option<Value> {
        host_rpc_response_for_extension_request(&self.manifest, &self.extension_name, request)
    }
}

struct OmegonReadinessValidator;

impl omegon_native_extension_host::ReadinessValidator for OmegonReadinessValidator {
    fn validate(&self, method: &str, response: &Value) -> Result<()> {
        if method != omegon_codescan_contracts::CODESCAN_STATUS_METHOD {
            return Ok(());
        }
        let status =
            serde_json::from_value::<omegon_codescan_contracts::CodescanStatusV1>(response.clone())
                .context("extension returned invalid codescan status")?;
        if status.protocol_version != omegon_codescan_contracts::CODESCAN_PROTOCOL_VERSION
            || status.service != omegon_codescan_contracts::CODESCAN_SERVICE_ID
            || !status.ready
        {
            anyhow::bail!("extension returned incompatible codescan status");
        }
        Ok(())
    }
}

#[derive(Clone)]
struct ExtensionRuntimeContext {
    name: String,
    contribution_generation_id: omegon_traits::RuntimeContributionGenerationId,
    inventory: crate::contribution_lifecycle::DynamicContributionInventory,
    contribution_id: omegon_traits::RuntimeContributionId,
    source_digest: String,
    state_dir: PathBuf,
    manifest: ExtensionManifest,
    _snapshot: Option<Arc<crate::contribution_loading::ContributionSnapshot>>,
    state_binding: Option<ExtensionStateBinding>,
    restart: Arc<Mutex<crate::contribution_lifecycle::RestartController>>,
}

struct ExtensionSource {
    ext_dir: PathBuf,
    state_dir: PathBuf,
    snapshot: Option<Arc<crate::contribution_loading::ContributionSnapshot>>,
    state_binding: Option<ExtensionStateBinding>,
    admission: crate::dynamic_admission::DynamicAdmissionPermit,
    inventory: crate::contribution_lifecycle::DynamicContributionInventory,
    project_root: Option<PathBuf>,
}

struct ExtensionGenerationAdmission {
    permit: crate::dynamic_admission::DynamicAdmissionPermit,
    inventory: crate::contribution_lifecycle::DynamicContributionInventory,
}

#[derive(Clone)]
struct ExtensionStateBinding {
    home: PathBuf,
    raw_name: Vec<u8>,
    source_identity: omegon_maintenance_contracts::PathIdentityV1,
}

/// Wrapper Feature for any extension (native or OCI).
/// Manages RPC communication via stdin/stdout, agnostic to runtime type.
#[derive(Clone)]
pub struct ExtensionFeature {
    runtime: ExtensionRuntimeContext,
    tools: Vec<ToolDefinition>,
    supervisor: Arc<ExtensionSupervisor>,
    widgets: Vec<WidgetDeclaration>,
    widget_tx: broadcast::Sender<WidgetEvent>,
    state: Arc<Mutex<ExtensionState>>,
}

impl ExtensionFeature {
    /// Create a new extension feature from already-handshaked process handles.
    fn new(
        runtime: ExtensionRuntimeContext,
        tools: Vec<ToolDefinition>,
        widgets: Vec<WidgetDeclaration>,
        supervisor: Arc<ExtensionSupervisor>,
        state: ExtensionState,
    ) -> (Self, broadcast::Receiver<WidgetEvent>) {
        let (widget_tx, widget_rx) = broadcast::channel::<WidgetEvent>(100);
        (
            Self {
                runtime,
                tools,
                supervisor,
                widgets,
                widget_tx,
                state: Arc::new(Mutex::new(state)),
            },
            widget_rx,
        )
    }

    /// Send a JSON-RPC request and receive the response.
    async fn rpc_call(&self, method: &str, params: Value) -> Result<Value> {
        self.rpc_call_with_cancel(
            method,
            params,
            CancellationToken::new(),
            Some(EXTENSION_TOOL_RPC_TIMEOUT),
        )
        .await
    }

    async fn rpc_call_with_cancel(
        &self,
        method: &str,
        params: Value,
        cancel: CancellationToken,
        idle_timeout: Option<std::time::Duration>,
    ) -> Result<Value> {
        let _generation_guard = self
            .runtime
            .inventory
            .begin_call(&self.runtime.contribution_id, &self.runtime.source_digest)?;
        self.supervisor
            .rpc_call_with_cancel(
                method,
                params,
                cancel,
                idle_timeout,
                omegon_native_extension_host::RpcRequestPolicy::HandleHostRequests,
                None,
            )
            .await
    }

    async fn extension_tool_result_with_context(
        &self,
        output: Value,
        call_id: &str,
        context: &omegon_traits::ToolExecutionContext,
    ) -> ToolResult {
        let mut envelope = tool_result::parse_extension_tool_envelope(output);
        if !envelope.host_actions.is_empty() {
            let outcomes = host_actions::process_declarative_host_actions_with_context(
                envelope.host_actions,
                &self.runtime.manifest,
                &self.runtime.name,
                call_id,
                context,
            )
            .await;
            envelope.host_actions = Vec::new();
            envelope.host_action_outcomes.extend(outcomes);
        }
        envelope.into_tool_result()
    }

    async fn extension_tool_result(&self, output: Value, call_id: &str) -> ToolResult {
        self.extension_tool_result_with_context(
            output,
            call_id,
            &omegon_traits::ToolExecutionContext::default(),
        )
        .await
    }

    async fn respawn_after_transport_error(&self, cause: &anyhow::Error) -> Result<()> {
        let decision = self.runtime.restart.lock().await.record_failure();
        let delay = match decision {
            crate::contribution_lifecycle::RestartDecision::RetryAfter(delay) => delay,
            crate::contribution_lifecycle::RestartDecision::Quarantined => {
                self.supervisor.mark_unavailable(format!(
                    "transport failed and restart budget was exhausted: {cause}"
                ));
                return Err(anyhow!(
                    "extension '{}' entered quarantine after exhausting its restart budget: {cause}",
                    self.runtime.name
                ));
            }
        };
        tokio::time::sleep(delay).await;
        let pid = self.supervisor.replace().await.map_err(|error| {
            anyhow!(
                "extension '{}' transport failed ({cause}); respawn failed: {error}",
                self.runtime.name
            )
        })?;
        tracing::warn!(
            extension = %self.runtime.name,
            pid,
            cause = %cause,
            "respawned extension after transport failure"
        );
        Ok(())
    }

    /// Get widgets declared by this extension.
    pub fn widgets(&self) -> &[WidgetDeclaration] {
        &self.widgets
    }

    /// Get extension state.
    pub async fn state(&self) -> ExtensionState {
        self.state.lock().await.clone()
    }

    /// Record an error in the extension state and persist it.
    pub async fn record_error(&self, error: String) {
        let mut state = self.state.lock().await;
        state.record_error(error);
        if let Some(binding) = &self.runtime.state_binding {
            let content = match toml::to_string_pretty(&*state) {
                Ok(content) => content,
                Err(error) => {
                    tracing::warn!(extension = %self.runtime.name, %error, "could not serialize extension state");
                    return;
                }
            };
            let mutation =
                crate::contribution_loading::GuardedContributionMutationDirectory::open_existing(
                    &binding.home,
                    &[b"extensions"],
                    &binding.home,
                    omegon_maintenance_contracts::ContributionKind::Extension,
                    "user",
                );
            match mutation {
                Ok(Some(mutation)) => {
                    if let Err(error) = mutation.write_file_in_directory(
                        &binding.raw_name,
                        b".omegon",
                        b"state.toml",
                        content.as_bytes(),
                        &binding.source_identity,
                    ) {
                        tracing::warn!(extension = %self.runtime.name, %error, "could not persist admitted extension state");
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(extension = %self.runtime.name, %error, "could not lock admitted extension state");
                }
            }
        } else {
            let _ = state.save(&self.runtime.state_dir);
        }
    }

    /// Broadcast a widget event (for internal use).
    pub fn send_widget_event(&self, event: WidgetEvent) -> Result<()> {
        self.widget_tx
            .send(event)
            .map_err(|e| anyhow!("widget event broadcast failed: {}", e))?;
        Ok(())
    }

    /// Subscribe to widget events.
    pub fn widget_events(&self) -> broadcast::Receiver<WidgetEvent> {
        self.widget_tx.subscribe()
    }

    /// Create a polling handle for calling RPC methods from outside the EventBus.
    /// Used by the daemon's vox event bridge to poll for inbound messages.
    pub fn polling_handle(&self) -> ExtensionPollingHandle {
        ExtensionPollingHandle {
            supervisor: self.supervisor.clone(),
            name: self.runtime.name.clone(),
            source_digest: self.supervisor.source_digest().to_string(),
            inventory: self.runtime.inventory.clone(),
            contribution_id: self.runtime.contribution_id.clone(),
        }
    }
}

/// Shareable handle for calling RPC methods on an extension subprocess.
/// Clones the Arc'd handles from ExtensionFeature so daemon background tasks
/// can poll the extension without going through the EventBus/agent turn.
#[derive(Clone)]
pub struct ExtensionPollingHandle {
    supervisor: Arc<ExtensionSupervisor>,
    name: String,
    source_digest: String,
    inventory: crate::contribution_lifecycle::DynamicContributionInventory,
    contribution_id: omegon_traits::RuntimeContributionId,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct ExtensionProcessProvenance {
    pub(crate) extension: String,
    pub(crate) source_digest: String,
    pub(crate) pid: Option<u32>,
}

impl std::fmt::Debug for ExtensionPollingHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExtensionPollingHandle")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
fn cancellation_notification(method: &str, request_id: u64) -> Value {
    let params = if method == omegon_codescan_contracts::CODESCAN_RPC_METHOD {
        json!({
            "protocol_version": omegon_codescan_contracts::CODESCAN_PROTOCOL_VERSION,
            "request_id": request_id,
        })
    } else {
        json!({"request_id": request_id})
    };
    json!({
        "jsonrpc": "2.0",
        "method": "notifications/cancelled",
        "params": params,
    })
}

impl ExtensionPollingHandle {
    /// Name of the extension this handle is connected to.
    pub fn extension_name(&self) -> &str {
        &self.name
    }

    pub async fn pump_notifications_for(&self, idle_timeout: std::time::Duration) -> Result<()> {
        let _generation_guard = self
            .inventory
            .begin_call(&self.contribution_id, &self.source_digest)?;
        self.supervisor.pump_notifications_for(idle_timeout).await
    }

    pub(crate) fn process_provenance(&self) -> ExtensionProcessProvenance {
        ExtensionProcessProvenance {
            extension: self.name.clone(),
            source_digest: self.source_digest.clone(),
            pid: self.supervisor.health().pid,
        }
    }

    /// Send a JSON-RPC request and receive the response.
    pub async fn rpc_call(&self, method: &str, params: Value) -> Result<Value> {
        self.rpc_call_with_cancel(
            method,
            params,
            CancellationToken::new(),
            Some(EXTENSION_TOOL_RPC_TIMEOUT),
        )
        .await
    }

    pub async fn rpc_call_with_cancel(
        &self,
        method: &str,
        params: Value,
        cancel: CancellationToken,
        idle_timeout: Option<std::time::Duration>,
    ) -> Result<Value> {
        let _generation_guard = self
            .inventory
            .begin_call(&self.contribution_id, &self.source_digest)?;
        let cancellation_params =
            (method == omegon_codescan_contracts::CODESCAN_RPC_METHOD).then(|| {
                json!({
                    "protocol_version": omegon_codescan_contracts::CODESCAN_PROTOCOL_VERSION,
                })
            });
        self.supervisor
            .rpc_call_with_cancel(
                method,
                params,
                cancel,
                idle_timeout,
                omegon_native_extension_host::RpcRequestPolicy::RejectHostRequests,
                cancellation_params,
            )
            .await
    }
}

fn extension_tool_surfaces(tool_name: &str) -> Option<Vec<omegon_traits::RuntimeSurface>> {
    match tool_name {
        "voice_session_stop" => Some(vec![
            omegon_traits::RuntimeSurface::Model,
            omegon_traits::RuntimeSurface::Tui,
        ]),
        "vox_route" => Some(vec![
            omegon_traits::RuntimeSurface::Model,
            omegon_traits::RuntimeSurface::Daemon,
        ]),
        _ => None,
    }
}

fn extension_tool_principals(tool_name: &str) -> Option<Vec<omegon_traits::RuntimePrincipalClass>> {
    match tool_name {
        "voice_session_stop" | "vox_route" => Some(vec![
            omegon_traits::RuntimePrincipalClass::Model,
            omegon_traits::RuntimePrincipalClass::Service,
        ]),
        _ => None,
    }
}

pub(crate) fn extension_rpc_invocation_name(extension_name: &str) -> String {
    format!("extension_rpc:{extension_name}")
}

#[async_trait::async_trait]
impl Feature for ExtensionFeature {
    fn name(&self) -> &str {
        &self.runtime.name
    }

    fn runtime_contribution_generation_id(
        &self,
    ) -> Option<omegon_traits::RuntimeContributionGenerationId> {
        Some(self.runtime.contribution_generation_id.clone())
    }

    fn tool_provenance(&self) -> omegon_traits::ToolProvenance {
        omegon_traits::ToolProvenance::Extension {
            name: self.runtime.name.clone(),
        }
    }

    fn runtime_lifecycle_policy(&self) -> Option<omegon_traits::RuntimeLifecyclePolicy> {
        Some(omegon_traits::RuntimeLifecyclePolicy {
            requirement: omegon_traits::RuntimeLifecycleRequirement::Optional,
            failure_disposition: omegon_traits::RuntimeFailureDisposition::Quarantine,
            readiness_timeout_ms: self.runtime.manifest.startup.timeout_ms.max(1),
            heartbeat_timeout_ms: None,
            restart_limit: 3,
        })
    }

    fn runtime_transition_policy(
        &self,
    ) -> Option<omegon_traits::RuntimeCompositionTransitionPolicy> {
        let strict_native =
            cfg!(unix) && matches!(&self.runtime.manifest.runtime, RuntimeConfig::Native { .. });
        Some(omegon_traits::RuntimeCompositionTransitionPolicy {
            activation_boundary: omegon_traits::RuntimeActivationBoundary::Boot,
            cleanup: if strict_native {
                omegon_traits::RuntimeCleanupRequirement::Strict
            } else {
                omegon_traits::RuntimeCleanupRequirement::BestEffort
            },
            cleanup_timeout_ms: 500,
        })
    }

    fn tools(&self) -> Vec<ToolDefinition> {
        self.tools.clone()
    }

    fn runtime_tool_surfaces(&self, tool_name: &str) -> Option<Vec<omegon_traits::RuntimeSurface>> {
        extension_tool_surfaces(tool_name)
    }

    fn runtime_tool_principals(
        &self,
        tool_name: &str,
    ) -> Option<Vec<omegon_traits::RuntimePrincipalClass>> {
        extension_tool_principals(tool_name)
    }

    fn runtime_acp_invocations(&self) -> Vec<omegon_traits::RuntimeAcpInvocationDefinition> {
        vec![omegon_traits::RuntimeAcpInvocationDefinition {
            name: extension_rpc_invocation_name(&self.runtime.name),
        }]
    }

    async fn execute_acp_invocation(
        &self,
        name: &str,
        args: Value,
        cancel: CancellationToken,
    ) -> Result<Value> {
        if name != extension_rpc_invocation_name(&self.runtime.name) {
            anyhow::bail!(
                "extension '{}' does not own ACP route '{name}'",
                self.runtime.name
            );
        }
        let method = args
            .get("method")
            .and_then(Value::as_str)
            .filter(|method| !method.trim().is_empty())
            .ok_or_else(|| anyhow!("invalid_request: 'method' field must not be empty"))?;
        let params = args.get("params").cloned().unwrap_or_else(|| json!({}));
        self.rpc_call_with_cancel(method, params, cancel, Some(EXTENSION_TOOL_RPC_TIMEOUT))
            .await
            .map_err(|error| {
                if is_extension_transport_error(&error) {
                    crate::invocation_service::UnknownCompletionError {
                        reason: format!(
                            "extension '{}' ACP method '{}' completion is unknown: {error}",
                            self.runtime.name, method
                        ),
                    }
                    .into()
                } else {
                    error
                }
            })
    }

    async fn execute(
        &self,
        tool_name: &str,
        call_id: &str,
        args: Value,
        cancel: CancellationToken,
    ) -> Result<ToolResult> {
        match self
            .rpc_call_with_cancel(
                "execute_tool",
                json!({ "name": tool_name, "args": args.clone() }),
                cancel.clone(),
                Some(EXTENSION_TOOL_RPC_TIMEOUT),
            )
            .await
        {
            Ok(output) => Ok(self.extension_tool_result(output, call_id).await),
            Err(e) if is_extension_transport_error(&e) => {
                self.record_error(format!("transport failure: {e}")).await;
                self.respawn_after_transport_error(&e).await?;
                let output = self
                    .rpc_call_with_cancel(
                        "execute_tool",
                        json!({ "name": tool_name, "args": args }),
                        cancel,
                        Some(EXTENSION_TOOL_RPC_TIMEOUT),
                    )
                    .await
                    .map_err(|retry_err| {
                        anyhow!(
                            "extension '{}' reconnected after transport failure, but retrying '{}' failed: {retry_err}",
                            self.runtime.name,
                            tool_name
                        )
                    })?;
                let mut result = self.extension_tool_result(output, call_id).await;
                result.details = match result.details {
                    Value::Object(mut details) => {
                        details.insert("extension_reconnected".to_string(), Value::Bool(true));
                        Value::Object(details)
                    }
                    other => json!({"extension_reconnected": true, "extension_details": other}),
                };
                Ok(result)
            }
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("MethodNotFound") {
                    Ok(ToolResult {
                        content: vec![ContentBlock::Text {
                            text: format!(
                                "Extension '{}' does not support tool execution. \
                                 The tool '{}' was advertised but cannot be called.",
                                self.runtime.name, tool_name
                            ),
                        }],
                        details: json!({"is_error": true}),
                    })
                } else {
                    Err(e)
                }
            }
        }
    }

    async fn execute_with_context(
        &self,
        tool_name: &str,
        call_id: &str,
        args: Value,
        cancel: CancellationToken,
        _sink: omegon_traits::ToolProgressSink,
        context: omegon_traits::ToolExecutionContext,
    ) -> Result<ToolResult> {
        let invocation = context.invocation.clone();
        match self
            .rpc_call_with_cancel(
                "execute_tool",
                json!({
                    "name": tool_name,
                    "args": args.clone(),
                    "call_id": call_id,
                    "invocation": invocation,
                }),
                cancel.clone(),
                Some(EXTENSION_TOOL_RPC_TIMEOUT),
            )
            .await
        {
            Ok(output) => Ok(self
                .extension_tool_result_with_context(output, call_id, &context)
                .await),
            Err(e) if is_extension_transport_error(&e) => {
                self.record_error(format!("transport failure: {e}")).await;
                if invocation.is_some() {
                    return Err(crate::invocation_service::UnknownCompletionError {
                        reason: format!(
                            "extension transport failed after invocation acknowledgement: {e}"
                        ),
                    }
                    .into());
                }
                self.respawn_after_transport_error(&e).await?;
                let output = self
                    .rpc_call_with_cancel(
                        "execute_tool",
                        json!({
                            "name": tool_name,
                            "args": args,
                            "call_id": call_id,
                            "invocation": context.invocation,
                        }),
                        cancel,
                        Some(EXTENSION_TOOL_RPC_TIMEOUT),
                    )
                    .await
                    .map_err(|retry_err| {
                        anyhow!(
                            "extension '{}' reconnected after transport failure, but retrying '{}' failed: {retry_err}",
                            self.runtime.name,
                            tool_name
                        )
                    })?;
                let mut result = self
                    .extension_tool_result_with_context(output, call_id, &context)
                    .await;
                result.details = match result.details {
                    Value::Object(mut details) => {
                        details.insert("extension_reconnected".to_string(), Value::Bool(true));
                        Value::Object(details)
                    }
                    other => json!({"extension_reconnected": true, "extension_details": other}),
                };
                Ok(result)
            }
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("MethodNotFound") {
                    Ok(ToolResult {
                        content: vec![ContentBlock::Text {
                            text: format!(
                                "Extension '{}' does not support tool execution. \
                                 The tool '{}' was advertised but cannot be called.",
                                self.runtime.name, tool_name
                            ),
                        }],
                        details: json!({"is_error": true}),
                    })
                } else {
                    Err(e)
                }
            }
        }
    }

    async fn execute_with_invocation_control(
        &self,
        tool_name: &str,
        call_id: &str,
        args: Value,
        cancel: CancellationToken,
        sink: omegon_traits::ToolProgressSink,
        context: omegon_traits::ToolExecutionContext,
        control: omegon_traits::InvocationControl,
    ) -> Result<ToolResult> {
        control.acknowledge().map_err(anyhow::Error::msg)?;
        self.execute_with_context(tool_name, call_id, args, cancel, sink, context)
            .await
    }
}

/// Result of spawning an extension: feature + widgets
pub struct SpawnedExtension {
    /// Canonical process owner used for deterministic host shutdown.
    pub supervisor: Arc<ExtensionSupervisor>,
    pub feature: Box<dyn Feature>,
    pub widgets: Vec<ExtensionTabWidget>,
    pub widget_rx: broadcast::Receiver<WidgetEvent>,
    /// Optional metadata returned by the extension initialize handshake.
    pub metadata: Option<Value>,
    /// SDK contract compatibility classification derived from initialize metadata.
    pub sdk_compatibility: SdkCompatibilityDiagnostic,
    /// Generic RPC handle for ACP/runtime control-plane calls.
    pub rpc_polling_handle: ExtensionPollingHandle,
    /// Polling handle for extensions that provide `vox_route` (event bridge).
    pub vox_polling_handle: Option<ExtensionPollingHandle>,
    /// Idle notification pump for voice-capable extensions.
    pub voice_polling_handle: Option<ExtensionPollingHandle>,
    /// Push notification receiver for voice-capable extensions.
    pub voice_notification_rx: Option<mpsc::UnboundedReceiver<ExtensionNotification>>,
}

pub(crate) fn dynamic_preflight(
    manifest: &ExtensionManifest,
    source: &Path,
) -> Result<omegon_traits::RuntimeDynamicContributionPreflight> {
    let id =
        omegon_traits::RuntimeContributionId::new(format!("extension:{}", manifest.extension.name))
            .map_err(|error| anyhow!(error))?;
    let (source_kind, requested_confinement) = match manifest.runtime {
        RuntimeConfig::Native { .. } => (
            omegon_traits::RuntimeDynamicSourceKind::NativeExtension,
            omegon_traits::RuntimeConfinementRequest::HostProcess,
        ),
        RuntimeConfig::Oci { .. } => (
            omegon_traits::RuntimeDynamicSourceKind::OciExtension,
            omegon_traits::RuntimeConfinementRequest::Oci,
        ),
    };
    Ok(omegon_traits::RuntimeDynamicContributionPreflight {
        schema_version: omegon_traits::RUNTIME_DYNAMIC_PREFLIGHT_SCHEMA_VERSION,
        id,
        source_digest: crate::dynamic_admission::digest_path(source)?,
        source_kind,
        protocol: omegon_traits::RuntimeProtocolRange::new(1, 1).map_err(|error| anyhow!(error))?,
        minimum_dependencies: Vec::new(),
        requested_trust: omegon_traits::RuntimeTrustRequest::OperatorManaged,
        requested_confinement,
        probe: omegon_traits::RuntimeProbeRequirements {
            operations: vec![
                omegon_traits::RuntimeProbeOperation::Initialize,
                omegon_traits::RuntimeProbeOperation::DiscoverCapabilities,
            ],
            timeout_ms: manifest.startup.timeout_ms.max(1),
            requested_effects: vec![
                omegon_traits::RuntimeEffect::FilesystemRead,
                omegon_traits::RuntimeEffect::ProcessSpawn,
                omegon_traits::RuntimeEffect::NetworkAccess,
                omegon_traits::RuntimeEffect::SecretDelivery,
            ],
        },
    })
}

fn extension_generation_id(
    name: &str,
    source_digest: &str,
) -> Result<omegon_traits::RuntimeContributionGenerationId> {
    omegon_traits::RuntimeContributionGenerationId::new(format!(
        "contribution:{name}-sha256-{source_digest}"
    ))
    .map_err(anyhow::Error::msg)
}

/// Spawn an extension from its manifest directory.
///
/// `resolved_secrets` contains pre-resolved (name, value) pairs for all secrets
/// declared in `manifest.secrets`. These are delivered via `bootstrap_secrets`
/// RPC — never via subprocess environment variables.
#[cfg(test)]
pub async fn spawn_from_manifest(
    ext_dir: &Path,
    resolved_secrets: &[(String, String)],
) -> Result<SpawnedExtension> {
    let manifest = ExtensionManifest::from_extension_dir(ext_dir)?;
    let preflight = dynamic_preflight(&manifest, ext_dir)?;
    let inventory = crate::contribution_lifecycle::DynamicContributionInventory::default();
    let candidate = inventory.discover(preflight.clone())?;
    let admission = crate::dynamic_admission::DynamicAdmissionPermit::for_test(preflight);
    let spawned = spawn_from_manifest_source(
        ext_dir,
        ext_dir,
        None,
        None,
        ExtensionGenerationAdmission {
            permit: admission,
            inventory: inventory.clone(),
        },
        None,
        resolved_secrets,
    )
    .await?;
    inventory.ready(&candidate.preflight.id);
    inventory.stage_ready();
    inventory.publish_staged();
    Ok(spawned)
}

pub(crate) async fn spawn_from_admitted_snapshot(
    snapshot: Arc<crate::contribution_loading::ContributionSnapshot>,
    state_dir: &Path,
    admission: crate::dynamic_admission::DynamicAdmissionPermit,
    inventory: crate::contribution_lifecycle::DynamicContributionInventory,
    project_root: &Path,
    resolved_secrets: &[(String, String)],
) -> Result<SpawnedExtension> {
    let ext_dir = snapshot.path().to_path_buf();
    let state_binding = extension_state_binding(state_dir, &snapshot)?;
    spawn_from_manifest_source(
        &ext_dir,
        state_dir,
        Some(snapshot),
        Some(state_binding),
        ExtensionGenerationAdmission {
            permit: admission,
            inventory,
        },
        Some(project_root.to_path_buf()),
        resolved_secrets,
    )
    .await
}

pub(crate) async fn spawn_from_release_snapshot(
    snapshot: Arc<crate::contribution_loading::ContributionSnapshot>,
    admission: crate::dynamic_admission::DynamicAdmissionPermit,
    inventory: crate::contribution_lifecycle::DynamicContributionInventory,
    project_root: &Path,
    resolved_secrets: &[(String, String)],
) -> Result<SpawnedExtension> {
    let ext_dir = snapshot.path().to_path_buf();
    spawn_from_manifest_source(
        &ext_dir,
        &ext_dir,
        Some(snapshot),
        None,
        ExtensionGenerationAdmission {
            permit: admission,
            inventory,
        },
        Some(project_root.to_path_buf()),
        resolved_secrets,
    )
    .await
}

async fn spawn_from_manifest_source(
    ext_dir: &Path,
    state_dir: &Path,
    snapshot: Option<Arc<crate::contribution_loading::ContributionSnapshot>>,
    state_binding: Option<ExtensionStateBinding>,
    generation_admission: ExtensionGenerationAdmission,
    project_root: Option<PathBuf>,
    resolved_secrets: &[(String, String)],
) -> Result<SpawnedExtension> {
    let source = ExtensionSource {
        ext_dir: ext_dir.to_path_buf(),
        state_dir: state_dir.to_path_buf(),
        snapshot,
        state_binding,
        admission: generation_admission.permit,
        inventory: generation_admission.inventory,
        project_root,
    };
    let manifest = ExtensionManifest::from_extension_dir(&source.ext_dir)?;
    if source.snapshot.is_some() {
        validate_admitted_runtime_paths(&manifest)?;
    }
    source.admission.validate_source_path(&source.ext_dir)?;

    // Enforce required secrets before spending any resources on spawning.
    // Check against the pre-resolved pairs rather than process env.
    let missing: Vec<&str> = manifest
        .secrets
        .required
        .iter()
        .filter(|name| !resolved_secrets.iter().any(|(k, _)| k == *name))
        .map(|s| s.as_str())
        .collect();
    if !missing.is_empty() {
        return Err(anyhow!(
            "extension '{}' requires secrets that could not be resolved: {}. \
             Configure them with: omegon secret set {}",
            manifest.extension.name,
            missing.join(", "),
            missing[0],
        ));
    }

    // Log optional secrets that are absent — extension will degrade gracefully.
    for name in &manifest.secrets.optional {
        if !resolved_secrets.iter().any(|(k, _)| k == name) {
            tracing::debug!(
                extension = %manifest.extension.name,
                secret = %name,
                "optional secret absent — extension may have reduced functionality"
            );
        }
    }

    let substrate = crate::execution_substrate::detect();
    if substrate.kind != omegon_traits::ExecutionSubstrateKind::HostNative
        && matches!(&manifest.runtime, RuntimeConfig::Native { .. })
    {
        return Err(anyhow!(
            "native extension '{}' is disabled under {:?} execution substrate; use an OCI/image-bundled extension build or run Omegon host-native",
            manifest.extension.name,
            substrate.kind
        ));
    }

    let state = ExtensionState::load(ext_dir)?;
    let widgets: Vec<WidgetDeclaration> = manifest
        .widgets
        .iter()
        .map(|(id, config)| WidgetDeclaration {
            id: id.clone(),
            label: config.label.clone(),
            kind: config.kind.clone(),
            renderer: config.renderer.clone(),
            description: config.description.clone(),
        })
        .collect();

    match manifest.runtime {
        RuntimeConfig::Native { .. } => {
            let binary = manifest.native_binary_path(&source.ext_dir)?;
            spawn_native(&manifest, source, binary, widgets, state, resolved_secrets).await
        }
        RuntimeConfig::Oci { .. } => {
            let image = manifest.oci_image()?;
            spawn_container(&manifest, source, &image, widgets, state, resolved_secrets).await
        }
    }
}

#[cfg(unix)]
fn extension_state_binding(
    state_dir: &Path,
    snapshot: &crate::contribution_loading::ContributionSnapshot,
) -> Result<ExtensionStateBinding> {
    use std::os::unix::ffi::OsStrExt;

    let raw_name = state_dir
        .file_name()
        .ok_or_else(|| anyhow!("extension state path has no contribution basename"))?
        .as_bytes()
        .to_vec();
    let extensions = state_dir
        .parent()
        .ok_or_else(|| anyhow!("extension state path has no extensions root"))?;
    if extensions.file_name().and_then(|name| name.to_str()) != Some("extensions") {
        anyhow::bail!("extension state path is outside the canonical extensions root");
    }
    let home = extensions
        .parent()
        .ok_or_else(|| anyhow!("extension state path has no Omegon home"))?;
    Ok(ExtensionStateBinding {
        home: home.to_path_buf(),
        raw_name,
        source_identity: snapshot.source_identity().clone(),
    })
}

#[cfg(not(unix))]
fn extension_state_binding(
    _state_dir: &Path,
    _snapshot: &crate::contribution_loading::ContributionSnapshot,
) -> Result<ExtensionStateBinding> {
    anyhow::bail!("guarded extension state requires Unix")
}

fn validate_admitted_runtime_paths(manifest: &ExtensionManifest) -> Result<()> {
    let RuntimeConfig::Native { binary, .. } = &manifest.runtime else {
        return Ok(());
    };
    let path = Path::new(binary);
    if path.as_os_str().is_empty()
        || !path
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
    {
        anyhow::bail!("native extension binary must be a relative path within its admitted bundle");
    }
    Ok(())
}

pub(crate) fn metadata_with_sdk_compatibility(
    metadata: Option<Value>,
    diagnostic: &SdkCompatibilityDiagnostic,
) -> Value {
    let sdk_compatibility = serde_json::to_value(diagnostic)
        .unwrap_or_else(|_| serde_json::json!({"status": "serialization_failed"}));
    let mut metadata = metadata.unwrap_or_else(|| serde_json::json!({}));
    if let Some(obj) = metadata.as_object_mut() {
        obj.insert("sdk_compatibility".to_string(), sdk_compatibility);
        metadata
    } else {
        serde_json::json!({
            "initialize": metadata,
            "sdk_compatibility": sdk_compatibility,
        })
    }
}

#[cfg(test)]
fn normalize_extension_tool_definitions(value: &Value) -> Result<Vec<ToolDefinition>> {
    omegon_native_extension_host::normalize_tool_definitions(value)
}

fn resolved_config(
    manifest: &ExtensionManifest,
    ext_dir: &Path,
) -> Result<serde_json::Map<String, Value>> {
    let mut config = serde_json::Map::new();
    for (name, value) in manifest.runtime.config() {
        config.insert(name.clone(), value.clone());
    }

    let stored = config_store::read_config(ext_dir)?;

    for (name, field) in &manifest.config {
        if let Some(default) = &field.default {
            config.insert(name.clone(), config_value_to_json(field, default));
        } else if field.required && !stored.contains_key(name) {
            return Err(anyhow!(
                "extension '{}' requires config value '{}'. \
                 Configure it with the extension settings UI or ACP config RPC.",
                manifest.extension.name,
                name
            ));
        }
    }

    for (name, value) in stored {
        if let Some(field) = manifest.config.get(&name) {
            config_store::validate_field(field, &value)?;
            config.insert(name, config_value_to_json(field, &value));
        } else {
            config.insert(name, Value::String(value));
        }
    }

    Ok(config)
}

fn is_extension_transport_error(error: &anyhow::Error) -> bool {
    let msg = error.to_string().to_ascii_lowercase();
    msg.contains("broken pipe")
        || msg.contains("connection reset")
        || msg.contains("connection aborted")
        || msg.contains("extension closed connection")
        || msg.contains("closed channel")
        || msg.contains("early eof")
        || msg.contains("unexpected eof")
}

fn config_value_to_json(field: &omegon_extension::ConfigField, value: &str) -> Value {
    use omegon_extension::ConfigFieldType;

    match field.field_type {
        ConfigFieldType::Boolean => Value::Bool(value == "true"),
        ConfigFieldType::Number => value
            .parse::<serde_json::Number>()
            .map(Value::Number)
            .unwrap_or_else(|_| Value::String(value.to_string())),
        ConfigFieldType::String | ConfigFieldType::Enum | ConfigFieldType::Text => {
            Value::String(value.to_string())
        }
    }
}

async fn spawn_native(
    manifest: &ExtensionManifest,
    source: ExtensionSource,
    binary: PathBuf,
    widgets: Vec<WidgetDeclaration>,
    state: ExtensionState,
    resolved_secrets: &[(String, String)],
) -> Result<SpawnedExtension> {
    let spawned =
        launch_supervised_extension(manifest, source, widgets, state, resolved_secrets).await?;
    tracing::info!(
        name = %manifest.extension.name,
        binary = %binary.display(),
        tools = spawned.feature.tools().len(),
        secrets = resolved_secrets.len(),
        "spawned native extension"
    );
    Ok(spawned)
}

async fn spawn_container(
    manifest: &ExtensionManifest,
    source: ExtensionSource,
    image: &str,
    widgets: Vec<WidgetDeclaration>,
    state: ExtensionState,
    resolved_secrets: &[(String, String)],
) -> Result<SpawnedExtension> {
    let spawned =
        launch_supervised_extension(manifest, source, widgets, state, resolved_secrets).await?;
    tracing::info!(
        name = %manifest.extension.name,
        image,
        tools = spawned.feature.tools().len(),
        secrets = resolved_secrets.len(),
        "spawned OCI extension"
    );
    Ok(spawned)
}

async fn launch_supervised_extension(
    manifest: &ExtensionManifest,
    source: ExtensionSource,
    widgets: Vec<WidgetDeclaration>,
    state: ExtensionState,
    resolved_secrets: &[(String, String)],
) -> Result<SpawnedExtension> {
    let config = resolved_config(manifest, &source.ext_dir)?;
    let source_digest = source.admission.source_digest().to_string();
    let contribution_id = source.admission.contribution_id().clone();
    let contribution_generation_id =
        extension_generation_id(&manifest.extension.name, &source_digest)?;
    let notification_pair = if manifest.capabilities.voice {
        let (tx, rx) = mpsc::unbounded_channel();
        (Some(tx), Some(rx))
    } else {
        (None, None)
    };
    let launch = omegon_native_extension_host::LaunchSpec {
        manifest: manifest.clone(),
        extension_dir: source.ext_dir,
        project_root: source.project_root,
        resolved_config: config,
        resolved_secrets: resolved_secrets.to_vec(),
        source_digest: source_digest.clone(),
        notification_tx: notification_pair.0,
        host_request_handler: Some(Arc::new(OmegonHostRequestHandler {
            manifest: manifest.clone(),
            extension_name: manifest.extension.name.clone(),
        })),
        readiness_validator: Some(Arc::new(OmegonReadinessValidator)),
    };
    let (supervisor, handshake) = ExtensionSupervisor::launch(launch).await?;
    let runtime = ExtensionRuntimeContext {
        name: manifest.extension.name.clone(),
        contribution_generation_id,
        inventory: source.inventory,
        contribution_id,
        source_digest,
        state_dir: source.state_dir,
        manifest: manifest.clone(),
        _snapshot: source.snapshot,
        state_binding: source.state_binding,
        restart: Arc::new(Mutex::new(
            crate::contribution_lifecycle::RestartController::new(
                3,
                std::time::Duration::from_millis(100),
                std::time::Duration::from_secs(2),
            ),
        )),
    };
    let (feature, widget_rx) = ExtensionFeature::new(
        runtime,
        handshake.tools.clone(),
        widgets.clone(),
        supervisor.clone(),
        state,
    );
    let vox_polling_handle = handshake
        .tools
        .iter()
        .any(|tool| tool.name == "vox_route")
        .then(|| feature.polling_handle());
    let voice_polling_handle = manifest
        .capabilities
        .voice
        .then(|| feature.polling_handle());
    let mut tab_widgets = Vec::new();
    for widget in widgets {
        let mut tab_widget = ExtensionTabWidget::new(
            widget.id.clone(),
            widget.label,
            widget.renderer,
            widget.kind,
        );
        if let Ok(data) = feature
            .rpc_call(&format!("get_{}", widget.id), json!({}))
            .await
        {
            tab_widget.update(data);
        }
        tab_widgets.push(tab_widget);
    }
    let rpc_polling_handle = feature.polling_handle();
    Ok(SpawnedExtension {
        supervisor,
        feature: Box::new(feature),
        widgets: tab_widgets,
        widget_rx,
        metadata: handshake.metadata,
        sdk_compatibility: handshake.sdk_compatibility,
        rpc_polling_handle,
        vox_polling_handle,
        voice_polling_handle,
        voice_notification_rx: notification_pair.1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use omegon_extension::{ConfigField, ConfigFieldType};
    use std::collections::HashMap;

    #[test]
    fn extension_manifest_paths() {
        // Placeholder for integration tests
    }

    #[test]
    fn contribution_generation_identity_is_bound_to_source_digest() {
        let generation_a = extension_generation_id("fixture", &"a".repeat(64)).unwrap();
        let generation_b = extension_generation_id("fixture", &"b".repeat(64)).unwrap();

        assert_ne!(generation_a, generation_b);
        assert_eq!(
            generation_a.as_str(),
            format!("contribution:fixture-sha256-{}", "a".repeat(64))
        );
    }

    #[test]
    fn codescan_cancellation_uses_the_versioned_wire_contract() {
        assert_eq!(
            cancellation_notification(omegon_codescan_contracts::CODESCAN_RPC_METHOD, 42),
            json!({
                "jsonrpc": "2.0",
                "method": "notifications/cancelled",
                "params": {
                    "protocol_version": omegon_codescan_contracts::CODESCAN_PROTOCOL_VERSION,
                    "request_id": 42,
                },
            })
        );
        assert_eq!(
            cancellation_notification("execute_tool", 42)["params"],
            json!({"request_id": 42})
        );
    }

    #[test]
    fn voice_stop_declares_model_and_tui_service_access() {
        assert_eq!(
            extension_tool_surfaces("voice_session_stop"),
            Some(vec![
                omegon_traits::RuntimeSurface::Model,
                omegon_traits::RuntimeSurface::Tui,
            ])
        );
        assert_eq!(
            extension_tool_principals("voice_session_stop"),
            Some(vec![
                omegon_traits::RuntimePrincipalClass::Model,
                omegon_traits::RuntimePrincipalClass::Service,
            ])
        );
        assert_eq!(extension_tool_surfaces("other_extension_tool"), None);
        assert_eq!(extension_tool_principals("other_extension_tool"), None);
    }

    #[test]
    fn vox_route_declares_model_and_daemon_service_access() {
        assert_eq!(
            extension_tool_surfaces("vox_route"),
            Some(vec![
                omegon_traits::RuntimeSurface::Model,
                omegon_traits::RuntimeSurface::Daemon,
            ])
        );
        assert_eq!(
            extension_tool_principals("vox_route"),
            Some(vec![
                omegon_traits::RuntimePrincipalClass::Model,
                omegon_traits::RuntimePrincipalClass::Service,
            ])
        );
    }

    #[test]
    fn required_secret_check_detects_missing() {
        // Required secret not in resolved_secrets → missing
        let required = ["GITHUB_TOKEN".to_string()];
        let resolved: Vec<(String, String)> = vec![];
        let missing: Vec<&str> = required
            .iter()
            .filter(|name| !resolved.iter().any(|(k, _)| k == *name))
            .map(|s| s.as_str())
            .collect();
        assert_eq!(missing, vec!["GITHUB_TOKEN"]);
    }

    #[test]
    fn required_secret_check_passes_when_present() {
        // Required secret is in resolved_secrets → no missing
        let required = ["GITHUB_TOKEN".to_string()];
        let resolved = [("GITHUB_TOKEN".to_string(), "ghp_test".to_string())];
        let missing: Vec<&str> = required
            .iter()
            .filter(|name| !resolved.iter().any(|(k, _)| k == *name))
            .map(|s| s.as_str())
            .collect();
        assert!(missing.is_empty());
    }

    #[test]
    fn resolved_config_applies_defaults_and_stored_overrides() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = test_manifest(HashMap::from([
            (
                "agent_browser_binary".to_string(),
                config_field(ConfigFieldType::String, Some("agent-browser"), false),
            ),
            (
                "max_output".to_string(),
                config_field(ConfigFieldType::Number, Some("50000"), false),
            ),
        ]));
        config_store::write_config_value(temp.path(), "max_output", "2000").unwrap();

        let config = resolved_config(&manifest, temp.path()).unwrap();

        assert_eq!(
            config.get("agent_browser_binary"),
            Some(&Value::String("agent-browser".to_string()))
        );
        assert_eq!(config.get("max_output"), Some(&Value::Number(2000.into())));
    }

    #[test]
    fn resolved_config_requires_missing_required_values() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = test_manifest(HashMap::from([(
            "required_value".to_string(),
            config_field(ConfigFieldType::String, None, true),
        )]));

        let err = resolved_config(&manifest, temp.path()).unwrap_err();
        assert!(err.to_string().contains("required_value"));
    }

    #[test]
    fn extension_transport_error_detection_covers_stale_handles() {
        assert!(is_extension_transport_error(&anyhow!("broken pipe")));
        assert!(is_extension_transport_error(&anyhow!(
            "extension closed connection"
        )));
        assert!(is_extension_transport_error(&anyhow!("unexpected EOF")));
        assert!(!is_extension_transport_error(&anyhow!(
            "RPC error: MethodNotFound"
        )));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn handshake_rejects_extension_that_refuses_resolved_config() {
        let _env_guard = crate::test_support::env::lock_async().await;
        unsafe {
            std::env::remove_var("OMEGON_RUNTIME_CONTEXT");
            std::env::remove_var("KUBERNETES_SERVICE_HOST");
        }
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let script = temp.path().join("reject-config.sh");
        std::fs::write(
            &script,
            r#"#!/bin/sh
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([^,}}]*\).*/\1/p')
  case "$line" in
    *initialize*) printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"sdk_contract_version\":\"0.25\"}}" ;;
    *get_tools*) printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":[]}" ;;
    *bootstrap_config*) printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":$id,\"error\":{\"code\":-32602,\"message\":\"invalid data_dir\"}}" ;;
  esac
done
"#,
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&script, permissions).unwrap();
        std::fs::write(
            temp.path().join("manifest.toml"),
            format!(
                r#"[extension]
name = "reject-config"
version = "0.1.0"
description = "fixture"
[runtime]
type = "native"
binary = "{}"
[startup]
timeout_ms = 30000
[runtime.config]
data_dir = "relative"
"#,
                script.display()
            ),
        )
        .unwrap();

        let error = match spawn_from_manifest(temp.path(), &[]).await {
            Ok(_) => panic!("rejected bootstrap config must prevent registration"),
            Err(error) => error,
        };
        assert!(
            error
                .to_string()
                .contains("failed to accept bootstrap_config")
        );
        assert!(error.to_string().contains("invalid data_dir"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn admitted_extension_respawns_from_original_snapshot_after_source_changes() {
        let _env_guard = crate::test_support::env::lock_async().await;
        unsafe {
            std::env::remove_var("OMEGON_RUNTIME_CONTEXT");
            std::env::remove_var("KUBERNETES_SERVICE_HOST");
        }
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let extension_dir = temp.path().join("extensions/flaky");
        std::fs::create_dir_all(&extension_dir).unwrap();
        let marker = temp.path().join("first-call-done");
        let script = extension_dir.join("flaky-extension.sh");
        let script_body = r#"#!/bin/sh
marker=__MARKER__
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([^,}}]*\).*/\1/p')
  case "$line" in
    *initialize*)
      printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":$id,\"error\":{\"code\":-32601,\"message\":\"Method not found\"}}"
      ;;
    *get_tools*)
      printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":[{\"name\":\"echo\",\"label\":\"Echo\",\"description\":\"Echo\",\"parameters\":{\"type\":\"object\",\"properties\":{}}}]}"
      ;;
    *execute_tool*)
      printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"ok\":true}}"
      if [ ! -f "$marker" ]; then
        touch "$marker"
        exit 0
      fi
      ;;
  esac
done
"#
        .replace("__MARKER__", &marker.display().to_string());
        std::fs::write(&script, script_body).unwrap();
        let mut perms = std::fs::metadata(&script).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script, perms).unwrap();

        std::fs::write(
            extension_dir.join("manifest.toml"),
            r#"
[extension]
name = "flaky"
version = "0.1.0"
description = "Flaky test extension"

[runtime]
type = "native"
binary = "flaky-extension.sh"
[startup]
timeout_ms = 30000
"#,
        )
        .unwrap();

        let source = std::fs::File::open(&extension_dir).unwrap();
        let snapshot = Arc::new(
            crate::contribution_loading::snapshot_contribution_directory(&source).unwrap(),
        );
        let manifest = ExtensionManifest::from_extension_dir(snapshot.path()).unwrap();
        let preflight = dynamic_preflight(&manifest, snapshot.path()).unwrap();
        let inventory = crate::contribution_lifecycle::DynamicContributionInventory::default();
        let candidate = inventory.discover(preflight.clone()).unwrap();
        let admission = crate::dynamic_admission::DynamicAdmissionPermit::for_test(preflight);
        let spawned = spawn_from_admitted_snapshot(
            snapshot,
            &extension_dir,
            admission,
            inventory.clone(),
            temp.path(),
            &[],
        )
        .await
        .unwrap();
        inventory.ready(&candidate.preflight.id);
        inventory.stage_ready();
        inventory.publish_staged();
        let first = spawned
            .feature
            .execute("echo", "call-1", json!({}), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(first.details["extension_reconnected"], Value::Null);
        std::fs::write(&script, "#!/bin/sh\nexit 91\n").unwrap();

        let second = spawned
            .feature
            .execute("echo", "call-2", json!({}), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(second.details["extension_reconnected"], true);
        assert!(
            ExtensionState::load(&extension_dir)
                .unwrap()
                .stability
                .last_error
                .is_some_and(|error| error.contains("transport failure"))
        );

        let replacement_pid = spawned.supervisor.replace().await.unwrap();
        let health = spawned.supervisor.health();
        assert_eq!(health.state, ExtensionProcessState::Healthy);
        assert_eq!(health.pid, Some(replacement_pid));
        let third = spawned
            .feature
            .execute("echo", "call-3", json!({}), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(third.details["is_error"], Value::Null);
        spawned
            .supervisor
            .shutdown(std::time::Duration::from_millis(500))
            .await
            .unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn explicit_replacement_rejects_changed_published_tool_shape() {
        let _env_guard = crate::test_support::env::lock_async().await;
        unsafe {
            std::env::remove_var("OMEGON_RUNTIME_CONTEXT");
            std::env::remove_var("KUBERNETES_SERVICE_HOST");
        }
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let extension_dir = temp.path().join("shape");
        std::fs::create_dir(&extension_dir).unwrap();
        let marker = temp.path().join("change-tools");
        let script = extension_dir.join("shape-extension.sh");
        std::fs::write(
            &script,
            format!(
                r#"#!/bin/sh
marker={}
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([^,}}]*\).*/\1/p')
  case "$line" in
    *initialize*) printf '%s\n' "{{\"jsonrpc\":\"2.0\",\"id\":$id,\"error\":{{\"code\":-32601,\"message\":\"Method not found\"}}}}" ;;
    *get_tools*)
      if [ -f "$marker" ]; then name=changed; else name=echo; fi
      printf '%s\n' "{{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":[{{\"name\":\"$name\",\"label\":\"Echo\",\"description\":\"Echo\",\"parameters\":{{\"type\":\"object\",\"properties\":{{}}}}}}]}}"
      ;;
  esac
done
"#,
                marker.display()
            ),
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&script, permissions).unwrap();
        std::fs::write(
            extension_dir.join("manifest.toml"),
            r#"[extension]
name = "shape"
version = "0.1.0"
description = "fixture"
[runtime]
type = "native"
binary = "shape-extension.sh"
[startup]
timeout_ms = 30000
"#,
        )
        .unwrap();

        let spawned = spawn_from_manifest(&extension_dir, &[]).await.unwrap();
        std::fs::write(&marker, "changed").unwrap();
        let error = spawned.supervisor.replace().await.unwrap_err();
        assert!(
            error
                .to_string()
                .contains("changed its published tool definitions"),
            "unexpected replacement error: {error:#}"
        );
        let health = spawned.supervisor.health();
        assert_eq!(health.state, ExtensionProcessState::Unavailable);
        assert!(health.pid.is_none());
        assert!(
            health
                .detail
                .is_some_and(|detail| detail.contains("published tool definitions"))
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn explicit_replacement_fences_and_terminates_an_in_flight_rpc() {
        let _env_guard = crate::test_support::env::lock_async().await;
        unsafe {
            std::env::remove_var("OMEGON_RUNTIME_CONTEXT");
            std::env::remove_var("KUBERNETES_SERVICE_HOST");
        }
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let extension_dir = temp.path().join("blocking");
        std::fs::create_dir(&extension_dir).unwrap();
        let marker = temp.path().join("rpc-started");
        let script = extension_dir.join("blocking-extension.sh");
        std::fs::write(
            &script,
            format!(
                r#"#!/bin/sh
marker={}
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([^,}}]*\).*/\1/p')
  case "$line" in
    *initialize*) printf '%s\n' "{{\"jsonrpc\":\"2.0\",\"id\":$id,\"error\":{{\"code\":-32601,\"message\":\"Method not found\"}}}}" ;;
    *get_tools*) printf '%s\n' "{{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":[{{\"name\":\"echo\",\"label\":\"Echo\",\"description\":\"Echo\",\"parameters\":{{\"type\":\"object\",\"properties\":{{}}}}}}]}}" ;;
    *execute_tool*) touch "$marker"; sleep 30 ;;
    *ping*) printf '%s\n' "{{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{{\"ok\":true}}}}" ;;
  esac
done
"#,
                marker.display()
            ),
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&script, permissions).unwrap();
        std::fs::write(
            extension_dir.join("manifest.toml"),
            r#"[extension]
name = "blocking"
version = "0.1.0"
description = "fixture"
[runtime]
type = "native"
binary = "blocking-extension.sh"
[startup]
timeout_ms = 30000
"#,
        )
        .unwrap();

        let spawned = spawn_from_manifest(&extension_dir, &[]).await.unwrap();
        let polling = spawned.rpc_polling_handle.clone();
        let blocked = tokio::spawn({
            let polling = polling.clone();
            async move { polling.rpc_call("execute_tool", json!({})).await }
        });
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while !marker.exists() {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("fixture RPC should start");

        let replacement_pid = spawned.supervisor.replace().await.unwrap();
        assert!(blocked.await.unwrap().is_err());
        assert_eq!(spawned.supervisor.health().pid, Some(replacement_pid));
        assert_eq!(
            polling.rpc_call("ping", json!({})).await.unwrap()["ok"],
            true
        );
        spawned
            .supervisor
            .shutdown(std::time::Duration::from_millis(500))
            .await
            .unwrap();
        assert!(polling.rpc_call("ping", json!({})).await.is_err());
    }

    #[test]
    fn extension_sdk_tool_schema_normalizes_input_schema() {
        let tools = normalize_extension_tool_definitions(&json!([
            {
                "name": "reader_doctor",
                "description": "Diagnose Bookokrat availability and HostAction readiness",
                "inputSchema": {"type": "object", "properties": {}}
            },
            {
                "name": "reader_open",
                "description": "Open a readable file",
                "inputSchema": {
                    "type": "object",
                    "properties": {"path": {"type": "string"}},
                    "required": ["path"]
                }
            }
        ]))
        .unwrap();

        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name, "reader_doctor");
        assert_eq!(tools[0].label, "reader_doctor");
        assert_eq!(tools[0].parameters["type"], "object");
        assert!(tools[0].description.starts_with(
            "Extension tool (not Omegon core; semantics are owned by the extension):"
        ));
        assert_eq!(tools[1].name, "reader_open");
        assert_eq!(tools[1].parameters["required"][0], "path");
    }

    #[test]
    fn extension_internal_tool_schema_still_accepts_parameters_and_label() {
        let tools = normalize_extension_tool_definitions(&json!([
            {
                "name": "hello_extension",
                "label": "Hello Extension",
                "description": "Say hello",
                "parameters": {"type": "object", "properties": {"name": {"type": "string"}}}
            }
        ]))
        .unwrap();

        assert_eq!(tools[0].name, "hello_extension");
        assert_eq!(tools[0].label, "Hello Extension");
        assert!(
            tools[0]
                .description
                .contains("semantics are owned by the extension")
        );
        assert_eq!(tools[0].parameters["properties"]["name"]["type"], "string");
    }

    #[test]
    fn extension_tool_schema_rejects_missing_name() {
        let err = normalize_extension_tool_definitions(&json!([
            {"description": "broken", "inputSchema": {"type": "object"}}
        ]))
        .unwrap_err();

        assert!(err.to_string().contains("missing non-empty name"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn voice_capable_extension_notification_does_not_break_get_tools_response_matching() {
        let _env_guard = crate::test_support::env::lock_async().await;
        unsafe {
            std::env::remove_var("OMEGON_RUNTIME_CONTEXT");
            std::env::remove_var("KUBERNETES_SERVICE_HOST");
        }
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let script = temp.path().join("voice-extension.sh");
        std::fs::write(
            &script,
            r#"#!/bin/sh
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([^,}}]*\).*/\1/p')
  case "$line" in
    *initialize*)
      printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":$id,\"error\":{\"code\":-32601,\"message\":\"Method not found\"}}"
      ;;
    *get_tools*)
      printf '%s\n' '{"jsonrpc":"2.0","method":"voice/transcription","params":{"text":"synthetic validation","duration_s":0.2}}'
      printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":[{\"name\":\"voice_status\",\"description\":\"Voice status\",\"inputSchema\":{\"type\":\"object\",\"properties\":{}}}]}"
      ;;
    *bootstrap_config*)
      printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"acknowledged\":true}}"
      ;;
    *execute_tool*)
      printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"content\":[{\"type\":\"text\",\"text\":\"ok\"}]}}"
      ;;
  esac
done
"#,
        )
        .unwrap();
        let mut perms = std::fs::metadata(&script).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script, perms).unwrap();

        std::fs::write(
            temp.path().join("manifest.toml"),
            r#"
[extension]
name = "voice-test"
version = "0.1.0"
description = "Voice test extension"

[runtime]
type = "native"
binary = "voice-extension.sh"

[startup]
timeout_ms = 30000

[capabilities]
voice = true
"#,
        )
        .unwrap();

        let spawned = spawn_from_manifest(temp.path(), &[]).await.unwrap();
        let mut rx = spawned
            .voice_notification_rx
            .expect("voice-capable extension should expose notification receiver");
        let names: Vec<String> = spawned
            .feature
            .tools()
            .into_iter()
            .map(|tool| tool.name)
            .collect();
        assert_eq!(names, vec!["voice_status"]);

        let notification = tokio::time::timeout(std::time::Duration::from_secs(10), rx.recv())
            .await
            .expect("notification received")
            .expect("notification channel open");
        assert_eq!(notification.extension_name, "voice-test");
        assert_eq!(notification.method, "voice/transcription");
        assert_eq!(notification.params["text"], "synthetic validation");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn voice_capable_extension_notification_reaches_daemon_queue_through_bridge() {
        let _env_guard = crate::test_support::env::lock_async().await;
        unsafe {
            std::env::remove_var("OMEGON_RUNTIME_CONTEXT");
            std::env::remove_var("KUBERNETES_SERVICE_HOST");
        }
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let script = temp.path().join("voice-extension.sh");
        std::fs::write(
            &script,
            r#"#!/bin/sh
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([^,}}]*\).*/\1/p')
  case "$line" in
    *initialize*)
      printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":$id,\"error\":{\"code\":-32601,\"message\":\"Method not found\"}}"
      ;;
    *get_tools*)
      printf '%s\n' '{"jsonrpc":"2.0","method":"voice/transcription","params":{"text":"summarize the current project","utterance_id":"test-u1","duration_s":1.2}}'
      printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":[{\"name\":\"voice_status\",\"description\":\"Voice status\",\"inputSchema\":{\"type\":\"object\",\"properties\":{}}}]}"
      ;;
  esac
done
"#,
        )
        .unwrap();
        let mut perms = std::fs::metadata(&script).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script, perms).unwrap();

        std::fs::write(
            temp.path().join("manifest.toml"),
            r#"
[extension]
name = "voice-test"
version = "0.1.0"
description = "Voice test extension"

[runtime]
type = "native"
binary = "voice-extension.sh"

[startup]
timeout_ms = 30000

[capabilities]
voice = true
"#,
        )
        .unwrap();

        let spawned = spawn_from_manifest(temp.path(), &[]).await.unwrap();
        let rx = spawned
            .voice_notification_rx
            .expect("voice-capable extension should expose notification receiver");
        let daemon_events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let cancel = tokio_util::sync::CancellationToken::new();
        crate::extensions::voice_bridge::start_voice_bridge(
            rx,
            daemon_events.clone(),
            cancel.clone(),
        );

        let event = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                if let Some(event) = daemon_events.lock().unwrap().first().cloned() {
                    return event;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("voice bridge should inject daemon event");
        cancel.cancel();

        assert_eq!(event.source, "voice");
        assert_eq!(event.trigger_kind, "prompt");
        assert_eq!(event.source_channel.as_deref(), Some("voice"));
        assert_eq!(event.caller_role.as_deref(), Some("edit"));
        assert_eq!(event.payload["text"], "summarize the current project");
        assert_eq!(event.payload["utterance_id"], "test-u1");
        assert_eq!(event.payload["duration_s"], 1.2);
        assert_eq!(event.payload["extension"], "voice-test");
        assert_eq!(event.payload["trust_level"], "operator");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn non_voice_extension_does_not_get_voice_notification_receiver() {
        let _env_guard = crate::test_support::env::lock_async().await;
        unsafe {
            std::env::remove_var("OMEGON_RUNTIME_CONTEXT");
            std::env::remove_var("KUBERNETES_SERVICE_HOST");
        }
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let script = temp.path().join("voice-extension.sh");
        std::fs::write(
            &script,
            r#"#!/bin/sh
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([^,}}]*\).*/\1/p')
  case "$line" in
    *initialize*)
      printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":$id,\"error\":{\"code\":-32601,\"message\":\"Method not found\"}}"
      ;;
    *get_tools*)
      printf '%s\n' '{"jsonrpc":"2.0","method":"voice/transcription","params":{"text":"should not inject"}}'
      printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":[{\"name\":\"status\",\"description\":\"Status\",\"inputSchema\":{\"type\":\"object\",\"properties\":{}}}]}"
      ;;
  esac
done
"#,
        )
        .unwrap();
        let mut perms = std::fs::metadata(&script).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script, perms).unwrap();

        std::fs::write(
            temp.path().join("manifest.toml"),
            r#"
[extension]
name = "not-voice"
version = "0.1.0"
description = "Non voice extension"

[runtime]
type = "native"
binary = "voice-extension.sh"

[startup]
timeout_ms = 30000
"#,
        )
        .unwrap();

        let spawned = spawn_from_manifest(temp.path(), &[]).await.unwrap();
        assert!(
            spawned.voice_notification_rx.is_none(),
            "non-voice extension must not get a voice notification receiver"
        );
        let names: Vec<String> = spawned
            .feature
            .tools()
            .into_iter()
            .map(|tool| tool.name)
            .collect();
        assert_eq!(names, vec!["status"]);
    }

    #[test]
    fn host_rpc_actions_execute_routes_to_policy_pipeline() {
        let manifest = test_manifest(HashMap::new());
        let request = omegon_extension::RpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!("ext-1")),
            method: "actions/execute".to_string(),
            params: json!({
                "action": {"id": "broken", "params": {}}
            }),
        };

        let response =
            host_rpc_response_for_extension_request(&manifest, "test-extension", &request).unwrap();
        assert_eq!(response["id"], "ext-1");
        assert_eq!(response["result"]["status"], "invalid");
    }

    #[test]
    fn host_rpc_actions_execute_requires_outer_lease() {
        let manifest = test_manifest(HashMap::new());
        let request = omegon_extension::RpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!("ext-2")),
            method: "actions/execute".to_string(),
            params: json!({
                "action": {"id": "open-reader", "type": "terminal.create@1", "params": {}}
            }),
        };

        let response =
            host_rpc_response_for_extension_request(&manifest, "test-extension", &request).unwrap();
        assert_eq!(response["result"]["status"], "denied");
        assert_eq!(response["result"]["error"]["code"], "outer_lease_required");
    }

    #[test]
    fn declarative_host_actions_render_as_outcomes_separate_from_content() {
        let mut envelope = tool_result::parse_extension_tool_envelope(json!({
            "content": [{"type": "text", "text": "Opening reader"}],
            "actions": [{"id": "open-reader", "type": "terminal.create@1", "params": {}}]
        }));
        let actions = std::mem::take(&mut envelope.host_actions);
        let outcomes = host_actions::process_declarative_host_actions(
            actions,
            &test_manifest(HashMap::new()),
            "reader",
            "call-1",
        );
        envelope.host_action_outcomes.extend(outcomes);
        let result = envelope.into_tool_result();

        match &result.content[0] {
            ContentBlock::Text { text } => assert_eq!(text, "Opening reader"),
            ContentBlock::Image { .. } => panic!("expected text"),
        }
        assert!(result.details.get("host_actions").is_none());
        assert_eq!(
            result.details["host_action_outcomes"][0]["status"],
            "denied"
        );
        assert_eq!(
            result.details["host_action_outcomes"][0]["error"]["code"],
            "manifest_denied"
        );
    }

    fn config_field(
        field_type: ConfigFieldType,
        default: Option<&str>,
        required: bool,
    ) -> ConfigField {
        ConfigField {
            field_type,
            label: "Test".to_string(),
            description: String::new(),
            required,
            default: default.map(ToString::to_string),
            pattern: None,
            placeholder: None,
            values: Vec::new(),
        }
    }

    fn test_manifest(config: HashMap<String, ConfigField>) -> ExtensionManifest {
        ExtensionManifest {
            extension: manifest::ExtensionMetadata {
                name: "test-extension".to_string(),
                version: "0.1.0".to_string(),
                description: String::new(),
            },
            runtime: RuntimeConfig::Native {
                binary: "test-extension".to_string(),
                env: HashMap::new(),
                env_passthrough: Vec::new(),
                config: HashMap::new(),
            },
            startup: manifest::StartupConfig::default(),
            widgets: HashMap::new(),
            secrets: manifest::SecretsConfig::default(),
            mcp: None,
            config,
            capabilities: omegon_extension::Capabilities::default(),
            permissions: omegon_extension::ManifestPermissions::default(),
            skills: Vec::new(),
        }
    }
}

#[cfg(test)]
mod sdk_compat_metadata_tests {
    use super::*;
    use serde_json::json;

    fn supported() -> SdkCompatibilityDiagnostic {
        sdk_compat::classify_sdk_version(Some(sdk_compat::SUPPORTED_SDK_CONTRACT_VERSION))
    }

    #[test]
    fn metadata_helper_inserts_sdk_compatibility_into_object_metadata() {
        let metadata = metadata_with_sdk_compatibility(
            Some(json!({"extension_info": {"name": "demo"}})),
            &supported(),
        );
        assert_eq!(metadata["extension_info"]["name"], "demo");
        assert_eq!(metadata["sdk_compatibility"]["status"], "supported");
        assert_eq!(metadata["sdk_compatibility"]["supported_version"], "0.25");
    }

    #[test]
    fn metadata_helper_creates_metadata_for_legacy_missing_initialize() {
        let diagnostic = sdk_compat::classify_sdk_version(None);
        let metadata = metadata_with_sdk_compatibility(None, &diagnostic);
        assert_eq!(metadata["sdk_compatibility"]["status"], "missing_legacy");
        assert_eq!(metadata["sdk_compatibility"]["severity"], "warning");
    }

    #[test]
    fn metadata_helper_wraps_non_object_initialize_payload() {
        let metadata = metadata_with_sdk_compatibility(Some(json!("legacy")), &supported());
        assert_eq!(metadata["initialize"], "legacy");
        assert_eq!(metadata["sdk_compatibility"]["status"], "supported");
    }
}

#[cfg(all(test, unix))]
mod sdk_compat_spawn_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::LazyLock;

    static SDK_COMPAT_SPAWN_TEST_LOCK: LazyLock<tokio::sync::Mutex<()>> =
        LazyLock::new(|| tokio::sync::Mutex::new(()));

    fn write_sdk_extension(dir: &Path, sdk_version: Option<&str>) -> PathBuf {
        let script = dir.join("sdk-extension.sh");
        let initialize = match sdk_version {
            Some(version) => format!(
                "printf '%s\\n' '{{\"jsonrpc\":\"2.0\",\"id\":'$id',\"result\":{{\"protocol_version\":2,\"extension_info\":{{\"name\":\"sdk-test\",\"version\":\"0.1.0\",\"sdk_version\":\"{version}\"}},\"capabilities\":{{\"tools\":true}},\"tools\":[]}}}}'"
            ),
            None => "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":'$id',\"error\":{\"code\":-32601,\"message\":\"Method not found\"}}'".to_string(),
        };
        let body = format!(
            r#"#!/bin/sh
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([^,}}]*\).*/\1/p')
  case "$line" in
    *initialize*)
      {initialize}
      ;;
    *get_tools*)
      printf '%s\n' '{{"jsonrpc":"2.0","id":'$id',"result":[{{"name":"status","description":"Status","inputSchema":{{"type":"object","properties":{{}}}}}}]}}'
      ;;
  esac
done
"#
        );
        std::fs::write(&script, body).unwrap();
        let mut perms = std::fs::metadata(&script).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script, perms).unwrap();
        std::fs::write(
            dir.join("manifest.toml"),
            r#"
[extension]
name = "sdk-test"
version = "0.1.0"
description = "SDK compatibility test extension"

[runtime]
type = "native"
binary = "sdk-extension.sh"
[startup]
timeout_ms = 30000
"#,
        )
        .unwrap();
        script
    }

    #[tokio::test]
    async fn spawn_rejects_native_extension_under_host_shim_oci() {
        let _env_guard = crate::test_support::env::lock_async().await;
        let _guard = SDK_COMPAT_SPAWN_TEST_LOCK.lock().await;
        unsafe {
            std::env::set_var("OMEGON_RUNTIME_CONTEXT", "host-shim-oci");
            std::env::remove_var("KUBERNETES_SERVICE_HOST");
        }

        let temp = tempfile::tempdir().unwrap();
        write_sdk_extension(
            temp.path(),
            Some(sdk_compat::SUPPORTED_SDK_CONTRACT_VERSION),
        );
        let err = match spawn_from_manifest(temp.path(), &[]).await {
            Ok(_) => panic!("native extension should be disabled under host-shim OCI"),
            Err(err) => err,
        };
        let message = err.to_string();
        assert!(
            message.contains("native extension 'sdk-test' is disabled"),
            "{message}"
        );
        assert!(message.contains("HostShimOci"), "{message}");

        unsafe {
            std::env::remove_var("OMEGON_RUNTIME_CONTEXT");
        }
    }

    #[tokio::test]
    async fn spawn_accepts_current_sdk_contract() {
        let _env_guard = crate::test_support::env::lock_async().await;
        let _guard = SDK_COMPAT_SPAWN_TEST_LOCK.lock().await;
        unsafe {
            std::env::remove_var("OMEGON_RUNTIME_CONTEXT");
            std::env::remove_var("KUBERNETES_SERVICE_HOST");
        }
        let temp = tempfile::tempdir().unwrap();
        write_sdk_extension(
            temp.path(),
            Some(sdk_compat::SUPPORTED_SDK_CONTRACT_VERSION),
        );
        let spawned = spawn_from_manifest(temp.path(), &[]).await.unwrap();
        assert_eq!(
            spawned.sdk_compatibility.status,
            sdk_compat::SdkCompatibilityStatus::Supported
        );
    }

    #[tokio::test]
    async fn spawn_allows_older_compatible_sdk_contract_with_warning() {
        let _env_guard = crate::test_support::env::lock_async().await;
        let _guard = SDK_COMPAT_SPAWN_TEST_LOCK.lock().await;
        unsafe {
            std::env::remove_var("OMEGON_RUNTIME_CONTEXT");
            std::env::remove_var("KUBERNETES_SERVICE_HOST");
        }
        let temp = tempfile::tempdir().unwrap();
        write_sdk_extension(
            temp.path(),
            Some(sdk_compat::MIN_COMPATIBLE_SDK_CONTRACT_VERSION),
        );
        let spawned = spawn_from_manifest(temp.path(), &[]).await.unwrap();
        assert_eq!(
            spawned.sdk_compatibility.status,
            sdk_compat::SdkCompatibilityStatus::OlderCompatible
        );
        assert!(!spawned.sdk_compatibility.is_blocking());
    }

    #[tokio::test]
    async fn spawn_rejects_newer_unknown_sdk_contract() {
        let _env_guard = crate::test_support::env::lock_async().await;
        let _guard = SDK_COMPAT_SPAWN_TEST_LOCK.lock().await;
        let temp = tempfile::tempdir().unwrap();
        write_sdk_extension(temp.path(), Some("0.26"));
        let err = match spawn_from_manifest(temp.path(), &[]).await {
            Ok(_) => panic!("newer SDK contract should fail spawn"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("SDK contract is incompatible"));
        assert!(err.to_string().contains("newer than supported contract"));
        assert!(err.to_string().contains("newer than supported contract"));
    }

    #[tokio::test]
    async fn spawn_rejects_malformed_sdk_contract() {
        let _env_guard = crate::test_support::env::lock_async().await;
        let _guard = SDK_COMPAT_SPAWN_TEST_LOCK.lock().await;
        unsafe {
            std::env::remove_var("OMEGON_RUNTIME_CONTEXT");
            std::env::remove_var("KUBERNETES_SERVICE_HOST");
        }
        let temp = tempfile::tempdir().unwrap();
        write_sdk_extension(temp.path(), Some("banana"));
        let err = match spawn_from_manifest(temp.path(), &[]).await {
            Ok(_) => panic!("malformed SDK contract should fail spawn"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("SDK contract is incompatible"));
        assert!(err.to_string().contains("malformed SDK contract version"));
    }

    #[tokio::test]
    async fn spawn_allows_missing_initialize_as_legacy_warning() {
        let _env_guard = crate::test_support::env::lock_async().await;
        let _guard = SDK_COMPAT_SPAWN_TEST_LOCK.lock().await;
        unsafe {
            std::env::remove_var("OMEGON_RUNTIME_CONTEXT");
            std::env::remove_var("KUBERNETES_SERVICE_HOST");
        }
        let temp = tempfile::tempdir().unwrap();
        write_sdk_extension(temp.path(), None);
        match spawn_from_manifest(temp.path(), &[]).await {
            Ok(spawned) => {
                assert_eq!(
                    spawned.sdk_compatibility.status,
                    sdk_compat::SdkCompatibilityStatus::MissingLegacy
                );
            }
            Err(err) => {
                let message = err.to_string();
                assert!(
                    message.contains("Method not found"),
                    "unexpected missing-initialize error: {message}"
                );
            }
        }
    }
}
