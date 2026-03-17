/**
 * Igor context loader — reads ~/.config/igor/context.toml and resolves
 * the active context's URL and API key. Falls back to env vars and well-known
 * default paths so zero config is required for the common local case.
 */

import { readFileSync, existsSync } from "node:fs";
import { resolve } from "node:path";

export interface IgorContext {
  url: string;
  apiKey: string;
}

const HOME = process.env.HOME || process.env.USERPROFILE || "~";
const CONTEXT_TOML = resolve(HOME, ".config", "igor", "context.toml");
const DEFAULT_KEY_FILE = resolve(HOME, ".local", "share", "igor", "api.key");
const DEFAULT_URL = "http://localhost:8765";

export function loadIgorContext(): IgorContext {
  // 1. Try context.toml
  try {
    if (existsSync(CONTEXT_TOML)) {
      // Simple hand-rolled TOML parse for our narrow schema —
      // avoids a dependency on @iarna/toml at this layer.
      const raw = readFileSync(CONTEXT_TOML, "utf8");
      const active = (raw.match(/^active\s*=\s*"([^"]+)"/m) || [])[1] ?? "local";
      // Extract the [contexts.<active>] section
      const urlMatch = raw.match(
        new RegExp(`\\[contexts\\.${active}\\][^\\[]*url\\s*=\\s*"([^"]+)"`, "s")
      );
      const keyMatch = raw.match(
        new RegExp(`\\[contexts\\.${active}\\][^\\[]*key_file\\s*=\\s*"([^"]+)"`, "s")
      );
      const url = urlMatch?.[1] ?? DEFAULT_URL;
      const keyFilePath = keyMatch?.[1]
        ? resolve(keyMatch[1].replace("~", HOME))
        : DEFAULT_KEY_FILE;
      const apiKey = readKeyFile(keyFilePath);
      return { url, apiKey };
    }
  } catch {
    // fall through to env/default
  }

  // 2. Env vars
  const url = process.env.IGOR_URL ?? DEFAULT_URL;
  const apiKey =
    process.env.IGOR_API_KEY ??
    readKeyFile(process.env.IGOR_API_KEY_FILE ?? DEFAULT_KEY_FILE);

  return { url, apiKey };
}

function readKeyFile(path: string): string {
  try {
    return existsSync(path) ? readFileSync(path, "utf8").trim() : "";
  } catch {
    return "";
  }
}
