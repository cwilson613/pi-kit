/**
 * Project Memory — Types
 */

export interface MemoryConfig {
  /** Max lines in active memory before extraction prunes */
  maxLines: number;
  /** Minimum total message tokens before first extraction */
  minimumTokensToInit: number;
  /** Token delta required between extractions */
  minimumTokensBetweenUpdate: number;
  /** Minimum tool calls since last extraction */
  toolCallsBetweenUpdates: number;
  /** Skip auto-extraction if LLM stored >= this many facts since last extraction */
  manualStoreThreshold: number;
  /** Model to use for extraction subagent */
  extractionModel: string;
  /** Timeout for extraction subprocess in ms */
  extractionTimeout: number;
  /** Timeout for shutdown extraction in ms (shorter — blocks exit) */
  shutdownExtractionTimeout: number;
}

export const DEFAULT_CONFIG: MemoryConfig = {
  maxLines: 80,
  minimumTokensToInit: 10_000,
  minimumTokensBetweenUpdate: 5_000,
  toolCallsBetweenUpdates: 3,
  manualStoreThreshold: 3,
  extractionModel: "claude-opus-4-6",
  extractionTimeout: 60_000,
  shutdownExtractionTimeout: 15_000,
};

