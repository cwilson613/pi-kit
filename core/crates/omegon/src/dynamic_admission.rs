//! Trust admission for contribution-controlled evaluation, spawn, and connect boundaries.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use omegon_traits::{
    RUNTIME_DYNAMIC_PREFLIGHT_SCHEMA_VERSION, RuntimeDynamicContributionPreflight,
    RuntimeTrustAdmission, RuntimeTrustAdmissionEvidence, RuntimeTrustedCodeAuthority,
};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Default)]
pub(crate) struct DynamicAdmissionPolicy {
    trusted_code: BTreeSet<omegon_traits::RuntimeContributionId>,
}

impl DynamicAdmissionPolicy {
    pub(crate) fn from_profile(profile: &crate::settings::Profile) -> Self {
        let trusted_code = profile
            .permissions
            .trusted_contribution_code
            .iter()
            .filter_map(
                |id| match omegon_traits::RuntimeContributionId::new(id.clone()) {
                    Ok(id) => Some(id),
                    Err(error) => {
                        tracing::warn!(
                            contribution = id,
                            error,
                            "invalid trusted contribution id ignored"
                        );
                        None
                    }
                },
            )
            .collect();
        Self { trusted_code }
    }

    pub(crate) fn admit(
        &self,
        preflight: RuntimeDynamicContributionPreflight,
    ) -> Result<DynamicAdmissionPermit> {
        preflight.validate().map_err(|error| anyhow!(error))?;
        if !self.trusted_code.contains(&preflight.id) {
            return Err(anyhow!(
                "dynamic contribution '{}' requires explicit trusted-code admission in permissions.trustedContributionCode; verified confinement is unavailable",
                preflight.id.as_str()
            ));
        }
        let admission = RuntimeTrustAdmission {
            schema_version: RUNTIME_DYNAMIC_PREFLIGHT_SCHEMA_VERSION,
            contribution_id: preflight.id.clone(),
            source_digest: preflight.source_digest.clone(),
            evidence: RuntimeTrustAdmissionEvidence::TrustedCode {
                authority: RuntimeTrustedCodeAuthority::OperatorPolicy,
                policy_id: "profile:trusted-contribution-code-v1".into(),
            },
        };
        admission
            .validate_for(&preflight)
            .map_err(|error| anyhow!(error))?;
        Ok(DynamicAdmissionPermit {
            preflight,
            admission,
        })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DynamicAdmissionPermit {
    preflight: RuntimeDynamicContributionPreflight,
    admission: RuntimeTrustAdmission,
}

impl DynamicAdmissionPermit {
    pub(crate) fn contribution_id(&self) -> &omegon_traits::RuntimeContributionId {
        &self.preflight.id
    }

    pub(crate) fn source_digest(&self) -> &str {
        &self.preflight.source_digest
    }

    pub(crate) fn validate(&self) -> Result<()> {
        self.admission
            .validate_for(&self.preflight)
            .map_err(|error| anyhow!(error))
    }

    pub(crate) fn validate_source_path(&self, source: &Path) -> Result<()> {
        self.validate()?;
        let observed = digest_path(source)?;
        if observed != self.preflight.source_digest {
            return Err(anyhow!(
                "dynamic contribution '{}' source changed after trust admission",
                self.preflight.id.as_str()
            ));
        }
        Ok(())
    }

    pub(crate) fn for_kernel_release(
        preflight: RuntimeDynamicContributionPreflight,
    ) -> Result<Self> {
        preflight.validate().map_err(|error| anyhow!(error))?;
        let admission = RuntimeTrustAdmission {
            schema_version: RUNTIME_DYNAMIC_PREFLIGHT_SCHEMA_VERSION,
            contribution_id: preflight.id.clone(),
            source_digest: preflight.source_digest.clone(),
            evidence: RuntimeTrustAdmissionEvidence::TrustedCode {
                authority: RuntimeTrustedCodeAuthority::KernelRelease,
                policy_id: "release:signed-generation-v1".into(),
            },
        };
        admission
            .validate_for(&preflight)
            .map_err(|error| anyhow!(error))?;
        Ok(Self {
            preflight,
            admission,
        })
    }

    #[cfg(test)]
    pub(crate) fn for_test(preflight: RuntimeDynamicContributionPreflight) -> Self {
        let admission = RuntimeTrustAdmission {
            schema_version: RUNTIME_DYNAMIC_PREFLIGHT_SCHEMA_VERSION,
            contribution_id: preflight.id.clone(),
            source_digest: preflight.source_digest.clone(),
            evidence: RuntimeTrustAdmissionEvidence::TrustedCode {
                authority: RuntimeTrustedCodeAuthority::OperatorPolicy,
                policy_id: "test:explicit-trust".into(),
            },
        };
        Self {
            preflight,
            admission,
        }
    }

    #[cfg(test)]
    pub(crate) fn for_test_id(
        id: &str,
        source_kind: omegon_traits::RuntimeDynamicSourceKind,
    ) -> Result<Self> {
        let preflight = RuntimeDynamicContributionPreflight {
            schema_version: RUNTIME_DYNAMIC_PREFLIGHT_SCHEMA_VERSION,
            id: omegon_traits::RuntimeContributionId::new(id.to_string())
                .map_err(|error| anyhow!(error))?,
            source_digest: digest_bytes(id.as_bytes()),
            source_kind,
            protocol: omegon_traits::RuntimeProtocolRange::new(1, 1)
                .map_err(|error| anyhow!(error))?,
            minimum_dependencies: Vec::new(),
            requested_trust: omegon_traits::RuntimeTrustRequest::OperatorManaged,
            requested_confinement: omegon_traits::RuntimeConfinementRequest::HostProcess,
            probe: omegon_traits::RuntimeProbeRequirements {
                operations: vec![omegon_traits::RuntimeProbeOperation::DiscoverCapabilities],
                timeout_ms: 1,
                requested_effects: Vec::new(),
            },
        };
        Ok(Self::for_test(preflight))
    }
}

pub(crate) fn digest_path(path: &Path) -> Result<String> {
    let mut hasher = Sha256::new();
    hash_path(&mut hasher, path, Path::new(""))?;
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

pub(crate) fn digest_bytes(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn hash_path(hasher: &mut Sha256, root: &Path, relative: &Path) -> Result<()> {
    let path = root.join(relative);
    let metadata = std::fs::symlink_metadata(&path).with_context(|| {
        format!(
            "could not inspect dynamic contribution source {}",
            path.display()
        )
    })?;
    if metadata.file_type().is_symlink() {
        return Err(anyhow!(
            "dynamic contribution source contains unsupported symlink: {}",
            path.display()
        ));
    }
    if metadata.is_file() {
        hasher.update(b"file\0");
        hasher.update(relative.as_os_str().as_encoded_bytes());
        hasher.update(b"\0");
        hasher.update(std::fs::read(&path)?);
        return Ok(());
    }
    if !metadata.is_dir() {
        return Err(anyhow!(
            "dynamic contribution source contains unsupported entry: {}",
            path.display()
        ));
    }

    let mut entries = std::fs::read_dir(&path)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<std::io::Result<Vec<_>>>()?;
    entries.sort();
    for name in entries {
        hash_path(hasher, root, &relative.join(name))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use omegon_traits::{
        RuntimeConfinementRequest, RuntimeDynamicSourceKind, RuntimeProbeOperation,
        RuntimeProbeRequirements, RuntimeProtocolRange, RuntimeTrustRequest,
    };

    fn preflight(id: &str, source: &Path) -> RuntimeDynamicContributionPreflight {
        RuntimeDynamicContributionPreflight {
            schema_version: RUNTIME_DYNAMIC_PREFLIGHT_SCHEMA_VERSION,
            id: omegon_traits::RuntimeContributionId::new(id).unwrap(),
            source_digest: digest_path(source).unwrap(),
            source_kind: RuntimeDynamicSourceKind::NativeExtension,
            protocol: RuntimeProtocolRange::new(1, 1).unwrap(),
            minimum_dependencies: Vec::new(),
            requested_trust: RuntimeTrustRequest::OperatorManaged,
            requested_confinement: RuntimeConfinementRequest::HostProcess,
            probe: RuntimeProbeRequirements {
                operations: vec![RuntimeProbeOperation::Initialize],
                timeout_ms: 1,
                requested_effects: Vec::new(),
            },
        }
    }

    #[test]
    fn policy_denies_unlisted_code_and_binds_permit_to_source_bytes() {
        let source = tempfile::tempdir().unwrap();
        std::fs::write(source.path().join("manifest.toml"), "v1").unwrap();
        let request = preflight("extension:test", source.path());
        let denied = DynamicAdmissionPolicy::default().admit(request.clone());
        assert!(
            denied
                .unwrap_err()
                .to_string()
                .contains("explicit trusted-code")
        );

        let mut profile = crate::settings::Profile::default();
        profile
            .permissions
            .trusted_contribution_code
            .push("extension:test".into());
        let permit = DynamicAdmissionPolicy::from_profile(&profile)
            .admit(request)
            .unwrap();
        permit.validate_source_path(source.path()).unwrap();

        std::fs::write(source.path().join("manifest.toml"), "v2").unwrap();
        assert!(permit.validate_source_path(source.path()).is_err());
    }

    #[test]
    fn kernel_release_permit_needs_no_operator_profile_grant() {
        let source = tempfile::tempdir().unwrap();
        std::fs::write(source.path().join("manifest.toml"), "release-v1").unwrap();
        let permit = DynamicAdmissionPermit::for_kernel_release(preflight(
            "extension:omegon-codescan",
            source.path(),
        ))
        .unwrap();
        permit.validate_source_path(source.path()).unwrap();

        std::fs::write(source.path().join("manifest.toml"), "release-v2").unwrap();
        assert!(permit.validate_source_path(source.path()).is_err());
    }
}
