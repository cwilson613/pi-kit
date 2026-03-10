/**
 * effort/tiers — Pure functions for effort tier configuration.
 *
 * No side effects, no imports beyond types. Safe to call from any context.
 */

import type { EffortConfig, EffortLevel } from "./types.ts";
import { EFFORT_NAMES } from "./types.ts";

/**
 * Static tier configuration table.
 * Each entry is a complete EffortConfig for levels 1-7.
 *
 *  1 = Servitor        — all local
 *  2 = Average    — local driver, local background
 *  3 = Substantial — sonnet driver, local background (daily default)
 *  4 = Ruthless   — sonnet + medium thinking
 *  5 = Lethal     — sonnet + high thinking, opus review
 *  6 = Absolute   — opus driver, sonnet background
 *  7 = Omnissiah  — all opus
 */
const TIERS: Record<EffortLevel, EffortConfig> = {
  1: {
    level: 1,
    name: "Servitor",
    driver: "local",
    thinking: "off",
    extraction: "local",
    compaction: "local",
    cleavePreferLocal: true,
    cleaveFloor: "local",
    reviewModel: "local",
  },
  2: {
    level: 2,
    name: "Average",
    driver: "local",
    thinking: "minimal",
    extraction: "local",
    compaction: "local",
    cleavePreferLocal: false,
    cleaveFloor: "local",
    reviewModel: "local",
  },
  3: {
    level: 3,
    name: "Substantial",
    driver: "sonnet",
    thinking: "low",
    extraction: "local",
    compaction: "sonnet",
    cleavePreferLocal: false,
    cleaveFloor: "local",
    reviewModel: "sonnet",
  },
  4: {
    level: 4,
    name: "Ruthless",
    driver: "sonnet",
    thinking: "medium",
    extraction: "local",
    compaction: "sonnet",
    cleavePreferLocal: false,
    cleaveFloor: "local",
    reviewModel: "sonnet",
  },
  5: {
    level: 5,
    name: "Lethal",
    driver: "sonnet",
    thinking: "high",
    extraction: "local",
    compaction: "sonnet",
    cleavePreferLocal: false,
    cleaveFloor: "sonnet",
    reviewModel: "opus",
  },
  6: {
    level: 6,
    name: "Absolute",
    driver: "opus",
    thinking: "high",
    extraction: "sonnet",
    compaction: "sonnet",
    cleavePreferLocal: false,
    cleaveFloor: "sonnet",
    reviewModel: "opus",
  },
  7: {
    level: 7,
    name: "Omnissiah",
    driver: "opus",
    thinking: "high",
    extraction: "opus",
    compaction: "opus",
    cleavePreferLocal: false,
    cleaveFloor: "opus",
    reviewModel: "opus",
  },
};

/**
 * Get the EffortConfig for a given numeric level.
 *
 * @param level - Effort level 1-7
 * @returns EffortConfig for the requested level (shared reference — do not mutate)
 * @throws RangeError if level is outside 1-7
 */
export function tierConfig(level: number): EffortConfig {
  if (level < 1 || level > 7 || !Number.isInteger(level)) {
    throw new RangeError(`Effort level must be an integer 1-7, got ${level}`);
  }
  return TIERS[level as EffortLevel];
}

/** Reverse lookup: tier name (case-insensitive) → level number. */
const NAME_TO_LEVEL: ReadonlyMap<string, EffortLevel> = new Map(
  (Object.entries(EFFORT_NAMES) as [string, string][]).map(([k, v]) => [
    v.toLowerCase(),
    Number(k) as EffortLevel,
  ]),
);

/**
 * Parse a tier name string to its numeric level.
 *
 * Case-insensitive. Returns `undefined` for unknown names.
 *
 * @param name - Tier name like "Ruthless", "low", "OMNISSIAH"
 * @returns Numeric level 1-7, or undefined if not recognized
 */
export function parseTierName(name: string): EffortLevel | undefined {
  return NAME_TO_LEVEL.get(name.toLowerCase());
}

/** Default effort level when no env var or config is present. */
export const DEFAULT_EFFORT_LEVEL: EffortLevel = 3;

/** All valid tier names for display/error messages. */
export const TIER_NAMES: readonly string[] = Object.values(EFFORT_NAMES);

/** Minimum valid effort level. */
export const MIN_LEVEL: EffortLevel = 1;

/** Maximum valid effort level. */
export const MAX_LEVEL: EffortLevel = 7;
