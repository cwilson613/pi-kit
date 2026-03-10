/**
 * Tests for pure compaction policy helpers.
 */

import { describe, it } from "node:test";
import * as assert from "node:assert/strict";
import { sanitizeCompactionText, shouldInterceptCompaction } from "./compaction-policy.ts";
import { DEFAULT_CONFIG } from "./types.ts";

describe("sanitizeCompactionText", () => {
  it("redacts transient pi-clipboard temp image paths", () => {
    const input = "Error while processing /var/folders/vl/w3m4rq616c9gv9cmbj99kz_80000gn/T/pi-clipboard-59952fe8-ab40-4f47-83ec-cb7173c8c4ea.png";
    assert.equal(
      sanitizeCompactionText(input),
      "Error while processing [clipboard image attachment]",
    );
  });

  it("preserves ordinary repository file paths", () => {
    const input = "Read extensions/project-memory/index.ts before summarizing.";
    assert.equal(sanitizeCompactionText(input), input);
  });

  it("only redacts transient clipboard paths when mixed with normal paths", () => {
    const input = [
      "See extensions/project-memory/index.ts",
      "Attachment: /var/folders/vl/w3m4rq616c9gv9cmbj99kz_80000gn/T/pi-clipboard-59952fe8-ab40-4f47-83ec-cb7173c8c4ea.png",
    ].join("\n");
    const output = sanitizeCompactionText(input);
    assert.match(output, /extensions\/project-memory\/index\.ts/);
    assert.doesNotMatch(output, /pi-clipboard-/);
    assert.match(output, /\[clipboard image attachment\]/);
  });
});

describe("shouldInterceptCompaction", () => {
  it("does not intercept by default when local-first is disabled", () => {
    assert.equal(
      shouldInterceptCompaction(undefined, DEFAULT_CONFIG, false),
      false,
    );
  });

  it("intercepts when effort explicitly requests local compaction", () => {
    assert.equal(
      shouldInterceptCompaction("local", DEFAULT_CONFIG, false),
      true,
    );
  });

  it("intercepts when retry fallback requests local compaction", () => {
    assert.equal(
      shouldInterceptCompaction("sonnet", DEFAULT_CONFIG, true),
      true,
    );
  });

  it("does not intercept when local fallback is disabled", () => {
    assert.equal(
      shouldInterceptCompaction("local", { ...DEFAULT_CONFIG, compactionLocalFallback: false }, true),
      false,
    );
  });
});
