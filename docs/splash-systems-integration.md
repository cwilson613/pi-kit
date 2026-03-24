---
id: splash-systems-integration
title: Splash screen systems check visualization — real loading behind the animation
status: exploring
parent: startup-systems-check
tags: [tui, splash, startup, systems-check, ux, 0.15.1]
open_questions: []
jj_change_id: qvrlrnwnrknsxntvkospulopsqkzozvm
priority: 2
---

# Splash screen systems check visualization — real loading behind the animation

## Overview

Replace the cosmetic splash loading checklist with real systems check progress. The splash animation runs ~1.7s; the systems check (GPU detection, Ollama probe, port scanning, provider auth check, memory loading) takes 100-500ms. Run them in parallel — the animation masks the latency, and each checklist item transitions from scanning → done/failed as the actual probe completes. The user sees Omegon genuinely discovering its environment, not a fake loading bar.

## Research

### Current splash architecture and what changes

**Current state** (`tui/splash.rs`):
- 3 hardcoded items: `providers`, `memory`, `tools`
- States: `Pending → Active → Done/Failed`
- Cosmetic cascade: providers done at frame 8, memory at 12, tools at 16
- No real work happens — items are set to Done at fixed frame thresholds
- Animation runs ~1.7s (38 frames × 45ms), then holds for dismissal
- Checklist renders as a single Line beneath the logo

**Proposed state**:
- Items reflect real async probes running on a tokio::spawn background task
- The splash loop receives probe results via a channel
- Each probe completes independently — fast probes (env var check) resolve in <1ms, slow probes (port scan, GPU detection) take 50-500ms

**Expanded checklist items**:

| Item | Probe | Expected time |
|---|---|---|
| `cloud` | Check ANTHROPIC_API_KEY, OPENAI_API_KEY, OPENROUTER_API_KEY env vars + auth.json | <5ms |
| `local` | Probe Ollama :11434, LM Studio :1234, vLLM :8080 via /v1/models | 50-200ms per port |
| `hardware` | sysctl/nvidia-smi for RAM/GPU/VRAM | 10-50ms |
| `memory` | Load facts.jsonl, init SQLite, count facts | 20-100ms |
| `tools` | Register tool definitions, load plugins | <10ms |
| `design` | Scan docs/ for design nodes, openspec/ for changes | 10-30ms |
| `secrets` | Probe vault backend, check keyring | 10-50ms |
| `container` | Check podman/docker availability | 20-50ms |

Total elapsed with parallelism: ~200-300ms. The splash animation at 1.7s has plenty of headroom — items will cascade through the scanning animation naturally and land on ✓/✗ well before the logo finishes resolving.

**Rendering refinement**: Instead of a single-line checklist, use a 2-column or 3-column grid beneath the logo. Each item shows its indicator + label. Failed items show red ✗ with a brief reason. The grid gives more room to show the true breadth of what Omegon does at startup.

Example layout (after all probes complete):
```
  ✓ cloud (anthropic, openai)   ✓ memory (1800 facts)    ✓ tools (42 registered)
  ✓ local (ollama: 7 models)    ✓ design (235 nodes)     ✓ secrets (vault, 3 stored)
  ✓ hardware (M2 Pro, 32GB)     ✓ container (podman)     · mcp (none configured)
```

### Implementation approach: async probes with channel feedback

The splash loop currently runs synchronously inside `run_tui`. To receive async probe results:

1. Before entering the splash loop, spawn a `tokio::spawn` task that runs all probes in parallel via `tokio::join!`
2. Each probe sends its result through an `mpsc::Sender<ProbeResult>` channel
3. The splash loop polls the channel each frame via `try_recv()` and updates item states
4. The `ProbeResult` enum carries both the item label and the result (done with summary, or failed with reason)

```rust
enum ProbeResult {
    Done { label: &'static str, summary: String },  // "anthropic, openai"
    Failed { label: &'static str, reason: String },  // "not installed"
}
```

The probe task is fire-and-forget — if the splash is dismissed early (keypress), the probes continue running and their results are available via the `CapabilityTier` struct that the startup-systems-check node produces.

Key constraint: the splash loop is NOT async — it's a synchronous `loop` that polls crossterm events. The channel receiver must be `try_recv()`, not `.await`. Use `tokio::sync::mpsc::Receiver::try_recv()` or `std::sync::mpsc`.

The probes themselves need tokio for HTTP requests (Ollama, LM Studio). The simplest approach: spawn the probe task before the splash loop, pass a `std::sync::mpsc::Sender` for results. The probe task uses `tokio::spawn` internally for parallelism.

## Decisions

### Decision: Multi-line grid beneath logo — 3 columns, shows the breadth of startup work

**Status:** decided
**Rationale:** A single line can only fit 3-4 items legibly. With 8-9 probe categories, a grid shows the true scope of what Omegon does at startup. The grid fills the vertical space between the logo and the 'press any key' prompt — space that's currently empty. Three columns of 3 rows is compact enough for the compact logo tier too. Each cell shows indicator + label + parenthetical summary when done. The visual effect of 9 items cascading from scanning to checkmark is significantly more impressive than 3 items blinking done.

## Open Questions

*No open questions.*
