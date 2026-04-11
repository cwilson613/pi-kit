---
id: workspace-ownership-model
title: "Workspace ownership model — one mutable agent per workspace"
status: exploring
tags: []
open_questions:
  - "What local runtime artifact should be the source of truth for mutable workspace ownership: per-workspace lease file only, a project-level local registry, or both?"
  - "How should a second mutable agent attach behave by default: refuse, offer read-only attach, or auto-create a sibling worktree/workspace?"
  - "How are release and benchmark authority isolated so RC cuts and release-candidate benchmarks cannot silently target post-tag HEAD state?"
dependencies: []
related: []
---

# Workspace ownership model — one mutable agent per workspace

## Overview

Omegon currently has a strong model for **project state**:
- the project is git-bound
- durable cognition is tracked in git (`.omegon/`, docs, specs, memory facts)
- lifecycle state is designed to survive across sessions and machines

What it lacks is an equally strong model for **workspace state**.

That gap shows up whenever multiple agents operate in parallel against the same repository path:
- RC identity becomes ambiguous
- benchmark provenance becomes untrustworthy
- controller tuning loses causal attribution
- multiple mutable agents can silently share a filesystem like they are one engineer, which they are not

The correct mental model is simple:

> Parallel Omegon agents should behave like parallel engineers.

That means parallel mutable work must be isolated in separate workspaces, just as two engineers would work on separate branches/worktrees.

This design introduces a first-class **workspace ownership** model so the filesystem hygiene problem is solved at the runtime/control-plane layer rather than left to operator folklore.

## Decisions

### Workspace ownership is a first-class runtime primitive

**Status:** decided

**Rationale:** Parallel mutable Omegon agents must behave like parallel engineers. Project state remains durable and git-tracked, but workspace ownership, leases, and occupancy are machine-local runtime coordination state. One mutable agent per workspace becomes the core filesystem hygiene rule, and cleave uses the same workspace model as all other parallel execution.
