## 1. Composition identity
<!-- specs: kernel-composition/artifact-profiles -->

- [x] Add failing tests for the current `feature:codescan` and `feature:codescan-adapter` disagreement.
- [x] Make runtime output, fixtures, and lock validation use one adapter and sidecar ownership vocabulary.
- [x] Refactor duplicated identity literals into the smallest existing authoritative owner.
- [x] Run source composition tests and the focused Python suite.

## 2. Release archive inventory
<!-- specs: kernel-composition/artifact-profiles -->

- [x] Add a failing archive test containing the required codescan manifest and executable.
- [x] Add failing mutations for missing, duplicate, misplaced, and unexpected extension members.
- [x] Update release inventory validation to accept only the exact declared sidecar members.
- [x] Run package, manifest, and release composition tests.

## 3. Optional-domain evidence
<!-- specs: kernel-composition/artifact-profiles -->

- [x] Add a failing proof test that exposes the retired in-process codescan markers.
- [x] Replace the stale matrix row with native-extension absence and degradation evidence.
- [x] Make the proof gate execute its referenced tests or repository-owned test command.
- [x] Run the optional-domain isolation gate from a clean test process.

## 4. Pre-merge enforcement
<!-- specs: kernel-composition/artifact-profiles -->

- [x] Add a failing workflow assertion for omitted composition and release-script tests.
- [x] Add one maintained Python release-policy recipe to pull-request CI.
- [x] Remove redundant one-off invocations after the consolidated gate is green.
- [x] Run workflow tests, formatting, and changed-code lint gates.
