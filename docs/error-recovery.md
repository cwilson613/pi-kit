+++
id = "2edde31a-96fa-4154-b1f0-c3eb8c192d64"
kind = "document"
tags = []
aliases = []
imported_reference = false

[publication]
enabled = false
visibility = "private"

[data]
design_docs = ["design/harness-upstream-error-recovery.md"]
last_updated = "2026-08-21"
openspec_baselines = ["harness/upstream-error-recovery.md"]
subsystem = "error-recovery"
+++

# Error Recovery

> Structured provider-request failure classification, mode-aware retry, and recovery state signaling to agent and operator.

## What It Does

When an upstream provider returns an error before a model response completes, the error recovery system:

1. **Classifies** provider and transport failures into typed upstream classes.
2. **Retries** transient request/stream failures with capped exponential backoff and deterministic jitter while honoring cancellation.
3. **Exhausts** according to the active mode: bounded workers keep their configured attempt cap, while interactive runs use failure-specific time envelopes and persistent Codex overload recovery.
4. **Emits** structured retry or terminal provider-failure events with operator guidance.

Classification chain order: context-overflow → auth → quota → tool-output → rate-limit → backoff → image-too-large → invalid-request → retryable-flake → non-retryable.

## Key Files

| File | Role |
|------|------|
| `core/crates/omegon/src/upstream_errors.rs` | Provider-aware classification and recovery-action vocabulary |
| `core/crates/omegon/src/loop.rs` | Request/stream retry loop, backoff, exhaustion, and retry/failure events |
| `core/crates/omegon/src/routing.rs` | Provider/model route eligibility and fallback policy |
| `core/crates/omegon/src/invocation_service.rs` | Separate privileged invocation replay and unknown-completion safeguards |

## Design Decisions

- **Provider request retry is not invocation replay**: Provider retries repeat inference while no completed tool call has been dispatched. They never authorize resending a privileged mutation whose completion is unknown.
- **Transient-only retry**: Rate limits, overload, 5xx responses, timeouts, stream stalls, selected network failures, decode failures, dropped bridges, and incomplete/cancelled responses enter backoff. Authentication, quota, invalid-request, and other non-transient failures return without that retry loop.
- **Mode-aware exhaustion**: `--max-retries` bounds headless/worker attempts. Interactive mode uses elapsed-time envelopes, with shorter rate-limit handling, provider/model-aware stall budgets, and persistent retry for Codex overload while operator cancellation remains authoritative.
- **Observable recovery**: Every retry emits `ProviderRetry`; exhaustion emits `ProviderFailure` and actionable guidance.

## Behavioral Contracts

See `openspec/baseline/harness/upstream-error-recovery.md` for Given/When/Then scenarios covering:
- Failure classification by error type
- Retry bounds and loop prevention
- Recovery event structure
- Operator notification format

## Constraints & Known Limitations

- Provider-request retry does not imply automatic provider failover; routing and higher-level orchestration decide whether another route is eligible after exhaustion.
- Authentication, quota, malformed request, and other non-transient errors are not retried by the transient backoff loop.
- Unknown completion after privileged owner dispatch is durable invocation state. Mutating replay is denied unless the original persisted contract was idempotent or used exact owner-enforced stable-call deduplication.
- The runtime does not currently schedule even a safety-eligible unknown invocation replay, and it exposes no command that reconciles historical unknown invocations or clears mutation fences.

## Related Subsystems

- [Model Routing](model-routing.md) — provides failure classification patterns and provider cooldowns
- [Dashboard](dashboard.md) — displays recovery state and cooldown timers
- [Operator Profile](operator-profile.md) — fallback policy determines which alternate providers are allowed
