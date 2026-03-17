/**
 * Project Memory — Fact Store
 *
 * SQLite-backed storage for memory facts with decay-based reinforcement.
 * Replaces the markdown-based MemoryStorage for structured persistence.
 *
 * Schema:
 *   facts — individual knowledge atoms with confidence decay
 *   minds — named memory stores with lifecycle
 *   facts_fts — FTS5 virtual table for full-text search
 *
 * Rendering:
 *   Active facts are rendered to Markdown-KV for LLM context injection.
 *   The LLM never sees the database directly.
 */

import * as path from "node:path";
import * as fs from "node:fs";
import * as crypto from "node:crypto";
import { SECTIONS, type SectionName } from "./template.ts";
import { cosineSimilarity, vectorToBlob, blobToVector } from "./embeddings.ts";
import {
  computeConfidence as coreComputeConfidence,
  contentHash as coreContentHash,
  normalizeForHash as coreNormalizeForHash,
  type DecayProfile as CoreDecayProfile,
  type DecayProfileName,
  resolveDecayProfile,
  DECAY as CORE_DECAY,
  GLOBAL_DECAY as CORE_GLOBAL_DECAY,
  RECENT_WORK_DECAY as CORE_RECENT_WORK_DECAY,
} from "./core.ts";

/**
 * Resolve the SQLite database constructor.
 * Prefers better-sqlite3 (native, battle-tested), falls back to node:sqlite.
 */
function loadDatabase(): any {
  try {
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const BetterSqlite3 = require("better-sqlite3");
    // Verify the native addon loads (ABI mismatch throws here, not at require time).
    // Create a throwaway in-memory DB to exercise the native binding.
    const test = new BetterSqlite3(":memory:");
    test.close();
    return BetterSqlite3;
  } catch {
    // Fallback: wrap node:sqlite DatabaseSync to match better-sqlite3 API subset.
    // Triggers when better-sqlite3 is missing OR its native addon was compiled
    // against a different Node.js version (ERR_DLOPEN_FAILED).
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const { DatabaseSync } = require("node:sqlite");
    return class NodeSqliteWrapper {
      private db: any;
      constructor(filepath: string) {
        this.db = new DatabaseSync(filepath);
      }
      pragma(stmt: string) {
        return this.db.prepare(`PRAGMA ${stmt}`).get();
      }
      exec(sql: string) {
        this.db.exec(sql);
      }
      prepare(sql: string) {
        const s = this.db.prepare(sql);
        return {
          run: (...args: any[]) => s.run(...args),
          get: (...args: any[]) => s.get(...args),
          all: (...args: any[]) => s.all(...args),
        };
      }
      close() {
        this.db.close();
      }
      transaction(fn: Function) {
        return (...args: any[]) => {
          this.db.exec("BEGIN");
          try {
            const result = fn(...args);
            this.db.exec("COMMIT");
            return result;
          } catch (e) {
            this.db.exec("ROLLBACK");
            throw e;
          }
        };
      }
    };
  }
}

const Database = loadDatabase();

function buildSafeFtsQuery(query: string, joiner: "AND" | "OR" = "AND"): string {
  const tokens = query
    .split(/\s+/)
    .map(token => token.trim())
    .filter(token => token.length > 0)
    .map(token => token.replace(/"/g, '""'))
    .filter(token => /[\p{L}\p{N}]/u.test(token))
    .map(token => `"${token}"`);

  return tokens.join(` ${joiner} `);
}

function isIgnorableFtsQueryError(error: unknown): boolean {
  if (!(error instanceof Error)) return false;
  const message = error.message.toLowerCase();
  return (
    message.includes("fts5")
    || message.includes("malformed match expression")
    || message.includes("syntax error")
    || message.includes("unterminated string")
    || message.includes("no such column")
  );
}

/** Generate a short unique ID */
function nanoid(size = 12): string {
  const bytes = crypto.randomBytes(size);
  return bytes.toString("base64url").slice(0, size);
}

/** Normalize content for dedup hashing — delegates to core.ts */
const normalizeForHash = coreNormalizeForHash;

/** Compute content hash for dedup — delegates to core.ts */
const contentHash = coreContentHash;

// --- Types ---

export interface Fact {
  id: string;
  mind: string;
  section: string;
  content: string;
  status: "active" | "superseded" | "archived";
  created_at: string;
  created_session: string | null;
  supersedes: string | null;
  superseded_at: string | null;
  archived_at: string | null;
  source: "manual" | "extraction" | "ingest" | "migration" | "lifecycle" | "tool-call";
  content_hash: string;
  confidence: number;
  last_reinforced: string;
  reinforcement_count: number;
  decay_rate: number;
  /** Decay profile discriminant — stored per-fact for correct read-time decay. */
  decay_profile: DecayProfileName;
  /** Lamport logical timestamp — incremented on every mutation. Higher version wins on git-sync. */
  version: number;
  /** Last time this fact was returned by semanticSearch. Null if never accessed. */
  last_accessed: string | null;
}

export interface MindRecord {
  name: string;
  description: string;
  status: "active" | "refined" | "retired";
  origin_type: "local" | "link" | "remote";
  origin_path: string | null;
  origin_url: string | null;
  readonly: number; // 0 or 1
  parent: string | null;
  created_at: string;
  last_sync: string | null;
}

export interface StoreFactOptions {
  mind?: string;
  section: SectionName;
  content: string;
  source?: Fact["source"];
  session?: string | null;
  supersedes?: string | null;
  confidence?: number;
  reinforcement_count?: number;
  decay_rate?: number;
  /** Decay profile discriminant — stored per-fact so read-time decay uses the correct profile. */
  decayProfile?: DecayProfileName;
}

export interface ReinforcementResult {
  reinforced: number;
  added: number;
  newFactIds: string[];
}

export interface Episode {
  id: string;
  mind: string;
  title: string;
  narrative: string;
  date: string;
  session_id: string | null;
  created_at: string;
}

export interface Edge {
  id: string;
  source_fact_id: string;
  target_fact_id: string;
  relation: string;
  description: string;
  confidence: number;
  last_reinforced: string;
  reinforcement_count: number;
  decay_rate: number;
  status: "active" | "archived";
  created_at: string;
  created_session: string | null;
  source_mind: string | null;
  target_mind: string | null;
}

export interface EdgeResult {
  added: number;
  reinforced: number;
}

// --- Decay math (delegated to core.ts — the Rust port target) ---

/** Re-export canonical decay profiles from core.ts so existing importers are unaffected. */
export const DECAY = CORE_DECAY;
export const GLOBAL_DECAY = CORE_GLOBAL_DECAY;
export const RECENT_WORK_DECAY = CORE_RECENT_WORK_DECAY;
export type DecayProfile = CoreDecayProfile;
export { DecayProfileName, resolveDecayProfile };

/** Section-specific decay overrides — keyed by section name */
const SECTION_DECAY_OVERRIDES: Partial<Record<string, typeof RECENT_WORK_DECAY>> = {
  "Recent Work": RECENT_WORK_DECAY,
};

/** Delegates to core.ts::computeConfidence — single source of truth. */
export const computeConfidence = coreComputeConfidence;

/**
 * Determine the decay profile name for a section.
 * Recent Work → "recent_work" (halfLife=2d). All others → undefined (uses storeFact's default "standard").
 * Centralizes the mapping so it's not duplicated across processExtraction call sites.
 */
function decayProfileForSection(section: string): DecayProfileName | undefined {
  return section === "Recent Work" ? "recent_work" : undefined;
}

// --- FactStore ---

export class FactStore {
  private db: any;
  private dbPath: string;
  private decayProfile: DecayProfile;

  constructor(memoryDir: string, opts?: { decay?: DecayProfile; dbName?: string }) {
    this.decayProfile = opts?.decay ?? DECAY;
    this.dbPath = path.join(memoryDir, opts?.dbName ?? "facts.db");
    fs.mkdirSync(memoryDir, { recursive: true });
    this.db = new Database(this.dbPath);
    this.db.pragma("journal_mode = WAL");
    this.db.pragma("busy_timeout = 10000");
    this.db.pragma("foreign_keys = ON");
    this.initSchema();
    this.runMigrations();
  }

  /** Current schema version — bump when adding migrations */
  static readonly SCHEMA_VERSION = 4;

  private getSchemaVersion(): number {
    try {
      const row = this.db.prepare(`SELECT version FROM schema_version ORDER BY version DESC LIMIT 1`).get();
      return row?.version ?? 0;
    } catch {
      return 0; // Table doesn't exist yet
    }
  }

  private setSchemaVersion(version: number): void {
    this.db.prepare(
      `INSERT INTO schema_version (version, applied_at) VALUES (?, ?)`
    ).run(version, new Date().toISOString());
  }

  /**
   * Run schema migrations incrementally.
   * Each migration is idempotent and tagged with a version number.
   * Version 1 = initial schema (CREATE TABLE IF NOT EXISTS in initSchema).
   * Version 2+ = incremental ALTER/CREATE statements.
   */
  private runMigrations(): void {
    const current = this.getSchemaVersion();
    const target = FactStore.SCHEMA_VERSION;

    if (current >= target) return;

    // Migration 1→2: Add vector and episode tables (v3 Hippocampus)
    // These use CREATE TABLE IF NOT EXISTS so they're idempotent even for
    // databases that already have them from the original non-versioned code.
    if (current < 2) {
      this.db.exec(`
        CREATE TABLE IF NOT EXISTS facts_vec (
          fact_id    TEXT PRIMARY KEY,
          embedding  BLOB NOT NULL,
          model      TEXT NOT NULL,
          dims       INTEGER NOT NULL,
          created_at TEXT NOT NULL,
          FOREIGN KEY (fact_id) REFERENCES facts(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS episodes (
          id          TEXT PRIMARY KEY,
          mind        TEXT NOT NULL DEFAULT 'default',
          title       TEXT NOT NULL,
          narrative   TEXT NOT NULL,
          date        TEXT NOT NULL,
          session_id  TEXT,
          created_at  TEXT NOT NULL,
          FOREIGN KEY (mind) REFERENCES minds(name) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS episode_facts (
          episode_id TEXT NOT NULL,
          fact_id    TEXT NOT NULL,
          PRIMARY KEY (episode_id, fact_id),
          FOREIGN KEY (episode_id) REFERENCES episodes(id) ON DELETE CASCADE,
          FOREIGN KEY (fact_id) REFERENCES facts(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_episodes_mind
          ON episodes(mind, date DESC);
        CREATE INDEX IF NOT EXISTS idx_episodes_date
          ON episodes(date DESC);

        CREATE TABLE IF NOT EXISTS episodes_vec (
          episode_id TEXT PRIMARY KEY,
          embedding  BLOB NOT NULL,
          model      TEXT NOT NULL,
          dims       INTEGER NOT NULL,
          created_at TEXT NOT NULL,
          FOREIGN KEY (episode_id) REFERENCES episodes(id) ON DELETE CASCADE
        );
      `);
      this.setSchemaVersion(2);
    }

    // Migration 2→3: Correctness and Rust-migration prerequisites
    //
    // decay_profile: fixes the wrong-profile decay bug — stores which decay
    //   profile was used at write time so read-time computeConfidence uses the
    //   correct profile. Existing facts get 'standard' (matches the effective
    //   behaviour since DECAY was always the default).
    //
    // version (Lamport timestamp): fixes git-sync conflict resolution bug where
    //   archived facts could be resurrected by concurrent reinforcement on
    //   another machine. Higher version always wins on import. Existing facts
    //   get version=0; new mutations start at MAX(version)+1.
    //
    // last_accessed: enables access-pattern reinforcement — decay timer resets
    //   when a fact is retrieved by memory_recall, independent of explicit
    //   memory_store reinforcement. Nullable; null means "never accessed".
    //
    // embedding_metadata: versions the embedding model + dimension in the DB.
    //   Dimension mismatch is now a detectable error rather than a silent skip.
    //   facts_vec gains model_name FK so multi-model coexistence is tracked.
    if (current < 3) {
      this.db.exec(`
        ALTER TABLE facts ADD COLUMN decay_profile TEXT NOT NULL DEFAULT 'standard';
        ALTER TABLE facts ADD COLUMN version INTEGER NOT NULL DEFAULT 0;
        ALTER TABLE facts ADD COLUMN last_accessed TEXT;

        CREATE INDEX IF NOT EXISTS idx_facts_version
          ON facts(version DESC);

        CREATE TABLE IF NOT EXISTS embedding_metadata (
          model_name  TEXT PRIMARY KEY,
          dims        INTEGER NOT NULL,
          inserted_at TEXT NOT NULL
        );

        ALTER TABLE facts_vec ADD COLUMN model_name TEXT NOT NULL DEFAULT '';
        ALTER TABLE episodes_vec ADD COLUMN model_name TEXT NOT NULL DEFAULT '';
      `);
      this.setSchemaVersion(3);
    }

    // --- Schema v4: Fix existing Recent Work facts with wrong decay_profile ---
    // Before this fix, all Recent Work facts were stored with decay_profile='standard'
    // (halfLife=14d) instead of 'recent_work' (halfLife=2d). This made Recent Work
    // facts persist 7× longer than intended and disagree with SECTION_DECAY_OVERRIDES
    // (which correctly applied RECENT_WORK_DECAY at read time by section name).
    // This migration aligns the stored column with the read-time behaviour.
    if (current < 4) {
      this.db.prepare(
        `UPDATE facts SET decay_profile = 'recent_work' WHERE section = 'Recent Work' AND decay_profile = 'standard'`
      ).run();
      this.setSchemaVersion(4);
    }
  }

  private initSchema(): void {
    // Schema version tracking — must be first so migrations can read it
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS schema_version (
        version    INTEGER PRIMARY KEY,
        applied_at TEXT NOT NULL
      );
    `);

    // Version 1 tables — core schema
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS minds (
        name        TEXT PRIMARY KEY,
        description TEXT NOT NULL DEFAULT '',
        status      TEXT NOT NULL DEFAULT 'active',
        origin_type TEXT NOT NULL DEFAULT 'local',
        origin_path TEXT,
        origin_url  TEXT,
        readonly    INTEGER NOT NULL DEFAULT 0,
        parent      TEXT,
        created_at  TEXT NOT NULL,
        last_sync   TEXT
      );

      CREATE TABLE IF NOT EXISTS facts (
        id                  TEXT PRIMARY KEY,
        mind                TEXT NOT NULL DEFAULT 'default',
        section             TEXT NOT NULL,
        content             TEXT NOT NULL,
        status              TEXT NOT NULL DEFAULT 'active',
        created_at          TEXT NOT NULL,
        created_session     TEXT,
        supersedes          TEXT,
        superseded_at       TEXT,
        archived_at         TEXT,
        source              TEXT NOT NULL DEFAULT 'manual',
        content_hash        TEXT NOT NULL,
        confidence          REAL NOT NULL DEFAULT 1.0,
        last_reinforced     TEXT NOT NULL,
        reinforcement_count INTEGER NOT NULL DEFAULT 1,
        decay_rate          REAL NOT NULL DEFAULT ${DECAY.baseRate},
        FOREIGN KEY (mind) REFERENCES minds(name) ON DELETE CASCADE
      );

      CREATE INDEX IF NOT EXISTS idx_facts_active
        ON facts(mind, status) WHERE status = 'active';
      CREATE INDEX IF NOT EXISTS idx_facts_hash
        ON facts(mind, content_hash);
      CREATE INDEX IF NOT EXISTS idx_facts_section
        ON facts(mind, section) WHERE status = 'active';
      CREATE INDEX IF NOT EXISTS idx_facts_supersedes
        ON facts(supersedes);
      CREATE INDEX IF NOT EXISTS idx_facts_temporal
        ON facts(created_at);
      CREATE INDEX IF NOT EXISTS idx_facts_confidence
        ON facts(mind, confidence) WHERE status = 'active';
      CREATE INDEX IF NOT EXISTS idx_facts_session
        ON facts(created_session);

      CREATE TABLE IF NOT EXISTS edges (
        id                  TEXT PRIMARY KEY,
        source_fact_id      TEXT NOT NULL,
        target_fact_id      TEXT NOT NULL,
        relation            TEXT NOT NULL,
        description         TEXT NOT NULL,
        confidence          REAL NOT NULL DEFAULT 1.0,
        last_reinforced     TEXT NOT NULL,
        reinforcement_count INTEGER NOT NULL DEFAULT 1,
        decay_rate          REAL NOT NULL DEFAULT ${DECAY.baseRate},
        status              TEXT NOT NULL DEFAULT 'active',
        created_at          TEXT NOT NULL,
        created_session     TEXT,
        source_mind         TEXT,
        target_mind         TEXT,
        FOREIGN KEY (source_fact_id) REFERENCES facts(id) ON DELETE CASCADE,
        FOREIGN KEY (target_fact_id) REFERENCES facts(id) ON DELETE CASCADE
      );

      CREATE INDEX IF NOT EXISTS idx_edges_source
        ON edges(source_fact_id) WHERE status = 'active';
      CREATE INDEX IF NOT EXISTS idx_edges_target
        ON edges(target_fact_id) WHERE status = 'active';
      CREATE INDEX IF NOT EXISTS idx_edges_relation
        ON edges(relation) WHERE status = 'active';
    `);

    // FTS5 virtual table for full-text search
    this.db.exec(`
      CREATE VIRTUAL TABLE IF NOT EXISTS facts_fts USING fts5(
        id UNINDEXED,
        mind UNINDEXED,
        section UNINDEXED,
        content,
        content='facts',
        content_rowid='rowid'
      );
    `);

    // Triggers to keep FTS in sync
    // Check if triggers exist before creating (CREATE TRIGGER IF NOT EXISTS not universally supported)
    const triggerExists = this.db.prepare(
      `SELECT 1 FROM sqlite_master WHERE type='trigger' AND name='facts_fts_insert'`
    ).get();

    if (!triggerExists) {
      this.db.exec(`
        CREATE TRIGGER facts_fts_insert AFTER INSERT ON facts BEGIN
          INSERT INTO facts_fts(rowid, id, mind, section, content)
            VALUES (NEW.rowid, NEW.id, NEW.mind, NEW.section, NEW.content);
        END;

        CREATE TRIGGER facts_fts_delete AFTER DELETE ON facts BEGIN
          INSERT INTO facts_fts(facts_fts, rowid, id, mind, section, content)
            VALUES ('delete', OLD.rowid, OLD.id, OLD.mind, OLD.section, OLD.content);
        END;

        CREATE TRIGGER facts_fts_update AFTER UPDATE ON facts BEGIN
          INSERT INTO facts_fts(facts_fts, rowid, id, mind, section, content)
            VALUES ('delete', OLD.rowid, OLD.id, OLD.mind, OLD.section, OLD.content);
          INSERT INTO facts_fts(rowid, id, mind, section, content)
            VALUES (NEW.rowid, NEW.id, NEW.mind, NEW.section, NEW.content);
        END;
      `);
    }

    // Vector and episode tables are created in runMigrations() (version 2+).

    // Mark version 1 if this is a fresh database
    if (this.getSchemaVersion() === 0) {
      this.setSchemaVersion(1);
    }

    // Ensure 'default' mind exists
    const defaultMind = this.db.prepare(`SELECT 1 FROM minds WHERE name = 'default'`).get();
    if (!defaultMind) {
      this.db.prepare(`
        INSERT INTO minds (name, description, status, origin_type, created_at)
        VALUES ('default', 'Project default memory', 'active', 'local', ?)
      `).run(new Date().toISOString());
    }
  }

  // ---------------------------------------------------------------------------
  // Fact CRUD
  // ---------------------------------------------------------------------------

  /**
   * Store a fact. Returns the fact ID if stored, or null if duplicate.
   * Handles dedup via content_hash and optional explicit supersession.
   */
  storeFact(opts: StoreFactOptions): { id: string; duplicate: boolean } {
    const mind = opts.mind ?? "default";
    const now = new Date().toISOString();
    const hash = contentHash(opts.content);
    const source = opts.source ?? "manual";
    const content = opts.content.replace(/^-\s*/, "").trim();

    // Dedup check — same mind, same hash, still active
    const existing = this.db.prepare(
      `SELECT id FROM facts WHERE mind = ? AND content_hash = ? AND status = 'active'`
    ).get(mind, hash);

    if (existing) {
      // Reinforce the existing fact instead of duplicating
      this.reinforceFact(existing.id);
      return { id: existing.id, duplicate: true };
    }

    const id = nanoid();

    // Lamport timestamp: MAX(version)+1 ensures this mutation is always "newer"
    // than any existing fact, even on import from another machine.
    const versionRow = this.db.prepare(`SELECT COALESCE(MAX(version), 0) + 1 AS v FROM facts`).get();
    const version = versionRow?.v ?? 1;

    // Decay profile discriminant — stored so read-time computeConfidence
    // uses the correct profile regardless of which profile is currently active.
    const decayProfileName: DecayProfileName = opts.decayProfile ?? "standard";

    // If superseding, mark old fact and record its version for conflict detection
    if (opts.supersedes) {
      this.db.prepare(
        `UPDATE facts SET status = 'superseded', superseded_at = ?, version = ? WHERE id = ?`
      ).run(now, version, opts.supersedes);
    }

    this.db.prepare(`
      INSERT INTO facts (id, mind, section, content, status, created_at, created_session,
                         supersedes, source, content_hash, confidence, last_reinforced,
                         reinforcement_count, decay_rate, decay_profile, version)
      VALUES (?, ?, ?, ?, 'active', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    `).run(
      id, mind, opts.section, content, now,
      opts.session ?? null,
      opts.supersedes ?? null,
      source, hash,
      opts.confidence ?? 1.0,
      now,
      opts.reinforcement_count ?? 1,
      opts.decay_rate ?? this.decayProfile.baseRate,
      decayProfileName,
      version,
    );

    return { id, duplicate: false };
  }

  /**
   * Reinforce a fact — bump confidence, extend half-life.
   * Updates last_reinforced and increments version (Lamport clock).
   */
  reinforceFact(id: string): void {
    const now = new Date().toISOString();
    const versionRow = this.db.prepare(`SELECT COALESCE(MAX(version), 0) + 1 AS v FROM facts`).get();
    const version = versionRow?.v ?? 1;
    this.db.prepare(`
      UPDATE facts
      SET confidence = 1.0,
          last_reinforced = ?,
          reinforcement_count = reinforcement_count + 1,
          version = ?
      WHERE id = ?
    `).run(now, version, id);
  }

  /**
   * Update last_accessed for access-pattern reinforcement.
   * Resets the effective decay timer without incrementing reinforcement_count.
   * Called by memory_recall after returning a fact to the agent.
   */
  touchFact(id: string): void {
    const now = new Date().toISOString();
    this.db.prepare(`UPDATE facts SET last_accessed = ? WHERE id = ?`).run(now, id);
  }

  /**
   * Process extraction output: a list of observed facts and directives.
   * Returns counts of what happened.
   */
  processExtraction(
    mind: string,
    actions: ExtractionAction[],
    session?: string,
  ): ReinforcementResult {
    let reinforced = 0;
    let added = 0;
    const newFactIds: string[] = [];

    const tx = this.db.transaction(() => {
      for (const action of actions) {
        switch (action.type) {
          case "observe": {
            // Fact observed in session — reinforce if exists, add if new
            const hash = contentHash(action.content ?? "");
            const existing = this.db.prepare(
              `SELECT id FROM facts WHERE mind = ? AND content_hash = ? AND status = 'active'`
            ).get(mind, hash);

            if (existing) {
              this.reinforceFact((existing as { id: string }).id);
              reinforced++;
            } else if (action.section && action.content) {
              const result = this.storeFact({
                mind,
                section: action.section,
                content: action.content,
                source: "extraction",
                session,
                decayProfile: decayProfileForSection(action.section),
              });
              if (!result.duplicate) newFactIds.push(result.id);
              added++;
            }
            break;
          }
          case "reinforce": {
            // Explicit reinforcement by ID
            if (action.id) {
              this.reinforceFact(action.id);
              reinforced++;
            }
            break;
          }
          case "supersede": {
            // Explicit replacement
            if (action.id && action.content && action.section) {
              const result = this.storeFact({
                mind,
                section: action.section,
                content: action.content,
                source: "extraction",
                session,
                decayProfile: decayProfileForSection(action.section),
                supersedes: action.id,
              });
              if (!result.duplicate) newFactIds.push(result.id);
              added++;
            }
            break;
          }
          case "archive": {
            // Explicit archival
            if (action.id) {
              this.archiveFact(action.id);
            }
            break;
          }
        }
      }
    });

    tx();
    return { reinforced, added, newFactIds };
  }

  /** Archive a fact and clean up its vector embedding */
  archiveFact(id: string): void {
    const now = new Date().toISOString();
    const versionRow = this.db.prepare(`SELECT COALESCE(MAX(version), 0) + 1 AS v FROM facts`).get();
    const version = versionRow?.v ?? 1;
    this.db.prepare(
      `UPDATE facts SET status = 'archived', archived_at = ?, version = ? WHERE id = ?`
    ).run(now, version, id);
    // Clean up orphaned vector (CASCADE only fires on DELETE, not status change)
    this.db.prepare(`DELETE FROM facts_vec WHERE fact_id = ?`).run(id);
  }

  /**
   * Sweep facts that have decayed below their profile's minimum confidence.
   * Archives them in bulk. Returns the number of facts archived.
   *
   * This converts passive decay (lower confidence on read) into active archival.
   * Without this sweep, decayed facts remain status='active' forever — inflating
   * fact counts, consuming vector storage, and cluttering section ceiling checks.
   *
   * Uses getActiveFacts() which already:
   *   1. Computes confidence via computeConfidence (canonical formula from core.ts)
   *   2. Exempts NO_DECAY_SECTIONS (Specs → confidence = 1.0, never archived)
   *   3. Applies SECTION_DECAY_OVERRIDES (Recent Work → RECENT_WORK_DECAY)
   * So fact.confidence is the correct, already-computed value — no re-derivation.
   *
   * Profile resolution uses SECTION_DECAY_OVERRIDES (same as getActiveFacts) so
   * the minimumConfidence threshold is consistent with the confidence computation.
   */
  sweepDecayedFacts(mind: string): number {
    const facts = this.getActiveFacts(mind);
    let swept = 0;

    for (const fact of facts) {
      // Resolve profile the same way getActiveFacts does — by section override
      // first, then per-fact decay_profile column. This ensures the minimum
      // confidence threshold matches the formula that computed fact.confidence.
      const profile = SECTION_DECAY_OVERRIDES[fact.section]
        ?? resolveDecayProfile(fact.decay_profile);

      if (fact.confidence <= profile.minimumConfidence) {
        this.archiveFact(fact.id);
        swept++;
      }
    }

    return swept;
  }

  /** Archive all facts from a specific session */
  archiveSession(session: string): number {
    const now = new Date().toISOString();
    const result = this.db.prepare(
      `UPDATE facts SET status = 'archived', archived_at = ?
       WHERE created_session = ? AND status = 'active'`
    ).run(now, session);
    return result.changes;
  }

  // ---------------------------------------------------------------------------
  // Edge CRUD
  // ---------------------------------------------------------------------------

  /**
   * Store an edge between two facts. Deduplicates by source+target+relation.
   * If the same edge exists, reinforces it instead.
   */
  storeEdge(opts: {
    sourceFact: string;
    targetFact: string;
    relation: string;
    description: string;
    session?: string;
    sourceMind?: string;
    targetMind?: string;
  }): { id: string; duplicate: boolean } {
    const now = new Date().toISOString();

    // Dedup: same source, target, and relation
    const existing = this.db.prepare(
      `SELECT id FROM edges
       WHERE source_fact_id = ? AND target_fact_id = ? AND relation = ? AND status = 'active'`
    ).get(opts.sourceFact, opts.targetFact, opts.relation);

    if (existing) {
      this.reinforceEdge(existing.id);
      return { id: existing.id, duplicate: true };
    }

    const id = nanoid();
    this.db.prepare(`
      INSERT INTO edges (id, source_fact_id, target_fact_id, relation, description,
                         confidence, last_reinforced, reinforcement_count, decay_rate,
                         status, created_at, created_session, source_mind, target_mind)
      VALUES (?, ?, ?, ?, ?, 1.0, ?, 1, ?, 'active', ?, ?, ?, ?)
    `).run(
      id, opts.sourceFact, opts.targetFact, opts.relation, opts.description,
      now, this.decayProfile.baseRate, now, opts.session ?? null,
      opts.sourceMind ?? null, opts.targetMind ?? null,
    );

    return { id, duplicate: false };
  }

  /** Reinforce an edge */
  reinforceEdge(id: string): void {
    const now = new Date().toISOString();
    this.db.prepare(`
      UPDATE edges
      SET confidence = 1.0, last_reinforced = ?, reinforcement_count = reinforcement_count + 1
      WHERE id = ?
    `).run(now, id);
  }

  /** Archive an edge */
  archiveEdge(id: string): void {
    this.db.prepare(
      `UPDATE edges SET status = 'archived' WHERE id = ?`
    ).run(id);
  }

  /** Get active edges for a fact (both directions) */
  getEdgesForFact(factId: string): Edge[] {
    const edges = this.db.prepare(`
      SELECT * FROM edges
      WHERE (source_fact_id = ? OR target_fact_id = ?) AND status = 'active'
    `).all(factId, factId) as Edge[];

    return this.applyEdgeDecay(edges);
  }

  /** Get all active edges, optionally filtered by mind */
  getActiveEdges(mind?: string): Edge[] {
    let edges: Edge[];
    if (mind) {
      const chain = this.resolveMindChain(mind);
      const placeholders = chain.map(() => "?").join(", ");
      edges = this.db.prepare(`
        SELECT * FROM edges
        WHERE (source_mind IN (${placeholders}) OR target_mind IN (${placeholders})) AND status = 'active'
      `).all(...chain, ...chain) as Edge[];
    } else {
      edges = this.db.prepare(
        `SELECT * FROM edges WHERE status = 'active'`
      ).all() as Edge[];
    }
    return this.applyEdgeDecay(edges);
  }

  /** Get a single edge by ID */
  getEdge(id: string): Edge | null {
    const edge = this.db.prepare(`SELECT * FROM edges WHERE id = ?`).get(id) as Edge | null;
    if (edge) {
      const [decayed] = this.applyEdgeDecay([edge]);
      return decayed;
    }
    return null;
  }

  /**
   * Get active edges connected to any of the given fact IDs.
   * Returns top N by reinforcement count, filtered by min confidence after decay.
   */
  getEdgesForFacts(factIds: string[], limit: number = 20, minConfidence: number = DECAY.minimumConfidence): Edge[] {
    if (factIds.length === 0) return [];

    const placeholders = factIds.map(() => "?").join(",");
    const edges = this.db.prepare(`
      SELECT * FROM edges
      WHERE status = 'active'
        AND (source_fact_id IN (${placeholders}) OR target_fact_id IN (${placeholders}))
      ORDER BY reinforcement_count DESC
      LIMIT ?
    `).all(...factIds, ...factIds, limit * 2) as Edge[]; // fetch extra to account for decay filtering

    const decayed = this.applyEdgeDecay(edges);
    return decayed
      .filter(e => e.confidence >= minConfidence)
      .slice(0, limit);
  }

  /** Apply confidence decay to edges (same decay profile as this store's facts) */
  private applyEdgeDecay(edges: Edge[]): Edge[] {
    const now = Date.now();
    for (const edge of edges) {
      const lastReinforced = new Date(edge.last_reinforced).getTime();
      const daysSince = (now - lastReinforced) / (1000 * 60 * 60 * 24);
      edge.confidence = computeConfidence(daysSince, edge.reinforcement_count, this.decayProfile);
    }
    return edges;
  }

  /**
   * Process edge actions from global extraction.
   * Handles connect and reinforce_edge action types.
   */
  processEdges(
    actions: ExtractionAction[],
    session?: string,
  ): EdgeResult {
    let added = 0;
    let reinforced = 0;

    const tx = this.db.transaction(() => {
      for (const action of actions) {
        if (action.type !== "connect") continue;
        if (!action.source || !action.target || !action.relation) continue;

        // Verify both facts exist
        const sourceFact = this.getFact(action.source);
        const targetFact = this.getFact(action.target);
        if (!sourceFact || !targetFact) continue;

        const result = this.storeEdge({
          sourceFact: action.source,
          targetFact: action.target,
          relation: action.relation,
          description: action.description ?? `${action.relation}: ${sourceFact.content.slice(0, 50)} → ${targetFact.content.slice(0, 50)}`,
          session,
          sourceMind: sourceFact.mind,
          targetMind: targetFact.mind,
        });

        if (result.duplicate) reinforced++;
        else added++;
      }
    });

    tx();
    return { added, reinforced };
  }

  // ---------------------------------------------------------------------------
  // Queries
  // ---------------------------------------------------------------------------

  /**
   * Resolve the mind chain: [mind, parent, grandparent, ...].
   * Used to include inherited facts from parent minds.
   * Stops at 'default' or when no parent exists. Max depth 5 to prevent cycles.
   */
  private resolveMindChain(mind: string): string[] {
    const chain: string[] = [mind];
    let current = mind;
    for (let i = 0; i < 5; i++) {
      const rec = this.getMind(current);
      if (!rec?.parent || rec.parent === current) break;
      chain.push(rec.parent);
      current = rec.parent;
    }
    return chain;
  }

  /**
   * Get active facts for a mind, with confidence decay applied.
   * If the mind has a parent, includes inherited parent facts (deduped
   * by content_hash — child facts shadow parent facts with the same content).
   * Optionally limit to top N by confidence.
   */
  getActiveFacts(mind: string, limit?: number): Fact[] {
    const chain = this.resolveMindChain(mind);
    const placeholders = chain.map(() => "?").join(", ");
    const allFacts = this.db.prepare(
      `SELECT * FROM facts WHERE mind IN (${placeholders}) AND status = 'active'
       ORDER BY section, created_at`
    ).all(...chain) as Fact[];

    // Deduplicate: child facts shadow parent facts with the same content_hash.
    // Keep the fact from the earliest mind in the chain (= the child).
    const seen = new Map<string, number>();
    const facts: Fact[] = [];
    for (const fact of allFacts) {
      const chainIdx = chain.indexOf(fact.mind);
      const existing = seen.get(fact.content_hash);
      if (existing !== undefined && existing <= chainIdx) continue; // child already present
      seen.set(fact.content_hash, chainIdx);
      // Remove any previously added parent fact with same hash
      if (existing !== undefined) {
        const idx = facts.findIndex(f => f.content_hash === fact.content_hash);
        if (idx !== -1) facts.splice(idx, 1);
      }
      facts.push(fact);
    }

    // Apply time-based confidence decay.
    // Specs are exempt (binary exist/not-exist).
    // "Recent Work" uses a fast-decay profile (half-life 2d, no reinforcement extension).
    // All other sections use the store's default decay profile.
    const NO_DECAY_SECTIONS: readonly string[] = ["Specs"];
    const now = Date.now();
    for (const fact of facts) {
      if (NO_DECAY_SECTIONS.includes(fact.section)) {
        fact.confidence = 1.0;
      } else {
        const lastReinforced = new Date(fact.last_reinforced).getTime();
        const daysSince = (now - lastReinforced) / (1000 * 60 * 60 * 24);
        const profile = SECTION_DECAY_OVERRIDES[fact.section] ?? this.decayProfile;
        fact.confidence = computeConfidence(daysSince, fact.reinforcement_count, profile);
      }
    }

    // Sort by confidence descending within each section
    facts.sort((a, b) => {
      if (a.section !== b.section) {
        const idxA = SECTIONS.indexOf(a.section as SectionName);
        const idxB = SECTIONS.indexOf(b.section as SectionName);
        return idxA - idxB;
      }
      return b.confidence - a.confidence;
    });

    if (limit) {
      return facts.slice(0, limit);
    }
    return facts;
  }

  /** Get active facts for a specific section, sorted by confidence descending. */
  getFactsBySection(mind: string, section: string): Fact[] {
    const chain = this.resolveMindChain(mind);
    const placeholders = chain.map(() => "?").join(", ");
    const allFacts = this.db.prepare(
      `SELECT * FROM facts WHERE mind IN (${placeholders}) AND section = ? AND status = 'active' ORDER BY created_at`
    ).all(...chain, section) as Fact[];
    // Deduplicate by content_hash (child shadows parent)
    const seen = new Set<string>();
    const facts: Fact[] = [];
    for (const f of allFacts) {
      if (seen.has(f.content_hash)) continue;
      seen.add(f.content_hash);
      facts.push(f);
    }

    const NO_DECAY_SECTIONS: readonly string[] = ["Specs"];
    const now = Date.now();
    for (const fact of facts) {
      if (NO_DECAY_SECTIONS.includes(fact.section)) {
        fact.confidence = 1.0;
      } else {
        const lastReinforced = new Date(fact.last_reinforced).getTime();
        const daysSince = (now - lastReinforced) / (1000 * 60 * 60 * 24);
        const profile = SECTION_DECAY_OVERRIDES[fact.section] ?? this.decayProfile;
        fact.confidence = computeConfidence(daysSince, fact.reinforcement_count, profile);
      }
    }

    facts.sort((a, b) => b.confidence - a.confidence);
    return facts;
  }

  /** Get the count of active facts per section for a mind. */
  getSectionCounts(mind: string): Map<string, number> {
    const chain = this.resolveMindChain(mind);
    const placeholders = chain.map(() => "?").join(", ");
    const rows = this.db.prepare(
      `SELECT section, COUNT(DISTINCT content_hash) as count FROM facts WHERE mind IN (${placeholders}) AND status = 'active' GROUP BY section`
    ).all(...chain) as { section: string; count: number }[];
    return new Map(rows.map(r => [r.section, r.count]));
  }

  /** Count active facts for a mind (including inherited parent facts) */
  countActiveFacts(mind: string): number {
    const chain = this.resolveMindChain(mind);
    const placeholders = chain.map(() => "?").join(", ");
    const row = this.db.prepare(
      `SELECT COUNT(DISTINCT content_hash) as count FROM facts WHERE mind IN (${placeholders}) AND status = 'active'`
    ).get(...chain);
    return row?.count ?? 0;
  }

  /** Find facts whose content starts with a given prefix using a LIKE query (no FTS5, safe for special chars) */
  findFactsByContentPrefix(prefix: string, mind?: string): Fact[] {
    // Use LIKE with escaped pattern — only % and _ need escaping in LIKE
    const escaped = prefix.replace(/\\/g, "\\\\").replace(/%/g, "\\%").replace(/_/g, "\\_");
    const pattern = `${escaped}%`;

    if (mind) {
      const chain = this.resolveMindChain(mind);
      const placeholders = chain.map(() => "?").join(", ");
      return this.db.prepare(`
        SELECT * FROM facts
        WHERE content LIKE ? ESCAPE '\\' AND mind IN (${placeholders}) AND status = 'active'
        ORDER BY created_at DESC
      `).all(pattern, ...chain) as Fact[];
    }

    return this.db.prepare(`
      SELECT * FROM facts
      WHERE content LIKE ? ESCAPE '\\' AND status = 'active'
      ORDER BY created_at DESC
    `).all(pattern) as Fact[];
  }

  /** Full-text search across all facts (all minds, all statuses) */
  searchFacts(query: string, mind?: string): Fact[] {
    const ftsQuery = buildSafeFtsQuery(query, "AND");
    if (!ftsQuery) return [];

    try {
      if (mind) {
        const chain = this.resolveMindChain(mind);
        const placeholders = chain.map(() => "?").join(", ");
        return this.db.prepare(`
          SELECT f.* FROM facts f
          JOIN facts_fts fts ON f.rowid = fts.rowid
          WHERE facts_fts MATCH ? AND f.mind IN (${placeholders})
          ORDER BY rank
        `).all(ftsQuery, ...chain) as Fact[];
      }

      return this.db.prepare(`
        SELECT f.* FROM facts f
        JOIN facts_fts fts ON f.rowid = fts.rowid
        WHERE facts_fts MATCH ?
        ORDER BY rank
      `).all(ftsQuery) as Fact[];
    } catch (error) {
      if (isIgnorableFtsQueryError(error)) return [];
      throw error;
    }
  }

  /** Search archived/superseded facts (replaces searchArchive) */
  searchArchive(query: string, mind?: string): Fact[] {
    const ftsQuery = buildSafeFtsQuery(query, "AND");
    if (!ftsQuery) return [];

    try {
      if (mind) {
        return this.db.prepare(`
          SELECT f.* FROM facts f
          JOIN facts_fts fts ON f.rowid = fts.rowid
          WHERE facts_fts MATCH ? AND f.mind = ? AND f.status IN ('archived', 'superseded')
          ORDER BY f.created_at DESC
        `).all(ftsQuery, mind) as Fact[];
      }

      return this.db.prepare(`
        SELECT f.* FROM facts f
        JOIN facts_fts fts ON f.rowid = fts.rowid
        WHERE facts_fts MATCH ? AND f.status IN ('archived', 'superseded')
        ORDER BY f.created_at DESC
      `).all(ftsQuery) as Fact[];
    } catch (error) {
      if (isIgnorableFtsQueryError(error)) return [];
      throw error;
    }
  }

  /** Get a single fact by ID */
  getFact(id: string): Fact | null {
    return this.db.prepare(`SELECT * FROM facts WHERE id = ?`).get(id) as Fact | null;
  }

  /** Get supersession chain for a fact */
  getSupersessionChain(id: string): Fact[] {
    const chain: Fact[] = [];
    let current = this.getFact(id);
    while (current) {
      chain.push(current);
      if (current.supersedes) {
        current = this.getFact(current.supersedes);
      } else {
        break;
      }
    }
    return chain;
  }

  // ---------------------------------------------------------------------------
  // Rendering — Markdown-KV for LLM injection
  // ---------------------------------------------------------------------------

  /**
   * Render active facts as Markdown-KV for LLM context injection.
   * Filters by confidence threshold and respects a line budget.
   */
  renderForInjection(mind: string, opts?: { maxFacts?: number; minConfidence?: number; maxEdges?: number; showIds?: boolean }): string {
    const maxFacts = opts?.maxFacts ?? 50;
    const maxEdges = opts?.maxEdges ?? 20;
    const minConfidence = opts?.minConfidence ?? this.decayProfile.minimumConfidence;
    const showIds = opts?.showIds ?? false;

    // Per-section caps: Architecture is the largest section by volume.
    // Cap it aggressively so it can't crowd out other sections or blow context.
    // Remaining sections are capped at reasonable defaults.
    const SECTION_CAPS: Partial<Record<SectionName, number>> = {
      Architecture: 12,
      Decisions: 10,
      Constraints: 6,
      "Known Issues": 6,
      "Patterns & Conventions": 6,
      Specs: 10,
    };

    let facts = this.getActiveFacts(mind);

    // Filter by confidence
    facts = facts.filter(f => f.confidence >= minConfidence);

    // Apply per-section caps (top N by confidence within each section)
    const cappedFacts: typeof facts = [];
    for (const section of SECTIONS) {
      const sectionFacts = facts
        .filter(f => f.section === section)
        .sort((a, b) => b.confidence - a.confidence)
        .slice(0, SECTION_CAPS[section as SectionName] ?? 10);
      cappedFacts.push(...sectionFacts);
    }
    facts = cappedFacts;

    // Apply global cap as a final safety net
    if (facts.length > maxFacts) {
      facts.sort((a, b) => b.confidence - a.confidence);
      facts = facts.slice(0, maxFacts);
    }

    // Re-sort by section order for display
    facts.sort((a, b) => {
      const idxA = SECTIONS.indexOf(a.section as SectionName);
      const idxB = SECTIONS.indexOf(b.section as SectionName);
      if (idxA !== idxB) return idxA - idxB;
      return b.confidence - a.confidence;
    });

    const lines: string[] = [
      "<!-- Project Memory — managed by project-memory extension -->",
      "",
    ];

    const sectionDescriptions: Record<string, string> = {
      Architecture: "_System structure, component relationships, key abstractions_",
      Decisions: "_Choices made and their rationale_",
      Constraints: "_Requirements, limitations, environment details_",
      "Known Issues": "_Bugs, flaky tests, workarounds_",
      "Patterns & Conventions": "_Code style, project conventions, common approaches_",
      Specs: "_Active specifications, acceptance criteria, and design contracts driving current work_",
    };

    // Build a set of rendered fact IDs for edge lookup
    const renderedFactIds = new Set<string>();

    for (const section of SECTIONS) {
      const sectionFacts = facts.filter(f => f.section === section);
      lines.push(`## ${section}`);
      lines.push(sectionDescriptions[section] ?? "");
      lines.push("");
      if (sectionFacts.length > 0) {
        for (const f of sectionFacts) {
          const date = f.created_at.split("T")[0];
          lines.push(showIds ? `- [${f.id}] ${f.content} [${date}]` : `- ${f.content} [${date}]`);
          renderedFactIds.add(f.id);
        }
      }
      lines.push("");
    }

    // Render edges between rendered facts (capped)
    const relevantEdges = renderedFactIds.size > 0
      ? this.getEdgesForFacts([...renderedFactIds], maxEdges, minConfidence)
      : [];

    if (relevantEdges.length > 0) {
      lines.push("## Connections");
      lines.push("_Relationships between facts across domains_");
      lines.push("");
      for (const edge of relevantEdges) {
        const sourceFact = this.getFact(edge.source_fact_id);
        const targetFact = this.getFact(edge.target_fact_id);
        if (!sourceFact || !targetFact) continue;
        const srcLabel = sourceFact.content.length > 60
          ? sourceFact.content.slice(0, 57) + "..."
          : sourceFact.content;
        const tgtLabel = targetFact.content.length > 60
          ? targetFact.content.slice(0, 57) + "..."
          : targetFact.content;
        lines.push(`- ${srcLabel} **—${edge.relation}→** ${tgtLabel}`);
      }
      lines.push("");
    }

    return lines.join("\n");
  }

  /**
   * Render an arbitrary list of facts as Markdown-KV.
   * Unlike renderForInjection, this doesn't query the DB — it formats what you give it.
   */
  renderFactList(facts: Fact[], opts?: { showIds?: boolean }): string {
    const showIds = opts?.showIds ?? false;

    const lines: string[] = [
      "<!-- Project Memory — managed by project-memory extension -->",
      "",
    ];

    const sectionDescriptions: Record<string, string> = {
      Architecture: "_System structure, component relationships, key abstractions_",
      Decisions: "_Choices made and their rationale_",
      Constraints: "_Requirements, limitations, environment details_",
      "Known Issues": "_Bugs, flaky tests, workarounds_",
      "Patterns & Conventions": "_Code style, project conventions, common approaches_",
      Specs: "_Active specifications, acceptance criteria, and design contracts driving current work_",
    };

    // Group by section, maintaining SECTIONS order
    for (const section of SECTIONS) {
      const sectionFacts = facts.filter(f => f.section === section);
      if (sectionFacts.length === 0) continue;
      lines.push(`## ${section}`);
      lines.push(sectionDescriptions[section] ?? "");
      lines.push("");
      for (const f of sectionFacts) {
        const date = f.created_at.split("T")[0];
        lines.push(showIds ? `- [${f.id}] ${f.content} [${date}]` : `- ${f.content} [${date}]`);
      }
      lines.push("");
    }

    return lines.join("\n");
  }

  // ---------------------------------------------------------------------------
  // Mind management
  // ---------------------------------------------------------------------------

  /** Create a mind */
  createMind(name: string, description: string, opts?: { parent?: string; origin_type?: string; origin_path?: string; readonly?: boolean }): void {
    this.db.prepare(`
      INSERT INTO minds (name, description, status, origin_type, origin_path, readonly, parent, created_at)
      VALUES (?, ?, 'active', ?, ?, ?, ?, ?)
    `).run(
      name, description,
      opts?.origin_type ?? "local",
      opts?.origin_path ?? null,
      opts?.readonly ? 1 : 0,
      opts?.parent ?? null,
      new Date().toISOString(),
    );
  }

  /** Get a mind record */
  getMind(name: string): MindRecord | null {
    return this.db.prepare(`SELECT * FROM minds WHERE name = ?`).get(name) as MindRecord | null;
  }

  /** List all minds */
  listMinds(): (MindRecord & { factCount: number })[] {
    return this.db.prepare(`
      SELECT m.*, COALESCE(fc.count, 0) as factCount
      FROM minds m
      LEFT JOIN (
        SELECT mind, COUNT(*) as count FROM facts WHERE status = 'active' GROUP BY mind
      ) fc ON m.name = fc.mind
      ORDER BY CASE m.status WHEN 'active' THEN 0 WHEN 'refined' THEN 1 WHEN 'retired' THEN 2 END
    `).all() as (MindRecord & { factCount: number })[];
  }

  /** Update mind status */
  setMindStatus(name: string, status: MindRecord["status"]): void {
    this.db.prepare(`UPDATE minds SET status = ? WHERE name = ?`).run(status, name);
  }

  /** Delete a mind and all its facts */
  deleteMind(name: string): void {
    if (name === "default") throw new Error("Cannot delete the default mind");
    const tx = this.db.transaction(() => {
      this.db.prepare(`DELETE FROM facts WHERE mind = ?`).run(name);
      this.db.prepare(`DELETE FROM minds WHERE name = ?`).run(name);
    });
    tx();
  }

  /** Check if a mind exists */
  mindExists(name: string): boolean {
    return !!this.db.prepare(`SELECT 1 FROM minds WHERE name = ?`).get(name);
  }

  /** Check if a mind is readonly */
  isMindReadonly(name: string): boolean {
    const mind = this.getMind(name);
    return mind?.readonly === 1;
  }

  /** Fork a mind — copy all active facts to a new mind */
  /**
   * Fork a mind — create a child scope that inherits the parent's facts.
   *
   * Lightweight: creates the mind record with `parent` set but copies zero
   * facts, edges, or embeddings. Query methods (getActiveFacts, vector search)
   * automatically include parent facts via the parent chain. Only facts
   * explicitly stored in the child are scoped to it.
   *
   * On archive, `ingestMind` copies child-only facts back to the parent.
   */
  forkMind(sourceName: string, newName: string, description: string): void {
    this.createMind(newName, description, { parent: sourceName });
  }

  /** Ingest facts from one mind into another */
  ingestMind(sourceName: string, targetName: string): { factsIngested: number; duplicatesSkipped: number } {
    // Only ingest facts directly stored in the source mind, not inherited from parents.
    // Inherited facts already exist in the parent (which is typically the target).
    const sourceFacts = this.db.prepare(
      `SELECT * FROM facts WHERE mind = ? AND status = 'active' ORDER BY section, created_at`
    ).all(sourceName) as Fact[];
    let ingested = 0;
    let skipped = 0;

    const tx = this.db.transaction(() => {
      for (const fact of sourceFacts) {
        const result = this.storeFact({
          mind: targetName,
          section: fact.section as SectionName,
          content: fact.content,
          source: "ingest",
          reinforcement_count: fact.reinforcement_count,
        });
        if (result.duplicate) {
          skipped++;
        } else {
          ingested++;
        }
      }

      // Retire source if writable
      if (!this.isMindReadonly(sourceName)) {
        this.setMindStatus(sourceName, "retired");
      }
    });
    tx();

    return { factsIngested: ingested, duplicatesSkipped: skipped };
  }

  // ---------------------------------------------------------------------------
  // Active mind state (persisted in DB via a settings table or pragma)
  // ---------------------------------------------------------------------------

  /** Get/set active mind using a simple key-value in the DB */
  getActiveMind(): string | null {
    // Use a lightweight approach — store in a settings row
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT)
    `);
    const row = this.db.prepare(`SELECT value FROM settings WHERE key = 'active_mind'`).get();
    if (!row) return null;
    const name = row.value;
    if (name && this.mindExists(name)) return name;
    return null;
  }

  setActiveMind(name: string | null): void {
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT)
    `);
    this.db.prepare(`
      INSERT OR REPLACE INTO settings (key, value) VALUES ('active_mind', ?)
    `).run(name);
  }

  // ---------------------------------------------------------------------------
  // JSONL Export/Import — portable fact sync across machines
  // ---------------------------------------------------------------------------

  /**
   * Export all facts and edges to JSONL format.
   * Each line is a self-contained JSON object with type prefix.
   * Includes all statuses so the full history is portable.
   */
  exportToJsonl(): string {
    const lines: string[] = [];

    // Export minds (except default which is auto-created)
    const minds = this.listMinds();
    for (const mind of minds) {
      if (mind.name === "default") continue;
      lines.push(JSON.stringify({
        _type: "mind",
        name: mind.name,
        description: mind.description,
        status: mind.status,
        origin_type: mind.origin_type,
        created_at: mind.created_at,
      }));
    }

    // Export all active facts — deterministic: chronological with id tie-break
    const allFacts = this.db.prepare(
      `SELECT * FROM facts WHERE status = 'active' ORDER BY mind, section, created_at, id`
    ).all() as Fact[];

    for (const fact of allFacts) {
      lines.push(JSON.stringify({
        _type: "fact",
        id: fact.id,
        mind: fact.mind,
        section: fact.section,
        content: fact.content,
        status: fact.status,
        created_at: fact.created_at,
        source: fact.source,
        content_hash: fact.content_hash,
        supersedes: fact.supersedes,
      }));
    }

    // Export active edges — deterministic: chronological with id tie-break
    const allEdges = this.db.prepare(
      `SELECT * FROM edges WHERE status = 'active' ORDER BY created_at, id`
    ).all() as Edge[];

    for (const edge of allEdges) {
      lines.push(JSON.stringify({
        _type: "edge",
        id: edge.id,
        source_fact_id: edge.source_fact_id,
        target_fact_id: edge.target_fact_id,
        relation: edge.relation,
        description: edge.description,
        source_mind: edge.source_mind,
        target_mind: edge.target_mind,
      }));
    }

    // Export episodes — deterministic: chronological with id tie-break
    const allEpisodes = this.db.prepare(
      `SELECT * FROM episodes ORDER BY date, created_at, id`
    ).all() as Episode[];

    for (const ep of allEpisodes) {
      const factIds = this.getEpisodeFactIds(ep.id);
      lines.push(JSON.stringify({
        _type: "episode",
        id: ep.id,
        mind: ep.mind,
        title: ep.title,
        narrative: ep.narrative,
        date: ep.date,
        session_id: ep.session_id,
        created_at: ep.created_at,
        fact_ids: factIds,
      }));
    }

    return lines.join("\n") + "\n";
  }

  /**
   * Import from JSONL, merging with existing data.
   * Uses content_hash dedup for facts — existing facts get reinforced,
   * new facts get inserted. Edges dedup by source+target+relation.
   * Returns counts of what happened.
   */
  importFromJsonl(jsonl: string): { factsAdded: number; factsReinforced: number; edgesAdded: number; edgesReinforced: number; mindsCreated: number } {
    let factsAdded = 0;
    let factsReinforced = 0;
    let edgesAdded = 0;
    let edgesReinforced = 0;
    let mindsCreated = 0;

    // Map from imported fact ID → local fact ID (for edge remapping)
    const factIdMap = new Map<string, string>();

    // Pre-dedup: merge=union in git can produce multiple lines with the same id.
    // Legacy exports may differ in volatile scoring metadata
    // (reinforcement_count, last_reinforced); newer stable exports may be byte-identical
    // apart from line duplication. Prefer the record with stronger legacy metadata when
    // present, otherwise keep the first durable record encountered.
    const dedupedRecords: any[] = [];
    const seenById = new Map<string, number>(); // id → index in dedupedRecords
    for (const line of jsonl.split("\n")) {
      const trimmed = line.trim();
      if (!trimmed) continue;
      let record: any;
      try {
        record = JSON.parse(trimmed);
      } catch {
        continue;
      }
      const id = record.id;
      if (id && seenById.has(id)) {
        const idx = seenById.get(id)!;
        const existing = dedupedRecords[idx];
        // Keep higher reinforcement_count; tie-break on last_reinforced
        if ((record.reinforcement_count ?? 0) > (existing.reinforcement_count ?? 0) ||
            ((record.reinforcement_count ?? 0) === (existing.reinforcement_count ?? 0) &&
             (record.last_reinforced ?? "") > (existing.last_reinforced ?? ""))) {
          dedupedRecords[idx] = record;
        }
      } else {
        if (id) seenById.set(id, dedupedRecords.length);
        dedupedRecords.push(record);
      }
    }

    const tx = this.db.transaction(() => {
      for (const record of dedupedRecords) {

        switch (record._type) {
          case "mind": {
            if (!this.mindExists(record.name)) {
              this.createMind(record.name, record.description ?? "", {
                origin_type: record.origin_type ?? "local",
              });
              mindsCreated++;
            }
            break;
          }
          case "fact": {
            const mind = record.mind ?? "default";
            if (!this.mindExists(mind)) {
              this.createMind(mind, "", { origin_type: "local" });
              mindsCreated++;
            }

            // Dedup by content hash — check ALL statuses to avoid resurrecting
            // archived or superseded facts from stale JSONL snapshots.
            const hash = record.content_hash ?? contentHash(record.content);
            const existingAny = this.db.prepare(
              `SELECT id, status FROM facts WHERE mind = ? AND content_hash = ?`
            ).get(mind, hash) as { id: string; status: string } | undefined;

            if (existingAny) {
              if (existingAny.status === "active") {
                // Reinforce, take higher reinforcement count
                const existingFact = this.getFact(existingAny.id);
                if (existingFact && record.reinforcement_count > existingFact.reinforcement_count) {
                  this.db.prepare(`
                    UPDATE facts SET reinforcement_count = ?, last_reinforced = ?, confidence = 1.0
                    WHERE id = ?
                  `).run(record.reinforcement_count, record.last_reinforced ?? new Date().toISOString(), existingAny.id);
                } else {
                  this.reinforceFact(existingAny.id);
                }
                factsReinforced++;
              }
              // Archived/superseded facts: skip silently (don't resurrect)
              factIdMap.set(record.id, existingAny.id);
            } else {
              const id = nanoid();
              const now = new Date().toISOString();
              this.db.prepare(`
                INSERT INTO facts (id, mind, section, content, status, created_at, created_session,
                                   supersedes, source, content_hash, confidence, last_reinforced,
                                   reinforcement_count, decay_rate)
                VALUES (?, ?, ?, ?, 'active', ?, NULL, ?, ?, ?, ?, ?, ?, ?)
              `).run(
                id, mind, record.section, record.content,
                record.created_at ?? now,
                record.supersedes ?? null,
                record.source ?? "ingest",
                hash,
                record.confidence ?? 1.0,
                record.last_reinforced ?? now,
                record.reinforcement_count ?? 1,
                record.decay_rate ?? this.decayProfile.baseRate,
              );
              factIdMap.set(record.id, id);
              factsAdded++;
            }
            break;
          }
          case "edge": {
            // Remap fact IDs
            const sourceId = factIdMap.get(record.source_fact_id) ?? record.source_fact_id;
            const targetId = factIdMap.get(record.target_fact_id) ?? record.target_fact_id;

            // Verify both facts exist locally
            if (!this.getFact(sourceId) || !this.getFact(targetId)) continue;

            const result = this.storeEdge({
              sourceFact: sourceId,
              targetFact: targetId,
              relation: record.relation,
              description: record.description,
              sourceMind: record.source_mind,
              targetMind: record.target_mind,
            });

            if (result.duplicate) {
              edgesReinforced++;
            } else {
              edgesAdded++;
            }
            break;
          }
          case "episode": {
            // Import episode — preserve original ID for cross-machine dedup.
            // getEpisode checks by ID, so using record.id ensures re-import is idempotent.
            const existing = this.getEpisode(record.id);
            if (!existing) {
              const mind = record.mind ?? "default";
              if (!this.mindExists(mind)) {
                this.createMind(mind, "", { origin_type: "local" });
                mindsCreated++;
              }
              // Remap fact IDs
              const factIds = (record.fact_ids as string[] ?? [])
                .map((id: string) => factIdMap.get(id) ?? id)
                .filter((id: string) => !!this.getFact(id));

              this._storeEpisodeInner(record.id, {
                mind,
                title: record.title,
                narrative: record.narrative,
                date: record.date,
                sessionId: record.session_id ?? null,
                factIds,
                createdAt: record.created_at,
              });
            }
            break;
          }
        }
      }
    });

    tx();
    return { factsAdded, factsReinforced, edgesAdded, edgesReinforced, mindsCreated };
  }

  /**
   * Get the mtime of the database file, or null if it doesn't exist.
   */
  getDbMtime(): Date | null {
    try {
      const stat = fs.statSync(this.dbPath);
      return stat.mtime;
    } catch {
      return null;
    }
  }

  // ---------------------------------------------------------------------------
  // Vector Embeddings — Semantic Retrieval
  // ---------------------------------------------------------------------------

  /** Register an embedding model in the metadata table (idempotent). */
  registerEmbeddingModel(model: string, dims: number): void {
    const now = new Date().toISOString();
    this.db.prepare(`
      INSERT OR IGNORE INTO embedding_metadata (model_name, dims, inserted_at)
      VALUES (?, ?, ?)
    `).run(model, dims, now);
  }

  /** Return the active embedding model metadata, or null if no vectors stored. */
  getActiveEmbeddingModel(): { model_name: string; dims: number } | null {
    return this.db.prepare(
      `SELECT model_name, dims FROM embedding_metadata ORDER BY inserted_at DESC LIMIT 1`
    ).get() ?? null;
  }

  /** Store an embedding for a fact — also registers the model. */
  storeFactVector(factId: string, embedding: Float32Array, model: string): void {
    this.registerEmbeddingModel(model, embedding.length);
    const blob = vectorToBlob(embedding);
    const now = new Date().toISOString();
    this.db.prepare(`
      INSERT OR REPLACE INTO facts_vec (fact_id, embedding, model, dims, created_at, model_name)
      VALUES (?, ?, ?, ?, ?, ?)
    `).run(factId, blob, model, embedding.length, now, model);
  }

  /** Get embedding for a fact */
  getFactVector(factId: string): Float32Array | null {
    const row = this.db.prepare(
      `SELECT embedding FROM facts_vec WHERE fact_id = ?`
    ).get(factId);
    if (!row?.embedding) return null;
    return blobToVector(row.embedding as Buffer);
  }

  /** Check if a fact has a stored vector */
  hasFactVector(factId: string): boolean {
    return !!this.db.prepare(
      `SELECT 1 FROM facts_vec WHERE fact_id = ?`
    ).get(factId);
  }

  /** Get all fact IDs that are missing vectors */
  getFactsMissingVectors(mind: string): string[] {
    const rows = this.db.prepare(`
      SELECT f.id FROM facts f
      LEFT JOIN facts_vec v ON f.id = v.fact_id
      WHERE f.mind = ? AND f.status = 'active' AND v.fact_id IS NULL
    `).all(mind) as { id: string }[];
    return rows.map(r => r.id);
  }

  /** Count facts with vectors for a mind */
  countFactVectors(mind: string): number {
    const row = this.db.prepare(`
      SELECT COUNT(*) as count FROM facts_vec v
      JOIN facts f ON v.fact_id = f.id
      WHERE f.mind = ? AND f.status = 'active'
    `).get(mind);
    return row?.count ?? 0;
  }

  /**
   * Semantic search: find top-k active facts most similar to a query vector.
   * Returns facts with similarity scores, filtered by mind and min confidence.
   * Applies confidence decay and computes final score as similarity × confidence.
   *
   * Skips vectors with mismatched dimensions (e.g., from a different embedding model).
   */
  semanticSearch(
    queryVec: Float32Array,
    mind: string,
    opts?: { k?: number; minSimilarity?: number; section?: string },
  ): (Fact & { similarity: number; score: number })[] {
    const k = opts?.k ?? 10;
    const minSim = opts?.minSimilarity ?? 0.3;
    const queryDims = queryVec.length;

    // Get all active facts with vectors for this mind (including parent chain)
    const chain = this.resolveMindChain(mind);
    const placeholders = chain.map(() => "?").join(", ");
    let query = `
      SELECT f.*, v.embedding, v.dims FROM facts f
      JOIN facts_vec v ON f.id = v.fact_id
      WHERE f.mind IN (${placeholders}) AND f.status = 'active'
    `;
    const params: any[] = [...chain];

    if (opts?.section) {
      query += ` AND f.section = ?`;
      params.push(opts.section);
    }

    const rows = this.db.prepare(query).all(...params) as (Fact & { embedding: Buffer; dims: number })[];

    // Compute similarities
    const NO_DECAY_SECTIONS: readonly string[] = ["Specs"];
    const now = Date.now();
    const scored: (Fact & { similarity: number; score: number })[] = [];

    let dimMismatchCount = 0;

    for (const row of rows) {
      // Dimension mismatch: log warning instead of silently skipping.
      // This happens when the embedding model changes (e.g., 384-dim → 1024-dim).
      if (row.dims !== queryDims) {
        dimMismatchCount++;
        continue;
      }

      const factVec = blobToVector(row.embedding);
      const similarity = cosineSimilarity(queryVec, factVec);

      if (similarity < minSim) continue;

      // Apply confidence decay using the fact's stored decay profile (not the
      // store-wide default). This fixes the wrong-profile decay bug where a
      // "recent_work" fact was decayed with the "standard" profile.
      let confidence: number;
      if (NO_DECAY_SECTIONS.includes(row.section)) {
        confidence = 1.0;
      } else {
        // Use access reinforcement: effective last-active is max(last_reinforced, last_accessed)
        const lastReinforced = new Date(row.last_reinforced).getTime();
        const lastAccessed = row.last_accessed ? new Date(row.last_accessed).getTime() : 0;
        const effectiveLastActive = Math.max(lastReinforced, lastAccessed);
        const daysSince = (now - effectiveLastActive) / (1000 * 60 * 60 * 24);
        const profile = resolveDecayProfile(row.decay_profile);
        confidence = computeConfidence(daysSince, row.reinforcement_count, profile);
      }

      // Remove embedding from returned object
      const { embedding: _, dims: _d, ...fact } = row;
      scored.push({
        ...fact,
        confidence,
        similarity,
        score: similarity * confidence,
      });
    }

    if (dimMismatchCount > 0) {
      console.warn(
        `[project-memory] semanticSearch: ${dimMismatchCount} vectors skipped due to dimension mismatch ` +
        `(query=${queryDims}d, stored vectors have different dims). ` +
        `Re-embed with the current model to fix.`
      );
    }

    // Sort by combined score descending, return top-k
    scored.sort((a, b) => b.score - a.score);
    const results = scored.slice(0, k);

    // Access reinforcement: touch returned facts so their effective decay timer
    // resets. Fire-and-forget — don't block the search response.
    for (const fact of results) {
      try { this.touchFact(fact.id); } catch { /* non-critical */ }
    }

    return results;
  }

  /**
   * Hybrid search: combines FTS5 keyword search with embedding-based semantic search
   * via Reciprocal Rank Fusion (RRF). Produces better recall than either method alone:
   *   - FTS5 catches exact keyword matches (file paths, function names, identifiers)
   *   - Embeddings catch semantic matches (synonyms, paraphrases, conceptual similarity)
   *
   * When queryVec is null (embeddings unavailable), degrades to FTS5-only.
   * RRF formula: score(d) = Σ 1/(k + rank_in_list), where k=60 (standard constant).
   *
   * Returns facts scored by RRF rank, with similarity and confidence fields populated.
   */
  hybridSearch(
    queryText: string,
    queryVec: Float32Array | null,
    mind: string,
    opts?: { k?: number; minSimilarity?: number; section?: string; ftsK?: number; semanticK?: number },
  ): (Fact & { similarity: number; score: number })[] {
    const k = opts?.k ?? 15;
    const ftsK = opts?.ftsK ?? 20;
    const semanticK = opts?.semanticK ?? 20;
    const RRF_K = 60; // Standard RRF constant

    // --- FTS5 leg ---
    const ftsRanked: Map<string, number> = new Map(); // fact.id → rank (0-indexed)
    if (queryText.length > 2) {
      // Use OR mode for broader recall — AND is too restrictive for injection
      const ftsQuery = buildSafeFtsQuery(queryText, "OR");
      if (ftsQuery) {
        try {
          let query = `
            SELECT f.* FROM facts f
            JOIN facts_fts fts ON f.rowid = fts.rowid
            WHERE facts_fts MATCH ? AND f.mind = ? AND f.status = 'active'
          `;
          const params: any[] = [ftsQuery, mind];
          if (opts?.section) {
            query += ` AND f.section = ?`;
            params.push(opts.section);
          }
          query += ` ORDER BY rank LIMIT ?`;
          params.push(ftsK);
          const rows = this.db.prepare(query).all(...params) as Fact[];
          for (let i = 0; i < rows.length; i++) {
            ftsRanked.set(rows[i].id, i);
          }
        } catch {
          // FTS5 query syntax error (e.g., special characters) — skip FTS leg
        }
      }
    }

    // --- Embedding leg ---
    const semanticRanked: Map<string, { rank: number; similarity: number }> = new Map();
    if (queryVec) {
      const hits = this.semanticSearch(queryVec, mind, {
        k: semanticK,
        minSimilarity: opts?.minSimilarity ?? 0.3,
        section: opts?.section,
      });
      for (let i = 0; i < hits.length; i++) {
        semanticRanked.set(hits[i].id, { rank: i, similarity: hits[i].similarity });
      }
    }

    // --- RRF merge ---
    const allIds = new Set([...ftsRanked.keys(), ...semanticRanked.keys()]);
    const scored: { id: string; rrfScore: number; similarity: number }[] = [];

    for (const id of allIds) {
      let rrfScore = 0;
      let similarity = 0;

      const ftsRank = ftsRanked.get(id);
      if (ftsRank !== undefined) {
        rrfScore += 1 / (RRF_K + ftsRank);
      }

      const semHit = semanticRanked.get(id);
      if (semHit !== undefined) {
        rrfScore += 1 / (RRF_K + semHit.rank);
        similarity = semHit.similarity;
      }

      scored.push({ id, rrfScore, similarity });
    }

    scored.sort((a, b) => b.rrfScore - a.rrfScore);
    const topIds = scored.slice(0, k);

    // Hydrate facts with scores
    const results: (Fact & { similarity: number; score: number })[] = [];
    for (const { id, rrfScore, similarity } of topIds) {
      const fact = this.getFact(id);
      if (!fact || fact.status !== "active") continue;
      results.push({ ...fact, similarity, score: rrfScore });
    }

    // Access reinforcement on returned results
    for (const fact of results) {
      try { this.touchFact(fact.id); } catch { /* non-critical */ }
    }

    return results;
  }

  /**
   * Find facts similar to a given fact (for conflict detection).
   * Returns facts in the same section with high similarity but different content hash.
   * Skips vectors with mismatched dimensions.
   */
  findSimilarFacts(
    factContent: string,
    queryVec: Float32Array,
    mind: string,
    section: string,
    opts?: { threshold?: number; limit?: number },
  ): (Fact & { similarity: number })[] {
    const threshold = opts?.threshold ?? 0.8;
    const limit = opts?.limit ?? 5;
    const queryDims = queryVec.length;
    const contentHashVal = contentHash(factContent);

    const chain = this.resolveMindChain(mind);
    const placeholders = chain.map(() => "?").join(", ");
    const rows = this.db.prepare(`
      SELECT f.*, v.embedding, v.dims FROM facts f
      JOIN facts_vec v ON f.id = v.fact_id
      WHERE f.mind IN (${placeholders}) AND f.section = ? AND f.status = 'active'
        AND f.content_hash != ?
    `).all(...chain, section, contentHashVal) as (Fact & { embedding: Buffer; dims: number })[];

    const results: (Fact & { similarity: number })[] = [];

    for (const row of rows) {
      if (row.dims !== queryDims) continue;

      const factVec = blobToVector(row.embedding);
      const similarity = cosineSimilarity(queryVec, factVec);

      if (similarity >= threshold) {
        const { embedding: _, dims: _d, ...fact } = row;
        results.push({ ...fact, similarity });
      }
    }

    results.sort((a, b) => b.similarity - a.similarity);
    return results.slice(0, limit);
  }

  /**
   * Purge vectors with mismatched dimensions. Called when embedding model changes.
   * Returns number of vectors purged.
   */
  purgeStaleVectors(expectedDims: number): number {
    const result = this.db.prepare(
      `DELETE FROM facts_vec WHERE dims != ?`
    ).run(expectedDims);
    const episodeResult = this.db.prepare(
      `DELETE FROM episodes_vec WHERE dims != ?`
    ).run(expectedDims);
    return result.changes + episodeResult.changes;
  }

  // ---------------------------------------------------------------------------
  // Episodes — Session Narratives
  // ---------------------------------------------------------------------------

  /** Store an episode */
  storeEpisode(opts: {
    mind: string;
    title: string;
    narrative: string;
    date: string;
    sessionId?: string;
    factIds?: string[];
  }): string {
    const id = nanoid();
    const tx = this.db.transaction(() => {
      this._storeEpisodeInner(id, opts);
    });
    tx();
    return id;
  }

  /**
   * Inner episode insert — no transaction wrapper.
   * Safe to call inside an existing transaction (e.g. importFromJsonl).
   */
  private _storeEpisodeInner(id: string, opts: {
    mind: string;
    title: string;
    narrative: string;
    date: string;
    sessionId?: string | null;
    factIds?: string[];
    createdAt?: string;
  }): void {
    const now = opts.createdAt ?? new Date().toISOString();
    this.db.prepare(`
      INSERT INTO episodes (id, mind, title, narrative, date, session_id, created_at)
      VALUES (?, ?, ?, ?, ?, ?, ?)
    `).run(id, opts.mind, opts.title, opts.narrative, opts.date, opts.sessionId ?? null, now);

    if (opts.factIds?.length) {
      const stmt = this.db.prepare(
        `INSERT OR IGNORE INTO episode_facts (episode_id, fact_id) VALUES (?, ?)`
      );
      for (const factId of opts.factIds) {
        stmt.run(id, factId);
      }
    }
  }

  /** Get episodes for a mind, ordered by date descending */
  getEpisodes(mind: string, limit?: number): Episode[] {
    const chain = this.resolveMindChain(mind);
    const placeholders = chain.map(() => "?").join(", ");
    const sql = `SELECT * FROM episodes WHERE mind IN (${placeholders}) ORDER BY date DESC` +
      (limit ? ` LIMIT ${limit}` : "");
    return this.db.prepare(sql).all(...chain) as Episode[];
  }

  /** Get a single episode by ID */
  getEpisode(id: string): Episode | null {
    return this.db.prepare(`SELECT * FROM episodes WHERE id = ?`).get(id) as Episode | null;
  }

  /** Get fact IDs linked to an episode */
  getEpisodeFactIds(episodeId: string): string[] {
    const rows = this.db.prepare(
      `SELECT fact_id FROM episode_facts WHERE episode_id = ?`
    ).all(episodeId) as { fact_id: string }[];
    return rows.map(r => r.fact_id);
  }

  /** Get episodes that reference a specific fact */
  getEpisodesForFact(factId: string): Episode[] {
    return this.db.prepare(`
      SELECT e.* FROM episodes e
      JOIN episode_facts ef ON e.id = ef.episode_id
      WHERE ef.fact_id = ?
      ORDER BY e.date DESC
    `).all(factId) as Episode[];
  }

  /** Store an episode embedding */
  storeEpisodeVector(episodeId: string, embedding: Float32Array, model: string): void {
    const blob = vectorToBlob(embedding);
    const now = new Date().toISOString();
    this.db.prepare(`
      INSERT OR REPLACE INTO episodes_vec (episode_id, embedding, model, dims, created_at)
      VALUES (?, ?, ?, ?, ?)
    `).run(episodeId, blob, model, embedding.length, now);
  }

  /** Semantic search over episodes. Skips vectors with mismatched dimensions. */
  semanticSearchEpisodes(
    queryVec: Float32Array,
    mind: string,
    opts?: { k?: number; minSimilarity?: number },
  ): (Episode & { similarity: number })[] {
    const k = opts?.k ?? 5;
    const minSim = opts?.minSimilarity ?? 0.3;
    const queryDims = queryVec.length;

    const chain = this.resolveMindChain(mind);
    const ePlaceholders = chain.map(() => "?").join(", ");
    const rows = this.db.prepare(`
      SELECT e.*, v.embedding, v.dims FROM episodes e
      JOIN episodes_vec v ON e.id = v.episode_id
      WHERE e.mind IN (${ePlaceholders})
    `).all(...chain) as (Episode & { embedding: Buffer; dims: number })[];

    const results: (Episode & { similarity: number })[] = [];

    for (const row of rows) {
      if (row.dims !== queryDims) continue;

      const vec = blobToVector(row.embedding);
      const similarity = cosineSimilarity(queryVec, vec);

      if (similarity >= minSim) {
        const { embedding: _, dims: _d, ...episode } = row;
        results.push({ ...episode, similarity });
      }
    }

    results.sort((a, b) => b.similarity - a.similarity);
    return results.slice(0, k);
  }

  /** Count episodes for a mind */
  countEpisodes(mind: string): number {
    const row = this.db.prepare(
      `SELECT COUNT(*) as count FROM episodes WHERE mind = ?`
    ).get(mind);
    return row?.count ?? 0;
  }

  // ---------------------------------------------------------------------------
  // Lifecycle
  // ---------------------------------------------------------------------------

  close(): void {
    this.db.close();
  }

  getDbPath(): string {
    return this.dbPath;
  }
}

// --- Extraction action types ---

export interface ExtractionAction {
  type: "observe" | "reinforce" | "supersede" | "archive" | "connect";
  id?: string;
  section?: SectionName;
  content?: string;
  // connect-specific fields
  source?: string;
  target?: string;
  relation?: string;
  description?: string;
}

/**
 * Parse extraction agent output (JSONL) into actions.
 * Tolerant — skips malformed lines.
 */
export function parseExtractionOutput(output: string): ExtractionAction[] {
  const actions: ExtractionAction[] = [];
  for (const line of output.split("\n")) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("//") || trimmed.startsWith("#")) continue;
    try {
      const parsed = JSON.parse(trimmed);
      if (parsed.type && typeof parsed.type === "string") {
        actions.push(parsed as ExtractionAction);
      } else if (parsed.action) {
        // Accept {action: "observe"} as alias for {type: "observe"}
        actions.push({ ...parsed, type: parsed.action } as ExtractionAction);
      }
    } catch {
      // Skip malformed lines — best effort
      continue;
    }
  }
  return actions;
}
