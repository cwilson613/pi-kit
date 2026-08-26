+++
id = "b2cdbca2-ac99-411d-a7fc-8986c49feb36"
kind = "document"
title = "OpenCode architecture profile"
status = "active"
tags = ["architecture", "harness", "opencode"]
aliases = ["opencode-architecture-profile"]
imported_reference = false

[publication]
enabled = false
visibility = "private"
+++

# OpenCode architecture profile

[Collection index](README.md) | [Matrix](matrix.md) | [Philosophies](philosophies.md)

## Identity and baseline

This profile covers the open-source coding agent at
[`anomalyco/opencode`](https://github.com/anomalyco/opencode), not Omegon's
OpenCode Go model-provider route and not another similarly named project.

- Source baseline: `dev` commit
  [`65c35977`](https://github.com/anomalyco/opencode/commit/65c35977bd564e23c0e9cf124b3e3e3b9308e9e8),
  observed 2026-08-17.
- Latest observed release: [`v1.18.18`](https://github.com/anomalyco/opencode/releases/tag/v1.18.18),
  published 2026-08-13.
- Live documentation can describe `dev` behavior newer than the release.

## Architecture

OpenCode is explicitly client/server. Running the default command starts a
local server and TUI client. The server exposes OpenAPI and SSE, and the
generated SDK consumes the same API. The practical architectural center is
therefore the session runtime rather than the terminal renderer.

Evidence:

- [server architecture](https://opencode.ai/docs/server/#how-it-works)
- [server API](https://opencode.ai/docs/server/)
- [SDK](https://opencode.ai/docs/sdk/)
- [pinned package tree](https://github.com/anomalyco/opencode/tree/65c35977bd564e23c0e9cf124b3e3e3b9308e9e8/packages)

The prompt loop persists the user message, resolves the selected agent, model,
and tools, streams assistant output, records tool calls/results and usage, and
continues until completion, interruption, denial/error, or a step limit. The
processor snapshots file state around steps and can detect repeated identical
tool calls.

- [prompt loop](https://github.com/anomalyco/opencode/blob/65c35977bd564e23c0e9cf124b3e3e3b9308e9e8/packages/opencode/src/session/prompt.ts)
- [stream processor](https://github.com/anomalyco/opencode/blob/65c35977bd564e23c0e9cf124b3e3e3b9308e9e8/packages/opencode/src/session/processor.ts)

## Capabilities

| Area | Baseline behavior |
|---|---|
| Providers | Vercel AI SDK and Models.dev; 75+ providers advertised; local and OpenAI-compatible endpoints supported. |
| Tools | Shell, read, edit, write, patch, grep, glob, web fetch, skills, todos, questions, and experimental LSP query; web search is route/environment dependent. |
| Agents | Primary agents and configurable child-session subagents with prompts, models, permissions, and depth policy. |
| Sessions | SQLite persistence, child sessions, fork, revert, diff, share, import/export, todos, and permission responses. |
| Context | Automatic compaction by default, configurable recent tail, optional old-tool-output pruning. |
| Extensions | Local/npm JS/TS plugins, custom tools, Agent Skills, local and remote MCP with OAuth. |
| Interfaces | TUI, non-interactive run, HTTP/OpenAPI/SSE server, Web UI, SDK, beta desktop, editor terminal integration, ACP. |

Primary evidence:

- [providers](https://opencode.ai/docs/providers/)
- [tools](https://opencode.ai/docs/tools/)
- [agents](https://opencode.ai/docs/agents/)
- [permissions](https://opencode.ai/docs/permissions/)
- [plugins](https://opencode.ai/docs/plugins/)
- [MCP](https://opencode.ai/docs/mcp-servers/)
- [sessions API](https://opencode.ai/docs/server/#sessions)
- [compaction config](https://opencode.ai/docs/config/#compaction)
- [ACP](https://opencode.ai/docs/acp/)

## Security and trust

Permission outcomes are `allow`, `ask`, or `deny`, with wildcard and
input-pattern rules. Most operations default to allow. External-directory,
repeated-call, and `.env` read guards default to ask in the pinned
implementation. Live permission documentation instead describes `.env` reads
as denied, so installed-version behavior should be verified. `--auto` approves
asks but does not override explicit denies.

This is an in-process policy system, not a universal OS sandbox. Plugins can add
or replace tools, remote services receive selected data, and session sharing
uploads conversation history and metadata to a publicly accessible link.

The headless server also requires deliberate deployment hardening: Basic
authentication is enabled only when `OPENCODE_SERVER_PASSWORD` is set.

## Philosophy

Documented positioning emphasizes open source, provider choice, local
operation, multiple interfaces, and shareable sessions. The architecture is
best summarized as a provider-neutral local agent service with interchangeable
clients. "Local" should not be misread as "no data leaves the machine" when a
cloud provider, remote MCP server, remote instructions, sharing, or a plugin is
configured.

## Material limitations at the baseline

- LSP is disabled by default and the direct query tool is experimental.
- MCP inventories can consume substantial context.
- File snapshots can be expensive and Git is required for file undo/redo.
- ACP does not support every TUI operation, including documented undo/redo.
- Desktop, native LLM execution, background subagents, Scout, and workspaces
  include beta or experimental surfaces.
- Official Plan-mode descriptions differ on whether mutation is disabled or
  merely approval-gated; installed-version behavior should be verified.
- Live `dev` documentation and released behavior may differ.
