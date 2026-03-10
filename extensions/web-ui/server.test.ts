/**
 * Tests for web UI server lifecycle.
 *
 * Covers: start on localhost, port reporting, clean stop,
 * subsequent requests fail after stop.
 */

import { describe, it, after, before } from "node:test";
import assert from "node:assert/strict";
import { startWebUIServer } from "./server.ts";
import type { WebUIServer } from "./server.ts";

// Helper: attempt a fetch and return the status code or null on network error
async function tryFetch(url: string): Promise<number | null> {
  try {
    const res = await fetch(url);
    return res.status;
  } catch {
    return null;
  }
}

// ── Start / stop lifecycle ────────────────────────────────────────────────────

describe("WebUIServer — lifecycle", () => {
  it("binds to 127.0.0.1 and returns a port + URL", async () => {
    const server = await startWebUIServer({ port: 0 });
    try {
      assert.ok(server.port > 0, "port must be > 0");
      assert.ok(server.url.startsWith("http://127.0.0.1:"), "URL must be localhost");
      assert.ok(server.url.includes(String(server.port)), "URL must include port");
    } finally {
      await server.stop();
    }
  });

  it("startedAt is a recent Unix epoch ms", async () => {
    const before = Date.now();
    const server = await startWebUIServer({ port: 0 });
    const after = Date.now();
    try {
      assert.ok(server.startedAt >= before, "startedAt must be ≥ before start");
      assert.ok(server.startedAt <= after, "startedAt must be ≤ after start");
    } finally {
      await server.stop();
    }
  });

  it("server responds before stop and fails after stop", async () => {
    const server = await startWebUIServer({ port: 0 });
    const url = server.url;

    const statusBefore = await tryFetch(`${url}/api/health`);
    assert.equal(statusBefore, 200, "should respond 200 before stop");

    await server.stop();

    const statusAfter = await tryFetch(`${url}/api/health`);
    assert.equal(statusAfter, null, "should fail after stop");
  });

  it("does not bind to 0.0.0.0 (address is 127.0.0.1)", async () => {
    const server = await startWebUIServer({ port: 0 });
    try {
      // The URL must start with the loopback address
      assert.ok(
        server.url.startsWith("http://127.0.0.1"),
        `Expected loopback URL but got: ${server.url}`
      );
    } finally {
      await server.stop();
    }
  });
});

// ── Root shell ────────────────────────────────────────────────────────────────

describe("WebUIServer — GET /", () => {
  let server: WebUIServer;
  before(async () => { server = await startWebUIServer({ port: 0 }); });
  after(async () => { await server.stop(); });

  it("returns 200 with HTML content-type", async () => {
    const res = await fetch(server.url + "/");
    assert.equal(res.status, 200);
    assert.ok(res.headers.get("content-type")?.includes("text/html"), "must be HTML");
  });

  it("HTML body includes client-side fetch of /api/state", async () => {
    const res = await fetch(server.url + "/");
    const body = await res.text();
    assert.ok(body.includes("/api/state"), "shell must reference /api/state");
    assert.ok(body.includes("<html"), "must be an HTML document");
  });
});
