/**
 * splash — Omegon branded startup splash with glitch-convergence logo animation.
 *
 * Replaces the default keybinding-hint header with an animated ASCII logo
 * that resolves from CRT noise, plus a loading checklist that tracks
 * subsystem initialisation in real-time.
 *
 * After the animation completes and loading finishes, transitions to a
 * minimal branded header with version + condensed help.
 */

import type { ExtensionAPI } from "@styrene-lab/pi-coding-agent";
import type { Component, TUI } from "@styrene-lab/pi-tui";
import { truncateToWidth } from "@styrene-lab/pi-tui";
import {
  LOGO_LINES,
  WORDMARK_LINES,
  LINE_WIDTH,
  COMPACT_LOGO_LINES,
  COMPACT_LINE_WIDTH,
  COMPACT_MARK_ROWS,
  FRAME_INTERVAL_MS,
  TOTAL_FRAMES,
  HOLD_FRAMES,
  assignUnlockFrames,
  renderFrame,
  PRIMARY,
  DIM,
  BRIGHT,
  SUCCESS,
  ERROR_CLR,
  RESET,
  BOLD,
} from "./logo.js";

// ---------------------------------------------------------------------------
// Shared state — other extensions write here, splash reads during render
// ---------------------------------------------------------------------------
const SPLASH_KEY = Symbol.for("omegon:splash");

export interface SplashItem {
  label: string;
  state: "hidden" | "pending" | "active" | "done" | "failed";
}

export interface QueuedNotification {
  message: string;
  type: "info" | "warning" | "error";
}

export interface SplashState {
  items: SplashItem[];
  /** Set to true when all session_start hooks have returned */
  loadingComplete: boolean;
  /** True while splash is displayed — notifications should be queued */
  active: boolean;
  /** Queued notifications to flush after splash dismissal */
  notificationQueue: QueuedNotification[];
}

function getSharedState(): SplashState {
  let state = (globalThis as any)[SPLASH_KEY] as SplashState | undefined;
  if (!state) {
    state = {
      items: [
        { label: "secrets",    state: "pending" },
        { label: "providers",  state: "pending" },
        { label: "memory",     state: "pending" },
        { label: "mcp",        state: "pending" },
        { label: "tools",      state: "pending" },
      ],
      loadingComplete: false,
      active: true,
      notificationQueue: [],
    };
    (globalThis as any)[SPLASH_KEY] = state;
  }
  return state;
}

/** Update a checklist item by label. Called by other extensions. */
export function splashUpdate(label: string, state: SplashItem["state"]): void {
  const s = getSharedState();
  const item = s.items.find(i => i.label === label);
  if (item) item.state = state;
}

/** Mark all loading complete. */
export function splashDone(): void {
  const s = getSharedState();
  s.loadingComplete = true;
  // Mark any still-pending items as done
  for (const item of s.items) {
    if (item.state === "pending" || item.state === "active") {
      item.state = "done";
    }
  }
}

// ---------------------------------------------------------------------------
// Checklist scan animation
// ---------------------------------------------------------------------------
const SCAN_FRAMES  = ["░ ", "▒ ", "▓ ", "▒ ", "░ ", "▸ ", "▸ ", "▸ "];
const DONE_GLYPH   = "✓ ";
const FAIL_GLYPH   = "✗ ";
const PENDING_GLYPH = "· ";

// ---------------------------------------------------------------------------
// Splash header component
// ---------------------------------------------------------------------------
class SplashHeader implements Component {
  private tui: TUI;
  private lines: string[];
  private frame = 0;
  private frameMap: ReturnType<typeof assignUnlockFrames>;
  private noiseSeed = (Date.now() * 7) & 0x7fffffff;
  private timer: ReturnType<typeof setInterval> | null = null;
  private scanFrame = 0;
  private animDone = false;
  private holdCount = 0;
  private transitioned = false;
  private onTransition: (() => void) | null = null;
  private cachedLines: string[] | undefined;
  private cachedWidth: number | undefined;
  private promptBlink = false;

  private markRows: number;
  private logoWidth: number;

  constructor(tui: TUI, onTransition: () => void, lines: string[], markRows: number, logoWidth: number) {
    this.tui = tui;
    this.onTransition = onTransition;
    this.lines = lines;
    this.markRows = markRows;
    this.logoWidth = logoWidth;
    this.frameMap = assignUnlockFrames(lines, TOTAL_FRAMES, Date.now() & 0xffff);
  }

  start(): void {
    this.timer = setInterval(() => this.tick(), FRAME_INTERVAL_MS);
  }

  /** Called externally (e.g. on keypress) to dismiss the splash. */
  dismiss(): void {
    if (this.transitioned) return;
    this.transitioned = true;
    this.dispose();
    this.onTransition?.();
    this.onTransition = null;
  }

  /** True when the animation is done and loading is complete — ready for dismissal. */
  get readyToDismiss(): boolean {
    const state = getSharedState();
    return this.animDone && this.holdCount >= HOLD_FRAMES && state.loadingComplete;
  }

  private tick(): void {
    this.frame++;
    this.scanFrame = (this.scanFrame + 1) % SCAN_FRAMES.length;
    this.cachedLines = undefined;

    if (this.frame >= TOTAL_FRAMES && !this.animDone) {
      this.animDone = true;
    }

    if (this.animDone && !this.transitioned) {
      this.holdCount++;
      // Blink the "press any key" prompt every ~500ms (10 frames at 50ms)
      if (this.readyToDismiss && this.holdCount % 10 === 0) {
        this.promptBlink = !this.promptBlink;
      }
    }

    this.tui.requestRender();
  }

  render(width: number): string[] {
    if (this.cachedLines && this.cachedWidth === width) return this.cachedLines;

    const state = getSharedState();
    const lines: string[] = [];

    // Centre the logo horizontally
    const logoW = this.logoWidth;
    const pad = Math.max(0, Math.floor((width - logoW) / 2));
    const padStr = " ".repeat(pad);

    // Render logo frame
    const logoFrame = this.transitioned
      ? renderFrame(TOTAL_FRAMES + 1, this.lines, this.frameMap, this.noiseSeed, this.markRows)
      : renderFrame(Math.min(this.frame, TOTAL_FRAMES), this.lines, this.frameMap, this.noiseSeed, this.markRows);

    lines.push(""); // top spacer
    for (const row of logoFrame) {
      lines.push(truncateToWidth(padStr + row, width));
    }

    // Checklist
    if (!this.transitioned) {
      lines.push(""); // spacer
      const checklistLines = this.renderChecklist(width);
      lines.push(...checklistLines);

      // "Press any key" prompt when ready
      if (this.readyToDismiss) {
        lines.push(""); // spacer
        const prompt = "press any key to continue";
        const promptPad = Math.max(0, Math.floor((width - prompt.length) / 2));
        const promptColor = this.promptBlink ? DIM : PRIMARY;
        lines.push(" ".repeat(promptPad) + `${promptColor}${prompt}${RESET}`);
      }
    }

    lines.push(""); // bottom spacer

    this.cachedLines = lines;
    this.cachedWidth = width;
    return lines;
  }

  private renderChecklist(width: number): string[] {
    const state = getSharedState();
    const lines: string[] = [];

    // Render all items on one line for compactness
    // Compute actual visible width: "✓ label  " per item (indicator 2 + label + 2 gap)
    const visibleItems = state.items.filter(i => i.state !== "hidden");
    const contentWidth = visibleItems.reduce((w, i) => w + 2 + i.label.length + 2, 0);
    let line = "";
    const pad = Math.max(0, Math.floor((width - contentWidth) / 2));
    line += " ".repeat(pad);

    for (const item of visibleItems) {
      let indicator: string;
      let labelColor: string;

      switch (item.state) {
        case "pending":
          indicator = `${DIM}${PENDING_GLYPH}${RESET}`;
          labelColor = DIM;
          break;
        case "active":
          indicator = `${PRIMARY}${SCAN_FRAMES[this.scanFrame]}${RESET}`;
          labelColor = PRIMARY;
          break;
        case "done":
          indicator = `${SUCCESS}${DONE_GLYPH}${RESET}`;
          labelColor = SUCCESS;
          break;
        case "failed":
          indicator = `${ERROR_CLR}${FAIL_GLYPH}${RESET}`;
          labelColor = ERROR_CLR;
          break;
        default:
          indicator = `${DIM}${PENDING_GLYPH}${RESET}`;
          labelColor = DIM;
          break;
      }

      line += `${indicator}${labelColor}${item.label}${RESET}  `;
    }

    lines.push(truncateToWidth(line, width));
    return lines;
  }

  invalidate(): void {
    this.cachedLines = undefined;
    this.cachedWidth = undefined;
  }

  dispose(): void {
    if (this.timer) {
      clearInterval(this.timer);
      this.timer = null;
    }
  }
}

// ---------------------------------------------------------------------------
// Minimal branded header (post-splash)
// ---------------------------------------------------------------------------
class BrandedHeader implements Component {
  private version: string;
  private cachedLines: string[] | undefined;
  private cachedWidth: number | undefined;

  constructor(version: string) {
    this.version = version;
  }

  render(width: number): string[] {
    if (this.cachedLines && this.cachedWidth === width) return this.cachedLines;

    const lines: string[] = [];
    const logo = `${BOLD}${PRIMARY}omegon${RESET} ${DIM}v${this.version}${RESET}`;
    const help = `${DIM}/ commands  ${PRIMARY}esc${RESET}${DIM} interrupt  ${PRIMARY}ctrl+c${RESET}${DIM} clear/exit${RESET}`;
    lines.push("");
    lines.push(truncateToWidth(` ${logo}   ${help}`, width));
    lines.push("");

    this.cachedLines = lines;
    this.cachedWidth = width;
    return lines;
  }

  invalidate(): void {
    this.cachedLines = undefined;
    this.cachedWidth = undefined;
  }
}

// ---------------------------------------------------------------------------
// Extension entry point
// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// Fullscreen splash replay component (easter egg)
// ---------------------------------------------------------------------------
class SplashReplay implements Component {
  private tui: TUI;
  private lines: string[];
  private frame = 0;
  private frameMap: ReturnType<typeof assignUnlockFrames>;
  private noiseSeed = (Date.now() * 7) & 0x7fffffff;
  private timer: ReturnType<typeof setInterval> | null = null;
  private holdCount = 0;
  private done: () => void;
  private markRows: number;
  private logoWidth: number;
  private cachedLines: string[] | undefined;
  private cachedWidth: number | undefined;

  constructor(tui: TUI, done: () => void, lines: string[], markRows: number, logoWidth: number) {
    this.tui = tui;
    this.done = done;
    this.lines = lines;
    this.markRows = markRows;
    this.logoWidth = logoWidth;
    this.frameMap = assignUnlockFrames(lines, TOTAL_FRAMES, Date.now() & 0xffff);
  }

  start(): void {
    this.timer = setInterval(() => this.tick(), FRAME_INTERVAL_MS);
  }

  private tick(): void {
    this.frame++;
    this.cachedLines = undefined;

    if (this.frame >= TOTAL_FRAMES) {
      this.holdCount++;
      if (this.holdCount >= HOLD_FRAMES + 12) {
        this.dispose();
        this.done();
        return;
      }
    }

    this.tui.requestRender();
  }

  render(width: number): string[] {
    if (this.cachedLines && this.cachedWidth === width) return this.cachedLines;

    const height = process.stdout.rows ?? 24;
    const lines: string[] = [];

    const logoFrame = renderFrame(
      Math.min(this.frame, TOTAL_FRAMES),
      this.lines,
      this.frameMap,
      this.noiseSeed,
      this.markRows,
    );

    // Vertically centre
    const topPad = Math.max(0, Math.floor((height - logoFrame.length) / 2));
    for (let i = 0; i < topPad; i++) lines.push("");

    // Horizontally centre
    const pad = Math.max(0, Math.floor((width - this.logoWidth) / 2));
    const padStr = " ".repeat(pad);
    for (const row of logoFrame) {
      lines.push(truncateToWidth(padStr + row, width));
    }

    // Fill remaining
    const remaining = height - lines.length;
    for (let i = 0; i < remaining; i++) lines.push("");

    this.cachedLines = lines;
    this.cachedWidth = width;
    return lines;
  }

  handleInput(input: string): boolean {
    // Any key dismisses early
    if (input) {
      this.dispose();
      this.done();
      return true;
    }
    return false;
  }

  invalidate(): void {
    this.cachedLines = undefined;
    this.cachedWidth = undefined;
  }

  dispose(): void {
    if (this.timer) {
      clearInterval(this.timer);
      this.timer = null;
    }
  }
}

// ---------------------------------------------------------------------------
// Extension entry point
// ---------------------------------------------------------------------------
export default function splashExtension(pi: ExtensionAPI): void {
  // Initialise shared state immediately so other extensions can write to it
  getSharedState();

  // Easter egg: /splash replays the animation fullscreen
  pi.registerCommand("splash", {
    description: "Replay the Omegon splash animation",
    handler: async (_args, ctx) => {
      if (!ctx.hasUI) return;
      const termWidth = process.stdout.columns ?? 80;
      const termRows = process.stdout.rows ?? 24;

      // Pick the best art that fits
      let artLines: string[];
      let markRows: number;
      let logoWidth: number;
      const canFitFull = termWidth >= LINE_WIDTH + 4 && termRows >= LOGO_LINES.length + 4;
      const canFitCompact = termWidth >= COMPACT_LINE_WIDTH + 4 && termRows >= COMPACT_LOGO_LINES.length + 4;
      if (canFitFull) {
        artLines = LOGO_LINES;
        markRows = 31;
        logoWidth = LINE_WIDTH;
      } else if (canFitCompact) {
        artLines = COMPACT_LOGO_LINES;
        markRows = COMPACT_MARK_ROWS;
        logoWidth = COMPACT_LINE_WIDTH;
      } else {
        artLines = WORDMARK_LINES;
        markRows = 0;
        logoWidth = LINE_WIDTH;
      }

      await ctx.ui.custom<void>((tui, _theme, _kb, done) => {
        const replay = new SplashReplay(tui, () => done(undefined), artLines, markRows, logoWidth);
        replay.start();
        return replay;
      });
    },
  });

  let version = "0.0.0";

  pi.on("session_start", async (_event, ctx) => {
    // Read version
    try {
      const fs = await import("node:fs");
      const path = await import("node:path");
      const url = await import("node:url");
      const thisDir = import.meta.dirname ?? path.dirname(url.fileURLToPath(import.meta.url));
      const pkgPath = path.join(thisDir, "..", "..", "package.json");
      const pkg = JSON.parse(fs.readFileSync(pkgPath, "utf8"));
      version = pkg.version || version;
    } catch { /* best effort */ }
    if (!ctx.hasUI) return;

    // Set the animated splash header (skip animation on narrow terminals).
    // NOTE: setHeader() only replaces the builtInHeader slot in pi's
    // headerContainer. If a changelog notification was added (only happens
    // after a version update), it renders as a one-liner below the splash.
    // This is acceptable — it only appears once per update.
    const termWidth = process.stdout.columns ?? 80;
    const termRows = process.stdout.rows ?? 24;

    // Four tiers based on terminal size:
    //   Full (sigil + wordmark): needs ~46 rows and LINE_WIDTH+4 cols (~84 cols)
    //   Compact (smaller sigil + wordmark): needs ~34 rows and COMPACT_LINE_WIDTH+4 cols (~58 cols)
    //   Wordmark only: needs ~14 rows and LINE_WIDTH+4 cols
    //   Minimal (no animation): everything else
    const canFitFull = termWidth >= LINE_WIDTH + 4 && termRows >= LOGO_LINES.length + 6;
    const canFitCompact = termWidth >= COMPACT_LINE_WIDTH + 4 && termRows >= COMPACT_LOGO_LINES.length + 6;
    const canFitWordmark = termWidth >= LINE_WIDTH + 4 && termRows >= WORDMARK_LINES.length + 6;

    // -----------------------------------------------------------------------
    // Notification queueing — intercept ctx.ui.notify while splash is active
    // -----------------------------------------------------------------------
    const state = getSharedState();
    const originalNotify = ctx.ui.notify.bind(ctx.ui);
    ctx.ui.notify = (message: string, type?: "info" | "warning" | "error") => {
      if (state.active) {
        state.notificationQueue.push({ message, type: type ?? "info" });
      } else {
        originalNotify(message, type);
      }
    };

    /** Flush queued notifications and restore original notify. */
    const flushAndRestore = () => {
      state.active = false;
      ctx.ui.notify = originalNotify;
      for (const n of state.notificationQueue) {
        originalNotify(n.message, n.type);
      }
      state.notificationQueue.length = 0;
    };

    // -----------------------------------------------------------------------
    // Splash header setup
    // -----------------------------------------------------------------------
    let activeSplash: SplashHeader | null = null;

    const dismissSplash = () => {
      if (!activeSplash) return;
      activeSplash.dismiss();
      activeSplash = null;
      if (unsubInput) { unsubInput(); unsubInput = null; }
      flushAndRestore();
    };

    let unsubInput: (() => void) | null = null;

    if (!canFitCompact && !canFitWordmark) {
      // Too small for any animation — minimal branded header, no splash gate
      state.active = false;
      ctx.ui.notify = originalNotify;
      ctx.ui.setHeader(() => new BrandedHeader(version));
    } else {
      let artLines: string[];
      let markRows: number;
      let logoWidth: number;
      if (canFitFull) {
        artLines = LOGO_LINES;
        markRows = 31; // MARK_ROWS
        logoWidth = LINE_WIDTH;
      } else if (canFitCompact) {
        artLines = COMPACT_LOGO_LINES;
        markRows = COMPACT_MARK_ROWS;
        logoWidth = COMPACT_LINE_WIDTH;
      } else {
        artLines = WORDMARK_LINES;
        markRows = 0; // all wordmark
        logoWidth = LINE_WIDTH;
      }
      ctx.ui.setHeader((tui, _theme) => {
        const splash = new SplashHeader(tui, () => {
          // Transition to minimal branded header
          ctx.ui.setHeader((_, _t) => new BrandedHeader(version));
          flushAndRestore();
        }, artLines, markRows, logoWidth);
        splash.start();
        activeSplash = splash;
        return splash;
      });

      // Listen for any keypress to dismiss splash once ready
      unsubInput = ctx.ui.onTerminalInput((data) => {
        if (activeSplash?.readyToDismiss && data) {
          dismissSplash();
          return { consume: true };
        }
        return undefined;
      });
    }

    // Poll shared state to detect when all subsystems have reported.
    // session_start hooks fire sequentially, so other extensions update
    // their items as they complete. We poll at animation frame rate.
    const pollTimer = setInterval(() => {
      const s = getSharedState();
      const allDone = s.items.every(i => i.state === "done" || i.state === "failed");
      if (allDone) {
        clearInterval(pollTimer);
        clearTimeout(safetyTimer);
        splashDone();
      }
    }, FRAME_INTERVAL_MS);

    // Safety timeout — if an extension never reports done, force the checklist
    // to complete so the "press any key" prompt appears. Does NOT auto-dismiss;
    // the user must press a key.
    const safetyTimer = setTimeout(() => {
      clearInterval(pollTimer);
      splashDone();
    }, 5000);

    // Clean up on early session exit to prevent timer leaks
    pi.on("session_shutdown", async () => {
      clearInterval(pollTimer);
      clearTimeout(safetyTimer);
      if (activeSplash) dismissSplash();
    });
  });
}
