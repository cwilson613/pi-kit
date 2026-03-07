/**
 * Design tree archive gate — transition implementing nodes to implemented on archive.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { scanDesignDocs, writeNodeDocument, getNodeSections } from "../design-tree/tree.ts";

/**
 * Scan the design tree for nodes matching the archived OpenSpec change.
 * Matches by explicit `openspec_change` frontmatter field OR by convention
 * (node ID = change name). Transitions `implementing` or `decided` nodes
 * to `implemented` — the decided fallback handles OpenSpec-first workflows
 * where the design tree `implement` action was never run.
 *
 * @param cwd     Project root (parent of the docs/ directory)
 * @param changeName  OpenSpec change name to match against
 * @returns IDs of nodes transitioned to implemented
 */
export function transitionDesignNodesOnArchive(cwd: string, changeName: string): string[] {
	const docsDir = path.join(cwd, "docs");
	if (!fs.existsSync(docsDir)) return [];

	const tree = scanDesignDocs(docsDir);
	const transitioned: string[] = [];

	for (const node of tree.nodes.values()) {
		// Match by explicit openspec_change field OR by convention (node ID = change name)
		const matches = node.openspec_change === changeName || node.id === changeName;
		// Transition implementing → implemented (primary path)
		// Also transition decided → implemented (fallback for OpenSpec-first workflows
		// where the design tree `implement` action was never run)
		const transitionable = node.status === "implementing" || node.status === "decided";
		if (matches && transitionable) {
			const sections = getNodeSections(node);
			writeNodeDocument({ ...node, status: "implemented" }, sections);
			transitioned.push(node.id);
		}
	}
	return transitioned;
}
