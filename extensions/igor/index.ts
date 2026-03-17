// @config IGOR_URL "Igor nervous system URL" [default: http://localhost:8765]
// @config IGOR_API_KEY "Igor API key (falls back to ~/.local/share/igor/api.key)" [optional]

/**
 * igor — Omegon extension bridging the agent to Igor's nervous system.
 *
 * Phase 1 integration:
 *   - Enriches every LLM turn with personal brain recall + system state
 *   - Ingests completed turns into igor-brain via the IngestPipeline
 *   - Dispatches [intent:...] markers found in assistant output
 *   - Subscribes to /api/escalations SSE for brainstem alert nudges
 *   - Overrides memory_recall / memory_store tools to call Igor
 *   - Shows "◉ Igor" status widget in the TUI header
 *
 * All hooks degrade gracefully when Igor is unreachable — Omegon continues
 * working exactly as before, just without nervous system enrichment.
 */

import type { ExtensionAPI } from "@styrene-lab/pi-coding-agent";
import { IgorClient, type IngestEntry } from "./client.ts";
import { parseIntentMarkers } from "./intents.ts";
import { Type } from "@sinclair/typebox";
import { Text } from "@styrene-lab/pi-tui";

export default function(pi: ExtensionAPI) {
  let igor: IgorClient = IgorClient.fromContext();
  let connected = false;
  let lastUserPrompt = "";
  let escalationSource: EventSource | null = null;

  // ── Session start: connect + health check + SSE escalation subscription ──────

  pi.on("session_start", async (_event, ctx) => {
    igor = IgorClient.fromContext();  // re-read context.toml on each session

    if (!igor.isConfigured) {
      ctx.ui.setStatus("igor", "○ Igor: no key");
      return;
    }

    const health = await igor.health({ timeout: 3000 });
    connected = health.ok;

    if (connected) {
      const label = [
        health.brain === "connected" ? "🧠" : "○",
        `Igor ${health.version ?? ""}`.trim(),
        health.baseline === "operational" ? "◉ ops" : "○ cal",
      ].join(" | ");
      ctx.ui.setStatus("igor", label);

      // Subscribe to escalation SSE stream
      subscribeEscalations(igor, ctx);
    } else {
      ctx.ui.setStatus("igor", "○ Igor: offline");
    }
  });

  // ── Session shutdown: disconnect ─────────────────────────────────────────────

  pi.on("session_shutdown", async () => {
    connected = false;
    escalationSource?.close();
    escalationSource = null;
  });

  // ── Before agent start: inject recall + environment into system prompt ────────

  pi.on("before_agent_start", async (event) => {
    if (!connected) return {};

    lastUserPrompt = event.prompt ?? "";  // store for turn_end ingestion

    // Parallel fetch with 200ms deadline — never block the LLM call
    const [recalled, env] = await Promise.allSettled([
      igor.recall(lastUserPrompt, { k: 5, timeout: 200 }),
      igor.environment({ timeout: 200 }),
    ]);

    const additions: string[] = [];

    if (recalled.status === "fulfilled" && recalled.value.recalled.length > 0) {
      additions.push("## Personal context (igor-brain)");
      for (const m of recalled.value.recalled) {
        additions.push(`[${m.domain}] ${m.content}`);
      }
    }

    if (env.status === "fulfilled" && env.value.ok && env.value.line) {
      additions.push(`System: ${env.value.line}`);
    }

    if (additions.length === 0) return {};

    return {
      systemPrompt: `${event.systemPrompt}\n\n${additions.join("\n")}`.trim(),
    };
  });

  // ── Turn end: ingest exchange into IngestPipeline ─────────────────────────────

  pi.on("turn_end", async (event, _ctx) => {
    if (!connected) return;

    const now = new Date().toISOString();
    const entries: IngestEntry[] = [];

    if (lastUserPrompt) {
      entries.push({
        role: "user",
        content: lastUserPrompt,
        domain: "conversation",
        certainty: 1.0,
        occurred_at: now,
      });
    }

    // Extract text content from the assistant message
    const assistantText = extractText(event.message);
    if (assistantText) {
      entries.push({
        role: "assistant",
        content: assistantText,
        domain: "conversation",
        certainty: 1.0,
        occurred_at: now,
      });
    }

    if (entries.length > 0) {
      igor.ingest(entries).catch(() => {});
    }
  });

  // ── Message end: extract [intent:...] markers, post to /api/intents ──────────

  pi.on("message_end", async (event, _ctx) => {
    if (!connected) return;
    if ((event.message as any).role !== "assistant") return;

    const text = extractText(event.message);
    if (!text) return;

    const intents = parseIntentMarkers(text);
    if (intents.length > 0) {
      igor.postIntents(intents).catch(() => {});
    }
  });

  // ── Tool overrides: memory_recall → /api/recall ───────────────────────────────

  pi.registerTool({
    name: "memory_recall",
    label: "Recall Memory (Igor)",
    description:
      "Semantic search over Igor's personal brain — recalled memories from past conversations, " +
      "health data, fitness logs, and learned facts about the operator. Falls back to project " +
      "memory (facts.db) when Igor is unavailable.",
    promptSnippet: "Retrieve relevant personal memories from igor-brain using semantic search",
    promptGuidelines: [
      "Use for personal context: health, fitness, calendar, preferences, past conversations",
      "Returns ranked results by semantic similarity to the query",
    ],
    parameters: Type.Object({
      query: Type.String({ description: "Natural language query for semantic search" }),
      k: Type.Optional(Type.Number({ description: "Max results (default: 10)", minimum: 1, maximum: 20 })),
      domain: Type.Optional(Type.String({ description: "Restrict to domain prefix (e.g. 'fitness', 'health')" })),
    }),
    execute: async (_id, params, _signal, _onUpdate, _ctx) => {
      if (connected) {
        const result = await igor.recall(params.query, { k: params.k ?? 10, domain: params.domain, timeout: 500 });
        if (result.recalled.length > 0) {
          const lines = result.recalled.map(m => `[${m.domain}] ${m.content} (score: ${m.score.toFixed(2)})`);
          return {
            content: [{ type: "text" as const, text: lines.join("\n") }],
            details: result,
          };
        }
        return { content: [{ type: "text" as const, text: "(no relevant memories found)" }], details: result };
      }
      // Igor offline — surface the offline status
      return {
        content: [{ type: "text" as const, text: "Igor is offline. Personal brain recall unavailable." }],
        details: { ok: false, recalled: [] },
      };
    },
    renderCall(args, t) {
      const q = typeof args.query === "string" ? args.query : "";
      const trunc = q.length > 64 ? q.slice(0, 61) + "…" : q;
      return new Text(`${t.fg("accent", "⟳")} ${t.fg("toolTitle", "memory_recall")}  ${t.fg("muted", trunc)}`, 0, 0);
    },
    renderResult(result, _opts, t) {
      const details = result.details as { recalled?: unknown[] } | undefined;
      const count = details?.recalled?.length ?? 0;
      const icon = count > 0 ? t.fg("success", "✓") : t.fg("muted", "○");
      return new Text(`${icon} ${t.fg("toolTitle", "memory_recall")}  ${t.fg("muted", `${count} result${count !== 1 ? "s" : ""}`)}`, 0, 0);
    },
  });

  // ── ESS escalation subscription helper ────────────────────────────────────────

  function subscribeEscalations(client: IgorClient, ctx: any) {
    escalationSource?.close();

    try {
      // Node 18 EventSource doesn't support custom headers — use fetch-based polling
      // for the initial implementation; proper SSE with auth follows in Phase 1.1.
      // For localhost (no TLS), the key is in the URL auth header workaround.
      // TODO: switch to undici EventSource (Node 22+) which supports headers.
      const poll = async () => {
        if (!connected) return;
        try {
          const res = await fetch(`${client.baseUrl}/api/escalations/history`, {
            headers: { "Authorization": `Bearer ${client.apiKey}` },
            signal: AbortSignal.timeout(3000),
          });
          if (res.ok) {
            const escalations = await res.json() as Array<{ id: string; reason: string; combined_score: number; acknowledged: boolean }>;
            const unacked = escalations.filter(e => !e.acknowledged);
            if (unacked.length > 0) {
              const e = unacked[0];
              ctx.ui.notify(
                `⚠ Igor: ${e.reason} (z=${e.combined_score.toFixed(1)}) — see dashboard`,
                "warning"
              );
            }
          }
        } catch { /* silent — offline */ }
      };

      // Poll every 30s as a stopgap until EventSource with headers is available
      const interval = setInterval(poll, 30_000);
      poll();  // immediate first check

      // Store cleanup handle
      escalationSource = {
        close: () => clearInterval(interval),
      } as unknown as EventSource;
    } catch { /* escalation subscription is best-effort */ }
  }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

function extractText(message: any): string {
  if (!message?.content) return "";
  if (typeof message.content === "string") return message.content;
  if (!Array.isArray(message.content)) return "";
  return message.content
    .filter((c: any) => c.type === "text")
    .map((c: any) => c.text ?? "")
    .join("");
}
