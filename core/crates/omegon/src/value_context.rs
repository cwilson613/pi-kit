//! Typed protocol for printable values and their consumer-specific availability.
//!
//! This first slice describes current behavior without widening propagation.
//! Consumers must opt into value injection explicitly; a binding existing in
//! the control plane does not imply that it is present in a child environment.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueSensitivity {
    Public,
    Secret,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueLifetime {
    Process,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueSource {
    ControlPlane,
    ProcessEnvironment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueConsumer {
    ControlPlane,
    BashTool,
    InteractiveTerminal,
    EnvironmentRecipe,
    CommandRecipe,
    Extension,
    McpServer,
    Delegate,
    Container,
}

impl ValueConsumer {
    pub const DIAGNOSTIC_CONSUMERS: [Self; 8] = [
        Self::BashTool,
        Self::InteractiveTerminal,
        Self::EnvironmentRecipe,
        Self::CommandRecipe,
        Self::Extension,
        Self::McpServer,
        Self::Delegate,
        Self::Container,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::ControlPlane => "control plane",
            Self::BashTool => "bash tool",
            Self::InteractiveTerminal => "interactive terminal",
            Self::EnvironmentRecipe => "env recipe",
            Self::CommandRecipe => "cmd recipe",
            Self::Extension => "extension",
            Self::McpServer => "MCP server",
            Self::Delegate => "delegate",
            Self::Container => "container",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueAvailability {
    Available,
    Unavailable,
    ExplicitDeclarationRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValueContextDiagnostic {
    pub consumer: ValueConsumer,
    pub availability: ValueAvailability,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValueRecord {
    pub name: String,
    pub value: String,
    pub sensitivity: ValueSensitivity,
    pub lifetime: ValueLifetime,
    pub source: ValueSource,
}

impl ValueRecord {
    pub fn public_control_plane(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            sensitivity: ValueSensitivity::Public,
            lifetime: ValueLifetime::Process,
            source: ValueSource::ControlPlane,
        }
    }

    pub fn diagnostic_for(&self, consumer: ValueConsumer) -> ValueContextDiagnostic {
        let process_env_available = std::env::var_os(&self.name).is_some();
        let (availability, reason) = match consumer {
            ValueConsumer::ControlPlane => (
                ValueAvailability::Available,
                "stored in Omegon's process-local control-plane registry".to_string(),
            ),
            ValueConsumer::BashTool
            | ValueConsumer::InteractiveTerminal
            | ValueConsumer::EnvironmentRecipe
            | ValueConsumer::CommandRecipe => {
                if process_env_available {
                    (
                        ValueAvailability::Available,
                        "a same-named value exists in Omegon's inherited process environment; the control-plane value is not injected".to_string(),
                    )
                } else {
                    (
                        ValueAvailability::Unavailable,
                        "control-plane variables are not injected into this consumer".to_string(),
                    )
                }
            }
            ValueConsumer::Extension | ValueConsumer::McpServer => (
                ValueAvailability::ExplicitDeclarationRequired,
                "requires an explicit manifest/config binding; control-plane variables are not ambient".to_string(),
            ),
            ValueConsumer::Delegate | ValueConsumer::Container => (
                ValueAvailability::Unavailable,
                "no value-context propagation contract exists for this consumer yet".to_string(),
            ),
        };
        ValueContextDiagnostic {
            consumer,
            availability,
            reason,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_plane_record_does_not_claim_ambient_bash_or_recipe_visibility() {
        let name = format!("OMEGON_VALUE_PROTOCOL_ABSENT_{}", std::process::id());
        let record = ValueRecord::public_control_plane(name, "staging");

        for consumer in [
            ValueConsumer::BashTool,
            ValueConsumer::InteractiveTerminal,
            ValueConsumer::EnvironmentRecipe,
            ValueConsumer::CommandRecipe,
            ValueConsumer::Delegate,
            ValueConsumer::Container,
        ] {
            assert_eq!(
                record.diagnostic_for(consumer).availability,
                ValueAvailability::Unavailable
            );
        }
    }

    #[test]
    fn extensions_require_explicit_declaration() {
        let record = ValueRecord::public_control_plane("PROJECT_ENV", "staging");
        assert_eq!(
            record.diagnostic_for(ValueConsumer::Extension).availability,
            ValueAvailability::ExplicitDeclarationRequired
        );
    }
}
