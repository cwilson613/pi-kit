+++
title = "Bug investigation: Leaked background processes, terminals, and subprocess trees"
tags = ["bug", "processes", "terminal", "serve", "runtime", "investigation"]
+++

# Bug investigation: Leaked background processes, terminals, and subprocess trees

**Status:** Suspected; evidence and sources not yet localized  
**Suggested branch:** `fix/runtime-process-lifecycle-audit`  
**Primary surfaces:** Process-spawning facilities and shutdown

## Assignment brief

Create an evidence-first inventory of all in-core process-spawning facilities, their lifecycle owners, intended lifetimes, registries, and cleanup paths. Add a read-only diagnostic projection and use it to confirm concrete leaks before consolidating supervisors or changing shutdown policy. Do not classify intentionally persistent or externally owned processes as leaks.

## Definitions

- **Leak:** Omegon-owned process remains alive beyond its declared lifecycle.
- **Orphan:** Live child has no active lifecycle owner.
- **Stale record:** Registry reports a resource active although its process exited.
- **Untracked child:** Live Omegon-owned process has no registry record.
- **Persistent service:** Explicitly allowed to outlive its creating turn/session and remains adoptable.

## Candidate facilities

Audit `serve`, interactive `terminal`, Bash, validation, delegates, cleave workers, provider/local-inference helpers, extensions/MCP, code-act proxies, sandbox/container helpers, browser/auth flows, daemon restart handoff, file watchers, schedulers, and recurring loops.

## Scope

- Spawn-site and ownership inventory.
- Turn/task/session/project/daemon/persistent lifetime taxonomy.
- Registry versus observed OS-state reconciliation.
- Cancellation, timeout, failure, restart, disconnect, and shutdown cleanup.
- Read-only diagnostics with secret-safe labels.
- Representative leak reproduction and regression tests.

## Non-goals

- Assuming one common root cause.
- Killing processes by command-name matching or unverified PID.
- Requiring one monolithic supervisor before evidence supports it.
- Exposing command arguments or environments in diagnostics.
- Treating daemon work surviving a client disconnect as inherently leaked.

## Investigation targets

Search for `Command::new`, `spawn`, PTY creation, process groups, cancellation tokens, kill-on-drop, registry insertion/removal, reaping, heartbeat, leases, shutdown hooks, restart adoption, `pkill`, and `killall`. For every spawn site record owner, process boundary, persistence policy, normal terminal state, and cleanup paths.

## Diagnostic projection

Report, without mutation:

- runtime object ID and facility;
- owning session/turn/task/project;
- PID and process-group/session ID where available;
- redacted executable label;
- spawn time and last heartbeat/activity;
- intended lifetime and persistence policy;
- registry state and observed process state;
- termination request and cleanup outcome;
- exit reason/status when known;
- whether descendants remain.

PID reuse must be guarded with ownership evidence such as start time, lease identity, or platform process identity.

## Lifecycle invariants

1. Every child has exactly one lifecycle owner or explicit external ownership.
2. Every owner defines normal and exceptional terminal states.
3. Cancellation targets the descendant boundary, not only the immediate PID.
4. Handle drop cannot silently detach unless persistence is explicit.
5. Session shutdown cleans session children before terminal restoration.
6. Restart handoff cannot create dual ownership.
7. Persistent services use leases/adoption rules.
8. Registries converge with process state after crashes and startup.
9. Cleanup is idempotent after natural exit.
10. Broad process-name killing is prohibited when tracked identity exists.

## Implementation sequence

1. Build the spawn-site ownership inventory.
2. Define a common read-only diagnostic DTO and facility adapters.
3. Capture before/after baselines for representative workflows.
4. Confirm and rank actual leak classes.
5. Fix one ownership gap at a time with facility-specific tests.
6. Add bounded shutdown aggregation and survivor reporting.
7. Add startup reconciliation for persisted registries.
8. Decide from evidence whether a shared process supervisor is warranted.

## Acceptance criteria

- Every in-core spawn site has documented ownership and lifetime.
- Natural terminal exit is reaped and removed from active listings.
- Stop/cancel/timeout removes children and grandchildren.
- Repeated service start/stop returns to baseline.
- Normal/error shutdown cleans session-scoped resources.
- Restart produces one owner per surviving resource.
- Persistent services survive only by explicit policy and remain discoverable.
- Startup detects stale records and PID reuse safely.
- Diagnostics compare registry and OS state without exposing secrets.
- Concurrent sessions clean only their own process trees.

## Regression plan

Exercise repeated `serve` lifecycle, natural and forced terminal exit, grandchildren under Bash/validation, delegate cancellation during startup/execution, restart with active resources, normal/error application exit, client disconnect from daemon work, unexpected extension exit, stale records with reused PIDs, and two concurrent sessions.

## Validation

Run each facility's focused suite and process-boundary smoke tests, then:

```bash
cargo test -p omegon <process-lifecycle-filter>
just clippy-changed
git diff --check
```

## Dependencies and conflict risks

The Bash timeout design shares process-group and cleanup primitives. This branch should establish evidence and common projection first; avoid broad concurrent edits to terminal, serve, daemon, delegation, extension, loop, and shutdown modules without scoped ownership.

## Definition of done

The spawn inventory and diagnostic projection exist, confirmed leaks have bounded fixes and regressions, intentional persistence is distinguishable, shutdown reports survivors, stale state reconciles safely, and all focused validation passes.
