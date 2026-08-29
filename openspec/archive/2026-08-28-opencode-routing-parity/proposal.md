# OpenCode routing parity

## Intent

Close the routing behaviors where OpenCode's current implementation provides
stronger operator-visible guarantees without weakening Omegon's route leases,
credential authority, or bounded fallback policy.

## Scope

Exact model selection and dispatch will fail closed against the active inference
inventory. Admitted HTTP endpoints using a supported adapter will become
executable through the existing provider bridge boundary. Central routing will
honor provider preference, model-level tool and reasoning evidence, server
`Retry-After` guidance, and the actual credential kind selected from the
environment.

This change does not add runtime package installation, unrestricted request
mutation hooks, recent-model defaulting, automatic cross-provider retry, or a
general small-model route.

## Success criteria

- Unknown, disabled, quarantined, incompatible, or capability-deficient exact
  routes are rejected before bridge replacement or provider dispatch.
- A validated manifest HTTP endpoint using the supported OpenAI-compatible
  adapter can construct a generation-bound executable route without hard-coded
  provider transport code.
- Retry policy uses valid server-directed backoff without weakening existing
  exhaustion limits or durable attempt evidence.
- Configured provider order deterministically affects otherwise eligible route
  selection.
- Tool and reasoning controls are admitted only when model-level evidence
  supports them.
- Route credential metadata distinguishes API-key and OAuth environment sources.
- Each behavior lands test-first with focused regression coverage.
