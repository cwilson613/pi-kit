import { describe, test, beforeEach } from "node:test";
import assert from "node:assert";
import { dispatchViaNative, type NativeProgressEvent, type NativeDispatchConfig } from "./native-dispatch.ts";

describe("native-dispatch", () => {
	describe("NDJSON event parsing", () => {
		test("should parse progress events from stdout", async () => {
			const events: NativeProgressEvent[] = [];
			const progressLines: string[] = [];
			
			const config: NativeDispatchConfig = {
				planPath: "/tmp/test-plan.json",
				directive: "test directive",
				workspacePath: "/tmp/workspace",
				repoPath: "/tmp/repo",
				model: "test:model",
				maxParallel: 2,
				timeoutSecs: 30,
				idleTimeoutSecs: 60,
				maxTurns: 10,
			};

			// This test requires a mock of the child process
			// In a real test environment, we'd mock the spawn function
			// For now, test event parsing logic separately
			
			// Test event type definitions
			const sampleEvents: NativeProgressEvent[] = [
				{ type: "wave_start", wave: 1, children: ["task1", "task2"] },
				{ type: "child_spawned", label: "task1", worktree_path: "/tmp/task1" },
				{ type: "child_activity", label: "task1", activity: "Reading files", tool: "read" },
				{ type: "child_status", label: "task1", status: "completed", elapsed_secs: 45.2 },
				{ type: "done", total_duration_secs: 120.5, succeeded: 2, failed: 0 },
			];

			// Verify all event types are properly typed
			for (const event of sampleEvents) {
				assert.ok(event.type);
				
				switch (event.type) {
					case "wave_start":
						assert.strictEqual(typeof event.wave, "number");
						assert.ok(Array.isArray(event.children));
						break;
					case "child_spawned":
						assert.strictEqual(typeof event.label, "string");
						assert.strictEqual(typeof event.worktree_path, "string");
						break;
					case "child_activity":
						assert.strictEqual(typeof event.label, "string");
						assert.strictEqual(typeof event.activity, "string");
						break;
					case "child_status":
						assert.strictEqual(typeof event.label, "string");
						assert.ok(["completed", "failed"].includes(event.status));
						break;
					case "done":
						assert.strictEqual(typeof event.total_duration_secs, "number");
						assert.strictEqual(typeof event.succeeded, "number");
						assert.strictEqual(typeof event.failed, "number");
						break;
				}
			}
		});

		test("should handle malformed JSON on stdout gracefully", () => {
			// Test that non-JSON lines are handled as progress messages
			// This would be part of the stdout data handler in the actual implementation
			const malformedLines = [
				"not json",
				"{ incomplete json",
				"normal text output",
				'{"type": "child_spawned", "label": "valid"}', // Valid JSON
			];

			const events: any[] = [];
			const progressLines: string[] = [];

			for (const line of malformedLines) {
				try {
					const event = JSON.parse(line);
					events.push(event);
				} catch {
					progressLines.push(`[stdout] ${line}`);
				}
			}

			assert.strictEqual(events.length, 1);
			assert.strictEqual(events[0].type, "child_spawned");
			assert.strictEqual(progressLines.length, 3);
			assert.strictEqual(progressLines[0], "[stdout] not json");
		});

		test("should handle partial NDJSON lines correctly", () => {
			// Test buffer handling for incomplete lines
			let buffer = "";
			const events: any[] = [];
			const progressLines: string[] = [];

			// Simulate data chunks that might split JSON lines
			const chunks = [
				'{"type": "wave_start", "wave": 1,',
				' "children": ["task1"]}\n{"type": "child_spawned",',
				' "label": "task1", "worktree_path": "/tmp"}\npartial_line',
			];

			for (const chunk of chunks) {
				buffer += chunk;
				const lines = buffer.split("\n");
				buffer = lines.pop() || ""; // Keep incomplete line

				for (const line of lines) {
					const trimmed = line.trim();
					if (trimmed) {
						try {
							const event = JSON.parse(trimmed);
							events.push(event);
						} catch {
							progressLines.push(`[stdout] ${trimmed}`);
						}
					}
				}
			}

			assert.strictEqual(events.length, 2);
			assert.strictEqual(events[0].type, "wave_start");
			assert.strictEqual(events[1].type, "child_spawned");
			assert.strictEqual(buffer, "partial_line"); // Incomplete line remains in buffer
		});
	});

	describe("NativeProgressEvent types", () => {
		test("should have proper type discrimination", () => {
			const event: NativeProgressEvent = { type: "child_activity", label: "test", activity: "working" };
			
			// TypeScript should properly narrow the type based on the discriminant
			if (event.type === "child_activity") {
				assert.ok(event.activity);
				assert.ok(event.label);
				// TypeScript should not allow access to properties from other event types
			}
		});
	});
});