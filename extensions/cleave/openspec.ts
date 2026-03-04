/**
 * cleave/openspec — OpenSpec tasks.md parser.
 *
 * Parses OpenSpec's tasks.md format into ChildPlan[] for cleave execution.
 * OpenSpec tasks.md uses numbered, grouped tasks with checkboxes:
 *
 *   ## 1. Theme Infrastructure
 *   - [ ] 1.1 Create ThemeContext with light/dark state
 *   - [ ] 1.2 Add CSS custom properties for colors
 *
 *   ## 2. UI Components
 *   - [ ] 2.1 Create ThemeToggle component
 *   - [ ] 2.2 Add toggle to settings page
 *
 * Each top-level group (## N. Title) becomes a ChildPlan.
 * Subtasks within a group become the scope/description.
 * Group ordering defines dependencies (later groups may depend on earlier).
 */

import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join, basename } from "node:path";
import type { ChildPlan, SplitPlan } from "./types.js";

// ─── Types ──────────────────────────────────────────────────────────────────

export interface OpenSpecChange {
	/** Change directory name (e.g., "add-dark-mode") */
	name: string;
	/** Full path to the change directory */
	path: string;
	/** Whether tasks.md exists */
	hasTasks: boolean;
	/** Whether proposal.md exists */
	hasProposal: boolean;
	/** Whether design.md exists */
	hasDesign: boolean;
}

export interface TaskGroup {
	/** Group number (1-based) */
	number: number;
	/** Group title (e.g., "Theme Infrastructure") */
	title: string;
	/** Individual tasks within the group */
	tasks: Array<{
		id: string;      // e.g., "1.1"
		text: string;    // e.g., "Create ThemeContext with light/dark state"
		done: boolean;   // checkbox state
	}>;
}

// ─── Detection ──────────────────────────────────────────────────────────────

/**
 * Detect whether an OpenSpec workspace exists in the given repo.
 * Returns the path to openspec/ if found, null otherwise.
 */
export function detectOpenSpec(repoPath: string): string | null {
	const openspecDir = join(repoPath, "openspec");
	if (existsSync(openspecDir)) return openspecDir;
	return null;
}

/**
 * List active (non-archived) OpenSpec changes.
 */
export function listChanges(openspecDir: string): OpenSpecChange[] {
	const changesDir = join(openspecDir, "changes");
	if (!existsSync(changesDir)) return [];

	const entries = readdirSync(changesDir, { withFileTypes: true });
	const changes: OpenSpecChange[] = [];

	for (const entry of entries) {
		if (!entry.isDirectory() || entry.name === "archive") continue;

		const changePath = join(changesDir, entry.name);
		changes.push({
			name: entry.name,
			path: changePath,
			hasTasks: existsSync(join(changePath, "tasks.md")),
			hasProposal: existsSync(join(changePath, "proposal.md")),
			hasDesign: existsSync(join(changePath, "design.md")),
		});
	}

	return changes;
}

/**
 * Find changes that have tasks.md ready for execution.
 */
export function findExecutableChanges(openspecDir: string): OpenSpecChange[] {
	return listChanges(openspecDir).filter((c) => c.hasTasks);
}

// ─── Parsing ────────────────────────────────────────────────────────────────

/**
 * Parse an OpenSpec tasks.md into task groups.
 *
 * Supports formats:
 *   ## 1. Group Title
 *   - [ ] 1.1 Task description
 *   - [x] 1.2 Completed task
 *
 * Also handles unnumbered groups:
 *   ## Group Title
 *   - [ ] Task description
 */
export function parseTasksFile(content: string): TaskGroup[] {
	const groups: TaskGroup[] = [];
	let currentGroup: TaskGroup | null = null;

	const lines = content.split("\n");

	for (const line of lines) {
		// Match group header: ## 1. Title or ## Title
		const groupMatch = line.match(/^##\s+(?:(\d+)\.\s+)?(.+)$/);
		if (groupMatch) {
			if (currentGroup) groups.push(currentGroup);
			currentGroup = {
				number: groupMatch[1] ? parseInt(groupMatch[1], 10) : groups.length + 1,
				title: groupMatch[2].trim(),
				tasks: [],
			};
			continue;
		}

		// Match task item: - [ ] 1.1 Description or - [x] 1.2 Description
		const taskMatch = line.match(/^\s*-\s+\[([ xX])\]\s+(?:(\d+(?:\.\d+)?)\s+)?(.+)$/);
		if (taskMatch && currentGroup) {
			currentGroup.tasks.push({
				id: taskMatch[2] || `${currentGroup.number}.${currentGroup.tasks.length + 1}`,
				text: taskMatch[3].trim(),
				done: taskMatch[1] !== " ",
			});
			continue;
		}

		// Match unnumbered bullet task under a group: - Task text (no checkbox)
		const bulletMatch = line.match(/^\s*-\s+(?!\[)(.+)$/);
		if (bulletMatch && currentGroup) {
			currentGroup.tasks.push({
				id: `${currentGroup.number}.${currentGroup.tasks.length + 1}`,
				text: bulletMatch[1].trim(),
				done: false,
			});
		}
	}

	if (currentGroup) groups.push(currentGroup);
	return groups;
}

// ─── Conversion ─────────────────────────────────────────────────────────────

/**
 * Convert OpenSpec task groups to cleave ChildPlan[].
 *
 * Each group becomes a child. Dependencies are inferred from:
 * - Explicit markers in title: "after X", "requires X", "depends on X"
 * - Task text references to earlier group titles
 *
 * Groups where ALL tasks are already done are filtered out.
 *
 * Returns null if fewer than 2 executable groups (not worth cleaving).
 */
export function taskGroupsToChildPlans(groups: TaskGroup[]): ChildPlan[] | null {
	// Filter out groups where all tasks are done
	const activeGroups = groups.filter((g) =>
		g.tasks.length === 0 || g.tasks.some((t) => !t.done),
	);

	if (activeGroups.length < 2) return null;

	// Cap at 4 children (cleave limit)
	const effectiveGroups = activeGroups.length > 4 ? mergeSmallGroups(activeGroups, 4) : activeGroups;

	const plans: ChildPlan[] = effectiveGroups.map((group) => {
		const label = group.title
			.toLowerCase()
			.replace(/[^\w\s-]/g, "")
			.replace(/[\s_]+/g, "-")
			.replace(/-+/g, "-")
			.replace(/^-|-$/g, "")
			.slice(0, 40);

		const taskDescriptions = group.tasks
			.filter((t) => !t.done) // Skip already-completed tasks
			.map((t) => `- ${t.text}`);

		const description = taskDescriptions.length > 0
			? `${group.title}:\n${taskDescriptions.join("\n")}`
			: group.title;

		// Infer scope from task text: look for file paths and patterns
		const scope = inferScope(group.tasks.map((t) => t.text));

		return {
			label,
			description,
			scope,
			dependsOn: [] as string[],
		};
	});

	// Infer dependencies from explicit markers and title references
	inferDependencies(plans);

	return plans;
}

/**
 * Infer inter-group dependencies from explicit markers in descriptions.
 *
 * Looks for patterns like:
 * - "after <label>" or "after <title words>"
 * - "requires <label>"
 * - "depends on <label>"
 * - Task text referencing an earlier group's title
 */
function inferDependencies(plans: ChildPlan[]): void {
	const labelSet = new Set(plans.map((p) => p.label));

	for (let i = 0; i < plans.length; i++) {
		const text = plans[i].description.toLowerCase();

		for (let j = 0; j < plans.length; j++) {
			if (i === j) continue;
			const otherLabel = plans[j].label;
			// Convert label back to words for fuzzy matching: "database-layer" → "database layer"
			const otherWords = otherLabel.replace(/-/g, " ");

			// Explicit markers: "after X", "requires X", "depends on X"
			const markers = [
				`after ${otherLabel}`, `after ${otherWords}`,
				`requires ${otherLabel}`, `requires ${otherWords}`,
				`depends on ${otherLabel}`, `depends on ${otherWords}`,
			];

			if (markers.some((m) => text.includes(m))) {
				if (labelSet.has(otherLabel) && !plans[i].dependsOn.includes(otherLabel)) {
					plans[i].dependsOn.push(otherLabel);
				}
			}
		}
	}
}

/**
 * Full pipeline: read an OpenSpec change and convert to SplitPlan.
 *
 * Returns null if the change doesn't have tasks or has fewer than 2 groups.
 */
export function openspecChangeToSplitPlan(changePath: string): SplitPlan | null {
	const tasksPath = join(changePath, "tasks.md");
	if (!existsSync(tasksPath)) return null;

	const content = readFileSync(tasksPath, "utf-8");
	const groups = parseTasksFile(content);
	const children = taskGroupsToChildPlans(groups);
	if (!children) return null;

	// Read proposal for rationale if available
	let rationale = `From OpenSpec change: ${basename(changePath)}`;
	const proposalPath = join(changePath, "proposal.md");
	if (existsSync(proposalPath)) {
		const proposal = readFileSync(proposalPath, "utf-8");
		// Extract intent section
		const intentMatch = proposal.match(/##\s+Intent\s*\n([\s\S]*?)(?=\n##|$)/);
		if (intentMatch) {
			rationale = intentMatch[1].trim().slice(0, 200);
		}
	}

	return { children, rationale };
}

// ─── Helpers ────────────────────────────────────────────────────────────────

/**
 * Merge small groups to fit within maxGroups.
 * Combines the smallest adjacent groups until we're at the limit.
 */
function mergeSmallGroups(groups: TaskGroup[], maxGroups: number): TaskGroup[] {
	const result = [...groups];

	while (result.length > maxGroups) {
		// Find the smallest group by task count
		let smallestIdx = 0;
		let smallestSize = Infinity;
		for (let i = 0; i < result.length - 1; i++) {
			const combined = result[i].tasks.length + result[i + 1].tasks.length;
			if (combined < smallestSize) {
				smallestSize = combined;
				smallestIdx = i;
			}
		}

		// Merge with next group
		const merged: TaskGroup = {
			number: result[smallestIdx].number,
			title: `${result[smallestIdx].title} + ${result[smallestIdx + 1].title}`,
			tasks: [...result[smallestIdx].tasks, ...result[smallestIdx + 1].tasks],
		};
		result.splice(smallestIdx, 2, merged);
	}

	return result;
}

/**
 * Infer file scope patterns from task descriptions.
 * Looks for quoted paths, file extensions, and common patterns.
 */
function inferScope(taskTexts: string[]): string[] {
	const scope = new Set<string>();
	const combined = taskTexts.join("\n");

	// Backtick-quoted paths: `src/auth/login.ts`
	for (const m of combined.matchAll(/`([a-zA-Z0-9_./-]+\.[a-zA-Z0-9]+)`/g)) {
		scope.add(m[1]);
	}

	// Directory references: src/auth/, components/
	for (const m of combined.matchAll(/\b((?:src|lib|app|components|pages|api|tests?|spec)\/?[a-zA-Z0-9_/-]*)\b/g)) {
		const dir = m[1].replace(/\/$/, "");
		if (dir.includes("/")) {
			scope.add(dir + "/**");
		}
	}

	return [...scope].slice(0, 10); // Cap at 10 patterns
}
