+++
id = "semantic-stream-watchdog"
status = "implemented"
tags = ["providers", "streaming", "watchdog", "codex", "performance"]
aliases = []
imported_reference = false

[publication]
enabled = false
visibility = "private"
+++

# Semantic stream watchdog

## Overview

Prevent a transport-active but semantically stalled provider stream from retaining an unbounded turn lease. Transport liveness, semantic progress, and absolute turn duration are separate signals. A heartbeat can prove that a socket is alive; it cannot reset semantic-progress time.

## Observed failure

A local Codex-backed session remained in `streaming answer` while one Tokio worker consumed a CPU core. The existing SSE and consumer timeouts were per-receive idle timeouts. Repeated unknown or no-op upstream events were normalized to `LlmEvent::Start`, so every event rearmed the consumer timeout despite producing no text, reasoning, tool call, or terminal result. The SSE framer also copied its remaining buffer for every parsed line, amplifying a noisy stream into allocation and copy pressure.

## Contract

### Signal classes

- **Transport activity**: bytes or an explicit provider heartbeat. It updates diagnostics only.
- **Semantic progress**: non-empty text/reasoning/tool deltas, a completed tool call, or a meaningful novel boundary. It resets the semantic-progress deadline.
- **Terminal activity**: completion or error. It ends the stream.
- **Absolute turn deadline**: a monotonic deadline that no event can reset.

`LlmEvent::Start` remains the one-time stream-open semantic event for compatibility. `LlmEvent::TransportHeartbeat` is the explicit no-progress event and must never extend semantic or absolute deadlines.

### Watchdog behavior

The consumer tracks `last_semantic_progress`, transport heartbeat count, current semantic phase, and turn start. Each receive waits only for the remaining semantic budget, not a fresh full budget. On expiry it aborts with bounded diagnostics naming the phase, no-progress duration, and heartbeat count.

The implementation preserves the phase-aware semantic budgets and adds a non-resettable absolute deadline. The absolute deadline defaults to 1,200 seconds and can be overridden with `OMEGON_LLM_ABSOLUTE_TIMEOUT_SECS`; values below 60 seconds are rejected in favor of the default.

### Codex normalization

- Known text, reasoning, tool, boundary, completion, and error events map to their semantic variants.
- Explicit liveness/no-op events map to `TransportHeartbeat`.
- Unknown event kinds are counted and logged with bounded cardinality; they may map to `TransportHeartbeat` but never to `Start`.
- Repeated identical phase events do not constitute semantic progress.

### SSE framing

The parser must consume bytes with a cursor or `BytesMut`-style split, avoiding allocation of both the current line and the complete remainder for every line. Framing changes must preserve CRLF handling, multi-line `data:` events, partial chunks, and terminal flush behavior.

## Decisions

1. Distinguish transport heartbeat from stream start in the provider-neutral event contract.
2. Base semantic timeout on elapsed time since semantic progress, not elapsed time since any channel receive.
3. Preserve existing phase-aware timeout values for the first bounded fix.
4. Add an absolute non-resettable turn deadline as a separate guard.
5. Optimize SSE framing independently; performance reduction does not substitute for lifecycle bounds.
6. Diagnostics must be bounded and must not include unbounded upstream payloads or high-cardinality event names.

## Resolved questions and assumptions

- Existing phase-aware idle budgets remain the initial semantic-progress budgets; production telemetry can justify later tuning without changing the contract.
- The absolute deadline defaults to 1,200 seconds. Operators may override it with `OMEGON_LLM_ABSOLUTE_TIMEOUT_SECS`, subject to a 60-second minimum.
- `response.created` and the first `response.in_progress` establish stream start. Content-part markers and unknown/no-op Codex event kinds are transport heartbeats. Text, reasoning, and tool payloads remain owned by the full semantic parser.
- Heartbeat floods are bounded by elapsed semantic time and the absolute deadline, not an event-count breaker. This avoids rejecting a healthy but noisy transport before the time contract expires.
- SSE framing now avoids remainder copies, but this change does not add a maximum frame-size guard. Existing HTTP/body controls remain the current aggregate defense; a hostile-stream frame cap is separate follow-up scope.
- The design does not depend on observing a particular upstream flood event name: unknown Codex events are transport-only by default.

## Implementation scope

- `core/crates/omegon/src/bridge.rs`: provider-neutral heartbeat event.
- `core/crates/omegon/src/providers.rs`: Codex normalization, bounded unknown-event diagnostics, allocation-bounded SSE framing.
- `core/crates/omegon/src/loop.rs`: semantic and absolute watchdogs with diagnostics.
- Focused tests in the owning modules for heartbeat floods, semantic reset, hard deadline, partial SSE chunks, CRLF, and unknown-event bounds.

## Acceptance criteria

1. Continuous `TransportHeartbeat` events cannot keep a stream alive beyond its semantic deadline.
2. Non-empty semantic deltas reset only the semantic deadline.
3. No event resets the absolute deadline.
4. Unknown Codex events cannot become repeated `Start` events.
5. Stall diagnostics report bounded heartbeat/event counts without embedding unbounded payloads.
6. SSE framing does not copy the entire remainder once per parsed line.
7. Existing provider stream tests, `just test-crate omegon`, and `just clippy-changed` pass.
8. An adversarial review finds no event class that accidentally grants an unbounded lease.
