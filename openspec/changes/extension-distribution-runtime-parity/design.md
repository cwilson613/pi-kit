# Extension distribution runtime parity design

## Dependency

Implement `additive-extension-composition-ladder` first. Distribution tests will
execute its composition rows against installed layouts.

## Declared composition classes

Each distribution is either `full-product` or `host-only` for a target and
version. The release policy records required extension members and runtime smoke
commands. A host-only distribution must not advertise full-product capability.

## Installed acceptance

The release tarball is the reference installed-layout test. It will be extracted
into a temporary prefix and exercised through the installed launcher or binary.
The host must discover the adjacent sidecar, run the conformance operation, and
settle the process tree. Other distribution tests reuse the same acceptance
driver when their environment can execute the target artifact.

## Artifact identity

Release-coupled extensions require their own manifest digest, executable digest,
protocol range, target, fallback policy, and signing identity. Sidecar ownership
must not be represented as bytes resident in the host executable.

## Atomic lifecycle

Install and update stage the host and required extension set before activation.
Rollback restores one internally consistent generation. Operator-managed
extensions remain protected from silent replacement by release-coupled assets.
