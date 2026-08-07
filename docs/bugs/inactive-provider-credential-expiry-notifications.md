+++
title = "Bug: Inactive-provider credential expiry produces misleading notifications"
tags = ["bug", "providers", "credentials", "routing", "tui"]
+++

# Bug: Inactive-provider credential expiry produces misleading notifications

**Status:** Observed; ready for investigation  
**Suggested branch:** `fix/inactive-provider-credential-notifications`  
**Primary surfaces:** Provider authentication, route state, notifications

## Assignment brief

Trace credential-state detection through provider routing and notification rendering. Implement relevance-aware notification behavior that distinguishes the active provider, configured fallbacks, providers required by queued work, and unused providers. Do not assume whether the defect originates in credential polling, event projection, routing state, or the TUI.

## Observed evidence

While an OpenAI/Codex GPT route remained active and healthy, expired Anthropic credentials produced an expired-credentials popup/toast. The notification appeared to describe an active-session failure even though the affected provider was unused.

## Intended behavior

Credential state remains observable for all configured providers, but presentation reflects operational relevance:

| Provider relationship | Expected presentation |
|---|---|
| Active route | Prominent, actionable authentication failure |
| Configured fallback | Non-blocking degraded-failover warning |
| Required by queued/delegated work | Warning attached to affected work |
| Configured but unused | Passive status; no disruptive popup |

Repeated checks of unchanged state must not emit duplicate notifications.

## Scope

- Credential health event production and projection.
- Provider relevance classification at notification time.
- Severity, wording, deduplication, and refresh/login actions.
- Reclassification after route, fallback, or queued-work changes.
- On-demand provider credential status.

## Non-goals

- Redesigning secret storage or refresh-token persistence.
- Changing provider routing strategy beyond exposing relevance.
- Suppressing credential health information entirely.
- Logging credential payloads or token metadata.

## Investigation targets

Search for `credential`, `expiry`, `refresh`, `auth`, `preflight`, `provider status`, `toast`, `popup`, `notification`, and `severity`. Trace active/fallback route projections, delegated provider selection, polling cadence, event emission, and notification deduplication. Verify whether expiry is proactive metadata inspection or reactive provider rejection.

Do not claim a file owns behavior until the implementation has been read.

## Architectural constraints

- The credential observer reports facts; route relevance determines operational severity.
- Relevance must be evaluated from current runtime state rather than captured permanently at detection time.
- Notification identity needs a stable provider/state key for transition deduplication.
- Events and diagnostics must redact all secret material.
- TUI, ACP, daemon, and web projections should consume one semantic health classification.

## Implementation sequence

1. Map credential-state producers and notification consumers.
2. Introduce or identify a provider-relevance projection.
3. Define a severity/presentation matrix and stable notification identity.
4. Apply the policy before disruptive notification rendering.
5. Add route-switch and credential-refresh state transitions.
6. Expose passive status through the existing provider-status surface.
7. Add focused unit and surface-level regressions.

## Acceptance criteria

1. Expiry of an unused provider does not interrupt a healthy active session.
2. Any passive message names the provider and states that the current route is unaffected.
3. Active-provider expiry remains prominent and provides the correct login/refresh action.
4. Fallback expiry reports degraded failover without claiming the current turn failed.
5. Queued or delegated work using the provider is identified as affected.
6. One unchanged credential state emits at most one toast.
7. Switching to known-expired credentials surfaces the problem before dispatch where possible.
8. Credential status remains queryable when no toast is shown.
9. No token, credential payload, or sensitive path appears in logs, events, or UI.

## Regression plan

Cover both Anthropic-expired/OpenAI-active and the inverse; expired first fallback; provider needed by queued work; successful refresh; repeated polling; and switching from a healthy route to the expired provider. Test semantic classification separately from TUI rendering.

## Validation

Use the narrow provider/auth and notification tests discovered during investigation, then run:

```bash
cargo test -p omegon <focused-filter>
just clippy-changed
git diff --check
```

## Dependencies and conflict risks

Likely conflicts include provider routing, authentication, notification rendering, and delegated-work scheduling. Coordinate event-schema changes before parallel surface work. This branch must not absorb unrelated provider-health UI redesign.

## Definition of done

The relevance policy is implemented once, all affected surfaces project it consistently, the regression matrix passes, secrets remain redacted, validation passes, and the focused branch contains only this fix and its tests/documentation.
