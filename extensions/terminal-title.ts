import type { ExtensionAPI } from "@mariozechner/pi-coding-agent";
import { basename } from "path";

/**
 * Dynamic terminal tab title for multi-tab workflows.
 *
 * Examples:
 *   π ai *                     — idle, awaiting input
 *   π ai: editing memory ext   — working on user's request
 *   π ai: ⚙ Bash              — executing a tool
 *   π ai: editing memory ext * — done, awaiting input
 */
export default function (pi: ExtensionAPI) {
  const project = basename(process.cwd());
  let promptSnippet = "";
  let toolOverride = "";
  let idle = true;
  let ctx: any = null;

  function truncate(text: string, max: number): string {
    const clean = text.split("\n")[0].trim().replace(/\s+/g, " ");
    if (clean.length <= max) return clean;
    return clean.slice(0, max).trimEnd() + "…";
  }

  function render() {
    if (!ctx?.ui?.setTitle) return;
    const snippet = toolOverride || promptSnippet;
    let title = `π ${project}`;
    if (snippet) title += `: ${snippet}`;
    if (idle) title += " *";
    ctx.ui.setTitle(title);
  }

  // --- Session lifecycle (defer past pi's own updateTerminalTitle) ---

  pi.on("session_start", (_e, c) => {
    ctx = c;
    promptSnippet = "";
    toolOverride = "";
    idle = true;
    setTimeout(render, 50);
  });

  pi.on("session_switch", (_e, c) => {
    ctx = c;
    promptSnippet = "";
    toolOverride = "";
    idle = true;
    setTimeout(render, 50);
  });

  pi.on("session_fork", (_e, c) => {
    ctx = c;
    promptSnippet = "";
    toolOverride = "";
    idle = true;
    setTimeout(render, 50);
  });

  // --- Agent lifecycle ---

  pi.on("before_agent_start", (event) => {
    if (event.prompt) {
      promptSnippet = truncate(event.prompt, 30);
    }
  });

  pi.on("agent_start", (_e, c) => {
    ctx = c;
    idle = false;
    toolOverride = "";
    render();
  });

  pi.on("tool_execution_start", (event) => {
    toolOverride = `⚙ ${event.toolName}`;
    render();
  });

  pi.on("tool_execution_end", () => {
    toolOverride = "";
    render();
  });

  pi.on("agent_end", (_e, c) => {
    ctx = c;
    idle = true;
    toolOverride = "";
    render();
  });
}
