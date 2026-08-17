# Design: bounded runtime liveness

## Source design

`docs/cpu-bound-tui-liveness-and-authoritative-interrupts.md`

## Architecture

Introduce two small state-machine seams before production behavior changes:

1. `TerminalLossCoordinator` accepts an injected terminal-boundary fault, snapshots the current runtime identity, and delegates revocation to the existing supervisor ingress. It owns bounded teardown completion but not a second cancellation state machine.
2. `ProcessTerminalizer` owns TERM, grace, KILL, reap budget, output-pump closure, and exactly-once settlement for one dedicated process group.

Both emit a shared liveness sequence record with monotonic boundary revisions. Controlled tests use fake supervisor/process implementations and paused Tokio time; OS-level PTY coverage remains opt-in.

## Safety

- Signal only the dedicated process group created for the child.
- Do not use process-name matching or global signals.
- Preserve audit evidence if terminalization is indeterminate.
- A reap deadline returns control to the runtime; it does not claim the process is absent.
- Emergency IPC cancellation enters the same supervisor transition.

## First TDD boundary

The deterministic seams are now implemented and covered by focused tests. Production terminal-boundary reception enters the coordinator-owned cancellation path without ordinary command-channel admission, and Bash cancellation retains timeout ownership through pipe EOF and leader reap.
