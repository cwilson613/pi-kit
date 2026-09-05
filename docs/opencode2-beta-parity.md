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

Design date: 2026-09-04. Implementation has not started.

The proposed pass targets instruction correctness, bounded integration lifetimes,
and continuity across clients. Preserve the completed routing work and prove
existing session guarantees before adding new infrastructure.

[Proposal](../openspec/changes/opencode2-beta-parity/proposal.md) ·
[Design](../openspec/changes/opencode2-beta-parity/design.md) ·
[Tasks](../openspec/changes/opencode2-beta-parity/tasks.md)

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
either historical revision to V2. No last-reviewed marker advances. The first
implementation task freezes a binary fixture and confirms each selected behavior.

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

## Decision matrix

“Adopt” selects an outcome for this pass. “Investigate” requires a reproduction
before implementation. Priorities reflect local impact, not upstream release order.
Local paths below are relative to `core/crates/omegon/src/` unless stated otherwise.

| ID / priority | V2 behavior and evidence | Local finding | Decision and bounded outcome |
|---|---|---|---|
| I / P0 | Ambient instructions become durable source deltas; temporary read failure preserves prior values. [Instructions](https://opencode.ai/v2/docs/instructions/); source `packages/core/src/session/instruction-state.ts`. | `prompt.rs::load_project_directives` returns the first readable cwd/root file, skips intermediate ancestors, and truncates at 4000 bytes. `context.rs` replaces persistent injections in memory. | **Adopt:** complete scoped ancestor discovery and durable admitted instruction generations. Preserve Omegon precedence and privileged-source boundaries. |
| M / P0 | Separate startup, catalog, and execution deadlines. [MCP](https://opencode.ai/v2/docs/mcp-servers/); source `packages/core/src/mcp/client.ts`. | `plugins/mcp.rs::McpServerConfig` has one `timeout_secs`, used in readiness, inventory, and execution paths. | **Adopt:** independently configurable phase budgets with legacy fallback and cancellation evidence. Do not copy the beta's long default execution timeout. |
| C / P1 | Checkpoint plus token-budgeted recent context; manual compaction admission and bounded overflow recovery. [Compaction](https://opencode.ai/v2/docs/compaction/). | `session_compaction.rs` already records lifecycle and context revisions. `context_compaction_service.rs::plan_compaction` retains turns, with fixed manual/pressure fallbacks. | **Adopt:** budget-based retention. **Investigate:** manual barrier ordering and recovery after partial output; retain existing authority and recovery machinery. |
| R / P1 | Named model variants resolve against catalog entries and reject unknown names. [Models](https://opencode.ai/v2/docs/models/); source `packages/core/src/model-resolver.ts::withVariant`. | `provider_route_service.rs` admits reasoning capability; inspected catalog has no general named-variant inventory. | **Adopt:** typed, model-specific request presets and exact admission across operator surfaces. Preserve current thinking controls as compatibility mappings. |
| S / P1 | Default shared user service, private server option, explicit remote server connection, full TUI, run, and mini clients. [CLI](https://opencode.ai/v2/docs/cli/). | Existing daemon/control, durable session, and surface machinery. End-to-end detach/reconnect equivalence was not established. | **Investigate:** reconnect, duplicate submission, pending approval, and terminal loss through existing surfaces. **Defer:** changing default launch topology or adding another renderer. |
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

1. Freeze V2 executable identity and run reference fixtures. Record supported,
   different, broken-in-beta, and unavailable outcomes separately.
2. Land I and M as independent changes. Instruction discovery can land before
   durable refresh, provided the latter remains explicitly pending.
3. Land R against the existing routing baseline. Land C after instruction
   generation semantics are defined, so compaction preserves the admitted generation.
4. Run the S/D/P/L/T verification campaign. Open bounded fixes only for reproduced gaps.
5. Reconcile every row with tests, retained differences, or a named deferral.

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
