import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import {
  CAPABILITY_ROLES,
  getDefaultOperatorProfile,
  loadOperatorRuntimeState,
  parseOperatorProfile,
  parseOperatorRuntimeState,
  readOperatorProfile,
  resolveRoleAlias,
  saveOperatorRuntimeState,
  toCapabilityProfile,
  toCapabilityRuntimeState,
  writeOperatorProfile,
} from "./operator-profile.ts";
import { readLastUsedModel, writeLastUsedModel } from "./model-preferences.ts";

function makeTmpDir(): string {
  return mkdtempSync(join(tmpdir(), "operator-profile-"));
}

describe("operator profile defaults", () => {
  it("synthesizes a conservative default profile when config is missing", () => {
    const profile = readOperatorProfile(makeTmpDir());
    assert.deepEqual(Object.keys(profile.roles), CAPABILITY_ROLES);
    assert.equal(profile.fallback.sameRoleCrossProvider, "allow");
    assert.equal(profile.fallback.crossSource, "ask");
    assert.equal(profile.fallback.heavyLocal, "deny");
    assert.equal(profile.fallback.unknownLocalPerformance, "ask");
    assert.equal(profile.roles.servoskull[0]?.maxThinking, "off");
  });

  it("returns a fresh clone for the default profile", () => {
    const a = getDefaultOperatorProfile();
    const b = getDefaultOperatorProfile();
    a.roles.archmagos[0]!.id = "mutated";
    assert.notEqual(a.roles.archmagos[0]!.id, b.roles.archmagos[0]!.id);
  });
});

describe("operator profile parsing", () => {
  it("preserves candidate maxThinking ceilings from persisted config", () => {
    const profile = parseOperatorProfile({
      roles: {
        magos: [
          {
            id: "claude-sonnet-4-6",
            provider: "anthropic",
            source: "upstream",
            weight: "normal",
            maxThinking: "low",
          },
        ],
      },
    });

    assert.equal(profile.roles.magos.length, 1);
    assert.equal(profile.roles.magos[0]?.id, "claude-sonnet-4-6");
    assert.equal(profile.roles.magos[0]?.provider, "anthropic");
    assert.equal(profile.roles.magos[0]?.source, "upstream");
    assert.equal(profile.roles.magos[0]?.weight, "normal");
    assert.equal(profile.roles.magos[0]?.maxThinking, "low");
  });

  it("ignores invalid candidates and keeps defaults for malformed roles", () => {
    const profile = parseOperatorProfile({
      roles: {
        archmagos: [null, { nope: true }],
      },
      fallback: {
        crossSource: "allow",
        heavyLocal: "invalid",
      },
    });

    assert.equal(profile.roles.archmagos[0]?.id, "claude-opus-4-6");
    assert.equal(profile.fallback.crossSource, "allow");
    assert.equal(profile.fallback.heavyLocal, "deny");
  });

  it("resolves legacy/internal aliases onto public roles", () => {
    assert.equal(resolveRoleAlias("opus"), "archmagos");
    assert.equal(resolveRoleAlias("sonnet"), "magos");
    assert.equal(resolveRoleAlias("haiku"), "adept");
    assert.equal(resolveRoleAlias("local"), "servoskull");
    assert.equal(resolveRoleAlias("servo-skull"), "servoskull");
  });

  it("normalizes legacy frontier source and numeric weight values", () => {
    const profile = parseOperatorProfile({
      roles: {
        servitor: [
          {
            id: "legacy-model",
            provider: "openai",
            source: "frontier",
            weight: 80,
            maxThinking: "minimal",
          },
        ],
      },
    });

    assert.equal(profile.roles.servitor[0]?.source, "upstream");
    assert.equal(profile.roles.servitor[0]?.weight, "normal");
  });
});

describe("operator profile persistence", () => {
  it("round-trips operator profile through .pi/config.json without regressing lastUsedModel", () => {
    const tmp = makeTmpDir();
    try {
      writeLastUsedModel(tmp, { provider: "openai", modelId: "gpt-5.4" });
      writeOperatorProfile(tmp, {
        roles: {
          archmagos: [{ id: "gpt-5.4", provider: "openai", source: "upstream", weight: "heavy", maxThinking: "high" }],
          magos: [{ id: "claude-sonnet-4-6", provider: "anthropic", source: "upstream", weight: "normal", maxThinking: "medium" }],
          adept: [{ id: "claude-haiku-3-5", provider: "anthropic", source: "upstream", weight: "light", maxThinking: "low" }],
          servitor: [{ id: "gpt-4o-mini", provider: "openai", source: "upstream", weight: "light", maxThinking: "minimal" }],
          servoskull: [{ id: "qwen3:8b", provider: "local", source: "local", weight: "light", maxThinking: "off" }],
        },
        fallback: {
          sameRoleCrossProvider: "allow",
          crossSource: "ask",
          heavyLocal: "deny",
          unknownLocalPerformance: "ask",
        },
        setupComplete: true,
      });

      const profile = readOperatorProfile(tmp);
      assert.equal(readLastUsedModel(tmp)?.modelId, "gpt-5.4");
      assert.equal(profile.roles.archmagos[0]?.id, "gpt-5.4");
      assert.equal(profile.roles.servoskull[0]?.maxThinking, "off");
      assert.equal(profile.setupComplete, true);
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });
});

describe("operator runtime state", () => {
  it("parses and persists cooldown data outside durable config", () => {
    const tmp = makeTmpDir();
    try {
      saveOperatorRuntimeState(tmp, {
        providers: {
          anthropic: { until: "2026-03-09T14:00:00.000Z", reason: "429" },
        },
        candidates: {
          "openai:gpt-5.4": { until: "2026-03-09T14:05:00.000Z" },
        },
      });

      const state = loadOperatorRuntimeState(tmp);
      assert.equal(state.providers?.anthropic?.reason, "429");
      assert.equal(state.candidates?.["openai:gpt-5.4"]?.until, "2026-03-09T14:05:00.000Z");

      const persistedPath = join(tmp, ".pi", "runtime", "operator-profile.json");
      const persisted = readFileSync(persistedPath, "utf-8");
      assert.ok(persisted.includes('"providers"'));
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });

  it("drops malformed cooldown entries", () => {
    const state = parseOperatorRuntimeState({
      providers: {
        anthropic: { until: "2026-03-09T14:00:00.000Z", reason: "429" },
        openai: { reason: "missing-until" },
      },
      candidates: {
        local: "bad",
      },
    });

    assert.deepEqual(Object.keys(state.providers ?? {}), ["anthropic"]);
    assert.equal(state.candidates, undefined);
  });

  it("converts persisted runtime cooldowns to resolver runtime state", () => {
    const runtimeState = toCapabilityRuntimeState({
      providers: {
        anthropic: { until: "2026-03-09T14:00:00.000Z", reason: "429" },
        ollama: { until: "2026-03-09T14:01:00.000Z", reason: "busy" },
      },
      candidates: {
        "openai:gpt-5.4": { until: "2026-03-09T14:05:00.000Z" },
      },
    });

    assert.equal(typeof runtimeState.providerCooldowns?.anthropic?.until, "number");
    assert.equal(runtimeState.providerCooldowns?.local?.reason, "busy");
    assert.ok(runtimeState.candidateCooldowns?.["openai/gpt-5.4"]);
  });
});

describe("resolver profile bridge", () => {
  it("converts operator profile roles and fallback policy into resolver profile", () => {
    const capabilityProfile = toCapabilityProfile(parseOperatorProfile({
      roles: {
        magos: [
          {
            id: "claude-sonnet-4-6",
            provider: "anthropic",
            source: "upstream",
            weight: "normal",
            maxThinking: "medium",
          },
        ],
        servoskull: [
          {
            id: "qwen3:8b",
            provider: "ollama",
            source: "local",
            weight: "light",
            maxThinking: "off",
          },
        ],
      },
      fallback: {
        sameRoleCrossProvider: "allow",
        crossSource: "ask",
        heavyLocal: "deny",
        unknownLocalPerformance: "ask",
      },
    }));

    assert.equal(capabilityProfile.roles.magos.candidates[0]?.provider, "anthropic");
    assert.equal(capabilityProfile.roles.magos.candidates[0]?.maxThinking, "medium");
    assert.equal(capabilityProfile.roles.servoskull.candidates[0]?.provider, "local");
    assert.equal(capabilityProfile.roles.servoskull.candidates[0]?.maxThinking, "off");
    assert.equal(capabilityProfile.policy.heavyLocal, "deny");
    assert.equal(capabilityProfile.internalAliases["cleave.leaf"], "adept");
  });
});
