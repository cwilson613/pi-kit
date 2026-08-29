# Sculpt-down modular composition design

## Layering

The monorepo uses four dependency directions:

1. Contract crates contain portable identities and request/response/status data.
2. Domain engine crates implement one optional capability and depend inward on
   contracts, never on the integration binary.
3. The `omegon` integration crate owns adapters, admission, setup, and host
   composition.
4. Executable and package profiles select additive domain and frontend features.

Existing crates should be reused when they already own a coherent contract or
engine. A new contract crate is justified when consumers must describe absence
without linking the engine's implementation graph.

## Artifact policy

Cargo features remain additive for code that belongs in one executable, such as
the terminal frontend. A stateful optional domain with an independent lifecycle
does not become a permanent `full` feature merely because Cargo can make it
optional. The Omegon package depends on portable contracts but never on the
codescan engine.

Release composition builds and installs the codescan extension as a separate
native executable. Source and minimal compositions may omit it. Always-built host
adapters expose stable tool schemas and typed unavailable behavior.

## First proof: codescan

`omegon-codescan-contracts` owns the versioned, serialized request, response,
status, and error protocol plus `SearchScope`, `ChunkType`, `SearchChunk`, and
`IndexStats`. `omegon-codescan` re-exports the domain types and retains indexing,
cache, parser, and search implementation. The `omegon` package depends only on
the contracts.

The `omegon-codescan` native extension owns one serial worker, one workspace
SQLite connection, freshness checks, and BM25 construction. It exposes a
versioned JSON-RPC service but no model-visible tools, avoiding ownership
collisions with host schemas. The extension loader owns process-group cleanup;
the service protocol owns request cancellation and graceful worker settlement.

The host captures the admitted extension RPC handle at boot. `codebase_search`,
`codebase_index`, and `request_context(kind="code")` all use that handle. Missing
extension, readiness failure, protocol mismatch, or retired transport yields
typed local unavailability without direct database fallback.

## Runtime diagnosis and replacement

`/doctor` and `/runtime doctor` share one read-only runtime health projection.
They inspect published inventory and supervisor state, then emit findings with
explicit recommended commands. Diagnosis never performs repair.

`/runtime replace <name>` is a one-shot process replacement, not a contribution
source update. The extension lifecycle owner invokes a control handle that
retains the admitted snapshot, manifest, secrets, and supervisor. Replacement
closes call admission, settles the old process tree, handshakes one candidate,
checks that its frozen tool shape is unchanged, and installs it behind the same
supervisor. Existing codescan and polling bindings therefore remain valid. A
failed replacement reaps the candidate and leaves only that extension
unavailable. Loading changed source bytes and publishing a new EventBus
generation remain separate future work.

## Evidence and rollout

The existing release remains the default product by packaging the extension with
the host. The Omegon dependency graph itself is the source and CI boundary: a
guard proves the engine is absent under default, no-default, and all supported
feature selections. Later slices will add artifact-specific budgets before
minimal packaging.

## Non-goals

- Renaming or publishing a minimal host executable in this slice.
- Removing memory, lifecycle, Git, compaction, dynamic contributions, ACP, or web.
- Moving EventBus or the agent loop into new crates before their stable ownership
  boundary is demonstrated.
- Supporting codescan through legacy HTTP/script plugins or a Rust dynamic library.
