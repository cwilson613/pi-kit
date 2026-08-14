---
id: bounded-core-tool-io-and-process-lifecycle
title: "Bounded core-tool I/O and owned process lifecycle"
status: decided
parent: cpu-bound-tui-liveness-and-authoritative-interrupts
tags: [tools, io, filesystem, processes, liveness, atomicity, tdd]
open_questions: []
dependencies: []
related:
  - cpu-bound-tui-liveness-and-authoritative-interrupts
  - authoritative-tui-input-and-bounded-presentation
---

# Bounded core-tool I/O and owned process lifecycle

## Overview

Replace the independent, legacy implementations behind `read`, `write`, `edit`, `change`, `bash`, `terminal`, and `validate` with two shared substrates:

1. **Bounded file I/O** — limits source work as well as returned output, rejects unsupported file kinds, stages mutations beside their destination, verifies expected identity/revision, and atomically publishes one-file replacements.
2. **Owned process execution** — owns a precise child process group, pumps bounded output, observes direct-child exit independently from pipe EOF, applies cancellation and post-exit drain deadlines, reaps descendants, and emits exactly one terminal outcome.

This work addresses the deeper defect exposed by the CPU-hot TUI incident: core tools generally bounded their returned payloads but did not consistently bound the work, memory, filesystem mutation, subprocess lifetime, or output pumping needed to produce those payloads.

## Research

### Incident evidence

A live release process remained near 100% CPU with an active `bash` tool turn that did not terminalize. The direct tool wrapper and `git diff --check` remained alive for roughly 55 minutes. Killing those processes did not settle the turn or stop the hot loop. A three-second process sample identified one CPU-hot Tokio worker dominated by allocation and memory-copy activity, while other workers slept. The dedicated terminal-input thread was alive but blocked in native `read` for 2,517 of 2,527 samples.

### Core-tool audit

- `read` calls `tokio::fs::read` before applying line/byte output limits, then allocates the complete byte buffer, UTF-8 string, line vector, and selected output. Image reads are unlimited before base64 expansion.
- `write` directly truncates the destination through `tokio::fs::write`; it has no staged publication or operation budget.
- `edit` creates several whole-file normalized copies, performs an incomplete reread TOCTOU check, and directly truncates the target. It can alter mixed line endings outside the replacement.
- `change` advertises atomic multi-file edits but writes files sequentially and rolls back through another fallible sequence of direct writes. Process death can leave a partial batch.
- `bash` retains all output in an unbounded `String` and derives repeated full snapshots from it. It waits for both inherited pipes to reach EOF before observing/reaping completion, so descendants can keep a completed direct child nonterminal indefinitely.
- `terminal` creates detached reader/watcher threads without retained handles, polls child status, performs synchronous open/write/close per transcript chunk, silently ignores transcript failures, and can split UTF-8 at its byte ceiling.
- `validate` has a stronger process-group timeout path but still performs synchronous unbounded file reads inside async validators and does not share process ownership with `bash`.

### Unstated assumptions surfaced and resolved

- **[assumption] Atomic rename is sufficient for every `change` batch.** False. Rename is atomic per file, not across files. `change` must expose staged best-effort batch commit with an explicit recovery journal unless a platform transaction exists.
- **[assumption] Tokio timeout cancels filesystem work.** False as a general guarantee. Dropping a future bounds caller waiting but does not prove an underlying blocking filesystem operation stopped. Regular-file classification and source budgets must precede reads.
- **[assumption] Direct-child exit implies output EOF.** False. Descendants may inherit stdout, stderr, or PTY handles. Exit observation and output-drain completion are distinct state transitions.
- **[assumption] Heartbeats prove tool progress.** False. Heartbeats prove only that the owner loop remains scheduled. Semantic progress requires new bounded output, direct-child state change, cancellation transition, or terminal settlement.
- **[assumption] Path approval remains valid through mutation commit.** False. Symlink and parent identity may change. Mutation must revalidate the destination immediately before publication.
- **[assumption] Exact total line count is compatible with bounded reads.** False for arbitrary large files. A bounded read may report an exact total only when EOF was observed; otherwise totals are explicitly unknown/lower-bounded.

## Decisions

### 1. Shared bounded file substrate

Introduce a focused internal module, initially `tools/file_io.rs`, used by `read`, `write`, `edit`, and `change`.

The substrate exposes:

- `inspect(path) -> FileIdentity` using symlink-aware metadata;
- `read_text_window(path, ReadBudget, offset, limit) -> TextWindow`;
- `read_binary_bounded(path, max_source_bytes) -> BoundedBytes`;
- `stage_replace(path, bytes, ExpectedIdentity) -> StagedReplacement`;
- `commit_replace(staged) -> CommitEvidence`;
- `discard(staged)` as idempotent cleanup.

`FileIdentity` includes platform-available device/inode or equivalent metadata, file type, size, modification marker, and canonical parent identity. It is evidence for conflict detection, not a security token.

Only regular files are accepted by default. Directories, FIFOs, sockets, devices, and other special files fail before content I/O. Symlinks are resolved and checked through the existing workspace boundary; mutation records both requested path and resolved target and revalidates them before commit.

### 2. Bounded work is part of every tool contract

Budgets cover source bytes, emitted bytes, records/lines, elapsed preparation time, and allocation growth where relevant. Output truncation alone is insufficient.

`read` streams from a bounded reader rather than loading the complete file. Pagination may return:

- `totalLines: <exact>` only after EOF is observed;
- `totalLinesExact: false` and `nextOffset` when the unread suffix was intentionally not scanned.

Images have a source-byte ceiling checked from metadata and enforced while reading before base64 allocation.

### 3. One-file mutations publish through sibling staging and rename

`write` and `edit` stage bytes in a unique file in the destination directory, apply destination-compatible permissions, flush the staged file, revalidate expected destination/parent identity, and rename into place. The staged file is removed on every known failure or cancellation path.

Durability is explicit:

- default guarantee: atomic visibility within one filesystem;
- optional stronger durability: staged-file sync plus parent-directory sync where supported.

No claim is made that rename provides cross-filesystem or power-loss transactional guarantees unless the selected durability mode establishes them.

### 4. Edit preserves untouched bytes

`edit` rejects empty `oldText`, searches raw bytes for the exact UTF-8 pattern, requires exactly one match, and replaces only that byte range. It does not normalize the complete file. Diagnostic fuzzy matching is bounded to a local window or a capped scan.

Commit requires the original `FileIdentity` plus a content revision. If either changed, the stage is discarded and the caller must reread.

### 5. `change` becomes an explicit staged batch, not falsely atomic

All target replacements are prepared and validated before any destination is changed. Commit order is deterministic. A small recovery journal records target, expected identity, staged path, and commit state.

The public result distinguishes:

- `prepared_no_commit`;
- `committed_all`;
- `partial_commit_recovery_required`.

Rollback is attempted only when it cannot overwrite externally changed content. The tool description must say **coordinated staged multi-file replacement**, not filesystem-transaction atomicity.

### 6. Shared owned-process substrate

Introduce `tools/process_owner.rs`, shared first by `bash` and `validate`, then by terminal session lifecycle where PTY differences permit.

The owner state machine is:

```text
Starting
  -> Running
  -> DirectChildExited
  -> DrainingOutput
  -> Completed | Cancelled | TimedOut | Failed | DrainExpired
```

Each execution owns:

- direct child PID and process-group/session identity;
- cancellation token and optional wall-clock deadline;
- bounded stdout/stderr pumps;
- direct-child wait future independent from pipe EOF;
- post-exit drain deadline;
- graceful termination and forced-kill deadlines;
- exactly-once terminal outcome arbitration;
- terminal evidence including exit status, signal, truncation, bytes observed, and whether descendants retained pipes.

### 7. Output storage is a bounded ring, never an unbounded history buffer

`bash` stores only a configured tail by bytes and lines, preserving UTF-8 boundaries. Counters retain total observed bytes/lines independently. Partial results project the bounded ring and carry a monotonic output revision.

A heartbeat carries scheduling/liveness evidence only and never increments semantic output progress. Repeated snapshots of unchanged content are suppressed.

### 8. Direct-child exit and pipe EOF are independent

The process owner waits concurrently for:

- child exit;
- stdout records;
- stderr records;
- cancellation;
- wall-clock timeout.

After direct-child exit, pumps receive a bounded drain grace period. If descendants retain pipes after that deadline, the owner terminates the owned process group, closes local pump handles, records `DrainExpired`, and terminalizes exactly once. Absence of an operator-specified timeout does not permit an unbounded post-exit drain.

A still-running direct child may remain unlimited only when the tool contract explicitly allows no wall-clock timeout; it must nevertheless retain responsive cancellation and bounded memory.

### 9. Terminal sessions retain lifecycle handles

Terminal sessions retain reader and watcher handles or equivalent owned tasks. Child exit starts a bounded PTY drain. Session stop and cleanup signal the exact process group/session, close the PTY owner, await bounded reader completion, and then detach only with explicit evidence.

Transcript output uses one owned buffered writer, bounded queueing, UTF-8-safe truncation, and surfaced write failures. Transcript and visible tail revisions are derived from the same accepted byte stream.

### 10. Path authority remains centralized

The existing workspace boundary remains the policy owner. The file substrate accepts already-authorized targets but must revalidate canonical parent/target identity at use and commit time. It does not create a second approval system.

### 11. Instrumentation precedes behavioral rollout

Every shared substrate transition emits structured, low-cardinality evidence suitable for the liveness ledger:

- operation ID and tool name;
- state transition and monotonic revision;
- source/retained/emitted byte counts;
- child exit, drain, termination, and reap timestamps;
- staged-file prepare/commit/discard evidence;
- terminal outcome and ambiguity classification.

Paths and command content are redacted or omitted from default telemetry.

## Adversarial assessment and amendments

The first decided draft was red-teamed against hostile filesystems, PID reuse, cancellation races, compatibility pressure, and tests that could pass while still doing unbounded work. The following weaknesses were found and resolved.

### 1. Path inspection followed by path open is still a race

A metadata check on a pathname does not constrain what a later `open` reaches. The substrate therefore treats path resolution and descriptor acquisition as one operation where the platform permits it. On Unix, reads open with no-follow semantics for the final component, inspect the opened descriptor, and verify it is a regular file. Mutation staging opens the already-authorized parent directory and performs staging and rename relative to that directory. Platforms lacking equivalent descriptor-relative operations use the strongest available identity revalidation and report the weaker guarantee in `CommitEvidence`; they must not silently claim race-free containment.

Tests must mutate symlinks and parent paths between every injectable boundary, not only before the initial inspection.

### 2. Full-file content hashing would reintroduce unbounded work

`edit` needs complete content to prove a unique exact match under its existing contract, but it must reject files over an explicit editable-source ceiling before allocation. Its content revision is computed during that one bounded read. Commit revalidates descriptor/path identity and metadata; if metadata changed, a bounded reread and revision comparison is permitted only within the same ceiling. No hidden second unbounded hash pass is allowed.

`read` does not compute a whole-file content hash and does not promise conflict detection.

### 3. Pagination needed a stable cursor definition

Existing `read` offsets are line-oriented and callers depend on that behavior. Slice A preserves `offset` and `limit` as line coordinates. `nextOffset` is the next line index, not a byte offset. Internally, `TextWindow` may carry a byte resume cursor, but that cursor is opaque, bound to `FileIdentity`, and invalid after identity change. The public tool does not accept an unauthenticated byte cursor in the first slice.

A large line is itself bounded: if one logical line exceeds the source-byte or emitted-byte ceiling, the result reports `lineTooLong` with the observed lower-bound size and does not grow until newline.

### 4. Metadata size is advisory, not an allocation authority

Files may grow after metadata inspection and sparse files may report misleading sizes. Every read enforces its byte ceiling while consuming the descriptor. Buffers grow only to the configured bound. Image encoding is streamed or uses a checked allocation derived from the enforced bytes, never solely from metadata size.

### 5. Atomic visibility and preservation semantics conflict on symlinks

Replacing a pathname that is a symlink replaces the link itself, whereas writing through it changes its target. Silent switching between those semantics is unacceptable. Core mutation tools reject symlink destinations by default in the first implementation. A future explicit follow-symlink mode requires separate authorization of the resolved target and must state which pathname is replaced. This keeps atomic publication and workspace authority coherent.

New files also require parent-directory identity to remain stable from staging through rename. Existing hard links are allowed only with an explicit contract: rename replaces this directory entry and does not mutate other links to the old inode.

### 6. Temporary-file naming and cleanup are security boundaries

Staging uses exclusive creation with unpredictable names in the destination directory, mode `0600` before permission adaptation, and no path supplied by the model. Cleanup is descriptor-owned and idempotent. Startup does not delete arbitrary files matching a broad prefix; recovery removes only journal-authenticated artifacts created by Omegon.

### 7. Cancellation cannot interrupt every filesystem syscall

The design does not promise instantaneous cancellation below a blocking host filesystem call. File operations have two bounds:

- logical work/allocation bounds enforced by Omegon;
- caller wait bounds with an outcome marked indeterminate if the host operation may still be executing.

Mutation publication is never launched in a detached task that can rename after the caller has received cancellation. Once commit enters the non-cancellable rename critical section, it returns the observed commit evidence before honoring cancellation. Cancellation before that point discards the stage.

### 8. Process-group identity alone is vulnerable after reap and PID reuse

The owner never sends a group signal after it has lost evidence that the group belongs to the execution. On Linux, pidfds are used when available for direct-child identity; process-group signalling remains scoped to the group created before exec and ends before final reap/ownership release. On other Unix systems, signalling and wait are serialized inside the owner before releasing the child handle. Windows uses a Job Object or reports reduced descendant ownership. Unsupported platforms must not claim descendant-tree guarantees.

### 9. Killing a process group after direct-child exit can terminate unrelated reused groups

Post-exit drain escalation occurs while ownership is still retained and before the execution is terminalized. The owner first closes its local read ends and requests group termination only if the platform ownership token remains valid. If ownership cannot be proven, it records `descendantOwnershipLost` and terminalizes without broad signalling. Command-name matching, `pkill`, and `killall` remain forbidden.

### 10. Output pumps need byte framing, not line-only framing

A child can emit an unlimited line or split UTF-8 indefinitely. Pumps consume bounded byte chunks, maintain at most three UTF-8 carry bytes, replace invalid sequences deterministically, and feed a byte-bounded ring. Line counts are derived incrementally and may be lower bounds. No `read_line` operation may allocate until newline.

Stdout/stderr chronological ordering cannot be reconstructed exactly from independent pipes. The result records stream identity and owner-assigned sequence order. It does not claim kernel write ordering. Compatibility text may interleave by observed arrival, while structured details preserve stream labels.

### 11. Partial snapshots can still be quadratic even with a bounded ring

The ring has a fixed maximum, but repeatedly filtering and cloning that maximum on every line remains expensive. Projection is revision-gated and rate-limited. At most one snapshot is constructed per flush interval unless a terminal transition occurs, and unchanged revisions produce no snapshot. Filtering must operate on retained bytes only and should return borrowed/shared storage where practical.

### 12. “No timeout” needed a liveness contract

A running direct child may be unlimited in wall-clock time only for explicitly long-running operations. It still has:

- bounded retained output;
- responsive cancellation;
- periodic scheduler yield;
- semantic-progress timestamps;
- no-progress evidence without automatic destructive action.

The process owner does not infer failure from quietness alone. Automatic termination is permitted for explicit timeout, cancellation, direct-child-exited drain expiry, or owner shutdown—not merely lack of output.

### 13. Terminal PTYs cannot blindly reuse pipe semantics

PTYs combine streams, have terminal process-group/session semantics, and may require closing the master to provoke EOF. `process_owner.rs` supplies outcome arbitration and lifecycle primitives, but `terminal` retains a PTY-specific adapter rather than pretending PTYs are ordinary pipes. The adapter owns the master descriptor, reader task, child/session handle, and transcript writer as one aggregate.

### 14. Recovery journals can leak paths and become executable instructions

`change` journals are owner-private (`0600`) and contain normalized identifiers plus required recovery fields. They are data, never shell commands. Recovery validates schema version, workspace identity, destination authorization, staged-file ownership, and expected identities before action. Sensitive absolute paths are not emitted into ordinary tool output.

### 15. Fault injection is required for meaningful TDD

Tests cannot reliably induce disk-full, rename ambiguity, delayed EOF, PID reuse, or flush failure through sleeps and host conditions. Both substrates expose crate-private test seams:

- a filesystem operations trait or narrow injected hooks around open/read/write/flush/rename/sync;
- a clock abstraction for deadlines;
- a process backend abstraction for wait/signal/reap and output pumps.

Production has exactly one concrete backend per platform. Test hooks are unavailable through tool parameters and add no operator-controlled behavior.

### 16. Compatibility rollout needs dual evidence, not dual mutation

Shadow mode may compare bounded-read results with the legacy reader on small regular files, but mutation paths must never execute both implementations. Rollout gates are:

1. bounded read substrate with parity tests for supported small files;
2. one mutation tool at a time with fault-injection tests;
3. process owner behind focused `bash` tests;
4. `validate` adoption;
5. PTY adapter adoption.

Each slice removes its replaced legacy path before completion. A permanent fallback would preserve the original failure mode and is rejected.

## Refined invariants

The following are non-negotiable and testable:

1. **Bounded allocation:** no single tool-owned buffer exceeds its declared ceiling, including one unterminated line and invalid UTF-8 input.
2. **Bounded retained history:** historical output size does not affect current memory or projection cost beyond fixed counters.
3. **Descriptor-backed classification:** regular-file acceptance is verified on the opened object, not only on a prior pathname lookup.
4. **No late publication after cancellation:** cancellation before commit cannot be followed by a detached rename.
5. **Atomic one-path visibility:** readers see the old or complete new file for supported same-filesystem replacements.
6. **Truthful weaker-platform evidence:** reduced path or descendant ownership guarantees are explicit in structured details.
7. **Exactly-one terminal outcome:** all cancellation/timeout/exit/drain races converge through one arbiter.
8. **Ownership before signalling:** no process or group is signalled after ownership evidence is released.
9. **No progress inflation:** heartbeat, repeated snapshots, polling, and unchanged state do not advance semantic revision.
10. **No false batch atomicity:** multi-file partial commit is representable, recoverable, and never labelled atomic.

## TDD implementation plan

### Slice A — bounded file reads

**Test seam:** a crate-private `FileOps` backend records bytes requested/read and can inject metadata changes, special-file identities, invalid UTF-8 boundaries, cancellation, and host-call stalls. Production uses the platform backend directly; tools cannot select a backend.

Write failing tests first for:

1. A small line window from a large file does not consume past its source budget, verified by backend byte counters rather than process RSS.
2. A limit of ten lines returns line-oriented `nextOffset` without scanning to EOF.
3. Exact totals are reported only when EOF was observed; otherwise `totalLinesExact` is false and the observed count is a lower bound.
4. FIFO/socket/device inputs are rejected after descriptor-backed classification and before content read.
5. Symlink inputs follow the existing read authorization policy but cannot escape between authorization and descriptor verification.
6. Files exceeding the image source ceiling are rejected before allocation/base64 encoding, and growth after metadata inspection remains bounded.
7. UTF-8 split across internal buffer boundaries is decoded correctly with at most three carry bytes.
8. Invalid UTF-8 reports the canonical byte offset while allocation remains bounded.
9. One unterminated line larger than all budgets returns `lineTooLong` without unbounded growth.
10. Cancellation or elapsed budget returns bounded evidence and no partial-success contract.
11. Small supported files remain text/details compatible with the legacy tool except for newly truthful exactness fields.
12. Directory replacement, truncation, and identity changes between injected open/inspect/read boundaries fail closed.

Implement `file_io::inspect_opened`, bounded readers, and migrate `read` only after these tests fail for the intended reasons. Slice A does not introduce staging or mutation APIs beyond interfaces required to avoid prematurely freezing Slice B.

### Slice B — atomic one-file mutation

**Test seam:** injected filesystem hooks fail each stage independently and expose a barrier immediately before the non-cancellable rename critical section.

Write failing tests first for:

1. Failed staged write leaves an existing destination byte-identical.
2. Cancellation before the rename critical section leaves the destination intact and removes staging artifacts; cancellation after entry returns observed commit evidence and cannot report a false rollback.
3. Concurrent destination mutation rejects commit without a hidden unbounded reread.
4. Parent retargeting and symlink destinations reject commit.
5. Successful concurrent readers observe either old or new complete content, never truncation.
6. Existing permissions are preserved and new-file permissions obey the configured policy without a permissive staging window.
7. Hard-link behavior is explicit: replacement changes one directory entry and leaves other links on the old inode.
8. Mixed line endings outside an edited byte range remain unchanged.
9. Empty `oldText` is rejected before file I/O.
10. Files over the editable-source ceiling fail before whole-file allocation.
11. Disk-full/write/flush/rename/sync failures produce unambiguous commit evidence where knowable and `Indeterminate` only where the platform cannot establish visibility.
12. Every journal-authenticated staging artifact is cleaned after known failure, while unknown files sharing a prefix are untouched.

Migrate `write`, then `edit`, onto the substrate. Do not execute legacy and new mutation paths in parallel or retain a silent fallback.

### Slice C — coordinated `change`

Write failing tests first for deterministic prepare/commit order, no writes before all edits validate, journal recovery after injected mid-commit failure, refusal to overwrite an externally changed target during recovery, and truthful partial-commit output. Then replace the existing rollback claim and tool description.

### Slice D — bounded `bash` process ownership

**Test seam:** a deterministic process backend and manual clock model child exit, inherited open pipes, cancellation/timeout races, signal delivery, ownership loss, and reap without wall-clock sleeps.

Write failing tests first for:

1. Continuous multi-megabyte output retains a bounded byte ring while total counters grow.
2. One unlimited line and arbitrarily split invalid UTF-8 remain within the same bound.
3. Partial projection work depends on retained bytes and flush revisions, not historical bytes or input line count.
4. Cancellation remains active after either output stream closes.
5. Direct-child exit is observed while a descendant retains stdout/stderr.
6. Post-exit drain expiry closes local pumps, signals only while ownership is valid, reaps as supported, and returns once.
7. Ownership loss records a weaker outcome and never broad-signals a reused PID/group.
8. Timeout/cancellation/exit races choose exactly one terminal outcome.
9. Heartbeats do not advance semantic output revision and unchanged snapshots are suppressed.
10. Stdout/stderr structured stream labels survive compatibility-text interleaving.
11. A synthetic nonterminal descendant produces scheduler yields and no busy-spin under a manual clock.
12. Platform capability details truthfully distinguish process-group, pidfd, Job Object, and reduced descendant ownership.

Migrate `bash`; then adapt `validate` to the same owner with validator-specific output projection. No-progress evidence remains observational and does not kill a still-running child without cancellation or timeout authority.

### Slice E — terminal session ownership

Write failing tests for retained lifecycle handles, bounded PTY drain, exact-group stop, UTF-8-safe transcript ceilings, surfaced transcript writer failure, and exactly-once background completion. Add an opt-in PTY black-box scenario after deterministic unit coverage.

## Slice A implementation handoff

The first TDD implementation is intentionally narrower than the eventual `file_io.rs` API.

### Concrete production types

```rust
pub(crate) struct ReadBudget {
    pub max_source_bytes: u64,
    pub max_emitted_bytes: usize,
    pub max_lines: usize,
    pub max_elapsed: Duration,
}

pub(crate) struct OpenedFileIdentity {
    pub kind: OpenedFileKind,
    pub size_hint: Option<u64>,
    pub modified: Option<SystemTime>,
    pub platform: PlatformFileIdentity,
    pub guarantee: IdentityGuarantee,
}

pub(crate) struct TextWindow {
    pub text: String,
    pub start_line: usize,
    pub lines_emitted: usize,
    pub next_offset: Option<usize>,
    pub observed_lines: usize,
    pub total_lines_exact: bool,
    pub source_bytes_read: u64,
    pub truncated: bool,
    pub truncation_reason: Option<ReadTruncationReason>,
    pub identity: OpenedFileIdentity,
}
```

Names may change during red tests, but semantics may not weaken. The internal reader returns typed errors for unsupported file kind, source budget, line too long, invalid UTF-8, identity mismatch, cancellation, deadline, and host I/O. `read.rs` maps those errors into the existing tool-result convention without exposing host paths beyond current policy.

### Initial budget policy

The first slice uses conservative constants colocated with `file_io.rs` and covered by tests:

- text source scan: enough to satisfy current 2,000-line/50 KiB output behavior plus bounded skipped-line work, with a hard ceiling independent of file size;
- one logical line: no larger than the emitted-byte ceiling plus a small framing allowance;
- image source: explicit ceiling chosen before base64 expansion and reported in the tool error;
- elapsed budget: outer safety bound, while deterministic unit tests assert byte/read counts instead of timing.

Exact values are implementation decisions to benchmark before merge, not configuration surface in Slice A. Tool parameters cannot raise them.

### Required red-test order

1. special-file descriptor classification;
2. bounded ten-line window from a large instrumented source;
3. line-oriented continuation and inexact totals;
4. oversized unterminated line;
5. UTF-8 boundary and invalid-byte offset;
6. image growth beyond metadata hint;
7. identity/path swap at injected boundaries;
8. cancellation/deadline outcome;
9. compatibility fixtures for ordinary UTF-8, empty files, offsets beyond EOF, and current truncation details.

The implementation must not begin with a generic abstraction framework. Add the smallest backend seam that makes these tests deterministic, then extract only duplication demonstrated by production code.

## Acceptance criteria

- Returned-output limits correspond to bounded source work or explicitly reported incomplete scans.
- No core file mutation directly truncates an existing destination before a complete staged replacement is ready.
- `change` no longer claims cross-file atomicity it cannot provide.
- Core command output memory remains bounded under unlimited-duration execution.
- Direct-child exit cannot remain nonterminal indefinitely because descendants retain pipes or PTYs.
- Cancellation, timeout, child exit, and drain expiry arbitrate one terminal outcome.
- No exact process is killed by command-name matching or broad host-wide signals.
- All deterministic regression tests pass without fragile sleeps; wall-clock tests use generous outer safety bounds only.
- The CPU-hot incident reproducer shows bounded retained memory, cooperative scheduler yield, interrupt admission, child-group reap, and exactly-once revoked settlement.

## Implementation scope

- `core/crates/omegon/src/tools/file_io.rs` — new bounded file substrate.
- `core/crates/omegon/src/tools/process_owner.rs` — new owned process substrate.
- `core/crates/omegon/src/tools/read.rs` — streaming bounded windows and truthful totals.
- `core/crates/omegon/src/tools/write.rs` — staged atomic visibility.
- `core/crates/omegon/src/tools/edit.rs` — byte-preserving exact replacement and revision commit.
- `core/crates/omegon/src/tools/change.rs` — coordinated batch and recovery evidence.
- `core/crates/omegon/src/tools/bash.rs` — bounded ring and independent child/drain lifecycle.
- `core/crates/omegon/src/tools/validate.rs` — shared process ownership and bounded file validators.
- `core/crates/omegon/src/tools/terminal.rs` — retained PTY lifecycle ownership and transcript writer.
- `core/crates/omegon/src/tools/mod.rs` — truthful schemas/descriptions and shared module composition.
- Focused unit tests colocated with each module; PTY/process integration tests under `core/crates/omegon/tests/` only where OS behavior must be exercised.

## Rollout and compatibility

Land the work in independently reviewable slices. Preserve existing successful result text where it is not misleading, but add structured details for exactness, truncation, identity, and terminal outcome. Changes to `read` total-count semantics and `change` atomicity language are intentional contract corrections and require changelog entries.

The first implementation PR is Slice A only. It establishes the shared file identity and bounded-read interfaces without simultaneously changing mutation or process behavior.
