+++
id = "f11a13b8-8be7-46fa-87e3-ec29c7803d19"
kind = "document"
title = "Coding harness philosophies and tradeoffs"
status = "active"
tags = ["architecture", "harness", "philosophy", "tradeoffs"]
aliases = ["coding-harness-philosophies"]
imported_reference = false

[publication]
enabled = false
visibility = "private"
+++

# Coding harness philosophies and tradeoffs

[Collection index](README.md) | [Matrix](matrix.md)

## Four different centers of gravity

### OpenCode: sessions as a service

OpenCode treats the local server and its sessions as the stable center. The TUI,
Web UI, SDK, ACP adapter, and editor integrations are clients over the same
runtime. This favors interface plurality, remote attachment, and automation.
Its plugin and provider surfaces are broad, but its default action policy is
comparatively permissive and several advertised capabilities remain
experimental or disabled by default.

The important philosophy is not merely "many providers." It is that a coding
session is a service that can outlive or be viewed through a particular client.

### Omegon: agency as an operational system

Omegon treats a coding session as more than a transcript. A live task can span
provider routes, durable semantic memory, Workbench plans, design/OpenSpec
lifecycle state, delegated agents, Git worktrees, daemon triggers, and multiple
operator surfaces. It therefore invests heavily in runtime authority,
provenance, typed status, process ownership, lifecycle reconciliation, and
semantic projections.

The benefit is richer operational control. The cost is authority density: more
state systems can disagree, and a large integration binary means the default
interface, provider loop, orchestration, lifecycle, and recovery machinery can
fail together. Current source shows several migrations where new and old
authorities coexist rather than one having cleanly replaced the other.

### Pi: minimal policy, maximal adaptation

Pi standardizes a small provider-neutral loop, four default coding tools,
sessions, compaction, terminal UX, RPC, and an unusually powerful extension
surface. It explicitly declines to standardize MCP, subagents, plan mode,
to-dos, permission popups, background process management, or security
containment in core.

This is not a lack of capability so much as a refusal to make one workflow
mandatory. The operator owns composition. The corresponding cost is that
extensions run with ambient process authority and important safety or
orchestration guarantees are only as consistent as the chosen environment.

### DeepSeek Harness: composition as the product

DeepSeek Harness goes further than conventional plugins: its model adapter,
tool registry, session log, and default agent loop are all Cordis plugins.
Profiles compose process-level bundles; presets compose per-agent behavior;
scoped registrations and reversible effects support replacement at runtime.
Its immutable event log and "model-visible means logged" invariant make replay
and provenance central architectural laws.

The tradeoff is complexity and maturity. At the pinned release it is a
developer-preview RC with expected compatibility breaks and no migration path
for older session formats. Dynamic extensions, MCP commands, and custom presets
remain trusted-code boundaries despite the explicit approval and filesystem
sandbox seams.

## Unlike meanings of minimalism

| Harness | What it tries to minimize | What it accepts instead |
|---|---|---|
| OpenCode | Client-specific runtime duplication | A resident local server and broad package surface |
| Omegon | Lost intent, unowned effects, lifecycle drift, and invisible operational state | More authorities, contracts, and integration complexity |
| Pi | Mandatory workflow policy and core feature accumulation | Operator-owned composition and external containment |
| DeepSeek Harness | Privileged, non-replaceable core components | A pervasive plugin graph and versioned composition complexity |

## Parity is not sameness

Several rows in the matrix look comparable but express different philosophies:

- OpenCode subagents are child sessions selected by agent policy.
- Omegon delegation also carries route provenance, operation projections, and,
  in cleave, worktree/merge ownership.
- Pi deliberately leaves subagents to processes or extensions.
- DeepSeek Harness defines a provider seam that can target in-process children
  or entirely different harnesses.

Likewise, "permissions" can mean input-pattern prompts, layered policy, no
in-process mediation, or monotonic guards plus a filesystem sandbox. Copying a
checkbox without copying its authority and failure semantics creates false
parity.

## The self-hosting problem

Using a harness to build itself provides excellent dogfooding evidence, but it
creates three coupled risks.

### 1. Execution coupling

A defect in the provider loop, tool dispatcher, TUI, session loader, permission
path, or process supervisor can prevent the agent from applying the repair.
Omegon's broad integration makes this coupling stronger than in a deliberately
small harness.

### 2. Observation coupling

The failing harness also chooses which events, logs, plans, and tool results the
operator sees. A presentation or state-reconciliation bug can make healthy work
look stuck, or stale work look authoritative.

### 3. Policy coupling

The harness under development supplies its own behavioral prompts, planning
pressure, continuation policy, and lifecycle instructions. A policy regression
can direct work toward the wrong task or prevent a clean handoff even when the
underlying tools still function.

## Maintenance-lane implications

The matrix supports a separate maintenance lane rather than a declaration that
one harness must replace another.

1. Keep a second harness install whose binary, configuration, session store,
   and update path do not depend on the Omegon checkout.
2. Give that fallback the smallest sufficient tool set: read, search, exact
   edit/patch, shell, Git inspection, and test execution.
3. Do not load mutable Omegon extensions, MCP servers, skills, or generated
   configuration into the fallback by default; that would recreate the same
   dependency graph.
4. Keep recovery instructions repository-native and human-readable so no
   harness database is required to discover them.
5. Use the fallback for repair and independent verification, while continuing
   to use Omegon dogfooding for ordinary development evidence.
6. Verify critical fixes from outside the repaired runtime before trusting its
   own success projection.

Pi is the smallest conceptual fallback in this set, but requires external
containment if that is part of the threat model. OpenCode offers stronger
multi-client/session service behavior but introduces a resident server and
broader runtime. DeepSeek Harness offers the most explicit compositional
experimentation, but its RC status makes it a poor sole recovery dependency.
The right operational choice depends on whether minimal coupling, client
plurality, or architecture experimentation is the primary goal.

## Design lessons for Omegon

These are comparison-derived questions, not adopted decisions:

- Can the repair-critical loop and tools become a smaller independently
  runnable kernel than the default integration binary?
- Can the TUI fail without taking prompt admission, cancellation, logs, or
  session persistence with it?
- Can every stateful subsystem name one authority and one reconstruction path?
- Can optional lifecycle and orchestration systems be removed from a maintenance
  profile rather than merely hidden from the model?
- Can recovery evidence be consumed by another harness without loading Omegon
  runtime state?
- Can dogfooding remain mandatory evidence without making self-hosting the only
  path to repair?

The adopted follow-up architecture is
[Selective Omegon kernel decomposition](../selective-kernel-decomposition.md).
