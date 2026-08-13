---
id: authoritative-tui-input-and-bounded-presentation
title: "Authoritative TUI input and bounded presentation"
status: decided
parent: stream-idle-boundary-semantics
tags: [tui, runtime, operator-agency, native-transcript, scheduling, cancellation]
open_questions: []
dependencies: []
related: []
---

# Authoritative TUI input and bounded presentation

## Overview

Restore operator authority when native-transcript publication or rendering is slow or wedged. Separate terminal input acquisition from presentation, route interrupts through a generation-scoped supervisor ingress, serialize and budget normal terminal output, make transcript publication transactional and resumable, bound streaming/render state, and replace deterministic no-progress surrender with one-shot evidence-aware synthesis.

This design distinguishes two guarantees:

1. **Runtime authority:** interrupt and shutdown initiation remain available when presentation is blocked.
2. **Presentation recovery:** terminal restoration is bounded and best-effort. An indefinitely blocked OS terminal write cannot be made recoverable in-process.

The first is mandatory. The second must never be overstated.

## Research

### Adversarial assessment and amendments

Adversarial review found the first draft overstated what an input thread guarantees, omitted bounded teardown under blocked OS writes, lacked exactly-once terminal outcome arbitration, did not bound streaming buffers/caches by bytes, used insufficient draw acknowledgements, risked dirty-state loss, could trap superseding lifecycle boundaries, treated mouse capture as universally desirable, and could create a second synthesis loop. The design was amended with explicit runtime-vs-presentation guarantees, supervisor CAS outcome, emergency best-effort restoration, non-reusable epochs, contiguous publication ranges and rebuild semantics, byte/count/time caps, boundary precedence, revisioned scheduler, circuit breaker, cross-surface ordering, native mouse policy, and one-shot synthesis integrated with final-response state.

## Decisions

### 1. Dedicated terminal input owner

**Status:**

**Rationale:** One dedicated OS thread exclusively owns `crossterm::event::poll/read`. A finite nonzero poll timeout permits cooperative shutdown without hot-looping. The worker never writes terminal output and never mutates application or conversation state.

Input transport has separate policies:

- key, paste, button, and focus events use an ordered bounded queue;
- mouse movement and resize use latest-value coalescing;
- interrupt chords and input-boundary loss use a separate priority ingress and never queue behind ordinary input;
- saturation never silently drops semantic keyboard/paste input: after bounded buffering and eligible coalescing, it raises an explicit input-overload boundary fault.

Only this worker may call Crossterm event-read APIs. Shutdown requests stop but does not indefinitely join a worker blocked below Crossterm or OS control.

### 2. Supervisor-owned interrupt ingress

**Status:**

**Rationale:** The input thread does not directly mutate `InteractiveRuntimeSupervisor`, `SharedCancel`, or conversation state. The coordinator creates a narrow thread-safe ingress consumed independently of the TUI render loop and ordinary coordinator command queue.

Each request contains a fresh runtime epoch, runtime turn ID, source, and input sequence. The supervisor validates identity and atomically chooses one terminal outcome:

```text
Running → Completed | Revoked | Failed
```

The winner closes admission for that identity, cancels its token, and emits the sole terminal lifecycle sequence. Duplicate and stale requests are idempotently rejected. Runtime identity uses a fresh session epoch plus checked turn increments; counters do not saturate into reusable identities.

Once revocation wins, no provider request, forced synthesis, mutation, or assistant completion may begin or publish for that turn.

### 3. One normal output owner; independent best-effort fallback

**Status:**

**Rationale:** The TUI presentation task remains the sole owner of normal terminal writes, Ratatui draws, viewport changes, and mode transitions. Normal output is serialized without a lock that emergency restoration must acquire.

Shutdown is phased and bounded:

1. mark the attachment closing and reject new presentation work;
2. revoke active runtime work when terminal loss is authoritative;
3. request input-worker stop;
4. give the presentation owner a bounded grace period to restore;
5. attempt idempotent emergency restoration without waiting for normal output ownership;
6. detach an unresponsive input worker and do not await a blocked presentation task indefinitely.

`TerminalSessionGuard` must separate claiming restoration responsibility from recording successful per-mode restoration. Panic fallback must not wait on a mutex potentially held by the failing path. Emergency writes may race after escalation; bounded process exit is preferable to deadlock.

No in-process design guarantees restoration if the kernel or terminal driver blocks writes forever. Acceptance requires supervisor progress and bounded teardown, with restoration explicitly best-effort.

### 4. Transactional, resumable transcript publication

**Status:**

**Rationale:** Replace “take all, flatten all, insert all” with a per-terminal-attachment state machine:

```text
Idle → Preparing(range) → Ready(chunk) → Writing(chunk)
     → Committed(range) | UnknownDelivery(range) | Failed(range)
```

Preparation is bounded by record count, bytes, visual rows, and elapsed time. Oversized records yield resumable chunks on grapheme/line boundaries. The canonical cursor is only peeked during preparation and commits after `insert_before` returns success.

Publication identity includes attachment epoch, canonical base revision, target revision, and canonical segment/range identity. Consumers apply only contiguous ranges. Stale, future, duplicate, or missing ranges trigger snapshot rebuild rather than delta append.

If physical delivery is ambiguous, do not blindly append again. Re-anchor and rebuild a bounded projection from canonical state, or disable native-scrollback publication and expose canonical content through the managed viewport. Physical terminal scrollback is not a transactional datastore; the guarantee is canonical losslessness and no intentional duplicate append, not exactly-once bytes after ambiguous OS failure.

Reset, revocation, terminal loss, and superseding message boundaries bypass deferred presentation queues and atomically invalidate stale work.

### 5. Explicit streaming backpressure

**Status:**

**Rationale:** Streaming presentation currently permits unbounded authoritative text, pending deltas, and blocked events. Add count and byte caps to every retained class.

- canonical conversation remains the source of record;
- completed text is referenced from canonical storage rather than duplicated indefinitely;
- pending deltas coalesce only up to a byte cap;
- lifecycle/control events have a reserved bounded lane;
- saturation enters snapshot/rebuild mode and publishes a presentation-backlog state;
- canonical assistant content is never silently dropped.

A superseding `MessageStart`, reset, revocation, or boundary loss cannot remain trapped behind blocked presentation events. Draw acknowledgement identifies the exact canonical snapshot and publication revision successfully rendered; beginning or attempting a draw is not acknowledgement.

### 6. Revisioned frame scheduling and circuit breaker

**Status:**

**Rationale:** Replace Boolean dirty state with monotonic requested and drawn revisions. A draw captures its requested revision; completion advances only to that captured value, so dirtiness raised during drawing survives. Urgent input reduces latency but remains frame-rate bounded and cannot create an unbounded redraw loop.

Instrument publication preparation, insertion, render callback, and total draw duration. Repeated over-budget operations enter degraded mode:

- suspend native-scrollback insertion;
- retain bounded canonical range metadata, not unlimited rendered lines;
- render compact backlog status when output is available;
- retry through exponential backoff and a half-open probe;
- never terminate agent work because presentation is behind.

Slow successful rendering and terminal I/O failure are distinct breaker causes. Timer ticks respect retry deadlines and cannot spin.

### 7. Revision-keyed caches with independent retention

**Status:**

**Rationale:** Cache immutable completed-segment projections by canonical segment identity/revision, width, theme revision, and presentation level. Streaming invalidates only the active segment. Cache bytes and entries have explicit LRU bounds.

TUI cache eviction never evicts canonical conversation or audit data. Publication preparation indexes canonical revisions incrementally and must not rescan all retained history each frame.

### 8. Native mouse capture is an explicit policy

**Status:**

**Rationale:** Mouse capture is not enabled unconditionally because it interferes with native terminal selection in some emulators and multiplexers.

Provide `auto | on | off`:

- `auto` enables capture only when mouse affordances are active and capability is known;
- `on` captures and advertises mouse controls;
- `off` preserves native selection and disables mouse-only affordances.

Keyboard operation remains complete in all modes. Capture state is observable and restored by terminal-session ownership.

### 9. One-shot no-progress synthesis

**Status:**

**Rationale:** Replace deterministic surrender with one explicit terminal phase shared with the existing final-response reservation, not a second independent mechanism.

At threshold, schedule at most one `ForcedSynthesis` attempt:

- tools disabled;
- all dead-mouse, progress, plan-reconciliation, stuck, and continuation nudges disabled;
- cancellation checked immediately before request admission and during streaming;
- termination on success, error, stall, empty response, or cancellation;
- deterministic fallback only if synthesis fails, labeled as runtime-generated rather than assistant-authored.

Progress is based on novel canonical evidence/effects, not token arrival, drawing, notifications, or repeated cached probes. Evidence identity includes normalized target, operation class, result digest, observation epoch, and hypothesis effect. Novel successful inspection advances evidence; an identical unchanged probe does not.

An open visible plan remains open or is explicitly marked blocked with evidence. Synthesis cannot silently clear operational state.

### Independent input acquisition with supervisor-owned interrupt authority

**Status:** accepted

**Rationale:** Input acquisition must not share a scheduling boundary with rendering, but input must remain a sensor rather than mutate runtime state. A priority, identity-scoped ingress lets the supervisor arbitrate cancellation independently and exactly once.

### Bounded presentation with transactional canonical projection

**Status:** accepted

**Rationale:** Native scrollback cannot provide transactional delivery guarantees. Canonical state remains authoritative; bounded contiguous publication ranges commit after known success and rebuild/degrade after stale or ambiguous delivery.

### Bounded teardown and best-effort terminal restoration

**Status:** accepted

**Rationale:** An indefinitely blocked OS write cannot be recovered in-process. Runtime shutdown must remain bounded and emergency restoration must not wait on normal output ownership.

### One-shot evidence-aware synthesis replaces deterministic surrender

**Status:** accepted

**Rationale:** Investigation can be real progress without mutation. The cutoff must share final-response state, prohibit tools/nudges, respect cancellation, and provenance-label any runtime fallback.

## Implementation Notes

### File Scope

- `core/crates/omegon/src/tui/native_publication.rs` —
- `core/crates/omegon/src/tui/mod.rs` —
