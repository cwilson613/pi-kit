/**
 * Tests for the secrets extension guard logic.
 *
 * Run: npx tsx --test extensions/00-secrets/index.test.ts
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  matchSensitivePath,
  isSecretAccessCommand,
  redactString,
  MIN_REDACT_LENGTH,
  SENSITIVE_PATH_PATTERNS,
} from "./index.js";

// ═══════════════════════════════════════════════════════════════════════════
// matchSensitivePath
// ═══════════════════════════════════════════════════════════════════════════

describe("matchSensitivePath", () => {
  // ── Hard blocks ──

  it("blocks secrets.json", () => {
    const m = matchSensitivePath("~/.pi/agent/secrets.json");
    assert.ok(m);
    assert.equal(m.action, "block");
  });

  it("blocks secrets.json with full path", () => {
    const m = matchSensitivePath("/Users/someone/.pi/agent/secrets.json");
    assert.ok(m);
    assert.equal(m.action, "block");
  });

  it("blocks audit log", () => {
    const m = matchSensitivePath("~/.pi/agent/secrets-audit.jsonl");
    assert.ok(m);
    assert.equal(m.action, "block");
  });

  // ── Confirm paths ──

  it("confirms .env", () => {
    const m = matchSensitivePath(".env");
    assert.ok(m);
    assert.equal(m.action, "confirm");
  });

  it("confirms .env.local", () => {
    const m = matchSensitivePath(".env.local");
    assert.ok(m);
    assert.equal(m.action, "confirm");
  });

  it("confirms .env.production", () => {
    const m = matchSensitivePath("/app/.env.production");
    assert.ok(m);
    assert.equal(m.action, "confirm");
  });

  it("confirms SSH private keys", () => {
    const m = matchSensitivePath("~/.ssh/id_rsa");
    assert.ok(m);
    assert.equal(m.action, "confirm");
  });

  it("confirms SSH ed25519 keys", () => {
    const m = matchSensitivePath("/home/user/.ssh/id_ed25519");
    assert.ok(m);
    assert.equal(m.action, "confirm");
  });

  it("confirms AWS credentials", () => {
    const m = matchSensitivePath("~/.aws/credentials");
    assert.ok(m);
    assert.equal(m.action, "confirm");
  });

  it("confirms Docker config", () => {
    const m = matchSensitivePath("~/.docker/config.json");
    assert.ok(m);
    assert.equal(m.action, "confirm");
  });

  it("confirms .npmrc", () => {
    const m = matchSensitivePath("~/.npmrc");
    assert.ok(m);
    assert.equal(m.action, "confirm");
  });

  it("confirms kubeconfig", () => {
    const m = matchSensitivePath("~/.kube/config");
    assert.ok(m);
    assert.equal(m.action, "confirm");
  });

  it("confirms .netrc", () => {
    const m = matchSensitivePath("~/.netrc");
    assert.ok(m);
    assert.equal(m.action, "confirm");
  });

  it("confirms credentials.json", () => {
    const m = matchSensitivePath("/app/config/credentials.json");
    assert.ok(m);
    assert.equal(m.action, "confirm");
  });

  it("confirms token.pem", () => {
    const m = matchSensitivePath("/etc/ssl/private_key.pem");
    assert.ok(m);
    assert.equal(m.action, "confirm");
  });

  // ── Safe paths ──

  it("allows normal source code", () => {
    const m = matchSensitivePath("extensions/00-secrets/index.ts");
    assert.equal(m, undefined);
  });

  it("allows normal config files", () => {
    const m = matchSensitivePath("tsconfig.json");
    assert.equal(m, undefined);
  });

  it("allows README", () => {
    const m = matchSensitivePath("README.md");
    assert.equal(m, undefined);
  });

  it("allows package.json", () => {
    const m = matchSensitivePath("package.json");
    assert.equal(m, undefined);
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// isSecretAccessCommand
// ═══════════════════════════════════════════════════════════════════════════

describe("isSecretAccessCommand", () => {
  // ── Should catch ──

  it("catches keychain access", () => {
    assert.ok(isSecretAccessCommand("security find-generic-password -a pi-kit -ws pi-kit/KEY"));
  });

  it("catches 1Password read", () => {
    assert.ok(isSecretAccessCommand('op read "op://vault/item/field"'));
  });

  it("catches pass show", () => {
    assert.ok(isSecretAccessCommand("pass show email/work"));
  });

  it("catches vault read", () => {
    assert.ok(isSecretAccessCommand("vault read secret/data/myapp"));
  });

  it("catches vault kv get", () => {
    assert.ok(isSecretAccessCommand("vault kv get secret/myapp"));
  });

  it("catches env dump with secret keywords", () => {
    assert.ok(isSecretAccessCommand("env | grep SECRET"));
  });

  it("catches echo of secret env vars", () => {
    assert.ok(isSecretAccessCommand("echo $GITHUB_TOKEN"));
  });

  it("catches cat of .env file", () => {
    assert.ok(isSecretAccessCommand("cat .env"));
  });

  it("catches cat of secrets.json", () => {
    assert.ok(isSecretAccessCommand("cat ~/.pi/agent/secrets.json"));
  });

  it("catches jq on secrets.json", () => {
    assert.ok(isSecretAccessCommand("jq . ~/.pi/agent/secrets.json"));
  });

  it("catches head on credentials", () => {
    assert.ok(isSecretAccessCommand("head ~/.aws/credentials"));
  });

  it("catches sed on secrets.json", () => {
    assert.ok(isSecretAccessCommand("sed -n 'p' secrets.json"));
  });

  it("catches redirect to secrets.json", () => {
    assert.ok(isSecretAccessCommand('echo "test" > ~/.pi/agent/secrets.json'));
  });

  it("catches node process.env dump", () => {
    assert.ok(isSecretAccessCommand("node -e 'console.log(process.env)'"));
  });

  it("catches python os.environ dump", () => {
    assert.ok(isSecretAccessCommand("python3 -c 'import os; print(os.environ)'"));
  });

  it("catches ruby ENV dump", () => {
    assert.ok(isSecretAccessCommand("ruby -e 'puts ENV'"));
  });

  it("catches perl %ENV dump", () => {
    assert.ok(isSecretAccessCommand("perl -e 'print %ENV'"));
  });

  it("catches base64 decode piped to shell", () => {
    assert.ok(isSecretAccessCommand("echo c2VjdXJpdHk= | base64 -d | bash"));
  });

  it("catches sh -c wrapping keychain access", () => {
    assert.ok(isSecretAccessCommand("sh -c 'security find-generic-password -ws foo'"));
  });

  it("catches python wrapping keychain access", () => {
    assert.ok(isSecretAccessCommand("python3 -c \"import subprocess; subprocess.run(['security', 'find-generic-password'])\""));
  });

  // ── Should NOT catch (false positive reduction) ──

  it("allows normal grep of source code", () => {
    assert.ok(!isSecretAccessCommand("grep -rn 'function' src/"));
  });

  it("allows normal cat of source files", () => {
    assert.ok(!isSecretAccessCommand("cat src/main.ts"));
  });

  it("allows ls", () => {
    assert.ok(!isSecretAccessCommand("ls -la"));
  });

  it("allows git commands", () => {
    assert.ok(!isSecretAccessCommand("git log --oneline"));
  });

  it("allows npm install", () => {
    assert.ok(!isSecretAccessCommand("npm install express"));
  });

  it("allows normal node execution", () => {
    assert.ok(!isSecretAccessCommand("node server.js"));
  });

  it("allows python running a script", () => {
    assert.ok(!isSecretAccessCommand("python3 app.py"));
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// redactString
// ═══════════════════════════════════════════════════════════════════════════

describe("redactString", () => {
  const secrets = [
    { name: "API_KEY", value: "sk-abc123def456" },
    { name: "TOKEN", value: "ghp_1234567890abcdef" },
  ];

  it("redacts exact match", () => {
    const result = redactString("key is sk-abc123def456 here", secrets);
    assert.equal(result, "key is [REDACTED:API_KEY] here");
  });

  it("redacts multiple occurrences", () => {
    const result = redactString("sk-abc123def456 and sk-abc123def456", secrets);
    assert.equal(result, "[REDACTED:API_KEY] and [REDACTED:API_KEY]");
  });

  it("redacts multiple different secrets", () => {
    const result = redactString("api: sk-abc123def456 token: ghp_1234567890abcdef", secrets);
    assert.equal(result, "api: [REDACTED:API_KEY] token: [REDACTED:TOKEN]");
  });

  it("does not redact when no match", () => {
    const result = redactString("nothing sensitive here", secrets);
    assert.equal(result, "nothing sensitive here");
  });

  it("skips secrets shorter than MIN_REDACT_LENGTH", () => {
    const shortSecrets = [{ name: "PIN", value: "ab" }];
    const result = redactString("value is ab here", shortSecrets);
    assert.equal(result, "value is ab here");
  });

  it("redacts secrets at exactly MIN_REDACT_LENGTH", () => {
    const minSecrets = [{ name: "KEY", value: "abcd" }];
    assert.equal(MIN_REDACT_LENGTH, 4); // Verify our assumption
    const result = redactString("value is abcd here", minSecrets);
    assert.equal(result, "value is [REDACTED:KEY] here");
  });

  it("handles regex special characters in secret values", () => {
    const specialSecrets = [{ name: "REGEX_KEY", value: "sk+test.value(123)" }];
    const result = redactString("secret: sk+test.value(123) done", specialSecrets);
    assert.equal(result, "secret: [REDACTED:REGEX_KEY] done");
  });

  it("redacts base64-encoded secrets", () => {
    const value = "sk-abc123def456";
    const b64 = Buffer.from(value).toString("base64");
    const b64Secrets = [{ name: "B64_KEY", value }];
    const result = redactString(`encoded: ${b64}`, b64Secrets);
    assert.ok(result.includes("[REDACTED:B64_KEY:base64]"), `Expected base64 redaction in: ${result}`);
  });

  it("redacts partial prefix for very long secrets (40+ chars)", () => {
    const longValue = "a".repeat(50);
    const longSecrets = [{ name: "LONG_KEY", value: longValue }];
    // The first 12 chars should also be caught
    const result = redactString("prefix: " + longValue.slice(0, 12) + " rest", longSecrets);
    assert.ok(result.includes("[REDACTED:LONG_KEY]"), `Expected prefix redaction in: ${result}`);
  });

  it("does NOT redact partial prefix for shorter secrets (< 40 chars)", () => {
    const medValue = "a".repeat(20);
    const medSecrets = [{ name: "MED_KEY", value: medValue }];
    // The first 12 chars alone should NOT be redacted
    const result = redactString("prefix: " + medValue.slice(0, 12) + " rest", medSecrets);
    assert.ok(!result.includes("[REDACTED:MED_KEY]"), `Unexpected prefix redaction in: ${result}`);
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// SENSITIVE_PATH_PATTERNS completeness
// ═══════════════════════════════════════════════════════════════════════════

describe("SENSITIVE_PATH_PATTERNS", () => {
  it("has at least one block-level pattern (secrets.json)", () => {
    const blocks = SENSITIVE_PATH_PATTERNS.filter(p => p.action === "block");
    assert.ok(blocks.length >= 1, "Expected at least one block pattern");
  });

  it("has confirm patterns for common credential stores", () => {
    const confirms = SENSITIVE_PATH_PATTERNS.filter(p => p.action === "confirm");
    assert.ok(confirms.length >= 5, `Expected at least 5 confirm patterns, got ${confirms.length}`);
  });

  it("every pattern has a description", () => {
    for (const entry of SENSITIVE_PATH_PATTERNS) {
      assert.ok(entry.description.length > 0, `Pattern ${entry.pattern} missing description`);
    }
  });
});
