import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import {
	buildGuidedProfile,
	loadOperatorProfile,
	needsOperatorProfileSetup,
	parseCommandArgv,
	requiresShell,
	routingPolicyFromProfile,
	runAsync,
	saveOperatorProfile,
	summarizeProviderReadiness,
	synthesizeSafeDefaultProfile,
	type OperatorCapabilityProfile,
} from "./index.ts";
import type { AuthResult } from "../01-auth/auth.ts";

function makeTmpDir(): string {
	return mkdtempSync(join(tmpdir(), "bootstrap-profile-"));
}

describe("bootstrap operator profile helpers", () => {
	it("reports setup needed when no operator profile exists", () => {
		const tmp = makeTmpDir();
		try {
			assert.equal(needsOperatorProfileSetup(tmp), true);
		} finally {
			rmSync(tmp, { recursive: true, force: true });
		}
	});

	it("persists operator profile without clobbering unrelated config keys", () => {
		const tmp = makeTmpDir();
		mkdirSync(join(tmp, ".pi"), { recursive: true });
		writeFileSync(join(tmp, ".pi", "config.json"), JSON.stringify({ editor: "vscode" }));
		const profile = buildGuidedProfile({
			primaryProvider: "openai",
			allowCloudCrossProviderFallback: true,
			automaticLightLocalFallback: false,
			heavyLocalFallback: "deny",
		});

		try {
			saveOperatorProfile(tmp, profile);
			const loaded = loadOperatorProfile(tmp);
			assert.deepEqual(loaded, profile);
			const config = JSON.parse(readFileSync(join(tmp, ".pi", "config.json"), "utf-8")) as {
				editor?: string;
				operatorProfile?: OperatorCapabilityProfile;
			};
			assert.equal(config.editor, "vscode");
			assert.deepEqual(config.operatorProfile, profile);
			assert.equal(needsOperatorProfileSetup(tmp), false);
		} finally {
			rmSync(tmp, { recursive: true, force: true });
		}
	});

	it("summarizes provider readiness from reused auth results", () => {
		const results: AuthResult[] = [
			{ provider: "github", status: "ok", detail: "ready" },
			{ provider: "gitlab", status: "expired", detail: "token expired" },
			{ provider: "aws", status: "missing", detail: "aws cli not installed" },
			{ provider: "git", status: "ok", detail: "ignored non-cloud provider" },
		];
		assert.deepEqual(summarizeProviderReadiness(results), {
			ready: ["github"],
			authAttention: ["gitlab"],
			missing: ["aws"],
		});
	});

	it("synthesizes conservative defaults when setup is skipped", () => {
		const profile = synthesizeSafeDefaultProfile([
			{ provider: "github", status: "ok", detail: "ready" },
			{ provider: "aws", status: "ok", detail: "ready" },
		]);
		assert.equal(profile.setupComplete, false);
		assert.equal(profile.roles.archmagos[0]?.provider, "anthropic");
		assert.equal(profile.fallback.sameRoleCrossProvider, "allow");
		assert.equal(profile.fallback.crossSource, "ask");
		assert.equal(profile.fallback.heavyLocal, "ask");
		assert.equal(profile.fallback.unknownLocalPerformance, "ask");
	});

	it("builds guided profile from qualitative setup answers", () => {
		const profile = buildGuidedProfile({
			primaryProvider: "openai",
			allowCloudCrossProviderFallback: false,
			automaticLightLocalFallback: true,
			heavyLocalFallback: "deny",
		});
		assert.equal(profile.setupComplete, true);
		assert.equal(profile.roles.archmagos[0]?.provider, "openai");
		assert.equal(profile.roles.magos[0]?.provider, "openai");
		assert.ok(profile.roles.servitor.some((candidate) => candidate.source === "local"));
		assert.equal(profile.fallback.sameRoleCrossProvider, "ask");
		assert.equal(profile.fallback.crossSource, "ask");
		assert.equal(profile.fallback.heavyLocal, "deny");
		assert.equal(profile.fallback.unknownLocalPerformance, "ask");
	});

	it("derives routing policy from operator profile preferences", () => {
		const profile = buildGuidedProfile({
			primaryProvider: "openai",
			allowCloudCrossProviderFallback: true,
			automaticLightLocalFallback: false,
			heavyLocalFallback: "deny",
		});
		const policy = routingPolicyFromProfile(profile);
		assert.deepEqual(policy.providerOrder, ["openai", "anthropic", "local"]);
		assert.deepEqual(policy.avoidProviders, ["local"]);
		assert.equal(policy.cheapCloudPreferredOverLocal, true);
		assert.match(policy.notes ?? "", /operator capability profile/i);
	});

	it("ignores invalid operator profile payloads", () => {
		const tmp = makeTmpDir();
		mkdirSync(join(tmp, ".pi"), { recursive: true });
		writeFileSync(join(tmp, ".pi", "config.json"), JSON.stringify({
			operatorProfile: { version: 1, setupComplete: "yes" },
		}));
		try {
			assert.equal(loadOperatorProfile(tmp), undefined);
			assert.equal(needsOperatorProfileSetup(tmp), true);
		} finally {
			rmSync(tmp, { recursive: true, force: true });
		}
	});
});

describe("bootstrap subprocess dispatch helpers", () => {
	describe("requiresShell", () => {
		it("detects pipe operator", () => {
			assert.equal(requiresShell("curl -fsSL https://ollama.com/install.sh | sh"), true);
		});

		it("detects logical-or operator", () => {
			assert.equal(requiresShell("sudo apt install gh || sudo dnf install gh"), true);
		});

		it("detects semicolon", () => {
			assert.equal(requiresShell("echo a; echo b"), true);
		});

		it("detects subshell", () => {
			assert.equal(requiresShell("$(which brew)"), true);
		});

		it("detects redirect", () => {
			assert.equal(requiresShell("echo hello > /dev/null"), true);
		});

		it("returns false for simple brew install command", () => {
			assert.equal(requiresShell("brew install ollama"), false);
		});

		it("returns false for brew install d2", () => {
			assert.equal(requiresShell("brew install d2"), false);
		});

		it("returns false for cargo install with flags", () => {
			// cargo install --git https://... uses no shell metacharacters
			assert.equal(requiresShell("cargo install --git https://github.com/cwilson613/mdserve --branch feature/wikilinks-graph"), false);
		});

		it("returns false for URL containing a fragment (#anchor)", () => {
			// '#' inside a URL fragment is not a shell comment — only '#' at the
			// start of a word (after whitespace or at string start) is.
			assert.equal(requiresShell("cargo install --git https://github.com/user/repo#v1.2.3"), false);
		});

		it("returns true for inline shell comment (# preceded by whitespace)", () => {
			// 'brew install foo # comment' requires shell to strip the comment
			assert.equal(requiresShell("brew install foo # install homebrew package"), true);
		});

		it("returns true for hash at start of string", () => {
			assert.equal(requiresShell("# this is a comment"), true);
		});

		it("returns false for apt install single package", () => {
			assert.equal(requiresShell("sudo apt install pandoc"), false);
		});
	});

	describe("parseCommandArgv", () => {
		it("splits brew install into executable plus args", () => {
			assert.deepEqual(parseCommandArgv("brew install ollama"), ["brew", "install", "ollama"]);
		});

		it("collapses extra whitespace", () => {
			assert.deepEqual(parseCommandArgv("  brew   install   d2  "), ["brew", "install", "d2"]);
		});

		it("handles single-token command", () => {
			assert.deepEqual(parseCommandArgv("ollama"), ["ollama"]);
		});

		it("splits cargo install with multiple flags", () => {
			const parts = parseCommandArgv("cargo install --git https://github.com/cwilson613/mdserve --branch feature/wikilinks-graph");
			assert.equal(parts[0], "cargo");
			assert.equal(parts[1], "install");
			assert.ok(parts.includes("--git"));
			assert.ok(parts.includes("--branch"));
		});

		it("throws on empty command", () => {
			assert.throws(() => parseCommandArgv("   "), /Empty command/);
		});
	});

	describe("runAsync dispatch", () => {
		it("runs a simple explicit-dispatch command and returns exit 0", async () => {
			// 'true' is a POSIX no-op that exits 0 — no shell needed
			const code = await runAsync("true", 5000);
			assert.equal(code, 0);
		});

		it("runs a shell-construct command and returns exit 0", async () => {
			// echo with pipe requires shell; shell should handle it fine
			const code = await runAsync("echo hello | cat", 5000);
			assert.equal(code, 0);
		});

		it("returns non-zero exit code for failing command", async () => {
			const code = await runAsync("false", 5000);
			assert.notEqual(code, 0);
		});

		it("returns 124 on timeout", async () => {
			// Use a generous timeout (500 ms) so that spawn + SIGTERM delivery
			// completes reliably even on heavily-loaded CI hosts.  The child
			// sleeps for 60 s so the timeout always fires first.
			const code = await runAsync("sleep 60", 500);
			assert.equal(code, 124);
		});

		it("returns 1 on spawn error for nonexistent executable", async () => {
			// A nonexistent command: explicit dispatch path will ENOENT → error handler
			const code = await runAsync("__nonexistent_binary_xyz__", 5000);
			assert.equal(code, 1);
		});
	});
});
