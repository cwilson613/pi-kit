/**
 * Tests for OpenSpec archive gate — design tree node transitions on archive.
 */
import { describe, it, beforeEach, afterEach } from "node:test";
import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import * as os from "node:os";

import { scanDesignDocs, setNodeStatus, generateFrontmatter } from "../design-tree/tree.ts";
import type { DesignNode } from "../design-tree/types.ts";
import { transitionDesignNodesOnArchive } from "./archive-gate.ts";

function writeDesignDoc(docsDir: string, id: string, status: string, openspecChange?: string): void {
	const node: any = {
		id,
		title: `Test ${id}`,
		status,
		dependencies: [],
		related: [],
		tags: [],
		open_questions: [],
		branches: [],
		openspec_change: openspecChange,
	};
	const fm = generateFrontmatter(node);
	const content = fm + `\n# ${node.title}\n\n## Overview\n\nTest node.\n`;
	fs.writeFileSync(path.join(docsDir, `${id}.md`), content);
}

describe("archive gate — design tree transitions", () => {
	let tmpDir: string;
	let docsDir: string;

	beforeEach(() => {
		tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "archive-gate-"));
		docsDir = path.join(tmpDir, "docs");
		fs.mkdirSync(docsDir, { recursive: true });
	});

	afterEach(() => {
		fs.rmSync(tmpDir, { recursive: true, force: true });
	});

	it("transitions implementing node to implemented on matching change name", () => {
		writeDesignDoc(docsDir, "my-feature", "implementing", "my-feature");
		const result = transitionDesignNodesOnArchive(tmpDir, "my-feature");
		assert.deepStrictEqual(result, ["my-feature"]);

		// Verify the file was updated
		const tree = scanDesignDocs(docsDir);
		assert.strictEqual(tree.nodes.get("my-feature")?.status, "implemented");
	});

	it("does not transition nodes with different openspec_change", () => {
		writeDesignDoc(docsDir, "node-a", "implementing", "other-change");
		const result = transitionDesignNodesOnArchive(tmpDir, "my-feature");
		assert.deepStrictEqual(result, []);

		const tree = scanDesignDocs(docsDir);
		assert.strictEqual(tree.nodes.get("node-a")?.status, "implementing");
	});

	it("transitions decided nodes with matching change (OpenSpec-first workflow)", () => {
		writeDesignDoc(docsDir, "node-b", "decided", "my-feature");
		const result = transitionDesignNodesOnArchive(tmpDir, "my-feature");
		assert.deepStrictEqual(result, ["node-b"]);

		const tree = scanDesignDocs(docsDir);
		assert.strictEqual(tree.nodes.get("node-b")?.status, "implemented");
	});

	it("transitions multiple implementing nodes", () => {
		writeDesignDoc(docsDir, "node-1", "implementing", "big-change");
		writeDesignDoc(docsDir, "node-2", "implementing", "big-change");
		writeDesignDoc(docsDir, "node-3", "decided", "big-change");

		const result = transitionDesignNodesOnArchive(tmpDir, "big-change");
		assert.strictEqual(result.length, 3);
		assert.ok(result.includes("node-1"));
		assert.ok(result.includes("node-2"));
		assert.ok(result.includes("node-3"));
	});

	it("returns empty when docs dir does not exist", () => {
		const result = transitionDesignNodesOnArchive("/nonexistent/path", "anything");
		assert.deepStrictEqual(result, []);
	});

	it("returns empty when no nodes have openspec_change and ID doesn't match", () => {
		writeDesignDoc(docsDir, "plain-node", "implementing");
		const result = transitionDesignNodesOnArchive(tmpDir, "my-feature");
		assert.deepStrictEqual(result, []);
	});

	it("transitions by convention when node ID matches change name (no openspec_change field)", () => {
		writeDesignDoc(docsDir, "my-feature", "decided");
		const result = transitionDesignNodesOnArchive(tmpDir, "my-feature");
		assert.deepStrictEqual(result, ["my-feature"]);

		const tree = scanDesignDocs(docsDir);
		assert.strictEqual(tree.nodes.get("my-feature")?.status, "implemented");
	});

	it("does not transition seed or exploring nodes even with matching ID", () => {
		writeDesignDoc(docsDir, "my-feature", "exploring");
		const result = transitionDesignNodesOnArchive(tmpDir, "my-feature");
		assert.deepStrictEqual(result, []);

		const tree = scanDesignDocs(docsDir);
		assert.strictEqual(tree.nodes.get("my-feature")?.status, "exploring");
	});
});
