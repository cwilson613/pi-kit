/**
 * Tests for degeneracy pressure curve.
 * The function is defined in index.ts — we replicate it here since index.ts
 * can't be imported without the full pi runtime.
 */

import { describe, it } from "node:test";
import * as assert from "node:assert/strict";

// Replicated from index.ts — keep in sync
function computeDegeneracyPressure(pct: number, onset: number, warning: number, k = 3): number {
  if (pct < onset) return 0;
  if (pct >= warning) return 1;
  const t = (pct - onset) / (warning - onset);
  return (Math.exp(k * t) - 1) / (Math.exp(k) - 1);
}

describe("computeDegeneracyPressure", () => {
  const onset = 40, warning = 65;

  it("returns 0 below onset", () => {
    assert.equal(computeDegeneracyPressure(0, onset, warning), 0);
    assert.equal(computeDegeneracyPressure(39, onset, warning), 0);
    assert.equal(computeDegeneracyPressure(40, onset, warning), 0);
  });

  it("returns 1 at and above warning threshold", () => {
    assert.equal(computeDegeneracyPressure(65, onset, warning), 1);
    assert.equal(computeDegeneracyPressure(80, onset, warning), 1);
    assert.equal(computeDegeneracyPressure(100, onset, warning), 1);
  });

  it("is monotonically increasing between onset and warning", () => {
    let prev = 0;
    for (let pct = 41; pct < 65; pct++) {
      const p = computeDegeneracyPressure(pct, onset, warning);
      assert.ok(p > prev, `pressure at ${pct}% (${p}) should be > ${prev}`);
      prev = p;
    }
  });

  it("is exponential — pressure at midpoint is below 0.5 (not linear)", () => {
    const mid = (onset + warning) / 2; // 52.5%
    const p = computeDegeneracyPressure(mid, onset, warning);
    assert.ok(p < 0.5, `midpoint pressure should be <0.5 for exponential curve, got ${p}`);
  });

  it("grows slowly at first, fast at end", () => {
    const early = computeDegeneracyPressure(45, onset, warning);
    const late = computeDegeneracyPressure(60, onset, warning);
    // Rate of change at 60% should be much higher than at 45%
    const earlyDelta = computeDegeneracyPressure(46, onset, warning) - early;
    const lateDelta = computeDegeneracyPressure(61, onset, warning) - late;
    assert.ok(lateDelta > earlyDelta * 3, `late delta (${lateDelta}) should be >3x early delta (${earlyDelta})`);
  });

  it("pressure at 45% is low (< 0.1)", () => {
    const p = computeDegeneracyPressure(45, onset, warning);
    assert.ok(p < 0.1, `45% should have low pressure, got ${p}`);
  });

  it("pressure at 55% is moderate (0.2-0.4)", () => {
    const p = computeDegeneracyPressure(55, onset, warning);
    assert.ok(p > 0.2 && p < 0.4, `55% should have moderate pressure, got ${p}`);
  });

  it("steepness parameter k controls curve shape", () => {
    const mid = 52.5;
    const gentle = computeDegeneracyPressure(mid, onset, warning, 1);
    const steep = computeDegeneracyPressure(mid, onset, warning, 5);
    // Higher k = more exponential = lower midpoint value
    assert.ok(steep < gentle, `k=5 (${steep}) should be more skewed than k=1 (${gentle})`);
  });

  it("handles edge case where onset equals warning", () => {
    // Degenerate config — should return 0 below, 1 at
    assert.equal(computeDegeneracyPressure(49, 50, 50), 0);
    assert.equal(computeDegeneracyPressure(50, 50, 50), 1);
  });
});
