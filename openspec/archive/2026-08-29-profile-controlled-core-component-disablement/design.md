# Profile-controlled core component disablement design

## Boundary

Core product components are release-authorized processes such as codescan. They
may use the extension transport, but they are not operator-managed SDK
extensions. Their authority comes from the signed product composition, not from
a manifest field that a process can assert for itself.

This change introduces activation policy only. Package inventory, signature,
digest, target, and update/rollback validation remain mandatory even when a
component is disabled at runtime.

## Identity

Policy uses stable composition IDs. The first ID is `core:codescan`; its existing
wire and manifest identity remains `omegon-codescan`. `core:*` selects every
component explicitly declared disableable by the active product composition.
Resident constitutional capabilities and the maintenance recovery path are not
component IDs and cannot be disabled by this policy.

## Configuration shape

Selected profiles gain an explicit map:

```json
{
  "components": {
    "core:codescan": { "enabled": false }
  }
}
```

An absent entry uses the product composition default. `enabled: true` is an
explicit profile request, still subject to artifact presence, compatibility,
readiness, and higher-authority denies.

A user-local policy under `OMEGON_HOME` supplies a monotonic deny floor across
project-selected profiles:

```json
{
  "schemaVersion": 1,
  "components": {
    "core:*": { "enabled": false }
  }
}
```

The local policy initially accepts only explicit denies. Removing an entry is
the way to remove that floor. Child runtimes receive the resolved deny set rather
than reparsing parent files.

## Resolution

The effective decision is resolved in this order:

1. Signed composition defaults.
2. Selected profile entries.
3. User-local deny overlay.
4. Parent/managed child deny propagation.

Any deny wins. A profile cannot override a user-local or propagated deny. This
follows OpenCode's useful separation between component lifecycle switches and
operation permissions, and its global policy floor, without copying OpenCode's
current schema/runtime mismatch.

Tool permission remains separate. Disabling a component prevents startup;
permission policy can independently restrict operations of a running component.

## Validation

Runtime parsing and the published/generated schema must agree. The new objects
reject unknown fields and invalid value types. Exact selectors must resolve to a
component declared by the selected artifact composition. `core:*` is the only
initial wildcard. Unknown exact IDs are errors rather than inert entries because
a misspelled compliance deny is unsafe.

Validation occurs before any component process, probe, readiness check, or
mutable engine path begins. Contradictory graph policy is rejected if disabling
a component would leave a non-disableable required dependent. Optional
dependents are omitted deterministically and reported.

## Runtime semantics

Component policy is boot-bound. Editing or selecting a profile does not mutate
the generation captured by an active session; command surfaces report that a
restart is required.

When disabled:

- the packaged files and signed composition record remain resident and valid;
- no component process or readiness probe starts;
- component-backed tools remain registered for direct compatibility but are
  excluded from the model-callable set;
- direct CLI, ACP, or tool invocation returns typed `service:disabled` evidence
  with component ID and policy source;
- unrelated components and host capabilities remain unchanged.

`disabled-by-profile` is not reported as absent, failed, incompatible, or
quarantined.

## Persistence and commands

Component policy mutation must write the actual selected profile source, not a
lower-precedence legacy singleton. Canonical command handlers are shared by TUI,
CLI remote execution, and ACP rather than duplicated in ACP workers.

The intended command vocabulary is:

```text
/profile component enable core:codescan
/profile component disable core:codescan
/profile components view
```

These commands update future-boot policy and report the source changed and the
effective deny floor. A separate user-local policy command may be added only if
it preserves the same validator and provenance model.

## Migration

Existing profiles that deny `omegon-codescan` through generic extension policy
must retain their effect. Load resolves that legacy entry to `core:codescan` and
emits a deprecation diagnostic. New saves write the component policy and remove
only the migrated codescan entry, preserving unrelated generic extension rules.

Generic `/extension enable|disable` remains installation-state control and must
not mutate product-component policy.

## Diagnostics

Effective configuration and runtime diagnostics expose, for each component:

- packaged identity and source;
- composition default;
- selected-profile decision and source path;
- user-local or propagated deny source;
- final activation decision;
- runtime state and process provenance when active.

This provides the equivalent of OpenCode's resolved-config and disabled MCP
status surfaces while retaining Omegon's profile provenance and composition
model.

## Distribution coordination

`extension-distribution-runtime-parity` must run normal full-product acceptance
with an isolated default profile. It must also prove that an installed packaged
component remains inventory-valid but does not start under a deny policy.
"Required extension inventory" means required package membership, not
unconditional runtime activation.
