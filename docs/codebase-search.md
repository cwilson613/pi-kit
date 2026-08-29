+++
id = "47774561-f997-4bcc-a1d4-d270bade54ad"
kind = "document"
title = "codebase_search — AST-aware code and knowledge retrieval"
status = "implemented"
tags = ["architecture", "tools", "code-intelligence", "memory", "lsp", "retrieval"]
aliases = ["codebase-search"]
imported_reference = false

[publication]
enabled = false
visibility = "private"

[data]
issue_type = "feature"
open_questions = []
priority = "1"
related = ["lsp-integration"]
+++

# codebase_search — AST-aware code and knowledge retrieval

## Overview

The `codebase_search` tool uses tree-sitter and language-specific fallback scanners to create
structural code chunks. It also indexes project knowledge files. A release-coupled native
extension owns the SQLite cache and BM25 index that rank concept queries such as
"find code about packet fragmentation."

The tool accepts `query`, `scope`, `max_results`, optional knowledge `tags`, and an optional
repository-relative `within` prefix. `codebase_index(invalidate)` runs an incremental or full
index operation. The tools remain declared if the codescan extension is absent or cannot start;
calls then return typed unavailable details.

Inspired by ATLAS's PageIndex component (itigges22/ATLAS), which replaced Qdrant vector RAG with
AST-aware chunking after finding that function/class boundaries are semantically meaningful chunk
boundaries while arbitrary token windows are not.

## Research

### Relationship to LSP

These are complementary layers at different levels of the code-intelligence stack:

```
codebase_search          LSP
────────────────         ────────────────────────────
"find code about X"      "where is symbol Y defined"
discovery mode           navigation mode
no server required       requires language server
tree-sitter + BM25       full type system
works on any project     needs per-language setup
```

LSP answers precise navigation questions about *known* symbols. `codebase_search` answers
discovery questions about *unknown* relevance. The agent needs both: LSP to navigate once it
knows what it's looking for, `codebase_search` to build the right context window before it
knows which symbols matter.

Shared dependency: tree-sitter. LSP client implementation and `codebase_search` both need
AST parsing. The tree-sitter crates (`tree-sitter`, `tree-sitter-rust`, `tree-sitter-python`,
etc.) should be factored into a shared `omegon-codescan` crate rather than duplicated.

### Future Memory Seeding

The following memory integration is a future direction. The current codescan service does not
write structural facts to project memory.

The indexing pass produces a complete structural map: modules, types, functions, their
relationships and locations. This is exactly the architectural knowledge that currently has to
be manually discovered each session via bash + `memory_store` calls.

Three integration modes with the memory system:

**1. Index-time seeding**
On first index (or after detected git HEAD change), write structural facts directly to project
memory. Example outputs:
- `Architecture: "styrene-lxmf depends on styrene-rns for transport; LXMF router owns delivery"`
- `Architecture: "Identity key material in styrene-identity/src/identity.rs lines 44–112"`

The agent arrives at a new project already knowing its structure rather than rediscovering it.

**2. Retrieval-time routing**
Memory facts (architectural decisions, known file locations) can pre-filter and weight BM25
search. Semantic memory as a retrieval hint layer on top of syntactic search.

**3. Mind/persona seeding**
Personas with minds (memory stores) can be instantiated with codebase-indexed knowledge.
A "Rust Developer" persona in styrene-rs would arrive knowing the module structure, key types,
and dependency graph — genuine project-specific knowledge, not generic expertise.

### Invalidation Strategy

The SQLite cache stores a content hash for each repository-relative path and content kind.
Incremental indexing atomically replaces changed paths. It prunes removed paths and publishes
the current Git HEAD only after a complete successful run. A cancelled or failed full invalidation
rolls back to the prior complete index.

### Current Implementation

```
core/crates/omegon-codescan/          Scanners, SQLite cache, indexer, and BM25
core/crates/omegon-codescan-contracts/
                                     Versioned request, response, status, and error protocol
extensions/omegon-codescan/           Native RPC process and serial engine worker
core/crates/omegon/src/codescan_service.rs
                                     Host binding to the admitted extension handle
core/crates/omegon/src/tools/codebase_search.rs
                                     Tool validation and result rendering
```

One extension-owned serial worker exclusively owns SQLite, scanning, Git HEAD freshness checks,
and BM25 construction. `codebase_search`, `codebase_index`, and code-context requests use one
boot-captured extension RPC handle. The host does not link the engine or open the database.
Cancellation uses the JSON-RPC request identity. Graceful shutdown cancels active work and joins
the worker. The host supervisor kills and reaps a non-cooperative process group.

## Decisions

### Decision: Two-index SQLite cache (.omegon/codescan.db) with tree-sitter code scanner and markdown/JSON knowledge scanner

**Status:** decided
**Rationale:** The cache is separate from `facts.db` because it uses file-content invalidation rather than fact decay. The code index uses tree-sitter declaration boundaries with language-specific fallbacks. The knowledge index scans supported project documentation and JSON sources. A HEAD-based fast path skips the file walk when the commit and relevant worktree state are unchanged. Each request performs freshness work on the extension worker; no detached refresh task owns the database.

## Open Questions

*No open questions.*

## Relations

- Builds on: `lsp-integration` (shared tree-sitter dependency, complementary layer)
- Feeds into: memory system (structural fact seeding, code-keyed invalidation)
- Feeds into: persona mind stores (project-specific knowledge at instantiation time)
- Inspired by: ATLAS PageIndex (itigges22/ATLAS — AST tree + BM25 hybrid retrieval)
