# Quiet startup and an explicit connection surface

## Intent

Startup currently presents the supported provider catalog as a readiness checklist.
Missing credentials for providers the operator has never chosen appear as warnings.
Move provider discovery and setup behind `/connect`, and show only the current route
and actionable route problems at startup.

## Scope

- Use the existing shared menu and authentication handlers for `/connect`.
- Keep startup provider output bounded in both inline and fullscreen presentations,
  independently of Active/Full detail.
- Make `/connect` the discoverable setup command across supported command surfaces.
- Preserve `/login` temporarily for compatibility; reserve its future purpose for
  renewing credentials on existing provider/plugin connections.
- Keep model selection under the existing `/model` command (`/models` is a TUI alias).

Plugin renewal, a new credential store, provider protocol changes, and a general
connection framework are outside this first implementation. Implementation is in
progress on `feature/provider-connect-surface`; verification is tracked separately.

## Success criteria

- Startup does not enumerate available providers or warn about unrelated missing credentials.
- An unavailable selected route gives one scoped action; a missing route points to `/connect`.
- Operators can inspect existing connections and deliberately discover another provider.
- Opening or searching the connection menu does not open browsers or modify credentials.
- Existing authentication safety, secret handling, and selected/serving route distinctions survive.
- Headless captured acceptance proves the behavior in both presentations.

## Reference

OpenCode 2 beta documents connecting built-in providers through `/connect`, with
model selection as a separate operation. Adopt that separation while retaining
Omegon's route admission and credential provenance contracts.

- https://opencode.ai/v2/docs
- https://opencode.ai/v2/docs/providers
