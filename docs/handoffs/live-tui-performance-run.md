+++
title = "Live TUI Performance Run Handoff"
tags = ["handoff","tui","performance","diagnostics"]
+++

# Live TUI Performance Run Handoff

# Live TUI Performance Run Handoff

## Objective

Resume diagnosis of operator-control loss, scrolling lag, stuttering, and hitching while the Omegon TUI live-renders conversation output.

Do not claim the issue is solved from scheduler tests alone. The next decision must be based on trace evidence from a live checkout-local run.

## Repository and execution provenance

Repository checkout:

```text
/Users/wilson/workspace/styrene-labs/omegon-secundus
```

Run the instrumented binary with:

```bash
just trace-tui
```

This recipe must:

1. Build this checkout's `target/dev-release/omegon`.
2. Execute that binary by absolute path, not the globally linked `omegon` command.
3. Set `OMEGON_EXPECTED_CHECKOUT` to the physical checkout path.
4. Remove the previous trace before starting.
5. Write the new trace to:

```text
.omegon/debug/tui-runtime.jsonl
```

Do not analyze a trace until its `build.executable` and `build.manifestDir` identify this checkout.

Expected provenance shape in every trace window:

```json
{
  "build": {
    "packageVersion": "0.29.0-dev",
    "gitSha": "<build-time SHA and dirty marker>",
    "gitDescribe": "<git describe>",
    "buildDate": "<commit date>",
    "buildProfile": "<Cargo profile>",
    "manifestDir": "/Users/wilson/workspace/styrene-labs/omegon-secundus/core/crates/omegon",
    "executable": "/Users/wilson/workspace/styrene-labs/omegon-secundus/target/dev-release/omegon",
    "processId": 12345,
    "workingDirectory": "/Users/wilson/workspace/styrene-labs/omegon-secundus"
  }
}
```

The absolute executable and manifest paths are the authoritative checkout checks. The SHA supplies source lineage but may reflect Cargo build-script rerun behavior.

## Reproduction procedure

During the live run:

1. Start or continue a conversation with enough history to exercise transcript layout.
2. Request a long response that streams continuously.
3. While it streams, repeatedly scroll upward and downward.
4. Stay detached from the transcript tail for at least 15–30 seconds.
5. Exercise keyboard and mouse-wheel scrolling if both are relevant.
6. Note approximately when visible stalls occur and whether they happen while streaming, while detached, or during both.
7. Exit Omegon normally so the run ends cleanly.

The trace emits approximately one JSONL record per five-second window unless `OMEGON_TUI_TRACE_WINDOW_SECS` overrides the interval.

## Trace schema

The live trace uses schema version 2. Each window includes:

### Build provenance

- `build.packageVersion`
- `build.gitSha`
- `build.gitDescribe`
- `build.buildDate`
- `build.buildProfile`
- `build.manifestDir`
- `build.executable`
- `build.processId`
- `build.workingDirectory`

### Draw performance

- `frames`
- `urgentFrames`
- `backgroundFrames`
- `dirtyPassesWithoutDraw`
- `drawUs.samples`
- `drawUs.total`
- `drawUs.mean`
- `drawUs.p50`
- `drawUs.p95`
- `drawUs.p99`
- `drawUs.max`
- `slowFramesOver16ms`
- `slowFramesOver33ms`
- `slowFramesOver100ms`

### Input responsiveness

- `operatorInputs`
- `inputBatches`
- `inputsPerBatch`
- `inputToFrameUs.samples`
- `inputToFrameUs.mean`
- `inputToFrameUs.p50`
- `inputToFrameUs.p95`
- `inputToFrameUs.p99`
- `inputToFrameUs.max`

The input timestamp is captured before terminal-event handling, so `inputToFrameUs` includes input-handler work plus the subsequent draw.

### Agent-event pressure

- `agentEvents`
- `agentDrainPasses`
- `agentBudgetHits`
- `agentEventsPerDrain`

### Conversation state

- `conversationSegments`
- `conversationScrollOffset`
- `streamingFrames`
- `detachedFrames`

These fields permit correlation among history size, live streaming, detached scrolling, and draw latency.

### Draw phases and runtime contention (schema v3)

- `drawCallbackUs`, `backendUs`
- `preparationUs`, `backgroundFillUs`
- `conversationProjectionUs`, `conversationRenderUs`, `remainingRenderUs`
- `processRssMb`
- `managedTerminalSessions`, `runningTerminalSessions`
- `extensionWidgets`, `extensionRpcHandles`, `extensionPollingHandles`, `widgetReceivers`

The contention values are sampled only when a trace window is flushed, keeping the diagnostic path low overhead. They identify correlation, not causation: a high terminal or extension count beside a slow window narrows the next investigation but does not prove that subsystem caused the stall.

## Wild-session debug configuration

Installed or normal development binaries can collect the same trace without using the checkout-specific `just trace-tui` recipe:

```bash
omegon --debug-tui
```

The trace is appended to `.omegon/debug/tui-runtime.jsonl` under the session working directory. Remove or archive an old trace before a new investigation when process-level provenance must be unambiguous. The equivalent environment activation remains available:

```bash
OMEGON_TUI_TRACE=1 omegon
```

This configuration records bounded five-second summaries; it does not capture terminal contents, prompts, tool arguments, or extension payloads.

## First commands after the run

Verify the file exists and inspect provenance:

```bash
wc -l .omegon/debug/tui-runtime.jsonl
head -n 1 .omegon/debug/tui-runtime.jsonl | jq '.build'
```

Reject the run if:

- `build.executable` is not this checkout's `target/dev-release/omegon`;
- `build.manifestDir` is not this checkout's `core/crates/omegon`;
- records contain mixed executable paths or process IDs unexpectedly.

Inspect all windows:

```bash
jq . .omegon/debug/tui-runtime.jsonl
```

Rank the worst draw windows:

```bash
jq -s 'sort_by(.drawUs.p95) | reverse | .[:10] | map({time: .generatedAtUnixMs, draw: .drawUs, inputToFrame: .inputToFrameUs, segments: .conversationSegments, scroll: .conversationScrollOffset, streamingFrames, detachedFrames, agentEvents, agentBudgetHits, frames, urgentFrames, backgroundFrames, slowFramesOver16ms, slowFramesOver33ms, slowFramesOver100ms})' .omegon/debug/tui-runtime.jsonl
```

Rank the worst input-response windows:

```bash
jq -s 'sort_by(.inputToFrameUs.p95) | reverse | .[:10] | map({time: .generatedAtUnixMs, inputToFrame: .inputToFrameUs, draw: .drawUs, operatorInputs, inputBatches, inputsPerBatch, segments: .conversationSegments, scroll: .conversationScrollOffset, streamingFrames, detachedFrames, agentEvents, agentBudgetHits})' .omegon/debug/tui-runtime.jsonl
```

Compare streaming/detached windows against baseline windows:

```bash
jq -s '{baseline: [ .[] | select(.streamingFrames == 0 and .detachedFrames == 0) ], streaming: [ .[] | select(.streamingFrames > 0) ], detached: [ .[] | select(.detachedFrames > 0) ]} | with_entries(.value |= {windows: length, meanDrawP95Us: ((map(.drawUs.p95) | add // 0) / (length | if . == 0 then 1 else . end)), maxDrawUs: (map(.drawUs.max) | max // 0), meanInputP95Us: ((map(.inputToFrameUs.p95) | add // 0) / (length | if . == 0 then 1 else . end)), maxInputUs: (map(.inputToFrameUs.max) | max // 0), slowOver33ms: (map(.slowFramesOver33ms) | add // 0), slowOver100ms: (map(.slowFramesOver100ms) | add // 0)})' .omegon/debug/tui-runtime.jsonl
```

## Decision rules

Use evidence to select the next implementation slice.

### Agent ingestion is the bottleneck

Evidence:

- `agentBudgetHits` is consistently nonzero or high;
- `agentEventsPerDrain` approaches its cap;
- `inputToFrameUs` is high while `drawUs` remains comparatively low.

Next action:

- inspect producer event granularity;
- coalesce token/chunk events before applying them to conversation state;
- preserve input-first scheduling and consider adaptive drain limits.

### Synchronous draw is the bottleneck

Evidence:

- `drawUs.p95` or `drawUs.max` is high;
- `inputToFrameUs` closely tracks `drawUs`;
- agent-budget pressure is low.

Next action:

- instrument `App::draw` phases, especially conversation projection, widget render, and terminal diff/write;
- benchmark history-size scaling with 50, 500, and 2,000 segments;
- do not lower frame rate blindly before locating the expensive phase.

### Conversation history/projection is the bottleneck

Evidence:

- draw latency rises with `conversationSegments`;
- detached/scrolling windows are much worse than baseline;
- agent pressure is low.

Known hotspot:

```text
core/crates/omegon/src/tui/render.rs
```

The conversation path reconstructs `conversation_projection::project_conversation(...)` before rendering. If phase instrumentation confirms this dominates, cache the projection by conversation revision and presentation level. Scroll offset must not invalidate content projection.

### Streaming invalidation is the bottleneck

Evidence:

- `streamingFrames > 0` windows are slow even at modest history sizes;
- baseline and detached-only windows are substantially cheaper;
- background frame count is excessive relative to useful updates.

Next action:

- coalesce streaming mutations per frame;
- track conversation revision separately from scroll revision;
- invalidate only the active streaming segment's measurement where possible.

### Terminal output is the bottleneck

Evidence:

- application projection/widget phase is later shown to be cheap, but total draw remains high;
- latency correlates more with terminal size or changed cells than history size.

Next action:

- measure render-to-buffer separately from backend flush/diff;
- inspect full-region clears and unnecessary changed cells;
- consider an adaptive background frame cap only after confirming terminal-output cost.

## Current implementation state

Relevant files:

```text
core/crates/omegon/src/tui/frame_scheduler.rs
core/crates/omegon/src/tui/runtime_trace.rs
core/crates/omegon/src/tui/mod.rs
core/crates/omegon/src/tui/render.rs
core/crates/omegon/src/tui/conv_widget.rs
Justfile
```

Implemented scheduler policy:

- operator input is serviced before producer traffic;
- agent-event draining is bounded by count and wall time;
- background redraws are coalesced to a frame interval;
- operator input makes a frame urgent;
- dirty-frame polling is deadline-aware.

Focused scheduler tests currently cover:

```text
operator_input_forces_immediate_draw
background_events_are_coalesced_to_frame_interval
agent_budget_is_bounded
dirty_background_frame_waits_only_until_frame_deadline
```

A deterministic ignored trace generator is available through:

```bash
just bench-tui-scroll-stream
```

It validates scheduler semantics but does not measure real renderer cost.

## Validation already completed

The current implementation passed:

```text
cargo fmt --all
cargo check -p omegon --locked --no-default-features
cargo check -p omegon --locked
cargo test -p omegon frame_scheduler::tests --locked
just check-interface-boundary
git diff --check
```

## Required posture on resume

1. Read and validate the trace before editing performance code.
2. Preserve checkout/build provenance in any reported results.
3. Separate known evidence from inference.
4. Do not claim victory from scheduler-only tests.
5. Select the smallest fix justified by the worst live windows.
6. Add a regression test or repeatable benchmark for whichever bottleneck is confirmed.
