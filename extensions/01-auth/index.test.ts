/**
 * Tests for 01-auth — error diagnosis, formatting, provider registry,
 * and provider check logic with mocked pi.exec.
 */

import { describe, it } from "node:test";
import * as assert from "node:assert/strict";
import {
	diagnoseError,
	extractErrorLine,
	ALL_PROVIDERS,
	formatResults,
	findProvider,
	checkAllProviders,
	type AuthResult,
} from "./auth.ts";

// ─── Mock pi.exec ───────────────────────────────────────────────

interface MockExecResult {
	code: number;
	stdout: string;
	stderr: string;
}

function createMockPi(responses: Map<string, MockExecResult>) {
	return {
		exec(command: string, args: string[], _opts?: unknown): Promise<MockExecResult> {
			const key = `${command} ${args.join(" ")}`;
			const result = responses.get(key);
			if (result) return Promise.resolve(result);
			// Default: command not found
			return Promise.resolve({ code: 1, stdout: "", stderr: `${command}: not found` });
		},
	} as any; // Partial mock — only exec is needed by providers
}

const OK = { code: 0, stdout: "", stderr: "" };
const FAIL = { code: 1, stdout: "", stderr: "" };

// ─── diagnoseError ──────────────────────────────────────────────

describe("diagnoseError", () => {
	it("detects 'token has expired'", () => {
		assert.equal(diagnoseError("Error: token has expired, please refresh").status, "expired");
	});

	it("detects 'token is expired'", () => {
		assert.equal(diagnoseError("The token is expired").status, "expired");
	});

	it("detects ExpiredToken (AWS style)", () => {
		const msg = "An error occurred (ExpiredTokenException): The security token included in the request is expired";
		assert.equal(diagnoseError(msg).status, "expired");
	});

	it("detects 'session expired'", () => {
		assert.equal(diagnoseError("error: session expired, please re-authenticate").status, "expired");
	});

	it("detects 'credentials have expired'", () => {
		assert.equal(diagnoseError("Your credentials have expired").status, "expired");
	});

	it("does NOT classify 'invalid region' as invalid auth", () => {
		// "invalid" alone without auth context should NOT match
		const result = diagnoseError("Error: invalid region us-east-99");
		assert.equal(result.status, "none", "Generic 'invalid' without auth context should be 'none'");
	});

	it("does NOT classify 'access denied to bucket' as invalid when no auth keywords", () => {
		// "access denied" IS an auth-specific phrase, should classify as invalid
		const result = diagnoseError("Error: access denied to s3://bucket/key");
		assert.equal(result.status, "invalid");
	});

	it("detects 'bad credentials'", () => {
		assert.equal(diagnoseError("HTTP 401: Bad credentials").status, "invalid");
	});

	it("detects 'unauthorized'", () => {
		assert.equal(diagnoseError("Error: unauthorized - authentication failed").status, "invalid");
	});

	it("detects 'revoked' token", () => {
		assert.equal(diagnoseError("Token has been revoked by the user").status, "invalid");
	});

	it("detects 403 with 'insufficient scope'", () => {
		assert.equal(diagnoseError("Error: 403 Forbidden - insufficient scope").status, "invalid");
	});

	it("detects 'not logged in'", () => {
		assert.equal(diagnoseError("You are not logged in. Run `gh auth login`").status, "none");
	});

	it("detects 'no token'", () => {
		assert.equal(diagnoseError("no token found").status, "none");
	});

	it("defaults to none for unrecognized errors", () => {
		assert.equal(diagnoseError("Something completely unexpected happened").status, "none");
	});

	it("returns non-empty reason", () => {
		const result = diagnoseError("Error: bad credentials\nPlease try again");
		assert.ok(result.reason.length > 0);
	});

	it("prioritizes expired over invalid for 'invalid token has expired'", () => {
		// Expired check runs first, so this should be 'expired'
		assert.equal(diagnoseError("Error: invalid token has expired").status, "expired");
	});
});

// ─── extractErrorLine ───────────────────────────────────────────

describe("extractErrorLine", () => {
	it("extracts line with error keyword", () => {
		const result = extractErrorLine("Starting check...\nError: token expired\nDone.");
		assert.ok(result.includes("token expired"));
	});

	it("falls back to first line if no error keyword", () => {
		assert.equal(extractErrorLine("Some output\nMore output"), "Some output");
	});

	it("handles empty input", () => {
		assert.equal(extractErrorLine(""), "Unknown error");
	});

	it("truncates long lines to 200 chars", () => {
		const longLine = "Error: " + "x".repeat(300);
		assert.ok(extractErrorLine(longLine).length <= 200);
	});
});

// ─── Provider Registry ──────────────────────────────────────────

describe("ALL_PROVIDERS", () => {
	it("has 7 providers", () => {
		assert.equal(ALL_PROVIDERS.length, 7);
	});

	it("every provider has required fields", () => {
		for (const p of ALL_PROVIDERS) {
			assert.ok(p.id, `Missing id`);
			assert.ok(p.name, `Missing name for ${p.id}`);
			assert.ok(p.cli, `Missing cli for ${p.id}`);
			assert.ok(p.refreshCommand, `Missing refreshCommand for ${p.id}`);
			assert.equal(typeof p.check, "function", `Missing check() for ${p.id}`);
		}
	});

	it("has unique ids", () => {
		const ids = ALL_PROVIDERS.map(p => p.id);
		assert.equal(new Set(ids).size, ids.length, "Duplicate provider IDs");
	});

	it("includes all expected providers", () => {
		const ids = ALL_PROVIDERS.map(p => p.id);
		for (const expected of ["git", "github", "gitlab", "aws", "kubernetes", "oci", "vault"]) {
			assert.ok(ids.includes(expected), `Missing provider: ${expected}`);
		}
	});
});

// ─── findProvider ───────────────────────────────────────────────

describe("findProvider", () => {
	it("finds by id", () => {
		const p = findProvider("github");
		assert.ok(p);
		assert.equal(p.id, "github");
	});

	it("finds by name (case-insensitive)", () => {
		const p = findProvider("GitHub");
		assert.ok(p);
		assert.equal(p.id, "github");
	});

	it("returns undefined for unknown", () => {
		assert.equal(findProvider("bitbucket"), undefined);
	});
});

// ─── formatResults ──────────────────────────────────────────────

describe("formatResults", () => {
	it("formats ok results without fix section", () => {
		const results: AuthResult[] = [
			{ provider: "github", status: "ok", detail: "cwilson613" },
		];
		const text = formatResults(results);
		assert.ok(text.includes("✓"));
		assert.ok(text.includes("github"));
		assert.ok(text.includes("cwilson613"));
		assert.ok(!text.includes("To fix"));
	});

	it("includes refresh commands for expired results", () => {
		const results: AuthResult[] = [
			{ provider: "github", status: "expired", detail: "Token expired", refresh: "gh auth login" },
		];
		const text = formatResults(results);
		assert.ok(text.includes("⚠"));
		assert.ok(text.includes("To fix"));
		assert.ok(text.includes("gh auth login"));
	});

	it("includes /secrets configure hint for invalid credentials", () => {
		const results: AuthResult[] = [
			{
				provider: "gitlab",
				status: "invalid",
				detail: "Bad credentials",
				refresh: "glab auth login",
				secretHint: "GITLAB_TOKEN",
			},
		];
		const text = formatResults(results);
		assert.ok(text.includes("/secrets configure GITLAB_TOKEN"));
	});

	it("shows error detail for failed checks", () => {
		const results: AuthResult[] = [
			{
				provider: "aws",
				status: "none",
				detail: "Not authenticated",
				error: "An error occurred: no credentials found",
				refresh: "aws sso login",
			},
		];
		const text = formatResults(results);
		assert.ok(text.includes("no credentials found"));
	});

	it("skips missing providers in fix section", () => {
		const results: AuthResult[] = [
			{ provider: "gitlab", status: "missing", detail: "glab CLI not installed" },
		];
		const text = formatResults(results);
		assert.ok(text.includes("·"));
		assert.ok(!text.includes("To fix"));
	});

	it("handles mixed statuses", () => {
		const results: AuthResult[] = [
			{ provider: "git", status: "ok", detail: "user <email>" },
			{ provider: "github", status: "ok", detail: "cwilson613" },
			{ provider: "gitlab", status: "none", detail: "Not authenticated", refresh: "glab auth login", secretHint: "GITLAB_TOKEN" },
			{ provider: "aws", status: "missing", detail: "aws CLI not installed" },
		];
		const text = formatResults(results);
		assert.ok(text.includes("✓"));
		assert.ok(text.includes("✗"));
		assert.ok(text.includes("·"));
		assert.ok(text.includes("To fix"));
		assert.ok(text.includes("glab auth login"));
	});
});

// ─── Provider check() with mock pi.exec ─────────────────────────

describe("gitProvider.check()", () => {
	const git = ALL_PROVIDERS.find(p => p.id === "git")!;

	it("returns ok with name and email", async () => {
		const pi = createMockPi(new Map([
			["git config user.name", { code: 0, stdout: "Chris Wilson\n", stderr: "" }],
			["git config user.email", { code: 0, stdout: "chris@example.com\n", stderr: "" }],
		]));
		const result = await git.check(pi);
		assert.equal(result.status, "ok");
		assert.equal(result.provider, "git");
		assert.ok(result.detail.includes("Chris Wilson"));
		assert.ok(result.detail.includes("chris@example.com"));
	});

	it("returns none when name is missing", async () => {
		const pi = createMockPi(new Map([
			["git config user.name", { code: 1, stdout: "", stderr: "" }],
			["git config user.email", { code: 0, stdout: "chris@example.com\n", stderr: "" }],
		]));
		const result = await git.check(pi);
		assert.equal(result.status, "none");
		assert.ok(result.detail.includes("(not set)"));
	});
});

describe("githubProvider.check()", () => {
	const gh = ALL_PROVIDERS.find(p => p.id === "github")!;

	it("returns missing when gh CLI not installed", async () => {
		const pi = createMockPi(new Map([
			["which gh", FAIL],
		]));
		const result = await gh.check(pi);
		assert.equal(result.status, "missing");
		assert.equal(result.provider, "github");
	});

	it("extracts account name from gh auth status output", async () => {
		const pi = createMockPi(new Map([
			["which gh", OK],
			["gh auth status", {
				code: 0,
				stdout: "github.com\n  ✓ Logged in to github.com account cwilson613 (GITHUB_TOKEN)\n  - Token scopes: 'gist', 'repo'\n",
				stderr: "",
			}],
		]));
		const result = await gh.check(pi);
		assert.equal(result.status, "ok");
		assert.ok(result.detail.includes("cwilson613"), `Expected 'cwilson613' in '${result.detail}'`);
		assert.ok(result.detail.includes("scopes:"), `Expected scopes in '${result.detail}'`);
	});

	it("returns expired diagnosis from stderr", async () => {
		const pi = createMockPi(new Map([
			["which gh", OK],
			["gh auth status", { code: 1, stdout: "", stderr: "token has expired" }],
		]));
		const result = await gh.check(pi);
		assert.equal(result.status, "expired");
		assert.equal(result.secretHint, "GITHUB_TOKEN");
	});
});

describe("gitlabProvider.check()", () => {
	const gl = ALL_PROVIDERS.find(p => p.id === "gitlab")!;

	it("returns ok from GITLAB_TOKEN env when glab not installed", async () => {
		const original = process.env.GITLAB_TOKEN;
		process.env.GITLAB_TOKEN = "test-token";
		try {
			const pi = createMockPi(new Map([
				["which glab", FAIL],
			]));
			const result = await gl.check(pi);
			assert.equal(result.status, "ok");
			assert.ok(result.detail.includes("GITLAB_TOKEN set"));
		} finally {
			if (original === undefined) delete process.env.GITLAB_TOKEN;
			else process.env.GITLAB_TOKEN = original;
		}
	});

	it("returns missing when glab not installed and no token", async () => {
		const original = process.env.GITLAB_TOKEN;
		delete process.env.GITLAB_TOKEN;
		try {
			const pi = createMockPi(new Map([
				["which glab", FAIL],
			]));
			const result = await gl.check(pi);
			assert.equal(result.status, "missing");
		} finally {
			if (original !== undefined) process.env.GITLAB_TOKEN = original;
		}
	});

	it("extracts account from 'as <user>' format", async () => {
		const pi = createMockPi(new Map([
			["which glab", OK],
			["glab auth status", {
				code: 0,
				stdout: "Logged in to gitlab.com as cwilson\n",
				stderr: "",
			}],
		]));
		const result = await gl.check(pi);
		assert.equal(result.status, "ok");
		assert.ok(result.detail.includes("cwilson"), `Expected 'cwilson' in '${result.detail}'`);
	});

	it("extracts account from 'account <user>' format", async () => {
		const pi = createMockPi(new Map([
			["which glab", OK],
			["glab auth status", {
				code: 0,
				stdout: "Logged in to gitlab.com account cwilson\n",
				stderr: "",
			}],
		]));
		const result = await gl.check(pi);
		assert.equal(result.status, "ok");
		assert.ok(result.detail.includes("cwilson"));
	});
});

describe("awsProvider.check()", () => {
	const aws = ALL_PROVIDERS.find(p => p.id === "aws")!;

	it("returns ok with ARN from sts response", async () => {
		const pi = createMockPi(new Map([
			["which aws", OK],
			["aws sts get-caller-identity --output json", {
				code: 0,
				stdout: JSON.stringify({ Arn: "arn:aws:iam::123:user/dev", Account: "123" }),
				stderr: "",
			}],
		]));
		const result = await aws.check(pi);
		assert.equal(result.status, "ok");
		assert.ok(result.detail.includes("arn:aws:iam"));
	});

	it("returns expired for ExpiredToken", async () => {
		const pi = createMockPi(new Map([
			["which aws", OK],
			["aws sts get-caller-identity --output json", {
				code: 1,
				stdout: "",
				stderr: "An error occurred (ExpiredTokenException): The security token included in the request is expired",
			}],
		]));
		const result = await aws.check(pi);
		assert.equal(result.status, "expired");
	});
});

describe("kubernetesProvider.check()", () => {
	const k8s = ALL_PROVIDERS.find(p => p.id === "kubernetes")!;

	it("returns ok with context name when cluster reachable", async () => {
		const pi = createMockPi(new Map([
			["which kubectl", OK],
			["kubectl config current-context", { code: 0, stdout: "prod-cluster\n", stderr: "" }],
			["kubectl cluster-info --request-timeout=5s", { code: 0, stdout: "Kubernetes control plane is running\n", stderr: "" }],
		]));
		const result = await k8s.check(pi);
		assert.equal(result.status, "ok");
		assert.ok(result.detail.includes("prod-cluster"));
	});

	it("returns none when no context set", async () => {
		const pi = createMockPi(new Map([
			["which kubectl", OK],
			["kubectl config current-context", { code: 1, stdout: "", stderr: "error: current-context is not set" }],
		]));
		const result = await k8s.check(pi);
		assert.equal(result.status, "none");
	});
});

describe("ociProvider.check()", () => {
	const oci = ALL_PROVIDERS.find(p => p.id === "oci")!;

	it("returns missing when neither podman nor docker installed", async () => {
		const pi = createMockPi(new Map([
			["which podman", FAIL],
			["which docker", FAIL],
		]));
		const result = await oci.check(pi);
		assert.equal(result.status, "missing");
	});

	it("returns ok when logged in via podman", async () => {
		const pi = createMockPi(new Map([
			["which podman", OK],
			["which docker", FAIL],
			["podman login --get-login ghcr.io", { code: 0, stdout: "cwilson613\n", stderr: "" }],
		]));
		const result = await oci.check(pi);
		assert.equal(result.status, "ok");
		assert.ok(result.detail.includes("cwilson613"));
		assert.ok(result.detail.includes("podman"));
	});
});

describe("vaultProvider.check()", () => {
	const vault = ALL_PROVIDERS.find(p => p.id === "vault")!;

	it("returns missing when vault CLI not installed", async () => {
		const pi = createMockPi(new Map([
			["which vault", FAIL],
		]));
		const result = await vault.check(pi);
		assert.equal(result.status, "missing");
		assert.equal(result.provider, "vault");
	});

	it("returns none when VAULT_ADDR not set", async () => {
		const original = process.env.VAULT_ADDR;
		delete process.env.VAULT_ADDR;
		try {
			const pi = createMockPi(new Map([
				["which vault", OK],
			]));
			const result = await vault.check(pi);
			assert.equal(result.status, "none");
			assert.ok(result.detail.includes("VAULT_ADDR not set"));
			assert.equal(result.secretHint, "VAULT_ADDR");
		} finally {
			if (original !== undefined) process.env.VAULT_ADDR = original;
		}
	});

	it("returns ok with policies and expiry from token lookup", async () => {
		const original = process.env.VAULT_ADDR;
		process.env.VAULT_ADDR = "https://vault.example.com:8200";
		try {
			const tokenLookupOutput = JSON.stringify({
				data: {
					display_name: "token-admin",
					policies: ["default", "admin"],
					expire_time: "2026-06-01T00:00:00Z",
				},
			});
			const pi = createMockPi(new Map([
				["which vault", OK],
				["vault token lookup -format=json", { code: 0, stdout: tokenLookupOutput, stderr: "" }],
			]));
			const result = await vault.check(pi);
			assert.equal(result.status, "ok");
			assert.ok(result.detail.includes("token-admin"), `Expected display_name in '${result.detail}'`);
			assert.ok(result.detail.includes("admin"), `Expected policies in '${result.detail}'`);
			assert.ok(result.detail.includes("2026-06-01"), `Expected expiry date in '${result.detail}'`);
		} finally {
			if (original !== undefined) process.env.VAULT_ADDR = original;
			else delete process.env.VAULT_ADDR;
		}
	});

	it("returns ok with 'no expiry' for non-expiring tokens", async () => {
		const original = process.env.VAULT_ADDR;
		process.env.VAULT_ADDR = "https://vault.example.com:8200";
		try {
			const tokenLookupOutput = JSON.stringify({
				data: {
					display_name: "root",
					policies: ["root"],
					expire_time: "",
				},
			});
			const pi = createMockPi(new Map([
				["which vault", OK],
				["vault token lookup -format=json", { code: 0, stdout: tokenLookupOutput, stderr: "" }],
			]));
			const result = await vault.check(pi);
			assert.equal(result.status, "ok");
			assert.ok(result.detail.includes("no expiry"), `Expected 'no expiry' in '${result.detail}'`);
		} finally {
			if (original !== undefined) process.env.VAULT_ADDR = original;
			else delete process.env.VAULT_ADDR;
		}
	});

	it("returns expired when token has expired", async () => {
		const original = process.env.VAULT_ADDR;
		process.env.VAULT_ADDR = "https://vault.example.com:8200";
		try {
			const pi = createMockPi(new Map([
				["which vault", OK],
				["vault token lookup -format=json", {
					code: 1,
					stdout: "",
					stderr: "Error looking up token: Code: 403. Errors:\n* token has expired",
				}],
			]));
			const result = await vault.check(pi);
			assert.equal(result.status, "expired");
			assert.equal(result.secretHint, "VAULT_TOKEN");
		} finally {
			if (original !== undefined) process.env.VAULT_ADDR = original;
			else delete process.env.VAULT_ADDR;
		}
	});

	it("returns none when token is not set", async () => {
		const original = process.env.VAULT_ADDR;
		process.env.VAULT_ADDR = "https://vault.example.com:8200";
		try {
			const pi = createMockPi(new Map([
				["which vault", OK],
				["vault token lookup -format=json", {
					code: 1,
					stdout: "",
					stderr: "Error looking up token: Code: 400. Errors:\n* missing client token",
				}],
			]));
			const result = await vault.check(pi);
			assert.equal(result.status, "none");
			assert.equal(result.secretHint, "VAULT_TOKEN");
		} finally {
			if (original !== undefined) process.env.VAULT_ADDR = original;
			else delete process.env.VAULT_ADDR;
		}
	});

	it("never includes VAULT_TOKEN value in any output", async () => {
		const original = process.env.VAULT_ADDR;
		const originalToken = process.env.VAULT_TOKEN;
		process.env.VAULT_ADDR = "https://vault.example.com:8200";
		process.env.VAULT_TOKEN = "s.supersecrettoken123";
		try {
			const pi = createMockPi(new Map([
				["which vault", OK],
				["vault token lookup -format=json", {
					code: 1,
					stdout: "",
					stderr: "Error: token has expired",
				}],
			]));
			const result = await vault.check(pi);
			const resultStr = JSON.stringify(result);
			assert.ok(
				!resultStr.includes("s.supersecrettoken123"),
				"VAULT_TOKEN value must never appear in auth result"
			);
		} finally {
			if (original !== undefined) process.env.VAULT_ADDR = original;
			else delete process.env.VAULT_ADDR;
			if (originalToken !== undefined) process.env.VAULT_TOKEN = originalToken;
			else delete process.env.VAULT_TOKEN;
		}
	});
});

// ─── checkAllProviders ──────────────────────────────────────────

describe("checkAllProviders", () => {
	it("catches provider errors and returns 'none' status", async () => {
		const pi = {
			exec() { throw new Error("exec crashed"); },
		} as any;
		const results = await checkAllProviders(pi);
		assert.equal(results.length, ALL_PROVIDERS.length);
		for (const r of results) {
			assert.equal(r.status, "none");
			assert.ok(r.detail.includes("Check failed"));
		}
	});

	it("uses provider.id (not name) in error fallback", async () => {
		const pi = {
			exec() { throw new Error("boom"); },
		} as any;
		const results = await checkAllProviders(pi);
		// All providers should report their id
		const providers = results.map(r => r.provider);
		for (const id of ["git", "github", "gitlab", "aws", "kubernetes", "oci", "vault"]) {
			assert.ok(providers.includes(id), `Expected id '${id}' in providers, got: ${providers}`);
		}
	});
});
