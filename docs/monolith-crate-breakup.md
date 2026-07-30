---
id: monolith-crate-breakup
title: "Monolith crate breakup: omegon compilation-unit decomposition"
status: implementing
tags: []
open_questions:
  - "Do `settings` (205 refs from tui) and `control_runtime` (106 refs) need to move or split before `omegon-tui` can become near-leaf? If they carry their own inbound coupling from the rest of the monolith, Phase 2 may require a further extraction round that is not yet scoped."
  - "Is the current validation cost dominated by crate size or by the linker? Debug binaries reached 250 MB and emitted repeated `__eh_frame section too large (max 16MB)` warnings; the `[profile.dev]` change in 08a02bdb targets exactly that and its effect has not been measured. If linking dominates, the crate split addresses the wrong bottleneck and should be deferred."
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
