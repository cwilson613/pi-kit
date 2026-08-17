---
state: implementing
---
# Bounded runtime liveness

## Intent

Ensure an interactive Omegon runtime cannot survive terminal attachment loss indefinitely and an owned tool process cannot hold a turn indefinitely during cancellation, timeout, or reap. Make both boundaries deterministic and observable before changing scheduling behavior.

## Scope

- Terminal-boundary loss enters the existing generation-scoped supervisor revocation path and completes bounded teardown.
- Bash process-group termination has independent TERM, KILL, and reap deadlines and settles exactly once.
- Liveness evidence records the last completed boundary without treating rendering, token arrival, or unchanged polling as progress.
- Deterministic tests use injected boundary faults and synthetic nonterminal process behavior.

## Non-goals

- Fleet-wide runtime leases and orphan cleanup.
- Codescan or transcript retention policy.
- Replacing the existing interrupt supervisor state machine.
- Claiming in-process terminal restoration when the OS blocks writes forever.

## Success criteria

- Terminal loss revokes the active generation and reaches a bounded terminal session outcome.
- Cancellation cannot await child reap forever after TERM/KILL.
- Synthetic regressions prove exactly-once settlement and bounded polling/yield behavior.
- Existing canonical conversation and audit evidence remain retained.
