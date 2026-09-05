+++
kind = "document"
title = "OpenCode2 beta follow-up parity pass"
status = "proposed"
tags = ["architecture", "parity", "opencode"]
imported_reference = false

[publication]
enabled = false
visibility = "private"
+++

# OpenCode2 beta follow-up parity pass

Design date: 2026-09-04. First three ranked tasks implemented and verified on 2026-09-05.

The completed pass covers complete instruction discovery, separate MCP phase
deadlines, and bounded reconnect and duplicate-action verification. The wider
comparison remains a ranked backlog. Durable instruction refresh, model presets,
and broader lifecycle campaigns remain deferred. Rank 4, token-budgeted retention,
was completed as a separate pass on 2026-09-05, including current authoritative
source alignment. Its [verification record](../openspec/archive/2026-09-05-token-budgeted-retention/verification.md)
documents the regressions, landing gates, and supported lineage boundaries.
Local tests establish these repairs; they do not establish full beta executable parity.

[Proposal](../openspec/archive/2026-09-05-opencode2-beta-parity/proposal.md) ·
[Design](../openspec/archive/2026-09-05-opencode2-beta-parity/design.md) ·
[Tasks](../openspec/archive/2026-09-05-opencode2-beta-parity/tasks.md) ·
[Verification](../openspec/archive/2026-09-05-opencode2-beta-parity/verification.md)

## References and confidence

OpenCode is a behavioral reference and implementation evidence. Omegon's session
authority, route leases, managed services, RBAC, and semantic surfaces remain
architectural authority. No upstream code is copied by this design.

| Reference | Identity | Review scope |
|---|---|---|
| Omegon | `ffddf4388de05282a2bc47c524a5422da2ee4595` | Local source and archived parity records; remote main matched before branching. |
| Previous architecture survey | OpenCode `65c35977bd564e23c0e9cf124b3e3e3b9308e9e8` | Historical [architecture profile](harness-architecture-parity/opencode.md), observed August 17. |
| Completed routing pass | OpenCode `c77100a40c16a1c7c39115023ccd6f284b476c77` | Reference recorded in [archived design](../openspec/archive/2026-08-28-opencode-routing-parity/design.md). |
| OpenCode2 source | [`41cb354c3eac138959b1a6c4690385b7c3a6d666`](https://github.com/anomalyco/opencode/tree/41cb354c3eac138959b1a6c4690385b7c3a6d666), observed `v2` head | Downloaded source; targeted instruction, MCP, and variant inspection. |
| Other upstream heads | `beta`: `baab05727d56678df1a34e263c8f757c55ffc01b`; `dev`: `5b1e31988ed74b821b3a7ca6647188446992aafc` | Identity observations only; not interchangeable with the inspected `v2` tree. |
| Published CLI | [`@opencode-ai/cli@0.0.0-beta-19129`](https://registry.npmjs.org/@opencode-ai/cli/0.0.0-beta-19129) | Registry metadata observed through the `beta` dist-tag; no binary execution. |
| V2 documentation | [Official beta documentation](https://opencode.ai/v2/docs/) | Live behavior descriptions observed September 4; may differ from both source and package. |

The package metadata did not establish its source commit. Binary-to-source
correspondence remains unverified. The downloaded npm metadata reported integrity
`sha512-FS7Nok193L1TAn1qMyJmQeeSgQFClMJbR8rBSxiKdk9NSsAHRSx6tmMG60fkITUv23FHC6bQ90JZrIkoLfeLuQ==`.
This identifies the wrapper package, not a platform executable.

This is a targeted behavior comparison, not a complete commit-range audit from
either historical revision to V2. No last-reviewed marker advances. A later
executable parity claim requires a pinned binary fixture. Locally reproduced
instruction and timeout fixes do not depend on completing that comparison.

## What the completed work already covers

The August 28 [routing tasks](../openspec/archive/2026-08-28-opencode-routing-parity/tasks.md)
are all checked, including the recorded landing gates. This review did not rerun
those gates. Their [baseline](../openspec/baseline/provider-routing/parity.md)
remains the regression contract:

- Exact model identity and offering admission fail closed.
- Provider preference ranks otherwise eligible routes.
- Credential metadata distinguishes API keys from OAuth.
- Central retry scheduling consumes bounded server delay guidance.
- Tool and reasoning requests require model-level capability evidence.
- Admitted manifest HTTP endpoints execute through supported adapters with provenance.

The architecture survey also covered sessions, tools, extensions, and clients.
Related design records include [delegation](subagent-architecture.md)
(`implemented`), [permissions](granular-permissions.md) (`exploring`), and
[LSP](lsp-integration.md) (`decided`). These records have different lifecycle
states and must not be grouped as completed parity work.

Current source confirms substantial foundations: `session_authority.rs` owns
prompt and compaction facts; `session_replay.rs` and `session_recovery_campaign.rs`
cover recovery; `features/delegate.rs` exposes async delegates and result retrieval;
`control_runtime.rs` routes shared commands. Adding equivalents would duplicate owners.

## ROI-ranked backlog

This order weighs user impact, confidence in the local gap, implementation scope,
and dependencies. Effort is a relative design estimate, not a delivery promise.
The order supersedes the initial broad proposal. Ranks 1 through 4 are completed in OpenSpec; later items need a separate scope.

| Rank | Task | Return and confidence | Relative effort | Disposition / entry criterion |
|---|---|---|---|---|
| 1 | Complete ancestor instruction discovery; remove silent truncation | Prevents lost project policy on ordinary nested-repository work. High confidence from inspected code. | Small–medium: prompt loader, callers, budget/error handling, fixtures. | **Completed.** Full ancestor discovery, worktree boundaries, explicit read failures, and pre-dispatch fixed-context admission. |
| 2 | Separate MCP startup, catalog, and execution deadlines | Lets slow tools complete without weakening discovery limits. High confidence in shared-timeout coupling. | Medium: configuration, lifecycle wiring, fake-server tests. | **Completed.** Independent phase budgets with legacy fallback, cancellation, pagination, and process cleanup fixtures. |
| 3 | Focused reconnect and duplicate-action verification | Protects active work and approvals; existing recovery owners reduce investigation cost. Snapshot handoff and approval replay defects were reproduced and repaired. | Small bounded investigation; repair effort unknown. | **Completed bounded pass.** Durable deduplication and detached results verified; snapshot handoff and web-owned approval replay repaired. Legacy retry identity remains unresolved. |
| 4 | Token-budgeted retained context | Addresses oversized recent turns during long sessions. Planner mismatch is confirmed; frequency and user impact need fixtures. | Medium–large: estimates, whole tool transactions, model limits, recovery. | **Completed.** Token-budgeted chronological suffixes preserve complete exchanges. Durable compaction aligns current sources; incompatible or legacy/mixed projections fail admission before mutation. |
| 5 | Durable instruction refresh and replay | Makes policy changes during long sessions explicit and reproducible. Valuable, but broader than loading files correctly. | Large: event contracts, privileged content, retries, replay, compaction, projections. | **Separate architectural follow-up.** Establish a stale-instruction/replay case and choose update, failure, and scope semantics first. |
| 6 | Remaining permission, cancellation, and inventory verification | High consequence if faulty, but current owners already implement guarantees. No defect established by this review. | Medium investigation; repair effort unknown. | **Target evidence gaps.** A concrete safety or data-loss defect overrides this provisional rank immediately. |
| 7 | Large MCP inventory / Code Mode comparison | May reduce schema cost and improve tool discovery; actual bottleneck unmeasured. | Medium investigation. | **Measure first.** Capture representative schema tokens and tool-discovery failures before proposing runtime changes. |
| 8 | Named model presets | Convenience beyond existing thinking controls is not demonstrated. | Medium–large across catalog, route provenance, and surfaces. | **Defer.** Require an operator workflow that existing controls cannot express cleanly. |
| 9 | Cache warming | Possible latency/cache benefit with recurring provider requests. Benefit unmeasured. | Medium plus ongoing request cost. | **Defer.** Require measured net benefit, explicit opt-in, and cost accounting. |
| — | Shared daemon by default, another renderer, OpenCode API/plugin compatibility | Product/topology changes rather than demonstrated local reliability fixes. | Large. | **Outside this pass.** Require independent product justification. |

## Evidence and disposition matrix

“Adopt” selects an outcome for the immediate pass. “Investigate” identifies a
follow-up evidence question, not a task required to close this change. The ROI
ranking above controls delivery order; P0/P1/P2 below retain impact groupings.
Local paths below are relative to `core/crates/omegon/src/` unless stated otherwise.

| ID / priority | V2 behavior and evidence | Local finding | Decision and bounded outcome |
|---|---|---|---|
| I / P0 | Ambient instructions become durable source deltas; temporary read failure preserves prior values. [Instructions](https://opencode.ai/v2/docs/instructions/); source `packages/core/src/session/instruction-state.ts`. | Previously selected the first readable cwd/root file and truncated at 4000 bytes; `prompt.rs::load_project_directives` now loads complete scoped ancestors. `context.rs` replaces persistent injections in memory. | **Adopt:** complete scoped ancestor discovery without silent truncation. **Defer:** durable refresh and admitted generations to rank 5. Preserve Omegon precedence and privileged-source boundaries. |
| M / P0 | Separate startup, catalog, and execution deadlines. [MCP](https://opencode.ai/v2/docs/mcp-servers/); source `packages/core/src/mcp/client.ts`. | `plugins/mcp.rs::McpServerConfig` now accepts three phase overrides; the original `timeout_secs` remains the fallback. | **Adopt:** independently configurable phase budgets with legacy fallback and cancellation evidence. Do not copy the beta's long default execution timeout. |
| C / P1 | Checkpoint plus token-budgeted recent context; manual compaction admission and bounded overflow recovery. [Compaction](https://opencode.ai/v2/docs/compaction/). | `session_compaction.rs` already records lifecycle and context revisions. `context_compaction_service.rs::plan_compaction` now budgets retained turns and complete exchanges; durable admission aligns current semantic sources. | **Completed in rank 4:** token retention and current authoritative source alignment. **Investigate:** manual barrier ordering and recovery after partial output; retain existing authority and recovery machinery. |
| R / P1 | Named model variants resolve against catalog entries and reject unknown names. [Models](https://opencode.ai/v2/docs/models/); source `packages/core/src/model-resolver.ts::withVariant`. | `provider_route_service.rs` admits reasoning capability; inspected catalog has no general named-variant inventory. | **Defer:** named presets until rank 8's operator use case is established. Preserve existing thinking controls and exact route admission. |
| S / P1 | Default shared user service, private server option, explicit remote server connection, full TUI, run, and mini clients. [CLI](https://opencode.ai/v2/docs/cli/). | Existing daemon/control, durable session, and surface machinery. Snapshot handoff and captured web approval replay are repaired. End-to-end detach/reconnect equivalence remains unclaimed. | **Investigate:** reconnect, duplicate submission, pending approval, and terminal loss through existing surfaces. **Defer:** changing default launch topology or adding another renderer. |
| D / P1 | Background child sessions and completion delivery. [Agents](https://opencode.ai/v2/docs/agents/). | Async delegates, notifications, progress, and result retrieval already exist in `features/delegate.rs`. | **Skip:** another delegation engine. **Investigate:** completion delivery across reconnect/restart and cancellation of descendants. Preserve write-worktree isolation. |
| P / P1 | Ordered rules, multi-resource checks, project approvals that cannot override configured deny, per-call Code Mode checks. [Permissions](https://opencode.ai/v2/docs/permissions/). | `permissions.rs`, `loop_permission.rs`, `tools/permissions.rs`, and `omegon-rbac` already own policy. | **Investigate:** parity fixtures for multi-file mutations, saved approval scope, remote clients, and nested dispatch. **Skip:** copying child-agent authority semantics or raw-command matching as an OS sandbox. |
| X / P2 | V2 breaks plugin and server APIs and moves terminal settings to global `cli.json`. [Migration](https://opencode.ai/v2/docs/migrate-v1/). | Omegon has its own extension RPC, command registry, configuration schemas, and ACP contracts. | **Skip:** wire/API/config compatibility and an embedded JS runtime. **Investigate:** feature-level client/server ownership only. |
| T / P2 | MCP tools can be grouped under Code Mode with per-server opt-out. [MCP](https://opencode.ai/v2/docs/mcp-servers/). | Omegon has a [Code Act skill](../skills/code-act/SKILL.md) and tool disclosure machinery; equivalent discovery and execution behavior was not verified. | **Investigate:** tool discoverability, schema cost, and nested permission enforcement with a large MCP inventory. Do not infer equivalence from the Code Act name. |
| L / P2 | Local model discovery refreshes capability metadata. [Models](https://opencode.ai/v2/docs/models/). | Inference inventory and manifest routes already exist. | **Investigate:** stale inventory and local capability refresh fixtures. **Skip:** guessed custom-model capabilities and automatic newest-model fallback. |
| W / P2 | Optional periodic idle model requests to maintain provider caches. [Warming](https://opencode.ai/v2/docs/warming/). | This review did not establish an equivalent idle-request feature or measured need. | **Defer:** require measured latency/cache benefit, request accounting, explicit opt-in, and route-bound cost limits first. |

Source links for the inspected implementation are pinned:
[instruction state](https://github.com/anomalyco/opencode/blob/41cb354c3eac138959b1a6c4690385b7c3a6d666/packages/core/src/session/instruction-state.ts),
[discovery](https://github.com/anomalyco/opencode/blob/41cb354c3eac138959b1a6c4690385b7c3a6d666/packages/core/src/instruction-discovery.ts),
[MCP client](https://github.com/anomalyco/opencode/blob/41cb354c3eac138959b1a6c4690385b7c3a6d666/packages/core/src/mcp/client.ts),
[variant resolver](https://github.com/anomalyco/opencode/blob/41cb354c3eac138959b1a6c4690385b7c3a6d666/packages/core/src/model-resolver.ts).

## Delivery order

1. Reproduce and fix instruction discovery through current prompt construction.
2. Reproduce and separate MCP deadlines without changing legacy defaults.
3. Validate and land each slice independently, then close the bounded OpenSpec pass.
4. Start a separate rank 3 investigation if pursued; promote later items only
   after their entry criteria are met. Deferred rows do not block this pass.

No blanket parity declaration follows from the existence of a command or module.
Each selected scenario needs an observable outcome on the current Omegon build.

## Open evidence questions

- Which source revision produced beta-19129's platform executable?
- Does the binary match documented instruction, timeout, and compaction behavior?
- The compaction documentation's overflow paragraph says recovery remains enabled
  when `auto=false`; its option table suggests otherwise. Use a fixture before
  adopting that switch relationship.
- Do reconnect paths recover pending inputs and approvals without duplicate actions?
- Which named presets add value beyond existing thinking controls for admitted models?
- Are MCP phase units and constraints represented in every applicable Pkl schema?

The upstream repository and wrapper metadata identify MIT licensing. This pass
uses behavior as reference; any later source reuse must verify notices at the
copied paths and retain provenance. No integration with OpenCode's beta APIs is required.
