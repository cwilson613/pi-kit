import type { ExtensionAPI } from "@styrene-lab/pi-coding-agent";

/**
 * Delete local git branches that are fully merged into HEAD.
 *
 * Safety rules:
 * - Never deletes `main` or `master`
 * - Never deletes the current HEAD branch
 * - Uses `git branch -d` (safe delete) — unmerged branches are skipped, not force-deleted
 * - Deduplicates input before processing
 * - Never touches remote refs
 */
export async function deleteMergedBranches(
	pi: ExtensionAPI,
	cwd: string,
	branches: string[],
): Promise<{ deleted: string[]; skipped: string[] }> {
	const deleted: string[] = [];
	const skipped: string[] = [];

	if (branches.length === 0) return { deleted, skipped };

	// Get current branch name
	let currentBranch = "";
	try {
		const r = await pi.exec("git", ["rev-parse", "--abbrev-ref", "HEAD"], { cwd, timeout: 5_000 });
		currentBranch = r.stdout.trim();
	} catch {
		// Can't determine HEAD — skip everything to be safe
		return { deleted, skipped: [...new Set(branches)] };
	}

	const PROTECTED = new Set(["main", "master"]);
	const seen = new Set<string>();

	for (const branch of branches) {
		if (seen.has(branch)) continue;
		seen.add(branch);

		// Skip protected and current branch
		if (PROTECTED.has(branch) || branch === currentBranch) {
			skipped.push(branch);
			continue;
		}

		// Verify fully merged into HEAD
		try {
			await pi.exec("git", ["merge-base", "--is-ancestor", branch, "HEAD"], { cwd, timeout: 5_000 });
		} catch {
			skipped.push(branch);
			continue;
		}

		// Safe delete
		try {
			await pi.exec("git", ["branch", "-d", branch], { cwd, timeout: 5_000 });
			deleted.push(branch);
		} catch {
			skipped.push(branch);
		}
	}

	return { deleted, skipped };
}
