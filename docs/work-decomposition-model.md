---
id: work-decomposition-model
title: "Work decomposition model — beyond the cleave/execute dichotomy"
status: exploring
tags: [architecture, cleave, core]
open_questions:
  - "Should the assessment output a strategy (phased plan with modes) rather than a binary cleave/execute decision?"
  - Can scope-graph conflict detection replace or augment the current pattern-matching complexity heuristic?
  - "What's the migration path from the current binary model to the spectrum model without breaking existing /cleave usage?"
  - Does phased execution (mode 2) need new infrastructure or can it be implemented as guidance to the in-session agent?
issue_type: epic
priority: 1
---

# Work decomposition model — beyond the cleave/execute dichotomy

## Overview

The current model offers two choices: execute in-session or cleave into parallel worktree children. This binary is showing cracks — cleave has high infrastructure overhead (worktrees, branches, merge, submodule issues), while in-session execution hits context limits on large changes. The test-architect design already implies a richer model (analysis phase → implementation phase). What's the right decomposition spectrum?

## Research

### Evidence: the binary model's failure modes from this session alone

**Vault-secret-backend cleave (complexity 4.5 → cleave)**:\n- 5 children planned, 2 completed in wave 0, 3 never dispatched (max_parallel interaction)\n- Both completed children wrote to wrong paths (stale task file paths)\n- Both completed children couldn't commit (submodule boundary)\n- Net result: full cleave infrastructure spun up, 7 minutes wall time, zero usable merged code\n- We ended up doing 100% of the work in-session after salvaging one child's vault.rs from a dirty worktree\n\n**Cleave improvement implementation (complexity 12 → cleave recommended)**:\n- We ignored the recommendation and executed in-session because all three changes touch orchestrator.rs\n- Parallel children would have produced merge conflicts on the same file\n- Sequential in-session execution took ~20 minutes and produced clean code\n\n**Assessment algorithm blind spots**:\n- `systems × (1 + 0.5 × modifiers)` counts SYSTEMS not FILE CONFLICTS. 8 systems across 2 files → score 12 → recommends cleave → guaranteed merge conflict\n- No consideration of submodule boundaries, worktree limitations, or scope overlap\n- No consideration of whether the task is inherently sequential (e.g., fix → test → fix more → retest)\n- 'execute' means 'do it all yourself right now'. 'cleave' means 'full parallel infrastructure'. No middle ground."

### The missing middle: a decomposition spectrum

The real decision isn't 'cleave or not'. It's 'what decomposition strategy fits this work'. Five modes on a spectrum:\n\n### 1. Direct execution\nDo it now in this session. Single context, sequential, no git overhead. Best for: focused changes, single-file edits, quick fixes, anything that fits in context.\n\n### 2. Phased execution\nStay in-session but break work into explicit phases with compaction between them. 'First implement the client, compact, then implement the recipe kind, compact, then tests.' The agent manages its own context budget. Best for: medium tasks that are sequential but too large for one context window. This is what we actually did for the vault work.\n\n### 3. Lightweight delegation\nSpawn a single child process for a bounded subtask (like the test-architect), wait for it, consume its output. No worktrees, no branches. The child works in a temp directory or reads the repo read-only. Best for: analysis passes, code generation, test plan creation — anything where the output is a file, not a git commit.\n\n### 4. Sequential children\nSpawn children one at a time, each building on the previous one's committed work. Like cleave waves but with wave size 1 and explicit checkpoints. The parent reviews each child's output before dispatching the next. Best for: dependent task chains where later work depends on earlier output.\n\n### 5. Parallel cleave (current model)\nFull worktree isolation, parallel dispatch, merge. Best for: truly independent scope partitions with no file overlap. The actual sweet spot for this is narrow: 3-5 children working on different directories with zero shared files.\n\n### What's missing from the algorithm\nThe current assessment asks 'how complex is this?' It should ask:\n- **Are the scopes overlapping?** If children will edit the same files → mode 2 or 4, not 5\n- **Is there a dependency chain?** If task B needs task A's output → mode 4, not 5\n- **Does the infrastructure support it?** Submodules, monorepos, large binary files → mode 2 or 3\n- **What's the context budget?** If the work fits in one context window → mode 1\n- **Is there analysis work separable from implementation?** → mode 3 for analysis, then mode 4/5 for impl"

### What the test-architect design already implies

The test-architect is a mode 3 (lightweight delegation) feeding into mode 5 (parallel cleave). We're already breaking out of the binary. The pattern generalizes:\n\n- **Pre-flight analysis**: test-architect, dependency discovery, scope conflict detection\n- **Parallel implementation**: current cleave for non-overlapping scopes\n- **Post-merge verification**: coverage check, spec validation\n\nThis is a pipeline, not a binary choice. The assessment should output a STRATEGY, not just 'cleave' or 'execute'. The strategy might be:\n\n```\nStrategy: phased-parallel\nPhase 1: test-architect (lightweight delegation, 30s)\nPhase 2: vault-client + vault-guards (parallel cleave, independent scopes)\nPhase 3: vault-recipe + vault-tui + vault-integrations (parallel cleave, depends on phase 2)\nPhase 4: coverage check (deterministic, <1s)\n```\n\nOr for the cleave-improvements work:\n```\nStrategy: phased-execution\nPhase 1: worktree.rs changes (submodule handling)\nPhase 2: context.rs (new module, depends on worktree.rs API)\nPhase 3: orchestrator.rs (wiring, depends on both)\nPhase 4: progress.rs (independent, could parallel with 3)\nCompact between phases.\n```\n\nThe strategy is the plan. The assessment produces it, not just a yes/no."

### Algorithm redesign: scope-graph analysis replaces system counting

The current formula `systems × (1 + 0.5 × modifiers)` is a proxy for 'how hard is this'. It counts nouns in the directive. It should instead analyze the SCOPE GRAPH:\n\n1. **Parse the plan's file scopes** (from OpenSpec tasks.md or the plan_json)\n2. **Build a conflict graph**: edges between children that share files\n3. **Compute maximum independent set**: children that CAN run in parallel without conflicts\n4. **Detect sequential dependencies**: children where B's scope includes A's output\n5. **Check infrastructure constraints**: submodules, monorepo boundaries, worktree limitations\n\nFrom the graph, derive the strategy:\n- Independent set size ≥ 3 and no infrastructure constraints → parallel cleave\n- Independent set size < 3 but tasks are separable → sequential children or phased execution\n- High file overlap → phased execution (stay in-session)\n- Single file → direct execution\n- Analysis + implementation separable → lightweight delegation then parallel\n\nThis replaces the pattern-matching heuristic with a structural analysis that can't be fooled by how the directive is worded. It also naturally discovers the wave structure instead of requiring the operator/agent to specify depends_on manually."

## Open Questions

- Should the assessment output a strategy (phased plan with modes) rather than a binary cleave/execute decision?
- Can scope-graph conflict detection replace or augment the current pattern-matching complexity heuristic?
- What's the migration path from the current binary model to the spectrum model without breaking existing /cleave usage?
- Does phased execution (mode 2) need new infrastructure or can it be implemented as guidance to the in-session agent?
