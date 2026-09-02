use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::{
    ExtensionManifest, ExtensionProcessState, SpawnedExtension, dynamic_preflight,
    spawn_from_admitted_snapshot,
};
use crate::contribution_lifecycle::{DiscoveredContributionState, DynamicContributionInventory};

struct ConformanceRun {
    _root: tempfile::TempDir,
    control: PathBuf,
    marker: PathBuf,
    inventory: DynamicContributionInventory,
    id: omegon_traits::RuntimeContributionId,
    spawned: SpawnedExtension,
}

struct FixturePaths {
    extension_dir: PathBuf,
    control: PathBuf,
    marker: PathBuf,
}

struct GenerationBumpFeature;

#[async_trait::async_trait]
impl omegon_traits::Feature for GenerationBumpFeature {
    fn name(&self) -> &str {
        "conformance-generation-bump"
    }
}

impl ConformanceRun {
    async fn shutdown(self) {
        self.spawned
            .supervisor
            .shutdown(std::time::Duration::from_millis(500))
            .await
            .unwrap();
    }
}

fn write_fixture(root: &Path, mode: &str) -> Result<FixturePaths> {
    write_fixture_with_marker(root, mode, root.join("fixture-started.json"))
}

fn write_fixture_with_marker(root: &Path, mode: &str, marker: PathBuf) -> Result<FixturePaths> {
    let extension_dir = root.join("extensions/native-conformance-fixture");
    let control = root.join("fixture-mode.txt");
    std::fs::create_dir_all(&extension_dir)?;
    std::fs::write(&control, mode)?;
    let fixture = extension_dir.join("fixture.py");
    std::fs::write(
        &fixture,
        include_str!("../../tests/fixtures/native_extension_conformance.py"),
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&fixture)?.permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&fixture, permissions)?;
    }
    std::fs::write(
        extension_dir.join("manifest.toml"),
        format!(
            r#"[extension]
name = "native-conformance-fixture"
version = "0.1.0"
description = "Deterministic native extension conformance fixture"

[runtime]
type = "native"
binary = "fixture.py"

[runtime.env]
OMEGON_FIXTURE_MODE = "{mode}"
OMEGON_FIXTURE_CONTROL = "{}"
OMEGON_FIXTURE_MARKER = "{}"

[startup]
ping_method = "fixture/status"
# Includes /usr/bin/env and interpreter startup on supported macOS hosts.
timeout_ms = 5000

[config.fixture_value]
type = "string"
label = "Fixture value"
description = "Exercises bootstrap_config"
default = "ready"
"#,
            control.display(),
            marker.display(),
        ),
    )?;
    Ok(FixturePaths {
        extension_dir,
        control,
        marker,
    })
}

async fn run_fixture(mode: &str) -> Result<ConformanceRun> {
    let root = tempfile::tempdir()?;
    let paths = write_fixture(root.path(), mode)?;
    run_extension(root, paths.extension_dir, paths.control, paths.marker).await
}

async fn run_fixture_with_marker(mode: &str, marker: PathBuf) -> Result<ConformanceRun> {
    let root = tempfile::tempdir()?;
    let paths = write_fixture_with_marker(root.path(), mode, marker)?;
    run_extension(root, paths.extension_dir, paths.control, paths.marker).await
}

async fn run_extension(
    root: tempfile::TempDir,
    extension_dir: PathBuf,
    control: PathBuf,
    marker: PathBuf,
) -> Result<ConformanceRun> {
    run_extension_with_inventory(
        root,
        extension_dir,
        control,
        marker,
        DynamicContributionInventory::default(),
        true,
    )
    .await
}

async fn run_extension_with_inventory(
    root: tempfile::TempDir,
    extension_dir: PathBuf,
    control: PathBuf,
    marker: PathBuf,
    inventory: DynamicContributionInventory,
    publish: bool,
) -> Result<ConformanceRun> {
    let source = std::fs::File::open(&extension_dir)?;
    let snapshot = Arc::new(crate::contribution_loading::snapshot_contribution_directory(&source)?);
    let manifest = ExtensionManifest::from_extension_dir(snapshot.path())?;
    let preflight = dynamic_preflight(&manifest, snapshot.path())?;
    let id = preflight.id.clone();
    let candidate = inventory.discover(preflight)?;
    let mut profile = crate::settings::Profile::default();
    profile
        .permissions
        .trusted_contribution_code
        .push(id.as_str().to_string());
    let policy = crate::dynamic_admission::DynamicAdmissionPolicy::from_profile(&profile);
    let admission = inventory.admit(&candidate, &policy)?;
    let spawned = match spawn_from_admitted_snapshot(
        snapshot,
        &extension_dir,
        admission,
        inventory.clone(),
        root.path(),
        &[],
    )
    .await
    {
        Ok(spawned) => spawned,
        Err(error) => {
            inventory.quarantine(&id, error.to_string());
            return Err(error);
        }
    };
    if spawned.sdk_compatibility.status != super::sdk_compat::SdkCompatibilityStatus::Supported {
        let reason = format!(
            "first-party conformance requires SDK contract version {}: {}",
            super::sdk_compat::SUPPORTED_SDK_CONTRACT_VERSION,
            spawned.sdk_compatibility.message
        );
        inventory.quarantine(&id, &reason);
        spawned
            .supervisor
            .shutdown(std::time::Duration::from_millis(500))
            .await?;
        return Err(anyhow!(reason));
    }
    inventory.ready(&id);
    if publish {
        inventory.stage_ready();
        inventory.publish_staged();
    }
    Ok(ConformanceRun {
        _root: root,
        control,
        marker,
        inventory,
        id,
        spawned,
    })
}

#[cfg(unix)]
fn install_first_party_extension(root: &Path, source: &Path) -> Result<(PathBuf, String)> {
    let manifest = ExtensionManifest::from_extension_dir(source)?;
    let binary = match &manifest.runtime {
        super::RuntimeConfig::Native { binary, .. } => binary,
        super::RuntimeConfig::Oci { .. } => anyhow::bail!("first-party fixture is not native"),
    };
    let extension_dir = root.join("extensions").join(&manifest.extension.name);
    let destination = extension_dir.join(binary);
    std::fs::create_dir_all(destination.parent().expect("binary has a parent"))?;
    std::fs::copy(
        source.join("manifest.toml"),
        extension_dir.join("manifest.toml"),
    )?;
    std::fs::copy(source.join(binary), &destination).map_err(|error| {
        anyhow!(
            "build first-party extension '{}' before running this ignored test: {error}",
            manifest.extension.name
        )
    })?;
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(&destination)?.permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(destination, permissions)?;
    Ok((extension_dir, manifest.extension.name))
}

async fn invoke_fixture(run: &ConformanceRun) -> Result<omegon_traits::ToolResult> {
    run.spawned
        .feature
        .execute(
            "fixture_echo",
            "conformance-call",
            json!({"value": "probe"}),
            CancellationToken::new(),
        )
        .await
}

async fn wait_for_marker(path: &Path) -> Result<Value> {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            match std::fs::read(path) {
                Ok(bytes) => return serde_json::from_slice(&bytes).map_err(Into::into),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                Err(error) => return Err(error.into()),
            }
        }
    })
    .await
    .map_err(|_| anyhow!("fixture did not write its invocation marker"))?
}

#[cfg(unix)]
fn process_exists(pid: u32) -> bool {
    // SAFETY: signal 0 only probes whether the process exists.
    (unsafe { libc::kill(pid as i32, 0) == 0 })
        || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

#[cfg(unix)]
async fn wait_for_process_exit(pid: u32) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while process_exists(pid) && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        !process_exists(pid),
        "fixture process {pid} survived cleanup"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn compatible_fixture_is_admitted_and_published_once() {
    let _env_guard = crate::test_support::env::lock_async().await;
    unsafe {
        std::env::remove_var("OMEGON_RUNTIME_CONTEXT");
        std::env::remove_var("KUBERNETES_SERVICE_HOST");
    }
    let run = run_fixture("compatible").await.unwrap();
    let evidence = run.inventory.evidence();
    assert_eq!(evidence.len(), 1);
    assert_eq!(evidence[0].candidate.preflight.id, run.id);
    assert_eq!(evidence[0].state, DiscoveredContributionState::Published);
    assert_eq!(
        run.spawned.sdk_compatibility.status,
        super::sdk_compat::SdkCompatibilityStatus::Supported
    );
    run.shutdown().await;
}

#[cfg(unix)]
#[tokio::test]
#[ignore = "slow: requires release-built first-party native extensions"]
async fn every_first_party_native_extension_passes_the_host_handshake() {
    let _env_guard = crate::test_support::env::lock_async().await;
    unsafe {
        std::env::remove_var("OMEGON_RUNTIME_CONTEXT");
        std::env::remove_var("KUBERNETES_SERVICE_HOST");
    }
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let extensions = repository.join("extensions");
    let mut sources = std::fs::read_dir(&extensions)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.join("manifest.toml").is_file())
        .filter(|path| {
            ExtensionManifest::from_extension_dir(path).is_ok_and(|manifest| {
                matches!(manifest.runtime, super::RuntimeConfig::Native { .. })
            })
        })
        .collect::<Vec<_>>();
    sources.sort();
    assert!(
        !sources.is_empty(),
        "no first-party native extensions found"
    );

    for source in sources {
        let root = tempfile::tempdir().unwrap();
        let (extension_dir, name) = install_first_party_extension(root.path(), &source).unwrap();
        let control = root.path().join("unused-control");
        let marker = root.path().join("unused-marker");
        let run = run_extension(root, extension_dir, control, marker)
            .await
            .unwrap_or_else(|error| panic!("{name} failed host conformance: {error:#}"));
        assert_eq!(run.spawned.rpc_polling_handle.extension_name(), name);
        assert_eq!(
            run.spawned.sdk_compatibility.status,
            super::sdk_compat::SdkCompatibilityStatus::Supported
        );
        run.shutdown().await;
    }
}

#[cfg(unix)]
#[test]
fn contribution_snapshot_accepts_a_release_sized_native_binary() {
    let source = tempfile::tempdir().unwrap();
    let binary = std::fs::File::create(source.path().join("extension")).unwrap();
    binary.set_len(17 * 1024 * 1024).unwrap();
    let directory = std::fs::File::open(source.path()).unwrap();

    let snapshot = crate::contribution_loading::snapshot_contribution_directory(&directory)
        .expect("release-sized native extension should fit the aggregate snapshot limit");

    assert_eq!(
        std::fs::metadata(snapshot.path().join("extension"))
            .unwrap()
            .len(),
        17 * 1024 * 1024
    );
}

#[cfg(unix)]
#[tokio::test]
async fn incompatible_fixture_matrix_is_refused_before_publication() {
    let _env_guard = crate::test_support::env::lock_async().await;
    unsafe {
        std::env::remove_var("OMEGON_RUNTIME_CONTEXT");
        std::env::remove_var("KUBERNETES_SERVICE_HOST");
    }
    for (mode, expected) in [
        ("missing_sdk", "SDK contract version"),
        ("unsupported_sdk", "SDK contract is incompatible"),
        ("malformed_tools", "missing non-empty name"),
        ("bootstrap_failure", "failed to accept bootstrap_config"),
        (
            "readiness_failure",
            "readiness probe 'fixture/status' failed",
        ),
    ] {
        let error = match run_fixture(mode).await {
            Ok(run) => {
                run.shutdown().await;
                anyhow!("{mode} fixture was published")
            }
            Err(error) => error,
        };
        assert!(
            error.to_string().contains(expected),
            "{mode}: unexpected error: {error:#}"
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn readiness_refusal_settles_the_candidate_process_tree() {
    let _env_guard = crate::test_support::env::lock_async().await;
    unsafe {
        std::env::remove_var("OMEGON_RUNTIME_CONTEXT");
        std::env::remove_var("KUBERNETES_SERVICE_HOST");
    }
    let evidence = tempfile::tempdir().unwrap();
    let marker = evidence.path().join("readiness-child.json");
    let error =
        match run_fixture_with_marker("readiness_failure,readiness_child_process", marker.clone())
            .await
        {
            Ok(run) => {
                run.shutdown().await;
                panic!("readiness failure fixture was published")
            }
            Err(error) => error,
        };
    assert!(
        error
            .to_string()
            .contains("readiness probe 'fixture/status' failed"),
        "{error:#}"
    );
    let marker = wait_for_marker(&marker).await.unwrap();
    wait_for_process_exit(marker["child_pid"].as_u64().unwrap() as u32).await;
}

#[cfg(unix)]
#[tokio::test]
async fn admitted_fixture_invokes_and_active_request_cancels() {
    let _env_guard = crate::test_support::env::lock_async().await;
    unsafe {
        std::env::remove_var("OMEGON_RUNTIME_CONTEXT");
        std::env::remove_var("KUBERNETES_SERVICE_HOST");
    }

    let run = run_fixture("compatible").await.unwrap();
    let result = invoke_fixture(&run).await.unwrap();
    assert_eq!(result.content.len(), 1);
    run.shutdown().await;

    let run = run_fixture("delay,child_process").await.unwrap();
    let cancel = CancellationToken::new();
    let error = {
        let invocation = run.spawned.feature.execute(
            "fixture_echo",
            "cancelled-conformance-call",
            json!({}),
            cancel.clone(),
        );
        tokio::pin!(invocation);
        let marker = tokio::select! {
            result = &mut invocation => panic!("fixture invocation ended before cancellation: {result:?}"),
            marker = wait_for_marker(&run.marker) => marker.unwrap(),
        };
        cancel.cancel();
        let error = tokio::time::timeout(Duration::from_secs(1), &mut invocation)
            .await
            .expect("cancelled invocation did not settle")
            .unwrap_err();
        let child_pid = marker["child_pid"].as_u64().unwrap() as u32;
        wait_for_process_exit(child_pid).await;
        run.spawned
            .rpc_polling_handle
            .rpc_call("fixture/status", json!({}))
            .await
            .expect("fixture remained usable after request cancellation");
        error
    };
    assert!(error.to_string().contains("cancelled"), "{error:#}");
    run.shutdown().await;
}

#[cfg(unix)]
#[tokio::test]
async fn bounded_model_scope_refuses_native_owner_entry_above_budget() {
    let _env_guard = crate::test_support::env::lock_async().await;
    unsafe {
        std::env::remove_var("OMEGON_RUNTIME_CONTEXT");
        std::env::remove_var("KUBERNETES_SERVICE_HOST");
    }

    let run = run_fixture("compatible").await.unwrap();
    let supervisor = run.spawned.supervisor.clone();
    let marker = run.marker.clone();
    let budget = omegon_kernel_runtime::ToolInvocationBudget::new(Some(1));
    let scope = crate::invocation_service::InvocationScope {
        tool_budget: Some(budget.clone()),
        ..Default::default()
    };
    let mut bus = crate::bus::EventBus::new();
    bus.register(run.spawned.feature);
    bus.finalize();

    bus.invoke_tool(
        "fixture_echo",
        "bounded-native-1",
        json!({}),
        CancellationToken::new(),
        scope.clone(),
    )
    .await
    .expect("exact-boundary invocation must reach the native owner");
    assert!(
        marker.exists(),
        "admitted invocation did not enter its owner"
    );
    std::fs::remove_file(&marker).unwrap();

    let error = bus
        .invoke_tool(
            "fixture_echo",
            "bounded-native-2",
            json!({}),
            CancellationToken::new(),
            scope,
        )
        .await
        .expect_err("one-above invocation must fail before native RPC");
    assert!(
        error.to_string().contains("invocation:budget_exhausted"),
        "{error:#}"
    );
    assert_eq!(budget.observed(), 1);
    assert!(!marker.exists(), "exhausted invocation entered its owner");
    assert_eq!(supervisor.health().state, ExtensionProcessState::Healthy);
    supervisor
        .shutdown(Duration::from_millis(500))
        .await
        .unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn replacement_enforces_tool_shape_and_preserves_stable_invocation() {
    let _env_guard = crate::test_support::env::lock_async().await;
    unsafe {
        std::env::remove_var("OMEGON_RUNTIME_CONTEXT");
        std::env::remove_var("KUBERNETES_SERVICE_HOST");
    }

    let run = run_fixture("child_process").await.unwrap();
    invoke_fixture(&run).await.unwrap();
    let marker = wait_for_marker(&run.marker).await.unwrap();
    let child_pid = marker["child_pid"].as_u64().unwrap() as u32;
    let first_pid = run.spawned.supervisor.health().pid.unwrap();
    let replacement_pid = run.spawned.supervisor.replace().await.unwrap();
    assert_ne!(replacement_pid, first_pid);
    wait_for_process_exit(child_pid).await;
    wait_for_process_exit(first_pid).await;
    invoke_fixture(&run).await.unwrap();
    run.shutdown().await;

    let run = run_fixture("compatible").await.unwrap();
    std::fs::write(&run.control, "changed_tool_shape").unwrap();
    let error = run.spawned.supervisor.replace().await.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("changed its published tool definitions"),
        "{error:#}"
    );
    assert_eq!(
        run.spawned.supervisor.health().state,
        ExtensionProcessState::Unavailable
    );
    assert!(invoke_fixture(&run).await.is_err());
    run.shutdown().await;
}

#[cfg(unix)]
#[tokio::test]
async fn stale_generation_is_rejected_before_extension_invocation() {
    let _env_guard = crate::test_support::env::lock_async().await;
    unsafe {
        std::env::remove_var("OMEGON_RUNTIME_CONTEXT");
        std::env::remove_var("KUBERNETES_SERVICE_HOST");
    }

    let run = run_fixture("compatible").await.unwrap();
    let supervisor = run.spawned.supervisor.clone();
    let polling_handle = run.spawned.rpc_polling_handle.clone();
    let inventory = run.inventory.clone();
    let contribution_id = run.id.clone();
    let marker = run.marker.clone();
    let mut bus = crate::bus::EventBus::new();
    bus.register(run.spawned.feature);
    bus.finalize();
    let args = json!({});
    let lease = match crate::invocation_service::InvocationService::admit_tool(
        &bus,
        "fixture_echo",
        crate::invocation_service::InvocationAdmissionRequest {
            call_id: "stale-extension-call",
            visible_tool_name: "fixture_echo",
            args: &args,
            scope: crate::invocation_service::InvocationScope::default(),
            permission_policy: None,
            permission_role: None,
        },
    ) {
        crate::invocation_service::InvocationAdmission::Lease(lease) => lease,
        _ => panic!("extension invocation was not admitted"),
    };
    lease
        .claim_dispatch("stale-extension-call", "fixture_echo")
        .unwrap();
    bus.register(Box::new(GenerationBumpFeature));
    bus.try_finalize().unwrap();

    let error = bus
        .execute_tool_with_lease(
            &lease,
            "fixture_echo",
            "stale-extension-call",
            args,
            CancellationToken::new(),
            omegon_traits::ToolProgressSink::noop(),
            omegon_traits::ToolExecutionContext::default(),
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("invocation:stale_generation"));
    assert!(!marker.exists(), "stale invocation reached the extension");

    let mut replacement = inventory
        .evidence()
        .into_iter()
        .next()
        .expect("published extension evidence")
        .candidate
        .preflight;
    replacement.source_digest = "sha256:replacement-generation".into();
    inventory.discover(replacement).unwrap();
    inventory.ready(&contribution_id);
    inventory.stage_ready();
    inventory.publish_staged();
    let error = polling_handle
        .rpc_call(
            "execute_tool",
            json!({"name": "fixture_echo", "arguments": {}}),
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("generation is stale"));
    assert!(
        !marker.exists(),
        "stale polling handle reached the extension"
    );
    supervisor
        .shutdown(Duration::from_millis(500))
        .await
        .unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn lif_001_lif_003_real_process_quiescent_replacement() {
    let _env_guard = crate::test_support::env::lock_async().await;
    unsafe {
        std::env::remove_var("OMEGON_RUNTIME_CONTEXT");
        std::env::remove_var("KUBERNETES_SERVICE_HOST");
    }

    let active_run = run_fixture("compatible").await.unwrap();
    let inventory = active_run.inventory.clone();
    let active_polling = active_run.spawned.rpc_polling_handle.clone();
    let active_supervisor = active_run.spawned.supervisor.clone();
    let active_digest = active_supervisor.source_digest().to_string();
    let active_id = active_run.id.clone();
    let active_marker = active_run.marker.clone();
    let mut bus = crate::bus::EventBus::new();
    bus.register(active_run.spawned.feature);
    bus.finalize();
    let stale_args = json!({});
    let stale_lease = match crate::invocation_service::InvocationService::admit_tool(
        &bus,
        "fixture_echo",
        crate::invocation_service::InvocationAdmissionRequest {
            call_id: "retained-a-lease",
            visible_tool_name: "fixture_echo",
            args: &stale_args,
            scope: crate::invocation_service::InvocationScope::default(),
            permission_policy: None,
            permission_role: None,
        },
    ) {
        crate::invocation_service::InvocationAdmission::Lease(lease) => lease,
        _ => panic!("active extension invocation was not admitted"),
    };
    stale_lease
        .claim_dispatch("retained-a-lease", "fixture_echo")
        .unwrap();
    let mut active_owner =
        crate::contribution_lifecycle::DynamicContributionGenerationOwner::new(inventory.clone());
    active_owner.own_extension(active_supervisor);
    active_owner.publish();

    let candidate_root = tempfile::tempdir().unwrap();
    let candidate_marker = candidate_root.path().join("candidate-owner-entry.json");
    let candidate_paths = write_fixture_with_marker(
        candidate_root.path(),
        "compatible",
        candidate_marker.clone(),
    )
    .unwrap();
    std::fs::create_dir_all(candidate_root.path().join(".omegon")).unwrap();
    std::fs::write(
        candidate_root.path().join(".omegon/profile.json"),
        r#"{"permissions":{"trustedContributionCode":["extension:native-conformance-fixture"]}}"#,
    )
    .unwrap();
    let secrets = Arc::new(
        omegon_secrets::SecretsManager::new(&candidate_root.path().join("secrets")).unwrap(),
    );
    let candidate = crate::setup::stage_installed_extension_replacement_from_home(
        candidate_root.path(),
        candidate_root.path(),
        "native-conformance-fixture",
        secrets,
        inventory.clone(),
    )
    .await
    .unwrap();
    let crate::setup::InstalledExtensionReplacement::Changed(candidate) = candidate else {
        panic!("changed fixture source must stage a new generation");
    };
    assert_eq!(candidate_paths.marker, candidate_marker);
    let mut coordinator =
        crate::contribution_lifecycle::DynamicExtensionPublicationCoordinator::default();
    coordinator.accept(candidate).await.unwrap();
    assert!(coordinator.has_pending());
    assert!(!candidate_marker.exists());

    let authority_dir = tempfile::tempdir().unwrap();
    let authority = crate::session_authority::SessionAuthority::open(
        &authority_dir.path().join("lif-session.json"),
        "lif-session",
        "lif-workspace",
        "lif-composition",
        crate::session_authority::ActorIdentity {
            principal: "campaign".into(),
            ingress: "test".into(),
        },
        "2026-09-01T00:00:00Z",
    )
    .unwrap();
    let mut supervisor =
        crate::runtime_supervisor::InteractiveRuntimeSupervisor::with_authority(authority).unwrap();

    for prompt in ["active turn", "turn before explicit commit"] {
        supervisor
            .admit_prompt(
                prompt.into(),
                Vec::new(),
                crate::runtime_prompt::RuntimeActor::tui(),
                crate::runtime_prompt::ControlSurface::Tui,
                crate::operator_commands::PromptMetadata::default(),
                None,
            )
            .unwrap();
        supervisor.start_next_turn().unwrap().unwrap();
        let identity = supervisor.current_identity().unwrap();
        assert!(
            coordinator
                .commit_at_quiescence(&active_id, &supervisor, &mut bus, &mut active_owner)
                .await
                .is_err()
        );
        bus.invoke_tool(
            "fixture_echo",
            prompt,
            json!({}),
            CancellationToken::new(),
            crate::invocation_service::InvocationScope::default(),
        )
        .await
        .unwrap();
        assert!(active_marker.exists());
        std::fs::remove_file(&active_marker).unwrap();
        supervisor
            .submit_loop_terminal_intent(crate::runtime_turn::LoopTerminalIntent {
                identity,
                outcome: crate::runtime_turn::RuntimeTurnOutcome::Completed,
                reason_code: "campaign_completed".into(),
            })
            .unwrap();
    }

    let active_call = inventory.begin_call(&active_id, &active_digest).unwrap();
    assert!(
        coordinator
            .commit_at_quiescence(&active_id, &supervisor, &mut bus, &mut active_owner)
            .await
            .unwrap_err()
            .to_string()
            .contains("extension calls remain active")
    );
    drop(active_call);

    let outcome = coordinator
        .commit_at_quiescence(&active_id, &supervisor, &mut bus, &mut active_owner)
        .await
        .unwrap();
    assert!(outcome.retirement_failures.is_empty());
    assert!(!coordinator.has_pending());

    let stale_error = active_polling
        .rpc_call(
            "execute_tool",
            json!({"name": "fixture_echo", "arguments": {}}),
        )
        .await
        .unwrap_err();
    assert!(stale_error.to_string().contains("generation is stale"));
    assert!(!active_marker.exists());
    let stale_lease_error = bus
        .execute_tool_with_lease(
            &stale_lease,
            "fixture_echo",
            "retained-a-lease",
            stale_args,
            CancellationToken::new(),
            omegon_traits::ToolProgressSink::noop(),
            omegon_traits::ToolExecutionContext::default(),
        )
        .await
        .unwrap_err();
    assert!(
        stale_lease_error
            .to_string()
            .contains("invocation:stale_generation")
    );
    assert!(!active_marker.exists());

    bus.invoke_tool(
        "fixture_echo",
        "fresh-candidate-call",
        json!({}),
        CancellationToken::new(),
        crate::invocation_service::InvocationScope::default(),
    )
    .await
    .unwrap();
    assert!(candidate_marker.exists());
    assert!(!active_marker.exists());
    assert!(active_owner.shutdown().await.is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn lif_001_newer_candidate_settles_pending_before_replacement() {
    let _env_guard = crate::test_support::env::lock_async().await;
    unsafe {
        std::env::remove_var("OMEGON_RUNTIME_CONTEXT");
        std::env::remove_var("KUBERNETES_SERVICE_HOST");
    }

    let active_run = run_fixture("compatible").await.unwrap();
    let inventory = active_run.inventory.clone();
    let contribution_id = active_run.id.clone();
    let active_polling = active_run.spawned.rpc_polling_handle.clone();
    let active_supervisor = active_run.spawned.supervisor.clone();
    let mut coordinator =
        crate::contribution_lifecycle::DynamicExtensionPublicationCoordinator::default();

    let b_root = tempfile::tempdir().unwrap();
    let b_paths = write_fixture_with_marker(
        b_root.path(),
        "compatible",
        b_root.path().join("b-owner-entry.json"),
    )
    .unwrap();
    let b_run = run_extension_with_inventory(
        b_root,
        b_paths.extension_dir,
        b_paths.control,
        b_paths.marker,
        inventory.clone(),
        false,
    )
    .await
    .unwrap();
    let b_pid = b_run.spawned.supervisor.health().pid.unwrap();
    coordinator
        .accept(
            crate::contribution_lifecycle::StagedDynamicExtensionGeneration::new(
                b_run.spawned.feature,
                b_run.spawned.supervisor,
                inventory.clone(),
            ),
        )
        .await
        .unwrap();

    let c_root = tempfile::tempdir().unwrap();
    let c_marker = c_root.path().join("c-owner-entry.json");
    let c_paths = write_fixture_with_marker(c_root.path(), "compatible", c_marker.clone()).unwrap();
    let c_run = run_extension_with_inventory(
        c_root,
        c_paths.extension_dir,
        c_paths.control,
        c_paths.marker,
        inventory.clone(),
        false,
    )
    .await
    .unwrap();
    let c_supervisor = c_run.spawned.supervisor.clone();
    let c_pid = c_supervisor.health().pid.unwrap();
    coordinator
        .accept(
            crate::contribution_lifecycle::StagedDynamicExtensionGeneration::new(
                c_run.spawned.feature,
                c_run.spawned.supervisor,
                inventory,
            ),
        )
        .await
        .unwrap();

    wait_for_process_exit(b_pid).await;
    active_polling
        .rpc_call(
            "execute_tool",
            json!({"name": "fixture_echo", "arguments": {}}),
        )
        .await
        .unwrap();
    assert_eq!(
        active_supervisor.health().state,
        ExtensionProcessState::Healthy
    );
    assert!(!c_marker.exists());
    assert!(coordinator.has_pending());
    assert!(
        coordinator
            .reject_pending(&contribution_id, "campaign complete")
            .await
            .is_empty()
    );
    wait_for_process_exit(c_pid).await;
    active_supervisor
        .shutdown(Duration::from_millis(500))
        .await
        .unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn shutdown_settles_fixture_descendants() {
    let _env_guard = crate::test_support::env::lock_async().await;
    unsafe {
        std::env::remove_var("OMEGON_RUNTIME_CONTEXT");
        std::env::remove_var("KUBERNETES_SERVICE_HOST");
    }

    let run = run_fixture("child_process").await.unwrap();
    invoke_fixture(&run).await.unwrap();
    let marker = wait_for_marker(&run.marker).await.unwrap();
    let child_pid = marker["child_pid"].as_u64().unwrap() as u32;
    assert!(process_exists(child_pid));
    run.shutdown().await;
    wait_for_process_exit(child_pid).await;
}

#[cfg(unix)]
#[tokio::test]
async fn crash_budget_quarantines_only_the_failing_extension() {
    let _env_guard = crate::test_support::env::lock_async().await;
    unsafe {
        std::env::remove_var("OMEGON_RUNTIME_CONTEXT");
        std::env::remove_var("KUBERNETES_SERVICE_HOST");
    }

    let failing = run_fixture("crash").await.unwrap();
    let healthy = run_fixture("compatible").await.unwrap();
    for _ in 0..4 {
        assert!(invoke_fixture(&failing).await.is_err());
    }

    let health = failing.spawned.supervisor.health();
    assert_eq!(health.state, ExtensionProcessState::Unavailable);
    assert!(
        health
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("restart budget was exhausted")),
        "{health:?}"
    );
    invoke_fixture(&healthy).await.unwrap();

    let mut owner = crate::contribution_lifecycle::DynamicContributionGenerationOwner::new(
        failing.inventory.clone(),
    );
    owner.own_extension(failing.spawned.supervisor.clone());
    let diagnosis = crate::control_runtime::runtime_doctor_response(Some(&owner.control()));
    let output = diagnosis.output.unwrap();
    assert!(output.contains("unavailable"), "{output}");
    assert!(output.contains("restart budget was exhausted"), "{output}");

    assert!(owner.shutdown().await.is_empty());
    healthy.shutdown().await;
}
