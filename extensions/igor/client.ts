/**
 * IgorClient — typed, timeout-safe fetch wrapper for Igor's nervous system API.
 *
 * Every method catches all errors and returns a safe default so the extension
 * never throws from a pi event handler.
 */

import { loadIgorContext, type IgorContext } from "./context.ts";

// ── Response types ────────────────────────────────────────────────────────────

export interface HealthResponse {
  ok: boolean;
  version?: string;
  brain?: string;
  baseline?: string;
  uptime_secs?: number;
}

export interface RecalledMemory {
  memory_id: string;
  domain: string;
  content: string;
  score: number;
  rank: number;
}

export interface RecallResponse {
  ok: boolean;
  recalled: RecalledMemory[];
  threshold_applied: number;
}

export interface EnvironmentResponse {
  ok: boolean;
  line: string;
  cpu_pct: number;
  mem_pct: number;
  uptime_secs: number;
  baseline_state: string;
}

export interface IngestEntry {
  role: string;
  content: string;
  domain: string;
  certainty: number;
  occurred_at: string;
}

export interface IntentPayload {
  type: string;
  [key: string]: unknown;
}

// ── Client ────────────────────────────────────────────────────────────────────

export class IgorClient {
  private readonly ctx: IgorContext;

  constructor(ctx: IgorContext) {
    this.ctx = ctx;
  }

  static fromContext(): IgorClient {
    return new IgorClient(loadIgorContext());
  }

  get baseUrl(): string { return this.ctx.url; }
  get apiKey(): string  { return this.ctx.apiKey; }
  get isConfigured(): boolean { return this.ctx.apiKey.length > 0; }

  private headers(): Record<string, string> {
    return {
      "Authorization": `Bearer ${this.ctx.apiKey}`,
      "Content-Type": "application/json",
    };
  }

  // ── Health ──────────────────────────────────────────────────────────────────

  async health(opts: { timeout?: number } = {}): Promise<HealthResponse> {
    if (!this.isConfigured) return { ok: false };
    try {
      const res = await fetch(`${this.ctx.url}/api/health`, {
        headers: this.headers(),
        signal: AbortSignal.timeout(opts.timeout ?? 3000),
      });
      if (!res.ok) return { ok: false };
      const data = await res.json() as HealthResponse;
      return { ...data, ok: true };
    } catch {
      return { ok: false };
    }
  }

  // ── Recall ──────────────────────────────────────────────────────────────────

  async recall(
    query: string,
    opts: { k?: number; domain?: string; threshold?: number; timeout?: number } = {}
  ): Promise<RecallResponse> {
    const empty: RecallResponse = { ok: true, recalled: [], threshold_applied: 0.3 };
    if (!this.isConfigured || !query.trim()) return empty;
    try {
      const params = new URLSearchParams({ q: query });
      if (opts.k !== undefined)         params.set("k",         String(opts.k));
      if (opts.domain !== undefined)    params.set("domain",    opts.domain);
      if (opts.threshold !== undefined) params.set("threshold", String(opts.threshold));
      const res = await fetch(`${this.ctx.url}/api/recall?${params}`, {
        headers: this.headers(),
        signal: AbortSignal.timeout(opts.timeout ?? 200),
      });
      if (!res.ok) return empty;
      return await res.json() as RecallResponse;
    } catch {
      return empty;
    }
  }

  // ── Environment ─────────────────────────────────────────────────────────────

  async environment(opts: { timeout?: number } = {}): Promise<EnvironmentResponse> {
    const empty: EnvironmentResponse = {
      ok: false, line: "", cpu_pct: 0, mem_pct: 0, uptime_secs: 0, baseline_state: "?",
    };
    if (!this.isConfigured) return empty;
    try {
      const res = await fetch(`${this.ctx.url}/api/environment`, {
        headers: this.headers(),
        signal: AbortSignal.timeout(opts.timeout ?? 200),
      });
      if (!res.ok) return empty;
      return await res.json() as EnvironmentResponse;
    } catch {
      return empty;
    }
  }

  // ── Ingest ──────────────────────────────────────────────────────────────────

  async ingest(entries: IngestEntry[], feature = "conversation"): Promise<void> {
    if (!this.isConfigured || entries.length === 0) return;
    try {
      await fetch(`${this.ctx.url}/api/ingest`, {
        method: "POST",
        headers: this.headers(),
        body: JSON.stringify({ entries, feature }),
        signal: AbortSignal.timeout(2000),
      });
    } catch { /* fire-and-forget */ }
  }

  // ── Intents ─────────────────────────────────────────────────────────────────

  async postIntents(intents: IntentPayload[]): Promise<void> {
    if (!this.isConfigured || intents.length === 0) return;
    try {
      await fetch(`${this.ctx.url}/api/intents`, {
        method: "POST",
        headers: this.headers(),
        body: JSON.stringify({ intents }),
        signal: AbortSignal.timeout(2000),
      });
    } catch { /* fire-and-forget */ }
  }
}
