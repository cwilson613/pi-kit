---
id: free-tier-tutorial
title: Free-tier tutorial — onboarding without a Pro subscription or API key
status: exploring
parent: tutorial-system
dependencies: [startup-systems-check]
tags: [tutorial, free-tier, accessibility, local-inference, onboarding, 0.15.1]
open_questions:
  - Can we use the Anthropic free tier programmatically (API key from free claude.ai account), or is free-tier access only through the web UI?
  - "Is there a cheap/free OpenAI-compatible API (Groq free tier, Together.ai free tier, Google Gemini free tier) that could power a demo without Anthropic credentials?"
  - "Should the tutorial auto-detect the user's segment at startup and select the right tutorial variant, or present the choice explicitly?"
jj_change_id: kvywttuknzuoxmkzorsyqmsrwqvwpnku
priority: 2
---

# Free-tier tutorial — onboarding without a Pro subscription or API key

## Overview

Design a tutorial experience for users who have no paid API subscription at all. Three user segments exist: (1) beefy machine — can run full local stack, treat like a normal user but with local models, (2) normal laptop — can maybe run a small model, limited capabilities, (3) no GPU at all — needs a completely different approach. The question: what can we show someone with zero tokens to spend that still demonstrates why Omegon matters?

## Research

### User segments and their capabilities

Three segments exist for users with no paid API key:

**Segment A: Beefy local machine** (32GB+ RAM, Apple Silicon M2 Pro+ or NVIDIA GPU with 12GB+ VRAM)
- Can run 14B-32B models locally via Ollama
- Full agent loop possible — local model as driver
- Tutorial experience: identical to paid, but slower and dumber
- Key gap: local models are worse at multi-step tool use, so the demo's 4-tool-call auto-prompts may produce worse results

**Segment B: Normal laptop** (16GB RAM, Apple Silicon M1/M2 or integrated GPU)
- Can run 4B-8B models — enough for delegation, memory extraction, compaction
- NOT enough for driver — these models can't reliably do multi-step tool orchestration
- Tutorial experience: needs a cloud driver (even free tier) for the agent loop, local for support tasks
- Key gap: Claude free tier has aggressive rate limits (messages per day)

**Segment C: Minimal hardware** (8GB RAM, no GPU, or older Intel Mac)
- Can maybe run a 1B-4B model — barely useful
- Tutorial experience: needs cloud entirely, or a fundamentally different demo
- Key gap: everything costs tokens they don't have

The Anthropic free tier (claude.ai) gives ~15-20 messages per day. That's enough for maybe 3-4 tutorial auto-prompt steps if we're careful. The current demo has 4 auto-prompts. It's tight but feasible if we don't waste messages.

### Tutorial design options for zero-cost onboarding

**Option 1: UI-only tour (zero tokens)**
Skip all AutoPrompt steps. The tutorial becomes a pure cockpit tour — Tab through 5-6 steps that explain the panels, show pre-rendered screenshots or canned output of what the AI would do, explain the workflow. No agent turns fire at all. This shows off the UI and explains the concepts but doesn't demonstrate real AI work.

Pro: Works for everyone. Zero cost. Fast.
Con: Doesn't show the magic. The user sees a cockpit they've never seen fly.

**Option 2: Minimal demo (2-3 agent turns)**
Strip the demo to its essentials: (1) read the project, (2) explain the fix plan, (3) done. Skip the design decision step, skip the cleave, skip the verify. The user sees the AI read code and explain it — that's impressive enough for a taste. Then say "upgrade for the full parallel fix experience."

Pro: Shows real AI work. 2-3 messages is within free tier limits.
Con: Doesn't show the signature feature (parallel branches).

**Option 3: Local-powered demo (zero cloud cost)**
For Segment A users: run the entire demo with a local model as driver. The results will be rougher but the experience is complete — read, design, spec, cleave, verify, browser. The tutorial adapts its expectations: "local models may produce different output, that's fine."

Pro: Full experience, zero cost. 
Con: Only works for beefy machines. Local models may fail at complex tool orchestration.

**Option 4: Hybrid — local driver + cloud fallback**
Try local first. If the local model fails a step (can't do multi-tool orchestration), fall back to one cloud API call for that step. Budget: max 2-3 cloud calls, rest local. The tutorial adapts based on what's available.

Pro: Best of both worlds.
Con: Complex routing logic. Hard to test. User experience unpredictable.

**Option 5: Pre-recorded demo**
Ship a recorded session as a replay — the tutorial "plays back" a real session with real tool calls and real output, but no live inference. The user sees exactly what a real session looks like, with real timing, real instrument activity, real design tree updates. Like watching a gameplay trailer before buying the game.

Pro: Perfect quality. Zero cost. Works everywhere.
Con: Not live. The user isn't doing anything. Less impressive once they realize it's canned.

## Open Questions

- Can we use the Anthropic free tier programmatically (API key from free claude.ai account), or is free-tier access only through the web UI?
- Is there a cheap/free OpenAI-compatible API (Groq free tier, Together.ai free tier, Google Gemini free tier) that could power a demo without Anthropic credentials?
- Should the tutorial auto-detect the user's segment at startup and select the right tutorial variant, or present the choice explicitly?
