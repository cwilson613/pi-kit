import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { getDefaultCapabilityProfile, getDefaultPolicy, type RegistryModel } from "./model-routing.ts";
import {
  buildFallbackGuidance,
  explainTierResolutionFailure,
  inferRolesForModel,
  planRecoveryForModel,
  recordTransientFailureForModel,
} from "./operator-fallback.ts";
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
  it("persists provider and candidate cooldowns for rate-limit failures", () => {
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

  it("does not cooldown retryable flakes, non-transient failures, or local failures", () => {
    const tmp = makeTmpDir();
    try {
      assert.equal(recordTransientFailureForModel(tmp, { provider: "openai", id: "gpt-5.4" }, "server_error", 1000), undefined);
      assert.equal(recordTransientFailureForModel(tmp, { provider: "openai", id: "gpt-5.4" }, "invalid api key", 1000), undefined);
      assert.equal(recordTransientFailureForModel(tmp, { provider: "local", id: "qwen3:8b" }, "429 rate limit", 1000), undefined);
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });
});

describe("planRecoveryForModel", () => {
  it("returns same-model retry for obvious upstream flakiness", () => {
    const models = [makeModel("anthropic", "claude-sonnet-4-6")];
    const profile = getDefaultCapabilityProfile(models);
    const plan = planRecoveryForModel(
      { provider: "anthropic", id: "claude-sonnet-4-6" },
      "server_error",
      models,
      getDefaultPolicy(),
      profile,
      {},
      1000,
    );

    assert.equal(plan.classification.class, "retryable-flake");
    assert.equal(plan.action, "retry-same-model");
    assert.equal(plan.sameModelRetry, true);
  });

  it("switches to an alternate candidate after rate-limit cooldown", () => {
    const models = [
      makeModel("anthropic", "claude-sonnet-4-6"),
      makeModel("openai", "gpt-5.3-codex-spark"),
    ];
    const profile = getDefaultCapabilityProfile(models);
    const plan = planRecoveryForModel(
      { provider: "anthropic", id: "claude-sonnet-4-6" },
      "429 rate limit",
      models,
      getDefaultPolicy(),
      profile,
      { providerCooldowns: { anthropic: { until: 5000, reason: "429" } } },
      1000,
    );

    assert.equal(plan.classification.class, "rate-limit");
    assert.equal(plan.action, "switch-model");
    assert.equal(plan.sameModelRetry, false);
    assert.equal(plan.alternateCandidate?.provider, "openai");
  });

  it("may hand off to local when only local remains viable", () => {
    const models = [
      makeModel("anthropic", "claude-haiku-3-5"),
      makeModel("local", "qwen3:8b"),
    ];
    const profile = getDefaultCapabilityProfile(models);
    profile.roles.adept.candidates = [
      { id: "claude-haiku-3-5", provider: "anthropic", source: "upstream", weight: "light", maxThinking: "low" },
      { id: "qwen3:8b", provider: "local", source: "local", weight: "normal", maxThinking: "medium" },
    ];
    profile.policy.crossSource = "allow";

    const plan = planRecoveryForModel(
      { provider: "anthropic", id: "claude-haiku-3-5" },
      "try again later",
      models,
      getDefaultPolicy(),
      profile,
      { providerCooldowns: { anthropic: { until: 5000, reason: "backoff" } } },
      1000,
    );

    assert.equal(plan.classification.class, "backoff");
    assert.equal(plan.action, "handoff-local");
    assert.equal(plan.alternateCandidate?.provider, "local");
  });

  it("surfaces non-retryable failures without generic retry guidance", () => {
    const models = [makeModel("anthropic", "claude-haiku-3-5")];
    const profile = getDefaultCapabilityProfile(models);

    const auth = planRecoveryForModel(
      { provider: "anthropic", id: "claude-haiku-3-5" },
      "invalid api key",
      models,
      getDefaultPolicy(),
      profile,
      {},
      1000,
    );
    const overflow = planRecoveryForModel(
      { provider: "anthropic", id: "claude-haiku-3-5" },
      "maximum context length exceeded",
      models,
      getDefaultPolicy(),
      profile,
      {},
      1000,
    );

    assert.equal(auth.action, "surface");
    assert.equal(auth.sameModelRetry, false);
    assert.equal(auth.classification.class, "auth");

    assert.equal(overflow.action, "handled-elsewhere");
    assert.equal(overflow.sameModelRetry, false);
    assert.equal(overflow.classification.class, "context-overflow");
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
