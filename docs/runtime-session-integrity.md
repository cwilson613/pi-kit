---
id: runtime-session-integrity
title: "Runtime session integrity — deterministic startup, non-surprising secret access, and diagnosable failures"
status: exploring
tags: [runtime, secrets, diagnostics, planning, release]
open_questions: []
dependencies: []
related:
  - session-secret-cache-preflight
  - harness-diagnostics
  - release-0-15-4-trust-hardening
---

# Runtime session integrity — deterministic startup, non-surprising secret access, and diagnosable failures

## Overview

Cross-cutting planning node for release-critical runtime trust work that currently spans unrelated parts of the tree. For 0.15.4, the key runtime integrity obligations are: startup resolves required secrets at a deterministic boundary, headless/child execution never surprises the operator with mid-task prompts, and failures leave structured evidence that can be queried after the fact. This node exists to connect those obligations without collapsing macOS secret UX work into diagnostics or treating diagnostics as solely an Omega concern.

## Decisions

### Decision: runtime session integrity for 0.15.4 is defined by deterministic secret resolution boundaries plus post-failure evidence

**Status:** decided

**Rationale:** A trustworthy harness session must be predictable before the first substantive task begins and inspectable after something goes wrong. That means required secrets resolve at startup for interactive sessions, child/headless runs inherit a resolved runtime context or fail fast, and the harness records enough structured diagnostics to explain tool failures, child failures, and crashes without log archaeology. These are coupled release concerns even though they currently sit under different parents.

### Decision: session-secret-cache-preflight and harness-diagnostics remain distinct design problems in this pass

**Status:** decided

**Rationale:** The secret preflight problem is about deterministic access boundaries, cache lifetime, and safe transport of resolved secrets into child processes. The diagnostics problem is about persistence, query surface, redaction, and crash/tool failure capture. They should be planned together for 0.15.4 because both define runtime trust, but merging them would blur two separate interfaces and likely create a vague node that is harder to execute.
