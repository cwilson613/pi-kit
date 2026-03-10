/**
 * Design tree archive/lifecycle gate helpers.
 *
 * Centralizes OpenSpec ↔ design-tree binding truth so status surfaces,
 * reconciliation, and archive transitions all agree on whether a change is
 * bound to a design node.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { scanDesignDocs, writeNodeDocument, getNodeSections } from "../design-tree/tree.ts";
import type { DesignNode } from "../design-tree/types.ts";

export type OpenSpecBindingMatch = "explicit" | "id-fallback";

export interface OpenSpecBindingResolution {
	bound: boolean;
	changeName: string | null;
	match: OpenSpecBindingMatch | null;
}

function listKnownOpenSpecChangeNames(cwd: string): Set<string> {
	const names = new Set<string>();
	const openspecDir = path.join(cwd, "openspec");
	const changesDir = path.join(openspecDir, "changes");
	const archiveDir = path.join(openspecDir, "archive");

	if (fs.existsSync(changesDir)) {
		for (const entry of fs.readdirSync(changesDir, { withFileTypes: true })) {
			if (entry.isDirectory()) names.add(entry.name);
		}
	}

	if (fs.existsSync(archiveDir)) {
		for (const entry of fs.readdirSync(archiveDir, { withFileTypes: true })) {
			if (!entry.isDirectory()) continue;
			const match = entry.name.match(/^\d{4}-\d{2}-\d{2}-(.+)$/);
			names.add(match ? match[1] : entry.name);
		}
	}

	return names;
}

export function resolveNodeOpenSpecBinding(cwd: string, node: DesignNode): OpenSpecBindingResolution {
	const knownChangeNames = listKnownOpenSpecChangeNames(cwd);

	if (node.openspec_change) {
		if (knownChangeNames.has(node.openspec_change)) {
			return {
				bound: true,
				changeName: node.openspec_change,
				match: "explicit",
			};
		}
		return {
			bound: false,
			changeName: node.openspec_change,
			match: null,
		};
	}

	if (knownChangeNames.has(node.id)) {
		return {
			bound: true,
			changeName: node.id,
			match: "id-fallback",
		};
	}

	return {
		bound: false,
		changeName: null,
		match: null,
	};
}

export function resolveBoundDesignNodes(cwd: string, changeName: string): DesignNode[] {
	const docsDir = path.join(cwd, "docs");
	if (!fs.existsSync(docsDir)) return [];

	const tree = scanDesignDocs(docsDir);
	return Array.from(tree.nodes.values()).filter((node) => {
		const binding = resolveNodeOpenSpecBinding(cwd, node);
		return binding.bound && binding.changeName === changeName;
	});
}

/**
 * Scan the design tree for nodes matching the archived OpenSpec change.
 * Matches by explicit `openspec_change` frontmatter field OR by convention
 * (node ID = change name) using the shared binding resolver. Transitions
 * `implementing` or `decided` nodes to `implemented`.
 *
 * @param cwd     Project root (parent of the docs/ directory)
 * @param changeName  OpenSpec change name to match against
 * @returns IDs of nodes transitioned to implemented
 */
export function transitionDesignNodesOnArchive(cwd: string, changeName: string): string[] {
	const transitioned: string[] = [];

	for (const node of resolveBoundDesignNodes(cwd, changeName)) {
		const transitionable = node.status === "implementing" || node.status === "decided";
		if (!transitionable) continue;
		const sections = getNodeSections(node);
		writeNodeDocument({ ...node, status: "implemented" }, sections);
		transitioned.push(node.id);
	}
	return transitioned;
}
