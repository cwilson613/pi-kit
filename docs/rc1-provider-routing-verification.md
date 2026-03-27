---
id: rc1-provider-routing-verification
title: "RC1: provider routing verification closure"
status: exploring
parent: release-0-15-4-trust-hardening
tags: [release, rc1, providers, verification]
open_questions:
  - "Which exact rc.1 verification cases must pass to declare that no known invalid default-model or fallback path remains in the routed execution slice?"
  - "What observable evidence should rc.1 capture for each routed child run — resolved provider, resolved model, fallback path taken or not taken, and final child outcome?"
dependencies: []
related:
  - orchestratable-provider-model
---

# RC1: provider routing verification closure

## Overview

Release-checklist node for the first rc.1 acceptance criterion: the orchestratable provider model must complete verifying-stage proof for the rc.1 slice. This node is not a new architecture effort; it tracks the concrete verification work needed to show that per-task/provider routing, default model resolution, and fallback behavior no longer contain known invalid paths in the integrity-first rc.1 checkpoint.

## Open Questions

- Which exact rc.1 verification cases must pass to declare that no known invalid default-model or fallback path remains in the routed execution slice?
- What observable evidence should rc.1 capture for each routed child run — resolved provider, resolved model, fallback path taken or not taken, and final child outcome?
