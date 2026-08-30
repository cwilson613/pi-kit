# Profile-controlled core component disablement

## Intent

Let operators and compliance owners disable release-coupled core product
components without uninstalling or corrupting the signed product generation.
Replace the current accidental reuse of generic extension policy with a typed,
validated component policy that explains the effective decision and prevents
the denied process from starting.

## Scope

Define stable `core:*` component selectors, selected-profile controls, a
user-local monotonic deny overlay, child-runtime propagation, strict validation,
startup admission, typed disabled behavior, restart semantics, and diagnostic
provenance. Cover release-coupled codescan as the first component and migrate
existing `extensions.disabled = ["omegon-codescan"]` intent.

This change does not make resident constitutional capabilities disableable,
hot-unload active components, create a component package manager, or change the
independent SDK-extension trust and update model.

## Success criteria

- A profile can disable `core:codescan`, and no codescan process, readiness
  probe, index, or database mutation occurs on the next boot.
- A user-local deny remains effective when a project selects a profile that
  enables the component.
- Invalid keys, malformed selectors, unknown exact component IDs, and attempts
  to disable constitutional capabilities fail validation with source paths.
- Effective settings and diagnostics distinguish packaged, disabled-by-policy,
  absent, incompatible, failed, and healthy component states with provenance.
- Full-product package locks remain valid when a packaged component is disabled,
  while direct invocation reports typed disabled behavior and model-facing tools
  are not callable.
