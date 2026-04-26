// Import the generated snippets JSON (produced by scripts/load-snippets.mjs).
// Astro pages call snippet("install.quick_install") to get the canonical command.
import data from "../data/snippets.json";

interface Snippet {
  cmd: string;
  desc: string;
}

const snippets = data as Record<string, Snippet>;

/** Return the command string for a snippet key, e.g. "install.quick_install". */
export function snippet(key: string): string {
  const s = snippets[key];
  if (!s) throw new Error(`Unknown snippet key: "${key}"`);
  return s.cmd;
}

/** Return the full snippet object (cmd + desc). */
export function snippetFull(key: string): Snippet {
  const s = snippets[key];
  if (!s) throw new Error(`Unknown snippet key: "${key}"`);
  return s;
}
