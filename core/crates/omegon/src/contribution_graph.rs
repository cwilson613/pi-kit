//! Pure candidate contribution graph construction and validation.

use std::collections::{BTreeMap, BTreeSet};

use omegon_traits::{
    RuntimeCapabilityGroupId, RuntimeCapabilityId, RuntimeContributionDeclaration,
    RuntimeContributionDependency, RuntimeContributionDiagnostic, RuntimeContributionId,
    RuntimeDependencyRequirement, RuntimeDependencyTarget, RuntimeDiagnosticCode,
    RuntimeDiagnosticSeverity, RuntimeDiagnosticSubject, RuntimeEffectEvidence,
    RuntimeInvocationBinding, RuntimeInvocationBindingRole, RuntimeInvocationKind,
    RuntimeProtocolRange,
};

type BindingKey = (RuntimeInvocationKind, String);
type BindingClaim = (
    RuntimeContributionId,
    RuntimeCapabilityId,
    RuntimeInvocationBinding,
);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CandidateGraphEnvironment {
    pub supported_protocol: RuntimeProtocolRange,
    pub operating_system: String,
    pub architecture: String,
    pub available_substrates: BTreeSet<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CandidateGraphRequest {
    pub declarations: Vec<RuntimeContributionDeclaration>,
    pub environment: CandidateGraphEnvironment,
    pub effect_evidence: Vec<RuntimeEffectEvidence>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CandidateGraphBuild {
    pub graph: Option<RuntimeCandidateGraph>,
    pub diagnostics: Vec<RuntimeContributionDiagnostic>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RuntimeCandidateGraph {
    pub declarations: BTreeMap<RuntimeContributionId, RuntimeContributionDeclaration>,
    pub superseded: BTreeMap<RuntimeContributionId, RuntimeContributionId>,
    pub capability_owners: BTreeMap<RuntimeCapabilityId, RuntimeContributionId>,
    pub invocation_owners:
        BTreeMap<(RuntimeInvocationKind, String), (RuntimeContributionId, RuntimeCapabilityId)>,
    pub groups: BTreeMap<RuntimeCapabilityGroupId, Vec<RuntimeCapabilityId>>,
    pub dependency_edges: BTreeMap<RuntimeContributionId, BTreeSet<RuntimeContributionId>>,
    pub activation_waves: Vec<Vec<RuntimeContributionId>>,
    pub negotiated_protocols: BTreeMap<RuntimeContributionId, u16>,
}

pub(crate) fn build_candidate_graph(mut request: CandidateGraphRequest) -> CandidateGraphBuild {
    request.declarations.sort_by(|left, right| {
        (&left.id, &left.generation_id)
            .cmp(&(&right.id, &right.generation_id))
            .then_with(|| {
                serde_json::to_string(left)
                    .expect("contribution declaration serializes")
                    .cmp(
                        &serde_json::to_string(right).expect("contribution declaration serializes"),
                    )
            })
    });
    request.effect_evidence.sort();

    let mut diagnostics = Vec::new();
    let mut declaration_claims: BTreeMap<
        RuntimeContributionId,
        Vec<RuntimeContributionDeclaration>,
    > = BTreeMap::new();
    let mut generation_claims: BTreeMap<_, Vec<RuntimeContributionId>> = BTreeMap::new();
    for declaration in request.declarations {
        generation_claims
            .entry(declaration.generation_id.clone())
            .or_default()
            .push(declaration.id.clone());
        declaration_claims
            .entry(declaration.id.clone())
            .or_default()
            .push(declaration);
    }

    for (id, claims) in &declaration_claims {
        if claims.len() > 1 {
            diagnostics.push(diagnostic(
                "graph:duplicate_contribution_id",
                Some(id.clone()),
                None,
                None,
                vec![id.clone()],
                vec![],
                format!("contribution {id:?} has {} declarations", claims.len()),
            ));
        }
    }
    for (generation, owners) in &generation_claims {
        let unique = owners.iter().cloned().collect::<BTreeSet<_>>();
        if unique.len() > 1 {
            diagnostics.push(diagnostic(
                "graph:duplicate_generation_id",
                unique.first().cloned(),
                None,
                None,
                unique.iter().cloned().collect(),
                vec![],
                format!(
                    "contribution generation {} is claimed by multiple owners",
                    generation.as_str()
                ),
            ));
        }
    }

    let all_declarations = declaration_claims
        .values()
        .flat_map(|claims| claims.iter().cloned())
        .collect::<Vec<_>>();
    let declarations = declaration_claims
        .iter()
        .map(|(id, claims)| (id.clone(), claims[0].clone()))
        .collect::<BTreeMap<_, _>>();

    let mut replacement_edges: BTreeMap<RuntimeContributionId, BTreeSet<RuntimeContributionId>> =
        BTreeMap::new();
    let mut replacers: BTreeMap<RuntimeContributionId, BTreeSet<RuntimeContributionId>> =
        BTreeMap::new();
    for declaration in &all_declarations {
        for target in &declaration.replaces {
            if target == &declaration.id {
                diagnostics.push(diagnostic(
                    "graph:self_replacement",
                    Some(declaration.id.clone()),
                    None,
                    None,
                    vec![],
                    vec![],
                    "a contribution cannot replace itself".into(),
                ));
            }
            if declarations.contains_key(target) {
                replacement_edges
                    .entry(declaration.id.clone())
                    .or_default()
                    .insert(target.clone());
                replacers
                    .entry(target.clone())
                    .or_default()
                    .insert(declaration.id.clone());
            }
        }
    }
    for (target, owners) in &replacers {
        if owners.len() > 1 {
            diagnostics.push(diagnostic(
                "graph:ambiguous_replacement",
                Some(target.clone()),
                None,
                None,
                owners.iter().cloned().collect(),
                vec![],
                format!("multiple contributions replace {}", target.as_str()),
            ));
        }
    }
    let replacement_cycles = cycle_components(&replacement_edges);
    for component in &replacement_cycles {
        for member in component {
            diagnostics.push(cycle_diagnostic(
                "graph:replacement_cycle",
                member,
                component,
            ));
        }
    }
    let replacement_cycle_members = replacement_cycles
        .into_iter()
        .flatten()
        .collect::<BTreeSet<_>>();
    let mut superseded = BTreeMap::new();
    for (target, owners) in &replacers {
        if owners.len() == 1
            && !replacement_cycle_members.contains(target)
            && !replacement_cycle_members.contains(owners.first().expect("one replacer"))
        {
            superseded.insert(
                target.clone(),
                owners.first().expect("one replacer").clone(),
            );
        }
    }
    let superseded_keys = superseded.keys().cloned().collect::<BTreeSet<_>>();
    let effective = declarations
        .iter()
        .filter(|(id, _)| !superseded_keys.contains(*id))
        .map(|(id, declaration)| (id.clone(), declaration.clone()))
        .collect::<BTreeMap<_, _>>();
    let effective_claims = all_declarations
        .iter()
        .filter(|declaration| !superseded_keys.contains(&declaration.id))
        .collect::<Vec<_>>();

    let mut capability_claims: BTreeMap<RuntimeCapabilityId, Vec<(RuntimeContributionId, usize)>> =
        BTreeMap::new();
    let mut binding_claims: BTreeMap<BindingKey, Vec<BindingClaim>> = BTreeMap::new();
    let mut group_claims: BTreeMap<
        RuntimeCapabilityGroupId,
        Vec<(RuntimeContributionId, Vec<RuntimeCapabilityId>)>,
    > = BTreeMap::new();

    for declaration in &effective_claims {
        if declaration.validate().is_err() {
            diagnostics.push(diagnostic(
                "graph:invalid_protocol_range",
                Some(declaration.id.clone()),
                None,
                None,
                vec![],
                vec![],
                "contribution protocol range is invalid".into(),
            ));
        }
        for (index, capability) in declaration.capabilities.iter().enumerate() {
            capability_claims
                .entry(capability.id.clone())
                .or_default()
                .push((declaration.id.clone(), index));
            let canonical_count = capability
                .bindings
                .iter()
                .filter(|binding| binding.role == RuntimeInvocationBindingRole::Canonical)
                .count();
            if canonical_count == 0
                && capability
                    .bindings
                    .iter()
                    .any(|binding| binding.role == RuntimeInvocationBindingRole::Alias)
            {
                diagnostics.push(diagnostic(
                    "graph:dangling_alias",
                    Some(declaration.id.clone()),
                    Some(capability.id.clone()),
                    capability.bindings.first().cloned(),
                    vec![],
                    vec![],
                    "alias binding has no canonical binding on its capability".into(),
                ));
            }
            let mut local_bindings = BTreeSet::new();
            for binding in &capability.bindings {
                if binding.name.is_empty() {
                    diagnostics.push(diagnostic(
                        "graph:empty_binding",
                        Some(declaration.id.clone()),
                        Some(capability.id.clone()),
                        Some(binding.clone()),
                        vec![],
                        vec![],
                        "invocation binding name is empty".into(),
                    ));
                }
                if !local_bindings.insert((binding.kind, binding.name.clone())) {
                    diagnostics.push(diagnostic(
                        "graph:duplicate_binding",
                        Some(declaration.id.clone()),
                        Some(capability.id.clone()),
                        Some(binding.clone()),
                        vec![],
                        vec![],
                        "capability repeats an invocation binding".into(),
                    ));
                }
                binding_claims
                    .entry((binding.kind, binding.name.clone()))
                    .or_default()
                    .push((
                        declaration.id.clone(),
                        capability.id.clone(),
                        binding.clone(),
                    ));
            }
        }
        for group in &declaration.groups {
            group_claims
                .entry(group.id.clone())
                .or_default()
                .push((declaration.id.clone(), group.members.clone()));
        }
    }

    let mut capability_owners = BTreeMap::new();
    for (capability, claims) in &capability_claims {
        if claims.len() == 1 {
            capability_owners.insert(capability.clone(), claims[0].0.clone());
        } else {
            let owners = claims
                .iter()
                .map(|(owner, _)| owner.clone())
                .collect::<BTreeSet<_>>();
            diagnostics.push(diagnostic(
                "graph:duplicate_owner",
                owners.first().cloned(),
                Some(capability.clone()),
                None,
                owners.iter().cloned().collect(),
                vec![capability.clone()],
                format!("capability {} has multiple owners", capability.as_str()),
            ));
        }
    }

    let mut invocation_owners = BTreeMap::new();
    for (key, claims) in &binding_claims {
        let capabilities = claims
            .iter()
            .map(|(_, capability, _)| capability.clone())
            .collect::<BTreeSet<_>>();
        let owners = claims
            .iter()
            .map(|(owner, _, _)| owner.clone())
            .collect::<BTreeSet<_>>();
        if claims.len() > 1 {
            diagnostics.push(diagnostic(
                "graph:ambiguous_binding",
                owners.first().cloned(),
                capabilities.first().cloned(),
                claims.first().map(|(_, _, binding)| binding.clone()),
                owners.iter().cloned().collect(),
                capabilities.iter().cloned().collect(),
                format!("invocation {} has multiple capabilities", key.1),
            ));
        } else {
            invocation_owners.insert(key.clone(), (claims[0].0.clone(), claims[0].1.clone()));
        }
    }

    let mut groups = BTreeMap::new();
    for (group, claims) in &group_claims {
        if claims.len() > 1 {
            diagnostics.push(diagnostic(
                "graph:duplicate_group_id",
                Some(claims[0].0.clone()),
                None,
                None,
                claims.iter().map(|(owner, _)| owner.clone()).collect(),
                vec![],
                format!("capability group {} has multiple owners", group.as_str()),
            ));
        } else {
            groups.insert(group.clone(), claims[0].1.clone());
        }
        for (owner, members) in claims {
            for member in members {
                if !capability_owners.contains_key(member) {
                    diagnostics.push(diagnostic(
                        "graph:dangling_group_member",
                        Some(owner.clone()),
                        Some(member.clone()),
                        None,
                        vec![],
                        vec![member.clone()],
                        format!(
                            "capability group {} references an unavailable member",
                            group.as_str()
                        ),
                    ));
                }
            }
        }
    }

    let mut dependency_edges = effective
        .keys()
        .cloned()
        .map(|id| (id, BTreeSet::new()))
        .collect::<BTreeMap<_, _>>();
    for declaration in &effective_claims {
        for dependency in &declaration.dependencies {
            match resolve_dependency(dependency, &effective, &superseded, &capability_owners) {
                Some(owner) => {
                    dependency_edges
                        .get_mut(&declaration.id)
                        .expect("effective declaration has dependency entry")
                        .insert(owner);
                }
                None if dependency.requirement == RuntimeDependencyRequirement::Required => {
                    diagnostics.push(diagnostic(
                        "graph:missing_dependency",
                        Some(declaration.id.clone()),
                        dependency_capability(&dependency.target),
                        None,
                        dependency_contributions(&dependency.target),
                        dependency_capabilities(&dependency.target),
                        "required contribution dependency is unavailable".into(),
                    ));
                }
                None => {}
            }
        }
    }
    for component in cycle_components(&dependency_edges) {
        for member in &component {
            diagnostics.push(cycle_diagnostic(
                "graph:dependency_cycle",
                member,
                &component,
            ));
        }
    }

    let mut seen_conflicts = BTreeSet::new();
    for declaration in &effective_claims {
        for target in &declaration.conflicts {
            let target = resolve_replacement(target, &superseded);
            if !effective.contains_key(&target) {
                continue;
            }
            let pair = if declaration.id <= target {
                (declaration.id.clone(), target)
            } else {
                (target, declaration.id.clone())
            };
            if seen_conflicts.insert(pair.clone()) {
                diagnostics.push(diagnostic(
                    "graph:conflict",
                    Some(pair.0.clone()),
                    None,
                    None,
                    vec![pair.0, pair.1],
                    vec![],
                    "active contributions conflict".into(),
                ));
            }
        }
    }

    let mut negotiated_protocols = BTreeMap::new();
    for declaration in &effective_claims {
        let minimum = declaration
            .protocol
            .minimum
            .max(request.environment.supported_protocol.minimum);
        let maximum = declaration
            .protocol
            .maximum
            .min(request.environment.supported_protocol.maximum);
        if minimum > maximum {
            diagnostics.push(diagnostic(
                "graph:protocol_incompatible",
                Some(declaration.id.clone()),
                None,
                None,
                vec![],
                vec![],
                "contribution and host protocol ranges do not overlap".into(),
            ));
        } else {
            negotiated_protocols.insert(declaration.id.clone(), maximum);
        }
        if !declaration.platform.operating_systems.is_empty()
            && !declaration
                .platform
                .operating_systems
                .contains(&request.environment.operating_system)
        {
            diagnostics.push(platform_diagnostic(
                &declaration.id,
                "operating system",
                &request.environment.operating_system,
            ));
        }
        if !declaration.platform.architectures.is_empty()
            && !declaration
                .platform
                .architectures
                .contains(&request.environment.architecture)
        {
            diagnostics.push(platform_diagnostic(
                &declaration.id,
                "architecture",
                &request.environment.architecture,
            ));
        }
        for substrate in declaration
            .platform
            .required_substrates
            .iter()
            .collect::<BTreeSet<_>>()
        {
            if !request.environment.available_substrates.contains(substrate) {
                diagnostics.push(platform_diagnostic(&declaration.id, "substrate", substrate));
            }
        }
    }

    for evidence in request.effect_evidence {
        let Some(declaration) = effective.get(&evidence.contribution_id) else {
            diagnostics.push(undeclared_effect_diagnostic(&evidence));
            continue;
        };
        let declared = match &evidence.capability_id {
            Some(capability_id) => declaration
                .capabilities
                .iter()
                .find(|capability| &capability.id == capability_id)
                .is_some_and(|capability| capability.effects.contains(&evidence.effect)),
            None => declaration
                .capabilities
                .iter()
                .any(|capability| capability.effects.contains(&evidence.effect)),
        };
        if !declared {
            diagnostics.push(undeclared_effect_diagnostic(&evidence));
        }
    }

    diagnostics.sort_by_key(RuntimeContributionDiagnostic::stable_order_key);
    diagnostics.dedup();
    if !diagnostics.is_empty() {
        return CandidateGraphBuild {
            graph: None,
            diagnostics,
        };
    }

    let activation_waves = activation_waves(&dependency_edges);
    CandidateGraphBuild {
        graph: Some(RuntimeCandidateGraph {
            declarations: effective,
            superseded,
            capability_owners,
            invocation_owners,
            groups,
            dependency_edges,
            activation_waves,
            negotiated_protocols,
        }),
        diagnostics,
    }
}

fn resolve_dependency(
    dependency: &RuntimeContributionDependency,
    declarations: &BTreeMap<RuntimeContributionId, RuntimeContributionDeclaration>,
    superseded: &BTreeMap<RuntimeContributionId, RuntimeContributionId>,
    capability_owners: &BTreeMap<RuntimeCapabilityId, RuntimeContributionId>,
) -> Option<RuntimeContributionId> {
    match &dependency.target {
        RuntimeDependencyTarget::Contribution { id } => {
            let resolved = resolve_replacement(id, superseded);
            declarations.contains_key(&resolved).then_some(resolved)
        }
        RuntimeDependencyTarget::Capability { id } => capability_owners.get(id).cloned(),
    }
}

fn resolve_replacement(
    id: &RuntimeContributionId,
    superseded: &BTreeMap<RuntimeContributionId, RuntimeContributionId>,
) -> RuntimeContributionId {
    let mut current = id.clone();
    let mut visited = BTreeSet::new();
    while visited.insert(current.clone()) {
        let Some(next) = superseded.get(&current) else {
            break;
        };
        current = next.clone();
    }
    current
}

fn dependency_capability(target: &RuntimeDependencyTarget) -> Option<RuntimeCapabilityId> {
    match target {
        RuntimeDependencyTarget::Capability { id } => Some(id.clone()),
        RuntimeDependencyTarget::Contribution { .. } => None,
    }
}

fn dependency_contributions(target: &RuntimeDependencyTarget) -> Vec<RuntimeContributionId> {
    match target {
        RuntimeDependencyTarget::Contribution { id } => vec![id.clone()],
        RuntimeDependencyTarget::Capability { .. } => vec![],
    }
}

fn dependency_capabilities(target: &RuntimeDependencyTarget) -> Vec<RuntimeCapabilityId> {
    match target {
        RuntimeDependencyTarget::Capability { id } => vec![id.clone()],
        RuntimeDependencyTarget::Contribution { .. } => vec![],
    }
}

fn cycle_components(
    edges: &BTreeMap<RuntimeContributionId, BTreeSet<RuntimeContributionId>>,
) -> Vec<Vec<RuntimeContributionId>> {
    let cycle_members = edges
        .keys()
        .filter(|node| {
            edges
                .get(*node)
                .is_some_and(|targets| targets.iter().any(|target| can_reach(target, node, edges)))
        })
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut assigned = BTreeSet::new();
    let mut components = Vec::new();
    for node in &cycle_members {
        if assigned.contains(node) {
            continue;
        }
        let component = cycle_members
            .iter()
            .filter(|other| can_reach(node, other, edges) && can_reach(other, node, edges))
            .cloned()
            .collect::<Vec<_>>();
        assigned.extend(component.iter().cloned());
        components.push(component);
    }
    components
}

fn can_reach(
    start: &RuntimeContributionId,
    target: &RuntimeContributionId,
    edges: &BTreeMap<RuntimeContributionId, BTreeSet<RuntimeContributionId>>,
) -> bool {
    let mut pending = vec![start.clone()];
    let mut visited = BTreeSet::new();
    while let Some(node) = pending.pop() {
        if &node == target {
            return true;
        }
        if visited.insert(node.clone())
            && let Some(next) = edges.get(&node)
        {
            pending.extend(next.iter().rev().cloned());
        }
    }
    false
}

fn activation_waves(
    edges: &BTreeMap<RuntimeContributionId, BTreeSet<RuntimeContributionId>>,
) -> Vec<Vec<RuntimeContributionId>> {
    let mut remaining = edges.clone();
    let mut waves = Vec::new();
    while !remaining.is_empty() {
        let ready = remaining
            .iter()
            .filter(|(_, requirements)| requirements.is_empty())
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        debug_assert!(!ready.is_empty(), "validated dependency graph is acyclic");
        for id in &ready {
            remaining.remove(id);
        }
        for requirements in remaining.values_mut() {
            for id in &ready {
                requirements.remove(id);
            }
        }
        waves.push(ready);
    }
    waves
}

fn diagnostic(
    code: &str,
    contribution_id: Option<RuntimeContributionId>,
    capability_id: Option<RuntimeCapabilityId>,
    invocation: Option<RuntimeInvocationBinding>,
    mut related_contributions: Vec<RuntimeContributionId>,
    mut related_capabilities: Vec<RuntimeCapabilityId>,
    message: String,
) -> RuntimeContributionDiagnostic {
    related_contributions.sort();
    related_contributions.dedup();
    related_capabilities.sort();
    related_capabilities.dedup();
    RuntimeContributionDiagnostic {
        code: RuntimeDiagnosticCode::new(code).expect("graph diagnostic code is valid"),
        severity: RuntimeDiagnosticSeverity::Error,
        subject: RuntimeDiagnosticSubject {
            contribution_id,
            capability_id,
            invocation,
        },
        related_contributions,
        related_capabilities,
        message,
    }
}

fn cycle_diagnostic(
    code: &str,
    member: &RuntimeContributionId,
    component: &[RuntimeContributionId],
) -> RuntimeContributionDiagnostic {
    diagnostic(
        code,
        Some(member.clone()),
        None,
        None,
        component.to_vec(),
        vec![],
        "contribution participates in a cycle".into(),
    )
}

fn platform_diagnostic(
    contribution: &RuntimeContributionId,
    dimension: &str,
    value: &str,
) -> RuntimeContributionDiagnostic {
    diagnostic(
        "graph:platform_incompatible",
        Some(contribution.clone()),
        None,
        None,
        vec![],
        vec![],
        format!("unsupported {dimension}: {value}"),
    )
}

fn undeclared_effect_diagnostic(evidence: &RuntimeEffectEvidence) -> RuntimeContributionDiagnostic {
    diagnostic(
        "graph:undeclared_effect",
        Some(evidence.contribution_id.clone()),
        evidence.capability_id.clone(),
        None,
        vec![],
        evidence.capability_id.clone().into_iter().collect(),
        format!(
            "{:?} effect evidence is absent from the frozen declaration: {:?}",
            evidence.kind, evidence.effect
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use omegon_traits::{
        RuntimeActivationBoundary, RuntimeAuthorityNarrowing, RuntimeCapabilityTransitionPolicy,
        RuntimeCleanupRequirement, RuntimeCompositionTransitionPolicy, RuntimeConfinementRequest,
        RuntimeContributionCapabilityDeclaration, RuntimeContributionCapabilityGroup,
        RuntimeContributionGenerationId, RuntimeContributionSchemaVersion, RuntimeDeduplication,
        RuntimeEffect, RuntimeEffectEvidenceKind, RuntimeExecutionPolicy,
        RuntimeFailureDisposition, RuntimeIdempotency, RuntimeLifecyclePolicy,
        RuntimeLifecycleRequirement, RuntimeOwnerTier, RuntimePlatformRequirements,
        RuntimeRetryClass, RuntimeSurface, RuntimeTimeoutClass, RuntimeTrustRequest,
    };

    fn contribution(name: &str, capability_name: &str) -> RuntimeContributionDeclaration {
        RuntimeContributionDeclaration {
            schema_version: RuntimeContributionSchemaVersion::V1,
            id: RuntimeContributionId::new(format!("feature:{name}")).unwrap(),
            generation_id: RuntimeContributionGenerationId::new(format!("contribution:{name}-v1"))
                .unwrap(),
            owner_tier: RuntimeOwnerTier::System,
            requested_trust: RuntimeTrustRequest::ReleaseArtifact,
            requested_confinement: RuntimeConfinementRequest::HostProcess,
            protocol: RuntimeProtocolRange::new(1, 2).unwrap(),
            platform: RuntimePlatformRequirements::default(),
            dependencies: vec![],
            conflicts: vec![],
            replaces: vec![],
            lifecycle: RuntimeLifecyclePolicy {
                requirement: RuntimeLifecycleRequirement::Required,
                failure_disposition: RuntimeFailureDisposition::FailComposition,
                readiness_timeout_ms: 1_000,
                heartbeat_timeout_ms: None,
                restart_limit: 0,
            },
            transition: RuntimeCompositionTransitionPolicy {
                activation_boundary: RuntimeActivationBoundary::Boot,
                cleanup: RuntimeCleanupRequirement::Strict,
                cleanup_timeout_ms: 1_000,
            },
            capabilities: vec![RuntimeContributionCapabilityDeclaration {
                id: RuntimeCapabilityId::new(format!("tool:{capability_name}")).unwrap(),
                kind: omegon_traits::RuntimeCapabilityKind::Tool,
                bindings: vec![RuntimeInvocationBinding {
                    kind: RuntimeInvocationKind::Tool,
                    name: capability_name.into(),
                    role: RuntimeInvocationBindingRole::Canonical,
                }],
                effects: vec![RuntimeEffect::FilesystemRead],
                execution: RuntimeExecutionPolicy {
                    principals: vec![omegon_traits::RuntimePrincipalClass::Model],
                    timeout_class: RuntimeTimeoutClass::Interactive,
                    retry_class: RuntimeRetryClass::IdempotentFailure,
                    idempotency: RuntimeIdempotency::Idempotent,
                    deduplication: RuntimeDeduplication::OwnerEnforcedStableCallId,
                    parallelism: omegon_traits::RuntimeParallelism::ParallelSafe,
                    transaction: omegon_traits::RuntimeTransactionBehavior::None,
                    max_attempts: Some(2),
                },
                transition: RuntimeCapabilityTransitionPolicy {
                    authority_narrowing: RuntimeAuthorityNarrowing::CompleteExisting,
                    active_call_timeout_ms: 1_000,
                },
                surfaces: vec![RuntimeSurface::Model],
            }],
            groups: vec![],
        }
    }

    fn environment() -> CandidateGraphEnvironment {
        CandidateGraphEnvironment {
            supported_protocol: RuntimeProtocolRange::new(2, 3).unwrap(),
            operating_system: "linux".into(),
            architecture: "x86_64".into(),
            available_substrates: BTreeSet::from(["host".into()]),
        }
    }

    fn build(declarations: Vec<RuntimeContributionDeclaration>) -> CandidateGraphBuild {
        build_candidate_graph(CandidateGraphRequest {
            declarations,
            environment: environment(),
            effect_evidence: vec![],
        })
    }

    fn codes(build: &CandidateGraphBuild) -> BTreeSet<&str> {
        build
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect()
    }

    #[test]
    fn valid_graph_is_deterministic_and_orders_dependencies_in_waves() {
        let core = contribution("core", "read");
        let mut dependent = contribution("dependent", "search");
        dependent.dependencies.push(RuntimeContributionDependency {
            target: RuntimeDependencyTarget::Capability {
                id: RuntimeCapabilityId::tool("read"),
            },
            requirement: RuntimeDependencyRequirement::Required,
        });
        dependent.capabilities[0]
            .bindings
            .push(RuntimeInvocationBinding {
                kind: RuntimeInvocationKind::Cli,
                name: "lookup".into(),
                role: RuntimeInvocationBindingRole::Alias,
            });
        let first = build(vec![dependent.clone(), core.clone()]);
        let second = build(vec![core, dependent]);

        assert_eq!(first, second);
        let graph = first.graph.unwrap();
        assert_eq!(
            graph.activation_waves,
            vec![
                vec![RuntimeContributionId::new("feature:core").unwrap()],
                vec![RuntimeContributionId::new("feature:dependent").unwrap()]
            ]
        );
        assert_eq!(
            graph
                .negotiated_protocols
                .values()
                .copied()
                .collect::<Vec<_>>(),
            vec![2, 2]
        );
    }

    #[test]
    fn duplicate_owners_and_ambiguous_bindings_report_all_claimants() {
        let first = contribution("first", "shared");
        let mut second = contribution("second", "other");
        second.capabilities[0].id = RuntimeCapabilityId::tool("shared");
        second.capabilities[0].bindings[0].name = "shared".into();
        let mut third = contribution("third", "third");
        third.capabilities[0].bindings[0].name = "shared".into();

        let result = build(vec![third, second, first]);
        assert!(result.graph.is_none());
        assert!(codes(&result).contains("graph:duplicate_owner"));
        assert!(codes(&result).contains("graph:ambiguous_binding"));
        let duplicate = result
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code.as_str() == "graph:duplicate_owner")
            .unwrap();
        assert_eq!(duplicate.related_contributions.len(), 2);
    }

    #[test]
    fn canonical_and_alias_with_the_same_transport_name_cannot_drop_the_owner() {
        let mut declaration = contribution("bindings", "read");
        declaration.capabilities[0]
            .bindings
            .push(RuntimeInvocationBinding {
                kind: RuntimeInvocationKind::Tool,
                name: "read".into(),
                role: RuntimeInvocationBindingRole::Alias,
            });

        let result = build(vec![declaration]);
        assert!(result.graph.is_none());
        assert!(codes(&result).contains("graph:duplicate_binding"));
        assert!(codes(&result).contains("graph:ambiguous_binding"));
    }

    #[test]
    fn duplicate_contribution_claims_still_report_every_claims_defects() {
        let first = contribution("duplicate", "read");
        let mut second = first.clone();
        second.generation_id =
            RuntimeContributionGenerationId::new("contribution:duplicate-v2").unwrap();
        second.protocol = RuntimeProtocolRange::new(5, 6).unwrap();
        second.platform.operating_systems = vec!["macos".into()];
        second.dependencies.push(required_contribution(
            &RuntimeContributionId::new("feature:missing").unwrap(),
        ));
        second.groups.push(RuntimeContributionCapabilityGroup {
            id: RuntimeCapabilityGroupId::new("group:missing").unwrap(),
            members: vec![RuntimeCapabilityId::tool("missing")],
        });

        let result = build(vec![second, first]);
        let actual = codes(&result);
        for expected in [
            "graph:duplicate_contribution_id",
            "graph:dangling_group_member",
            "graph:missing_dependency",
            "graph:platform_incompatible",
            "graph:protocol_incompatible",
        ] {
            assert!(actual.contains(expected), "missing {expected}");
        }
    }

    #[test]
    fn valid_replacement_resolves_duplicate_capability_owner() {
        let old = contribution("old", "read");
        let mut new = contribution("new", "read");
        new.replaces.push(old.id.clone());

        let result = build(vec![old.clone(), new.clone()]);
        let graph = result.graph.unwrap();
        assert_eq!(graph.superseded.get(&old.id), Some(&new.id));
        assert_eq!(
            graph
                .capability_owners
                .get(&RuntimeCapabilityId::tool("read")),
            Some(&new.id)
        );
    }

    #[test]
    fn replacement_chains_redirect_dependencies_and_cycles_fail_closed() {
        let old = contribution("old", "read");
        let mut middle = contribution("middle", "read");
        middle.replaces.push(old.id.clone());
        let mut latest = contribution("latest", "read");
        latest.replaces.push(middle.id.clone());
        let mut dependent = contribution("dependent", "search");
        dependent.dependencies.push(required_contribution(&old.id));

        let result = build(vec![dependent.clone(), old, middle, latest.clone()]);
        let graph = result.graph.unwrap();
        assert_eq!(
            graph.dependency_edges.get(&dependent.id),
            Some(&BTreeSet::from([latest.id.clone()]))
        );

        let mut first = contribution("cycle-first", "first");
        let mut second = contribution("cycle-second", "second");
        first.replaces.push(second.id.clone());
        second.replaces.push(first.id.clone());
        let cycle = build(vec![second, first]);
        assert!(cycle.graph.is_none());
        assert_eq!(
            cycle
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code.as_str() == "graph:replacement_cycle")
                .count(),
            2
        );
    }

    #[test]
    fn alias_without_canonical_binding_and_dangling_group_are_rejected() {
        let mut declaration = contribution("aliases", "read");
        declaration.capabilities[0].bindings[0].role = RuntimeInvocationBindingRole::Alias;
        declaration.groups.push(RuntimeContributionCapabilityGroup {
            id: RuntimeCapabilityGroupId::new("group:missing").unwrap(),
            members: vec![RuntimeCapabilityId::tool("missing")],
        });

        let result = build(vec![declaration]);
        assert!(codes(&result).contains("graph:dangling_alias"));
        assert!(codes(&result).contains("graph:dangling_group_member"));
    }

    #[test]
    fn missing_requirement_and_actual_cycle_members_are_reported() {
        let mut first = contribution("first", "first");
        let mut second = contribution("second", "second");
        let mut downstream = contribution("downstream", "downstream");
        first.dependencies.push(required_contribution(&second.id));
        second.dependencies.push(required_contribution(&first.id));
        downstream
            .dependencies
            .push(required_contribution(&first.id));
        downstream.dependencies.push(required_contribution(
            &RuntimeContributionId::new("feature:missing").unwrap(),
        ));

        let result = build(vec![downstream.clone(), second, first]);
        assert!(codes(&result).contains("graph:missing_dependency"));
        let cycle_subjects = result
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code.as_str() == "graph:dependency_cycle")
            .filter_map(|diagnostic| diagnostic.subject.contribution_id.as_ref())
            .cloned()
            .collect::<BTreeSet<_>>();
        assert_eq!(
            cycle_subjects,
            BTreeSet::from([
                RuntimeContributionId::new("feature:first").unwrap(),
                RuntimeContributionId::new("feature:second").unwrap(),
            ])
        );
        assert!(!cycle_subjects.contains(&downstream.id));
    }

    #[test]
    fn conflicts_are_canonical_and_symmetric_declarations_do_not_duplicate_them() {
        let mut first = contribution("first", "first");
        let mut second = contribution("second", "second");
        first.conflicts.push(second.id.clone());
        second.conflicts.push(first.id.clone());

        let result = build(vec![second, first]);
        assert_eq!(
            result
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code.as_str() == "graph:conflict")
                .count(),
            1
        );
    }

    #[test]
    fn protocol_and_each_platform_dimension_report_together() {
        let mut declaration = contribution("portable", "read");
        declaration.protocol = RuntimeProtocolRange::new(4, 5).unwrap();
        declaration.platform.operating_systems = vec!["macos".into()];
        declaration.platform.architectures = vec!["aarch64".into()];
        declaration.platform.required_substrates = vec!["oci".into(), "sandbox".into()];

        let result = build(vec![declaration]);
        assert!(codes(&result).contains("graph:protocol_incompatible"));
        assert_eq!(
            result
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code.as_str() == "graph:platform_incompatible")
                .count(),
            4
        );
    }

    #[test]
    fn requested_and_observed_undeclared_effects_fail_without_mutating_declarations() {
        let declaration = contribution("effects", "read");
        let evidence = [
            RuntimeEffectEvidenceKind::Requested,
            RuntimeEffectEvidenceKind::Observed,
        ]
        .into_iter()
        .map(|kind| RuntimeEffectEvidence {
            contribution_id: declaration.id.clone(),
            capability_id: Some(declaration.capabilities[0].id.clone()),
            effect: RuntimeEffect::ProcessSpawn,
            kind,
        })
        .collect();
        let result = build_candidate_graph(CandidateGraphRequest {
            declarations: vec![declaration],
            environment: environment(),
            effect_evidence: evidence,
        });

        assert!(result.graph.is_none());
        assert_eq!(
            result
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code.as_str() == "graph:undeclared_effect")
                .count(),
            2
        );
    }

    #[test]
    fn all_error_categories_are_retained_and_input_order_is_irrelevant() {
        let mut first = contribution("first", "shared");
        let mut second = contribution("second", "second");
        second.capabilities[0].bindings[0].name = "shared".into();
        first.dependencies.push(required_contribution(
            &RuntimeContributionId::new("feature:missing").unwrap(),
        ));
        first.conflicts.push(second.id.clone());
        first.protocol = RuntimeProtocolRange::new(5, 6).unwrap();
        first.platform.operating_systems = vec!["macos".into()];
        first.groups.push(RuntimeContributionCapabilityGroup {
            id: RuntimeCapabilityGroupId::new("group:bad").unwrap(),
            members: vec![RuntimeCapabilityId::tool("missing")],
        });
        let evidence = vec![RuntimeEffectEvidence {
            contribution_id: first.id.clone(),
            capability_id: Some(first.capabilities[0].id.clone()),
            effect: RuntimeEffect::ProcessSpawn,
            kind: RuntimeEffectEvidenceKind::Observed,
        }];
        let request = |declarations| CandidateGraphRequest {
            declarations,
            environment: environment(),
            effect_evidence: evidence.clone(),
        };
        let forward = build_candidate_graph(request(vec![first.clone(), second.clone()]));
        let reverse = build_candidate_graph(request(vec![second, first]));

        assert_eq!(forward, reverse);
        assert!(forward.graph.is_none());
        let actual = codes(&forward);
        for expected in [
            "graph:ambiguous_binding",
            "graph:conflict",
            "graph:dangling_group_member",
            "graph:missing_dependency",
            "graph:platform_incompatible",
            "graph:protocol_incompatible",
            "graph:undeclared_effect",
        ] {
            assert!(actual.contains(expected), "missing {expected}");
        }
    }

    fn required_contribution(id: &RuntimeContributionId) -> RuntimeContributionDependency {
        RuntimeContributionDependency {
            target: RuntimeDependencyTarget::Contribution { id: id.clone() },
            requirement: RuntimeDependencyRequirement::Required,
        }
    }
}
