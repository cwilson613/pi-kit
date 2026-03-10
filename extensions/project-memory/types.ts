/**
 * Project Memory — Types
 */

export interface LifecycleMemoryMessage {
  source: "design-tree" | "openspec" | "cleave";
  context: string;
  candidates: LifecycleMemoryCandidate[];
}

export interface LifecycleMemoryCandidate {
  sourceKind: "design-decision" | "design-constraint" | "openspec-archive" | "openspec-assess" | "cleave-outcome" | "cleave-bug-fix";
  authority: "explicit" | "inferred";
  section: "Architecture" | "Decisions" | "Constraints" | "Known Issues" | "Patterns & Conventions" | "Specs";
  content: string;
  artifactRef?: {
    type: "design-node" | "openspec-spec" | "openspec-baseline" | "cleave-review";
    path: string;
    subRef?: string;
  };
  supersedes?: string;
  session?: string;
}

export interface MemoryConfig {
  /** Max lines in active memory before extraction prunes */
  maxLines: number;
  /** Embedding backend used for semantic retrieval */
  embeddingProvider: "openai" | "ollama";
  /** Embedding model to use for fact and episode vectors */
  embeddingModel: string;
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
  /** Context % at which degeneracy pressure begins (gradient onset) */
  pressureOnsetPercent: number;
  /** Context % at which to warn the agent to consider compacting */
  compactionWarningPercent: number;
  /** Context % at which to auto-compact without asking */
  compactionAutoPercent: number;
  /** Use local model as fallback when cloud compaction fails */
  compactionLocalFallback: boolean;
  /** Try local model FIRST for compaction (cloud only if local unavailable) */
  compactionLocalFirst: boolean;
  /** Timeout for local model compaction inference in ms */
  compactionLocalTimeout: number;
  /** Enable intelligent fallback chain: local → gpt-5.3-codex-spark → haiku */
  compactionFallbackChain: boolean;
  /** Timeout for gpt-5.3-codex-spark compaction fallback in ms */
  compactionCodexTimeout: number;
  /** Timeout for haiku compaction fallback in ms */
  compactionHaikuTimeout: number;
  /** Max consecutive compaction retry attempts before giving up for the session */
  compactionRetryLimit: number;
  /** Enable Phase 2 global extraction (generalizes project facts to user-level store) */
  globalExtractionEnabled: boolean;
}

export const DEFAULT_CONFIG: MemoryConfig = {
  maxLines: 50,
  embeddingProvider: "openai",
  embeddingModel: "text-embedding-3-small",
  minimumTokensToInit: 10_000,
  minimumTokensBetweenUpdate: 5_000,
  toolCallsBetweenUpdates: 8,
  manualStoreThreshold: 3,
  extractionModel: "gpt-5.3-codex-spark",
  extractionTimeout: 60_000,
  shutdownExtractionTimeout: 15_000,
  pressureOnsetPercent: 55,
  compactionWarningPercent: 75,
  compactionAutoPercent: 85,
  compactionLocalFallback: true,
  compactionLocalFirst: false,
  compactionLocalTimeout: 45_000,
  compactionFallbackChain: true,
  compactionCodexTimeout: 60_000,
  compactionHaikuTimeout: 30_000,
  compactionRetryLimit: 3,
  globalExtractionEnabled: false,
};

