---
id: cpu-bound-tui-liveness-and-authoritative-interrupts
title: "CPU-bound TUI liveness and authoritative interrupts"
status: exploring
parent: authoritative-tui-input-and-bounded-presentation
tags: [tui, runtime, liveness, interrupts, tool-processes, scheduling, observability]
open_questions:
  - "Which exact thread and loop consumed the CPU during the incident: terminal input pump, TUI coordinator scheduling, provider/tool completion polling, frame scheduling, or another background worker?"
  - "At which boundary did Ctrl+C disappear: Crossterm acquisition, priority channel send, identity snapshot lookup, coordinator receive, supervisor admission, cancellation-token propagation, or child-process reap?"
  - "Why did `git diff --check` remain nonterminal for 55 minutes, and what process/PTY/file-descriptor condition prevented its wrapper from completing?"
  - "[assumption] The incident can be reproduced deterministically with a synthetic permanently nonterminal tool subprocess and does not require the exact feature/tui-presentation-settings repository state."
  - "[assumption] Bounded CPU usage and interrupt latency can be asserted in CI without relying on wall-clock-sensitive terminal integration tests."
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

## Open Questions

- Which exact thread and loop consumed the CPU during the incident: terminal input pump, TUI coordinator scheduling, provider/tool completion polling, frame scheduling, or another background worker?
- At which boundary did Ctrl+C disappear: Crossterm acquisition, priority channel send, identity snapshot lookup, coordinator receive, supervisor admission, cancellation-token propagation, or child-process reap?
- Why did `git diff --check` remain nonterminal for 55 minutes, and what process/PTY/file-descriptor condition prevented its wrapper from completing?
- [assumption] The incident can be reproduced deterministically with a synthetic permanently nonterminal tool subprocess and does not require the exact feature/tui-presentation-settings repository state.
- [assumption] Bounded CPU usage and interrupt latency can be asserted in CI without relying on wall-clock-sensitive terminal integration tests.

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
