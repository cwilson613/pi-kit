## 1. Composition identity
<!-- specs: kernel-composition/artifact-profiles -->

- [ ] Add failing tests for the current `feature:codescan` and `feature:codescan-adapter` disagreement.
- [ ] Make runtime output, fixtures, and lock validation use one adapter and sidecar ownership vocabulary.
- [ ] Refactor duplicated identity literals into the smallest existing authoritative owner.
- [ ] Run source composition tests and the focused Python suite.

## 2. Release archive inventory
<!-- specs: kernel-composition/artifact-profiles -->

- [ ] Add a failing archive test containing the required codescan manifest and executable.
- [ ] Add failing mutations for missing, duplicate, misplaced, and unexpected extension members.
- [ ] Update release inventory validation to accept only the exact declared sidecar members.
- [ ] Run package, manifest, and release composition tests.

## 3. Optional-domain evidence
<!-- specs: kernel-composition/artifact-profiles -->

- [ ] Add a failing proof test that exposes the retired in-process codescan markers.
- [ ] Replace the stale matrix row with native-extension absence and degradation evidence.
- [ ] Make the proof gate execute its referenced tests or repository-owned test command.
- [ ] Run the optional-domain isolation gate from a clean test process.

## 4. Pre-merge enforcement
<!-- specs: kernel-composition/artifact-profiles -->

- [ ] Add a failing workflow assertion for omitted composition and release-script tests.
- [ ] Add one maintained Python release-policy recipe to pull-request CI.
- [ ] Remove redundant one-off invocations after the consolidated gate is green.
- [ ] Run workflow tests, formatting, and changed-code lint gates.
