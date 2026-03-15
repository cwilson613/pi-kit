/**
 * Tests for vector embeddings, semantic search, episode storage,
 * conflict detection, dimension mismatch handling, schema versioning,
 * and JSONL export/import of episodes.
 */

import { describe, it, beforeEach, afterEach } from "node:test";
import * as assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import * as os from "node:os";
import * as crypto from "node:crypto";
import { FactStore } from "./factstore.ts";
import { cosineSimilarity, vectorToBlob, blobToVector } from "./embeddings.ts";

function tmpDir(): string {
  const dir = path.join(os.tmpdir(), `vec-test-${crypto.randomBytes(8).toString("hex")}`);
  fs.mkdirSync(dir, { recursive: true });
  return dir;
}

/** Generate a random unit vector of given dimensions */
function randomVec(dims: number): Float32Array {
  const v = new Float32Array(dims);
  let norm = 0;
  for (let i = 0; i < dims; i++) {
    v[i] = Math.random() * 2 - 1;
    norm += v[i] * v[i];
  }
  norm = Math.sqrt(norm);
  for (let i = 0; i < dims; i++) v[i] /= norm;
  return v;
}

/** Generate a vector similar to `base` with some noise */
function similarVec(base: Float32Array, noise: number = 0.1): Float32Array {
  const v = new Float32Array(base.length);
  for (let i = 0; i < base.length; i++) {
    v[i] = base[i] + (Math.random() * 2 - 1) * noise;
  }
  // Normalize
  let norm = 0;
  for (let i = 0; i < v.length; i++) norm += v[i] * v[i];
  norm = Math.sqrt(norm);
  for (let i = 0; i < v.length; i++) v[i] /= norm;
  return v;
}

describe("Fact Vector Storage", () => {
  let dir: string;
  let store: FactStore;

  beforeEach(() => {
    dir = tmpDir();
    store = new FactStore(dir);
  });

  afterEach(() => {
    store.close();
    fs.rmSync(dir, { recursive: true, force: true });
  });

  it("stores and retrieves a fact vector", () => {
    const { id } = store.storeFact({ section: "Architecture", content: "Test fact" });
    const vec = randomVec(1024);
    store.storeFactVector(id, vec, "qwen3-embedding:0.6b");

    const retrieved = store.getFactVector(id);
    assert.ok(retrieved);
    assert.equal(retrieved.length, 1024);
    // Verify exact values preserved
    for (let i = 0; i < 10; i++) {
      assert.ok(Math.abs(retrieved[i] - vec[i]) < 1e-6);
    }
  });

  it("hasFactVector returns correct status", () => {
    const { id } = store.storeFact({ section: "Architecture", content: "Test" });
    assert.equal(store.hasFactVector(id), false);

    store.storeFactVector(id, randomVec(1024), "test-model");
    assert.equal(store.hasFactVector(id), true);
  });

  it("getFactsMissingVectors finds unembedded facts", () => {
    const f1 = store.storeFact({ section: "Architecture", content: "Fact 1" });
    const f2 = store.storeFact({ section: "Architecture", content: "Fact 2" });
    store.storeFact({ section: "Architecture", content: "Fact 3" });

    store.storeFactVector(f1.id, randomVec(1024), "test-model");
    store.storeFactVector(f2.id, randomVec(1024), "test-model");

    const missing = store.getFactsMissingVectors("default");
    assert.equal(missing.length, 1);
  });

  it("countFactVectors counts only active facts", () => {
    const f1 = store.storeFact({ section: "Architecture", content: "Active fact" });
    const f2 = store.storeFact({ section: "Architecture", content: "Will archive" });
    store.storeFactVector(f1.id, randomVec(1024), "test-model");
    store.storeFactVector(f2.id, randomVec(1024), "test-model");

    assert.equal(store.countFactVectors("default"), 2);

    store.archiveFact(f2.id);
    // archiveFact now cleans up the vector too
    assert.equal(store.countFactVectors("default"), 1);
  });

  it("archiveFact cleans up orphaned vectors", () => {
    const { id } = store.storeFact({ section: "Architecture", content: "Test" });
    store.storeFactVector(id, randomVec(1024), "test-model");
    assert.equal(store.hasFactVector(id), true);

    store.archiveFact(id);
    assert.equal(store.hasFactVector(id), false);
  });

  it("storeFactVector overwrites on re-embed (INSERT OR REPLACE)", () => {
    const { id } = store.storeFact({ section: "Architecture", content: "Test" });
    const vec1 = randomVec(1024);
    const vec2 = randomVec(2048);
    store.storeFactVector(id, vec1, "model-a");
    store.storeFactVector(id, vec2, "model-b");

    const retrieved = store.getFactVector(id);
    assert.ok(retrieved);
    assert.equal(retrieved.length, 2048);
  });
});

describe("Semantic Search", () => {
  let dir: string;
  let store: FactStore;
  const DIMS = 64; // Small dims for test speed

  beforeEach(() => {
    dir = tmpDir();
    store = new FactStore(dir);
  });

  afterEach(() => {
    store.close();
    fs.rmSync(dir, { recursive: true, force: true });
  });

  it("returns facts ranked by cosine similarity", () => {
    const baseVec = randomVec(DIMS);
    const closeVec = similarVec(baseVec, 0.05);
    const farVec = randomVec(DIMS);

    const f1 = store.storeFact({ section: "Architecture", content: "Close fact" });
    const f2 = store.storeFact({ section: "Architecture", content: "Far fact" });
    store.storeFactVector(f1.id, closeVec, "test");
    store.storeFactVector(f2.id, farVec, "test");

    const results = store.semanticSearch(baseVec, "default", { k: 10, minSimilarity: 0 });
    assert.ok(results.length >= 1);
    // Close fact should rank higher
    if (results.length >= 2) {
      assert.ok(results[0].similarity >= results[1].similarity);
    }
  });

  it("respects minSimilarity threshold", () => {
    const query = randomVec(DIMS);
    const orthogonal = new Float32Array(DIMS);
    // Create a vector orthogonal-ish to query
    for (let i = 0; i < DIMS; i++) orthogonal[i] = (i % 2 === 0 ? 1 : -1) * query[i];

    const f1 = store.storeFact({ section: "Architecture", content: "Orthogonal" });
    store.storeFactVector(f1.id, orthogonal, "test");

    const results = store.semanticSearch(query, "default", { minSimilarity: 0.99 });
    assert.equal(results.length, 0);
  });

  it("respects k limit", () => {
    const baseVec = randomVec(DIMS);
    for (let i = 0; i < 10; i++) {
      const f = store.storeFact({ section: "Architecture", content: `Fact ${i}` });
      store.storeFactVector(f.id, similarVec(baseVec, 0.05), "test");
    }

    const results = store.semanticSearch(baseVec, "default", { k: 3, minSimilarity: 0 });
    assert.equal(results.length, 3);
  });

  it("filters by section", () => {
    const vec = randomVec(DIMS);
    const f1 = store.storeFact({ section: "Architecture", content: "Arch fact" });
    const f2 = store.storeFact({ section: "Decisions", content: "Decision fact" });
    store.storeFactVector(f1.id, similarVec(vec, 0.05), "test");
    store.storeFactVector(f2.id, similarVec(vec, 0.05), "test");

    const results = store.semanticSearch(vec, "default", {
      section: "Architecture",
      minSimilarity: 0,
    });
    assert.equal(results.length, 1);
    assert.equal(results[0].section, "Architecture");
  });

  it("skips vectors with mismatched dimensions", () => {
    const query = randomVec(DIMS);
    const f1 = store.storeFact({ section: "Architecture", content: "1024-dim fact" });
    const f2 = store.storeFact({ section: "Architecture", content: "64-dim fact" });
    store.storeFactVector(f1.id, randomVec(1024), "big-model");
    // Use similar vector so cosine similarity is positive
    store.storeFactVector(f2.id, similarVec(query, 0.05), "small-model");

    // Query with 64-dim vector — should only match f2 (1024-dim skipped)
    const results = store.semanticSearch(query, "default", { minSimilarity: 0.5 });
    assert.equal(results.length, 1);
    assert.equal(results[0].content, "64-dim fact");
  });

  it("excludes archived facts", () => {
    const vec = randomVec(DIMS);
    const f1 = store.storeFact({ section: "Architecture", content: "Active" });
    const f2 = store.storeFact({ section: "Architecture", content: "Archived" });
    store.storeFactVector(f1.id, similarVec(vec, 0.01), "test");
    store.storeFactVector(f2.id, similarVec(vec, 0.01), "test");
    store.archiveFact(f2.id);

    const results = store.semanticSearch(vec, "default", { minSimilarity: 0 });
    assert.equal(results.length, 1);
    assert.equal(results[0].content, "Active");
  });

  it("returns score as similarity × confidence", () => {
    const vec = randomVec(DIMS);
    const f = store.storeFact({ section: "Architecture", content: "Test" });
    store.storeFactVector(f.id, vec, "test");

    const results = store.semanticSearch(vec, "default", { minSimilarity: 0 });
    assert.equal(results.length, 1);
    // Fresh fact: confidence ≈ 1.0, similarity ≈ 1.0
    assert.ok(results[0].score > 0.9);
    assert.ok(Math.abs(results[0].score - results[0].similarity * results[0].confidence) < 1e-6);
  });
});

describe("findSimilarFacts (conflict detection)", () => {
  let dir: string;
  let store: FactStore;
  const DIMS = 64;

  beforeEach(() => {
    dir = tmpDir();
    store = new FactStore(dir);
  });

  afterEach(() => {
    store.close();
    fs.rmSync(dir, { recursive: true, force: true });
  });

  it("finds facts with high similarity in same section", () => {
    const baseVec = randomVec(DIMS);
    const f1 = store.storeFact({ section: "Architecture", content: "System uses PostgreSQL 15" });
    store.storeFactVector(f1.id, baseVec, "test");

    const queryVec = similarVec(baseVec, 0.02); // Very similar
    const results = store.findSimilarFacts(
      "System uses PostgreSQL 16",
      queryVec,
      "default",
      "Architecture",
      { threshold: 0.8 },
    );
    assert.ok(results.length >= 1);
    assert.ok(results[0].similarity >= 0.8);
  });

  it("excludes facts with matching content hash", () => {
    const vec = randomVec(DIMS);
    const f1 = store.storeFact({ section: "Architecture", content: "Exact content" });
    store.storeFactVector(f1.id, vec, "test");

    // Search for the SAME content — should find nothing (same hash excluded)
    const results = store.findSimilarFacts("Exact content", vec, "default", "Architecture");
    assert.equal(results.length, 0);
  });

  it("skips vectors with mismatched dimensions", () => {
    const f1 = store.storeFact({ section: "Architecture", content: "Old fact" });
    store.storeFactVector(f1.id, randomVec(1024), "big-model");

    const queryVec = randomVec(DIMS); // Different dims
    const results = store.findSimilarFacts(
      "New fact", queryVec, "default", "Architecture", { threshold: 0 },
    );
    assert.equal(results.length, 0);
  });

  it("respects threshold", () => {
    const baseVec = randomVec(DIMS);
    const f1 = store.storeFact({ section: "Architecture", content: "Somewhat related" });
    store.storeFactVector(f1.id, randomVec(DIMS), "test"); // Random = low similarity

    const results = store.findSimilarFacts(
      "Different content", baseVec, "default", "Architecture", { threshold: 0.99 },
    );
    assert.equal(results.length, 0);
  });

  it("limits results", () => {
    const baseVec = randomVec(DIMS);
    for (let i = 0; i < 10; i++) {
      const f = store.storeFact({ section: "Architecture", content: `Variant ${i}` });
      store.storeFactVector(f.id, similarVec(baseVec, 0.02), "test");
    }

    const results = store.findSimilarFacts(
      "New variant", baseVec, "default", "Architecture", { threshold: 0.5, limit: 3 },
    );
    assert.ok(results.length <= 3);
  });
});

describe("purgeStaleVectors", () => {
  let dir: string;
  let store: FactStore;

  beforeEach(() => {
    dir = tmpDir();
    store = new FactStore(dir);
  });

  afterEach(() => {
    store.close();
    fs.rmSync(dir, { recursive: true, force: true });
  });

  it("purges vectors with wrong dimensions", () => {
    const f1 = store.storeFact({ section: "Architecture", content: "Fact 1" });
    const f2 = store.storeFact({ section: "Architecture", content: "Fact 2" });
    const f3 = store.storeFact({ section: "Architecture", content: "Fact 3" });

    store.storeFactVector(f1.id, randomVec(1024), "old-model");
    store.storeFactVector(f2.id, randomVec(1024), "old-model");
    store.storeFactVector(f3.id, randomVec(2048), "new-model");

    const purged = store.purgeStaleVectors(2048);
    assert.equal(purged, 2); // Two 1024-dim vectors purged

    assert.equal(store.hasFactVector(f1.id), false);
    assert.equal(store.hasFactVector(f2.id), false);
    assert.equal(store.hasFactVector(f3.id), true);
  });

  it("also purges episode vectors with wrong dimensions", () => {
    const epId = store.storeEpisode({
      mind: "default",
      title: "Test Episode",
      narrative: "Something happened",
      date: "2026-03-01",
    });
    store.storeEpisodeVector(epId, randomVec(1024), "old-model");

    const purged = store.purgeStaleVectors(2048);
    assert.equal(purged, 1);
  });

  it("returns 0 when all vectors match expected dims", () => {
    const f = store.storeFact({ section: "Architecture", content: "Test" });
    store.storeFactVector(f.id, randomVec(1024), "model");

    const purged = store.purgeStaleVectors(1024);
    assert.equal(purged, 0);
  });
});

describe("Episodes", () => {
  let dir: string;
  let store: FactStore;

  beforeEach(() => {
    dir = tmpDir();
    store = new FactStore(dir);
  });

  afterEach(() => {
    store.close();
    fs.rmSync(dir, { recursive: true, force: true });
  });

  it("stores and retrieves an episode", () => {
    const id = store.storeEpisode({
      mind: "default",
      title: "Migrated auth to OIDC",
      narrative: "Replaced JWT with OIDC provider. Updated middleware and tests.",
      date: "2026-03-01",
      sessionId: "session-abc",
    });

    const ep = store.getEpisode(id);
    assert.ok(ep);
    assert.equal(ep.title, "Migrated auth to OIDC");
    assert.equal(ep.narrative, "Replaced JWT with OIDC provider. Updated middleware and tests.");
    assert.equal(ep.date, "2026-03-01");
    assert.equal(ep.mind, "default");
    assert.equal(ep.session_id, "session-abc");
  });

  it("links episodes to facts", () => {
    const f1 = store.storeFact({ section: "Decisions", content: "Chose OIDC" });
    const f2 = store.storeFact({ section: "Architecture", content: "Auth middleware" });

    const epId = store.storeEpisode({
      mind: "default",
      title: "Auth migration",
      narrative: "Migrated auth system",
      date: "2026-03-01",
      factIds: [f1.id, f2.id],
    });

    const factIds = store.getEpisodeFactIds(epId);
    assert.equal(factIds.length, 2);
    assert.ok(factIds.includes(f1.id));
    assert.ok(factIds.includes(f2.id));
  });

  it("getEpisodesForFact returns episodes linked to a fact", () => {
    const f = store.storeFact({ section: "Architecture", content: "Key fact" });
    store.storeEpisode({
      mind: "default",
      title: "Episode 1",
      narrative: "First session",
      date: "2026-03-01",
      factIds: [f.id],
    });
    store.storeEpisode({
      mind: "default",
      title: "Episode 2",
      narrative: "Second session",
      date: "2026-03-02",
      factIds: [f.id],
    });

    const episodes = store.getEpisodesForFact(f.id);
    assert.equal(episodes.length, 2);
  });

  it("getEpisodes returns ordered by date descending", () => {
    store.storeEpisode({ mind: "default", title: "Old", narrative: "old", date: "2026-01-01" });
    store.storeEpisode({ mind: "default", title: "New", narrative: "new", date: "2026-03-01" });
    store.storeEpisode({ mind: "default", title: "Mid", narrative: "mid", date: "2026-02-01" });

    const episodes = store.getEpisodes("default");
    assert.equal(episodes.length, 3);
    assert.equal(episodes[0].title, "New");
    assert.equal(episodes[1].title, "Mid");
    assert.equal(episodes[2].title, "Old");
  });

  it("getEpisodes respects limit", () => {
    for (let i = 0; i < 10; i++) {
      store.storeEpisode({ mind: "default", title: `Ep ${i}`, narrative: "n", date: `2026-03-${String(i + 1).padStart(2, "0")}` });
    }

    const episodes = store.getEpisodes("default", 3);
    assert.equal(episodes.length, 3);
  });

  it("countEpisodes works", () => {
    assert.equal(store.countEpisodes("default"), 0);
    store.storeEpisode({ mind: "default", title: "Ep", narrative: "n", date: "2026-03-01" });
    assert.equal(store.countEpisodes("default"), 1);
  });

  it("stores and searches episode vectors", () => {
    const DIMS = 64;
    const baseVec = randomVec(DIMS);

    const ep1 = store.storeEpisode({ mind: "default", title: "Relevant", narrative: "relevant", date: "2026-03-01" });
    const ep2 = store.storeEpisode({ mind: "default", title: "Irrelevant", narrative: "irrelevant", date: "2026-03-02" });

    store.storeEpisodeVector(ep1, similarVec(baseVec, 0.02), "test");
    store.storeEpisodeVector(ep2, randomVec(DIMS), "test");

    const results = store.semanticSearchEpisodes(baseVec, "default", { k: 5, minSimilarity: 0.5 });
    assert.ok(results.length >= 1);
    assert.equal(results[0].title, "Relevant");
  });

  it("semanticSearchEpisodes skips mismatched dimensions", () => {
    const ep = store.storeEpisode({ mind: "default", title: "Test", narrative: "n", date: "2026-03-01" });
    store.storeEpisodeVector(ep, randomVec(1024), "big-model");

    const results = store.semanticSearchEpisodes(randomVec(64), "default", { minSimilarity: 0 });
    assert.equal(results.length, 0);
  });
});

describe("renderFactList", () => {
  let dir: string;
  let store: FactStore;

  beforeEach(() => {
    dir = tmpDir();
    store = new FactStore(dir);
  });

  afterEach(() => {
    store.close();
    fs.rmSync(dir, { recursive: true, force: true });
  });

  it("renders facts grouped by section in SECTIONS order", () => {
    const f1 = store.storeFact({ section: "Decisions", content: "Chose SQLite" });
    const f2 = store.storeFact({ section: "Architecture", content: "Uses TypeScript" });

    const facts = [f1, f2].map(r => store.getFact(r.id)!);
    const rendered = store.renderFactList(facts);

    // Architecture should come before Decisions in SECTIONS order
    const archIdx = rendered.indexOf("## Architecture");
    const decIdx = rendered.indexOf("## Decisions");
    assert.ok(archIdx < decIdx, "Architecture should precede Decisions");
    assert.ok(rendered.includes("- Uses TypeScript"));
    assert.ok(rendered.includes("- Chose SQLite"));
  });

  it("shows fact IDs when showIds is true", () => {
    const { id } = store.storeFact({ section: "Architecture", content: "Test fact" });
    const fact = store.getFact(id)!;
    const rendered = store.renderFactList([fact], { showIds: true });
    assert.ok(rendered.includes(`[${id}]`));
  });

  it("hides fact IDs by default", () => {
    const { id } = store.storeFact({ section: "Architecture", content: "Test fact" });
    const fact = store.getFact(id)!;
    const rendered = store.renderFactList([fact]);
    assert.ok(!rendered.includes(`[${id}]`));
  });

  it("skips empty sections", () => {
    const { id } = store.storeFact({ section: "Architecture", content: "Only arch" });
    const fact = store.getFact(id)!;
    const rendered = store.renderFactList([fact]);
    assert.ok(rendered.includes("## Architecture"));
    assert.ok(!rendered.includes("## Decisions"));
    assert.ok(!rendered.includes("## Constraints"));
  });
});

describe("JSONL Episode Export/Import", () => {
  let dir: string;
  let store: FactStore;

  beforeEach(() => {
    dir = tmpDir();
    store = new FactStore(dir);
  });

  afterEach(() => {
    store.close();
    fs.rmSync(dir, { recursive: true, force: true });
  });

  it("exports episodes in JSONL", () => {
    const f = store.storeFact({ section: "Architecture", content: "Test fact" });
    store.storeEpisode({
      mind: "default",
      title: "Test Episode",
      narrative: "Something happened",
      date: "2026-03-01",
      factIds: [f.id],
    });

    const jsonl = store.exportToJsonl();
    const lines = jsonl.trim().split("\n").map(l => JSON.parse(l));

    const fact = lines.find(l => l._type === "fact");
    assert.ok(fact, "expected fact line in export");
    assert.ok(!("confidence" in fact));
    assert.ok(!("reinforcement_count" in fact));
    assert.ok(!("last_reinforced" in fact));
    assert.ok(!("decay_rate" in fact));

    const edge = lines.find(l => l._type === "edge");
    assert.equal(edge, undefined, "no edge expected in this export fixture");

    const episodes = lines.filter(l => l._type === "episode");
    assert.equal(episodes.length, 1);
    assert.equal(episodes[0].title, "Test Episode");
    assert.equal(episodes[0].narrative, "Something happened");
    assert.equal(episodes[0].date, "2026-03-01");
    assert.ok(Array.isArray(episodes[0].fact_ids));
    assert.equal(episodes[0].fact_ids.length, 1);
  });

  it("imports episodes from JSONL into a fresh store", () => {
    // Set up source
    const f = store.storeFact({ section: "Architecture", content: "Imported fact" });
    store.storeEpisode({
      mind: "default",
      title: "Imported Episode",
      narrative: "Narrative text",
      date: "2026-03-02",
      factIds: [f.id],
    });

    const jsonl = store.exportToJsonl();

    // Import into fresh store (close original later in afterEach)
    const dir2 = tmpDir();
    const store2 = new FactStore(dir2);
    const result = store2.importFromJsonl(jsonl);

    assert.ok(result.factsAdded >= 1);
    const episodes = store2.getEpisodes("default");
    assert.equal(episodes.length, 1);
    assert.equal(episodes[0].title, "Imported Episode");

    store2.close();
    fs.rmSync(dir2, { recursive: true, force: true });
  });

  it("does not duplicate episodes on re-import", () => {
    store.storeEpisode({
      mind: "default",
      title: "Episode",
      narrative: "Text",
      date: "2026-03-01",
    });

    const jsonl = store.exportToJsonl();

    // Import twice — should be idempotent (import preserves original episode ID)
    store.importFromJsonl(jsonl);
    store.importFromJsonl(jsonl);

    const episodes = store.getEpisodes("default");
    assert.equal(episodes.length, 1, `Expected 1 episode, got ${episodes.length}`);
  });
});

describe("Schema Versioning", () => {
  let dir: string;

  beforeEach(() => {
    dir = tmpDir();
  });

  afterEach(() => {
    fs.rmSync(dir, { recursive: true, force: true });
  });

  it("creates schema_version table on fresh database", () => {
    const store = new FactStore(dir);
    const version = (store as any).getSchemaVersion();
    assert.equal(version, FactStore.SCHEMA_VERSION);
    store.close();
  });

  it("SCHEMA_VERSION is at least 2", () => {
    assert.ok(FactStore.SCHEMA_VERSION >= 2);
  });

  it("creates vector and episode tables via migration", () => {
    const store = new FactStore(dir);
    // These tables should exist after migration
    const f = store.storeFact({ section: "Architecture", content: "Test" });
    store.storeFactVector(f.id, randomVec(64), "test-model");
    assert.equal(store.hasFactVector(f.id), true);

    const epId = store.storeEpisode({
      mind: "default",
      title: "Ep",
      narrative: "n",
      date: "2026-03-01",
    });
    assert.ok(store.getEpisode(epId));

    store.close();
  });

  it("re-opening existing database does not re-run migrations", () => {
    const store1 = new FactStore(dir);
    store1.storeFact({ section: "Architecture", content: "Persist me" });
    store1.close();

    // Re-open — should not error and facts should persist
    const store2 = new FactStore(dir);
    assert.equal(store2.countActiveFacts("default"), 1);
    assert.equal((store2 as any).getSchemaVersion(), FactStore.SCHEMA_VERSION);
    store2.close();
  });
});
