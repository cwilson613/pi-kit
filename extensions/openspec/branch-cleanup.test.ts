import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { deleteMergedBranches } from "./branch-cleanup.ts";
import type { ExtensionAPI } from "@styrene-lab/pi-coding-agent";

// ─── Mock pi factory ─────────────────────────────────────────────────────────

type ExecCall = { cmd: string; args: string[] };

function makePi(options: {
	currentBranch?: string;
	mergedBranches?: Set<string>;  // branches that pass is-ancestor
	deleteFailBranches?: Set<string>;  // branches where branch -d fails
	failGetHead?: boolean;
}): { pi: ExtensionAPI; calls: ExecCall[] } {
	const calls: ExecCall[] = [];
	const {
		currentBranch = "main",
		mergedBranches = new Set<string>(),
		deleteFailBranches = new Set<string>(),
		failGetHead = false,
	} = options;

	const pi = {
		exec: async (_cmd: string, args: string[], _opts?: unknown) => {
			calls.push({ cmd: args[0] ?? "", args });

			// git rev-parse --abbrev-ref HEAD
			if (args[0] === "rev-parse" && args[1] === "--abbrev-ref") {
				if (failGetHead) throw new Error("not a git repo");
				return { stdout: currentBranch + "\n", stderr: "", exitCode: 0 };
			}

			// git merge-base --is-ancestor <branch> HEAD
			if (args[0] === "merge-base" && args[1] === "--is-ancestor") {
				const branch = args[2] ?? "";
				if (!mergedBranches.has(branch)) throw new Error("not an ancestor");
				return { stdout: "", stderr: "", exitCode: 0 };
			}

			// git branch -d <branch>
			if (args[0] === "branch" && args[1] === "-d") {
				const branch = args[2] ?? "";
				if (deleteFailBranches.has(branch)) throw new Error("branch not fully merged");
				return { stdout: `Deleted branch ${branch}`, stderr: "", exitCode: 0 };
			}

			throw new Error(`Unexpected git call: ${args.join(" ")}`);
		},
	} as unknown as ExtensionAPI;

	return { pi, calls };
}

// ─── Tests ────────────────────────────────────────────────────────────────────

describe("deleteMergedBranches", () => {
	it("deletes branches that are fully merged", async () => {
		const { pi } = makePi({
			currentBranch: "main",
			mergedBranches: new Set(["feature/foo", "feature/bar"]),
		});

		const result = await deleteMergedBranches(pi, "/repo", ["feature/foo", "feature/bar"]);

		assert.deepEqual(result.deleted.sort(), ["feature/bar", "feature/foo"]);
		assert.deepEqual(result.skipped, []);
	});

	it("skips branches that fail the merge-base ancestry check", async () => {
		const { pi } = makePi({
			currentBranch: "main",
			mergedBranches: new Set(["feature/done"]),
		});

		const result = await deleteMergedBranches(pi, "/repo", ["feature/done", "feature/wip"]);

		assert.deepEqual(result.deleted, ["feature/done"]);
		assert.deepEqual(result.skipped, ["feature/wip"]);
	});

	it("unconditionally skips 'main'", async () => {
		const { pi, calls } = makePi({
			currentBranch: "develop",
			mergedBranches: new Set(["main"]),
		});

		const result = await deleteMergedBranches(pi, "/repo", ["main"]);

		assert.deepEqual(result.deleted, []);
		assert.deepEqual(result.skipped, ["main"]);
		// merge-base should never have been called for main
		const mergeBaseCalls = calls.filter((c) => c.args[0] === "merge-base");
		assert.equal(mergeBaseCalls.length, 0);
	});

	it("unconditionally skips 'master'", async () => {
		const { pi } = makePi({
			currentBranch: "develop",
			mergedBranches: new Set(["master"]),
		});

		const result = await deleteMergedBranches(pi, "/repo", ["master"]);

		assert.deepEqual(result.deleted, []);
		assert.deepEqual(result.skipped, ["master"]);
	});

	it("skips the current HEAD branch", async () => {
		const { pi, calls } = makePi({
			currentBranch: "feature/active",
			mergedBranches: new Set(["feature/active"]),
		});

		const result = await deleteMergedBranches(pi, "/repo", ["feature/active"]);

		assert.deepEqual(result.deleted, []);
		assert.deepEqual(result.skipped, ["feature/active"]);
		const mergeBaseCalls = calls.filter((c) => c.args[0] === "merge-base");
		assert.equal(mergeBaseCalls.length, 0);
	});

	it("returns empty lists for empty input without making git calls", async () => {
		const { pi, calls } = makePi({ currentBranch: "main" });

		const result = await deleteMergedBranches(pi, "/repo", []);

		assert.deepEqual(result.deleted, []);
		assert.deepEqual(result.skipped, []);
		assert.equal(calls.length, 0);
	});

	it("deduplicates input — processes each branch only once", async () => {
		const { pi, calls } = makePi({
			currentBranch: "main",
			mergedBranches: new Set(["feature/foo"]),
		});

		const result = await deleteMergedBranches(pi, "/repo", [
			"feature/foo", "feature/foo", "feature/foo",
		]);

		assert.deepEqual(result.deleted, ["feature/foo"]);
		assert.deepEqual(result.skipped, []);
		const deleteCalls = calls.filter((c) => c.args[0] === "branch" && c.args[1] === "-d");
		assert.equal(deleteCalls.length, 1);
	});

	it("skips (not throws) when git branch -d fails", async () => {
		const { pi } = makePi({
			currentBranch: "main",
			mergedBranches: new Set(["feature/foo"]),
			deleteFailBranches: new Set(["feature/foo"]),
		});

		const result = await deleteMergedBranches(pi, "/repo", ["feature/foo"]);

		assert.deepEqual(result.deleted, []);
		assert.deepEqual(result.skipped, ["feature/foo"]);
	});

	it("skips all branches when git rev-parse fails to determine HEAD", async () => {
		const { pi } = makePi({ failGetHead: true });

		const result = await deleteMergedBranches(pi, "/repo", ["feature/foo", "feature/bar"]);

		assert.deepEqual(result.deleted, []);
		assert.equal(result.skipped.sort().join(","), "feature/bar,feature/foo");
	});

	it("mixes deleted and skipped in a single call", async () => {
		const { pi } = makePi({
			currentBranch: "main",
			mergedBranches: new Set(["feature/done1", "feature/done2"]),
			// feature/wip: not merged; feature/done2: delete fails
			deleteFailBranches: new Set(["feature/done2"]),
		});

		const result = await deleteMergedBranches(pi, "/repo", [
			"feature/done1", "feature/done2", "feature/wip", "main",
		]);

		assert.deepEqual(result.deleted, ["feature/done1"]);
		assert.deepEqual(result.skipped.sort(), ["feature/done2", "feature/wip", "main"]);
	});
});
