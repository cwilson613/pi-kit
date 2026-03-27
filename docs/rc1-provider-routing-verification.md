---
id: rc1-provider-routing-verification
title: "RC1: provider routing verification closure"
status: exploring
parent: release-0-15-4-trust-hardening
tags: [release, rc1, providers, verification]
open_questions: []
dependencies: []
related:
  - orchestratable-provider-model
---

# RC1: provider routing verification closure

## Overview

Release-checklist node for the first rc.1 acceptance criterion: the orchestratable provider model must complete verifying-stage proof for the rc.1 slice. This node is not a new architecture effort; it tracks the concrete verification work needed to show that per-task/provider routing, default model resolution, and fallback behavior no longer contain known invalid paths in the integrity-first rc.1 checkpoint.

## Decisions

### Decision: rc.1 routed verification must cover default, explicit, fallback, and failure-reporting cases

**Status:** decided

**Rationale:** The rc.1 routing slice is trustworthy only if it proves the four failure-prone paths we actually care about: (1) default routed execution with no explicit child model, (2) explicit per-child provider:model assignment, (3) fallback from a non-viable first candidate to a viable second candidate without lying about the final route, and (4) failure reporting when no viable route exists. These cases should be exercised with realistic repo-backed orchestration or targeted harness tests so that invalid defaults like `codex-mini-latest` and silent fallback ambiguity cannot hide inside the rc.1 slice.

### Decision: rc.1 routed-run evidence must record resolved provider, resolved model, fallback behavior, and final child outcome

**Status:** decided

**Rationale:** A passing run is not enough if the operator cannot tell what actually happened. For each rc.1 verification case, the evidence should show: requested or inferred route, concrete resolved provider, concrete resolved model, whether fallback occurred, and the final child outcome (success, no-op, or failure). Without that evidence, rc.1 could appear stable while still hiding routing mistakes behind successful end states.
