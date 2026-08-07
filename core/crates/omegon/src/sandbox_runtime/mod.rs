//! Sandbox profiles — deterministic sandbox isolation for Omegon agents.
//!
//! A Sandbox profile is a declarative environment specification that materializes
//! as an OCI container. Agents spawned with a Sandbox profile run inside the
//! container with scoped filesystem, network, and tool access.
//!
//! # Architecture
//!
//! ```text
//! SandboxManifest (TOML)  ──parse──→  SandboxProfile (Rust types)
//!                                      │
//!                     ┌────────────────┼────────────────┐
//!                     ↓                ↓                ↓
//!               SandboxRegistry      materialize()    bind_identity()
//!            (lookup by name)   (→ podman run)   (→ RuntimeIdentity)
//! ```
//!
//! Profiles are deterministic: same manifest content → same profile hash →
//! same OCI image. Identity binding links a profile to its creator via
//! Styrene Identity (when available) or local-operator placeholders.

pub mod compose;
mod container;
mod manifest;
mod profile;
mod registry;
pub mod spawn;

pub use container::materialize_container;
pub use manifest::SandboxManifest;
pub use profile::{
    SandboxCapabilities, SandboxDomain, SandboxEgressFilter, SandboxIdentityBinding,
    SandboxNetworkPolicy, SandboxOverlay, SandboxPortMapping, SandboxPortProtocol, SandboxProfile,
    SandboxResourceLimits,
};
pub use registry::SandboxRegistry;
pub use spawn::{detect_container_runtime_public, spawn_containerized_child_agent};
