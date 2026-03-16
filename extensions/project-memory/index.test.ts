import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { getJsonlSyncState, writeJsonlIfChanged } from "./jsonl-io.ts";

describe("getJsonlSyncState", () => {
  it("reports in-sync when content is unchanged", () => {
    const fsSync = {
      existsSync: (_path: string) => true,
      readFileSync: (_path: string, _encoding: string) => "same-content\n",
      writeFileSync: (_path: string, _content: string, _encoding: string) => {
        throw new Error("should not write");
      },
    };

    const state = getJsonlSyncState(fsSync as any, "/tmp/facts.jsonl", "same-content\n");
    assert.equal(state.exists, true);
    assert.equal(state.inSync, true);
    assert.equal(state.currentContent, "same-content\n");
  });

  it("reports drift when file content differs", () => {
    const fsSync = {
      existsSync: (_path: string) => true,
      readFileSync: (_path: string, _encoding: string) => "old-content\n",
      writeFileSync: (_path: string, _content: string, _encoding: string) => {
        throw new Error("should not write");
      },
    };

    const state = getJsonlSyncState(fsSync as any, "/tmp/facts.jsonl", "new-content\n");
    assert.equal(state.exists, true);
    assert.equal(state.inSync, false);
    assert.equal(state.currentContent, "old-content\n");
  });

  it("reports missing file as drift without current content", () => {
    const fsSync = {
      existsSync: (_path: string) => false,
      readFileSync: (_path: string, _encoding: string) => {
        throw new Error("should not read");
      },
      writeFileSync: (_path: string, _content: string, _encoding: string) => {
        throw new Error("should not write");
      },
    };

    const state = getJsonlSyncState(fsSync as any, "/tmp/facts.jsonl", "new-content\n");
    assert.equal(state.exists, false);
    assert.equal(state.inSync, false);
    assert.equal(state.currentContent, null);
  });
});

describe("writeJsonlIfChanged", () => {
  it("does not rewrite facts.jsonl when content is unchanged", () => {
    let writes = 0;
    const fsSync = {
      existsSync: (_path: string) => true,
      readFileSync: (_path: string, _encoding: string) => "same-content\n",
      writeFileSync: (_path: string, _content: string, _encoding: string) => {
        writes += 1;
      },
    };

    const changed = writeJsonlIfChanged(fsSync as any, "/tmp/facts.jsonl", "same-content\n");
    assert.equal(changed, false);
    assert.equal(writes, 0);
  });

  it("rewrites facts.jsonl when content differs", () => {
    let writes = 0;
    let written = "";
    const fsSync = {
      existsSync: (_path: string) => true,
      readFileSync: (_path: string, _encoding: string) => "old-content\n",
      writeFileSync: (_path: string, content: string, _encoding: string) => {
        writes += 1;
        written = content;
      },
    };

    const changed = writeJsonlIfChanged(fsSync as any, "/tmp/facts.jsonl", "new-content\n");
    assert.equal(changed, true);
    assert.equal(writes, 1);
    assert.equal(written, "new-content\n");
  });
});
