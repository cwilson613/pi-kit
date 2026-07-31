+++
id = "d6d13479-a20c-4586-a983-df1770a7e51d"
kind = "document"
title = "Native Extension Generation Promotion"
status = "exploring"
tags = ["extensions", "development", "iteration"]
aliases = ["extension-hot-reload"]
imported_reference = false

[publication]
enabled = false
visibility = "private"

[data]
dependencies = []
open_questions = [
  "[assumption] The tool registry can atomically replace all registrations owned by one extension generation.",
  "[assumption] Resolved bootstrap secrets and configuration can be safely retained and replayed for the session lifetime.",
  "What admission behavior should calls receive while an extension generation drains?",
  "What is the atomic rollback boundary across tools, commands, widgets, and event subscriptions?",
  "How is model-visible tool inventory updated safely between or during inference turns?",
  "Should the first implementation support only targeted reload or also transactional reload-all?",
  "How should optional state transfer work for stateful extensions?",
  "Which shared control-plane operation provides TUI, CLI, ACP, and Web parity?",
]
related = []
+++

# Native Extension Generation Promotion

## Overview

Omegon can currently discover and spawn native extensions at startup, while `/extension reload`, `/extension restart`, and `/runtime refresh` only refresh skills and inspect extension candidates. A rebuilt native extension therefore remains unavailable until the session restarts: the current generation retains the old child process, RPC handle, tool schemas, widgets, and event subscriptions.

Introduce a generation-aware extension supervisor that can safely promote one rebuilt native extension without severing the harness session. The supervisor must drain the old generation, validate and bootstrap a replacement process, atomically swap its registrations, and either complete promotion or retain a usable prior generation.

## Current Evidence

- `core/crates/omegon/src/setup.rs` discovers, spawns, bootstraps, and registers extensions during startup.
- `TuiApp::refresh_runtime_substrate` in `core/crates/omegon/src/tui/mod.rs` explicitly reports that running extension processes and widgets are not restarted.
- The runtime refresh candidate path can inspect manifests but cannot promote candidate extension generations.
- Extension credentials are harness-managed and may not exist in the child process environment, so reload must reuse the resolved in-memory bootstrap material rather than depend on environment variables.
- Rebuilding an extension in place updates its binary on disk but does not alter the already-running child or model-visible tool registry.

## Desired Contract

A targeted reload such as `/extension reload omegon-omada` should:

1. Resolve the currently active extension generation and reject concurrent promotions for the same extension.
2. Stop admission of new calls and allow bounded in-flight calls to drain.
3. Re-read and validate the manifest, binary, SDK contract, and declared capabilities.
4. Spawn a candidate child without disturbing the active generation.
5. Replay resolved configuration and secrets through the normal bootstrap RPC without exposing secret values.
6. Run startup health checks and fetch candidate tool schemas.
7. Validate tool names, schemas, commands, widget registrations, and event subscriptions for conflicts.
8. Atomically promote the candidate RPC handles and registrations as a new generation.
9. Retire the old child and subscriptions after promotion.
10. Preserve the old generation if any pre-promotion step fails; report a degraded state if failure occurs after the swap boundary.

The command must return structured generation data: extension id, old and new generation, promotion status, drained calls, changed tools, diagnostics, and rollback outcome. TUI, CLI, ACP, and Web surfaces should invoke the same lifecycle operation rather than implementing independent reload behavior.

## Safety and Concurrency Requirements

- Extension calls bind to a generation for their entire lifetime.
- Reload has a finite drain/startup timeout and cannot wait indefinitely.
- Secrets remain redacted and are never copied into logs, command arguments, or operator-visible diagnostics.
- Candidate tools are unavailable until schema validation and atomic promotion complete.
- Event/widget subscriptions carry generation ownership so retiring a generation cannot leave duplicate consumers.
- Process termination targets the tracked child PID only.
- Failed candidates are terminated and reaped.
- Reloading one extension must not interrupt unrelated extensions or the active session transport.

## Initial Scope

Ship explicit, targeted reload for native process extensions. Automatic filesystem watching can follow after promotion semantics are proven. Do not include hot replacement of the Omegon executable or in-process compiled components.

## Acceptance Criteria

- Rebuilding a native extension and invoking targeted reload exposes added or removed tools in the same session.
- An active call on the old generation either completes normally during drain or receives a typed reload timeout/cancellation result.
- Invalid manifests, startup failures, bootstrap rejection, and malformed tool schemas leave the previous generation callable.
- A successful promotion leaves no old child process or duplicate event/widget subscription.
- Resolved secrets are replayed without appearing in logs or lifecycle responses.
- Runtime status identifies active and candidate generation, PID, health, and last promotion outcome per extension.
- Focused tests cover successful promotion, rollback before swap, drain timeout, concurrent reload rejection, tool-schema replacement, and subscription cleanup.
- An integration test rebuilds or substitutes a fixture extension, reloads it, and calls a newly introduced tool without restarting the harness.

## Open Questions

- [assumption] The tool registry can support an atomic owner-scoped replacement without reconstructing the whole agent runtime. Validate the current registry ownership and locking model.
- [assumption] Resolved extension bootstrap material can be retained safely for replay for the session lifetime. Determine whether the secret/config cache already has the necessary ownership and zeroization semantics.
- Should calls arriving during drain fail fast with a typed `extension_reloading` error, queue behind promotion, or route to the old generation until the swap?
- What is the precise rollback boundary once tool registrations, commands, widgets, and event receivers span multiple stores?
- Can model-visible tools be updated between turns without rebuilding provider context, and what should happen if reload occurs during an inference turn?
- Should the first implementation support only one named extension, or also a transactional reload-all operation?
- How should stateful extensions export/import optional runtime state across generations without making state transfer mandatory for ordinary extensions?
- Which control-plane operation and event types provide parity across TUI, CLI, ACP, and Web surfaces?
