//! Stateless behavior policy published as an optional in-process service.

use std::sync::Arc;

use async_trait::async_trait;
use omegon_traits::{
    Feature, RuntimeActivationBoundary, RuntimeCapabilityId, RuntimeCleanupRequirement,
    RuntimeCompositionTransitionPolicy, RuntimeContributionDependency,
    RuntimeContributionGenerationId, RuntimeDependencyRequirement, RuntimeDependencyTarget,
    RuntimeFailureDisposition, RuntimeInProcessService, RuntimeLifecyclePolicy,
    RuntimeLifecycleRequirement, RuntimeServiceInterfaceId,
};

use crate::behavior::{BehaviorPolicyBinding, BehaviorPolicyService, DefaultBehaviorPolicy};

pub(crate) const BEHAVIOR_POLICY_CAPABILITY: &str = "service:behavior-policy";
pub(crate) const BEHAVIOR_POLICY_INTERFACE: &str = "interface:omegon-behavior-policy-v1";
const BEHAVIOR_POLICY_GENERATION: &str = "behavior-policy:v1";

pub(crate) fn behavior_policy_capability_id() -> RuntimeCapabilityId {
    RuntimeCapabilityId::new(BEHAVIOR_POLICY_CAPABILITY).expect("static capability id is valid")
}

pub(crate) fn behavior_policy_interface_id() -> RuntimeServiceInterfaceId {
    RuntimeServiceInterfaceId::new(BEHAVIOR_POLICY_INTERFACE).expect("static interface id is valid")
}

pub(crate) fn capture_behavior_policy(
    bus: &crate::bus::EventBus,
) -> anyhow::Result<Option<BehaviorPolicyBinding>> {
    Ok(bus
        .in_process_service::<dyn BehaviorPolicyService>(
            &behavior_policy_capability_id(),
            &behavior_policy_interface_id(),
        )?
        .map(|handle| BehaviorPolicyBinding {
            capability_id: handle.capability_id,
            owner: handle.owner,
            generation_id: handle.generation_id,
            service: handle.service,
        }))
}

pub(crate) struct BehaviorPolicyFeature {
    service: Arc<dyn BehaviorPolicyService>,
}

/// Declares the loop host's optional service dependency even when a profile omits the provider.
pub(crate) struct BehaviorPolicyHostFeature;

#[async_trait]
impl Feature for BehaviorPolicyHostFeature {
    fn name(&self) -> &str {
        "behavior-policy-host"
    }

    fn runtime_dependencies(&self) -> Vec<RuntimeContributionDependency> {
        vec![RuntimeContributionDependency {
            target: RuntimeDependencyTarget::Capability {
                id: behavior_policy_capability_id(),
            },
            requirement: RuntimeDependencyRequirement::Optional,
        }]
    }
}

impl Default for BehaviorPolicyFeature {
    fn default() -> Self {
        Self {
            service: Arc::new(DefaultBehaviorPolicy),
        }
    }
}

#[async_trait]
impl Feature for BehaviorPolicyFeature {
    fn name(&self) -> &str {
        "behavior-policy"
    }

    fn runtime_contribution_generation_id(&self) -> Option<RuntimeContributionGenerationId> {
        Some(
            RuntimeContributionGenerationId::new(BEHAVIOR_POLICY_GENERATION)
                .expect("static generation id is valid"),
        )
    }

    fn runtime_in_process_services(&self) -> Vec<RuntimeInProcessService> {
        vec![RuntimeInProcessService::no_resource_read_service(
            behavior_policy_capability_id(),
            behavior_policy_interface_id(),
            Arc::clone(&self.service),
        )]
    }

    fn runtime_lifecycle_policy(&self) -> Option<RuntimeLifecyclePolicy> {
        Some(RuntimeLifecyclePolicy {
            requirement: RuntimeLifecycleRequirement::Optional,
            failure_disposition: RuntimeFailureDisposition::DegradeLocally,
            readiness_timeout_ms: 0,
            heartbeat_timeout_ms: None,
            restart_limit: 0,
        })
    }

    fn runtime_transition_policy(&self) -> Option<RuntimeCompositionTransitionPolicy> {
        Some(RuntimeCompositionTransitionPolicy {
            activation_boundary: RuntimeActivationBoundary::Boot,
            cleanup: RuntimeCleanupRequirement::Strict,
            cleanup_timeout_ms: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn behavior_policy_publishes_and_captures_full_identity() {
        let mut bus = crate::bus::EventBus::new();
        bus.register(Box::new(BehaviorPolicyFeature::default()));
        bus.try_finalize().unwrap();

        let binding = capture_behavior_policy(&bus).unwrap().unwrap();
        assert_eq!(binding.capability_id.as_str(), BEHAVIOR_POLICY_CAPABILITY);
        assert_eq!(binding.owner.as_str(), "feature:behavior-policy");
        assert_eq!(binding.generation_id.as_str(), BEHAVIOR_POLICY_GENERATION);
        assert_eq!(
            binding.service.infer_unpinned_task_mode("explain this"),
            crate::conversation::TaskMode::Research
        );
    }

    #[test]
    fn behavior_policy_absence_is_none_without_fabricated_identity() {
        let mut bus = crate::bus::EventBus::new();
        bus.register(Box::new(BehaviorPolicyHostFeature));
        bus.try_finalize().unwrap();

        assert!(capture_behavior_policy(&bus).unwrap().is_none());
        let diagnostics = bus.composition_diagnostic_projection().unwrap().diagnostics;
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code.as_str() == "graph:optional_dependency_unavailable"
                && diagnostic.severity == omegon_traits::RuntimeDiagnosticSeverity::Warning
                && diagnostic
                    .subject
                    .contribution_id
                    .as_ref()
                    .is_some_and(|id| id.as_str() == "feature:behavior-policy-host")
        }));
    }

    #[test]
    fn behavior_policy_declares_strict_zero_timeout_no_resource_lifecycle() {
        let feature = BehaviorPolicyFeature::default();
        let lifecycle = feature.runtime_lifecycle_policy().unwrap();
        let transition = feature.runtime_transition_policy().unwrap();
        let service = feature.runtime_in_process_services().pop().unwrap();

        assert_eq!(lifecycle.requirement, RuntimeLifecycleRequirement::Optional);
        assert_eq!(transition.cleanup, RuntimeCleanupRequirement::Strict);
        assert_eq!(transition.cleanup_timeout_ms, 0);
        assert!(service.capability.effects.is_empty());
        assert!(service.capability.bindings.is_empty());
    }
}
