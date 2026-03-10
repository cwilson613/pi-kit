import type { Model } from "@mariozechner/pi-ai";
import {
  classifyTransientFailure,
  resolveCapabilityRole,
  getTierDisplayLabel,
  withCandidateCooldown,
  withProviderCooldown,
  type CapabilityProfile,
  type CapabilityRole,
  type CapabilityRuntimeState,
  type ModelTier,
  type ProviderRoutingPolicy,
  type RegistryModel,
} from "./model-routing.ts";
import { fromCapabilityRuntimeState, loadOperatorRuntimeState, saveOperatorRuntimeState, toCapabilityRuntimeState, type RuntimeFallbackGuidance } from "./operator-profile.ts";

const ROLE_ORDER: CapabilityRole[] = ["archmagos", "magos", "adept", "servitor", "servoskull"];
const TIER_ROLE_MAP: Record<Exclude<ModelTier, "local">, CapabilityRole> = {
  opus: "archmagos",
  sonnet: "magos",
  haiku: "adept",
};

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
  if (!classifyTransientFailure(reason)) return undefined;
  const provider = normalizeProvider(model.provider);
  if (!provider || provider === "local") return undefined;

  let state = toCapabilityRuntimeState(loadOperatorRuntimeState(root));
  state = withProviderCooldown(state, provider, reason, now);
  state = withCandidateCooldown(state, {
    id: model.id,
    provider,
    source: "upstream",
    weight: "normal",
    maxThinking: "high",
  }, reason, now);
  saveOperatorRuntimeState(root, fromCapabilityRuntimeState(state));
  return state;
}
