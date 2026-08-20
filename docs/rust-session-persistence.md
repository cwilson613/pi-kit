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

This document records the original persistence plan. The Rust runtime now owns
interactive session listing, orderly save, and `--resume`. Conversation JSON is
a compatibility snapshot containing messages, operator observations, intent,
decay window, and compaction summary. Resume restores recent canonical history
and summarizes older messages; it is not exact model-context replay.

Slice 1 also adds an adjacent append-only authority stream for durable prompt,
queue, turn, interruption, minimum invocation, recovery, and closure facts. See
`docs/runtime-session-semantic-protocol.md` for the current contract.

## Research

### Implemented boundaries

- `ConversationState::save_session(path)` — serialize to JSON (messages + intent + decay_window + compaction_summary)
- `ConversationState::load_session(path)` — deserialize and reconstruct canonical history
- session directory management, listing, prefix selection, and resume
- orderly interactive auto-save, with `--no-session` disabling that snapshot and interactive authority sidecar
- strict authority replay and conservative active-turn recovery

Complete provider/model-context replay remains deferred to Slice 5. Project
memory, logs, and audit records are separate persistence systems.

## Decisions

### Decision: Conversation snapshots and semantic authority have separate roles

**Status:** decided
**Rationale:** Whole-file conversation snapshots support user-facing continuity and
legacy resume. Adjacent authority JSONL records the minimum ordered facts needed
for strict queue, interruption, recovery, and closure semantics. Neither record
is presented as complete Slice-5 semantic replay.

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
