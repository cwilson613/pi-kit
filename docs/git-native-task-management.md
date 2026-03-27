---
id: git-native-task-management
title: "Git-native task management — extend design tree into a full in-repo issue/task system"
status: exploring
tags: [architecture, task-management, git, design-tree, workflow, strategic]
open_questions:
  - Should we add a GitHub Issues bridge (bidirectional sync like git-issue does), or is git-native-only sufficient? A bridge adds complexity but lets collaborators who prefer GitHub workflows participate. For a single-operator project, the bridge is unnecessary overhead.
  - "Should the index.json be git-tracked (shared across clones/agents) or gitignored (local cache, rebuilt on startup)? Tracked: consistent state across sessions, no rebuild cost. Gitignored: no merge conflicts from concurrent mutations, simpler."
  - "Do we want a `/board` TUI command that shows a kanban-style view, or is the dashboard sidebar tree sufficient? The sidebar groups by parent (hierarchy), not by status (workflow). Both views are useful for different questions — hierarchy answers 'what's the scope?', kanban answers 'what's in progress?'"
jj_change_id: xvrlvxwxvttwnnmpmsxoqlosqqvulqrr
issue_type: epic
priority: 2
---

# Git-native task management — extend design tree into a full in-repo issue/task system

## Overview

The design tree is already ~80% of a task management system. It has: hierarchical nodes with status lifecycle, priority, issue types (epic/feature/task/bug/chore), open questions, decisions with rationale, research sections, dependencies, related nodes, file scope, acceptance criteria, readiness scoring, OpenSpec binding, branch binding, lifecycle doctor, and focus injection — all git-backed as markdown in docs/.

The missing 20% to make it a complete git-native issue tracker (replacing GitHub Issues/Linear/Jira for single-project use):

1. **Filtering/querying** — list by status, tag, priority, assignee, milestone (currently: list returns everything)
2. **Activity feed** — recent changes across all nodes (git log provides this, but we don't surface it)
3. **Milestone binding** — milestones.json exists but isn't linked to design nodes
4. **Assignee/ownership** — who's working on it (human, agent, cleave child)
5. **Effort estimation** — estimated vs actual effort per node
6. **Due dates** — optional deadlines
7. **Board/kanban view** — status-based grouping for the TUI and web dashboard
8. **Closed/archived state** — done nodes that shouldn't clutter the active list

Storage moves from docs/ to .omegon/design/ — separating machine-managed design state from hand-authored documentation. git-backed, human-readable markdown, diffable, no external service dependency. Like git-bug but using markdown files instead of git objects, making them browsable without special tooling.

## Research

### What we already have vs what's missing — the 80/20 gap



### Already implemented (the 80%)

| Capability | Current implementation | Equivalent in GitHub Issues |
|------------|----------------------|----------------------------|
| Node lifecycle | seed→exploring→resolved→decided→implementing→implemented→blocked→deferred | Open→In Progress→Done |
| Hierarchy | parent/children (unlimited depth) | Labels + Milestones (flat) |
| Issue types | epic/feature/task/bug/chore | Labels |
| Priority | 1-5 (P1 critical → P5 trivial) | Priority labels |
| Tags | Freeform string tags | Labels |
| Dependencies | Explicit dep edges, `blocked` and `ready` queries | Manual cross-references |
| Research | Structured heading+content sections | Comment threads (unstructured) |
| Decisions | Title + status + rationale, per-node | Comments (lost in noise) |
| Open questions | First-class, including [assumption] tagging | — (nothing equivalent) |
| Acceptance criteria | Scenarios + falsifiability + constraints | Checkbox lists |
| File scope | Path + description + action (new/modified/deleted) | — |
| Branch binding | Linked git branches per node | Branch refs in PR |
| OpenSpec binding | Spec-driven implementation lifecycle | — |
| Readiness scoring | decisions/(decisions+questions) = 0.0–1.0 | — |
| Lifecycle doctor | Automated drift detection (37 finding types) | — |
| Agent-native tools | design_tree/design_tree_update (query+mutation) | API |
| Context injection | Focused node injected into system prompt | — |
| Git-backed | Markdown files, diffable, no external service | External service |

### Missing (the 20%)

**1. Filtered queries** (SMALL delta)
Current `list` returns all 255 nodes as JSON. Need:
- `list --status=exploring --priority=1` 
- `list --tag=rust --type=bug`
- `list --milestone=0.15.3`
- `list --assignee=cwilson`

Implementation: add optional filter parameters to the `design_tree` tool's `list` action. ~50 lines of filter logic.

**2. Milestone binding** (SMALL delta)
`.omegon/milestones.json` exists with release tracking. Design nodes have no `milestone` field. Add:
- `milestone: Option<String>` to `DesignNode`
- `set_milestone` action to `design_tree_update`
- Milestone-grouped views in dashboard

The milestone JSON already has a `nodes: Vec<String>` field — it's just empty. Wire the binding.

**3. Assignee/ownership** (SMALL delta)
Add `assignee: Option<String>` to `DesignNode`. Values: operator name, agent instance ID, or `cleave:{child_label}`. The harness can auto-assign when a cleave child starts working on a node.

For single-developer use: always the operator. For multi-agent: the agent/session that has focus. For teams: git author identity.

**4. Activity feed** (MEDIUM delta)
Two approaches:
- **Git-derived**: `git log --follow -- docs/{node-id}.md` gives the full activity history of any node. Surface this via a `design_tree(action='history', node_id='...')` query.
- **Embedded changelog**: Append `## Activity` section to each node's markdown with timestamped entries. Pro: readable without git. Con: grows files, merge conflicts.

Recommendation: git-derived. The activity IS the git history. Add a `history` action that shells out to `git log`.

**5. Effort estimation** (SMALL delta)
Add `estimate: Option<String>` to frontmatter. Values: t-shirt sizes (`XS`, `S`, `M`, `L`, `XL`) or time (`2h`, `1d`, `3d`). Add `actual: Option<String>` populated when the node reaches `implemented`. The delta between estimate and actual is useful for calibration.

**6. Due dates** (SMALL delta)
Add `due: Option<String>` (ISO date) to frontmatter. The lifecycle doctor can flag overdue nodes. Dashboard shows upcoming deadlines.

**7. Board view** (MEDIUM delta, TUI/web)
Group nodes by status in a kanban layout. The web dashboard already has a tree view — add a board tab. The TUI could show a compact status summary:
```
◌ Seed: 12  ◐ Exploring: 8  ● Decided: 3  ⚙ Implementing: 2  ✓ Done: 230
```

**8. Archived state** (SMALL delta)
255 nodes is already unwieldy. Nodes at `implemented` that are old enough should move to `.omegon/design/archive/`. The `list` action excludes archived by default, `list --include-archived` shows them.

Alternative: keep files in place, add `archived: true` to frontmatter. Cheaper, no file moves.

### Comparison with git-bug and git-issue



### git-bug (MichaelMure/git-bug)

**Storage**: Custom git objects in `refs/bugs/` namespace. Not files — git objects accessed via git-bug tooling.
**Pro**: Fully distributed, merge-friendly (CRDTs for comments), bridges to GitHub/GitLab/Jira.
**Con**: Invisible without git-bug binary. Can't browse issues in a text editor or on GitHub. Complex object model.

### git-issue (dspinellis/git-issue)

**Storage**: Files in `.issues/` directory. Each issue is a directory with `description`, `comments/`, `tags/`, `assignee` files.
**Pro**: Simple, file-based, browsable. Bidirectional GitHub/GitLab sync.
**Con**: One-directory-per-issue creates filesystem clutter. No structured metadata beyond simple key-value files.

### Omegon design tree (our approach)

**Storage**: Markdown files with YAML frontmatter in `docs/` (soon `.omegon/design/`). One file per node.
**Pro**: Human-readable, git-diffable, editable in any editor or on GitHub. Rich structured content (research, decisions, acceptance criteria). Agent-native (LLM tools for query/mutation). Already integrated with OpenSpec, cleave, milestones, dashboard.
**Con**: 255 files in a flat directory. No CRDT merge (relies on git merge). No external platform sync (no GitHub Issues bridge — yet).

### Key advantage: agent-native by design

Neither git-bug nor git-issue are designed for AI agents. They're CLI tools for humans. Our design tree IS the agent's working memory for design decisions — it's injected into the system prompt, the agent queries it for context, mutations happen via tool calls during design conversations.

This is not a human issue tracker with agent bolted on. It's an agent-native knowledge structure that happens to also be human-readable. The deltas to make it a "full task system" are about human workflow affordances (board views, filters, milestones) — the agent side is already complete.

### What we DON'T need to replicate

- **Comment threads**: Research sections and decisions already capture structured discourse. No need for an unstructured comment stream.
- **Notifications**: Single-operator system. The dashboard IS the notification surface.
- **Pull request integration**: OpenSpec + branch binding already tracks implementation. Commits reference node IDs in conventional commit scope.
- **Team assignment workflow**: For now, single operator + agents. Assignee field covers the multi-agent case.

### Storage migration: docs/ → .omegon/design/



### Why move

Currently design docs live in `docs/` alongside hand-authored documentation (if any). This conflates two concerns:
- Machine-managed design state (created/mutated by design_tree_update)
- Human-authored reference docs, guides, architecture overviews

Projects using Omegon that also have a `docs/` site (Astro, MkDocs, etc.) would get 255 design node files mixed into their documentation.

### Proposed layout

```
.omegon/
  milestones.json          # already exists
  profile.json             # already exists
  history                  # already exists
  design/
    perpetual-rolling-context.md    # active design nodes
    git-native-task-management.md
    ...
    archive/                        # completed nodes (optional)
      rust-phase-0.md
      ...
  sessions/                # session persistence (already used)
  memory/                  # facts.db, facts.jsonl (via ai/memory symlink)
  agents/                  # agent definitions
```

### Migration

1. Move `docs/*.md` design nodes → `.omegon/design/`
2. Update `LifecycleProvider` to scan `.omegon/design/` instead of `docs/`
3. Fallback: if `.omegon/design/` doesn't exist but `docs/*.md` has frontmatter with `status:`, scan there (backward compat)
4. One-time migration command: `/migrate-design` or auto-detect on startup

### Index file

Add `.omegon/design/index.json` — a lightweight index rebuilt on startup:
```json
{
  "nodes": {
    "perpetual-rolling-context": {
      "status": "exploring",
      "priority": 1,
      "type": "epic",
      "parent": "rust-agent-loop",
      "milestone": null,
      "assignee": null,
      "tags": ["architecture", "context"],
      "modified": "2026-03-27T12:00:00Z"
    }
  },
  "stats": {
    "total": 255,
    "by_status": { "seed": 12, "exploring": 18, "decided": 5, "implemented": 215 },
    "by_type": { "epic": 8, "feature": 45, "task": 30, "bug": 12, "chore": 5 }
  }
}
```

The index enables fast filtered queries without parsing 255 markdown files. Rebuilt on startup and after any mutation. Git-tracked (so other agents/sessions see the same index).

### Effort estimate for each delta

| Delta | Size | Files touched | Est. effort |
|-------|------|---------------|-------------|
| Filtered `list` queries | S | lifecycle.rs (add filter params) | 2h |
| Milestone binding | S | types.rs, design.rs, lifecycle.rs, milestones | 3h |
| Assignee field | XS | types.rs, design.rs, lifecycle.rs | 1h |
| Activity/history query | S | lifecycle.rs (shell to git log) | 2h |
| Effort estimation fields | XS | types.rs, design.rs | 1h |
| Due date field | XS | types.rs, design.rs, doctor.rs (overdue check) | 1h |
| Board view (TUI) | M | tui/dashboard.rs, tui/mod.rs | 4h |
| Board view (web) | M | web/api.rs, web dashboard JS | 4h |
| Archive state | S | types.rs, design.rs, lifecycle.rs | 2h |
| Storage migration docs/→.omegon/design/ | M | design.rs, setup.rs, migration script | 3h |
| Index file | S | design.rs (rebuild on startup + after mutation) | 2h |
| **Total** | | | **~25h** |

The filterable list + milestone binding + archive state are the highest-value, lowest-effort items — they solve the "255 nodes is unwieldy" problem immediately. The board views are the most visible improvement but can be deferred.

All 8 frontmatter additions (milestone, assignee, estimate, actual, due, archived) can be done in a single pass — they're just new `Option<String>` fields on DesignNode with corresponding frontmatter parsing.

## Decisions

### Decision: Design tree IS the task system — extend, don't replace or add a parallel tracker

**Status:** exploring
**Rationale:** The temptation is to build a separate "issues" subsystem alongside the design tree. This would be wrong — it creates two competing sources of truth for "what work exists."

The design tree already has richer semantics than any issue tracker: research, decisions, acceptance criteria, readiness scoring, lifecycle doctor. Adding issue-tracker fields (assignee, milestone, due date, estimate) to the EXISTING design node format gives us the best of both worlds: structured design exploration AND operational task management in one system.

The node's status lifecycle already covers the workflow: seed (backlog) → exploring (triage/analysis) → decided (ready) → implementing (in progress) → implemented (done). These are strictly richer than Open/In Progress/Done.

Adding 6 optional fields to DesignNode (milestone, assignee, estimate, actual, due, archived) and a filtered list query makes it a complete task system. No new data model, no new storage format, no new tools. Just wider frontmatter.

## Open Questions

- Should we add a GitHub Issues bridge (bidirectional sync like git-issue does), or is git-native-only sufficient? A bridge adds complexity but lets collaborators who prefer GitHub workflows participate. For a single-operator project, the bridge is unnecessary overhead.
- Should the index.json be git-tracked (shared across clones/agents) or gitignored (local cache, rebuilt on startup)? Tracked: consistent state across sessions, no rebuild cost. Gitignored: no merge conflicts from concurrent mutations, simpler.
- Do we want a `/board` TUI command that shows a kanban-style view, or is the dashboard sidebar tree sufficient? The sidebar groups by parent (hierarchy), not by status (workflow). Both views are useful for different questions — hierarchy answers 'what's the scope?', kanban answers 'what's in progress?'

## Implementation Notes

### File Scope

- `core/crates/omegon/src/lifecycle/types.rs` (modified) — Add 6 fields to DesignNode: milestone, assignee, estimate, actual, due, archived. All Option<String>.
- `core/crates/omegon/src/lifecycle/design.rs` (modified) — Parse new frontmatter fields. Add index rebuild. Add git-log-based history query.
- `core/crates/omegon/src/features/lifecycle.rs` (modified) — Add filter params to list action. Add set_milestone/set_assignee/set_estimate/set_due actions to design_tree_update. Add history action to design_tree.
- `core/crates/omegon/src/lifecycle/doctor.rs` (modified) — Add overdue-node finding. Add milestone-without-nodes finding.
- `.omegon/design/` (new) — NEW directory — migration target for docs/*.md design nodes.
- `.omegon/design/index.json` (new) — NEW — lightweight queryable index rebuilt on startup and after mutations.

### Constraints

- Design nodes must remain human-readable markdown with YAML frontmatter — no binary format, no SQLite for primary storage
- Existing docs/*.md nodes must continue to work during migration — scan both locations with .omegon/design/ preferred
- New frontmatter fields are all optional — existing nodes don't need updating to remain valid
- The index.json is a cache, not source of truth — if deleted, it's rebuilt from markdown on next startup
