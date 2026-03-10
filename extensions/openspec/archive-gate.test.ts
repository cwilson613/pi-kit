import { afterEach, beforeEach, describe, it } from "node:test";
import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";

import { generateFrontmatter, scanDesignDocs } from "../design-tree/tree.ts";
import type { DesignNode } from "../design-tree/types.ts";
import {
	resolveBoundDesignNodes,
	resolveNodeOpenSpecBinding,
	transitionDesignNodesOnArchive,
} from "./archive-gate.ts";

function writeDesignDoc(docsDir: string, node: DesignNode): void {
	const content = `${generateFrontmatter(node)}\n# ${node.title}\n\n## Overview\n\nTest node.\n`;
	fs.writeFileSync(node.filePath, content);
}

describe("archive-gate binding resolution", () => {
	let tmpDir: string;
	let docsDir: string;

	beforeEach(() => {
		tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "openspec-archive-gate-"));
		docsDir = path.join(tmpDir, "docs");
		fs.mkdirSync(docsDir, { recursive: true });
		fs.mkdirSync(path.join(tmpDir, "openspec", "changes", "my-change"), { recursive: true });
		fs.writeFileSync(path.join(tmpDir, "openspec", "changes", "my-change", "proposal.md"), "# Proposal\n");
	});

	afterEach(() => {
		fs.rmSync(tmpDir, { recursive: true, force: true });
	});

	it("resolves explicit bindings against known changes", () => {
		const node: DesignNode = {
			id: "design-node",
			title: "Design Node",
			status: "implementing",
			dependencies: [],
			related: [],
			tags: [],
			open_questions: [],
			branches: [],
			openspec_change: "my-change",
			filePath: path.join(docsDir, "design-node.md"),
			lastModified: Date.now(),
		};
		writeDesignDoc(docsDir, node);

		const resolved = resolveNodeOpenSpecBinding(tmpDir, node);
		assert.equal(resolved.bound, true);
		assert.equal(resolved.changeName, "my-change");
		assert.equal(resolved.match, "explicit");
	});

	it("resolves fallback bindings when node id matches the change name", () => {
		const node: DesignNode = {
			id: "my-change",
			title: "My Change",
			status: "implementing",
			dependencies: [],
			related: [],
			tags: [],
			open_questions: [],
			branches: [],
			filePath: path.join(docsDir, "my-change.md"),
			lastModified: Date.now(),
		};
		writeDesignDoc(docsDir, node);

		const resolved = resolveNodeOpenSpecBinding(tmpDir, node);
		assert.equal(resolved.bound, true);
		assert.equal(resolved.changeName, "my-change");
		assert.equal(resolved.match, "id-fallback");
		assert.deepStrictEqual(resolveBoundDesignNodes(tmpDir, "my-change").map((entry) => entry.id), ["my-change"]);
	});

	it("transitions fallback-bound decided nodes on archive", () => {
		const node: DesignNode = {
			id: "my-change",
			title: "My Change",
			status: "decided",
			dependencies: [],
			related: [],
			tags: [],
			open_questions: [],
			branches: [],
			filePath: path.join(docsDir, "my-change.md"),
			lastModified: Date.now(),
		};
		writeDesignDoc(docsDir, node);

		const transitioned = transitionDesignNodesOnArchive(tmpDir, "my-change");
		assert.deepStrictEqual(transitioned, ["my-change"]);
		const tree = scanDesignDocs(docsDir);
		assert.equal(tree.nodes.get("my-change")?.status, "implemented");
	});
});
