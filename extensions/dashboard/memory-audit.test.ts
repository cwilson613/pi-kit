import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { formatMemoryAuditSummary } from "./memory-audit.ts";

describe("formatMemoryAuditSummary", () => {
  it("returns a fallback message when no snapshot exists", () => {
    assert.equal(formatMemoryAuditSummary(undefined), "Memory · pending first injection");
  });

  it("formats compact audit text", () => {
    assert.equal(
      formatMemoryAuditSummary({
        mode: "semantic",
        projectFactCount: 30,
        edgeCount: 0,
        workingMemoryFactCount: 4,
        semanticHitCount: 12,
        episodeCount: 3,
        globalFactCount: 15,
        payloadChars: 4800,
        estimatedTokens: 1200,
      }),
      "Memory semantic · facts:30 · wm:4 · ep:3 · global:15 · ~1200 tok",
    );
  });

  it("formats wide audit text with full breakdown", () => {
    assert.equal(
      formatMemoryAuditSummary(
        {
          mode: "bulk",
          projectFactCount: 50,
          edgeCount: 20,
          workingMemoryFactCount: 0,
          semanticHitCount: 0,
          episodeCount: 2,
          globalFactCount: 7,
          payloadChars: 6000,
          estimatedTokens: 1500,
        },
        { wide: true },
      ),
      "Memory audit: bulk · facts:50 · edges:20 · wm:0 · hits:0 · ep:2 · global:7 · chars:6000 · ~1500 tok",
    );
  });
});
