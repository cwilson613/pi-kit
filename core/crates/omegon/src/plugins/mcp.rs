//! MCP plugin Feature — connects to MCP servers via stdio child-process transport,
//! discovers their tools, and exposes them as Omegon tools.
//!
//! MCP servers are declared in plugin.toml or project config:
//! ```toml
//! [mcp_servers.filesystem]
//! command = "npx"
//! args = ["-y", "@modelcontextprotocol/server-filesystem", "/home/user"]
//!
//! [mcp_servers.brave-search]
//! command = "npx"
//! args = ["-y", "@modelcontextprotocol/server-brave-search"]
//! env = { BRAVE_API_KEY = "..." }
//! ```

use async_trait::async_trait;
use omegon_traits::*;
use rmcp::{
    handler::client::ClientHandler,
    model::*,
    service::{self, RoleClient, RunningService},
    transport::TokioChildProcess,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::Mutex;

/// Configuration for a single MCP server.
#[derive(Debug, Clone, Deserialize)]
pub struct McpServerConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_timeout() -> u64 { 30 }

/// A discovered tool from an MCP server.
#[derive(Debug, Clone)]
struct McpTool {
    name: String,
    description: String,
    parameters: Value,
    server_name: String,
}

/// Minimal client handler — we don't need to handle server requests,
/// just connect and call tools.
#[derive(Clone)]
struct OmegonMcpClient;

impl ClientHandler for OmegonMcpClient {
    fn get_info(&self) -> ClientInfo {
        let mut impl_info = Implementation::default();
        impl_info.name = "omegon".into();
        impl_info.version = env!("CARGO_PKG_VERSION").into();
        InitializeRequestParams::new(ClientCapabilities::default(), impl_info)
    }
}

/// Running connection to an MCP server.
type McpConnection = RunningService<RoleClient, OmegonMcpClient>;

/// Feature that wraps one or more MCP server connections.
pub struct McpFeature {
    feature_name: String,
    tools: Vec<McpTool>,
    clients: Arc<Mutex<HashMap<String, McpConnection>>>,
}

impl McpFeature {
    /// Connect to MCP servers and discover their tools.
    pub async fn connect(
        plugin_name: &str,
        servers: &HashMap<String, McpServerConfig>,
    ) -> anyhow::Result<Self> {
        let mut all_tools = Vec::new();
        let mut clients = HashMap::new();

        for (server_name, config) in servers {
            match Self::connect_one(server_name, config).await {
                Ok((server_tools, client)) => {
                    tracing::info!(
                        plugin = plugin_name,
                        server = server_name,
                        tools = server_tools.len(),
                        "MCP server connected"
                    );
                    all_tools.extend(server_tools);
                    clients.insert(server_name.clone(), client);
                }
                Err(e) => {
                    tracing::warn!(
                        plugin = plugin_name,
                        server = server_name,
                        error = %e,
                        "failed to connect MCP server — skipping"
                    );
                }
            }
        }

        Ok(Self {
            feature_name: plugin_name.to_string(),
            tools: all_tools,
            clients: Arc::new(Mutex::new(clients)),
        })
    }

    async fn connect_one(
        server_name: &str,
        config: &McpServerConfig,
    ) -> anyhow::Result<(Vec<McpTool>, McpConnection)> {
        let mut cmd = Command::new(&config.command);
        cmd.args(&config.args);
        for (key, value) in &config.env {
            cmd.env(key, resolve_env_template(value));
        }

        let transport = TokioChildProcess::new(cmd)?;
        let client = service::serve_client(OmegonMcpClient, transport).await?;

        // Discover tools via MCP tools/list
        let tools_result = client.list_tools(None).await?;
        let tools: Vec<McpTool> = tools_result
            .tools
            .into_iter()
            .map(|t| {
                // Convert the input_schema (Arc<Map>) to a serde_json::Value
                let params: Value = serde_json::to_value(&t.input_schema)
                    .unwrap_or_else(|_| serde_json::json!({"type": "object", "properties": {}}));
                McpTool {
                    name: format!("{}_{}", server_name, t.name),
                    description: t.description.map(|d| d.to_string()).unwrap_or_default(),
                    parameters: params,
                    server_name: server_name.to_string(),
                }
            })
            .collect();

        Ok((tools, client))
    }

    /// Parse "servername_toolname" → ("servername", "toolname").
    fn split_tool_name(prefixed: &str) -> (&str, &str) {
        if let Some(pos) = prefixed.find('_') {
            (&prefixed[..pos], &prefixed[pos + 1..])
        } else {
            ("", prefixed)
        }
    }
}

#[async_trait]
impl Feature for McpFeature {
    fn name(&self) -> &str {
        &self.feature_name
    }

    fn tools(&self) -> Vec<ToolDefinition> {
        self.tools
            .iter()
            .map(|t| ToolDefinition {
                name: t.name.clone(),
                label: format!("mcp:{}", t.server_name),
                description: t.description.clone(),
                parameters: t.parameters.clone(),
            })
            .collect()
    }

    async fn execute(
        &self,
        tool_name: &str,
        _call_id: &str,
        args: Value,
        _cancel: tokio_util::sync::CancellationToken,
    ) -> anyhow::Result<ToolResult> {
        let (server_name, mcp_name) = Self::split_tool_name(tool_name);
        let server_name = server_name.to_string();
        let mcp_name = mcp_name.to_string();

        let clients = self.clients.lock().await;
        let client = clients.get(&server_name).ok_or_else(|| {
            anyhow::anyhow!("MCP server '{}' not connected", server_name)
        })?;

        let arguments = if args.is_object() {
            Some(args.as_object().unwrap().clone())
        } else {
            None
        };

        let mut params = CallToolRequestParams::default();
        params.name = mcp_name.into();
        params.arguments = arguments;

        let result = client.call_tool(params).await?;

        // Convert MCP content to Omegon content blocks
        let content: Vec<ContentBlock> = result
            .content
            .into_iter()
            .filter_map(|c| match c.raw {
                RawContent::Text(t) => Some(ContentBlock::Text {
                    text: t.text.to_string(),
                }),
                RawContent::Image(img) => Some(ContentBlock::Image {
                    url: img.data.to_string(),
                    media_type: img.mime_type.to_string(),
                }),
                _ => None,
            })
            .collect();

        Ok(ToolResult {
            content,
            details: Value::Null,
        })
    }
}

/// Resolve `{ENV_VAR}` patterns from environment variables.
fn resolve_env_template(template: &str) -> String {
    let mut result = template.to_string();
    while let Some(start) = result.find('{') {
        if let Some(end) = result[start..].find('}') {
            let var = &result[start + 1..start + end];
            let value = std::env::var(var).unwrap_or_default();
            result = format!("{}{}{}", &result[..start], value, &result[start + end + 1..]);
        } else {
            break;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_env_template_basic() {
        unsafe { std::env::set_var("TEST_MCP_KEY", "secret123"); }
        let result = resolve_env_template("Bearer {TEST_MCP_KEY}");
        assert_eq!(result, "Bearer secret123");
        unsafe { std::env::remove_var("TEST_MCP_KEY"); }
    }

    #[test]
    fn resolve_env_template_missing_var() {
        let result = resolve_env_template("{NONEXISTENT_VAR_12345}");
        assert_eq!(result, "");
    }

    #[test]
    fn resolve_env_template_no_pattern() {
        let result = resolve_env_template("plain string");
        assert_eq!(result, "plain string");
    }

    #[test]
    fn split_tool_name_prefixed() {
        let (server, tool) = McpFeature::split_tool_name("filesystem_read_file");
        assert_eq!(server, "filesystem");
        assert_eq!(tool, "read_file");
    }

    #[test]
    fn split_tool_name_no_prefix() {
        let (server, tool) = McpFeature::split_tool_name("standalone");
        assert_eq!(server, "");
        assert_eq!(tool, "standalone");
    }

    #[test]
    fn mcp_server_config_deserialize() {
        let toml = r#"
            command = "npx"
            args = ["-y", "@modelcontextprotocol/server-filesystem", "/home"]
            timeout_secs = 60
        "#;
        let config: McpServerConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.command, "npx");
        assert_eq!(config.args.len(), 3);
        assert_eq!(config.timeout_secs, 60);
    }

    #[test]
    fn mcp_server_config_with_env() {
        let toml = r#"
            command = "npx"
            args = ["-y", "@modelcontextprotocol/server-brave-search"]
            [env]
            BRAVE_API_KEY = "{BRAVE_API_KEY}"
        "#;
        let config: McpServerConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.env["BRAVE_API_KEY"], "{BRAVE_API_KEY}");
    }

    #[test]
    fn mcp_server_config_defaults() {
        let toml = r#"command = "my-server""#;
        let config: McpServerConfig = toml::from_str(toml).unwrap();
        assert!(config.args.is_empty());
        assert!(config.env.is_empty());
        assert_eq!(config.timeout_secs, 30);
    }

    #[test]
    fn armory_manifest_with_mcp_servers() {
        let toml = r#"
            [plugin]
            type = "extension"
            id = "dev.example.mcp-tools"
            name = "MCP Tools"
            version = "1.0.0"
            description = "Tools via MCP servers"

            [mcp_servers.filesystem]
            command = "npx"
            args = ["-y", "@modelcontextprotocol/server-filesystem", "/home"]

            [mcp_servers.brave]
            command = "npx"
            args = ["-y", "@modelcontextprotocol/server-brave-search"]
            [mcp_servers.brave.env]
            BRAVE_API_KEY = "{BRAVE_API_KEY}"
        "#;
        let manifest = super::super::armory::ArmoryManifest::parse(toml).unwrap();
        assert_eq!(manifest.mcp_servers.len(), 2);
        assert!(manifest.mcp_servers.contains_key("filesystem"));
        assert!(manifest.mcp_servers.contains_key("brave"));
    }
}
