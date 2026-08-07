+++
title = "GitHub Copilot GPT-5.6 API contract"
tags = ["bugfix","provider","github-copilot"]
+++

# GitHub Copilot GPT-5.6 API contract

# GitHub Copilot GPT-5.6 uses the wrong API contract

## Assignment brief

Repair GitHub Copilot dispatch for the GPT-5.6 model family. The current provider selects Copilot's Responses API only for the exact model `gpt-5.5`; discovered GPT-5.6 offerings therefore fall through to `chat/completions`, producing a malformed or unhelpful API error (observed in the TUI as `API error: model \\`).

## Evidence

- `core/crates/omegon/src/providers.rs` routes only exact `gpt-5.5` through `copilot_model_requires_responses_api`.
- The operator reproduced the failure using `github-copilot:gpt-5.6-sol`.
- The provider already has separate request builders and response parsers for Copilot Responses and Chat Completions APIs.

## Scope

- Correct endpoint selection for GPT-5.6 Copilot model IDs, including suffixed offerings such as `gpt-5.6-sol`.
- Add regression coverage for exact and suffixed GPT-5.5/GPT-5.6 models and a representative chat-completions model.
- Ensure provider errors preserve a safe, useful upstream message instead of collapsing to the observed `model \\` output.
- Run focused provider tests and `just clippy-changed`.

## Non-goals

- Changing OpenAI Codex routing.
- Hard-coding one discovered GPT-5.6 suffix as the only supported offering.
- Reworking the generic provider error UI.
- Adding fallback to another provider when Copilot rejects a model.

## Acceptance criteria

- `github-copilot:gpt-5.6-sol` uses the Copilot Responses endpoint and parser.
- Exact and suffixed GPT-5.5 models retain Responses behavior.
- Non-GPT-5.5/5.6 Copilot models retain their existing endpoint behavior.
- An upstream non-success response produces a redacted but diagnostically useful error.
- Regression tests fail under the previous exact-`gpt-5.5` predicate and pass after the fix.

## Regression plan

- Unit-test endpoint selection for `gpt-5.5`, a GPT-5.5 suffix, `gpt-5.6`, `gpt-5.6-sol`, and `gpt-5.4`.
- Test the selected body shape for `gpt-5.6-sol`.
- Exercise/redact a representative Copilot model-contract error payload.

## Definition of done

- Focused tests pass.
- `just clippy-changed` passes.
- The change is committed on a focused branch and submitted independently.
