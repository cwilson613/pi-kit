/**
 * lib/git-state — Shared helpers for inspecting git dirty-tree state.
 *
 * Pure helpers only: parse porcelain output, separate tracked/untracked files,
 * classify built-in volatile artifacts, and prepare checkpoint/stash plans
 * without executing git mutations.
 */

export const BUILTIN_VOLATILE_ALLOWLIST = [".pi/memory/facts.jsonl"] as const;

export type GitStatusCode =
	| "modified"
	| "added"
	| "deleted"
	| "renamed"
	| "copied"
	| "unmerged"
	| "untracked"
	| "ignored"
	| "unknown";

export interface GitStatusEntry {
	path: string;
	code: string;
	indexStatus: string;
	worktreeStatus: string;
	tracked: boolean;
	untracked: boolean;
	staged: boolean;
	unstaged: boolean;
	deleted: boolean;
	renamed: boolean;
	copied: boolean;
	originalPath?: string;
	indexKind: GitStatusCode;
	worktreeKind: GitStatusCode;
}

export interface GitStateSnapshot {
	entries: GitStatusEntry[];
	tracked: GitStatusEntry[];
	untracked: GitStatusEntry[];
	volatile: GitStatusEntry[];
	nonVolatile: GitStatusEntry[];
}

export interface GitPathsetPlan {
	paths: string[];
	includesTracked: boolean;
	includesUntracked: boolean;
	pathspec: string[];
}

export interface GitPathsetInput {
	tracked?: string[];
	untracked?: string[];
}

export interface PreparedStashPlan extends GitPathsetPlan {
	kind: "stash";
	label: string;
	includeUntracked: boolean;
	command: string[];
}

export interface PreparedCheckpointPlan extends GitPathsetPlan {
	kind: "checkpoint";
	message: string;
	command: string[];
	requiresApproval: true;
}

export function parseGitStatus(output: string): GitStatusEntry[] {
	return output
		.split("\n")
		.map((line) => line.trimEnd())
		.filter(Boolean)
		.map(parseGitStatusLine);
}

export function inspectGitState(
	output: string,
	volatileAllowlist: readonly string[] = BUILTIN_VOLATILE_ALLOWLIST,
): GitStateSnapshot {
	const entries = parseGitStatus(output);
	const tracked = entries.filter((entry) => entry.tracked);
	const untracked = entries.filter((entry) => entry.untracked);
	const volatile = entries.filter((entry) => isVolatilePath(entry.path, volatileAllowlist));
	return {
		entries,
		tracked,
		untracked,
		volatile,
		nonVolatile: entries.filter((entry) => !isVolatilePath(entry.path, volatileAllowlist)),
	};
}

export function buildPathsetFromEntries(entries: GitStatusEntry[]): GitPathsetInput {
	return {
		tracked: entries.filter((entry) => entry.tracked).map((entry) => entry.path),
		untracked: entries.filter((entry) => entry.untracked).map((entry) => entry.path),
	};
}

export function isVolatilePath(
	path: string,
	volatileAllowlist: readonly string[] = BUILTIN_VOLATILE_ALLOWLIST,
): boolean {
	return volatileAllowlist.some((allowed) => pathMatchesAllowlist(path, allowed));
}

export function prepareStashPlan(input: {
	paths?: string[];
	tracked?: string[];
	untracked?: string[];
	label: string;
	includesUntracked?: boolean;
}): PreparedStashPlan {
	const pathset = prepareGitPathset({
		tracked: input.paths ?? input.tracked,
		untracked: input.untracked,
	});
	const includeUntracked = input.includesUntracked ?? pathset.includesUntracked;
	const command = ["git", "stash", "push", "-m", input.label];
	if (includeUntracked) command.push("--include-untracked");
	command.push(...pathset.pathspec);
	return {
		kind: "stash",
		label: input.label,
		includeUntracked,
		command,
		...pathset,
	};
}

export function prepareCheckpointPlan(input: {
	paths?: string[];
	tracked?: string[];
	untracked?: string[];
	message: string;
}): PreparedCheckpointPlan {
	const pathset = prepareGitPathset({
		tracked: input.paths ?? input.tracked,
		untracked: input.untracked,
	});
	return {
		kind: "checkpoint",
		message: input.message,
		requiresApproval: true,
		command: ["git", "commit", "-m", input.message],
		...pathset,
	};
}

export function prepareGitPathset(input: string[] | GitPathsetInput): GitPathsetPlan {
	const tracked = Array.isArray(input) ? input : (input.tracked ?? []);
	const untracked = Array.isArray(input) ? [] : (input.untracked ?? []);
	const uniquePaths = [...new Set([...tracked, ...untracked].filter(Boolean))].sort();
	return {
		paths: uniquePaths,
		includesTracked: tracked.length > 0,
		includesUntracked: untracked.length > 0,
		pathspec: uniquePaths.length > 0 ? ["--", ...uniquePaths] : [],
	};
}

function parseGitStatusLine(line: string): GitStatusEntry {
	const indexStatus = line[0] ?? " ";
	const worktreeStatus = line[1] ?? " ";
	const payload = line.slice(3);
	const renameParts = payload.split(" -> ");
	const path = renameParts[renameParts.length - 1] ?? payload;
	const originalPath = renameParts.length === 2 ? renameParts[0] : undefined;
	const tracked = indexStatus !== "?" && worktreeStatus !== "?";
	const untracked = indexStatus === "?" && worktreeStatus === "?";
	return {
		path,
		code: `${indexStatus}${worktreeStatus}`,
		indexStatus,
		worktreeStatus,
		tracked,
		untracked,
		staged: isChangedStatus(indexStatus),
		unstaged: isChangedStatus(worktreeStatus),
		deleted: indexStatus === "D" || worktreeStatus === "D",
		renamed: indexStatus === "R" || worktreeStatus === "R",
		copied: indexStatus === "C" || worktreeStatus === "C",
		originalPath,
		indexKind: decodeStatus(indexStatus),
		worktreeKind: decodeStatus(worktreeStatus),
	};
}

function decodeStatus(status: string): GitStatusCode {
	switch (status) {
		case "M":
			return "modified";
		case "A":
			return "added";
		case "D":
			return "deleted";
		case "R":
			return "renamed";
		case "C":
			return "copied";
		case "U":
			return "unmerged";
		case "?":
			return "untracked";
		case "!":
			return "ignored";
		case " ":
			return "unknown";
		default:
			return "unknown";
	}
}

function isChangedStatus(status: string): boolean {
	return status !== " " && status !== "?" && status !== "!";
}

function pathMatchesAllowlist(path: string, allowed: string): boolean {
	if (path === allowed) return true;
	if (allowed.endsWith("/")) return path.startsWith(allowed);
	return false;
}
