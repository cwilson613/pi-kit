//! Update checker — polls GitHub Releases API for new versions.
//!
//! At startup, spawns an async task that checks for newer releases.
//! Results are surfaced as a banner in the TUI footer.
//! The `/update` command triggers download + replace + exec restart.

use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::watch;

/// Version comparison result.
#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub download_url: String,
    pub release_notes: String,
    pub is_newer: bool,
}

/// Shared state for the update checker.
pub type UpdateReceiver = watch::Receiver<Option<UpdateInfo>>;
pub type UpdateSender = watch::Sender<Option<UpdateInfo>>;

/// Create the update channel.
pub fn channel() -> (UpdateSender, UpdateReceiver) {
    watch::channel(None)
}

/// GitHub release info (minimal subset).
#[derive(serde::Deserialize)]
struct GitHubRelease {
    tag_name: String,
    body: Option<String>,
    assets: Vec<GitHubAsset>,
}

#[derive(serde::Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

/// Spawn the background update check.
pub fn spawn_check(tx: UpdateSender) {
    let current = env!("CARGO_PKG_VERSION").to_string();
    tokio::spawn(async move {
        // Delay slightly so startup isn't blocked
        tokio::time::sleep(Duration::from_secs(5)).await;

        match check_latest(&current).await {
            Ok(Some(info)) => {
                tracing::info!(
                    current = %info.current,
                    latest = %info.latest,
                    "new version available"
                );
                let _ = tx.send(Some(info));
            }
            Ok(None) => {
                tracing::debug!("up to date");
            }
            Err(e) => {
                tracing::debug!("update check failed (non-fatal): {e}");
            }
        }
    });
}

/// Check GitHub Releases for a newer version.
async fn check_latest(current: &str) -> anyhow::Result<Option<UpdateInfo>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent(format!("omegon/{current}"))
        .build()?;

    let resp: GitHubRelease = client
        .get("https://api.github.com/repos/styrene-lab/omegon/releases/latest")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let latest = resp.tag_name.trim_start_matches('v').to_string();

    if !is_newer(&latest, current) {
        return Ok(None);
    }

    // Find the right asset for this platform
    let target = platform_asset_name();
    let download_url = resp
        .assets
        .iter()
        .find(|a| a.name.contains(&target))
        .map(|a| a.browser_download_url.clone())
        .unwrap_or_default();

    Ok(Some(UpdateInfo {
        current: current.to_string(),
        latest,
        download_url,
        release_notes: resp.body.unwrap_or_default(),
        is_newer: true,
    }))
}

/// Semver comparison: is `latest` newer than `current`?
/// A stable release (0.15.2) is newer than its own RC (0.15.2-rc.3).
fn is_newer(latest: &str, current: &str) -> bool {
    let parse = |s: &str| -> (Vec<u32>, bool) {
        let is_rc = s.contains("-rc");
        let base = s.split('-').next().unwrap_or(s);
        let parts: Vec<u32> = base.split('.').filter_map(|p| p.parse().ok()).collect();
        (parts, is_rc)
    };
    let (l, l_rc) = parse(latest);
    let (c, c_rc) = parse(current);
    match l.cmp(&c) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        // Same base version: stable > rc
        std::cmp::Ordering::Equal => c_rc && !l_rc,
    }
}

/// Platform-specific asset name pattern.
fn platform_asset_name() -> String {
    let os = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "unknown"
    };
    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else {
        "unknown"
    };
    format!("omegon-{os}-{arch}")
}

/// Download, verify, and replace the current binary, then exec() into it.
/// Returns the path to the new binary on success (caller does the exec).
pub async fn download_and_replace(info: &UpdateInfo) -> anyhow::Result<PathBuf> {
    if info.download_url.is_empty() {
        anyhow::bail!("No download URL for this platform");
    }

    let current_exe = std::env::current_exe()?;
    let tmp_path = current_exe.with_extension("new");
    let backup_path = current_exe.with_extension("bak");

    tracing::info!(url = %info.download_url, "downloading update");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .user_agent(format!("omegon/{}", info.current))
        .build()?;

    let bytes = client
        .get(&info.download_url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;

    // Write to temp file
    tokio::fs::write(&tmp_path, &bytes).await?;

    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o755)).await?;
    }

    // Verify the new binary runs
    let output = tokio::process::Command::new(&tmp_path)
        .arg("--version")
        .output()
        .await?;

    if !output.status.success() {
        tokio::fs::remove_file(&tmp_path).await.ok();
        anyhow::bail!("Downloaded binary failed --version check");
    }

    let version_output = String::from_utf8_lossy(&output.stdout);
    if !version_output.contains(&info.latest) {
        tokio::fs::remove_file(&tmp_path).await.ok();
        anyhow::bail!(
            "Version mismatch: expected {}, got {}",
            info.latest,
            version_output.trim()
        );
    }

    // Atomic replace: current → backup, new → current
    if backup_path.exists() {
        tokio::fs::remove_file(&backup_path).await.ok();
    }
    tokio::fs::rename(&current_exe, &backup_path).await?;
    tokio::fs::rename(&tmp_path, &current_exe).await?;

    tracing::info!("binary replaced: {} → {}", info.current, info.latest);
    Ok(current_exe)
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
    fn version_comparison() {
        assert!(is_newer("0.15.2", "0.15.1"));
        assert!(is_newer("0.16.0", "0.15.2"));
        assert!(is_newer("1.0.0", "0.15.2"));
        assert!(!is_newer("0.15.1", "0.15.2"));
        assert!(!is_newer("0.15.2", "0.15.2"));
        // RC versions: strip suffix for comparison
        assert!(is_newer("0.15.2", "0.15.2-rc.3"));
        assert!(!is_newer("0.15.1", "0.15.2-rc.3"));
    }

    #[test]
    fn platform_asset_name_is_valid() {
        let name = platform_asset_name();
        assert!(name.starts_with("omegon-"), "got: {name}");
        assert!(
            name.contains("darwin") || name.contains("linux"),
            "got: {name}"
        );
    }
}
