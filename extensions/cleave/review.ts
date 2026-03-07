/**
 * cleave/review — Adversarial review loop for child task quality gating.
 *
 * Implements the tiered execution loop (design D4):
 *   Execute (cheap) → Review (opus) → [pass? done : Fix (cheap) → Review (opus)]
 *
 * Severity gating (D4a):
 *   - Nits only → PASS, no fix needed
 *   - Warnings → 1 fix iteration max
 *   - Critical → 2 fix iterations, then escalate
 *   - Critical+security → immediate escalate, no fix
 *
 * Diminishing returns (D4b):
 *   - Hash issue descriptions between rounds
 *   - >50% reappearance → bail + escalate
 */

// ─── Types ──────────────────────────────────────────────────────────────────

/** Severity level for a review issue */
export type IssueSeverity = "critical" | "warning" | "nit";

/** A single issue found during review */
export interface ReviewIssue {
	/** Unique identifier within a review round, e.g. "C1", "W2", "N3" */
	id: string;
	/** Severity level */
	severity: IssueSeverity;
	/** File path where the issue was found */
	file?: string;
	/** Line number (approximate) */
	line?: number;
	/** Description of the issue */
	description: string;
	/** Whether this is a security or data-loss issue (triggers immediate escalation) */
	security?: boolean;
}

/** Overall verdict from a review round */
export type ReviewVerdictStatus = "PASS" | "PASS_WITH_CONCERNS" | "NEEDS_REWORK" | "REJECT";

/** Result of a single review round */
export interface ReviewVerdict {
	/** Overall status */
	status: ReviewVerdictStatus;
	/** Issues found, categorized by severity */
	issues: ReviewIssue[];
	/** Raw review output (for history/debugging) */
	rawOutput: string;
}

/** Configuration for the review loop */
export interface ReviewConfig {
	/** Whether review is enabled at all */
	enabled: boolean;
	/** Maximum fix iterations for warnings (default: 1) */
	maxWarningFixes: number;
	/** Maximum fix iterations for critical issues (default: 2) */
	maxCriticalFixes: number;
	/** Churn detection threshold — fraction of reappearing issues (default: 0.5) */
	churnThreshold: number;
}

/** A single review round entry for history tracking */
export interface ReviewRound {
	/** Round number (0 = initial review, 1+ = post-fix review) */
	round: number;
	/** The verdict from this round */
	verdict: ReviewVerdict;
	/** Issue IDs that reappeared from the previous round */
	reappeared: string[];
}

/** Default review configuration */
export const DEFAULT_REVIEW_CONFIG: ReviewConfig = {
	enabled: false,
	maxWarningFixes: 1,
	maxCriticalFixes: 2,
	churnThreshold: 0.5,
};

// ─── Prompt Builders ────────────────────────────────────────────────────────

/**
 * Build the adversarial review prompt for a child's work.
 *
 * The review agent runs in the child's worktree (D5) with full file access.
 * Prompt instructs a hostile reviewer to check: spec scenarios, bugs,
 * security, omissions, scope compliance (D4c).
 *
 * @param taskContent - The child's task file content (for scope/mission context)
 * @param rootDirective - The parent directive
 * @param worktreePath - Path to the child's worktree
 */
export function buildReviewPrompt(
	taskContent: string,
	rootDirective: string,
	worktreePath: string,
): string {
	return [
		"## Adversarial Code Review",
		"",
		"You are a hostile code reviewer. Your job is to find everything wrong with the implementation.",
		"Do not be polite. Do not hedge. If something is broken, say it's broken.",
		"",
		"### Context",
		"",
		`**Root directive:** ${rootDirective}`,
		`**Worktree:** ${worktreePath}`,
		"",
		"The following task file describes what was supposed to be implemented:",
		"",
		"```markdown",
		taskContent,
		"```",
		"",
		"### Review Procedure",
		"",
		"1. Read every file in the task's scope",
		"2. Check for:",
		"   - Logic errors, off-by-ones, wrong operators, unreachable branches",
		"   - Unhandled edge cases: null/undefined, empty inputs, boundary values",
		"   - Security: injection vectors, hardcoded secrets, path traversal",
		"   - Missing error handling, resource leaks",
		"   - Scope violations: modifications outside declared scope",
		"   - Spec scenario satisfaction (if acceptance criteria listed in task)",
		"3. Run tests if a test command is declared in the task file",
		"",
		"### Required Output Format",
		"",
		"You MUST output your review in exactly this format:",
		"",
		"```",
		"VERDICT: <PASS|PASS_WITH_CONCERNS|NEEDS_REWORK|REJECT>",
		"",
		"ISSUES:",
		"C1: [file.ts:42] Description of critical issue",
		"C2: [file.ts:99] Another critical issue",
		"C2: SECURITY - SQL injection in query builder",
		"W1: [file.ts:15] Warning description",
		"N1: [file.ts:3] Nit description",
		"```",
		"",
		"Rules:",
		"- Use C prefix for critical issues (bugs, logic errors, security, missing functionality)",
		"- Use W prefix for warnings (fragile patterns, missing edge cases, poor naming)",
		"- Use N prefix for nits (style, dead code, minor improvements)",
		"- Tag security/data-loss criticals with SECURITY after the ID",
		"- Include file path and line number in brackets where possible",
		"- If no issues found, output VERDICT: PASS with empty ISSUES section",
	].join("\n");
}

/**
 * Build a fix prompt from review issues.
 *
 * The fix agent (D4: same tier as execute) receives the issue list
 * verbatim with file paths and line numbers.
 */
export function buildFixPrompt(
	taskContent: string,
	rootDirective: string,
	issues: ReviewIssue[],
	round: number,
): string {
	const issueLines = issues.map((issue) => {
		const location = issue.file
			? `[${issue.file}${issue.line ? `:${issue.line}` : ""}]`
			: "";
		const security = issue.security ? " SECURITY" : "";
		return `${issue.id}:${security} ${location} ${issue.description}`;
	});

	return [
		"## Fix Issues from Code Review",
		"",
		`This is fix iteration ${round}. Address ALL issues listed below.`,
		"",
		"### Original Task",
		"",
		"```markdown",
		taskContent,
		"```",
		"",
		`**Root directive:** ${rootDirective}`,
		"",
		"### Issues to Fix",
		"",
		...issueLines,
		"",
		"### Instructions",
		"",
		"1. Fix each issue in order of severity (Critical first, then Warnings)",
		"2. Do NOT introduce new issues while fixing existing ones",
		"3. Run tests after fixing to verify nothing broke",
		"4. Update the task file's Result section if fixes change the summary",
		"5. Commit your fixes with a clear message",
	].join("\n");
}

// ─── Result Parsing ─────────────────────────────────────────────────────────

/**
 * Parse the review agent's output into a structured ReviewVerdict.
 *
 * Expects the format:
 *   VERDICT: <status>
 *   ISSUES:
 *   C1: [file:line] description
 *   W1: description
 *   N1: description
 *
 * Tolerant parser — handles missing fields and sloppy formatting.
 */
export function parseReviewResult(rawOutput: string): ReviewVerdict {
	// Extract verdict
	const verdictMatch = rawOutput.match(/VERDICT:\s*(PASS_WITH_CONCERNS|PASS|NEEDS_REWORK|REJECT)/i);
	let status: ReviewVerdictStatus = "NEEDS_REWORK"; // default if unparseable
	if (verdictMatch) {
		const raw = verdictMatch[1].toUpperCase().replace(/\s+/g, "_");
		if (raw === "PASS") status = "PASS";
		else if (raw === "PASS_WITH_CONCERNS") status = "PASS_WITH_CONCERNS";
		else if (raw === "NEEDS_REWORK") status = "NEEDS_REWORK";
		else if (raw === "REJECT") status = "REJECT";
	}

	// Extract issues
	const issues: ReviewIssue[] = [];
	// Match lines like: C1: [file.ts:42] description
	// or: C1: SECURITY [file.ts:42] description
	// or: C1: description (no file/line)
	const issueRegex = /^([CNW])(\d+):\s*(?:(SECURITY)\s*)?(?:\[([^\]]*)\]\s*)?(.+)$/gm;
	let match: RegExpExecArray | null;

	while ((match = issueRegex.exec(rawOutput)) !== null) {
		const prefix = match[1];
		const num = match[2];
		const securityTag = match[3];
		const locationStr = match[4];
		const description = match[5].trim();

		let severity: IssueSeverity;
		switch (prefix) {
			case "C": severity = "critical"; break;
			case "W": severity = "warning"; break;
			default: severity = "nit"; break;
		}

		let file: string | undefined;
		let line: number | undefined;
		if (locationStr) {
			const parts = locationStr.split(":");
			file = parts[0];
			if (parts.length > 1) {
				const lineNum = parseInt(parts[1], 10);
				if (!isNaN(lineNum)) line = lineNum;
			}
		}

		issues.push({
			id: `${prefix}${num}`,
			severity,
			file,
			line,
			description,
			security: securityTag === "SECURITY" || undefined,
		});
	}

	// If the verdict is PASS but there are critical/warning issues, adjust
	if (status === "PASS" && issues.some((i) => i.severity === "critical")) {
		status = "NEEDS_REWORK";
	} else if (status === "PASS" && issues.some((i) => i.severity === "warning")) {
		status = "PASS_WITH_CONCERNS";
	}

	return { status, issues, rawOutput };
}

// ─── Severity Gate ──────────────────────────────────────────────────────────

/**
 * Severity gate decision — determines what to do after a review round.
 *
 * Per design D4a:
 *   - Nits only → accept (PASS)
 *   - Warnings → allow up to maxWarningFixes iterations
 *   - Critical → allow up to maxCriticalFixes iterations
 *   - Critical+security → immediate escalate, no fix
 */
export type GateDecision =
	| { action: "accept" }
	| { action: "fix"; issues: ReviewIssue[] }
	| { action: "escalate"; reason: string };

export function severityGate(
	verdict: ReviewVerdict,
	currentRound: number,
	config: ReviewConfig,
): GateDecision {
	const criticals = verdict.issues.filter((i) => i.severity === "critical");
	const warnings = verdict.issues.filter((i) => i.severity === "warning");
	const securityCriticals = criticals.filter((i) => i.security);

	// PASS → accept
	if (verdict.status === "PASS") {
		return { action: "accept" };
	}

	// NEEDS_REWORK or REJECT with zero parseable issues — review output was garbled,
	// escalate rather than silently accepting
	if ((verdict.status === "NEEDS_REWORK" || verdict.status === "REJECT") && verdict.issues.length === 0) {
		return {
			action: "escalate",
			reason: `Review returned ${verdict.status} but no parseable issues — review output may be garbled.`,
		};
	}

	// Only nits (no criticals, no warnings) → accept
	if (criticals.length === 0 && warnings.length === 0) {
		return { action: "accept" };
	}

	// Critical+security → immediate escalate (D4a)
	if (securityCriticals.length > 0) {
		return {
			action: "escalate",
			reason: `Security/data-loss critical issues found: ${securityCriticals.map((i) => i.id).join(", ")}. Immediate escalation required.`,
		};
	}

	// Critical (non-security) → up to maxCriticalFixes iterations
	if (criticals.length > 0) {
		if (currentRound >= config.maxCriticalFixes) {
			return {
				action: "escalate",
				reason: `Critical issues remain after ${currentRound} fix iterations: ${criticals.map((i) => i.id).join(", ")}`,
			};
		}
		// Fix all critical and warning issues
		return { action: "fix", issues: [...criticals, ...warnings] };
	}

	// Warnings only → up to maxWarningFixes iterations
	if (warnings.length > 0) {
		if (currentRound >= config.maxWarningFixes) {
			// Exceeded warning fix budget — accept with concerns
			return { action: "accept" };
		}
		return { action: "fix", issues: warnings };
	}

	return { action: "accept" };
}

// ─── Churn Detection ────────────────────────────────────────────────────────

/**
 * Detect churn (diminishing returns) across review rounds.
 *
 * Per design D4b:
 *   - Normalize issue descriptions for comparison
 *   - Compute reappearance fraction (Jaccard-inspired)
 *   - If >threshold fraction of issues reappear → bail
 *
 * @returns Object with churn detected flag, reappearance rate, and reappeared issue IDs
 */
export function detectChurn(
	previousIssues: ReviewIssue[],
	currentIssues: ReviewIssue[],
	threshold: number = 0.5,
): { churning: boolean; reappearanceRate: number; reappeared: string[] } {
	if (previousIssues.length === 0 || currentIssues.length === 0) {
		return { churning: false, reappearanceRate: 0, reappeared: [] };
	}

	// Normalize descriptions for comparison (lowercase, punctuation→space, collapse whitespace)
	const normalize = (s: string): string =>
		s.toLowerCase().replace(/[^\w\s]/g, " ").replace(/\s+/g, " ").trim();

	const prevDescriptions = new Set(previousIssues.map((i) => normalize(i.description)));
	const reappeared: string[] = [];

	for (const issue of currentIssues) {
		if (prevDescriptions.has(normalize(issue.description))) {
			reappeared.push(issue.id);
		}
	}

	const reappearanceRate = reappeared.length / currentIssues.length;

	return {
		churning: reappearanceRate > threshold,
		reappearanceRate,
		reappeared,
	};
}

// ─── Execute With Review ────────────────────────────────────────────────────

/**
 * Callback interface for the review loop to spawn subprocesses.
 *
 * This abstraction allows testing without actual pi process spawning.
 */
export interface ReviewExecutor {
	/** Spawn the initial execution (or a fix iteration) */
	execute(prompt: string, cwd: string, modelFlag?: string): Promise<{ exitCode: number; stdout: string; stderr: string }>;
	/** Spawn a review of the completed work */
	review(prompt: string, cwd: string): Promise<{ exitCode: number; stdout: string; stderr: string }>;
	/** Read a file from the worktree */
	readFile(path: string): string;
}

/**
 * Execute a child task with optional adversarial review loop.
 *
 * Full flow (D4):
 *   1. Execute child task (cheap model)
 *   2. If review enabled: run adversarial review (opus)
 *   3. Severity gate determines next action:
 *      - ACCEPT → done
 *      - FIX → run fix agent (cheap model), then re-review
 *      - ESCALATE → report to orchestrator
 *   4. Churn detection between rounds → bail if not converging
 *
 * @returns History of review rounds + final decision
 */
export async function executeWithReview(
	executor: ReviewExecutor,
	taskFilePath: string,
	rootDirective: string,
	worktreePath: string,
	config: ReviewConfig,
	executeModelFlag?: string,
): Promise<{
	executeResult: { exitCode: number; stdout: string; stderr: string };
	reviewHistory: ReviewRound[];
	finalDecision: "accepted" | "escalated" | "no_review";
	escalationReason?: string;
}> {
	// Step 1: Initial execution
	const taskContent = executor.readFile(taskFilePath);
	const executeResult = await executor.execute(taskContent, worktreePath, executeModelFlag);

	// If review is disabled, return immediately
	if (!config.enabled) {
		return {
			executeResult,
			reviewHistory: [],
			finalDecision: "no_review",
		};
	}

	// If initial execution failed (timeout, crash), skip review — don't waste
	// an opus call reviewing an incomplete/empty worktree
	if (executeResult.exitCode !== 0) {
		return {
			executeResult,
			reviewHistory: [],
			finalDecision: "no_review",
		};
	}

	// Step 2+: Review loop
	const reviewHistory: ReviewRound[] = [];
	let previousIssues: ReviewIssue[] = [];
	let round = 0;

	while (true) {
		// Re-read task content (may have been updated by execute/fix)
		const currentTaskContent = executor.readFile(taskFilePath);

		// Run review
		const reviewPrompt = buildReviewPrompt(currentTaskContent, rootDirective, worktreePath);
		const reviewResult = await executor.review(reviewPrompt, worktreePath);

		// Parse review output
		const verdict = parseReviewResult(reviewResult.stdout);

		// Check for churn (D4b)
		let reappeared: string[] = [];
		if (round > 0 && previousIssues.length > 0) {
			const churnResult = detectChurn(previousIssues, verdict.issues, config.churnThreshold);
			reappeared = churnResult.reappeared;

			if (churnResult.churning) {
				reviewHistory.push({ round, verdict, reappeared });
				return {
					executeResult,
					reviewHistory,
					finalDecision: "escalated",
					escalationReason: `Churn detected: ${(churnResult.reappearanceRate * 100).toFixed(0)}% of issues reappeared (threshold: ${(config.churnThreshold * 100).toFixed(0)}%). Fix iterations are not converging.`,
				};
			}
		}

		// Record this review round
		reviewHistory.push({ round, verdict, reappeared });

		// Severity gate (D4a)
		const gate = severityGate(verdict, round, config);

		switch (gate.action) {
			case "accept":
				return {
					executeResult,
					reviewHistory,
					finalDecision: "accepted",
				};

			case "escalate":
				return {
					executeResult,
					reviewHistory,
					finalDecision: "escalated",
					escalationReason: gate.reason,
				};

			case "fix": {
				// Build fix prompt and dispatch fix agent
				const fixPrompt = buildFixPrompt(
					currentTaskContent,
					rootDirective,
					gate.issues,
					round + 1,
				);
				await executor.execute(fixPrompt, worktreePath, executeModelFlag);

				// Prepare for next review round
				previousIssues = verdict.issues;
				round++;
				break;
			}
		}
	}
}
