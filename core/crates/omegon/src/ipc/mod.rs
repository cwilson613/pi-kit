//! Native IPC server — Auspex/Omegon Unix socket control plane.
//!
//! Exposes the contract defined in `omegon-traits` and documented in
//! `docs/auspex-ipc-contract.md` over a Unix domain socket.
//!
//! ## Socket path
//!
//! `{cwd}/.omegon/ipc.sock` — project-scoped and deterministic from `cwd`.
//! Auspex knows the cwd at launch time so no additional discovery is needed.
//!
//! ## Single controller model
//!
//! Only one client may be connected at a time. A second connection while one
//! is active is rejected with `IpcErrorCode::Busy`.

#[cfg(unix)]
pub mod connection;
pub mod snapshot;
pub mod wire;

use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[cfg(unix)]
use tokio::net::UnixListener;
use tokio::sync::{broadcast, mpsc};
use tokio_util::sync::CancellationToken;
#[cfg(unix)]
use tracing::{debug, info, warn};

use omegon_traits::AgentEvent;
#[cfg(unix)]
use omegon_traits::{IpcEnvelope, IpcErrorCode};

use crate::operator_commands::OperatorCommand as TuiCommand;
use crate::runtime_state::RuntimeStateHandles as DashboardHandles;

#[cfg(unix)]
use connection::{ConnectionConfig, IpcConnection};
#[cfg(unix)]
use wire::encode_envelope;

/// Configuration for the IPC server.
#[derive(Clone)]
pub struct IpcServerConfig {
    pub socket_path: PathBuf,
    pub omegon_version: String,
    pub cwd: String,
    pub started_at: String,
    pub server_instance_id: String,
    pub session_view_binding: crate::session_consumers::SessionViewBinding,
}

impl IpcServerConfig {
    /// Build from a project root directory. Socket lives at `{cwd}/.omegon/ipc.sock`.
    pub fn from_cwd(
        cwd: &Path,
        omegon_version: &str,
        session_view_binding: crate::session_consumers::SessionViewBinding,
    ) -> Self {
        let socket_path = cwd.join(".omegon").join("ipc.sock");
        let started_at = chrono::Utc::now().to_rfc3339();
        let server_instance_id =
            format!("{:x}", std::process::id() as u64 ^ started_at.len() as u64);
        Self {
            socket_path,
            omegon_version: omegon_version.to_string(),
            cwd: cwd.to_string_lossy().to_string(),
            started_at,
            server_instance_id,
            session_view_binding,
        }
    }
}

/// Start the IPC server in a background tokio task.
///
/// Returns immediately. The server task runs until `cancel` is triggered.
pub fn start_ipc_server(
    cfg: IpcServerConfig,
    handles: DashboardHandles,
    events_tx: broadcast::Sender<AgentEvent>,
    command_tx: mpsc::Sender<TuiCommand>,
    shared_settings: crate::settings::SharedSettings,
    cancel: CancellationToken,
) {
    crate::task_spawn::spawn_infra("ipc-server", async move {
        run_server(cfg, handles, events_tx, command_tx, shared_settings, cancel).await
    });
}

#[cfg(unix)]
async fn run_server(
    cfg: IpcServerConfig,
    handles: DashboardHandles,
    events_tx: broadcast::Sender<AgentEvent>,
    command_tx: mpsc::Sender<TuiCommand>,
    shared_settings: crate::settings::SharedSettings,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    // Clean up stale socket file from a previous run.
    let _ = std::fs::remove_file(&cfg.socket_path);

    // Ensure the parent directory exists.
    if let Some(parent) = cfg.socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let listener = UnixListener::bind(&cfg.socket_path)?;

    // Restrict socket permissions to owner-only (0600).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&cfg.socket_path, std::fs::Permissions::from_mode(0o600))?;
    }

    info!(
        socket = %cfg.socket_path.display(),
        version = %cfg.omegon_version,
        "IPC server listening"
    );

    let has_controller = Arc::new(AtomicBool::new(false));
    let cfg = Arc::new(cfg);

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                debug!("IPC server: cancel token fired, shutting down");
                break;
            }
            result = listener.accept() => {
                match result {
                    Ok((stream, _addr)) => {
                        if has_controller
                            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                            .is_ok()
                        {
                            // Accept the controller connection.
                            let conn = IpcConnection::new(
                                stream,
                                ConnectionConfig {
                                    omegon_version: cfg.omegon_version.clone(),
                                    cwd: cfg.cwd.clone(),
                                    started_at: cfg.started_at.clone(),
                                    server_instance_id: cfg.server_instance_id.clone(),
                                     session_view_binding: cfg.session_view_binding.clone(),
                                    handles: handles.clone(),
                                    events_tx: events_tx.clone(),
                                    command_tx: command_tx.clone(),
                                    shared_settings: shared_settings.clone(),
                                    has_controller: has_controller.clone(),
                                },
                            );
                            debug!("IPC: accepted controller connection");
                            tokio::spawn(async move {
                                if let Err(e) = conn.run().await {
                                    debug!("IPC connection closed: {e}");
                                }
                            });
                        } else {
                            // Reject — a controller is already connected.
                            debug!("IPC: rejecting second connection (busy)");
                            let env = IpcEnvelope::error_response(
                                None,
                                IpcErrorCode::Busy,
                                "a controller is already connected",
                            );
                            if let Ok(raw) = encode_envelope(&env) {
                                let mut frame = Vec::with_capacity(4 + raw.len());
                                frame.extend_from_slice(&(raw.len() as u32).to_be_bytes());
                                frame.extend_from_slice(&raw);
                                // best-effort write; ignore errors
                                use tokio::io::AsyncWriteExt;
                                let (_, mut w) = tokio::io::split(stream);
                                let _ = w.write_all(&frame).await;
                            }
                        }
                    }
                    Err(e) => {
                        warn!("IPC accept error: {e}");
                    }
                }
            }
        }
    }

    let _ = std::fs::remove_file(&cfg.socket_path);
    debug!("IPC server stopped");
    Ok(())
}

#[cfg(not(unix))]
async fn run_server(
    _cfg: IpcServerConfig,
    _handles: DashboardHandles,
    _events_tx: broadcast::Sender<AgentEvent>,
    _command_tx: mpsc::Sender<TuiCommand>,
    _shared_settings: crate::settings::SharedSettings,
    _cancel: CancellationToken,
) -> anyhow::Result<()> {
    anyhow::bail!("native IPC requires Unix domain sockets")
}
