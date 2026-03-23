# Interactive /tutorial system — structured onboarding replacing /demo — Design

## Architecture Decisions

### Decision: Individual markdown files per lesson (.omegon/tutorial/01-*.md) with YAML frontmatter

**Status:** decided
**Rationale:** Each lesson is self-contained. The agent sees one file at a time — structurally impossible to read ahead. Files are ordered by numeric prefix. Frontmatter carries title and optional validation criteria. Easy to add, remove, reorder lessons without touching code.

### Decision: Harness-controlled pacing via /next command — agent sees one lesson at a time

**Status:** decided
**Rationale:** The root cause of the demo's pause failure is that pacing was delegated to agent self-control. The fix: the harness injects one lesson as a queued prompt, the agent responds, the operator reads, the operator types /next, the harness injects the next lesson. The agent never sees the lesson list. Structural enforcement, not advisory instructions.

### Decision: Sandbox tutorial project (clone/create temp), not in-place

**Status:** decided
**Rationale:** New operators shouldn't risk their real project during onboarding. /tutorial clones a tutorial repo with pre-seeded content (like /demo does now), exec's omegon inside it. The tutorial repo has lesson files, seed data, and tone directives. Safe to experiment, break things, create/delete files.

### Decision: Progress persists in .omegon/tutorial/progress.json — /tutorial resumes, /tutorial reset starts over

**Status:** decided
**Rationale:** Operators may not finish the tutorial in one sitting. Progress is cheap to store (one JSON file with current_lesson and completed list). /tutorial without args resumes. /tutorial reset clears progress. /tutorial status shows where you are.

## Research Context

### Current /demo architecture and failure mode

The current /demo:
1. Clones styrene-lab/omegon-demo into /tmp
2. exec's omegon with --initial-prompt-file pointing at demo.md
3. demo.md contains Phase 1 only, tells agent to wait
4. AGENTS.md contains Phases 2-8, agent reads it when told "next"

The failure mode: the agent treats "read AGENTS.md and do the next phase" as "read AGENTS.md and do everything." The STOP instruction is advisory. The agent's context window sees all 8 phases and its completion bias takes over.

Root cause: the harness has no concept of "lesson" or "step." The initial-prompt is just a string. The agent decides when to stop. There's no structural gate between phases — just natural language instructions saying "please stop."

The fix must be structural: the harness feeds one lesson at a time, and the agent literally cannot see the next lesson until the operator advances.

### Architecture proposal: Tutorial as a Feature with harness-controlled pacing


