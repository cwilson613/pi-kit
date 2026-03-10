import { afterEach, beforeEach, describe, it } from "node:test";
import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";

import type { ExtensionAPI } from "@mariozechner/pi-coding-agent";
import designTreeExtension from "./index.ts";
import { generateFrontmatter } from "./tree.ts";
import type { DesignNode } from "./types.ts";

interface RegisteredTool {
	name: string;
	execute: (...args: unknown[]) => Promise<unknown>;
}

function createFakePi() {
	const tools: RegisteredTool[] = [];
	const commands = new Map<string, unknown>();
	const eventHandlers = new Map<string, unknown[]>();
	return {
		tools,
		commands,
		events: {
			emit() {},
		},
		registerTool(tool: RegisteredTool) {
			tools.push(tool);
		},
		registerCommand(name: string, command: unknown) {
			commands.set(name, command);
		},
		registerMessageRenderer() {},
		on(event: string, handler: unknown) {
			const handlers = eventHandlers.get(event) ?? [];
			handlers.push(handler);
			eventHandlers.set(event, handlers);
		},
		async sendMessage() {},
	};
}

function writeDesignDoc(docsDir: string, id: string): void {
	const node: DesignNode = {
		id,
		title: `Test ${id}`,
		status: "decided",
		dependencies: [],
		related: [],
		tags: [],
		open_questions: [],
		branches: [],
		filePath: path.join(docsDir, `${id}.md`),
		lastModified: Date.now(),
	};
	const content = `${generateFrontmatter(node)}\n# ${node.title}\n\n## Overview\n\nTest node.\n`;
	fs.writeFileSync(node.filePath, content);
}

describe("design-tree lifecycle metadata", () => {
	let tmpDir: string;
	let pi: ReturnType<typeof createFakePi>;

	beforeEach(() => {
		tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "design-tree-index-"));
		const docsDir = path.join(tmpDir, "docs");
		const changeDir = path.join(tmpDir, "openspec", "changes", "my-change");
		fs.mkdirSync(docsDir, { recursive: true });
		fs.mkdirSync(changeDir, { recursive: true });
		fs.writeFileSync(path.join(changeDir, "proposal.md"), "# Proposal\n");
		writeDesignDoc(docsDir, "my-change");

		pi = createFakePi();
		designTreeExtension(pi as unknown as ExtensionAPI);
	});

	afterEach(() => {
		fs.rmSync(tmpDir, { recursive: true, force: true });
	});

	async function runTool(params: Record<string, unknown>) {
		const tool = pi.tools.find((entry) => entry.name === "design_tree");
		assert.ok(tool, "missing design_tree tool");
		const result = await tool.execute("tool-1", params, {} as never, () => {}, { cwd: tmpDir });
		return result as {
			details: {
				nodes: Array<{ lifecycle: { boundToOpenSpec: boolean } }>;
				node: { lifecycle: { boundToOpenSpec: boolean; reopenSignalTarget: string } };
			};
		};
	}

	it("reports fallback id-based OpenSpec bindings in list and node metadata", async () => {
		const listResult = await runTool({ action: "list" });
		assert.equal(listResult.details.nodes[0].lifecycle.boundToOpenSpec, true);

		const nodeResult = await runTool({ action: "node", node_id: "my-change" });
		assert.equal(nodeResult.details.node.lifecycle.boundToOpenSpec, true);
		assert.equal(nodeResult.details.node.lifecycle.reopenSignalTarget, "my-change");
	});
});
