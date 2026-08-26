+++
id = "dad32203-2408-4c83-967f-e1cbee44b623"
kind = "document"
title = "Memory schema authority - Rust owns schema v8"
status = "implemented"
tags = ["architecture", "memory", "schema", "rust", "typescript", "migration", "persona"]
aliases = ["memory-schema-authority"]
imported_reference = false

[publication]
enabled = false
visibility = "private"

[data]
issue_type = "task"
open_questions = []
priority = "1"
+++

# Memory schema authority - Rust owns schema v8

## Overview

The Rust `omegon-memory` crate is the authoritative memory schema. `types.rs`, `sqlite.rs`, and `schema-contract.json` define the persisted and wire contracts. Other implementations must adapt to this contract through explicit migrations.

## Current v8 contract

Schema v8 adds `memory_operation_receipts` and durable episode metadata fields. A receipt stores the operation ID, exact payload hash, compact effect JSON, and commit time. It does not duplicate fact content or vectors. The episode table now persists affected nodes, affected changes, changed files, tags, and tool-call count.

Migration accepts schemas v5-v7. The v5/v6 migration retains its quarantine rule and moves historical `default` records to `legacy`. The v7 migration treats `default` records as post-migration writes and moves them to `primensus`. Both paths add missing v8 columns and the receipt table in one exclusive transaction before schema v8 is published.

## Historical research

### Current schema alignment between Rust and TS

**Aligned (both sides have):**
- facts table: id, mind, content, section, status, confidence, reinforcement_count, decay_rate, decay_profile, last_reinforced, created_at, version, supersedes/superseded_by, source, content_hash, last_accessed, created_session, superseded_at, archived_at, jj_change_id
- episodes table: id, mind, date, title, narrative, created_at, affected_nodes, affected_changes, files_changed, tags, tool_calls_count
- edges table: id, source_id, target_id, relation, description, weight/confidence, created_at
- minds table: name, description, status, origin_type, created_at
- facts_vec table: fact_id, embedding, model_name, dims, created_at
- FTS5 index on facts(content)
- Schema versioning (TS: SCHEMA_VERSION=5, Rust: implicit via init_schema)

**TS has, Rust missing:**
- episodes.jj_change_id column (TS migration 5 adds it, Rust types.rs Episode struct doesn't have it)
- Explicit schema_version table with migration tracking (Rust uses idempotent CREATE IF NOT EXISTS)

**Rust has, TS missing:**
- Nothing critical — Rust was designed to mirror TS

**NEITHER side has (needed for persona system):**
- facts.persona_id — which persona was active when this fact was stored
- facts.layer — which memory layer this fact belongs to ('project', 'persona', 'working')
- facts.tags — searchable tags (persona mind stores use tags for domain classification)
- minds table persona fields — minds can represent persona mind stores, not just projects

### Schema requirements from the persona system (decided design)

The persona system (design node `persona-system`, decided) requires these schema additions:

**1. `facts.persona_id TEXT` (nullable, additive)**
When a persona is active and a fact is stored into the persona mind layer, this field records which persona owns it. NULL = project fact (default, backward-compatible). This is the key discriminator for the layered merge — on persona deactivation, facts with `persona_id = X` are removed from the active query set.

**2. `facts.layer TEXT NOT NULL DEFAULT 'project'` (additive)**
Memory layer classification: 'project' (default), 'persona', 'working'. Controls injection priority ordering and lifecycle (persona facts are portable across projects, working facts are session-scoped). The PluginRegistry already models this in Rust (MemoryLayers struct) — the DB column persists it.

**3. `facts.tags TEXT` (nullable, JSON array, additive)**
Persona mind seed facts carry tags for domain classification (e.g. ["pcb", "trace-width", "thermal"]). Tags enable filtered queries within a persona mind. Stored as JSON array in SQLite.

**4. `minds.origin_type` extended values**
Currently: 'active', 'archived'. Needs: 'persona' — indicates this mind record represents a persona's dedicated mind store, not a project. The field already exists as TEXT — the new value is purely semantic, no schema change needed.

**5. `episodes.jj_change_id TEXT` (nullable, additive)**
The TS side already adds this in migration 5. The Rust Episode struct in types.rs needs the field added to match.

**6. Schema migration table**
The Rust side should adopt explicit schema versioning (like TS's schema_version table) instead of relying solely on CREATE IF NOT EXISTS. This enables incremental migrations without full table recreation.

All additions are nullable with defaults — existing databases read cleanly without migration. New fields appear as NULL until populated. This is the "non-destructive adaptation" contract: the TS factstore can add these columns via its existing migration system, and old data keeps working.

## Decisions

### Decision: Schema v6: persona_id, layer, tags columns — Rust is the authority

**Status:** decided
**Rationale:** The Rust omegon-memory crate defines the canonical schema. Schema v6 adds: facts.persona_id (TEXT, nullable — which persona owns this fact), facts.layer (TEXT, default 'project' — memory layer classification), facts.tags (TEXT, JSON array — domain classification tags). Migration from v5 is additive (ALTER TABLE ADD COLUMN with defaults). Existing data reads cleanly. The TS factstore should add these same columns in its next migration, reading from the schema-contract.json file that Rust generates. Indexes: idx_facts_persona (WHERE persona_id IS NOT NULL), idx_facts_layer (WHERE status = 'active').

### Decision: Schema v8 adds operation replay and complete episode metadata

**Status:** implemented

**Rationale:** Managed memory needs crash-safe retry without duplicate reinforcement, replacement facts, edges, or episodes. Schema v8 records a payload-bound compact effect in the same SQLite transaction as the mutation. Entity-specific versions reject stale targeted changes without serializing independent facts through a global revision. Episode metadata is now persisted consistently by SQLite and the in-memory backend.

## Open Questions

*No open questions.*

## Implementation Notes

### File Scope

- `core/crates/omegon-memory/src/types.rs` defines canonical facts, episodes, mutation envelopes, preconditions, and compact effects.
- `core/crates/omegon-memory/src/sqlite.rs` defines schema v8, governed v5-v7 migration, atomic mutations, and durable operation receipts.
- `core/crates/omegon-memory/src/inmemory.rs` provides transaction-like staging and behavioral parity without persistence.
- `core/crates/omegon-memory/schema-contract.json` is generated from the Rust schema and records schema v8 tables and columns.

### Constraints

- Existing stores must complete the governed migration before `SqliteBackend::open` admits them.
- A mutation receipt and its durable effects commit in the same SQLite transaction.
- Receipt payload hashes are exact. They do not use normalized fact-content hashing.
- `schema-contract.json` is the generated cross-language schema contract.
- Rust is the authority. Other implementations adapt through explicit compatible migrations.
