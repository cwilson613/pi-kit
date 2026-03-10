import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { getDefaultCapabilityProfile, getDefaultPolicy, type RegistryModel } from "./model-routing.ts";
import { buildFallbackGuidance, explainTierResolutionFailure, inferRolesForModel, recordTransientFailureForModel } from "./operator-fallback.ts";
import { loadOperatorRuntimeState, toCapabilityRuntimeState } from "./operator-profile.ts";

function makeTmpDir(): string {
  return mkdtempSync(join(tmpdir(), "operator-fallback-"));
}

function makeModel(provider: string, id: string): RegistryModel {
  return { provider, id };
}

describe("inferRolesForModel", () => {
  it("finds canonical roles containing the current model candidate", () => {
    const models = [
      makeModel("anthropic", "claude-opus-4-6"),
      makeModel("openai", "gpt-5.3-codex-spark"),
      makeModel("local", "qwen3:8b"),
    ];
    const profile = getDefaultCapabilityProfile(models);
    assert.deepEqual(inferRolesForModel({ provider: "anthropic", id: "claude-opus-4-6" }, profile), ["archmagos"]);
  });
});

describe("recordTransientFailureForModel", () => {
  it("persists provider and candidate cooldowns for transient upstream failures", () => {
    const tmp = makeTmpDir();
    try {
      const state = recordTransientFailureForModel(tmp, { provider: "anthropic", id: "claude-sonnet-4-6" }, "429 rate limit", 1000);
      assert.ok(state?.providerCooldowns?.anthropic);
      assert.ok(state?.candidateCooldowns?.["anthropic/claude-sonnet-4-6"]);

      const persisted = toCapabilityRuntimeState(loadOperatorRuntimeState(tmp));
      assert.ok(persisted.providerCooldowns?.anthropic);
      assert.ok(persisted.candidateCooldowns?.["anthropic/claude-sonnet-4-6"]);
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });

  it("ignores non-transient and local failures", () => {
    const tmp = makeTmpDir();
    try {
      assert.equal(recordTransientFailureForModel(tmp, { provider: "openai", id: "gpt-5.4" }, "invalid api key", 1000), undefined);
      assert.equal(recordTransientFailureForModel(tmp, { provider: "local", id: "qwen3:8b" }, "overloaded", 1000), undefined);
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });
});

describe("buildFallbackGuidance", () => {
  it("suggests same-role cross-provider alternative after cooldown", () => {
    const models = [
      makeModel("anthropic", "claude-sonnet-4-6"),
      makeModel("openai", "gpt-5.3-codex-spark"),
    ];
    const profile = getDefaultCapabilityProfile(models);
    const runtimeState = {
      providerCooldowns: {
        anthropic: { until: 5000, reason: "429" },
      },
    };
    const guidance = buildFallbackGuidance(
      { provider: "anthropic", id: "claude-sonnet-4-6" },
      models,
      getDefaultPolicy(),
      profile,
      runtimeState,
      1000,
    );
    assert.equal(guidance?.ok, true);
    assert.equal(guidance?.alternateCandidate?.provider, "openai");
  });

  it("expires cooldown guidance after the window passes", () => {
    const models = [
      makeModel("anthropic", "claude-sonnet-4-6"),
      makeModel("openai", "gpt-5.3-codex-spark"),
    ];
    const profile = getDefaultCapabilityProfile(models);
    const guidance = buildFallbackGuidance(
      { provider: "anthropic", id: "claude-sonnet-4-6" },
      models,
      getDefaultPolicy(),
      profile,
      { providerCooldowns: { anthropic: { until: 1500, reason: "429" } } },
      2000,
    );
    assert.equal(guidance, undefined);
  });

  it("surfaces blocked heavy-local fallback guidance when policy forbids it", () => {
    const models = [
      makeModel("anthropic", "claude-haiku-3-5"),
      makeModel("local", "qwen3:30b"),
    ];
    const profile = getDefaultCapabilityProfile(models);
    profile.roles.adept.candidates = [
      { id: "claude-haiku-3-5", provider: "anthropic", source: "upstream", weight: "light", maxThinking: "low" },
      { id: "qwen3:30b", provider: "local", source: "local", weight: "heavy", maxThinking: "medium" },
    ];
    profile.policy.crossSource = "deny";
    profile.policy.heavyLocal = "deny";

    const guidance = buildFallbackGuidance(
      { provider: "anthropic", id: "claude-haiku-3-5" },
      models,
      getDefaultPolicy(),
      profile,
      { providerCooldowns: { anthropic: { until: 5000, reason: "429" } } },
      1000,
    );

    assert.equal(guidance?.ok, false);
    assert.match(guidance?.reason ?? "", /blocked by policy|not permitted/i);
  });
});

describe("explainTierResolutionFailure", () => {
  it("returns the policy explanation for blocked tier switches", () => {
    const models = [
      makeModel("anthropic", "claude-haiku-3-5"),
      makeModel("local", "qwen3:30b"),
    ];
    const profile = getDefaultCapabilityProfile(models);
    profile.roles.adept.candidates = [
      { id: "claude-haiku-3-5", provider: "anthropic", source: "upstream", weight: "light", maxThinking: "low" },
      { id: "qwen3:30b", provider: "local", source: "local", weight: "heavy", maxThinking: "medium" },
    ];
    profile.policy.crossSource = "deny";
    profile.policy.heavyLocal = "deny";

    const message = explainTierResolutionFailure(
      "haiku",
      models,
      getDefaultPolicy(),
      profile,
      { providerCooldowns: { anthropic: { until: 5000, reason: "429" } } },
      1000,
    );

    assert.match(message ?? "", /Unable to switch to Adept \[haiku\]/);
    assert.match(message ?? "", /blocked by policy|not permitted/i);
  });
});
