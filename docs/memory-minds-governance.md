---
id: memory-minds-governance
title: "Governed Local Minds and Future Remote Corpora"
status: seed
tags: [memory, minds, local-first, federation]
open_questions:
  - "[assumption] Existing v6 records can be conservatively assigned to a quarantined legacy auxiliary while a new empty Primensus is created, without breaking callers that currently use the legacy mind name."
  - "[assumption] Mind identity can become immutable without requiring immediate cross-database global uniqueness; globally stable IDs can be introduced before the first remote mount."
  - "What evidence thresholds should admit legacy facts from a quarantined auxiliary into Primensus?"
dependencies: []
related: []
---

# Governed Local Minds and Future Remote Corpora

## Overview

Keep v1 local-first while establishing explicit mind identity, one Primensus primary role, governed auxiliary lifecycles, and storage-neutral contracts that can later support immutable remote corpora, local overlays, and candidate promotion without making a vector index authoritative.

## Decisions

### Exactly one Primensus per memory database

**Status:** accepted

**Rationale:** Primensus is a role backed by immutable identity, not a string convention. It is the sole implicit ambient and default write domain; auxiliary minds require explicit selection.

### Local SQLite remains authoritative in v1

**Status:** accepted

**Rationale:** Current scale and workflow do not justify distributed operations. Canonical records remain local; indexes are rebuildable projections.

### Auxiliary minds require governance boundaries

**Status:** accepted

**Rationale:** Sessions, topics, tasks, and imports do not automatically become durable minds. Durable auxiliaries require distinct ownership, trust, access, retention, synchronization, or lifecycle boundaries.

### Future remote corpora use immutable base plus local overlay

**Status:** accepted

**Rationale:** Remote canonical snapshots are read-only to ordinary clients. Subsequent observations enter a local writable overlay and reach the remote authority only through candidate admission.

### Vector stores are rebuildable projections

**Status:** accepted

**Rationale:** A vector database may later provide HA retrieval, but it does not own canonical facts, provenance, lifecycle, episodes, or authority.

### Storage-neutral repository boundary precedes distribution

**Status:** accepted

**Rationale:** Logical identity, manifests, selectors, snapshots, queries, and candidate submissions must not encode SQLite or a specific remote vector engine.

### Optimize Minds for harness cognition, not generic storage infrastructure

**Status:** accepted

**Rationale:** The system exists to improve Omegon's long-term recall, continuity, and reasoning. Persistence, migration, and backend abstraction are supporting constraints only; avoid enterprise multi-tenant, distributed-consensus, and backend-pluggability work unless an observed harness need requires it.

### Use a minimal cognitive model: Primensus plus scratch and legacy auxiliaries

**Status:** accepted

**Rationale:** Ship one authoritative ambient memory, temporary task/session scratch memories, and a quarantined legacy corpus. Defer general-purpose mind catalogs, remote mounts, distributed writers, and elaborate admission workflows.

## Open Questions

- [assumption] Existing v6 records can be conservatively assigned to a quarantined legacy auxiliary while a new empty Primensus is created, without breaking callers that currently use the legacy mind name.
- [assumption] Mind identity can become immutable without requiring immediate cross-database global uniqueness; globally stable IDs can be introduced before the first remote mount.
- What evidence thresholds should admit legacy facts from a quarantined auxiliary into Primensus?
