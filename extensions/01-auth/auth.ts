/**
 * 01-auth/auth — Domain logic for authentication status checking.
 *
 * Extracted from index.ts so tests can import without pulling in
 * pi-tui/pi-coding-agent dependencies (which aren't resolvable under tsx).
 */

import type { ExtensionAPI } from "@styrene-lab/pi-coding-agent";

// ─── Types ───────────────────────────────────────────────────────

export type AuthStatus = "ok" | "expired" | "invalid" | "none" | "missing";

export interface AuthResult {
	provider: string;
	status: AuthStatus;
	detail: string;
	error?: string;
	refresh?: string;
	secretHint?: string;
}

export interface AuthProvider {
	/** Unique identifier: "github", "gitlab", "aws", etc. */
	id: string;
	/** Display name: "GitHub", "GitLab", "AWS", etc. */
	name: string;
	/** CLI binary name: "gh", "glab", "aws", etc. */
	cli: string;
	/** Env var that can provide a token (checked via process.env, populated by 00-secrets) */
	tokenEnvVar?: string;
	/** Command to refresh/login */
	refreshCommand: string;
	/** Check auth status. Returns structured result with diagnosis. */
	check(pi: ExtensionAPI, signal?: AbortSignal): Promise<AuthResult>;
}

// ─── Error Diagnosis Helpers ─────────────────────────────────────

/**
 * Classify auth-specific error patterns from CLI stderr.
 *
 * Pattern ordering matters: expired is checked before invalid because
 * "invalid token has expired" should classify as expired, not invalid.
 *
 * Only auth-specific keywords are matched. Generic terms like "denied"
 * or "invalid" are scoped with adjacent auth context words to avoid
 * false positives on non-auth errors like "invalid region".
 */
export function diagnoseError(stderr: string): { status: AuthStatus; reason: string } {
	const lower = stderr.toLowerCase();

	// Expired tokens — most specific, check first
	if (lower.includes("token has expired") || lower.includes("token is expired")
		|| lower.includes("session expired") || lower.includes("expiredtoken")
		|| lower.includes("credentials have expired")
		|| /\bexpired\b.*\b(?:token|session|credential|certificate)\b/.test(lower)
		|| /\b(?:token|session|credential|certificate)\b.*\bexpired\b/.test(lower)) {
		return { status: "expired", reason: "Token or session has expired" };
	}

	// Not logged in — check before invalid to avoid "not authenticated" matching "invalid"
	if (lower.includes("not logged") || lower.includes("no token") || lower.includes("not authenticated")
		|| lower.includes("login required") || lower.includes("no credentials")
		|| lower.includes("no valid credentials")
		|| lower.includes("missing client token")) {
		return { status: "none", reason: "Not authenticated" };
	}

	// Invalid/revoked credentials — scoped to auth-relevant context
	if (lower.includes("bad credentials") || lower.includes("authentication failed")
		|| lower.includes("revoked")
		|| /\b401\b/.test(lower) || lower.includes("unauthorized")) {
		return { status: "invalid", reason: extractErrorLine(stderr) };
	}

	// Forbidden (authenticated but insufficient permissions)
	if (/\b403\b/.test(lower) || lower.includes("insufficient scope")
		|| lower.includes("access denied") || lower.includes("permission denied")) {
		return { status: "invalid", reason: `Authenticated but forbidden: ${extractErrorLine(stderr)}` };
	}

	return { status: "none", reason: extractErrorLine(stderr) || "Authentication failed" };
}

/** Extract the most informative error line from multi-line stderr. */
export function extractErrorLine(stderr: string): string {
	const lines = stderr.trim().split("\n").filter(l => l.trim());
	// Prefer lines with auth-relevant error keywords
	const errorLine = lines.find(l => /error|failed|invalid|expired|denied|unauthorized|401|403/i.test(l));
	if (errorLine) return errorLine.trim().slice(0, 200);
	// Fall back to first non-empty line
	return (lines[0] || "Unknown error").trim().slice(0, 200);
}

// ─── Providers ───────────────────────────────────────────────────

const gitProvider: AuthProvider = {
	id: "git",
	name: "Git",
	cli: "git",
	refreshCommand: 'git config --global user.name "Your Name" && git config --global user.email "you@example.com"',

	async check(pi, signal) {
		const nameResult = await pi.exec("git", ["config", "user.name"], { signal, timeout: 5_000 });
		const emailResult = await pi.exec("git", ["config", "user.email"], { signal, timeout: 5_000 });
		const name = nameResult.stdout.trim() || "";
		const email = emailResult.stdout.trim() || "";

		if (name && email) {
			return { provider: this.id, status: "ok", detail: `${name} <${email}>` };
		}
		return {
			provider: this.id,
			status: "none",
			detail: `name: ${name || "(not set)"}, email: ${email || "(not set)"}`,
			refresh: this.refreshCommand,
		};
	},
};

const githubProvider: AuthProvider = {
	id: "github",
	name: "GitHub",
	cli: "gh",
	tokenEnvVar: "GITHUB_TOKEN",
	refreshCommand: "gh auth login",

	async check(pi, signal) {
		const which = await pi.exec("which", ["gh"], { signal, timeout: 3_000 });
		if (which.code !== 0) {
			return { provider: this.id, status: "missing", detail: "gh CLI not installed" };
		}

		const result = await pi.exec("gh", ["auth", "status"], { signal, timeout: 10_000 });
		const output = (result.stdout + "\n" + result.stderr).trim();

		if (result.code === 0) {
			const accountMatch = output.match(/Logged in to \S+ account (\S+)/);
			const scopeMatch = output.match(/Token scopes:(.+)/);
			let detail = accountMatch ? accountMatch[1] : "authenticated";
			if (scopeMatch) detail += ` (scopes: ${scopeMatch[1].trim()})`;
			return { provider: this.id, status: "ok", detail, refresh: this.refreshCommand };
		}

		const diag = diagnoseError(output);
		return {
			provider: this.id,
			status: diag.status,
			detail: diag.reason,
			error: output.slice(0, 300),
			refresh: this.refreshCommand,
			secretHint: "GITHUB_TOKEN",
		};
	},
};

const gitlabProvider: AuthProvider = {
	id: "gitlab",
	name: "GitLab",
	cli: "glab",
	tokenEnvVar: "GITLAB_TOKEN",
	refreshCommand: "glab auth login",

	async check(pi, signal) {
		const which = await pi.exec("which", ["glab"], { signal, timeout: 3_000 });
		if (which.code !== 0) {
			if (process.env.GITLAB_TOKEN) {
				return {
					provider: this.id,
					status: "ok",
					detail: "GITLAB_TOKEN set (glab CLI not installed)",
				};
			}
			return { provider: this.id, status: "missing", detail: "glab CLI not installed" };
		}

		const result = await pi.exec("glab", ["auth", "status"], { signal, timeout: 10_000 });
		const output = (result.stdout + "\n" + result.stderr).trim();

		if (result.code === 0) {
			const accountMatch = output.match(/Logged in to \S+ (?:as|account) (\S+)/i);
			const hostMatch = output.match(/Logged in to (\S+)/i);
			let detail = accountMatch ? accountMatch[1] : "authenticated";
			if (hostMatch) detail += ` @ ${hostMatch[1]}`;
			return { provider: this.id, status: "ok", detail, refresh: this.refreshCommand };
		}

		const diag = diagnoseError(output);
		return {
			provider: this.id,
			status: diag.status,
			detail: diag.reason,
			error: output.slice(0, 300),
			refresh: this.refreshCommand,
			secretHint: "GITLAB_TOKEN",
		};
	},
};

const awsProvider: AuthProvider = {
	id: "aws",
	name: "AWS",
	cli: "aws",
	tokenEnvVar: "AWS_ACCESS_KEY_ID",
	refreshCommand: "aws sso login --profile <profile>",

	async check(pi, signal) {
		const which = await pi.exec("which", ["aws"], { signal, timeout: 3_000 });
		if (which.code !== 0) {
			return { provider: this.id, status: "missing", detail: "aws CLI not installed" };
		}

		const result = await pi.exec("aws", ["sts", "get-caller-identity", "--output", "json"], { signal, timeout: 10_000 });

		if (result.code === 0) {
			try {
				const identity = JSON.parse(result.stdout.trim());
				return {
					provider: this.id,
					status: "ok",
					detail: identity.Arn || identity.Account || "authenticated",
					refresh: this.refreshCommand,
				};
			} catch {
				return { provider: this.id, status: "ok", detail: "authenticated", refresh: this.refreshCommand };
			}
		}

		const diag = diagnoseError(result.stderr || result.stdout);
		return {
			provider: this.id,
			status: diag.status,
			detail: diag.reason,
			error: (result.stderr || result.stdout).slice(0, 300),
			refresh: this.refreshCommand,
			secretHint: "AWS_ACCESS_KEY_ID",
		};
	},
};

const kubernetesProvider: AuthProvider = {
	id: "kubernetes",
	name: "Kubernetes",
	cli: "kubectl",
	refreshCommand: "kubectl config use-context <context>",

	async check(pi, signal) {
		const which = await pi.exec("which", ["kubectl"], { signal, timeout: 3_000 });
		if (which.code !== 0) {
			return { provider: this.id, status: "missing", detail: "kubectl not installed" };
		}

		const kctx = await pi.exec("kubectl", ["config", "current-context"], { signal, timeout: 5_000 });
		if (kctx.code === 0) {
			const context = kctx.stdout.trim();
			const verify = await pi.exec("kubectl", ["cluster-info", "--request-timeout=5s"], { signal, timeout: 10_000 });
			if (verify.code === 0) {
				return {
					provider: this.id,
					status: "ok",
					detail: `context: ${context}`,
					refresh: this.refreshCommand,
				};
			}
			const diag = diagnoseError(verify.stderr || verify.stdout);
			return {
				provider: this.id,
				status: diag.status,
				detail: `context: ${context} — ${diag.reason}`,
				error: (verify.stderr || "").slice(0, 300),
				refresh: this.refreshCommand,
			};
		}

		return {
			provider: this.id,
			status: "none",
			detail: "No context set",
			refresh: this.refreshCommand,
		};
	},
};

const ociProvider: AuthProvider = {
	id: "oci",
	name: "OCI Registry (ghcr.io)",
	cli: "podman",
	refreshCommand: "gh auth token | podman login ghcr.io -u <user> --password-stdin",

	async check(pi, signal) {
		const podmanWhich = await pi.exec("which", ["podman"], { signal, timeout: 3_000 });
		const dockerWhich = await pi.exec("which", ["docker"], { signal, timeout: 3_000 });
		const cmd = podmanWhich.code === 0 ? "podman" : dockerWhich.code === 0 ? "docker" : null;

		if (!cmd) {
			return { provider: this.id, status: "missing", detail: "Neither podman nor docker installed" };
		}

		const refresh = `gh auth token | ${cmd} login ghcr.io -u $(gh api user --jq .login) --password-stdin`;

		const result = await pi.exec(cmd, ["login", "--get-login", "ghcr.io"], { signal, timeout: 5_000 });
		if (result.code === 0) {
			return {
				provider: this.id,
				status: "ok",
				detail: `ghcr.io: ${result.stdout.trim()} (${cmd})`,
				refresh,
			};
		}

		return {
			provider: this.id,
			status: "none",
			detail: `Not logged in to ghcr.io (${cmd})`,
			refresh,
		};
	},
};

const vaultProvider: AuthProvider = {
	id: "vault",
	name: "Vault",
	cli: "vault",
	tokenEnvVar: "VAULT_TOKEN",
	refreshCommand: "vault login",

	async check(pi, signal) {
		// 1. Check CLI is installed
		const which = await pi.exec("which", ["vault"], { signal, timeout: 3_000 });
		if (which.code !== 0) {
			return { provider: this.id, status: "missing", detail: "vault CLI not installed" };
		}

		// 2. Check VAULT_ADDR is configured — without it, no meaningful check is possible
		const addr = process.env["VAULT_ADDR"];
		if (!addr) {
			return {
				provider: this.id,
				status: "none",
				detail: "VAULT_ADDR not set",
				refresh: this.refreshCommand,
				secretHint: "VAULT_ADDR",
			};
		}

		// 3. Run vault token lookup — read-only, returns token metadata (never the token itself)
		// VAULT_TOKEN is read by the vault CLI from the environment; we never access it directly.
		const result = await pi.exec("vault", ["token", "lookup", "-format=json"], { signal, timeout: 10_000 });

		if (result.code === 0) {
			try {
				const data = JSON.parse(result.stdout.trim());
				const tokenData = data?.data ?? {};

				// Extract safe metadata — policies and expiry only, never the token value
				const policies: string[] = tokenData.policies ?? [];
				const displayName: string = tokenData.display_name ?? "";
				const expireTime: string = tokenData.expire_time ?? "";

				// Build a human-readable detail string
				const parts: string[] = [];
				if (displayName) parts.push(displayName);
				if (policies.length > 0) parts.push(`policies: ${policies.filter(p => p !== "default").join(", ") || "default"}`);
				if (expireTime) parts.push(`expires: ${expireTime.split("T")[0]}`);
				else parts.push("no expiry");

				return {
					provider: this.id,
					status: "ok",
					detail: parts.join(" · ") || "authenticated",
					refresh: this.refreshCommand,
				};
			} catch {
				// JSON parse failed but command succeeded — still authenticated
				return { provider: this.id, status: "ok", detail: "authenticated", refresh: this.refreshCommand };
			}
		}

		// 4. Diagnose failure — truncate to 300 chars, never log token values
		const output = (result.stdout + "\n" + result.stderr).trim();
		const diag = diagnoseError(output);
		return {
			provider: this.id,
			status: diag.status,
			detail: `${addr} — ${diag.reason}`,
			error: output.slice(0, 300),
			refresh: this.refreshCommand,
			secretHint: "VAULT_TOKEN",
		};
	},
};

// ─── Provider Registry ───────────────────────────────────────────

/** All providers, ordered by typical check priority. */
export const ALL_PROVIDERS: AuthProvider[] = [
	gitProvider,
	githubProvider,
	gitlabProvider,
	awsProvider,
	kubernetesProvider,
	ociProvider,
	vaultProvider,
];

export function findProvider(idOrName: string): AuthProvider | undefined {
	const lower = idOrName.toLowerCase();
	return ALL_PROVIDERS.find(p => p.id === lower || p.name.toLowerCase() === lower);
}

// ─── Shared check-all helper ─────────────────────────────────────

export async function checkAllProviders(pi: ExtensionAPI, signal?: AbortSignal): Promise<AuthResult[]> {
	const results: AuthResult[] = [];
	for (const provider of ALL_PROVIDERS) {
		try {
			results.push(await provider.check(pi, signal));
		} catch (e: any) {
			results.push({
				provider: provider.id,
				status: "none",
				detail: `Check failed: ${e.message}`,
			});
		}
	}
	return results;
}

// ─── Formatting ──────────────────────────────────────────────────

export const STATUS_ICONS: Record<AuthStatus, string> = {
	ok: "✓",
	expired: "⚠",
	invalid: "✗",
	none: "✗",
	missing: "·",
};

export function formatResults(results: AuthResult[]): string {
	const lines: string[] = ["**Auth Status**", ""];

	for (const r of results) {
		const icon = STATUS_ICONS[r.status];
		let line = `  ${icon}  **${r.provider}**: ${r.detail}`;
		if (r.error && r.status !== "ok") {
			line += `\n      Error: ${r.error.split("\n")[0].slice(0, 120)}`;
		}
		lines.push(line);
	}

	// Actionable items
	const fixable = results.filter(r =>
		r.status === "expired" || r.status === "invalid" || r.status === "none"
	);
	if (fixable.length > 0) {
		lines.push("", "**To fix:**");
		for (const r of fixable) {
			if (r.status === "expired") {
				lines.push(`  ${r.provider}: token expired → \`${r.refresh}\``);
			} else if (r.status === "invalid") {
				lines.push(`  ${r.provider}: credentials invalid → \`${r.refresh}\``);
				if (r.secretHint) {
					lines.push(`    Or configure via: \`/secrets configure ${r.secretHint}\``);
				}
			} else {
				lines.push(`  ${r.provider}: \`${r.refresh}\``);
				if (r.secretHint) {
					lines.push(`    Or configure via: \`/secrets configure ${r.secretHint}\``);
				}
			}
		}
	}

	return lines.join("\n");
}
