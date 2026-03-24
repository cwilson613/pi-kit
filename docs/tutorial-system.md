---
id: tutorial-system
title: Interactive /tutorial system — structured onboarding replacing /demo
status: implementing
parent: null
tags: [tutorial, onboarding, ux, tui, overlay]
open_questions: []
jj_change_id: null
issue_type: feature
priority: 2
---

# Interactive /tutorial system — structured onboarding replacing /demo

## Overview

The tutorial system replaces the old `/demo` with a two-layer onboarding experience: a **compiled overlay engine** for game-style first-play guidance, and a **lesson runner** for markdown-based tutorial content. Both are wired into the TUI and controlled by the harness — the agent never decides pacing.

The overlay engine is the primary system. It renders a floating callout on top of the normal UI, highlights relevant cockpit regions, and auto-sends prompts to the agent. The operator advances with Tab, goes back with Shift+Tab, or dismisses with Esc. Steps can also wait for a specific slash command (`/dash`, `/cleave`) or any user input before advancing. The overlay passes keyboard input through to the editor on Command/AnyInput steps so the user can actually type.

Two step arrays are compiled in: `STEPS_DEMO` (9 steps, sprint board project) and `STEPS_HANDS_ON` (7 steps, user's own project). A project-choice widget on step 0 lets users pick between demo and hands-on mode.

## Architecture

### Overlay engine (`tui/tutorial.rs`)

The core types:

```rust
pub struct Step {
    pub title: &'static str,
    pub body: &'static str,
    pub anchor: Anchor,       // Center | Upper
    pub trigger: Trigger,     // Enter | Command("dash") | AnyInput | AutoPrompt("...")
    pub highlight: Option<Highlight>,  // InstrumentPanel | EnginePanel | InputBar | Dashboard
}

pub struct Tutorial {
    current: usize,
    pub active: bool,
    pub auto_prompt_sent: bool,
    pub is_demo: bool,
    pub has_design_tree: bool,
    pub choice: TutorialChoice,  // Demo | MyProject
    pub choice_confirmed: bool,
}
```

Key behaviors:
- **`Trigger::Enter`** — Tab advances. All other keys consumed (overlay blocks input).
- **`Trigger::Command("dash")`** — Overlay stays visible. Keys pass through to editor. Step advances when the slash command fires.
- **`Trigger::AutoPrompt("...")`** — Tab sends the compiled prompt to the agent. Overlay shows "agent is working..." until the turn completes, then auto-advances.
- **`Trigger::AnyInput`** — Keys pass through. Step advances when user sends any message.

Rendering: the overlay positions itself using smart anchoring — steps highlighting footer elements go to the upper area, steps highlighting the dashboard sit in the conversation zone. An active AutoPrompt step shows a large centered overlay to cover conversation chaos while the agent works.

### Lesson runner (`TutorialState` in `tui/mod.rs`)

A simpler fallback for projects with `.omegon/tutorial/*.md` files. Loads markdown with frontmatter, queues lesson content as prompts. Progress persisted in `progress.json`. Used when the tutorial directory exists in the project.

### TUI integration (`tui/mod.rs`)

- `App.tutorial_overlay: Option<Tutorial>` — the active overlay
- `App.tutorial: Option<TutorialState>` — the active lesson runner
- **Rendering**: overlay renders after effects, before toasts
- **Event loop**: Tab/Esc/ShiftTab intercepted when overlay active; Command/AnyInput steps pass all other keys through
- **AgentEnd**: calls `on_agent_turn_complete()` to auto-advance AutoPrompt steps
- **Slash commands**: `check_command(cmd)` advances Command-triggered steps
- **User messages**: `check_any_input()` advances AnyInput steps

### Demo project (`test-project/`)

A broken sprint board (browser-based task tracker) with 4 seeded bugs, 6 design nodes, pre-written OpenSpec specs, and 5 memory facts. Used by `STEPS_DEMO`. Located in `test-project/` in the repo; cloned from `styrene-lab/omegon-demo` when users run `/tutorial demo` outside the dev workspace.

## Step content (STEPS_DEMO — 9 steps)

1. **Welcome to the Omegon Demo** — "4 bugs, about 3 minutes" (Enter)
2. **Your Cockpit** — quick orientation: bottom-left model, bottom-center activity, right panel design notes (Enter, highlights instruments)
3. **Reading the Code** — AI reads project, stores facts, ~30 seconds (AutoPrompt, highlights instruments)
4. **Making a Design Decision** — AI researches search-filter open question, records decision (AutoPrompt, highlights dashboard)
5. **The Fix Plan** — AI reads OpenSpec spec and explains 4-branch fix plan (AutoPrompt)
6. **Fix All 4 Bugs** — user types `/cleave fix-board-bugs`, overlay stays visible (Command("cleave"), highlights instruments)
7. **Verify and Launch** — AI checks fixes and opens browser (AutoPrompt)
8. **Web Dashboard** — optional, skippable with Tab (Command("dash"))
9. **What Just Happened** — summary without jargon, next steps (Enter)

## Step content (STEPS_HANDS_ON — 7 steps)

1. **Welcome to Omegon** — "your AI coding agent, about 5 minutes" (Enter)
2. **Your Cockpit** — same orientation step (Enter)
3. **Reading Your Code** — AI reads user's project, stores 3 facts (AutoPrompt)
4. **Design Notes** — AI creates or explores a design node (AutoPrompt)
5. **Writing a Spec** — AI proposes an improvement and writes Given/When/Then spec (AutoPrompt)
6. **Web Dashboard** — optional (Command("dash"))
7. **What's Next** — summary, mention `/tutorial demo` for the full demo (Enter)

## Slash commands

| Command | Behavior |
|---|---|
| `/tutorial` | Start overlay (hands-on mode), or resume if active |
| `/tutorial demo` | Start overlay (demo mode) |
| `/tutorial status` | Show current step/total and mode |
| `/tutorial reset` | Dismiss and clear overlay |
| `/next` | Advance overlay or lesson runner |
| `/prev` | Go back in overlay or lesson runner |

When `.omegon/tutorial/` exists with markdown lesson files, `/tutorial` uses the lesson runner instead of the overlay.

## Decisions

### Individual markdown files per lesson with YAML frontmatter (decided)
For the lesson runner path. Each lesson is self-contained — the agent sees one file at a time. Files ordered by numeric prefix. Frontmatter carries title. This is the simpler of the two systems and exists as a fallback.

### Harness-controlled pacing (decided)
The root cause of the old /demo failure was delegating pacing to the agent. Both systems enforce structural pacing: the overlay feeds one step at a time via compiled steps; the lesson runner queues one markdown file as a prompt.

### Sandbox tutorial project (decided)
`/tutorial demo` clones `styrene-lab/omegon-demo` into `/tmp/omegon-tutorial` and exec's omegon there. Safe to experiment. The test-project/ in this repo is the source of that demo content.

### Progress persistence (decided)
Lesson runner persists to `.omegon/tutorial/progress.json`. The overlay doesn't persist progress — it's a single-session experience (~3 minutes).

### Junior-friendly content (decided, rc.16)
Tutorial step text was rewritten for accessibility:
- Collapsed 3 cockpit tour steps into 1
- Removed all Omegon jargon from first encounter (no Retribution/Victory/Gloriana, no "inference instruments", no "design tree nodes")
- Cleave step uses Command trigger — overlay stays visible, no Esc/come-back dance
- Web Dashboard moved to after the action, made optional/skippable
- Time estimates on every auto-prompt step
- Recovery text on the cleave step

## Cost philosophy

This tool is not designed to save tokens or save costs. Those levers exist to service the primary goal of Omegon: building functional things. Cost is a real-world constraint, so all levers are tunable and the entire system takes them into account. But remember: Omegon will smile as it sacrifices your wallet upon the altar of productivity.

This applies to the tutorial: auto-prompts are not cheap (each one is a full agent turn with tool calls), but they're the right way to show what the tool does. A tutorial that doesn't demonstrate real AI work is worthless.
