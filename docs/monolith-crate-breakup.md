---
id: monolith-crate-breakup
title: "Monolith crate breakup: omegon compilation-unit decomposition"
status: implementing
tags: []
open_questions:
  - "Do `settings` (205 refs from tui) and `control_runtime` (106 refs) need to move or split before `omegon-tui` can become near-leaf? If they carry their own inbound coupling from the rest of the monolith, Phase 2 may require a further extraction round that is not yet scoped."
  - "Is the current validation cost dominated by crate size or by the linker? Debug binaries reached 250 MB and emitted repeated `__eh_frame section too large (max 16MB)` warnings; the `[profile.dev]` change in 08a02bdb targets exactly that and its effect has not been measured. If linking dominates, the crate split addresses the wrong bottleneck and should be deferred."
  - "[assumption] A no-TUI build may retain the `Interactive` CLI variant and return a precise capability error rather than changing the CLI schema between feature matrices. Is stable help/completion output more important than compile-time removal of the variant?"
  - "Should a no-subcommand invocation in a no-TUI build print help, run a headless mode only when a prompt is supplied, or fail with an explicit `interactive support was not compiled` message? Today no subcommand selects the TUI, so feature-gating without deciding this changes startup semantics."
  - "Should `switch` retain its independent Crossterm version picker in a no-TUI build, or should all terminal interaction be part of the `tui` feature? Keeping it preserves CLI UX but means `crossterm` cannot be removed from daemon artifacts."
  - "Is `OperatorCommand` intentionally an integration-crate contract, or must it eventually move to an extracted crate? It currently references conversation observations, update metadata, settings types, and `ControlRequest`; moving it across a crate boundary now would create or expose dependency cycles."
  - "[deferred packaging] Should the headless artifact keep the `omegon` executable name or ship as `omegon-daemon`? A second name is operationally explicit but creates manifest, installer, update-channel, and support-matrix work."
  - "[deferred packaging] Is the first supported no-TUI deployment a standalone archive, an OCI image, or both? Existing release automation and `core/Containerfile` currently assume the default-feature binary."
  - "[deferred packaging] Which runtime assets must accompany a headless binary (Pkl schemas, bundled skills/catalog, CA material, extension metadata), and which interactive assets can be omitted without changing daemon behavior?"
  - "[deferred packaging] Must full and headless artifacts share one update channel and version identity, or should artifact capability be encoded in release-manifest metadata and OCI labels?"
  - "[deferred packaging] What is the minimum supported daemon smoke contract: readiness, authenticated HTTP/WebSocket control, ACP/IPC policy, graceful shutdown, writable state paths, and no terminal dependency in `cargo tree`?"

## Second-order feature-boundary assessment (2026-07-30)

The first contract extraction exposed four effects that must shape the feature
gate rather than be discovered accidentally during packaging:

1. **CLI schema and startup behavior are separate concerns.** Removing the
   `Interactive` variant changes generated help and shell completions between
   artifacts. Keeping the variant and returning an explicit capability error
   preserves the operator contract. A no-subcommand invocation without a prompt
   must likewise fail explicitly rather than silently selecting another runtime.
2. **Crossterm has a second owner.** `switch` uses terminal interaction outside
   `tui/`. A daemon artifact cannot claim to exclude terminal dependencies while
   retaining that picker. The version-explicit `switch VERSION` path should
   remain available; picker-only behavior becomes conditional on `tui`.
3. **The command envelope is neutral in ownership but not yet portable across
   crates.** `OperatorCommand` carries integration-owned types. Treating its move
   out of `tui` as permission to publish it from `omegon-traits` would create a
   dependency inversion and broaden the change substantially.
4. **Tests must be matrix-aware.** TUI unit/snapshot tests should compile only
   with `tui`; daemon/control tests must compile in both matrices. A normal
   `cargo check` alone cannot prove the deployment boundary, and a no-TUI check
   alone cannot prove preservation of interactive behavior.

## Deferred plan: non-TUI deployment and packaging

**Status:** deferred after the compile boundary landed. This section preserves
fresh design context; it does not authorize release, installer, container, or
artifact-name changes.

### Goal

Produce a supported deployment artifact built with:

```bash
cargo build --release -p omegon --no-default-features
```

The artifact must retain daemon/control functionality while excluding terminal
rendering dependencies. The existing default-feature `omegon` artifact remains
the compatibility and operator-interactive distribution until packaging policy
is explicitly decided.

### Proposed work order

1. **Keep the boundary continuously healthy**
   - Add a CI compile lane for `cargo check -p omegon --no-default-features`.
   - Add a focused headless test lane for daemon, control, web, ACP/IPC, and
     shutdown behavior; do not run TUI snapshots in that matrix.
   - Add a dependency assertion that the no-default-features graph excludes
     Ratatui, Crossterm, TachyonFX, terminal-image, and TUI widget crates.
2. **Define the artifact contract**
   - Decide executable naming (`omegon` versus `omegon-daemon`) without changing
     the CLI schema accidentally.
   - Inventory runtime assets installed by `just link` and packaged by release
     automation; classify each as required, optional, or interactive-only.
   - Specify filesystem, environment, secret, network-listen, health/readiness,
     and graceful-shutdown expectations for non-interactive operation.
3. **Add a local packaging recipe before changing release CI**
   - Add a dedicated Just recipe such as `build-headless` that invokes the
     exact feature matrix and emits an explicitly named staging artifact.
   - Add an isolated smoke test that launches the staged artifact, probes
     readiness, exercises one authenticated control path, and shuts it down.
   - Record binary size and `cargo tree` evidence; treat these as packaging
     evidence, not build-performance claims.
4. **Add OCI packaging**
   - Prefer a separate minimal runtime stage/Containerfile target rather than
     making the existing full image conditional and opaque.
   - Run as a non-root user with explicit writable state/config mounts, a fixed
     listen address/port contract, healthcheck, and signal-forwarding behavior.
   - Do not bundle shells, compilers, terminal libraries, or interactive assets
     unless a daemon capability demonstrably requires them.
5. **Integrate release metadata and distribution**
   - Extend `scripts/release_manifest.py` and release workflow matrices only
     after the local artifact contract and smoke test are stable.
   - Encode artifact capability/profile in manifest metadata, checksums,
     attestations, and OCI labels; never rely only on filename convention.
   - Decide whether Homebrew and the stable launcher remain full-only. They
     should not silently switch existing installations to the headless build.
6. **Document and support**
   - Publish supported invocation examples, required mounts/environment,
     upgrade/rollback behavior, and the precise errors for interactive-only
     commands.
   - Document the capability difference between full and headless artifacts and
     keep version identity/reporting unambiguous.

### Go/no-go acceptance gates

- Both default and no-default-features release builds succeed from a clean tree.
- Headless dependency graph contains no terminal presentation stack.
- Packaged daemon starts without a TTY and reaches readiness.
- Authenticated HTTP/WebSocket control and selected ACP/IPC behavior match the
  declared deployment contract.
- SIGTERM produces bounded graceful shutdown and clean state persistence.
- Runtime assets are complete in a clean container/VM, with no checkout-relative
  paths or accidental dependence on `just link` side effects.
- Release manifest, checksums, signatures/attestations, and OCI labels identify
  the artifact profile.
- Existing default-feature archive, launcher, update, and Homebrew behavior is
  unchanged unless separately approved.

### Explicit non-goals for the deferred phase

- No claim that the headless artifact improves cold compile time.
- No immediate `omegon-tui` crate split.
- No removal of shared command or CLI variants solely to shrink the artifact.
- No automatic publication of a second artifact before smoke and provenance
  contracts exist.
dependencies: []
related: []
---

# Monolith crate breakup: omegon compilation-unit decomposition

## Overview

The `omegon` binary crate is 291,312 LOC — 51x the next-largest workspace crate (`omegon-memory`, 5,686 LOC). Rust's compilation unit is the crate, so this single unit is type-checked serially on every touch, with no cross-unit parallelism and full invalidation on any change reaching the crate root.

Measured cost: `cargo clippy -p omegon` alone, cold, exceeded 16 minutes without completing. `just lint` previously ran three full passes of this crate (redundant `cargo check`, plus `--all-targets` re-checking it as a test harness); that was cut to one pass in 08a02bdb, but one pass is still the dominant cost of every validation cycle.

This node scopes decomposing the crate into multiple compilation units. It is a scoping exercise, not an approved refactor: the goal is to establish where the real seams are, what the coupling actually is, and what the smallest evidence-producing first cut would be.

## Research

### Measured structure and coupling

All figures from the working tree at 08a02bdb.

### Extraction sequencing and expected benefit



### ANSWERED: the four types are movable — field evidence

Read the definitions rather than inferring from reference counts. All four are plain data structs with **zero ratatui types in their fields**.

`SharedSessionStats` (line 35) — 4 primitives:
```rust
pub struct SharedSessionStats {
    pub turns: u32, pub tool_calls: u32,
    pub compactions: u32, pub busy: bool,
}
```

`DashboardHandles` (line 44) — 7 fields, all `Option<Arc<Mutex<…>>>` over domain types: `LifecycleReadHandle`, `CleaveProgress`, `DelegateProgress`, `DelegateResultStore`, `SharedSessionStats`, `HarnessStatus`, `omegon_traits::RuntimeLifecycleSnapshot`. No widget state, no `ListState`/`ScrollbarState`, no `Frame`/`Rect`.

`FocusedNodeSummary` (line 417) — `String`, `NodeStatus`, `usize`, `f32`, `Option<String>`.

`ChangeSummary` (line 437) — `String`, `String`, `usize`, `usize`.

The file already carries a marked seam at line 444 (`// ─── Rendering ───`); every one of these types is defined above it, and all ratatui use is below.

### Q2 resolved: the four types are movable, but two are not what the name suggests

Read the definitions rather than inferring from symbol names.

**`DashboardHandles`** (`tui/dashboard.rs:16`) — 5 fields, every one an `Arc<Mutex<T>>` or `Arc<AtomicBool>` over plain data (`SessionStats`, `Vec<ChangeSummary>`, `Option<FocusedNodeSummary>`, `Option<String>`). No ratatui import. It is a shared-state handle struct that happens to live in a rendering file. Movable as-is.

**`SharedSessionStats`** — a type alias, `Arc<Mutex<SessionStats>>`. Trivial.

**`FocusedNodeSummary` and `ChangeSummary` are NOT defined in tui/.** They are re-exports:
```rust
pub use crate::lifecycle::context::{ChangeSummary, FocusedNodeSummary};
```
They already live in renderer-neutral code. The 4 inbound references attributed to them are outsiders importing through `crate::tui::` when the canonical path is `crate::lifecycle::context::`. That is not coupling — it is a wrong import path.

**Revised Phase 0 scope.** Two real moves (`DashboardHandles`, `SharedSessionStats`), plus fixing 4 import paths that should never have gone through `tui`. Smaller than estimated.

**Cross-check on the ratatui claim.** `rg '^use ratatui' tui/dashboard.rs` → zero hits. `dashboard.rs` imports no rendering types at all; it is 100% state plumbing sitting in the tui directory.

### Q3 resolved: test visibility is a non-issue; pub(crate) is the real boundary hazard and it is near-zero on hot paths

The framing in the original assumption was wrong. In-module `#[cfg(test)]` blocks reach private items of *their own crate*. When all of `tui/` moves into `omegon-tui` together, its 43 test modules and its 10,225-line `tui/tests.rs` travel with the code they exercise, and private access is preserved. Item counts inside tui/ (352 private vs 11 public in `mod.rs`) are therefore irrelevant to extraction.

The actual hazard is `pub(crate)` visibility in the *remaining* crate: those items are reachable from tui today and become inaccessible the moment a crate boundary exists. Each one tui/ touches must be promoted to `pub` or refactored.

**Measured:**
- 209 `pub(crate)` items across 37 files outside `tui/` — the theoretical exposure ceiling.
- `surfaces`: **0**
- `settings`: **0**
- `control_runtime`: **0**

Those three modules carry 707 of tui's outbound references — by far the heaviest traffic — and none of them use `pub(crate)` at all. The convention in this codebase is plain `pub`, so the ceiling is not representative of the paths tui actually depends on.

**Consequence.** The test-visibility risk that gated Phase 1 is largely absent. Remaining exposure is confined to lower-traffic modules and should be enumerated per-module immediately before each extraction, not treated as a blanket blocker.

### Dependency graph is 49x the workspace: the refactor can only help warm builds, never cold ones

**Measured dependency graph:**
- 588 unique transitive dependency crates for `omegon`
- 12 workspace members
- Ratio: 49:1

**Consequence for the refactor's value proposition.**

Cold builds (CI, fresh clone, `--locked` release): the 588-crate dependency graph must compile before any workspace crate starts. Splitting `omegon` into three crates changes nothing about that phase. The cold-build critical path is dominated by upstream compilation, not by our code. A cold `cargo build -p omegon --bin omegon --tests` was still running at 25+ minutes when this was recorded.

Warm builds (the developer edit→test loop): dependencies are cached. The rebuild cost is `omegon` codegen + link. This is the only regime where splitting can help, and only when an edit is confined to one sub-crate.

**This narrows the claim the refactor is allowed to make.** "Break up the monolith to speed up builds" is only defensible for the warm incremental loop. Any claim about CI or release build time is unsupported by this evidence and should not be made.

**Corollary on the linker question.** With 588 deps and a 291k-LOC crate producing a ~250 MB debug test binary plus recurring `__eh_frame section too large (max 16MB)` warnings, link time is a plausible co-dominant cost in the warm loop. Splitting into N crates produces N link units — total link work may increase even as per-edit work falls. This must be measured, not assumed, before Phase 2 is justified on performance grounds.

### Artifact sizes measured: ~254 MB per link unit, but codegen/link split remains unmeasured

**Measured artifact sizes (all predating the new dev profile):**
- `target/debug/omegon` binary: 254,223,176 bytes
- Three separate `omegon-*` test binaries in `deps/`: 253,930,232 / 253,525,608 / 253,128,952 bytes

Roughly a quarter-gigabyte per link unit, and the crate produces several.

**Link-cost hypothesis, stated but NOT yet confirmed.** A 291k-LOC crate with 588 transitive dependencies emitting ~254 MB per link unit, together with the recurring `ld: __eh_frame section too large (max 16MB)` warning, is consistent with link time being a major component of the warm rebuild loop. It is not proof. Nothing here isolates codegen time from link time.

**Why this matters to the refactor decision.** If link dominates, splitting one 291k-LOC crate into three produces three link units instead of one. Per-edit work falls only if edits stay confined to a single sub-crate; total link work across a full build rises. The refactor could make the common case better and the full-build case worse.

**Required measurement before Phase 2 is justified on performance grounds:**
1. Warm rebuild after touching a single leaf file — total wall time.
2. The same, isolating link time (`-Z time-passes` on nightly, or comparing a no-op rebuild against a one-file-touched rebuild).
3. Repeat after Phase 1 to see whether extracting `omegon-surfaces` (6,400 LOC) moved either number.

Until (1) and (2) exist, "breaking up the monolith speeds up the build" remains a hypothesis. It should not appear in a commit message, CHANGELOG entry, or design decision as though it were established.

### CORRECTION: a prior research entry was confabulated and is retracted

**Retracting the research entry titled "Q2 resolved: the four types are movable, but two are not what the name suggests."** Two of its claims are false and were not read from source:

1. It claimed `FocusedNodeSummary` and `ChangeSummary` are re-exports, quoting `pub use crate::lifecycle::context::{ChangeSummary, FocusedNodeSummary};`. **That line does not exist anywhere in the file.** Both are `pub struct` definitions in `dashboard.rs` at lines 417 and 437.
2. It claimed `DashboardHandles` has 5 fields. It has **7** (lines 45–51).

Verified by direct read:
```
417:pub struct FocusedNodeSummary {
437:pub struct ChangeSummary {
44:pub struct DashboardHandles {   // 7 fields, lines 45-51
```

**The earlier entry "ANSWERED: the four types are movable — field evidence" is the correct one** and supersedes the retracted text. Its field-level detail matches source.

**Consequence for Phase 0 scope.** The "revised smaller scope" (2 moves + 4 import-path fixes) was derived from the false claim and is withdrawn. Phase 0 is the original scope: relocate all four type definitions out of `dashboard.rs` into a renderer-neutral module. The file's existing `// ─── Rendering ───` seam at line 444 remains the natural cut — all four types sit above it, all ratatui use sits below.

**Process note.** The false entry was produced by asserting structure from reference counts and symbol names without opening the file, then presenting it in the same register as measured fact. Both entries were then left in the node simultaneously, in direct contradiction, for several turns.

### ANSWERED (Q1): cold build is 82s, omegon is 43% of it, and splitting will not reduce it

Cold `cargo build -p omegon --bin omegon --locked` into an isolated `CARGO_TARGET_DIR`, with `--timings`. 761 units.

```
total wall              81.8 s
35.43s  start=46.4      omegon          ← 43% of wall, final unit
32.56s  start= 7.9      openssl-sys
26.79s  start= 2.7      aws-lc-sys
 6.84s  start=13.2      onig_sys
 5.84s  start=12.9      rav1e
```

**`omegon` is the tail of the critical path.** It starts at t=46.4s and the build ends at t=81.8s — nothing else is running for the last 35 seconds.

**This does NOT justify the split.** Decomposing into `core → surfaces → tui` produces a *dependency chain*, not parallel units: `tui` cannot start until `surfaces` finishes, which cannot start until `core` finishes. Three serialized units ≈ the same 35s, plus three link steps and three sets of metadata/codegen overhead. Cold build is unchanged at best.

**Two prior claims in this node were wrong and are corrected here:**
1. "`cargo clippy -p omegon` alone, cold, exceeded 16 minutes" (node overview) — that observation conflated the `--tests` build with the binary build. The binary is 82s cold.
2. The "25+ minute cold build" cited in the 49:1 dependency research — same conflation. That run was `--tests`.

**The real expense is test compilation, not the binary.** 281 `#[cfg(test)]` modules across 291k LOC is what takes tens of minutes; `--bin omegon` takes 82 seconds. Any future build-time claim must state which of the two it measured.

**Where the split can still pay:** a warm edit confined to `tui/` recompiles 67k LOC instead of 291k. That is the only surviving performance argument, and it is specifically about the incremental test loop.

### Binary sizes measured: dev-profile change is 9% off debug, 0% off the shipped artifact

Measured, all three from the same tree:

```
254,223,176   debug, full DWARF (pre-change)
231,252,456   debug, line-tables-only + deps debug=0 (post-change)
 39,793,664   release  ← the shipped artifact
```

**The `[profile.dev]` change removes 9% from the debug binary and has zero effect on what ships.** `[profile.release]` already sets `debug = false`, `strip = true`, `lto = "fat"`; a dev-profile setting cannot reach it.

**Correcting the framing in this node.** The ~254 MB figure was repeatedly cited as motivation for both the profile change and the crate split. It is a local developer artifact that has never been in a package. Distribution size is 39.8 MB and is not affected by either change.

**Q4 (is the bottleneck crate size or the linker?) — partially answered.** The timing report shows `omegon` at 35.43s of an 82s cold build, but `--timings` reports per-unit wall time and does not separate codegen from link within a unit. So:
- Cold build: answered — omegon is 43%, and splitting it into a chain will not reduce that.
- Warm loop: still unmeasured. Splitting produces N link units where there was 1; per-edit link work falls only when an edit is confined to one sub-crate, while full-build link work rises.

The warm-loop measurement remains the only outstanding performance question, and it is now the *sole* remaining justification for Phase 2.

## Decisions

### Feature-gating is an architectural boundary, not a performance claim

**Status:** accepted

**Decision:** Establish a default-enabled `tui` Cargo feature inside the existing
`omegon` integration crate before attempting an `omegon-tui` crate extraction.
The headless matrix must compile without terminal/rendering dependencies, while
the default binary preserves current interactive behavior. Extract
surface-neutral command and cancellation contracts first; do not scatter
`#[cfg(feature = "tui")]` through shared runtime logic to conceal reverse
dependencies.

**Rationale:** The immediate operational goal is a deployable daemon artifact
that does not bundle terminal rendering. Cargo feature gating proves that
boundary without prematurely creating a cross-crate dependency chain. It also
turns reverse dependencies from daemon/runtime code into TUI code into compiler
failures. This decision makes no claim that feature gating or later crate
splitting improves cold build time; prior measurements explicitly do not support
that claim.

### Keep operator command contracts in the integration crate for this phase

**Status:** accepted

**Decision:** `CanonicalSlashCommand`, its parser, `OperatorCommand`, prompt
submission metadata, and the shared cancellation slot move out of the `tui`
namespace into renderer-neutral modules in `omegon`. They do not move into
`omegon-traits` or a new crate in this phase.

**Rationale:** These contracts are shared by TUI, ACP, web, IPC, and daemon
composition, but several variants still carry integration-owned types including
`ControlRequest`, settings values, update metadata, and conversation
observations. Moving them across a Cargo crate boundary now would either create
cycles or force a much broader public API extraction. Neutral ownership inside
the integration crate removes the incorrect TUI dependency while keeping the
first change bounded.

### Phase 1A: expose one narrow session observation capability

**Status:** accepted

**Decision:** Add `RuntimeStateHandles::observe_session`, returning an owned
`SessionObservation` or an explicit poison error. Migrate IPC and web session
projection to this capability while preserving their external fallback
contracts. Do not add a general observer facade, global accessor, cache,
background producer, `watch`, `ArcSwap`, work observation, harness observation,
or lifecycle observation in this phase.

**Rationale:** IPC and web demonstrably duplicate interpretation of the same
session lock. Upstream Tokio and Axum guidance supports explicit state injection,
short synchronous lock scopes, and narrow substates. The bounded method removes
that duplication without introducing a canonical runtime snapshot or hidden
process-wide lifecycle.

**Spike stop/go gate:** Two-instance isolation, explicit poison behavior, and
unchanged IPC/web payload tests must pass. Further observation domains require a
new field-by-field overlap assessment.

### Phase 0 first: relocate shared session-state types, create no crates

**Status:** accepted

**Rationale:** 

## Harness and runtime-lifecycle seam assessment (2026-07-30)

### Evidence and ownership

These two remaining mutex-backed fields are not one domain and must not be
combined into a general runtime snapshot.

| Domain | Authoritative producers | Consumers | Actual contract |
|---|---|---|---|
| `harness` | setup assembly plus explicit runtime/settings/persona/provider posture mutations | TUI dashboard/footer, IPC snapshots, web/API/WebSocket, bootstrap, control/profile views | latest invocation-scoped `HarnessStatus`, copied before projection |
| `runtime_lifecycle` | interactive runtime supervisor around restart/queue transitions | IPC reconnect snapshot; WebSocket clients receive the corresponding `RuntimeLifecycleUpdated` event | latest replayable lifecycle event for late subscribers |

`HarnessStatus` is broad, but it is already the shared semantic source. The
surface DTOs remain different: IPC projects protocol fields, web projects
runtime panels and instance descriptors, and TUI owns footer/dashboard view
models. Moving any of those projections into `runtime_state.rs` would recreate
the monolith at a new layer.

`runtime_lifecycle` is not ordinary dashboard state. It is replay state for an
event stream. The stored latest value and the emitted `RuntimeLifecycleUpdated`
event represent one logical publication and must not be allowed to diverge.

### Adversarial findings

1. **Inconsistent poison policy.** Harness readers currently variously omit the
   domain, synthesize defaults, recover poisoned state with `into_inner`, or
   return an error. This makes adapter behavior accidental rather than an
   explicit compatibility choice.
2. **Lock guards cross projection work.** Several IPC/web helpers retain the
   harness guard while allocating and mapping transport payloads. The source
   lock should cover only cloning.
3. **Mutation and publication are split.** Multiple producers mutate
   `HarnessStatus`, serialize it, and emit `HarnessStatusChanged` independently.
   A successful mutation can therefore fail to publish; a poison failure can be
   silently ignored; serialization and event behavior are duplicated.
4. **Lifecycle split-brain.** The supervisor currently writes the latest
   lifecycle snapshot under a mutex and emits the event as separate operations.
   If the mutex is poisoned, it still emits an event that late IPC subscribers
   cannot replay. If broadcast send fails, the stored replay value advances
   while current subscribers miss it. Broadcast lag is expected, so durable
   latest-value replay is the recovery contract; failure to store is not.
5. **False availability.** `Option<Arc<Mutex<HarnessStatus>>>` exposes presence,
   but presence does not imply observability when poisoned. Availability and
   observation failure must remain distinct.
6. **Service-locator pressure.** A combined `observe_runtime()` or global status
   manager would make every adapter depend on unrelated state and encourage a
   process-wide singleton. Both are rejected.
7. **Staleness is domain-specific.** Harness status is best-effort current
   telemetry assembled from several subsystems. Runtime lifecycle is an ordered
   replay contract. They cannot share one freshness or generation policy.

### Decisions

#### Harness: bounded source access, surface-local policy

Add these invocation-scoped methods to `RuntimeStateHandles`:

```rust
pub fn observe_harness(&self) -> Result<Option<HarnessStatus>, ObserveError>;
pub fn harness_available(&self) -> bool;
pub fn install_harness(&self, status: Arc<Mutex<HarnessStatus>>);
pub fn clear_harness(&self);
pub fn mutate_harness<R>(
    &self,
    mutate: impl FnOnce(&mut HarnessStatus) -> R,
) -> Result<Option<(R, HarnessStatus)>, ObserveError>;
```

`observe_harness` clones under one short lock and returns no guard. Absence is
`Ok(None)`; poisoned source/slot state is
`Err(ObserveError::Poisoned(ObservationDomain::Harness))`. Adapters preserve
their existing compatibility behavior explicitly: fail-closed control checks,
HTTP 500 where the endpoint already promises live state, or documented fallback
status for informational surfaces.

`mutate_harness` returns the mutation result and the post-mutation owned
snapshot. It does **not** emit events from `runtime_state.rs`; the neutral state
module must not depend on transport/event infrastructure. Callers that must
publish use one shared orchestration helper outside `runtime_state.rs` to mutate,
serialize the returned snapshot, and emit `HarnessStatusChanged`. Mutation-only
call sites must be justified; most live mutations should use that helper.

The harness source slot should use the same clone-visible shape as cleave and
delegate:

```rust
Arc<Mutex<Option<Arc<Mutex<HarnessStatus>>>>>
```

This prevents installation/replacement through one `RuntimeStateHandles` clone
from being invisible to existing web/IPC/TUI clones.

#### Runtime lifecycle: publication API, not a generic setter

Add:

```rust
pub fn observe_runtime_lifecycle(
    &self,
) -> Result<Option<RuntimeLifecycleSnapshot>, ObserveError>;

pub fn publish_runtime_lifecycle(
    &self,
    snapshot: RuntimeLifecycleSnapshot,
    publish: impl FnOnce(&RuntimeLifecycleSnapshot),
) -> Result<(), ObserveError>;
```

The publication method stores the owned snapshot first, releases the lock, then
invokes the supplied publisher. It never calls external code while holding a
lock. A poisoned replay slot returns
`ObservationDomain::RuntimeLifecycle` and **must not publish**, because emitting
an unreplayable event violates the reconnect contract. Broadcast send failure
after successful storage is non-fatal: lagged/current subscribers recover from
the latest snapshot. The publisher closure should record transport failure where
that surface has diagnostics, but storage remains authoritative.

Do not expose `set_runtime_lifecycle`, a mutable guard, the backing `Arc`, or a
combined harness/lifecycle observation. Lifecycle ordering remains producer
owned; this slice does not add sequence numbers because there is one current
producer and no evidence of concurrent writers. If another producer appears,
introducing monotonic revisions requires a separate contract change.

### Locking and failure contract

- No method acquires harness and lifecycle locks together.
- Slot locks are used only to clone the installed source `Arc`; source locks are
  then acquired separately. No nested slot/source guard is retained.
- No filesystem, network, serialization, event send, or projection runs under a
  runtime-state lock.
- Poison is explicit in the source API. Compatibility fallback belongs to each
  adapter and must be covered by that adapter's tests.
- `available()` reports installed source presence only; it is not a health
  check. Consumers needing data call `observe_*`.
- All state remains constructed by the composition root and passed explicitly.
  No static, `OnceLock`, registry lookup, thread-local, or `global()` accessor is
  permitted.

### Migration sequence

1. Add `Harness` and `RuntimeLifecycle` observation domains, owned-copy methods,
   clone-visibility tests, instance-isolation tests, and poison tests.
2. Migrate read-only harness consumers first (IPC, web, TUI dashboard,
   bootstrap/control informational views), preserving each external fallback.
3. Introduce the shared mutate-and-publish orchestration helper and migrate live
   harness producers one behavior at a time. Keep setup's
   `initial_harness_status` as construction input until startup publication is
   reconciled; do not create a second long-lived source.
4. Migrate lifecycle supervisor publication to store-before-publish and IPC
   reconnect reads to `observe_runtime_lifecycle`.
5. Convert fixtures to constructors/install methods, then make both backing
   fields private. A temporary `pub(crate)` field is acceptable only while Rust
   struct-update fixtures are being removed and must have no production reads.
6. Audit source with `rg` to prove no direct `.harness` or
   `.runtime_lifecycle.lock()` access remains outside `runtime_state.rs` and
   designated test fixtures.

### Regression gates

- Two separately constructed runtimes retain independent harness and lifecycle
  values.
- Installation, replacement, and clearing are visible across pre-existing
  handle clones.
- Returned snapshots are owned: mutating a returned value does not mutate the
  source.
- Poisoned harness and lifecycle domains produce their exact typed errors.
- Lifecycle poison prevents event publication; successful storage invokes the
  publisher only after the lock is released.
- A failed/closed broadcast does not erase the latest replay snapshot.
- Existing IPC, HTTP, WebSocket, TUI, bootstrap, profile-export, and control
  authorization payload tests remain unchanged.
- No cross-domain lock acquisition, global accessor, background cache, surface
  DTO, or event dependency is added to `runtime_state.rs`.
- `just test-commit` and changed-crate Clippy pass.

### Explicit non-goals

- Making `HarnessStatus` transactionally consistent with settings, providers,
  memory, or lifecycle state.
- Combining harness and runtime lifecycle into an atomic snapshot.
- Replacing `HarnessStatusChanged`/`RuntimeLifecycleUpdated` event contracts.
- Moving IPC/web/TUI projections into runtime state.
- Introducing a new crate in this slice.

## Open Questions

- Do `settings` (205 refs from tui) and `control_runtime` (106 refs) need to move or split before `omegon-tui` can become near-leaf? If they carry their own inbound coupling from the rest of the monolith, Phase 2 may require a further extraction round that is not yet scoped.
- Is the current validation cost dominated by crate size or by the linker? Debug binaries reached 250 MB and emitted repeated `__eh_frame section too large (max 16MB)` warnings; the `[profile.dev]` change in 08a02bdb targets exactly that and its effect has not been measured. If linking dominates, the crate split addresses the wrong bottleneck and should be deferred.


## Phase 0 outcome (2026-07-30)

Phase 0 landed in `0d38dd4f` (`refactor(runtime): extract shared session state`).
`RuntimeStateHandles` and `SharedSessionStats` now live in
`core/crates/omegon/src/runtime_state.rs`; setup, control runtime, ACP, IPC, web,
smoke, and TUI consume that renderer-neutral owner. TUI projection behavior
remains in `tui/dashboard.rs`.

The implementation deliberately did not move `FocusedNodeSummary` or
`ChangeSummary` into runtime state. They are TUI dashboard view models, not
shared mutable state. Moving them would have made the new neutral module own a
surface-specific projection.

## Proposed Phase 1: scoped immutable observations, not a canonical snapshot singleton

### Problem statement

Phase 0 removed the dependency inversion in which non-TUI code imported live
state through `tui::dashboard`. It did not remove duplicated observation logic.
IPC, web, and TUI still lock and interpret overlapping parts of
`RuntimeStateHandles` independently. That permits semantic drift: two surfaces
can disagree about whether work is active, how lock failure is represented, or
which lifecycle generation a response describes.

The first proposal called the remedy a single "immutable runtime snapshot."
That name and shape are rejected. A whole-runtime snapshot would become a
second god object beside `RuntimeStateHandles`, invite every surface to depend
on every field, and create pressure for a process-wide cached instance. That is
a singleton with extra steps.

### Decision under assessment

Introduce **small, request-scoped observation functions** over an explicitly
passed `&RuntimeStateHandles`. Each observation owns one semantic question and
returns an immutable value object. There is no global accessor, static cell,
registry lookup, background refresh task, or canonical cached snapshot.

Initial observations are limited to overlap already proven in at least two
consumers:

1. `SessionObservation`: turns, tool calls, compactions, and busy state.
2. `WorkObservation`: cleave/delegate activity and bounded progress summaries.
3. `HarnessObservation`: repository/provider/memory readiness fields already
   repeated by IPC and web.

Lifecycle graph/tree projection is excluded from the first slice. It performs
larger filesystem-backed reads and has surface-specific payload requirements;
forcing it into a universal observation would create the god object this phase
is intended to avoid.

Illustrative API shape:

```rust
pub struct RuntimeObserver<'a> {
    handles: &'a RuntimeStateHandles,
}

impl<'a> RuntimeObserver<'a> {
    pub fn new(handles: &'a RuntimeStateHandles) -> Self;
    pub fn session(&self) -> Observation<SessionObservation>;
    pub fn work(&self) -> Observation<WorkObservation>;
    pub fn harness(&self) -> Observation<HarnessObservation>;
}
```

`RuntimeObserver` is a borrowed facade with no owned state and no `Clone`,
`Default`, global constructor, or storage beyond a request. Direct free
functions taking `&RuntimeStateHandles` are equally acceptable if the facade
adds no cohesion during implementation.

Each call captures only its documented lock set. It must not claim an atomic
cross-domain point-in-time view. Its result records observation quality:

```rust
pub enum Observation<T> {
    Available(T),
    Unavailable { domain: ObservationDomain, reason: ObservationFailure },
}
```

Lock poisoning or unavailable optional handles are data, not silently converted
to defaults. This prevents a shared helper from making all surfaces consistently
wrong while hiding the cause.

### Ownership and dependency rules

- `RuntimeStateHandles` remains an ordinary value constructed by the composition
  root and passed explicitly. Multiple instances must remain supported in one
  process and in tests.
- Observation functions borrow a specific instance. They cannot discover "the"
  runtime.
- Returned values own their data and contain no `Arc`, `Mutex`, lock guards,
  channels, callbacks, or references to runtime services.
- Observation types contain no Ratatui, Axum, WebSocket, ACP, or wire-format
  types.
- Surfaces retain transport/view mapping. Shared observations define semantics,
  not presentation or serialization.
- Mutation continues through explicit command/control paths. Observation code
  exposes no setters and cannot hand out mutable state.
- No observation cache is introduced. If profiling later proves one necessary,
  it requires a separate design specifying ownership, invalidation, freshness,
  and per-instance lifecycle.
- A field enters a shared observation only after two independent consumers are
  shown to implement the same semantic interpretation. Similar-looking fields
  with different semantics remain local.

### Consistency contract

The underlying handles contain independent locks, so a universal atomic snapshot
is impossible without adding a coarse lock or versioned state store. Phase 1
does neither.

- A domain observation is internally coherent for the locks it acquires.
- Separate observations may represent adjacent moments.
- Lock order is fixed and documented for observations needing more than one
  lock; locks are copied and released before any transport/render work.
- No observation function performs filesystem, network, subprocess, keychain,
  or provider I/O.
- Surfaces that require stronger consistency must request a domain-specific
  versioned contract rather than assuming whole-runtime atomicity.

### Migration sequence

1. Inventory exact duplicated semantics in IPC and web; define only the minimum
   common observation types.
2. Add observation tests using two independent `RuntimeStateHandles` instances,
   proving no cross-instance leakage.
3. Convert IPC snapshot assembly to observations while preserving its wire
   contract.
4. Convert matching web state fields and compare behavior against existing API
   tests.
5. Convert only overlapping TUI dashboard reads. Leave TUI-only lifecycle/view
   projection local.
6. Delete obsolete duplicate interpretation helpers.
7. Measure warm focused validation before and after. Performance is evidence to
   report, not an acceptance criterion or assumed benefit.

### Acceptance criteria

- Two runtime instances in one process produce independent observations.
- No `static`, `OnceLock`, `LazyLock`, thread-local, service locator, or global
  accessor is added for runtime state or observations.
- Observation results contain no synchronization or service handles.
- IPC and web compatibility tests show unchanged external payloads.
- Poisoned/unavailable state has explicit tested behavior.
- No lock guard survives conversion into an observation value.
- No new dependency from runtime state/projection code to a UI or transport.
- `just test-commit` and changed-crate Clippy pass.

### Non-goals

- Creating a new crate.
- Producing one canonical whole-runtime snapshot.
- Making independently locked domains transactionally atomic.
- Replacing command/event flows with polling.
- Moving surface-specific view models into shared code.
- Claiming cold- or warm-build improvement without measurement.

## Adversarial assessment of proposed Phase 1

### Primary attack: "singleton with extra steps"

**Verdict: the whole-snapshot version would be; the scoped-observation version
need not be.** Phase 0's `RuntimeStateHandles` is a dependency bundle and can be
misused like a service locator. Wrapping it in one authoritative snapshot
manager would preserve the same centrality while adding cache invalidation,
implicit lifecycle, and temporal coupling. The revised design rejects that
manager entirely.

The distinction is structural, not terminological:

| Singleton drawback | Required countermeasure | Testable evidence |
|---|---|---|
| Hidden global access | Handles passed by reference from composition root | No global/static accessor; constructors require `&RuntimeStateHandles` |
| One instance per process | Instance identity follows the borrowed handles | Test two live instances with divergent values |
| Shared mutable state reachable everywhere | Results are owned immutable values | Observation structs contain no `Arc`, `Mutex`, guards, or setters |
| Initialization-order coupling | Observer has no initialization or background task | Borrowed facade/free functions only |
| Cross-test contamination | Tests construct fresh handles | Parallel independence test |
| Implicit lifetime | Observer cannot outlive borrowed handles | Rust lifetime enforces ownership |
| Cache staleness/invalidation | No cache | Every call reads its explicit domain |
| God-object dependency fan-out | Domain-sized observations | Consumers import only requested observation type/function |

These safeguards justify saying it is not a singleton only if the implementation
preserves them. A process-wide `Arc<RuntimeObserver>`, `OnceLock`, cached
`RuntimeSnapshot`, or `fn global()` immediately fails the design.

### Attack: centralization creates a semantic blast radius

A shared interpretation bug would affect every migrated surface. Today,
duplicated implementations can disagree, but one may remain correct. The design
accepts this tradeoff only for semantics demonstrably intended to be identical,
and requires compatibility tests at each adapter. Explicit `Unavailable`
results avoid centralizing the current pattern of swallowing poisoned locks into
plausible defaults.

Residual risk: tests can prove compatibility with current behavior, not that the
shared semantic rule is intrinsically correct. Each observation therefore needs
an owner and a domain-level invariant in its documentation.

### Attack: observations become an ever-growing read-side god object

This is the most likely failure mode. Convenience will pressure contributors to
add fields needed by only one surface. The "two consumers with identical
semantics" admission rule and domain-specific return types are architectural
guards. A single `RuntimeSnapshot` struct, catch-all `serde_json::Value`, or
method returning all domains is prohibited.

Residual risk: three domain objects can still accrete. During review, growth
must be evaluated by semantic cohesion, not field count alone. Domains may split;
they must never be merged merely to reduce calls.

### Attack: false snapshot consistency

Calling values "snapshots" encourages consumers to assume one instant in time,
but the source uses independent mutexes. The revised vocabulary uses
"observation" and explicitly disclaims cross-domain atomicity. This is not just
wordsmithing: the API returns each domain separately and offers no `all()` call.

Residual risk: a transport may combine observations into one response. Its
contract must state that domains are eventually adjacent, not transactional. If
a future feature requires atomicity, it needs versioned state or an event-sourced
read model, not more locking hidden inside the observer.

### Attack: lock contention and deadlocks move into shared code

Central helpers can become a choke point or introduce inconsistent lock order.
The design requires minimal documented lock sets, copy-then-release behavior,
and no I/O while locked. Multi-lock observations should be treated as suspect;
prefer independent observations. Deadlock and poisoned-lock tests are required
where multiple locks cannot be avoided.

### Attack: polling duplicates the event system

A convenient observation API could encourage surfaces to poll instead of using
AgentEvent/command projections. Phase 1 is restricted to request-time status and
initial state assembly. Live deltas remain event-driven. No timer, watcher, or
background refresh belongs in the observation layer.

### Attack: this does not actually enable a crate split

Correct. It improves dependency direction and semantic ownership but may not
reduce enough coupling to justify `omegon-tui`. `settings` and `control_runtime`
remain unresolved high-traffic dependencies. Phase 1 must not be sold as a
crate split or build optimization. After migration, rerun coupling measurements;
if no useful crate boundary emerges, stop rather than manufacture one.

### Attack: the facade is architecture theater

A zero-state `RuntimeObserver` with methods that merely call free functions can
be needless ceremony. The design explicitly permits free functions. Keep the
facade only if it enforces a coherent borrowed capability and reduces parameter
sprawl; otherwise use `observe_session(&RuntimeStateHandles)`. The invariant is
explicit instance injection and scoped outputs, not object-oriented shape.

### Adversarial conclusion

Proceed only with the minimum IPC/web overlap inventory and a two-instance test
first. Do not pre-design a comprehensive observation schema. The design survives
the singleton critique because it forbids global discovery, unique process
identity, caching, mutable access, and universal snapshots. It still centralizes
semantic policy, which creates blast-radius and accretion risks; those are real
and are controlled by domain scoping, explicit failure values, per-adapter
compatibility tests, and a stop/go reassessment before any crate extraction.

## Additional open questions and assumptions

- [assumption] IPC and web currently duplicate at least one semantic
  interpretation, not merely similarly named fields. Validate by side-by-side
  code inventory before defining the first observation.
- [assumption] Existing external contracts distinguish unavailable state from
  empty/default state, or can preserve their current wire representation while
  retaining explicit failure internally.
- Which domain, if any, genuinely requires acquisition of more than one lock?
- Should observation value types live beside `runtime_state.rs` or in a sibling
  `runtime_observation.rs` module? Prefer the sibling if the first slice exceeds
  a small cohesive module.
- What exact warm validation baseline should be recorded before migration?

## Upstream Rust research reassessment (2026-07-30)

### Sources consulted

Primary/upstream documentation:

- Tokio, **Shared state**: synchronous mutexes are appropriate in async code
  when contention is low and guards are not held across `.await`; wrapping the
  mutex and exposing short non-async operations is the safest pattern.
  <https://tokio.rs/tokio/tutorial/shared-state>
- Tokio `Mutex` documentation: ordinary `std::sync::Mutex` is often preferred
  for data rather than I/O; a dedicated task plus message passing is preferable
  when shared state is an I/O resource.
  <https://docs.rs/tokio/latest/tokio/sync/struct.Mutex.html>
- Axum `State` documentation: application state is supplied explicitly to the
  router, and `FromRef` supports narrow substates rather than requiring every
  handler to consume one omnibus state object.
  <https://docs.rs/axum/latest/axum/extract/struct.State.html>
- Rust `std::sync::Mutex` documentation: poisoning signals that protected data
  may violate invariants; silently treating poison as absence/default is not a
  generally sound recovery policy.
  <https://doc.rust-lang.org/std/sync/struct.Mutex.html>
- `arc-swap` documentation: `ArcSwap` is intended for read-mostly values that
  are atomically replaced, such as periodically renewed configuration or a
  deliberately maintained snapshot. It solves a measured publication problem;
  it does not justify creating a snapshot cache by itself.
  <https://docs.rs/arc-swap>
- Tokio `watch` source/documentation: a watch channel publishes the latest value
  and tracks change visibility for receivers. It is suitable when a producer
  intentionally maintains a current value and consumers need notification.
  <https://docs.rs/tokio/latest/tokio/sync/watch/>

Secondary architectural material used as supporting, not normative, evidence:

- Firezone's Rust sans-I/O discussion: keep policy/state transformation
  synchronous and represent inputs/outputs as data, leaving I/O in adapters.
  <https://www.firezone.dev/blog/sans-io>
- Tyler Mandry, **Contexts and capabilities in Rust**: explicit context
  parameters are inconvenient but make capability requirements visible; a
  borrowed context should grant only the required capability.
  <https://tmandry.gitlab.io/blog/posts/2021-12-21-context-capabilities>

### Findings applied to this codebase

1. **Explicit instance injection is idiomatic.** Axum's state model validates
   passing state from the composition root, but its `FromRef` substate pattern
   argues against handing every consumer all of `RuntimeStateHandles`.
2. **Short synchronous observation methods are appropriate.** Omegon's handles
   protect in-memory data with `std::sync::Mutex`; synchronous copy-and-release
   operations with no `.await` or I/O match Tokio's guidance.
3. **A maintained snapshot is a different architecture.** `watch` and
   `ArcSwap` are good tools only when there is an intentional producer,
   publication lifecycle, freshness contract, and need for latest-value reads.
   Phase 1 has not established those requirements. Adding either now would
   create the cache/lifecycle singleton risk under review.
4. **Poison is not ordinary unavailability.** Current IPC and web code often
   maps lock poison to empty/default payloads. The upstream `Mutex` contract
   says the state may be tainted. Shared observation code should preserve that
   distinction internally, while adapters may map it to their existing wire
   representation for compatibility.
5. **Pure policy should be separate from acquisition.** The strongest
   sans-I/O-shaped boundary is not a large observer object. It is:
   acquire/copy one domain under a short lock, then run pure conversion over
   owned data outside the lock.

### Source audit against the proposed domains

The implementation audit changes the proposed scope:

- **Session is proven overlap.** IPC and web both read turns, tool calls, and
  compactions from `handles.session`; IPC additionally reads `busy`. This is a
  valid first observation domain.
- **Cleave is only partially common.** IPC and web both copy cleave progress,
  but their external payloads and child detail differ. The shared unit should
  be an owned domain copy (or pure summary primitive), not a shared transport
  projection.
- **Harness is not yet proven as one common semantic domain.** IPC uses harness
  data for session Git state, instance identity, harness details, and health;
  web clones the whole `HarnessStatus` and maps it later. Defining a broad
  `HarnessObservation` now would reproduce the omnibus object problem.
- **Delegate/work is not proven overlap in the inspected full-state builders.**
  IPC emits operation episodes from delegate and cleave; web's state snapshot
  primarily exposes cleave. A combined `WorkObservation` is premature.
- **Lifecycle is duplicated but large and semantically divergent.** It remains
  excluded.

### Reassessment: revise Phase 1 downward

The research supports explicit, scoped read capabilities, but it does **not**
support implementing the three-domain observer API proposed above. That API
was still speculative framework design ahead of demonstrated common semantics.

**Revised recommendation: Phase 1A should implement session observation only.**

Preferred API:

```rust
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SessionObservation {
    pub turns: u32,
    pub tool_calls: u32,
    pub compactions: u32,
    pub busy: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObserveError {
    Poisoned(ObservationDomain),
}

impl RuntimeStateHandles {
    pub fn observe_session(&self) -> Result<SessionObservation, ObserveError>;
}
```

This method is not singleton behavior: it is invoked on an explicitly supplied
instance, reads one lock, returns a small owned value, performs no I/O, has no
cache or background lifecycle, and is independently testable across multiple
instances.

A separate `RuntimeObserver<'a>` facade is **not recommended** for Phase 1A.
With one proven operation it is ceremony and risks evolving into a service
locator. Put the narrow method on `RuntimeStateHandles`, or use a free function
if later crate placement requires it.

IPC and web should map `SessionObservation` into their existing payloads. They
must preserve current external behavior on poison for compatibility, but the
adapter mapping should be explicit, e.g. `unwrap_or_default()` at the wire/view
boundary rather than inside the domain observation.

### Stop/go gate after Phase 1A

After session migration, inventory cleave field-by-field. Proceed to a cleave
observation only if both consumers need the same owned source semantics. Do not
introduce `WorkObservation` or `HarnessObservation` without equivalent evidence.
Do not add `watch`, `ArcSwap`, a cached read model, or a snapshot producer unless
profiling or consistency requirements demonstrate a publication problem.

### Updated adversarial verdict

The no-global/no-cache constraints remain necessary but were not sufficient.
An explicitly passed omnibus context can still function as a service locator,
and a borrowed facade over it can still conceal excessive capability. Upstream
practice strengthens the design with **capability minimization**: consumers
should depend on the smallest substate/read capability they require.

Accordingly:

- The broad `RuntimeObserver` proposal is rejected for the first slice.
- `SessionObservation` is approved as a bounded experiment.
- `WorkObservation` and `HarnessObservation` return to open questions.
- A maintained immutable read model is deferred; it is not currently justified.
- Phase 1 succeeds only if it removes duplicate session lock interpretation
  without increasing the number of consumers that can see unrelated runtime
  state.

## Phase 1A implementation status

Phase 1A now lands the bounded session capability described above:

- `RuntimeStateHandles::observe_session()` returns an owned
  `SessionObservation` and exposes lock poisoning as `ObserveError`.
- IPC, web state, and web surface adapters use that observation rather than
  acquiring the session mutex themselves.
- Session producers use `update_session_counters()` and `set_session_busy()`;
  direct session mutex access is confined to `runtime_state.rs`. The backing
  field remains `pub(crate)` only because Rust struct-update syntax in existing
  in-crate tests requires field visibility even when the default supplies it;
  repository-wide source audit enforces the intended boundary until those
  fixtures migrate to constructors.
- `RuntimeStateHandles::new(...)` constructs invocation-owned state without
  allowing composition code to inject or share a process-global session cell.
- Tests prove clone sharing within one invocation, isolation between separately
  constructed invocations, and explicit poison handling.

This is deliberately not a maintained aggregate snapshot. Each call copies one
small domain under one short lock, and the caller maps that owned value into its
surface contract. The handles object still exposes other legacy domains, so the
service-locator risk is reduced for session state but not eliminated for the
whole type. Future slices must privatize one domain at a time rather than add an
omnibus observer.

### Remaining Phase 1 questions

1. Does cleave have a shared owned source contract across IPC, web, and TUI, or
   only superficially similar transport projections?
2. Can harness reads be split into narrow health and identity capabilities
   without cloning the complete `HarnessStatus`?
3. Should mutation methods return typed poison errors instead of preserving the
   previous best-effort behavior? That is a behavior-policy decision and is not
   included in Phase 1A.

## Phase 1B implementation status: cleave observation

The cleave audit supports one shared source contract and rejects a shared
presentation contract. IPC, web daemon status, TUI, and smoke orchestration all
need the current `CleaveProgress`, but they interpret it differently:

- IPC emits active operation episodes.
- web daemon status extracts supervised child runtimes.
- TUI renders progress and accounts child tokens.
- smoke orchestration owns installation and removal of synthetic progress.

Accordingly, `observe_cleave()` copies the source domain under one short lock
and returns `Option<CleaveProgress>`. `None` means that the invocation has no
cleave source installed; it is intentionally distinct from an installed but
inactive progress value. No `WorkObservation`, cached aggregate, cross-domain
snapshot, or surface-neutral presentation model is introduced.

Singleton/service-locator controls remain structural: the method requires an
explicit invocation-owned `RuntimeStateHandles`, observations are owned copies,
and independent handle instances retain independent cleave sources. Mutation is
limited to explicit `install_cleave()` and `clear_cleave()` composition methods;
normal surface consumers use observation and availability methods. The backing
field remains visible during the same test-fixture migration described for
session state, but production read paths no longer acquire its mutex directly.

The adversarial stop condition is unchanged: delegate, harness, or lifecycle
must not be folded into an omnibus work/runtime observation merely because they
are adjacent fields. Each requires demonstrated shared source semantics and its
own poison/absence policy before migration.
