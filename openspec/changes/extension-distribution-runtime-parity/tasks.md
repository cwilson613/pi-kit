## 1. Distribution policy
<!-- specs: kernel-composition/distribution-parity -->

- [ ] Add a failing policy test for every distribution without a composition class and exact extension inventory.
- [ ] Classify release archive, direct install, Homebrew, npm, Nix, and OCI outputs by target.
- [ ] Require host-only outputs to publish explicit typed-absence and non-parity metadata.
- [ ] Document the operator-visible capability difference for each host-only output.

## 2. Reference package runtime smoke
<!-- specs: kernel-composition/distribution-parity -->

- [ ] Add a failing test that extracts a normal release archive and launches the installed host.
- [ ] Discover and invoke the packaged codescan sidecar through the host acceptance driver.
- [ ] Assert executable modes, canonical paths, protocol identity, and complete process settlement.
- [ ] Mutate or remove the sidecar and verify typed local unavailability.

## 3. Sidecar provenance
<!-- specs: kernel-composition/distribution-parity -->

- [ ] Add failing lock and manifest tests that reject host-attributed sidecar ownership.
- [ ] Record extension manifest and executable digests, target, protocol range, fallback, and signing identity.
- [ ] Verify signed package inventory and runtime admission bind to the same sidecar bytes.
- [ ] Run corruption, substitution, wrong-target, and wrong-protocol negative tests.

## 4. Atomic install and rollback
<!-- specs: kernel-composition/distribution-parity -->

- [ ] Extend installer tests with a failing partial-sidecar activation case.
- [ ] Stage and activate host plus required extensions as one generation.
- [ ] Add rollback and operator-managed collision tests.
- [ ] Verify update failure leaves the previous host and extension generation callable.

## 5. Distribution smokes and CI
<!-- specs: kernel-composition/distribution-parity -->

- [ ] Add failing npm pack/install and direct-installer runtime smokes.
- [ ] Reuse the acceptance driver for Homebrew and executable Nix targets.
- [ ] Test the declared host-plus-sidecar deployment for OCI or retain explicit host-only status.
- [ ] Run distribution policy on pull requests and target runtime smokes before publication.
