//! Version switcher for Omegon
//!
//! Provides version management capabilities including:
//! - GitHub Releases API client
//! - Platform detection and artifact mapping
//! - Download and signed composition verification
//! - Version storage management
//! - Interactive terminal picker
//! - .omegon-version auto-detection

use anyhow::{Result, anyhow};
use dirs::home_dir;
#[cfg(feature = "tui")]
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// GitHub repository info for releases
const REPO_OWNER: &str = "styrene-lab";
const REPO_NAME: &str = "omegon";
const GITHUB_API_BASE: &str = "https://api.github.com";
const MAX_ARCHIVE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_EVIDENCE_BYTES: u64 = 16 * 1024 * 1024;

/// Platform target mapping
#[derive(Debug, Clone, PartialEq)]
pub struct PlatformInfo {
    pub os: String,
    pub arch: String,
    pub target: String,
}

impl PlatformInfo {
    /// Rust target triple (e.g. "aarch64-apple-darwin") for matching CI artifact names.
    pub fn rust_triple(&self) -> &'static str {
        match (self.os.as_str(), self.arch.as_str()) {
            ("darwin", "arm64") => "aarch64-apple-darwin",
            ("darwin", "x64") => "x86_64-apple-darwin",
            ("linux", "arm64") => "aarch64-unknown-linux-gnu",
            ("linux", "x64") => "x86_64-unknown-linux-gnu",
            _ => "unknown-unknown-unknown",
        }
    }
}

/// Represents a GitHub release
#[derive(Debug, Clone, serde::Deserialize)]
pub struct GitHubRelease {
    pub tag_name: String,
    pub name: String,
    pub body: String,
    pub prerelease: bool,
    pub assets: Vec<GitHubAsset>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct GitHubAsset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}

/// Parsed version information
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub rc: Option<u32>,
    pub raw: String,
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;

        // First compare major.minor.patch
        match (self.major, self.minor, self.patch).cmp(&(other.major, other.minor, other.patch)) {
            Ordering::Equal => {
                // If versions are equal, stable > RC
                match (&self.rc, &other.rc) {
                    (None, None) => Ordering::Equal,
                    (None, Some(_)) => Ordering::Greater, // Stable > RC
                    (Some(_), None) => Ordering::Less,    // RC < Stable
                    (Some(a), Some(b)) => a.cmp(b),       // Compare RC numbers
                }
            }
            other => other,
        }
    }
}

/// Version state in local storage
#[derive(Debug, Clone)]
pub struct VersionInfo {
    pub version: Version,
    pub path: PathBuf,
    pub is_active: bool,
    pub is_installed: bool,
}

/// Version switcher configuration
pub struct VersionSwitcher {
    pub versions_dir: PathBuf,
    pub current_exe: PathBuf,
    client: reqwest::Client,
    cache: Option<Vec<GitHubRelease>>,
}

struct SwitchWorkDir(PathBuf);

impl Drop for SwitchWorkDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

impl Default for VersionSwitcher {
    fn default() -> Self {
        Self::new()
    }
}

impl VersionSwitcher {
    /// Create a new version switcher instance
    pub fn new() -> Self {
        let versions_dir = home_dir()
            .expect("HOME directory not set — cannot manage versions without a home directory")
            .join(".omegon/versions");

        let current_exe = env::current_exe().unwrap_or_else(|_| PathBuf::from("omegon"));

        Self {
            versions_dir,
            current_exe,
            client: reqwest::Client::new(),
            cache: None,
        }
    }

    /// Fetch releases from GitHub API with caching
    pub async fn fetch_releases(&mut self) -> Result<&[GitHubRelease]> {
        if self.cache.is_none() {
            let url = format!(
                "{}/repos/{}/{}/releases",
                GITHUB_API_BASE, REPO_OWNER, REPO_NAME
            );

            let response = self
                .client
                .get(&url)
                .header("User-Agent", "omegon-version-switcher")
                .send()
                .await?;

            if !response.status().is_success() {
                return Err(anyhow!(
                    "Failed to fetch releases: HTTP {}",
                    response.status()
                ));
            }

            let releases: Vec<GitHubRelease> = response.json().await?;
            self.cache = Some(releases);
        }

        Ok(self.cache.as_ref().unwrap())
    }

    /// List all installed versions
    pub fn list_installed_versions(&self) -> Result<Vec<VersionInfo>> {
        let mut versions = Vec::new();

        if !self.versions_dir.exists() {
            return Ok(versions);
        }

        let active_version = self.get_active_version()?;

        for entry in fs::read_dir(&self.versions_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let version_name = entry.file_name();
                let version_str = version_name.to_string_lossy();

                if let Ok(version) = Version::parse(&version_str) {
                    let binary_path = entry.path().join("omegon");
                    let is_installed =
                        crate::installed_release::validate_release_coupled_generation(
                            &entry.path(),
                        )
                        .is_ok();
                    let is_active = active_version
                        .as_ref()
                        .map(|v| v.raw == version.raw)
                        .unwrap_or(false);

                    versions.push(VersionInfo {
                        version,
                        path: binary_path,
                        is_active,
                        is_installed,
                    });
                }
            }
        }

        // Sort by version (newest first)
        versions.sort_by(|a, b| b.version.cmp(&a.version));
        Ok(versions)
    }

    /// Get the currently active version by resolving symlink
    pub fn get_active_version(&self) -> Result<Option<Version>> {
        let current = self
            .versions_dir
            .parent()
            .ok_or_else(|| anyhow!("versions directory has no parent"))?
            .join("current");
        let target = match fs::read_link(current) {
            Ok(target) => target,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };

        // Extract version from path like ~/.omegon/versions/1.2.3.
        if let Some(version_name) = target.file_name() {
            let version_str = version_name.to_string_lossy();
            return Ok(Some(Version::parse(&version_str)?));
        }

        Ok(None)
    }

    /// Authenticate, publish, and activate a specific version while the selector is locked.
    async fn switch_version(&mut self, version: &str) -> Result<PathBuf> {
        crate::installed_release::validate_version_component(version)?;
        fs::create_dir_all(&self.versions_dir)?;
        let versions_dir = self.versions_dir.canonicalize()?;
        let current = versions_dir
            .parent()
            .ok_or_else(|| anyhow!("versions directory has no parent"))?
            .join("current");
        let _switch_lock = crate::filelock::try_acquire_lock(&current)
            .map_err(|error| anyhow!("could not acquire switch lock: {error}"))?
            .ok_or_else(|| anyhow!("another release update is already active"))?;

        let active = capture_active_generation(&versions_dir, &current)?;
        let active_maintainer = active.join("omegon-maintain").canonicalize()?;
        clean_stale_switch_work(&versions_dir, &active)?;

        let releases = self.fetch_releases().await?;
        // Match tag_name with or without 'v' prefix
        let version_bare = version.strip_prefix('v').unwrap_or(version);
        let version_tagged = format!("v{version_bare}");
        let release = releases
            .iter()
            .find(|r| r.tag_name == version_bare || r.tag_name == version_tagged)
            .ok_or_else(|| anyhow!("Version {} not found in releases", version))?
            .clone();

        let artifact_name = format!(
            "omegon-{}-{}.tar.gz",
            version_bare,
            crate::installed_release::compiled_target()
        );
        let manifest_name = format!("{artifact_name}.manifest.json");
        let bundle_name = format!("{artifact_name}.manifest.sigstore.json");
        let asset = required_asset(&release, &artifact_name)?;
        let manifest_asset = required_asset(&release, &manifest_name)?;
        let bundle_asset = required_asset(&release, &bundle_name)?;

        let work = SwitchWorkDir(versions_dir.join(format!(".switch-{}", uuid::Uuid::new_v4())));
        fs::create_dir(&work.0)?;
        let archive_path = work.0.join(&artifact_name);
        let manifest_path = work.0.join(&manifest_name);
        let bundle_path = work.0.join(&bundle_name);
        self.download_asset_to(&asset, &archive_path, MAX_ARCHIVE_BYTES)
            .await?;
        self.download_asset_to(&manifest_asset, &manifest_path, MAX_EVIDENCE_BYTES)
            .await?;
        self.download_asset_to(&bundle_asset, &bundle_path, MAX_EVIDENCE_BYTES)
            .await?;

        verify_with_active_maintainer(
            &active_maintainer,
            &archive_path,
            &manifest_path,
            &bundle_path,
            None,
            version_bare,
        )?;

        let active_receipt = fs::read_to_string(active.join("install-receipt.json"))?;
        let staging = work.0.join("candidate");
        fs::create_dir(&staging)?;
        extract_tarball_path(&archive_path, &staging)?;
        verify_with_active_maintainer(
            &active_maintainer,
            &archive_path,
            &manifest_path,
            &bundle_path,
            Some(&staging),
            version_bare,
        )?;

        let version_dir = versions_dir.join(version_bare);
        let mut receipt: serde_json::Value = serde_json::from_str(&active_receipt)?;
        receipt["version"] = serde_json::Value::String(version_bare.to_string());
        receipt["version_dir"] = serde_json::Value::String(version_dir.display().to_string());
        receipt["versioned_binary"] =
            serde_json::Value::String(version_dir.join("omegon").display().to_string());
        receipt["versioned_maintenance_binary"] =
            serde_json::Value::String(version_dir.join("omegon-maintain").display().to_string());
        receipt["activation"] = serde_json::Value::String(current.display().to_string());
        receipt["layout"] = serde_json::Value::String("versioned-current-v1".into());
        fs::write(
            staging.join("install-receipt.json"),
            serde_json::to_string_pretty(&receipt)? + "\n",
        )?;
        let layout = crate::installed_release::InstalledReleaseLayout::new(
            versions_dir,
            self.current_exe.clone(),
            self.current_exe.with_file_name("omegon-maintain"),
            active.join("install-receipt.json"),
        )?;
        crate::installed_release::validate_release_coupled_generation(&staging)?;
        let (published, published_new) = if version_dir.exists() {
            authenticated_generation_matches(&staging, &version_dir)?;
            (version_dir, false)
        } else {
            (layout.publish_new_generation(&staging, version_bare)?, true)
        };
        if let Err(error) = layout.activate(&published) {
            cleanup_unselected_published_generation(
                &current,
                &published,
                &layout.versions_root,
                published_new,
            )
            .map_err(|cleanup| {
                anyhow!("selector activation failed ({error}); candidate cleanup failed: {cleanup}")
            })?;
            return Err(error);
        }
        Ok(published.join("omegon"))
    }

    /// Interactive version picker.
    #[cfg(feature = "tui")]
    pub async fn interactive_picker(&mut self) -> Result<Option<String>> {
        use crossterm::{
            cursor,
            event::{self, Event, KeyCode, KeyEvent},
            execute,
            style::{Color, Print, ResetColor, SetForegroundColor},
            terminal::{self, Clear, ClearType},
        };

        // Get installed versions first (only needs immutable borrow)
        let installed = self.list_installed_versions()?;
        let installed_map: HashMap<String, &VersionInfo> = installed
            .iter()
            .map(|v| (v.version.raw.clone(), v))
            .collect();

        // Fetch releases (needs mutable borrow, but installed is done)
        let releases = self.fetch_releases().await?;

        // Parse and sort versions
        let mut versions: Vec<Version> = releases
            .iter()
            .filter_map(|r| Version::parse(&r.tag_name).ok())
            .collect();
        versions.sort_by(|a, b| b.cmp(a)); // Newest first

        // Separate stable and RC versions
        let stable: Vec<&Version> = versions.iter().filter(|v| v.rc.is_none()).collect();
        let rc: Vec<&Version> = versions.iter().filter(|v| v.rc.is_some()).collect();

        let mut all_options = Vec::new();
        all_options.extend(stable.iter().map(|v| (*v, false))); // false = not RC
        if !rc.is_empty() {
            all_options.extend(rc.iter().map(|v| (*v, true))); // true = RC
        }

        if all_options.is_empty() {
            println!("No versions available");
            return Ok(None);
        }

        let mut selected = 0;

        // Enter raw mode with a guard that restores on panic/early return
        terminal::enable_raw_mode()?;
        struct RawModeGuard;
        impl Drop for RawModeGuard {
            fn drop(&mut self) {
                let _ = terminal::disable_raw_mode();
            }
        }
        let _guard = RawModeGuard;
        let mut stdout = std::io::stdout();

        let result = loop {
            // Clear screen and print header
            execute!(
                stdout,
                Clear(ClearType::All),
                cursor::MoveTo(0, 0),
                SetForegroundColor(Color::Cyan),
                Print("Select Omegon version (↑/↓ to navigate, Enter to select, q to quit):\n\n"),
                ResetColor,
            )?;

            // Render version groups
            for (label, color, filter_rc) in [
                ("Stable Releases:", Color::Green, false),
                ("Release Candidates:", Color::Yellow, true),
            ] {
                let has_entries = all_options.iter().any(|(v, _)| v.rc.is_some() == filter_rc);
                if !has_entries {
                    continue;
                }

                execute!(
                    stdout,
                    SetForegroundColor(color),
                    Print(format!("{label}\n")),
                    ResetColor
                )?;

                for (i, (version, _)) in all_options.iter().enumerate() {
                    if version.rc.is_some() != filter_rc {
                        continue;
                    }

                    let marker = if i == selected { "→ " } else { "  " };
                    let mut status_parts = Vec::new();
                    if let Some(info) = installed_map.get(&version.raw) {
                        if info.is_active {
                            status_parts.push("● active");
                        }
                        if info.is_installed {
                            status_parts.push("installed");
                        }
                    }
                    let status = if status_parts.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", status_parts.join(", "))
                    };

                    if i == selected {
                        execute!(
                            stdout,
                            SetForegroundColor(Color::Yellow),
                            Print(format!("{marker}{}{status}\n", version.raw)),
                            ResetColor
                        )?;
                    } else {
                        execute!(stdout, Print(format!("{marker}{}{status}\n", version.raw)))?;
                    }
                }
                execute!(stdout, Print("\n"))?;
            }

            // Handle input
            if let Event::Key(KeyEvent { code, .. }) = event::read()? {
                match code {
                    KeyCode::Up => {
                        selected = selected.saturating_sub(1);
                    }
                    KeyCode::Down if selected < all_options.len() - 1 => {
                        selected += 1;
                    }
                    KeyCode::Enter => {
                        let chosen_version = &all_options[selected].0;
                        break Some(chosen_version.raw.clone());
                    }
                    KeyCode::Char('q') | KeyCode::Esc => {
                        break None;
                    }
                    _ => {}
                }
            }
        };

        // Guard handles disable_raw_mode on drop (including panic paths)
        drop(_guard);
        execute!(stdout, Clear(ClearType::All), cursor::MoveTo(0, 0))?;

        Ok(result)
    }

    /// Check for .omegon-version file and warn if mismatch
    pub fn check_version_file(&self, cwd: &Path) -> Result<Option<String>> {
        let version_file = find_version_file(cwd)?;

        let Some(version_file_path) = version_file else {
            return Ok(None);
        };

        let required_version = fs::read_to_string(&version_file_path)?.trim().to_string();

        let active_version = self.get_active_version()?;

        match active_version {
            Some(active) if active.raw != required_version => {
                let warning = format!(
                    "Warning: .omegon-version specifies '{}' but active version is '{}'",
                    required_version, active.raw
                );
                Ok(Some(warning))
            }
            None => {
                let warning = format!(
                    "Warning: .omegon-version specifies '{}' but no version is active",
                    required_version
                );
                Ok(Some(warning))
            }
            _ => Ok(None), // Versions match
        }
    }

    /// Download an asset from GitHub
    async fn download_asset_to(&self, asset: &GitHubAsset, path: &Path, limit: u64) -> Result<()> {
        use std::io::Write;

        if asset.size > limit {
            return Err(anyhow!(
                "Release asset {} exceeds the {} byte limit",
                asset.name,
                limit
            ));
        }
        let mut response = self
            .client
            .get(&asset.browser_download_url)
            .header("User-Agent", "omegon-version-switcher")
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "Failed to download {}: HTTP {}",
                asset.name,
                response.status()
            ));
        }

        let mut file = fs::File::create(path)?;
        let mut downloaded = 0_u64;
        while let Some(chunk) = response.chunk().await? {
            downloaded = downloaded
                .checked_add(chunk.len() as u64)
                .ok_or_else(|| anyhow!("release asset size overflow"))?;
            if downloaded > limit {
                return Err(anyhow!(
                    "Release asset {} exceeds the {} byte limit",
                    asset.name,
                    limit
                ));
            }
            file.write_all(&chunk)?;
        }
        file.sync_all()?;
        Ok(())
    }
}

fn required_asset(release: &GitHubRelease, name: &str) -> Result<GitHubAsset> {
    release
        .assets
        .iter()
        .find(|asset| asset.name == name)
        .cloned()
        .ok_or_else(|| {
            anyhow!(
                "Release {} is missing required asset {name}",
                release.tag_name
            )
        })
}

fn capture_active_generation(versions_dir: &Path, current: &Path) -> Result<PathBuf> {
    let metadata = fs::symlink_metadata(current).map_err(|error| {
        anyhow!("version switching requires an installer-managed active release: {error}")
    })?;
    if !metadata.file_type().is_symlink() {
        return Err(anyhow!(
            "version switching requires the installer-managed current selector"
        ));
    }
    let active = current.canonicalize()?;
    if active.parent() != Some(versions_dir) {
        return Err(anyhow!("active release is outside the version store"));
    }
    crate::installed_release::validate_release_coupled_generation(&active)?;
    Ok(active)
}

fn clean_stale_switch_work(versions_dir: &Path, active: &Path) -> Result<()> {
    for entry in fs::read_dir(versions_dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let switch_owned = name.starts_with(".switch-");
        if !switch_owned || !entry.file_type()?.is_dir() {
            continue;
        }
        if entry.path().canonicalize().is_ok_and(|path| path == active) {
            continue;
        }
        fs::remove_dir_all(entry.path())?;
    }
    Ok(())
}

fn selector_names_generation(selector: &Path, generation: &Path) -> bool {
    selector
        .canonicalize()
        .ok()
        .zip(generation.canonicalize().ok())
        .is_some_and(|(selected, expected)| selected == expected)
}

fn cleanup_unselected_published_generation(
    selector: &Path,
    generation: &Path,
    versions_dir: &Path,
    published_new: bool,
) -> Result<()> {
    if published_new && !selector_names_generation(selector, generation) {
        fs::remove_dir_all(generation)?;
        fs::File::open(versions_dir)?.sync_all()?;
    }
    Ok(())
}

fn verify_with_active_maintainer(
    maintainer: &Path,
    archive: &Path,
    manifest: &Path,
    bundle: &Path,
    extracted_root: Option<&Path>,
    expected_version: &str,
) -> Result<()> {
    for operand in [maintainer, archive, manifest, bundle] {
        if !operand.is_absolute() {
            return Err(anyhow!(
                "release verification operand is not absolute: {}",
                operand.display()
            ));
        }
    }
    if extracted_root.is_some_and(|path| !path.is_absolute()) {
        return Err(anyhow!("extracted release root is not absolute"));
    }

    let mut command = Command::new(maintainer);
    command.args(["--json", "release", "verify", "--archive"]);
    command.arg(archive);
    command.arg("--manifest").arg(manifest);
    command.arg("--bundle").arg(bundle);
    if let Some(root) = extracted_root {
        command.arg("--extracted-root").arg(root);
    }
    let output = command.output()?;
    if !output.status.success() {
        return Err(anyhow!(
            "active release maintainer refused candidate authentication: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let result: omegon_maintenance_contracts::MaintenanceResultV1 =
        serde_json::from_slice(&output.stdout)
            .map_err(|error| anyhow!("active maintainer returned malformed JSON: {error}"))?;
    result
        .validate()
        .map_err(|error| anyhow!("active maintainer returned an invalid result: {error}"))?;
    if result.command != "release.verify"
        || result.status != omegon_maintenance_contracts::ResultStatus::Success
    {
        return Err(anyhow!(
            "active maintainer did not report verification success"
        ));
    }
    let verified = result
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "release_verified")
        .ok_or_else(|| anyhow!("active maintainer omitted release_verified evidence"))?;
    let evidence: serde_json::Value = serde_json::from_str(
        verified
            .evidence
            .as_deref()
            .ok_or_else(|| anyhow!("active maintainer omitted verification evidence"))?,
    )
    .map_err(|error| anyhow!("active maintainer returned malformed evidence: {error}"))?;
    if evidence["version"] != expected_version
        || evidence["target"] != crate::installed_release::compiled_target()
        || evidence["extracted_root_verified"] != extracted_root.is_some()
    {
        return Err(anyhow!(
            "active maintainer evidence does not match the requested release"
        ));
    }
    Ok(())
}

fn authenticated_generation_matches(candidate: &Path, existing: &Path) -> Result<()> {
    crate::installed_release::validate_release_coupled_generation(existing)?;
    compare_generation_tree(candidate, existing, Path::new("")).map_err(|error| {
        anyhow!("installed version differs from authenticated release: {error}")
    })?;
    let candidate_receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(candidate.join("install-receipt.json"))?)?;
    let existing_receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(existing.join("install-receipt.json"))?)?;
    for field in [
        "version",
        "platform",
        "binary",
        "maintenance_binary",
        "version_dir",
        "versioned_binary",
        "versioned_maintenance_binary",
        "activation",
        "layout",
    ] {
        if candidate_receipt.get(field) != existing_receipt.get(field) {
            return Err(anyhow!(
                "installed version receipt differs from authenticated generation: {field}"
            ));
        }
    }
    Ok(())
}

fn compare_generation_tree(candidate: &Path, existing: &Path, relative: &Path) -> Result<()> {
    let candidate_dir = candidate.join(relative);
    let existing_dir = existing.join(relative);
    for entry in fs::read_dir(&candidate_dir)? {
        let entry = entry?;
        let child_relative = relative.join(entry.file_name());
        if child_relative == Path::new("install-receipt.json") {
            continue;
        }
        let metadata = entry.file_type()?;
        let existing_metadata = fs::symlink_metadata(existing.join(&child_relative))?;
        if metadata.is_symlink() || existing_metadata.file_type().is_symlink() {
            return Err(anyhow!("generation member is a symlink"));
        }
        if metadata.is_dir() && existing_metadata.is_dir() {
            compare_generation_tree(candidate, existing, &child_relative)?;
        } else if metadata.is_file()
            && existing_metadata.is_file()
            && generation_modes_match(&entry.path(), &existing.join(&child_relative))?
            && fs::read(entry.path())? == fs::read(existing.join(&child_relative))?
        {
        } else {
            return Err(anyhow!(
                "generation member differs: {}",
                child_relative.display()
            ));
        }
    }
    for entry in fs::read_dir(existing_dir)? {
        let child_relative = relative.join(entry?.file_name());
        if child_relative != Path::new("install-receipt.json")
            && !candidate.join(&child_relative).exists()
        {
            return Err(anyhow!(
                "installed generation has unauthenticated member: {}",
                child_relative.display()
            ));
        }
    }
    Ok(())
}

#[cfg(unix)]
fn generation_modes_match(left: &Path, right: &Path) -> Result<bool> {
    use std::os::unix::fs::PermissionsExt;

    Ok(fs::metadata(left)?.permissions().mode() & 0o777
        == fs::metadata(right)?.permissions().mode() & 0o777)
}

#[cfg(not(unix))]
fn generation_modes_match(_left: &Path, _right: &Path) -> Result<bool> {
    Ok(true)
}

/// Detect the current platform and map to artifact name
pub fn detect_platform() -> Result<PlatformInfo> {
    let os = match env::consts::OS {
        "linux" => "linux",
        "macos" => "darwin",
        "windows" => "windows",
        other => return Err(anyhow!("Unsupported OS: {}", other)),
    };

    let arch = match env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        other => return Err(anyhow!("Unsupported architecture: {}", other)),
    };

    let target = format!("{}-{}", os, arch);

    Ok(PlatformInfo {
        os: os.to_string(),
        arch: arch.to_string(),
        target,
    })
}

/// Extract an authenticated release archive to an owned staging directory.
fn extract_tarball_path(archive_path: &Path, dest_dir: &Path) -> Result<()> {
    let file = fs::File::open(archive_path)?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    archive.unpack(dest_dir)?;
    Ok(())
}

/// Find .omegon-version file by walking up directories
fn find_version_file(start_dir: &Path) -> Result<Option<PathBuf>> {
    let mut current = start_dir;

    loop {
        let version_file = current.join(".omegon-version");
        if version_file.exists() {
            return Ok(Some(version_file));
        }

        match current.parent() {
            Some(parent) => current = parent,
            None => return Ok(None),
        }
    }
}

/// Extract version from command output like "omegon 1.2.3 (abc123 2026-03-21)"
fn extract_version_from_output(output: &str) -> Option<String> {
    let parts: Vec<&str> = output.split_whitespace().collect();
    if parts.len() >= 2 {
        Some(parts[1].to_string())
    } else {
        None
    }
}

impl Version {
    /// Parse a version string like "1.2.3" or "1.2.3-rc.4"
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.strip_prefix('v').unwrap_or(s); // Remove 'v' prefix if present

        let (base, rc) = if let Some(rc_pos) = s.find("-rc.") {
            let (base, rc_part) = s.split_at(rc_pos);
            let rc_num = rc_part
                .strip_prefix("-rc.")
                .ok_or_else(|| anyhow!("Invalid RC format"))?
                .parse::<u32>()?;
            (base, Some(rc_num))
        } else {
            (s, None)
        };

        let parts: Vec<&str> = base.split('.').collect();
        if parts.len() != 3 {
            return Err(anyhow!("Invalid version format: {}", s));
        }

        let major = parts[0].parse()?;
        let minor = parts[1].parse()?;
        let patch = parts[2].parse()?;

        Ok(Version {
            major,
            minor,
            patch,
            rc,
            raw: s.to_string(),
        })
    }

    /// Check if this is a stable release (not RC)
    pub fn is_stable(&self) -> bool {
        self.rc.is_none()
    }
}

// ─── Top-level CLI entrypoints ──────────────────────────────────────────────

/// `omegon switch --list` — show installed versions, mark active.
pub async fn list_versions() -> anyhow::Result<()> {
    let switcher = VersionSwitcher::new();
    let installed = switcher.list_installed_versions()?;
    let active = switcher.get_active_version()?;

    if installed.is_empty() {
        println!("No versions installed in ~/.omegon/versions/");
        println!("Run `omegon switch <version>` to install one.");
        return Ok(());
    }

    println!("Installed versions:");
    for v in &installed {
        let marker = if active.as_ref().is_some_and(|a| a.raw == v.version.raw) {
            " ● active"
        } else {
            ""
        };
        let kind = if v.version.is_stable() { "" } else { " (rc)" };
        println!("  {}{kind}{marker}", v.version.raw);
    }
    Ok(())
}

/// `omegon switch <version>` — download (if needed) and activate.
pub async fn switch_to_version(version: &str) -> anyhow::Result<()> {
    // Normalize: always strip 'v' prefix so directory names are consistent
    let version = version.strip_prefix('v').unwrap_or(version);
    let mut switcher = VersionSwitcher::new();
    println!("Downloading and authenticating omegon {version}...");
    switcher.switch_version(version).await?;
    println!("✓ Switched to omegon {version}");
    println!("  Restart omegon to use the new version.");
    Ok(())
}

/// `omegon switch --latest` — find and switch to latest stable release.
/// The `_include_rc` parameter is accepted for backward compatibility
/// but ignored — the RC channel has been retired.
pub async fn switch_to_latest(_include_rc: bool) -> anyhow::Result<()> {
    let mut switcher = VersionSwitcher::new();
    println!("Fetching releases...");
    let releases = switcher.fetch_releases().await?;

    let mut candidates: Vec<(&GitHubRelease, Version)> = releases
        .iter()
        .filter(|r| !r.prerelease)
        .filter_map(|r| Version::parse(&r.tag_name).ok().map(|v| (r, v)))
        .collect();
    candidates.sort_by(|a, b| b.1.cmp(&a.1));

    let (release, _) = candidates
        .first()
        .ok_or_else(|| anyhow::anyhow!("No stable releases found"))?;
    let version = release
        .tag_name
        .strip_prefix('v')
        .unwrap_or(&release.tag_name);
    println!("Latest stable: {version}");
    switch_to_version(version).await
}

/// `omegon switch` (no args) — interactive picker.
#[cfg(feature = "tui")]
pub async fn interactive_picker() -> anyhow::Result<()> {
    let mut switcher = VersionSwitcher::new();
    println!("Fetching releases...");
    let _ = switcher.fetch_releases().await?;

    match switcher.interactive_picker().await? {
        Some(version) => switch_to_version(&version).await,
        None => {
            println!("No version selected.");
            Ok(())
        }
    }
}

/// Check `.omegon-version` and return a warning message if version mismatches.
/// Returns None if no file exists or versions match.
/// Caller decides how to display (bootstrap panel, SystemNotification, etc.)
pub fn check_version_file_warning(cwd: &std::path::Path) -> Option<String> {
    let switcher = VersionSwitcher::new();
    // check_version_file already compares active vs required and returns
    // a warning string on mismatch, None on match or missing file.
    match switcher.check_version_file(cwd) {
        Ok(Some(warning)) => Some(format!("⚠ {warning}\n  Run `omegon switch` to fix.")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_parsing() {
        let v = Version::parse("1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
        assert_eq!(v.rc, None);
        assert!(v.is_stable());

        let v = Version::parse("1.2.3-rc.4").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
        assert_eq!(v.rc, Some(4));
        assert!(!v.is_stable());

        let v = Version::parse("v0.14.1-rc.12").unwrap();
        assert_eq!(v.major, 0);
        assert_eq!(v.minor, 14);
        assert_eq!(v.patch, 1);
        assert_eq!(v.rc, Some(12));
        assert_eq!(v.raw, "0.14.1-rc.12");

        assert!(Version::parse("invalid").is_err());
        assert!(Version::parse("1.2").is_err());
        assert!(Version::parse("1.2.3.4").is_err());
    }

    #[test]
    fn test_version_ordering() {
        let v1 = Version::parse("1.2.3").unwrap();
        let v2 = Version::parse("1.2.4").unwrap();
        let v3 = Version::parse("1.2.3-rc.1").unwrap();
        let v4 = Version::parse("1.2.4-rc.1").unwrap();

        assert!(v2 > v1);
        assert!(v1 > v3); // Stable > RC for same version
        assert!(v2 > v4); // Stable > RC for same version
        assert!(v4 > v1); // Higher version RC > lower stable
    }

    #[test]
    fn test_platform_detection() {
        let platform = detect_platform().unwrap();
        assert!(!platform.os.is_empty());
        assert!(!platform.arch.is_empty());
        assert!(!platform.target.is_empty());

        // Should be in format "os-arch"
        assert_eq!(
            platform.target,
            format!("{}-{}", platform.os, platform.arch)
        );
    }

    #[test]
    fn test_version_extraction() {
        assert_eq!(
            extract_version_from_output("omegon 1.2.3 (abc123 2026-03-21)"),
            Some("1.2.3".to_string())
        );
        assert_eq!(
            extract_version_from_output("omegon 0.14.1-rc.12"),
            Some("0.14.1-rc.12".to_string())
        );
        assert_eq!(extract_version_from_output("invalid"), None);
        assert_eq!(extract_version_from_output(""), None);
    }

    #[test]
    fn test_find_version_file() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create nested directory structure
        let nested_dir = root.join("project").join("sub");
        fs::create_dir_all(&nested_dir).unwrap();

        // Create .omegon-version in root
        let version_file = root.join(".omegon-version");
        fs::write(&version_file, "1.2.3").unwrap();

        // Should find the file when starting from nested directory
        let found = find_version_file(&nested_dir).unwrap();
        assert_eq!(found, Some(version_file));

        // Should return None if no file exists
        let temp_dir2 = TempDir::new().unwrap();
        let found = find_version_file(temp_dir2.path()).unwrap();
        assert_eq!(found, None);
    }

    #[test]
    fn test_check_version_file_warning_mismatch() {
        // check_version_file_warning returns a warning when .omegon-version
        // doesn't match the active version. Since we're not running from a
        // symlink in tests, get_active_version returns None → warning.
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join(".omegon-version"), "99.99.99").unwrap();

        let warning = check_version_file_warning(dir.path());
        assert!(
            warning.is_some(),
            "should warn when version can't be determined"
        );
        assert!(warning.unwrap().contains("99.99.99"));
    }

    #[test]
    fn test_check_version_file_warning_no_file() {
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        let warning = check_version_file_warning(dir.path());
        assert!(warning.is_none(), "no .omegon-version = no warning");
    }

    #[test]
    fn test_rust_triple_mapping() {
        let p = PlatformInfo {
            os: "darwin".into(),
            arch: "arm64".into(),
            target: "darwin-arm64".into(),
        };
        assert_eq!(p.rust_triple(), "aarch64-apple-darwin");

        let p = PlatformInfo {
            os: "linux".into(),
            arch: "x64".into(),
            target: "linux-x64".into(),
        };
        assert_eq!(p.rust_triple(), "x86_64-unknown-linux-gnu");

        let p = PlatformInfo {
            os: "darwin".into(),
            arch: "x64".into(),
            target: "darwin-x64".into(),
        };
        assert_eq!(p.rust_triple(), "x86_64-apple-darwin");
    }

    #[test]
    fn test_arch_maps_to_release_names() {
        // env::consts::ARCH values → must match release artifact naming
        // Release artifacts use: darwin-arm64, darwin-x64, linux-arm64, linux-x64
        let arch_map: &[(&str, &str)] = &[
            ("x86_64", "x64"), // NOT "x86_64" — release uses "x64"
            ("aarch64", "arm64"),
        ];
        for (rust_arch, expected) in arch_map {
            let mapped = match *rust_arch {
                "x86_64" => "x64",
                "aarch64" => "arm64",
                other => other,
            };
            assert_eq!(
                mapped, *expected,
                "ARCH {rust_arch} should map to {expected}"
            );
        }
    }

    #[test]
    fn test_version_tag_prefix_stripping() {
        // install_version should match tags with or without 'v' prefix
        let v = Version::parse("v0.14.1-rc.12").unwrap();
        assert_eq!(v.raw, "0.14.1-rc.12"); // v stripped
        assert_eq!(v.rc, Some(12));

        let v = Version::parse("0.14.1").unwrap();
        assert_eq!(v.raw, "0.14.1"); // no v to strip
        assert!(v.is_stable());
    }

    #[test]
    fn test_switch_to_latest_sorts_by_version_not_api_order() {
        // Verify that Version ordering puts the right thing first
        let mut versions = [
            Version::parse("0.13.0").unwrap(),
            Version::parse("0.14.1-rc.12").unwrap(),
            Version::parse("0.14.0").unwrap(),
            Version::parse("0.14.1-rc.3").unwrap(),
        ];
        versions.sort_by(|a, b| b.cmp(a));
        assert_eq!(versions[0].raw, "0.14.1-rc.12"); // highest RC
        assert_eq!(versions[1].raw, "0.14.1-rc.3");
        assert_eq!(versions[2].raw, "0.14.0"); // highest stable
        assert_eq!(versions[3].raw, "0.13.0");
    }

    #[cfg(unix)]
    fn write_test_generation(path: &Path, version: &str, maintainer: &[u8]) {
        use sha2::{Digest, Sha256};
        use std::os::unix::fs::PermissionsExt;

        fs::create_dir_all(path).unwrap();
        fs::write(path.join("omegon"), format!("omegon-{version}")).unwrap();
        fs::write(path.join("omegon-maintain"), maintainer).unwrap();
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
            let bytes = fs::read(path.join(executable)).unwrap();
            let digest = omegon_maintenance_contracts::AuthorityKey::from_bytes(
                Sha256::digest(bytes).into(),
            );
            fs::write(
                path.join(format!("{executable}.composition-lock.json")),
                serde_json::to_vec(&serde_json::json!({
                    "schema_version": 1,
                    "executable_identity": identity,
                    "executable_digest": digest,
                    "target": crate::installed_release::compiled_target(),
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
        let content = "share/omegon/content-packs/omegon-shipped/content-pack.toml";
        let extension_manifest = "share/omegon/extensions/omegon-codescan/manifest.toml";
        let extension_binary =
            "share/omegon/extensions/omegon-codescan/target/release/omegon-codescan";
        for (relative, bytes) in [
            (content, b"id = \"omegon-shipped\"\n".as_slice()),
            (extension_manifest, manifest.as_slice()),
            (extension_binary, codescan.as_slice()),
        ] {
            let member = path.join(relative);
            fs::create_dir_all(member.parent().unwrap()).unwrap();
            fs::write(member, bytes).unwrap();
        }
        let component_lock = "share/omegon/components/core-codescan.lock.json";
        let component_lock_path = path.join(component_lock);
        fs::create_dir_all(component_lock_path.parent().unwrap()).unwrap();
        fs::write(
            component_lock_path,
            serde_json::to_vec(&serde_json::json!({
                "schema_version": 1,
                "component_id": "core:codescan",
                "wire_manifest_id": "omegon-codescan",
                "manifest_path": extension_manifest,
                "manifest_digest": omegon_maintenance_contracts::AuthorityKey::from_bytes(Sha256::digest(manifest).into()),
                "executable_path": extension_binary,
                "executable_digest": omegon_maintenance_contracts::AuthorityKey::from_bytes(Sha256::digest(&codescan).into()),
                "target": crate::installed_release::compiled_target(),
                "protocol_minimum": 1,
                "protocol_maximum": 1,
                "protocol_version": 1,
                "fallback": "typed_unavailable",
                "signing_identity": signing_identity
            }))
            .unwrap(),
        )
        .unwrap();
        for executable in ["omegon", "omegon-maintain", extension_binary] {
            fs::set_permissions(path.join(executable), fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    #[cfg(unix)]
    fn verifier_script(log: &Path, version: &str, target: &str) -> Vec<u8> {
        format!(
            r#"#!/bin/sh
printf '%s\n' "$0|$*" >> '{}'
extracted=false
case " $* " in *" --extracted-root "*) extracted=true;; esac
printf '%s\n' '{{"schema_version":1,"command":"release.verify","status":"success","request_id":"00000000-0000-0000-0000-000000000000","artifact":{{"version":"1.0.0","commit":"test","target":"{target}","digest":"{zero}"}},"composition":{{"profile":"full-product","generation":"{zero}","excluded_inputs":[]}},"deadline":{{"requested_ms":300000,"elapsed_ms":1,"expired":false}},"diagnostics":[{{"code":"release_verified","severity":"info","scope":"release","message":"verified","evidence":"{{\"version\":\"{version}\",\"target\":\"{target}\",\"extracted_root_verified\":'$extracted'}}"}}],"mutations":[],"errors":[],"truncated":false,"next_cursor":null}}'
"#,
            log.display(),
            zero = "0".repeat(64)
        )
        .into_bytes()
    }

    #[cfg(unix)]
    fn archive_generation(path: &Path) -> Vec<u8> {
        let output = Vec::new();
        let encoder = flate2::write::GzEncoder::new(output, flate2::Compression::default());
        let mut archive = tar::Builder::new(encoder);
        for entry in walk_generation(path, Path::new("")) {
            if entry == Path::new("install-receipt.json") {
                continue;
            }
            archive
                .append_path_with_name(path.join(&entry), &entry)
                .unwrap();
        }
        archive.into_inner().unwrap().finish().unwrap()
    }

    #[cfg(unix)]
    fn walk_generation(root: &Path, relative: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        for entry in fs::read_dir(root.join(relative)).unwrap() {
            let entry = entry.unwrap();
            let child = relative.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                files.extend(walk_generation(root, &child));
            } else {
                files.push(child);
            }
        }
        files.sort();
        files
    }

    #[cfg(unix)]
    fn serve_assets(assets: Vec<(String, Vec<u8>)>) -> (String, std::thread::JoinHandle<()>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            for _ in 0..assets.len() {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0_u8; 2048];
                let read = stream.read(&mut request).unwrap();
                let first = String::from_utf8_lossy(&request[..read]);
                let path = first.split_whitespace().nth(1).unwrap();
                let body = assets
                    .iter()
                    .find(|(name, _)| path == format!("/{name}"))
                    .unwrap()
                    .1
                    .clone();
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                stream.write_all(&body).unwrap();
            }
        });
        (base, handle)
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn switch_uses_captured_active_authority_twice_and_changes_one_selector() {
        let temp = tempfile::tempdir().unwrap();
        let versions = temp.path().join("versions");
        let old = versions.join("1.0.0");
        let candidate_source = temp.path().join("candidate-source");
        let log = temp.path().join("maintainer.log");
        let candidate_ran = temp.path().join("candidate-ran");
        let active_script =
            verifier_script(&log, "2.0.0", crate::installed_release::compiled_target());
        write_test_generation(&old, "1.0.0", &active_script);
        let malicious = format!("#!/bin/sh\ntouch '{}'\nexit 99\n", candidate_ran.display());
        write_test_generation(&candidate_source, "2.0.0", malicious.as_bytes());
        fs::create_dir_all(&versions).unwrap();
        let current = temp.path().join("current");
        crate::installed_release::atomic_replace_symlink(&current, &old).unwrap();
        fs::create_dir(versions.join(".switch-stale")).unwrap();
        fs::create_dir(versions.join(".installer.staging-123")).unwrap();

        let archive_name = format!(
            "omegon-2.0.0-{}.tar.gz",
            crate::installed_release::compiled_target()
        );
        let manifest_name = format!("{archive_name}.manifest.json");
        let bundle_name = format!("{archive_name}.manifest.sigstore.json");
        let archive_bytes = archive_generation(&candidate_source);
        let assets = vec![
            (archive_name.clone(), archive_bytes),
            (manifest_name.clone(), b"manifest".to_vec()),
            (bundle_name.clone(), b"bundle".to_vec()),
        ];
        let (base, server) = serve_assets(assets.clone());
        let release = GitHubRelease {
            tag_name: "v2.0.0".into(),
            name: "2.0.0".into(),
            body: String::new(),
            prerelease: false,
            assets: assets
                .iter()
                .map(|(name, bytes)| GitHubAsset {
                    name: name.clone(),
                    browser_download_url: format!("{base}/{name}"),
                    size: bytes.len() as u64,
                })
                .collect(),
        };
        let mut switcher = VersionSwitcher {
            versions_dir: versions.clone(),
            current_exe: temp.path().join("bin/omegon"),
            client: reqwest::Client::new(),
            cache: Some(vec![release]),
        };

        switcher.switch_version("2.0.0").await.unwrap();
        server.join().unwrap();

        assert_eq!(
            current.canonicalize().unwrap(),
            versions.join("2.0.0").canonicalize().unwrap()
        );
        assert_eq!(
            fs::read_to_string(current.join("omegon")).unwrap(),
            "omegon-2.0.0"
        );
        assert_eq!(
            fs::read_to_string(old.join("omegon")).unwrap(),
            "omegon-1.0.0"
        );
        assert!(
            !candidate_ran.exists(),
            "candidate executables must never run"
        );
        assert!(!versions.join(".switch-stale").exists());
        assert!(
            versions.join(".installer.staging-123").exists(),
            "switch recovery must not remove another publisher's staging"
        );
        assert!(fs::read_dir(&versions).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".switch-")
        }));
        let calls = fs::read_to_string(log).unwrap();
        let lines: Vec<_> = calls.lines().collect();
        assert_eq!(lines.len(), 2, "{calls}");
        let captured = old.join("omegon-maintain").canonicalize().unwrap();
        assert!(
            lines
                .iter()
                .all(|line| line.starts_with(captured.to_str().unwrap()))
        );
        assert!(!lines[0].contains("--extracted-root"));
        assert!(lines[1].contains("--extracted-root"));
        for line in lines {
            assert!(line.contains(&archive_name));
            assert!(line.contains(&manifest_name));
            assert!(line.contains(&bundle_name));
        }
        for member in [
            "omegon",
            "omegon-maintain",
            "omegon.composition-lock.json",
            "omegon-maintain.composition-lock.json",
            "install-receipt.json",
            "share/omegon/content-packs/omegon-shipped/content-pack.toml",
            "share/omegon/extensions/omegon-codescan/manifest.toml",
            "share/omegon/extensions/omegon-codescan/target/release/omegon-codescan",
            "share/omegon/components/core-codescan.lock.json",
        ] {
            assert_eq!(
                fs::read(current.join(member)).unwrap(),
                fs::read(versions.join("2.0.0").join(member)).unwrap(),
                "selector mixed generation member {member}"
            );
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn missing_signed_operand_preserves_current_and_cleans_stale_work() {
        let temp = tempfile::tempdir().unwrap();
        let versions = temp.path().join("versions");
        let old = versions.join("1.0.0");
        let log = temp.path().join("maintainer.log");
        write_test_generation(
            &old,
            "1.0.0",
            &verifier_script(&log, "2.0.0", crate::installed_release::compiled_target()),
        );
        let current = temp.path().join("current");
        crate::installed_release::atomic_replace_symlink(&current, &old).unwrap();
        fs::create_dir(versions.join(".switch-stale")).unwrap();
        let archive_name = format!(
            "omegon-2.0.0-{}.tar.gz",
            crate::installed_release::compiled_target()
        );
        let release = GitHubRelease {
            tag_name: "v2.0.0".into(),
            name: "2.0.0".into(),
            body: String::new(),
            prerelease: false,
            assets: vec![GitHubAsset {
                name: archive_name,
                browser_download_url: "http://127.0.0.1:1/archive".into(),
                size: 1,
            }],
        };
        let mut switcher = VersionSwitcher {
            versions_dir: versions.clone(),
            current_exe: temp.path().join("bin/omegon"),
            client: reqwest::Client::new(),
            cache: Some(vec![release]),
        };

        let error = switcher
            .switch_version("2.0.0")
            .await
            .expect_err("checksum or archive alone must not authorize activation");

        assert!(error.to_string().contains("manifest.json"), "{error}");
        assert_eq!(current.canonicalize().unwrap(), old.canonicalize().unwrap());
        assert!(!versions.join(".switch-stale").exists());
        assert!(
            !log.exists(),
            "verification cannot run without all operands"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn active_verifier_refusal_cleans_downloads_and_preserves_current() {
        let temp = tempfile::tempdir().unwrap();
        let versions = temp.path().join("versions");
        let old = versions.join("1.0.0");
        let candidate = temp.path().join("candidate");
        let log = temp.path().join("maintainer.log");
        write_test_generation(
            &old,
            "1.0.0",
            &verifier_script(&log, "2.0.0", "wrong-target"),
        );
        write_test_generation(&candidate, "2.0.0", b"candidate-maintainer");
        let current = temp.path().join("current");
        crate::installed_release::atomic_replace_symlink(&current, &old).unwrap();

        let archive_name = format!(
            "omegon-2.0.0-{}.tar.gz",
            crate::installed_release::compiled_target()
        );
        let manifest_name = format!("{archive_name}.manifest.json");
        let bundle_name = format!("{archive_name}.manifest.sigstore.json");
        let assets = vec![
            (archive_name, archive_generation(&candidate)),
            (manifest_name, b"manifest".to_vec()),
            (bundle_name, b"bundle".to_vec()),
        ];
        let (base, server) = serve_assets(assets.clone());
        let release = GitHubRelease {
            tag_name: "v2.0.0".into(),
            name: "2.0.0".into(),
            body: String::new(),
            prerelease: false,
            assets: assets
                .iter()
                .map(|(name, bytes)| GitHubAsset {
                    name: name.clone(),
                    browser_download_url: format!("{base}/{name}"),
                    size: bytes.len() as u64,
                })
                .collect(),
        };
        let mut switcher = VersionSwitcher {
            versions_dir: versions.clone(),
            current_exe: temp.path().join("bin/omegon"),
            client: reqwest::Client::new(),
            cache: Some(vec![release]),
        };

        switcher
            .switch_version("2.0.0")
            .await
            .expect_err("wrong-target active verification must refuse the candidate");
        server.join().unwrap();

        assert_eq!(current.canonicalize().unwrap(), old.canonicalize().unwrap());
        assert!(!versions.join("2.0.0").exists());
        assert!(fs::read_dir(&versions).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".switch-")
        }));
        let calls = fs::read_to_string(log).unwrap();
        assert_eq!(calls.lines().count(), 1, "{calls}");
        let captured = old.join("omegon-maintain").canonicalize().unwrap();
        assert!(calls.starts_with(captured.to_str().unwrap()));
    }

    #[cfg(unix)]
    #[test]
    fn maintainer_result_rejects_malformed_wrong_version_and_wrong_target() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("archive");
        let manifest = temp.path().join("manifest");
        let bundle = temp.path().join("bundle");
        for path in [&archive, &manifest, &bundle] {
            fs::write(path, "operand").unwrap();
        }
        for (version, target) in [
            ("wrong", crate::installed_release::compiled_target()),
            ("2.0.0", "wrong-target"),
        ] {
            let verifier = temp.path().join(format!("verifier-{version}-{target}"));
            fs::write(
                &verifier,
                verifier_script(&temp.path().join("log"), version, target),
            )
            .unwrap();
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&verifier, fs::Permissions::from_mode(0o755)).unwrap();
            assert!(
                verify_with_active_maintainer(
                    &verifier, &archive, &manifest, &bundle, None, "2.0.0"
                )
                .is_err()
            );
        }
        let malformed = temp.path().join("malformed");
        fs::write(&malformed, b"#!/bin/sh\necho not-json\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&malformed, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(
            verify_with_active_maintainer(&malformed, &archive, &manifest, &bundle, None, "2.0.0")
                .is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn existing_generation_requires_generation_bound_receipt() {
        let temp = tempfile::tempdir().unwrap();
        let candidate = temp.path().join("candidate");
        let existing = temp.path().join("existing");
        write_test_generation(&candidate, "2.0.0", b"maintainer");
        write_test_generation(&existing, "2.0.0", b"maintainer");
        authenticated_generation_matches(&candidate, &existing).unwrap();

        fs::write(
            existing.join("install-receipt.json"),
            b"{\"version\":\"1.0.0\",\"layout\":\"versioned-current-v1\"}",
        )
        .unwrap();
        assert!(authenticated_generation_matches(&candidate, &existing).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn failed_activation_removes_only_an_unselected_new_generation() {
        let temp = tempfile::tempdir().unwrap();
        let versions = temp.path().join("versions");
        let old = versions.join("1.0.0");
        let candidate = versions.join("2.0.0");
        fs::create_dir_all(&old).unwrap();
        fs::create_dir(&candidate).unwrap();
        let current = temp.path().join("current");
        crate::installed_release::atomic_replace_symlink(&current, &old).unwrap();

        cleanup_unselected_published_generation(&current, &candidate, &versions, true).unwrap();

        assert!(!candidate.exists());
        assert_eq!(current.canonicalize().unwrap(), old.canonicalize().unwrap());
    }
}
