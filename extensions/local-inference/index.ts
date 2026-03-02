/**
 * local-inference — Delegate sub-tasks to locally running LLM inference servers
 *
 * Registers an `ask_local_model` tool that the driving agent (Claude) can call to
 * delegate specific sub-tasks to local models running via Ollama or any
 * OpenAI-compatible local server. Zero API cost for delegated work.
 *
 * Use cases:
 *   - Boilerplate/template generation
 *   - File summarization
 *   - Code transforms (formatting, conversion)
 *   - Draft generation for review by the driving agent
 *   - Embeddings generation
 *
 * Architecture:
 *   This is Option C (tool-callable sub-agent): the driving agent stays Claude
 *   with reliable tool use and reasoning, but can offload cheap work to local models.
 *   The abstraction layer supports any OpenAI-compatible backend. Default: Ollama.
 *
 * Environment:
 *   LOCAL_INFERENCE_URL — API base URL (default: http://localhost:11434, Ollama)
 */

import type { ExtensionAPI } from "@mariozechner/pi-coding-agent";
import { Type } from "@sinclair/typebox";
import { StringEnum } from "@mariozechner/pi-ai";

const DEFAULT_URL = "http://localhost:11434";

interface LocalModel {
  id: string;
  object: string;
  owned_by: string;
}

interface ChatMessage {
  role: "system" | "user" | "assistant";
  content: string;
}

interface ChatResponse {
  id: string;
  choices: Array<{
    message: {
      role: string;
      content: string;
      reasoning?: string;
    };
    finish_reason: string;
  }>;
  usage: {
    prompt_tokens: number;
    completion_tokens: number;
    total_tokens: number;
  };
}

function getBaseUrl(): string {
  return process.env.LOCAL_INFERENCE_URL || DEFAULT_URL;
}

async function discoverModels(baseUrl: string): Promise<LocalModel[]> {
  try {
    const resp = await fetch(`${baseUrl}/v1/models`, {
      signal: AbortSignal.timeout(3000),
    });
    if (!resp.ok) return [];
    const data = await resp.json();
    return (data.data || []).filter(
      (m: LocalModel) => !m.id.includes("embed") // exclude embedding models from chat
    );
  } catch {
    return [];
  }
}

async function listAllModels(baseUrl: string): Promise<LocalModel[]> {
  try {
    const resp = await fetch(`${baseUrl}/v1/models`, {
      signal: AbortSignal.timeout(3000),
    });
    if (!resp.ok) return [];
    const data = await resp.json();
    return data.data || [];
  } catch {
    return [];
  }
}

function stripThinkTokens(text: string): string {
  // Clean up leaked thinking tokens from various model families
  return text
    .replace(/<think>[\s\S]*?<\/think>\s*/g, "")       // <think>...</think>
    .replace(/<\|begin_of_box\|>/g, "")                  // GLM box tokens
    .replace(/<\|end_of_box\|>/g, "")
    .trim();
}

async function chatCompletion(
  baseUrl: string,
  model: string,
  messages: ChatMessage[],
  opts: { maxTokens?: number; temperature?: number; signal?: AbortSignal }
): Promise<{ content: string; reasoning?: string; usage: ChatResponse["usage"] }> {
  const resp = await fetch(`${baseUrl}/v1/chat/completions`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      model,
      messages,
      max_tokens: opts.maxTokens || 2048,
      temperature: opts.temperature ?? 0.3,
    }),
    signal: opts.signal,
  });

  if (!resp.ok) {
    const body = await resp.text().catch(() => "");
    throw new Error(`Local inference failed (${resp.status}): ${body}`);
  }

  const data: ChatResponse = await resp.json();
  const choice = data.choices?.[0];
  if (!choice) throw new Error("No response from local model");

  return {
    content: stripThinkTokens(choice.message.content || ""),
    reasoning: choice.message.reasoning,
    usage: data.usage,
  };
}

export default function (pi: ExtensionAPI) {
  // Track available models (refreshed on session start and via command)
  let cachedModels: LocalModel[] = [];
  let serverOnline = false;

  async function refreshModels() {
    const baseUrl = getBaseUrl();
    cachedModels = await discoverModels(baseUrl);
    serverOnline = cachedModels.length > 0 || (await listAllModels(baseUrl)).length > 0;
    return cachedModels;
  }

  // Check server on session start
  pi.on("session_start", async (_event, ctx) => {
    await refreshModels();
    if (serverOnline) {
      const modelNames = cachedModels.map((m) => m.id.split("/").pop()).join(", ");
      ctx.ui.setStatus("local-inference", `🏠 ${cachedModels.length} local models`);
    }
  });

  // Main delegation tool
  pi.registerTool({
    name: "ask_local_model",
    label: "Ask Local Model",
    description:
      "Delegate a sub-task to a locally running LLM (zero API cost). " +
      "The local model runs on-device via Ollama. Use for:\n" +
      "- Boilerplate/template generation\n" +
      "- File summarization or content transforms\n" +
      "- Code formatting, conversion, or simple generation\n" +
      "- Drafting text for your review\n" +
      "- Any task where perfect accuracy isn't critical\n\n" +
      "You receive the local model's response and can review, edit, or use it. " +
      "The local model has NO access to tools, files, or conversation context — " +
      "you must include all necessary context in the prompt.",
    parameters: Type.Object({
      prompt: Type.String({
        description: "Complete prompt for the local model. Include ALL necessary context — the local model cannot see our conversation or access any tools.",
      }),
      system: Type.Optional(
        Type.String({
          description: "Optional system prompt to set the local model's behavior (e.g., 'You are a Python expert. Output only code, no explanations.')",
        })
      ),
      model: Type.Optional(
        Type.String({
          description: "Specific model ID to use. Omit to auto-select the best available model.",
        })
      ),
      max_tokens: Type.Optional(
        Type.Number({
          description: "Maximum response tokens (default: 2048)",
        })
      ),
      temperature: Type.Optional(
        Type.Number({
          description: "Sampling temperature 0.0-1.0 (default: 0.3, lower = more deterministic)",
        })
      ),
    }),
    execute: async (
      _toolCallId,
      params: {
        prompt: string;
        system?: string;
        model?: string;
        max_tokens?: number;
        temperature?: number;
      },
      signal,
      _onUpdate,
      ctx
    ) => {
      const baseUrl = getBaseUrl();

      // Refresh models if cache is empty
      if (cachedModels.length === 0) await refreshModels();

      if (!serverOnline) {
        return {
          content: [
            {
              type: "text" as const,
              text: `Local inference server not available at ${baseUrl}. Is Ollama running? Start with: ollama serve`,
            },
          ],
        };
      }

      // Model selection: explicit > auto (prefer largest/most capable)
      let modelId = params.model;
      if (!modelId) {
        // Prefer models roughly by capability heuristic (larger/newer = higher score)
        const ranked = [...cachedModels].sort((a, b) => {
          const score = (id: string) => {
            if (id.includes("qwen2.5:32b")) return 100;
            if (id.includes("qwen")) return 80;
            if (id.includes("llama3.1")) return 70;
            if (id.includes("llama")) return 60;
            if (id.includes("mistral")) return 50;
            if (id.includes("gemma")) return 45;
            return 30;
          };
          return score(b.id) - score(a.id);
        });
        modelId = ranked[0]?.id;
      }

      if (!modelId) {
        return {
          content: [
            {
              type: "text" as const,
              text: "No chat models available in Ollama. Pull a model with: ollama pull qwen2.5:32b",
            },
          ],
        };
      }

      const messages: ChatMessage[] = [];
      if (params.system) {
        messages.push({ role: "system", content: params.system });
      }
      messages.push({ role: "user", content: params.prompt });

      try {
        const result = await chatCompletion(baseUrl, modelId, messages, {
          maxTokens: params.max_tokens,
          temperature: params.temperature,
          signal: signal,
        });

        const parts: Array<{ type: "text"; text: string }> = [];
        parts.push({
          type: "text" as const,
          text: `**Local model:** ${modelId}\n**Tokens:** ${result.usage.prompt_tokens} in → ${result.usage.completion_tokens} out\n\n---\n\n${result.content}`,
        });

        if (result.reasoning) {
          parts.push({
            type: "text" as const,
            text: `\n\n---\n**Model reasoning:** ${result.reasoning}`,
          });
        }

        return { content: parts };
      } catch (err: any) {
        return {
          content: [
            {
              type: "text" as const,
              text: `Local inference error (${modelId}): ${err.message}`,
            },
          ],
        };
      }
    },
  });

  // List available local models
  pi.registerTool({
    name: "list_local_models",
    label: "List Local Models",
    description:
      "List all models currently available in the local inference server (LMStudio). " +
      "Use to check what's loaded before delegating work.",
    parameters: Type.Object({}),
    execute: async (_toolCallId, _params, _signal, _onUpdate, ctx) => {
      const baseUrl = getBaseUrl();
      const all = await listAllModels(baseUrl);

      if (all.length === 0) {
        return {
          content: [
            {
              type: "text" as const,
              text: `No models available at ${baseUrl}. Is Ollama running? Start with: ollama serve`,
            },
          ],
        };
      }

      const lines = all.map((m) => {
        const isEmbed = m.id.includes("embed");
        return `- \`${m.id}\` ${isEmbed ? "(embeddings)" : "(chat)"}`;
      });

      return {
        content: [
          {
            type: "text" as const,
            text: `**Local models at ${baseUrl}:**\n${lines.join("\n")}`,
          },
        ],
      };
    },
  });

  // Manual commands
  pi.registerCommand("local-models", {
    description: "List available local inference models",
    handler: async (_args, ctx) => {
      const models = await refreshModels();
      const all = await listAllModels(getBaseUrl());
      if (all.length === 0) {
        ctx.ui.notify("No local models available — is Ollama running?", "warning");
      } else {
        const names = all.map((m) => m.id).join("\n  ");
        ctx.ui.notify(`Local models:\n  ${names}`, "info");
      }
    },
  });

  pi.registerCommand("local-status", {
    description: "Check local inference server status",
    handler: async (_args, ctx) => {
      await refreshModels();
      if (serverOnline) {
        ctx.ui.notify(
          `🏠 Local inference online — ${cachedModels.length} chat models available`,
          "info"
        );
      } else {
        ctx.ui.notify(`Local inference offline at ${getBaseUrl()}`, "warning");
      }
    },
  });
}
