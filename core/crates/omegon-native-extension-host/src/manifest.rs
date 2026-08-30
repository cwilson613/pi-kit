use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExtensionManifest {
    pub extension: ExtensionMetadata,
    pub runtime: RuntimeConfig,
    #[serde(default)]
    pub startup: StartupConfig,
    #[serde(default)]
    pub widgets: HashMap<String, WidgetConfig>,
    #[serde(default)]
    pub secrets: SecretsConfig,
    #[serde(default)]
    pub mcp: Option<McpConfig>,
    #[serde(default)]
    pub config: HashMap<String, omegon_extension::ConfigField>,
    #[serde(default)]
    pub capabilities: omegon_extension::Capabilities,
    #[serde(default)]
    pub permissions: omegon_extension::ManifestPermissions,
    #[serde(default)]
    pub skills: Vec<ExtensionSkillConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExtensionSkillConfig {
    #[serde(default)]
    pub name: Option<String>,
    pub path: String,
}

impl ExtensionManifest {
    pub fn allows_host_action_type(&self, action_type: &str) -> bool {
        self.permissions
            .host_actions
            .allows_action_type(action_type)
    }

    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        toml::from_str(&content)
            .map_err(|e| anyhow!("failed to parse manifest at {}: {}", path.display(), e))
    }

    pub fn from_extension_dir(dir: &Path) -> Result<Self> {
        Self::from_file(&dir.join("manifest.toml"))
    }

    pub fn native_binary_path(&self, base_dir: &Path) -> Result<std::path::PathBuf> {
        match &self.runtime {
            RuntimeConfig::Native { binary, .. } => {
                let resolved = base_dir.join(binary);
                if resolved.exists() {
                    Ok(resolved)
                } else {
                    Err(anyhow!(
                        "native extension binary not found: {} (resolved to {})",
                        binary,
                        resolved.display()
                    ))
                }
            }
            RuntimeConfig::Oci { .. } => Err(anyhow!("expected native runtime, got OCI")),
        }
    }

    pub fn oci_image(&self) -> Result<String> {
        match &self.runtime {
            RuntimeConfig::Oci { image, .. } => Ok(image.clone()),
            RuntimeConfig::Native { .. } => Err(anyhow!("expected OCI runtime, got native")),
        }
    }

    pub fn is_native(&self) -> bool {
        matches!(self.runtime, RuntimeConfig::Native { .. })
    }

    pub fn is_oci(&self) -> bool {
        matches!(self.runtime, RuntimeConfig::Oci { .. })
    }

    pub fn is_mcp_capable(&self) -> bool {
        self.mcp.is_some()
    }

    pub fn connection_mode(&self, remote_url: Option<&str>) -> ConnectionMode {
        match (&self.mcp, remote_url) {
            (Some(config), Some(url)) => ConnectionMode::RemoteMcp {
                url: url.to_owned(),
                transport: config.transport.clone(),
            },
            _ => ConnectionMode::Local,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExtensionMetadata {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum RuntimeConfig {
    #[serde(rename = "native")]
    Native {
        binary: String,
        #[serde(default)]
        env: HashMap<String, String>,
        #[serde(default)]
        env_passthrough: Vec<String>,
        #[serde(default)]
        config: HashMap<String, Value>,
    },
    #[serde(rename = "oci")]
    Oci {
        image: String,
        #[serde(default)]
        env: HashMap<String, String>,
        #[serde(default)]
        env_passthrough: Vec<String>,
        #[serde(default)]
        config: HashMap<String, Value>,
    },
}

impl RuntimeConfig {
    pub fn env(&self) -> &HashMap<String, String> {
        match self {
            Self::Native { env, .. } | Self::Oci { env, .. } => env,
        }
    }

    pub fn env_passthrough(&self) -> &[String] {
        match self {
            Self::Native {
                env_passthrough, ..
            }
            | Self::Oci {
                env_passthrough, ..
            } => env_passthrough,
        }
    }

    pub fn config(&self) -> &HashMap<String, Value> {
        match self {
            Self::Native { config, .. } | Self::Oci { config, .. } => config,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StartupConfig {
    #[serde(default)]
    pub ping_method: Option<String>,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
}

fn default_timeout_ms() -> u64 {
    5000
}

impl Default for StartupConfig {
    fn default() -> Self {
        Self {
            ping_method: Some("get_tools".to_string()),
            timeout_ms: default_timeout_ms(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct SecretsConfig {
    #[serde(default)]
    pub required: Vec<String>,
    #[serde(default)]
    pub optional: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpConfig {
    #[serde(default)]
    pub transport: McpTransport,
    #[serde(default)]
    pub default_port: Option<u16>,
    #[serde(default = "default_serve_subcommand")]
    pub serve_subcommand: String,
}

fn default_serve_subcommand() -> String {
    "serve".to_string()
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            transport: McpTransport::default(),
            default_port: None,
            serve_subcommand: default_serve_subcommand(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum McpTransport {
    #[default]
    Stdio,
    Http,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WidgetConfig {
    pub label: String,
    pub kind: String,
    pub renderer: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionMode {
    Local,
    RemoteMcp {
        url: String,
        transport: McpTransport,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_native_manifest_and_resolves_connection_mode() {
        let manifest: ExtensionManifest = toml::from_str(
            r#"
[extension]
name = "fixture"
version = "1.0.0"
[runtime]
type = "native"
binary = "fixture"
[mcp]
transport = "http"
"#,
        )
        .unwrap();
        assert!(manifest.is_native());
        assert_eq!(
            manifest.connection_mode(Some("http://localhost")),
            ConnectionMode::RemoteMcp {
                url: "http://localhost".to_string(),
                transport: McpTransport::Http,
            }
        );
    }
}
