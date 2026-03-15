/**
 * style — /style command for the Alpharius design system
 *
 * Registers `/style [subcommand]` as an interactive command.
 * Subcommands: (none), palette, d2, excalidraw, check <file>
 */

import type { ExtensionAPI } from "@styrene-lab/pi-coding-agent";

// ---------------------------------------------------------------------------
// Palette data — single source of truth
// ---------------------------------------------------------------------------

const CORE_PALETTE = {
	primary:       "#2ab4c8",
	primaryMuted:  "#1a8898",
	primaryBright: "#6ecad8",
	fg:            "#c4d8e4",
	mutedFg:       "#607888",
	dimFg:         "#344858",
	bg:            "#06080e",
	cardBg:        "#0e1622",
	surfaceBg:     "#131e2e",
	borderColor:   "#1a3448",
	borderDim:     "#0e1e30",
	syntaxType:    "#5aa8e0", // sky-blue, distinct from primary teal
};

const SIGNALS = {
	green:  "#1ab878",
	red:    "#c83030",
	orange: "#c86418",
	yellow: "#b89020",
};

const EXCALIDRAW_SEMANTICS: Record<string, { fill: string; stroke: string; use: string }> = {
	primary:   { fill: "#1a4a6e", stroke: "#2ab4c8", use: "Default components, neutral nodes" },
	secondary: { fill: "#1a3a5a", stroke: "#1a8898", use: "Supporting/related components" },
	tertiary:  { fill: "#0e2a40", stroke: "#344858", use: "Third-level, background detail" },
	start:     { fill: "#0e2e20", stroke: "#1ab878", use: "Entry points, triggers, inputs" },
	end:       { fill: "#2e2010", stroke: "#b89020", use: "Outputs, completion, results" },
	decision:  { fill: "#2a1010", stroke: "#c83030", use: "Conditionals, branches, choices" },
	ai:        { fill: "#1a1040", stroke: "#6060c0", use: "AI/LLM components, inference" },
	warning:   { fill: "#2a1808", stroke: "#c86418", use: "Warnings, degraded states" },
	error:     { fill: "#2e0e0e", stroke: "#c83030", use: "Error states, failures" },
	evidence:  { fill: "#06080e", stroke: "#1a3448", use: "Code snippets, data samples" },
	inactive:  { fill: "#0e1622", stroke: "#344858", use: "Disabled, inactive, future-state" },
};

const D2_STYLE_TEMPLATE = `# Alpharius D2 style template — paste into your .d2 file

# Primary component (ceramite blue-teal)
component: Label {
  style: {
    fill: "#1a4a6e"
    stroke: "#2ab4c8"
    font-color: "#c4d8e4"
    border-radius: 8
  }
}

# Start / entry point (hydra green)
entry: Trigger {
  style: {
    fill: "#0e2e20"
    stroke: "#1ab878"
    font-color: "#c4d8e4"
  }
}

# End / output (brass gold)
result: Output {
  style: {
    fill: "#2e2010"
    stroke: "#b89020"
    font-color: "#c4d8e4"
  }
}

# Connection
entry -> component -> result {
  style: {
    stroke: "#2ab4c8"
    font-color: "#c4d8e4"
  }
}

# Container
group: Infrastructure {
  style: {
    fill: "#06080e"
    stroke: "#1a3448"
    font-color: "#6ecad8"
  }
}`;

// Collect all known hex values for color auditing
const ALL_TOKENS: Record<string, string> = {
	...CORE_PALETTE,
	...SIGNALS,
};
for (const [name, colors] of Object.entries(EXCALIDRAW_SEMANTICS)) {
	ALL_TOKENS[`excalidraw.${name}.fill`] = colors.fill;
	ALL_TOKENS[`excalidraw.${name}.stroke`] = colors.stroke;
}

// ---------------------------------------------------------------------------
// Formatters
// ---------------------------------------------------------------------------

function quickRef(): string {
	return [
		"**Alpharius Design System — Quick Reference**",
		"",
		"```",
		"BACKGROUNDS          ACCENTS              SIGNALS",
		`bg:       ${CORE_PALETTE.bg}    primary:    ${CORE_PALETTE.primary}  green:  ${SIGNALS.green}`,
		`cardBg:   ${CORE_PALETTE.cardBg}    primaryMu:  ${CORE_PALETTE.primaryMuted}  red:    ${SIGNALS.red}`,
		`surfaceBg:${CORE_PALETTE.surfaceBg}    primaryBr:  ${CORE_PALETTE.primaryBright}  orange: ${SIGNALS.orange}`,
		`                                          yellow: ${SIGNALS.yellow}`,
		"",
		"TEXT                 BORDERS              SYNTAX",
		`fg:       ${CORE_PALETTE.fg}    border:     ${CORE_PALETTE.borderColor}  syntaxType: ${CORE_PALETTE.syntaxType}`,
		`mutedFg:  ${CORE_PALETTE.mutedFg}    borderDim:  ${CORE_PALETTE.borderDim}`,
		`dimFg:    ${CORE_PALETTE.dimFg}`,
		"",
		"EXCALIDRAW SEMANTICS (fill / stroke)",
		`primary:   ${EXCALIDRAW_SEMANTICS.primary.fill} / ${EXCALIDRAW_SEMANTICS.primary.stroke}    start:     ${EXCALIDRAW_SEMANTICS.start.fill} / ${EXCALIDRAW_SEMANTICS.start.stroke}`,
		`secondary: ${EXCALIDRAW_SEMANTICS.secondary.fill} / ${EXCALIDRAW_SEMANTICS.secondary.stroke}    end:       ${EXCALIDRAW_SEMANTICS.end.fill} / ${EXCALIDRAW_SEMANTICS.end.stroke}`,
		`decision:  ${EXCALIDRAW_SEMANTICS.decision.fill} / ${EXCALIDRAW_SEMANTICS.decision.stroke}    ai:        ${EXCALIDRAW_SEMANTICS.ai.fill} / ${EXCALIDRAW_SEMANTICS.ai.stroke}`,
		`warning:   ${EXCALIDRAW_SEMANTICS.warning.fill} / ${EXCALIDRAW_SEMANTICS.warning.stroke}    error:     ${EXCALIDRAW_SEMANTICS.error.fill} / ${EXCALIDRAW_SEMANTICS.error.stroke}`,
		`evidence:  ${EXCALIDRAW_SEMANTICS.evidence.fill} / ${EXCALIDRAW_SEMANTICS.evidence.stroke}    inactive:  ${EXCALIDRAW_SEMANTICS.inactive.fill} / ${EXCALIDRAW_SEMANTICS.inactive.stroke}`,
		"```",
		"",
		"`/style palette` — render visual swatch  ·  `/style d2` — D2 style template",
		"`/style excalidraw` — semantic palette table  ·  `/style check <file>` — audit colors",
	].join("\n");
}

function excalidrawTable(): string {
	const rows = Object.entries(EXCALIDRAW_SEMANTICS).map(
		([name, { fill, stroke, use }]) => `| \`${name}\` | \`${fill}\` | \`${stroke}\` | ${use} |`
	);
	return [
		"**Excalidraw Semantic Palette**",
		"",
		"| Purpose | Fill | Stroke | Use |",
		"|---------|------|--------|-----|",
		...rows,
		"",
		"Text on all fills: use `#c4d8e4` (Alpharius silver-white foreground)",
	].join("\n");
}

function d2Template(): string {
	return [
		"**D2 Alpharius Style Template**",
		"",
		"Copy and adapt for your diagrams. Renders with `--theme 200 --layout elk`:",
		"",
		"```d2",
		D2_STYLE_TEMPLATE,
		"```",
	].join("\n");
}

function auditColors(filePath: string): string {
	const fs = require("node:fs");
	let content: string;
	try {
		content = fs.readFileSync(filePath, "utf-8");
	} catch {
		return `❌ Could not read file: \`${filePath}\``;
	}

	const hexPattern = /#[0-9a-fA-F]{6}\b/g;
	const found = new Set<string>();
	let match: RegExpExecArray | null;
	while ((match = hexPattern.exec(content)) !== null) {
		found.add(match[0].toLowerCase());
	}

	if (found.size === 0) {
		return `✅ No hex colors found in \`${filePath}\``;
	}

	const onPalette: string[] = [];
	const offPalette: string[] = [];
	const tokensByHex = new Map<string, string[]>();
	for (const [name, hex] of Object.entries(ALL_TOKENS)) {
		const h = hex.toLowerCase();
		if (!tokensByHex.has(h)) tokensByHex.set(h, []);
		tokensByHex.get(h)!.push(name);
	}

	for (const hex of [...found].sort()) {
		const tokens = tokensByHex.get(hex);
		if (tokens) {
			onPalette.push(`  ✅ \`${hex}\` → ${tokens.join(", ")}`);
		} else {
			offPalette.push(`  ⚠️  \`${hex}\` — **off-palette**`);
		}
	}

	const lines = [`**Color Audit: \`${filePath}\`**`, "", `Found ${found.size} unique hex colors:`, ""];
	if (onPalette.length) lines.push("**On-palette:**", ...onPalette, "");
	if (offPalette.length) lines.push("**Off-palette:**", ...offPalette, "");
	return lines.join("\n");
}

// ---------------------------------------------------------------------------
// Extension
// ---------------------------------------------------------------------------

export default function styleExtension(pi: ExtensionAPI) {
	pi.registerCommand("style", {
		description: "Alpharius design system (usage: /style [palette|d2|excalidraw|check <file>])",
		getArgumentCompletions: (prefix: string) => {
			const parts = prefix.split(/\s+/);
			if (parts.length <= 1) {
				const subs: Array<{ value: string; label: string; description: string }> = [
					{ value: "palette", label: "palette", description: "Render visual swatch" },
					{ value: "d2", label: "d2", description: "D2 style template" },
					{ value: "excalidraw", label: "excalidraw", description: "Semantic palette table" },
					{ value: "check", label: "check", description: "Audit file colors" },
				];
				const filtered = subs.filter(s => s.value.startsWith(parts[0] || ""));
				return filtered.length > 0 ? filtered : null;
			}
			return null;
		},
		handler: async (args, _ctx) => {
			const trimmed = (args || "").trim();
			const [subcommand, ...rest] = trimmed.split(/\s+/);

			let output: string;

			switch (subcommand || "") {
				case "":
					output = quickRef();
					break;
				case "palette":
					// Delegate to agent to render via render_diagram tool (D2)
					pi.sendUserMessage(
						"Render a D2 diagram showing the Alpharius palette as a visual swatch. " +
						"Use the style skill's color tokens. Group into containers: Core (bg/cardBg/surfaceBg + primary/primaryMuted/primaryBright + fg/mutedFg/dimFg), " +
						"Signals (green/red/orange/yellow), and Borders (borderColor/borderDim). " +
						"Style each node with its actual hex value as fill color, appropriate font-color for contrast, " +
						"and label it with token name + hex value. Use D2 style blocks.",
						{ deliverAs: "followUp" },
					);
					return;
				case "d2":
					output = d2Template();
					break;
				case "excalidraw":
					output = excalidrawTable();
					break;
				case "check": {
					const filePath = rest.join(" ");
					if (!filePath) {
						output = "Usage: `/style check <file path>`";
					} else {
						output = auditColors(filePath);
					}
					break;
				}
				default:
					// Treat as a question — delegate to agent with style skill context
					pi.sendUserMessage(
						`The user asked about the style system: "${trimmed}". ` +
						`Answer using the style skill (Alpharius design system). Load /skill:style if needed.`,
						{ deliverAs: "followUp" },
					);
					return;
			}

			pi.sendMessage({ customType: "view", content: output, display: true });
		},
	});
}
