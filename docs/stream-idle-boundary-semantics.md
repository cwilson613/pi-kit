---
id: stream-idle-boundary-semantics
title: "Auditable stream idle and boundary semantics"
status: seed
tags: [runtime, providers, streaming, audit, operator-agency]
open_questions:
  - "[assumption] Every provider adapter can classify a post-block boundary as terminal, more-content/reasoning expected, or unknown without provider-name checks in the shared loop."
  - "[assumption] Existing AgentEvent projections and audit sinks can carry structured watchdog transitions without leaking chain-of-thought or sensitive provider payloads."
  - "[assumption] A default 90s active threshold plus 90s ambiguity grace is tolerable across supported cloud and local providers; provider/profile overrides remain bounded."
  - "What assumptions is this design making that haven't been stated? In particular, do transport heartbeats, usage events, and repeated boundary markers count as semantic progress for any supported provider?"
  - "[assumption] Provider adapters can reliably classify explicit boundaries as MoreReasoning, MoreContent, Terminal, or Unknown without inspecting hidden reasoning content."
  - "[assumption] A 180-second default cap for unknown post-block silence is long enough for current providers while materially improving recovery UX."
  - "[assumption] Existing AgentEvent/audit sinks can persist structured watchdog decisions without exposing raw chain-of-thought or requiring a new telemetry backend."
  - "[assumption] A 250ms cooperative teardown grace is sufficient before forced abort while still feeling immediate to operators; validate with local, cloud, tool-child, and extension-child cancellation tests."
  - "What `/exit` confirmation policy should apply across TUI, CLI, web, ACP, and IPC, and which explicit force-exit form bypasses confirmation?"
dependencies: []
related: []
---

# Auditable stream idle and boundary semantics

## Overview

Replace implicit post-block timeout promotion with provider-normalized boundary expectations, dual transport/semantic-progress clocks, bounded ambiguity escalation, and auditable operator extensions. This prevents unexplained silence from inheriting the full reasoning budget while preserving providers that interleave reasoning and visible replies.

## Research

### Current implementation evidence

`core/crates/omegon/src/loop.rs` currently maps TextEnd/ThinkingEnd/ToolCallEnd to one AmbiguousSilent phase, losing why the boundary occurred. StreamIdlePolicy then grants that phase the reasoning budget. `core/crates/omegon/src/providers.rs` already has adapter-local phase knowledge (for example Anthropic content_block_start/stop and SsePhaseGate), but provider and consumer watchdogs can disagree. The design must establish one normalized semantic source and prevent independent blind promotion.

### Agency gap in current cancellation path

The current `CancelActiveTurn` path is cooperative: TUI input routes Esc/Ctrl+C to a runtime command and `loop.rs` selects the token during LLM streaming, but this alone does not prove descendants terminate, prevent late event publication, or make readiness independent of worker return. The required semantic is supervisor-enforced revocation with generation-scoped publication/mutation authority and owned child handles.

## Decisions

### Normalize provider boundaries before the shared loop

**Status:** accepted

**Rationale:** The adapter owns protocol semantics. The shared loop consumes BoundaryExpectation::{MoreReasoning, MoreContent, Terminal, Unknown} and never branches on provider names.

### Track transport activity and semantic progress separately

**Status:** accepted

**Rationale:** Bytes, pings, empty deltas, and repeated phase markers prove connection liveness but cannot extend a turn indefinitely. Only content/tool/reasoning progress or a meaningful authoritative transition resets semantic deadlines.

### Make every watchdog transition auditable

**Status:** accepted

**Rationale:** Emit structured events for phase changes, threshold selection, ambiguity warnings, operator extensions, timeout/EOF terminalization, and retry disposition. Include turn/provider/model IDs, prior/new phase, boundary expectation, elapsed durations, configured budgets, evidence counters, and provenance; exclude raw reasoning and secrets.

### Bound ambiguity and operator extensions

**Status:** accepted

**Rationale:** Unknown post-block silence warns at 90s and terminalizes at 180s by default. Operator keep-waiting grants a bounded interval, records actor/surface/time/reason, and cannot disable the absolute turn ceiling.

### Operator interrupt is enforced teardown, not advisory cancellation

**Status:** accepted

**Rationale:** Esc/Ctrl+C transfers authority to the supervisor immediately. The supervisor closes admission for the active turn, revokes its capability to publish events or mutate session state, cancels provider/tool/extension children, and force-aborts non-cooperative tasks after a short bounded grace. Provider acknowledgement is not required for the TUI to become ready.

### Separate turn interruption from destructive process exit

**Status:** accepted

**Rationale:** Esc/Ctrl+C terminalizes only the active turn and preserves queued/draft work. `/exit` during an active turn is a destructive session/process operation: require confirmation by default, then revoke the turn, terminate descendants, flush bounded durable state, restore terminal modes, and exit without waiting indefinitely for upstream cooperation.

### Extension process lifetime has explicit supervisor ownership

**Status:** accepted

**Rationale:** Fold the extension hang breakfix into authoritative terminalization. Cloned RPC/polling handles may observe services but must not own child lifetime. Full session shutdown closes admission and respawn, cancels polling, gracefully closes then kills and reaps canonical extension children; turn interruption only revokes turn-owned extension calls.

## Open Questions

- [assumption] Every provider adapter can classify a post-block boundary as terminal, more-content/reasoning expected, or unknown without provider-name checks in the shared loop.
- [assumption] Existing AgentEvent projections and audit sinks can carry structured watchdog transitions without leaking chain-of-thought or sensitive provider payloads.
- [assumption] A default 90s active threshold plus 90s ambiguity grace is tolerable across supported cloud and local providers; provider/profile overrides remain bounded.
- What assumptions is this design making that haven't been stated? In particular, do transport heartbeats, usage events, and repeated boundary markers count as semantic progress for any supported provider?
- [assumption] Provider adapters can reliably classify explicit boundaries as MoreReasoning, MoreContent, Terminal, or Unknown without inspecting hidden reasoning content.
- [assumption] A 180-second default cap for unknown post-block silence is long enough for current providers while materially improving recovery UX.
- [assumption] Existing AgentEvent/audit sinks can persist structured watchdog decisions without exposing raw chain-of-thought or requiring a new telemetry backend.
- [assumption] A 250ms cooperative teardown grace is sufficient before forced abort while still feeling immediate to operators; validate with local, cloud, tool-child, and extension-child cancellation tests.
- What `/exit` confirmation policy should apply across TUI, CLI, web, ACP, and IPC, and which explicit force-exit form bypasses confirmation?

## Implementation Notes

### Constraints

- No provider-name branching in the shared loop
- No raw reasoning text, credentials, or full tool arguments in watchdog audit records
- Heartbeats and empty deltas never reset semantic progress deadlines
- Operator extension remains bounded by an absolute turn ceiling
- Stream EOF without an authoritative terminal event remains abnormal
- Esc/Ctrl+C revokes the active turn immediately; readiness never depends on provider acknowledgement
- Revoked turn IDs cannot publish message/tool/terminal events or mutate conversation/session state
- Supervisor owns durable state and all spawned child handles for provider, tool, terminal, delegate, extension, MCP, and subprocess work
- Cooperative cancellation grace is short and bounded; remaining child tasks/process groups are force-aborted/killed afterward
- TUI emits authoritative cancelled TurnEnd/AgentEnd and becomes input-ready before background cleanup finishes
- Late events from revoked generations are dropped and audited
- Queued prompts and editor drafts survive turn interruption
- `/exit` during an active turn requires confirmation by default and performs destructive session/process teardown
- Confirmed exit uses bounded state flush and terminal restoration; it does not wait indefinitely for upstream cleanup
- A force-exit command or repeated explicit confirmation may bypass the prompt only when provenance is operator-authenticated
