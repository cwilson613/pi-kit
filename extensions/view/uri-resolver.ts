/**
 * URI resolver for context-aware OSC 8 hyperlinks.
 *
 * Routes file paths to the best URI scheme based on:
 *   - File extension (.md → mdserve, code → editor, .excalidraw → Obsidian)
 *   - Running services (mdserve port in shared state)
 *   - User config (.pi/config.json editor preference)
 *   - Obsidian vault detection (walk parents for .obsidian/)
 *
 * Fallback is always file://
 */

import { existsSync, readFileSync } from "node:fs";
import { basename, dirname, extname, join, relative } from "node:path";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface PiConfig {
  editor?: string;
}

export interface ResolveUriOptions {
  mdservePort?: number;
  config?: PiConfig;
  /** Override project root for mdserve relative paths. Defaults to cwd. */
  projectRoot?: string;
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const CODE_EXTS = new Set([
  ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs",
  ".py", ".rs", ".go", ".c", ".cpp", ".cc", ".h", ".hpp",
  ".java", ".kt", ".rb", ".lua", ".sh", ".bash", ".zsh",
  ".css", ".scss", ".less", ".sql", ".swift", ".zig",
  ".vue", ".svelte", ".elm", ".hs", ".ml", ".ex", ".exs",
  ".php", ".pl", ".r", ".scala", ".dart", ".nim",
]);

const MARKDOWN_EXTS = new Set([".md", ".markdown", ".mdx"]);

const KNOWN_EDITORS = new Set(["vscode", "cursor", "zed"]);

// ---------------------------------------------------------------------------
// Config loading
// ---------------------------------------------------------------------------

/**
 * Load .pi/config.json from the given directory (or cwd).
 * Returns empty config on missing/invalid file — never throws.
 */
export function loadConfig(root?: string): PiConfig {
  const configPath = join(root ?? process.cwd(), ".pi", "config.json");
  try {
    if (!existsSync(configPath)) return {};
    const raw = JSON.parse(readFileSync(configPath, "utf-8"));
    if (raw && typeof raw === "object" && !Array.isArray(raw)) {
      return { editor: typeof raw.editor === "string" ? raw.editor : undefined };
    }
    return {};
  } catch {
    return {};
  }
}

// ---------------------------------------------------------------------------
// Obsidian vault detection
// ---------------------------------------------------------------------------

/**
 * Walk parent directories looking for .obsidian/ folder.
 * Returns { vaultName, vaultRoot } or undefined.
 */
export function detectObsidianVault(absPath: string): { vaultName: string; vaultRoot: string } | undefined {
  let dir = dirname(absPath);
  const seen = new Set<string>();
  while (dir && !seen.has(dir)) {
    seen.add(dir);
    if (existsSync(join(dir, ".obsidian"))) {
      return { vaultName: basename(dir), vaultRoot: dir };
    }
    const parent = dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  return undefined;
}

// ---------------------------------------------------------------------------
// URI resolution
// ---------------------------------------------------------------------------

/**
 * Resolve the best URI for a given absolute file path.
 */
export function resolveUri(absPath: string, options?: ResolveUriOptions): string {
  const ext = extname(absPath).toLowerCase();
  const config = options?.config ?? loadConfig();
  const mdservePort = options?.mdservePort;
  const projectRoot = options?.projectRoot ?? process.cwd();

  // Markdown → mdserve if running, else file://
  if (MARKDOWN_EXTS.has(ext)) {
    if (mdservePort) {
      const rel = relative(projectRoot, absPath);
      return `http://localhost:${mdservePort}/${rel}`;
    }
    return `file://${absPath}`;
  }

  // Excalidraw → Obsidian if vault detected, else file://
  if (ext === ".excalidraw") {
    const vault = detectObsidianVault(absPath);
    if (vault) {
      const relPath = relative(vault.vaultRoot, absPath);
      return `obsidian://open?vault=${encodeURIComponent(vault.vaultName)}&file=${encodeURIComponent(relPath)}`;
    }
    return `file://${absPath}`;
  }

  // Code files → editor scheme if configured, else file://
  if (CODE_EXTS.has(ext)) {
    const editor = config.editor;
    if (editor && KNOWN_EDITORS.has(editor)) {
      return `${editor}://file/${absPath}`;
    }
    return `file://${absPath}`;
  }

  // Everything else (images, PDFs, etc.) → file://
  return `file://${absPath}`;
}

// ---------------------------------------------------------------------------
// OSC 8 helpers
// ---------------------------------------------------------------------------

/**
 * Wrap text in an OSC 8 hyperlink escape sequence.
 * Terminals that don't support OSC 8 simply ignore the sequences.
 */
export function osc8Link(uri: string, text: string): string {
  return `\x1b]8;;${uri}\x1b\\${text}\x1b]8;;\x1b\\`;
}
