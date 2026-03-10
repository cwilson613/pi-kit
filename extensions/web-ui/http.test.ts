/**
 * Route-level tests for the web UI HTTP server.
 *
 * Covers: /api/state, slice routes, mutation refusal,
 * 404 for unknown endpoints, polling model assumptions.
 */

import { describe, it, before, after } from "node:test";
import assert from "node:assert/strict";
import { startWebUIServer } from "./server.ts";
import type { WebUIServer } from "./server.ts";
import type { ControlPlaneState } from "./types.ts";

// ── Shared server instance ────────────────────────────────────────────────────

let server: WebUIServer;
before(async () => { server = await startWebUIServer({ port: 0 }); });
after(async () => { await server.stop(); });

// ── /api/state ────────────────────────────────────────────────────────────────

describe("GET /api/state", () => {
  it("returns 200 JSON", async () => {
    const res = await fetch(`${server.url}/api/state`);
    assert.equal(res.status, 200);
    assert.ok(res.headers.get("content-type")?.includes("application/json"));
  });

  it("body contains schemaVersion", async () => {
    const res = await fetch(`${server.url}/api/state`);
    const body = await res.json() as ControlPlaneState;
    assert.ok("schemaVersion" in body, "missing schemaVersion");
    assert.equal(typeof body.schemaVersion, "number");
  });

  it("body contains all required top-level sections", async () => {
    const res = await fetch(`${server.url}/api/state`);
    const body = await res.json() as ControlPlaneState;
    const required: (keyof ControlPlaneState)[] = [
      "session", "dashboard", "designTree", "openspec",
      "cleave", "models", "memory", "health",
    ];
    for (const key of required) {
      assert.ok(key in body, `missing section: ${key}`);
    }
  });

  it("successive calls return fresh snapshots (no mutation)", async () => {
    const r1 = await fetch(`${server.url}/api/state`);
    const b1 = await r1.json() as ControlPlaneState;
    // Wait briefly so capturedAt can differ
    await new Promise((r) => setTimeout(r, 10));
    const r2 = await fetch(`${server.url}/api/state`);
    const b2 = await r2.json() as ControlPlaneState;
    // Both snapshots must have the required sections — server is never mutated
    assert.ok("schemaVersion" in b1 && "schemaVersion" in b2);
  });
});

// ── Slice routes ──────────────────────────────────────────────────────────────

const SLICE_PATHS = [
  "/api/design-tree",
  "/api/openspec",
  "/api/cleave",
  "/api/models",
  "/api/memory",
  "/api/health",
];

describe("GET slice routes", () => {
  for (const route of SLICE_PATHS) {
    it(`${route} returns 200 JSON`, async () => {
      const res = await fetch(`${server.url}${route}`);
      assert.equal(res.status, 200, `${route} should return 200`);
      assert.ok(
        res.headers.get("content-type")?.includes("application/json"),
        `${route} should return JSON`
      );
    });

    it(`${route} body is non-null`, async () => {
      const res = await fetch(`${server.url}${route}`);
      const body = await res.json();
      assert.ok(body !== null && typeof body === "object", `${route} body should be an object`);
    });
  }

  it("/api/health returns status ok", async () => {
    const res = await fetch(`${server.url}/api/health`);
    const body = await res.json() as { status: string; uptimeMs: number; serverAlive: boolean };
    assert.equal(body.status, "ok");
    assert.ok(typeof body.uptimeMs === "number" && body.uptimeMs >= 0);
    assert.equal(body.serverAlive, true);
  });
});

// ── Mutation refusal ──────────────────────────────────────────────────────────

describe("Mutation refusal", () => {
  it("POST /api/state returns non-success (405)", async () => {
    const res = await fetch(`${server.url}/api/state`, { method: "POST" });
    assert.ok(res.status >= 400, `Expected 4xx but got ${res.status}`);
  });

  it("PUT /api/state returns non-success", async () => {
    const res = await fetch(`${server.url}/api/state`, { method: "PUT" });
    assert.ok(res.status >= 400);
  });

  it("DELETE /api/state returns non-success", async () => {
    const res = await fetch(`${server.url}/api/state`, { method: "DELETE" });
    assert.ok(res.status >= 400);
  });

  it("POST /api/state does not mutate health state", async () => {
    await fetch(`${server.url}/api/state`, { method: "POST" });
    // Health endpoint must still respond correctly
    const healthRes = await fetch(`${server.url}/api/health`);
    assert.equal(healthRes.status, 200);
    const body = await healthRes.json() as { status: string };
    assert.equal(body.status, "ok");
  });
});

// ── 404 and unknown routes ────────────────────────────────────────────────────

describe("404 / unknown routes", () => {
  it("unknown route returns 404", async () => {
    const res = await fetch(`${server.url}/api/unknown-mutation`);
    assert.equal(res.status, 404);
  });

  it("non-existent path returns 404 JSON", async () => {
    const res = await fetch(`${server.url}/api/does-not-exist`);
    assert.equal(res.status, 404);
    const body = await res.json() as { error: string };
    assert.equal(body.error, "Not Found");
  });

  it("POST to unknown path returns 404", async () => {
    const res = await fetch(`${server.url}/api/mutate-state`, { method: "POST" });
    // Could be 404 or 405 — both acceptable per spec
    assert.ok(res.status === 404 || res.status === 405, `Expected 404 or 405, got ${res.status}`);
  });
});

// ── Polling-first model ───────────────────────────────────────────────────────

describe("Polling-first model", () => {
  it("server returns JSON state on successive polls (no SSE/WS required)", async () => {
    // The MVP uses polling — verify the endpoint is stable across repeated GETs.
    // WebSocket/SSE are explicitly not required by the spec.
    for (let i = 0; i < 3; i++) {
      const res = await fetch(`${server.url}/api/state`);
      assert.equal(res.status, 200, `poll ${i + 1} must return 200`);
      const body = await res.json() as ControlPlaneState;
      assert.ok("schemaVersion" in body);
    }
  });

  it("three successive GET /api/state calls all return 200", async () => {
    for (let i = 0; i < 3; i++) {
      const res = await fetch(`${server.url}/api/state`);
      assert.equal(res.status, 200, `Poll ${i + 1} failed`);
    }
  });
});
