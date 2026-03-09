# Provider-neutral model controls and driver persistence

## Intent

Update operator-facing model tier commands and related messaging so /haiku, /sonnet, /opus, and set_model_tier reflect provider-neutral multi-provider routing instead of reading as Anthropic-specific products. Persist the last explicitly selected driver model so new sessions restore the last used model instead of forcing a manual switch back after startup.

## Scope

<!-- Define what is in scope and out of scope -->

## Success Criteria

<!-- How will we know this change is complete and correct? -->
