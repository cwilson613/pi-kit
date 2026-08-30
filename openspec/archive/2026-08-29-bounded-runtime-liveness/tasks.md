# Bounded runtime liveness — Tasks

Dependencies: group 1 before groups 2–3; groups 2–3 before group 4.

## 1. Deterministic liveness contracts and red tests
<!-- specs: runtime-liveness/terminal-loss, runtime-liveness/process-terminalization -->

- [x] 1.1 Add an injected process-terminalization test seam with controlled TERM, KILL, reap, and output-pump outcomes.
- [x] 1.2 Add a failing regression proving a non-reaping child cannot hold cancellation beyond the reap budget.
- [x] 1.3 Add an injected terminal-boundary seam and a failing regression proving idle and active sessions terminalize without another draw/input event.
- [x] 1.4 Assert exactly-once settlement and monotonic last-boundary evidence.

## 2. Bounded tool process terminalization
<!-- specs: runtime-liveness/process-terminalization -->

- [x] 2.1 Implement a staged process terminalizer for dedicated Bash process groups.
- [x] 2.2 Give ordinary Bash execution a finite default absolute deadline.
- [x] 2.3 Bound leader reap and output-pump closure independently after KILL.
- [x] 2.4 Preserve indeterminate terminalization evidence without blocking the runtime.

## 3. Authoritative terminal-loss shutdown
<!-- specs: runtime-liveness/terminal-loss -->

- [x] 3.1 Route permanent terminal-boundary loss directly to generation-scoped supervisor revocation.
- [x] 3.2 Cancel and boundedly join session-owned tasks and child terminalization owners.
- [x] 3.3 Persist retained state and settle the session without requiring presentation progress.
- [x] 3.4 Emit the last completed liveness boundary for teardown faults.

## 4. Integration and verification
<!-- specs: runtime-liveness/terminal-loss, runtime-liveness/process-terminalization -->

- [x] 4.1 Add opt-in PTY coverage for real terminal detachment.
- [x] 4.2 Run focused tests, `just test-crate omegon`, and `just clippy-changed`.
- [x] 4.3 Reconcile scenario evidence and archive only after merged verification.
