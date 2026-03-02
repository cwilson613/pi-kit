import type { ExtensionAPI } from "@mariozechner/pi-coding-agent";
import { Type } from "@sinclair/typebox";

/**
 * Offline Driver Extension
 *
 * Provides seamless failover from cloud (Anthropic) to local (Ollama) models.
 * Registers /offline and /online commands plus a switch_to_offline_driver tool
 * the agent can self-invoke when it detects connectivity issues.
 *
 * Depends on models.json having a "local" provider with Ollama models registered.
 */

const OLLAMA_URL = process.env.LOCAL_INFERENCE_URL || "http://localhost:11434";

// Preferred offline models in priority order
const OFFLINE_MODELS = [
  { id: "nemotron-3-nano:30b", label: "Nemotron 3 Nano 30B", icon: "🏔️" },
  { id: "devstral-small-2:24b", label: "Devstral Small 2 24B", icon: "🔧" },
  { id: "qwen3:30b", label: "Qwen3 30B", icon: "🐉" },
] as const;

// State
let savedCloudModel: string | null = null;
let savedCloudProvider: string | null = null;
let isOffline = false;

async function checkOllama(): Promise<{ ok: boolean; models: string[] }> {
  try {
    const res = await fetch(`${OLLAMA_URL}/api/tags`, { signal: AbortSignal.timeout(3000) });
    if (!res.ok) return { ok: false, models: [] };
    const data = (await res.json()) as { models?: Array<{ name: string }> };
    const models = (data.models || []).map((m) => m.name.replace(/:latest$/, ""));
    return { ok: true, models };
  } catch {
    return { ok: false, models: [] };
  }
}

async function checkAnthropic(): Promise<boolean> {
  // Connectivity check only — we don't need to authenticate, just verify
  // we can reach Anthropic's servers. A HEAD or lightweight GET to their
  // domain suffices. Pi handles auth via OAuth (auth.json), not env vars.
  try {
    const res = await fetch("https://api.anthropic.com/v1/messages", {
      method: "POST",
      headers: {
        "anthropic-version": "2023-06-01",
        "content-type": "application/json",
      },
      body: JSON.stringify({
        model: "claude-haiku-4-5",
        max_tokens: 1,
        messages: [{ role: "user", content: "." }],
      }),
      signal: AbortSignal.timeout(5000),
    });
    // Any HTTP response (including 401 auth error) means network is reachable.
    // Only a fetch exception (DNS failure, timeout, connection refused) means offline.
    return true;
  } catch {
    return false;
  }
}

async function selectBestOfflineModel(
  ctx: any
): Promise<{ model: any; spec: (typeof OFFLINE_MODELS)[number] } | null> {
  const ollama = await checkOllama();
  if (!ollama.ok) return null;

  for (const spec of OFFLINE_MODELS) {
    // Check if the model is available in Ollama
    const available = ollama.models.some(
      (m) => m === spec.id || m === spec.id.replace(/:.*$/, "")
    );
    if (!available) continue;

    // Check if it's registered in the model registry
    const model = ctx.modelRegistry.find("local", spec.id);
    if (model) return { model, spec };
  }

  return null;
}

async function goOffline(
  pi: ExtensionAPI,
  ctx: any,
  preferredModel?: string
): Promise<{ success: boolean; message: string }> {
  if (isOffline) {
    // Verify the model is still available in Ollama (may have been unloaded)
    const ollama = await checkOllama();
    if (ollama.ok) {
      return { success: true, message: "Already in offline mode." };
    }
    // Ollama went away — reset state and fall through to re-select
    isOffline = false;
  }

  // Save current cloud model for /online restoration
  const current = ctx.model;
  if (current && current.provider !== "local") {
    savedCloudModel = current.id;
    savedCloudProvider = current.provider;
  }

  // If a specific model was requested, try that first
  if (preferredModel) {
    const spec = OFFLINE_MODELS.find((m) => m.id === preferredModel);
    if (spec) {
      const model = ctx.modelRegistry.find("local", spec.id);
      if (model) {
        const success = await pi.setModel(model);
        if (success) {
          isOffline = true;
          ctx.ui.setStatus("offline-driver", `${spec.icon} OFFLINE: ${spec.label}`);
          return {
            success: true,
            message: `Switched to offline driver: ${spec.label} (${spec.id})`,
          };
        }
      }
    }
  }

  // Auto-select best available
  const best = await selectBestOfflineModel(ctx);
  if (!best) {
    return {
      success: false,
      message:
        "No offline models available. Is Ollama running? Are models pulled? (ollama list)",
    };
  }

  const success = await pi.setModel(best.model);
  if (success) {
    isOffline = true;
    ctx.ui.setStatus("offline-driver", `${best.spec.icon} OFFLINE: ${best.spec.label}`);
    return {
      success: true,
      message: `Switched to offline driver: ${best.spec.label} (${best.spec.id})`,
    };
  }

  return { success: false, message: "Failed to set offline model." };
}

async function goOnline(
  pi: ExtensionAPI,
  ctx: any
): Promise<{ success: boolean; message: string }> {
  if (!isOffline) {
    return { success: true, message: "Already in online mode." };
  }

  const provider = savedCloudProvider || "anthropic";
  const modelId = savedCloudModel || "claude-opus-4-6";
  const model = ctx.modelRegistry.find(provider, modelId);

  if (!model) {
    return {
      success: false,
      message: `Cannot restore cloud model ${provider}/${modelId} — not found in registry.`,
    };
  }

  const anthropicOk = await checkAnthropic();
  if (!anthropicOk) {
    return {
      success: false,
      message:
        "Anthropic API is still unreachable. Staying in offline mode. Retry with /online when connectivity is restored.",
    };
  }

  const success = await pi.setModel(model);
  if (success) {
    isOffline = false;
    ctx.ui.setStatus("offline-driver", "");
    return {
      success: true,
      message: `Restored cloud driver: ${provider}/${modelId}`,
    };
  }

  return { success: false, message: "Failed to restore cloud model." };
}

export default function (pi: ExtensionAPI) {
  // Health check on session start
  pi.on("session_start", async (_event, ctx) => {
    const [anthropicOk, ollama] = await Promise.all([checkAnthropic(), checkOllama()]);

    const ollamaModels = ollama.ok
      ? OFFLINE_MODELS.filter((m) =>
          ollama.models.some((om) => om === m.id || om === m.id.replace(/:.*$/, ""))
        )
      : [];

    const parts: string[] = [];

    if (anthropicOk) {
      parts.push("☁️ Anthropic: reachable");
    } else {
      parts.push("⚠️ Anthropic: UNREACHABLE");
    }

    if (ollama.ok) {
      const names = ollamaModels.map((m) => m.label).join(", ");
      parts.push(
        `🏠 Ollama: ${ollamaModels.length} driver model${ollamaModels.length !== 1 ? "s" : ""} ready${ollamaModels.length > 0 ? ` (${names})` : ""}`
      );
    } else {
      parts.push("🏠 Ollama: not running");
    }

    ctx.ui.notify(parts.join(" | "), anthropicOk ? "info" : "warn");

    // Always save the starting cloud model (reset stale state from prior sessions)
    const current = ctx.model;
    if (current && current.provider !== "local") {
      savedCloudModel = current.id;
      savedCloudProvider = current.provider;
    }
    // Reset offline flag — each session starts fresh
    isOffline = false;

    // If Anthropic is unreachable and we have local models, suggest /offline
    if (!anthropicOk && ollamaModels.length > 0) {
      ctx.ui.notify(
        "💡 Cloud unavailable. Use /offline to switch to local driver.",
        "warn"
      );
    }
  });

  // /offline command
  pi.registerCommand("offline", {
    description: "Switch to best available local model as the driving agent",
    handler: async (args, ctx) => {
      const preferredModel = args?.trim() || undefined;
      const result = await goOffline(pi, ctx, preferredModel);
      ctx.ui.notify(result.message, result.success ? "info" : "error");
    },
  });

  // /online command
  pi.registerCommand("online", {
    description: "Restore the cloud (Anthropic) model as the driving agent",
    handler: async (_args, ctx) => {
      const result = await goOnline(pi, ctx);
      ctx.ui.notify(result.message, result.success ? "info" : "error");
    },
  });

  // Agent-invocable tool for self-recovery
  pi.registerTool({
    name: "switch_to_offline_driver",
    label: "Switch to Offline Driver",
    description:
      "Switch the driving model from cloud (Anthropic) to a local offline model (Ollama). " +
      "Use when you detect connectivity issues, API errors, or when the user requests offline mode. " +
      "The best available local model is auto-selected: Nemotron 3 Nano (1M context), " +
      "Devstral Small 2 (384K, code-focused), or Qwen3 30B (256K, general).",
    promptSnippet: "Switch from cloud to local Ollama model for offline operation or API failure recovery",
    promptGuidelines: [
      "Use when detecting repeated API errors, timeouts, or connectivity failures",
    ],
    parameters: Type.Object({
      reason: Type.String({
        description: "Why switching to offline mode",
      }),
      preferred_model: Type.Optional(
        Type.String({
          description:
            "Optional: specific model ID to use (nemotron-3-nano:30b, devstral-small-2:24b, qwen3:30b)",
        })
      ),
    }),
    execute: async (
      _toolCallId,
      params: { reason: string; preferred_model?: string },
      _signal,
      _onUpdate,
      ctx
    ) => {
      const result = await goOffline(pi, ctx, params.preferred_model);
      if (result.success) {
        ctx.ui.notify(`🔌 Offline: ${params.reason}`, "info");
      }
      return {
        content: [
          {
            type: "text" as const,
            text: `${result.success ? "✅" : "❌"} ${result.message}${result.success ? ` (reason: ${params.reason})` : ""}`,
          },
        ],
      };
    },
  });
}
