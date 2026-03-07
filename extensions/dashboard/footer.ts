/**
 * Custom footer component for the unified dashboard.
 *
 * Implements two rendering modes:
 *   Layer 0 (compact): 3 lines — dashboard summary + original footer data
 *   Layer 1 (raised):  up to 10 lines — section details + original footer data
 *
 * Reads sharedState for design-tree, openspec, and cleave data.
 * Reads footerData for git branch, extension statuses, provider count.
 * Reads ExtensionContext for token stats, model, context usage.
 */

import type { Component } from "@mariozechner/pi-tui";
import type { Theme, ThemeColor } from "@mariozechner/pi-coding-agent";
import type { ReadonlyFooterDataProvider } from "@mariozechner/pi-coding-agent";
import type { ExtensionContext } from "@mariozechner/pi-coding-agent";
import type { TUI } from "@mariozechner/pi-tui";
import { truncateToWidth } from "@mariozechner/pi-tui";
import type { DashboardState } from "./types.ts";
import { sharedState } from "../shared-state.ts";

/**
 * Format token counts to compact display (e.g. 1.2k, 45k, 1.3M)
 */
function formatTokens(count: number): string {
  if (count < 1000) return count.toString();
  if (count < 10000) return `${(count / 1000).toFixed(1)}k`;
  if (count < 1000000) return `${Math.round(count / 1000)}k`;
  if (count < 10000000) return `${(count / 1000000).toFixed(1)}M`;
  return `${Math.round(count / 1000000)}M`;
}

/**
 * Sanitize text for display in a single-line status.
 */
function sanitizeStatusText(text: string): string {
  return text
    .replace(/[\r\n\t]/g, " ")
    .replace(/ +/g, " ")
    .trim();
}

export class DashboardFooter implements Component {
  private tui: TUI;
  private theme: Theme;
  private footerData: ReadonlyFooterDataProvider;
  private dashState: DashboardState;
  private ctxRef: ExtensionContext | null = null;

  /** Cached cumulative token stats — updated incrementally. */
  private cachedTokens = { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, cost: 0 };
  private cachedThinkingLevel = "off";
  private lastEntryCount = 0;

  constructor(
    tui: TUI,
    theme: Theme,
    footerData: ReadonlyFooterDataProvider,
    dashState: DashboardState,
  ) {
    this.tui = tui;
    this.theme = theme;
    this.footerData = footerData;
    this.dashState = dashState;
  }

  /** Update the extension context reference (called on each event) */
  setContext(ctx: ExtensionContext): void {
    this.ctxRef = ctx;
  }

  /** No-op — theme is passed by reference */
  invalidate(): void {}

  dispose(): void {
    this.ctxRef = null;
  }

  render(width: number): string[] {
    if (this.dashState.mode === "raised") {
      return this.renderRaised(width);
    }
    return this.renderCompact(width);
  }

  // ── Compact Mode (Layer 0) ────────────────────────────────────

  private renderCompact(width: number): string[] {
    const theme = this.theme;
    const lines: string[] = [];

    // Width breakpoints — expand details as space allows
    const wide = width >= 120;
    const ultraWide = width >= 160;

    // Line 1: Dashboard summary + context gauge
    const dashParts: string[] = [];

    // Design tree summary — responsive expansion
    const dt = sharedState.designTree;
    if (dt && dt.nodeCount > 0) {
      if (ultraWide && dt.focusedNode) {
        // Ultra-wide: show focused node title inline
        const statusIcon = dt.focusedNode.status === "decided" ? "●"
          : dt.focusedNode.status === "implementing" ? "⚙"
          : dt.focusedNode.status === "exploring" ? "◐"
          : "○";
        const qSuffix = dt.focusedNode.questions.length > 0
          ? theme.fg("dim", ` (${dt.focusedNode.questions.length}?)`)
          : "";
        dashParts.push(theme.fg("accent", `◈ ${dt.decidedCount}/${dt.nodeCount}`) +
          ` ${statusIcon} ${dt.focusedNode.title}${qSuffix}`);
      } else if (wide) {
        // Wide: spell out counts
        const parts = [`${dt.decidedCount} decided`];
        if (dt.exploringCount > 0) parts.push(`${dt.exploringCount} exploring`);
        if (dt.implementingCount > 0) parts.push(`${dt.implementingCount} impl`);
        if (dt.openQuestionCount > 0) parts.push(`${dt.openQuestionCount}?`);
        dashParts.push(theme.fg("accent", `◈ Design`) + theme.fg("dim", ` ${parts.join(", ")}`));
      } else {
        // Narrow: terse
        let dtSummary = `◈ D:${dt.decidedCount}`;
        if (dt.implementingCount > 0) dtSummary += ` I:${dt.implementingCount}`;
        if (dt.implementedCount > 0) dtSummary += ` ✓:${dt.implementedCount}`;
        dtSummary += `/${dt.nodeCount}`;
        dashParts.push(theme.fg("accent", dtSummary));
      }
    }

    // OpenSpec summary — responsive expansion
    const os = sharedState.openspec;
    if (os && os.changes.length > 0) {
      const active = os.changes.filter(c => c.stage !== "archived");
      if (active.length > 0) {
        if (wide) {
          // Wide: show change names with progress
          const changeParts = active.slice(0, ultraWide ? 4 : 2).map(c => {
            const done = c.tasksTotal > 0 && c.tasksDone >= c.tasksTotal;
            const icon = done ? theme.fg("success", "✓") : "";
            const progress = c.tasksTotal > 0
              ? theme.fg("dim", ` ${c.tasksDone}/${c.tasksTotal}`)
              : "";
            return `${c.name}${progress}${icon}`;
          });
          const overflow = active.length > (ultraWide ? 4 : 2)
            ? theme.fg("dim", ` +${active.length - (ultraWide ? 4 : 2)}`)
            : "";
          dashParts.push(theme.fg("accent", "◎ Spec") + " " + changeParts.join(theme.fg("dim", " · ")) + overflow);
        } else {
          dashParts.push(theme.fg("accent", `◎ OS:${active.length}`));
        }
      }
    }

    // Cleave summary — responsive expansion
    const cl = sharedState.cleave;
    if (cl) {
      if (cl.status === "idle") {
        dashParts.push(theme.fg("dim", "⚡ idle"));
      } else if (cl.status === "done") {
        const childInfo = wide && cl.children
          ? ` ${cl.children.filter(c => c.status === "done").length}/${cl.children.length}`
          : "";
        dashParts.push(theme.fg("success", `⚡ done${childInfo}`));
      } else if (cl.status === "failed") {
        dashParts.push(theme.fg("error", "⚡ fail"));
      } else {
        // Active dispatch — show child progress at wide widths
        if (wide && cl.children && cl.children.length > 0) {
          const done = cl.children.filter(c => c.status === "done").length;
          const running = cl.children.filter(c => c.status === "running").length;
          dashParts.push(theme.fg("warning", `⚡ ${cl.status}`) +
            theme.fg("dim", ` ${done}✓ ${running}⟳ /${cl.children.length}`));
        } else {
          dashParts.push(theme.fg("warning", `⚡ ${cl.status}`));
        }
      }
    }

    // Context gauge — wider bar at wider terminals
    const barWidth = ultraWide ? 24 : wide ? 20 : 16;
    const gauge = this.buildContextGauge(barWidth);
    if (gauge) {
      dashParts.push(gauge);
    }

    if (dashParts.length > 0) {
      lines.push(truncateToWidth(dashParts.join("  "), width, "…"));
    }

    // Line 2-3: Original footer data (pwd + stats)
    lines.push(...this.renderFooterData(width));

    return lines;
  }

  // ── Raised Mode (Layer 1) ─────────────────────────────────────

  private renderRaised(width: number): string[] {
    const theme = this.theme;

    // At wide widths, use multi-column layout
    if (width >= 120) {
      return this.renderRaisedColumns(width);
    }

    const lines: string[] = [];

    // Design tree section
    lines.push(...this.buildDesignTreeLines(width));

    // OpenSpec section
    lines.push(...this.buildOpenSpecLines(width));

    // Cleave section
    lines.push(...this.buildCleaveLines());

    // Separator
    if (lines.length > 0) {
      lines.push(theme.fg("dim", "─".repeat(Math.min(width, 60))));
    }

    // Original footer data
    lines.push(...this.renderFooterData(width));

    // Cap at 10 lines
    return lines.slice(0, 10);
  }

  /** Multi-column raised layout for wide terminals (≥120 cols) */
  private renderRaisedColumns(width: number): string[] {
    const theme = this.theme;
    const divider = theme.fg("dim", " │ ");
    const dividerPlain = " │ ";

    // Build left column (Design Tree + Cleave) and right column (OpenSpec)
    const leftLines = this.buildDesignTreeLines(width);
    leftLines.push(...this.buildCleaveLines());
    const rightLines = this.buildOpenSpecLines(width);

    // Calculate column widths — give each half, minus divider
    const dividerWidth = dividerPlain.length;
    const colWidth = Math.floor((width - dividerWidth) / 2);

    // Merge columns side by side
    const merged: string[] = [];
    const maxRows = Math.max(leftLines.length, rightLines.length);
    for (let i = 0; i < maxRows; i++) {
      const left = i < leftLines.length ? leftLines[i] : "";
      const right = i < rightLines.length ? rightLines[i] : "";

      // Truncate left to column width, then pad to fill
      const leftTrunc = truncateToWidth(left, colWidth, "…");
      const leftVisLen = leftTrunc.replace(/\x1b\[[0-9;]*m/g, "").length;
      const leftPad = Math.max(0, colWidth - leftVisLen);
      merged.push(leftTrunc + " ".repeat(leftPad) + divider + right);
    }

    // Separator spans full width
    if (merged.length > 0) {
      merged.push(theme.fg("dim", "─".repeat(Math.min(width, 80))));
    }

    // Footer data
    merged.push(...this.renderFooterData(width));

    return merged.slice(0, 10);
  }

  // ── Section builders (shared by stacked + column layouts) ─────

  private buildDesignTreeLines(_width: number): string[] {
    const theme = this.theme;
    const lines: string[] = [];
    const dt = sharedState.designTree;
    if (!dt || dt.nodeCount === 0) return lines;

    const statusParts: string[] = [];
    if (dt.decidedCount > 0) statusParts.push(theme.fg("success", `${dt.decidedCount} decided`));
    if (dt.implementingCount > 0) statusParts.push(theme.fg("accent", `${dt.implementingCount} implementing`));
    if (dt.implementedCount > 0) statusParts.push(theme.fg("success", `${dt.implementedCount} implemented`));
    if (dt.exploringCount > 0) statusParts.push(theme.fg("accent", `${dt.exploringCount} exploring`));
    if (dt.blockedCount > 0) statusParts.push(theme.fg("error", `${dt.blockedCount} blocked`));
    if (dt.openQuestionCount > 0) statusParts.push(theme.fg("dim", `${dt.openQuestionCount}?`));

    lines.push(theme.fg("accent", "◈ Design Tree") + "  " + statusParts.join(" · "));

    if (dt.focusedNode) {
      const statusIcon = dt.focusedNode.status === "decided" ? theme.fg("success", "●")
        : dt.focusedNode.status === "implementing" ? theme.fg("accent", "⚙")
        : dt.focusedNode.status === "implemented" ? theme.fg("success", "✓")
        : dt.focusedNode.status === "exploring" ? theme.fg("accent", "◐")
        : dt.focusedNode.status === "blocked" ? theme.fg("error", "✕")
        : theme.fg("dim", "○");
      const qCount = dt.focusedNode.questions.length > 0
        ? theme.fg("dim", ` — ${dt.focusedNode.questions.length} open questions`)
        : "";
      const branchExtra = (dt.focusedNode.branchCount ?? 0) > 1
        ? theme.fg("dim", ` +${dt.focusedNode.branchCount! - 1}`)
        : "";
      const branchInfo = dt.focusedNode.status === "implementing" && dt.focusedNode.branch
        ? theme.fg("dim", ` → ${dt.focusedNode.branch}`) + branchExtra
        : "";
      lines.push(`  ${statusIcon} ${dt.focusedNode.title}${branchInfo}${qCount}`);
    }

    if (dt.implementingNodes && dt.implementingNodes.length > 0 && !dt.focusedNode) {
      for (const n of dt.implementingNodes.slice(0, 3)) {
        const branchSuffix = n.branch ? theme.fg("dim", ` → ${n.branch}`) : "";
        lines.push(`  ${theme.fg("accent", "⚙")} ${n.title}${branchSuffix}`);
      }
    }

    return lines;
  }

  private buildOpenSpecLines(_width: number): string[] {
    const theme = this.theme;
    const lines: string[] = [];
    const os = sharedState.openspec;
    if (!os || os.changes.length === 0) return lines;

    lines.push(theme.fg("accent", "◎ OpenSpec") + "  " + theme.fg("dim", `${os.changes.length} change${os.changes.length > 1 ? "s" : ""}`));
    for (const c of os.changes.slice(0, 3)) {
      const done = c.tasksTotal > 0 && c.tasksDone >= c.tasksTotal;
      const icon = done ? theme.fg("success", "✓") : theme.fg("dim", "◦");
      const progress = c.tasksTotal > 0
        ? theme.fg(done ? "success" : "dim", ` ${c.tasksDone}/${c.tasksTotal}`)
        : "";
      const stage = c.stage ? theme.fg("dim", ` [${c.stage}]`) : "";
      lines.push(`  ${icon} ${c.name}${progress}${stage}`);
    }

    return lines;
  }

  private buildCleaveLines(): string[] {
    const theme = this.theme;
    const lines: string[] = [];
    const cl = sharedState.cleave;
    if (!cl || cl.status === "idle") return lines;

    const statusColor: ThemeColor = cl.status === "done" ? "success"
      : cl.status === "failed" ? "error"
      : "warning";
    lines.push(theme.fg("accent", "⚡ Cleave") + "  " + theme.fg(statusColor, cl.status));

    if (cl.children && cl.children.length > 0) {
      const doneCount = cl.children.filter(c => c.status === "done").length;
      const failCount = cl.children.filter(c => c.status === "failed").length;
      const summary = `  ${doneCount}/${cl.children.length} ✓`;
      const failSuffix = failCount > 0 ? theme.fg("error", ` ${failCount} ✕`) : "";
      lines.push(theme.fg("dim", summary) + failSuffix);
    }

    return lines;
  }

  // ── Context Gauge (from status-bar) ───────────────────────────

  private buildContextGauge(barWidth: number): string {
    const theme = this.theme;
    const ctx = this.ctxRef;
    if (!ctx) return "";

    const usage = ctx.getContextUsage();
    const pct = usage?.percent ?? 0;
    const contextWindow = usage?.contextWindow ?? 0;

    // Calculate memory's share
    const memTokens = sharedState.memoryTokenEstimate;
    const memPct = contextWindow > 0 ? (memTokens / contextWindow) * 100 : 0;
    const convPct = Math.max(0, pct - memPct);

    // Convert to block counts (ceil ensures tiny values don't round to 0,
    // but the floor on totalFilled prevents overcount)
    const memBlocks = memPct > 0 ? Math.ceil((memPct / 100) * barWidth) : 0;
    const convBlocks = convPct > 0 ? Math.ceil((convPct / 100) * barWidth) : 0;
    const totalFilled = Math.min(memBlocks + convBlocks, barWidth);
    const freeBlocks = barWidth - totalFilled;

    // Severity color
    const convColor: ThemeColor = pct > 70 ? "error" : pct > 45 ? "warning" : "muted";

    let bar = "";
    if (memBlocks > 0) bar += theme.fg("accent", "▓".repeat(memBlocks));
    if (convBlocks > 0) bar += theme.fg(convColor, "█".repeat(convBlocks));
    if (freeBlocks > 0) bar += theme.fg("dim", "░".repeat(freeBlocks));

    const turns = this.dashState.turns;
    const pctStr = `${Math.round(pct)}%`;
    const pctColored = pct > 70 ? theme.fg("error", pctStr)
      : pct > 45 ? theme.fg("warning", pctStr)
      : theme.fg("dim", pctStr);

    return `${theme.fg("dim", `T${turns}`)} ${bar} ${pctColored}`;
  }

  // ── Original Footer Data ──────────────────────────────────────

  private renderFooterData(width: number): string[] {
    const theme = this.theme;
    const ctx = this.ctxRef;
    const lines: string[] = [];

    // pwd + git branch + session name
    let pwd = process.cwd();
    const home = process.env.HOME || process.env.USERPROFILE;
    if (home && pwd.startsWith(home)) {
      pwd = `~${pwd.slice(home.length)}`;
    }

    const branch = this.footerData.getGitBranch();
    if (branch) pwd = `${pwd} (${branch})`;

    const sessionName = ctx?.sessionManager?.getSessionName?.();
    if (sessionName) pwd = `${pwd} • ${sessionName}`;

    lines.push(truncateToWidth(theme.fg("dim", pwd), width, "…"));

    // Stats line: tokens + cost + context% + model
    if (ctx) {
      const statsParts: string[] = [];

      // Incrementally update cached token stats (only scan new entries)
      try {
        const entries = ctx.sessionManager.getEntries();
        for (let i = this.lastEntryCount; i < entries.length; i++) {
          const entry = entries[i] as any;
          if (entry.type === "message" && entry.message?.role === "assistant") {
            const usage = entry.message.usage;
            if (usage) {
              this.cachedTokens.input += usage.input || 0;
              this.cachedTokens.output += usage.output || 0;
              this.cachedTokens.cacheRead += usage.cacheRead || 0;
              this.cachedTokens.cacheWrite += usage.cacheWrite || 0;
              this.cachedTokens.cost += usage.cost?.total || 0;
            }
          }
          if (entry.type === "thinking_level_change" && entry.thinkingLevel) {
            this.cachedThinkingLevel = entry.thinkingLevel;
          }
        }
        this.lastEntryCount = entries.length;
      } catch { /* session may not be ready */ }

      const t = this.cachedTokens;
      if (t.input) statsParts.push(`↑${formatTokens(t.input)}`);
      if (t.output) statsParts.push(`↓${formatTokens(t.output)}`);
      if (t.cacheRead) statsParts.push(`R${formatTokens(t.cacheRead)}`);
      if (t.cacheWrite) statsParts.push(`W${formatTokens(t.cacheWrite)}`);

      if (t.cost) {
        statsParts.push(`$${t.cost.toFixed(3)}`);
      }

      // Context %
      const usage = ctx.getContextUsage();
      const pct = usage?.percent ?? 0;
      const contextWindow = usage?.contextWindow ?? 0;
      const pctDisplay = usage?.percent !== null
        ? `${pct.toFixed(1)}%/${formatTokens(contextWindow)}`
        : `?/${formatTokens(contextWindow)}`;

      if (pct > 90) {
        statsParts.push(theme.fg("error", pctDisplay));
      } else if (pct > 70) {
        statsParts.push(theme.fg("warning", pctDisplay));
      } else {
        statsParts.push(pctDisplay);
      }

      const statsLeft = statsParts.join(" ");

      // Right side: model + thinking
      const model = ctx.model;
      const modelName = model?.id || "no-model";
      let rightSide = modelName;

      // Thinking level (cached incrementally alongside token stats)
      if (model?.reasoning) {
        rightSide = this.cachedThinkingLevel === "off"
          ? `${modelName} • thinking off`
          : `${modelName} • ${this.cachedThinkingLevel}`;
      }

      // Multi-provider indicator
      if (this.footerData.getAvailableProviderCount() > 1 && model) {
        rightSide = `(${model.provider}) ${rightSide}`;
      }

      // Layout: left-align stats, right-align model
      const statsLeftPlain = statsLeft.replace(/\x1b\[[0-9;]*m/g, "");
      const rightSidePlain = rightSide.replace(/\x1b\[[0-9;]*m/g, "");
      const totalNeeded = statsLeftPlain.length + 2 + rightSidePlain.length;

      let statsLine: string;
      if (totalNeeded <= width) {
        const padding = " ".repeat(width - statsLeftPlain.length - rightSidePlain.length);
        statsLine = statsLeft + padding + rightSide;
      } else {
        statsLine = statsLeft;
      }

      lines.push(theme.fg("dim", statsLine));
    }

    // Extension statuses
    const extensionStatuses = this.footerData.getExtensionStatuses();
    if (extensionStatuses.size > 0) {
      const sortedStatuses = Array.from(extensionStatuses.entries())
        .sort(([a], [b]) => a.localeCompare(b))
        .map(([, text]) => sanitizeStatusText(text));
      const statusLine = sortedStatuses.join(" ");
      lines.push(truncateToWidth(statusLine, width, "…"));
    }

    return lines;
  }
}
