/**
 * Tests for embeddings module — cosine similarity, vector serialization,
 * and model dimension constants.
 */

import { describe, it } from "node:test";
import * as assert from "node:assert/strict";
import {
  cosineSimilarity,
  vectorToBlob,
  blobToVector,
  MODEL_DIMS,
} from "./embeddings.js";

describe("cosineSimilarity", () => {
  it("returns 1.0 for identical vectors", () => {
    const v = new Float32Array([1, 2, 3, 4]);
    assert.ok(Math.abs(cosineSimilarity(v, v) - 1.0) < 1e-6);
  });

  it("returns -1.0 for opposite vectors", () => {
    const a = new Float32Array([1, 0, 0]);
    const b = new Float32Array([-1, 0, 0]);
    assert.ok(Math.abs(cosineSimilarity(a, b) - (-1.0)) < 1e-6);
  });

  it("returns 0.0 for orthogonal vectors", () => {
    const a = new Float32Array([1, 0, 0]);
    const b = new Float32Array([0, 1, 0]);
    assert.ok(Math.abs(cosineSimilarity(a, b)) < 1e-6);
  });

  it("returns 0 for mismatched dimensions", () => {
    const a = new Float32Array([1, 2, 3]);
    const b = new Float32Array([1, 2]);
    assert.equal(cosineSimilarity(a, b), 0);
  });

  it("returns 0 for zero vectors", () => {
    const a = new Float32Array([0, 0, 0]);
    const b = new Float32Array([1, 2, 3]);
    assert.equal(cosineSimilarity(a, b), 0);
  });

  it("is symmetric", () => {
    const a = new Float32Array([1, 3, -5]);
    const b = new Float32Array([4, -2, 1]);
    assert.ok(Math.abs(cosineSimilarity(a, b) - cosineSimilarity(b, a)) < 1e-6);
  });

  it("handles high-dimensional vectors", () => {
    const dims = 1024;
    const a = new Float32Array(dims);
    const b = new Float32Array(dims);
    for (let i = 0; i < dims; i++) {
      a[i] = Math.sin(i);
      b[i] = Math.cos(i);
    }
    const sim = cosineSimilarity(a, b);
    // sin and cos over 0..1023 should have near-zero correlation
    assert.ok(Math.abs(sim) < 0.1, `Expected near-zero, got ${sim}`);
  });

  it("magnitude-invariant (normalized vs unnormalized)", () => {
    const a = new Float32Array([1, 2, 3]);
    const b = new Float32Array([2, 4, 6]); // 2x scaled
    assert.ok(Math.abs(cosineSimilarity(a, b) - 1.0) < 1e-6);
  });
});

describe("vectorToBlob / blobToVector roundtrip", () => {
  it("preserves exact values through roundtrip", () => {
    const original = new Float32Array([1.5, -2.7, 0, 3.14159, -1e10, 1e-10]);
    const blob = vectorToBlob(original);
    const restored = blobToVector(blob);

    assert.equal(restored.length, original.length);
    for (let i = 0; i < original.length; i++) {
      assert.equal(restored[i], original[i], `Mismatch at index ${i}`);
    }
  });

  it("handles empty vectors", () => {
    const empty = new Float32Array(0);
    const blob = vectorToBlob(empty);
    const restored = blobToVector(blob);
    assert.equal(restored.length, 0);
  });

  it("handles 1024-dim vectors (qwen3-embedding:0.6b)", () => {
    const vec = new Float32Array(1024);
    for (let i = 0; i < 1024; i++) vec[i] = Math.random() * 2 - 1;
    const blob = vectorToBlob(vec);
    assert.equal(blob.length, 1024 * 4); // 4 bytes per float32
    const restored = blobToVector(blob);
    assert.equal(restored.length, 1024);
    for (let i = 0; i < 1024; i++) {
      assert.equal(restored[i], vec[i]);
    }
  });

  it("handles 2048-dim vectors (qwen3-embedding:4b)", () => {
    const vec = new Float32Array(2048);
    for (let i = 0; i < 2048; i++) vec[i] = Math.random() * 2 - 1;
    const blob = vectorToBlob(vec);
    assert.equal(blob.length, 2048 * 4);
    const restored = blobToVector(blob);
    assert.equal(restored.length, 2048);
  });

  it("produces aligned Float32Array from unaligned buffer", () => {
    // Simulate SQLite returning a buffer at an odd offset
    const original = new Float32Array([1.0, 2.0, 3.0]);
    const blob = vectorToBlob(original);
    // Create an unaligned copy (1-byte offset)
    const padded = Buffer.alloc(blob.length + 1);
    blob.copy(padded, 1);
    const unaligned = padded.subarray(1);
    const restored = blobToVector(unaligned);
    assert.equal(restored[0], 1.0);
    assert.equal(restored[1], 2.0);
    assert.equal(restored[2], 3.0);
  });
});

describe("MODEL_DIMS", () => {
  it("maps known models to expected dimensions", () => {
    assert.equal(MODEL_DIMS["qwen3-embedding:0.6b"], 1024);
    assert.equal(MODEL_DIMS["qwen3-embedding:4b"], 2048);
  });
});
