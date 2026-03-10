export type MemoryInjectionMode = "tiny" | "bulk" | "semantic";

export interface MemoryInjectionMetrics {
  mode: MemoryInjectionMode;
  projectFactCount: number;
  edgeCount: number;
  workingMemoryFactCount: number;
  semanticHitCount: number;
  episodeCount: number;
  globalFactCount: number;
  payloadChars: number;
  estimatedTokens: number;
  baselineContextTokens?: number | null;
  userPromptTokensEstimate?: number;
  observedInputTokens?: number;
  inferredAdditionalPromptTokens?: number | null;
  estimatedVsObservedDelta?: number | null;
}

export function estimateTokensFromChars(content: string): number {
  return Math.round(content.length / 4);
}

export function createMemoryInjectionMetrics(input: {
  mode: MemoryInjectionMode;
  projectFactCount?: number;
  edgeCount?: number;
  workingMemoryFactCount?: number;
  semanticHitCount?: number;
  episodeCount?: number;
  globalFactCount?: number;
  payloadChars: number;
  baselineContextTokens?: number | null;
  userPromptTokensEstimate?: number;
  observedInputTokens?: number;
  inferredAdditionalPromptTokens?: number | null;
  estimatedVsObservedDelta?: number | null;
}): MemoryInjectionMetrics {
  return {
    mode: input.mode,
    projectFactCount: input.projectFactCount ?? 0,
    edgeCount: input.edgeCount ?? 0,
    workingMemoryFactCount: input.workingMemoryFactCount ?? 0,
    semanticHitCount: input.semanticHitCount ?? 0,
    episodeCount: input.episodeCount ?? 0,
    globalFactCount: input.globalFactCount ?? 0,
    payloadChars: input.payloadChars,
    estimatedTokens: Math.round(input.payloadChars / 4),
    baselineContextTokens: input.baselineContextTokens,
    userPromptTokensEstimate: input.userPromptTokensEstimate,
    observedInputTokens: input.observedInputTokens,
    inferredAdditionalPromptTokens: input.inferredAdditionalPromptTokens,
    estimatedVsObservedDelta: input.estimatedVsObservedDelta,
  };
}

export function formatMemoryInjectionMetrics(metrics: MemoryInjectionMetrics | null | undefined): string[] {
  if (!metrics) {
    return ["Last injection: none recorded this session"];
  }

  const lines = [
    `Last injection mode: ${metrics.mode}`,
    `Last injection facts: ${metrics.projectFactCount}`,
    `Last injection edges: ${metrics.edgeCount}`,
    `Last injection working-memory facts: ${metrics.workingMemoryFactCount}`,
    `Last injection semantic hits: ${metrics.semanticHitCount}`,
    `Last injection episodes: ${metrics.episodeCount}`,
    `Last injection global facts: ${metrics.globalFactCount}`,
    `Last injection payload: ${metrics.payloadChars} chars`,
    `Last injection estimate: ${metrics.estimatedTokens} tokens`,
  ];

  if (metrics.baselineContextTokens !== undefined) {
    lines.push(`Baseline context before injection: ${metrics.baselineContextTokens ?? "unknown"}`);
  }
  if (metrics.userPromptTokensEstimate !== undefined) {
    lines.push(`User prompt estimate: ${metrics.userPromptTokensEstimate} tokens`);
  }
  if (metrics.observedInputTokens !== undefined) {
    lines.push(`Observed next input usage: ${metrics.observedInputTokens} tokens`);
  }
  if (metrics.inferredAdditionalPromptTokens !== undefined) {
    lines.push(`Inferred added prompt tokens: ${metrics.inferredAdditionalPromptTokens ?? "unknown"}`);
  }
  if (metrics.estimatedVsObservedDelta !== undefined) {
    lines.push(`Estimate delta vs inferred: ${metrics.estimatedVsObservedDelta ?? "unknown"}`);
  }

  return lines;
}
