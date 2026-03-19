---
id: rust-memory-tools
title: "Register memory_* agent-callable tools in Rust"
status: decided
parent: ts-to-rust-migration
open_questions: []
---

# Register memory_* agent-callable tools in Rust

## Overview

Bridge the 7 memory tools (memory_query, memory_recall, memory_store, memory_supersede, memory_archive, memory_focus, memory_release, memory_episodes, memory_connect, memory_compact, memory_search_archive) to the omegon-memory crate. Storage layer exists — need tool registration and JSON schema definitions.

## Decisions

### Decision: All 11 memory tools registered

**Status:** decided
**Rationale:** 8 tools were already implemented. Added memory_episodes (backend search_episodes), memory_compact (signals conversation-level compaction), memory_search_archive (FTS search). 11 total tools matching the TS surface.

## Open Questions

*No open questions.*
