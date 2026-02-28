# Global Mind & Edges — Design

## Architecture

```
Project A (.pi/memory/facts.db)     Project B (.pi/memory/facts.db)
  └─ default mind (project facts)     └─ default mind (project facts)
         │                                    │
         ▼ new facts trigger                  ▼ new facts trigger
    ┌──────────────────────────────────────────────┐
    │  Global FactStore (~/.pi/memory/facts.db)    │
    │                                              │
    │  facts table  ── generalized knowledge       │
    │  edges table  ── relationships between facts │
    │  provenance   ── which project sourced what  │
    └──────────────────────────────────────────────┘
```

### Two databases, one extraction chain

- **Project DB** (`$CWD/.pi/memory/facts.db`): Scoped facts about this codebase.
  Already exists.
- **Global DB** (`~/.pi/memory/facts.db`): Cross-cutting knowledge + edges.
  New. Lives at user home, shared across all projects.

Both are SQLite with the same FactStore class. The global DB adds an `edges`
table for relationships between facts.

## Schema Extension — edges table

```sql
CREATE TABLE IF NOT EXISTS edges (
  id                  TEXT PRIMARY KEY,
  source_fact_id      TEXT NOT NULL,
  target_fact_id      TEXT NOT NULL,
  relation_type       TEXT NOT NULL,      -- 'relates_to', 'contradicts', 'depends_on', 'generalizes', 'instance_of'
  description         TEXT NOT NULL,      -- LLM-generated edge label
  confidence          REAL NOT NULL DEFAULT 1.0,
  last_reinforced     TEXT NOT NULL,
  reinforcement_count INTEGER NOT NULL DEFAULT 1,
  decay_rate          REAL NOT NULL DEFAULT 0.049,
  status              TEXT NOT NULL DEFAULT 'active',
  created_at          TEXT NOT NULL,
  created_session     TEXT,
  source_mind         TEXT,               -- which project mind the source fact came from
  target_mind         TEXT,               -- which project mind the target fact came from
  FOREIGN KEY (source_fact_id) REFERENCES facts(id) ON DELETE CASCADE,
  FOREIGN KEY (target_fact_id) REFERENCES facts(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_edges_source ON edges(source_fact_id) WHERE status = 'active';
CREATE INDEX IF NOT EXISTS idx_edges_target ON edges(target_fact_id) WHERE status = 'active';
CREATE INDEX IF NOT EXISTS idx_edges_type   ON edges(relation_type)  WHERE status = 'active';
```

Edges decay with the same math as facts. Reinforcement extends half-life.
When a fact is archived/superseded, CASCADE deletes its edges.

## Extraction Chain

Single extraction event, two phases:

### Phase 1: Project extraction (existing)
```
conversation context + project facts → JSONL actions → processExtraction(project_mind)
                                                        returns { reinforced, added, newFactIds[] }
```

### Phase 2: Global extraction (new, conditional)
Only fires if Phase 1 produced new facts (`newFactIds.length > 0`).

```
new project facts + global facts → JSONL actions → processExtraction(global_mind)
                                                    + processEdges(global_mind)
```

The global extraction prompt is different:
- Receives the newly-created project facts (with project context)
- Receives existing global facts (with IDs)
- Asks: "What generalizable knowledge do these new facts represent?
         What connections exist between these facts and existing global knowledge?"
- Outputs the same action types PLUS:
  ```
  {"type":"connect","source":"<fact_id>","target":"<fact_id>","relation":"relates_to","description":"Both involve..."}
  ```

### Cost control
- Phase 2 only fires when new facts are created (not on pure reinforcements)
- Phase 2 prompt is smaller: just new facts + global facts, no conversation replay
- Global mind decay is slower (longer base half-life) — fewer facts churn through

## processExtraction changes

Current return: `{ reinforced: number, added: number }`
New return: `{ reinforced: number, added: number, newFactIds: string[] }`

This is the signal that chains Phase 1 → Phase 2.

## ExtractionAction changes

```typescript
export interface ExtractionAction {
  type: "observe" | "reinforce" | "supersede" | "archive" | "connect";
  id?: string;
  section?: SectionName;
  content?: string;
  // connect-specific:
  source?: string;       // source fact ID
  target?: string;       // target fact ID
  relation?: string;     // relation_type
  description?: string;  // edge label
}
```

`connect` actions are only valid in global extraction. processExtraction
ignores them; processEdges handles them.

## Global extraction prompt

```
You are a cross-project knowledge synthesizer. You receive:
1. New facts just extracted from a coding session (with project context)
2. Existing facts in the global knowledge base (with IDs)

Your job: identify generalizable knowledge and connections.

ACTIONS:

{"type":"observe","section":"Architecture","content":"..."}
  → A new fact generalizes beyond its source project. Rewrite it to be project-agnostic.

{"type":"reinforce","id":"abc123"}
  → An existing global fact is confirmed by this project's new evidence.

{"type":"connect","source":"<id>","target":"<id>","relation":"relates_to","description":"..."}
  → Two facts are meaningfully related. Relation types:
    - relates_to: general thematic connection
    - contradicts: facts are in tension
    - depends_on: source fact requires target fact
    - generalizes: source is a more general form of target
    - instance_of: source is a specific instance of target pattern

RULES:
- Only promote facts that would be useful across MULTIPLE projects.
- Rewrite promoted facts to remove project-specific details.
- Connections should represent genuine analytical insight, not surface similarity.
- Prefer fewer, high-quality connections over many weak ones.
- Output ONLY valid JSONL.
```

## Context injection

When rendering memory for LLM context:
1. Project facts (current behavior)
2. Relevant global facts (FTS5 query using project context as search terms)
3. Edges connecting injected facts (shows the agent relationships it should be aware of)

This gives the agent both project-specific and cross-cutting context.

## Rendering format

```markdown
## Global Knowledge

- **Architecture**: Pattern X applies across distributed systems [↔ relates_to: "Local finding Y"]
- **Decisions**: Embedded DBs preferred over client-server for CLI tooling [← generalizes: project-specific SQLite choice]
```

Edges rendered inline as annotations on the facts they connect.

## Decay parameters — global mind

Longer half-life baseline since global facts should be more durable:

```typescript
const GLOBAL_DECAY = {
  baseRate: 0.033,          // ~21 day half-life (vs 14 for project)
  reinforcementFactor: 2.0, // stronger reinforcement effect
  minimumConfidence: 0.1,
};
```

## File changes

| File | Change |
|------|--------|
| `factstore.ts` | Add `edges` table to schema, `storeEdge()`, `getEdgesForFact()`, `processEdges()`. Return `newFactIds` from `processExtraction`. |
| `extraction-v2.ts` | Add `buildGlobalExtractionPrompt()`, `formatNewFactsForGlobalExtraction()`. |
| `index.ts` | Open global FactStore at `~/.pi/memory/facts.db`. Chain Phase 2 after Phase 1. Include global facts in context injection. |
| `triggers.ts` | No changes — trigger logic is Phase 1 only. Phase 2 is conditional on Phase 1 output. |
| `types.ts` | Add `globalMemoryDir` config option. |

## Open questions

1. **Edge rendering budget**: How many global facts + edges to inject alongside project facts? Need to avoid blowing context window.
2. **Edge search**: Should `memory_search_archive` also search edges? FTS5 on edge descriptions?
3. **Manual edge creation**: Should there be a `memory_connect` tool for explicit user-created edges?
4. **Bidirectionality**: Are edges directional or bidirectional? Current design is directional (source → target) but `relates_to` is inherently bidirectional.
