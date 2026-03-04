/**
 * cleave/worktree — Git worktree operations for child isolation.
 *
 * Each child gets its own git worktree on a separate branch,
 * preventing file conflicts during parallel execution.
 * Completed branches are merged back to base after harvesting.
 */

import { mkdirSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import type { ExtensionAPI } from "@mariozechner/pi-coding-agent";

/** Base directory for cleave worktrees. */
const WORKTREE_HOME = join(homedir(), ".pi", "cleave", "wt");

export interface WorktreeInfo {
	path: string;
	branch: string;
}

/**
 * Get the current git branch name.
 */
export async function getCurrentBranch(
	pi: ExtensionAPI,
	repoPath: string,
): Promise<string> {
	const result = await pi.exec("git", ["branch", "--show-current"], {
		cwd: repoPath,
		timeout: 5_000,
	});
	const branch = result.stdout.trim();
	if (!branch) {
		// Detached HEAD — get the SHA
		const sha = await pi.exec("git", ["rev-parse", "--short", "HEAD"], {
			cwd: repoPath,
			timeout: 5_000,
		});
		return sha.stdout.trim();
	}
	return branch;
}

/**
 * Ensure the working tree is clean (no uncommitted changes).
 * Throws if dirty.
 */
export async function ensureCleanWorktree(
	pi: ExtensionAPI,
	repoPath: string,
): Promise<void> {
	const result = await pi.exec("git", ["status", "--porcelain"], {
		cwd: repoPath,
		timeout: 5_000,
	});
	if (result.stdout.trim()) {
		throw new Error(
			"Working tree has uncommitted changes. Commit or stash before cleaving.\n" +
			result.stdout.trim(),
		);
	}
}

/**
 * Create a git worktree for a child task.
 *
 * Creates a new branch from baseBranch and checks it out in a
 * worktree directory adjacent to the repo.
 */
export async function createWorktree(
	pi: ExtensionAPI,
	repoPath: string,
	childLabel: string,
	childId: number,
	baseBranch: string,
): Promise<WorktreeInfo> {
	const branch = `cleave/${childId}-${childLabel}`;
	// Worktree goes in ~/.pi/cleave/wt/ to avoid polluting the repo or its parent
	mkdirSync(WORKTREE_HOME, { recursive: true });
	const worktreePath = join(WORKTREE_HOME, `${childId}-${childLabel}`);

	// Delete branch if it already exists (leftover from a previous run)
	await pi.exec("git", ["branch", "-D", branch], {
		cwd: repoPath,
		timeout: 5_000,
	}).catch(() => {}); // ignore if it doesn't exist

	// Remove stale worktree path if it exists
	await pi.exec("rm", ["-rf", worktreePath], { timeout: 5_000 }).catch(() => {});

	// Create worktree with new branch from base
	const result = await pi.exec(
		"git",
		["worktree", "add", "-b", branch, worktreePath, baseBranch],
		{ cwd: repoPath, timeout: 30_000 },
	);

	if (result.code !== 0) {
		throw new Error(`Failed to create worktree: ${result.stderr}`);
	}

	return { path: worktreePath, branch };
}

/**
 * Merge a child's branch back into the base branch.
 *
 * Returns { success, conflictFiles } — does NOT abort on conflict,
 * leaving the merge state for manual resolution.
 */
export async function mergeBranch(
	pi: ExtensionAPI,
	repoPath: string,
	childBranch: string,
	baseBranch: string,
): Promise<{ success: boolean; conflictFiles: string[]; error?: string }> {
	// Checkout base branch
	let result = await pi.exec("git", ["checkout", baseBranch], {
		cwd: repoPath,
		timeout: 10_000,
	});
	if (result.code !== 0) {
		return { success: false, conflictFiles: [], error: `Failed to checkout ${baseBranch}: ${result.stderr}` };
	}

	// Attempt merge
	result = await pi.exec("git", ["merge", "--no-ff", childBranch, "-m", `merge: cleave child ${childBranch}`], {
		cwd: repoPath,
		timeout: 30_000,
	});

	if (result.code === 0) {
		return { success: true, conflictFiles: [] };
	}

	// Merge conflict — detect which files
	const statusResult = await pi.exec("git", ["diff", "--name-only", "--diff-filter=U"], {
		cwd: repoPath,
		timeout: 5_000,
	});
	const conflictFiles = statusResult.stdout.trim().split("\n").filter(Boolean);

	// Abort the merge to leave repo in clean state
	await pi.exec("git", ["merge", "--abort"], { cwd: repoPath, timeout: 5_000 }).catch(() => {});

	return {
		success: false,
		conflictFiles,
		error: `Merge conflict in ${conflictFiles.length} file(s)`,
	};
}

/**
 * Clean up all cleave worktrees and their branches.
 */
export async function cleanupWorktrees(
	pi: ExtensionAPI,
	repoPath: string,
): Promise<void> {
	// List worktrees
	const result = await pi.exec("git", ["worktree", "list", "--porcelain"], {
		cwd: repoPath,
		timeout: 5_000,
	});

	const lines = result.stdout.split("\n");
	const worktreePaths: string[] = [];

	for (const line of lines) {
		if (line.startsWith("worktree ")) {
			const wtPath = line.replace("worktree ", "").trim();
			// Match worktrees in the cleave wt directory or legacy .cleave-wt- locations
			if (wtPath.includes(WORKTREE_HOME) || wtPath.includes(".cleave-wt-")) {
				worktreePaths.push(wtPath);
			}
		}
	}

	// Remove each worktree
	for (const wtPath of worktreePaths) {
		await pi.exec("git", ["worktree", "remove", "--force", wtPath], {
			cwd: repoPath,
			timeout: 10_000,
		}).catch(() => {});
	}

	// Prune stale worktree references
	await pi.exec("git", ["worktree", "prune"], {
		cwd: repoPath,
		timeout: 5_000,
	}).catch(() => {});

	// Delete orphaned cleave/* branches
	const branchResult = await pi.exec(
		"git", ["branch", "--list", "cleave/*"],
		{ cwd: repoPath, timeout: 5_000 },
	);
	const branches = branchResult.stdout
		.split("\n")
		.map((b) => b.trim().replace(/^\*\s*/, ""))
		.filter(Boolean);
	for (const branch of branches) {
		await pi.exec("git", ["branch", "-D", branch], {
			cwd: repoPath,
			timeout: 5_000,
		}).catch(() => {});
	}
}
