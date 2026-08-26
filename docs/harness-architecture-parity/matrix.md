+++
id = "ead3fe14-4c43-4af3-ab27-e663f68c4cf8"
kind = "document"
title = "Coding harness architecture matrix"
status = "active"
tags = ["architecture", "harness", "matrix", "parity"]
aliases = ["coding-harness-architecture-matrix"]
imported_reference = false

[publication]
enabled = false
visibility = "private"
+++

# Coding harness architecture matrix

[Collection index](README.md) | [Philosophies](philosophies.md)

## Reading the matrix

Cells summarize the dated profiles; they are not substitutes for profile
qualifications. "Yes" is avoided because superficially similar features often
have different authorities and failure semantics.

## Architectural center

| Dimension | [OpenCode](opencode.md) | [Omegon](omegon.md) | [Pi](pi.md) | [DeepSeek Harness](deepseek-harness.md) |
|---|---|---|---|---|
| Primary implementation | TypeScript/Bun monorepo | Rust workspace and integration binary | TypeScript monorepo | TypeScript/Node plugin graph; Python SDK runtime distribution |
| Design center | Server-owned sessions with interchangeable clients | Runtime-owned agency, typed effects, lifecycle and operator control | Small event-streaming loop with a large extension boundary | Cordis service/event graph where the loop, tools, and persistence are plugins |
| Composition unit | Server runtime, packages, agents, plugins | Cargo crates, `Feature`/`EventBus`, extensions, skills, runtime setup | `pi-ai`, `pi-agent-core`, coding-agent composition, extensions | Bundles, profiles, presets, scoped plugins |
| Main state authority | Server session/message database | LLM-facing session snapshots plus distinct interactive runtime, memory, lifecycle, and Git authorities | Append-only JSONL session tree | Immutable typed session event log |
| Default human surface | TUI | TUI | TUI | Local Web UI |
| Headless center | HTTP/OpenAPI/SSE server and `run` | Bounded `run`, daemon, IPC/WebSocket/ACP | Print/JSON, JSONL RPC, TypeScript SDK | One-shot headless profile and Python SDK |
| Architectural maturity signal | Broad released product; experimental edges | Broad product in active extraction/migration | Released, actively evolving minimal core and extension ecosystem | Sophisticated contracts, but developer-preview RC with breaking changes expected |

## Loop, context, and state

| Capability | [OpenCode](opencode.md) | [Omegon](omegon.md) | [Pi](pi.md) | [DeepSeek Harness](deepseek-harness.md) |
|---|---|---|---|---|
| Multi-step tool loop | **Built-in** persisted prompt/processor loop | **Built-in** Rust loop with retries, limits, recovery, and typed events | **Built-in** conventional event-streaming loop | **Built-in** plugin-provided step/turn loop |
| Parallel tool calls | **Built-in** stream processor support | **Partial**, serialized in normal composed runtimes; a latent path permits up to four calls from three permissionless tool families when no secrets manager is present | **Built-in**, parallel by default with sequential controls | **Mixed**, tool metadata and profile wrappers control concurrency |
| Mid-run operator steering | Session prompts and abort APIs | **Partial**, authoritative queue/cancellation/supervisor state in interactive mode; daemon cancellation is incomplete | **Built-in** steering and follow-up queues | **Built-in** inbox and turn lifecycle |
| Automatic compaction | **Built-in**, optional pruning | **Built-in**, but policy is split between loop and predictive feature | **Built-in**, original JSONL history retained | **Optional** plugin outside loop spine; original events retained |
| Conversation branching/fork | **Built-in** session fork/revert surfaces | Resume and durable sessions; no equivalent first-class conversation tree | **Built-in** parent-linked JSONL tree, `/tree`, `/fork`, `/clone` | **Built-in** stable between-turn fork prefixes |
| Durable semantic memory beyond transcript | **Unknown**, no comparable built-in fact graph identified | **Built-in** facts, decay, edges, episodes, optional vectors, vault sync | **External**, via extension | **Unknown**, no comparable built-in fact graph identified |
| Durable plan/work state | Todos and agent modes; session-owned | **Built-in**, session-local plans persist in conversation snapshots; lifecycle plans derive from Git/OpenSpec/design artifacts; Workbench projects both | **Absent by design** in core | **Optional**, logged plan-mode state; guidance rather than enforcement |
| Model-visible request replay evidence | Persistent message parts and snapshots | Canonical/LLM-facing split with events and audit surfaces | Rebuilt from selected session branch | **Built-in**, with the invariant that model-visible input must be reconstructable from log |

## Providers and model routing

| Capability | [OpenCode](opencode.md) | [Omegon](omegon.md) | [Pi](pi.md) | [DeepSeek Harness](deepseek-harness.md) |
|---|---|---|---|---|
| Provider breadth | **Built-in**, Vercel AI SDK/Models.dev, 75+ advertised | **Built-in**, native Rust bridges plus compatible routes | **Built-in**, broad `pi-ai` provider collection | **Built-in**, native DeepSeek plus `pi-ai` adapter and custom routes |
| Local/OpenAI-compatible models | **Built-in** | **Built-in**, including Ollama and experimental DwarfStar | **Built-in** | **Built-in** custom compatible endpoints |
| Route identity | Provider/model config and per-agent model | Explicit route controller plus semantic/capability/inventory layers | Unified model/provider abstraction | Adapter-owned route recorded in session log |
| Automatic fallback | Provider/config dependent | Narrow same-family fallback; no arbitrary family substitution | Caller/extension policy rather than a canonical fallback engine | Composition/provider policy; no universal fallback inferred |
| Per-agent model selection | **Built-in** | **Built-in** child runtime profiles and route policy | SDK/extension composition | **Built-in** presets and subagent providers |
| Provider schema normalization | AI SDK plus provider transforms | Explicit Full/OpenAI/Gemini dialect normalization | `pi-ai` normalized messages and tool contracts | Adapter and canonical tool schema layers |

## Tools, extensibility, and code intelligence

| Capability | [OpenCode](opencode.md) | [Omegon](omegon.md) | [Pi](pi.md) | [DeepSeek Harness](deepseek-harness.md) |
|---|---|---|---|---|
| Default coding tools | Broad built-in read/edit/write/shell/search/todo set; web-search availability is conditional | Broad feature-composed inventory with progressive disclosure | Four defaults: `read`, `bash`, `edit`, `write`; three optional file tools | Preset-specific: Standard is broad; Minimal has persistent Bash and editor only |
| Dynamic tool extensions | JS/TS plugins and custom tools | Native/OCI JSON-RPC extensions, OpenAPI tools, MCP | Powerful in-process TS extensions can replace tools | Cordis plugins; Creation mode can author temporary dynamic plugins |
| MCP client | **Built-in**, stdio and remote/OAuth | **Built-in**, stdio, HTTP, OCI, gateway, and bridge paths | **Absent by design** in core; extension/package possible | **Optional**, stdio or Streamable HTTP; tools only at baseline |
| Skills | **Built-in** Agent Skills discovery/on-demand loading | **Built-in** portable skills, provenance, activation, disclosure | **Built-in** Agent Skills progressive disclosure | **Built-in** provider registry and filesystem sources |
| LSP | **Optional**, experimental and disabled by default at baseline | **Absent**; tree-sitter/BM25 codescan instead | **External**, via extension | **Optional**, first-party LSP packages exist but are not composed into shipped presets |
| Web search/fetch | **Mixed**, built-in web fetch; web search requires OpenCode/OpenCode Go routes or `OPENCODE_ENABLE_EXA` | **Built-in** extracted web engine | **External**, via extension; no built-in web fetch/search tool | **Mixed**, Standard enables web search but disables fetch |
| Extension isolation | Plugins execute in harness process | Native/OCI subprocesses with cleared environment and secret bootstrap | Extensions execute arbitrary code in harness process | Cordis code is trusted; `node:vm` dynamic extensions are explicitly not a security boundary |
| Hot replacement/composition | Plugin hooks and tool replacement | Registration is dynamic, but first-registration-wins remains a migration concern | Deep extension interception/replacement | First-order reversible Cordis effects and scoped service replacement |

## Agency, security, and containment

| Capability | [OpenCode](opencode.md) | [Omegon](omegon.md) | [Pi](pi.md) | [DeepSeek Harness](deepseek-harness.md) |
|---|---|---|---|---|
| Action permission model | `allow` / `ask` / `deny`; mostly permissive defaults | **Partial**, four-layer evaluator scaffold, but current settings populate project policy only; no-match defaults allow | **Absent by design**; runs with process authority | Monotonic allow/deny/ask guards; approval fails closed |
| Filesystem boundary | External-directory guard and path permissions | Canonical workspace/trusted-path boundary | Process/container boundary only | `read-only`, `workspace-write`, `danger-full-access` sandbox modes |
| OS sandbox | Not a universal built-in boundary | **Optional** OCI session/child sandbox | **External**, deliberately delegated to containers/VMs with official guidance | **Mixed**, profile/backend-dependent filesystem confinement; enforcement can be partial and full-access bypasses it; network/process visibility are outside vocabulary |
| Project-local code trust | Rules/config and permission posture | Skill/extension provenance and workspace policy | Explicit project-trust prompt before local extensions/resources | Presets/plugins/MCP commands are trusted composition inputs |
| Explicit dangerous bypass | `--auto` bypasses asks, not denies | `--dangerously-bypass-permissions` disables filesystem-boundary prompting and some command confirmations, not policy/RBAC/sandbox enforcement | Ambient execution is the default | `danger-full-access`; Minimal SDK example uses it |
| Fail-closed characteristic | Explicit denies survive auto mode | Deny-overrides, secret/Vault guards; some unmapped cases allow | Relies on external containment | Missing approval support and unsupported capabilities fail closed |
| Process-tree cleanup | **Unknown**, not assessed here | Strong Unix process-group ownership; non-Unix weaker | Depends on tool/extension execution | Persistent Bash and sandbox backends; exact tree guarantees require profile-specific review |

## Agents, orchestration, and lifecycle

| Capability | [OpenCode](opencode.md) | [Omegon](omegon.md) | [Pi](pi.md) | [DeepSeek Harness](deepseek-harness.md) |
|---|---|---|---|---|
| Built-in subagents | **Built-in** primary/subagent sessions with depth policy | **Built-in** delegate and cleave children | **Absent by design**; process/extension/SDK composition | **Built-in** spawn and history-fork children in Standard; optional providers can target ACP, Codex, Claude, or SDK children |
| Parallel isolated implementation | Subagents; workspace support is experimental | **Built-in** cleave Git worktrees, dependency waves, merge and review | External tmux/process/package pattern | Provider-dependent child sessions; no canonical Git-worktree merge workflow evidenced |
| Child provenance/status | Session child relationships | Typed operation projections, route, termination, result acknowledgement | Extension-defined | Durable lineage, origin, delegation depth, continuation state |
| Lifecycle FSM | **Unknown**, no comparable repository-native design/OpenSpec FSM identified | **Built-in** design/change/milestone FSM and drift-aware artifact authority | **Absent by design** | **Absent** as a project-design lifecycle; plugin/preset/session lifecycles serve different concerns |
| Long-running autonomous work | Server sessions and clients | **Built-in** daemon, sentry, triggers, bounded `run`, turn/wall-clock limits; token budgets are post-run observations | SDK/RPC caller owns supervision | One-shot headless plus Web jobs/subagents; headless has no follow-up |
| Explicit completed/blocked outcome | Session finish/error state | Bounded run returns completed/error/exhausted/timeout; plans carry blocked state | Caller interprets events/results | Headless exits success only for `completed`; richer finish reason returned by SDK |

## Interfaces and recovery shape

| Capability | [OpenCode](opencode.md) | [Omegon](omegon.md) | [Pi](pi.md) | [DeepSeek Harness](deepseek-harness.md) |
|---|---|---|---|---|
| TUI | **Built-in** | **Built-in**, default | **Built-in**, default | **Unknown**, no first-party TUI evidenced; Web is primary |
| Browser/desktop | Web and beta desktop | Embedded control-plane/dashboard APIs; companion clients possible | External/SDK client | **Built-in** local Web UI |
| HTTP API | **Built-in** OpenAPI/SSE server | **Built-in** HTTP/WebSocket control plane | Not core; RPC/SDK are primary | Web Host API; headless opens no port |
| Native/stdio RPC | ACP and SDK/server protocols | ACP stdio/WebSocket and native MessagePack IPC | **Built-in** JSONL RPC | ACP automation adapter and Python JSON-RPC stdio SDK |
| Session resume | **Built-in** | **Partial**, interactive/standalone and ACP support resume; bounded run does not, and serve does not restore its default session at startup | **Built-in** | **Built-in** generally; ACP adapter is fresh-session only |
| Independent maintenance fallback | Server can be driven by multiple clients | Runtime and default TUI share the same large integration binary | Small standalone CLI can edit another harness with minimal coupling | `dsh` can maintain another checkout, but RC/runtime complexity is material |
| Recovery concern | Unauthenticated externally bound server if password omitted | Self-hosting couples repair to unstable runtime; duplicated authorities increase failure surface | Minimal core omits policy and containment that must be supplied externally | RC compatibility, no old-session migration, trusted plugin graph |

## What the matrix does not establish

- It does not establish which harness produces better code.
- It does not establish equivalent security merely because two cells say
  "permission" or "sandbox."
- It does not establish that an optional extension is as dependable as a core
  contract.
- It does not establish that Omegon should copy every competing feature.
- It does establish where a capability is absent, differently owned, or coupled
  to the same runtime that may need repair.
