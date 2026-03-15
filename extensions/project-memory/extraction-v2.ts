/**
 * Project Memory — Extraction v2
 *
 * Updated extraction for SQLite-backed fact store.
 * The extraction agent outputs JSONL actions instead of rewriting a markdown file.
 *
 * Action types:
 *   observe   — "I see this fact in the conversation" (reinforces or adds)
 *   reinforce — "This existing fact is still true" (by ID)
 *   supersede — "This new fact replaces that old one" (by ID + new content)
 *   archive   — "This fact appears stale/wrong" (by ID)
 *   connect   — "These two facts are related" (global extraction only)
 *
 * All LLM calls use direct HTTP (llm-direct.ts) — zero subprocess overhead.
 */

import type { MemoryConfig } from "./types.ts";
import type { Fact, Edge } from "./factstore.ts";
import { chatDirect, cleanModelOutput, isCloudModel, getBudgetCloudModel } from "./llm-direct.ts";

// ---------------------------------------------------------------------------
// Cancellation support
// ---------------------------------------------------------------------------

/** Active AbortController for the current extraction — killable externally */
let activeAbort: AbortController | null = null;

/**
 * Kill the active extraction (abort in-flight HTTP request).
 * Returns true if something was aborted.
 */
export function killActiveExtraction(): boolean {
  if (activeAbort) {
    activeAbort.abort();
    activeAbort = null;
    return true;
  }
  return false;
}

/**
 * Kill all active operations. Alias for killActiveExtraction since we no
 * longer spawn subprocesses — kept for API compatibility with index.ts.
 */
export function killAllSubprocesses(): void {
  killActiveExtraction();
}

/** Check if an extraction is currently in progress */
export function isExtractionRunning(): boolean {
  return activeAbort !== null;
}

// ---------------------------------------------------------------------------
// Shared LLM call with abort tracking
// ---------------------------------------------------------------------------

/**
 * Run a tracked LLM call — sets activeAbort for external cancellation.
 * Only one tracked call at a time (new call aborts previous).
 */
async function trackedChat(opts: {
  model: string;
  systemPrompt: string;
  userMessage: string;
  timeout: number;
  maxTokens?: number;
  label: string;
}): Promise<string> {
  // Cancel any previous tracked call
  if (activeAbort) activeAbort.abort();
  const controller = new AbortController();
  activeAbort = controller;

  try {
    const result = await chatDirect({
      model: opts.model,
      systemPrompt: opts.systemPrompt,
      userMessage: opts.userMessage,
      maxTokens: opts.maxTokens ?? 2048,
      timeout: opts.timeout,
      signal: controller.signal,
    });
    return result.content;
  } finally {
    if (activeAbort === controller) activeAbort = null;
  }
}

/**
 * Run an untracked LLM call — does NOT set activeAbort.
 * Used for secondary calls (pruning, episodes) that shouldn't cancel extraction.
 */
async function untrackedChat(opts: {
  model: string;
  systemPrompt: string;
  userMessage: string;
  timeout: number;
  maxTokens?: number;
}): Promise<string> {
  const result = await chatDirect({
    model: opts.model,
    systemPrompt: opts.systemPrompt,
    userMessage: opts.userMessage,
    maxTokens: opts.maxTokens ?? 2048,
    timeout: opts.timeout,
  });
  return result.content;
}

// ---------------------------------------------------------------------------
// Cloud fallback model — cheapest available for budget tasks
// ---------------------------------------------------------------------------

const CLOUD_FALLBACK_MODEL = "claude-haiku-4-5";

function resolveModel(configModel: string): string {
  // If the configured model is a cloud model with a key, use it directly
  if (isCloudModel(configModel)) return configModel;
  // If it's a local model, try it (chatDirect handles Ollama)
  // If Ollama is down, chatDirect will throw and caller handles fallback
  return configModel;
}

// ---------------------------------------------------------------------------
// Phase 1: Project extraction
// ---------------------------------------------------------------------------

function buildExtractionPrompt(maxLines: number): string {
  return `You are a project memory curator. You receive:
1. Current active facts (with IDs) from the project's memory database
2. Recent conversation context from a coding session

Your job: output JSONL (one JSON object per line) describing what you observed.

ACTION TYPES:

{"type":"observe","section":"Architecture","content":"The project uses SQLite for storage"}
  → You saw evidence of this fact in the conversation. If it already exists, it gets reinforced.
    If it's new, it gets added.

{"type":"reinforce","id":"abc123"}
  → An existing fact (by ID) is confirmed still true by the conversation context.

{"type":"supersede","id":"abc123","section":"Architecture","content":"The project migrated from SQLite to PostgreSQL"}
  → A specific existing fact is wrong/outdated. Provide the replacement.

{"type":"archive","id":"abc123"}
  → A specific existing fact is clearly wrong, obsolete, or no longer relevant.

RULES:
- Output ONLY valid JSONL. One JSON object per line. No commentary, no explanation.
- Focus on DURABLE technical facts — architecture, decisions, constraints, patterns, bugs.
- DO NOT output facts about transient details (debugging steps, file contents, command output).
- DO NOT output facts that are obvious from reading code (basic imports, boilerplate).
- Prefer "observe" for new facts. Use "supersede" only when you can identify the specific old fact being replaced.
- Use "reinforce" when the conversation confirms an existing fact without changing it.
- Use "archive" sparingly — only when a fact is clearly contradicted.
- Keep fact content self-contained and concise (one line, no bullet prefix).
- Valid sections: Architecture, Decisions, Constraints, Known Issues, Patterns & Conventions, Specs

FACT DENSITY — POINTERS OVER CONTENT:
- Facts are injected into every agent turn. Every token counts.
- For implementation details (formulas, method signatures, schemas, config shapes):
  store a POINTER fact — name the concept + reference the file path. The agent can
  read the file when it actually needs the details.
  GOOD: "project-memory pressure system: 3 tiers (40%/65%/85%). See extensions/project-memory/pressure.ts"
  BAD:  "project-memory degeneracy pressure uses computeDegeneracyPressure(pct, onset, warning, k=3) with formula (e^(k*t)-1)/(e^k-1) where t=..."
- INLINE the content only when the fact is frequently needed and short enough that a
  file read would waste more tokens than the inline content (e.g., env var names,
  CLI flags, version numbers, short constraints).
- When in doubt: if the fact exceeds ~40 words, it probably belongs as a pointer.

TARGET: aim for at most ${maxLines} active facts total. If the memory is near capacity, use "archive" on the least relevant facts to make room.

If the conversation contains nothing worth remembering, output nothing.`;
}

/**
 * Format current facts for the extraction agent's input.
 * Shows facts with IDs so the agent can reference them.
 */
export function formatFactsForExtraction(facts: Fact[]): string {
  if (facts.length === 0) return "(no existing facts)";

  const lines: string[] = [];
  let currentSection = "";

  for (const fact of facts) {
    if (fact.section !== currentSection) {
      currentSection = fact.section;
      lines.push(`\n## ${currentSection}`);
    }
    const date = fact.created_at.split("T")[0];
    const rc = fact.reinforcement_count;
    lines.push(`[${fact.id}] ${fact.content} (${date}, reinforced ${rc}x)`);
  }

  return lines.join("\n");
}

/**
 * Run project extraction (Phase 1).
 * Returns raw JSONL output from the extraction agent.
 * Uses direct HTTP — no subprocess spawning.
 */
export async function runExtractionV2(
  _cwd: string,
  currentFacts: Fact[],
  recentConversation: string,
  config: MemoryConfig,
): Promise<string> {
  const prompt = buildExtractionPrompt(config.maxLines);
  const factsFormatted = formatFactsForExtraction(currentFacts);

  const userMessage = [
    "Current active facts:\n",
    factsFormatted,
    "\n\n---\n\nRecent conversation:\n\n",
    recentConversation,
    "\n\nOutput JSONL actions based on what you observe.",
  ].join("");

  const model = resolveModel(config.extractionModel);

  try {
    return await trackedChat({
      model,
      systemPrompt: prompt,
      userMessage,
      timeout: config.extractionTimeout,
      label: "Project extraction",
    });
  } catch (err) {
    // If configured model failed (e.g., Ollama down), try cloud fallback
    if (!isCloudModel(model)) {
      const fallback = getBudgetCloudModel();
      if (fallback) {
        return await trackedChat({
          model: fallback,
          systemPrompt: prompt,
          userMessage,
          timeout: config.extractionTimeout,
          label: "Project extraction (cloud fallback)",
        });
      }
    }
    throw err;
  }
}

// ---------------------------------------------------------------------------
// Phase 2: Global extraction
// ---------------------------------------------------------------------------

function buildGlobalExtractionPrompt(): string {
  return `You are a cross-project knowledge synthesizer. You receive:
1. New facts just extracted from a project-scoped coding session
2. Existing facts in the global knowledge base (with IDs)
3. Existing connections (edges) between global facts

Your job: identify generalizable knowledge and meaningful connections between facts.

ACTION TYPES:

{"type":"observe","section":"Architecture","content":"Embedded DBs preferred over client-server for CLI tooling"}
  → A new fact that generalizes beyond its source project. Rewrite to be project-agnostic.

{"type":"reinforce","id":"abc123"}
  → An existing global fact is confirmed by this project's evidence.

{"type":"connect","source":"<fact_id>","target":"<fact_id>","relation":"runs_on","description":"k8s deployment depends on host OS kernel features"}
  → Two GLOBAL facts are meaningfully related. Both source and target must be IDs from
    the EXISTING GLOBAL FACTS section — not from the new project facts.
    First promote a project fact via "observe", then connect the promoted global copy.
    The relation is a short verb phrase describing the directional relationship.
    Common patterns: runs_on, depends_on, motivated_by, contradicts, enables,
    generalizes, instance_of, requires, conflicts_with, replaces, preceded_by
    But use whatever verb phrase best captures the relationship.

{"type":"supersede","id":"abc123","section":"Decisions","content":"Updated understanding..."}
  → An existing global fact is outdated. Provide the replacement.

{"type":"archive","id":"abc123"}
  → An existing global fact is clearly wrong or obsolete.

RULES:
- Output ONLY valid JSONL. One JSON object per line.
- Only promote facts that would be useful across MULTIPLE projects.
- Rewrite promoted facts to remove project-specific names, paths, and details.
- Connections must reference GLOBAL fact IDs only (from "EXISTING GLOBAL FACTS" section).
  To connect a new project fact, first promote it with "observe", then in the NEXT
  extraction cycle it will have a global ID you can reference.
- Connections should represent genuine analytical insight, not surface keyword overlap.
- Prefer fewer, high-quality connections over many weak ones.
- A connection between facts in different sections is more valuable than within the same section.
- If the new project facts don't contain anything generalizable, output nothing.

FACT DENSITY — keep facts concise (~40 words max). For implementation details,
reference file paths instead of inlining formulas/schemas/signatures. Global facts
especially must be lean since they're injected across ALL projects.`;
}

/**
 * Format new project facts + existing global facts + edges for global extraction.
 */
export function formatGlobalExtractionInput(
  newProjectFacts: Fact[],
  globalFacts: Fact[],
  globalEdges: Edge[],
): string {
  const lines: string[] = [];

  lines.push("=== NEW PROJECT FACTS (candidates for promotion — these IDs are project-scoped, NOT referenceable in connect actions) ===");
  if (newProjectFacts.length === 0) {
    lines.push("(none)");
  } else {
    for (const f of newProjectFacts) {
      lines.push(`(${f.section}) ${f.content}`);
    }
  }

  lines.push("\n=== EXISTING GLOBAL FACTS (use these IDs in connect actions) ===");
  if (globalFacts.length === 0) {
    lines.push("(empty — this is the first global extraction)");
  } else {
    let currentSection = "";
    for (const f of globalFacts) {
      if (f.section !== currentSection) {
        currentSection = f.section;
        lines.push(`\n## ${currentSection}`);
      }
      const rc = f.reinforcement_count;
      lines.push(`[${f.id}] ${f.content} (reinforced ${rc}x)`);
    }
  }

  if (globalEdges.length > 0) {
    lines.push("\n=== EXISTING CONNECTIONS ===");
    for (const e of globalEdges) {
      lines.push(`[${e.source_fact_id}] --${e.relation}--> [${e.target_fact_id}]: ${e.description}`);
    }
  }

  return lines.join("\n");
}

/**
 * Run global extraction (Phase 2).
 * Only called when Phase 1 produced new facts.
 * Uses direct HTTP — no subprocess spawning.
 */
export async function runGlobalExtraction(
  _cwd: string,
  newProjectFacts: Fact[],
  globalFacts: Fact[],
  globalEdges: Edge[],
  config: MemoryConfig,
): Promise<string> {
  const input = formatGlobalExtractionInput(newProjectFacts, globalFacts, globalEdges);

  const userMessage = [
    input,
    "\n\nOutput JSONL actions: promote generalizable facts and identify connections between GLOBAL facts.",
  ].join("");

  const model = resolveModel(config.extractionModel);

  try {
    return await trackedChat({
      model,
      systemPrompt: buildGlobalExtractionPrompt(),
      userMessage,
      timeout: config.extractionTimeout,
      label: "Global extraction",
    });
  } catch (err) {
    if (!isCloudModel(model)) {
      const fallback = getBudgetCloudModel();
      if (fallback) {
        return await trackedChat({
          model: fallback,
          systemPrompt: buildGlobalExtractionPrompt(),
          userMessage,
          timeout: config.extractionTimeout,
          label: "Global extraction (cloud fallback)",
        });
      }
    }
    throw err;
  }
}

// ---------------------------------------------------------------------------
// Phase 3: Episode generation
// ---------------------------------------------------------------------------

const EPISODE_PROMPT = `You are a session narrator. You receive the tail of a coding session conversation.
Your job: produce a JSON object summarizing what happened.

Output format (MUST be valid JSON, nothing else):
{"title":"<Short title, 5-10 words>","narrative":"<2-4 sentence summary: what was the goal, what was accomplished, what decisions were made, what's still open>"}

RULES:
- Title should be specific and descriptive (e.g., "Migrated auth from JWT to OIDC" not "Working on auth")
- Narrative should capture the ARC: goal → actions → outcome → open threads
- Focus on decisions and outcomes, not mechanical steps
- Keep narrative under 300 words
- Output ONLY the JSON object. No markdown, no commentary.`;

export interface EpisodeOutput {
  title: string;
  narrative: string;
}

/**
 * Session telemetry collected during a session — used to build template episodes
 * when all model-based generation fails.
 */
export interface SessionTelemetry {
  /** ISO date string for the session */
  date: string;
  /** Total tool calls made during the session */
  toolCallCount: number;
  /** Files that were written (via Write tool) */
  filesWritten: string[];
  /** Files that were edited (via Edit tool) */
  filesEdited: string[];
}

/**
 * Generate a session episode via direct LLM call.
 * Uses chatDirect — no subprocess. Tries configured model, falls back to budget cloud.
 */
export async function generateEpisodeDirect(
  recentConversation: string,
  config: MemoryConfig,
): Promise<EpisodeOutput | null> {
  const userMessage = `Session conversation:\n\n${recentConversation}\n\nOutput the episode JSON.`;
  const timeout = Math.min(config.shutdownExtractionTimeout, 10_000);

  // Try configured extraction model first
  try {
    const raw = await untrackedChat({
      model: resolveModel(config.extractionModel),
      systemPrompt: EPISODE_PROMPT,
      userMessage,
      timeout,
      maxTokens: 512,
    });
    if (raw) {
      const parsed = JSON.parse(raw);
      if (parsed.title && parsed.narrative) return parsed as EpisodeOutput;
    }
  } catch {
    // Fall through
  }

  // Try budget cloud model
  const budgetModel = getBudgetCloudModel();
  if (budgetModel && budgetModel !== config.extractionModel) {
    try {
      const raw = await untrackedChat({
        model: budgetModel,
        systemPrompt: EPISODE_PROMPT,
        userMessage,
        timeout,
        maxTokens: 512,
      });
      if (raw) {
        const parsed = JSON.parse(raw);
        if (parsed.title && parsed.narrative) return parsed as EpisodeOutput;
      }
    } catch {
      // Fall through
    }
  }

  return null;
}

/**
 * Build a minimum viable episode from raw session telemetry.
 * Zero I/O — assembled deterministically from already-collected data.
 * This is the guaranteed floor: always emitted when every model fails.
 */
export function buildTemplateEpisode(telemetry: SessionTelemetry): EpisodeOutput {
  const allModified = [...new Set([...telemetry.filesWritten, ...telemetry.filesEdited])];

  // Infer topics from file paths (directory names)
  const skipDirs = new Set([".", "..", "src", "lib", "dist", "extensions", "tests"]);
  const topics = new Set<string>();
  for (const f of allModified) {
    const parts = f.replace(/\\/g, "/").split("/");
    for (const p of parts.slice(0, -1)) {
      if (p && !skipDirs.has(p) && !p.startsWith(".")) topics.add(p);
    }
  }

  const topicStr = topics.size > 0
    ? `Work touched: ${[...topics].slice(0, 4).join(", ")}.`
    : "";

  const fileList = allModified.length > 0
    ? allModified.slice(0, 5).map(f => f.split("/").pop() ?? f).join(", ") +
      (allModified.length > 5 ? ` (+${allModified.length - 5} more)` : "")
    : "no files modified";

  const title = allModified.length > 0
    ? `Session ${telemetry.date}: modified ${allModified.length} file${allModified.length !== 1 ? "s" : ""}`
    : `Session ${telemetry.date}`;

  const narrative =
    `Session on ${telemetry.date} — ${telemetry.toolCallCount} tool calls. ` +
    `Files modified: ${fileList}. ${topicStr}` +
    ` (Template episode — model generation unavailable for this session.)`;

  return { title, narrative };
}

/**
 * Generate a session episode with fallback chain:
 *   1. Direct LLM call (configured model → budget cloud)
 *   2. Template episode (deterministic, zero I/O) — always succeeds
 *
 * No subprocess spawning. Total time bounded by config timeouts.
 */
export async function generateEpisodeWithFallback(
  recentConversation: string,
  telemetry: SessionTelemetry,
  config: MemoryConfig,
  _cwd: string,
): Promise<EpisodeOutput> {
  const result = await generateEpisodeDirect(recentConversation, config);
  if (result) return result;
  return buildTemplateEpisode(telemetry);
}

// ---------------------------------------------------------------------------
// Per-section archival pruning pass
// ---------------------------------------------------------------------------

const SECTION_PRUNING_PROMPT = `You are a memory curator for a project-memory system.
You will receive a list of facts from a single memory section that has exceeded its size limit.
Your job: identify facts to archive (remove from active memory) to bring the section under the target count.

Rules:
- Archive duplicates, overly-specific details, outdated implementation notes, and facts that are
  superseded by other facts in the same list.
- KEEP: architectural decisions, design rationale, critical constraints, patterns that prevent bugs,
  and any fact that is still clearly relevant and has no equivalent in the list.
- Prefer to archive older, less-reinforced, or more transient facts.
- Return ONLY a JSON array of fact IDs to archive. Example: ["id1", "id2", "id3"]
- If unsure whether to archive, keep it.`;

/**
 * Run a targeted LLM archival pass over a single section when it exceeds the ceiling.
 * Returns the list of fact IDs recommended for archival.
 * Uses direct HTTP — no subprocess spawning.
 */
export async function runSectionPruningPass(
  section: string,
  facts: Fact[],
  targetCount: number,
  config: MemoryConfig,
): Promise<string[]> {
  if (facts.length <= targetCount) return [];

  const excessCount = facts.length - targetCount;
  const factList = facts.map((f, i) =>
    `${i + 1}. [ID: ${f.id}] [reinforced: ${f.reinforcement_count}x] [age: ${Math.round((Date.now() - new Date(f.created_at).getTime()) / 86400000)}d] ${f.content}`
  ).join("\n");

  const userMessage = [
    `Section: ${section}`,
    `Current count: ${facts.length} (target: ≤${targetCount}, archive at least ${excessCount})`,
    ``,
    `Facts (sorted by confidence descending — lowest confidence facts are at the bottom):`,
    factList,
    ``,
    `Return a JSON array of fact IDs to archive. Archive at least ${excessCount} to bring the section under ${targetCount + 1}.`,
  ].join("\n");

  const model = resolveModel(config.extractionModel);

  try {
    const raw = await untrackedChat({
      model,
      systemPrompt: SECTION_PRUNING_PROMPT,
      userMessage,
      timeout: 30_000,
      maxTokens: 1024,
    });
    if (raw) {
      const parsed = JSON.parse(raw);
      if (Array.isArray(parsed)) return parsed.filter((id: unknown) => typeof id === "string");
    }
  } catch {
    // Try budget cloud fallback
    const fallback = getBudgetCloudModel();
    if (fallback && fallback !== model) {
      try {
        const raw = await untrackedChat({
          model: fallback,
          systemPrompt: SECTION_PRUNING_PROMPT,
          userMessage,
          timeout: 30_000,
          maxTokens: 1024,
        });
        if (raw) {
          const parsed = JSON.parse(raw);
          if (Array.isArray(parsed)) return parsed.filter((id: unknown) => typeof id === "string");
        }
      } catch {
        // Best effort
      }
    }
  }

  return [];
}

// ---------------------------------------------------------------------------
// Legacy API shim — generateEpisode (subprocess-based) now delegates to direct
// ---------------------------------------------------------------------------

/**
 * @deprecated Use generateEpisodeDirect instead. Kept for API compatibility.
 */
export async function generateEpisode(
  _cwd: string,
  recentConversation: string,
  config: MemoryConfig,
): Promise<EpisodeOutput | null> {
  return generateEpisodeDirect(recentConversation, config);
}
