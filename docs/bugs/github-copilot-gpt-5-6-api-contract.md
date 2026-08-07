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

- Audit every model family currently advertised by GitHub Copilot discovery, including OpenAI GPT/Codex, Anthropic Claude, Google Gemini, xAI Grok, Moonshot Kimi, Microsoft MAI, and GitHub-hosted auxiliary families.
- Treat `GET /models` as the account-specific admission contract: models absent from discovery must not be assumed usable merely because GitHub documentation lists them.
- Extend discovery to retain provider-advertised wire-contract metadata when present (endpoint/API family, capabilities, limits, picker eligibility), rather than discarding it after catalog projection.
- Select the inference endpoint and request/response codec from discovered model metadata when available.
- Define a conservative, centrally tested fallback classifier for legacy or incomplete `/models` payloads; it must operate on normalized model families/prefixes, not exact model names.
- Correct endpoint selection for GPT-5.6 Copilot model IDs, including suffixed offerings such as `gpt-5.6-sol`.
- Add regression coverage across the heterogeneous Copilot model roster and for incomplete/unknown metadata.
- Ensure provider errors preserve a safe, useful upstream message instead of collapsing to the observed `model \\` output.
- Run focused provider/discovery tests and `just clippy-changed`.

## Non-goals

- Changing OpenAI Codex routing.
- Hard-coding one discovered GPT-5.6 suffix as the only supported offering.
- Reworking the generic provider error UI.
- Adding fallback to another provider when Copilot rejects a model.

## Acceptance criteria

- `github-copilot:gpt-5.6-sol` uses the endpoint and codec advertised by Copilot discovery, or the conservative Responses fallback when metadata is absent.
- Endpoint selection is represented as a typed Copilot wire contract rather than a GPT-specific boolean.
- Discovered model metadata survives into dispatch sufficiently to select among supported Copilot API families.
- Exact and suffixed GPT-5.5/GPT-5.6 models retain Responses behavior under incomplete legacy metadata.
- Representative Claude, Gemini, Grok, Kimi, MAI, and auxiliary model fixtures select their advertised API family without being forced through GPT heuristics.
- Unknown models with incomplete metadata fail safely or use a documented conservative default; they are never silently classified from one exact-name exception.
- Non-chat/embedding models discovered by Copilot remain excluded from chat dispatch.
- Account-specific availability remains discovery-driven.
- An upstream non-success response produces a redacted but diagnostically useful error.
- Regression tests fail under the previous exact-`gpt-5.5` predicate and pass after the fix.

## Regression plan

Use table-driven fixtures that mirror the heterogeneous `/models` roster:

- OpenAI: `gpt-5.5`, a GPT-5.5 suffix, `gpt-5.6`, `gpt-5.6-sol`, and a legacy GPT chat model.
- Anthropic: current Claude Sonnet and Opus identifiers.
- Google: a Gemini identifier.
- xAI: a Grok identifier.
- Moonshot: a Kimi identifier.
- Microsoft: an MAI identifier.
- Auxiliary/non-chat: an embedding model must remain non-dispatchable as chat.
- Metadata variants: explicit endpoint/API-family metadata, capabilities-only metadata, and a legacy payload with neither.
- Test that account discovery controls admission and does not synthesize unavailable models.
- Test each selected request body and response parser as a paired wire contract.
- Exercise/redact representative Copilot model-contract error payloads, including nested JSON and malformed/plain-text errors.

## Architectural constraint

GitHub Copilot is an inference aggregator, not one homogeneous OpenAI-compatible endpoint. Provider discovery owns what models are available; a typed wire-contract projection owns how each discovered model is invoked. Model-name heuristics are compatibility fallback only and must remain centralized, conservative, observable, and covered by a broad matrix.

## Definition of done

- Focused tests pass.
- `just clippy-changed` passes.
- The change is committed on a focused branch and submitted independently.
