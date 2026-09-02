# Operational kernel and core readiness

## Intent

Turn the proven kernel/core composition boundary into an operational product
boundary. The reduced kernel must execute bounded turns, every first-party
domain must have an explicit packaging class, pre-publication distribution
policy must preserve channel-appropriate trust boundaries, and every runtime edge must
consume the same enforceable semantic and lifecycle contracts.

## Scope

This milestone includes deterministic and provider-backed bounded execution in
the reduced kernel, a machine-readable classification of first-party domains,
fail-closed installation and activation evidence, cross-surface semantic
acceptance, prospective bounded-run enforcement, quiescent activation of newly
installed extension generations, and reconciliation of architecture and public
operator documentation.

It does not extract every eligible domain, publish a release, change package
versions, or activate Homebrew, Nix, or OCI as public release lanes. Those
channels retain policy and packaging evidence but are deferred to stable-release
work. Domain
classification determines later extraction work; it does not prejudge that
every managed service should become a process boundary.

## Success criteria

- The reduced kernel completes deterministic conformance turns and real
  provider-backed bounded turns through typed runtime authorities.
- Every first-party domain has a checked packaging class and extraction
  disposition; future `core:*` extraction must extend the additive ladder.
- Release archives, direct installation, and switching fail closed with exact
  composition evidence. Homebrew, Nix, and OCI retain explicit pre-publication
  composition policy without becoming milestone promotion lanes.
- TUI, ACP, Web, IPC, CLI, daemon, and bounded execution pass shared semantic
  parity scenarios; task manifests and budgets are enforced before authority is
  exceeded; new extension generations activate only at a quiescent boundary.
- A scenario-indexed corpus distinguishes scripted, provider-backed, signed-core,
  SDK-addon, and milestone promotion evidence without allowing substitution.
- Durable architecture and public operator documentation describe the shipped
  boundaries and trust posture without aspirational claims.
- Every task and scenario is complete, change validation and archive-check pass,
  and the repository landing gates pass before the branch is declared PR-ready.
