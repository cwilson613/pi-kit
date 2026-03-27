---
id: rc1-openai-routing-honesty
title: "RC1: OpenAI-family routing honesty landing"
status: exploring
parent: release-0-15-4-trust-hardening
tags: [release, rc1, providers, auth, ux]
open_questions:
  - "Which operator-visible surfaces are mandatory for rc.1 honesty — bootstrap/auth summary, model selector gating, active engine display, conversation footer, and diagnostics/report output?"
  - "What concrete rc.1 proof cases show the OpenAI-family split is honest: OpenAI API-only credentials, ChatGPT/Codex OAuth-only credentials, both present, and fallback from openai intent to openai-codex execution?"
dependencies: []
related:
  - openai-provider-identity-and-routing-honesty
---

# RC1: OpenAI-family routing honesty landing

## Overview

Release-checklist node for the second rc.1 acceptance criterion: OpenAI-family auth and routing honesty must be landed in a way the operator can actually trust. This includes distinguishing OpenAI API from ChatGPT/Codex OAuth in visible surfaces and ensuring that the concrete runtime provider/model shown to the operator matches the executable path used by the harness.

## Open Questions

- Which operator-visible surfaces are mandatory for rc.1 honesty — bootstrap/auth summary, model selector gating, active engine display, conversation footer, and diagnostics/report output?
- What concrete rc.1 proof cases show the OpenAI-family split is honest: OpenAI API-only credentials, ChatGPT/Codex OAuth-only credentials, both present, and fallback from openai intent to openai-codex execution?
