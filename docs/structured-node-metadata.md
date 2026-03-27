---
id: structured-node-metadata
title: "Structured node metadata — key/value planning attributes for milestone-aware design tree views"
status: exploring
tags: [design-tree, metadata, dashboard, planning, release]
open_questions:
  - "What should the frontmatter key be for structured metadata — `attrs`, `meta`, or another name that is terse but not ambiguous?"
  - "Should `milestone` be a single string in v1, or should the metadata system support list values from the start for cases where a node spans multiple horizons?"
  - "How should the dashboard treat nodes with no structured metadata during rollout — group under `unassigned`, hide from milestone grouping by default, or fall back to current sorting?"
dependencies: []
related:
  - release-0-15-4-trust-hardening
  - runtime-session-integrity
  - provider-routing-integrity
---

# Structured node metadata — key/value planning attributes for milestone-aware design tree views

## Overview

Add a structured key/value metadata surface to design-tree nodes so release planning and dashboard views can group/filter by delivery horizon and other planning dimensions without proliferating bespoke top-level fields. The initial motivation is milestone-aware tree rendering, but the design should support a small controlled vocabulary such as milestone, theme, blocker, and area/owner while preserving the existing flat tags list for broad categorical labels. This should improve dashboard legibility by separating lifecycle state, priority, and planning metadata instead of overloading status colors and ad hoc conventions.

## Decisions

### Decision: structured planning metadata lives in a dedicated key/value map, not in the flat tags list

**Status:** decided

**Rationale:** The existing flat `tags: []` field is useful for broad categorical labeling (`providers`, `routing`, `release`, `rust`), but it is the wrong shape for milestone-aware grouping and other planning metadata. A dedicated structured map such as `attrs:` or `meta:` should hold queryable key/value planning attributes while `tags: []` remains available for loose categorical indexing. This avoids stuffing values like `milestone:0.15.4` or `blocker:true` into an unstructured tag namespace.

### Decision: start with a small reserved planning vocabulary — milestone, theme, blocker, and area

**Status:** decided

**Rationale:** The metadata system should not become an uncontrolled dumping ground. The first release should support only a compact set of reserved keys that directly improve planning and dashboard views: `milestone` (delivery horizon), `theme` (cross-cutting initiative such as trust-hardening), `blocker` (whether the node is release-blocking for the targeted horizon), and `area` (owning surface such as providers, runtime, release, tui). Additional keys can be added later once the operator has real usage pressure.

## Open Questions

- What should the frontmatter key be for structured metadata — `attrs`, `meta`, or another name that is terse but not ambiguous?
- Should `milestone` be a single string in v1, or should the metadata system support list values from the start for cases where a node spans multiple horizons?
- How should the dashboard treat nodes with no structured metadata during rollout — group under `unassigned`, hide from milestone grouping by default, or fall back to current sorting?
