/**
 * Pure utility functions extracted for testability.
 * The main index.ts imports from here; tests import directly.
 */

// ---------------------------------------------------------------------------
// Config discrimination
// ---------------------------------------------------------------------------

export interface StdioServerConfig {
  command: string;
  args?: string[];
  env?: Record<string, string>;
}

export interface HttpServerConfig {
  url: string;
  headers?: Record<string, string>;
  timeout?: number;
}

export type ServerConfig = StdioServerConfig | HttpServerConfig;

export function isHttpConfig(config: ServerConfig): config is HttpServerConfig {
  return "url" in config;
}

// ---------------------------------------------------------------------------
// Env var resolution
// ---------------------------------------------------------------------------

export function resolveEnvVars(
  value: string,
  env: Record<string, string | undefined> = process.env
): string {
  return value.replace(/\$\{(\w+)\}/g, (_, key) => env[key] ?? "");
}

export function resolveEnvObj(
  obj: Record<string, string>,
  env: Record<string, string | undefined> = process.env
): Record<string, string> {
  const resolved: Record<string, string> = {};
  for (const [k, v] of Object.entries(obj)) {
    resolved[k] = resolveEnvVars(v, env);
  }
  return resolved;
}

// ---------------------------------------------------------------------------
// Auth error detection
// ---------------------------------------------------------------------------

export const AUTH_REMEDIATION =
  "Your GitHub token may be expired or invalid.\n" +
  "Run `gh auth login` to re-authenticate, then restart your pi session.";

export function isAuthError(err: any): boolean {
  if (err?.code === 401 || err?.code === 403) return true;
  const msg = err?.message ?? "";
  if (/HTTP\s+40[13]\b/.test(msg)) return true;
  if (/unauthorized|forbidden|invalid.*token|expired.*token|token.*expired/i.test(msg)) return true;
  return false;
}

// ---------------------------------------------------------------------------
// Transport error detection
// ---------------------------------------------------------------------------

export function isTransportError(err: any): boolean {
  const msg = err?.message ?? "";
  return (
    msg.includes("not connected") ||
    msg.includes("aborted") ||
    msg.includes("ECONNREFUSED") ||
    msg.includes("fetch failed") ||
    msg.includes("network") ||
    err?.code === "ECONNRESET"
  );
}

// ---------------------------------------------------------------------------
// Response text extraction
// ---------------------------------------------------------------------------

export function extractText(result: any): string {
  return (result.content as any[])
    .filter((c: any) => c.type === "text")
    .map((c: any) => c.text)
    .join("\n") || "(empty response)";
}
