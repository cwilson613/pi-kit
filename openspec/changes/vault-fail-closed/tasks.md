# Vault client fail-closed security hardening — Tasks

## 1. PathPolicy enum and fail-closed path enforcement (vault.rs)
<!-- specs: vault-security -->

- [ ] 1.1 Add `PathPolicy` enum: `DenyAll`, `AllowList { allow: GlobSet, deny: GlobSet }` — replaces raw `allowed_paths`/`denied_paths` fields on VaultClient
- [ ] 1.2 `PathPolicy::from_config(allowed: &[String], denied: &[String])` — empty allowed → DenyAll, non-empty → AllowList with compiled globs
- [ ] 1.3 Refactor `check_path_allowed()` to match on PathPolicy — DenyAll rejects with "no paths allowed", AllowList does traversal check → deny check → allow check
- [ ] 1.4 Update `VaultClient::new()` to use `PathPolicy::from_config()`
- [ ] 1.5 Ensure health(), seal_status(), unseal() bypass path enforcement (they use fixed paths, not user-controlled)
- [ ] 1.6 Tests: empty allowlist denies, explicit allowlist allows, DenyAll + health still works

## 2. VAULT_ADDR deny-all default and error sanitization (vault.rs)
<!-- specs: vault-security -->

- [ ] 2.1 Change `load_config()` VAULT_ADDR fallback to use `allowed_paths: vec![]` (DenyAll) instead of hardcoded `secret/data/*`
- [ ] 2.2 Add `tracing::warn!` in load_config when VAULT_ADDR-only: "vault configured via VAULT_ADDR but no vault.json — all secret paths denied. Create ~/.omegon/vault.json with allowed_paths"
- [ ] 2.3 Sanitize error messages: replace `body = response.text().await.unwrap_or_default()` with truncated, scrubbed version (max 200 chars, no token-like patterns)
- [ ] 2.4 Add `sanitize_error_body(body: &str) -> String` helper — truncate + strip token patterns
- [ ] 2.5 Tests: VAULT_ADDR-only creates DenyAll config, error body sanitization strips sensitive content

## 3. Fail-closed auth in SecretsManager (lib.rs)
<!-- specs: vault-security -->

- [ ] 3.1 Refactor `init_vault()`: only store `Some(client)` when `authenticate()` succeeds; on auth failure set `vault_client = None` and log warning
- [ ] 3.2 Add `init_vault_health_only()` or fold health probe into init_vault — allow checking seal/health status even when auth fails (for /vault status and startup notification)
- [ ] 3.3 Tests: auth failure → vault_client is None, successful auth → vault_client is Some

## 4. Recipe path validation (resolve.rs)
<!-- specs: vault-security -->

- [ ] 4.1 In `resolve_vault_secret()`, validate the parsed path before passing to VaultClient — reject `..` segments, null bytes, control chars
- [ ] 4.2 Validate the key portion (after `#`) — reject empty keys, keys with path separators
- [ ] 4.3 Tests: traversal in recipe path rejected, empty key rejected, valid recipe resolves

## Cross-cutting constraints

- Every code path that can touch secrets must default to deny on unexpected conditions
- Empty allowlist = deny all, not allow all
- Auth failure = no client stored, not half-initialized client
- Error messages must not include raw Vault response bodies
- VAULT_ADDR-only must start with DenyAll + startup warning
