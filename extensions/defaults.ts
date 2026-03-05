/**
 * defaults — Auto-configure pi-kit defaults on first install
 *
 * - Sets theme to "default" if no theme is configured
 * - Deploys global AGENTS.md to ~/.pi/agent/ for cross-project directives
 *
 * Guards:
 * - Only writes settings/AGENTS.md if not already present or if managed by pi-kit
 * - Never overwrites a user-authored AGENTS.md (detected by absence of marker comment)
 */

import * as fs from "node:fs";
import * as path from "node:path";
import type { ExtensionAPI } from "@mariozechner/pi-coding-agent";

const AGENT_DIR = path.join(
  process.env.HOME || process.env.USERPROFILE || "~",
  ".pi", "agent",
);

const SETTINGS_PATH = path.join(AGENT_DIR, "settings.json");
const GLOBAL_AGENTS_PATH = path.join(AGENT_DIR, "AGENTS.md");

/** Marker embedded in the deployed AGENTS.md to identify pi-kit ownership */
const PIKIT_MARKER = "<!-- managed by pi-kit -->";

/** Path to the template shipped with the pi-kit package */
const TEMPLATE_PATH = path.join(import.meta.dirname, "..", "config", "AGENTS.md");

export default function (pi: ExtensionAPI) {
  pi.on("session_start", async (_event, ctx) => {
    // --- Theme default ---
    try {
      const raw = fs.readFileSync(SETTINGS_PATH, "utf8");
      const settings = JSON.parse(raw);

      let changed = false;

      if (!settings.theme) {
        settings.theme = "default";
        changed = true;
      }

      if (changed) {
        fs.writeFileSync(SETTINGS_PATH, JSON.stringify(settings, null, 2) + "\n", "utf8");
        if (ctx.hasUI) {
          ctx.ui.notify("pi-kit: set theme to default (restart to apply)", "success");
        }
      }
    } catch {
      // Best effort
    }

    // --- Global AGENTS.md deployment ---
    try {
      if (!fs.existsSync(TEMPLATE_PATH)) return;
      const template = fs.readFileSync(TEMPLATE_PATH, "utf8");
      const deployContent = `${template.trimEnd()}\n\n${PIKIT_MARKER}\n`;

      if (fs.existsSync(GLOBAL_AGENTS_PATH)) {
        const existing = fs.readFileSync(GLOBAL_AGENTS_PATH, "utf8");

        if (existing.includes(PIKIT_MARKER)) {
          // We own this file — update if template has changed
          if (existing !== deployContent) {
            fs.writeFileSync(GLOBAL_AGENTS_PATH, deployContent, "utf8");
          }
        }
        // else: user-authored file, don't touch it
      } else {
        // No AGENTS.md exists — deploy ours
        fs.writeFileSync(GLOBAL_AGENTS_PATH, deployContent, "utf8");
        if (ctx.hasUI) {
          ctx.ui.notify("pi-kit: deployed global directives to ~/.pi/agent/AGENTS.md", "success");
        }
      }
    } catch {
      // Best effort — don't break startup
    }
  });
}
