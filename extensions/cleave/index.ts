/**
 * cleave — Recursive task decomposition extension for pi.
 *
 * Provides:
 *   - `cleave_assess` tool: Assess directive complexity (LLM-callable)
 *   - `/cleave` command: Full decomposition workflow
 *
 * State machine: ASSESS → PLAN → CONFIRM → DISPATCH → HARVEST → REPORT
 *
 * Ported from styrene-lab/cleave (Python) — the pattern library, complexity
 * formula, conflict detection, and worktree management are preserved.
 * The Claude Code SDK calls are replaced with pi's extension API.
 */

import { StringEnum } from "@mariozechner/pi-ai";
import type { ExtensionAPI } from "@mariozechner/pi-coding-agent";
import { truncateTail, DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES, formatSize } from "@mariozechner/pi-coding-agent";
import { Type } from "@sinclair/typebox";

import { assessDirective, PATTERNS } from "./assessment.js";
import { detectConflicts, parseTaskResult } from "./conflicts.js";
import { dispatchChildren } from "./dispatcher.js";
import { detectOpenSpec, findExecutableChanges, openspecChangeToSplitPlan } from "./openspec.js";
import { buildPlannerPrompt, getRepoTree, parsePlanResponse } from "./planner.js";
import type { CleaveState, ChildState, SplitPlan } from "./types.js";
import { DEFAULT_CONFIG } from "./types.js";
import { initWorkspace, readTaskFiles, saveState } from "./workspace.js";
import {
	cleanupWorktrees,
	createWorktree,
	ensureCleanWorktree,
	getCurrentBranch,
	mergeBranch,
} from "./worktree.js";

// ─── Helpers ────────────────────────────────────────────────────────────────

function generateRunId(): string {
	return `clv-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 6)}`;
}

function formatAssessment(a: ReturnType<typeof assessDirective>): string {
	const lines = [
		"**Assessment**",
		"",
		`  Decision: **${a.decision}**`,
		`  Complexity: ${a.complexity}`,
		`  Systems: ${a.systems}`,
		`  Modifiers: ${a.modifiers.length > 0 ? a.modifiers.join(", ") : "none"}`,
		`  Method: ${a.method}`,
	];
	if (a.pattern) {
		lines.push(`  Pattern: ${a.pattern} (${(a.confidence * 100).toFixed(0)}%)`);
	}
	lines.push("", `  ${a.reasoning}`);
	return lines.join("\n");
}

function formatConflicts(conflicts: ReturnType<typeof detectConflicts>): string {
	if (conflicts.length === 0) return "No conflicts detected. ✓";
	return conflicts
		.map(
			(c, i) =>
				`**Conflict ${i + 1}:** ${c.type}\n` +
				`  ${c.description}\n` +
				`  Involved: tasks ${c.involved.join(", ")}\n` +
				`  Resolution: ${c.resolution}`,
		)
		.join("\n\n");
}

// ─── Extension ──────────────────────────────────────────────────────────────

export default function cleaveExtension(pi: ExtensionAPI) {

	// ── cleave_assess tool ───────────────────────────────────────────────
	pi.registerTool({
		name: "cleave_assess",
		label: "Cleave Assess",
		description:
			"Assess the complexity of a task directive to determine if it should be " +
			"decomposed (cleaved) into subtasks or executed directly. Returns complexity " +
			"score, matched pattern, confidence, and decision (execute/cleave).\n\n" +
			"Use before attempting complex multi-system tasks to decide whether decomposition is warranted.",
		promptSnippet:
			"Assess task complexity for decomposition — returns pattern match, complexity score, and execute/cleave decision",
		promptGuidelines: [
			"Call cleave_assess before starting any multi-system or cross-cutting task to determine if decomposition is needed",
			"If decision is 'execute', proceed directly. If 'cleave', use /cleave to decompose.",
			"Complexity formula: (1 + systems) × (1 + 0.5 × modifiers). Threshold default: 2.0.",
		],

		parameters: Type.Object({
			directive: Type.String({ description: "The task directive to assess" }),
			threshold: Type.Optional(Type.Number({ description: "Complexity threshold (default: 2.0)" })),
		}),

		async execute(_toolCallId, params, _signal, _onUpdate, _ctx) {
			const assessment = assessDirective(params.directive, params.threshold ?? DEFAULT_CONFIG.threshold);
			const text = formatAssessment(assessment);

			return {
				content: [{ type: "text", text }],
				details: {
					...assessment,
					availablePatterns: Object.values(PATTERNS).map((p) => p.name),
				},
			};
		},
	});

	// ── /cleave command ──────────────────────────────────────────────────
	pi.registerCommand("cleave", {
		description: "Recursive task decomposition (usage: /cleave <directive>)",
		handler: async (args, ctx) => {
			const directive = (args || "").trim();

			if (!directive) {
				pi.sendMessage({
					customType: "view",
					content: [
						"**Cleave — Recursive Task Decomposition**",
						"",
						"Usage: `/cleave <directive>`",
						"",
						"Example: `/cleave Implement JWT authentication with refresh tokens`",
						"",
						"The directive will be assessed for complexity. If it exceeds the",
						"threshold, it will be decomposed into 2-4 child tasks executed",
						"in parallel via git worktrees.",
						"",
						"Available patterns: " + Object.values(PATTERNS).map((p) => p.name).join(", "),
					].join("\n"),
					display: true,
				});
				return;
			}

			// Delegate the full workflow to the LLM via a structured prompt.
			// This allows the LLM to handle the interactive confirm gates
			// and adapt to user feedback, while we provide all the mechanical
			// infrastructure via tools.

			const assessment = assessDirective(directive);
			const assessmentText = formatAssessment(assessment);

			if (assessment.decision === "execute" || assessment.decision === "needs_assessment") {
				pi.sendMessage({
					customType: "view",
					content: [
						assessmentText,
						"",
						assessment.decision === "needs_assessment"
							? "**→ Execute directly** — no pattern matched; heuristic suggests in-session execution."
							: "**→ Execute directly** — complexity is below threshold.",
						"Proceeding with the task in-session.",
					].join("\n"),
					display: true,
				});

				// Hand off to the LLM to execute directly
				pi.sendUserMessage(
					`Execute this task directly (cleave assessment says it's simple enough):\n\n${directive}`,
					{ deliverAs: "followUp" },
				);
				return;
			}

			// Task needs cleaving — check for OpenSpec first, then fall back to LLM
			const repoPath = ctx.cwd;

			// ── OpenSpec fast path ─────────────────────────────────────
			const openspecDir = detectOpenSpec(repoPath);
			if (openspecDir) {
				const executableChanges = findExecutableChanges(openspecDir);
				if (executableChanges.length > 0) {
					// Try to find a change whose name matches the directive
					const directiveSlug = directive.toLowerCase().replace(/[^\w]+/g, "-");
					const matched = executableChanges.find((c) =>
						directiveSlug.includes(c.name) || c.name.includes(directiveSlug.slice(0, 20)),
					);

					// Only use OpenSpec if we found a matching change — never silently
					// pick an unrelated change
					if (!matched) {
						// No match — mention available changes but fall through to LLM planner
						pi.sendMessage({
							customType: "view",
							content: [
								`OpenSpec changes found but none matched the directive.`,
								`Available: ${executableChanges.map((c) => c.name).join(", ")}`,
								`Falling back to LLM planner.`,
							].join("\n"),
							display: true,
						});
					}
					const change = matched;
					const plan = change ? openspecChangeToSplitPlan(change.path) : null;

					if (plan) {
						const planJson = JSON.stringify(plan, null, 2);
						pi.sendMessage({
							customType: "view",
							content: [
								assessmentText,
								"",
								`**→ OpenSpec plan detected** from \`${change.name}/tasks.md\``,
								"",
								`**Rationale:** ${plan.rationale}`,
								`**Children:** ${plan.children.map((c) => c.label).join(", ")}`,
								"",
								"Review the plan and confirm to execute via `cleave_run`.",
							].join("\n"),
							display: true,
						});

						pi.sendUserMessage(
							[
								"## Cleave Decomposition (OpenSpec)",
								"",
								`OpenSpec change \`${change.name}\` provides a pre-built split plan.`,
								"",
								"### Split Plan",
								"",
								"```json",
								planJson,
								"```",
								"",
								"Present this plan to the user for review. After confirmation,",
								"use the `cleave_run` tool with this plan_json and the original directive.",
								"",
								"### Original Directive",
								"",
								directive,
							].join("\n"),
							{ deliverAs: "followUp" },
						);
						return;
					}
				}
			}

			// ── LLM planning fallback ──────────────────────────────────
			let repoTree: string;
			try {
				repoTree = await getRepoTree(pi, repoPath);
			} catch {
				repoTree = "(unable to read repo structure)";
			}

			const plannerPrompt = buildPlannerPrompt(directive, repoTree, []);

			pi.sendMessage({
				customType: "view",
				content: [
					assessmentText,
					"",
					"**→ Decomposition needed.** Generating split plan...",
				].join("\n"),
				display: true,
			});

			// Delegate to the LLM to:
			// 1. Generate a split plan (can use ask_local_model or think about it)
			// 2. Present the plan for confirmation
			// 3. Execute via cleave_run tool
			pi.sendUserMessage(
				[
					`## Cleave Decomposition`,
					"",
					`The directive needs decomposition (complexity ${assessment.complexity}, pattern: ${assessment.pattern || "none"}).`,
					"",
					"### Step 1: Generate a split plan",
					"",
					"Use `ask_local_model` with this planning prompt to generate a JSON split plan:",
					"",
					"```",
					plannerPrompt,
					"```",
					"",
					"Parse the JSON response and present the plan to me for review.",
					"",
					"### Step 2: After I confirm",
					"",
					"Use the `cleave_run` tool with the plan to execute the decomposition.",
					"",
					"### Original Directive",
					"",
					directive,
				].join("\n"),
				{ deliverAs: "followUp" },
			);
		},
	});

	// ── cleave_run tool ──────────────────────────────────────────────────
	pi.registerTool({
		name: "cleave_run",
		label: "Cleave Run",
		description:
			"Execute a cleave decomposition plan. Creates git worktrees for each child, " +
			"dispatches child pi processes, harvests results, detects conflicts, and " +
			"merges branches back. Requires a split plan (from cleave_assess + planning).\n\n" +
			"Each child runs in an isolated git worktree on its own branch.",
		parameters: Type.Object({
			directive: Type.String({ description: "The original task directive" }),
			plan_json: Type.String({
				description:
					'JSON string of the split plan: {"children": [{"label": "...", "description": "...", "scope": [...], "depends_on": [...]}], "rationale": "..."}',
			}),
			prefer_local: Type.Optional(
				Type.Boolean({ description: "Use local model for leaf tasks when possible (default: true)" }),
			),
			max_parallel: Type.Optional(
				Type.Number({ description: "Maximum parallel children (default: 4)" }),
			),
		}),

		async execute(_toolCallId, params, signal, onUpdate, ctx) {
			// Parse the plan
			let plan: SplitPlan;
			try {
				plan = parsePlanResponse(params.plan_json);
			} catch (e: any) {
				throw new Error(`Invalid split plan: ${e.message}`);
			}

			const repoPath = ctx.cwd;
			const maxParallel = params.max_parallel ?? DEFAULT_CONFIG.maxParallel;
			const preferLocal = params.prefer_local ?? DEFAULT_CONFIG.preferLocal;

			// ── PREFLIGHT ──────────────────────────────────────────────
			await ensureCleanWorktree(pi, repoPath);
			const baseBranch = await getCurrentBranch(pi, repoPath);

			// ── INITIALIZE STATE ───────────────────────────────────────
			const state: CleaveState = {
				runId: generateRunId(),
				phase: "dispatch",
				directive: params.directive,
				repoPath,
				baseBranch,
				assessment: assessDirective(params.directive),
				plan,
				children: plan.children.map((c, i) => ({
					childId: i,
					label: c.label,
					dependsOn: c.dependsOn,
					status: "pending" as const,
					branch: `cleave/${i}-${c.label}`,
					backend: preferLocal ? "local" as const : "cloud" as const,
				})),
				workspacePath: "",
				totalDurationSec: 0,
				createdAt: new Date().toISOString(),
			};

			// Create workspace
			const wsPath = initWorkspace(state, plan, repoPath);
			state.workspacePath = wsPath;

			// ── CREATE WORKTREES ───────────────────────────────────────
			onUpdate?.({
				content: [{ type: "text", text: "Creating git worktrees..." }],
				details: { phase: "dispatch", children: state.children },
			});

			for (const child of state.children) {
				try {
					const wt = await createWorktree(pi, repoPath, child.label, child.childId, baseBranch);
					child.worktreePath = wt.path;
					child.branch = wt.branch;
				} catch (e: any) {
					child.status = "failed";
					child.error = `Worktree creation failed: ${e.message}`;
				}
			}

			saveState(state);

			// ── DISPATCH ───────────────────────────────────────────────
			// Determine local model if preferLocal
			let localModel: string | undefined;
			if (preferLocal) {
				try {
					// Check for available local models via Ollama
					const ollamaResult = await pi.exec("ollama", ["list", "--json"], { timeout: 5_000 });
					if (ollamaResult.code === 0) {
						// Just use the first available model — the dispatcher will use it
						// for children marked as "local" backend
						const models = JSON.parse(ollamaResult.stdout);
						if (Array.isArray(models?.models) && models.models.length > 0) {
							localModel = models.models[0].name;
						}
					}
				} catch {
					// No local model available — all children go cloud
				}
			}

			onUpdate?.({
				content: [{ type: "text", text: `Dispatching ${state.children.length} children...` }],
				details: { phase: "dispatch", children: state.children },
			});

			await dispatchChildren(
				pi,
				state,
				maxParallel,
				120 * 60 * 1000, // 2 hour timeout per child
				localModel,
				signal ?? undefined,
				(msg) => {
					onUpdate?.({
						content: [{ type: "text", text: msg }],
						details: { phase: "dispatch", children: state.children },
					});
				},
			);

			// ── HARVEST + CONFLICTS ────────────────────────────────────
			state.phase = "harvest";
			saveState(state);

			const taskContents = readTaskFiles(wsPath);
			const taskResults = [...taskContents.entries()].map(([id, content]) =>
				parseTaskResult(content, `${id}-task.md`),
			);
			const conflicts = detectConflicts(taskResults);

			// ── MERGE ──────────────────────────────────────────────────
			state.phase = "reunify";
			saveState(state);

			const completedChildren = state.children.filter((c) => c.status === "completed");
			const mergeResults: Array<{ label: string; branch: string; success: boolean; conflicts: string[] }> = [];

			for (const child of completedChildren) {
				const result = await mergeBranch(pi, repoPath, child.branch, baseBranch);
				mergeResults.push({
					label: child.label,
					branch: child.branch,
					success: result.success,
					conflicts: result.conflictFiles,
				});
				// On merge failure, stop merging further children to avoid
				// compounding a partially-merged state
				if (!result.success) break;
			}

			// ── CLEANUP ────────────────────────────────────────────────
			// Only clean up worktrees if all merges succeeded. On merge
			// failure, preserve branches so the user can manually resolve.
			const mergeFailures = mergeResults.filter((m) => !m.success);
			if (mergeFailures.length === 0) {
				await cleanupWorktrees(pi, repoPath);
			} else {
				// Still prune worktree directories (they're copies), but keep the branches
				const wtResult = await pi.exec("git", ["worktree", "list", "--porcelain"], {
					cwd: repoPath, timeout: 5_000,
				});
				for (const line of wtResult.stdout.split("\n")) {
					if (line.startsWith("worktree ")) {
						const wtPath = line.replace("worktree ", "").trim();
						if (wtPath.includes("cleave") || wtPath.includes(".cleave-wt-")) {
							await pi.exec("git", ["worktree", "remove", "--force", wtPath], {
								cwd: repoPath, timeout: 10_000,
							}).catch(() => {});
						}
					}
				}
				await pi.exec("git", ["worktree", "prune"], { cwd: repoPath, timeout: 5_000 }).catch(() => {});
			}

			// ── REPORT ─────────────────────────────────────────────────
			state.phase = "complete";
			state.completedAt = new Date().toISOString();
			state.totalDurationSec = Math.round(
				(new Date(state.completedAt).getTime() - new Date(state.createdAt).getTime()) / 1000,
			);

			const allOk =
				state.children.every((c) => c.status === "completed") &&
				mergeResults.every((m) => m.success) &&
				conflicts.length === 0;

			if (!allOk) state.phase = "failed";
			saveState(state);

			// Build report
			const completedCount = state.children.filter((c) => c.status === "completed").length;
			const failedCount = state.children.filter((c) => c.status === "failed").length;

			const reportLines = [
				`## Cleave Report: ${state.runId}`,
				"",
				`**Directive:** ${params.directive}`,
				`**Status:** ${allOk ? "✓ SUCCESS" : "✗ ISSUES DETECTED"}`,
				`**Children:** ${completedCount} completed, ${failedCount} failed of ${state.children.length}`,
				`**Duration:** ${state.totalDurationSec}s`,
				`**Workspace:** \`${wsPath}\``,
				"",
			];

			// Child details
			for (const child of state.children) {
				const icon = child.status === "completed" ? "✓" : child.status === "failed" ? "✗" : "⏳";
				const dur = child.durationSec ? ` (${child.durationSec}s)` : "";
				reportLines.push(`  ${icon} **${child.label}** [${child.backend ?? "cloud"}]${dur}: ${child.status}`);
				if (child.error) reportLines.push(`    Error: ${child.error}`);
			}

			// Conflicts
			if (conflicts.length > 0) {
				reportLines.push("", "### Conflicts", "", formatConflicts(conflicts));
			}

			// Merge results (always show — makes partial merge state explicit)
			if (mergeResults.length > 0) {
				reportLines.push("", "### Merge Results");
				const mergeSuccesses = mergeResults.filter((m) => m.success);
				const notAttempted = completedChildren
					.filter((c) => !mergeResults.some((m) => m.label === c.label))
					.map((c) => c.label);
				for (const m of mergeSuccesses) {
					reportLines.push(`  ✓ ${m.label} merged`);
				}
				for (const m of mergeFailures) {
					reportLines.push(`  ✗ ${m.label}: conflicts in ${m.conflicts.join(", ")}`);
				}
				for (const label of notAttempted) {
					reportLines.push(`  ⏭ ${label}: skipped (earlier merge failed)`);
				}
			}

			const rawReport = reportLines.join("\n");
			const truncation = truncateTail(rawReport, {
				maxLines: DEFAULT_MAX_LINES,
				maxBytes: DEFAULT_MAX_BYTES,
			});
			let report = truncation.content;
			if (truncation.truncated) {
				report += `\n\n[Report truncated: ${truncation.outputLines} of ${truncation.totalLines} lines` +
					` (${formatSize(truncation.outputBytes)} of ${formatSize(truncation.totalBytes)})]`;
			}

			return {
				content: [{ type: "text", text: report }],
				details: {
					runId: state.runId,
					success: allOk,
					childrenCompleted: completedCount,
					childrenFailed: failedCount,
					conflictsFound: conflicts.length,
					mergeFailures: mergeFailures.length,
					workspacePath: wsPath,
				},
			};
		},
	});
}
