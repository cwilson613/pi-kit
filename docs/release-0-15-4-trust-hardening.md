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
