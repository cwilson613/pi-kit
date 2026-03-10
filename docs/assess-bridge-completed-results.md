---
id: assess-bridge-completed-results
title: Assess bridge returns completed structured results
status: implementing
parent: lifecycle-hygiene-verification-substates
tags: [cleave, assess, openspec, lifecycle, bridge]
open_questions: []
branches: ["feature/assess-bridge-completed-results"]
openspec_change: assess-bridge-completed-results
---

# Assess bridge returns completed structured results

## Overview

Close the contract gap where bridged `/assess spec` returns only a kickoff banner while the real review runs in a follow-up turn. Tool and agent callers need a completed, trustworthy structured result they can reconcile and archive against.

## Research

### Bridge contract mismatch

`/assess spec` currently prepares a review by emitting a kickoff banner and sending the real reviewer prompt as a follow-up turn. The slash-command bridge returns immediately with the kickoff `humanText`, so tool callers observe preparation rather than completion. This makes the returned lifecycle metadata non-authoritative for `reconcile_after_assess`.

## Decisions

### Decision: Bridged /assess must produce completed structured results in-band

**Status:** decided
**Rationale:** Agent and tool callers rely on the bridge result as the authoritative contract. If the real assessment happens in a later follow-up turn, callers cannot safely persist lifecycle state. The structured executor should perform the assessment synchronously for bridged usage and reserve follow-up prompting for interactive-only flows.

## Open Questions

*No open questions.*

## Implementation Notes

### File Scope

- `extensions/cleave/index.ts` (modified) — Separate interactive follow-up assessment flows from bridged synchronous structured execution
- `extensions/cleave/assessment.ts` (modified) — Define completed structured assessment result shape and lifecycle-safe outcomes for bridged assess execution
- `extensions/cleave/bridge.ts` (modified) — Preserve normalized bridge envelope while carrying completed assessment data
- `extensions/lib/slash-command-bridge.ts` (modified) — Keep bridge semantics explicit for synchronous structured slash-command execution
- `extensions/cleave/*.test.ts` (modified) — Regression coverage for bridged /assess spec completion semantics

### Constraints

- Bridged `/assess spec` must not claim a completed lifecycle outcome until the review logic has actually finished.
- Interactive `/assess` can remain follow-up driven, but tool/agent invocation must return the completed structured envelope in the initial result.
- `result.args` must continue to preserve the full original tokenized invocation.
