+++
id = "49b02718-69e1-4753-ae36-9d99c8030146"
kind = "document"
title = "Coding harness architecture parity"
status = "active"
tags = ["architecture", "harness", "comparison", "parity"]
aliases = ["harness-architecture-parity"]
imported_reference = false

[publication]
enabled = false
visibility = "private"
+++

# Coding harness architecture parity

## Purpose

This documentation set compares four coding-agent harnesses without assuming
that feature count is the same thing as architectural quality:

- [OpenCode](opencode.md), the `anomalyco/opencode` coding agent;
- [Omegon](omegon.md), the Rust-native harness in this repository;
- [Pi](pi.md), the `earendil-works/pi` coding agent, historically
  `badlogic/pi-mono`;
- [DeepSeek Harness](deepseek-harness.md), the first-party
  `deepseek-ai/deepseek-harness` product and `dsh` command.

The immediate motivation is operational: using Omegon to develop Omegon couples
the development environment to the system under repair. When the harness is
unstable, its unique capabilities may be unavailable precisely when they are
needed to diagnose or restore it. A useful comparison therefore needs to expose
not only feature parity, but also dependency shape, recovery paths, state
authority, extension boundaries, and whether another harness can serve as a
credible maintenance fallback.

Start with:

- [Architecture matrix](matrix.md) for the comparable inventory;
- [Philosophies and tradeoffs](philosophies.md) for the unlike design centers;
- the individual profiles for evidence, qualifications, and limitations.

## Method

This is an architecture inventory, not a benchmark or product ranking.

1. Pin every external harness to a named repository and dated revision.
2. Prefer implementation and subsystem documentation over marketing summaries.
3. Use current source as the authority for Omegon; older design notes are not
   implementation evidence.
4. Separate built-in behavior from optional extensions and third-party packages.
5. Separate a model provider from the harness that invokes it. In particular,
   DeepSeek models running inside Pi or OpenCode are not DeepSeek Harness.
6. Record omissions by design rather than automatically treating them as
   defects.
7. Treat security, recovery, and non-interactive execution as first-order
   architecture, not footnotes to the tool list.

## Status vocabulary

| Status | Meaning |
|---|---|
| **Built-in** | Shipped and composed in the standard product or profile being described. |
| **Optional** | Shipped, but requires configuration, a non-default profile, or an official extension surface. |
| **Partial** | Implemented with material surface, authority, or lifecycle gaps. |
| **Mixed** | The row combines capabilities with different statuses; the cell names each material distinction. |
| **External** | Achievable through third-party code or generic shell composition, but not owned by the harness. |
| **Absent** | Not present in the pinned baseline; no claim is made that the omission is deliberate. |
| **Absent by design** | The project explicitly leaves the concern to another layer. |
| **Planned** | Described as intended work without a current execution path. |
| **Unknown** | Primary evidence was insufficient; no inference is substituted. |

"Built-in" does not mean safe by default, available on every platform, or
equivalent across all frontends. Profile notes retain those distinctions.

Profiles own factual evidence, the matrix owns compact classification, and
`philosophies.md` owns cross-harness synthesis. A changed claim should be
corrected in its profile first and then propagated into the matrix or synthesis.

## Evidence baseline

| Harness | Evidence baseline | Maturity at baseline |
|---|---|---|
| OpenCode | `anomalyco/opencode` `65c35977`, 2026-08-17; latest observed release `v1.18.18` | Released, fast-moving `dev` documentation |
| Omegon | this repository at `a443b9b6`, 2026-08-17 | `0.29.0-dev` |
| Pi | `earendil-works/pi` `209bc7b9`, 2026-08-17; npm `0.84.2` | Released, actively evolving |
| DeepSeek Harness | `deepseek-ai/deepseek-harness` `99f6f02f`, tag `dsh-v0.1.0-rc.7`, 2026-08-17 | Developer preview / release candidate |

The dates matter. Provider inventories, experimental surfaces, defaults, and
session formats change quickly. Update the pinned baseline and profile evidence
before using this set for a later architectural decision.

## Deliberate exclusions

This first pass does not compare Claude Code, Codex CLI, Cursor, Goose, Aider,
or benchmark scores. It also does not compare model quality. Harness behavior
and model behavior need separate experimental controls; conflating them makes
both conclusions unreliable.

## Result at this baseline

For an independent Omegon maintenance lane, Pi has the lowest default coupling
and the smallest mandatory policy surface, but requires external containment
when that is part of the threat model. OpenCode offers stronger resident-session
and multi-client recovery behavior at the cost of a broader server runtime.
DeepSeek Harness is architecturally valuable for composition research, but its
developer-preview RC status makes it unsuitable as the sole recovery dependency.
See [Philosophies and tradeoffs](philosophies.md#maintenance-lane-implications)
for the operational reasoning.

The resulting Omegon architecture direction is documented in
[Selective Omegon kernel decomposition](../selective-kernel-decomposition.md).
