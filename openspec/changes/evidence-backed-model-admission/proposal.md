# Evidence-backed model admission

## Intent

Allow newly provider-discovered model offerings to become explicitly selectable without an Omegon release while preventing model names or incomplete discovery payloads from inventing context limits, capabilities, grades, aliases, or autonomous-routing eligibility.

## Scope

- Derive an explicit admission status from inventory evidence and route state.
- Project admission status through model catalog and model-list control output.
- Preserve conservative explicit-use semantics for ungraded discovered models.
- Add regression coverage for curated, provisional, observed, quarantined, and unavailable states.

## Non-goals

- Active compatibility probes.
- Signed remote manifests.
- Automatic alias/default promotion.
- Pricing ingestion.
