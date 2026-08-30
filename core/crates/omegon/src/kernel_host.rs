use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand};
use omegon_codescan_contracts::{
    CODESCAN_PROTOCOL_VERSION, CODESCAN_RPC_METHOD, CODESCAN_SERVICE_ID, CODESCAN_STATUS_METHOD,
    CodescanOperationV1, CodescanOutcomeV1, CodescanRequestV1, CodescanResponseV1,
    CodescanStatusV1, IndexRequestV1, SearchRequestV1, SearchScope,
};
use omegon_native_extension_host::{
    ExtensionManifest, ExtensionProcessState, ExtensionSupervisor, LaunchSpec, ReadinessValidator,
    RpcRequestPolicy, shutdown_supervisors,
};
use serde::Serialize;
use serde_json::{Map, Value, json};
use tokio_util::sync::CancellationToken;

const ARTIFACT_PROFILE: &str = "kernel-host-v1";
const CODESCAN_EXTENSION: &str = "omegon-codescan";
const CORE_MARKER: &str = "omegon-composition-core-probe";

#[derive(Debug)]
enum CodescanServiceError {
    Unavailable,
}

impl CodescanServiceError {
    const fn code(&self) -> &'static str {
        match self {
            Self::Unavailable => "service:unavailable",
        }
    }
}

impl std::fmt::Display for CodescanServiceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("codescan extension is unavailable")
    }
}

impl std::error::Error for CodescanServiceError {}

#[derive(Parser)]
#[command(name = "omegon")]
struct Cli {
    #[arg(long, global = true)]
    cwd: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    CompositionInspect {
        #[arg(long, value_parser = ["kernel"])]
        profile: String,
        #[arg(long, value_parser = ["core-read", "codescan-search"])]
        probe: String,
    },
}

#[derive(Serialize)]
struct Inspection {
    schema_version: u32,
    artifact_profile: &'static str,
    canonical_entrypoint: [&'static str; 1],
    profile: &'static str,
    runtime_mode: &'static str,
    surfaces: [&'static str; 2],
    absent_optional: [&'static str; 8],
    startup_tasks: CountedOwners,
    model_schema: CountedOwners,
    resident_capabilities: [&'static str; 4],
    callable_capabilities: [&'static str; 3],
    external_processes: Vec<ExternalProcess>,
    functional_probe: Value,
}

#[derive(Serialize)]
struct CountedOwners {
    count: usize,
    owners: BTreeMap<String, usize>,
}

#[derive(Serialize)]
struct ExternalProcess {
    owner: String,
    state: &'static str,
    pid: Option<u32>,
}

struct CodescanReadiness;

impl ReadinessValidator for CodescanReadiness {
    fn validate(&self, method: &str, response: &Value) -> Result<()> {
        if method != CODESCAN_STATUS_METHOD {
            return Ok(());
        }
        let status: CodescanStatusV1 = serde_json::from_value(response.clone())
            .context("extension returned invalid codescan status")?;
        if status.protocol_version != CODESCAN_PROTOCOL_VERSION
            || status.service != CODESCAN_SERVICE_ID
            || !status.ready
        {
            anyhow::bail!("extension returned incompatible codescan status");
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    let cwd = bounded_workspace(cli.cwd.as_deref())?;
    let Command::CompositionInspect { profile, probe } = cli.command;
    if profile != "kernel" {
        anyhow::bail!("composition profile '{profile}' is incompatible with {ARTIFACT_PROFILE}");
    }

    let mut supervisors = Vec::new();
    let probe_result = execute_probe(&probe, &cwd, &mut supervisors).await;
    let external_processes = supervisors.iter().map(process_inventory).collect();
    let shutdown_failures = shutdown_supervisors(&supervisors, Duration::from_secs(2)).await;
    if !shutdown_failures.is_empty() {
        anyhow::bail!(
            "native extension shutdown failed: {}",
            shutdown_failures.join(", ")
        );
    }
    let functional_probe = probe_result?;
    let inspection = Inspection {
        schema_version: 1,
        artifact_profile: ARTIFACT_PROFILE,
        canonical_entrypoint: ["omegon"],
        profile: "kernel",
        runtime_mode: "kernel",
        surfaces: ["agent-loop", "bounded-task"],
        absent_optional: [
            "context-compaction",
            "git",
            "lifecycle",
            "memory",
            "provider-clients",
            "shipped-content",
            "tui",
            "self-update",
        ],
        startup_tasks: CountedOwners {
            count: 0,
            owners: BTreeMap::new(),
        },
        model_schema: CountedOwners {
            count: 0,
            owners: BTreeMap::new(),
        },
        resident_capabilities: [
            "system:constitutional-kernel",
            "system:default-loop",
            "system:host-effects",
            "feature:codescan-adapter",
        ],
        callable_capabilities: ["tool:read", "tool:codebase_index", "tool:codebase_search"],
        external_processes,
        functional_probe,
    };
    println!("{}", serde_json::to_string(&inspection)?);
    Ok(())
}

fn bounded_workspace(requested: Option<&Path>) -> Result<PathBuf> {
    requested
        .unwrap_or(Path::new("."))
        .canonicalize()
        .context("failed to resolve workspace root")
        .and_then(|path| {
            if path.is_dir() {
                Ok(path)
            } else {
                Err(anyhow!(
                    "workspace root is not a directory: {}",
                    path.display()
                ))
            }
        })
}

async fn execute_probe(
    probe: &str,
    cwd: &Path,
    supervisors: &mut Vec<Arc<ExtensionSupervisor>>,
) -> Result<Value> {
    match probe {
        "core-read" => core_read_probe(cwd),
        "codescan-search" => codescan_probe(cwd, supervisors).await,
        _ => anyhow::bail!("unknown composition probe: {probe}"),
    }
}

fn core_read_probe(cwd: &Path) -> Result<Value> {
    let requested = cwd.join("composition-probe.txt");
    let resolved = requested
        .canonicalize()
        .with_context(|| format!("failed to resolve core-read path: {}", requested.display()))?;
    if !resolved.starts_with(cwd) || !resolved.is_file() {
        anyhow::bail!(
            "core-read path escapes the workspace: {}",
            resolved.display()
        );
    }
    let content = std::fs::read_to_string(&resolved)
        .with_context(|| format!("failed to read core probe: {}", resolved.display()))?;
    if !content.contains(CORE_MARKER) {
        anyhow::bail!("kernel core-read probe did not return the fixture marker");
    }
    let absence = match release_codescan_dir() {
        Ok(_) => anyhow::bail!("kernel-only install unexpectedly contains codescan"),
        Err(error) => error,
    };
    Ok(json!({
        "name": "core-read",
        "status": "ok",
        "codescan": absence.code()
    }))
}

async fn codescan_probe(
    cwd: &Path,
    supervisors: &mut Vec<Arc<ExtensionSupervisor>>,
) -> Result<Value> {
    let extension_dir = release_codescan_dir().map_err(anyhow::Error::new)?;
    let manifest = ExtensionManifest::from_extension_dir(&extension_dir)?;
    if manifest.extension.name != CODESCAN_EXTENSION || !manifest.is_native() {
        anyhow::bail!("release-layout codescan manifest has an invalid identity or runtime");
    }
    let launch = LaunchSpec {
        manifest,
        extension_dir,
        project_root: Some(cwd.to_path_buf()),
        resolved_config: Map::new(),
        resolved_secrets: Vec::new(),
        source_digest: "release-layout:omegon-codescan".to_string(),
        notification_tx: None,
        host_request_handler: None,
        readiness_validator: Some(Arc::new(CodescanReadiness)),
    };
    let (supervisor, _handshake) = ExtensionSupervisor::launch(launch).await?;
    supervisors.push(Arc::clone(&supervisor));

    codescan_call(
        &supervisor,
        CodescanOperationV1::Index(IndexRequestV1 { invalidate: true }),
    )
    .await?;
    let response = codescan_call(
        &supervisor,
        CodescanOperationV1::Search(SearchRequestV1 {
            query: "omegon_composition_codescan_probe".to_string(),
            scope: SearchScope::Code,
            max_results: 5,
            tags: Vec::new(),
            within: None,
        }),
    )
    .await?;
    let CodescanResponseV1::Search(search) = response else {
        anyhow::bail!("codescan search returned the wrong response kind");
    };
    if search.results.is_empty() {
        anyhow::bail!("additive codescan probe did not restore search");
    }
    let health = supervisor.health();
    if health.state != ExtensionProcessState::Healthy || health.pid.is_none() {
        anyhow::bail!("codescan process was not healthy after search");
    }
    Ok(json!({
        "name": "codescan-search",
        "status": "ok",
        "service_provenance": {
            "extension": CODESCAN_EXTENSION,
            "transport": "native-json-rpc",
            "pid": health.pid
        }
    }))
}

async fn codescan_call(
    supervisor: &ExtensionSupervisor,
    operation: CodescanOperationV1,
) -> Result<CodescanResponseV1> {
    let request = CodescanRequestV1::new(operation);
    let value = supervisor
        .rpc_call_with_cancel(
            CODESCAN_RPC_METHOD,
            serde_json::to_value(request)?,
            CancellationToken::new(),
            Some(Duration::from_secs(120)),
            RpcRequestPolicy::RejectHostRequests,
            None,
        )
        .await?;
    match serde_json::from_value::<CodescanOutcomeV1>(value)? {
        CodescanOutcomeV1::Ok {
            protocol_version,
            response,
        } if protocol_version == CODESCAN_PROTOCOL_VERSION => Ok(response),
        CodescanOutcomeV1::Error { error, .. } => {
            anyhow::bail!(
                "codescan operation failed ({:?}): {}",
                error.code,
                error.message
            )
        }
        _ => anyhow::bail!("codescan extension returned an incompatible protocol response"),
    }
}

fn release_codescan_dir() -> std::result::Result<PathBuf, CodescanServiceError> {
    let executable = std::env::current_exe().map_err(|_| CodescanServiceError::Unavailable)?;
    let install_root = executable
        .parent()
        .ok_or(CodescanServiceError::Unavailable)?;
    let extension_dir = install_root.join("share/omegon/extensions/omegon-codescan");
    if !extension_dir.join("manifest.toml").is_file() {
        return Err(CodescanServiceError::Unavailable);
    }
    Ok(extension_dir)
}

fn process_inventory(supervisor: &Arc<ExtensionSupervisor>) -> ExternalProcess {
    let health = supervisor.health();
    ExternalProcess {
        owner: health.name,
        state: match health.state {
            ExtensionProcessState::Healthy => "healthy",
            ExtensionProcessState::Unavailable => "unavailable",
            ExtensionProcessState::Replacing => "replacing",
            ExtensionProcessState::ShuttingDown => "shutting_down",
        },
        pid: health.pid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_read_is_real_and_workspace_bounded() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("composition-probe.txt"), CORE_MARKER).unwrap();
        let canonical_root = root.path().canonicalize().unwrap();
        assert_eq!(core_read_probe(&canonical_root).unwrap()["status"], "ok");
    }
}
