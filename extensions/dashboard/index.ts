/**
 * dashboard — Unified live dashboard for Design Tree + OpenSpec + Cleave
 *
 * Renders a custom footer via setFooter() that supports modes:
 *   compact:  Dashboard summary + context gauge + original footer data
 *   raised:   Section details for design tree, openspec, cleave + footer data
 *   panel:    Non-capturing overlay (visible but doesn't steal input)
 *   focused:  Interactive overlay with keyboard navigation
 *
 * Toggle: ctrl+` or /dashboard command.
 * Cycle: compact → raised → panel → focused → compact
 *
 * Reads sharedState written by producer extensions (design-tree, openspec, cleave).
 * Subscribes to "dashboard:update" events for live re-rendering.
 */

import type { ExtensionAPI, ExtensionContext } from "@mariozechner/pi-coding-agent";
import type { GuardrailResult } from "../cleave/guardrails.ts";
import { DASHBOARD_UPDATE_EVENT } from "../shared-state.ts";
import { DashboardFooter } from "./footer.ts";
import { DashboardOverlay, showDashboardOverlay } from "./overlay.ts";
import type { DashboardState, DashboardMode } from "./types.ts";
import { debug } from "../debug.ts";

/** Mode cycle order for ctrl+` toggling */
const MODE_CYCLE: DashboardMode[] = ["compact", "raised", "panel", "focused"];

export default function (pi: ExtensionAPI) {
  const state: DashboardState = {
    mode: "compact",
    turns: 0,
  };

  let footer: DashboardFooter | null = null;
  let tui: any = null; // TUI reference for requestRender
  let unsubscribeEvents: (() => void) | null = null;

  // ── Non-capturing overlay state ─────────────────────────────
  /** Overlay handle from ctx.ui.custom() for non-capturing panel */
  let overlayHandle: { hide(): void; setHidden(h: boolean): void; isHidden(): boolean; focus(): void; unfocus(): void; isFocused(): boolean } | null = null;
  /** The done() callback to resolve the custom() promise when permanently closing */
  let overlayDone: ((result: void) => void) | null = null;
  /** Track whether we've created the non-capturing overlay this session */
  let overlayCreated = false;

  /**
   * Restore persisted dashboard mode from session entries.
   * Panel/focused modes restore to raised (overlay is session-transient).
   */
  function restoreMode(ctx: ExtensionContext): void {
    try {
      const entries = ctx.sessionManager.getEntries();
      for (let i = entries.length - 1; i >= 0; i--) {
        const entry = entries[i] as any;
        if (entry.type === "dashboard-state" && entry.data?.mode) {
          const saved = entry.data.mode as DashboardMode;
          // Overlay modes don't persist — fall back to raised
          state.mode = (saved === "panel" || saved === "focused") ? "raised" : saved;
          return;
        }
      }
    } catch { /* first session, no entries yet */ }
  }

  /**
   * Persist the current mode to the session.
   */
  function persistMode(_ctx: ExtensionContext): void {
    try {
      // Persist the base mode (panel/focused stored as raised)
      const persistable = (state.mode === "panel" || state.mode === "focused") ? "raised" : state.mode;
      pi.appendEntry("dashboard-state", { mode: persistable });
    } catch { /* session may not support it */ }
  }

  /**
   * Update footer context and trigger re-render.
   */
  function refresh(ctx: ExtensionContext): void {
    debug("dashboard", "refresh", {
      hasFooter: !!footer,
      hasTui: !!tui,
      footerType: footer?.constructor?.name,
    });
    if (footer) {
      footer.setContext(ctx);
    }
    tui?.requestRender();
  }

  /**
   * Show the non-capturing overlay panel.
   * Creates it on first call, then toggles visibility.
   */
  function showPanel(ctx: ExtensionContext): void {
    if (overlayHandle && !overlayHandle.isHidden()) {
      // Already visible — nothing to do
      return;
    }

    if (overlayHandle) {
      // Was hidden — show it
      overlayHandle.setHidden(false);
      tui?.requestRender();
      return;
    }

    if (overlayCreated) {
      // Was permanently closed — don't recreate in same session
      return;
    }

    // Create the non-capturing overlay (fire-and-forget — don't await)
    overlayCreated = true;
    void ctx.ui.custom<void>(
      (tuiRef, theme, _kb, done) => {
        overlayDone = done;
        const overlay = new DashboardOverlay(tuiRef, theme, () => {
          // Esc from focused mode → unfocus, stay visible
          if (overlayHandle?.isFocused()) {
            overlayHandle.unfocus();
            state.mode = "panel";
            tui?.requestRender();
          } else {
            // Esc from panel → hide
            hidePanel();
          }
        });
        overlay.setEventBus(pi.events);
        return overlay;
      },
      {
        overlay: true,
        overlayOptions: {
          anchor: "right-center" as any,
          width: "40%",
          minWidth: 40,
          maxHeight: "80%",
          margin: { top: 1, right: 1, bottom: 1 },
          visible: (termWidth: number) => termWidth >= 80,
          nonCapturing: true,
        },
        onHandle: (handle) => {
          overlayHandle = handle;
        },
      },
    );
  }

  /**
   * Hide the non-capturing overlay without destroying it.
   */
  function hidePanel(): void {
    if (overlayHandle) {
      if (overlayHandle.isFocused()) {
        overlayHandle.unfocus();
      }
      overlayHandle.setHidden(true);
    }
    state.mode = "compact";
    tui?.requestRender();
  }

  /**
   * Focus the non-capturing overlay for interactive keyboard navigation.
   */
  function focusPanel(): void {
    if (overlayHandle && !overlayHandle.isHidden()) {
      overlayHandle.focus();
    }
  }

  /**
   * Cycle through dashboard modes: compact → raised → panel → focused → compact
   */
  function cycleTo(ctx: ExtensionContext, targetMode: DashboardMode): void {
    state.mode = targetMode;

    switch (targetMode) {
      case "compact":
        hidePanel();
        break;
      case "raised":
        hidePanel();
        break;
      case "panel":
        showPanel(ctx);
        break;
      case "focused":
        showPanel(ctx);
        // Small delay to ensure overlay is created before focusing
        setTimeout(() => focusPanel(), 50);
        break;
    }

    persistMode(ctx);
    tui?.requestRender();
  }

  /**
   * Advance to the next mode in the cycle.
   */
  function cycleNext(ctx: ExtensionContext): void {
    const currentIdx = MODE_CYCLE.indexOf(state.mode);
    const nextIdx = (currentIdx + 1) % MODE_CYCLE.length;
    cycleTo(ctx, MODE_CYCLE[nextIdx]!);
  }

  // ── Session start: set up the custom footer ──────────────────

  pi.on("session_start", async (_event, ctx) => {
    debug("dashboard", "session_start:enter", {
      hasUI: ctx.hasUI,
      cwd: ctx.cwd,
      hasSetFooter: typeof ctx.ui?.setFooter === "function",
    });
    if (!ctx.hasUI) {
      debug("dashboard", "session_start:bail", { reason: "no UI" });
      return;
    }

    state.turns = 0;
    overlayHandle = null;
    overlayDone = null;
    overlayCreated = false;
    restoreMode(ctx);
    debug("dashboard", "session_start:mode", { mode: state.mode });

    // Set the custom footer
    try {
      ctx.ui.setFooter((tuiRef, theme, footerData) => {
        debug("dashboard", "footer:factory:enter", {
          hasTui: !!tuiRef,
          hasTheme: !!theme,
          hasFooterData: !!footerData,
          themeFgType: typeof theme?.fg,
        });
        try {
          tui = tuiRef;
          footer = new DashboardFooter(tuiRef, theme, footerData, state);
          footer.setContext(ctx);
          debug("dashboard", "footer:factory:ok", {
            footerType: footer?.constructor?.name,
            hasRender: typeof footer?.render === "function",
          });
          return footer;
        } catch (factoryErr: any) {
          debug("dashboard", "footer:factory:ERROR", {
            error: factoryErr?.message,
            stack: factoryErr?.stack?.split("\n").slice(0, 5).join(" | "),
          });
          throw factoryErr;
        }
      });
      debug("dashboard", "session_start:setFooter:ok");
    } catch (err: any) {
      debug("dashboard", "session_start:setFooter:ERROR", {
        error: err?.message,
        stack: err?.stack?.split("\n").slice(0, 5).join(" | "),
      });
    }

    // Subscribe to dashboard:update events from producer extensions.
    unsubscribeEvents = pi.events.on(DASHBOARD_UPDATE_EVENT, (_data) => {
      debug("dashboard", "update-event", _data as Record<string, unknown>);
      tui?.requestRender();
    });

    // Deferred initial render
    queueMicrotask(() => {
      debug("dashboard", "microtask:render", {
        tuiSet: !!tui,
        footerSet: !!footer,
        footerType: footer?.constructor?.name,
      });
      tui?.requestRender();
    });

    // Non-blocking guardrail health check
    setTimeout(async () => {
      try {
        const { discoverGuardrails, runGuardrails } = await import("../cleave/guardrails.ts");
        const checks = discoverGuardrails(ctx.cwd);
        if (checks.length === 0) return;
        const suite = runGuardrails(ctx.cwd, checks);
        if (!suite.allPassed) {
          const failures = suite.results.filter((r: GuardrailResult) => !r.passed);
          const msg = failures
            .map((f: GuardrailResult) =>
              `${f.check.name}: ${f.exitCode !== 0 ? f.output.split("\n").length + " errors" : "failed"}`,
            )
            .join(", ");
          ctx.ui.notify(`⚠ Guardrail check failed: ${msg}`, "warning");
        }
      } catch {
        /* non-fatal */
      }
    }, 2000);
  });

  // ── Session shutdown: cleanup ─────────────────────────────────

  pi.on("session_shutdown", async () => {
    if (unsubscribeEvents) {
      unsubscribeEvents();
      unsubscribeEvents = null;
    }
    // Permanently close the non-capturing overlay
    if (overlayHandle) {
      overlayHandle.hide();
      overlayHandle = null;
    }
    if (overlayDone) {
      overlayDone();
      overlayDone = null;
    }
    overlayCreated = false;
    footer = null;
    tui = null;
  });

  // ── Events that trigger re-render ─────────────────────────────

  pi.on("turn_end", async (_event, ctx) => {
    state.turns++;
    refresh(ctx);
  });

  pi.on("message_end", async (_event, ctx) => {
    refresh(ctx);
  });

  pi.on("tool_execution_end", async (_event, ctx) => {
    refresh(ctx);
  });

  // ── Keyboard shortcut: ctrl+` ────────────────────────────────
  // Cycles through: compact → raised → panel → focused → compact

  pi.registerShortcut("ctrl+`", {
    description: "Cycle dashboard mode (compact → raised → panel → focused)",
    handler: (ctx) => {
      cycleNext(ctx);
    },
  });

  // ── Slash command: /dashboard [open|compact|raised|panel|focus] ─

  pi.registerCommand("dashboard", {
    description: "Toggle dashboard mode. Subcommands: compact, raised, panel, focus, open (legacy modal)",
    handler: async (args, ctx) => {
      const arg = (args ?? "").trim().toLowerCase();

      if (arg === "open") {
        // Legacy modal overlay (capturing, blocks until Esc)
        state.mode = "raised";
        persistMode(ctx);
        tui?.requestRender();
        await showDashboardOverlay(ctx, pi);
        return;
      }

      if (arg === "compact") {
        cycleTo(ctx, "compact");
        ctx.ui.notify("Dashboard: compact", "info");
        return;
      }

      if (arg === "raised") {
        cycleTo(ctx, "raised");
        ctx.ui.notify("Dashboard: raised", "info");
        return;
      }

      if (arg === "panel") {
        cycleTo(ctx, "panel");
        ctx.ui.notify("Dashboard: panel (non-capturing)", "info");
        return;
      }

      if (arg === "focus") {
        cycleTo(ctx, "focused");
        ctx.ui.notify("Dashboard: focused (interactive)", "info");
        return;
      }

      // Default: cycle to next mode
      cycleNext(ctx);
      ctx.ui.notify(`Dashboard: ${state.mode}`, "info");
    },
  });
}
