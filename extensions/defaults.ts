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
import * as crypto from "node:crypto";
import type { ExtensionAPI } from "@mariozechner/pi-coding-agent";

const AGENT_DIR = path.join(
  process.env.HOME || process.env.USERPROFILE || "~",
  ".pi", "agent",
);

const SETTINGS_PATH = path.join(AGENT_DIR, "settings.json");
const GLOBAL_AGENTS_PATH = path.join(AGENT_DIR, "AGENTS.md");

/** Marker embedded in the deployed AGENTS.md to identify pi-kit ownership */
const PIKIT_MARKER = "<!-- managed by pi-kit -->";

/** Hash file tracks the last content we deployed, so we detect user edits */
const HASH_PATH = path.join(AGENT_DIR, ".agents-md-hash");

/** Path to the template shipped with the pi-kit package */
const TEMPLATE_PATH = path.join(import.meta.dirname, "..", "config", "AGENTS.md");

function contentHash(content: string): string {
  return crypto.createHash("sha256").update(content).digest("hex").slice(0, 16);
}

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
          ctx.ui.notify("pi-kit: set theme to default (restart to apply)", "info");
        }
      }
    } catch {
      // Best effort
    }

    // --- Global AGENTS.md deployment ---
    try {
      if (!fs.existsSync(TEMPLATE_PATH)) return;
      fs.mkdirSync(AGENT_DIR, { recursive: true });
      const template = fs.readFileSync(TEMPLATE_PATH, "utf8");
      const deployContent = `${template.trimEnd()}\n\n${PIKIT_MARKER}\n`;

      if (fs.existsSync(GLOBAL_AGENTS_PATH)) {
        const existing = fs.readFileSync(GLOBAL_AGENTS_PATH, "utf8");

        if (existing.includes(PIKIT_MARKER)) {
          // We own this file — check if user has edited it since last deploy
          if (existing !== deployContent) {
            const lastHash = fs.existsSync(HASH_PATH) ? fs.readFileSync(HASH_PATH, "utf8").trim() : null;
            const existingHash = contentHash(existing);

            if (!lastHash) {
              // First run with hash tracking — adopt current content as baseline
              // so we don't overwrite edits made before the hash mechanism existed
              fs.writeFileSync(HASH_PATH, existingHash, "utf8");
              if (ctx.hasUI) {
                ctx.ui.notify(
                  "pi-kit: AGENTS.md template updated. Changes will apply on next session start.",
                  "info",
                );
              }
            } else if (lastHash !== existingHash) {
              // File was modified externally — warn, don't overwrite
              if (ctx.hasUI) {
                ctx.ui.notify(
                  "pi-kit: ~/.pi/agent/AGENTS.md has local edits. Remove the pi-kit marker to keep them, or delete the file to re-deploy.",
                  "warning",
                );
              }
            } else {
              // File matches our last deploy — safe to update
              fs.writeFileSync(GLOBAL_AGENTS_PATH, deployContent, "utf8");
              fs.writeFileSync(HASH_PATH, contentHash(deployContent), "utf8");
            }
          }
        }
        // else: user-authored file (no marker), don't touch it
      } else {
        // No AGENTS.md exists — deploy ours
        fs.writeFileSync(GLOBAL_AGENTS_PATH, deployContent, "utf8");
        fs.writeFileSync(HASH_PATH, contentHash(deployContent), "utf8");
        if (ctx.hasUI) {
          ctx.ui.notify("pi-kit: deployed global directives to ~/.pi/agent/AGENTS.md", "info");
        }
      }
    } catch {
      // Best effort — don't break startup
    }
  });
}
