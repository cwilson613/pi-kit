//! Host-owned codescan schemas backed by an optional native extension.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use async_trait::async_trait;
use omegon_codescan_contracts::{
    CODESCAN_PROTOCOL_VERSION, CODESCAN_RPC_METHOD, CodescanErrorCodeV1, CodescanErrorV1,
    CodescanOperationV1, CodescanOutcomeV1, CodescanRequestV1, CodescanResponseV1,
};
use omegon_traits::Feature;
use tokio_util::sync::CancellationToken;

pub(crate) const CODESCAN_EXTENSION: &str = "omegon-codescan";
pub(crate) const CODESCAN_CAPABILITY: &str = "service:codescan";

#[derive(Debug)]
pub(crate) enum CodescanCallError {
    Disabled(crate::component_policy::ComponentPolicyDecision),
    Unavailable,
    InvalidResponse,
    Operation(CodescanErrorV1),
}

impl CodescanCallError {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::Disabled(_) => "service:disabled",
            Self::Unavailable => "service:unavailable",
            Self::InvalidResponse => "service:invalid_response",
            Self::Operation(error) => match error.code {
                CodescanErrorCodeV1::Cancelled => "request:cancelled",
                CodescanErrorCodeV1::InvalidRequest => "request:invalid",
                CodescanErrorCodeV1::UnsupportedProtocol => "service:incompatible",
                CodescanErrorCodeV1::WorkspaceUnavailable
                | CodescanErrorCodeV1::DatabaseUnavailable => "service:unavailable",
                CodescanErrorCodeV1::IndexFailed
                | CodescanErrorCodeV1::SearchFailed
                | CodescanErrorCodeV1::Internal => "service:operation_failed",
            },
        }
    }

    pub(crate) fn disabled_evidence(
        &self,
    ) -> Option<&crate::component_policy::ComponentPolicyDecision> {
        match self {
            Self::Disabled(decision) => Some(decision),
            _ => None,
        }
    }
}

impl std::fmt::Display for CodescanCallError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disabled(decision) => write!(
                formatter,
                "codescan component {} is disabled by policy",
                decision.component_id
            ),
            Self::Unavailable => formatter.write_str("codescan extension is unavailable"),
            Self::InvalidResponse => {
                formatter.write_str("codescan extension returned an invalid response")
            }
            Self::Operation(error) => formatter.write_str(&error.message),
        }
    }
}

#[async_trait]
pub(crate) trait CodescanClient: Send + Sync {
    fn process_provenance(&self) -> Option<crate::extensions::ExtensionProcessProvenance> {
        None
    }

    async fn execute(
        &self,
        operation: CodescanOperationV1,
        cancellation: CancellationToken,
    ) -> Result<CodescanResponseV1, CodescanCallError>;
}

struct ExtensionCodescanClient {
    handle: crate::extensions::ExtensionPollingHandle,
}

#[async_trait]
impl CodescanClient for ExtensionCodescanClient {
    fn process_provenance(&self) -> Option<crate::extensions::ExtensionProcessProvenance> {
        Some(self.handle.process_provenance())
    }

    async fn execute(
        &self,
        operation: CodescanOperationV1,
        cancellation: CancellationToken,
    ) -> Result<CodescanResponseV1, CodescanCallError> {
        let request = CodescanRequestV1::new(operation);
        let caller_cancelled = cancellation.clone();
        let value = self
            .handle
            .rpc_call_with_cancel(
                CODESCAN_RPC_METHOD,
                serde_json::to_value(request).map_err(|_| CodescanCallError::InvalidResponse)?,
                cancellation,
                Some(Duration::from_secs(120)),
            )
            .await
            .map_err(|_| {
                if caller_cancelled.is_cancelled() {
                    CodescanCallError::Operation(CodescanErrorV1 {
                        code: CodescanErrorCodeV1::Cancelled,
                        message: "request cancelled".into(),
                    })
                } else {
                    CodescanCallError::Unavailable
                }
            })?;
        let outcome = serde_json::from_value::<CodescanOutcomeV1>(value)
            .map_err(|_| CodescanCallError::InvalidResponse)?;
        match outcome {
            CodescanOutcomeV1::Ok {
                protocol_version,
                response,
            } if protocol_version == CODESCAN_PROTOCOL_VERSION => Ok(response),
            CodescanOutcomeV1::Error {
                protocol_version,
                error,
            } if protocol_version == CODESCAN_PROTOCOL_VERSION => {
                Err(CodescanCallError::Operation(error))
            }
            _ => Err(CodescanCallError::InvalidResponse),
        }
    }
}

#[derive(Clone, Default)]
pub(crate) struct CodescanBinding {
    client: Arc<OnceLock<Option<Arc<dyn CodescanClient>>>>,
    disabled: Option<Arc<crate::component_policy::ComponentPolicyDecision>>,
}

impl CodescanBinding {
    pub(crate) fn from_component_decision(
        decision: Option<&crate::component_policy::ComponentPolicyDecision>,
    ) -> Self {
        Self {
            client: Arc::default(),
            disabled: decision
                .filter(|decision| !decision.enabled)
                .cloned()
                .map(Arc::new),
        }
    }

    pub(crate) fn capture(
        &self,
        handle: Option<crate::extensions::ExtensionPollingHandle>,
    ) -> anyhow::Result<()> {
        let client = handle
            .map(|handle| Arc::new(ExtensionCodescanClient { handle }) as Arc<dyn CodescanClient>);
        self.client
            .set(client)
            .map_err(|_| anyhow::anyhow!("codescan extension binding was already captured"))
    }

    pub(crate) async fn execute(
        &self,
        operation: CodescanOperationV1,
        cancellation: CancellationToken,
    ) -> Result<CodescanResponseV1, CodescanCallError> {
        if let Some(decision) = &self.disabled {
            return Err(CodescanCallError::Disabled((**decision).clone()));
        }
        let client = self
            .client
            .get()
            .and_then(Clone::clone)
            .ok_or(CodescanCallError::Unavailable)?;
        client.execute(operation, cancellation).await
    }

    pub(crate) fn process_provenance(
        &self,
    ) -> Option<crate::extensions::ExtensionProcessProvenance> {
        self.client
            .get()
            .and_then(Option::as_ref)
            .and_then(|client| client.process_provenance())
    }

    #[cfg(test)]
    pub(crate) fn from_test_client(client: Arc<dyn CodescanClient>) -> Self {
        let binding = Self::default();
        assert!(
            binding.client.set(Some(client)).is_ok(),
            "fresh test binding"
        );
        binding
    }
}

pub(crate) struct CodescanFeature {
    provider: crate::tools::codebase_search::CodescanProvider,
}

impl CodescanFeature {
    pub(crate) fn new(repo_path: PathBuf, binding: CodescanBinding) -> Self {
        Self {
            provider: crate::tools::codebase_search::CodescanProvider::new(repo_path, binding),
        }
    }
}

#[async_trait]
impl Feature for CodescanFeature {
    fn name(&self) -> &str {
        "codescan-adapter"
    }

    fn tools(&self) -> Vec<omegon_traits::ToolDefinition> {
        omegon_traits::ToolProvider::tools(&self.provider)
    }

    fn runtime_tool_surfaces(&self, tool_name: &str) -> Option<Vec<omegon_traits::RuntimeSurface>> {
        matches!(
            tool_name,
            crate::tool_registry::codescan::CODEBASE_SEARCH
                | crate::tool_registry::codescan::CODEBASE_INDEX
        )
        .then(|| {
            vec![
                omegon_traits::RuntimeSurface::Model,
                omegon_traits::RuntimeSurface::Cli,
                omegon_traits::RuntimeSurface::Acp,
            ]
        })
    }

    fn runtime_tool_policy(&self, tool_name: &str) -> Option<omegon_traits::RuntimeToolPolicy> {
        use omegon_traits::{
            RuntimeDeduplication, RuntimeEffect, RuntimeExecutionPolicy, RuntimeIdempotency,
            RuntimeParallelism, RuntimePrincipalClass, RuntimeRetryClass, RuntimeTimeoutClass,
            RuntimeToolPolicy, RuntimeTransactionBehavior,
        };

        matches!(
            tool_name,
            crate::tool_registry::codescan::CODEBASE_SEARCH
                | crate::tool_registry::codescan::CODEBASE_INDEX
        )
        .then(|| RuntimeToolPolicy {
            effects: vec![
                RuntimeEffect::FilesystemRead,
                RuntimeEffect::ProcessSpawn,
                RuntimeEffect::NetworkAccess,
                RuntimeEffect::SecretDelivery,
            ],
            execution: RuntimeExecutionPolicy {
                principals: vec![
                    RuntimePrincipalClass::Model,
                    RuntimePrincipalClass::Operator,
                ],
                timeout_class: RuntimeTimeoutClass::Interactive,
                retry_class: RuntimeRetryClass::Never,
                idempotency: RuntimeIdempotency::NonIdempotent,
                deduplication: RuntimeDeduplication::Unsupported,
                parallelism: RuntimeParallelism::Serial,
                transaction: RuntimeTransactionBehavior::None,
                mutation_fence: None,
                max_attempts: None,
            },
        })
    }

    async fn execute(
        &self,
        tool_name: &str,
        call_id: &str,
        args: serde_json::Value,
        cancel: CancellationToken,
    ) -> anyhow::Result<omegon_traits::ToolResult> {
        omegon_traits::ToolProvider::execute(&self.provider, tool_name, call_id, args, cancel).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disabled_codescan_decision() -> crate::component_policy::ComponentPolicyDecision {
        crate::component_policy::ComponentPolicyDecision {
            component_id: "core:codescan".into(),
            enabled: false,
            evidence: vec![],
            determining_source: crate::component_policy::ComponentPolicySource::SelectedProfile {
                profile: "compliance".into(),
                path: "/repo/.omegon/profiles/compliance.json".into(),
            },
        }
    }

    #[tokio::test]
    async fn absent_extension_is_typed_unavailable() {
        let error = CodescanBinding::default()
            .execute(
                CodescanOperationV1::Index(omegon_codescan_contracts::IndexRequestV1 {
                    invalidate: false,
                }),
                CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code(), "service:unavailable");
    }

    #[tokio::test]
    async fn policy_disabled_extension_is_distinct_from_unavailable() {
        let binding = CodescanBinding::from_component_decision(Some(&disabled_codescan_decision()));
        let error = binding
            .execute(
                CodescanOperationV1::Index(omegon_codescan_contracts::IndexRequestV1 {
                    invalidate: false,
                }),
                CancellationToken::new(),
            )
            .await
            .unwrap_err();

        assert_eq!(error.code(), "service:disabled");
        let evidence = error.disabled_evidence().expect("typed disabled evidence");
        assert_eq!(evidence.component_id, "core:codescan");
        assert!(matches!(
            evidence.determining_source,
            crate::component_policy::ComponentPolicySource::SelectedProfile { .. }
        ));
    }
}
