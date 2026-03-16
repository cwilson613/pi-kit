import { afterEach, beforeEach, describe, it } from "node:test";
import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { execFileSync } from "node:child_process";

import {
	assertTrackedLifecycleArtifacts,
	detectMemoryTransportState,
	detectUntrackedLifecycleArtifacts,
	formatLifecycleArtifactError,
	formatMemoryTransportNotice,
	isDurableLifecycleArtifact,
	parsePorcelainZ,
} from "./lifecycle-files.ts";

describe("lifecycle-files", () => {
	describe("isDurableLifecycleArtifact", () => {
		it("matches docs and openspec paths", () => {
			assert.equal(isDurableLifecycleArtifact("docs/example.md"), true);
			assert.equal(isDurableLifecycleArtifact("openspec/changes/foo/tasks.md"), true);
			assert.equal(isDurableLifecycleArtifact("./openspec/baseline/x.md"), true);
		});

		it("excludes non-durable and transient cleave paths", () => {
			assert.equal(isDurableLifecycleArtifact("README.md"), false);
			assert.equal(isDurableLifecycleArtifact(".pi/cleave/run/state.json"), false);
			assert.equal(isDurableLifecycleArtifact("tmp/openspec-notes.md"), false);
		});
	});

	describe("parsePorcelainZ", () => {
		it("extracts only untracked entries", () => {
			const stdout = [
				"?? docs/new-node.md",
				" M openspec/changes/foo/tasks.md",
				"?? openspec/changes/foo/specs/bar.md",
			].join("\0") + "\0";
			assert.deepStrictEqual(parsePorcelainZ(stdout), [
				"docs/new-node.md",
				"openspec/changes/foo/specs/bar.md",
			]);
		});
	});

	describe("formatLifecycleArtifactError", () => {
		it("includes actionable resolution guidance", () => {
			const text = formatLifecycleArtifactError({
				untracked: ["docs/node.md", "openspec/changes/x/tasks.md"],
			});
			assert.match(text, /Untracked durable lifecycle artifacts detected/);
			assert.match(text, /docs\/node\.md/);
			assert.match(text, /openspec\/changes\/x\/tasks\.md/);
			assert.match(text, /git add/);
			assert.match(text, /move transient scratch artifacts outside docs\/ and openspec\//);
		});
	});

	describe("formatMemoryTransportNotice", () => {
		it("returns null when memory transport is clean", () => {
			assert.equal(formatMemoryTransportNotice({
				tracked: true,
				dirty: false,
				untracked: false,
				path: ".pi/memory/facts.jsonl",
			}), null);
		});

		it("reports memory transport drift separately from lifecycle blockers", () => {
			const text = formatMemoryTransportNotice({
				tracked: true,
				dirty: true,
				untracked: false,
				path: ".pi/memory/facts.jsonl",
			});
			assert.ok(text);
			assert.match(text!, /Memory transport drift detected/);
			assert.match(text!, /reported separately from durable lifecycle artifact blockers/);
			assert.match(text!, /\/memory export/);
		});
	});

	describe("git integration", () => {
		let tmpDir: string;

		beforeEach(() => {
			tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "lifecycle-files-"));
			execFileSync("git", ["init"], { cwd: tmpDir, encoding: "utf-8" });
			execFileSync("git", ["config", "user.name", "Test User"], { cwd: tmpDir, encoding: "utf-8" });
			execFileSync("git", ["config", "user.email", "test@example.com"], { cwd: tmpDir, encoding: "utf-8" });
			fs.writeFileSync(path.join(tmpDir, "README.md"), "# test\n");
			execFileSync("git", ["add", "README.md"], { cwd: tmpDir, encoding: "utf-8" });
			execFileSync("git", ["commit", "-m", "init"], { cwd: tmpDir, encoding: "utf-8" });
		});

		afterEach(() => {
			fs.rmSync(tmpDir, { recursive: true, force: true });
		});

		it("detects untracked docs and openspec artifacts", () => {
			fs.mkdirSync(path.join(tmpDir, "docs"), { recursive: true });
			fs.mkdirSync(path.join(tmpDir, "openspec", "changes", "x"), { recursive: true });
			fs.mkdirSync(path.join(tmpDir, ".pi", "cleave", "run"), { recursive: true });
			fs.writeFileSync(path.join(tmpDir, "docs", "node.md"), "# Node\n");
			fs.writeFileSync(path.join(tmpDir, "openspec", "changes", "x", "tasks.md"), "# Tasks\n");
			fs.writeFileSync(path.join(tmpDir, ".pi", "cleave", "run", "state.json"), "{}\n");

			assert.deepStrictEqual(detectUntrackedLifecycleArtifacts(tmpDir), [
				"docs/node.md",
				"openspec/changes/x/tasks.md",
			]);
		});

		it("passes when durable lifecycle artifacts are tracked", () => {
			fs.mkdirSync(path.join(tmpDir, "docs"), { recursive: true });
			fs.mkdirSync(path.join(tmpDir, "openspec", "changes", "x"), { recursive: true });
			fs.writeFileSync(path.join(tmpDir, "docs", "node.md"), "# Node\n");
			fs.writeFileSync(path.join(tmpDir, "openspec", "changes", "x", "tasks.md"), "# Tasks\n");
			execFileSync("git", ["add", "docs/node.md", "openspec/changes/x/tasks.md"], {
				cwd: tmpDir,
				encoding: "utf-8",
			});

			assert.doesNotThrow(() => assertTrackedLifecycleArtifacts(tmpDir));
		});

		it("throws with explicit guidance when durable artifacts remain untracked", () => {
			fs.mkdirSync(path.join(tmpDir, "docs"), { recursive: true });
			fs.writeFileSync(path.join(tmpDir, "docs", "node.md"), "# Node\n");

			assert.throws(
				() => assertTrackedLifecycleArtifacts(tmpDir),
				/error:|Untracked durable lifecycle artifacts detected/i,
			);
			try {
				assertTrackedLifecycleArtifacts(tmpDir);
				assert.fail("expected assertTrackedLifecycleArtifacts to throw");
			} catch (error) {
				const message = error instanceof Error ? error.message : String(error);
				assert.match(message, /docs\/node\.md/);
				assert.match(message, /git add/);
			}
		});

		it("classifies tracked facts.jsonl drift separately from lifecycle blockers", () => {
			fs.mkdirSync(path.join(tmpDir, ".pi", "memory"), { recursive: true });
			fs.writeFileSync(path.join(tmpDir, ".pi", "memory", "facts.jsonl"), '{"_type":"fact"}\n');
			execFileSync("git", ["add", ".pi/memory/facts.jsonl"], { cwd: tmpDir, encoding: "utf-8" });
			execFileSync("git", ["commit", "-m", "add facts"], { cwd: tmpDir, encoding: "utf-8" });
			fs.writeFileSync(path.join(tmpDir, ".pi", "memory", "facts.jsonl"), '{"_type":"fact","id":"x"}\n');

			assert.doesNotThrow(() => assertTrackedLifecycleArtifacts(tmpDir));
			assert.deepStrictEqual(detectUntrackedLifecycleArtifacts(tmpDir), []);
			assert.deepStrictEqual(detectMemoryTransportState(tmpDir), {
				tracked: true,
				dirty: true,
				untracked: false,
				path: ".pi/memory/facts.jsonl",
			});
		});
	});
});
