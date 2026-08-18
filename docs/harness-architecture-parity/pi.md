+++
id = "786e9a53-680b-4f9f-b0ee-942bb9b38808"
kind = "document"
title = "Pi architecture profile"
status = "active"
tags = ["architecture", "harness", "pi"]
aliases = ["pi-architecture-profile"]
imported_reference = false

[publication]
enabled = false
visibility = "private"
+++

# Pi architecture profile

[Collection index](README.md) | [Matrix](matrix.md) | [Philosophies](philosophies.md)

## Identity and baseline

This profile covers the Pi coding agent at
[`earendil-works/pi`](https://github.com/earendil-works/pi), historically
`badlogic/pi-mono`. It does not describe Omegon's historical TypeScript
extension layer as current Omegon architecture.

- Source baseline: commit
  [`209bc7b9`](https://github.com/earendil-works/pi/commit/209bc7b9a89b01c8fd05861cf5bbdda3e300037a),
  2026-08-17.
- Package baseline:
  [`@earendil-works/pi-coding-agent@0.84.2`](https://www.npmjs.com/package/@earendil-works/pi-coding-agent/v/0.84.2).

## Architecture

Pi separates provider access (`pi-ai`), the stateful event-streaming loop
(`pi-agent-core`), terminal rendering (`pi-tui`), and the composed coding-agent
session/CLI. The coding agent describes itself as a minimal terminal coding
harness and makes extensions, skills, prompts, themes, and packages the primary
adaptation mechanism.

- [package architecture](https://github.com/earendil-works/pi/blob/209bc7b9a89b01c8fd05861cf5bbdda3e300037a/README.md#all-packages)
- [coding-agent overview](https://github.com/earendil-works/pi/blob/209bc7b9a89b01c8fd05861cf5bbdda3e300037a/packages/coding-agent/README.md)
- [agent-core loop](https://github.com/earendil-works/pi/blob/209bc7b9a89b01c8fd05861cf5bbdda3e300037a/packages/agent/src/agent-loop.ts)

The loop streams assistant messages, validates and executes tool calls, inserts
results into context, and continues until no tools, steering messages, or
follow-ups remain, or until cancellation/error/caller policy stops it. Tool
execution is parallel by default after sequential preflight; global and
per-tool controls can make a batch sequential.

## Capabilities

| Area | Baseline behavior |
|---|---|
| Providers | Broad `pi-ai` provider collection, subscription/OAuth and API-key auth, local/custom compatible endpoints. |
| Tools | Default model-visible tools are exactly `read`, `bash`, `edit`, and `write`; `grep`, `find`, and `ls` are optional built-ins. |
| Sessions | Append-only JSONL with stable entry/parent IDs, in-place branches, fork/clone/tree navigation, import/export, naming, and ephemeral mode. |
| Context | Automatic/manual compaction summarizes old context while original history remains in the session tree. |
| Steering | Separate steering queue after the current turn/tool calls and follow-up queue before otherwise stopping. |
| Extensions | In-process TypeScript modules can register or replace tools, providers, commands, shortcuts, UI, and lifecycle interception. |
| Skills | Agent Skills progressive disclosure; only names/descriptions enter initial context and bodies load on demand. |
| Interfaces | Interactive TUI, print/JSON output, JSONL RPC, and in-process TypeScript SDK. |

Primary evidence:

- [providers](https://github.com/earendil-works/pi/blob/209bc7b9a89b01c8fd05861cf5bbdda3e300037a/packages/ai/README.md#supported-providers)
- [SDK and tools](https://github.com/earendil-works/pi/blob/209bc7b9a89b01c8fd05861cf5bbdda3e300037a/packages/coding-agent/docs/sdk.md)
- [extensions](https://github.com/earendil-works/pi/blob/209bc7b9a89b01c8fd05861cf5bbdda3e300037a/packages/coding-agent/docs/extensions.md)
- [skills](https://github.com/earendil-works/pi/blob/209bc7b9a89b01c8fd05861cf5bbdda3e300037a/packages/coding-agent/docs/skills.md)
- [session format](https://github.com/earendil-works/pi/blob/209bc7b9a89b01c8fd05861cf5bbdda3e300037a/packages/coding-agent/docs/session-format.md)
- [RPC](https://github.com/earendil-works/pi/blob/209bc7b9a89b01c8fd05861cf5bbdda3e300037a/packages/coding-agent/docs/rpc.md)

## Omissions by design

Pi explicitly does not standardize these concerns in core:

- MCP;
- dedicated subagents;
- plan mode and a to-do manager;
- per-action permission popups;
- background-process management.

The shipped core tool inventory also has no built-in web fetch/search tool.

The recommended alternatives are ordinary CLI tools described by skills,
extensions/packages, separate Pi processes or tmux, visible repository files,
and external containers/VMs/sandboxes. These omissions reduce mandatory policy
but do not make the capabilities impossible.

## Security and trust

Pi runs with the launching process's permissions and has no built-in action
permission system. Tool allowlists control what the model sees, not what the Pi
process or arbitrary extensions can do. Project trust prevents automatic loading
of project-local settings/resources/extensions before approval, but does not add
per-command mediation afterward.

The official security model places Pi inside the user's trust boundary and
recommends containment through a container, VM, Gondolin extension, or policy
sandbox where needed.

- [security policy](https://github.com/earendil-works/pi/blob/209bc7b9a89b01c8fd05861cf5bbdda3e300037a/SECURITY.md)
- [containerization](https://github.com/earendil-works/pi/blob/209bc7b9a89b01c8fd05861cf5bbdda3e300037a/packages/coding-agent/docs/containerization.md)

## Philosophy

Pi's minimalism is primarily policy minimalism, not an absence of product
features. It has broad providers, rich sessions, compaction, project trust,
packages, multiple interfaces, and deep hooks. It declines to decide how every
operator should orchestrate agents, represent plans, approve commands, or
contain execution.

This makes Pi a useful low-coupling maintenance candidate: its default conceptual
surface is small and it can edit another harness without loading that harness's
runtime. The operator must still supply any required security boundary and avoid
reintroducing coupling through Omegon-specific extensions.

## Material limitations at the baseline

- Extensions and skill scripts are trusted arbitrary code.
- There is no consistent built-in permission, subagent, MCP, or plan contract
  across installations.
- Shell access means absence of a web tool is not network isolation.
- Security and process supervision quality depend on external composition.
- Multi-agent workflow provenance and merge semantics are extension-defined.
