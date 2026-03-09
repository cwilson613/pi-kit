import type { ExtensionAPI } from "@mariozechner/pi-coding-agent";

import { sharedState, DASHBOARD_UPDATE_EVENT } from "../shared-state.ts";
import { debug } from "../debug.ts";
import { listChanges } from "./spec.ts";

/**
 * Emit OpenSpec state to sharedState for the unified dashboard.
 * Reads all active changes, maps to the dashboard shape, and fires
 * the dashboard:update event for re-render.
 */
export function emitOpenSpecState(cwd: string, pi: ExtensionAPI): void {
	try {
		const changes = listChanges(cwd);
		const mapped = changes.map((c) => {
			const artifacts: Array<"proposal" | "design" | "specs" | "tasks"> = [];
			if (c.hasProposal) artifacts.push("proposal");
			if (c.hasDesign) artifacts.push("design");
			if (c.hasSpecs) artifacts.push("specs");
			if (c.hasTasks) artifacts.push("tasks");
			const specDomains = c.specs.map((s) => s.domain).filter(Boolean);
			return {
				name: c.name,
				stage: c.stage || "proposal",
				tasksDone: c.doneTasks,
				tasksTotal: c.totalTasks,
				artifacts,
				specDomains,
				path: c.path,
			};
		});
		sharedState.openspec = { changes: mapped };
		debug("openspec", "emitState", { count: mapped.length, cwd });
		pi.events.emit(DASHBOARD_UPDATE_EVENT, { source: "openspec" });
	} catch (err) {
		debug("openspec", "emitState:error", { error: err instanceof Error ? err.message : String(err), cwd });
		// Non-fatal — don't break the extension if openspec dir is missing
	}
}
