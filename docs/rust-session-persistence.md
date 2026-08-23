+++
id = "2d809756-7b5e-44dd-b31a-d9c34747c965"
kind = "document"
title = "Rust session persistence — save/load conversation state, session resume"
status = "implemented"
tags = ["rust", "session", "persistence", "resume"]
aliases = ["rust-session-persistence"]
imported_reference = false

[publication]
enabled = false
visibility = "private"

[data]
open_questions = []
parent = "rust-phase-1"
priority = "2"
+++

# Rust session persistence — save/load conversation state, session resume

## Overview

The Rust runtime owns session listing, save, and `--resume` through a plural-store
semantic protocol. Event v1 plus content blobs are semantic truth. Reducer/cache
v5, projector cursor and projection v1, host-state checkpoint v1, observation
ledger v1, and catalog v1 each retain distinct version and authority roles. See
`docs/runtime-session-semantic-protocol.md` for the normative contract.

Schema-v1 conversation JSON is now a bounded compatibility importer. Legacy
resume may still summarize older messages. Opening a valid pair beside
pre-boundary authority materializes its model-facing view exactly once and
establishes mixed lineage. Full lineage and materialized mixed lineage neither
require nor rewrite the pair; existing artifacts are not automatically deleted.

## Research

### Implemented boundaries

- `ConversationState::save_session(path)` and `load_session(path)` retain the schema-v1 legacy codec and nonsemantic local uses
- session directory management, listing, prefix selection, and resume
- semantic authority/blob publication plus separately versioned host, observation, catalog, projection, telemetry, audit, and journal stores
- strict authority replay, conservative active-turn recovery, and exact full or exact-suffix context reduction
- catalog-first maintenance inspection and quarantine, with legacy-pair fallback

Project memory, logs, and audit records remain separate persistence systems.

## Decisions

### Decision: Conversation snapshots and semantic authority have separate roles

**Status:** decided
**Rationale:** Whole-file conversation snapshots support one-way legacy import.
Authority events and blobs provide exact semantic replay; host state,
observations, catalogs, projections, telemetry, audit, and journal records do not
become semantic authority. Mixed history is one labeled imported base plus an
exact suffix and never claims an exact full historical transcript.

## Open Questions

*No open questions.*

## Implementation Notes

### File Scope

- `core/crates/omegon/src/session.rs` — session directory layout, save, list, resume, and ID generation
- `core/crates/omegon/src/conversation.rs` — conversation snapshot encoding and resume reconstruction
- `core/crates/omegon/src/session_authority.rs` — adjacent semantic authority and recovery
- `core/crates/omegon/src/main.rs` — CLI and runtime composition

### Constraints

- `--resume` with no argument resumes the most recent eligible session for the workspace.
- `--resume <id>` accepts the supported session identifier or prefix form.
- Maintenance resume-deny authority is checked before conversation loading.
- Corrupt or unsupported semantic authority fails recovery rather than being reconstructed from conversation JSON.
- Required blobs, catalogs, host stores, and observation records fail closed; only proven projector-owned derived chunks may rebuild from validated authority.
- `/transcript` is exact committed semantic output; `/session-export` and `/copy session` are presentation/evidence output.
- Compatibility migration is forward-only. There is no old-writer or mirror-authority rollback mode.
