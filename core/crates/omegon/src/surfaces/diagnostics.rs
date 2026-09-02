//! Semantic projections for operator-facing `/status` and `/stats` diagnostics.
//!
//! Projection builders own fact selection and unknown-vs-zero semantics. Surface
//! adapters may format these values differently, but must not probe runtime state.

use serde::{Deserialize, Serialize};

use omegon_traits::{
    RuntimeCleanupAssurance, RuntimeCleanupState, RuntimeCompositionGenerationId,
    RuntimeContributionDeclaration, RuntimeContributionDiagnostic, RuntimeContributionId,
    RuntimeContributionLifecycleRecord, RuntimeContributionLifecycleState,
    RuntimeOwnedResourceRecord,
};

pub const DIAGNOSTIC_PROJECTION_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompatibilityDispatchMode {
    GraphDerivedLegacy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompatibilityDispatchProjection {
    pub mode: CompatibilityDispatchMode,
    pub parity_verified: bool,
    pub published_bindings: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositionContributionProjection {
    pub declaration: RuntimeContributionDeclaration,
    pub negotiated_protocol: u16,
    pub health: RuntimeContributionLifecycleState,
    pub cleanup_assurance: RuntimeCleanupAssurance,
    pub cleanup_state: RuntimeCleanupState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositionReplacementProjection {
    pub superseded: RuntimeContributionId,
    pub replacement: RuntimeContributionId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagedOwnerDisposition {
    Published,
    RejectedCandidate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedResourceDiagnosticProjection {
    pub record: RuntimeOwnedResourceRecord,
    pub stop_attempted: bool,
    pub force_attempted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedOwnerDiagnosticProjection {
    pub attempt_id: u64,
    pub disposition: ManagedOwnerDisposition,
    pub lifecycle: RuntimeContributionLifecycleRecord,
    pub resources: Vec<ManagedResourceDiagnosticProjection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositionDiagnosticProjection {
    pub version: u16,
    pub generation_id: RuntimeCompositionGenerationId,
    pub contributions: Vec<CompositionContributionProjection>,
    pub replacements: Vec<CompositionReplacementProjection>,
    pub activation_waves: Vec<Vec<RuntimeContributionId>>,
    pub diagnostics: Vec<RuntimeContributionDiagnostic>,
    pub compatibility_dispatch: CompatibilityDispatchProjection,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub managed_owners: Vec<ManagedOwnerDiagnosticProjection>,
}

impl CompositionDiagnosticProjection {
    pub fn render_markdown(&self) -> String {
        let mut output = format!(
            "\n\nComposition\n  Generation:   {}\n  Contributions: {}\n  Dispatch:     graph-derived legacy ({} bindings, parity {})",
            self.generation_id.as_str(),
            self.contributions.len(),
            self.compatibility_dispatch.published_bindings,
            if self.compatibility_dispatch.parity_verified {
                "verified"
            } else {
                "unverified"
            }
        );
        for contribution in &self.contributions {
            output.push_str(&format!(
                "\n  - {} [{}] generation={} health={} cleanup={}/{}",
                contribution.declaration.id.as_str(),
                serialized_label(&contribution.declaration.owner_tier),
                contribution.declaration.generation_id.as_str(),
                serialized_label(&contribution.health),
                serialized_label(&contribution.cleanup_assurance),
                serialized_label(&contribution.cleanup_state),
            ));
        }
        if !self.diagnostics.is_empty() {
            output.push_str("\n  Diagnostics:");
            for diagnostic in &self.diagnostics {
                output.push_str(&format!(
                    "\n  - {}: {}",
                    diagnostic.code.as_str(),
                    diagnostic.message
                ));
            }
        }
        if !self.managed_owners.is_empty() {
            output.push_str("\n  Managed owners:");
            for owner in &self.managed_owners {
                output.push_str(&format!(
                    "\n  - attempt={} disposition={} owner={} generation={} state={} boundary={} cleanup={}/{}",
                    owner.attempt_id,
                    serialized_label(&owner.disposition),
                    owner.lifecycle.contribution_id.as_str(),
                    owner.lifecycle.generation_id.as_str(),
                    serialized_label(&owner.lifecycle.state),
                    serialized_label(&owner.lifecycle.last_completed_boundary),
                    serialized_label(&owner.lifecycle.cleanup_assurance),
                    serialized_label(&owner.lifecycle.cleanup_state),
                ));
                if let Some(reason_code) = &owner.lifecycle.reason_code {
                    output.push_str(&format!(" reason_code={}", reason_code.as_str()));
                }
                if let Some(reason) = &owner.lifecycle.reason {
                    output.push_str(&format!(" reason={reason}"));
                }
                for resource in &owner.resources {
                    output.push_str(&format!(
                        "\n    - resource={} kind={} cleanup={}/{} stop={} force={} reason={}",
                        resource.record.id.as_str(),
                        serialized_label(&resource.record.kind),
                        serialized_label(&resource.record.cleanup_assurance),
                        serialized_label(&resource.record.cleanup_state),
                        resource.stop_attempted,
                        resource.force_attempted,
                        resource.reason.as_deref().unwrap_or("none"),
                    ));
                }
            }
        }
        output
    }
}

fn serialized_label(value: &impl Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".into())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessStatusProjection {
    pub version: u16,
    pub harness: serde_json::Value,
    pub runtime_generation: u64,
    pub session_id: String,
    pub instance_id: String,
    pub automation_level: String,
    pub automation_summary: String,
    #[serde(skip)]
    pub bootstrap_markdown: String,
}

impl HarnessStatusProjection {
    pub fn new(
        harness: serde_json::Value,
        runtime_generation: u64,
        session_id: impl Into<String>,
        instance_id: impl Into<String>,
        automation_level: impl Into<String>,
        automation_summary: impl Into<String>,
        bootstrap_markdown: impl Into<String>,
    ) -> Self {
        Self {
            version: DIAGNOSTIC_PROJECTION_VERSION,
            harness,
            runtime_generation,
            session_id: session_id.into(),
            instance_id: instance_id.into(),
            automation_level: automation_level.into(),
            automation_summary: automation_summary.into(),
            bootstrap_markdown: bootstrap_markdown.into(),
        }
    }

    pub fn render_markdown(&self) -> String {
        format!(
            "{}\nRuntime\n  Generation:   {}\n  Session:      {}\n  Instance:     {}\nAutomation\n  Level:        {} ({})",
            self.bootstrap_markdown,
            self.runtime_generation,
            self.session_id,
            self.instance_id,
            self.automation_level,
            self.automation_summary,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionStatsProjection {
    pub version: u16,
    pub turns: u32,
    /// `None` means this surface has no authoritative tool-call observation.
    pub tool_calls: Option<u32>,
    pub model: String,
    pub thinking: String,
    pub posture: String,
    pub estimated_context_tokens: usize,
    pub context_window: usize,
    pub max_turns: u32,
    pub persona: Option<String>,
    pub tone: Option<String>,
    pub authenticated_providers: Option<usize>,
    pub provider_count: Option<usize>,
    pub mcp_servers: Option<usize>,
    pub memory_available: Option<bool>,
    pub cleave_available: Option<bool>,
}

impl SessionStatsProjection {
    pub fn context_usage_percent(&self) -> Option<f64> {
        (self.context_window > 0)
            .then(|| (self.estimated_context_tokens as f64 / self.context_window as f64) * 100.0)
    }

    pub fn render_markdown(&self) -> String {
        let tool_calls = self
            .tool_calls
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let context = self
            .context_usage_percent()
            .map(|percent| {
                format!(
                    "{} tokens ({percent:.0}% of {})",
                    self.estimated_context_tokens, self.context_window
                )
            })
            .unwrap_or_else(|| {
                format!("{} tokens (window unknown)", self.estimated_context_tokens)
            });

        let mut output = format!(
            "Session Overview\n\nActivity\n  Turns:            {}\n  Tool calls:       {}\n  Model:            {}\n  Thinking:         {}\n  Posture:          {}\n\nContext\n  Usage:            {}\n  Max turns:        {}",
            self.turns,
            tool_calls,
            self.model,
            self.thinking,
            self.posture,
            context,
            self.max_turns,
        );

        if self.persona.is_some()
            || self.tone.is_some()
            || self.provider_count.is_some()
            || self.mcp_servers.is_some()
        {
            output.push_str("\n\nHarness");
            if let Some(persona) = &self.persona {
                output.push_str(&format!("\n  Persona:          {persona}"));
            }
            if let Some(tone) = &self.tone {
                output.push_str(&format!("\n  Tone:             {tone}"));
            }
            if let (Some(authenticated), Some(total)) =
                (self.authenticated_providers, self.provider_count)
            {
                output.push_str(&format!(
                    "\n  Providers:        {authenticated}/{total} authenticated"
                ));
            }
            if let Some(servers) = self.mcp_servers {
                output.push_str(&format!("\n  MCP servers:      {servers}"));
            }
        }

        if self.memory_available.is_some() || self.cleave_available.is_some() {
            output.push_str("\n\nCapabilities");
            if let Some(available) = self.memory_available {
                output.push_str(&format!(
                    "\n  Memory:           {}",
                    if available {
                        "available"
                    } else {
                        "UNAVAILABLE"
                    }
                ));
            }
            if let Some(available) = self.cleave_available {
                output.push_str(&format!(
                    "\n  Cleave:           {}",
                    if available {
                        "available"
                    } else {
                        "UNAVAILABLE"
                    }
                ));
            }
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_tool_telemetry_is_not_rendered_as_zero() {
        let projection = SessionStatsProjection {
            version: DIAGNOSTIC_PROJECTION_VERSION,
            turns: 2,
            tool_calls: None,
            model: "test:model".into(),
            thinking: "minimal".into(),
            posture: "architect".into(),
            estimated_context_tokens: 12,
            context_window: 0,
            max_turns: 20,
            persona: None,
            tone: None,
            authenticated_providers: None,
            provider_count: None,
            mcp_servers: None,
            memory_available: None,
            cleave_available: None,
        };

        let rendered = projection.render_markdown();
        assert!(rendered.contains("Tool calls:       unknown"));
        assert!(rendered.contains("12 tokens (window unknown)"));
        assert!(!rendered.contains("NaN"));
        assert!(!rendered.contains("inf"));
    }

    #[test]
    fn projection_serialization_contains_no_secret_values() {
        let projection = HarnessStatusProjection::new(
            serde_json::json!({}),
            1,
            "session",
            "instance",
            "guarded",
            "confirm mutations",
            "Harness",
        );
        let value = serde_json::to_value(&projection).unwrap();
        assert_eq!(value["harness"], serde_json::json!({}));
        assert!(value.get("bootstrap_markdown").is_none());
        let json = serde_json::to_string(&value).unwrap();
        assert!(!json.contains("secret_value"));
        assert!(!json.contains("api_key"));
    }
}
