+++
id = "optional-domain-proof-matrix"
kind = "document"
title = "Optional domain proof matrix"
status = "decided"
tags = ["architecture", "kernel", "maintenance", "decomposition"]
aliases = []
imported_reference = false

[publication]
enabled = false
visibility = "private"

[data]
dependencies = ["selective-kernel-decomposition", "omegon-maintain"]
open_questions = []
related = ["binary-composition-and-kernel-admission"]
+++

# Optional domain proof matrix

## Authority

The executable matrix is
`openspec/archive/2026-08-26-selective-kernel-decomposition/fixtures/optional-domain-proof-v1.toml`.
Run `just check-optional-domain-isolation` to validate it. The checker requires
each row to name its composition class, architecture evidence, executable
absence and degradation tests, maintenance dependency exclusions, and public
documentation disposition. It also rejects optional implementation tokens in
the selected constitutional authority sources.

The matrix covers the optional domains extracted through Slices 6.1, 6.3, and
6.4. Slice 6.2 is not an optional domain. It is the owner-neutral projection
boundary used by runtime edges.

## Proof summary

| Domain | Composition class | Absence or degradation result | Public feature evidence |
|---|---|---|---|
| Plans/work | No-resource in-process service | Session plans remain usable; repository work is empty or source-locally degraded. | Not applicable: command syntax, durable contracts, and wire shapes did not change. |
| Behavior policy | No-resource in-process service | Ordinary turns and host recovery remain active; advisory policy output is omitted. | Not applicable: this is internal advisory loop policy with no public syntax or schema change. |
| Codescan | Release-coupled native extension | Search tools stay declared with typed unavailability when the extension is absent or fails. | Public code-search availability guidance. |
| Lifecycle/OpenSpec | Managed in-process service | Lifecycle tools report typed unavailability; unrelated EventBus work continues. | Public OpenSpec availability guidance. |
| Memory | Managed in-process service | Durable memory context is omitted; tools and status report typed unavailability; the session continues. | Public memory availability guidance. |
| Context/compaction | Managed in-process service | Ordinary context and turns continue; managed planning reports typed unavailability. | Public compaction availability guidance. |
| Git | Managed in-process service | Git-backed operations report typed unavailability; non-Git sessions and local-directory workspaces continue. | Public cleave/Git availability guidance. |
| Dynamic contributions | Out-of-process contribution adapters | Optional absence is local; rejected or degraded candidates cannot replace the accepted graph. | Public extension and plugin lifecycle guidance. |
| Shipped content | Boot-only content pack | The six constitutional host axioms remain; optional content and model-driven compaction disable locally. | Public install, skill, and plugin content-pack guidance. |

`omegon-maintain` remains independently buildable because its normal/build
dependency graph excludes every runtime package named by the matrix. The
constitutional proof is runtime and source-bound, not a claim that a separately
packaged minimal kernel artifact already exists. The selected authority sources
contain generic identity, graph, admission, invocation, generation cleanup, and
session truth only; per-domain tests prove that optional startup or generation
failure remains local.
