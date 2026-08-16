use std::collections::{BTreeMap, BTreeSet};

use omegon_traits::{
    CommandDefinition, RuntimeCapabilityDeclaration, RuntimeCapabilityDiagnostic,
    RuntimeCapabilityGroup, RuntimeCapabilityId, RuntimeCapabilityInvocation,
    RuntimeCapabilityKind, RuntimeCapabilityOwner, RuntimeCapabilityRegistry,
    RuntimeInvocationKind, ToolDefinition,
};

#[derive(Clone, Debug)]
pub(crate) struct OwnedToolDefinition {
    pub owner: String,
    pub definition: ToolDefinition,
}

#[derive(Clone, Debug)]
pub(crate) struct OwnedCommandDefinition {
    pub owner: String,
    pub definition: CommandDefinition,
}

pub(crate) fn declarations_from_registries(
    tools: impl IntoIterator<Item = OwnedToolDefinition>,
    commands: impl IntoIterator<Item = OwnedCommandDefinition>,
) -> Vec<RuntimeCapabilityDeclaration> {
    let mut declarations = tools
        .into_iter()
        .map(|owned| RuntimeCapabilityDeclaration {
            id: RuntimeCapabilityId::tool(&owned.definition.name),
            kind: RuntimeCapabilityKind::Tool,
            owner: RuntimeCapabilityOwner::feature(owned.owner),
            invocations: vec![RuntimeCapabilityInvocation {
                kind: RuntimeInvocationKind::Tool,
                name: owned.definition.name,
            }],
        })
        .chain(
            commands
                .into_iter()
                .map(|owned| RuntimeCapabilityDeclaration {
                    id: RuntimeCapabilityId::action(&owned.definition.name),
                    kind: RuntimeCapabilityKind::OperatorAction,
                    owner: RuntimeCapabilityOwner::feature(owned.owner),
                    invocations: vec![RuntimeCapabilityInvocation {
                        kind: RuntimeInvocationKind::Command,
                        name: owned.definition.name,
                    }],
                }),
        )
        .collect::<Vec<_>>();
    declarations.sort_by(|left, right| left.id.cmp(&right.id));
    declarations
}

pub(crate) fn validate_registry(
    declarations: Vec<RuntimeCapabilityDeclaration>,
    groups: Vec<RuntimeCapabilityGroup>,
) -> RuntimeCapabilityRegistry {
    let mut diagnostics = Vec::new();
    let mut ids: BTreeMap<RuntimeCapabilityId, RuntimeCapabilityOwner> = BTreeMap::new();
    let mut invocations: BTreeMap<(RuntimeInvocationKind, String), RuntimeCapabilityId> =
        BTreeMap::new();

    for declaration in &declarations {
        if declaration.owner.id.trim().is_empty() {
            diagnostics.push(RuntimeCapabilityDiagnostic::MissingOwner {
                capability_id: declaration.id.clone(),
            });
        }
        if let Some(first_owner) = ids.insert(declaration.id.clone(), declaration.owner.clone()) {
            diagnostics.push(RuntimeCapabilityDiagnostic::DuplicateCapabilityId {
                capability_id: declaration.id.clone(),
                first_owner,
                conflicting_owner: declaration.owner.clone(),
            });
        }
        for invocation in &declaration.invocations {
            let key = (invocation.kind, invocation.name.clone());
            if let Some(first_capability_id) = invocations.insert(key, declaration.id.clone())
                && first_capability_id != declaration.id
            {
                diagnostics.push(RuntimeCapabilityDiagnostic::AmbiguousInvocation {
                    invocation_kind: invocation.kind,
                    name: invocation.name.clone(),
                    first_capability_id,
                    conflicting_capability_id: declaration.id.clone(),
                });
            }
        }
    }

    let known_ids = ids.keys().cloned().collect::<BTreeSet<_>>();
    for group in &groups {
        for member in &group.members {
            if !known_ids.contains(member) {
                diagnostics.push(RuntimeCapabilityDiagnostic::DanglingGroupMember {
                    group: group.name.clone(),
                    capability_id: member.clone(),
                });
            }
        }
    }
    diagnostics.sort();

    RuntimeCapabilityRegistry {
        declarations,
        groups,
        diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omegon_traits::{
        CommandAvailability, CommandSafety, CommandSurface, RuntimeCapabilityOwnerKind,
    };
    use serde_json::json;

    fn tool(name: &str, owner: &str) -> OwnedToolDefinition {
        OwnedToolDefinition {
            owner: owner.into(),
            definition: ToolDefinition {
                name: name.into(),
                label: name.into(),
                description: format!("{name} description"),
                parameters: json!({"type": "object"}),
                capabilities: vec![],
            },
        }
    }

    fn command(name: &str, owner: &str) -> OwnedCommandDefinition {
        OwnedCommandDefinition {
            owner: owner.into(),
            definition: CommandDefinition {
                name: name.into(),
                description: format!("{name} description"),
                subcommands: vec![],
                availability: CommandAvailability::ALL,
                safety: CommandSafety::READ_ONLY,
                surface: CommandSurface::default(),
            },
        }
    }

    #[test]
    fn declarations_preserve_tool_and_command_ownership() {
        let declarations = declarations_from_registries(
            [tool("read", "core-tools")],
            [command("status", "runtime")],
        );

        assert_eq!(declarations[0].id.as_str(), "action:status");
        assert_eq!(declarations[0].owner.id, "runtime");
        assert_eq!(declarations[1].id.as_str(), "tool:read");
        assert_eq!(declarations[1].owner.id, "core-tools");
        assert_eq!(
            declarations[1].owner.kind,
            RuntimeCapabilityOwnerKind::Feature
        );
    }

    #[test]
    fn duplicate_ids_report_both_owners() {
        let declarations =
            declarations_from_registries([tool("read", "first"), tool("read", "second")], []);
        let registry = validate_registry(declarations, vec![]);

        assert!(registry.diagnostics.iter().any(|diagnostic| matches!(
            diagnostic,
            RuntimeCapabilityDiagnostic::DuplicateCapabilityId {
                capability_id,
                first_owner,
                conflicting_owner,
            } if capability_id.as_str() == "tool:read"
                && first_owner.id == "first"
                && conflicting_owner.id == "second"
        )));
    }

    #[test]
    fn ambiguous_invocation_is_distinct_from_duplicate_identity() {
        let declarations = vec![
            RuntimeCapabilityDeclaration {
                id: RuntimeCapabilityId::new("tool:first").unwrap(),
                kind: RuntimeCapabilityKind::Tool,
                owner: RuntimeCapabilityOwner::feature("first"),
                invocations: vec![RuntimeCapabilityInvocation {
                    kind: RuntimeInvocationKind::Tool,
                    name: "shared".into(),
                }],
            },
            RuntimeCapabilityDeclaration {
                id: RuntimeCapabilityId::new("tool:second").unwrap(),
                kind: RuntimeCapabilityKind::Tool,
                owner: RuntimeCapabilityOwner::feature("second"),
                invocations: vec![RuntimeCapabilityInvocation {
                    kind: RuntimeInvocationKind::Tool,
                    name: "shared".into(),
                }],
            },
        ];
        let registry = validate_registry(declarations, vec![]);

        assert!(registry.diagnostics.iter().any(|diagnostic| matches!(
            diagnostic,
            RuntimeCapabilityDiagnostic::AmbiguousInvocation { name, .. } if name == "shared"
        )));
    }

    #[test]
    fn dangling_group_members_are_reported() {
        let declarations = declarations_from_registries([tool("read", "core")], []);
        let registry = validate_registry(
            declarations,
            vec![RuntimeCapabilityGroup {
                name: "lifecycle".into(),
                members: vec![RuntimeCapabilityId::tool("missing")],
            }],
        );

        assert_eq!(
            registry.diagnostics,
            vec![RuntimeCapabilityDiagnostic::DanglingGroupMember {
                group: "lifecycle".into(),
                capability_id: RuntimeCapabilityId::tool("missing"),
            }]
        );
    }

    #[test]
    fn empty_owner_is_rejected() {
        let declarations = declarations_from_registries([tool("read", "")], []);
        let registry = validate_registry(declarations, vec![]);
        assert!(matches!(
            registry.diagnostics.as_slice(),
            [RuntimeCapabilityDiagnostic::MissingOwner { .. }]
        ));
    }
}
