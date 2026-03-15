/**
 * Direct HTTP LLM chat — zero subprocess overhead.
 *
 * Replaces spawnExtraction() which launched a full Omegon runtime just to
 * make a single chat completion. Supports Anthropic, OpenAI, and Ollama
 * APIs directly via fetch().
 *
 * Provider auto-detection: resolves from model name prefix + available API keys.
 */

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface ChatRequest {
  model: string;
  systemPrompt: string;
  userMessage: string;
  maxTokens?: number;
  temperature?: number;
  timeout?: number;
  signal?: AbortSignal;
}

export interface ChatResult {
  content: string;
  provider: "anthropic" | "openai" | "ollama";
}

// ---------------------------------------------------------------------------
// Provider detection
// ---------------------------------------------------------------------------

const ANTHROPIC_PREFIXES = ["claude-"];
const OPENAI_PREFIXES = ["gpt-", "o1-", "o3-", "o4-"];

type Provider = "anthropic" | "openai" | "ollama";

function detectProvider(model: string): Provider {
  for (const p of ANTHROPIC_PREFIXES) {
    if (model.startsWith(p)) return "anthropic";
  }
  for (const p of OPENAI_PREFIXES) {
    if (model.startsWith(p)) return "openai";
  }
  // Everything else assumed local/Ollama
  return "ollama";
}

function getApiKey(provider: Provider): string | null {
  switch (provider) {
    case "anthropic":
      return process.env.ANTHROPIC_API_KEY?.trim() || null;
    case "openai":
      return process.env.OPENAI_API_KEY?.trim() || null;
    case "ollama":
      return null; // No key needed
  }
}

function getBaseUrl(provider: Provider): string {
  switch (provider) {
    case "anthropic":
      return process.env.ANTHROPIC_BASE_URL?.trim() || "https://api.anthropic.com";
    case "openai":
      return process.env.OPENAI_BASE_URL?.trim() || "https://api.openai.com";
    case "ollama":
      return process.env.OLLAMA_HOST?.trim() || process.env.LOCAL_INFERENCE_URL?.trim() || "http://localhost:11434";
  }
}

// ---------------------------------------------------------------------------
// Provider-specific chat implementations
// ---------------------------------------------------------------------------

async function chatAnthropic(
  baseUrl: string,
  apiKey: string,
  req: ChatRequest,
  signal: AbortSignal,
): Promise<string> {
  const resp = await fetch(`${baseUrl}/v1/messages`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "x-api-key": apiKey,
      "anthropic-version": "2023-06-01",
    },
    body: JSON.stringify({
      model: req.model,
      max_tokens: req.maxTokens ?? 2048,
      temperature: req.temperature ?? 0.2,
      system: req.systemPrompt,
      messages: [{ role: "user", content: req.userMessage }],
    }),
    signal,
  });

  if (!resp.ok) {
    const body = await resp.text().catch(() => "");
    throw new Error(`Anthropic API ${resp.status}: ${body.slice(0, 200)}`);
  }

  const data = await resp.json() as {
    content?: Array<{ type: string; text?: string }>;
  };
  const text = data.content?.find(b => b.type === "text")?.text?.trim();
  if (!text) throw new Error("Anthropic returned empty content");
  return text;
}

async function chatOpenAI(
  baseUrl: string,
  apiKey: string,
  req: ChatRequest,
  signal: AbortSignal,
): Promise<string> {
  const resp = await fetch(`${baseUrl}/v1/chat/completions`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Authorization": `Bearer ${apiKey}`,
    },
    body: JSON.stringify({
      model: req.model,
      max_tokens: req.maxTokens ?? 2048,
      temperature: req.temperature ?? 0.2,
      messages: [
        { role: "system", content: req.systemPrompt },
        { role: "user", content: req.userMessage },
      ],
    }),
    signal,
  });

  if (!resp.ok) {
    const body = await resp.text().catch(() => "");
    throw new Error(`OpenAI API ${resp.status}: ${body.slice(0, 200)}`);
  }

  const data = await resp.json() as {
    choices?: Array<{ message?: { content?: string } }>;
  };
  const text = data.choices?.[0]?.message?.content?.trim();
  if (!text) throw new Error("OpenAI returned empty content");
  return text;
}

async function chatOllama(
  baseUrl: string,
  req: ChatRequest,
  signal: AbortSignal,
): Promise<string> {
  const resp = await fetch(`${baseUrl}/api/chat`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      model: req.model,
      stream: false,
      options: {
        temperature: req.temperature ?? 0.2,
        num_predict: req.maxTokens ?? 2048,
        num_ctx: 32768,
      },
      messages: [
        { role: "system", content: req.systemPrompt },
        { role: "user", content: req.userMessage },
      ],
    }),
    signal,
  });

  if (!resp.ok) {
    const body = await resp.text().catch(() => "");
    throw new Error(`Ollama API ${resp.status}: ${body.slice(0, 200)}`);
  }

  const data = await resp.json() as { message?: { content?: string } };
  const text = data.message?.content?.trim();
  if (!text) throw new Error("Ollama returned empty content");
  return text;
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Clean model output: strip code fences and <think> blocks.
 */
export function cleanModelOutput(raw: string): string {
  return raw
    .replace(/^```(?:jsonl?|json)?\n?/, "")
    .replace(/\n?```\s*$/, "")
    .replace(/<think>[\s\S]*?<\/think>\s*/g, "")
    .trim();
}

/**
 * Send a chat completion to the appropriate provider.
 * Auto-detects provider from model name. Returns cleaned content.
 * Throws on failure (caller handles fallback).
 */
export async function chatDirect(req: ChatRequest): Promise<ChatResult> {
  const provider = detectProvider(req.model);
  const baseUrl = getBaseUrl(provider);
  const timeout = req.timeout ?? 60_000;

  // Build combined signal: external abort + timeout
  const signals: AbortSignal[] = [AbortSignal.timeout(timeout)];
  if (req.signal) signals.push(req.signal);
  const combinedSignal = typeof AbortSignal.any === "function"
    ? AbortSignal.any(signals)
    : signals[0]; // Node <20.3 fallback

  let content: string;

  if (provider === "ollama") {
    content = await chatOllama(baseUrl, req, combinedSignal);
  } else {
    const apiKey = getApiKey(provider);
    if (!apiKey) {
      throw new Error(`No API key for ${provider} (set ${provider === "anthropic" ? "ANTHROPIC_API_KEY" : "OPENAI_API_KEY"})`);
    }
    if (provider === "anthropic") {
      content = await chatAnthropic(baseUrl, apiKey, req, combinedSignal);
    } else {
      content = await chatOpenAI(baseUrl, apiKey, req, combinedSignal);
    }
  }

  return { content: cleanModelOutput(content), provider };
}

/**
 * Check if a model ID is a cloud model (has API key available).
 * Returns false for Ollama/local models.
 */
export function isCloudModel(model: string): boolean {
  const provider = detectProvider(model);
  if (provider === "ollama") return false;
  return getApiKey(provider) !== null;
}

/**
 * Check if any cloud provider is available (has API key configured).
 */
export function hasCloudProvider(): boolean {
  return !!(process.env.ANTHROPIC_API_KEY?.trim() || process.env.OPENAI_API_KEY?.trim());
}

/**
 * Get the best available cloud model for lightweight tasks.
 * Prefers Haiku (cheapest), falls back to GPT models.
 */
export function getBudgetCloudModel(): string | null {
  if (process.env.ANTHROPIC_API_KEY?.trim()) return "claude-haiku-4-5";
  if (process.env.OPENAI_API_KEY?.trim()) return "gpt-4.1-mini";
  return null;
}
