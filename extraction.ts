/**
 * Project Memory — Background Extraction
 *
 * Spawns a toolless pi subprocess with Opus 4.6 to update memory.
 * The extraction agent receives current memory + recent conversation,
 * returns updated markdown with optional ---ARCHIVE--- section.
 */

import { spawn, type ChildProcess } from "node:child_process";
import type { MemoryConfig } from "./types.js";

const EXTRACTION_PROMPT = `You are a project memory curator. You receive:
1. The current project memory file (structured markdown with section headers)
2. Recent conversation context from a coding session

Your job: update the memory file with durable technical facts discovered in the conversation.

RULES:
- ADD facts that would be useful in a FUTURE session on this project
- UPDATE existing facts if the conversation reveals they've changed
- REMOVE facts clearly contradicted by new evidence
- CONSOLIDATE redundant or near-duplicate entries
- DO NOT add transient details (debugging steps, file contents, raw command output)
- DO NOT add facts obvious from reading code (basic imports, boilerplate)
- PRESERVE all existing facts not contradicted or superseded
- Keep each bullet point self-contained and concise
- Maintain the existing section structure and descriptions

CRITICAL SIZE CONSTRAINT:
- The memory file MUST stay under {maxLines} content lines (excluding HTML comments and blank lines)
- If adding new facts would exceed this limit, REMOVE the least relevant existing facts
- Prefer recent, actionable facts over old, general ones
- Facts about CURRENT state supersede facts about PAST state

When removing facts to stay within the line limit, place them AFTER a line containing exactly:
---ARCHIVE---

Archived facts should be prefixed with their original section name in brackets, e.g.:
[Architecture] Old fact about deprecated component
[Constraints] Outdated environment detail

If no facts need to be archived, do not include the ---ARCHIVE--- line.
If no updates are needed at all, return the memory file unchanged.

Return ONLY the markdown. No explanation, no commentary, no code fences.`;

export interface ExtractionTriggerState {
  lastExtractedTokens: number;
  toolCallsSinceExtract: number;
  manualStoresSinceExtract: number;
  isInitialized: boolean;
  isRunning: boolean;
}

export function createTriggerState(): ExtractionTriggerState {
  return {
    lastExtractedTokens: 0,
    toolCallsSinceExtract: 0,
    manualStoresSinceExtract: 0,
    isInitialized: false,
    isRunning: false,
  };
}

export function shouldExtract(state: ExtractionTriggerState, currentTokens: number, config: MemoryConfig): boolean {
  if (state.isRunning) return false;

  // Skip if LLM is actively self-storing
  if (state.manualStoresSinceExtract >= config.manualStoreThreshold) return false;

  const tokenDelta = currentTokens - state.lastExtractedTokens;

  if (!state.isInitialized) {
    // First extraction: need minimum tokens
    return currentTokens >= config.minimumTokensToInit;
  }

  // Subsequent: need both token delta AND tool call threshold
  return tokenDelta >= config.minimumTokensBetweenUpdate && state.toolCallsSinceExtract >= config.toolCallsBetweenUpdates;
}

/** Currently running extraction process, if any */
let activeExtractionProc: ChildProcess | null = null;

export async function runExtraction(
  cwd: string,
  currentMemory: string,
  recentConversation: string,
  config: MemoryConfig,
): Promise<string> {
  const prompt = EXTRACTION_PROMPT.replace("{maxLines}", String(config.maxLines));

  const userMessage = [
    "Current project memory:\n",
    currentMemory,
    "\n\n---\n\nRecent conversation:\n\n",
    recentConversation,
    "\n\nReturn the updated memory file.",
  ].join("");

  return new Promise<string>((resolve, reject) => {
    // Guard against concurrent extractions
    if (activeExtractionProc) {
      reject(new Error("Extraction already in progress"));
      return;
    }

    const args = [
      "--model",
      config.extractionModel,
      "--no-session",
      "--no-tools",
      "--no-extensions",
      "--no-skills",
      "--no-themes",
      "--thinking",
      "off",
      "--system-prompt",
      prompt,
      "-p",
      userMessage,
    ];

    const proc = spawn("pi", args, {
      cwd,
      stdio: ["ignore", "pipe", "pipe"],
    });
    activeExtractionProc = proc;

    let stdout = "";
    let stderr = "";

    proc.stdout.on("data", (d: Buffer) => {
      stdout += d.toString();
    });
    proc.stderr.on("data", (d: Buffer) => {
      stderr += d.toString();
    });

    const timeout = setTimeout(() => {
      proc.kill("SIGTERM");
      setTimeout(() => {
        if (!proc.killed) proc.kill("SIGKILL");
      }, 5000);
      reject(new Error("Extraction timed out"));
    }, config.extractionTimeout);

    proc.on("close", (code) => {
      clearTimeout(timeout);
      activeExtractionProc = null;
      const output = stdout.trim();
      if (code === 0 && output) {
        // Strip code fences if the model wraps output despite instructions
        const cleaned = output.replace(/^```(?:markdown|md)?\n?/, "").replace(/\n?```\s*$/, "");
        resolve(cleaned);
      } else {
        reject(new Error(`Extraction failed (exit ${code}): ${stderr.slice(0, 500)}`));
      }
    });

    proc.on("error", (err) => {
      clearTimeout(timeout);
      activeExtractionProc = null;
      reject(err);
    });
  });
}
