use std::path::{Path, PathBuf};

use anyhow::Context;
use chrono::{DateTime, Utc};
#[cfg(unix)]
use sha2::{Digest, Sha256};

use super::types::{WorkspaceLease, WorkspaceRegistry};

const STALE_HEARTBEAT_SECS: i64 = 300;

pub fn workspace_root(cwd: &Path) -> PathBuf {
    let canonical = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
    for ancestor in canonical.ancestors() {
        if ancestor.join(".git").is_file() && dirs::home_dir().as_deref() != Some(ancestor) {
            return ancestor.to_path_buf();
        }
        if ancestor.join(".git").is_dir() {
            break;
        }
    }
    crate::setup::find_project_root(&canonical)
}

pub fn runtime_dir(cwd: &Path) -> PathBuf {
    workspace_root(cwd).join(".omegon").join("runtime")
}

/// Per-instance lease path: `.omegon/runtime/{instance_id}/workspace.json`.
pub fn instance_lease_path(cwd: &Path, instance_id: &str) -> PathBuf {
    runtime_dir(cwd).join(instance_id).join("workspace.json")
}

/// Legacy shared lease path (pre-isolation). Used as read fallback.
pub fn workspace_lease_path(cwd: &Path) -> PathBuf {
    runtime_dir(cwd).join("workspace.json")
}

pub fn workspace_registry_path(cwd: &Path) -> PathBuf {
    runtime_dir(cwd).join("workspaces.json")
}

pub fn ensure_runtime_dir(cwd: &Path) -> anyhow::Result<PathBuf> {
    let dir = runtime_dir(cwd);
    std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    Ok(dir)
}

fn ensure_instance_dir(cwd: &Path, instance_id: &str) -> anyhow::Result<PathBuf> {
    let dir = runtime_dir(cwd).join(instance_id);
    std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    Ok(dir)
}

/// Read a workspace lease — checks instance-specific paths first, then legacy.
///
/// Returns the first active (non-stale) lease found, or any lease if all are stale.
pub fn read_workspace_lease(cwd: &Path) -> anyhow::Result<Option<WorkspaceLease>> {
    // Try instance-specific leases first
    let active = read_all_active_leases(cwd);
    if let Some((_id, lease)) = active.into_iter().next() {
        return Ok(Some(lease));
    }
    // Fallback to legacy shared path
    let path = workspace_lease_path(cwd);
    if !path.exists() {
        return Ok(None);
    }
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let lease = serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
    Ok(Some(lease))
}

/// Write workspace lease to the instance-specific path.
pub fn write_workspace_lease(
    cwd: &Path,
    instance_id: &str,
    lease: &WorkspaceLease,
) -> anyhow::Result<()> {
    ensure_instance_dir(cwd, instance_id)?;
    let path = instance_lease_path(cwd, instance_id);
    let json = serde_json::to_string_pretty(lease)?;
    crate::filelock::atomic_write(&path, json.as_bytes())
        .with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

/// Read all active (non-stale) instance leases.
pub fn read_all_active_leases(cwd: &Path) -> Vec<(String, WorkspaceLease)> {
    let rt_dir = runtime_dir(cwd);
    let now = Utc::now().timestamp();
    let mut leases = Vec::new();

    let entries = match std::fs::read_dir(&rt_dir) {
        Ok(e) => e,
        Err(_) => return leases,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let dir_name = match entry.file_name().into_string() {
            Ok(n) => n,
            Err(_) => continue,
        };
        // Must match {mode}-{pid} pattern
        if !dir_name.contains('-') {
            continue;
        }
        let lease_file = path.join("workspace.json");
        if !lease_file.exists() {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&lease_file) else {
            continue;
        };
        let Ok(lease) = serde_json::from_str::<WorkspaceLease>(&text) else {
            continue;
        };

        let ownership_fresh = ownership_heartbeat_is_fresh(&path, now);
        if ownership_fresh
            || heartbeat_epoch_secs(&lease.last_heartbeat)
                .is_some_and(|epoch| !heartbeat_is_stale(now, epoch))
        {
            let mut lease = lease;
            if ownership_fresh {
                lease.last_heartbeat = current_timestamp();
            }
            leases.push((dir_name, lease));
        }
    }

    leases
}

pub fn read_workspace_registry(cwd: &Path) -> anyhow::Result<Option<WorkspaceRegistry>> {
    let path = workspace_registry_path(cwd);
    if !path.exists() {
        return Ok(None);
    }
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let registry =
        serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
    Ok(Some(registry))
}

pub fn write_workspace_registry(cwd: &Path, registry: &WorkspaceRegistry) -> anyhow::Result<()> {
    ensure_runtime_dir(cwd)?;
    let path = workspace_registry_path(cwd);
    let json = serde_json::to_string_pretty(registry)?;
    crate::filelock::atomic_write_locked(&path, json.as_bytes())
        .with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

/// Remove this instance's runtime directory on clean shutdown.
pub fn cleanup_instance(cwd: &Path, instance_id: &str) {
    let dir = runtime_dir(cwd).join(instance_id);
    if dir.is_dir() {
        let _ = std::fs::remove_dir_all(&dir);
    }
}

pub struct RuntimeOwnership {
    runtime_id: String,
    runtime_directory: PathBuf,
    heartbeat: Option<tokio::task::JoinHandle<()>>,
    retain_evidence: std::sync::Arc<std::sync::atomic::AtomicBool>,
    managed_retention: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl RuntimeOwnership {
    pub fn start(cwd: &Path, mode: &str) -> anyhow::Result<Self> {
        let root = workspace_root(cwd).canonicalize()?;
        let runtime_id = format!("{mode}-{}-{}", std::process::id(), uuid::Uuid::new_v4());
        omegon_maintenance_contracts::validate_child_name(runtime_id.as_bytes())?;
        let record = ownership_record(&root, &runtime_id)?;
        let runtime_directory = runtime_dir(&root).join(&runtime_id);
        std::fs::create_dir_all(&runtime_directory)?;
        let directory = match omegon_maintenance_contracts::open_secure_root(&runtime_directory) {
            Ok(directory) => directory,
            Err(error) => {
                let _ = std::fs::remove_dir_all(&runtime_directory);
                return Err(error.into());
            }
        };
        let mut record = record;
        if let Err(error) = omegon_maintenance_contracts::replace_record_at(
            &directory,
            b"ownership-v1.json",
            &record,
            "runtime-start",
        ) {
            let _ = std::fs::remove_dir_all(&runtime_directory);
            return Err(error.into());
        }
        let heartbeat_directory = match directory.try_clone() {
            Ok(directory) => directory,
            Err(error) => {
                let _ = std::fs::remove_dir_all(&runtime_directory);
                return Err(error.into());
            }
        };
        let heartbeat = tokio::runtime::Handle::try_current().ok().map(|handle| {
            handle.spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                interval.tick().await;
                loop {
                    interval.tick().await;
                    let Some(ticks) = omegon_maintenance_contracts::current_monotonic_ns() else {
                        tracing::warn!("runtime ownership monotonic clock became unavailable");
                        continue;
                    };
                    if let Err(error) = record
                        .refresh_heartbeat(ownership_timestamp(), ticks)
                        .and_then(|()| {
                            omegon_maintenance_contracts::replace_record_at(
                                &heartbeat_directory,
                                b"ownership-v1.json",
                                &record,
                                "runtime-heartbeat",
                            )
                        })
                    {
                        tracing::warn!(%error, "could not refresh runtime ownership heartbeat");
                    }
                }
            })
        });
        Ok(Self {
            runtime_id,
            runtime_directory,
            heartbeat,
            retain_evidence: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            managed_retention: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
    }

    pub fn runtime_id(&self) -> &str {
        &self.runtime_id
    }

    pub(crate) fn retention_flag(&self) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        std::sync::Arc::clone(&self.managed_retention)
    }

    pub(crate) fn retain_for_stale_pruning(&self) {
        self.retain_evidence
            .store(true, std::sync::atomic::Ordering::Release);
    }

    #[cfg(test)]
    pub(crate) fn test_stub() -> Self {
        Self {
            runtime_id: "test-instance".into(),
            runtime_directory: std::env::temp_dir()
                .join(format!("omegon-runtime-test-stub-{}", uuid::Uuid::new_v4())),
            heartbeat: None,
            retain_evidence: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            managed_retention: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }
}

impl Drop for RuntimeOwnership {
    fn drop(&mut self) {
        if let Some(heartbeat) = self.heartbeat.take() {
            heartbeat.abort();
        }
        if self
            .retain_evidence
            .load(std::sync::atomic::Ordering::Acquire)
            || self
                .managed_retention
                .load(std::sync::atomic::Ordering::Acquire)
        {
            tracing::warn!(
                path = %self.runtime_directory.display(),
                "retaining degraded runtime ownership evidence for stale pruning"
            );
            return;
        }
        if let Err(error) = std::fs::remove_dir_all(&self.runtime_directory)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(path = %self.runtime_directory.display(), %error, "could not remove runtime ownership directory");
        }
    }
}

fn ownership_record(
    workspace_root: &Path,
    runtime_id: &str,
) -> anyhow::Result<omegon_maintenance_contracts::OwnershipRecordV1> {
    #[cfg(unix)]
    use std::os::unix::ffi::OsStrExt;

    #[cfg(not(unix))]
    anyhow::bail!("runtime ownership v1 requires Unix");
    #[cfg(unix)]
    {
        let authority_path = std::env::var_os("OMEGON_HOST_WORKSPACE")
            .map(PathBuf::from)
            .unwrap_or_else(|| workspace_root.to_path_buf());
        let normalized = omegon_maintenance_contracts::normalize_workspace_path(
            authority_path.as_os_str().as_bytes(),
        )?;
        let workspace_key = omegon_maintenance_contracts::workspace_key("unix", &normalized);
        let pid = std::process::id();
        let boot_id = omegon_maintenance_contracts::current_boot_id()
            .ok_or_else(|| anyhow::anyhow!("could not observe platform boot identity"))?;
        let process_start_token = match omegon_maintenance_contracts::observe_process_start(pid) {
            omegon_maintenance_contracts::ProcessObservation::Present(token) => token,
            evidence => anyhow::bail!("could not observe current process identity: {evidence:?}"),
        };
        let heartbeat_monotonic_ticks = omegon_maintenance_contracts::current_monotonic_ns()
            .ok_or_else(|| anyhow::anyhow!("could not read monotonic clock"))?;
        static EXECUTABLE_DIGEST: std::sync::OnceLock<omegon_maintenance_contracts::AuthorityKey> =
            std::sync::OnceLock::new();
        let digest = if let Some(digest) = EXECUTABLE_DIGEST.get() {
            *digest
        } else {
            let executable = std::env::current_exe()?;
            let digest = omegon_maintenance_contracts::AuthorityKey::from_bytes(
                Sha256::digest(std::fs::read(executable)?).into(),
            );
            let _ = EXECUTABLE_DIGEST.set(digest);
            digest
        };
        let writer = omegon_maintenance_contracts::ArtifactIdentityV1 {
            version: env!("CARGO_PKG_VERSION").into(),
            commit: env!("OMEGON_GIT_SHA").into(),
            target: env!("OMEGON_BUILD_TARGET").into(),
            digest,
        };
        let cross_boundary = std::env::var_os("OMEGON_INSIDE_OCI").is_some()
            || std::env::var_os("OMEGON_INSIDE_SANDBOX").is_some();
        Ok(omegon_maintenance_contracts::OwnershipRecordV1::new(
            runtime_id.to_string(),
            format!("generation-{}", uuid::Uuid::new_v4()),
            workspace_key,
            boot_id,
            pid,
            None,
            process_start_token,
            if cross_boundary {
                omegon_maintenance_contracts::LifecycleBoundary::CrossBoundary
            } else {
                omegon_maintenance_contracts::LifecycleBoundary::OwnedProcessTree
            },
            if cross_boundary {
                omegon_maintenance_contracts::CleanupCapability::Unverifiable
            } else {
                omegon_maintenance_contracts::CleanupCapability::BestEffort
            },
            writer,
            ownership_timestamp(),
            heartbeat_monotonic_ticks,
        )?)
    }
}

fn ownership_timestamp() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Prune stale instance directories (heartbeat older than 5 minutes AND PID dead).
pub fn prune_stale_instances(cwd: &Path) -> Vec<String> {
    let rt_dir = runtime_dir(cwd);
    let now = Utc::now().timestamp();
    let mut pruned = Vec::new();

    let entries = match std::fs::read_dir(&rt_dir) {
        Ok(e) => e,
        Err(_) => return pruned,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        // V1 ownership is pruned only by the maintenance evidence decision table.
        if path.join("ownership-v1.json").is_file() {
            continue;
        }
        let dir_name = match entry.file_name().into_string() {
            Ok(n) => n,
            Err(_) => continue,
        };
        if !dir_name.contains('-') {
            continue;
        }
        let lease_file = path.join("workspace.json");
        if !lease_file.exists() {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&lease_file) else {
            continue;
        };
        let Ok(lease) = serde_json::from_str::<WorkspaceLease>(&text) else {
            continue;
        };

        let stale = heartbeat_epoch_secs(&lease.last_heartbeat)
            .map(|epoch| heartbeat_is_stale(now, epoch))
            .unwrap_or(true);

        if !stale {
            continue;
        }

        // Check if the PID is still alive
        #[cfg(unix)]
        let pid_alive = dir_name
            .rsplit_once('-')
            .and_then(|(_, pid_str)| pid_str.parse::<i32>().ok())
            .map(|pid| unsafe { libc::kill(pid, 0) } == 0)
            .unwrap_or(false);
        // Without a secure process-identity probe, retain the directory rather than
        // risking deletion of a live runtime owned by another process.
        #[cfg(not(unix))]
        let pid_alive = true;

        if !pid_alive {
            let _ = std::fs::remove_dir_all(&path);
            pruned.push(dir_name);
        }
    }

    // Also clean up legacy workspace.json if stale
    let legacy_path = workspace_lease_path(cwd);
    if legacy_path.exists()
        && let Ok(text) = std::fs::read_to_string(&legacy_path)
        && let Ok(lease) = serde_json::from_str::<WorkspaceLease>(&text)
    {
        let stale = heartbeat_epoch_secs(&lease.last_heartbeat)
            .map(|epoch| heartbeat_is_stale(now, epoch))
            .unwrap_or(true);
        if stale {
            let _ = std::fs::remove_file(&legacy_path);
            pruned.push("legacy".to_string());
        }
    }

    pruned
}

fn ownership_heartbeat_is_fresh(runtime_directory: &Path, now: i64) -> bool {
    let Ok(bytes) = std::fs::read(runtime_directory.join("ownership-v1.json")) else {
        return false;
    };
    let Ok(record) = omegon_maintenance_contracts::parse_record::<
        omegon_maintenance_contracts::OwnershipRecordV1,
    >(&bytes) else {
        return false;
    };
    let Some(runtime_id) = runtime_directory.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    if record.runtime_id != runtime_id {
        return false;
    }
    let root = runtime_directory
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent);
    #[cfg(unix)]
    let workspace_matches = std::env::var_os("OMEGON_HOST_WORKSPACE")
        .map(PathBuf::from)
        .or_else(|| root.map(Path::to_path_buf))
        .is_some_and(|root| {
            use std::os::unix::ffi::OsStrExt;
            omegon_maintenance_contracts::normalize_workspace_path(root.as_os_str().as_bytes())
                .ok()
                .is_some_and(|path| {
                    record.workspace_key
                        == omegon_maintenance_contracts::workspace_key("unix", &path)
                })
        });
    #[cfg(not(unix))]
    let workspace_matches = false;
    if !workspace_matches {
        return false;
    }
    heartbeat_epoch_secs(&record.heartbeat_utc).is_some_and(|heartbeat| {
        heartbeat <= now.saturating_add(STALE_HEARTBEAT_SECS) && !heartbeat_is_stale(now, heartbeat)
    })
}

pub fn current_timestamp() -> String {
    Utc::now().to_rfc3339()
}

pub fn heartbeat_epoch_secs(heartbeat: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(heartbeat)
        .ok()
        .map(|dt| dt.timestamp())
}

pub fn workspace_id_from_path(path: &Path) -> String {
    let normalized = path
        .components()
        .filter_map(|component| {
            let text = component.as_os_str().to_string_lossy();
            if text == "/" || text.is_empty() {
                None
            } else {
                Some(text)
            }
        })
        .collect::<Vec<_>>()
        .join("::");
    if normalized.is_empty() {
        "root".into()
    } else {
        normalized
    }
}

pub fn heartbeat_is_stale(now_epoch_secs: i64, heartbeat_epoch_secs: i64) -> bool {
    now_epoch_secs.saturating_sub(heartbeat_epoch_secs) > STALE_HEARTBEAT_SECS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::types::{
        Mutability, WorkspaceBackendKind, WorkspaceKind, WorkspaceRole, WorkspaceSummary,
        WorkspaceVcsRef,
    };

    #[test]
    fn runtime_paths_are_under_workspace_root_runtime() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        let cwd = dir.path().join("nested/project");
        std::fs::create_dir_all(&cwd).unwrap();
        let root = dir.path().canonicalize().unwrap();
        assert_eq!(workspace_root(&cwd), root);
        assert_eq!(runtime_dir(&cwd), root.join(".omegon/runtime"));
        assert_eq!(
            workspace_lease_path(&cwd),
            root.join(".omegon/runtime/workspace.json")
        );
        assert_eq!(
            workspace_registry_path(&cwd),
            root.join(".omegon/runtime/workspaces.json")
        );
        assert_eq!(
            instance_lease_path(&cwd, "runtime-test"),
            root.join(".omegon/runtime/runtime-test/workspace.json")
        );
    }

    #[test]
    fn linked_worktree_is_its_own_workspace_root() {
        let main = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(main.path().join(".git/worktrees/linked")).unwrap();
        let linked = tempfile::tempdir().unwrap();
        std::fs::write(
            linked.path().join(".git"),
            format!(
                "gitdir: {}\n",
                main.path().join(".git/worktrees/linked").display()
            ),
        )
        .unwrap();
        let nested = linked.path().join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        assert_eq!(
            workspace_root(&nested),
            linked.path().canonicalize().unwrap()
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runtime_ownership_writes_valid_unique_record_and_cleans_up() {
        let project = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(project.path().join(".git")).unwrap();
        let nested = project.path().join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        let first = RuntimeOwnership::start(&nested, "test").unwrap();
        let second = RuntimeOwnership::start(&nested, "test").unwrap();
        assert_ne!(first.runtime_id(), second.runtime_id());
        let first_dir = runtime_dir(&nested).join(first.runtime_id());
        let bytes = std::fs::read(first_dir.join("ownership-v1.json")).unwrap();
        let record: omegon_maintenance_contracts::OwnershipRecordV1 =
            omegon_maintenance_contracts::parse_record(&bytes).unwrap();
        assert_eq!(record.runtime_id, first.runtime_id());
        assert_eq!(record.pid, std::process::id());
        assert_eq!(record.expires_after_seconds, 300);
        let second_dir = runtime_dir(&nested).join(second.runtime_id());
        drop(first);
        assert!(!first_dir.exists());
        assert!(second_dir.exists());
        drop(second);
        assert!(!second_dir.exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn degraded_runtime_ownership_is_retained_for_stale_pruning() {
        let project = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(project.path().join(".git")).unwrap();
        let ownership = RuntimeOwnership::start(project.path(), "test-degraded").unwrap();
        let directory = runtime_dir(project.path()).join(ownership.runtime_id());
        ownership.retain_for_stale_pruning();

        drop(ownership);

        assert!(directory.join("ownership-v1.json").exists());
    }

    #[test]
    fn workspace_id_is_deterministic_from_path() {
        assert_eq!(
            workspace_id_from_path(Path::new("/tmp/example-project")),
            "tmp::example-project"
        );
    }

    #[test]
    fn heartbeat_staleness_threshold_is_deterministic() {
        assert!(!heartbeat_is_stale(1_000, 701));
        assert!(heartbeat_is_stale(1_000, 699));
    }

    #[test]
    fn registry_round_trip_io() {
        let dir = tempfile::tempdir().unwrap();
        let registry = WorkspaceRegistry {
            project_id: "proj".into(),
            repo_root: dir.path().display().to_string(),
            workspaces: vec![WorkspaceSummary {
                workspace_id: "ws".into(),
                label: "primary".into(),
                path: dir.path().display().to_string(),
                backend_kind: WorkspaceBackendKind::LocalDir,
                vcs_ref: Some(WorkspaceVcsRef {
                    vcs: "git".into(),
                    branch: Some("main".into()),
                    revision: None,
                    remote: Some("origin".into()),
                }),
                bindings: crate::workspace::types::WorkspaceBindings::default(),
                branch: "main".into(),
                role: WorkspaceRole::Primary,
                workspace_kind: WorkspaceKind::Mixed,
                mutability: Mutability::Mutable,
                owner_session_id: Some("s1".into()),
                last_heartbeat: current_timestamp(),
                archived: false,
                archived_at: None,
                archive_reason: None,
                stale: false,
            }],
        };
        write_workspace_registry(dir.path(), &registry).unwrap();
        let loaded = read_workspace_registry(dir.path()).unwrap().unwrap();
        assert_eq!(loaded, registry);
    }

    fn make_lease(path: &str, heartbeat: &str) -> WorkspaceLease {
        WorkspaceLease {
            project_id: "test-project".into(),
            workspace_id: "ws".into(),
            label: "test".into(),
            path: path.into(),
            backend_kind: WorkspaceBackendKind::LocalDir,
            vcs_ref: None,
            bindings: crate::workspace::types::WorkspaceBindings::default(),
            branch: "main".into(),
            role: WorkspaceRole::Primary,
            workspace_kind: WorkspaceKind::Mixed,
            mutability: Mutability::Mutable,
            owner_session_id: Some("s1".into()),
            owner_agent_id: None,
            created_at: current_timestamp(),
            last_heartbeat: heartbeat.into(),
            source: "test".into(),
            archived: false,
            archived_at: None,
            archive_reason: None,
            parent_workspace_id: None,
        }
    }

    #[test]
    fn instance_lease_path_is_namespaced() {
        let dir = tempfile::tempdir().unwrap();
        let path = instance_lease_path(dir.path(), "tui-123");
        assert!(path.to_string_lossy().contains("tui-123"));
        assert!(path.to_string_lossy().ends_with("workspace.json"));
    }

    #[test]
    fn two_instances_write_separate_leases() {
        let dir = tempfile::tempdir().unwrap();
        let lease_a = make_lease("/a", &current_timestamp());
        let lease_b = make_lease("/b", &current_timestamp());

        write_workspace_lease(dir.path(), "tui-111", &lease_a).unwrap();
        write_workspace_lease(dir.path(), "acp-222", &lease_b).unwrap();

        let path_a = instance_lease_path(dir.path(), "tui-111");
        let path_b = instance_lease_path(dir.path(), "acp-222");
        assert!(path_a.exists());
        assert!(path_b.exists());
        assert_ne!(path_a, path_b);
    }

    #[test]
    fn read_all_active_leases_finds_both() {
        let dir = tempfile::tempdir().unwrap();
        let now = current_timestamp();
        let lease_a = make_lease("/a", &now);
        let lease_b = make_lease("/b", &now);

        write_workspace_lease(dir.path(), "tui-111", &lease_a).unwrap();
        write_workspace_lease(dir.path(), "acp-222", &lease_b).unwrap();

        let active = read_all_active_leases(dir.path());
        assert_eq!(active.len(), 2);
        let ids: Vec<&str> = active.iter().map(|(id, _)| id.as_str()).collect();
        assert!(ids.contains(&"tui-111"));
        assert!(ids.contains(&"acp-222"));
    }

    #[test]
    fn cleanup_instance_removes_dir() {
        let dir = tempfile::tempdir().unwrap();
        let lease = make_lease("/x", &current_timestamp());
        write_workspace_lease(dir.path(), "tui-999", &lease).unwrap();

        let inst_dir = crate::paths::runtime_instance_dir(dir.path(), "tui-999");
        assert!(inst_dir.is_dir());

        cleanup_instance(dir.path(), "tui-999");
        assert!(!inst_dir.exists());
    }

    #[test]
    fn prune_stale_removes_old_dirs() {
        let dir = tempfile::tempdir().unwrap();
        // Create a lease with a very old heartbeat (stale)
        let stale_lease = make_lease("/old", "2020-01-01T00:00:00Z");
        // pid 99999999 is almost certainly not running
        let inst_dir = crate::paths::runtime_instance_dir(dir.path(), "tui-99999999");
        std::fs::create_dir_all(&inst_dir).unwrap();
        let json = serde_json::to_string_pretty(&stale_lease).unwrap();
        std::fs::write(inst_dir.join("workspace.json"), json).unwrap();

        let pruned = prune_stale_instances(dir.path());
        assert!(pruned.contains(&"tui-99999999".to_string()));
        assert!(!inst_dir.exists());
    }

    #[test]
    fn read_workspace_lease_reads_instance_leases() {
        let dir = tempfile::tempdir().unwrap();
        let lease = make_lease("/test", &current_timestamp());
        write_workspace_lease(dir.path(), "tui-111", &lease).unwrap();

        let loaded = read_workspace_lease(dir.path()).unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().path, "/test");
    }

    #[test]
    fn read_workspace_lease_falls_back_to_legacy() {
        let dir = tempfile::tempdir().unwrap();
        // Write to legacy path directly
        let rt = runtime_dir(dir.path());
        std::fs::create_dir_all(&rt).unwrap();
        let lease = make_lease("/legacy", &current_timestamp());
        let json = serde_json::to_string_pretty(&lease).unwrap();
        std::fs::write(rt.join("workspace.json"), json).unwrap();

        let loaded = read_workspace_lease(dir.path()).unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().path, "/legacy");
    }
}
