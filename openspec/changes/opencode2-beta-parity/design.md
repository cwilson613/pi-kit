# OpenCode2 beta parity design

## Status and authority

Planned; no implementation tasks are complete. The
[comparison](../../../docs/opencode2-beta-parity.md) defines the reference snapshot
and evidence confidence. This design proposes Omegon behavior even where beta
behavior differs. A matching beta command is not an acceptance test.

## Instruction admission

Extend `prompt.rs` discovery to include every ancestor from the active worktree
root to cwd. Keep global directives separate and preserve Omegon's declared
precedence: immutable core, operator/project policy, then behavior defaults.
Nearest project guidance adds scope; it must not discard root guidance.
Use canonical worktree boundaries and deduplicate canonical paths.

Represent instruction sources as typed observations: available value, confirmed
absence, or temporarily unavailable. A source record contains its identity,
scope, content reference, and hash. Reject silent truncation. If a configured
instruction budget cannot hold required guidance, report the condition before
dispatch instead of silently dropping policy.

The session authority admits changed generations before the physical model
attempt. No-op reads append no event. Retry assembly consumes admitted content,
and recovery reconstructs the same content without rereading historical files.
Temporary failures preserve the previous admitted value; initial failure blocks
dispatch with a recoverable diagnostic. Confirmed deletion admits a removal.
Bodies remain in the privileged content store; public projections expose source
identity and generation, subject to existing redaction rules.

Add event types or explicitly version contracts; do not alter frozen payloads.
Legacy sessions establish an initial generation at the next safe model boundary.
Compaction binds its input and replacement to an admitted generation. It cannot
promote quoted history, tool output, or remote MCP material into policy authority.
Nested directives discovered by file tools require a separate scope check before
admission; they must not become unscoped session-wide authority.

## MCP phase budgets

Add optional startup, catalog, and execution budgets at the existing server
configuration owner. Retain `timeout_secs` as a fallback for each unset phase;
retain current defaults when no fields are supplied. Use explicit units in Rust
and Pkl and validate zero, overflow, and invalid inputs consistently.

Transport establishment and initialization share a startup deadline. Catalog
listing and pagination have a bounded catalog deadline. Tool execution, resource
reads, and prompt retrieval use the execution budget. Progress does not extend
the hard deadline. Cancellation can settle work earlier than any budget.

Local process cleanup remains tree-scoped. Remote transport cancellation records
what is known; it must not claim a remote process stopped without evidence.
Managed readiness and cleanup policies remain authoritative and can impose a
stricter outer bound. Errors identify the phase, configured budget, and settlement.

## Context retention

Extend `ContextCompactionSnapshotV1` through a compatible versioned boundary if
its contract requires it. Carry the admitted model limits and estimates for
instructions, tool schemas, requested output, and conversation content.
Select the newest complete conversation units that fit the retained-token budget.
Keep tool calls paired with their results, and preserve referenced attachment
identity. Do not reuse OpenCode's character heuristic as a universal tokenizer.

`context_compaction_service.rs` remains a planner. `session_compaction.rs` and
session authority retain commit, abandonment, context revision, and recovery
ownership. Failed summaries do not replace the active context. Existing
route-service attempt limits govern summary and overflow retries.

Verify manual requests during active work, cancellation, and a second overflow.
Do not impose the beta's manual-input priority or `auto` switch behavior until
the reference fixture and local queue contract resolve their interaction.

## Model presets

Extend offering evidence with model-specific named presets containing typed,
adapter-supported controls. A selection binds offering, preset, normalized
controls, and inventory generation to existing route provenance.
Unknown presets and unsupported controls fail before bridge replacement.
Provider/model capability admission still applies after preset resolution.

Expose available presets through semantic model projections and the command
registry consumed by TUI, CLI, and ACP. Preserve existing thinking settings as
explicit mappings where supported. Do not permit arbitrary request headers,
credential replacement, transport changes, or invented capabilities in a preset.

## Continuity campaign

Use `session_recovery_campaign.rs`, `surface_parity_campaign.rs`,
`control_runtime.rs`, and delegate fixtures before adding production code.
Test two clients against one existing server, disconnect during streaming,
reconnect with pending approval, duplicate input submission, completion while
detached, restart after ambiguous tool execution, and cancellation descendants.

Compare semantic state, not renderer text. Input identity and durable facts must
prevent duplicate admission; uncertain external side effects must remain
uncertain rather than being automatically rerun. Delegation preserves worktree
isolation and the current child authority ceiling. Permissions apply to every
nested invocation and every resource in a multi-file mutation.

Treat missing endpoints, unavailable beta commands, and fixture infrastructure
failures separately from behavioral differences. The reference executable uses
isolated configuration and temporary repositories; do not share personal
credentials or user services between fixtures.

## Validation and rollout

Keep production changes split by I, M, C, R and reproduced campaign findings.
Each starts with a focused failing scenario and ends with the narrowest relevant
crate gate plus `just clippy-changed`. Shared contracts or multiple crates use
`just test-commit`; broad recovery/security changes justify the broader ladder.
Add schema checks when Pkl changes. Run long Cargo gates to completion.

Exercise current source through `just run`. Use `just link` only if installed
launcher or bundled-asset identity matters, then verify `omegon --which`.
Record build identity, fixture identity, events, exit status, and cleanup result.
Update Workbench/OpenSpec status as implementation lands; archive only after
scenario reconciliation. This documentation-only proposal needs OpenSpec
validation and `git diff --check`, not a Rust build or installation.
