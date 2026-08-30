//! Crash-safe publication and activation for release-coupled executables.

use std::fs::{self, File};
use std::path::{Component, Path, PathBuf};

const CODESCAN_COMPONENT_LOCK: &str = "share/omegon/components/core-codescan.lock.json";
const CODESCAN_MANIFEST: &str = "share/omegon/extensions/omegon-codescan/manifest.toml";
const CODESCAN_EXECUTABLE: &str =
    "share/omegon/extensions/omegon-codescan/target/release/omegon-codescan";
const CONTENT_MANIFEST: &str = "share/omegon/content-packs/omegon-shipped/content-pack.toml";

#[derive(Debug, Clone)]
pub(crate) struct InstalledReleaseLayout {
    pub(crate) versions_root: PathBuf,
    pub(crate) current_link: PathBuf,
    pub(crate) binary_link: PathBuf,
    pub(crate) om_link: PathBuf,
    pub(crate) maintenance_link: PathBuf,
    pub(crate) receipt_link: PathBuf,
}

impl InstalledReleaseLayout {
    pub(crate) fn new(
        versions_root: PathBuf,
        binary_link: PathBuf,
        maintenance_link: PathBuf,
        receipt_link: PathBuf,
    ) -> anyhow::Result<Self> {
        let release_root = versions_root
            .parent()
            .ok_or_else(|| anyhow::anyhow!("versions directory has no parent"))?
            .to_path_buf();
        let install_dir = binary_link
            .parent()
            .ok_or_else(|| anyhow::anyhow!("binary launcher has no parent"))?;
        Ok(Self {
            versions_root,
            current_link: release_root.join("current"),
            om_link: install_dir.join("om"),
            binary_link,
            maintenance_link,
            receipt_link,
        })
    }

    pub(crate) fn generation_dir(&self, version: &str) -> anyhow::Result<PathBuf> {
        validate_version_component(version)?;
        Ok(self.versions_root.join(version))
    }

    pub(crate) fn prepare_stable_links(&self) -> anyhow::Result<()> {
        atomic_replace_symlink(&self.binary_link, &self.current_link.join("omegon"))?;
        atomic_replace_symlink(&self.om_link, &self.current_link.join("omegon"))?;
        atomic_replace_symlink(
            &self.maintenance_link,
            &self.current_link.join("omegon-maintain"),
        )?;
        atomic_replace_symlink(
            &self.receipt_link,
            &self.current_link.join("install-receipt.json"),
        )
    }

    pub(crate) fn publish_generation(
        &self,
        staging_dir: &Path,
        version: &str,
    ) -> anyhow::Result<PathBuf> {
        let destination = self.generation_dir(version)?;
        validate_release_coupled_generation(staging_dir)?;
        fs::create_dir_all(&self.versions_root)?;
        sync_generation(staging_dir)?;
        match fs::symlink_metadata(&destination) {
            Ok(_) => {
                validate_release_coupled_generation(&destination)?;
                fs::remove_dir_all(staging_dir)?;
                return Ok(destination);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        fs::rename(staging_dir, &destination)?;
        sync_directory(&self.versions_root)?;
        Ok(destination)
    }

    pub(crate) fn activate(&self, generation: &Path) -> anyhow::Result<()> {
        validate_release_coupled_generation(generation)?;
        if generation.parent() != Some(self.versions_root.as_path()) {
            anyhow::bail!("release generation is outside the version store");
        }
        atomic_replace_symlink(&self.current_link, generation)
    }

    pub(crate) fn active_generation(&self) -> anyhow::Result<Option<PathBuf>> {
        match fs::read_link(&self.current_link) {
            Ok(target) if target.is_absolute() => Ok(Some(target)),
            Ok(target) => Ok(Some(
                self.current_link
                    .parent()
                    .ok_or_else(|| anyhow::anyhow!("activation link has no parent"))?
                    .join(target),
            )),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
}

pub(crate) fn validate_version_component(version: &str) -> anyhow::Result<()> {
    let mut components = Path::new(version).components();
    if version.is_empty()
        || !matches!(components.next(), Some(Component::Normal(_)))
        || components.next().is_some()
        || version == "."
        || version == ".."
        || version.contains(['/', '\\'])
    {
        anyhow::bail!("release version is not a safe path component: {version}");
    }
    Ok(())
}

pub(crate) fn validate_generation(path: &Path) -> anyhow::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        anyhow::bail!(
            "release generation is not a real directory: {}",
            path.display()
        );
    }
    for name in ["omegon", "omegon-maintain", "install-receipt.json"] {
        let member = path.join(name);
        if !fs::metadata(&member).is_ok_and(|metadata| metadata.is_file()) {
            anyhow::bail!("release generation is missing {name}: {}", path.display());
        }
    }
    Ok(())
}

pub(crate) fn validate_release_coupled_generation(path: &Path) -> anyhow::Result<()> {
    validate_generation(path)?;
    for name in [
        "omegon.composition-lock.json",
        "omegon-maintain.composition-lock.json",
        CONTENT_MANIFEST,
        CODESCAN_MANIFEST,
        CODESCAN_EXECUTABLE,
        CODESCAN_COMPONENT_LOCK,
    ] {
        let member = path.join(name);
        if !fs::metadata(&member).is_ok_and(|metadata| metadata.is_file()) {
            anyhow::bail!("release generation is missing {name}: {}", path.display());
        }
    }
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(path.join("install-receipt.json"))?)?;
    if receipt["layout"] != "versioned-current-v1"
        || receipt["version"].as_str().is_none_or(str::is_empty)
    {
        anyhow::bail!(
            "release generation receipt is incompatible: {}",
            path.display()
        );
    }
    for name in ["omegon", "omegon-maintain", CODESCAN_EXECUTABLE] {
        validate_executable(&path.join(name))?;
    }
    validate_resident_lock(path, "omegon", "omegon")?;
    validate_resident_lock(path, "omegon-maintain", "omegon-maintain")?;
    validate_product_component(path)
}

fn validate_resident_lock(path: &Path, executable: &str, identity: &str) -> anyhow::Result<()> {
    use sha2::{Digest, Sha256};

    let lock_path = path.join(format!("{executable}.composition-lock.json"));
    let lock: omegon_maintenance_contracts::ResidentCompositionLockV1 =
        serde_json::from_slice(&fs::read(lock_path)?)?;
    let actual = omegon_maintenance_contracts::AuthorityKey::from_bytes(
        Sha256::digest(fs::read(path.join(executable))?).into(),
    );
    if lock.schema_version != 1
        || lock.executable_identity != identity
        || lock.executable_digest != actual
        || lock.target != compiled_target()
        || lock.protocol_minimum == 0
        || lock.protocol_minimum > lock.protocol_maximum
        || lock.signing_identity.issuer != "https://token.actions.githubusercontent.com"
        || lock.signing_identity.verification != "required"
        || !lock.signing_identity.workflow_identity.starts_with(
            "https://github.com/styrene-lab/omegon/.github/workflows/release.yml@refs/tags/v",
        )
    {
        anyhow::bail!("resident composition lock is incompatible for {identity}");
    }
    Ok(())
}

#[cfg(unix)]
fn validate_executable(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    if fs::metadata(path)?.permissions().mode() & 0o111 == 0 {
        anyhow::bail!(
            "release generation member is not executable: {}",
            path.display()
        );
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_executable(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

pub(crate) fn validate_product_component(path: &Path) -> anyhow::Result<()> {
    use sha2::{Digest, Sha256};

    let lock: omegon_maintenance_contracts::ProductComponentLockV1 =
        serde_json::from_slice(&fs::read(path.join(CODESCAN_COMPONENT_LOCK))?)?;
    if lock.schema_version != 1
        || lock.component_id != "core:codescan"
        || lock.wire_manifest_id != "omegon-codescan"
        || lock.manifest_path != "share/omegon/extensions/omegon-codescan/manifest.toml"
        || lock.executable_path
            != "share/omegon/extensions/omegon-codescan/target/release/omegon-codescan"
        || lock.protocol_minimum != 1
        || lock.protocol_maximum != 1
        || lock.protocol_version != u32::from(omegon_codescan_contracts::CODESCAN_PROTOCOL_VERSION)
        || lock.fallback != "typed_unavailable"
        || lock.target != compiled_target()
        || lock.signing_identity.issuer != "https://token.actions.githubusercontent.com"
        || lock.signing_identity.verification != "required"
        || !lock.signing_identity.workflow_identity.starts_with(
            "https://github.com/styrene-lab/omegon/.github/workflows/release.yml@refs/tags/v",
        )
    {
        anyhow::bail!("release-coupled codescan component lock is incompatible");
    }
    for (relative, expected) in [
        (&lock.manifest_path, lock.manifest_digest),
        (&lock.executable_path, lock.executable_digest),
    ] {
        let actual = omegon_maintenance_contracts::AuthorityKey::from_bytes(
            Sha256::digest(fs::read(path.join(relative))?).into(),
        );
        if actual != expected {
            anyhow::bail!("release-coupled codescan component bytes were substituted");
        }
    }
    Ok(())
}

pub(crate) fn compiled_target() -> &'static str {
    if cfg!(all(target_arch = "aarch64", target_os = "macos")) {
        "aarch64-apple-darwin"
    } else if cfg!(all(target_arch = "x86_64", target_os = "macos")) {
        "x86_64-apple-darwin"
    } else if cfg!(all(target_arch = "aarch64", target_os = "linux")) {
        "aarch64-unknown-linux-gnu"
    } else if cfg!(all(
        target_arch = "x86_64",
        target_os = "linux",
        target_env = "musl"
    )) {
        "x86_64-unknown-linux-musl"
    } else if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
        "x86_64-unknown-linux-gnu"
    } else {
        "unsupported"
    }
}

fn sync_generation(path: &Path) -> anyhow::Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let member = entry.path();
        let metadata = fs::symlink_metadata(&member)?;
        if metadata.file_type().is_symlink() {
            anyhow::bail!(
                "release generation contains a symlink: {}",
                member.display()
            );
        }
        if metadata.is_dir() {
            sync_generation(&member)?;
        } else if metadata.is_file() {
            File::open(member)?.sync_all()?;
        }
    }
    sync_directory(path)
}

fn sync_directory(path: &Path) -> anyhow::Result<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(unix)]
pub(crate) fn atomic_replace_symlink(link: &Path, target: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::symlink;

    let parent = link
        .parent()
        .ok_or_else(|| anyhow::anyhow!("activation link has no parent"))?;
    fs::create_dir_all(parent)?;
    if let Ok(metadata) = fs::symlink_metadata(link)
        && metadata.is_dir()
        && !metadata.file_type().is_symlink()
    {
        anyhow::bail!(
            "refusing to replace directory with symlink: {}",
            link.display()
        );
    }
    let temp = parent.join(format!(
        ".{}.tmp-{}-{}",
        link.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("link"),
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    symlink(target, &temp)?;
    if let Err(error) = fs::rename(&temp, link) {
        let _ = fs::remove_file(&temp);
        return Err(error.into());
    }
    sync_directory(parent)
}

#[cfg(not(unix))]
pub(crate) fn atomic_replace_symlink(_link: &Path, _target: &Path) -> anyhow::Result<()> {
    anyhow::bail!("atomic release activation is unsupported on this platform")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_generation(path: &Path, version: &str) {
        use sha2::{Digest, Sha256};

        fs::create_dir_all(path).unwrap();
        fs::write(path.join("omegon"), version).unwrap();
        fs::write(path.join("omegon-maintain"), version).unwrap();
        fs::write(
            path.join("install-receipt.json"),
            format!("{{\"version\":\"{version}\",\"layout\":\"versioned-current-v1\"}}"),
        )
        .unwrap();
        let signing_identity = serde_json::json!({
            "issuer": "https://token.actions.githubusercontent.com",
            "workflow_identity": "https://github.com/styrene-lab/omegon/.github/workflows/release.yml@refs/tags/v1.0.0",
            "verification": "required"
        });
        for (executable, identity) in [("omegon", "omegon"), ("omegon-maintain", "omegon-maintain")]
        {
            let digest = omegon_maintenance_contracts::AuthorityKey::from_bytes(
                Sha256::digest(version.as_bytes()).into(),
            );
            fs::write(
                path.join(format!("{executable}.composition-lock.json")),
                serde_json::to_vec(&serde_json::json!({
                    "schema_version": 1,
                    "executable_identity": identity,
                    "executable_digest": digest,
                    "target": compiled_target(),
                    "protocol_minimum": 1,
                    "protocol_maximum": 1,
                    "contributions": [],
                    "signing_identity": signing_identity.clone()
                }))
                .unwrap(),
            )
            .unwrap();
        }
        let manifest = b"name = \"omegon-codescan\"\n";
        let codescan = format!("codescan-{version}").into_bytes();
        for (relative, bytes) in [
            (CONTENT_MANIFEST, b"id = \"omegon-shipped\"\n".as_slice()),
            (CODESCAN_MANIFEST, manifest.as_slice()),
            (CODESCAN_EXECUTABLE, codescan.as_slice()),
        ] {
            let member = path.join(relative);
            fs::create_dir_all(member.parent().unwrap()).unwrap();
            fs::write(member, bytes).unwrap();
        }
        let component_lock = serde_json::json!({
            "schema_version": 1,
            "component_id": "core:codescan",
            "wire_manifest_id": "omegon-codescan",
            "manifest_path": CODESCAN_MANIFEST,
            "manifest_digest": omegon_maintenance_contracts::AuthorityKey::from_bytes(Sha256::digest(manifest).into()),
            "executable_path": CODESCAN_EXECUTABLE,
            "executable_digest": omegon_maintenance_contracts::AuthorityKey::from_bytes(Sha256::digest(&codescan).into()),
            "target": compiled_target(),
            "protocol_minimum": 1,
            "protocol_maximum": 1,
            "protocol_version": 1,
            "fallback": "typed_unavailable",
            "signing_identity": signing_identity
        });
        let component_lock_path = path.join(CODESCAN_COMPONENT_LOCK);
        fs::create_dir_all(component_lock_path.parent().unwrap()).unwrap();
        fs::write(
            component_lock_path,
            serde_json::to_vec(&component_lock).unwrap(),
        )
        .unwrap();
        #[cfg(unix)]
        for executable in ["omegon", "omegon-maintain", CODESCAN_EXECUTABLE] {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path.join(executable), fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    fn layout(root: &Path) -> InstalledReleaseLayout {
        InstalledReleaseLayout::new(
            root.join("store/versions"),
            root.join("bin/omegon"),
            root.join("bin/omegon-maintain"),
            root.join("config/install-receipt.json"),
        )
        .unwrap()
    }

    #[test]
    fn publication_does_not_change_active_generation() {
        let temp = tempfile::tempdir().unwrap();
        let layout = layout(temp.path());
        let old = layout.generation_dir("1.0.0").unwrap();
        write_generation(&old, "1.0.0");
        layout.activate(&old).unwrap();
        layout.prepare_stable_links().unwrap();

        let staging = layout.versions_root.join(".2.0.0.staging");
        write_generation(&staging, "2.0.0");
        let published = layout.publish_generation(&staging, "2.0.0").unwrap();

        assert_eq!(layout.active_generation().unwrap(), Some(old));
        assert_eq!(fs::read_to_string(&layout.binary_link).unwrap(), "1.0.0");
        assert_eq!(
            fs::read_to_string(&layout.maintenance_link).unwrap(),
            "1.0.0"
        );
        assert!(published.is_dir());
    }

    #[test]
    fn one_activation_switches_pair_and_receipt_together() {
        let temp = tempfile::tempdir().unwrap();
        let layout = layout(temp.path());
        let old = layout.generation_dir("1.0.0").unwrap();
        let new = layout.generation_dir("2.0.0").unwrap();
        write_generation(&old, "1.0.0");
        write_generation(&new, "2.0.0");
        layout.activate(&old).unwrap();
        layout.prepare_stable_links().unwrap();
        layout.activate(&new).unwrap();

        assert_eq!(fs::read_to_string(&layout.binary_link).unwrap(), "2.0.0");
        assert_eq!(fs::read_to_string(&layout.om_link).unwrap(), "2.0.0");
        assert_eq!(
            fs::read_to_string(&layout.maintenance_link).unwrap(),
            "2.0.0"
        );
        assert!(
            fs::read_to_string(&layout.receipt_link)
                .unwrap()
                .contains("2.0.0")
        );
        assert_eq!(fs::read_to_string(old.join("omegon")).unwrap(), "1.0.0");
        assert_eq!(
            fs::read_to_string(new.join(CODESCAN_EXECUTABLE)).unwrap(),
            "codescan-2.0.0"
        );

        layout.activate(&old).unwrap();
        assert_eq!(fs::read_to_string(&layout.binary_link).unwrap(), "1.0.0");
        assert_eq!(
            fs::read_to_string(layout.current_link.join(CODESCAN_EXECUTABLE)).unwrap(),
            "codescan-1.0.0"
        );
    }

    #[test]
    fn failed_candidate_cleanup_is_generation_scoped_and_preserves_operator_extension() {
        let temp = tempfile::tempdir().unwrap();
        let layout = layout(temp.path());
        let old = layout.generation_dir("1.0.0").unwrap();
        write_generation(&old, "1.0.0");
        layout.activate(&old).unwrap();
        layout.prepare_stable_links().unwrap();

        let operator_extension = temp
            .path()
            .join("store/extensions/omegon-codescan/operator-sentinel");
        fs::create_dir_all(operator_extension.parent().unwrap()).unwrap();
        fs::write(&operator_extension, "operator-owned").unwrap();

        let candidate = layout.versions_root.join(".2.0.0.update");
        write_generation(&candidate, "2.0.0");
        fs::write(candidate.join(CODESCAN_EXECUTABLE), "substituted").unwrap();
        assert!(validate_release_coupled_generation(&candidate).is_err());
        fs::remove_dir_all(&candidate).unwrap();

        assert_eq!(fs::read_to_string(&layout.binary_link).unwrap(), "1.0.0");
        assert_eq!(
            fs::read_to_string(layout.current_link.join(CODESCAN_EXECUTABLE)).unwrap(),
            "codescan-1.0.0"
        );
        assert_eq!(
            fs::read_to_string(operator_extension).unwrap(),
            "operator-owned"
        );
    }

    #[test]
    fn interrupted_launcher_migration_keeps_every_surface_on_old_generation() {
        for completed_boundaries in 0..=4 {
            let temp = tempfile::tempdir().unwrap();
            let layout = layout(temp.path());
            let old = layout.generation_dir("1.0.0").unwrap();
            write_generation(&old, "1.0.0");
            layout.activate(&old).unwrap();
            fs::create_dir_all(layout.binary_link.parent().unwrap()).unwrap();
            fs::create_dir_all(layout.receipt_link.parent().unwrap()).unwrap();
            fs::copy(old.join("omegon"), &layout.binary_link).unwrap();
            fs::copy(old.join("omegon"), &layout.om_link).unwrap();
            fs::copy(old.join("omegon-maintain"), &layout.maintenance_link).unwrap();
            fs::copy(old.join("install-receipt.json"), &layout.receipt_link).unwrap();

            let boundaries = [
                (&layout.binary_link, layout.current_link.join("omegon")),
                (&layout.om_link, layout.current_link.join("omegon")),
                (
                    &layout.maintenance_link,
                    layout.current_link.join("omegon-maintain"),
                ),
                (
                    &layout.receipt_link,
                    layout.current_link.join("install-receipt.json"),
                ),
            ];
            for (link, target) in boundaries.iter().take(completed_boundaries) {
                atomic_replace_symlink(link, target).unwrap();
            }

            assert_eq!(fs::read_to_string(&layout.binary_link).unwrap(), "1.0.0");
            assert_eq!(fs::read_to_string(&layout.om_link).unwrap(), "1.0.0");
            assert_eq!(
                fs::read_to_string(&layout.maintenance_link).unwrap(),
                "1.0.0"
            );
            assert!(
                fs::read_to_string(&layout.receipt_link)
                    .unwrap()
                    .contains("1.0.0")
            );
        }
    }

    #[test]
    fn version_and_generation_validation_fail_closed() {
        for invalid in ["", ".", "..", "../escape", "a/b", "a\\b"] {
            assert!(validate_version_component(invalid).is_err(), "{invalid}");
        }
        let temp = tempfile::tempdir().unwrap();
        let incomplete = temp.path().join("incomplete");
        fs::create_dir(&incomplete).unwrap();
        fs::write(incomplete.join("omegon"), b"only one").unwrap();
        assert!(validate_generation(&incomplete).is_err());
    }

    #[test]
    fn activation_rejects_partial_full_product_generation_and_preserves_current() {
        let temp = tempfile::tempdir().unwrap();
        let layout = layout(temp.path());
        let old = layout.generation_dir("1.0.0").unwrap();
        let partial = layout.generation_dir("2.0.0").unwrap();
        write_generation(&old, "1.0.0");
        write_generation(&partial, "2.0.0");
        fs::remove_file(partial.join(CODESCAN_COMPONENT_LOCK)).unwrap();
        layout.activate(&old).unwrap();

        let error = layout
            .activate(&partial)
            .expect_err("a full-product activation must require component evidence");

        assert!(error.to_string().contains("core-codescan"), "{error}");
        assert_eq!(layout.active_generation().unwrap(), Some(old));
    }

    #[test]
    fn release_coupled_generation_rejects_missing_component_lock() {
        let temp = tempfile::tempdir().unwrap();
        let generation = temp.path().join("generation");
        write_generation(&generation, "1.0.0");
        fs::remove_file(generation.join(CODESCAN_COMPONENT_LOCK)).unwrap();
        assert!(validate_release_coupled_generation(&generation).is_err());
    }
}
