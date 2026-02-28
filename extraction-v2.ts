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
 */

import { spawn, type ChildProcess } from "node:child_process";
import type { MemoryConfig } from "./types.js";
import type { Fact, Edge } from "./factstore.js";

// ---------------------------------------------------------------------------
// Shared subprocess runner
// ---------------------------------------------------------------------------

/** Track the currently running extraction process for cancellation */
let activeProc: ChildProcess | null = null;

/**
 * Kill the active extraction process if one is running.
 * Returns true if a process was killed.
 */
export function killActiveExtraction(): boolean {
  if (activeProc) {
    activeProc.kill("SIGTERM");
    activeProc = null;
    return true;
  }
  return false;
}

/** Check if an extraction is currently in progress */
export function isExtractionRunning(): boolean {
  return activeProc !== null;
}

/**
 * Spawn a pi subprocess with a system prompt and user message.
 * Returns the raw stdout output. Handles timeout, cleanup, code fence stripping.
 */
function spawnExtraction(opts: {
  cwd: string;
  model: string;
  systemPrompt: string;
  userMessage: string;
  timeout: number;
  label: string;
}): Promise<string> {
  return new Promise<string>((resolve, reject) => {
    if (activeProc) {
      reject(new Error(`${opts.label}: extraction already in progress`));
      return;
    }

    const args = [
      "--model", opts.model,
      "--no-session", "--no-tools", "--no-extensions",
      "--no-skills", "--no-themes", "--thinking", "off",
      "--system-prompt", opts.systemPrompt,
      "-p", opts.userMessage,
    ];

    const proc = spawn("pi", args, {
      cwd: opts.cwd,
      stdio: ["ignore", "pipe", "pipe"],
    });
    activeProc = proc;

    let stdout = "";
    let stderr = "";

    proc.stdout.on("data", (d: Buffer) => { stdout += d.toString(); });
    proc.stderr.on("data", (d: Buffer) => { stderr += d.toString(); });

    let escalationTimer: ReturnType<typeof setTimeout> | null = null;
    const timeoutHandle = setTimeout(() => {
      proc.kill("SIGTERM");
      escalationTimer = setTimeout(() => {
        if (!proc.killed) proc.kill("SIGKILL");
      }, 5000);
      reject(new Error(`${opts.label} timed out`));
    }, opts.timeout);

    proc.on("close", (code) => {
      clearTimeout(timeoutHandle);
      if (escalationTimer) clearTimeout(escalationTimer);
      activeProc = null;

      const output = stdout.trim();
      if (code === 0 && output) {
        // Strip code fences if the model wraps output
        const cleaned = output
          .replace(/^```(?:jsonl?|json)?\n?/, "")
          .replace(/\n?```\s*$/, "");
        resolve(cleaned);
      } else if (code === 0 && !output) {
        resolve("");
      } else {
        reject(new Error(`${opts.label} failed (exit ${code}): ${stderr.slice(0, 500)}`));
      }
    });

    proc.on("error", (err) => {
      clearTimeout(timeoutHandle);
      activeProc = null;
      reject(err);
    });
  });
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
- Valid sections: Architecture, Decisions, Constraints, Known Issues, Patterns & Conventions

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
 */
export async function runExtractionV2(
  cwd: string,
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

  return spawnExtraction({
    cwd,
    model: config.extractionModel,
    systemPrompt: prompt,
    userMessage,
    timeout: config.extractionTimeout,
    label: "Project extraction",
  });
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
- If the new project facts don't contain anything generalizable, output nothing.`;
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
 */
export async function runGlobalExtraction(
  cwd: string,
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

  return spawnExtraction({
    cwd,
    model: config.extractionModel,
    systemPrompt: buildGlobalExtractionPrompt(),
    userMessage,
    timeout: config.extractionTimeout,
    label: "Global extraction",
  });
}
