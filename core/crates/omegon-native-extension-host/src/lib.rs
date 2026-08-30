//! Dependency-clean native extension manifest, protocol transport, and process host.
//!
//! The host owns every spawned child and all stdin/stdout JSON-RPC access. Product
//! policy such as admission, secret resolution, host actions, and result mapping is
//! supplied by callers before launch or through [`HostRequestHandler`].

mod manifest;
mod process;
mod sdk_compat;

pub use manifest::{
    ConnectionMode, ExtensionManifest, ExtensionMetadata, ExtensionSkillConfig, McpConfig,
    McpTransport, RuntimeConfig, SecretsConfig, StartupConfig, WidgetConfig,
};
pub use process::{
    ExtensionHandshake, ExtensionNotification, ExtensionProcessHealth, ExtensionProcessState,
    ExtensionSupervisor, HostRequestHandler, LaunchSpec, ReadinessValidator, RpcRequestPolicy,
    normalize_tool_definitions, shutdown_supervisors,
};
pub use sdk_compat::{
    MIN_COMPATIBLE_SDK_CONTRACT_VERSION, SUPPORTED_SDK_CONTRACT_VERSION,
    SdkCompatibilityDiagnostic, SdkCompatibilitySeverity, SdkCompatibilityStatus,
    classify_initialize_metadata, classify_sdk_version,
};
