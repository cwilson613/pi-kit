import type { MemoryInjectionMetrics } from "../project-memory/injection-metrics.ts";

export function formatMemoryAuditSummary(
  metrics: MemoryInjectionMetrics | undefined,
  opts?: { wide?: boolean },
): string {
  if (!metrics) {
    return "Memory · pending first injection";
  }

  const wide = opts?.wide ?? false;
  if (wide) {
    return [
      `Memory audit: ${metrics.mode}`,
      `facts:${metrics.projectFactCount}`,
      `edges:${metrics.edgeCount}`,
      `wm:${metrics.workingMemoryFactCount}`,
      `hits:${metrics.semanticHitCount}`,
      `ep:${metrics.episodeCount}`,
      `global:${metrics.globalFactCount}`,
      `chars:${metrics.payloadChars}`,
      `~${metrics.estimatedTokens} tok`,
    ].join(" · ");
  }

  return [
    `Memory ${metrics.mode}`,
    `facts:${metrics.projectFactCount}`,
    `wm:${metrics.workingMemoryFactCount}`,
    `ep:${metrics.episodeCount}`,
    `global:${metrics.globalFactCount}`,
    `~${metrics.estimatedTokens} tok`,
  ].join(" · ");
}
