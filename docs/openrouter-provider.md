---
id: openrouter-provider
title: OpenRouter as first-class provider — client, credential storage, task-aware model routing
status: exploring
parent: free-tier-tutorial
tags: [providers, openrouter, routing, free-tier, 0.15.1]
open_questions: []
jj_change_id: woxolrmlskyyypxkwllltztnnwmzznxo
priority: 1
---

# OpenRouter as first-class provider — client, credential storage, task-aware model routing

## Overview

Add OpenRouter alongside Anthropic and OpenAI as a first-class provider. Thin client (OpenAI wire protocol with different base URL and model catalog). Separate credential storage (OPENROUTER_API_KEY). Task-aware routing: driver → Qwen3 Coder 480B, cleave children → openrouter/free meta-model, compaction → Nemotron Nano 9B, memory extraction → smallest viable. The 27 free models with tool calling support make this a zero-cost full-stack inference option.

## Open Questions

*No open questions.*
