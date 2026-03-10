import type { Model } from "@mariozechner/pi-ai";
import {
  classifyUpstreamFailure,
  resolveCapabilityRole,
  getTierDisplayLabel,
  withCandidateCooldown,
  withProviderCooldown,
  type CapabilityCandidate,
  type CapabilityProfile,
  type CapabilityRole,
  type CapabilityRuntimeState,
  type ModelTier,
  type ProviderRoutingPolicy,
  type RegistryModel,
  type UpstreamFailureClassification,
} from "./model-routing.ts";
import {
  fromCapabilityRuntimeState,
  loadOperatorRuntimeState,
  saveOperatorRuntimeState,
  toCapabilityRuntimeState,
  type RuntimeFallbackGuidance,
} from "./operator-profile.ts";

const ROLE_ORDER: CapabilityRole[] = ["archmagos", "magos", "adept", "servitor", "servoskull"];
const TIER_ROLE_MAP: Record<Exclude<ModelTier, "local">, CapabilityRole> = {
  opus: "archmagos",
  sonnet: "magos",
  haiku: "adept",
};

export interface RecoveryPlan {
  classification: UpstreamFailureClassification;
  role?: CapabilityRole;
  action: "retry-same-model" | "switch-model" | "handoff-local" | "surface" | "handled-elsewhere";
  sameModelRetry: boolean;
  requiresConfirmation?: boolean;
  reason: string;
  alternateCandidate?: {
    provider: CapabilityCandidate["provider"];
    id: string;
  };
}

function normalizeProvider(provider: string): "anthropic" | "openai" | "local" | undefined {
  if (provider === "anthropic" || provider === "openai" || provider === "local") return provider;
  if (provider === "ollama") return "local";
  return undefined;
}

function currentModelKey(model: Pick<Model<any>, "provider" | "id">): string | undefined {
  const provider = normalizeProvider(model.provider);
  if (!provider) return undefined;
  return `${provider}/${model.id}`;
}

export function inferRolesForModel(model: Pick<Model<any>, "provider" | "id">, profile: CapabilityProfile): CapabilityRole[] {
  const key = currentModelKey(model);
  if (!key) return [];
  return ROLE_ORDER.filter((role) => profile.roles[role].candidates.some((candidate) => `${candidate.provider}/${candidate.id}` === key));
}

export function planRecoveryForModel(
  model: Pick<Model<any>, "provider" | "id">,
  failure: unknown,
  models: RegistryModel[],
  policy: ProviderRoutingPolicy,
  profile: CapabilityProfile,
  runtimeState: CapabilityRuntimeState,
  now: number = Date.now(),
): RecoveryPlan {
  const classification = classifyUpstreamFailure(failure);
  const [role] = inferRolesForModel(model, profile);

  if (classification.recoveryAction === "retry-same-model") {
    return {
      classification,
      role,
      action: "retry-same-model",
      sameModelRetry: true,
      reason: classification.reason,
    };
  }

  if (classification.recoveryAction === "handled-elsewhere") {
    return {
      classification,
      role,
      action: "handled-elsewhere",
      sameModelRetry: false,
      reason: classification.reason,
    };
  }

  if (classification.recoveryAction === "failover" && role) {
    const resolution = resolveCapabilityRole(role, models, policy, profile, runtimeState, now);
    if (resolution.ok && resolution.selected) {
      const selected = resolution.selected.candidate;
      if (selected.id !== model.id || selected.provider !== normalizeProvider(model.provider)) {
        return {
          classification,
          role,
          action: selected.provider === "local" ? "handoff-local" : "switch-model",
          sameModelRetry: false,
          reason: classification.reason,
          alternateCandidate: {
            provider: selected.provider,
            id: selected.id,
          },
        };
      }
    }

    return {
      classification,
      role,
      action: "surface",
      sameModelRetry: false,
      requiresConfirmation: resolution.requiresConfirmation,
      reason: resolution.reason ?? classification.reason,
    };
  }

  return {
    classification,
    role,
    action: "surface",
    sameModelRetry: false,
    reason: classification.reason,
  };
}

export function buildFallbackGuidance(
  model: Pick<Model<any>, "provider" | "id">,
  models: RegistryModel[],
  policy: ProviderRoutingPolicy,
  profile: CapabilityProfile,
  runtimeState: CapabilityRuntimeState,
  now: number = Date.now(),
): RuntimeFallbackGuidance | undefined {
  const [role] = inferRolesForModel(model, profile);
  if (!role) return undefined;
  const resolution = resolveCapabilityRole(role, models, policy, profile, runtimeState, now);
  if (resolution.ok && resolution.selected) {
    const selected = resolution.selected.candidate;
    if (selected.id === model.id && selected.provider === normalizeProvider(model.provider)) return undefined;
    return {
      role,
      ok: true,
      alternateCandidate: {
        provider: selected.provider,
        id: selected.id,
      },
    };
  }
  return {
    role,
    ok: false,
    requiresConfirmation: resolution.requiresConfirmation,
    reason: resolution.reason,
  };
}

export function explainTierResolutionFailure(
  tier: ModelTier,
  models: RegistryModel[],
  policy: ProviderRoutingPolicy,
  profile: CapabilityProfile,
  runtimeState: CapabilityRuntimeState,
  now: number = Date.now(),
): string | undefined {
  if (tier === "local") return undefined;
  const resolution = resolveCapabilityRole(TIER_ROLE_MAP[tier], models, policy, profile, runtimeState, now);
  if (resolution.ok || !resolution.reason) return undefined;
  return `Unable to switch to ${getTierDisplayLabel(tier)} [${tier}]: ${resolution.reason}`;
}

export function recordTransientFailureForModel(
  root: string,
  model: Pick<Model<any>, "provider" | "id">,
  reason: string,
  now: number = Date.now(),
): CapabilityRuntimeState | undefined {
  const classification = classifyUpstreamFailure(reason);
  if (!classification.cooldownProvider && !classification.cooldownCandidate) return undefined;

  const provider = normalizeProvider(model.provider);
  if (!provider || provider === "local") return undefined;

  let state = toCapabilityRuntimeState(loadOperatorRuntimeState(root));
  if (classification.cooldownProvider) {
    state = withProviderCooldown(state, provider, reason, now);
  }
  if (classification.cooldownCandidate) {
    state = withCandidateCooldown(state, {
      id: model.id,
      provider,
      source: "upstream",
      weight: "normal",
      maxThinking: "high",
    }, reason, now);
  }
  saveOperatorRuntimeState(root, fromCapabilityRuntimeState(state));
  return state;
}
