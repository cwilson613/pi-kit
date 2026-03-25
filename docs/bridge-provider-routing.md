---
id: bridge-provider-routing
title: Bridge-provider routing — dynamic provider switching without restart
status: implemented
parent: blattman-pattern-harvest
tags: [architecture, providers, routing, bridge, hot-swap]
open_questions: []
jj_change_id: ulryuzrsrlwyxtyysnklyxxtuuqqyxzy
---

# Bridge-provider routing — dynamic provider switching without restart

## Overview

The LlmBridge is created once at startup and determines which provider API receives requests. /model can change the model string but cannot change the bridge. This means cross-provider switching (/model openai:gpt-5.4 while on AnthropicClient) silently falls back to the default model on the current provider. Cleave children are unaffected — each is an independent process with its own bridge resolution.

## Research

### Current architecture (rc.44)

**Parent TUI process:**
- Bridge created once at startup via `auto_detect_bridge(model_spec)`
- Now wrapped in `Arc<RwLock<Box<dyn LlmBridge>>>` for hot-swap
- Hot-swap triggers ONLY on `/login` success (added in rc.44)
- `/model` changes the model string in SharedSettings but does NOT swap the bridge
- Cross-provider model switch silently falls back to current provider's default model

**Cleave children:**
- Each child is an independent `omegon` process spawned via `Command::new(agent_binary)`
- Each runs its own `auto_detect_bridge()` at startup
- Children inherit parent env vars and read auth.json fresh from disk
- NOT affected by parent's bridge choice — fully independent credential resolution

**Native providers (Rust HTTP clients):**
- AnthropicClient: re-resolves credentials on every `stream()` call (handles mid-session /login)
- OpenAIClient: same pattern
- OpenRouterClient: same pattern
- All strip their own prefix (anthropic:, openai:, openrouter:) from the model spec

**The gap:**
- Switching from `anthropic:claude-sonnet-4-6` to `openai:gpt-5.4` via `/model` changes the settings but the AnthropicClient still receives the request
- AnthropicClient sees `openai:gpt-5.4`, can't strip `anthropic:` prefix, falls back to `claude-sonnet-4-6`
- The user thinks they switched to OpenAI but they're still on Anthropic with the default model

## Decisions

### Decision: /model provider change should trigger bridge re-detection via the existing Arc<RwLock> hot-swap

**Status:** decided
**Rationale:** The Arc<RwLock> wrapper already exists from the /login hot-swap. The simplest fix: when SetModel arrives and the provider prefix differs from the current bridge's provider, re-run auto_detect_bridge and swap. No registry needed for now — the single-bridge-at-a-time model is correct for the current use case (one active conversation). A registry becomes relevant when we need parallel conversations on different providers, which is a future concern.

## Open Questions

*No open questions.*
