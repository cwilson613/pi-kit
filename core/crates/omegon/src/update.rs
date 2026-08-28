//! Update checker — polls GitHub Releases API for new versions.
//!
//! At startup, spawns an async task that checks for newer releases.
//! Results are surfaced as a banner in the TUI footer.
//! The `/update` command triggers download + replace + exec restart.

use std::path::{Path, PathBuf};
use std::str;
use std::time::Duration;
use tokio::sync::watch;

/// Version comparison result.
#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub download_url: String,
    pub signature_url: String,
    pub certificate_url: String,
    pub release_notes: String,
    pub is_newer: bool,
}

impl UpdateInfo {
    pub fn has_downloadable_archive(&self) -> bool {
        !self.download_url.is_empty()
            && !self.signature_url.is_empty()
            && !self.certificate_url.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateChannel {
    Stable,
    Nightly,
}

impl UpdateChannel {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "stable" => Some(Self::Stable),
            "rc" => Some(Self::Stable), // RC deprecated — redirect to stable
            "nightly" => Some(Self::Nightly),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Nightly => "nightly",
        }
    }
}

/// Shared state for the update checker.
pub type UpdateReceiver = watch::Receiver<Option<UpdateInfo>>;
pub type UpdateSender = watch::Sender<Option<UpdateInfo>>;

/// Create the update channel.
pub fn channel() -> (UpdateSender, UpdateReceiver) {
    watch::channel(None)
}

/// GitHub release info (minimal subset).
#[derive(serde::Deserialize, Clone)]
pub(crate) struct GitHubRelease {
    pub tag_name: String,
    pub body: Option<String>,
    pub assets: Vec<GitHubAsset>,
    pub prerelease: bool,
}

#[derive(serde::Deserialize, Clone)]
pub(crate) struct GitHubAsset {
    pub name: String,
    pub browser_download_url: String,
}

fn find_asset_url(assets: &[GitHubAsset], suffix: &str) -> String {
    assets
        .iter()
        .find(|a| a.name.ends_with(suffix))
        .map(|a| a.browser_download_url.clone())
        .unwrap_or_default()
}

/// Path for the update check cache file.
fn cache_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".omegon/update-check.json"))
}

/// Read the cached update check result. Returns Some if the cache is
/// fresh (< 24 hours old) and matches the requested channel.
pub fn read_cache(channel: UpdateChannel) -> Option<UpdateInfo> {
    let path = cache_path()?;
    let content = std::fs::read_to_string(&path).ok()?;
    let cached: serde_json::Value = serde_json::from_str(&content).ok()?;

    let cached_channel = cached["channel"].as_str()?;
    if cached_channel != channel.as_str() {
        return None;
    }

    let cached_at = cached["checked_at"].as_str()?;
    let checked_at = chrono::DateTime::parse_from_rfc3339(cached_at).ok()?;
    let age = chrono::Utc::now().signed_duration_since(checked_at);
    if age.num_hours() >= 24 {
        return None;
    }

    let latest = cached["latest"].as_str()?.to_string();
    let current = env!("CARGO_PKG_VERSION");
    if !is_newer(&latest, current) {
        return None;
    }

    let info = UpdateInfo {
        current: current.to_string(),
        latest,
        download_url: cached["download_url"].as_str().unwrap_or("").to_string(),
        signature_url: cached["signature_url"].as_str().unwrap_or("").to_string(),
        certificate_url: cached["certificate_url"].as_str().unwrap_or("").to_string(),
        release_notes: cached["release_notes"].as_str().unwrap_or("").to_string(),
        is_newer: true,
    };
    info.has_downloadable_archive().then_some(info)
}

/// Write the update check result to cache.
fn write_cache(info: &UpdateInfo, channel: UpdateChannel) {
    if !info.has_downloadable_archive() {
        clear_cache();
        return;
    }
    let Some(path) = cache_path() else { return };
    let cached = serde_json::json!({
        "channel": channel.as_str(),
        "latest": info.latest,
        "download_url": info.download_url,
        "signature_url": info.signature_url,
        "certificate_url": info.certificate_url,
        "release_notes": info.release_notes,
        "checked_at": chrono::Utc::now().to_rfc3339(),
    });
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(
        &path,
        serde_json::to_string_pretty(&cached).unwrap_or_default(),
    );
}

fn clear_cache() {
    if let Some(path) = cache_path() {
        let _ = std::fs::remove_file(path);
    }
}

fn spawn_check_with_options(
    tx: UpdateSender,
    channel: UpdateChannel,
    delay: Duration,
    use_cache: bool,
) {
    // Check cache first — avoid a GitHub API call if we checked recently.
    if use_cache && let Some(cached) = read_cache(channel) {
        tracing::debug!(
            latest = %cached.latest,
            "update check: using cached result (< 24h old)"
        );
        let _ = tx.send(Some(cached));
        return;
    }

    let current = env!("CARGO_PKG_VERSION").to_string();
    crate::task_spawn::spawn_best_effort_result("update-check", async move {
        tokio::time::sleep(delay).await;

        match check_latest_for_channel(&current, channel).await {
            Ok(Some(info)) => {
                tracing::info!(
                    current = %info.current,
                    latest = %info.latest,
                    channel = channel.as_str(),
                    "new version available"
                );
                write_cache(&info, channel);
                let _ = tx.send(Some(info));
            }
            Ok(None) => {
                tracing::debug!(channel = channel.as_str(), "up to date");
                let _ = tx.send(None);
            }
            Err(e) => {
                tracing::debug!(
                    channel = channel.as_str(),
                    "update check failed (non-fatal): {e}"
                );
            }
        }
        Ok(())
    });
}

pub fn spawn_check_with_delay(tx: UpdateSender, channel: UpdateChannel, delay: Duration) {
    spawn_check_with_options(tx, channel, delay, true);
}

/// Spawn the background update check.
pub fn spawn_check(tx: UpdateSender, channel: UpdateChannel) {
    spawn_check_with_delay(tx, channel, Duration::from_secs(5));
}

/// Spawn an update check that bypasses the cache. Used for explicit operator
/// `/update` requests so a release first observed before assets were uploaded
/// cannot stay stuck as "not downloadable" for the cache TTL.
pub fn spawn_check_now(tx: UpdateSender, channel: UpdateChannel) {
    spawn_check_with_options(tx, channel, Duration::from_secs(0), false);
}

/// Poll for updates periodically so long-running TUI sessions notice new releases.
pub fn spawn_polling(tx: UpdateSender, settings: crate::settings::SharedSettings) {
    crate::task_spawn::spawn_best_effort("update-poller", async move {
        loop {
            tokio::time::sleep(Duration::from_secs(300)).await;
            let channel = settings
                .lock()
                .ok()
                .and_then(|s| UpdateChannel::parse(&s.update_channel))
                .unwrap_or(UpdateChannel::Stable);
            spawn_check_with_delay(tx.clone(), channel, Duration::from_secs(0));
        }
    });
}

/// Check GitHub Releases for a newer version on the selected channel.
pub async fn check_latest_for_channel(
    current: &str,
    channel: UpdateChannel,
) -> anyhow::Result<Option<UpdateInfo>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent(format!("omegon/{current}"))
        .build()?;

    let releases: Vec<GitHubRelease> = if matches!(channel, UpdateChannel::Stable) {
        vec![
            client
                .get("https://api.github.com/repos/styrene-lab/omegon/releases/latest")
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?,
        ]
    } else {
        client
            .get("https://api.github.com/repos/styrene-lab/omegon/releases")
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?
    };

    let target = platform_archive_target();
    let selected = releases.into_iter().find(|resp| {
        let latest = resp.tag_name.trim_start_matches('v');
        let latest = latest.to_lowercase();
        let channel_match = match channel {
            UpdateChannel::Stable => !resp.prerelease,
            UpdateChannel::Nightly => resp.prerelease && latest.contains("-nightly."),
        };
        channel_match && is_newer(&latest, current)
    });

    let Some(resp) = selected else {
        return Ok(None);
    };

    let latest = resp.tag_name.trim_start_matches('v').to_string();

    let archive_name = resp
        .assets
        .iter()
        .find(|a| a.name.contains(&target) && a.name.ends_with(".tar.gz"))
        .map(|a| a.name.clone())
        .unwrap_or_default();
    let download_url = find_asset_url(&resp.assets, &archive_name);
    let signature_url = if archive_name.is_empty() {
        String::new()
    } else {
        find_asset_url(&resp.assets, &format!("{archive_name}.sig"))
    };
    let certificate_url = if archive_name.is_empty() {
        String::new()
    } else {
        find_asset_url(&resp.assets, &format!("{archive_name}.pem"))
    };

    Ok(Some(UpdateInfo {
        current: current.to_string(),
        latest,
        download_url,
        signature_url,
        certificate_url,
        release_notes: resp.body.unwrap_or_default(),
        is_newer: true,
    }))
}

/// Semver comparison: is `latest` newer than `current`?
/// A stable release (0.15.2) is newer than its own prerelease variants.
fn is_newer(latest: &str, current: &str) -> bool {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    enum SuffixKind {
        Rc,
        Nightly,
        Stable,
    }

    let parse = |s: &str| -> (Vec<u32>, SuffixKind, u32) {
        let mut parts = s.splitn(2, '-');
        let base = parts.next().unwrap_or(s);
        let suffix = parts.next().unwrap_or("");
        let version: Vec<u32> = base.split('.').filter_map(|p| p.parse().ok()).collect();
        if let Some(num) = suffix.strip_prefix("rc.").and_then(|n| n.parse().ok()) {
            (version, SuffixKind::Rc, num)
        } else if let Some(num) = suffix.strip_prefix("nightly.").and_then(|n| n.parse().ok()) {
            (version, SuffixKind::Nightly, num)
        } else {
            (version, SuffixKind::Stable, 0)
        }
    };

    let (l_ver, l_kind, l_num) = parse(latest);
    let (c_ver, c_kind, c_num) = parse(current);
    match l_ver.cmp(&c_ver) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => match l_kind.cmp(&c_kind) {
            std::cmp::Ordering::Greater => true,
            std::cmp::Ordering::Less => false,
            std::cmp::Ordering::Equal => l_num > c_num,
        },
    }
}

/// Platform-specific asset name pattern.
pub(crate) fn platform_archive_target() -> String {
    if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
        "aarch64-apple-darwin".into()
    } else if cfg!(target_os = "macos") && cfg!(target_arch = "x86_64") {
        "x86_64-apple-darwin".into()
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "aarch64") {
        "aarch64-unknown-linux-gnu".into()
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "x86_64") {
        "x86_64-unknown-linux-gnu".into()
    } else {
        "unknown".into()
    }
}

async fn download_to_path(client: &reqwest::Client, url: &str, path: &Path) -> anyhow::Result<()> {
    let bytes = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    tokio::fs::write(path, &bytes).await?;
    Ok(())
}

/// Normalize a downloaded signing certificate for signature/identity verification.
///
/// `cosign sign-blob --output-certificate` emits the PEM certificate
/// base64-encoded (sigstore/cosign#2059). Older releases shipped the raw PEM
/// text. Accept either: if the content doesn't begin with a PEM boundary,
/// attempt one base64 decode and require the result to be PEM.
fn normalize_certificate_pem(content: &str) -> anyhow::Result<String> {
    use base64::Engine;

    let trimmed = content.trim();
    if trimmed.contains("-----BEGIN") {
        return Ok(trimmed.to_string());
    }

    let compact: String = trimmed.split_whitespace().collect();
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&compact)
        .map_err(|e| {
            anyhow::anyhow!("certificate is neither PEM text nor base64-encoded PEM: {e}")
        })?;
    let decoded = String::from_utf8(decoded)
        .map_err(|e| anyhow::anyhow!("base64-decoded certificate is not UTF-8: {e}"))?;
    let decoded = decoded.trim().to_string();
    if !decoded.contains("-----BEGIN") {
        anyhow::bail!("base64-decoded certificate does not contain a PEM boundary");
    }
    Ok(decoded)
}

fn verify_archive_signature(
    archive_path: &Path,
    sig_path: &Path,
    cert_path: &Path,
) -> anyhow::Result<()> {
    let blob = std::fs::read(archive_path)?;
    let signature = std::fs::read_to_string(sig_path)?;
    let cert_pem = std::fs::read_to_string(cert_path)?;
    let cert_pem = normalize_certificate_pem(&cert_pem)?;

    <sigstore::cosign::Client as sigstore::cosign::CosignCapabilities>::verify_blob(
        &cert_pem,
        signature.trim(),
        &blob,
    )
    .map_err(|e| anyhow::anyhow!("blob signature verification failed: {e}"))?;

    verify_certificate_identity(&cert_pem)?;
    Ok(())
}

fn verify_certificate_identity(cert_pem: &str) -> anyhow::Result<()> {
    use x509_parser::extensions::GeneralName;
    use x509_parser::pem::parse_x509_pem;
    use x509_parser::prelude::*;

    let (_, pem) = parse_x509_pem(cert_pem.as_bytes())
        .map_err(|e| anyhow::anyhow!("failed to parse PEM certificate: {e}"))?;
    let (_, cert) = X509Certificate::from_der(&pem.contents)
        .map_err(|e| anyhow::anyhow!("failed to parse certificate DER: {e}"))?;

    let mut subject_uri: Option<String> = None;
    for ext in cert.extensions() {
        if let ParsedExtension::SubjectAlternativeName(san) = ext.parsed_extension() {
            for name in &san.general_names {
                if let GeneralName::URI(uri) = name {
                    let uri_str = uri.to_string();
                    if uri_str.starts_with("https://github.com/") {
                        subject_uri = Some(uri_str);
                        break;
                    }
                }
            }
        }
    }

    let subject_uri =
        subject_uri.ok_or_else(|| anyhow::anyhow!("certificate missing GitHub Actions SAN URI"))?;
    if !subject_uri
        .starts_with("https://github.com/styrene-lab/omegon/.github/workflows/release.yml@")
    {
        anyhow::bail!("certificate SAN URI does not match release workflow policy: {subject_uri}");
    }

    let issuer_oid = "1.3.6.1.4.1.57264.1.1";
    let issuer = cert
        .extensions()
        .iter()
        .find(|ext| ext.oid.to_id_string() == issuer_oid)
        .map(|ext| {
            String::from_utf8_lossy(ext.value)
                .trim_matches(char::from(0))
                .to_string()
        })
        .unwrap_or_default();
    if issuer != "https://token.actions.githubusercontent.com" {
        anyhow::bail!("certificate issuer policy failed: {issuer}");
    }

    let repo_oid = "1.3.6.1.4.1.57264.1.5";
    let repo = cert
        .extensions()
        .iter()
        .find(|ext| ext.oid.to_id_string() == repo_oid)
        .map(|ext| {
            String::from_utf8_lossy(ext.value)
                .trim_matches(char::from(0))
                .to_string()
        })
        .unwrap_or_default();
    if repo != "styrene-lab/omegon" {
        anyhow::bail!("certificate repository policy failed: {repo}");
    }

    Ok(())
}

/// Detect whether the running binary is managed by Homebrew.
///
/// Homebrew installs to paths like:
///   /opt/homebrew/Cellar/omegon/<version>/bin/omegon   (macOS arm64)
///   /usr/local/Cellar/omegon/<version>/bin/omegon       (macOS x86_64)
///   /home/linuxbrew/.linuxbrew/Cellar/omegon/...        (Linux)
///
/// In-place upgrade of a Cellar-managed binary corrupts brew's tracking —
/// brew still reports the old version after the binary is replaced.
pub fn is_homebrew_managed(exe: &Path) -> bool {
    exe.components()
        .any(|c| c.as_os_str() == "Cellar" || c.as_os_str() == ".linuxbrew")
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
struct InstallReceipt {
    version: Option<String>,
    binary: Option<PathBuf>,
    maintenance_binary: Option<PathBuf>,
    version_dir: Option<PathBuf>,
    versioned_binary: Option<PathBuf>,
    versioned_maintenance_binary: Option<PathBuf>,
    activation: Option<PathBuf>,
    layout: Option<String>,
}

impl InstallReceipt {
    fn versioned_binary_path(&self) -> Option<PathBuf> {
        self.versioned_binary.clone().or_else(|| {
            self.version_dir
                .as_ref()
                .map(|version_dir| version_dir.join("omegon"))
        })
    }

    fn versions_root(&self) -> Option<PathBuf> {
        self.version_dir.as_ref()?.parent().map(Path::to_path_buf)
    }

    fn maintenance_binary_path(&self) -> Option<PathBuf> {
        self.versioned_maintenance_binary.clone().or_else(|| {
            self.version_dir
                .as_ref()
                .map(|version_dir| version_dir.join("omegon-maintain"))
        })
    }
}

fn install_receipt_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| {
        home.join(".config")
            .join("omegon")
            .join("install-receipt.json")
    })
}

fn read_install_receipt() -> anyhow::Result<InstallReceipt> {
    let path = install_receipt_path().ok_or_else(|| anyhow::anyhow!("home directory not found"))?;
    let content = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

fn read_install_receipt_value() -> anyhow::Result<serde_json::Value> {
    let path = install_receipt_path().ok_or_else(|| anyhow::anyhow!("home directory not found"))?;
    Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
}

fn installed_release_layout(
    receipt: &InstallReceipt,
) -> anyhow::Result<crate::installed_release::InstalledReleaseLayout> {
    let versions_root = receipt
        .versions_root()
        .ok_or_else(|| anyhow::anyhow!("install receipt has no version directory"))?;
    let binary = receipt
        .binary
        .clone()
        .ok_or_else(|| anyhow::anyhow!("install receipt has no public binary launcher"))?;
    let maintenance = receipt
        .maintenance_binary
        .clone()
        .or_else(|| binary.parent().map(|parent| parent.join("omegon-maintain")))
        .ok_or_else(|| anyhow::anyhow!("install receipt has no maintenance launcher"))?;
    let receipt_path =
        install_receipt_path().ok_or_else(|| anyhow::anyhow!("home directory not found"))?;
    crate::installed_release::InstalledReleaseLayout::new(
        versions_root,
        binary,
        maintenance,
        receipt_path,
    )
}

fn generation_receipt(
    mut value: serde_json::Value,
    receipt: &InstallReceipt,
    layout: &crate::installed_release::InstalledReleaseLayout,
    version: &str,
    version_dir: &Path,
) -> serde_json::Value {
    value["version"] = serde_json::Value::String(version.to_string());
    value["binary"] = serde_json::Value::String(layout.binary_link.display().to_string());
    value["maintenance_binary"] =
        serde_json::Value::String(layout.maintenance_link.display().to_string());
    value["version_dir"] = serde_json::Value::String(version_dir.display().to_string());
    value["versioned_binary"] =
        serde_json::Value::String(version_dir.join("omegon").display().to_string());
    value["versioned_maintenance_binary"] =
        serde_json::Value::String(version_dir.join("omegon-maintain").display().to_string());
    value["activation"] = serde_json::Value::String(layout.current_link.display().to_string());
    value["layout"] = serde_json::Value::String("versioned-current-v1".into());
    value["installed_at"] = serde_json::Value::String(chrono::Utc::now().to_rfc3339());
    if value.get("source").is_none()
        && let Some(old_version) = receipt.version.as_deref()
    {
        value["source"] = serde_json::Value::String(format!("migrated-from:{old_version}"));
    }
    value
}

fn prepare_installed_release_layout(
    layout: &crate::installed_release::InstalledReleaseLayout,
    receipt: &InstallReceipt,
    raw_receipt: &serde_json::Value,
) -> anyhow::Result<()> {
    let old_dir = receipt
        .version_dir
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("install receipt has no active version directory"))?;
    let old_version = receipt
        .version
        .as_deref()
        .or_else(|| old_dir.file_name().and_then(|name| name.to_str()))
        .ok_or_else(|| anyhow::anyhow!("install receipt has no active version"))?;
    if !old_dir.join("install-receipt.json").is_file() {
        let value = generation_receipt(raw_receipt.clone(), receipt, layout, old_version, old_dir);
        crate::filelock::atomic_write(
            &old_dir.join("install-receipt.json"),
            (serde_json::to_string_pretty(&value)? + "\n").as_bytes(),
        )?;
    }
    crate::installed_release::validate_generation(old_dir)?;
    match layout.active_generation()? {
        Some(active) if paths_refer_to_same_file(&active, old_dir) => {}
        Some(active) => anyhow::bail!(
            "install receipt and active release disagree: {} != {}",
            old_dir.display(),
            active.display()
        ),
        None => layout.activate(old_dir)?,
    }
    layout.prepare_stable_links()
}

fn paths_refer_to_same_file(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

fn ensure_parent_writable(path: &Path, context: &str) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("{context} has no parent: {}", path.display()))?;
    let probe = parent.join(format!(".omegon-update-write-test-{}", std::process::id()));
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            Ok(())
        }
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(err) => Err(anyhow::anyhow!(
            "{context} is not writable: {} ({err}). Re-run the installer or update with elevated permissions.",
            parent.display()
        )),
    }
}

fn is_cargo_managed(exe: &Path) -> bool {
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".cargo")));
    cargo_home
        .as_ref()
        .is_some_and(|home| is_cargo_managed_with_home(exe, home))
}

fn is_cargo_managed_with_home(exe: &Path, cargo_home: &Path) -> bool {
    let cargo_bin = cargo_home.join("bin");
    exe.parent()
        .is_some_and(|parent| paths_refer_to_same_file(parent, &cargo_bin))
}

fn preflight_update_target(current_exe: &Path) -> anyhow::Result<()> {
    preflight_update_target_with_cargo_home(current_exe, std::env::var_os("CARGO_HOME"))
}

fn preflight_update_target_with_cargo_home(
    current_exe: &Path,
    cargo_home: Option<std::ffi::OsString>,
) -> anyhow::Result<()> {
    let cargo_home = cargo_home
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".cargo")));
    if cargo_home
        .as_ref()
        .is_some_and(|home| is_cargo_managed_with_home(current_exe, home))
    {
        anyhow::bail!(
            "This binary appears to be managed by Cargo. To upgrade, run:\n  cargo install --git https://github.com/styrene-lab/omegon --locked --force"
        );
    }
    ensure_parent_writable(current_exe, "running binary directory")
}

fn extract_release_pair(
    archive_path: &Path,
    omegon_path: &Path,
    maintenance_path: &Path,
) -> anyhow::Result<()> {
    let file = std::fs::File::open(archive_path)?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);
    let mut extracted_omegon = false;
    let mut extracted_maintenance = false;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;
        let (destination, extracted) = match path.as_ref() {
            path if path == Path::new("omegon") => (omegon_path, &mut extracted_omegon),
            path if path == Path::new("omegon-maintain") => {
                (maintenance_path, &mut extracted_maintenance)
            }
            _ => continue,
        };
        if *extracted || !entry.header().entry_type().is_file() {
            anyhow::bail!("Downloaded archive contains an invalid release companion pair");
        }
        let mut out = std::fs::File::create(destination)?;
        std::io::copy(&mut entry, &mut out)?;
        *extracted = true;
    }
    if !extracted_omegon || !extracted_maintenance {
        anyhow::bail!("Downloaded archive did not contain the complete release companion pair");
    }
    Ok(())
}

fn extract_release_assets(archive_path: &Path, generation: &Path) -> anyhow::Result<()> {
    let file = std::fs::File::open(archive_path)?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);
    let codescan_manifest = Path::new("share/omegon/extensions/omegon-codescan/manifest.toml");
    let codescan_binary =
        Path::new("share/omegon/extensions/omegon-codescan/target/release/omegon-codescan");
    let content_manifest = Path::new("share/omegon/content-packs/omegon-shipped/content-pack.toml");
    let mut extracted = std::collections::HashSet::new();

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let allowed = path == Path::new("omegon.composition-lock.json")
            || path == Path::new("omegon-maintain.composition-lock.json")
            || path.starts_with("share/omegon/content-packs/omegon-shipped")
            || path == codescan_manifest
            || path == codescan_binary;
        if !allowed {
            continue;
        }
        if !entry.header().entry_type().is_file() || !extracted.insert(path.clone()) {
            anyhow::bail!("Downloaded archive contains an invalid release asset");
        }
        let destination = generation.join(&path);
        let parent = destination
            .parent()
            .ok_or_else(|| anyhow::anyhow!("release asset has no parent"))?;
        std::fs::create_dir_all(parent)?;
        let mut output = std::fs::File::create(destination)?;
        std::io::copy(&mut entry, &mut output)?;
    }

    for required in [content_manifest, codescan_manifest, codescan_binary] {
        if !extracted.contains(required) {
            anyhow::bail!(
                "Downloaded archive did not contain release asset {}",
                required.display()
            );
        }
    }
    Ok(())
}

async fn validate_release_pair(
    omegon_path: &Path,
    maintenance_path: &Path,
    expected_version: &str,
) -> anyhow::Result<()> {
    let omegon_output = tokio::process::Command::new(omegon_path)
        .arg("--version")
        .output()
        .await?;
    if !omegon_output.status.success() {
        anyhow::bail!("Downloaded omegon binary failed --version check");
    }
    let version_output = String::from_utf8_lossy(&omegon_output.stdout);
    if !version_output.contains(expected_version) {
        anyhow::bail!(
            "Version mismatch: expected {}, got {}",
            expected_version,
            version_output.trim()
        );
    }

    let maintenance_output = tokio::process::Command::new(maintenance_path)
        .args(["--json", "identity"])
        .output()
        .await?;
    if !maintenance_output.status.success() {
        anyhow::bail!("Downloaded omegon-maintain binary failed identity check");
    }
    let identity: serde_json::Value = serde_json::from_slice(&maintenance_output.stdout)?;
    if identity["status"] != "success" || identity["artifact"]["version"] != expected_version {
        anyhow::bail!(
            "Maintenance companion identity mismatch: expected {}",
            expected_version
        );
    }
    Ok(())
}

/// Download, verify, and replace the current binary, then exec() into it.
/// Returns the path to the new binary on success (caller does the exec).
pub async fn download_and_replace(info: &UpdateInfo) -> anyhow::Result<PathBuf> {
    if info.download_url.is_empty() {
        anyhow::bail!("No download URL for this platform");
    }
    if info.signature_url.is_empty() || info.certificate_url.is_empty() {
        anyhow::bail!("Release is missing signature sidecars; refusing unverified install");
    }

    let current_exe = std::env::current_exe()?;

    if is_homebrew_managed(&current_exe) {
        let formula = "omegon";
        anyhow::bail!(
            "This binary is managed by Homebrew — in-place upgrade would corrupt brew's \
             version tracking.\n\nTo upgrade, run:\n  brew upgrade {formula}"
        );
    }
    preflight_update_target(&current_exe)?;
    let install_receipt = read_install_receipt().map_err(|error| {
        anyhow::anyhow!(
            "Self-update requires an installer-managed release layout ({error}). Re-run the installer or package-manager upgrade."
        )
    })?;
    let managed_install = install_receipt
        .versioned_binary_path()
        .is_some_and(|path| paths_refer_to_same_file(&path, &current_exe));
    if !managed_install {
        anyhow::bail!(
            "Self-update refuses to adopt a source, development, or unmanaged binary. Re-run the installer, or run `just link` from the owning checkout."
        );
    }
    let raw_receipt = read_install_receipt_value()?;
    let layout = installed_release_layout(&install_receipt)?;
    let _update_lock = crate::filelock::try_acquire_lock(&layout.current_link)
        .map_err(|error| anyhow::anyhow!("could not acquire update lock: {error}"))?
        .ok_or_else(|| anyhow::anyhow!("another release update is already active"))?;
    prepare_installed_release_layout(&layout, &install_receipt, &raw_receipt)?;

    crate::installed_release::validate_version_component(&info.latest)?;
    tokio::fs::create_dir_all(&layout.versions_root).await?;
    let work_dir = layout
        .versions_root
        .join(format!(".update-{}", uuid::Uuid::new_v4()));
    let candidate_dir = work_dir.join("candidate");
    tokio::fs::create_dir_all(&candidate_dir).await?;
    let tmp_path = candidate_dir.join("omegon");
    let maintenance_tmp_path = candidate_dir.join("omegon-maintain");
    let archive_path = work_dir.join("release.tar.gz");
    let signature_path = work_dir.join("release.tar.gz.sig");
    let certificate_path = work_dir.join("release.tar.gz.pem");

    tracing::info!(url = %info.download_url, "downloading update archive");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .user_agent(format!("omegon/{}", info.current))
        .build()?;

    download_to_path(&client, &info.download_url, &archive_path).await?;
    download_to_path(&client, &info.signature_url, &signature_path).await?;
    download_to_path(&client, &info.certificate_url, &certificate_path).await?;

    verify_archive_signature(&archive_path, &signature_path, &certificate_path)?;

    let archive_path_clone = archive_path.clone();
    let tmp_path_clone = tmp_path.clone();
    let maintenance_tmp_path_clone = maintenance_tmp_path.clone();
    let candidate_dir_clone = candidate_dir.clone();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        extract_release_pair(
            &archive_path_clone,
            &tmp_path_clone,
            &maintenance_tmp_path_clone,
        )?;
        extract_release_assets(&archive_path_clone, &candidate_dir_clone)
    })
    .await??;

    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o755)).await?;
        tokio::fs::set_permissions(
            &maintenance_tmp_path,
            std::fs::Permissions::from_mode(0o755),
        )
        .await?;
        tokio::fs::set_permissions(
            candidate_dir
                .join("share/omegon/extensions/omegon-codescan/target/release/omegon-codescan"),
            std::fs::Permissions::from_mode(0o755),
        )
        .await?;
    }

    validate_release_pair(&tmp_path, &maintenance_tmp_path, &info.latest).await?;
    let destination = layout.generation_dir(&info.latest)?;
    let receipt = generation_receipt(
        raw_receipt,
        &install_receipt,
        &layout,
        &info.latest,
        &destination,
    );
    tokio::fs::write(
        candidate_dir.join("install-receipt.json"),
        serde_json::to_string_pretty(&receipt)? + "\n",
    )
    .await?;
    crate::installed_release::validate_release_coupled_generation(&candidate_dir)?;
    let published = layout.publish_generation(&candidate_dir, &info.latest)?;
    crate::installed_release::validate_release_coupled_generation(&published)?;
    validate_release_pair(
        &published.join("omegon"),
        &published.join("omegon-maintain"),
        &info.latest,
    )
    .await?;
    layout.activate(&published)?;
    tokio::fs::remove_dir_all(&work_dir).await.ok();

    tracing::info!("release activated: {} → {}", info.current, info.latest);
    Ok(layout.current_link.join("omegon"))
}

/// Perform an exec() restart — replaces the current process with the new binary.
/// This preserves no state — the session will need to be resumed from disk.
#[cfg(unix)]
pub fn exec_restart(binary: &Path, args: &[String]) -> anyhow::Result<()> {
    use std::os::unix::process::CommandExt;
    let err = std::process::Command::new(binary).args(args).exec();
    // exec() only returns on error
    Err(err.into())
}

#[cfg(not(unix))]
pub fn exec_restart(binary: &Path, args: &[String]) -> anyhow::Result<()> {
    std::process::Command::new(binary).args(args).spawn()?;
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn homebrew_managed_detection() {
        assert!(is_homebrew_managed(Path::new(
            "/opt/homebrew/Cellar/omegon/0.15.4/bin/omegon"
        )));
        assert!(is_homebrew_managed(Path::new(
            "/usr/local/Cellar/omegon/0.15.4/bin/omegon"
        )));
        assert!(is_homebrew_managed(Path::new(
            "/home/linuxbrew/.linuxbrew/Cellar/omegon/0.15.4/bin/omegon"
        )));
        assert!(!is_homebrew_managed(Path::new("/usr/local/bin/omegon")));
        assert!(!is_homebrew_managed(Path::new(
            "/Users/cwilson/.local/bin/omegon"
        )));
        assert!(!is_homebrew_managed(Path::new(
            "/tmp/omegon-release-ws/core/target/release/omegon"
        )));
    }

    #[test]
    fn cargo_managed_detection_respects_cargo_home() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cargo_binary = temp.path().join("bin").join("omegon");
        let other_binary = temp.path().join("other").join("omegon");

        assert!(is_cargo_managed_with_home(&cargo_binary, temp.path()));
        assert!(!is_cargo_managed_with_home(&other_binary, temp.path()));
    }

    #[test]
    fn preflight_rejects_cargo_managed_binary() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cargo_bin = temp.path().join("bin");
        std::fs::create_dir_all(&cargo_bin).expect("cargo bin");
        let cargo_binary = cargo_bin.join("omegon");
        std::fs::write(&cargo_binary, "binary").expect("binary");

        let err = preflight_update_target_with_cargo_home(
            &cargo_binary,
            Some(temp.path().as_os_str().to_os_string()),
        )
        .expect_err("cargo install should reject");
        assert!(err.to_string().contains("managed by Cargo"), "{err}");
    }

    #[test]
    fn install_receipt_uses_installer_receipt_layout() {
        let receipt = InstallReceipt {
            version: Some("0.27.0".into()),
            binary: Some(PathBuf::from("/usr/local/bin/omegon")),
            maintenance_binary: Some(PathBuf::from("/usr/local/bin/omegon-maintain")),
            version_dir: Some(PathBuf::from("/home/me/.omegon/versions/0.27.0")),
            versioned_binary: Some(PathBuf::from("/home/me/.omegon/versions/0.27.0/omegon")),
            versioned_maintenance_binary: Some(PathBuf::from(
                "/home/me/.omegon/versions/0.27.0/omegon-maintain",
            )),
            activation: None,
            layout: None,
        };

        assert_eq!(
            receipt.versioned_binary_path().as_deref(),
            Some(Path::new("/home/me/.omegon/versions/0.27.0/omegon"))
        );
        assert_eq!(
            receipt.versions_root().as_deref(),
            Some(Path::new("/home/me/.omegon/versions"))
        );
    }

    #[test]
    fn install_receipt_derives_binary_from_version_dir_when_needed() {
        let receipt = InstallReceipt {
            version: Some("0.27.0".into()),
            binary: None,
            maintenance_binary: None,
            version_dir: Some(PathBuf::from("/home/me/.omegon/versions/0.27.0")),
            versioned_binary: None,
            versioned_maintenance_binary: None,
            activation: None,
            layout: None,
        };

        assert_eq!(
            receipt.versioned_binary_path().as_deref(),
            Some(Path::new("/home/me/.omegon/versions/0.27.0/omegon"))
        );
        assert_eq!(
            receipt.maintenance_binary_path().as_deref(),
            Some(Path::new(
                "/home/me/.omegon/versions/0.27.0/omegon-maintain"
            ))
        );
    }

    #[test]
    fn archive_extraction_rejects_missing_companion() {
        let temp = tempfile::tempdir().expect("tempdir");
        let archive_path = temp.path().join("release.tar.gz");
        let archive_file = std::fs::File::create(&archive_path).expect("archive");
        let encoder = flate2::write::GzEncoder::new(archive_file, flate2::Compression::default());
        let mut archive = tar::Builder::new(encoder);
        let bytes = b"omegon";
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        archive
            .append_data(&mut header, "omegon", &bytes[..])
            .expect("archive member");
        archive
            .into_inner()
            .expect("finish archive")
            .finish()
            .expect("gzip");

        let error = extract_release_pair(
            &archive_path,
            &temp.path().join("omegon.new"),
            &temp.path().join("omegon-maintain.new"),
        )
        .expect_err("missing companion must fail");
        assert!(
            error
                .to_string()
                .contains("complete release companion pair")
        );
    }

    #[test]
    fn update_info_requires_signed_archive_sidecars() {
        let mut info = UpdateInfo {
            current: "0.22.2".into(),
            latest: "0.22.3".into(),
            download_url: "https://example.invalid/omegon.tar.gz".into(),
            signature_url: "https://example.invalid/omegon.tar.gz.sig".into(),
            certificate_url: "https://example.invalid/omegon.tar.gz.pem".into(),
            release_notes: String::new(),
            is_newer: true,
        };
        assert!(info.has_downloadable_archive());

        info.signature_url.clear();
        assert!(!info.has_downloadable_archive());
    }

    #[test]
    fn rc_channel_parses_distinct_from_nightly() {
        // RC is deprecated — parses to Stable for backward compatibility
        assert_eq!(UpdateChannel::parse("rc"), Some(UpdateChannel::Stable));
        assert_eq!(
            UpdateChannel::parse("nightly"),
            Some(UpdateChannel::Nightly)
        );
        assert_ne!(UpdateChannel::parse("rc"), UpdateChannel::parse("nightly"));
    }

    #[test]
    fn version_comparison() {
        assert!(is_newer("0.15.2", "0.15.1"));
        assert!(is_newer("0.16.0", "0.15.2"));
        assert!(is_newer("1.0.0", "0.15.2"));
        assert!(!is_newer("0.15.1", "0.15.2"));
        assert!(!is_newer("0.15.2", "0.15.2"));
        assert!(is_newer("0.15.2", "0.15.2-rc.3"));
        assert!(!is_newer("0.15.1", "0.15.2-rc.3"));
        assert!(is_newer("0.15.3-rc.7", "0.15.2"));
        assert!(is_newer("0.15.3-nightly.20260326", "0.15.3-rc.7"));
        assert!(is_newer(
            "0.15.3-nightly.20260327",
            "0.15.3-nightly.20260326"
        ));
        assert!(is_newer("0.15.3", "0.15.3-nightly.20260327"));
    }

    #[test]
    fn platform_archive_target_is_valid() {
        let name = platform_archive_target();
        assert!(
            name.contains("darwin") || name.contains("linux"),
            "got: {name}"
        );
        assert!(
            name.contains("aarch64") || name.contains("x86_64"),
            "got: {name}"
        );
    }

    #[test]
    fn find_asset_url_matches_exact_suffix() {
        let assets = vec![
            GitHubAsset {
                name: "omegon-0.15.3-rc.7-aarch64-apple-darwin.tar.gz".into(),
                browser_download_url: "https://example.invalid/archive".into(),
            },
            GitHubAsset {
                name: "omegon-0.15.3-rc.7-aarch64-apple-darwin.tar.gz.sig".into(),
                browser_download_url: "https://example.invalid/archive.sig".into(),
            },
        ];
        assert_eq!(
            find_asset_url(
                &assets,
                "omegon-0.15.3-rc.7-aarch64-apple-darwin.tar.gz.sig"
            ),
            "https://example.invalid/archive.sig"
        );
    }

    #[test]
    fn certificate_identity_requires_repo_workflow_prefix() {
        let cert = "-----BEGIN CERTIFICATE-----\nMIIB\n-----END CERTIFICATE-----";
        let err = verify_certificate_identity(cert).expect_err("invalid cert should fail");
        assert!(
            err.to_string().contains("parse PEM certificate")
                || err.to_string().contains("parse certificate DER")
        );
    }

    #[test]
    fn normalize_certificate_pem_accepts_raw_pem() {
        let pem = "-----BEGIN CERTIFICATE-----\nMIIB\n-----END CERTIFICATE-----\n";
        let normalized = normalize_certificate_pem(pem).expect("raw pem should pass through");
        assert!(normalized.starts_with("-----BEGIN CERTIFICATE-----"));
        assert!(normalized.ends_with("-----END CERTIFICATE-----"));
    }

    #[test]
    fn normalize_certificate_pem_decodes_cosign_output() {
        use base64::Engine;
        let pem = "-----BEGIN CERTIFICATE-----\nMIIB\n-----END CERTIFICATE-----\n";
        let encoded = base64::engine::general_purpose::STANDARD.encode(pem);
        let normalized = normalize_certificate_pem(&encoded).expect("base64 pem should decode");
        assert_eq!(normalized, pem.trim());
    }

    #[test]
    fn normalize_certificate_pem_rejects_neither_pem_nor_base64() {
        let err =
            normalize_certificate_pem("definitely not a cert!!!").expect_err("garbage should fail");
        assert!(
            err.to_string().contains("neither PEM text nor base64"),
            "{err}"
        );
    }

    #[test]
    fn normalize_certificate_pem_rejects_base64_without_pem_boundary() {
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode("just some bytes");
        let err = normalize_certificate_pem(&encoded)
            .expect_err("base64 without PEM boundary should fail");
        assert!(
            err.to_string().contains("does not contain a PEM boundary"),
            "{err}"
        );
    }
}
