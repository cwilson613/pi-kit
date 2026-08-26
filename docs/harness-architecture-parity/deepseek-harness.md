+++
id = "7b065f77-8733-46c3-970c-83d40ba88ac6"
kind = "document"
title = "DeepSeek Harness architecture profile"
status = "active"
tags = ["architecture", "harness", "deepseek", "dsh"]
aliases = ["deepseek-harness-architecture-profile"]
imported_reference = false

[publication]
enabled = false
visibility = "private"
+++

# DeepSeek Harness architecture profile

[Collection index](README.md) | [Matrix](matrix.md) | [Philosophies](philosophies.md)

## Identity and baseline

This profile covers the first-party
[`deepseek-ai/deepseek-harness`](https://github.com/deepseek-ai/deepseek-harness)
product and `dsh` command. It does not refer to a DeepSeek model configured in
OpenCode, Pi, Claude Code, Codex, or DwarfStar.

- Source and release baseline:
  [`dsh-v0.1.0-rc.7`](https://github.com/deepseek-ai/deepseek-harness/tree/dsh-v0.1.0-rc.7),
  commit
  [`99f6f02f`](https://github.com/deepseek-ai/deepseek-harness/commit/99f6f02fecdb7dff40c3fbc9470f5907c29f74ca),
  2026-08-17.
- Maturity: developer preview and prerelease RC; compatibility-breaking changes
  are explicitly expected.

## Architecture

DeepSeek Harness's architectural claim is literal: the model adapter, tool
registry, session log, and default loop are plugins over a Cordis context.
Plugins contribute services, typed events, and effect-bound registrations whose
disposers support unload and replacement. Profiles compose process-level
bundles; presets compose per-agent tools and prompts.

- [architecture](https://github.com/deepseek-ai/deepseek-harness/blob/99f6f02fecdb7dff40c3fbc9470f5907c29f74ca/docs/architecture.md)
- [Cordis primer](https://github.com/deepseek-ai/deepseek-harness/blob/99f6f02fecdb7dff40c3fbc9470f5907c29f74ca/docs/cordis-primer.md)
- [preset runtime](https://github.com/deepseek-ai/deepseek-harness/blob/99f6f02fecdb7dff40c3fbc9470f5907c29f74ca/packages/preset/agent-presets/README.md)

A turn contains zero or more steps. Each step assembles scoped prompt sections
and tool schemas, logs the request envelope, streams one model request, executes
requested tools through policy waterfalls, and either continues or closes the
turn. The governing invariant is "model-visible means logged": request content
must be reconstructable from the session event log.

## Shipped presets

| Display name | Internal ID | Character |
|---|---|---|
| Standard | `standard` | Full coding agent with files, shell, search, skills, planning, goals, workflows, and subagents. |
| PTC | `code` | Standard capabilities exposed through a TypeScript Code Mode SDK and `run_code`. |
| Minimal | `minimal` | Fixed short prompt, persistent Bash and `str_replace_editor`, bare local filesystem, no compaction. |
| Creation | `cordis` | Standard abilities plus live Cordis inspection and temporary plugin experimentation. |

Preset evidence is under the pinned
[`agent-presets`](https://github.com/deepseek-ai/deepseek-harness/tree/99f6f02fecdb7dff40c3fbc9470f5907c29f74ca/apps/cli/config/agent-presets)
tree.

## Capabilities

| Area | Baseline behavior |
|---|---|
| Providers | Native DeepSeek route, generic `pi-ai` adapter, catalog/custom providers, compatible/self-hosted endpoints. |
| Tools | Scoped canonical registry with schema filtering, output rendering, timeout/concurrency metadata, and monotonic policy waterfalls. |
| Sessions | Immutable typed event log, JSONL/Zstandard or SQLite persistence, crash repair, resume and between-turn fork. |
| Context | Optional logged compaction checkpoints outside the loop spine; original events retained. |
| Planning | Optional durable plan-mode state, explicitly guidance rather than a security boundary. |
| Subagents | Standard composes spawn and history-fork children; the provider seam can additionally target ACP, Codex, Claude Code, or DSH SDK backends. |
| Extensions | Cordis bundles/plugins, skills providers, MCP tools, and Creation-mode dynamic packages. |
| LSP | First-party LSP service, stdio, and model-tool packages exist, but are not composed into the shipped presets. |
| Interfaces | Local Web UI, one-shot headless CLI, Python SDK over JSON-RPC stdio, and limited automation-oriented ACP. |

Primary subsystem evidence:

- [sessions](https://github.com/deepseek-ai/deepseek-harness/blob/99f6f02fecdb7dff40c3fbc9470f5907c29f74ca/docs/subsystems/session.md)
- [persistence](https://github.com/deepseek-ai/deepseek-harness/blob/99f6f02fecdb7dff40c3fbc9470f5907c29f74ca/docs/subsystems/persistence.md)
- [tools](https://github.com/deepseek-ai/deepseek-harness/blob/99f6f02fecdb7dff40c3fbc9470f5907c29f74ca/docs/subsystems/tools.md)
- [compaction](https://github.com/deepseek-ai/deepseek-harness/blob/99f6f02fecdb7dff40c3fbc9470f5907c29f74ca/docs/subsystems/compaction.md)
- [planning](https://github.com/deepseek-ai/deepseek-harness/blob/99f6f02fecdb7dff40c3fbc9470f5907c29f74ca/docs/subsystems/plan.md)
- [subagents](https://github.com/deepseek-ai/deepseek-harness/blob/99f6f02fecdb7dff40c3fbc9470f5907c29f74ca/docs/subsystems/subagent.md)
- [Python SDK](https://github.com/deepseek-ai/deepseek-harness/blob/99f6f02fecdb7dff40c3fbc9470f5907c29f74ca/docs/user/guide/python-sdk.md)
- [ACP](https://github.com/deepseek-ai/deepseek-harness/blob/99f6f02fecdb7dff40c3fbc9470f5907c29f74ca/packages/acp/acp/README.md)

## Security and trust

Tool policy supports allow, deny, and ask. Guards can reduce authority but cannot
force an allow. Missing approval support fails closed. Filesystem sandbox modes
are `read-only`, `workspace-write`, and `danger-full-access`; requested
confinement must not silently degrade to unconfined execution.

The sandbox vocabulary does not confine network access or process visibility.
Enforcement is backend-dependent: Windows ACL confinement is explicitly
partial, and older Landlock ABIs may also report partial enforcement.
`danger-full-access` bypasses the sandbox entirely.
MCP stdio commands, user presets, installed plugins, and dynamic packages are
trusted-code boundaries. DeepSeek explicitly states that Creation mode's
`node:vm` is not a security boundary and should be treated like Bash access.

- [approval](https://github.com/deepseek-ai/deepseek-harness/blob/99f6f02fecdb7dff40c3fbc9470f5907c29f74ca/docs/subsystems/approval.md)
- [sandbox](https://github.com/deepseek-ai/deepseek-harness/blob/99f6f02fecdb7dff40c3fbc9470f5907c29f74ca/docs/subsystems/sandbox.md)
- [dynamic runner warning](https://github.com/deepseek-ai/deepseek-harness/blob/99f6f02fecdb7dff40c3fbc9470f5907c29f74ca/packages/extensions/cordis-host-runner/README.md)

## Philosophy

DeepSeek Harness treats composition, replayability, and explicit failure as
first-order product behavior. Unsupported child capabilities fail loudly,
unknown required session events refuse to load, and model requests retain enough
evidence for reconstruction. Unlike Pi's smaller released core, it minimizes
privileged components by making nearly everything replaceable.

## Material limitations at the baseline

- Developer-preview RC status and expected compatibility breaks.
- Session format versions have no migration path.
- Minimal mode and the introductory Python example use broad local authority.
- Plan mode is guidance, not enforcement.
- MCP bridges tools only, not resources or prompts.
- ACP supports fresh sessions and committed output but omits resume/replay,
  plans, commands, reasoning, and rich live tool presentation.
- Headless accepts one submitted task and has no interactive follow-up.
- Compaction cannot repair one individually oversized retained unit.
- Dynamic browser packages can wait indefinitely for approval.
- Web serving is intentionally loopback-oriented rather than a documented
  first-party production deployment platform.
- Telemetry is off by default, but enabled full export has no shipped redaction
  rule for prompts, tool data, or workspace paths.
