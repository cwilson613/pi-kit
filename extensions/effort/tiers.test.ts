/**
 * Tests for effort/tiers — tier configuration and name parsing.
 */

import { describe, it } from "node:test";
import * as assert from "node:assert/strict";
import { tierConfig, parseTierName, DEFAULT_EFFORT_LEVEL, TIER_NAMES, MIN_LEVEL, MAX_LEVEL } from "./tiers.ts";
import { EFFORT_NAMES } from "./types.ts";
import type { EffortConfig, EffortLevel } from "./types.ts";

// ─── tierConfig ─────────────────────────────────────────────────────────────

describe("tierConfig", () => {
  it("Servitor tier (1) is fully local", () => {
    const c = tierConfig(1);
    assert.equal(c.level, 1);
    assert.equal(c.name, "Servitor");
    assert.equal(c.driver, "local");
    assert.equal(c.thinking, "off");
    assert.equal(c.extraction, "local");
    assert.equal(c.compaction, "local");
    assert.equal(c.cleavePreferLocal, true);
    assert.equal(c.cleaveFloor, "local");
    assert.equal(c.reviewModel, "local");
  });

  it("Average tier (2) is local driver with scope-based cleave (local bias)", () => {
    const c = tierConfig(2);
    assert.equal(c.level, 2);
    assert.equal(c.name, "Average");
    assert.equal(c.driver, "local");
    assert.equal(c.thinking, "minimal");
    assert.equal(c.extraction, "local");
    assert.equal(c.compaction, "local");
    assert.equal(c.cleavePreferLocal, false);
    assert.equal(c.cleaveFloor, "local");
    assert.equal(c.reviewModel, "local");
  });

  it("Servitor and Average differ behaviorally", () => {
    const s = tierConfig(1);
    const a = tierConfig(2);
    // Average has scope-based cleave (not forced-local) and minimal thinking
    assert.notEqual(s.thinking, a.thinking, "thinking should differ");
    assert.notEqual(s.cleavePreferLocal, a.cleavePreferLocal, "cleavePreferLocal should differ");
  });

  it("Substantial tier (3) is the daily driver", () => {
    const c = tierConfig(3);
    assert.equal(c.level, 3);
    assert.equal(c.name, "Substantial");
    assert.equal(c.driver, "sonnet");
    assert.equal(c.thinking, "low");
    assert.equal(c.extraction, "local");
    assert.equal(c.compaction, "local");
    assert.equal(c.cleavePreferLocal, false);
    assert.equal(c.cleaveFloor, "local");
    assert.equal(c.reviewModel, "sonnet");
  });

  it("Ruthless tier (4) has sonnet + medium thinking, local background", () => {
    const c = tierConfig(4);
    assert.equal(c.level, 4);
    assert.equal(c.name, "Ruthless");
    assert.equal(c.driver, "sonnet");
    assert.equal(c.thinking, "medium");
    assert.equal(c.extraction, "local");
    assert.equal(c.compaction, "local");
    assert.equal(c.cleavePreferLocal, false);
    assert.equal(c.cleaveFloor, "local");
    assert.equal(c.reviewModel, "sonnet");
  });

  it("Lethal tier (5) has sonnet + high thinking, local extraction, opus review", () => {
    const c = tierConfig(5);
    assert.equal(c.level, 5);
    assert.equal(c.name, "Lethal");
    assert.equal(c.driver, "sonnet");
    assert.equal(c.thinking, "high");
    assert.equal(c.extraction, "local");
    assert.equal(c.compaction, "local");
    assert.equal(c.cleavePreferLocal, false);
    assert.equal(c.cleaveFloor, "sonnet");
    assert.equal(c.reviewModel, "opus");
  });

  it("Absolute tier (6) has opus driver, sonnet background", () => {
    const c = tierConfig(6);
    assert.equal(c.level, 6);
    assert.equal(c.name, "Absolute");
    assert.equal(c.driver, "opus");
    assert.equal(c.thinking, "high");
    assert.equal(c.extraction, "sonnet");
    assert.equal(c.compaction, "sonnet");
    assert.equal(c.cleavePreferLocal, false);
    assert.equal(c.cleaveFloor, "sonnet");
    assert.equal(c.reviewModel, "opus");
  });

  it("Omnissiah tier (7) is all opus", () => {
    const c = tierConfig(7);
    assert.equal(c.level, 7);
    assert.equal(c.name, "Omnissiah");
    assert.equal(c.driver, "opus");
    assert.equal(c.thinking, "high");
    assert.equal(c.extraction, "opus");
    assert.equal(c.compaction, "opus");
    assert.equal(c.cleavePreferLocal, false);
    assert.equal(c.cleaveFloor, "opus");
    assert.equal(c.reviewModel, "opus");
  });

  it("every tier has all required fields", () => {
    const requiredKeys: (keyof EffortConfig)[] = [
      "level", "name", "driver", "thinking",
      "extraction", "compaction",
      "cleavePreferLocal", "cleaveFloor", "reviewModel",
    ];
    for (let level = 1; level <= 7; level++) {
      const c = tierConfig(level);
      for (const key of requiredKeys) {
        assert.notEqual(c[key], undefined, `tier ${level} missing ${key}`);
      }
    }
  });

  it("returns consistent objects across calls", () => {
    const a = tierConfig(4);
    const b = tierConfig(4);
    assert.deepEqual(a, b);
  });

  // ── Edge cases ──

  it("throws RangeError for level 0", () => {
    assert.throws(() => tierConfig(0), RangeError);
  });

  it("throws RangeError for level 8", () => {
    assert.throws(() => tierConfig(8), RangeError);
  });

  it("throws RangeError for negative level", () => {
    assert.throws(() => tierConfig(-1), RangeError);
  });

  it("throws RangeError for non-integer level", () => {
    assert.throws(() => tierConfig(3.5), RangeError);
  });

  it("throws RangeError for NaN", () => {
    assert.throws(() => tierConfig(NaN), RangeError);
  });

  it("throws RangeError for Infinity", () => {
    assert.throws(() => tierConfig(Infinity), RangeError);
  });
});

// ─── parseTierName ──────────────────────────────────────────────────────────

describe("parseTierName", () => {
  it("resolves 'Ruthless' to level 4", () => {
    assert.equal(parseTierName("Ruthless"), 4);
  });

  it("resolves 'Servitor' to level 1", () => {
    assert.equal(parseTierName("Servitor"), 1);
  });

  it("resolves 'Omnissiah' to level 7", () => {
    assert.equal(parseTierName("Omnissiah"), 7);
  });

  it("is case-insensitive (lowercase)", () => {
    assert.equal(parseTierName("ruthless"), 4);
  });

  it("is case-insensitive (uppercase)", () => {
    assert.equal(parseTierName("OMNISSIAH"), 7);
  });

  it("is case-insensitive (mixed case)", () => {
    assert.equal(parseTierName("sUbStAnTiAl"), 3);
  });

  it("returns undefined for unknown name", () => {
    assert.equal(parseTierName("Legendary"), undefined);
  });

  it("returns undefined for empty string", () => {
    assert.equal(parseTierName(""), undefined);
  });

  it("returns undefined for numeric string", () => {
    assert.equal(parseTierName("4"), undefined);
  });

  it("resolves all 7 tier names", () => {
    const expected: [string, number][] = [
      ["Servitor", 1], ["Average", 2], ["Substantial", 3],
      ["Ruthless", 4], ["Lethal", 5], ["Absolute", 6],
      ["Omnissiah", 7],
    ];
    for (const [name, level] of expected) {
      assert.equal(parseTierName(name), level, `${name} should resolve to ${level}`);
    }
  });
});

// ─── Constants ──────────────────────────────────────────────────────────────

describe("effort constants", () => {
  it("DEFAULT_EFFORT_LEVEL is 3 (Substantial)", () => {
    assert.equal(DEFAULT_EFFORT_LEVEL, 3);
  });

  it("TIER_NAMES has 7 entries", () => {
    assert.equal(TIER_NAMES.length, 7);
  });

  it("TIER_NAMES contains all expected names", () => {
    assert.ok(TIER_NAMES.includes("Servitor"));
    assert.ok(TIER_NAMES.includes("Average"));
    assert.ok(TIER_NAMES.includes("Substantial"));
    assert.ok(TIER_NAMES.includes("Ruthless"));
    assert.ok(TIER_NAMES.includes("Lethal"));
    assert.ok(TIER_NAMES.includes("Absolute"));
    assert.ok(TIER_NAMES.includes("Omnissiah"));
  });

  it("MIN_LEVEL is 1 and MAX_LEVEL is 7", () => {
    assert.equal(MIN_LEVEL, 1);
    assert.equal(MAX_LEVEL, 7);
  });

  it("EFFORT_NAMES maps all levels", () => {
    for (let i = 1; i <= 7; i++) {
      assert.ok(EFFORT_NAMES[i as EffortLevel], `EFFORT_NAMES missing level ${i}`);
    }
  });
});

// ─── Tier progression invariants ────────────────────────────────────────────

describe("tier progression invariants", () => {
  const MODEL_ORDER = { local: 0, haiku: 1, sonnet: 2, opus: 3 } as const;
  const THINKING_ORDER = { off: 0, minimal: 1, low: 2, medium: 3, high: 4 } as const;

  it("driver model tier never decreases as level increases", () => {
    for (let i = 1; i < 7; i++) {
      const curr = tierConfig(i);
      const next = tierConfig(i + 1);
      assert.ok(
        MODEL_ORDER[next.driver] >= MODEL_ORDER[curr.driver],
        `driver should not decrease from level ${i} (${curr.driver}) to ${i + 1} (${next.driver})`,
      );
    }
  });

  it("thinking level never decreases as level increases", () => {
    for (let i = 1; i < 7; i++) {
      const curr = tierConfig(i);
      const next = tierConfig(i + 1);
      assert.ok(
        THINKING_ORDER[next.thinking] >= THINKING_ORDER[curr.thinking],
        `thinking should not decrease from level ${i} (${curr.thinking}) to ${i + 1} (${next.thinking})`,
      );
    }
  });

  it("cleaveFloor model tier never decreases as level increases", () => {
    for (let i = 1; i < 7; i++) {
      const curr = tierConfig(i);
      const next = tierConfig(i + 1);
      assert.ok(
        MODEL_ORDER[next.cleaveFloor] >= MODEL_ORDER[curr.cleaveFloor],
        `cleaveFloor should not decrease from level ${i} to ${i + 1}`,
      );
    }
  });

  it("reviewModel tier never decreases as level increases", () => {
    for (let i = 1; i < 7; i++) {
      const curr = tierConfig(i);
      const next = tierConfig(i + 1);
      assert.ok(
        MODEL_ORDER[next.reviewModel] >= MODEL_ORDER[curr.reviewModel],
        `reviewModel should not decrease from level ${i} to ${i + 1}`,
      );
    }
  });
});
