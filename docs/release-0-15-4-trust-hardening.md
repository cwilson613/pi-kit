---
id: release-0-15-4-trust-hardening
title: "0.15.4 trust hardening — runtime trust and release integrity"
status: exploring
tags: [release, stabilization, trust, runtime, planning]
open_questions: []
dependencies: []
related:
  - orchestratable-provider-model
  - openai-provider-identity-and-routing-honesty
  - session-secret-cache-preflight
  - harness-diagnostics
  - merge-safety-improvements
  - release-candidate-system
  - update-channels
---

# 0.15.4 trust hardening — runtime trust and release integrity

## Overview

Cross-cutting release-planning umbrella for the 0.15.4 RC series. This node does not replace the existing domain taxonomy; it consolidates the release-critical work needed to make Omegon trustworthy to operate, evaluate, and ship. The core release thesis is: routing is honest, startup is deterministic, failures leave evidence, and release/merge flow resists silent regressions. This umbrella should group the existing critical nodes without collapsing their distinct design questions.

## Decisions

### Decision: 0.15.4 is a trust-and-stability release, not a broad feature release

**Status:** decided

**Rationale:** The design tree currently has enough active fronts that 0.15.4 could sprawl into another long RC train. The release thesis should stay narrow: routing must be honest, startup must be deterministic, failures must leave evidence, and release/merge flow must resist silent regressions. Large new platform surfaces (Omega expansion, tutorial ecosystem, speculative memory systems, cross-instance orchestration) dilute that goal and should not define this release.

### Decision: 0.15.4 release blockers are provider/routing integrity, secret preflight, diagnostics v1, and merge/release safety

**Status:** decided

**Rationale:** The minimum set that makes the harness trustworthy to evaluate and ship is: (1) close out orchestratable-provider-model verification, (2) land OpenAI-family provider identity and routing honesty, (3) implement session secret cache/startup preflight v1 so interactive sessions warm required secrets and headless children never prompt mid-task, (4) ship harness diagnostics v1 so failures leave structured evidence, and (5) implement the actionable merge-safety improvements that catch silent regressions before or immediately after merge. Without these, 0.15.4 remains difficult to trust operationally.

### Decision: update-channels and TUI operator visibility improvements are stretch for 0.15.4, not blockers

**Status:** decided

**Rationale:** Update channels, in-TUI self-update, footer/engine display, and input-area UX improvements can improve operator experience, but they are not the critical path to restoring trust in the harness. They should land in the RC series only if they stay low-risk and directly support runtime honesty or release evaluation. If they threaten schedule or expand scope, they defer without blocking 0.15.4.

### Decision: the 0.15.4 RC sequence should progress integrity first, then determinism, then observability

**Status:** decided

**Rationale:** Use the RC series to stage risk in a rational order. RC1 should validate provider/routing/auth integrity and realistic orchestrated execution. RC2 should harden startup determinism and merge/release safety, including session secret preflight and release guardrails. RC3 should add diagnostics v1 and any low-risk operator visibility improvements needed to inspect runtime truth. Additional RCs, if any, are for stabilization and bugfixes rather than new strategic scope.

### Decision: 0.15.4 explicitly defers major platform expansion and ecosystem work beyond trust hardening

**Status:** decided

**Rationale:** To keep 0.15.4 shippable, the RC series should not absorb large strategic fronts whose value is real but whose scope is orthogonal to immediate runtime trust. Explicitly deferred from the 0.15.4 critical path are: Omega platform expansion beyond diagnostics directly needed for harness trust, composable tutorial/plugin ecosystem work, self-curated memory/autonomy layers, and A2A/external agent interoperability. These nodes remain valid and may continue as design work, but they do not block 0.15.4 and should not be allowed to expand the release thesis.

### Decision: target 0.15.4-rc.1 for 2026-04-03 with an integrity-first scope

**Status:** decided

**Rationale:** Today is Friday, 2026-03-27. A realistic first RC target is Friday, 2026-04-03 — one working week to land and verify the integrity-first slice without inventing false certainty. RC1 should aim to ship the narrowest release-worthy checkpoint of the 0.15.4 program: close out orchestratable-provider-model verification, land OpenAI-family provider identity/routing honesty, and prove realistic orchestrated execution against repo-backed tasks. Session secret preflight, diagnostics v1, and broader merge/release hardening remain part of the 0.15.4 release thesis but should not be forced into rc.1 if that would turn the first RC into another moving target.

### Decision: 0.15.4-rc.1 exit criteria are verification closure, routing honesty, and repo-backed orchestration proof

**Status:** decided

**Rationale:** A date-only RC target is not enough. 0.15.4-rc.1 is ready to cut only when three concrete conditions hold: (1) `orchestratable-provider-model` has completed its verifying-stage proof with no known invalid default model/fallback path remaining in the rc.1 slice, (2) OpenAI-family auth/routing honesty is landed such that operator-visible surfaces distinguish OpenAI API from ChatGPT/Codex OAuth and report the concrete runtime provider/model truthfully, and (3) at least one realistic repo-backed orchestrated execution path (not a synthetic scratch probe) succeeds end-to-end and leaves child/provider state that matches what the operator sees. If any of these remain ambiguous, rc.1 should slip rather than ship a false trust checkpoint.
