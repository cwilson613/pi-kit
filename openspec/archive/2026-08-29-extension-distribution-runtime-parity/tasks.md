## 1. Distribution policy
<!-- specs: kernel-composition/distribution-parity -->

- [x] Add a failing policy test for every distribution without an exact host profile, composition class, core-component inventory, and SDK-extension posture.
- [x] Classify release archive, direct install, Homebrew, Nix, and OCI outputs by target, and mark retained npm scaffolding unsupported.
- [x] Require host-only outputs to publish explicit typed-absence and non-parity metadata.
- [x] Document the operator-visible capability difference for each host-only output.

## 2. Reference package runtime smoke
<!-- specs: kernel-composition/distribution-parity -->

- [x] Add a failing test that extracts a normal release archive and launches the installed host.
- [x] Discover and invoke packaged component `core:codescan` through the host acceptance driver.
- [x] Assert executable modes, canonical paths, protocol identity, and complete process settlement.
- [x] Mutate or remove the packaged component and verify typed local unavailability.

## 3. Sidecar provenance
<!-- specs: kernel-composition/distribution-parity -->

- [x] Add failing lock and manifest tests that reject host-attributed component ownership or SDK self-promotion.
- [x] Record component ID, wire manifest ID, manifest and executable digests, target, protocol range, fallback, and signing identity.
- [x] Verify signed package inventory and runtime admission bind to the same product-component bytes.
- [x] Run corruption, substitution, wrong-target, and wrong-protocol negative tests.

## 4. Atomic install and rollback
<!-- specs: kernel-composition/distribution-parity -->

- [x] Extend installer tests with a failing partial-component activation case.
- [x] Stage and activate host plus required product components as one generation.
- [x] Add rollback and operator-managed collision tests.
- [x] Verify update failure leaves the previous host and extension generation callable.

## 5. Distribution smokes and CI
<!-- specs: kernel-composition/distribution-parity -->

- [x] Add a failing direct-installer runtime smoke and a policy test that retired npm scaffolding cannot enter release publication.
- [x] Reuse the acceptance driver for Homebrew and executable Nix targets.
- [x] Test the declared host-plus-sidecar deployment for OCI or retain explicit host-only status.
- [x] Run distribution policy on pull requests and target runtime smokes before publication.
