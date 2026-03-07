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
import { debug } from "../debug.ts";

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
    debug("dashboard", "render", {
      mode: this.dashState.mode,
      width,
      hasDT: !!sharedState.designTree,
      hasOS: !!sharedState.openspec,
      hasCL: !!sharedState.cleave,
      hasCtx: !!this.ctxRef,
      hasTheme: !!this.theme,
      themeFgType: typeof this.theme?.fg,
    });
    try {
      if (this.dashState.mode === "raised") {
        return this.renderRaised(width);
      }
      return this.renderCompact(width);
    } catch (err: any) {
      debug("dashboard", "render:ERROR", {
        error: err?.message,
        stack: err?.stack?.split("\n").slice(0, 5).join(" | "),
      });
      return [`[dashboard render error: ${err?.message}]`];
    }
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
        // Wide: spell out counts, no node IDs (visible in raised mode)
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
          // Wide: aggregate progress only — individual changes visible in raised mode
          const totalDone = active.reduce((s, c) => s + c.tasksDone, 0);
          const totalAll = active.reduce((s, c) => s + c.tasksTotal, 0);
          const allDone = totalAll > 0 && totalDone >= totalAll;
          const progress = totalAll > 0
            ? theme.fg(allDone ? "success" : "dim", ` ${totalDone}/${totalAll}`)
            : "";
          const icon = allDone ? theme.fg("success", " ✓") : "";
          dashParts.push(theme.fg("accent", `◎ Spec`) +
            theme.fg("dim", ` ${active.length} change${active.length > 1 ? "s" : ""}`) +
            progress + icon);
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

    // Separator — thin rule matching section header style
    if (lines.length > 0) {
      const rule = "╶" + "─".repeat(Math.min(width - 2, 58)) + "╴";
      lines.push(theme.fg("dim", rule));
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

    // Separator — thin rule matching section header style
    if (merged.length > 0) {
      const rule = "╶" + "─".repeat(Math.min(width - 2, 78)) + "╴";
      merged.push(theme.fg("dim", rule));
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

    // Focused node gets priority display
    if (dt.focusedNode) {
      const statusIcon = this.nodeStatusIcon(dt.focusedNode.status);
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

    // Implementing nodes (if no focused node)
    if (dt.implementingNodes && dt.implementingNodes.length > 0 && !dt.focusedNode) {
      for (const n of dt.implementingNodes.slice(0, 3)) {
        const branchSuffix = n.branch ? theme.fg("dim", ` → ${n.branch}`) : "";
        lines.push(`  ${theme.fg("accent", "⚙")} ${n.title}${branchSuffix}`);
      }
    }

    // If no focused node and no implementing nodes, show all nodes (up to 4)
    if (!dt.focusedNode && (!dt.implementingNodes || dt.implementingNodes.length === 0) && dt.nodes) {
      const maxShow = 4;
      for (const n of dt.nodes.slice(0, maxShow)) {
        const icon = this.nodeStatusIcon(n.status);
        const qSuffix = n.questionCount > 0 ? theme.fg("dim", ` (${n.questionCount}?)`) : "";
        lines.push(`  ${icon} ${theme.fg("dim", n.id)}${qSuffix}`);
      }
      if (dt.nodes.length > maxShow) {
        lines.push(theme.fg("dim", `  +${dt.nodes.length - maxShow} more`));
      }
    }

    return lines;
  }

  private nodeStatusIcon(status: string): string {
    const theme = this.theme;
    switch (status) {
      case "decided": return theme.fg("success", "●");
      case "implementing": return theme.fg("accent", "⚙");
      case "implemented": return theme.fg("success", "✓");
      case "exploring": return theme.fg("accent", "◐");
      case "blocked": return theme.fg("error", "✕");
      case "seed": return theme.fg("dim", "○");
      default: return theme.fg("dim", "○");
    }
  }

  private buildOpenSpecLines(_width: number): string[] {
    const theme = this.theme;
    const lines: string[] = [];
    const os = sharedState.openspec;
    if (!os || os.changes.length === 0) return lines;

    // Header with count and aggregate progress
    const totalDone = os.changes.reduce((s, c) => s + c.tasksDone, 0);
    const totalAll = os.changes.reduce((s, c) => s + c.tasksTotal, 0);
    const allComplete = totalAll > 0 && totalDone >= totalAll;
    const aggregateProgress = totalAll > 0
      ? theme.fg(allComplete ? "success" : "dim", ` ${totalDone}/${totalAll} tasks`)
      : "";
    lines.push(theme.fg("accent", "◎ OpenSpec") + "  " +
      theme.fg("dim", `${os.changes.length} change${os.changes.length > 1 ? "s" : ""}`) +
      aggregateProgress);

    for (const c of os.changes.slice(0, 4)) {
      const done = c.tasksTotal > 0 && c.tasksDone >= c.tasksTotal;
      const icon = done ? theme.fg("success", "✓") : theme.fg("dim", "◦");
      const progress = c.tasksTotal > 0
        ? theme.fg(done ? "success" : "dim", ` ${c.tasksDone}/${c.tasksTotal}`)
        : "";

      // Stage with semantic coloring
      const stageColor = c.stage === "verifying" ? "warning"
        : c.stage === "implementing" ? "accent"
        : c.stage === "ready" ? "success"
        : "dim";
      const stage = c.stage ? theme.fg(stageColor, ` [${c.stage}]`) : "";

      // Artifact badges — show which lifecycle files exist
      const artifacts = c.artifacts && c.artifacts.length > 0
        ? " " + theme.fg("dim", c.artifacts.map(a => a[0]).join(""))
        : "";

      lines.push(`  ${icon} ${c.name}${progress}${stage}${artifacts}`);
    }

    // Hint for actionable next steps
    if (allComplete && os.changes.some(c => c.stage === "verifying")) {
      lines.push(theme.fg("dim", "  → /opsx:verify → /opsx:archive"));
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
    debug("dashboard", "renderFooterData:enter", {
      width,
      hasCtx: !!this.ctxRef,
      hasTheme: !!this.theme,
      hasBranch: !!this.footerData?.getGitBranch?.(),
    });
    const theme = this.theme;
    const ctx = this.ctxRef;
    const lines: string[] = [];
    const wide = width >= 120;

    // ── Line 1: pwd + git branch + session ──
    let pwd = process.cwd();
    const home = process.env.HOME || process.env.USERPROFILE;
    if (home && pwd.startsWith(home)) {
      pwd = `~${pwd.slice(home.length)}`;
    }

    let pwdLine = theme.fg("dim", "⌂ ") + theme.fg("muted", pwd);

    const branch = this.footerData.getGitBranch();
    if (branch) {
      // Color branch by convention: feature→accent, fix→warning, main/master→success
      const branchColor: ThemeColor = /^(main|master)$/.test(branch) ? "success"
        : branch.startsWith("feature/") ? "accent"
        : branch.startsWith("fix/") || branch.startsWith("hotfix/") ? "warning"
        : branch.startsWith("refactor/") ? "accent"
        : "muted";
      pwdLine += theme.fg("dim", "  ") + theme.fg(branchColor, branch);
    }

    const sessionName = ctx?.sessionManager?.getSessionName?.();
    if (sessionName) {
      pwdLine += theme.fg("dim", " • ") + theme.fg("muted", sessionName);
    }

    lines.push(truncateToWidth(pwdLine, width, "…"));

    // ── Line 2: token stats + cost │ model + thinking ──
    if (ctx) {
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

      // Left side: token flow + cost + context window
      const t = this.cachedTokens;
      const tokenParts: string[] = [];
      if (t.input) tokenParts.push(theme.fg("accent", "↑") + theme.fg("dim", formatTokens(t.input)));
      if (t.output) tokenParts.push(theme.fg("success", "↓") + theme.fg("dim", formatTokens(t.output)));
      if (wide) {
        if (t.cacheRead) tokenParts.push(theme.fg("muted", "⟐") + theme.fg("dim", formatTokens(t.cacheRead)));
        if (t.cacheWrite) tokenParts.push(theme.fg("muted", "⟑") + theme.fg("dim", formatTokens(t.cacheWrite)));
      }

      // Cost with severity coloring
      if (t.cost) {
        const costStr = `$${t.cost.toFixed(3)}`;
        const costColor: ThemeColor = t.cost > 5 ? "error" : t.cost > 1 ? "warning" : "dim";
        tokenParts.push(theme.fg(costColor, costStr));
      }

      // Context % with window size
      const usage = ctx.getContextUsage();
      const pct = usage?.percent ?? 0;
      const contextWindow = usage?.contextWindow ?? 0;
      const pctColor: ThemeColor = pct > 90 ? "error" : pct > 70 ? "warning" : pct > 45 ? "muted" : "dim";
      const pctStr = usage?.percent !== null ? `${pct.toFixed(1)}%` : "?";
      tokenParts.push(theme.fg(pctColor, pctStr) + theme.fg("dim", `/${formatTokens(contextWindow)}`));

      const statsLeft = tokenParts.join(theme.fg("dim", " "));

      // Right side: provider + model + thinking level badge
      const model = ctx.model;
      const modelName = model?.id || "no-model";
      const rightParts: string[] = [];

      // Multi-provider indicator
      if (this.footerData.getAvailableProviderCount() > 1 && model) {
        rightParts.push(theme.fg("dim", `(${model.provider})`));
      }

      rightParts.push(theme.fg("muted", modelName));

      // Thinking level badge with semantic color
      if (model?.reasoning) {
        const thinkColor: ThemeColor = this.cachedThinkingLevel === "high" ? "accent"
          : this.cachedThinkingLevel === "medium" ? "muted"
          : this.cachedThinkingLevel === "low" || this.cachedThinkingLevel === "minimal" ? "dim"
          : "dim";
        const thinkIcon = this.cachedThinkingLevel === "off" ? "○" : "◉";
        rightParts.push(theme.fg("dim", "•") + " " +
          theme.fg(thinkColor, `${thinkIcon} ${this.cachedThinkingLevel}`));
      }

      const rightSide = rightParts.join(" ");

      // Layout: left-align stats, right-align model
      const statsLeftPlain = statsLeft.replace(/\x1b\[[0-9;]*m/g, "").length;
      const rightSidePlain = rightSide.replace(/\x1b\[[0-9;]*m/g, "").length;

      let statsLine: string;
      if (statsLeftPlain + 2 + rightSidePlain <= width) {
        const padding = " ".repeat(width - statsLeftPlain - rightSidePlain);
        statsLine = statsLeft + padding + rightSide;
      } else {
        statsLine = statsLeft;
      }

      lines.push(statsLine);
    }

    // ── Extension statuses — raised mode only ──
    if (this.dashState.mode === "raised") {
      const extensionStatuses = this.footerData.getExtensionStatuses();
      if (extensionStatuses.size > 0) {
        const sortedStatuses = Array.from(extensionStatuses.entries())
          .sort(([a], [b]) => a.localeCompare(b))
          .map(([name, text]) => {
            const cleanText = sanitizeStatusText(text);
            return theme.fg("dim", "▪ ") + theme.fg("muted", cleanText);
          });
        const statusLine = sortedStatuses.join(theme.fg("dim", "  "));
        lines.push(truncateToWidth(statusLine, width, "…"));
      }
    }

    return lines;
  }
}
