use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use omegon_traits::{ContentBlock, ToolDefinition, ToolProvider, ToolResult};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::nex::substrate::NexSubstrateDelegation;
use crate::tool_registry::core as reg;
use crate::tools::WorkspaceBoundary;

pub struct NexSubstrateProvider {
    cwd: PathBuf,
    boundary: Option<WorkspaceBoundary>,
    delegations: Vec<NexSubstrateDelegation>,
    executor: Option<Arc<dyn NexDelegationExecutor>>,
}

#[async_trait]
pub trait NexDelegationExecutor: Send + Sync {
    async fn execute_devenv_inspect(&self, tool: &str, path: &Path) -> anyhow::Result<ToolResult>;
}

impl NexSubstrateProvider {
    pub fn new(cwd: PathBuf) -> Self {
        Self {
            cwd,
            boundary: None,
            delegations: Vec::new(),
            executor: None,
        }
    }

    pub fn with_boundary(mut self, boundary: WorkspaceBoundary) -> Self {
        self.boundary = Some(boundary);
        self
    }

    pub fn with_delegations(mut self, delegations: Vec<NexSubstrateDelegation>) -> Self {
        self.delegations = delegations;
        self
    }

    pub fn with_executor(mut self, executor: Arc<dyn NexDelegationExecutor>) -> Self {
        self.executor = Some(executor);
        self
    }

    fn resolve_path(&self, path: &str) -> anyhow::Result<PathBuf> {
        if let Some(boundary) = &self.boundary {
            return boundary.check_path(path);
        }
        let path = expand_tilde(path);
        let path = if path.is_absolute() {
            path
        } else {
            self.cwd.join(path)
        };
        Ok(path)
    }

    fn unavailable(&self, tool_name: &str) -> anyhow::Result<ToolResult> {
        let (description, parameters) = unavailable_contract(tool_name)
            .ok_or_else(|| anyhow::anyhow!("unsupported Nex compatibility tool: {tool_name}"))?;
        Ok(ToolResult {
            content: vec![ContentBlock::Text {
                text: format!(
                    "{description} is unavailable because the default omegon-nex extension is not installed or enabled. Run `just install-default-extensions` from an Omegon checkout or install omegon-nex explicitly."
                ),
            }],
            details: json!({
                "is_error": true,
                "blocked": true,
                "reason": "nex_extension_unavailable",
                "tool": tool_name,
                "parameters": parameters,
                "extension": "omegon-nex",
            }),
        })
    }
}

#[async_trait]
impl ToolProvider for NexSubstrateProvider {
    fn tools(&self) -> Vec<ToolDefinition> {
        let mut tools = Vec::new();
        if !self
            .delegations
            .iter()
            .any(|delegation| delegation.tool == "nex_capability")
        {
            tools.push(unavailable_definition(reg::NEX_CAPABILITY));
        }
        if !self
            .delegations
            .iter()
            .any(|delegation| delegation.tool == "nex_substrate")
        {
            tools.push(unavailable_definition(reg::NEX_SUBSTRATE));
        }
        tools
    }

    async fn execute(
        &self,
        tool_name: &str,
        _call_id: &str,
        args: Value,
        _cancel: CancellationToken,
    ) -> anyhow::Result<ToolResult> {
        if tool_name == reg::NEX_CAPABILITY {
            return self.unavailable(tool_name);
        }
        if tool_name != reg::NEX_SUBSTRATE {
            anyhow::bail!("unsupported Nex compatibility tool: {tool_name}");
        }
        if self.executor.is_none() {
            return self.unavailable(tool_name);
        }
        let action = args["action"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'action' argument"))?;
        if action != "inspect" {
            anyhow::bail!(
                "unsupported nex_substrate action: {action}; MVP is read-only and supports only inspect"
            );
        }
        let mode = args["mode"].as_str().unwrap_or("devenv");
        if mode != "devenv" {
            anyhow::bail!("unsupported nex_substrate mode: {mode}; MVP supports only devenv");
        }
        let path = match args["path"].as_str() {
            Some(path) => self.resolve_path(path)?,
            None => self.cwd.clone(),
        };
        let mut report = if let Some(delegation) =
            crate::nex::substrate::delegation_for_command(&self.delegations, "devenv.inspect")
        {
            if let Some(executor) = &self.executor {
                match executor
                    .execute_devenv_inspect(&delegation.tool, &path)
                    .await
                {
                    Ok(result) => report_from_delegated_result(&path, result)?,
                    Err(error) => {
                        let mut report = crate::nex::substrate::inspect_devenv(&path).await;
                        report.diagnostics.push(format!(
                            "omegon-nex delegation failed; used direct fallback: {error}"
                        ));
                        report
                    }
                }
            } else {
                crate::nex::substrate::inspect_devenv(&path).await
            }
        } else {
            crate::nex::substrate::inspect_devenv(&path).await
        };
        report.delegation =
            crate::nex::substrate::delegation_for_command(&self.delegations, "devenv.inspect");
        Ok(ToolResult {
            content: vec![ContentBlock::Text {
                text: crate::nex::substrate::summary_text(&report),
            }],
            details: serde_json::to_value(&report)?,
        })
    }
}

fn unavailable_definition(tool_name: &str) -> ToolDefinition {
    let (description, parameters) =
        unavailable_contract(tool_name).expect("known Nex compatibility tool");
    ToolDefinition {
        name: tool_name.into(),
        label: tool_name.into(),
        description: format!("{description} Requires the default omegon-nex extension."),
        parameters,
        capabilities: vec![omegon_traits::ToolCapability::RepoInspection],
    }
}

fn unavailable_contract(tool_name: &str) -> Option<(&'static str, Value)> {
    match tool_name {
        reg::NEX_CAPABILITY => Some((
            "Read-only Nex capability resolution",
            json!({
                "type": "object",
                "properties": {
                    "action": {"type": "string", "enum": ["check", "resolve"]},
                    "capability": {"type": "string"},
                    "profile": {"type": "string"}
                },
                "required": ["action", "capability"]
            }),
        )),
        reg::NEX_SUBSTRATE => Some((
            "Read-only Nex substrate inspection",
            json!({
                "type": "object",
                "properties": {
                    "action": {"type": "string", "enum": ["inspect"]},
                    "path": {"type": "string"},
                    "mode": {"type": "string", "enum": ["devenv"]}
                },
                "required": ["action"]
            }),
        )),
        _ => None,
    }
}

fn expand_tilde(path: &str) -> PathBuf {
    if path == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from(path));
    }
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest);
    }
    Path::new(path).to_path_buf()
}

fn report_from_delegated_result(
    path: &Path,
    result: ToolResult,
) -> anyhow::Result<crate::nex::substrate::NexSubstrateReport> {
    let report_json = result
        .details
        .get("data")
        .and_then(|data| data.get("report"))
        .cloned()
        .or_else(|| result.details.get("report").cloned())
        .ok_or_else(|| {
            anyhow::anyhow!("delegated nex_devenv_inspect result did not include data.report")
        })?;
    let policy = crate::nex::substrate::derive_policy(&report_json);
    let mut diagnostics = Vec::new();
    if let Some(text) = result
        .details
        .get("data")
        .and_then(|data| data.get("degraded_reason"))
        .and_then(Value::as_str)
    {
        diagnostics.push(format!("omegon-nex degraded: {text}"));
    }
    Ok(crate::nex::substrate::NexSubstrateReport {
        schema: crate::nex::substrate::REPORT_SCHEMA,
        source: "omegon-nex",
        nex_available: true,
        path: path.display().to_string(),
        mode: "devenv".to_string(),
        reports: crate::nex::substrate::NexSubstrateReports {
            devenv_import: Some(report_json),
        },
        delegation: None,
        policy,
        diagnostics,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use omegon_traits::ToolProvider;
    use serde_json::json;

    #[tokio::test]
    async fn compatibility_tools_are_deterministically_unavailable_without_extension() {
        let dir = tempfile::tempdir().unwrap();
        let provider = NexSubstrateProvider::new(dir.path().to_path_buf());
        assert_eq!(provider.tools().len(), 2);

        for tool in [reg::NEX_CAPABILITY, reg::NEX_SUBSTRATE] {
            let result = provider
                .execute(tool, "test", json!({}), CancellationToken::new())
                .await
                .unwrap();
            assert_eq!(result.details["is_error"], true);
            assert_eq!(result.details["reason"], "nex_extension_unavailable");
            assert!(
                matches!(result.content.first(), Some(ContentBlock::Text { text }) if text.contains("omegon-nex") && text.contains("unavailable"))
            );
        }
    }
}
