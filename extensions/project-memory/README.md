# Project Memory Extension

Persistent, semantic memory for the pi coding agent. Facts, decisions, and patterns survive across sessions and inform future work.

## Architecture

```
index.ts          — Extension entry: tools, lifecycle events, context injection
factstore.ts      — SQLite-backed fact/episode/vector storage with FTS5
embeddings.ts     — Ollama embedding client, cosine similarity, vector serialization
extraction-v2.ts  — Subagent extraction (JSONL actions), episode generation
triggers.ts       — Extraction trigger heuristics (token count, tool calls)
template.ts       — Section definitions (Architecture, Decisions, etc.)
types.ts          — Shared types and default config
migration.ts      — One-time markdown → SQLite migration
```

### Data Model

**Facts** are atomic knowledge units stored in SQLite with:
- Section classification (Architecture, Decisions, Constraints, Known Issues, Patterns & Conventions, Specs)
- Confidence decay based on age and reinforcement count
- Content-hash deduplication
- Supersession chains for fact evolution
- FTS5 full-text search index

**Minds** are named memory stores. Each project gets a `default` mind. Additional minds can be created for branches, experiments, or ingested external knowledge.

**Episodes** are session narratives generated at shutdown — capturing what happened, what was decided, and what's still open. Linked to the facts created during that session.

**Edges** connect facts with typed relationships (depends_on, contradicts, enables, etc.) forming a knowledge graph.

**Vectors** (Float32Array BLOBs) enable semantic retrieval via cosine similarity against Ollama-generated embeddings.

### Schema Versioning

The database uses incremental schema versioning via a `schema_version` table:

| Version | Description |
|---------|-------------|
| 1       | Core tables: minds, facts, facts_fts, edges |
| 2       | Vector + episode tables: facts_vec, episodes, episode_facts, episodes_vec |

Migrations run automatically on database open. `CREATE TABLE IF NOT EXISTS` ensures idempotency.

## Tools

| Tool | Description |
|------|-------------|
| `memory_query` | Read all project memory (full dump) |
| `memory_recall(query)` | Semantic search — returns ranked facts by relevance × confidence |
| `memory_store(section, content)` | Store a new fact with pre-store conflict detection |
| `memory_supersede(fact_id, section, content)` | Atomically replace a fact |
| `memory_archive(fact_ids)` | Archive stale facts |
| `memory_search_archive(query)` | Search archived/superseded facts |
| `memory_connect(source, target, relation)` | Create a typed edge between facts |
| `memory_compact` | Trigger context compaction with extraction |
| `memory_recall(query)` | Semantic search over facts |
| `memory_episodes(query)` | Search session episode narratives |
| `memory_focus(fact_ids)` | Pin facts to working memory (survives compaction) |
| `memory_release` | Clear working memory buffer |

## Semantic Features (v3 — Hippocampus)

Requires a local Ollama instance with an embedding model:

```bash
ollama pull qwen3-embedding:0.6b   # 639MB, 1024 dims, ~108ms/embed
# or
ollama pull qwen3-embedding:4b     # 2.5GB, 2048 dims, higher quality
```

### How It Works

1. **Background indexing**: On session start, all unembedded facts are vectorized asynchronously
2. **Contextual injection**: `before_agent_start` embeds the user prompt and injects only the most relevant facts (top-20 semantic + core sections + working memory) instead of dumping all facts
3. **Semantic recall**: `memory_recall(query)` returns facts ranked by cosine similarity × confidence
4. **Conflict detection**: `memory_store` checks for >85% similar facts BEFORE storing and warns about potential duplicates
5. **Episode search**: `memory_episodes(query)` finds relevant past session narratives
6. **Working memory**: Facts accessed via recall/store enter a session-scoped buffer (cap 25) with priority injection

### Graceful Degradation

All semantic features fall back when Ollama is unavailable:
- `memory_recall` → FTS5 keyword search
- Context injection → full fact dump
- Conflict detection → skipped
- Episode search → most recent episodes (chronological)
- Status bar shows `🧠⚡` when embeddings available, `🧠` when not

### Dimension Mismatch Handling

If the embedding model changes between sessions (e.g., switching from 0.6b to 4b), vectors with the old dimensions are automatically purged and re-indexed. The `purgeStaleVectors()` method removes vectors whose `dims` column doesn't match the current model's output. Semantic search also skips individual vectors with mismatched dimensions as a safety net.

## Context Injection

On each agent turn, memory is injected as a system message. The injection strategy depends on available capabilities:

| Condition | Strategy |
|-----------|----------|
| <3 facts | Welcome message only |
| No embeddings OR <50% vector coverage OR <20 facts | Full dump via `renderForInjection()` |
| Embeddings available + sufficient coverage | Semantic subset: core sections (Constraints, Specs) + working memory + top-20 relevant facts |

Recent episodes (last 3) and global knowledge are always appended when available.

## Extraction

A background subagent periodically scans conversation history and emits JSONL actions:

```jsonl
{"type":"observe","section":"Architecture","content":"System uses k8s 1.29"}
{"type":"reinforce","id":"abc123"}
{"type":"supersede","id":"def456","section":"Architecture","content":"Updated to k8s 1.30"}
{"type":"archive","id":"ghi789"}
{"type":"connect","source":"abc123","target":"def456","relation":"replaces"}
```

Extraction triggers when:
- Token count exceeds threshold since last extraction
- Sufficient tool calls have occurred
- Manual `/memory refresh` command
- Session compaction event

### Episode Generation

At session shutdown (if >5 messages exchanged), a subagent generates a session episode:

```json
{"title":"Migrated auth from JWT to OIDC","narrative":"Goal was to replace JWT auth with OIDC. Updated middleware, added Keycloak config, fixed 3 test failures. Decision: use PKCE flow for all clients. Open: need to update API docs."}
```

Episodes are stored with links to facts created during the session and optionally embedded for semantic search.

## Configuration

Defined in `types.ts` as `MemoryConfig`:

| Key | Default | Description |
|-----|---------|-------------|
| `extractionModel` | `claude-sonnet-4-6` | Model for extraction subagent |
| `maxLines` | 50 | Max facts before pruning |
| `minimumTokensToInit` | 10000 | Min tokens before first extraction |
| `minimumTokensBetweenUpdate` | 5000 | Min tokens between extractions |
| `toolCallsBetweenUpdates` | 8 | Min tool calls between extractions |
| `extractionTimeout` | 60000 | Extraction subprocess timeout (ms) |
| `shutdownExtractionTimeout` | 15000 | Episode generation timeout (ms) |

## Confidence Decay

Facts decay exponentially based on time since last reinforcement:

```
confidence = e^(-ln(2) × daysSince / halfLife)
halfLife = baseHalfLife × reinforcementFactor^(count - 1)
```

| Profile | Base Half-Life | Reinforcement Factor |
|---------|---------------|---------------------|
| Project | 14 days | 1.8× per reinforcement |
| Global | 30 days | 2.5× per reinforcement |

**Specs section facts are exempt from decay** (always confidence 1.0).

## Testing

```bash
cd extensions/project-memory
npx tsx --test *.test.ts
```

Test files:
- `factstore.test.ts` — Core CRUD, dedup, supersession, archival, FTS5, rendering, minds, extraction processing, decay math
- `embeddings.test.ts` — Cosine similarity, vector serialization roundtrips, dimension constants
- `vectors-episodes.test.ts` — Vector storage, semantic search, dimension mismatch handling, conflict detection, episode CRUD, episode vectors, renderFactList, JSONL export/import with episodes, schema versioning
- `edges.test.ts` — Edge CRUD, dedup, rendering, cross-mind edges, extraction processing
- `extraction.test.ts` — Trigger heuristics
- `template.test.ts` — Section definitions, markdown template, appendToSection
- `migration.test.ts` — Markdown → SQLite migration

## Slash Commands

| Command | Description |
|---------|-------------|
| `/memory` | Show stats: fact count, confidence, vector coverage, episodes, working memory |
| `/memory refresh` | Force extraction cycle (prune, consolidate, reinforce) |
| `/memory export` | Export facts.jsonl for cross-machine sync |

## File Layout

```
.pi/memory/
├── facts.db           # SQLite database (gitignored)
├── facts.db-wal       # WAL journal (gitignored)
├── facts.jsonl        # Portable export (git-tracked)
└── memory.md.migrated # Pre-migration backup (if migrated)

~/.pi/memory/
└── global.db          # Cross-project global knowledge
```
