//! Native Auspex discovery, compatibility probing, and session handoff.
//!
//! This adapter owns process and transport mechanics. The TUI remains
//! responsible for deciding when to launch Auspex and how to present status.

use crate::web::WebStartupInfo;
use omegon_traits::OmegonTransportSecurity;
use std::net::SocketAddr;
use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum AuspexHandoffMode {
    Env,
    BrowserUrl,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct AuspexAttachPayload {
    version: u16,
    transport: String,
    preferred_handoff: AuspexHandoffMode,
    startup_url: String,
    http_base: String,
    ws_url: String,
    ws_token: String,
    http_transport_security: OmegonTransportSecurity,
    ws_transport_security: OmegonTransportSecurity,
    instance: Option<omegon_traits::OmegonInstanceDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AuspexProbe {
    pub(super) target: String,
    pub(super) source: &'static str,
    pub(super) compatibility: AuspexCompatibility,
    pub(super) handoff_modes: Vec<AuspexHandoffMode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum AuspexCompatibility {
    Unknown,
    Compatible,
    Incompatible(String),
}

pub(super) fn launch_with_startup(startup: &WebStartupInfo) -> anyhow::Result<String> {
    let probe = detect_target().ok_or_else(|| {
        anyhow::anyhow!("Auspex not detected. Set AUSPEX_BIN or install Auspex first.")
    })?;
    if let AuspexCompatibility::Incompatible(reason) = &probe.compatibility {
        anyhow::bail!(
            "Auspex detected at {} but is not compatible: {reason}",
            probe.target
        );
    }

    let target = probe.target;
    let preferred_handoff = preferred_handoff(&probe.handoff_modes);
    let attach_payload = build_attach_payload(startup, preferred_handoff.clone())?;

    if matches!(preferred_handoff, AuspexHandoffMode::BrowserUrl) {
        super::open_browser(&startup.http_base);
        return Ok(format!("{target} via browser-url"));
    }

    let mut command = launch_command(&target);
    command
        .env("AUSPEX_OMEGON_STARTUP_URL", startup.startup_url.clone())
        .env("AUSPEX_OMEGON_WS_URL", startup.ws_url.clone())
        .env("AUSPEX_OMEGON_WS_TOKEN", startup.token.clone())
        .env("AUSPEX_OMEGON_ATTACH_JSON", attach_payload.clone())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(target_os = "macos")]
    if target.ends_with(".app") {
        command
            .arg("--env")
            .arg(format!("AUSPEX_OMEGON_STARTUP_URL={}", startup.startup_url));
        command
            .arg("--env")
            .arg(format!("AUSPEX_OMEGON_WS_URL={}", startup.ws_url));
        command
            .arg("--env")
            .arg(format!("AUSPEX_OMEGON_WS_TOKEN={}", startup.token));
        command
            .arg("--env")
            .arg(format!("AUSPEX_OMEGON_ATTACH_JSON={attach_payload}"));
    }

    command.spawn()?;
    Ok(format!("{target} via env"))
}

fn launch_command(target: &str) -> Command {
    if let Some(explicit) = target.strip_prefix("AUSPEX_BIN=") {
        Command::new(explicit)
    } else if target.ends_with(".app") {
        #[cfg(target_os = "macos")]
        {
            let mut command = Command::new("open");
            command.arg("-a").arg(target);
            command
        }
        #[cfg(not(target_os = "macos"))]
        {
            Command::new(target)
        }
    } else {
        Command::new(target)
    }
}

fn preferred_handoff(modes: &[AuspexHandoffMode]) -> AuspexHandoffMode {
    if modes.contains(&AuspexHandoffMode::Env) {
        AuspexHandoffMode::Env
    } else {
        AuspexHandoffMode::BrowserUrl
    }
}

pub(super) fn build_attach_payload(
    startup: &WebStartupInfo,
    preferred_handoff: AuspexHandoffMode,
) -> anyhow::Result<String> {
    let (http_transport_security, ws_transport_security) = transport_security(startup);
    let payload = AuspexAttachPayload {
        version: 1,
        transport: "omegon-ipc".into(),
        preferred_handoff,
        startup_url: startup.startup_url.clone(),
        http_base: startup.http_base.clone(),
        ws_url: startup.ws_url.clone(),
        ws_token: startup.token.clone(),
        http_transport_security,
        ws_transport_security,
        instance: startup.instance_descriptor.clone(),
    };
    serde_json::to_string(&payload).map_err(Into::into)
}

pub(super) fn transport_security(
    startup: &WebStartupInfo,
) -> (OmegonTransportSecurity, OmegonTransportSecurity) {
    let http = startup
        .instance_descriptor
        .as_ref()
        .and_then(|instance| instance.control_plane.http_transport_security.clone())
        .unwrap_or_else(|| {
            if startup.http_base.starts_with("https://") {
                OmegonTransportSecurity::Secure
            } else {
                OmegonTransportSecurity::InsecureBootstrap
            }
        });
    let ws = startup
        .instance_descriptor
        .as_ref()
        .and_then(|instance| instance.control_plane.ws_transport_security.clone())
        .unwrap_or_else(|| {
            if startup.ws_url.starts_with("wss://") {
                OmegonTransportSecurity::Secure
            } else {
                OmegonTransportSecurity::InsecureBootstrap
            }
        });
    (http, ws)
}

pub(super) fn format_transport_security(value: &OmegonTransportSecurity) -> &'static str {
    match value {
        OmegonTransportSecurity::LocalIpc => "local-ipc",
        OmegonTransportSecurity::InsecureBootstrap => "insecure-bootstrap",
        OmegonTransportSecurity::Secure => "secure",
        OmegonTransportSecurity::IdentityMesh => "identity-mesh",
    }
}

pub(super) fn browser_url(
    startup: Option<&WebStartupInfo>,
    addr: Option<SocketAddr>,
) -> Option<String> {
    startup
        .map(|startup| startup.http_base.clone())
        .or_else(|| addr.map(|addr| format!("http://{addr}")))
}

fn parse_handoff_modes(value: &serde_json::Value) -> Vec<AuspexHandoffMode> {
    let Some(modes) = value
        .get("handoff_modes")
        .and_then(|value| value.as_array())
    else {
        return vec![AuspexHandoffMode::Env];
    };
    let parsed = modes
        .iter()
        .filter_map(|mode| match mode.as_str() {
            Some("env") => Some(AuspexHandoffMode::Env),
            Some("browser-url") => Some(AuspexHandoffMode::BrowserUrl),
            _ => None,
        })
        .collect::<Vec<_>>();
    if parsed.is_empty() {
        vec![AuspexHandoffMode::Env]
    } else {
        parsed
    }
}

fn compatibility_from_value(
    value: &serde_json::Value,
) -> (AuspexCompatibility, Vec<AuspexHandoffMode>) {
    let modes = parse_handoff_modes(value);
    let Some(protocol) = value
        .get("omegon_ipc_protocol")
        .and_then(|value| value.as_u64())
    else {
        return (AuspexCompatibility::Unknown, modes);
    };
    if protocol == omegon_traits::IPC_PROTOCOL_VERSION as u64 {
        (AuspexCompatibility::Compatible, modes)
    } else {
        (
            AuspexCompatibility::Incompatible(format!(
                "reported omegon_ipc_protocol={protocol}, expected {}",
                omegon_traits::IPC_PROTOCOL_VERSION
            )),
            modes,
        )
    }
}

fn probe_target(target: &str) -> (AuspexCompatibility, Vec<AuspexHandoffMode>) {
    if target.ends_with(".app") {
        return (AuspexCompatibility::Unknown, vec![AuspexHandoffMode::Env]);
    }
    let bin = target.strip_prefix("AUSPEX_BIN=").unwrap_or(target);
    let output = Command::new(bin)
        .arg("--omegon-compat")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();
    let Ok(output) = output else {
        return (AuspexCompatibility::Unknown, vec![AuspexHandoffMode::Env]);
    };
    if !output.status.success() {
        return (AuspexCompatibility::Unknown, vec![AuspexHandoffMode::Env]);
    }
    let body = String::from_utf8_lossy(&output.stdout);
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&body) else {
        return (AuspexCompatibility::Unknown, vec![AuspexHandoffMode::Env]);
    };
    compatibility_from_value(&value)
}

fn path_contains_executable(candidate: &Path) -> bool {
    candidate.is_file()
}

pub(super) fn detect_target() -> Option<AuspexProbe> {
    if let Ok(explicit) = std::env::var("AUSPEX_BIN") {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            let path = Path::new(trimmed);
            if path_contains_executable(path) {
                let target = format!("AUSPEX_BIN={trimmed}");
                let (compatibility, handoff_modes) = probe_target(&target);
                return Some(AuspexProbe {
                    compatibility,
                    handoff_modes,
                    target,
                    source: "env",
                });
            }
        }
    }

    if let Ok(path_env) = std::env::var("PATH") {
        for entry in std::env::split_paths(&path_env) {
            let candidate = entry.join("auspex");
            if path_contains_executable(&candidate) {
                let target = candidate.display().to_string();
                let (compatibility, handoff_modes) = probe_target(&target);
                return Some(AuspexProbe {
                    compatibility,
                    handoff_modes,
                    target,
                    source: "path",
                });
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        let app_bundle = Path::new("/Applications/Auspex.app");
        if app_bundle.exists() {
            return Some(AuspexProbe {
                compatibility: AuspexCompatibility::Unknown,
                handoff_modes: vec![AuspexHandoffMode::Env],
                target: app_bundle.display().to_string(),
                source: "app-bundle",
            });
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn startup(http_base: &str, ws_url: &str) -> WebStartupInfo {
        WebStartupInfo {
            schema_version: 2,
            addr: "127.0.0.1:7842".into(),
            http_base: http_base.into(),
            state_url: format!("{http_base}/api/state"),
            startup_url: format!("{http_base}/api/startup"),
            health_url: format!("{http_base}/api/healthz"),
            ready_url: format!("{http_base}/api/readyz"),
            ws_url: ws_url.into(),
            acp_url: None,
            token: "test".into(),
            auth_mode: "ephemeral-bearer".into(),
            auth_source: "generated".into(),
            web_authority: crate::web::WebAuthorityConfig::default().status(),
            control_plane_state: crate::web::ControlPlaneState::Ready,
            daemon_status: crate::web::WebDaemonStatus::default(),
            instance_descriptor: None,
        }
    }

    #[test]
    fn attach_payload_carries_startup_and_transport_metadata() {
        let startup = startup("http://127.0.0.1:7842", "ws://127.0.0.1:7842/ws?token=test");
        let payload = build_attach_payload(&startup, AuspexHandoffMode::Env).unwrap();
        let json: serde_json::Value = serde_json::from_str(&payload).unwrap();

        assert_eq!(json["transport"], "omegon-ipc");
        assert_eq!(json["preferred_handoff"], "env");
        assert_eq!(json["startup_url"], "http://127.0.0.1:7842/api/startup");
        assert_eq!(json["http_transport_security"], "insecure-bootstrap");
        assert_eq!(json["ws_transport_security"], "insecure-bootstrap");
        assert_eq!(json["ws_token"], "test");
        assert!(json["instance"].is_null());
    }

    #[test]
    fn attach_payload_infers_tls_transport_security_without_instance() {
        let startup = startup(
            "https://127.0.0.1:7842",
            "wss://127.0.0.1:7842/ws?token=test",
        );
        let payload = build_attach_payload(&startup, AuspexHandoffMode::Env).unwrap();
        let json: serde_json::Value = serde_json::from_str(&payload).unwrap();

        assert_eq!(json["http_transport_security"], "secure");
        assert_eq!(json["ws_transport_security"], "secure");
    }

    #[test]
    fn handoff_modes_default_to_env_when_unspecified_or_unsupported() {
        assert_eq!(
            parse_handoff_modes(&serde_json::json!({"omegon_ipc_protocol": 1})),
            vec![AuspexHandoffMode::Env]
        );
        assert_eq!(
            parse_handoff_modes(&serde_json::json!({"handoff_modes": ["unknown"]})),
            vec![AuspexHandoffMode::Env]
        );
    }

    #[test]
    fn handoff_modes_preserve_supported_order() {
        assert_eq!(
            parse_handoff_modes(&serde_json::json!({
                "handoff_modes": ["browser-url", "env", "unknown"]
            })),
            vec![AuspexHandoffMode::BrowserUrl, AuspexHandoffMode::Env]
        );
    }

    #[test]
    fn env_handoff_is_preferred_when_available() {
        assert_eq!(
            preferred_handoff(&[AuspexHandoffMode::BrowserUrl, AuspexHandoffMode::Env]),
            AuspexHandoffMode::Env
        );
        assert_eq!(
            preferred_handoff(&[AuspexHandoffMode::BrowserUrl]),
            AuspexHandoffMode::BrowserUrl
        );
    }

    #[test]
    fn compatibility_accepts_current_protocol_and_rejects_other_versions() {
        let current = serde_json::json!({
            "omegon_ipc_protocol": omegon_traits::IPC_PROTOCOL_VERSION,
            "handoff_modes": ["env"]
        });
        assert_eq!(
            compatibility_from_value(&current),
            (
                AuspexCompatibility::Compatible,
                vec![AuspexHandoffMode::Env]
            )
        );

        let incompatible = serde_json::json!({
            "omegon_ipc_protocol": omegon_traits::IPC_PROTOCOL_VERSION + 1,
            "handoff_modes": ["browser-url"]
        });
        let (compatibility, modes) = compatibility_from_value(&incompatible);
        assert!(matches!(
            compatibility,
            AuspexCompatibility::Incompatible(_)
        ));
        assert_eq!(modes, vec![AuspexHandoffMode::BrowserUrl]);
    }

    #[test]
    fn compatibility_without_protocol_is_unknown_but_preserves_modes() {
        assert_eq!(
            compatibility_from_value(&serde_json::json!({"handoff_modes": ["browser-url"]})),
            (
                AuspexCompatibility::Unknown,
                vec![AuspexHandoffMode::BrowserUrl]
            )
        );
    }
}
