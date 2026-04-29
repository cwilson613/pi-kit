//! Core Nex profile types.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A Nex profile — declarative environment specification for agent sandboxing.
///
/// Deterministic: same profile_hash = same OCI image.
/// Identity-bound: signed_by links to a Styrene Identity principal.
/// Materializable: resolves to an OCI image reference for container execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexProfile {
    /// Human-readable name (e.g., "coding-python", "infra-k8s-prod").
    pub name: String,

    /// Content-addressed hash of the profile manifest (SHA-256).
    /// Same hash = same environment. Computed from the canonicalized manifest.
    pub profile_hash: String,

    /// Base domain from nix/profiles.nix this inherits from.
    pub base_domain: NexDomain,

    /// Additional package layers on top of the base domain.
    #[serde(default)]
    pub overlays: Vec<NexOverlay>,

    /// Resource constraints for the container.
    #[serde(default)]
    pub resource_limits: NexResourceLimits,

    /// Capability grants — what this profile is allowed to do.
    #[serde(default)]
    pub capabilities: NexCapabilities,

    /// OCI image reference. Populated after build/resolve.
    /// e.g. "ghcr.io/styrene-lab/omegon-coding-python:0.17.6"
    #[serde(default)]
    pub image_ref: Option<String>,

    /// Identity binding — who created/signed this profile.
    #[serde(default)]
    pub signed_by: Option<NexIdentityBinding>,
}

/// Base domain — maps to nix/profiles.nix domain definitions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NexDomain {
    Chat,
    Coding,
    CodingPython,
    CodingNode,
    CodingRust,
    Infra,
    Full,
    Custom(String),
}

impl NexDomain {
    /// Default OCI image tag suffix for this domain.
    pub fn image_suffix(&self) -> &str {
        match self {
            Self::Chat => "omegon-chat",
            Self::Coding => "omegon",
            Self::CodingPython => "omegon-coding-python",
            Self::CodingNode => "omegon-coding-node",
            Self::CodingRust => "omegon-coding-rust",
            Self::Infra => "omegon-infra",
            Self::Full => "omegon-full",
            Self::Custom(name) => name.as_str(),
        }
    }

    /// Resolve to a default image reference for a given version.
    pub fn default_image_ref(&self, registry: &str, version: &str) -> String {
        format!("{}/{}:{}", registry, self.image_suffix(), version)
    }
}

impl std::fmt::Display for NexDomain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Chat => write!(f, "chat"),
            Self::Coding => write!(f, "coding"),
            Self::CodingPython => write!(f, "coding-python"),
            Self::CodingNode => write!(f, "coding-node"),
            Self::CodingRust => write!(f, "coding-rust"),
            Self::Infra => write!(f, "infra"),
            Self::Full => write!(f, "full"),
            Self::Custom(name) => write!(f, "{name}"),
        }
    }
}

/// Named package overlay layered on top of the base domain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexOverlay {
    pub name: String,
    /// Nix packages to add (e.g., "python312Packages.torch").
    #[serde(default)]
    pub packages: Vec<String>,
}

/// Container resource constraints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexResourceLimits {
    /// Memory limit in megabytes. None = unlimited.
    #[serde(default)]
    pub memory_mb: Option<u64>,

    /// CPU shares (relative weight). None = fair share.
    #[serde(default)]
    pub cpu_shares: Option<u64>,

    /// Maximum number of processes. None = unlimited.
    #[serde(default)]
    pub pids_limit: Option<u32>,

    /// Mount the root filesystem read-only. Mounted volumes remain writable.
    #[serde(default = "default_true")]
    pub readonly_rootfs: bool,

    /// Container network mode.
    #[serde(default)]
    pub network_mode: NexNetworkMode,
}

impl Default for NexResourceLimits {
    fn default() -> Self {
        Self {
            memory_mb: None,
            cpu_shares: None,
            pids_limit: None,
            readonly_rootfs: true,
            network_mode: NexNetworkMode::None,
        }
    }
}

/// Container network isolation mode.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NexNetworkMode {
    /// No network access (default — safest for agent sandboxing).
    #[default]
    None,
    /// Share the host network namespace.
    Host,
    /// Bridge network with outbound access.
    Bridge,
    /// Custom network name or config.
    Custom(String),
}

impl NexNetworkMode {
    /// Container CLI flag value.
    pub fn as_flag(&self) -> &str {
        match self {
            Self::None => "none",
            Self::Host => "host",
            Self::Bridge => "bridge",
            Self::Custom(name) => name.as_str(),
        }
    }
}

/// Capability grants scoping what the sandboxed agent can do.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexCapabilities {
    /// Allow writing to the mounted workspace filesystem.
    #[serde(default = "default_true")]
    pub filesystem_write: bool,

    /// Allow outbound network access (overrides resource_limits.network_mode
    /// if both are set — capabilities take precedence).
    #[serde(default)]
    pub network_access: bool,

    /// Mount the operator's current working directory into the container.
    #[serde(default = "default_true")]
    pub mount_cwd: bool,

    /// Additional host paths to mount (read-only unless filesystem_write is true).
    #[serde(default)]
    pub mount_paths: Vec<PathBuf>,

    /// Environment variables to pass through from host to container.
    #[serde(default)]
    pub env_passthrough: Vec<String>,

    /// Allowlist of tools the agent may use. Empty = all tools allowed.
    #[serde(default)]
    pub allowed_tools: Vec<String>,

    /// Denylist of tools the agent may not use. Checked after allowed_tools.
    #[serde(default)]
    pub denied_tools: Vec<String>,
}

impl Default for NexCapabilities {
    fn default() -> Self {
        Self {
            filesystem_write: true,
            network_access: false,
            mount_cwd: true,
            mount_paths: Vec::new(),
            env_passthrough: Vec::new(),
            allowed_tools: Vec::new(),
            denied_tools: Vec::new(),
        }
    }
}

/// Identity binding — links a profile to its creator/signer.
///
/// In Phase 4, `signature` will be populated via Styrene Identity Ed25519.
/// Until then, the principal fields provide traceability without crypto.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexIdentityBinding {
    /// Principal who created/signed this profile.
    pub principal_id: String,

    /// Identity issuer (e.g., "local-session", "styrene-identity").
    #[serde(default)]
    pub issuer: Option<String>,

    /// Ed25519 signature over the profile manifest (Phase 4+).
    #[serde(default)]
    pub signature: Option<String>,
}

impl NexProfile {
    /// Bind this profile to the current operator identity.
    pub fn bind_identity(&mut self, identity: &crate::settings::RuntimeIdentity) {
        self.signed_by = Some(NexIdentityBinding {
            principal_id: identity
                .principal_id
                .clone()
                .unwrap_or_else(|| "anonymous".into()),
            issuer: identity.issuer.clone(),
            signature: None,
        });
    }

    /// Derive an AuthorizationContext from this profile's capabilities.
    pub fn authorization_context(&self) -> crate::settings::AuthorizationContext {
        let mut caps = Vec::new();
        if self.capabilities.filesystem_write {
            caps.push("fs:write".into());
        }
        if self.capabilities.network_access {
            caps.push("net:access".into());
        }
        if self.capabilities.mount_cwd {
            caps.push("fs:mount-cwd".into());
        }
        if !self.capabilities.mount_paths.is_empty() {
            caps.push(format!("fs:mount-extra:{}", self.capabilities.mount_paths.len()));
        }
        crate::settings::AuthorizationContext {
            roles: vec!["nex-agent".into()],
            capabilities: caps,
            trust_domain: self
                .signed_by
                .as_ref()
                .and_then(|b| b.issuer.clone()),
        }
    }
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_image_suffix() {
        assert_eq!(NexDomain::CodingPython.image_suffix(), "omegon-coding-python");
        assert_eq!(NexDomain::Coding.image_suffix(), "omegon");
    }

    #[test]
    fn default_resource_limits_are_restrictive() {
        let limits = NexResourceLimits::default();
        assert!(limits.readonly_rootfs);
        assert_eq!(limits.network_mode, NexNetworkMode::None);
    }

    #[test]
    fn default_capabilities_allow_fs_deny_network() {
        let caps = NexCapabilities::default();
        assert!(caps.filesystem_write);
        assert!(!caps.network_access);
        assert!(caps.mount_cwd);
    }

    #[test]
    fn network_mode_flags() {
        assert_eq!(NexNetworkMode::None.as_flag(), "none");
        assert_eq!(NexNetworkMode::Host.as_flag(), "host");
        assert_eq!(NexNetworkMode::Custom("mynet".into()).as_flag(), "mynet");
    }
}
