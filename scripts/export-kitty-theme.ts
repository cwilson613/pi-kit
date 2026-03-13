#!/usr/bin/env node
/**
 * Generate themes/alpharius.conf (Kitty terminal theme) from themes/alpharius.json.
 *
 * Run after any palette change:
 *   npx tsx scripts/export-kitty-theme.ts
 *
 * The generated file can be included in kitty.conf:
 *   cp themes/alpharius.conf ~/.config/kitty/themes/alpharius.conf
 *   # then in kitty.conf: include themes/alpharius.conf
 */

import { readFileSync, writeFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, "..");

const theme = JSON.parse(
  readFileSync(resolve(root, "themes/alpharius.json"), "utf8"),
) as { vars: Record<string, string>; colors: Record<string, string> };

const v = theme.vars;

// ── ANSI colour slot mapping ─────────────────────────────────────────────────
// Maps alpharius semantic tokens to the 16 ANSI palette slots (color0–color15).
// Slots 0–7 are "normal", 8–15 are "bright" variants.
// Goal: oh-my-bash, ls, git diff, and system tools read alpharius vocabulary.
//
//  0 black       → void bg          8  bright-black  → dim text
//  1 red         → error            9  bright-red     → lighter error
//  2 green       → success         10  bright-green   → lighter success
//  3 yellow      → warning/numbers 11  bright-yellow  → lighter warning
//  4 blue        → primaryMuted    12  bright-blue    → primary accent
//  5 magenta     → ai/purple       13  bright-magenta → lighter purple
//  6 cyan        → syntaxType      14  bright-cyan    → primaryBright
//  7 white       → mutedFg         15  bright-white   → full fg

const ansiColors: Record<number, string> = {
  0:  v.bg!,            // black  — void background
  1:  v.red!,           // red    — error
  2:  v.green!,         // green  — success
  3:  v.yellow!,        // yellow — warning / numbers
  4:  v.primaryMuted!,  // blue   — secondary accent / links
  5:  "#6060c0",        // magenta — ai/purple (no direct var)
  6:  "#4aa8c0",        // cyan   — syntaxType (no direct var)
  7:  v.mutedFg!,       // white  — muted foreground

  8:  v.dimFg!,         // bright-black — dim text
  9:  "#e04040",        // bright-red   — lighter error
  10: "#20d890",        // bright-green — lighter success
  11: "#d4aa30",        // bright-yellow — lighter warning
  12: v.primary!,       // bright-blue  — primary accent (ceramite teal)
  13: "#8080d0",        // bright-magenta — lighter purple
  14: v.primaryBright!, // bright-cyan  — silver-teal shimmer
  15: v.fg!,            // bright-white — full foreground
};

// ── Build conf ───────────────────────────────────────────────────────────────

const lines: string[] = [
  `# Alpharius — Alpha Legion terminal theme for Kitty`,
  `# Generated from themes/alpharius.json by scripts/export-kitty-theme.ts`,
  `# Do not edit manually — re-run the script after palette changes.`,
  `#`,
  `# Usage:`,
  `#   cp themes/alpharius.conf ~/.config/kitty/themes/alpharius.conf`,
  `#   then in kitty.conf: include themes/alpharius.conf`,
  ``,
  `# ── Core palette ─────────────────────────────────────────────────────────`,
  `background            ${v.bg}`,
  `foreground            ${v.fg}`,
  ``,
  `# ── Cursor ───────────────────────────────────────────────────────────────`,
  `cursor                ${v.primary}`,
  `cursor_text_color     ${v.bg}`,
  ``,
  `# ── Selection ────────────────────────────────────────────────────────────`,
  `selection_background  #102030`,
  `selection_foreground  ${v.fg}`,
  ``,
  `# ── URL underline colour ─────────────────────────────────────────────────`,
  `url_color             ${v.primaryMuted}`,
  ``,
  `# ── ANSI 16-colour palette ───────────────────────────────────────────────`,
  `# Mapped to alpharius semantic roles so shell prompts and system tools`,
  `# (oh-my-bash, ls, git diff) align with the alpharius colour vocabulary.`,
  ``,
];

for (const [slot, hex] of Object.entries(ansiColors)) {
  lines.push(`color${slot.padEnd(2)}  ${hex}`);
}

lines.push(``);
lines.push(`# ── Tab bar ──────────────────────────────────────────────────────────────`);
lines.push(`active_tab_foreground   ${v.fg}`);
lines.push(`active_tab_background   ${v.cardBg}`);
lines.push(`inactive_tab_foreground ${v.mutedFg}`);
lines.push(`inactive_tab_background ${v.bg}`);
lines.push(`tab_bar_background      ${v.bg}`);
lines.push(``);
lines.push(`# ── Window border ────────────────────────────────────────────────────────`);
lines.push(`active_border_color     ${v.primary}`);
lines.push(`inactive_border_color   ${v.borderColor}`);
lines.push(`bell_border_color        ${v.orange}`);
lines.push(``);

const output = lines.join("\n") + "\n";
const outPath = resolve(root, "themes/alpharius.conf");
writeFileSync(outPath, output, "utf8");
console.log(`Written: themes/alpharius.conf`);
