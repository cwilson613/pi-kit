---
id: unified-auth-surface
title: "Unified auth surface — single /login command, agent-callable, all backends"
status: exploring
tags: [auth, ux, oauth, providers, mcp, vault, secrets, unification]
open_questions:
  - "Should /vault be kept as a separate command (power-user shortcut) or fully absorbed into /auth? Vault has sub-actions (unseal, configure, init-policy) that don't map cleanly to the auth surface."
  - Should MCP remote server OAuth be added now (enable rmcp transport-streamable-http-client-reqwest + auth features) or deferred until remote MCP servers are a real use case?
issue_type: feature
priority: 2
---

# Unified auth surface — single /login command, agent-callable, all backends

## Overview

Auth is fragmented across 6 mechanisms with 3 different UX paths: CLI-only for LLM providers (`omegon login`), TUI-only for Vault (`/vault login`), and nothing for MCP remote OAuth or secrets store unlock. The operator has no single place to see what's authenticated, what's expired, and what needs attention.\n\nGoal: one `/auth` slash command + one `auth` agent tool + one `omegon auth` CLI subcommand that covers all backends uniformly.

## Research

### Current state — 6 auth mechanisms, 3 UX paths

**LLM Providers (auth.rs)**
- Anthropic OAuth: PKCE flow → `~/.pi/agent/auth.json["anthropic"]`
- OpenAI OAuth: PKCE flow → `~/.pi/agent/auth.json["openai"]`
- CLI only: `omegon login anthropic` / `omegon login openai`
- No TUI command, no agent tool
- Token refresh on expiry is automatic in `resolve_api_key_sync()`
- GitHub Copilot: managed by pi-ai internally, no Omegon-side auth

**Vault (omegon-secrets/vault.rs)**
- Token, AppRole, Kubernetes SA auth methods
- TUI only: `/vault login`, `/vault status`, `/vault unseal`
- No CLI subcommand, no agent tool
- Config in `vault.json` or VAULT_ADDR env

**Encrypted Secrets Store (omegon-secrets/store.rs)**
- Keyring backend (macOS Keychain, libsecret, Windows Credential Manager)
- Passphrase backend (Argon2id KDF)
- No CLI unlock command, no TUI command, no agent tool
- Operator interaction deferred to "when first secret is needed"

**MCP Remote Servers (rmcp auth feature)**
- rmcp crate supports OAuth for remote MCP servers (Streamable HTTP transport)
- Feature enabled in Cargo.toml but not wired
- No auth flow, no token storage

**API Keys (env vars)**
- `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, etc.
- Read by `resolve_api_key()` in providers.rs
- No management surface — operator sets env vars externally

**whoami Tool**
- Checks: git config, GitHub CLI, GitLab CLI, AWS, k8s, OCI registries
- Does NOT check: Anthropic/OpenAI OAuth, Vault, MCP, secrets store

### Proposed unified surface

**Three entry points, one backend:**

### 1. CLI: `omegon auth <action> [provider]`

```
omegon auth status              # show all auth states
omegon auth login anthropic     # OAuth flow
omegon auth login openai        # OAuth flow
omegon auth login vault         # Vault token/AppRole auth
omegon auth unlock secrets      # unlock encrypted store
omegon auth logout anthropic    # revoke + remove token
```

Replaces: `omegon login <provider>` (backward compat alias kept).

### 2. TUI: `/auth [action] [provider]`

```
/auth                           # show auth status table
/auth login anthropic           # trigger OAuth (opens browser)
/auth login vault               # prompt for Vault token
/auth unlock                    # unlock secrets store (prompt for passphrase)
/auth logout openai             # revoke token
```

Replaces: `/vault login`, `/vault status` (vault becomes a sub-surface of auth).

### 3. Agent tool: `auth_status`

```json
{"action": "status"}            // returns all provider auth states
{"action": "check", "provider": "anthropic"}  // check specific provider
```

Read-only — the agent can check auth status but cannot trigger login flows (those require operator interaction). The agent CAN detect "auth expired" and suggest `/auth login <provider>` to the operator via BusRequest::Notify.

### Auth status table format

```
Provider      Status     Method    Expires
─────────────────────────────────────────────
Anthropic     ✓ active   OAuth     2h 15m
OpenAI        ✓ active   API key   never
Vault         ✓ unsealed Token     session
Secrets       🔒 locked  keyring   —
MCP:github    ✗ expired  OAuth     —3m ago
```

### HarnessStatus integration

The `providers` field in HarnessStatus (currently empty at startup) should be populated from this unified auth check. The bootstrap panel and footer already render it — they just need real data.

### Storage consolidation

All auth tokens stay where they are (auth.json, vault.json, secrets.db, env vars). The unified surface is a *read* layer that probes each backend and presents a coherent view. No storage migration needed.

## Open Questions

- Should /vault be kept as a separate command (power-user shortcut) or fully absorbed into /auth? Vault has sub-actions (unseal, configure, init-policy) that don't map cleanly to the auth surface.
- Should MCP remote server OAuth be added now (enable rmcp transport-streamable-http-client-reqwest + auth features) or deferred until remote MCP servers are a real use case?
