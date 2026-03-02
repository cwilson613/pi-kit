/**
 * Tests for mcp-bridge pure utility functions.
 *
 * Run: npx tsx --test extensions/mcp-bridge/lib.test.ts
 *   or: node --import tsx --test extensions/mcp-bridge/lib.test.ts
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  isHttpConfig,
  resolveEnvVars,
  resolveEnvObj,
  isAuthError,
  isTransportError,
  extractText,
  AUTH_REMEDIATION,
} from "./lib.js";

// ═══════════════════════════════════════════════════════════════════════════
// isHttpConfig
// ═══════════════════════════════════════════════════════════════════════════

describe("isHttpConfig", () => {
  it("returns true for objects with url", () => {
    assert.ok(isHttpConfig({ url: "https://example.com/mcp/" }));
  });

  it("returns true for url + headers", () => {
    assert.ok(isHttpConfig({ url: "https://x.com", headers: { Authorization: "Bearer tok" } }));
  });

  it("returns false for stdio config", () => {
    assert.ok(!isHttpConfig({ command: "npx", args: ["-y", "foo"] }));
  });

  it("returns false for stdio config with env", () => {
    assert.ok(!isHttpConfig({ command: "python", env: { KEY: "val" } }));
  });

  // Edge: an object with both url and command — url wins (discriminant)
  it("returns true if both url and command present", () => {
    assert.ok(isHttpConfig({ url: "https://x.com", command: "npx" } as any));
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// resolveEnvVars
// ═══════════════════════════════════════════════════════════════════════════

describe("resolveEnvVars", () => {
  const env = { TOKEN: "abc123", EMPTY: "" };

  it("replaces ${VAR} with value", () => {
    assert.equal(resolveEnvVars("Bearer ${TOKEN}", env), "Bearer abc123");
  });

  it("replaces multiple vars", () => {
    assert.equal(resolveEnvVars("${TOKEN}:${TOKEN}", env), "abc123:abc123");
  });

  it("replaces missing var with empty string", () => {
    assert.equal(resolveEnvVars("key=${MISSING}", env), "key=");
  });

  it("replaces empty var with empty string", () => {
    assert.equal(resolveEnvVars("key=${EMPTY}", env), "key=");
  });

  it("passes through strings without vars", () => {
    assert.equal(resolveEnvVars("no vars here", env), "no vars here");
  });

  it("handles empty string input", () => {
    assert.equal(resolveEnvVars("", env), "");
  });

  it("does not replace $VAR without braces", () => {
    assert.equal(resolveEnvVars("$TOKEN", env), "$TOKEN");
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// resolveEnvObj
// ═══════════════════════════════════════════════════════════════════════════

describe("resolveEnvObj", () => {
  it("resolves all values in an object", () => {
    const env = { A: "1", B: "2" };
    const result = resolveEnvObj({ x: "${A}", y: "${B}", z: "literal" }, env);
    assert.deepEqual(result, { x: "1", y: "2", z: "literal" });
  });

  it("returns empty object for empty input", () => {
    assert.deepEqual(resolveEnvObj({}, {}), {});
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// isAuthError
// ═══════════════════════════════════════════════════════════════════════════

describe("isAuthError", () => {
  // --- Positive: code-based detection ---

  it("detects error with code 401", () => {
    assert.ok(isAuthError({ code: 401, message: "whatever" }));
  });

  it("detects error with code 403", () => {
    assert.ok(isAuthError({ code: 403, message: "whatever" }));
  });

  // --- Positive: HTTP status in message (SDK send() format) ---

  it("detects 'HTTP 401' in message", () => {
    assert.ok(isAuthError(new Error("Error POSTing to endpoint (HTTP 401): {\"error\":\"Invalid token\"}")));
  });

  it("detects 'HTTP 403' in message", () => {
    assert.ok(isAuthError(new Error("Error POSTing to endpoint (HTTP 403): forbidden")));
  });

  // --- Positive: keyword patterns ---

  it("detects 'unauthorized'", () => {
    assert.ok(isAuthError(new Error("Unauthorized access")));
  });

  it("detects 'forbidden'", () => {
    assert.ok(isAuthError(new Error("Forbidden")));
  });

  it("detects 'invalid token'", () => {
    assert.ok(isAuthError(new Error("Invalid GitHub token")));
  });

  it("detects 'expired token'", () => {
    assert.ok(isAuthError(new Error("Your expired token cannot be used")));
  });

  it("detects 'token expired'", () => {
    assert.ok(isAuthError(new Error("Token expired at 2026-01-01")));
  });

  // --- Positive: real Scribe server responses ---

  it("detects Scribe invalid token response", () => {
    assert.ok(isAuthError(new Error(
      "Error POSTing to endpoint (HTTP 401): " +
      '{"error":"Invalid GitHub token. Provide a valid GitHub PAT with at least read:user and read:org scopes."}'
    )));
  });

  it("detects Scribe missing auth response", () => {
    assert.ok(isAuthError(new Error(
      "Error POSTing to endpoint (HTTP 401): " +
      '{"error":"Missing or invalid Authorization header. Provide an API key or GitHub PAT via: Authorization: Bearer <token>"}'
    )));
  });

  it("detects Scribe org membership failure", () => {
    assert.ok(isAuthError(new Error(
      "Error POSTing to endpoint (HTTP 403): " +
      '{"error":"User \'someone\' is not a member of the \'recro\' organization"}'
    )));
  });

  // --- Negative: should NOT match ---

  it("rejects HTTP 404", () => {
    assert.ok(!isAuthError(new Error("Error POSTing to endpoint (HTTP 404): not found")));
  });

  it("rejects HTTP 500", () => {
    assert.ok(!isAuthError(new Error("Error POSTing to endpoint (HTTP 500): server error")));
  });

  it("rejects ECONNREFUSED", () => {
    assert.ok(!isAuthError(new Error("connect ECONNREFUSED 127.0.0.1:8000")));
  });

  it("rejects timeout errors", () => {
    assert.ok(!isAuthError(new Error("[mcp-bridge] scribe: timed out after 15000ms")));
  });

  it("rejects generic errors", () => {
    assert.ok(!isAuthError(new Error("Something went wrong")));
  });

  it("rejects null/undefined", () => {
    assert.ok(!isAuthError(null));
    assert.ok(!isAuthError(undefined));
  });

  it("rejects error with no message", () => {
    assert.ok(!isAuthError({}));
  });

  it("rejects error with code 200", () => {
    assert.ok(!isAuthError({ code: 200, message: "ok" }));
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// isTransportError
// ═══════════════════════════════════════════════════════════════════════════

describe("isTransportError", () => {
  it("detects 'not connected'", () => {
    assert.ok(isTransportError(new Error("Client not connected")));
  });

  it("detects 'aborted'", () => {
    assert.ok(isTransportError(new Error("Request aborted")));
  });

  it("detects ECONNREFUSED", () => {
    assert.ok(isTransportError(new Error("connect ECONNREFUSED 127.0.0.1:8000")));
  });

  it("detects 'fetch failed'", () => {
    assert.ok(isTransportError(new Error("fetch failed")));
  });

  it("detects 'network'", () => {
    assert.ok(isTransportError(new Error("network error")));
  });

  it("detects ECONNRESET via code", () => {
    const err: any = new Error("read ECONNRESET");
    err.code = "ECONNRESET";
    assert.ok(isTransportError(err));
  });

  // Negative
  it("rejects auth errors", () => {
    assert.ok(!isTransportError(new Error("HTTP 401: unauthorized")));
  });

  it("rejects generic errors", () => {
    assert.ok(!isTransportError(new Error("Something else")));
  });

  it("rejects null", () => {
    assert.ok(!isTransportError(null));
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// extractText
// ═══════════════════════════════════════════════════════════════════════════

describe("extractText", () => {
  it("extracts text from single text block", () => {
    assert.equal(
      extractText({ content: [{ type: "text", text: "hello" }] }),
      "hello"
    );
  });

  it("joins multiple text blocks with newlines", () => {
    assert.equal(
      extractText({
        content: [
          { type: "text", text: "line 1" },
          { type: "text", text: "line 2" },
        ],
      }),
      "line 1\nline 2"
    );
  });

  it("filters out non-text blocks", () => {
    assert.equal(
      extractText({
        content: [
          { type: "image", data: "..." },
          { type: "text", text: "only this" },
          { type: "resource", uri: "..." },
        ],
      }),
      "only this"
    );
  });

  it("returns '(empty response)' for no text blocks", () => {
    assert.equal(
      extractText({ content: [{ type: "image", data: "..." }] }),
      "(empty response)"
    );
  });

  it("returns '(empty response)' for empty content array", () => {
    assert.equal(extractText({ content: [] }), "(empty response)");
  });

  it("returns '(empty response)' for text blocks with empty strings", () => {
    assert.equal(
      extractText({ content: [{ type: "text", text: "" }] }),
      "(empty response)"
    );
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// AUTH_REMEDIATION
// ═══════════════════════════════════════════════════════════════════════════

describe("AUTH_REMEDIATION", () => {
  it("mentions gh auth login", () => {
    assert.ok(AUTH_REMEDIATION.includes("gh auth login"));
  });

  it("mentions restart", () => {
    assert.ok(AUTH_REMEDIATION.includes("restart"));
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// Mutual exclusivity: auth vs transport errors
// ═══════════════════════════════════════════════════════════════════════════

describe("auth vs transport mutual exclusivity", () => {
  const authErrors = [
    { code: 401, message: "Unauthorized" },
    new Error("Error POSTing to endpoint (HTTP 401): invalid token"),
    new Error("Error POSTing to endpoint (HTTP 403): forbidden"),
  ];

  const transportErrors = [
    new Error("Client not connected"),
    new Error("connect ECONNREFUSED 127.0.0.1:8000"),
    new Error("fetch failed"),
  ];

  for (const err of authErrors) {
    const label = err instanceof Error ? err.message.slice(0, 50) : `code=${(err as any).code}`;
    it(`auth error '${label}' is NOT a transport error`, () => {
      assert.ok(isAuthError(err));
      assert.ok(!isTransportError(err));
    });
  }

  for (const err of transportErrors) {
    it(`transport error '${err.message}' is NOT an auth error`, () => {
      assert.ok(isTransportError(err));
      assert.ok(!isAuthError(err));
    });
  }
});
