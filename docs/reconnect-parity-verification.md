+++
title = "Reconnect and duplicate-action verification"
kind = "document"
status = "active"
tags = ["sessions", "web", "verification"]
imported_reference = false

[publication]
enabled = false
visibility = "private"
+++

# Reconnect and duplicate-action verification

The focused parity pass fixes snapshot subscription ordering and reconnect
projection of pending web-owned approvals. It verifies existing durable input,
cleave approval, delegate result, and operator-agency behavior.

Both WebSocket routes now subscribe before building and sending the initial
snapshot. A completion event emitted during either await remains available to
the new subscriber. Snapshot and live state can overlap; the browser deduplicates
permission prompts by request ID. General exactly-once transcript replay is not
established by this change.

Web-owned pending tool approvals retain redacted tool/path metadata for the live
session. A reconnect snapshot can reconstruct an actionable prompt. Answers,
authoritative idle state, and session reset clear retained approvals. Historical
and replaced sessions cannot expose or answer another session's pending request.
The additive `pending_permissions` field is documented in
[the web API contract](web-api.openapi.yaml).

Permission responders keep their existing first-consumer ownership between TUI
and web. Replay covers requests already captured by a web client. A tool approval
emitted while no web client owns its responder is not made web-recoverable by this
change; capturing it globally would require a separate ownership decision.

| Case | Evidence and boundary |
|---|---|
| Snapshot/live handoff | Deterministic sink emits completion inside snapshot delivery through the helper used by both real WebSocket handlers. |
| Pending web approval | Snapshot reconstruction, one answer, duplicate-answer rejection, redaction, historical isolation, TUI ownership, and idle/reset cleanup. |
| Browser approval | Node executes the actual embedded UI functions against a DOM fixture; repeated snapshots and live overlap retain one actionable prompt. |
| Durable duplicate input | Repeated admission identity across session-authority reopen retains one admission. |
| Legacy WebSocket retry | `user_prompt` has no client submission ID. Two receipts are separate input, even when text matches. |
| Cleave approval | Persisted workstream state reconstructs an actionable approval. |
| Delegate completion | Results remain retrievable after client detach within the same running runtime; repeated reads do not rerun the child. |
| Operator agency | Existing tests cover supervisor completion without AgentEnd, authoritative idle recovery, and successful second-turn submission. |

Do not automatically retry a legacy WebSocket prompt on the assumption that the
server will deduplicate it. A future retry-safe protocol needs a stable caller
submission ID, an admission acknowledgement, and a defined deduplication scope.
Deduplicating prompt text would incorrectly suppress intentional repeated input.

Delegate verification covers client detach, not daemon restart. The snapshot
handoff fixture exercises production serialization and subscription code without
a live socket or GUI; it does not claim a browser-network end-to-end run.
