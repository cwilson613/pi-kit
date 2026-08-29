//! Crash-safe publication and activation for release-coupled executables.

use std::fs::{self, File};
use std::path::{Component, Path, PathBuf};

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
        validate_generation(staging_dir)?;
        fs::create_dir_all(&self.versions_root)?;
        sync_generation(staging_dir)?;
        match fs::symlink_metadata(&destination) {
            Ok(_) => {
                validate_generation(&destination)?;
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
        validate_generation(generation)?;
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
        "share/omegon/content-packs/omegon-shipped/content-pack.toml",
        "share/omegon/extensions/omegon-codescan/manifest.toml",
        "share/omegon/extensions/omegon-codescan/target/release/omegon-codescan",
    ] {
        let member = path.join(name);
        if !fs::metadata(&member).is_ok_and(|metadata| metadata.is_file()) {
            anyhow::bail!("release generation is missing {name}: {}", path.display());
        }
    }
    Ok(())
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
        fs::create_dir_all(path).unwrap();
        fs::write(path.join("omegon"), version).unwrap();
        fs::write(path.join("omegon-maintain"), version).unwrap();
        fs::write(
            path.join("install-receipt.json"),
            format!("{{\"version\":\"{version}\"}}"),
        )
        .unwrap();
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
}
