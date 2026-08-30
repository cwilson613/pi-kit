# Extension distribution runtime parity

## Intent

Ensure every supported distribution declares its compiled host profile, installed
product composition, signed core-component inventory, and independently managed
SDK-extension posture, then proves the declared runtime behavior after install.

## Scope

Add package-to-runtime smoke tests, product-component artifact identity, atomic
installation and update assertions, and an explicit parity matrix for release
archives, Homebrew, direct install, Nix, and OCI. Retired npm scaffolding must be
identified as unsupported rather than silently treated as a current distribution.
A surface may remain host-only when that status is explicit and tested.

## Success criteria

- Every distribution has an explicit host profile, composition class, core-component
  inventory, and SDK-extension posture.
- Full-product packages discover and invoke their packaged core components after installation.
- Host-only packages report typed absence and do not claim full-product parity.
- Install, update, rollback, and removal preserve atomic product-component ownership
  without taking ownership of operator-managed SDK extensions.
