## 1. Artifact contract
<!-- specs: kernel-composition/artifact-profiles -->

- [x] Add the marker and self-update features with fail-closed combinations.
- [x] Derive capsule composition identity from compiled features.
- [x] Add focused artifact-identity and unavailable-update tests.
- [x] Document the v0 build, execution, exclusions, and retained domains.

## 2. Build and dependency ratchet
<!-- specs: kernel-composition/artifact-profiles -->

- [x] Add dedicated capsule build and check recipes.
- [x] Enforce TUI, codescan-engine, Sigstore, and X.509 parser absence.
- [x] Verify default and capsule feature graphs compile independently.

## 3. Verification
<!-- specs: kernel-composition/artifact-profiles -->

- [x] Validate the OpenSpec change and focused Rust tests.
- [x] Run capsule dependency, formatting, and changed-code lint gates.

## 4. Review hardening
<!-- specs: kernel-composition/artifact-profiles -->

- [x] Make exact capsule identity mutually exclusive with every larger feature graph.
- [x] Reject incompatible profiles before settings and runtime setup side effects.
- [x] Complete and mutation-test the presentation dependency inventory.
- [x] Add exact capsule CI, expected compile failures, and release-binary smoke coverage.
- [x] Distinguish the source-built V0 artifact from published packaging and command fencing.
