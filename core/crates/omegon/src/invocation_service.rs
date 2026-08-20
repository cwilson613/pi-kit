//! Kernel-owned privileged invocation admission and ephemeral execution leases.
//!
//! Slice 3.1 centralizes accepted-graph resolution, RBAC, permission policy,
//! approval, and generation binding. Durable Prepared/Dispatched state begins in
//! Slice 3.3; these leases deliberately make no crash-consistency claim.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use omegon_traits::{
    RuntimeCapabilityId, RuntimeCapabilityTransitionPolicy, RuntimeCompositionGenerationId,
    RuntimeContributionGenerationId, RuntimeContributionId, RuntimeEffect, RuntimeInvocationKind,
    RuntimeSurface,
};
use serde_json::Value;
use uuid::Uuid;

use crate::permissions::{
    LayeredPermissionPolicy, PermissionAction, PermissionLayer, subjects_from_tool_args,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InvocationScope {
    pub principal: String,
    pub surface: RuntimeSurface,
    pub session_id: Option<String>,
    pub turn_id: Option<Uuid>,
}

impl Default for InvocationScope {
    fn default() -> Self {
        Self {
            principal: "model".into(),
            surface: RuntimeSurface::Model,
            session_id: None,
            turn_id: None,
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
    pub principal: String,
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
    pub transition: RuntimeCapabilityTransitionPolicy,
    terminal: Arc<AtomicU8>,
}

impl ExecutionLease {
    fn issue(call_id: &str, scope: InvocationScope, resolved: ResolvedInvocation) -> Self {
        Self {
            lease_id: Uuid::new_v4(),
            invocation_id: Uuid::new_v4(),
            call_id: call_id.to_string(),
            principal: scope.principal,
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
            transition: resolved.transition,
            terminal: Arc::new(AtomicU8::new(LeaseTerminal::Open as u8)),
        }
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
        Ok(ExecutionLease::issue(
            &self.call_id,
            self.scope,
            self.resolved,
        ))
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
        if let Some(role) = request.permission_role
            && !crate::permissions::styrene_role_allows_tool(role, request.visible_tool_name)
        {
            return InvocationAdmission::Denied(InvocationDenial {
                code: InvocationDenialCode::RbacDenied,
                message: format!(
                    "{} requires a Styrene capability not held by role {}",
                    request.visible_tool_name,
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

        InvocationAdmission::Lease(ExecutionLease::issue(
            request.call_id,
            request.scope,
            resolved,
        ))
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
            transition: RuntimeCapabilityTransitionPolicy {
                authority_narrowing: omegon_traits::RuntimeAuthorityNarrowing::DrainExisting,
                active_call_timeout_ms: 1_000,
            },
            surfaces: vec![RuntimeSurface::Model],
        }
    }
}
