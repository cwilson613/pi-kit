//! Immutable session execution-driver and provider-route-service capture.

use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::bridge::LlmBridge;
use crate::context::ContextManager;
use crate::conversation::ConversationState;
use crate::r#loop::LoopConfig;
use crate::loop_driver::{LoopDriverContract, LoopDriverExecution, LoopDriverTurn};
use crate::provider_route_service::ProviderRouteServiceContract;
use crate::session_authority::{
    AuthorityError, ExecutionBindingGeneration, ExecutionBindingMigrationError,
    ExecutionBindingMigrationRejection, SessionAuthorityHandle,
};

const BUILTIN_DRIVER_GENERATION: &str = "loop-driver:release-coupled/builtin-v1";
const BUILTIN_ROUTE_SERVICE_GENERATION: &str = "provider-route-service:builtin-v1";

#[derive(Clone)]
pub(crate) struct SessionExecutionBinding {
    generation: ExecutionBindingGeneration,
    driver: Arc<dyn LoopDriverContract>,
    route_service: Arc<dyn ProviderRouteServiceContract>,
}

impl SessionExecutionBinding {
    fn new(
        generation: ExecutionBindingGeneration,
        driver: Arc<dyn LoopDriverContract>,
        route_service: Arc<dyn ProviderRouteServiceContract>,
    ) -> Self {
        Self {
            generation,
            driver,
            route_service,
        }
    }

    fn release_coupled() -> Self {
        Self::new(
            ExecutionBindingGeneration::new(
                BUILTIN_DRIVER_GENERATION,
                BUILTIN_ROUTE_SERVICE_GENERATION,
            )
            .expect("built-in execution binding generations are valid"),
            Arc::new(crate::loop_driver::ReleaseCoupledLoopDriver),
            Arc::new(crate::provider_route_service::ProviderRouteService),
        )
    }

    pub(crate) fn generation(&self) -> &ExecutionBindingGeneration {
        &self.generation
    }

    pub(crate) fn capture(&self) -> Self {
        self.clone()
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn execute(
        &self,
        bridge: &dyn LlmBridge,
        bus: &mut crate::bus::EventBus,
        context: &mut ContextManager,
        conversation: &mut ConversationState,
        events: &broadcast::Sender<omegon_traits::AgentEvent>,
        cancellation: CancellationToken,
        config: &LoopConfig,
    ) -> LoopDriverExecution {
        let turn = LoopDriverTurn::new(
            bridge,
            bus,
            context,
            conversation,
            events,
            cancellation,
            config,
            self.route_service.clone(),
        );
        self.driver.run(turn).await
    }

    pub(crate) async fn resolve_provider_route(
        &self,
        model_spec: &str,
        secrets: Option<&omegon_secrets::SecretsManager>,
    ) -> Option<crate::provider_route_service::ResolvedProviderRoute> {
        self.route_service.resolve(model_spec, secrets).await
    }

    pub(crate) async fn resolve_exact_admitted_provider_route(
        &self,
        model_spec: &str,
        secrets: Option<&omegon_secrets::SecretsManager>,
        inventory: &crate::inference_inventory::InventorySnapshot,
        required_capabilities: &[String],
    ) -> Option<crate::provider_route_service::ResolvedProviderRoute> {
        self.route_service
            .resolve_exact_admitted(model_spec, secrets, inventory, required_capabilities)
            .await
    }

    pub(crate) async fn compact(
        &self,
        bridge: &dyn LlmBridge,
        payload: &str,
        options: &crate::bridge::StreamOptions,
    ) -> anyhow::Result<String> {
        let selected_model = bridge
            .selected_model_hint()
            .or(options.model.as_deref())
            .ok_or_else(|| anyhow::anyhow!("compaction route has no selected model identity"))?;
        self.route_service
            .compact(
                bridge,
                crate::provider_route_service::LoopCompactionRequest {
                    payload,
                    options,
                    selected_model,
                    scope: &crate::invocation_service::InvocationScope::default(),
                    step_id: uuid::Uuid::new_v4(),
                    authority: None,
                },
            )
            .await
    }

    pub(crate) async fn compact_scoped(
        &self,
        bridge: &dyn LlmBridge,
        payload: &str,
        options: &crate::bridge::StreamOptions,
        scope: &crate::invocation_service::InvocationScope,
        authority: &dyn crate::loop_driver::LoopCompactionAuthority,
    ) -> anyhow::Result<String> {
        let selected_model = bridge
            .selected_model_hint()
            .or(options.model.as_deref())
            .ok_or_else(|| anyhow::anyhow!("compaction route has no selected model identity"))?;
        self.route_service
            .compact(
                bridge,
                crate::provider_route_service::LoopCompactionRequest {
                    payload,
                    options,
                    selected_model,
                    scope,
                    step_id: uuid::Uuid::new_v4(),
                    authority: Some(authority),
                },
            )
            .await
    }

    pub(crate) fn record_ephemeral_route_lease(
        &self,
        selected_model: &str,
        serving_model: &str,
        credential_source_class: Option<&str>,
    ) -> anyhow::Result<()> {
        crate::provider_route_service::record_loop_route_lease(
            &crate::invocation_service::InvocationScope::default(),
            uuid::Uuid::new_v4(),
            selected_model,
            serving_model,
            credential_source_class,
        )
    }

    #[cfg(test)]
    pub(crate) fn route_service(&self) -> Arc<dyn ProviderRouteServiceContract> {
        self.route_service.clone()
    }

    #[cfg(test)]
    fn for_test(
        generation: ExecutionBindingGeneration,
        driver: Arc<dyn LoopDriverContract>,
        route_service: Arc<dyn ProviderRouteServiceContract>,
    ) -> Self {
        Self::new(generation, driver, route_service)
    }

    #[cfg(test)]
    pub(crate) fn release_coupled_for_test(generation: ExecutionBindingGeneration) -> Self {
        Self::new(
            generation,
            boot_execution_binding().driver.clone(),
            boot_execution_binding().route_service.clone(),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionExecutionReplacementRejection {
    NoPendingReplacement,
    StaleExpectedGeneration,
    ActiveTurn,
    UnresolvedInvocation,
    UnchangedTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionExecutionReplacementOutcome {
    Applied,
    Pending,
    Rejected(SessionExecutionReplacementRejection),
    NoAuthority,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum SessionExecutionOwnerError {
    #[error("session execution boot binding was rejected")]
    BootBinding(#[source] AuthorityError),
    #[error("session execution authority operation failed")]
    Authority(#[source] AuthorityError),
}

#[derive(Clone)]
pub(crate) struct SessionExecutionCapture {
    binding: Arc<SessionExecutionBinding>,
}

impl std::fmt::Debug for SessionExecutionCapture {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SessionExecutionCapture")
            .field("generation", &self.generation())
            .finish()
    }
}

impl SessionExecutionCapture {
    #[cfg(test)]
    pub(crate) fn binding(&self) -> &SessionExecutionBinding {
        &self.binding
    }

    pub(crate) fn generation(&self) -> &ExecutionBindingGeneration {
        self.binding.generation()
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn execute(
        &self,
        bridge: &dyn LlmBridge,
        bus: &mut crate::bus::EventBus,
        context: &mut ContextManager,
        conversation: &mut ConversationState,
        events: &broadcast::Sender<omegon_traits::AgentEvent>,
        cancellation: CancellationToken,
        config: &LoopConfig,
    ) -> LoopDriverExecution {
        self.binding
            .execute(
                bridge,
                bus,
                context,
                conversation,
                events,
                cancellation,
                config,
            )
            .await
    }
}

pub(crate) struct SessionExecutionTurnStart {
    pub(crate) started: bool,
    pub(crate) capture: SessionExecutionCapture,
}

struct PendingExecutionReplacement {
    expected_generation: ExecutionBindingGeneration,
    target: Arc<SessionExecutionBinding>,
}

struct SessionExecutionOwnerState {
    current: Arc<SessionExecutionBinding>,
    pending: Option<PendingExecutionReplacement>,
}

#[derive(Clone)]
pub(crate) struct SessionExecutionOwner {
    authority: Option<SessionAuthorityHandle>,
    coordination: Arc<Mutex<SessionExecutionOwnerState>>,
}

impl std::fmt::Debug for SessionExecutionOwner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SessionExecutionOwner")
            .field("generation", &self.capture().generation())
            .field("authority_backed", &self.authority.is_some())
            .finish()
    }
}

impl SessionExecutionOwner {
    pub(crate) fn immutable_at_boot() -> Self {
        Self::new(boot_execution_binding().capture(), None)
            .expect("sessionless boot execution binding cannot be rejected")
    }

    pub(crate) fn new(
        boot_binding: SessionExecutionBinding,
        authority: Option<SessionAuthorityHandle>,
    ) -> Result<Self, SessionExecutionOwnerError> {
        if let Some(authority) = &authority {
            authority
                .bind_execution_at_boot(boot_binding.generation().clone())
                .map_err(SessionExecutionOwnerError::BootBinding)?;
        }
        Ok(Self {
            authority,
            coordination: Arc::new(Mutex::new(SessionExecutionOwnerState {
                current: Arc::new(boot_binding),
                pending: None,
            })),
        })
    }

    fn lock(&self) -> MutexGuard<'_, SessionExecutionOwnerState> {
        self.coordination
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(crate) fn capture(&self) -> SessionExecutionCapture {
        let state = self.lock();
        SessionExecutionCapture {
            binding: state.current.clone(),
        }
    }

    pub(crate) fn has_pending_replacement(&self) -> bool {
        self.lock().pending.is_some()
    }

    pub(crate) fn start_turn_and_capture(
        &self,
        command_id: uuid::Uuid,
        recorded_at: &str,
        turn_id: uuid::Uuid,
        prompt_id: uuid::Uuid,
    ) -> Result<SessionExecutionTurnStart, SessionExecutionOwnerError> {
        let state = self.lock();
        let started = match &self.authority {
            Some(authority) => authority
                .start_turn(command_id, recorded_at, turn_id, prompt_id)
                .map_err(SessionExecutionOwnerError::Authority)?,
            None => false,
        };
        Ok(SessionExecutionTurnStart {
            started,
            capture: SessionExecutionCapture {
                binding: state.current.clone(),
            },
        })
    }

    pub(crate) fn request_replacement(
        &self,
        command_id: uuid::Uuid,
        recorded_at: &str,
        candidate: SessionExecutionBinding,
    ) -> Result<SessionExecutionReplacementOutcome, SessionExecutionOwnerError> {
        let Some(authority) = &self.authority else {
            return Ok(SessionExecutionReplacementOutcome::NoAuthority);
        };
        let mut state = self.lock();
        let expected_generation = state.current.generation().clone();
        if candidate.generation() == &expected_generation {
            return Ok(SessionExecutionReplacementOutcome::Rejected(
                SessionExecutionReplacementRejection::UnchangedTarget,
            ));
        }
        if authority.state().active_turn.is_some() {
            state.pending = Some(PendingExecutionReplacement {
                expected_generation,
                target: Arc::new(candidate),
            });
            return Ok(SessionExecutionReplacementOutcome::Pending);
        }
        let target = Arc::new(candidate);
        let outcome = Self::migrate_and_publish(
            authority,
            &mut state,
            command_id,
            recorded_at,
            expected_generation.clone(),
            target.clone(),
        )?;
        if outcome
            == SessionExecutionReplacementOutcome::Rejected(
                SessionExecutionReplacementRejection::ActiveTurn,
            )
        {
            state.pending = Some(PendingExecutionReplacement {
                expected_generation,
                target,
            });
            return Ok(SessionExecutionReplacementOutcome::Pending);
        }
        Ok(outcome)
    }

    pub(crate) fn commit_pending_at_quiescence(
        &self,
        command_id: uuid::Uuid,
        recorded_at: &str,
    ) -> Result<SessionExecutionReplacementOutcome, SessionExecutionOwnerError> {
        let Some(authority) = &self.authority else {
            return Ok(SessionExecutionReplacementOutcome::NoAuthority);
        };
        let mut state = self.lock();
        let Some(pending) = state.pending.as_ref() else {
            return Ok(SessionExecutionReplacementOutcome::Rejected(
                SessionExecutionReplacementRejection::NoPendingReplacement,
            ));
        };
        if pending.expected_generation != *state.current.generation() {
            return Ok(SessionExecutionReplacementOutcome::Rejected(
                SessionExecutionReplacementRejection::StaleExpectedGeneration,
            ));
        }
        let expected_generation = pending.expected_generation.clone();
        let target = pending.target.clone();
        let outcome = Self::migrate_and_publish(
            authority,
            &mut state,
            command_id,
            recorded_at,
            expected_generation,
            target,
        )?;
        if outcome == SessionExecutionReplacementOutcome::Applied {
            state.pending = None;
        }
        Ok(outcome)
    }

    fn migrate_and_publish(
        authority: &SessionAuthorityHandle,
        state: &mut SessionExecutionOwnerState,
        command_id: uuid::Uuid,
        recorded_at: &str,
        expected_generation: ExecutionBindingGeneration,
        target: Arc<SessionExecutionBinding>,
    ) -> Result<SessionExecutionReplacementOutcome, SessionExecutionOwnerError> {
        match authority.migrate_execution_binding_typed(
            command_id,
            recorded_at,
            expected_generation,
            target.generation().clone(),
        ) {
            Ok(_) => {
                state.current = target;
                Ok(SessionExecutionReplacementOutcome::Applied)
            }
            Err(ExecutionBindingMigrationError::Rejected(rejection)) => Ok(
                SessionExecutionReplacementOutcome::Rejected(match rejection {
                    ExecutionBindingMigrationRejection::NoProcessLocalBinding
                    | ExecutionBindingMigrationRejection::StaleSource => {
                        SessionExecutionReplacementRejection::StaleExpectedGeneration
                    }
                    ExecutionBindingMigrationRejection::ActiveTurn => {
                        SessionExecutionReplacementRejection::ActiveTurn
                    }
                    ExecutionBindingMigrationRejection::UnresolvedInvocation => {
                        SessionExecutionReplacementRejection::UnresolvedInvocation
                    }
                    ExecutionBindingMigrationRejection::UnchangedTarget => {
                        SessionExecutionReplacementRejection::UnchangedTarget
                    }
                }),
            ),
            Err(ExecutionBindingMigrationError::Authority(error)) => {
                Err(SessionExecutionOwnerError::Authority(error))
            }
        }
    }
}

pub(crate) fn boot_execution_binding() -> &'static SessionExecutionBinding {
    static BINDING: OnceLock<SessionExecutionBinding> = OnceLock::new();
    BINDING.get_or_init(SessionExecutionBinding::release_coupled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Barrier, Mutex};

    use crate::provider_route_service::{
        LoopCompactionRequest, LoopRoute, LoopRouteFailure, LoopRoutePolicy, LoopRouteRequest,
        ProviderStopNotice,
    };
    use crate::session_authority::{
        ActorIdentity, InvocationClassifiedUnknown, InvocationDispatched, InvocationPrepared,
        PromptAdmitted, PromptContent, QueueMode, SessionAuthority, TurnClosed, TurnOutcome,
    };
    use omegon_traits::{
        RuntimeCapabilityId, RuntimeCapabilityTransitionPolicy, RuntimeCompositionGenerationId,
        RuntimeContributionGenerationId, RuntimeContributionId, RuntimeEffect,
        RuntimeExecutionPolicy, RuntimeInvocationKind, RuntimePrincipalClass, RuntimeSurface,
    };

    const NOW: &str = "2026-08-21T12:00:00Z";

    struct FakeDriver {
        generation: &'static str,
        expected_route: &'static str,
        executions: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait::async_trait]
    impl LoopDriverContract for FakeDriver {
        async fn run(&self, turn: LoopDriverTurn<'_>) -> LoopDriverExecution {
            let route = turn.route().startup_route().await;
            assert_eq!(route.provider_id, self.expected_route);
            self.executions.lock().unwrap().push(format!(
                "{}:{}:{}",
                self.generation, route.provider_id, route.selected_model
            ));
            LoopDriverExecution {
                result: Ok(()),
                terminal: crate::loop_driver::LoopTerminalProposal {
                    outcome: crate::runtime_turn::RuntimeTurnOutcome::Completed,
                    reason_code: "fake_completed",
                },
            }
        }
    }

    struct FakeRouteService {
        generation: &'static str,
        policies: Arc<Mutex<Vec<String>>>,
    }

    struct HintBridge(&'static str);

    #[async_trait::async_trait]
    impl LlmBridge for HintBridge {
        async fn stream(
            &self,
            _system_prompt: &str,
            _messages: &[crate::bridge::LlmMessage],
            _tools: &[omegon_traits::ToolDefinition],
            _options: &crate::bridge::StreamOptions,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<crate::bridge::LlmEvent>> {
            unreachable!("the fake driver never dispatches")
        }

        fn serving_model_hint(&self) -> Option<&str> {
            Some(self.0)
        }
    }

    #[async_trait::async_trait]
    impl ProviderRouteServiceContract for FakeRouteService {
        async fn startup_route(
            &self,
            _bridge: &dyn LlmBridge,
            _setup: &crate::loop_driver::LoopRouteSetup,
            policy: &LoopRoutePolicy,
        ) -> LoopRoute {
            self.policies
                .lock()
                .unwrap()
                .push(policy.selected_model.clone());
            LoopRoute {
                selected_model: policy.selected_model.clone(),
                serving_model: format!("{}:{}", self.generation, policy.selected_model),
                provider_id: self.generation.into(),
                schema_dialect: "full".into(),
                contribution_generation_id: format!("provider-route:{}", self.generation),
                normalizer_contribution_id: omegon_traits::RuntimeContributionId::new(
                    "provider:test",
                )
                .unwrap(),
                normalizer_generation_id: omegon_traits::RuntimeContributionGenerationId::new(
                    format!("provider-route:{}", self.generation),
                )
                .unwrap(),
                options: Default::default(),
            }
        }

        async fn turn_route(
            &self,
            bridge: &dyn LlmBridge,
            setup: &crate::loop_driver::LoopRouteSetup,
            policy: &LoopRoutePolicy,
            _base: &crate::bridge::StreamOptions,
        ) -> LoopRoute {
            self.startup_route(bridge, setup, policy).await
        }

        async fn prepare(
            &self,
            _route: &LoopRoute,
            _setup: &crate::loop_driver::LoopRouteSetup,
            _events: &broadcast::Sender<omegon_traits::AgentEvent>,
        ) {
        }

        async fn dispatch(
            &self,
            _bridge: &dyn LlmBridge,
            _request: LoopRouteRequest<'_>,
        ) -> anyhow::Result<crate::provider_route_service::LoopRouteDispatch> {
            unreachable!("the fake driver only selects a route")
        }

        async fn compact(
            &self,
            _bridge: &dyn LlmBridge,
            _request: LoopCompactionRequest<'_>,
        ) -> anyhow::Result<String> {
            unreachable!("the fake driver does not compact")
        }

        fn stop_notice(
            &self,
            _route: &LoopRoute,
            _raw: &serde_json::Value,
        ) -> Option<ProviderStopNotice> {
            None
        }

        fn failure_kind(&self, _error: &anyhow::Error) -> LoopRouteFailure {
            LoopRouteFailure::Other
        }

        fn provider_id(&self, _model: &str) -> String {
            self.generation.into()
        }

        fn canonical_model_spec(&self, model: &str) -> String {
            model.into()
        }
    }

    fn generation(suffix: &str) -> ExecutionBindingGeneration {
        ExecutionBindingGeneration::new(
            format!("loop-driver:{suffix}"),
            format!("provider-route-service:{suffix}"),
        )
        .unwrap()
    }

    fn fake_binding(
        suffix: &'static str,
        executions: Arc<Mutex<Vec<String>>>,
        policies: Arc<Mutex<Vec<String>>>,
    ) -> SessionExecutionBinding {
        SessionExecutionBinding::for_test(
            generation(suffix),
            Arc::new(FakeDriver {
                generation: suffix,
                expected_route: suffix,
                executions,
            }),
            Arc::new(FakeRouteService {
                generation: suffix,
                policies,
            }),
        )
    }

    async fn execute_with_bridge(
        binding: &SessionExecutionBinding,
        model: &str,
        bridge: &dyn LlmBridge,
    ) {
        let mut bus = crate::bus::EventBus::new();
        let mut context = ContextManager::new(String::new(), vec![]);
        let mut conversation = ConversationState::new();
        let (events, _) = broadcast::channel(8);
        let config = LoopConfig {
            model: model.into(),
            ..Default::default()
        };
        binding
            .execute(
                bridge,
                &mut bus,
                &mut context,
                &mut conversation,
                &events,
                CancellationToken::new(),
                &config,
            )
            .await
            .result
            .unwrap();
    }

    async fn execute(binding: &SessionExecutionBinding, model: &str) {
        execute_with_bridge(binding, model, &crate::bridge::NullBridge).await;
    }

    fn authority() -> (tempfile::TempDir, SessionAuthorityHandle) {
        let directory = tempfile::tempdir().unwrap();
        let authority = SessionAuthority::open(
            &directory.path().join("session.json"),
            "session-execution-owner",
            "workspace",
            "composition:test",
            ActorIdentity {
                principal: "operator".into(),
                ingress: "test".into(),
            },
            NOW,
        )
        .unwrap();
        (directory, SessionAuthorityHandle::new(authority))
    }

    fn admit_prompt(authority: &SessionAuthorityHandle) -> uuid::Uuid {
        let prompt_id = uuid::Uuid::new_v4();
        authority
            .admit_prompt(
                uuid::Uuid::new_v4(),
                NOW,
                PromptAdmitted {
                    submission_id: uuid::Uuid::new_v4(),
                    prompt_id,
                    principal: "operator".into(),
                    ingress: "test".into(),
                    queue_mode: QueueMode::UntilReady,
                    content: PromptContent {
                        text: "test".into(),
                        attachments: Vec::new(),
                    },
                    metadata: serde_json::json!({}),
                },
            )
            .unwrap();
        prompt_id
    }

    fn start(owner: &SessionExecutionOwner, prompt_id: uuid::Uuid) -> uuid::Uuid {
        let turn_id = uuid::Uuid::new_v4();
        let started = owner
            .start_turn_and_capture(uuid::Uuid::new_v4(), NOW, turn_id, prompt_id)
            .unwrap();
        assert!(started.started);
        turn_id
    }

    fn close(authority: &SessionAuthorityHandle, turn_id: uuid::Uuid) {
        authority
            .close_turn(
                uuid::Uuid::new_v4(),
                NOW,
                TurnClosed {
                    turn_id,
                    outcome: TurnOutcome::Completed,
                    reason_code: "test_complete".into(),
                    recovery_rule_version: None,
                },
            )
            .unwrap();
    }

    fn durable_unknown(turn_id: uuid::Uuid) -> InvocationPrepared {
        InvocationPrepared {
            invocation_id: uuid::Uuid::new_v4(),
            lease_id: uuid::Uuid::new_v4(),
            turn_id,
            call_id: "unknown-call".into(),
            deduplication_id: Some("unknown-call".into()),
            invocation_kind: RuntimeInvocationKind::Tool,
            invocation_name: "write".into(),
            capability_id: RuntimeCapabilityId::new("tool:write").unwrap(),
            contribution_id: RuntimeContributionId::new("feature:writer").unwrap(),
            owner_generation_id: RuntimeContributionGenerationId::new("contribution:writer-v1")
                .unwrap(),
            issue_generation_id: RuntimeCompositionGenerationId::new("composition:test").unwrap(),
            principal: "model".into(),
            principal_class: RuntimePrincipalClass::Model,
            surface: RuntimeSurface::Model,
            admitted_effects: vec![RuntimeEffect::FilesystemWrite],
            execution: RuntimeExecutionPolicy {
                principals: vec![RuntimePrincipalClass::Model],
                timeout_class: omegon_traits::RuntimeTimeoutClass::Interactive,
                retry_class: omegon_traits::RuntimeRetryClass::Never,
                idempotency: omegon_traits::RuntimeIdempotency::NonIdempotent,
                deduplication: omegon_traits::RuntimeDeduplication::Unsupported,
                parallelism: omegon_traits::RuntimeParallelism::Serial,
                transaction: omegon_traits::RuntimeTransactionBehavior::None,
                mutation_fence: None,
                max_attempts: Some(1),
            },
            transition: RuntimeCapabilityTransitionPolicy {
                authority_narrowing: omegon_traits::RuntimeAuthorityNarrowing::CompleteExisting,
                active_call_timeout_ms: 30_000,
            },
            surfaces: vec![RuntimeSurface::Model],
        }
    }

    #[tokio::test]
    async fn captured_binding_executes_its_atomic_driver_and_route_service_pair() {
        let executions = Arc::new(Mutex::new(Vec::new()));
        let policies = Arc::new(Mutex::new(Vec::new()));
        let first = fake_binding("first", executions.clone(), policies.clone()).capture();
        let second = fake_binding("second", executions.clone(), policies.clone()).capture();

        execute(&first, "model-a").await;
        execute(&second, "model-b").await;

        assert_eq!(
            executions.lock().unwrap().as_slice(),
            ["first:first:model-a", "second:second:model-b"]
        );
        assert_eq!(
            first.generation(),
            &generation("first"),
            "capture retains exact generation identity"
        );
        assert_eq!(second.generation(), &generation("second"));
        assert_eq!(policies.lock().unwrap().as_slice(), ["model-a", "model-b"]);
    }

    #[tokio::test]
    async fn active_turn_retains_atomic_a_until_explicit_idle_commit_then_captures_b() {
        let executions = Arc::new(Mutex::new(Vec::new()));
        let policies = Arc::new(Mutex::new(Vec::new()));
        let (_directory, authority) = authority();
        let owner = SessionExecutionOwner::new(
            fake_binding("a", executions.clone(), policies.clone()),
            Some(authority.clone()),
        )
        .unwrap();
        let turn_id = start(&owner, admit_prompt(&authority));

        assert_eq!(
            owner
                .request_replacement(
                    uuid::Uuid::new_v4(),
                    NOW,
                    fake_binding("b", executions.clone(), policies.clone()),
                )
                .unwrap(),
            SessionExecutionReplacementOutcome::Pending
        );
        let active_capture = owner.capture();
        execute(active_capture.binding(), "during-a").await;
        close(&authority, turn_id);

        let still_a = owner.capture();
        assert_eq!(still_a.generation(), &generation("a"));
        execute(still_a.binding(), "after-close-before-commit").await;
        assert_eq!(
            owner
                .commit_pending_at_quiescence(uuid::Uuid::new_v4(), NOW)
                .unwrap(),
            SessionExecutionReplacementOutcome::Applied
        );
        let next = owner.capture();
        assert_eq!(next.generation(), &generation("b"));
        execute(next.binding(), "after-commit").await;
        assert_eq!(
            executions.lock().unwrap().as_slice(),
            [
                "a:a:during-a",
                "a:a:after-close-before-commit",
                "b:b:after-commit"
            ]
        );
    }

    #[test]
    fn concurrent_owner_start_and_commit_have_exactly_one_gate_winner() {
        for _ in 0..32 {
            let executions = Arc::new(Mutex::new(Vec::new()));
            let policies = Arc::new(Mutex::new(Vec::new()));
            let (_directory, authority) = authority();
            let owner = SessionExecutionOwner::new(
                fake_binding("a", executions.clone(), policies.clone()),
                Some(authority.clone()),
            )
            .unwrap();
            let first_turn = start(&owner, admit_prompt(&authority));
            assert_eq!(
                owner
                    .request_replacement(
                        uuid::Uuid::new_v4(),
                        NOW,
                        fake_binding("b", executions, policies),
                    )
                    .unwrap(),
                SessionExecutionReplacementOutcome::Pending
            );
            close(&authority, first_turn);
            let next_prompt = admit_prompt(&authority);
            let barrier = Arc::new(Barrier::new(3));
            let start_owner = owner.clone();
            let start_barrier = barrier.clone();
            let starter = std::thread::spawn(move || {
                start_barrier.wait();
                start_owner
                    .start_turn_and_capture(
                        uuid::Uuid::new_v4(),
                        NOW,
                        uuid::Uuid::new_v4(),
                        next_prompt,
                    )
                    .unwrap()
                    .capture
                    .generation()
                    .clone()
            });
            let commit_owner = owner.clone();
            let commit_barrier = barrier.clone();
            let committer = std::thread::spawn(move || {
                commit_barrier.wait();
                commit_owner
                    .commit_pending_at_quiescence(uuid::Uuid::new_v4(), NOW)
                    .unwrap()
            });
            barrier.wait();
            let captured = starter.join().unwrap();
            let committed = committer.join().unwrap();

            match committed {
                SessionExecutionReplacementOutcome::Applied => {
                    assert_eq!(captured, generation("b"));
                }
                SessionExecutionReplacementOutcome::Rejected(
                    SessionExecutionReplacementRejection::ActiveTurn,
                ) => assert_eq!(captured, generation("a")),
                other => panic!("unexpected commit outcome: {other:?}"),
            }
        }
    }

    #[test]
    fn durable_append_failure_does_not_publish_pending_binding() {
        let executions = Arc::new(Mutex::new(Vec::new()));
        let policies = Arc::new(Mutex::new(Vec::new()));
        let (_directory, authority) = authority();
        let owner = SessionExecutionOwner::new(
            fake_binding("a", executions.clone(), policies.clone()),
            Some(authority.clone()),
        )
        .unwrap();
        let turn_id = start(&owner, admit_prompt(&authority));
        owner
            .request_replacement(
                uuid::Uuid::new_v4(),
                NOW,
                fake_binding("b", executions, policies),
            )
            .unwrap();
        close(&authority, turn_id);
        authority.make_next_append_fail();

        assert!(
            owner
                .commit_pending_at_quiescence(uuid::Uuid::new_v4(), NOW)
                .is_err()
        );
        assert_eq!(owner.capture().generation(), &generation("a"));
    }

    #[test]
    fn stale_pending_generation_cannot_overwrite_newer_authority_migration() {
        let executions = Arc::new(Mutex::new(Vec::new()));
        let policies = Arc::new(Mutex::new(Vec::new()));
        let (_directory, authority) = authority();
        let owner = SessionExecutionOwner::new(
            fake_binding("a", executions.clone(), policies.clone()),
            Some(authority.clone()),
        )
        .unwrap();
        let turn_id = start(&owner, admit_prompt(&authority));
        owner
            .request_replacement(
                uuid::Uuid::new_v4(),
                NOW,
                fake_binding("b", executions, policies),
            )
            .unwrap();
        close(&authority, turn_id);
        authority
            .migrate_execution_binding(uuid::Uuid::new_v4(), NOW, generation("a"), generation("c"))
            .unwrap();

        assert_eq!(
            owner
                .commit_pending_at_quiescence(uuid::Uuid::new_v4(), NOW)
                .unwrap(),
            SessionExecutionReplacementOutcome::Rejected(
                SessionExecutionReplacementRejection::StaleExpectedGeneration
            )
        );
        assert_eq!(
            authority.state().execution_binding_generation,
            Some(generation("c"))
        );
        assert_eq!(owner.capture().generation(), &generation("a"));
    }

    #[test]
    fn durable_unknown_invocation_blocks_pending_commit() {
        let executions = Arc::new(Mutex::new(Vec::new()));
        let policies = Arc::new(Mutex::new(Vec::new()));
        let (_directory, authority) = authority();
        let owner = SessionExecutionOwner::new(
            fake_binding("a", executions.clone(), policies.clone()),
            Some(authority.clone()),
        )
        .unwrap();
        let turn_id = start(&owner, admit_prompt(&authority));
        owner
            .request_replacement(
                uuid::Uuid::new_v4(),
                NOW,
                fake_binding("b", executions, policies),
            )
            .unwrap();
        let preparation = durable_unknown(turn_id);
        authority
            .prepare_invocation(NOW, preparation.clone())
            .unwrap();
        authority
            .mark_invocation_dispatched(
                NOW,
                InvocationDispatched {
                    invocation_id: preparation.invocation_id,
                    lease_id: preparation.lease_id,
                },
            )
            .unwrap();
        authority
            .classify_invocation_unknown(
                NOW,
                InvocationClassifiedUnknown {
                    invocation_id: preparation.invocation_id,
                    reason_code: "runtime_lost".into(),
                    recovery_rule_version: 2,
                },
            )
            .unwrap();
        close(&authority, turn_id);

        assert_eq!(
            owner
                .commit_pending_at_quiescence(uuid::Uuid::new_v4(), NOW)
                .unwrap(),
            SessionExecutionReplacementOutcome::Rejected(
                SessionExecutionReplacementRejection::UnresolvedInvocation
            )
        );
        assert_eq!(owner.capture().generation(), &generation("a"));
    }

    #[test]
    fn no_authority_owner_is_an_immutable_boot_binding() {
        let executions = Arc::new(Mutex::new(Vec::new()));
        let policies = Arc::new(Mutex::new(Vec::new()));
        let owner = SessionExecutionOwner::new(
            fake_binding("a", executions.clone(), policies.clone()),
            None,
        )
        .unwrap();

        assert_eq!(
            owner
                .request_replacement(
                    uuid::Uuid::new_v4(),
                    NOW,
                    fake_binding("b", executions, policies),
                )
                .unwrap(),
            SessionExecutionReplacementOutcome::NoAuthority
        );
        assert_eq!(
            owner
                .commit_pending_at_quiescence(uuid::Uuid::new_v4(), NOW)
                .unwrap(),
            SessionExecutionReplacementOutcome::NoAuthority
        );
        assert_eq!(owner.capture().generation(), &generation("a"));
    }

    #[tokio::test]
    async fn model_and_bridge_inputs_do_not_change_the_service_generation() {
        let executions = Arc::new(Mutex::new(Vec::new()));
        let policies = Arc::new(Mutex::new(Vec::new()));
        let binding = fake_binding("stable", executions, policies.clone());
        let generation = binding.generation().clone();

        execute_with_bridge(&binding, "model-a", &crate::bridge::NullBridge).await;
        execute_with_bridge(
            &binding,
            "other-provider:model-b",
            &HintBridge("changed-bridge:model-b"),
        )
        .await;

        assert_eq!(binding.generation(), &generation);
        assert_eq!(
            generation.provider_route_service_generation_id.as_str(),
            "provider-route-service:stable"
        );
        assert_eq!(
            policies.lock().unwrap().as_slice(),
            ["model-a", "other-provider:model-b"]
        );
    }

    #[test]
    fn boot_binding_is_release_coupled_and_stable() {
        let first = boot_execution_binding();
        let second = boot_execution_binding();

        assert!(std::ptr::eq(first, second));
        assert_eq!(
            first.generation().driver_generation_id.as_str(),
            BUILTIN_DRIVER_GENERATION
        );
        assert_eq!(
            first
                .generation()
                .provider_route_service_generation_id
                .as_str(),
            BUILTIN_ROUTE_SERVICE_GENERATION
        );
    }

    #[test]
    fn loop_route_port_delegates_selected_service_policy() {
        let source = include_str!("loop_driver.rs");
        let port = source
            .split("impl LoopRouteContract for LoopRoutePort")
            .nth(1)
            .and_then(|source| source.split("impl LoopRoutePort").next())
            .expect("loop route port implementation");

        for forbidden in [
            "provider_route_service::loop_startup_route",
            "provider_route_service::loop_turn_route",
            "provider_route_service::prepare_loop_route",
            "provider_route_service::dispatch_loop_route",
            "provider_route_service::compact_loop_route",
            "provider_route_service::provider_stop_notice",
            "provider_route_service::classify_loop_route_failure",
            "providers::infer_provider_id",
        ] {
            assert!(
                !port.contains(forbidden),
                "LoopRoutePort hardwires provider policy through {forbidden}"
            );
        }
    }
}
