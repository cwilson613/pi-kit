/**
 * tool-profile — Smart tool activation based on project context.
 *
 * On session_start, scans the cwd for project signals (package.json, .git,
 * Cargo.toml, etc.) and activates only the relevant tool profiles. Saves
 * ~12K tokens of context window by disabling irrelevant tools.
 *
 * Commands:
 *   /profile         — Show active profiles and tool counts
 *   /profile <name>  — Toggle a profile on/off
 *   /profile reset   — Re-detect from project signals
 *
 * Tool (LLM-callable):
 *   manage_tools     — List, enable, disable tools or switch profiles
 */

import type { ExtensionAPI } from "@styrene-lab/pi-coding-agent";
import { Type } from "@sinclair/typebox";
import { StringEnum } from "../lib/typebox-helpers.ts";
import {
  PROFILES,
  detectProfiles,
  loadProfileConfig,
  resolveActiveTools,
  formatProfileSummary,
  type ProfileConfig,
} from "./profiles.ts";

export default function (pi: ExtensionAPI) {
  let currentDetected: string[] = [];
  let currentConfig: ProfileConfig = {};
  let allToolNames: string[] = [];

  function applyProfile(ctx: { ui: { notify: (msg: string, type?: "info" | "warning" | "error") => void } } | null): void {
    const activeTools = resolveActiveTools(allToolNames, currentDetected, currentConfig);
    pi.setActiveTools(activeTools);
    if (ctx) {
      const inactive = allToolNames.length - activeTools.length;
      if (inactive > 0) {
        ctx.ui.notify(`Profile: ${activeTools.length} tools active (${inactive} disabled)`, "info");
      }
    }
  }

  // ── Session Start: Auto-detect ──────────────────────────────

  pi.on("session_start", async (_event, ctx) => {
    allToolNames = pi.getAllTools().map((t) => t.name);
    currentDetected = detectProfiles(ctx.cwd);
    currentConfig = loadProfileConfig(ctx.cwd);

    // Only apply if we'd actually disable something.
    // If pi-dev profile detected, everything stays on anyway.
    if (currentDetected.includes("pi-dev")) return;

    applyProfile(ctx);
  });

  // ── /profile Command ────────────────────────────────────────

  pi.registerCommand("profile", {
    description: "Show or toggle tool profiles (/profile [name|reset])",
    handler: async (args, ctx) => {
      const arg = args.trim().toLowerCase();

      if (arg === "reset") {
        allToolNames = pi.getAllTools().map((t) => t.name);
        // Re-read getAllTools before detection since we may have reset
        // Actually, getAllTools returns ALL registered (not just active), so this is fine
        currentDetected = detectProfiles(ctx.cwd);
        currentConfig = loadProfileConfig(ctx.cwd);
        applyProfile(ctx);
        ctx.ui.notify("Profiles re-detected from project context", "info");
        return;
      }

      if (arg && arg !== "status") {
        // Toggle a specific profile
        const profile = PROFILES.find((p) => p.id === arg);
        if (!profile) {
          ctx.ui.notify(`Unknown profile: ${arg}. Available: ${PROFILES.map((p) => p.id).join(", ")}`, "warning");
          return;
        }

        const isActive = currentDetected.includes(arg) ||
          currentConfig.include?.includes(arg);
        const isExcluded = currentConfig.exclude?.includes(arg);

        if (isActive && !isExcluded) {
          // Disable it
          if (!currentConfig.exclude) currentConfig.exclude = [];
          currentConfig.exclude.push(arg);
          // Remove from include if manually added
          if (currentConfig.include) {
            currentConfig.include = currentConfig.include.filter((id) => id !== arg);
          }
          applyProfile(ctx);
          ctx.ui.notify(`Profile '${profile.label}' disabled`, "info");
        } else {
          // Enable it
          if (!currentConfig.include) currentConfig.include = [];
          currentConfig.include.push(arg);
          // Remove from exclude
          if (currentConfig.exclude) {
            currentConfig.exclude = currentConfig.exclude.filter((id) => id !== arg);
          }
          applyProfile(ctx);
          ctx.ui.notify(`Profile '${profile.label}' enabled`, "info");
        }
        return;
      }

      // Show status
      // Refresh allToolNames to show full catalog
      const fullToolNames = pi.getAllTools().map((t) => t.name);
      const summary = formatProfileSummary(currentDetected, currentConfig, fullToolNames);
      ctx.ui.notify(summary, "info");
    },
    getArgumentCompletions: (prefix) => {
      const options = [...PROFILES.map((p) => p.id), "reset", "status"];
      return options
        .filter((o) => o.startsWith(prefix))
        .map((o) => ({ value: o, label: o, description: PROFILES.find((p) => p.id === o)?.description ?? o }));
    },
  });

  // ── manage_tools Tool (LLM-callable) ────────────────────────

  pi.registerTool({
    name: "manage_tools",
    label: "Manage Tools",
    description: "List, enable, or disable tools and tool profiles. Use to activate tools the user requests or disable irrelevant ones to save context window space.",
    promptSnippet: "manage_tools: list/enable/disable tools and profiles",
    promptGuidelines: [
      "Use manage_tools to enable tools when the user asks for a capability that's currently disabled",
      "Use manage_tools with action 'list' to see what's available before trying to use a tool that might be disabled",
    ],
    parameters: Type.Object({
      action: StringEnum(["list", "enable", "disable", "profiles", "apply_profile"], {
        description: "Action: list (show tools), enable/disable (toggle tools), profiles (show profiles), apply_profile (switch profile)",
      }),
      tools: Type.Optional(Type.Array(Type.String(), {
        description: "Tool names to enable/disable (for enable/disable actions)",
      })),
      profile: Type.Optional(Type.String({
        description: "Profile id to apply (for apply_profile action)",
      })),
    }),
    execute: async (_toolCallId, params, _signal, _onUpdate, ctx) => {
      // Refresh tool catalog
      const allTools = pi.getAllTools();
      const activeToolSet = new Set(pi.getActiveTools());

      switch (params.action) {
        case "list": {
          const lines: string[] = [];
          const active = allTools.filter((t) => activeToolSet.has(t.name));
          const inactive = allTools.filter((t) => !activeToolSet.has(t.name));

          lines.push(`Active tools (${active.length}):`);
          for (const t of active) {
            lines.push(`  ✓ ${t.name}`);
          }
          if (inactive.length > 0) {
            lines.push(`\nInactive tools (${inactive.length}):`);
            for (const t of inactive) {
              lines.push(`  ○ ${t.name} — ${t.description?.slice(0, 80) ?? ""}`);
            }
          }
          return { content: [{ type: "text", text: lines.join("\n") }], details: undefined };
        }

        case "enable": {
          if (!params.tools?.length) {
            return { content: [{ type: "text", text: "Error: provide tool names to enable" }], details: undefined } as any;
          }
          const currentActive = new Set(pi.getActiveTools());
          const allNames = new Set(allTools.map((t) => t.name));
          const added: string[] = [];
          const notFound: string[] = [];

          for (const name of params.tools) {
            if (!allNames.has(name)) {
              notFound.push(name);
            } else if (!currentActive.has(name)) {
              currentActive.add(name);
              added.push(name);
            }
          }

          pi.setActiveTools([...currentActive]);

          // Also track in config overrides so re-detect doesn't undo it
          if (!currentConfig.tools) currentConfig.tools = {};
          if (!currentConfig.tools.enable) currentConfig.tools.enable = [];
          for (const name of added) {
            if (!currentConfig.tools.enable.includes(name)) {
              currentConfig.tools.enable.push(name);
            }
            // Remove from disable if present
            if (currentConfig.tools.disable) {
              currentConfig.tools.disable = currentConfig.tools.disable.filter((n) => n !== name);
            }
          }

          const parts: string[] = [];
          if (added.length) parts.push(`Enabled: ${added.join(", ")}`);
          if (notFound.length) parts.push(`Not found: ${notFound.join(", ")}`);
          return { content: [{ type: "text", text: parts.join(". ") || "No changes" }], details: undefined };
        }

        case "disable": {
          if (!params.tools?.length) {
            return { content: [{ type: "text", text: "Error: provide tool names to disable" }], details: undefined } as any;
          }
          const currentActive = new Set(pi.getActiveTools());
          const removed: string[] = [];

          for (const name of params.tools) {
            if (currentActive.has(name)) {
              currentActive.delete(name);
              removed.push(name);
            }
          }

          pi.setActiveTools([...currentActive]);

          // Track in config
          if (!currentConfig.tools) currentConfig.tools = {};
          if (!currentConfig.tools.disable) currentConfig.tools.disable = [];
          for (const name of removed) {
            if (!currentConfig.tools.disable.includes(name)) {
              currentConfig.tools.disable.push(name);
            }
            if (currentConfig.tools.enable) {
              currentConfig.tools.enable = currentConfig.tools.enable.filter((n) => n !== name);
            }
          }

          return { content: [{ type: "text", text: removed.length ? `Disabled: ${removed.join(", ")}` : "No changes" }], details: undefined };
        }

        case "profiles": {
          const fullNames = allTools.map((t) => t.name);
          const summary = formatProfileSummary(currentDetected, currentConfig, fullNames);
          return { content: [{ type: "text", text: summary }], details: undefined };
        }

        case "apply_profile": {
          if (!params.profile) {
            return { content: [{ type: "text", text: "Error: provide a profile id" }], details: undefined } as any;
          }
          const profile = PROFILES.find((p) => p.id === params.profile);
          if (!profile) {
            const available = PROFILES.map((p) => p.id).join(", ");
            return { content: [{ type: "text", text: `Unknown profile: ${params.profile}. Available: ${available}` }], details: undefined } as any;
          }

          // Toggle the profile
          const isExcluded = currentConfig.exclude?.includes(params.profile);
          const isIncluded = currentDetected.includes(params.profile) || currentConfig.include?.includes(params.profile);

          if (isIncluded && !isExcluded) {
            // Already active — disable it
            if (!currentConfig.exclude) currentConfig.exclude = [];
            currentConfig.exclude.push(params.profile);
            if (currentConfig.include) {
              currentConfig.include = currentConfig.include.filter((id) => id !== params.profile);
            }
          } else {
            // Activate it
            if (!currentConfig.include) currentConfig.include = [];
            currentConfig.include.push(params.profile);
            if (currentConfig.exclude) {
              currentConfig.exclude = currentConfig.exclude.filter((id) => id !== params.profile);
            }
          }

          allToolNames = allTools.map((t) => t.name);
          applyProfile(null);
          const activeCount = pi.getActiveTools().length;
          const action = (isIncluded && !isExcluded) ? "disabled" : "enabled";
          return { content: [{ type: "text", text: `Profile '${profile.label}' ${action}. ${activeCount}/${allToolNames.length} tools now active.` }], details: undefined };
        }

        default:
          return { content: [{ type: "text", text: `Unknown action: ${params.action}` }], details: undefined } as any;
      }
    },
  });
}
