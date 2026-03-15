/**
 * auto-compact — Proactive context compaction extension
 *
 * Monitors context usage after each turn and triggers compaction
 * before the context window fills up. The built-in auto-compaction
 * only fires at the hard limit (contextWindow - reserveTokens);
 * this extension fires earlier at a configurable percentage threshold,
 * so the agent can clean up its own context without user intervention.
 *
 * Configuration (environment variables):
 *   AUTO_COMPACT_PERCENT  — trigger threshold (default: 70)
 *   AUTO_COMPACT_COOLDOWN — minimum seconds between compactions (default: 60)
 */

import type { ExtensionAPI, ExtensionContext } from "@styrene-lab/pi-coding-agent";

const COMPACT_PERCENT = Number(process.env.AUTO_COMPACT_PERCENT) || 70;
const COOLDOWN_MS = (Number(process.env.AUTO_COMPACT_COOLDOWN) || 60) * 1000;

export default function autoCompactExtension(pi: ExtensionAPI) {
	let lastCompactTime = 0;
	let compacting = false;

	pi.on("turn_end", (_event, ctx) => {
		if (compacting) return;

		const usage = ctx.getContextUsage();
		if (!usage || usage.percent === null) return;
		if (usage.percent < COMPACT_PERCENT) return;

		const now = Date.now();
		if (now - lastCompactTime < COOLDOWN_MS) return;

		compacting = true;
		lastCompactTime = now;

		ctx.compact({
			onComplete: () => { compacting = false; },
			onError: () => { compacting = false; },
		});
	});
}
