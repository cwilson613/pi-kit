/**
 * Tests for cleave/review — adversarial review loop, severity gating, churn detection.
 */

import { describe, it } from "node:test";
import * as assert from "node:assert/strict";
import {
	buildReviewPrompt,
	buildFixPrompt,
	parseReviewResult,
	severityGate,
	detectChurn,
	executeWithReview,
	DEFAULT_REVIEW_CONFIG,
	type ReviewConfig,
	type ReviewIssue,
	type ReviewVerdict,
	type ReviewExecutor,
} from "./review.js";

// ─── buildReviewPrompt ─────────────────────────────────────────────────────

describe("buildReviewPrompt", () => {
	it("includes task content, root directive, and worktree path", () => {
		const prompt = buildReviewPrompt(
			"# Task\nBuild a REST API",
			"Implement microservice",
			"/tmp/worktree",
		);
		assert.ok(prompt.includes("Build a REST API"));
		assert.ok(prompt.includes("Implement microservice"));
		assert.ok(prompt.includes("/tmp/worktree"));
	});

	it("contains adversarial posture instructions", () => {
		const prompt = buildReviewPrompt("task", "directive", "/tmp");
		assert.ok(prompt.includes("hostile"));
		assert.ok(prompt.includes("VERDICT"));
		assert.ok(prompt.includes("ISSUES"));
	});

	it("instructs reviewer to check scope compliance", () => {
		const prompt = buildReviewPrompt("task", "directive", "/tmp");
		assert.ok(prompt.includes("Scope violations"));
	});

	it("instructs reviewer to check spec scenarios", () => {
		const prompt = buildReviewPrompt("task", "directive", "/tmp");
		assert.ok(prompt.includes("Spec scenario"));
	});
});

// ─── buildFixPrompt ─────────────────────────────────────────────────────────

describe("buildFixPrompt", () => {
	const issues: ReviewIssue[] = [
		{ id: "C1", severity: "critical", file: "api.ts", line: 42, description: "Missing null check" },
		{ id: "W1", severity: "warning", description: "Poor variable name" },
	];

	it("includes all issues with identifiers", () => {
		const prompt = buildFixPrompt("task content", "directive", issues, 1);
		assert.ok(prompt.includes("C1:"));
		assert.ok(prompt.includes("W1:"));
		assert.ok(prompt.includes("Missing null check"));
		assert.ok(prompt.includes("Poor variable name"));
	});

	it("includes file location when available", () => {
		const prompt = buildFixPrompt("task content", "directive", issues, 1);
		assert.ok(prompt.includes("[api.ts:42]"));
	});

	it("includes round number", () => {
		const prompt = buildFixPrompt("task content", "directive", issues, 2);
		assert.ok(prompt.includes("fix iteration 2"));
	});

	it("marks security issues", () => {
		const secIssues: ReviewIssue[] = [
			{ id: "C1", severity: "critical", description: "SQL injection", security: true },
		];
		const prompt = buildFixPrompt("task", "directive", secIssues, 1);
		assert.ok(prompt.includes("SECURITY"));
	});

	it("includes task content for context", () => {
		const prompt = buildFixPrompt("my task content here", "directive", issues, 1);
		assert.ok(prompt.includes("my task content here"));
	});
});

// ─── parseReviewResult ──────────────────────────────────────────────────────

describe("parseReviewResult", () => {
	it("parses PASS verdict with no issues", () => {
		const result = parseReviewResult("VERDICT: PASS\n\nISSUES:\n");
		assert.equal(result.status, "PASS");
		assert.equal(result.issues.length, 0);
	});

	it("parses NEEDS_REWORK with critical and warning issues", () => {
		const raw = [
			"VERDICT: NEEDS_REWORK",
			"",
			"ISSUES:",
			"C1: [api.ts:42] Missing null check on user input",
			"C2: [db.ts:99] SQL injection vulnerability",
			"W1: [utils.ts:15] Misleading function name",
			"N1: [types.ts:3] Unused import",
		].join("\n");

		const result = parseReviewResult(raw);
		assert.equal(result.status, "NEEDS_REWORK");
		assert.equal(result.issues.length, 4);

		const c1 = result.issues.find((i) => i.id === "C1");
		assert.ok(c1);
		assert.equal(c1.severity, "critical");
		assert.equal(c1.file, "api.ts");
		assert.equal(c1.line, 42);
		assert.equal(c1.description, "Missing null check on user input");

		const w1 = result.issues.find((i) => i.id === "W1");
		assert.ok(w1);
		assert.equal(w1.severity, "warning");
	});

	it("parses SECURITY tag on critical issues", () => {
		const raw = "VERDICT: REJECT\n\nISSUES:\nC1: SECURITY [auth.ts:10] Hardcoded API key";
		const result = parseReviewResult(raw);
		assert.equal(result.issues.length, 1);
		assert.equal(result.issues[0].security, true);
		assert.equal(result.issues[0].description, "Hardcoded API key");
	});

	it("parses issues without file/line info", () => {
		const raw = "VERDICT: NEEDS_REWORK\n\nISSUES:\nC1: Missing error handling throughout";
		const result = parseReviewResult(raw);
		assert.equal(result.issues.length, 1);
		assert.equal(result.issues[0].file, undefined);
		assert.equal(result.issues[0].line, undefined);
		assert.equal(result.issues[0].description, "Missing error handling throughout");
	});

	it("parses PASS_WITH_CONCERNS", () => {
		const raw = "VERDICT: PASS_WITH_CONCERNS\n\nISSUES:\nW1: [config.ts:5] Magic number";
		const result = parseReviewResult(raw);
		assert.equal(result.status, "PASS_WITH_CONCERNS");
		assert.equal(result.issues.length, 1);
	});

	it("defaults to NEEDS_REWORK when verdict is unparseable", () => {
		const raw = "Some rambling review without proper format\nC1: [foo.ts:1] bug";
		const result = parseReviewResult(raw);
		assert.equal(result.status, "NEEDS_REWORK");
		assert.equal(result.issues.length, 1);
	});

	it("adjusts PASS to NEEDS_REWORK when critical issues present", () => {
		const raw = "VERDICT: PASS\n\nISSUES:\nC1: [x.ts:1] Real bug here";
		const result = parseReviewResult(raw);
		assert.equal(result.status, "NEEDS_REWORK");
	});

	it("adjusts PASS to PASS_WITH_CONCERNS when warnings present", () => {
		const raw = "VERDICT: PASS\n\nISSUES:\nW1: [x.ts:1] Minor warning";
		const result = parseReviewResult(raw);
		assert.equal(result.status, "PASS_WITH_CONCERNS");
	});

	it("preserves raw output", () => {
		const raw = "VERDICT: PASS\nISSUES:\n";
		const result = parseReviewResult(raw);
		assert.equal(result.rawOutput, raw);
	});
});

// ─── severityGate ───────────────────────────────────────────────────────────

describe("severityGate", () => {
	const config: ReviewConfig = {
		enabled: true,
		maxWarningFixes: 1,
		maxCriticalFixes: 2,
		churnThreshold: 0.5,
	};

	it("accepts PASS verdict", () => {
		const verdict: ReviewVerdict = { status: "PASS", issues: [], rawOutput: "" };
		const gate = severityGate(verdict, 0, config);
		assert.equal(gate.action, "accept");
	});

	it("accepts when only nits are present", () => {
		const verdict: ReviewVerdict = {
			status: "PASS_WITH_CONCERNS",
			issues: [{ id: "N1", severity: "nit", description: "Minor style" }],
			rawOutput: "",
		};
		const gate = severityGate(verdict, 0, config);
		assert.equal(gate.action, "accept");
	});

	it("requests fix for warnings on round 0", () => {
		const verdict: ReviewVerdict = {
			status: "PASS_WITH_CONCERNS",
			issues: [{ id: "W1", severity: "warning", description: "Poor naming" }],
			rawOutput: "",
		};
		const gate = severityGate(verdict, 0, config);
		assert.equal(gate.action, "fix");
		if (gate.action === "fix") {
			assert.equal(gate.issues.length, 1);
		}
	});

	it("accepts warnings after maxWarningFixes rounds", () => {
		const verdict: ReviewVerdict = {
			status: "PASS_WITH_CONCERNS",
			issues: [{ id: "W1", severity: "warning", description: "Still bad naming" }],
			rawOutput: "",
		};
		const gate = severityGate(verdict, 1, config);
		assert.equal(gate.action, "accept");
	});

	it("requests fix for critical issues on round 0", () => {
		const verdict: ReviewVerdict = {
			status: "NEEDS_REWORK",
			issues: [
				{ id: "C1", severity: "critical", description: "Null deref" },
				{ id: "W1", severity: "warning", description: "Bad name" },
			],
			rawOutput: "",
		};
		const gate = severityGate(verdict, 0, config);
		assert.equal(gate.action, "fix");
		if (gate.action === "fix") {
			assert.equal(gate.issues.length, 2); // both critical and warning
		}
	});

	it("escalates critical issues after maxCriticalFixes rounds", () => {
		const verdict: ReviewVerdict = {
			status: "NEEDS_REWORK",
			issues: [{ id: "C1", severity: "critical", description: "Persistent bug" }],
			rawOutput: "",
		};
		const gate = severityGate(verdict, 2, config);
		assert.equal(gate.action, "escalate");
	});

	it("escalates immediately for security critical issues", () => {
		const verdict: ReviewVerdict = {
			status: "REJECT",
			issues: [{ id: "C1", severity: "critical", description: "SQL injection", security: true }],
			rawOutput: "",
		};
		const gate = severityGate(verdict, 0, config);
		assert.equal(gate.action, "escalate");
		if (gate.action === "escalate") {
			assert.ok(gate.reason.includes("Security"));
		}
	});

	it("escalates if ANY critical is security, even if others are not", () => {
		const verdict: ReviewVerdict = {
			status: "REJECT",
			issues: [
				{ id: "C1", severity: "critical", description: "Logic error" },
				{ id: "C2", severity: "critical", description: "Data leak", security: true },
			],
			rawOutput: "",
		};
		const gate = severityGate(verdict, 0, config);
		assert.equal(gate.action, "escalate");
	});

	it("escalates when NEEDS_REWORK but zero parseable issues (garbled output)", () => {
		const verdict: ReviewVerdict = {
			status: "NEEDS_REWORK",
			issues: [],
			rawOutput: "some garbled text with no parseable issues",
		};
		const gate = severityGate(verdict, 0, config);
		assert.equal(gate.action, "escalate");
		if (gate.action === "escalate") {
			assert.ok(gate.reason.includes("garbled"));
		}
	});

	it("escalates when REJECT but zero parseable issues", () => {
		const verdict: ReviewVerdict = {
			status: "REJECT",
			issues: [],
			rawOutput: "",
		};
		const gate = severityGate(verdict, 0, config);
		assert.equal(gate.action, "escalate");
	});
});

// ─── detectChurn ────────────────────────────────────────────────────────────

describe("detectChurn", () => {
	it("detects churn when >50% of issues reappear", () => {
		const prev: ReviewIssue[] = [
			{ id: "C1", severity: "critical", description: "Missing null check" },
			{ id: "W1", severity: "warning", description: "Bad variable name" },
			{ id: "W2", severity: "warning", description: "No error handling" },
		];
		const curr: ReviewIssue[] = [
			{ id: "C1", severity: "critical", description: "Missing null check" },
			{ id: "W1", severity: "warning", description: "Bad variable name" },
			{ id: "W3", severity: "warning", description: "New issue" },
		];
		const result = detectChurn(prev, curr, 0.5);
		assert.equal(result.churning, true);
		assert.ok(result.reappearanceRate > 0.5);
		assert.equal(result.reappeared.length, 2);
	});

	it("allows progress when issues are resolved", () => {
		const prev: ReviewIssue[] = [
			{ id: "C1", severity: "critical", description: "Missing null check" },
			{ id: "C2", severity: "critical", description: "Buffer overflow" },
			{ id: "W1", severity: "warning", description: "Bad name" },
			{ id: "W2", severity: "warning", description: "Dead code" },
		];
		const curr: ReviewIssue[] = [
			{ id: "W3", severity: "warning", description: "New minor issue" },
		];
		const result = detectChurn(prev, curr, 0.5);
		assert.equal(result.churning, false);
		assert.equal(result.reappearanceRate, 0);
		assert.equal(result.reappeared.length, 0);
	});

	it("returns no churn when previous round has no issues", () => {
		const result = detectChurn(
			[],
			[{ id: "C1", severity: "critical", description: "Bug" }],
			0.5,
		);
		assert.equal(result.churning, false);
		assert.equal(result.reappearanceRate, 0);
	});

	it("returns no churn when current round has no issues", () => {
		const result = detectChurn(
			[{ id: "C1", severity: "critical", description: "Bug" }],
			[],
			0.5,
		);
		assert.equal(result.churning, false);
	});

	it("normalizes descriptions for comparison (case, whitespace, punctuation)", () => {
		const prev: ReviewIssue[] = [
			{ id: "C1", severity: "critical", description: "Missing null-check!!!" },
		];
		const curr: ReviewIssue[] = [
			{ id: "C1", severity: "critical", description: "missing null check" },
		];
		const result = detectChurn(prev, curr, 0.5);
		assert.equal(result.churning, true);
		assert.equal(result.reappeared.length, 1);
	});

	it("uses custom threshold", () => {
		const prev: ReviewIssue[] = [
			{ id: "C1", severity: "critical", description: "Bug A" },
			{ id: "C2", severity: "critical", description: "Bug B" },
		];
		const curr: ReviewIssue[] = [
			{ id: "C1", severity: "critical", description: "Bug A" },
			{ id: "W1", severity: "warning", description: "New" },
			{ id: "W2", severity: "warning", description: "Also new" },
		];
		// 1/3 = 33% — below 0.5 but above 0.3
		const atLow = detectChurn(prev, curr, 0.3);
		assert.equal(atLow.churning, true);
		const atHigh = detectChurn(prev, curr, 0.5);
		assert.equal(atHigh.churning, false);
	});
});

// ─── executeWithReview ──────────────────────────────────────────────────────

describe("executeWithReview", () => {
	/**
	 * Create a mock executor that returns predefined review outputs
	 * for successive review calls.
	 */
	function mockExecutor(opts: {
		taskContent?: string;
		reviewOutputs: string[];
		executeExitCode?: number;
	}): ReviewExecutor {
		let reviewCallIndex = 0;
		return {
			execute: async () => ({
				exitCode: opts.executeExitCode ?? 0,
				stdout: "executed",
				stderr: "",
			}),
			review: async () => {
				const output = opts.reviewOutputs[reviewCallIndex] ?? "VERDICT: PASS\nISSUES:\n";
				reviewCallIndex++;
				return { exitCode: 0, stdout: output, stderr: "" };
			},
			readFile: () => opts.taskContent ?? "# Task\n\nDo the thing\n",
		};
	}

	it("skips review when disabled", async () => {
		const executor = mockExecutor({ reviewOutputs: [] });
		const result = await executeWithReview(
			executor,
			"/path/to/task.md",
			"directive",
			"/tmp/wt",
			{ ...DEFAULT_REVIEW_CONFIG, enabled: false },
		);
		assert.equal(result.finalDecision, "no_review");
		assert.equal(result.reviewHistory.length, 0);
	});

	it("accepts on PASS verdict after review", async () => {
		const executor = mockExecutor({
			reviewOutputs: ["VERDICT: PASS\n\nISSUES:\n"],
		});
		const result = await executeWithReview(
			executor,
			"/path/to/task.md",
			"directive",
			"/tmp/wt",
			{ ...DEFAULT_REVIEW_CONFIG, enabled: true },
		);
		assert.equal(result.finalDecision, "accepted");
		assert.equal(result.reviewHistory.length, 1);
		assert.equal(result.reviewHistory[0].verdict.status, "PASS");
	});

	it("runs one fix iteration for warnings then accepts", async () => {
		const executor = mockExecutor({
			reviewOutputs: [
				// Round 0: warnings
				"VERDICT: PASS_WITH_CONCERNS\n\nISSUES:\nW1: [x.ts:5] Bad naming",
				// Round 1 (after fix): clean
				"VERDICT: PASS\n\nISSUES:\n",
			],
		});
		const result = await executeWithReview(
			executor,
			"/path/to/task.md",
			"directive",
			"/tmp/wt",
			{ ...DEFAULT_REVIEW_CONFIG, enabled: true, maxWarningFixes: 1 },
		);
		assert.equal(result.finalDecision, "accepted");
		assert.equal(result.reviewHistory.length, 2);
	});

	it("accepts after warning fix budget exhausted", async () => {
		const executor = mockExecutor({
			reviewOutputs: [
				// Round 0: warnings
				"VERDICT: PASS_WITH_CONCERNS\n\nISSUES:\nW1: [x.ts:5] Bad naming",
				// Round 1 (after fix): still warnings — budget exhausted, accepts
				"VERDICT: PASS_WITH_CONCERNS\n\nISSUES:\nW1: [x.ts:5] Still bad naming",
			],
		});
		const result = await executeWithReview(
			executor,
			"/path/to/task.md",
			"directive",
			"/tmp/wt",
			{ ...DEFAULT_REVIEW_CONFIG, enabled: true, maxWarningFixes: 1 },
		);
		assert.equal(result.finalDecision, "accepted");
		assert.equal(result.reviewHistory.length, 2);
	});

	it("escalates on security critical issues", async () => {
		const executor = mockExecutor({
			reviewOutputs: [
				"VERDICT: REJECT\n\nISSUES:\nC1: SECURITY [auth.ts:1] Hardcoded credential",
			],
		});
		const result = await executeWithReview(
			executor,
			"/path/to/task.md",
			"directive",
			"/tmp/wt",
			{ ...DEFAULT_REVIEW_CONFIG, enabled: true },
		);
		assert.equal(result.finalDecision, "escalated");
		assert.ok(result.escalationReason?.includes("Security"));
		assert.equal(result.reviewHistory.length, 1);
	});

	it("escalates after max critical fix iterations", async () => {
		const executor = mockExecutor({
			reviewOutputs: [
				// Round 0: critical
				"VERDICT: NEEDS_REWORK\n\nISSUES:\nC1: [x.ts:1] Bug A",
				// Round 1 (after fix): still critical
				"VERDICT: NEEDS_REWORK\n\nISSUES:\nC1: [x.ts:1] Bug B",
				// Round 2 (after fix): still critical — escalate
				"VERDICT: NEEDS_REWORK\n\nISSUES:\nC1: [x.ts:1] Bug C",
			],
		});
		const result = await executeWithReview(
			executor,
			"/path/to/task.md",
			"directive",
			"/tmp/wt",
			{ ...DEFAULT_REVIEW_CONFIG, enabled: true, maxCriticalFixes: 2 },
		);
		assert.equal(result.finalDecision, "escalated");
		assert.equal(result.reviewHistory.length, 3); // round 0, 1, 2
	});

	it("detects churn and bails", async () => {
		const executor = mockExecutor({
			reviewOutputs: [
				// Round 0: criticals
				"VERDICT: NEEDS_REWORK\n\nISSUES:\nC1: [x.ts:1] Missing null check\nW1: [x.ts:5] Bad name\nW2: [x.ts:10] Dead code",
				// Round 1: same issues reappear (67% churn)
				"VERDICT: NEEDS_REWORK\n\nISSUES:\nC1: [x.ts:1] Missing null check\nW1: [x.ts:5] Bad name\nW3: [x.ts:20] New issue",
			],
		});
		const result = await executeWithReview(
			executor,
			"/path/to/task.md",
			"directive",
			"/tmp/wt",
			{ ...DEFAULT_REVIEW_CONFIG, enabled: true, maxCriticalFixes: 5, churnThreshold: 0.5 },
		);
		assert.equal(result.finalDecision, "escalated");
		assert.ok(result.escalationReason?.includes("Churn"));
		assert.equal(result.reviewHistory.length, 2);
	});

	it("skips review when initial execution fails", async () => {
		const executor = mockExecutor({
			executeExitCode: 1,
			reviewOutputs: ["VERDICT: PASS\n\nISSUES:\n"],
		});
		const result = await executeWithReview(
			executor,
			"/path/to/task.md",
			"directive",
			"/tmp/wt",
			{ ...DEFAULT_REVIEW_CONFIG, enabled: true },
		);
		assert.equal(result.finalDecision, "no_review");
		assert.equal(result.reviewHistory.length, 0);
		assert.equal(result.executeResult.exitCode, 1);
	});

	it("tracks review iterations correctly", async () => {
		const executor = mockExecutor({
			reviewOutputs: [
				// Round 0: criticals → fix
				"VERDICT: NEEDS_REWORK\n\nISSUES:\nC1: [x.ts:1] Bug",
				// Round 1: clean → accept
				"VERDICT: PASS\n\nISSUES:\n",
			],
		});
		const result = await executeWithReview(
			executor,
			"/path/to/task.md",
			"directive",
			"/tmp/wt",
			{ ...DEFAULT_REVIEW_CONFIG, enabled: true, maxCriticalFixes: 2 },
		);
		assert.equal(result.finalDecision, "accepted");
		assert.equal(result.reviewHistory.length, 2);
		assert.equal(result.reviewHistory[0].round, 0);
		assert.equal(result.reviewHistory[1].round, 1);
	});
});
