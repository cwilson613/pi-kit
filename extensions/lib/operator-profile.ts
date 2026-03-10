import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import type {
  CapabilityProfile,
  CapabilityRuntimeState,
  CapabilityCandidate,
  ProviderName,
  CooldownEntry,
} from "./model-routing.ts";
import { loadPiConfig, savePiConfig, type PiConfig } from "./model-preferences.ts";

export const CAPABILITY_ROLES = ["archmagos", "magos", "adept", "servitor", "servoskull"] as const;

export type CapabilityRole = typeof CAPABILITY_ROLES[number];
export type CapabilityRoleAlias = "opus" | "sonnet" | "haiku" | "local" | "servo-skull";
export type CandidateSource = "upstream" | "local";
export type CandidateWeight = "light" | "normal" | "heavy";
export type ThinkingLevel = "off" | "minimal" | "low" | "medium" | "high";
export type FallbackPolicyValue = "allow" | "ask" | "deny";

export interface OperatorProfileCandidate {
  id?: string;
  provider?: string;
  source?: CandidateSource;
  weight?: CandidateWeight;
  maxThinking?: ThinkingLevel;
}

export interface OperatorFallbackPolicy {
  sameRoleCrossProvider: FallbackPolicyValue;
  // Reserved for future values like "allow_once" and "background_only".
  crossSource: FallbackPolicyValue;
  // Reserved for future values like "allow_once" and "background_only".
  heavyLocal: FallbackPolicyValue;
  // Reserved for future values like "allow_once" and "background_only".
  unknownLocalPerformance: FallbackPolicyValue;
}

export type OperatorRoleMap = Record<CapabilityRole, OperatorProfileCandidate[]>;

export interface OperatorCapabilityProfile {
  roles: OperatorRoleMap;
  fallback: OperatorFallbackPolicy;
  setupComplete?: boolean;
}

export interface CandidateCooldownState {
  until: string;
  reason?: string;
}

export interface OperatorRuntimeState {
  providers?: Record<string, CandidateCooldownState>;
  candidates?: Record<string, CandidateCooldownState>;
}

export interface RuntimeFallbackGuidance {
  role: CapabilityRole;
  ok: boolean;
  requiresConfirmation?: boolean;
  reason?: string;
  alternateCandidate?: {
    provider: ProviderName;
    id: string;
  };
}

const DEFAULT_FALLBACK_POLICY: OperatorFallbackPolicy = {
  sameRoleCrossProvider: "allow",
  crossSource: "ask",
  heavyLocal: "deny",
  unknownLocalPerformance: "ask",
};

const DEFAULT_PROFILE: OperatorCapabilityProfile = {
  roles: {
    archmagos: [
      { id: "claude-opus-4-6", provider: "anthropic", source: "upstream", weight: "heavy", maxThinking: "high" },
      { id: "gpt-5.4", provider: "openai", source: "upstream", weight: "heavy", maxThinking: "high" },
    ],
    magos: [
      { id: "claude-sonnet-4-6", provider: "anthropic", source: "upstream", weight: "normal", maxThinking: "medium" },
      { id: "gpt-5.3-codex-spark", provider: "openai", source: "upstream", weight: "normal", maxThinking: "medium" },
    ],
    adept: [
      { id: "claude-haiku-3-5", provider: "anthropic", source: "upstream", weight: "light", maxThinking: "low" },
      { id: "gpt-5.1-codex", provider: "openai", source: "upstream", weight: "light", maxThinking: "low" },
    ],
    servitor: [
      { id: "gpt-4o-mini", provider: "openai", source: "upstream", weight: "light", maxThinking: "minimal" },
      { id: "claude-haiku-3-5", provider: "anthropic", source: "upstream", weight: "light", maxThinking: "minimal" },
    ],
    servoskull: [
      { id: "qwen3:8b", provider: "local", source: "local", weight: "light", maxThinking: "off" },
    ],
  },
  fallback: DEFAULT_FALLBACK_POLICY,
  setupComplete: false,
};

function deepCloneDefaultProfile(): OperatorCapabilityProfile {
  return JSON.parse(JSON.stringify(DEFAULT_PROFILE)) as OperatorCapabilityProfile;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === "object" && !Array.isArray(value);
}

function isCapabilityRole(value: string): value is CapabilityRole {
  return (CAPABILITY_ROLES as readonly string[]).includes(value);
}

function parseCandidate(value: unknown): OperatorProfileCandidate | undefined {
  if (!isRecord(value)) return undefined;
  const candidate: OperatorProfileCandidate = {};
  if (typeof value.id === "string") candidate.id = value.id;
  if (typeof value.provider === "string") candidate.provider = value.provider;
  if (value.source === "upstream" || value.source === "local") {
    candidate.source = value.source;
  } else if (value.source === "frontier") {
    candidate.source = "upstream";
  }
  if (value.weight === "light" || value.weight === "normal" || value.weight === "heavy") {
    candidate.weight = value.weight;
  } else if (typeof value.weight === "number" && Number.isFinite(value.weight)) {
    candidate.weight = value.weight >= 90 ? "heavy" : value.weight >= 50 ? "normal" : "light";
  }
  if (["off", "minimal", "low", "medium", "high"].includes(String(value.maxThinking))) {
    candidate.maxThinking = value.maxThinking as ThinkingLevel;
  }
  return Object.keys(candidate).length > 0 ? candidate : undefined;
}

function parseFallbackValue(value: unknown, fallback: FallbackPolicyValue): FallbackPolicyValue {
  return value === "allow" || value === "ask" || value === "deny" ? value : fallback;
}

function normalizeProviderName(provider: string | undefined): ProviderName | undefined {
  if (provider === "anthropic" || provider === "openai") return provider;
  if (provider === "local" || provider === "ollama") return "local";
  return undefined;
}

function normalizeCandidate(candidate: OperatorProfileCandidate): CapabilityCandidate | undefined {
  if (!candidate.id) return undefined;
  const provider = normalizeProviderName(candidate.provider);
  if (!provider) return undefined;
  return {
    id: candidate.id,
    provider,
    source: candidate.source ?? (provider === "local" ? "local" : "upstream"),
    weight: candidate.weight ?? "normal",
    maxThinking: candidate.maxThinking ?? "medium",
  };
}

function normalizeCooldownEntry(value: CandidateCooldownState): CooldownEntry | undefined {
  const until = Date.parse(value.until);
  if (!Number.isFinite(until)) return undefined;
  return {
    until,
    reason: value.reason,
  };
}

export function getDefaultOperatorProfile(): OperatorCapabilityProfile {
  return deepCloneDefaultProfile();
}

export function parseOperatorProfile(raw: unknown): OperatorCapabilityProfile {
  const profile = deepCloneDefaultProfile();
  if (!isRecord(raw)) return profile;

  if (isRecord(raw.roles)) {
    for (const [key, value] of Object.entries(raw.roles)) {
      if (!isCapabilityRole(key) || !Array.isArray(value)) continue;
      const parsed = value
        .map((candidate) => parseCandidate(candidate))
        .filter((candidate): candidate is OperatorProfileCandidate => !!candidate);
      if (parsed.length > 0) profile.roles[key] = parsed;
    }
  }

  if (isRecord(raw.fallback)) {
    profile.fallback = {
      sameRoleCrossProvider: parseFallbackValue(raw.fallback.sameRoleCrossProvider, profile.fallback.sameRoleCrossProvider),
      crossSource: parseFallbackValue(raw.fallback.crossSource, profile.fallback.crossSource),
      heavyLocal: parseFallbackValue(raw.fallback.heavyLocal, profile.fallback.heavyLocal),
      unknownLocalPerformance: parseFallbackValue(raw.fallback.unknownLocalPerformance, profile.fallback.unknownLocalPerformance),
    };
  }

  if (typeof raw.setupComplete === "boolean") profile.setupComplete = raw.setupComplete;
  return profile;
}

export function readOperatorProfile(root: string): OperatorCapabilityProfile {
  return parseOperatorProfile(loadPiConfig(root).operatorProfile);
}

export function writeOperatorProfile(root: string, profile: OperatorCapabilityProfile): void {
  const config: PiConfig = loadPiConfig(root);
  config.operatorProfile = parseOperatorProfile(profile);
  savePiConfig(root, config);
}

function runtimeStatePath(root: string): string {
  return join(root, ".pi", "runtime", "operator-profile.json");
}

export function loadOperatorRuntimeState(root: string): OperatorRuntimeState {
  try {
    const path = runtimeStatePath(root);
    if (!existsSync(path)) return {};
    const raw = JSON.parse(readFileSync(path, "utf-8"));
    return parseOperatorRuntimeState(raw);
  } catch {
    return {};
  }
}

export function saveOperatorRuntimeState(root: string, state: OperatorRuntimeState): void {
  const dir = join(root, ".pi", "runtime");
  mkdirSync(dir, { recursive: true });
  writeFileSync(runtimeStatePath(root), JSON.stringify(parseOperatorRuntimeState(state), null, 2) + "\n", "utf-8");
}

export function parseOperatorRuntimeState(raw: unknown): OperatorRuntimeState {
  if (!isRecord(raw)) return {};
  const normalize = (value: unknown): Record<string, CandidateCooldownState> | undefined => {
    if (!isRecord(value)) return undefined;
    const entries: [string, CandidateCooldownState][] = [];
    for (const [key, candidate] of Object.entries(value)) {
      if (!isRecord(candidate) || typeof candidate.until !== "string") continue;
      entries.push([
        key,
        {
          until: candidate.until,
          reason: typeof candidate.reason === "string" ? candidate.reason : undefined,
        },
      ]);
    }
    return entries.length > 0 ? Object.fromEntries(entries) : undefined;
  };

  return {
    providers: normalize(raw.providers),
    candidates: normalize(raw.candidates),
  };
}

export function resolveRoleAlias(role: CapabilityRole | CapabilityRoleAlias): CapabilityRole {
  switch (role) {
    case "opus":
      return "archmagos";
    case "sonnet":
      return "magos";
    case "haiku":
      return "adept";
    case "local":
    case "servo-skull":
      return "servoskull";
    default:
      return role;
  }
}

export function toCapabilityProfile(profile: OperatorCapabilityProfile): CapabilityProfile {
  return {
    roles: {
      archmagos: { candidates: profile.roles.archmagos.map(normalizeCandidate).filter((c): c is CapabilityCandidate => !!c) },
      magos: { candidates: profile.roles.magos.map(normalizeCandidate).filter((c): c is CapabilityCandidate => !!c) },
      adept: { candidates: profile.roles.adept.map(normalizeCandidate).filter((c): c is CapabilityCandidate => !!c) },
      servitor: { candidates: profile.roles.servitor.map(normalizeCandidate).filter((c): c is CapabilityCandidate => !!c) },
      servoskull: { candidates: profile.roles.servoskull.map(normalizeCandidate).filter((c): c is CapabilityCandidate => !!c) },
    },
    internalAliases: {
      opus: "archmagos",
      sonnet: "magos",
      haiku: "adept",
      local: "servitor",
      review: "archmagos",
      planning: "archmagos",
      compaction: "servitor",
      extraction: "servitor",
      "cleave.leaf": "adept",
      summary: "servoskull",
      background: "servoskull",
    },
    policy: {
      sameRoleCrossProvider: profile.fallback.sameRoleCrossProvider,
      crossSource: profile.fallback.crossSource,
      heavyLocal: profile.fallback.heavyLocal,
      unknownLocalPerformance: profile.fallback.unknownLocalPerformance,
    },
  };
}

export function toCapabilityRuntimeState(state: OperatorRuntimeState): CapabilityRuntimeState {
  const providerCooldowns = state.providers
    ? Object.fromEntries(
        Object.entries(state.providers)
          .map(([provider, value]) => {
            const normalizedProvider = normalizeProviderName(provider);
            const entry = normalizeCooldownEntry(value);
            return normalizedProvider && entry ? [normalizedProvider, entry] : null;
          })
          .filter((entry): entry is [ProviderName, CooldownEntry] => !!entry),
      )
    : undefined;

  const candidateCooldowns = state.candidates
    ? Object.fromEntries(
        Object.entries(state.candidates)
          .map(([key, value]) => {
            const entry = normalizeCooldownEntry(value);
            if (!entry) return null;
            const normalizedKey = key.replace(/^([^:/]+):/, "$1/");
            return [normalizedKey, entry] as const;
          })
          .filter((entry): entry is readonly [string, CooldownEntry] => !!entry),
      )
    : undefined;

  return {
    providerCooldowns: providerCooldowns && Object.keys(providerCooldowns).length > 0 ? providerCooldowns : undefined,
    candidateCooldowns: candidateCooldowns && Object.keys(candidateCooldowns).length > 0 ? candidateCooldowns : undefined,
  };
}

export function fromCapabilityRuntimeState(state: CapabilityRuntimeState): OperatorRuntimeState {
  const providerEntries: Array<[string, CandidateCooldownState]> = [];
  for (const [provider, entry] of Object.entries(state.providerCooldowns ?? {})) {
    if (!entry) continue;
    providerEntries.push([
      provider,
      {
        until: new Date(entry.until).toISOString(),
        reason: entry.reason,
      },
    ]);
  }

  const candidateEntries: Array<[string, CandidateCooldownState]> = [];
  for (const [key, entry] of Object.entries(state.candidateCooldowns ?? {})) {
    candidateEntries.push([
      key,
      {
        until: new Date(entry.until).toISOString(),
        reason: entry.reason,
      },
    ]);
  }

  return {
    providers: providerEntries.length > 0 ? Object.fromEntries(providerEntries) : undefined,
    candidates: candidateEntries.length > 0 ? Object.fromEntries(candidateEntries) : undefined,
  };
}
