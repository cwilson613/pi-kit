+++
id = "continuation-arbiter"
status = "implemented"
tags = ["agent-loop", "continuation", "context-budget"]
aliases = []
imported_reference = false

[publication]
enabled = false
visibility = "private"
+++

# Continuation Arbiter

## Overview

Replace scattered continuation injections in the agent loop with one bounded arbiter. The arbiter selects at most one continuation cause per loop iteration, preserving context budget and deterministic ordering while preventing plan reconciliation, compaction, feature messages, and empty-response recovery from competing or duplicating prompts.

## Invariants

1. At most one arbiter-owned continuation message is emitted per loop iteration.
2. Existing feature-provided system messages remain content inputs; the arbiter does not rewrite them.
3. Compaction executes before continuation selection so the selected message targets the post-compaction conversation.
4. Plan reconciliation continuation is emitted only when reconciliation changed the visible plan and open work remains.
5. Empty-response recovery is the lowest-priority fallback and remains bounded by its existing retry budget.
6. Normal non-empty tool continuations incur no additional inference turn and no additional prompt text.
7. Selection is a pure function over explicit pending causes and is covered by precedence and suppression tests.

## Decisions

### Decision: One typed pending-cause set and one selector

**Status:** decided
**Rationale:** Ad hoc `push_user` branches create order-dependent behavior. A small typed cause set makes precedence testable without introducing a generalized scheduler.

### Decision: Precedence is feature message, plan continuation, empty recovery

**Status:** decided
**Rationale:** Explicit feature messages represent harness-owned work already requested; plan continuation preserves operator-visible execution state; empty recovery is generic and therefore last. Compaction is an action performed before selection, not a competing prompt.

### Decision: Keep messages compact and reuse existing budgets

**Status:** decided
**Rationale:** Less-capable models must not pay for a policy framework in every prompt. The arbiter emits only the selected existing compact message and adds no persistent system instructions.

## Assumptions Resolved

- Feature injections can coexist in one drained request batch: preserve their order as conversation content, but do not add a second arbiter prompt.
- Compaction can fail: selection still proceeds against the unchanged conversation and existing failure telemetry remains authoritative.
- A completed visible plan needs no narration-only turn; `PlanUpdated` and the tool result remain sufficient.

## Implementation Notes

### Initial scope

- `core/crates/omegon/src/loop.rs` — introduce typed continuation causes and pure precedence selection; route reconciled-plan continuation and empty-response recovery through it.
- Focused unit tests in `loop.rs` — precedence, one-message bound, open-plan continuation, completed-plan suppression, and retry-budget preservation.

### Deferred scope

- Converting every historical progress/skill/dead-mouse nudge is intentionally deferred. The first patch establishes the arbiter at the collision boundary without a risky whole-loop rewrite.

## Implementation Result

The initial arbiter is implemented as a pure precedence selector in `loop.rs`. Open reconciled-plan continuation now passes through the selector, completed plans remain suppressed, and focused tests prove deterministic single-cause selection plus both plan outcomes. Feature-message and empty-recovery routing remain explicit next migration points rather than being rewritten speculatively in this patch.
