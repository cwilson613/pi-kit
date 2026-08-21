//! Kernel-owned privileged invocation admission and ephemeral execution leases.
//!
//! Slice 3.1 centralizes accepted-graph resolution, RBAC, permission policy,
//! approval, and generation binding. Durable Prepared/Dispatched state begins in
//! Slice 3.3; these leases deliberately make no crash-consistency claim.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use omegon_traits::{
    RuntimeCapabilityId, RuntimeCapabilityTransitionPolicy, RuntimeCompositionGenerationId,
    RuntimeContributionGenerationId, RuntimeContributionId, RuntimeDeduplication, RuntimeEffect,
    RuntimeExecutionPolicy, RuntimeInvocationKind, RuntimePrincipalClass, RuntimeSurface,
    RuntimeTimeoutClass,
};
use serde_json::Value;
use uuid::Uuid;

use crate::permissions::{
    LayeredPermissionPolicy, PermissionAction, PermissionLayer, subjects_from_tool_args,
};

#[derive(Debug, thiserror::Error)]
#[error("unknown invocation completion: {reason}")]
pub(crate) struct UnknownCompletionError {
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InvocationScope {
    pub principal: String,
    pub principal_class: RuntimePrincipalClass,
    pub surface: RuntimeSurface,
    pub session_id: Option<String>,
    pub turn_id: Option<Uuid>,
    pub authority: Option<crate::session_authority::SessionAuthorityHandle>,
}

impl Default for InvocationScope {
    fn default() -> Self {
        Self {
            principal: "model".into(),
            principal_class: RuntimePrincipalClass::Model,
            surface: RuntimeSurface::Model,
            session_id: None,
            turn_id: None,
            authority: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedInvocation {
    pub kind: RuntimeInvocationKind,
    pub name: String,
    pub capability_id: RuntimeCapabilityId,
    pub contribution_id: RuntimeContributionId,
    pub owner_generation_id: RuntimeContributionGenerationId,
    pub composition_generation_id: RuntimeCompositionGenerationId,
    pub effects: Vec<RuntimeEffect>,
    pub execution: RuntimeExecutionPolicy,
    pub transition: RuntimeCapabilityTransitionPolicy,
    pub surfaces: Vec<RuntimeSurface>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InvocationDenialCode {
    UnknownInvocation,
    IncompleteDeclaration,
    UnsupportedSurface,
    RbacDenied,
    PermissionPolicyDenied,
    ApprovalDenied,
    AuthorityUnavailable,
    StaleGeneration,
    LeaseClosed,
    LeaseMismatch,
}

impl InvocationDenialCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UnknownInvocation => "invocation:unknown",
            Self::IncompleteDeclaration => "invocation:incomplete_declaration",
            Self::UnsupportedSurface => "invocation:unsupported_surface",
            Self::RbacDenied => "invocation:rbac_denied",
            Self::PermissionPolicyDenied => "invocation:permission_policy_denied",
            Self::ApprovalDenied => "invocation:approval_denied",
            Self::AuthorityUnavailable => "invocation:authority_unavailable",
            Self::StaleGeneration => "invocation:stale_generation",
            Self::LeaseClosed => "invocation:lease_closed",
            Self::LeaseMismatch => "invocation:lease_mismatch",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InvocationDenial {
    pub code: InvocationDenialCode,
    pub message: String,
    pub policy_layer: Option<PermissionLayer>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum LeaseTerminal {
    Open = 0,
    Dispatching = 1,
    Completed = 2,
    Failed = 3,
    Revoked = 4,
}

#[derive(Debug)]
pub(crate) struct ExecutionLease {
    pub lease_id: Uuid,
    pub invocation_id: Uuid,
    pub call_id: String,
    pub deduplication_id: Option<String>,
    pub principal: String,
    pub principal_class: RuntimePrincipalClass,
    pub surface: RuntimeSurface,
    pub session_id: Option<String>,
    pub turn_id: Option<Uuid>,
    pub kind: RuntimeInvocationKind,
    pub invocation_name: String,
    pub capability_id: RuntimeCapabilityId,
    pub contribution_id: RuntimeContributionId,
    pub owner_generation_id: RuntimeContributionGenerationId,
    pub issue_generation_id: RuntimeCompositionGenerationId,
    pub admitted_effects: Vec<RuntimeEffect>,
    pub execution: RuntimeExecutionPolicy,
    pub transition: RuntimeCapabilityTransitionPolicy,
    pub surfaces: Vec<RuntimeSurface>,
    authority: Option<crate::session_authority::SessionAuthorityHandle>,
    acknowledged: Arc<std::sync::Mutex<bool>>,
    terminal: Arc<AtomicU8>,
}

impl ExecutionLease {
    fn issue(call_id: &str, scope: InvocationScope, resolved: ResolvedInvocation) -> Self {
        Self::issue_with_identity(
            Uuid::new_v4(),
            Uuid::new_v4(),
            None,
            call_id,
            scope,
            resolved,
        )
    }

    fn issue_with_identity(
        lease_id: Uuid,
        invocation_id: Uuid,
        deduplication_id: Option<String>,
        call_id: &str,
        scope: InvocationScope,
        resolved: ResolvedInvocation,
    ) -> Self {
        Self {
            lease_id,
            invocation_id,
            call_id: call_id.to_string(),
            deduplication_id,
            principal: scope.principal,
            principal_class: scope.principal_class,
            surface: scope.surface,
            session_id: scope.session_id,
            turn_id: scope.turn_id,
            kind: resolved.kind,
            invocation_name: resolved.name,
            capability_id: resolved.capability_id,
            contribution_id: resolved.contribution_id,
            owner_generation_id: resolved.owner_generation_id,
            issue_generation_id: resolved.composition_generation_id,
            admitted_effects: resolved.effects,
            execution: resolved.execution,
            transition: resolved.transition,
            surfaces: resolved.surfaces,
            authority: scope.authority,
            acknowledged: Arc::new(std::sync::Mutex::new(false)),
            terminal: Arc::new(AtomicU8::new(LeaseTerminal::Open as u8)),
        }
    }

    fn prepare_and_issue(
        call_id: &str,
        scope: InvocationScope,
        resolved: ResolvedInvocation,
    ) -> Result<Self, InvocationDenial> {
        let Some(authority) = scope.authority.clone() else {
            if scope.session_id.is_some() || scope.turn_id.is_some() {
                return Err(denial(
                    InvocationDenialCode::AuthorityUnavailable,
                    "durable invocation scope has no session authority writer",
                ));
            }
            return Ok(Self::issue(call_id, scope, resolved));
        };
        let session_id = scope.session_id.as_deref().ok_or_else(|| {
            denial(
                InvocationDenialCode::AuthorityUnavailable,
                "session authority writer requires a session identity",
            )
        })?;
        let turn_id = scope.turn_id.ok_or_else(|| {
            denial(
                InvocationDenialCode::AuthorityUnavailable,
                "session authority writer requires an active turn identity",
            )
        })?;
        if authority.session_id() != session_id {
            return Err(denial(
                InvocationDenialCode::AuthorityUnavailable,
                "invocation scope does not match the session authority writer",
            ));
        }

        let lease_id = Uuid::new_v4();
        let invocation_id = Uuid::new_v4();
        let deduplication_id = (resolved.execution.deduplication
            == RuntimeDeduplication::OwnerEnforcedStableCallId)
            .then(|| call_id.to_string());
        authority
            .prepare_invocation(
                &recorded_at_now(),
                crate::session_authority::InvocationPrepared {
                    invocation_id,
                    lease_id,
                    turn_id,
                    call_id: call_id.to_string(),
                    deduplication_id: deduplication_id.clone(),
                    invocation_kind: resolved.kind,
                    invocation_name: resolved.name.clone(),
                    capability_id: resolved.capability_id.clone(),
                    contribution_id: resolved.contribution_id.clone(),
                    owner_generation_id: resolved.owner_generation_id.clone(),
                    issue_generation_id: resolved.composition_generation_id.clone(),
                    principal: scope.principal.clone(),
                    principal_class: scope.principal_class,
                    surface: scope.surface,
                    admitted_effects: resolved.effects.clone(),
                    execution: resolved.execution.clone(),
                    transition: resolved.transition.clone(),
                    surfaces: resolved.surfaces.clone(),
                },
            )
            .map_err(|error| {
                denial(
                    InvocationDenialCode::AuthorityUnavailable,
                    format!("failed to persist invocation preparation: {error}"),
                )
            })?;
        Ok(Self::issue_with_identity(
            lease_id,
            invocation_id,
            deduplication_id,
            call_id,
            scope,
            resolved,
        ))
    }

    pub fn terminal(&self) -> LeaseTerminal {
        match self.terminal.load(Ordering::Acquire) {
            0 => LeaseTerminal::Open,
            1 => LeaseTerminal::Dispatching,
            2 => LeaseTerminal::Completed,
            3 => LeaseTerminal::Failed,
            _ => LeaseTerminal::Revoked,
        }
    }

    pub fn execution_timeout(&self, args: &Value) -> std::time::Duration {
        let declared_seconds = match self.execution.timeout_class {
            RuntimeTimeoutClass::Immediate => 30,
            RuntimeTimeoutClass::Interactive => 300,
            RuntimeTimeoutClass::Background => 900,
            RuntimeTimeoutClass::LongRunning => 21_600,
        };
        let requested_seconds = args
            .get("timeout_secs")
            .and_then(Value::as_u64)
            .or_else(|| args.get("timeout").and_then(Value::as_u64));
        std::time::Duration::from_secs(requested_seconds.map_or(declared_seconds, |requested| {
            requested.min(declared_seconds)
        }))
    }

    pub fn dispatch_metadata(&self) -> omegon_traits::InvocationDispatchMetadata {
        omegon_traits::InvocationDispatchMetadata {
            invocation_id: self.invocation_id.to_string(),
            visible_call_id: self.call_id.clone(),
            deduplication_id: self.deduplication_id.clone(),
            session_id: self.session_id.clone(),
            turn_id: self.turn_id.map(|turn_id| turn_id.to_string()),
        }
    }

    pub fn invocation_control(&self) -> omegon_traits::InvocationControl {
        let authority = self.authority.clone();
        let acknowledged = self.acknowledged.clone();
        let invocation_id = self.invocation_id;
        let lease_id = self.lease_id;
        omegon_traits::InvocationControl::new(move || {
            let mut acknowledged = acknowledged
                .lock()
                .map_err(|_| "invocation acknowledgement state is unavailable".to_string())?;
            if *acknowledged {
                return Ok(());
            }
            if let Some(authority) = &authority {
                authority
                    .acknowledge_invocation(
                        &recorded_at_now(),
                        crate::session_authority::InvocationAcknowledged {
                            invocation_id,
                            lease_id,
                        },
                    )
                    .map_err(|error| {
                        format!("failed to persist invocation acknowledgement: {error}")
                    })?;
            }
            *acknowledged = true;
            Ok(())
        })
    }

    pub fn persist_dispatched(&self) -> Result<(), InvocationDenial> {
        let Some(authority) = &self.authority else {
            return Ok(());
        };
        authority
            .mark_invocation_dispatched(
                &recorded_at_now(),
                crate::session_authority::InvocationDispatched {
                    invocation_id: self.invocation_id,
                    lease_id: self.lease_id,
                },
            )
            .map(|_| ())
            .map_err(|error| {
                self.revoke();
                denial(
                    InvocationDenialCode::AuthorityUnavailable,
                    format!("failed to persist invocation dispatch: {error}"),
                )
            })
    }

    pub fn persist_settlement(
        &self,
        outcome: crate::session_authority::InvocationOutcome,
    ) -> Result<(), InvocationDenial> {
        let Some(authority) = &self.authority else {
            return Ok(());
        };
        if !*self.acknowledged.lock().map_err(|_| {
            denial(
                InvocationDenialCode::AuthorityUnavailable,
                "invocation acknowledgement state is unavailable",
            )
        })? {
            return Err(denial(
                InvocationDenialCode::AuthorityUnavailable,
                "owner has not durably acknowledged the invocation",
            ));
        }
        authority
            .settle_invocation(
                &recorded_at_now(),
                crate::session_authority::InvocationSettled {
                    invocation_id: self.invocation_id,
                    outcome,
                    terminal_evidence_reference: None,
                },
            )
            .map(|_| ())
            .map_err(|error| {
                self.revoke();
                denial(
                    InvocationDenialCode::AuthorityUnavailable,
                    format!("failed to persist invocation settlement: {error}"),
                )
            })
    }

    pub fn persist_unknown(&self, reason_code: &str) -> Result<(), InvocationDenial> {
        let Some(authority) = &self.authority else {
            return Ok(());
        };
        authority
            .classify_invocation_unknown(
                &recorded_at_now(),
                crate::session_authority::InvocationClassifiedUnknown {
                    invocation_id: self.invocation_id,
                    reason_code: reason_code.into(),
                    recovery_rule_version: 2,
                },
            )
            .map(|_| ())
            .map_err(|error| {
                self.revoke();
                denial(
                    InvocationDenialCode::AuthorityUnavailable,
                    format!("failed to persist unknown completion: {error}"),
                )
            })
    }

    pub fn claim_dispatch(&self, call_id: &str, name: &str) -> Result<(), InvocationDenial> {
        if self.call_id != call_id || self.invocation_name != name {
            return Err(denial(
                InvocationDenialCode::LeaseMismatch,
                "execution lease does not match the requested call",
            ));
        }
        self.terminal
            .compare_exchange(
                LeaseTerminal::Open as u8,
                LeaseTerminal::Dispatching as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .map(|_| ())
            .map_err(|_| {
                denial(
                    InvocationDenialCode::LeaseClosed,
                    "execution lease is no longer open",
                )
            })
    }

    pub fn close(&self, terminal: LeaseTerminal) -> bool {
        debug_assert!(matches!(
            terminal,
            LeaseTerminal::Completed | LeaseTerminal::Failed | LeaseTerminal::Revoked
        ));
        self.terminal
            .compare_exchange(
                LeaseTerminal::Dispatching as u8,
                terminal as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    pub fn revoke(&self) -> bool {
        loop {
            let current = self.terminal.load(Ordering::Acquire);
            if current >= LeaseTerminal::Completed as u8 {
                return false;
            }
            if self
                .terminal
                .compare_exchange(
                    current,
                    LeaseTerminal::Revoked as u8,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
            {
                return true;
            }
        }
    }
}

impl Drop for ExecutionLease {
    fn drop(&mut self) {
        self.revoke();
    }
}

pub(crate) enum InvocationAdmission {
    Lease(ExecutionLease),
    ApprovalRequired(PendingInvocationApproval),
    Denied(InvocationDenial),
}

pub(crate) struct PendingInvocationApproval {
    pub requested: String,
    pub policy_layer: Option<PermissionLayer>,
    call_id: String,
    scope: InvocationScope,
    resolved: ResolvedInvocation,
}

impl PendingInvocationApproval {
    pub fn decide(self, approved: bool) -> Result<ExecutionLease, InvocationDenial> {
        if !approved {
            return Err(InvocationDenial {
                code: InvocationDenialCode::ApprovalDenied,
                message: "operator denied the permission-policy challenge".into(),
                policy_layer: self.policy_layer,
            });
        }
        ExecutionLease::prepare_and_issue(&self.call_id, self.scope, self.resolved)
    }
}

pub(crate) struct InvocationAdmissionRequest<'a> {
    pub call_id: &'a str,
    pub visible_tool_name: &'a str,
    pub args: &'a Value,
    pub scope: InvocationScope,
    pub permission_policy: Option<&'a LayeredPermissionPolicy>,
    pub permission_role: Option<styrene_rbac::Role>,
}

pub(crate) struct InvocationService;

impl InvocationService {
    pub fn admit_tool(
        bus: &crate::bus::EventBus,
        execution_tool_name: &str,
        request: InvocationAdmissionRequest<'_>,
    ) -> InvocationAdmission {
        let resolved =
            match bus.resolve_invocation(RuntimeInvocationKind::Tool, execution_tool_name) {
                Ok(resolved) => resolved,
                Err(denial) => return InvocationAdmission::Denied(denial),
            };
        if !resolved.surfaces.contains(&request.scope.surface) {
            return InvocationAdmission::Denied(denial(
                InvocationDenialCode::UnsupportedSurface,
                "declared capability does not support this invocation surface",
            ));
        }
        if !resolved
            .execution
            .principals
            .contains(&request.scope.principal_class)
        {
            return InvocationAdmission::Denied(denial(
                InvocationDenialCode::RbacDenied,
                "declared capability does not admit this principal class",
            ));
        }
        if let Some(role) = request.permission_role
            && !crate::permissions::styrene_role_allows_effects(role, &resolved.effects)
        {
            return InvocationAdmission::Denied(InvocationDenial {
                code: InvocationDenialCode::RbacDenied,
                message: format!(
                    "declared effects require a Styrene capability not held by role {}",
                    role.as_str()
                ),
                policy_layer: None,
            });
        }

        if let Some(policy) = request.permission_policy {
            let subjects = subjects_from_tool_args(request.visible_tool_name, request.args);
            let decision = policy.evaluate_subjects(request.visible_tool_name, &subjects);
            match decision.action {
                PermissionAction::Deny => {
                    return InvocationAdmission::Denied(InvocationDenial {
                        code: InvocationDenialCode::PermissionPolicyDenied,
                        message: "permission policy denied the invocation".into(),
                        policy_layer: decision.layer,
                    });
                }
                PermissionAction::Prompt => {
                    return InvocationAdmission::ApprovalRequired(PendingInvocationApproval {
                        requested: permission_subject(request.visible_tool_name, subjects.first()),
                        policy_layer: decision.layer,
                        call_id: request.call_id.to_string(),
                        scope: request.scope,
                        resolved,
                    });
                }
                PermissionAction::Allow => {}
            }
        }

        match ExecutionLease::prepare_and_issue(request.call_id, request.scope, resolved) {
            Ok(lease) => InvocationAdmission::Lease(lease),
            Err(denial) => InvocationAdmission::Denied(denial),
        }
    }
}

fn permission_subject(
    tool: &str,
    subject: Option<&crate::permissions::PermissionSubject>,
) -> String {
    match subject {
        Some(subject) if subject.kind == crate::permissions::PermissionSubjectKind::Path => {
            subject.value.clone()
        }
        Some(subject) => format!("policy:{}:{}", tool, subject.value),
        None => format!("policy:{tool}"),
    }
}

fn recorded_at_now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub(crate) fn denial(code: InvocationDenialCode, message: impl Into<String>) -> InvocationDenial {
    InvocationDenial {
        code,
        message: message.into(),
        policy_layer: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lease_claim_and_close_are_exactly_once() {
        let lease =
            ExecutionLease::issue("call-1", InvocationScope::default(), fixture_resolution());
        lease.claim_dispatch("call-1", "read").unwrap();
        assert!(lease.close(LeaseTerminal::Completed));
        assert!(!lease.close(LeaseTerminal::Failed));
        assert_eq!(lease.terminal(), LeaseTerminal::Completed);
    }

    #[test]
    fn lease_cannot_be_claimed_for_another_call() {
        let lease =
            ExecutionLease::issue("call-1", InvocationScope::default(), fixture_resolution());
        let error = lease.claim_dispatch("call-2", "read").unwrap_err();
        assert_eq!(error.code, InvocationDenialCode::LeaseMismatch);
        assert_eq!(lease.terminal(), LeaseTerminal::Open);
    }

    #[test]
    fn declared_timeout_is_a_ceiling_and_caller_may_only_narrow_it() {
        let mut resolved = fixture_resolution();
        resolved.execution.timeout_class = RuntimeTimeoutClass::Immediate;
        let lease = ExecutionLease::issue("call-1", InvocationScope::default(), resolved);
        assert_eq!(
            lease.execution_timeout(&serde_json::json!({"timeout": 600})),
            std::time::Duration::from_secs(30)
        );
        assert_eq!(
            lease.execution_timeout(&serde_json::json!({"timeout_secs": 5})),
            std::time::Duration::from_secs(5)
        );
    }

    #[test]
    fn durable_scope_persists_owner_acceptance_and_terminal_settlement() {
        let directory = tempfile::tempdir().unwrap();
        let recorded_at = "2026-08-20T12:00:00Z";
        let mut authority = crate::session_authority::SessionAuthority::open(
            &directory.path().join("session.json"),
            "session-1",
            "workspace-1",
            "composition:test",
            crate::session_authority::ActorIdentity {
                principal: "operator".into(),
                ingress: "test".into(),
            },
            recorded_at,
        )
        .unwrap();
        let prompt_id = Uuid::new_v4();
        authority
            .admit_prompt(
                Uuid::new_v4(),
                recorded_at,
                crate::session_authority::PromptAdmitted {
                    submission_id: Uuid::new_v4(),
                    prompt_id,
                    principal: "operator".into(),
                    ingress: "test".into(),
                    queue_mode: crate::session_authority::QueueMode::UntilReady,
                    content: crate::session_authority::PromptContent {
                        text: "run".into(),
                        attachments: vec![],
                    },
                    metadata: serde_json::json!({}),
                },
            )
            .unwrap();
        let turn_id = Uuid::new_v4();
        authority
            .start_turn(Uuid::new_v4(), recorded_at, turn_id, prompt_id)
            .unwrap();
        let authority = crate::session_authority::SessionAuthorityHandle::new(authority);
        let mut resolved = fixture_resolution();
        resolved.execution.deduplication = RuntimeDeduplication::OwnerEnforcedStableCallId;
        let scope = InvocationScope {
            session_id: Some("session-1".into()),
            turn_id: Some(turn_id),
            authority: Some(authority.clone()),
            ..Default::default()
        };

        let lease = ExecutionLease::prepare_and_issue("call-1", scope, resolved).unwrap();
        assert_eq!(lease.deduplication_id.as_deref(), Some("call-1"));
        assert!(matches!(
            authority.state().invocations.get(&lease.invocation_id),
            Some(crate::session_authority::InvocationState::Prepared { preparation })
                if preparation.lease_id == lease.lease_id
                    && preparation.call_id == "call-1"
                    && preparation.deduplication_id.as_deref() == Some("call-1")
        ));

        lease.claim_dispatch("call-1", "read").unwrap();
        lease.persist_dispatched().unwrap();
        assert!(matches!(
            authority.state().invocations.get(&lease.invocation_id),
            Some(crate::session_authority::InvocationState::Dispatched { dispatch, .. })
                if dispatch.lease_id == lease.lease_id
        ));

        lease.invocation_control().acknowledge().unwrap();
        assert!(matches!(
            authority.state().invocations.get(&lease.invocation_id),
            Some(crate::session_authority::InvocationState::Acknowledged {
                acknowledgement,
                ..
            }) if acknowledgement.lease_id == lease.lease_id
        ));

        lease
            .persist_settlement(crate::session_authority::InvocationOutcome::Completed)
            .unwrap();
        assert!(matches!(
            authority.state().invocations.get(&lease.invocation_id),
            Some(crate::session_authority::InvocationState::DurableSettled {
                settlement,
                ..
            }) if settlement.outcome == crate::session_authority::InvocationOutcome::Completed
        ));
    }

    #[test]
    fn scoped_invocation_without_authority_writer_receives_no_lease() {
        let scope = InvocationScope {
            session_id: Some("session-1".into()),
            turn_id: Some(Uuid::new_v4()),
            ..Default::default()
        };
        let error =
            ExecutionLease::prepare_and_issue("call-1", scope, fixture_resolution()).unwrap_err();
        assert_eq!(error.code, InvocationDenialCode::AuthorityUnavailable);
    }

    fn fixture_resolution() -> ResolvedInvocation {
        ResolvedInvocation {
            kind: RuntimeInvocationKind::Tool,
            name: "read".into(),
            capability_id: RuntimeCapabilityId::new("tool:read").unwrap(),
            contribution_id: RuntimeContributionId::new("feature:reader").unwrap(),
            owner_generation_id: RuntimeContributionGenerationId::new("contribution:reader-v1")
                .unwrap(),
            composition_generation_id: RuntimeCompositionGenerationId::new("composition:test")
                .unwrap(),
            effects: vec![RuntimeEffect::FilesystemRead],
            execution: RuntimeExecutionPolicy {
                principals: vec![RuntimePrincipalClass::Model],
                timeout_class: RuntimeTimeoutClass::Interactive,
                retry_class: omegon_traits::RuntimeRetryClass::Never,
                idempotency: omegon_traits::RuntimeIdempotency::NonIdempotent,
                deduplication: omegon_traits::RuntimeDeduplication::Unsupported,
                parallelism: omegon_traits::RuntimeParallelism::Serial,
                transaction: omegon_traits::RuntimeTransactionBehavior::None,
                mutation_fence: None,
                max_attempts: None,
            },
            transition: RuntimeCapabilityTransitionPolicy {
                authority_narrowing: omegon_traits::RuntimeAuthorityNarrowing::DrainExisting,
                active_call_timeout_ms: 1_000,
            },
            surfaces: vec![RuntimeSurface::Model],
        }
    }
}
