//! Nex manifest — TOML on-disk format for profile definitions.
//!
//! Operators write `.omegon/nex/my-profile.toml` or
//! `~/.config/omegon/nex/shared-profile.toml`. The manifest is parsed
//! into a [`NexProfile`] with a content-addressed hash for identity.

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;

use super::profile::{
    NexCapabilities, NexDomain, NexOverlay, NexProfile, NexResourceLimits,
};

/// On-disk TOML manifest. Parsed then converted to a [`NexProfile`].
#[derive(Debug, Clone, serde::Deserialize)]
pub struct NexManifest {
    pub profile: ManifestProfile,
    #[serde(default)]
    pub overlays: BTreeMap<String, ManifestOverlay>,
    #[serde(default)]
    pub resources: ManifestResources,
    #[serde(default)]
    pub capabilities: ManifestCapabilities,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ManifestProfile {
    pub name: String,
    /// Base domain name — must match a NexDomain variant or custom image.
    pub base: String,
    /// Explicit OCI image override. If set, bypasses domain-based resolution.
    #[serde(default)]
    pub image: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ManifestOverlay {
    #[serde(default)]
    pub packages: Vec<String>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ManifestResources {
    #[serde(default)]
    pub memory_mb: Option<u64>,
    #[serde(default)]
    pub cpu_shares: Option<u64>,
    #[serde(default)]
    pub pids_limit: Option<u32>,
    #[serde(default)]
    pub readonly_rootfs: Option<bool>,
    #[serde(default)]
    pub network: Option<String>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ManifestCapabilities {
    #[serde(default)]
    pub filesystem_write: Option<bool>,
    #[serde(default)]
    pub network_access: Option<bool>,
    #[serde(default)]
    pub mount_cwd: Option<bool>,
    #[serde(default)]
    pub mount_paths: Vec<std::path::PathBuf>,
    #[serde(default)]
    pub env_passthrough: Vec<String>,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    pub denied_tools: Vec<String>,
}

impl NexManifest {
    /// Parse a manifest from a TOML file on disk.
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read nex manifest: {}", path.display()))?;
        Self::from_toml(&content)
    }

    /// Parse a manifest from a TOML string.
    pub fn from_toml(content: &str) -> Result<Self> {
        toml::from_str(content).context("failed to parse nex manifest")
    }

    /// Convert this manifest into a resolved [`NexProfile`].
    ///
    /// The profile hash is computed from the canonicalized manifest content.
    pub fn into_profile(self) -> NexProfile {
        let profile_hash = compute_manifest_hash(&self);

        let base_domain = parse_domain(&self.profile.base);

        let overlays = self
            .overlays
            .into_iter()
            .map(|(name, overlay)| NexOverlay {
                name,
                packages: overlay.packages,
            })
            .collect();

        let network_mode = self
            .resources
            .network
            .as_deref()
            .map(parse_network_mode)
            .unwrap_or_default();

        let resource_limits = NexResourceLimits {
            memory_mb: self.resources.memory_mb,
            cpu_shares: self.resources.cpu_shares,
            pids_limit: self.resources.pids_limit,
            readonly_rootfs: self.resources.readonly_rootfs.unwrap_or(true),
            network_mode,
        };

        let defaults = NexCapabilities::default();
        let capabilities = NexCapabilities {
            filesystem_write: self.capabilities.filesystem_write.unwrap_or(defaults.filesystem_write),
            network_access: self.capabilities.network_access.unwrap_or(defaults.network_access),
            mount_cwd: self.capabilities.mount_cwd.unwrap_or(defaults.mount_cwd),
            mount_paths: self.capabilities.mount_paths,
            env_passthrough: self.capabilities.env_passthrough,
            allowed_tools: self.capabilities.allowed_tools,
            denied_tools: self.capabilities.denied_tools,
        };

        NexProfile {
            name: self.profile.name,
            profile_hash,
            base_domain,
            overlays,
            resource_limits,
            capabilities,
            image_ref: self.profile.image,
            signed_by: None,
        }
    }
}

fn parse_domain(s: &str) -> NexDomain {
    match s {
        "chat" => NexDomain::Chat,
        "coding" => NexDomain::Coding,
        "coding-python" => NexDomain::CodingPython,
        "coding-node" => NexDomain::CodingNode,
        "coding-rust" => NexDomain::CodingRust,
        "infra" => NexDomain::Infra,
        "full" => NexDomain::Full,
        other => NexDomain::Custom(other.into()),
    }
}

fn parse_network_mode(s: &str) -> super::profile::NexNetworkMode {
    match s {
        "none" => super::profile::NexNetworkMode::None,
        "host" => super::profile::NexNetworkMode::Host,
        "bridge" => super::profile::NexNetworkMode::Bridge,
        other => super::profile::NexNetworkMode::Custom(other.into()),
    }
}

/// Compute a content-addressed SHA-256 hash of the manifest.
///
/// Covers ALL security-relevant fields: profile identity, overlays,
/// resource limits, image override, and every capability. Two profiles
/// that differ in any security-relevant dimension produce different hashes.
/// BTreeMap ensures deterministic overlay order.
fn compute_manifest_hash(manifest: &NexManifest) -> String {
    let mut hasher = Sha256::new();

    // Profile identity
    hasher.update(b"name:");
    hasher.update(manifest.profile.name.as_bytes());
    hasher.update(b"\nbase:");
    hasher.update(manifest.profile.base.as_bytes());
    if let Some(ref img) = manifest.profile.image {
        hasher.update(b"\nimage:");
        hasher.update(img.as_bytes());
    }

    // Overlays (BTreeMap iteration is sorted)
    for (name, overlay) in &manifest.overlays {
        hasher.update(b"\novl:");
        hasher.update(name.as_bytes());
        for pkg in &overlay.packages {
            hasher.update(b",");
            hasher.update(pkg.as_bytes());
        }
    }

    // Resource limits — all fields
    if let Some(mem) = manifest.resources.memory_mb {
        hasher.update(format!("\nmem:{mem}").as_bytes());
    }
    if let Some(cpu) = manifest.resources.cpu_shares {
        hasher.update(format!("\ncpu:{cpu}").as_bytes());
    }
    if let Some(pids) = manifest.resources.pids_limit {
        hasher.update(format!("\npids:{pids}").as_bytes());
    }
    if let Some(ro) = manifest.resources.readonly_rootfs {
        hasher.update(format!("\nro:{ro}").as_bytes());
    }
    if let Some(ref net) = manifest.resources.network {
        hasher.update(format!("\nnet:{net}").as_bytes());
    }

    // Capabilities — every security-relevant field
    if let Some(fw) = manifest.capabilities.filesystem_write {
        hasher.update(format!("\ncap.fs_write:{fw}").as_bytes());
    }
    if let Some(na) = manifest.capabilities.network_access {
        hasher.update(format!("\ncap.net:{na}").as_bytes());
    }
    if let Some(mc) = manifest.capabilities.mount_cwd {
        hasher.update(format!("\ncap.mount_cwd:{mc}").as_bytes());
    }
    for mp in &manifest.capabilities.mount_paths {
        hasher.update(b"\ncap.mount:");
        hasher.update(mp.to_string_lossy().as_bytes());
    }
    for ep in &manifest.capabilities.env_passthrough {
        hasher.update(b"\ncap.env:");
        hasher.update(ep.as_bytes());
    }
    for at in &manifest.capabilities.allowed_tools {
        hasher.update(b"\ncap.allow:");
        hasher.update(at.as_bytes());
    }
    for dt in &manifest.capabilities.denied_tools {
        hasher.update(b"\ncap.deny:");
        hasher.update(dt.as_bytes());
    }

    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_MANIFEST: &str = r#"
[profile]
name = "test-project"
base = "coding-python"

[overlays.ml-deps]
packages = ["python312Packages.torch", "python312Packages.numpy"]

[resources]
memory_mb = 2048
network = "none"
readonly_rootfs = true

[capabilities]
mount_cwd = true
filesystem_write = true
network_access = false
allowed_tools = ["bash", "read_file", "write_file"]
"#;

    #[test]
    fn parse_sample_manifest() {
        let manifest = NexManifest::from_toml(SAMPLE_MANIFEST).unwrap();
        assert_eq!(manifest.profile.name, "test-project");
        assert_eq!(manifest.profile.base, "coding-python");
        assert_eq!(manifest.overlays.len(), 1);
        assert_eq!(manifest.overlays["ml-deps"].packages.len(), 2);
        assert_eq!(manifest.resources.memory_mb, Some(2048));
    }

    #[test]
    fn manifest_to_profile() {
        let manifest = NexManifest::from_toml(SAMPLE_MANIFEST).unwrap();
        let profile = manifest.into_profile();
        assert_eq!(profile.name, "test-project");
        assert_eq!(profile.base_domain, NexDomain::CodingPython);
        assert_eq!(profile.overlays.len(), 1);
        assert_eq!(profile.resource_limits.memory_mb, Some(2048));
        assert!(profile.capabilities.filesystem_write);
        assert!(!profile.capabilities.network_access);
        assert!(!profile.profile_hash.is_empty());
    }

    #[test]
    fn hash_is_deterministic() {
        let m1 = NexManifest::from_toml(SAMPLE_MANIFEST).unwrap();
        let m2 = NexManifest::from_toml(SAMPLE_MANIFEST).unwrap();
        let p1 = m1.into_profile();
        let p2 = m2.into_profile();
        assert_eq!(p1.profile_hash, p2.profile_hash);
    }

    #[test]
    fn minimal_manifest() {
        let toml = r#"
[profile]
name = "minimal"
base = "coding"
"#;
        let profile = NexManifest::from_toml(toml).unwrap().into_profile();
        assert_eq!(profile.base_domain, NexDomain::Coding);
        assert!(profile.overlays.is_empty());
        assert!(profile.resource_limits.readonly_rootfs);
    }

    #[test]
    fn hash_differs_when_capabilities_differ() {
        let locked = r#"
[profile]
name = "locked"
base = "coding"

[capabilities]
network_access = false
filesystem_write = false
"#;
        let open = r#"
[profile]
name = "locked"
base = "coding"

[capabilities]
network_access = true
filesystem_write = true
"#;
        let p1 = NexManifest::from_toml(locked).unwrap().into_profile();
        let p2 = NexManifest::from_toml(open).unwrap().into_profile();
        assert_ne!(p1.profile_hash, p2.profile_hash,
            "profiles with different capabilities must have different hashes");
    }

    #[test]
    fn hash_differs_when_image_differs() {
        let a = r#"
[profile]
name = "test"
base = "coding"
image = "ghcr.io/good/image:v1"
"#;
        let b = r#"
[profile]
name = "test"
base = "coding"
image = "ghcr.io/evil/image:v1"
"#;
        let p1 = NexManifest::from_toml(a).unwrap().into_profile();
        let p2 = NexManifest::from_toml(b).unwrap().into_profile();
        assert_ne!(p1.profile_hash, p2.profile_hash);
    }
}
