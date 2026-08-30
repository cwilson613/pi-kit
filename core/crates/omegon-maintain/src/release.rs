use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Component, Path},
    time::{Duration, Instant},
};

use flate2::bufread::GzDecoder;
use omegon_maintenance_contracts::{
    AuthorityKey, MaintenanceResultV1, PackageManifestV1, PackageMemberV1,
    ResidentCompositionLockV1, canonical_json, parse_record,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use sigstore_verify::{
    VerificationPolicy,
    trust_root::{SIGSTORE_PRODUCTION_TRUSTED_ROOT, TrustedRoot},
    types::{Bundle, HashAlgorithm, SignatureContent},
    verify,
};

const MAX_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;
const MAX_BUNDLE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_ARCHIVE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_MEMBER_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_AGGREGATE_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_MEMBERS: usize = 100_000;
const EXPECTED_REPOSITORY: &str = "styrene-lab/omegon";
const EXPECTED_ISSUER: &str = "https://token.actions.githubusercontent.com";
const RELEASE_WORKFLOW: &str = ".github/workflows/release.yml";
const OFFICIAL_BUNDLE_V03: &str = "application/vnd.dev.sigstore.bundle.v0.3+json";
const ALLOWED_METADATA: &[&str] = &["THIRD_PARTY_NOTICES", "sbom.cdx.json"];
const RESIDENT_LOCKS: &[&str] = &[
    "omegon.composition-lock.json",
    "omegon-maintain.composition-lock.json",
];
const CONTENT_MANIFEST: &str = "share/omegon/content-packs/omegon-shipped/content-pack.toml";
const CODESCAN_MANIFEST: &str = "share/omegon/extensions/omegon-codescan/manifest.toml";
const CODESCAN_EXECUTABLE: &str =
    "share/omegon/extensions/omegon-codescan/target/release/omegon-codescan";
const CODESCAN_COMPONENT_LOCK: &str = "share/omegon/components/core-codescan.lock.json";
const LEGACY_FIXTURE_TAG: &str = "v0.29.0-dev-fixture.1";
const LEGACY_FIXTURE_ARCHIVE_DIGEST: &str =
    "77b590261b59f46d00abdb9f617e5bd460b0900a08263aefc69af94b4c9f4528";
const LEGACY_FIXTURE_RECORD_ID: &str =
    "8e726e838978fdecb6eafe35e9708fb7b8a8616823896bdb1192c4692858f7ab";

#[derive(Debug)]
struct VerificationError {
    code: &'static str,
    message: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct OperandIdentity {
    device: u64,
    inode: u64,
    size: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
}

impl VerificationError {
    fn new(code: &'static str, message: impl ToString) -> Self {
        Self {
            code,
            message: message.to_string(),
        }
    }
}

pub(super) fn verify_release(
    archive_path: &Path,
    manifest_path: &Path,
    bundle_path: &Path,
    started: Instant,
    deadline: Duration,
    result: &mut MaintenanceResultV1,
) {
    match verify_release_inner(archive_path, manifest_path, bundle_path, started, deadline) {
        Ok(evidence) => super::diagnostic(
            result,
            "release_verified",
            omegon_maintenance_contracts::Severity::Info,
            "release",
            "release archive, signed manifest, transparency evidence, and compiled identity policy are valid",
            Some(evidence),
        ),
        Err(error) => super::fail(result, error.code, "verification", false, &error.message),
    }
}

fn verify_release_inner(
    archive_path: &Path,
    manifest_path: &Path,
    bundle_path: &Path,
    started: Instant,
    deadline: Duration,
) -> Result<serde_json::Value, VerificationError> {
    check_deadline(started, deadline)?;
    let manifest_bytes = read_operand(
        manifest_path,
        MAX_MANIFEST_BYTES,
        "release_manifest_invalid",
        started,
        deadline,
    )?;
    let manifest: PackageManifestV1 = parse_record(&manifest_bytes)
        .map_err(|error| VerificationError::new("release_manifest_invalid", error))?;
    verify_compiled_policy(&manifest, archive_path)?;

    let bundle_bytes = read_operand(
        bundle_path,
        MAX_BUNDLE_BYTES,
        "release_bundle_invalid",
        started,
        deadline,
    )?;
    let bundle_json = std::str::from_utf8(&bundle_bytes)
        .map_err(|error| VerificationError::new("release_bundle_invalid", error))?;
    let bundle = Bundle::from_json(bundle_json)
        .map_err(|error| VerificationError::new("release_bundle_invalid", error))?;
    let bundle_integrated_time = verify_bundle_profile(&bundle)?;
    check_deadline(started, deadline)?;

    let trusted_root = TrustedRoot::from_json(SIGSTORE_PRODUCTION_TRUSTED_ROOT)
        .map_err(|error| VerificationError::new("release_trust_root_invalid", error))?;
    verify_rekor_binding(&bundle, &trusted_root, bundle_integrated_time)?;
    let trusted_root = trust_root_at_integrated_time(trusted_root, bundle_integrated_time)?;
    let policy = VerificationPolicy::default()
        .require_identity(&manifest.workflow_identity)
        .require_issuer(&manifest.issuer);
    let signature = verify(&manifest_bytes, &bundle, &policy, &trusted_root)
        .map_err(|error| VerificationError::new("release_signature_invalid", error))?;
    let integrated_time = signature.integrated_time.ok_or_else(|| {
        VerificationError::new(
            "release_transparency_invalid",
            "verified bundle did not yield a Rekor integrated time",
        )
    })?;

    check_deadline(started, deadline)?;
    let mut archive_file = open_operand(
        archive_path,
        MAX_ARCHIVE_BYTES,
        "release_archive_invalid",
        started,
        deadline,
    )?;
    let archive_identity = file_identity(&archive_file, "release_archive_invalid")?;
    let archive_digest = hash_archive(&mut archive_file, started, deadline)?;
    if archive_digest != manifest.archive_digest {
        return Err(VerificationError::new(
            "release_archive_digest_mismatch",
            "archive digest does not match the signed package manifest",
        ));
    }
    archive_file
        .seek(SeekFrom::Start(0))
        .map_err(|error| VerificationError::new("release_archive_invalid", error))?;
    verify_archive_members(
        archive_file
            .try_clone()
            .map_err(|error| VerificationError::new("release_archive_invalid", error))?,
        &manifest.members,
        started,
        deadline,
    )?;
    verify_resident_locks(
        archive_file
            .try_clone()
            .map_err(|error| VerificationError::new("release_archive_invalid", error))?,
        &manifest,
        started,
        deadline,
    )?;
    verify_product_component_lock(
        archive_file
            .try_clone()
            .map_err(|error| VerificationError::new("release_archive_invalid", error))?,
        &manifest,
        started,
        deadline,
    )?;
    archive_file
        .seek(SeekFrom::Start(0))
        .map_err(|error| VerificationError::new("release_archive_invalid", error))?;
    if hash_archive(&mut archive_file, started, deadline)? != archive_digest {
        return Err(VerificationError::new(
            "release_archive_invalid",
            "archive changed while it was being verified",
        ));
    }
    if file_identity(&archive_file, "release_archive_invalid")? != archive_identity {
        return Err(VerificationError::new(
            "release_archive_invalid",
            "archive identity changed during verification",
        ));
    }

    Ok(json!({
        "repository": manifest.repository,
        "version": manifest.version,
        "target": manifest.target,
        "commit": manifest.commit,
        "archive_digest": archive_digest,
        "integrated_time": integrated_time,
        "members_verified": manifest.members.len(),
        "composition_locks_verified": manifest.composition_locks.len(),
        "signing_identity": manifest.workflow_identity,
        "signature_verification": "verified",
        "trust_root": "sigstore-production-embedded",
    }))
}

fn verify_product_component_lock(
    file: File,
    manifest: &PackageManifestV1,
    started: Instant,
    deadline: Duration,
) -> Result<(), VerificationError> {
    if manifest.product_component_locks.is_empty() {
        if is_pinned_legacy_fixture(manifest) {
            return Ok(());
        }
        return Err(VerificationError::new(
            "release_component_lock_invalid",
            "full-product manifest lacks required product-component evidence",
        ));
    }
    let inventory = manifest.core_components.as_deref().unwrap_or_default();
    if manifest.host_profile.as_deref() != Some("full-product")
        || manifest.composition_class.as_deref() != Some("full-product")
        || inventory.len() != 1
        || inventory[0].component_id != "core:codescan"
        || inventory[0].wire_manifest_id != "omegon-codescan"
        || manifest.product_component_locks.len() != 1
    {
        return Err(VerificationError::new(
            "release_component_lock_invalid",
            "full-product manifest does not declare the exact product-component inventory",
        ));
    }
    let lock = &manifest.product_component_locks[0];
    let members = manifest
        .members
        .iter()
        .map(|member| (member.path.as_str(), member))
        .collect::<BTreeMap<_, _>>();
    if lock.schema_version != 1
        || lock.component_id != "core:codescan"
        || lock.wire_manifest_id != "omegon-codescan"
        || lock.manifest_path != CODESCAN_MANIFEST
        || lock.executable_path != CODESCAN_EXECUTABLE
        || lock.target != manifest.target
        || lock.protocol_minimum != 1
        || lock.protocol_maximum != 1
        || lock.protocol_version != 1
        || lock.fallback != "typed_unavailable"
        || lock.signing_identity.issuer != manifest.issuer
        || lock.signing_identity.workflow_identity != manifest.workflow_identity
        || lock.signing_identity.verification != "required"
        || members
            .get(CODESCAN_MANIFEST)
            .is_none_or(|member| member.digest != lock.manifest_digest)
        || members
            .get(CODESCAN_EXECUTABLE)
            .is_none_or(|member| member.digest != lock.executable_digest)
    {
        return Err(VerificationError::new(
            "release_component_lock_invalid",
            "product-component evidence does not match signed package inventory",
        ));
    }
    let expected = canonical_json(&manifest.product_component_locks[0])
        .map_err(|error| VerificationError::new("release_component_lock_invalid", error))?;
    let decoder = GzDecoder::new(BufReader::new(file));
    let mut archive = tar::Archive::new(decoder);
    for entry in archive
        .entries()
        .map_err(|error| VerificationError::new("release_archive_invalid", error))?
    {
        check_deadline(started, deadline)?;
        let mut entry =
            entry.map_err(|error| VerificationError::new("release_archive_invalid", error))?;
        if entry.path().ok().as_deref() != Some(Path::new(CODESCAN_COMPONENT_LOCK)) {
            continue;
        }
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|error| VerificationError::new("release_component_lock_invalid", error))?;
        if bytes != expected {
            return Err(VerificationError::new(
                "release_component_lock_invalid",
                "archive component lock does not equal signed product-component evidence",
            ));
        }
        return Ok(());
    }
    Err(VerificationError::new(
        "release_component_lock_invalid",
        "archive lacks the signed product-component lock",
    ))
}

fn verify_compiled_policy(
    manifest: &PackageManifestV1,
    archive_path: &Path,
) -> Result<(), VerificationError> {
    let expected_ref = format!("refs/tags/{}", manifest.tag);
    let expected_tag = format!("v{}", manifest.version);
    let expected_workflow = format!(
        "https://github.com/{EXPECTED_REPOSITORY}/{RELEASE_WORKFLOW}@{}",
        manifest.git_ref
    );
    if manifest.repository != EXPECTED_REPOSITORY
        || manifest.issuer != EXPECTED_ISSUER
        || manifest.git_ref != expected_ref
        || manifest.tag != expected_tag
        || manifest.workflow_identity != expected_workflow
        || !super::SUPPORTED_TARGETS.contains(&manifest.target.as_str())
    {
        return Err(VerificationError::new(
            "release_policy_mismatch",
            "signed manifest does not match the compiled repository, workflow, issuer, ref, tag, version, or target policy",
        ));
    }
    let archive_name = archive_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            VerificationError::new("release_archive_invalid", "archive filename is not UTF-8")
        })?;
    let expected_archive = format!("omegon-{}-{}.tar.gz", manifest.version, manifest.target);
    if archive_name != manifest.archive_filename || archive_name != expected_archive {
        return Err(VerificationError::new(
            "release_archive_invalid",
            "archive filename or format does not match the signed manifest",
        ));
    }
    if manifest.commit.len() != 40 || !manifest.commit.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(VerificationError::new(
            "release_policy_mismatch",
            "signed manifest commit must be a full hexadecimal Git object ID",
        ));
    }
    for member in &manifest.members {
        match member.path.as_str() {
            "omegon" | "omegon-maintain" if member.mode == 0o755 => {}
            path if RESIDENT_LOCKS.contains(&path) && member.mode == 0o644 => {}
            path if ALLOWED_METADATA.contains(&path) && member.mode == 0o644 => {}
            path if path.starts_with("share/omegon/content-packs/omegon-shipped/")
                && member.mode == 0o644 => {}
            CODESCAN_MANIFEST if member.mode == 0o644 => {}
            CODESCAN_EXECUTABLE if member.mode == 0o755 => {}
            CODESCAN_COMPONENT_LOCK if member.mode == 0o644 => {}
            _ => {
                return Err(VerificationError::new(
                    "release_policy_mismatch",
                    "signed manifest contains a member or mode outside the compiled package allowlist",
                ));
            }
        }
    }
    if manifest.composition_locks.is_empty() {
        if !is_pinned_legacy_fixture(manifest) {
            return Err(VerificationError::new(
                "release_composition_lock_invalid",
                "signed package manifest lacks required composition locks",
            ));
        }
    } else {
        validate_exact_package_lock_set(manifest)?;
        let members = manifest
            .members
            .iter()
            .map(|member| (member.path.as_str(), member))
            .collect::<BTreeMap<_, _>>();
        for lock in manifest
            .composition_locks
            .iter()
            .filter(|lock| lock.required)
        {
            validate_package_composition_lock(lock, &members, manifest)?;
        }
        for lock in manifest
            .composition_locks
            .iter()
            .filter(|lock| !lock.required)
        {
            validate_package_composition_lock(lock, &members, manifest)?;
        }
    }
    Ok(())
}

fn is_pinned_legacy_fixture(manifest: &PackageManifestV1) -> bool {
    manifest.tag == LEGACY_FIXTURE_TAG
        && manifest.archive_digest.to_hex() == LEGACY_FIXTURE_ARCHIVE_DIGEST
        && manifest.record_id.to_hex() == LEGACY_FIXTURE_RECORD_ID
        && manifest.members.len() == 2
}

fn validate_exact_package_lock_set(manifest: &PackageManifestV1) -> Result<(), VerificationError> {
    let expected = BTreeMap::from([
        (
            "executable:omegon",
            (
                "omegon",
                true,
                "fail_closed",
                Some("omegon.composition-lock.json"),
            ),
        ),
        (
            "executable:omegon-maintain",
            (
                "omegon-maintain",
                true,
                "fail_closed",
                Some("omegon-maintain.composition-lock.json"),
            ),
        ),
        (
            "content-pack:omegon-shipped",
            (CONTENT_MANIFEST, false, "typed_unavailable", None),
        ),
    ]);
    if manifest.composition_locks.len() != expected.len() {
        return Err(VerificationError::new(
            "release_composition_lock_invalid",
            "signed package manifest does not contain the exact required composition lock set",
        ));
    }
    let mut seen = BTreeSet::new();
    for lock in &manifest.composition_locks {
        let Some((path, required, fallback, resident_path)) = expected.get(lock.identity.as_str())
        else {
            return Err(VerificationError::new(
                "release_composition_lock_invalid",
                format!(
                    "unknown package composition lock identity: {}",
                    lock.identity
                ),
            ));
        };
        if !seen.insert(lock.identity.as_str())
            || lock.artifact_path != *path
            || lock.required != *required
            || lock.fallback != *fallback
            || lock.resident_lock_path.as_deref() != *resident_path
            || lock.targets != [manifest.target.as_str()]
        {
            return Err(VerificationError::new(
                "release_composition_lock_invalid",
                format!(
                    "package composition lock does not match its exact contract: {}",
                    lock.identity
                ),
            ));
        }
    }
    Ok(())
}

fn validate_package_composition_lock(
    lock: &omegon_maintenance_contracts::ArtifactCompositionLockV1,
    members: &BTreeMap<&str, &PackageMemberV1>,
    manifest: &PackageManifestV1,
) -> Result<(), VerificationError> {
    let member = members.get(lock.artifact_path.as_str()).ok_or_else(|| {
        VerificationError::new(
            "release_composition_lock_invalid",
            format!("locked artifact is absent: {}", lock.artifact_path),
        )
    })?;
    if member.digest != lock.artifact_digest
        || lock.targets != [manifest.target.as_str()]
        || lock.protocol_minimum == 0
        || lock.protocol_minimum > lock.protocol_maximum
        || (lock.required && lock.fallback != "fail_closed")
        || (!lock.required && lock.fallback != "typed_unavailable")
    {
        return Err(VerificationError::new(
            "release_composition_lock_invalid",
            format!("required artifact lock is invalid: {}", lock.identity),
        ));
    }
    if let Some(path) = &lock.resident_lock_path
        && !members.contains_key(path.as_str())
    {
        return Err(VerificationError::new(
            "release_composition_lock_invalid",
            format!("resident lock is absent for {}", lock.identity),
        ));
    }
    Ok(())
}

fn verify_resident_locks(
    file: File,
    manifest: &PackageManifestV1,
    started: Instant,
    deadline: Duration,
) -> Result<(), VerificationError> {
    if manifest.composition_locks.is_empty() {
        return Ok(());
    }
    let expected = manifest
        .composition_locks
        .iter()
        .filter_map(|lock| lock.resident_lock_path.as_deref().map(|path| (path, lock)))
        .collect::<BTreeMap<_, _>>();
    let decoder = GzDecoder::new(BufReader::new(file));
    let mut archive = tar::Archive::new(decoder);
    let mut verified = BTreeSet::new();
    for entry in archive
        .entries()
        .map_err(|error| VerificationError::new("release_archive_invalid", error))?
    {
        check_deadline(started, deadline)?;
        let mut entry =
            entry.map_err(|error| VerificationError::new("release_archive_invalid", error))?;
        let path = entry
            .path()
            .map_err(|error| VerificationError::new("release_archive_invalid", error))?
            .into_owned();
        let Some(path) = path.to_str().map(str::to_owned) else {
            continue;
        };
        let Some(package_lock) = expected.get(path.as_str()) else {
            continue;
        };
        if entry.size() > MAX_MANIFEST_BYTES {
            return Err(VerificationError::new(
                "release_composition_lock_invalid",
                "resident composition lock exceeds its byte limit",
            ));
        }
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut bytes)
            .map_err(|error| VerificationError::new("release_composition_lock_invalid", error))?;
        let lock: ResidentCompositionLockV1 = serde_json::from_slice(&bytes)
            .map_err(|error| VerificationError::new("release_composition_lock_invalid", error))?;
        let canonical = canonical_json(&lock)
            .map_err(|error| VerificationError::new("release_composition_lock_invalid", error))?;
        if canonical != bytes
            || lock.schema_version != 1
            || lock.executable_identity != package_lock.artifact_path
            || lock.executable_digest != package_lock.artifact_digest
            || lock.target != manifest.target
            || lock.protocol_minimum == 0
            || lock.protocol_minimum > lock.protocol_maximum
            || lock.signing_identity.issuer != manifest.issuer
            || lock.signing_identity.workflow_identity != manifest.workflow_identity
            || lock.signing_identity.verification != "required"
        {
            return Err(VerificationError::new(
                "release_composition_lock_invalid",
                format!("resident composition lock is invalid: {path}"),
            ));
        }
        // Required entries are validated as one fail-closed set before any
        // optional entry is inspected. Verification never executes a member.
        validate_exact_resident_set(&lock)?;
        for contribution in lock.contributions.iter().filter(|entry| entry.required) {
            validate_resident_contribution(contribution, &lock, true)?;
        }
        for contribution in lock.contributions.iter().filter(|entry| !entry.required) {
            validate_resident_contribution(contribution, &lock, false)?;
        }
        verified.insert(path);
    }
    if verified.len() != expected.len() {
        return Err(VerificationError::new(
            "release_composition_lock_invalid",
            "one or more signed resident composition locks were not verified",
        ));
    }
    Ok(())
}

fn validate_exact_resident_set(lock: &ResidentCompositionLockV1) -> Result<(), VerificationError> {
    let expected: BTreeSet<&str> = match lock.executable_identity.as_str() {
        "omegon" => omegon_maintenance_contracts::OMEGON_REQUIRED_RESIDENT_IDENTITIES
            .iter()
            .chain(omegon_maintenance_contracts::OMEGON_OPTIONAL_RESIDENT_IDENTITIES)
            .copied()
            .collect(),
        "omegon-maintain" => omegon_maintenance_contracts::OMEGON_MAINTAIN_RESIDENT_IDENTITIES
            .iter()
            .copied()
            .collect(),
        _ => {
            return Err(VerificationError::new(
                "release_composition_lock_invalid",
                "resident lock has an unknown executable identity",
            ));
        }
    };
    let actual = lock
        .contributions
        .iter()
        .map(|entry| entry.identity.as_str())
        .collect::<BTreeSet<_>>();
    if actual != expected || actual.len() != lock.contributions.len() {
        return Err(VerificationError::new(
            "release_composition_lock_invalid",
            format!(
                "resident lock for {} does not contain its exact unique contribution set",
                lock.executable_identity
            ),
        ));
    }
    Ok(())
}

fn validate_resident_contribution(
    contribution: &omegon_maintenance_contracts::ResidentContributionLockV1,
    lock: &ResidentCompositionLockV1,
    required: bool,
) -> Result<(), VerificationError> {
    let expected_fallback = if required {
        "fail_closed"
    } else {
        "typed_unavailable"
    };
    if contribution.identity.trim().is_empty()
        || contribution.artifact_path != lock.executable_identity
        || contribution.artifact_digest != lock.executable_digest
        || contribution.protocol_minimum < lock.protocol_minimum
        || contribution.protocol_maximum > lock.protocol_maximum
        || contribution.protocol_minimum > contribution.protocol_maximum
        || contribution.targets != [lock.target.as_str()]
        || contribution.fallback != expected_fallback
        || contribution.required != required
        || contribution.state
            != if required {
                "resident"
            } else {
                "resident_optional"
            }
    {
        return Err(VerificationError::new(
            "release_composition_lock_invalid",
            format!(
                "resident contribution lock is invalid: {}",
                contribution.identity
            ),
        ));
    }
    Ok(())
}

fn verify_bundle_profile(bundle: &Bundle) -> Result<i64, VerificationError> {
    if bundle.media_type != OFFICIAL_BUNDLE_V03 {
        return Err(VerificationError::new(
            "release_bundle_invalid",
            "release verification accepts only the canonical Sigstore bundle v0.3 media type",
        ));
    }
    if !matches!(
        bundle.verification_material.content,
        sigstore_verify::types::bundle::VerificationMaterialContent::Certificate(_)
    ) {
        return Err(VerificationError::new(
            "release_bundle_invalid",
            "release bundle v0.3 must contain one signing certificate",
        ));
    }
    let SignatureContent::MessageSignature(message) = &bundle.content else {
        return Err(VerificationError::new(
            "release_bundle_invalid",
            "release manifest must use a Sigstore message signature, not DSSE",
        ));
    };
    if message
        .message_digest
        .as_ref()
        .is_none_or(|digest| digest.algorithm != HashAlgorithm::Sha2256)
    {
        return Err(VerificationError::new(
            "release_bundle_invalid",
            "release message signature must declare a SHA-256 digest",
        ));
    }
    if !bundle
        .verification_material
        .timestamp_verification_data
        .rfc3161_timestamps
        .is_empty()
    {
        return Err(VerificationError::new(
            "release_bundle_invalid",
            "the accepted release profile uses Rekor integrated time and forbids RFC3161 timestamps",
        ));
    }
    let [entry] = bundle.verification_material.tlog_entries.as_slice() else {
        return Err(VerificationError::new(
            "release_transparency_invalid",
            "release bundle must contain exactly one Rekor transparency entry",
        ));
    };
    if entry.kind_version.kind != "hashedrekord" || entry.kind_version.version != "0.0.1" {
        return Err(VerificationError::new(
            "release_transparency_invalid",
            "release bundle must use one Rekor hashedrekord v0.0.1 entry",
        ));
    }
    if entry.integrated_time <= 0 || entry.inclusion_promise.is_none() {
        return Err(VerificationError::new(
            "release_transparency_invalid",
            "Rekor entry must contain positive integrated time and a signed entry timestamp",
        ));
    }
    let proof = entry.inclusion_proof.as_ref().ok_or_else(|| {
        VerificationError::new(
            "release_transparency_invalid",
            "Rekor entry lacks an inclusion proof",
        )
    })?;
    let checkpoint = proof
        .checkpoint
        .parse()
        .map_err(|error| VerificationError::new("release_transparency_invalid", error))?;
    if proof.tree_size <= 0 || checkpoint.tree_size != proof.tree_size as u64 {
        return Err(VerificationError::new(
            "release_transparency_invalid",
            "signed checkpoint tree size does not match the inclusion proof",
        ));
    }
    Ok(entry.integrated_time)
}

fn verify_rekor_binding(
    bundle: &Bundle,
    trusted_root: &TrustedRoot,
    integrated_time: i64,
) -> Result<(), VerificationError> {
    let entry = &bundle.verification_material.tlog_entries[0];
    let integrated = jiff::Timestamp::from_second(integrated_time)
        .map_err(|error| VerificationError::new("release_transparency_invalid", error))?;
    trusted_root
        .rekor_key_for_log_at(&entry.log_id.key_id, integrated)
        .map_err(|error| VerificationError::new("release_transparency_invalid", error))?;
    let tlog = trusted_root
        .tlogs
        .iter()
        .find(|log| log.log_id.key_id == entry.log_id.key_id)
        .ok_or_else(|| {
            VerificationError::new(
                "release_transparency_invalid",
                "Rekor log ID is absent from the compiled trust root",
            )
        })?;
    let key_id = entry
        .log_id
        .key_id
        .decode()
        .map_err(|error| VerificationError::new("release_transparency_invalid", error))?;
    if key_id.len() < 4 {
        return Err(VerificationError::new(
            "release_transparency_invalid",
            "Rekor log ID is too short to bind a checkpoint signer",
        ));
    }
    let proof = entry
        .inclusion_proof
        .as_ref()
        .expect("profile requires proof");
    let checkpoint = proof
        .checkpoint
        .parse()
        .map_err(|error| VerificationError::new("release_transparency_invalid", error))?;
    if !checkpoint
        .signatures
        .iter()
        .any(|signature| signature.key_id.as_bytes() == &key_id[..4])
    {
        return Err(VerificationError::new(
            "release_transparency_invalid",
            "checkpoint signer is not bound to the Rekor entry log ID",
        ));
    }
    let host = tlog
        .base_url
        .strip_prefix("https://")
        .and_then(|value| value.split('/').next())
        .ok_or_else(|| {
            VerificationError::new(
                "release_trust_root_invalid",
                "compiled Rekor URL is not canonical HTTPS",
            )
        })?;
    if !checkpoint.origin.starts_with(&format!("{host} - ")) {
        return Err(VerificationError::new(
            "release_transparency_invalid",
            "checkpoint origin is not bound to the Rekor entry log",
        ));
    }
    Ok(())
}

fn trust_root_at_integrated_time(
    mut trusted_root: TrustedRoot,
    integrated_time: i64,
) -> Result<TrustedRoot, VerificationError> {
    let integrated = jiff::Timestamp::from_second(integrated_time)
        .map_err(|error| VerificationError::new("release_transparency_invalid", error))?;
    trusted_root.certificate_authorities.retain(|authority| {
        authority
            .valid_for
            .as_ref()
            .is_none_or(|period| period.contains(integrated).unwrap_or(false))
    });
    trusted_root.tlogs.retain(|log| {
        log.public_key
            .valid_for
            .as_ref()
            .is_none_or(|period| period.contains(integrated).unwrap_or(false))
    });
    trusted_root.ctlogs.retain(|log| {
        log.public_key
            .valid_for
            .as_ref()
            .is_none_or(|period| period.contains(integrated).unwrap_or(false))
    });
    if trusted_root.certificate_authorities.is_empty()
        || trusted_root.tlogs.is_empty()
        || trusted_root.ctlogs.is_empty()
    {
        return Err(VerificationError::new(
            "release_trust_root_invalid",
            "compiled trust root has no material valid by Rekor integrated time",
        ));
    }
    for authority in &mut trusted_root.certificate_authorities {
        authority.valid_for = None;
    }
    for log in &mut trusted_root.tlogs {
        log.public_key.valid_for = None;
    }
    for log in &mut trusted_root.ctlogs {
        log.public_key.valid_for = None;
    }
    Ok(trusted_root)
}

fn hash_archive(
    file: &mut File,
    started: Instant,
    deadline: Duration,
) -> Result<AuthorityKey, VerificationError> {
    let mut hasher = Sha256::new();
    let mut consumed = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        check_deadline(started, deadline)?;
        let read = file
            .read(&mut buffer)
            .map_err(|error| VerificationError::new("release_archive_invalid", error))?;
        if read == 0 {
            break;
        }
        consumed = consumed.checked_add(read as u64).ok_or_else(|| {
            VerificationError::new("release_archive_limit", "archive size overflow")
        })?;
        if consumed > MAX_ARCHIVE_BYTES {
            return Err(VerificationError::new(
                "release_archive_limit",
                "archive exceeds the compressed-byte limit",
            ));
        }
        hasher.update(&buffer[..read]);
    }
    Ok(AuthorityKey::from_bytes(hasher.finalize().into()))
}

fn verify_archive_members(
    file: File,
    members: &[PackageMemberV1],
    started: Instant,
    deadline: Duration,
) -> Result<(), VerificationError> {
    let mut expected = BTreeMap::new();
    let mut folded = BTreeSet::new();
    for member in members {
        validate_member_path(&member.path)?;
        if expected.insert(member.path.clone(), member).is_some()
            || !folded.insert(member.path.to_ascii_lowercase())
        {
            return Err(VerificationError::new(
                "release_manifest_invalid",
                "signed manifest contains duplicate or case-colliding member paths",
            ));
        }
    }

    let decoder = GzDecoder::new(BufReader::new(file));
    let mut archive = tar::Archive::new(decoder);
    let mut seen = BTreeSet::new();
    let mut folded_seen = BTreeSet::new();
    let mut aggregate = 0_u64;
    {
        let mut entries = archive
            .entries()
            .map_err(|error| VerificationError::new("release_archive_invalid", error))?
            .raw(true);
        for entry in &mut entries {
            check_deadline(started, deadline)?;
            if seen.len() == MAX_MEMBERS {
                return Err(VerificationError::new(
                    "release_archive_limit",
                    "archive exceeds the 100000-member limit",
                ));
            }
            let mut entry =
                entry.map_err(|error| VerificationError::new("release_archive_invalid", error))?;
            if !entry.header().entry_type().is_file() {
                return Err(VerificationError::new(
                    "release_archive_invalid",
                    "archive links, devices, directories, and special entries are forbidden",
                ));
            }
            let path = entry
                .path()
                .map_err(|error| VerificationError::new("release_archive_invalid", error))?;
            let path = path.to_str().ok_or_else(|| {
                VerificationError::new(
                    "release_archive_invalid",
                    "archive member path is not UTF-8",
                )
            })?;
            validate_member_path(path)?;
            if !seen.insert(path.to_string()) || !folded_seen.insert(path.to_ascii_lowercase()) {
                return Err(VerificationError::new(
                    "release_archive_invalid",
                    "archive contains duplicate or case-colliding member paths",
                ));
            }
            let expected_member = expected.get(path).ok_or_else(|| {
                VerificationError::new(
                    "release_archive_invalid",
                    "archive contains a member absent from the signed manifest",
                )
            })?;
            verify_member(
                &mut entry,
                expected_member,
                &mut aggregate,
                started,
                deadline,
            )?;
        }
    }
    if seen.len() != expected.len() || expected.keys().any(|path| !seen.contains(path)) {
        return Err(VerificationError::new(
            "release_archive_invalid",
            "archive members do not exactly match the signed manifest",
        ));
    }
    let mut decoder = archive.into_inner();
    let mut trailing = Vec::new();
    decoder
        .by_ref()
        .take(10 * 1024 * 1024 + 1)
        .read_to_end(&mut trailing)
        .map_err(|error| VerificationError::new("release_archive_invalid", error))?;
    if trailing.len() > 10 * 1024 * 1024 || trailing.iter().any(|byte| *byte != 0) {
        return Err(VerificationError::new(
            "release_archive_invalid",
            "archive contains noncanonical or excessive data after its end marker",
        ));
    }
    let mut reader = decoder.into_inner();
    let compressed_position = reader
        .stream_position()
        .map_err(|error| VerificationError::new("release_archive_invalid", error))?;
    let compressed_size = reader
        .get_ref()
        .metadata()
        .map_err(|error| VerificationError::new("release_archive_invalid", error))?
        .len();
    let buffered = reader
        .fill_buf()
        .map_err(|error| VerificationError::new("release_archive_invalid", error))?;
    if compressed_position != compressed_size || !buffered.is_empty() {
        return Err(VerificationError::new(
            "release_archive_invalid",
            "archive contains trailing compressed bytes or an additional gzip member",
        ));
    }
    Ok(())
}

fn verify_member(
    entry: &mut tar::Entry<'_, GzDecoder<BufReader<File>>>,
    expected: &PackageMemberV1,
    aggregate: &mut u64,
    started: Instant,
    deadline: Duration,
) -> Result<(), VerificationError> {
    let size = entry.size();
    if size > MAX_MEMBER_BYTES || size != expected.size {
        return Err(VerificationError::new(
            "release_archive_limit",
            "archive member size exceeds its limit or signed manifest value",
        ));
    }
    *aggregate = aggregate
        .checked_add(size)
        .ok_or_else(|| VerificationError::new("release_archive_limit", "archive size overflow"))?;
    if *aggregate > MAX_AGGREGATE_BYTES {
        return Err(VerificationError::new(
            "release_archive_limit",
            "archive exceeds the aggregate uncompressed-byte limit",
        ));
    }
    let mode = entry
        .header()
        .mode()
        .map_err(|error| VerificationError::new("release_archive_invalid", error))?;
    if mode != expected.mode {
        return Err(VerificationError::new(
            "release_archive_invalid",
            "archive member mode does not match the signed manifest",
        ));
    }
    let mut hasher = Sha256::new();
    let mut consumed = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        check_deadline(started, deadline)?;
        let read = entry
            .read(&mut buffer)
            .map_err(|error| VerificationError::new("release_archive_invalid", error))?;
        if read == 0 {
            break;
        }
        consumed += read as u64;
        hasher.update(&buffer[..read]);
    }
    let digest = AuthorityKey::from_bytes(hasher.finalize().into());
    if consumed != expected.size || digest != expected.digest {
        return Err(VerificationError::new(
            "release_archive_digest_mismatch",
            "archive member bytes do not match the signed manifest",
        ));
    }
    Ok(())
}

fn validate_member_path(path: &str) -> Result<(), VerificationError> {
    let components: Vec<_> = Path::new(path)
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .collect();
    if path.is_empty()
        || path.contains('\\')
        || path.contains(':')
        || components.join("/") != path
        || Path::new(path)
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(VerificationError::new(
            "release_archive_invalid",
            "archive member path is absolute, noncanonical, or platform-prefixed",
        ));
    }
    Ok(())
}

fn read_operand(
    path: &Path,
    limit: u64,
    code: &'static str,
    started: Instant,
    deadline: Duration,
) -> Result<Vec<u8>, VerificationError> {
    let mut file = open_operand(path, limit, code, started, deadline)?;
    let identity = file_identity(&file, code)?;
    let mut bytes = Vec::with_capacity(identity.size as usize);
    file.by_ref()
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| VerificationError::new(code, error))?;
    check_deadline(started, deadline)?;
    if bytes.len() as u64 > limit || file_identity(&file, code)? != identity {
        return Err(VerificationError::new(
            code,
            "operand exceeds its byte limit or changed during verification",
        ));
    }
    Ok(bytes)
}

fn open_operand(
    path: &Path,
    limit: u64,
    code: &'static str,
    started: Instant,
    deadline: Duration,
) -> Result<File, VerificationError> {
    check_deadline(started, deadline)?;
    if !path.is_absolute() {
        return Err(VerificationError::new(
            code,
            "operand path must be absolute",
        ));
    }
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
        .map_err(|error| VerificationError::new(code, error))?;
    check_deadline(started, deadline)?;
    let metadata = file
        .metadata()
        .map_err(|error| VerificationError::new(code, error))?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(VerificationError::new(
            code,
            "operand must be a regular file within its byte limit",
        ));
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|error| VerificationError::new(code, error))?;
    Ok(file)
}

fn file_identity(file: &File, code: &'static str) -> Result<OperandIdentity, VerificationError> {
    let metadata = file
        .metadata()
        .map_err(|error| VerificationError::new(code, error))?;
    Ok(OperandIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
        size: metadata.len(),
        modified_seconds: metadata.mtime(),
        modified_nanoseconds: metadata.mtime_nsec(),
    })
}

fn check_deadline(started: Instant, deadline: Duration) -> Result<(), VerificationError> {
    if started.elapsed() >= deadline {
        return Err(VerificationError::new(
            "deadline_expired",
            "deadline expired during offline release verification",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{io::Write, path::PathBuf};

    use base64::{Engine, engine::general_purpose::STANDARD};
    use flate2::{Compression, write::GzEncoder};
    use tar::{Builder, EntryType, Header};

    use super::*;

    fn package_member(path: &str, bytes: &[u8]) -> PackageMemberV1 {
        PackageMemberV1 {
            path: path.to_string(),
            mode: 0o755,
            size: bytes.len() as u64,
            digest: AuthorityKey::from_bytes(Sha256::digest(bytes).into()),
        }
    }

    fn archive_bytes(pax: bool, symlink: bool) -> Vec<u8> {
        let encoder = GzEncoder::new(Vec::new(), Compression::default());
        let mut archive = Builder::new(encoder);
        if pax {
            archive
                .append_pax_extensions([("comment", b"forbidden".as_slice())])
                .unwrap();
        }
        for (path, bytes) in [
            ("omegon", b"agent".as_slice()),
            ("omegon-maintain", b"maintain"),
        ] {
            let mut header = Header::new_ustar();
            header.set_mode(0o755);
            if symlink && path == "omegon" {
                header.set_entry_type(EntryType::Symlink);
                header.set_size(0);
                header.set_cksum();
                archive
                    .append_link(&mut header, path, "omegon-maintain")
                    .unwrap();
            } else {
                header.set_entry_type(EntryType::Regular);
                header.set_size(bytes.len() as u64);
                header.set_cksum();
                archive.append_data(&mut header, path, bytes).unwrap();
            }
        }
        archive.into_inner().unwrap().finish().unwrap()
    }

    fn verify_archive(bytes: &[u8]) -> Result<(), VerificationError> {
        verify_archive_with_members(
            bytes,
            &[
                package_member("omegon", b"agent"),
                package_member("omegon-maintain", b"maintain"),
            ],
        )
    }

    fn verify_archive_with_members(
        bytes: &[u8],
        members: &[PackageMemberV1],
    ) -> Result<(), VerificationError> {
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(bytes).unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        verify_archive_members(file, members, Instant::now(), Duration::from_secs(5))
    }

    fn incomplete_bundle() -> serde_json::Value {
        json!({
            "mediaType": OFFICIAL_BUNDLE_V03,
            "verificationMaterial": {
                "certificate": { "rawBytes": "" },
                "tlogEntries": [],
                "timestampVerificationData": { "rfc3161Timestamps": [] }
            },
            "messageSignature": {
                "messageDigest": { "algorithm": "SHA2_256", "digest": "" },
                "signature": ""
            }
        })
    }

    fn parse_test_bundle(value: serde_json::Value) -> Bundle {
        Bundle::from_json(&value.to_string()).unwrap()
    }

    fn production_fixture() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
        let encoded = include_str!("../tests/fixtures/release-verifier-v1.tar.gz.b64")
            .split_whitespace()
            .collect::<String>();
        let bytes = STANDARD.decode(encoded).unwrap();
        let decoder = GzDecoder::new(bytes.as_slice());
        let mut container = tar::Archive::new(decoder);
        let directory = tempfile::tempdir().unwrap();
        let mut files = BTreeMap::new();
        for entry in container.entries().unwrap() {
            let mut entry = entry.unwrap();
            let name = entry.path().unwrap().to_str().unwrap().to_string();
            if name.starts_with("._") {
                continue;
            }
            assert!(
                name.ends_with(".tar.gz")
                    || name.ends_with(".manifest.json")
                    || name.ends_with(".manifest.sigstore.json")
            );
            let path = directory.path().join(&name);
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            std::fs::write(&path, bytes).unwrap();
            files.insert(name, path);
        }
        let prefix = "omegon-0.29.0-dev-fixture.1-x86_64-unknown-linux-gnu.tar.gz";
        let archive = files.remove(prefix).unwrap();
        let manifest = files.remove(&format!("{prefix}.manifest.json")).unwrap();
        let bundle = files
            .remove(&format!("{prefix}.manifest.sigstore.json"))
            .unwrap();
        assert!(files.is_empty(), "unexpected fixture files: {files:?}");
        (directory, archive, manifest, bundle)
    }

    fn mutated_fixture_bundle(
        mutate: impl FnOnce(&mut serde_json::Value),
    ) -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
        let (directory, archive, manifest, bundle) = production_fixture();
        let mut value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&bundle).unwrap()).unwrap();
        mutate(&mut value);
        std::fs::write(&bundle, serde_json::to_vec(&value).unwrap()).unwrap();
        (directory, archive, manifest, bundle)
    }

    fn verify_fixture_paths(
        archive: &Path,
        manifest: &Path,
        bundle: &Path,
    ) -> Result<serde_json::Value, VerificationError> {
        verify_release_inner(
            archive,
            manifest,
            bundle,
            Instant::now(),
            Duration::from_secs(30),
        )
    }

    #[test]
    fn member_paths_reject_confusion_forms() {
        for path in [
            "",
            "/omegon",
            "../omegon",
            "a/../omegon",
            "C:omegon",
            "a\\b",
        ] {
            assert!(validate_member_path(path).is_err(), "accepted {path}");
        }
        assert!(validate_member_path("omegon-maintain").is_ok());
    }

    #[test]
    fn compiled_policy_is_exact() {
        let manifest: PackageManifestV1 = parse_record(include_bytes!(
            "../../omegon-maintenance-contracts/tests/fixtures/package-manifest-v1.json"
        ))
        .unwrap();
        let path = Path::new("/tmp/omegon-0.29.0-dev-x86_64-unknown-linux-gnu.tar.gz");
        assert!(verify_compiled_policy(&manifest, path).is_err());
    }

    #[test]
    fn legacy_fixture_composition_exception_is_immutable() {
        let (_directory, _archive, manifest_path, _bundle) = production_fixture();
        let manifest: PackageManifestV1 =
            parse_record(&std::fs::read(manifest_path).unwrap()).unwrap();
        assert!(is_pinned_legacy_fixture(&manifest));

        let mut changed = manifest.clone();
        changed.tag = "v0.29.0-dev-fixture.2".into();
        assert!(!is_pinned_legacy_fixture(&changed));
        let mut changed = manifest.clone();
        changed.archive_digest = AuthorityKey::from_bytes([9; 32]);
        assert!(!is_pinned_legacy_fixture(&changed));
        let mut changed = manifest;
        changed.members.push(package_member("unexpected", b"value"));
        assert!(!is_pinned_legacy_fixture(&changed));
    }

    #[test]
    fn package_composition_lock_set_rejects_unknown_and_duplicate_identity() {
        let (_directory, _archive, manifest_path, _bundle) = production_fixture();
        let mut manifest: PackageManifestV1 =
            parse_record(&std::fs::read(manifest_path).unwrap()).unwrap();
        let target = manifest.target.clone();
        let digest = AuthorityKey::from_bytes([7; 32]);
        let lock = |identity: &str,
                    artifact_path: &str,
                    required: bool,
                    resident_lock_path: Option<&str>| {
            omegon_maintenance_contracts::ArtifactCompositionLockV1 {
                identity: identity.into(),
                artifact_path: artifact_path.into(),
                artifact_digest: digest,
                protocol_minimum: 1,
                protocol_maximum: 1,
                targets: vec![target.clone()],
                required,
                fallback: if required {
                    "fail_closed".into()
                } else {
                    "typed_unavailable".into()
                },
                resident_lock_path: resident_lock_path.map(str::to_string),
            }
        };
        manifest.composition_locks = vec![
            lock(
                "executable:omegon",
                "omegon",
                true,
                Some("omegon.composition-lock.json"),
            ),
            lock(
                "executable:omegon-maintain",
                "omegon-maintain",
                true,
                Some("omegon-maintain.composition-lock.json"),
            ),
            lock("content-pack:omegon-shipped", CONTENT_MANIFEST, false, None),
        ];
        validate_exact_package_lock_set(&manifest).unwrap();

        let mut malformed = manifest.clone();
        malformed.composition_locks[2].identity = "executable:omegon".into();
        assert!(validate_exact_package_lock_set(&malformed).is_err());
        let mut malformed = manifest;
        malformed.composition_locks[0].resident_lock_path = Some("arbitrary.json".into());
        assert!(validate_exact_package_lock_set(&malformed).is_err());
    }

    #[test]
    fn modern_full_product_manifest_requires_product_component_lock() {
        let (_directory, archive, manifest_path, _bundle) = production_fixture();
        let mut manifest: PackageManifestV1 =
            parse_record(&std::fs::read(manifest_path).unwrap()).unwrap();
        manifest.tag = "v1.2.3".into();
        manifest.host_profile = Some("full-product".into());
        manifest.composition_class = Some("full-product".into());
        manifest.core_components = Some(vec![
            omegon_maintenance_contracts::ProductComponentInventoryV1 {
                component_id: "core:codescan".into(),
                wire_manifest_id: "omegon-codescan".into(),
            },
        ]);
        manifest.product_component_locks.clear();

        let error = verify_product_component_lock(
            File::open(archive).unwrap(),
            &manifest,
            Instant::now(),
            Duration::from_secs(5),
        )
        .expect_err("full-product inventory requires signed component evidence");
        assert_eq!(error.code, "release_component_lock_invalid");
    }

    #[test]
    fn resident_composition_lock_set_rejects_duplicates_and_unknowns() {
        let contribution = omegon_maintenance_contracts::ResidentContributionLockV1 {
            identity: "system:maintenance-kernel".into(),
            artifact_path: "omegon-maintain".into(),
            artifact_digest: AuthorityKey::from_bytes([3; 32]),
            protocol_minimum: 1,
            protocol_maximum: 1,
            targets: vec!["x86_64-unknown-linux-gnu".into()],
            required: true,
            fallback: "fail_closed".into(),
            state: "resident".into(),
        };
        let mut lock = ResidentCompositionLockV1 {
            schema_version: 1,
            executable_identity: "omegon-maintain".into(),
            executable_digest: contribution.artifact_digest,
            target: "x86_64-unknown-linux-gnu".into(),
            protocol_minimum: 1,
            protocol_maximum: 1,
            contributions: vec![contribution.clone()],
            signing_identity: omegon_maintenance_contracts::SigningIdentityV1 {
                issuer: EXPECTED_ISSUER.into(),
                workflow_identity: "test".into(),
                verification: "required".into(),
            },
        };
        validate_exact_resident_set(&lock).unwrap();
        lock.contributions.push(contribution);
        assert!(validate_exact_resident_set(&lock).is_err());
        lock.contributions.truncate(1);
        lock.contributions[0].identity = "system:unknown".into();
        assert!(validate_exact_resident_set(&lock).is_err());
    }

    #[test]
    fn required_resident_lock_failure_is_fail_closed_before_optional_inventory() {
        let required = omegon_maintenance_contracts::ResidentContributionLockV1 {
            identity: "system:kernel".into(),
            artifact_path: "omegon".into(),
            artifact_digest: AuthorityKey::from_bytes([1; 32]),
            protocol_minimum: 2,
            protocol_maximum: 1,
            targets: vec!["x86_64-unknown-linux-gnu".into()],
            required: true,
            fallback: "fail_closed".into(),
            state: "resident".into(),
        };
        let lock = ResidentCompositionLockV1 {
            schema_version: 1,
            executable_identity: "omegon".into(),
            executable_digest: AuthorityKey::from_bytes([1; 32]),
            target: "x86_64-unknown-linux-gnu".into(),
            protocol_minimum: 1,
            protocol_maximum: 1,
            contributions: vec![required.clone()],
            signing_identity: omegon_maintenance_contracts::SigningIdentityV1 {
                issuer: EXPECTED_ISSUER.into(),
                workflow_identity: "test".into(),
                verification: "required".into(),
            },
        };
        assert_eq!(
            validate_resident_contribution(&required, &lock, true)
                .unwrap_err()
                .code,
            "release_composition_lock_invalid"
        );
    }

    #[test]
    fn optional_resident_lock_requires_typed_unavailable_fallback() {
        let optional = omegon_maintenance_contracts::ResidentContributionLockV1 {
            identity: "feature:memory".into(),
            artifact_path: "omegon".into(),
            artifact_digest: AuthorityKey::from_bytes([2; 32]),
            protocol_minimum: 1,
            protocol_maximum: 1,
            targets: vec!["x86_64-unknown-linux-gnu".into()],
            required: false,
            fallback: "typed_unavailable".into(),
            state: "resident_optional".into(),
        };
        let lock = ResidentCompositionLockV1 {
            schema_version: 1,
            executable_identity: "omegon".into(),
            executable_digest: AuthorityKey::from_bytes([2; 32]),
            target: "x86_64-unknown-linux-gnu".into(),
            protocol_minimum: 1,
            protocol_maximum: 1,
            contributions: vec![optional.clone()],
            signing_identity: omegon_maintenance_contracts::SigningIdentityV1 {
                issuer: EXPECTED_ISSUER.into(),
                workflow_identity: "test".into(),
                verification: "required".into(),
            },
        };
        validate_resident_contribution(&optional, &lock, false).unwrap();
    }

    #[test]
    fn archive_accepts_exact_regular_members() {
        verify_archive(&archive_bytes(false, false)).unwrap();
    }

    #[test]
    fn archive_rejects_links_and_extension_headers() {
        assert_eq!(
            verify_archive(&archive_bytes(false, true))
                .unwrap_err()
                .code,
            "release_archive_invalid"
        );
        assert_eq!(
            verify_archive(&archive_bytes(true, false))
                .unwrap_err()
                .code,
            "release_archive_invalid"
        );
    }

    #[test]
    fn archive_rejects_additional_gzip_member() {
        let bytes = archive_bytes(false, false);
        let mut concatenated = bytes.clone();
        concatenated.extend_from_slice(&bytes);
        assert_eq!(
            verify_archive(&concatenated).unwrap_err().code,
            "release_archive_invalid"
        );
    }

    #[test]
    fn archive_rejects_missing_or_corrupt_companions() {
        let bytes = archive_bytes(false, false);
        assert_eq!(
            verify_archive_with_members(&bytes, &[package_member("omegon", b"agent")])
                .unwrap_err()
                .code,
            "release_archive_invalid"
        );
        assert_eq!(
            verify_archive_with_members(
                &bytes,
                &[
                    package_member("omegon", b"agent"),
                    package_member("omegon-maintain", b"corrupt!"),
                ],
            )
            .unwrap_err()
            .code,
            "release_archive_digest_mismatch"
        );
    }

    #[test]
    fn bundle_profile_rejects_wrong_media_and_material() {
        let mut value = incomplete_bundle();
        value["mediaType"] = json!("application/vnd.dev.sigstore.bundle.v0.2+json");
        assert_eq!(
            verify_bundle_profile(&parse_test_bundle(value))
                .unwrap_err()
                .code,
            "release_bundle_invalid"
        );

        let mut value = incomplete_bundle();
        value["verificationMaterial"]
            .as_object_mut()
            .unwrap()
            .remove("certificate");
        value["verificationMaterial"]["publicKey"] = json!({ "hint": "test" });
        assert_eq!(
            verify_bundle_profile(&parse_test_bundle(value))
                .unwrap_err()
                .code,
            "release_bundle_invalid"
        );
    }

    #[test]
    fn bundle_profile_rejects_wrong_digest_and_missing_transparency() {
        let mut value = incomplete_bundle();
        value["messageSignature"]["messageDigest"]["algorithm"] = json!("SHA2_384");
        assert_eq!(
            verify_bundle_profile(&parse_test_bundle(value))
                .unwrap_err()
                .code,
            "release_bundle_invalid"
        );

        assert_eq!(
            verify_bundle_profile(&parse_test_bundle(incomplete_bundle()))
                .unwrap_err()
                .code,
            "release_transparency_invalid"
        );
    }

    #[test]
    fn production_fixture_verifies() {
        let (_directory, archive, manifest, bundle) = production_fixture();
        verify_release_inner(
            &archive,
            &manifest,
            &bundle,
            Instant::now(),
            Duration::from_secs(30),
        )
        .unwrap();
    }

    #[test]
    fn production_fixture_rejects_manifest_and_archive_corruption() {
        let (_directory, archive, manifest, bundle) = production_fixture();
        let mut manifest_bytes = std::fs::read(&manifest).unwrap();
        let commit = manifest_bytes
            .windows(40)
            .position(|window| window == b"d4ee9a6bfd500052fb52419e87af7b750321b35f")
            .unwrap();
        manifest_bytes[commit] = b'e';
        std::fs::write(&manifest, manifest_bytes).unwrap();
        assert_eq!(
            verify_release_inner(
                &archive,
                &manifest,
                &bundle,
                Instant::now(),
                Duration::from_secs(30),
            )
            .unwrap_err()
            .code,
            "release_signature_invalid"
        );

        let (_directory, archive, manifest, bundle) = production_fixture();
        let mut archive_bytes = std::fs::read(&archive).unwrap();
        archive_bytes[0] ^= 1;
        std::fs::write(&archive, archive_bytes).unwrap();
        assert_eq!(
            verify_release_inner(
                &archive,
                &manifest,
                &bundle,
                Instant::now(),
                Duration::from_secs(30),
            )
            .unwrap_err()
            .code,
            "release_archive_digest_mismatch"
        );
    }

    #[test]
    fn production_fixture_requires_set_and_inclusion_proof() {
        for field in ["inclusionPromise", "inclusionProof"] {
            let (_directory, archive, manifest, bundle) = production_fixture();
            let mut value: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&bundle).unwrap()).unwrap();
            value["verificationMaterial"]["tlogEntries"][0]
                .as_object_mut()
                .unwrap()
                .remove(field);
            std::fs::write(&bundle, serde_json::to_vec(&value).unwrap()).unwrap();
            assert_eq!(
                verify_release_inner(
                    &archive,
                    &manifest,
                    &bundle,
                    Instant::now(),
                    Duration::from_secs(30),
                )
                .unwrap_err()
                .code,
                "release_transparency_invalid"
            );
        }
    }

    #[test]
    fn production_fixture_rejects_wrong_ref_and_checkpoint() {
        let (_directory, archive, manifest, bundle) = production_fixture();
        let mut manifest_bytes = std::fs::read(&manifest).unwrap();
        let git_ref = manifest_bytes
            .windows(31)
            .position(|window| window == b"refs/tags/v0.29.0-dev-fixture.1")
            .unwrap();
        manifest_bytes[git_ref + 30] = b'2';
        std::fs::write(&manifest, manifest_bytes).unwrap();
        assert_eq!(
            verify_release_inner(
                &archive,
                &manifest,
                &bundle,
                Instant::now(),
                Duration::from_secs(30),
            )
            .unwrap_err()
            .code,
            "release_policy_mismatch"
        );

        let (_directory, archive, manifest, bundle) = production_fixture();
        let mut value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&bundle).unwrap()).unwrap();
        let checkpoint = value["verificationMaterial"]["tlogEntries"][0]["inclusionProof"]
            ["checkpoint"]["envelope"]
            .as_str()
            .unwrap();
        value["verificationMaterial"]["tlogEntries"][0]["inclusionProof"]["checkpoint"]["envelope"] =
            json!(format!("x{}", &checkpoint[1..]));
        std::fs::write(&bundle, serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(
            verify_release_inner(
                &archive,
                &manifest,
                &bundle,
                Instant::now(),
                Duration::from_secs(30),
            )
            .unwrap_err()
            .code,
            "release_transparency_invalid"
        );
    }

    #[test]
    fn production_fixture_rejects_timestamp_and_log_substitution() {
        let (_directory, archive, manifest, bundle) = mutated_fixture_bundle(|value| {
            value["verificationMaterial"]["timestampVerificationData"]["rfc3161Timestamps"] =
                json!([{ "signedTimestamp": "" }]);
        });
        assert_eq!(
            verify_fixture_paths(&archive, &manifest, &bundle)
                .unwrap_err()
                .code,
            "release_bundle_invalid"
        );

        let (_directory, archive, manifest, bundle) = mutated_fixture_bundle(|value| {
            value["verificationMaterial"]["tlogEntries"][0]["logId"]["keyId"] =
                json!("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=");
        });
        assert_eq!(
            verify_fixture_paths(&archive, &manifest, &bundle)
                .unwrap_err()
                .code,
            "release_transparency_invalid"
        );
    }

    #[test]
    fn production_fixture_rejects_tampered_proof_root() {
        let (_directory, archive, manifest, bundle) = mutated_fixture_bundle(|value| {
            value["verificationMaterial"]["tlogEntries"][0]["inclusionProof"]["rootHash"] =
                json!("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=");
        });
        assert_eq!(
            verify_fixture_paths(&archive, &manifest, &bundle)
                .unwrap_err()
                .code,
            "release_signature_invalid"
        );
    }
}
