+++
title = "Incident: Provider refusal recovery failure and misleading remediation"
tags = ["incident","providers","anthropic","github-copilot","tui","error-handling","reliability"]
+++

# Incident: Provider refusal recovery failure and misleading remediation

# Incident: Provider refusal recovery failure and misleading remediation

**Date:** 2026-07-29
**Status:** Documented; root cause not yet established
**Affected surface:** Omegon TUI provider routing and completion recovery
**Providers observed:** GitHub Copilot (`github-copilot:claude-fable-5`), Anthropic (`anthropic:claude-fable-5`)

## Executive summary

An agent completed its investigative work and announced that it had enough evidence to write an adversarial analysis. The requested deliverable was never produced.

The first selected route, GitHub Copilot, failed twice while parsing the provider response because `choices[0].message` was absent. The operator then switched to Anthropic. Anthropic terminated the request with a refusal. Subsequent Anthropic attempts continued to refuse even after full application restarts and other retry attempts.

Omegon represented the Anthropic refusal as an abnormal provider stop and recommended either issuing a continuation prompt or retrying with a larger output budget. That guidance did not match the observed failure class. A refusal is not evidence of token exhaustion, and repeated continuation attempts preserve—or reconstruct—the context likely responsible for the refusal.

The incident therefore comprises at least three distinct reliability failures:

1. A Copilot response could not be normalized into Omegon's expected assistant-message shape.
2. Anthropic persistently refused the effective request across retries and restarts.
3. Omegon did not provide actionable refusal diagnostics or a recovery path appropriate to a persistent refusal.

## Operator-visible chronology

The captured TUI transcript showed the following sequence:

```text
• bash • =
graphql redaction default + kill switch • 1 operation
I have enough. Writing the adversarial analysis.
Provider route switched to GitHub Copilot (github-copilot:claude-fable-5).
Make it so
A LLM error: GitHub Copilot response parse failed: missing choices[0]. message
proceed
A LLM error: GitHub Copilot response parse failed: missing choices[0]. message
Provider route switched to Anthropic/Claude (anthropic:claude-fable-5).
make it so
Provider stop: anthropic/refusal
The provider ended the response abnormally; the visible answer may be incomplete.
Use a continuation prompt or retry with a larger output budget if needed.
• ready • idle
```

The operator subsequently reported that Anthropic produced only further refusals after full restarts and additional attempts. The original analysis remained unavailable.

## Expected behavior

Once the agent reported that research was complete, Omegon should have done one of the following:

- delivered the analysis through the selected provider;
- retried a transient transport or schema-normalization failure safely;
- switched to a compatible provider while preserving the completed work;
- or surfaced a precise, actionable failure diagnosis with a recovery operation that changed the effective request.

For a provider refusal specifically, the UI should have:

- classified it as a refusal rather than generic abnormal cessation;
- avoided token-budget remediation unless truncation evidence existed;
- identified whether the refusal originated in provider response metadata, streamed stop reasons, or Omegon's adapter classification;
- shown which durable context classes were included in the effective request, without exposing secrets;
- offered an explicit clean-context or reduced-context retry;
- and preserved the pending deliverable as unfinished work rather than returning to an apparently normal idle state.

## Actual behavior

- Copilot failed twice with the same response-shape error.
- Switching providers did not recover the deliverable.
- Anthropic refused the request and continued refusing across full restarts.
- The remediation message suggested continuation or a larger output budget despite no displayed evidence of truncation.
- The TUI returned to `ready • idle`, visually implying recovery or completion although the promised analysis had not been produced.
- No operator-visible diagnostic identified what request material persisted across restart or what specifically triggered refusal.

## Impact

### Direct impact

- The requested work product was lost or stranded after the research phase.
- The operator spent time retrying, restarting, and switching providers without a meaningful change in outcome.
- The system provided recovery advice that sent the operator back through the same failing path.

### Trust impact

This failure occurred after the agent explicitly stated, “I have enough. Writing the adversarial analysis.” The system therefore appeared to possess the necessary result but could not render it through any available route. Returning to idle without preserving or exposing that pending result makes the harness look unreliable precisely at the handoff between completed reasoning and operator-visible output.

### Engineering impact

The incident obscures several boundaries that should be independently diagnosable:

- provider transport success versus response normalization;
- assistant content generation versus provider policy refusal;
- persisted conversation state versus process-local state;
- provider stop classification versus operator remediation;
- and runtime readiness versus completion of the active operator intent.

## Evidence-backed findings

### 1. The Copilot failure is a response-normalization failure

The visible error explicitly states that `choices[0].message` was missing. This establishes that Omegon expected an OpenAI-compatible assistant-message shape and did not receive—or did not correctly decode—that shape.

It does **not** establish whether Copilot returned an error envelope, a content-filter response, a streaming-only delta, an empty choices array, or another valid provider-specific payload. Raw sanitized response metadata is required to distinguish those cases.

### 2. The Anthropic stop was classified as a refusal

The TUI displayed `anthropic/refusal`. That is materially different from output truncation, timeout, transport failure, or context-window exhaustion.

### 3. The displayed recovery guidance was not supported by the displayed evidence

“Retry with a larger output budget” is appropriate for a length-limited completion. No length stop was shown. The displayed stop was a refusal. Continuation also preserves the conversational context and is therefore unlikely to resolve a context-sensitive refusal.

### 4. Restart did not remove the refusal condition

The operator reports persistent refusals after full restarts. This rules against a purely transient in-process failure. It does not, by itself, identify the persistent cause.

Likely persistence boundaries include restored conversation history, project instructions, profiles, skills, generated continuation prompts, cached provider state, or the request/model combination. These are hypotheses, not established causes.

## Root-cause hypotheses requiring verification

1. **Conversation restoration reproduced the same effective request.** A restart resumed the exact conversation, so Anthropic continued to receive the trigger.
2. **A durable instruction source triggered refusal.** Project directives, skill text, profile prompts, or another injected source may have persisted independently of the visible conversation.
3. **Refusal classification is too broad.** The Anthropic adapter may map multiple abnormal stop conditions to `refusal` without retaining detailed provider metadata.
4. **Provider switching preserved incompatible continuation state.** A continuation generated for one provider may have been replayed through another without adapting or reducing context.
5. **The requested adversarial-analysis content itself triggered provider policy.** This remains possible, but cannot be asserted without the effective request and provider response metadata.

## Product and engineering defects

### P0/P1: Preserve unfinished deliverables

When an agent announces or records a pending deliverable and provider completion fails, the operation must remain visibly incomplete. `ready • idle` must not imply that the operator intent was satisfied.

### P1: Class-specific remediation

Recovery guidance must be derived from the stop class:

| Failure class | Appropriate remediation |
|---|---|
| Length/output limit | Continue or increase output budget |
| Context limit | Reduce context or open a clean session with a handoff summary |
| Provider refusal | Inspect/refactor effective request; offer clean-context retry; optionally change provider/model |
| Response parse failure | Capture sanitized envelope; retry adapter path or select compatible route |
| Transport/rate limit | Backoff and retry according to provider metadata |

### P1: Effective-request provenance

Expose a safe diagnostic inventory of request contributors:

- system/core instructions;
- project directives;
- active skills;
- profile instructions;
- conversation turns;
- tool outputs or summaries;
- continuation/recovery scaffolding;
- media attachments;
- provider adaptation transforms.

The diagnostic must show provenance and approximate size without leaking secrets or necessarily displaying full prompt content.

### P1: Clean-context recovery

Provide a one-step recovery operation that:

1. creates a fresh provider conversation;
2. carries forward a bounded, inspectable task handoff;
3. excludes the failing transcript unless explicitly selected;
4. records what was removed;
5. preserves the original operation as unresolved until output is delivered.

### P1: Preserve sanitized provider diagnostics

For schema failures and refusals, retain enough structured evidence to diagnose the adapter:

- HTTP status;
- provider request ID;
- response content type;
- top-level response keys;
- stop reason or refusal category;
- whether content or choices were present;
- adapter and schema dialect used;
- retryability classification.

Secrets and raw sensitive prompt content must remain redacted.

### P2: Refusal-loop detection

After repeated equivalent refusals, Omegon should stop recommending continuation and identify the loop explicitly. Equivalence should consider provider, model, effective-request fingerprint, and stop class.

## Acceptance criteria for remediation

1. A refusal never produces output-budget advice unless a separate length signal is also present.
2. Two equivalent refusals trigger a refusal-loop state with a materially different recovery option.
3. A full restart diagnostic states whether the same conversation/effective request was restored.
4. A clean-context retry visibly lists which context classes are retained and omitted.
5. A missing `choices[0].message` error records a sanitized response-shape summary.
6. Provider switching adapts continuation state rather than blindly replaying provider-specific assumptions.
7. The active operation remains incomplete until the promised deliverable is emitted, explicitly abandoned by the operator, or superseded.
8. The TUI distinguishes `refusal`, `length`, `context_limit`, `transport`, `rate_limit`, and `parse_failure` in both status and remediation copy.

## Reproduction and evidence collection plan

Because the exact request and provider envelopes were not captured in this document, reproduction should be instrumented rather than attempted blindly:

1. Recreate a multi-turn task that reaches a pending final deliverable.
2. Route through the Copilot adapter and capture a sanitized envelope when `choices[0].message` is absent.
3. Switch the same operation to Anthropic and record stop metadata.
4. Restart Omegon and compare effective-request fingerprints before and after restart.
5. Retry in a genuinely fresh context with only a bounded task handoff.
6. Compare results to determine whether persistence is conversation-, project-, adapter-, or content-driven.

## Source evidence

- Screenshot supplied by the operator: `/Users/wilson/Downloads/tmp.png`
- Operator report: Anthropic continued returning refusals after full restarts and other retry attempts.

The screenshot should be retained with the incident record if project policy permits copying external evidence into the repository. This document references the original path but does not copy the image.

## Conclusion

The central failure was not merely that two providers failed. The harness failed to preserve the unfinished operator intent, failed to distinguish refusal recovery from truncation recovery, and failed to explain why restarting did not change the effective request. The remediation should focus on typed failure semantics, request provenance, clean-context recovery, and durable incomplete-work tracking rather than additional blind retries.
