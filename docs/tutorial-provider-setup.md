---
id: tutorial-provider-setup
title: Tutorial provider setup widget — 4-path guided onboarding for unconfigured users
status: exploring
parent: free-tier-tutorial
dependencies: [openrouter-provider, startup-systems-check]
tags: [tutorial, onboarding, providers, ux, 0.15.1]
open_questions: []
jj_change_id: woxolrmlskyyypxkwllltztnnwmzznxo
priority: 1
---

# Tutorial provider setup widget — 4-path guided onboarding for unconfigured users

## Overview

When /tutorial launches with no providers configured, present a 4-option choice widget: Local (Ollama), Free (guided OpenRouter signup), Login (OAuth flow), API Key (direct entry). Each path guides the user through setup in 30-60 seconds, then flows into the normal tutorial. Only shows when systems check finds nothing — users with existing providers skip straight to the tutorial.

## Open Questions

*No open questions.*
