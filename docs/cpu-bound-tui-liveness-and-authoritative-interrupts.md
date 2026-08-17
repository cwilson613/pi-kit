---
id: cpu-bound-tui-liveness-and-authoritative-interrupts
title: "CPU-bound TUI liveness and authoritative interrupts"
status: decided
parent: authoritative-tui-input-and-bounded-presentation
tags: [tui, runtime, liveness, interrupts, tool-processes, scheduling, observability]
open_questions: []
dependencies: []
related:
  - authoritative-tui-input-and-bounded-presentation
---

# CPU-bound TUI liveness and authoritative interrupts

## Overview

Eliminate a confirmed native-TUI failure mode where a nonterminal tool subprocess leaves Omegon at approximately 100% CPU, the active turn never terminalizes, and keyboard/mouse input no longer produces observable interaction despite dedicated terminal-input ownership. Treat liveness as an end-to-end control-plane invariant: input acquisition, priority ingress, supervisor admission, tool-child terminalization, coordinator fairness, and presentation scheduling must each remain observable and bounded under a CPU-hot loop. Build a deterministic regression harness that simulates a permanently nonterminal child and proves Ctrl+C reaches generation-scoped revocation, CPU consumption remains bounded, the child process tree is reaped, and the TUI can restore or detach within a deadline.

## Research

### Confirmed incident and falsified guarantee

Live incident evidence on 2026-08-13: Omegon PID 97222 remained near 99–100% CPU for roughly two hours while the visible active turn was stuck. Its tool wrapper `/bin/bash -lc cargo fmt --all && just test-crate omegon && just clippy-changed && git diff --check` had been alive for about 56 minutes; child `git diff --check` had been alive for about 55 minutes. Killing the child and wrapper did not stop Omegon's CPU-hot loop. The terminal fd remained open. A process sample showed ordinary Tokio workers mostly asleep, so the hot path was likely another runtime/TUI-owned thread or an uninstrumented loop. No keyboard or mouse interaction was observable. This disproves the assumption that a dedicated Crossterm reader alone establishes end-to-end operator authority.

A later fleet inspection found six additional Omegon processes surviving for roughly two to four days with stdin, stdout, and stderr all revoked. They consumed approximately 900% aggregate CPU and about 1.3 GiB aggregate RSS. Exact process groups ignored `SIGTERM` and required `SIGKILL`. Removing them dropped machine load from approximately 10 to approximately 2. Separately, an attached session retained a quiet `git diff --check` descendant for roughly 16 hours. This resolves the design's earlier uncertainty: terminal detachment and child terminalization are independently non-bounded, and detached runtimes can remain CPU-hot even after their child process is removed. The first implementation slice therefore does not depend on identifying one historical hot function; it proves and enforces the lifecycle boundaries that were observably violated.

### Assumptions resolved for TDD

The reproducer need not recreate the exact historical repository state. The violated contracts are transport- and command-independent: a synthetic child can model a nonterminal reap, and an injected terminal-boundary event can model revoked descriptors. CI will assert state-machine steps, bounded poll/yield counts, and completion under Tokio's paused clock where possible; a small wall-clock ceiling remains only as a deadlock guard. CPU percentage itself is diagnostic evidence, not the deterministic assertion.

## Decisions

### End-to-end liveness ledger, not thread existence, defines operator authority

**Status:** accepted

**Rationale:** Instrument monotonic counters and timestamps at acquisition, priority enqueue, coordinator receipt, supervisor admission, token cancellation, child termination request, child reap, turn settlement, and terminal restoration. Each interrupt sequence receives an identity and exposes its last completed boundary. A dedicated input thread is necessary but no longer accepted as proof of authority.

### Hot-loop watchdog samples semantic progress and scheduler yield

**Status:** accepted

**Rationale:** Track loop iterations, elapsed time, semantic progress revision, input-drain revision, and cooperative yields. A loop exceeding a bounded iteration/time budget without any revision advance must emit diagnostics, yield, and escalate through a circuit breaker rather than continue CPU-hot. Rendering or token arrival does not count as runtime progress unless it advances a canonical boundary.

### Tool process trees have explicit terminalization ownership

**Status:** accepted

**Rationale:** Every spawned tool command records a process-group/session identity, timeout/cancellation owner, output-pump state, and terminal outcome. Cancellation closes pumps, signals the process group, waits for a bounded grace period, escalates to kill, reaps descendants, and settles the tool exactly once. EOF on one stream cannot disable timeout or cancellation.

### Deterministic liveness harness gates the fix

**Status:** accepted

**Rationale:** Build a test seam with fake input, synthetic nonterminal child, controlled clock/yield accounting, and supervisor identity. Acceptance requires bounded Ctrl+C admission, bounded child reap, exactly-once Revoked settlement, no sustained busy-spin, continued authoritative event draining, and bounded TUI teardown. Add an opt-in PTY black-box test for real Crossterm wiring.

### External emergency interrupt remains independent of the TUI process

**Status:** accepted

**Rationale:** Expose a daemon/IPC control-plane cancellation path that targets the same runtime identity and supervisor admission logic. This is a last-resort authority path when terminal input acquisition or the entire TUI process is compromised; it must not create a second cancellation state machine.

## Resolved Questions

- The exact historical hot function is not required to gate the first fix. Fleet evidence proves the process remained runnable after terminal revocation and after child removal; the implementation must instrument boundary progress so any future loop is attributable.
- Ctrl+C loss is treated as an end-to-end authority failure rather than assigned speculatively to one boundary. The liveness ledger must expose the last completed boundary.
- The historical `git diff --check` kernel/PTY condition is unknown, but the contract failure is known: ordinary Bash execution allowed an unbounded command and subsequently awaited reap without an independent deadline.
- A synthetic permanently nonterminal child plus injected terminal-boundary loss is sufficient to test the violated contracts without the historical repository state.
- CI asserts bounded state transitions and yield/poll budgets with controlled time; wall-clock limits serve only as deadlock guards, not CPU-performance thresholds.

## Implementation Notes

### File Scope

- `core/crates/omegon/src/tui/terminal_input.rs` —
- `core/crates/omegon/src/tui/mod.rs` —
- `core/crates/omegon/src/interactive_coordinator.rs` —
- `core/crates/omegon/src/main.rs` —
- `core/crates/omegon/src/tools/bash.rs` —
- `core/crates/omegon/src/tools/terminal.rs` —
- `core/crates/omegon/src/runtime_trace.rs` —
- `core/crates/omegon/tests` —

### Constraints

- Start with instrumentation and a deterministic reproducer before changing scheduling behavior.
- Do not count loop iterations, drawing, token arrival, or repeated unchanged polling as semantic progress.
- Priority interrupt handling must remain generation-scoped and use the existing supervisor arbitration path.
- No process-global signals by name or broad `pkill`; own and terminate the exact spawned process group/tree.
- All wait, channel-send, process-reap, and teardown paths require explicit deadlines or nonblocking behavior.
- The regression must assert bounded CPU/yield behavior without fragile absolute timing where possible.
- Preserve canonical conversation and tool audit evidence even when forcibly revoking a wedged child.
- Keep IPC emergency cancellation as another ingress to the same supervisor transition, not a parallel state machine.
