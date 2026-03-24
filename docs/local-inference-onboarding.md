---
id: local-inference-onboarding
title: "Local inference onboarding — smooth Ollama experience via /tutorial"
status: exploring
parent: tutorial-system
dependencies: [startup-systems-check]
tags: [local-inference, ollama, onboarding, tutorial, ux, 0.15.1]
open_questions:
  - Should the tutorial install Ollama automatically if missing (curl the install script), or just tell the user how and wait?
  - What is the minimum model that can meaningfully drive a tutorial — can a 4B model do single-tool-call steps, or do we need 14B+ for reliable tool use?
  - Should the local inference tutorial be a separate step array (STEPS_LOCAL) or a mode flag that adapts the existing demo steps (simpler prompts, lower expectations)?
jj_change_id: kvywttuknzuoxmkzorsyqmsrwqvwpnku
priority: 2
---

# Local inference onboarding — smooth Ollama experience via /tutorial

## Overview

Make local inference a first-class guided experience. If Ollama is available, the tutorial should demonstrate it — model pulling, delegation, cost-free operation. If it's not installed, the tutorial should offer to set it up or gracefully skip. The goal: a user with a beefy machine and no API key should still have a complete, impressive first experience.

## Research

### Current local inference state in Omegon

Omegon already has:
- `tools/local_inference.rs`: `manage_ollama` tool (start/stop/status/pull) and `ask_local_model` tool (delegation)
- `list_local_models` tool: lists what's loaded in Ollama
- Bootstrap panel: shows Ollama availability and model count
- Agent can delegate work to local models via `ask_local_model`
- Compaction fallback chain: tries local model first for context summarization

What's not smooth:
- No guided Ollama installation if it's missing
- No model recommendation based on hardware ("you have 32GB, pull qwen3:14b")
- `manage_ollama` with action `pull` requires the user to know model names
- No tutorial step demonstrates local inference
- No way to set a local model as the *driver* (primary agent model) from the tutorial
- The bootstrap panel shows "Ollama: 7 models" but doesn't show which ones or their sizes

## Open Questions

- Should the tutorial install Ollama automatically if missing (curl the install script), or just tell the user how and wait?
- What is the minimum model that can meaningfully drive a tutorial — can a 4B model do single-tool-call steps, or do we need 14B+ for reliable tool use?
- Should the local inference tutorial be a separate step array (STEPS_LOCAL) or a mode flag that adapts the existing demo steps (simpler prompts, lower expectations)?
