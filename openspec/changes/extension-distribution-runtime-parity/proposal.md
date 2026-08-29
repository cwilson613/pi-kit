# Extension distribution runtime parity

## Intent

Ensure every supported distribution declares whether it ships a host-only or
full-product composition and proves the declared runtime behavior after install.

## Scope

Add package-to-runtime smoke tests, sidecar artifact identity, atomic installation
and update assertions, and an explicit parity matrix for release archives,
Homebrew, npm, direct install, Nix, and OCI. A surface may remain host-only when
that status is explicit and tested.

## Success criteria

- Every distribution has an explicit composition class and extension inventory.
- Full-product packages discover and invoke their packaged extensions after installation.
- Host-only packages report typed absence and do not claim full-product parity.
- Install, update, rollback, and removal preserve atomic sidecar ownership.
