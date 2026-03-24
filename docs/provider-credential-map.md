---
id: provider-credential-map
title: Provider credential map — auth method, storage, and resolution for every supported service
status: implemented
tags: [auth, secrets, providers, credentials, reference]
---

# Provider Credential Map

Canonical reference for how each service's credentials are stored, resolved, and refreshed. Update this file when the landscape shifts.

## Auth Method Types

| Type | How it works | Storage | Refresh |
|---|---|---|---|
| **OAuth** | Browser flow → callback → token exchange | auth.json (access + refresh) | Automatic on expiry |
| **API Key** | Direct value, no expiry | auth.json + OS keyring | Never (manual rotation) |
| **Dynamic** | CLI tool executed on demand | secrets.json recipe | Every invocation |
| **Environment** | Read from env var | secrets.json recipe | Inherited from shell |

## Provider Map

### LLM Providers (drive the agent loop)

| Provider | Auth Type | Env Var | auth.json Key | /login Flow |
|---|---|---|---|---|
| Anthropic | OAuth | `ANTHROPIC_API_KEY` | `anthropic` | Browser OAuth (PKCE) |
| OpenAI | OAuth | `OPENAI_API_KEY` | `openai-codex` | Browser OAuth (PKCE) |
| OpenRouter | API Key | `OPENROUTER_API_KEY` | `openrouter` | Secret input (hidden) |

### Search Providers (web_search tool)

| Provider | Auth Type | Env Var | auth.json Key | /login Flow |
|---|---|---|---|---|
| Brave | API Key | `BRAVE_API_KEY` | `brave` | Secret input (hidden) |
| Tavily | API Key | `TAVILY_API_KEY` | `tavily` | Secret input (hidden) |
| Serper | API Key | `SERPER_API_KEY` | `serper` | Secret input (hidden) |

### Git Forges

| Provider | Auth Type | Env Var | Resolution | /login Flow |
|---|---|---|---|---|
| GitHub | Dynamic | `GITHUB_TOKEN` / `GH_TOKEN` | `cmd:gh auth token` | Auto-set on selection |
| GitLab | API Key | `GITLAB_TOKEN` | `keyring:GITLAB_TOKEN` | Secret input (hidden) |

### AI/ML Platforms

| Provider | Auth Type | Env Var | auth.json Key | /login Flow |
|---|---|---|---|---|
| Hugging Face | API Key | `HF_TOKEN` / `HUGGING_FACE_TOKEN` | `huggingface` | Secret input (hidden) |
| Replicate | API Key | `REPLICATE_API_TOKEN` | — | Via /secrets set |

### Cloud Infrastructure

| Provider | Auth Type | Env Var | Resolution | /login Flow |
|---|---|---|---|---|
| AWS | Environment | `AWS_ACCESS_KEY_ID` + `AWS_SECRET_ACCESS_KEY` | `env:` recipe | Via /secrets set |
| GCP | Environment | `GOOGLE_APPLICATION_CREDENTIALS` | `env:` recipe | Via /secrets set |
| Azure | Environment | `AZURE_CLIENT_SECRET` | `env:` recipe | Via /secrets set |

### Databases (commonly needed by agent tools)

| Service | Auth Type | Env Var | Resolution | /login Flow |
|---|---|---|---|---|
| PostgreSQL | Environment | `PGPASSWORD` / `DATABASE_URL` | `env:` recipe | Via /secrets set |
| MongoDB | Environment | `MONGO_URI` | `env:` recipe | Via /secrets set |
| Redis | Environment | `REDIS_URL` | `env:` recipe | Via /secrets set |

### Package Registries

| Service | Auth Type | Env Var | Resolution | /login Flow |
|---|---|---|---|---|
| npm | Dynamic | `NPM_TOKEN` | `cmd:npm token get` | Via /secrets set |
| crates.io | Environment | `CARGO_REGISTRY_TOKEN` | `env:` recipe | Via /secrets set |
| PyPI | Environment | `PYPI_TOKEN` | `env:` recipe | Via /secrets set |

## Resolution Priority

When Omegon needs a credential, it checks in this order:

1. **Environment variable** — fastest, highest priority
2. **auth.json** (`~/.pi/agent/auth.json`) — OAuth tokens with auto-refresh
3. **Secrets recipe** (`~/.omegon/secrets.json`) — dynamic (cmd:), keyring, env:, file:, vault:
4. **Well-known env vars** — hardcoded list, auto-detected for redaction

## Dual Storage for API Keys

API keys entered via `/login` are stored in **two places**:

1. **auth.json** — so the provider resolution chain finds them (`resolve_api_key`, `auto_detect_bridge`)
2. **OS keyring** via secrets recipe — so the output redaction engine catches them

This is intentional. auth.json drives routing. The secrets engine drives redaction. Both need to know about the key.

## OS Keychain: Scope and Boundaries

### Omegon's keychain namespace

All keyring entries use the service name **`omegon`**. This is set as `KEYRING_SERVICE` in `omegon-secrets/src/resolve.rs`. Omegon creates, reads, updates, and deletes entries **only** under this service name.

**Omegon SHALL NOT:**
- Read from other applications' keychain entries
- Enumerate the keychain beyond its own `omegon` namespace
- Access system-level credentials, browser passwords, or SSH keys via keychain APIs
- Request blanket keychain access beyond its own service scope

The OS keychain permission prompt you see is the system asking: "Do you want to let Omegon access *Omegon's own* keychain entries?" The answer is yes — that's where you told it to put your secrets.

### Why keychain prompts appear

When Omegon accesses a secret stored in the OS keyring, macOS Keychain Access or Linux Secret Service will prompt for approval. **This is expected and correct.**

The OS keychain is the most secure local storage available:
- **macOS Keychain**: encrypted at rest, protected by login password, hardware-backed on Apple Silicon. Each app gets its own keychain items — Omegon cannot see Safari's passwords or SSH keys.
- **Linux Secret Service** (GNOME Keyring / KWallet): encrypted, session-locked, D-Bus scoped.
- **Windows Credential Manager**: encrypted, user-scoped, per-application isolation.

If you don't trust your OS to store your keys, you have bigger problems than an agent tool. The prompts mean the system is working — your keys are protected by the same mechanism that guards your browser passwords.

### Prompt frequency and how to eliminate it

The OS keychain prompts once per secret per session launch. If you have 3 secrets in the keyring, that's 3 prompts every time you start Omegon. This is friction, and we know it.

**To eliminate prompts permanently on macOS:** When the Keychain Access dialog appears, click **"Always Allow"**. This grants Omegon permanent access to *its own* keychain entries (service name `omegon`). It does NOT grant access to your browser passwords, SSH keys, or any other application's entries.

This is the recommended path. One click per secret, once ever, and you never see the prompt again.

If you don't want to grant persistent keychain access, see "Opting out" below for alternatives.

### Opting out of the OS keychain

If "Always Allow" isn't your style, you have alternatives. In decreasing order of how seriously we take your operational judgment:

**1. HashiCorp Vault (recommended for teams)**
No local prompts. Centralized secret management. Token-based access with TTL. The grown-up answer:
```
/secrets set MY_KEY vault:secret/data/myproject/key
```

**2. Environment variables (recommended for solo operators)**
No prompts. Set in your shell profile, inherited by Omegon. Simple, portable, works everywhere:
```
# In ~/.zshrc or ~/.bashrc:
export OPENROUTER_API_KEY="sk-or-..."

# Then tell Omegon to resolve from env:
/secrets set OPENROUTER_API_KEY env:OPENROUTER_API_KEY
```

**3. Dynamic CLI resolution (recommended for tokens with CLIs)**
No prompts. Fresh token every time. The right answer for GitHub, npm, gcloud:
```
/secrets set GITHUB_TOKEN cmd:gh auth token
```

**4. File-based storage (you asked for it)**
Stores secrets in auth.json (0600 permissions) instead of the OS keyring. No prompts. No hardware encryption. If someone gets read access to your home directory, they get your keys.

We will implement this if operators demand it. We will also judge them quietly.

The default is keyring. The recommendation is "Always Allow." Everything else is your funeral.

## Adding a New Provider

When a new service needs credentials:

1. Add the env var name to `WELL_KNOWN_SECRET_ENVS` in `omegon-secrets/src/resolve.rs`
2. Add an entry to the `/login` selector in `tui/mod.rs` → `open_login_selector()`
3. Add an entry to the `/secrets set` catalog in `tui/mod.rs` → `SECRET_CATALOG`
4. If it's an LLM provider: add to `resolve_api_key` and `auto_detect_bridge` in `providers.rs`
5. If it needs OAuth: implement the flow in `auth.rs` (PKCE, callback server, token exchange)
6. Add the provider key mapping in the secret input handler (`take_secret` → provider_name match)
7. Update this file

## Known Landscape Shifts to Watch

- **Anthropic** may deprecate the current OAuth flow if they ship their own CLI
- **OpenAI** subscription auth may change when Codex evolves
- **OpenRouter** may add OAuth (currently API key only)
- **GitHub** may deprecate PATs in favor of fine-grained tokens (gh CLI handles this transparently)
- **Cloud providers** are moving toward workload identity / OIDC — env vars may not be the right long-term approach for AWS/GCP/Azure
