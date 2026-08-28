## 1. Portable codescan protocol
<!-- specs: kernel-composition/artifact-profiles -->

- [x] Add the lightweight codescan contract crate and preserve engine type re-exports.
- [x] Add versioned serialized operation, response, error, and cancellation DTOs.
- [x] Add stable wire-format and compatibility tests.

## 2. Native extension engine
<!-- specs: kernel-composition/artifact-profiles, runtime-contributions/lifecycle -->

- [x] Add the codescan native extension with one serial SQLite/index worker.
- [x] Implement search, index, readiness, cancellation, and graceful shutdown RPC behavior.
- [x] Add extension protocol, rollback, and lifecycle tests.

## 3. Host adapter
<!-- specs: kernel-composition/artifact-profiles, runtime-contributions/lifecycle -->

- [x] Bind host-owned codescan tools and code context to the admitted extension RPC handle.
- [x] Propagate caller cancellation and preserve typed unavailable behavior.
- [x] Remove every `omegon` dependency and direct source reference to the codescan engine.

## 4. Packaging and ratchets
<!-- specs: kernel-composition/artifact-profiles -->

- [x] Build, package, install, and identify the release-coupled codescan extension.
- [x] Enforce engine absence from every supported Omegon feature graph in CI.
- [x] Update contributor, architecture, codescan, and release documentation.

## 5. Verification
<!-- specs: kernel-composition/artifact-profiles, runtime-contributions/lifecycle -->

- [x] Validate the OpenSpec change and every observable scenario.
- [x] Run contracts, engine, extension, default Omegon, and no-default Omegon tests/checks.
- [x] Run changed-code lint, packaging tests, dependency guards, and landing gates.

## 6. Runtime doctor and replacement
<!-- specs: runtime-contributions/lifecycle -->

- [x] Add shared `/doctor` and `/runtime doctor` diagnostics with typed replacement recommendations.
- [x] Add one-shot `/runtime replace <name>` using the retained admitted snapshot and stable supervisor.
- [x] Preserve process-instance fencing, frozen capability shape, local failure, and process-tree cleanup.
- [x] Verify TUI, CLI, ACP, diagnosis-only, successful replacement, and failed replacement behavior.
