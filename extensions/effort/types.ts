/**
 * effort/types — Type definitions for the effort-tiers system.
 *
 * Effort tiers provide 7-level global inference cost control,
 * from fully-local (Servitor) to all-opus (Omnissiah).
 */

// ─── Model Tiers ─────────────────────────────────────────────

/** Model tier for effort configuration — which model class to use. */
export type EffortModelTier = "local" | "sonnet" | "opus";

/** Thinking level passed to the driver model. */
export type ThinkingLevel = "off" | "minimal" | "low" | "medium" | "high";

// ─── Effort Level ────────────────────────────────────────────

/** Numeric effort levels 1-7. */
export type EffortLevel = 1 | 2 | 3 | 4 | 5 | 6 | 7;

/** Human-readable tier names, indexed by level. */
export const EFFORT_NAMES = {
  1: "Servitor",
  2: "Average",
  3: "Substantial",
  4: "Ruthless",
  5: "Lethal",
  6: "Absolute",
  7: "Omnissiah",
} as const satisfies Record<EffortLevel, string>;

/** Union of valid tier name strings. */
export type EffortName = (typeof EFFORT_NAMES)[EffortLevel];

// ─── Effort Config ───────────────────────────────────────────

/**
 * Immutable configuration for a single effort tier.
 * Returned by `tierConfig(level)` — pure data, no side effects.
 */
export interface EffortConfig {
  /** Numeric level 1-7. */
  level: EffortLevel;
  /** Human-readable tier name. */
  name: EffortName;

  // ── Model selection ──
  /** Primary driver model. */
  driver: EffortModelTier;
  /** Extended thinking budget for the driver. */
  thinking: ThinkingLevel;

  // ── Background tasks ──
  /** Model for memory extraction / summarization. */
  extraction: EffortModelTier;
  /** Model for context compaction. */
  compaction: EffortModelTier;

  // ── Cleave dispatch ──
  /** Whether cleave should prefer local models for child tasks. */
  cleavePreferLocal: boolean;
  /** Minimum model tier for cleave child tasks. */
  cleaveFloor: EffortModelTier;

  // ── Review ──
  /** Model used for code review in cleave review loops. */
  reviewModel: EffortModelTier;
}

// ─── Effort State ────────────────────────────────────────────

/**
 * Runtime effort state stored in SharedState.
 * Extends EffortConfig with cap tracking.
 */
export interface EffortState extends EffortConfig {
  /** Whether the effort level is capped (ceiling-locked by operator). */
  capped: boolean;
  /** If capped, the level at which the cap is set. */
  capLevel?: EffortLevel;
}
