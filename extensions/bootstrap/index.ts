/**
 * bootstrap — First-time setup and dependency management for pi-kit.
 *
 * On first session start after install, presents a friendly checklist of
 * external dependencies grouped by tier (core / recommended / optional).
 * Offers interactive installation for missing deps.
 *
 * Commands:
 *   /bootstrap          — Run interactive setup (install missing deps)
 *   /bootstrap status   — Show dependency checklist without installing
 *   /bootstrap install  — Install all missing core + recommended deps
 *
 * Guards:
 *   - First-run detection via ~/.pi/agent/pi-kit-bootstrap-done marker
 *   - Re-running /bootstrap is always safe (idempotent checks)
 *   - Never auto-installs anything — always asks or requires explicit command
 */

import { execSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { homedir } from "node:os";
import type { ExtensionAPI } from "@mariozechner/pi-coding-agent";
import { checkAll, checkTier, formatReport, formatStatus, DEPS, type DepStatus, type DepTier } from "./deps.js";

const AGENT_DIR = join(homedir(), ".pi", "agent");
const MARKER_PATH = join(AGENT_DIR, "pi-kit-bootstrap-done");
const MARKER_VERSION = "1"; // bump to re-trigger bootstrap after adding new core deps

function isFirstRun(): boolean {
	if (!existsSync(MARKER_PATH)) return true;
	try {
		const version = readFileSync(MARKER_PATH, "utf8").trim();
		return version !== MARKER_VERSION;
	} catch {
		return true;
	}
}

function markDone(): void {
	mkdirSync(AGENT_DIR, { recursive: true });
	writeFileSync(MARKER_PATH, MARKER_VERSION + "\n", "utf8");
}

export default function (pi: ExtensionAPI) {
	// --- First-run detection on session start ---
	pi.on("session_start", async (_event, ctx) => {
		if (!isFirstRun()) return;
		if (!ctx.hasUI) return;

		const statuses = checkAll();
		const missing = statuses.filter((s) => !s.available);

		if (missing.length === 0) {
			// Everything's already installed — mark done silently
			markDone();
			return;
		}

		const coreMissing = missing.filter((s) => s.dep.tier === "core");
		const recMissing = missing.filter((s) => s.dep.tier === "recommended");

		let msg = "Welcome to pi-kit! ";
		if (coreMissing.length > 0) {
			msg += `${coreMissing.length} core dep${coreMissing.length > 1 ? "s" : ""} missing. `;
		}
		if (recMissing.length > 0) {
			msg += `${recMissing.length} recommended dep${recMissing.length > 1 ? "s" : ""} missing. `;
		}
		msg += "Run /bootstrap to set up.";

		ctx.ui.notify(msg, coreMissing.length > 0 ? "warning" : "info");
	});

	pi.addCommand({
		name: "bootstrap",
		description: "First-time setup — check and install pi-kit external dependencies",
		execute: async (ctx, args) => {
			const sub = args.trim().toLowerCase();

			if (sub === "status") {
				const statuses = checkAll();
				ctx.say(formatReport(statuses));
				return;
			}

			if (sub === "install") {
				// Non-interactive: install all missing core + recommended
				await installMissing(ctx, ["core", "recommended"]);
				return;
			}

			// Default: interactive setup
			await interactiveSetup(ctx);
		},
	});
}

async function interactiveSetup(ctx: any): Promise<void> {
	const statuses = checkAll();
	const missing = statuses.filter((s) => !s.available);

	ctx.say(formatReport(statuses));

	if (missing.length === 0) {
		markDone();
		return;
	}

	if (!ctx.hasUI) {
		ctx.say("\nRun individual install commands above, or use `/bootstrap install` to install all core + recommended deps.");
		return;
	}

	// Group missing by tier for interactive selection
	const coreMissing = missing.filter((s) => s.dep.tier === "core");
	const recMissing = missing.filter((s) => s.dep.tier === "recommended");
	const optMissing = missing.filter((s) => s.dep.tier === "optional");

	// Install core deps first
	if (coreMissing.length > 0) {
		const proceed = await ctx.ui.confirm(
			`Install ${coreMissing.length} missing core dep${coreMissing.length > 1 ? "s" : ""}? (${coreMissing.map((s: DepStatus) => s.dep.name).join(", ")})`,
		);
		if (proceed) {
			await installDeps(ctx, coreMissing);
		}
	}

	// Then recommended
	if (recMissing.length > 0) {
		const proceed = await ctx.ui.confirm(
			`Install ${recMissing.length} recommended dep${recMissing.length > 1 ? "s" : ""}? (${recMissing.map((s: DepStatus) => s.dep.name).join(", ")})`,
		);
		if (proceed) {
			await installDeps(ctx, recMissing);
		}
	}

	// Mention optional but don't push
	if (optMissing.length > 0) {
		ctx.say(
			`\n${optMissing.length} optional dep${optMissing.length > 1 ? "s" : ""} not installed: ${optMissing.map((s: DepStatus) => s.dep.name).join(", ")}.\n` +
			`Install individually when needed — see \`/bootstrap status\` for commands.`,
		);
	}

	// Re-check and report final state
	const final = checkAll();
	const stillMissing = final.filter((s) => !s.available && (s.dep.tier === "core" || s.dep.tier === "recommended"));

	if (stillMissing.length === 0) {
		ctx.say("\n🎉 Setup complete! All core and recommended dependencies are available.");
		markDone();
	} else {
		ctx.say(
			`\n⚠️  ${stillMissing.length} dep${stillMissing.length > 1 ? "s" : ""} still missing. ` +
			`Run \`/bootstrap\` again after installing manually.`,
		);
	}
}

async function installMissing(ctx: any, tiers: DepTier[]): Promise<void> {
	const statuses = checkAll();
	const toInstall = statuses.filter(
		(s) => !s.available && tiers.includes(s.dep.tier),
	);

	if (toInstall.length === 0) {
		ctx.say("All core and recommended dependencies are already installed. ✅");
		markDone();
		return;
	}

	await installDeps(ctx, toInstall);

	const final = checkAll();
	const stillMissing = final.filter(
		(s) => !s.available && tiers.includes(s.dep.tier),
	);
	if (stillMissing.length === 0) {
		ctx.say("\n🎉 All core and recommended dependencies installed!");
		markDone();
	} else {
		ctx.say(
			`\n⚠️  ${stillMissing.length} dep${stillMissing.length > 1 ? "s" : ""} failed to install:`,
		);
		for (const s of stillMissing) {
			ctx.say(`  ❌ ${s.dep.name}: try manually → ${s.dep.install[0]}`);
		}
	}
}

async function installDeps(ctx: any, deps: DepStatus[]): Promise<void> {
	for (const { dep } of deps) {
		const cmd = dep.install[0]; // Use preferred install method
		ctx.say(`\n📦 Installing ${dep.name}...`);
		ctx.say(`   → \`${cmd}\``);

		try {
			execSync(cmd, {
				stdio: "inherit",
				timeout: 300_000, // 5 min per dep
				env: { ...process.env, NONINTERACTIVE: "1", HOMEBREW_NO_AUTO_UPDATE: "1" },
			});

			// Verify it worked
			if (dep.check()) {
				ctx.say(`   ✅ ${dep.name} installed successfully`);
			} else {
				ctx.say(`   ⚠️  Command succeeded but ${dep.name} not found on PATH. You may need to restart your shell.`);
			}
		} catch (e: any) {
			ctx.say(`   ❌ Failed to install ${dep.name}`);
			if (dep.install.length > 1) {
				ctx.say(`   Alternative: \`${dep.install[1]}\``);
			}
			if (dep.url) {
				ctx.say(`   Manual install: ${dep.url}`);
			}
		}
	}
}
