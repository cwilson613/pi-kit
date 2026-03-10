/**
 * Regression tests for openspec/dashboard-state — shared refresh helper.
 *
 * Verifies that:
 * - emitOpenSpecState writes dashboard-facing OpenSpec state to sharedState
 * - it fires the DASHBOARD_UPDATE_EVENT via pi.events.emit
 * - callers never need to duplicate dashboard refresh boilerplate inline
 * - the helper is non-fatal when the openspec directory is missing
 */

import { describe, it, beforeEach } from "node:test";
import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";

import { emitOpenSpecState } from "./dashboard-state.ts";
import { sharedState, DASHBOARD_UPDATE_EVENT } from "../shared-state.ts";
import { createChange } from "./spec.ts";

// ─── Helpers ─────────────────────────────────────────────────────────────────

function makeTmpDir(): string {
	return fs.mkdtempSync(path.join(os.tmpdir(), "openspec-dashboard-test-"));
}

function createFakePi() {
	const emitted: Array<{ channel: string; data: unknown }> = [];
	return {
		emitted,
		events: {
			emit(channel: string, data: unknown) {
				emitted.push({ channel, data });
			},
		},
	};
}

// ─── Tests ───────────────────────────────────────────────────────────────────

describe("emitOpenSpecState — shared refresh helper", () => {
	let tmpDir: string;

	beforeEach(() => {
		tmpDir = makeTmpDir();
		// Reset openspec slice of sharedState before each test
		sharedState.openspec = undefined as any;
	});

	it("writes openspec changes to sharedState.openspec", () => {
		createChange(tmpDir, "my-feature");

		const pi = createFakePi();
		emitOpenSpecState(tmpDir, pi as any);

		assert.ok(sharedState.openspec, "sharedState.openspec should be set after emitOpenSpecState");
		assert.ok(Array.isArray(sharedState.openspec.changes), "changes should be an array");
		assert.equal(sharedState.openspec.changes.length, 1);
		assert.equal(sharedState.openspec.changes[0].name, "my-feature");
	});

	it("fires DASHBOARD_UPDATE_EVENT with source=openspec", () => {
		const pi = createFakePi();
		emitOpenSpecState(tmpDir, pi as any);

		const dashboardEvents = pi.emitted.filter((e) => e.channel === DASHBOARD_UPDATE_EVENT);
		assert.equal(dashboardEvents.length, 1, "should emit exactly one dashboard update event");
		assert.deepEqual(dashboardEvents[0].data, { source: "openspec" });
	});

	it("maps artifacts correctly based on change filesystem presence", () => {
		createChange(tmpDir, "with-proposal");

		const pi = createFakePi();
		emitOpenSpecState(tmpDir, pi as any);

		const change = sharedState.openspec?.changes[0];
		assert.ok(change, "change should exist");
		// createChange writes proposal.md
		assert.ok(change.artifacts.includes("proposal"), "should include 'proposal' artifact");
	});

	it("emits empty changes array when openspec/changes dir is empty", () => {
		// Ensure openspec dir exists but has no changes
		const changesDir = path.join(tmpDir, "openspec", "changes");
		fs.mkdirSync(changesDir, { recursive: true });

		const pi = createFakePi();
		emitOpenSpecState(tmpDir, pi as any);

		assert.ok(sharedState.openspec, "sharedState.openspec should be set");
		assert.deepEqual(sharedState.openspec.changes, []);
		// Event should still fire so dashboard clears stale state
		const dashboardEvents = pi.emitted.filter((e) => e.channel === DASHBOARD_UPDATE_EVENT);
		assert.equal(dashboardEvents.length, 1);
	});

	it("is non-fatal when openspec directory does not exist", () => {
		const missingDir = path.join(tmpDir, "nonexistent");
		const pi = createFakePi();

		// Should not throw — listChanges returns [] gracefully for missing dirs
		assert.doesNotThrow(() => emitOpenSpecState(missingDir, pi as any));

		// sharedState still updated with empty array so dashboard clears stale state
		assert.ok(sharedState.openspec, "sharedState.openspec should be set");
		assert.deepEqual(sharedState.openspec.changes, []);
	});

	it("emits task progress from tasks.md when present", () => {
		createChange(tmpDir, "with-tasks");
		const tasksPath = path.join(tmpDir, "openspec", "changes", "with-tasks", "tasks.md");
		fs.writeFileSync(
			tasksPath,
			`# Tasks\n\n## Group 1\n\n- [x] done\n- [ ] pending\n`,
		);

		const pi = createFakePi();
		emitOpenSpecState(tmpDir, pi as any);

		const change = sharedState.openspec?.changes[0];
		assert.ok(change, "change should exist");
		assert.equal(change.tasksTotal, 2);
		assert.equal(change.tasksDone, 1);
	});
});
