import * as path from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const DURABLE_ROOTS = ["docs", "openspec"] as const;
const MEMORY_TRANSPORT_PATH = ".pi/memory/facts.jsonl";

export interface LifecycleArtifactCheckResult {
	untracked: string[];
}

export interface MemoryTransportState {
	tracked: boolean;
	dirty: boolean;
	untracked: boolean;
	path: string;
}

export function isDurableLifecycleArtifact(filePath: string): boolean {
	const normalized = filePath.replaceAll("\\", "/").replace(/^\.\//, "");
	return DURABLE_ROOTS.some((root) => normalized === root || normalized.startsWith(`${root}/`));
}

export function parsePorcelainZ(stdout: string): string[] {
	const entries = stdout.split("\0").filter(Boolean);
	const untracked: string[] = [];
	for (const entry of entries) {
		if (entry.startsWith("?? ")) {
			untracked.push(entry.slice(3));
		}
	}
	return untracked;
}

export function detectUntrackedLifecycleArtifacts(repoPath: string): string[] {
	try {
		const stdout = execFileSync(
			"git",
			["status", "--porcelain", "--untracked-files=all", "-z", "--", ...DURABLE_ROOTS],
			{ cwd: repoPath, encoding: "utf-8" },
		);
		return parsePorcelainZ(stdout)
			.filter(isDurableLifecycleArtifact)
			.sort((a, b) => a.localeCompare(b));
	} catch {
		return [];
	}
}

export function detectMemoryTransportState(repoPath: string): MemoryTransportState {
	try {
		const stdout = execFileSync(
			"git",
			["status", "--porcelain", "--untracked-files=all", "--", MEMORY_TRANSPORT_PATH],
			{ cwd: repoPath, encoding: "utf-8" },
		).trim();
		if (!stdout) {
			return { tracked: true, dirty: false, untracked: false, path: MEMORY_TRANSPORT_PATH };
		}
		const line = stdout.split("\n").find(Boolean) ?? "";
		const normalized = line.replaceAll("\\", "/");
		if (normalized.startsWith("?? ")) {
			return { tracked: false, dirty: true, untracked: true, path: MEMORY_TRANSPORT_PATH };
		}
		return { tracked: true, dirty: true, untracked: false, path: MEMORY_TRANSPORT_PATH };
	} catch {
		return { tracked: true, dirty: false, untracked: false, path: MEMORY_TRANSPORT_PATH };
	}
}

export function formatLifecycleArtifactError(result: LifecycleArtifactCheckResult): string {
	const lines = [
		"Untracked durable lifecycle artifacts detected.",
		"",
		"The following files live under docs/ or openspec/ and are treated as version-controlled project documentation:",
		...result.untracked.map((file) => `- ${file}`),
		"",
		"Resolution:",
		"- git add the durable lifecycle files listed above, or",
		"- move transient scratch artifacts outside docs/ and openspec/.",
	];
	return lines.join("\n");
}

export function formatMemoryTransportNotice(state: MemoryTransportState): string | null {
	if (!state.dirty) return null;
	const lines = [
		"Memory transport drift detected.",
		"",
		`${state.path} differs from the live branch state, but this is reported separately from durable lifecycle artifact blockers.`,
		"",
		"Suggested resolution:",
		"- run `/memory export` if you intend to reconcile tracked memory transport, or",
		"- leave it alone if the drift is incidental branch-local memory state.",
	];
	if (state.untracked) {
		lines.splice(3, 0, `${state.path} is currently untracked.`);
	}
	return lines.join("\n");
}

export function assertTrackedLifecycleArtifacts(repoPath: string): void {
	const untracked = detectUntrackedLifecycleArtifacts(repoPath);
	if (untracked.length === 0) return;
	throw new Error(formatLifecycleArtifactError({ untracked }));
}

function runCli(): void {
	const repoPath = process.cwd();
	assertTrackedLifecycleArtifacts(repoPath);
	const memoryNotice = formatMemoryTransportNotice(detectMemoryTransportState(repoPath));
	if (memoryNotice) {
		process.stdout.write(`${memoryNotice}\n`);
	}
}

const isMain = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) {
	runCli();
}
