# Task capsule artifact

## Intent

Create the first explicit smaller Omegon runtime layer as a bounded-task
artifact instead of treating every non-TUI build as an implicit minimal runtime.

## Scope

`task-capsule-v0` uses the existing `omegon run` execution path and preserves its
provider, safety, structured-output, timeout, and exit-code behavior. Its build
excludes the TUI and self-update signature stack, has a compile-derived artifact
identity, and is guarded by a dedicated dependency check.

This slice does not yet remove the embedded control plane, ACP, memory,
lifecycle, Git, MCP, extensions, or archive/install command support. Those
domains remain explicit retained dependencies for later subtraction. V0 is
source-built and does not add a published archive, package, update channel, or
container image.

## Success criteria

- A dedicated command builds `task-capsule-v0` without default features.
- The build fails if TUI, self-update, or local-embedding features are combined with the capsule.
- Runtime composition inspection reports capsule identity from compiled features.
- The no-default capsule graph excludes TUI, codescan-engine, Sigstore, and X.509 parser packages.
- CI builds and exercises the exact release-mode capsule graph.
- Existing default product and bounded-task behavior remain compatible.
