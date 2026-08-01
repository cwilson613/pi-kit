+++
id = "bash-long-running-execution"
tags = ["tools", "bash", "terminal", "timeouts"]
aliases = []
imported_reference = false

[publication]
enabled = false
visibility = "private"
+++

# Long-running command execution

## Overview

The native bash executor accepts caller-selected timeouts and correctly terminates process groups on timeout or cancellation. A separate provider, transport, or host-action boundary can nevertheless abort one blocking tool call after roughly ten minutes. Raising the bash argument does not override that outer deadline.

The immediate operator-safe workaround is to route commands expected to exceed ten minutes through the interactive `terminal` session surface, then monitor them with `terminal.read`. This preserves output and process lifetime without occupying one blocking tool call. The bash tool schema and repository directives must make that routing rule explicit.

## Status

exploring

## Open Questions

- [assumption] Every provider and client surface exposes the terminal session tools whenever bash is exposed.
- Which layer currently imposes the observed 600-second deadline: provider request handling, host-action transport, tool dispatch, or an outer harness watchdog?
- Should long-running bash calls be promoted automatically into managed terminal sessions, or should bash return a typed continuation/session handle?
- What cancellation and permission semantics must remain identical when execution moves from bash to terminal?
- How should ACP, TUI, CLI, and daemon clients observe and resume the same long-running operation?

## Research

- `core/crates/omegon/src/tools/bash.rs` honors `timeout_secs` directly and has process-group timeout/cancellation cleanup.
- `core/crates/omegon/src/tools/mod.rs` previously described the timeout without warning that an outer transport deadline may be shorter.
- A blocking bash invocation requesting 3600 seconds was terminated by the active tool boundary at 600 seconds, while the same command completed successfully in a managed terminal session.
- `core/crates/omegon/src/host_context.rs` has a separate ACP delegated-bash timeout default of 600 seconds, confirming timeout policy is currently distributed across execution surfaces.

## Candidate Direction

Create one long-running operation abstraction shared by bash, terminal, ACP delegation, and daemon/control-plane projections. A command should either complete synchronously within a bounded foreground budget or transition to a durable managed operation with a stable ID, streamed progress, explicit cancellation, terminal result retention, and consistent process-tree cleanup.

Do not silently detach arbitrary commands until permission checks, cancellation ownership, output retention, and client capability negotiation are specified and tested.
