/**
 * status-bar — Consolidated status bar with session usage tracking
 *
 * Renders: ↑12 ↓1.4k R249k W43k $0.43 (sub) │ ████████░░░░ 21%/200k │ 🧠 T9
 *
 * Left:   Session token usage + estimated cost
 * Center: Context window gauge (green → tan → red)
 * Right:  Model tier icon + turn counter
 *
 * Also provides /usage command that runs claude-code-usage (ccu).
 */

import type { ExtensionAPI, ExtensionContext } from "@mariozechner/pi-coding-agent";

interface SessionUsage {
  input: number;
  output: number;
  cacheRead: number;
  cacheWrite: number;
  cost: number;
}

const TIER_ICONS: Record<string, string> = {
  "claude-opus-4-6": "🧠",
  "claude-sonnet-4-6": "⚡",
  "claude-haiku-4-5": "💨",
};

export default function (pi: ExtensionAPI) {
  let turnCount = 0;
  let currentState: "working" | "idle" = "idle";

  const session: SessionUsage = {
    input: 0,
    output: 0,
    cacheRead: 0,
    cacheWrite: 0,
    cost: 0,
  };

  // ANSI 256-color for the context gauge
  const ansi = (code: number, text: string) => `\x1b[38;5;${code}m${text}\x1b[0m`;

  function formatTokens(n: number): string {
    if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
    if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
    return n.toString();
  }

  function formatCost(n: number): string {
    if (n >= 100) return `$${Math.round(n)}`;
    if (n >= 10) return `$${n.toFixed(1)}`;
    return `$${n.toFixed(3)}`;
  }

  function contextBar(theme: ExtensionContext["ui"]["theme"], percent: number | null, contextWindow: number): string {
    const BAR_WIDTH = 14;
    const FILLED = "█";
    const EMPTY = "░";

    if (percent === null) {
      return theme.fg("dim", EMPTY.repeat(BAR_WIDTH));
    }

    const clamped = Math.max(0, Math.min(100, percent));
    const filled = Math.round((clamped / 100) * BAR_WIDTH);
    const empty = BAR_WIDTH - filled;

    let filledStr = "";
    for (let i = 0; i < filled; i++) {
      const blockPct = ((i + 0.5) / BAR_WIDTH) * 100;
      const color = blockPct <= 40 ? 34 : blockPct <= 60 ? 180 : 196;
      filledStr += ansi(color, FILLED);
    }

    const pctStr = `${Math.round(clamped)}%`;
    const winStr = contextWindow > 0 ? `/${formatTokens(contextWindow)}` : "";

    return `${filledStr}${theme.fg("dim", EMPTY.repeat(empty))} ${theme.fg("dim", pctStr + winStr)}`;
  }

  function render(ctx: ExtensionContext) {
    if (!ctx.hasUI) return;

    try {
      const theme = ctx.ui.theme;
      const dim = (s: string) => theme.fg("dim", s);
      const sep = dim(" │ ");
      const parts: string[] = [];

      // — Session usage —
      const usageParts: string[] = [];
      if (session.input > 0 || session.output > 0) {
        usageParts.push(dim("↑") + formatTokens(session.input));
        usageParts.push(dim("↓") + formatTokens(session.output));
        if (session.cacheRead > 0) usageParts.push(dim("R") + formatTokens(session.cacheRead));
        if (session.cacheWrite > 0) usageParts.push(dim("W") + formatTokens(session.cacheWrite));
        usageParts.push(theme.fg("accent", formatCost(session.cost)));
      }
      if (usageParts.length > 0) {
        parts.push(usageParts.join(" "));
      }

      // — Context gauge —
      const usage = ctx.getContextUsage();
      const pct = usage?.percent ?? null;
      const win = usage?.contextWindow || ctx.model?.contextWindow || 0;
      parts.push(contextBar(theme, pct, win));

      // — Tier icon + turn counter —
      const modelId = ctx.model?.id || "";
      const icon = TIER_ICONS[modelId] || "●";
      const stateIcon = currentState === "working"
        ? theme.fg("warning", icon)
        : theme.fg("success", icon);
      const turnStr = turnCount > 0 ? ` ${dim(`T${turnCount}`)}` : "";
      parts.push(`${stateIcon}${turnStr}`);

      ctx.ui.setStatus("status-bar", parts.join(sep));
    } catch {
      // Don't break anything
    }
  }

  // — Events —

  pi.on("session_start", async (_event, ctx) => {
    turnCount = 0;
    session.input = 0;
    session.output = 0;
    session.cacheRead = 0;
    session.cacheWrite = 0;
    session.cost = 0;
    currentState = "idle";
    render(ctx);
  });

  pi.on("turn_start", async (_event, ctx) => {
    turnCount++;
    currentState = "working";
    render(ctx);
  });

  pi.on("turn_end", async (_event, ctx) => {
    currentState = "idle";
    render(ctx);
  });

  // Accumulate usage from assistant messages
  pi.on("message_end", async (event: any, ctx) => {
    const msg = event?.message;
    if (msg?.role === "assistant" && msg?.usage) {
      const u = msg.usage;
      session.input += u.input || 0;
      session.output += u.output || 0;
      session.cacheRead += u.cacheRead || 0;
      session.cacheWrite += u.cacheWrite || 0;
      if (u.cost?.total) {
        session.cost += u.cost.total;
      }
    }
    render(ctx);
  });

  pi.on("tool_execution_end", async (_event, ctx) => {
    render(ctx);
  });

  // — /usage command — opens Claude usage dashboard
  pi.registerCommand("usage", {
    description: "Open Claude usage dashboard in browser",
    handler: async (_args, ctx) => {
      await pi.exec("open", ["https://claude.ai/settings/usage"]);
      ctx.ui.notify("Opened claude.ai/settings/usage", "info");
    },
  });
}
