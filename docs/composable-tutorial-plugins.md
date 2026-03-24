---
id: composable-tutorial-plugins
title: Composable tutorial plugins — URL-addressable tutorial packs as the onboarding primitive
status: exploring
parent: tutorial-system
tags: [architecture, tutorials, plugins, onboarding, ux, extensibility, 0.16.0]
open_questions:
  - What is the tutorial pack manifest format — TOML like plugins, or markdown with frontmatter like the current lessons?
  - How do tutorial packs declare and bundle seed artifacts (design nodes, memory facts, OpenSpec changes, project files) vs. generating them at runtime via auto-prompts?
  - Should tutorial steps remain compiled Rust consts, or should the overlay engine accept runtime-loaded step definitions from the pack format?
  - "What is the URL addressing scheme — git URLs like persona repos, or a registry/namespace like `omegon tutorial run styrene-lab/rust-onboarding`?"
  - "How does the tutorial sandbox interact with the user's real project — isolated temp dir (like current demo), overlay on the user's repo, or configurable per-pack?"
  - What validation and security model applies to community tutorial packs — can auto-prompts execute arbitrary tool calls, and how is that trust boundary managed?
jj_change_id: rmurttsluurmpnuxnswpwyswlzlzvlqn
issue_type: feature
priority: 2
---

# Composable tutorial plugins — URL-addressable tutorial packs as the onboarding primitive

## Overview

Generalize the hardcoded tutorial overlay into a composable plugin system where tutorial packs are URL-addressable, community-authorable, and project-specific. The built-in demo and hands-on tutorials become the reference implementations of this format.

A tutorial pack is a git repo (or subdirectory) containing step definitions, seed artifacts (design nodes, memory facts, OpenSpec changes, project files), and a manifest declaring prerequisites and capabilities exercised. Omegon fetches, validates, and runs them through the same overlay engine that powers the current /tutorial.

This transforms onboarding from "one tutorial fits all" into a composable library where teams, communities, and Omegon itself can ship purpose-built learning experiences — language-specific tours, workflow deep-dives, feature showcases — all running inside the real TUI with real instruments.

## Research

### Current tutorial architecture (starting point)

The current tutorial system has two layers:

1. **Overlay engine** (`tui/tutorial.rs`) — compiled Rust structs with `Step { title, body, anchor, trigger, highlight }`. Two hardcoded step arrays: `STEPS_DEMO` (9 steps, sprint board project) and `STEPS_HANDS_ON` (7 steps, user's own project). Triggers: `Enter` (passive), `Command("cleave")` / `Command("dash")` (wait for slash command — overlay stays visible, keys pass through), `AnyInput` (wait for user message), `AutoPrompt("...")` (auto-send to agent, auto-advance on agent turn complete). Renders as a floating overlay with smart positioning, highlight hints, and input passthrough for Command/AnyInput steps.

2. **Lesson runner** (`TutorialState` in `tui/mod.rs`) — loads markdown files from `.omegon/tutorial/` with frontmatter, queues lesson content as prompts. Progress persisted to `progress.json`.

The overlay engine is the richer system — it has visual anchoring, highlight regions, auto-prompt lifecycle, project-choice widget, and input passthrough for Command steps. The lesson runner is simpler (just markdown → prompt queue).

Key insight: the overlay engine's `Step` struct is almost a universal tutorial step primitive. Making it loadable from a manifest rather than compiled would unlock external authoring.

### Existing composable patterns in Omegon

Three existing systems already use URL-addressable, externally-authored content:

1. **Persona repos** — git URL → clone → TOML manifest + prompt files + skills. Addressing: `omegon persona install github.com/user/persona-name`. Trust: operator explicitly installs, prompts are injected into system context.

2. **Skills** — SKILL.md files discovered by path convention. Loaded on-demand when task matches description. Simple: markdown + optional templates.

3. **Plugin loader** — TOML manifest in `.omegon/plugins/`, declares HTTP-backed tools and context injections. Most structured of the three.

A tutorial pack format could inherit from all three: git-addressable like personas, markdown-heavy like skills, and manifest-driven like plugins. The TOML manifest pattern (`[tutorial]` section with metadata, step references, artifact declarations) is the most natural fit.

### Proposed tutorial pack structure

```
my-tutorial/
├── tutorial.toml          # manifest: metadata, step order, prerequisites
├── steps/
│   ├── 01-welcome.md      # step definition (frontmatter + body)
│   ├── 02-read-code.md
│   └── ...
├── seed/                  # optional: artifacts to scaffold before step 1
│   ├── design-tree/       # design nodes to create
│   ├── memory/            # facts to seed
│   ├── openspec/          # pre-written specs/tasks
│   └── project/           # project files (src/, tests/, etc.)
└── README.md              # human-readable description
```

Each step `.md` file uses frontmatter for machine-readable fields:
```yaml
---
title: "Reading the Broken App"
anchor: upper           # center | upper
trigger: auto-prompt    # enter | command:dash | any-input | auto-prompt
highlight: instruments  # instruments | engine | input | dashboard | none
---
The agent will read the sprint board code...

---prompt---
Read this project. Start with README.md...
```

The `tutorial.toml` manifest:
```toml
[tutorial]
name = "sprint-board-demo"
title = "Fix a Broken Sprint Board"
description = "Watch Omegon find, spec, and fix 4 bugs in parallel"
mode = "demo"           # demo (temp dir) | hands-on (user's project)
min_version = "0.15.0"

[prerequisites]
tools = ["design_tree", "openspec_manage", "cleave_run"]
```

### Migration path from current hardcoded steps

The migration is additive — the compiled steps remain as the fallback/default, and the overlay engine gains a second code path for loading runtime steps from the pack format.

Phase 1: Extract `Step` parsing from markdown frontmatter into `tutorial.rs`. The overlay engine accepts `Vec<Step>` from either compiled consts or parsed files. The `Tutorial::from_pack(path)` constructor loads and validates a pack directory.

Phase 2: The built-in demo becomes a bundled pack (included via `include_dir!` or shipped as a git submodule reference). `/tutorial demo` still works identically — it just loads from the pack format instead of compiled consts.

Phase 3: `/tutorial run <url>` clones a pack from a git URL, scaffolds seed artifacts into a temp dir, and runs the overlay. `/tutorial list` shows installed packs.

The overlay rendering engine, input passthrough, highlight system, and auto-prompt lifecycle are all unchanged — they already operate on the `Step` trait boundary. Only the loading path is new.

## Open Questions

- What is the tutorial pack manifest format — TOML like plugins, or markdown with frontmatter like the current lessons?
- How do tutorial packs declare and bundle seed artifacts (design nodes, memory facts, OpenSpec changes, project files) vs. generating them at runtime via auto-prompts?
- Should tutorial steps remain compiled Rust consts, or should the overlay engine accept runtime-loaded step definitions from the pack format?
- What is the URL addressing scheme — git URLs like persona repos, or a registry/namespace like `omegon tutorial run styrene-lab/rust-onboarding`?
- How does the tutorial sandbox interact with the user's real project — isolated temp dir (like current demo), overlay on the user's repo, or configurable per-pack?
- What validation and security model applies to community tutorial packs — can auto-prompts execute arbitrary tool calls, and how is that trust boundary managed?
