# OpenCode2 parity: immediate reliability fixes

## Intent

Fix two locally evidenced problems with the highest immediate return: incomplete
project instruction loading and one MCP timeout serving unrelated operation phases.
The [ranked comparison](../../../docs/opencode2-beta-parity.md#roi-ranked-backlog)
retains the wider parity assessment and deferred candidates.

## Scope

1. Load all applicable ancestor AGENTS.md files inside the active worktree,
   preserve root and nearest-scope guidance, and remove silent truncation.
2. Separate MCP startup, catalog, and execution deadlines with compatible
   configuration fallback and cancellation settlement.
3. Verify reconnect with pending approval, duplicate input admission, and detached
   delegate completion; fix reproduced event-loss defects in existing owners.

These are independently landable slices. Instruction discovery does not depend
on a new durable instruction event model. MCP deadlines do not depend on either
instruction work or a new transport runtime. A beta executable comparison is
useful reference evidence but is not a prerequisite for these local fixes.

Deferred: durable instruction refresh, token-budgeted compaction, model presets,
and broad lifecycle or tool-inventory campaigns beyond those three cases. Their entry criteria remain in
the ranked comparison; they are not requirements of this change.

Excluded: OpenCode API/config compatibility, a new plugin runtime or renderer,
automatic shared-daemon startup, cache warming, and changes to route authority.

## Success criteria

- Root, intermediate, and cwd directives load once in documented order.
- Worktree boundaries and global/project ownership remain correct.
- Required guidance is not silently truncated or silently omitted on read failure.
- MCP phases honor independent budgets while legacy configuration retains its behavior.
- Cancellation and timeout errors identify the phase and report cleanup truthfully.
- Reconnect tests distinguish durable idempotency from client retry support and record protocol limitations explicitly.
- Each slice passes its focused scenarios, applicable landing checks, and runtime exercise.
