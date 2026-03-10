/**
 * Pure helpers for compaction policy and prompt hygiene.
 */

import type { MemoryConfig } from "./types.ts";

/**
 * Redact transient clipboard image temp paths captured from pi's clipboard paste flow.
 * These files live under macOS temp directories and become stale/noisy immediately.
 */
export function sanitizeCompactionText(input: string): string {
  return input.replace(
    /\/var\/folders\/[A-Za-z0-9_-]+\/[A-Za-z0-9_-]+\/T\/pi-clipboard-[A-Fa-f0-9-]+\.(?:png|jpe?g|gif|webp)/g,
    "[clipboard image attachment]",
  );
}

/**
 * Whether project-memory should intercept compaction before pi core.
 * Local interception is only enabled for explicit local policy or fallback retry.
 */
export function shouldInterceptCompaction(
  effortCompaction: "local" | "haiku" | "sonnet" | "opus" | undefined,
  config: MemoryConfig,
  useLocalCompaction: boolean,
): boolean {
  const liveCompactionLocal = effortCompaction ? effortCompaction === "local" : config.compactionLocalFirst;
  return (liveCompactionLocal || useLocalCompaction) && config.compactionLocalFallback;
}
