/**
 * Tests for cleave/dispatcher — AsyncSemaphore concurrency control.
 *
 * We can't easily test the full dispatch pipeline (requires pi subprocess),
 * but the semaphore is the critical fix and is testable in isolation.
 */

import { describe, it } from "node:test";
import * as assert from "node:assert/strict";
import { AsyncSemaphore } from "./dispatcher.js";

// ─── AsyncSemaphore ─────────────────────────────────────────────────────────

describe("AsyncSemaphore", () => {
	it("allows up to limit concurrent acquires", async () => {
		const sem = new AsyncSemaphore(3);
		await sem.acquire();
		await sem.acquire();
		await sem.acquire();
		assert.equal(sem.activeCount, 3);
	});

	it("blocks beyond limit", async () => {
		const sem = new AsyncSemaphore(1);
		await sem.acquire();

		let acquired = false;
		const pending = sem.acquire().then(() => { acquired = true; });

		// Give microtasks a chance to run
		await new Promise((r) => setTimeout(r, 10));
		assert.equal(acquired, false, "Should be blocked");
		assert.equal(sem.waitingCount, 1);

		sem.release();
		await pending;
		assert.equal(acquired, true, "Should be unblocked after release");
	});

	it("maintains correct count through acquire/release cycles", async () => {
		const sem = new AsyncSemaphore(2);

		await sem.acquire();
		assert.equal(sem.activeCount, 2 - 1); // Actually count=1
		// Wait, activeCount returns this.count which starts at 0 and increments on acquire
		assert.equal(sem.activeCount, 1);

		await sem.acquire();
		assert.equal(sem.activeCount, 2);

		sem.release();
		assert.equal(sem.activeCount, 1);

		sem.release();
		assert.equal(sem.activeCount, 0);
	});

	it("guarantees no more than N concurrent tasks", async () => {
		const sem = new AsyncSemaphore(2);
		let maxConcurrent = 0;
		let currentConcurrent = 0;
		const results: number[] = [];

		const task = async (id: number) => {
			await sem.acquire();
			try {
				currentConcurrent++;
				maxConcurrent = Math.max(maxConcurrent, currentConcurrent);
				// Simulate async work
				await new Promise((r) => setTimeout(r, 5));
				results.push(id);
			} finally {
				currentConcurrent--;
				sem.release();
			}
		};

		await Promise.all([task(0), task(1), task(2), task(3), task(4)]);

		assert.equal(results.length, 5, "All tasks should complete");
		assert.ok(maxConcurrent <= 2, `Max concurrent was ${maxConcurrent}, expected <= 2`);
	});

	it("handles limit of 1 (serial execution)", async () => {
		const sem = new AsyncSemaphore(1);
		const order: number[] = [];

		const task = async (id: number) => {
			await sem.acquire();
			try {
				order.push(id);
				await new Promise((r) => setTimeout(r, 1));
			} finally {
				sem.release();
			}
		};

		await Promise.all([task(0), task(1), task(2)]);
		assert.equal(order.length, 3);
		// With limit=1, tasks execute serially in FIFO order
		assert.deepEqual(order, [0, 1, 2]);
	});

	it("handles release before any waiters (count decrements)", async () => {
		const sem = new AsyncSemaphore(3);
		await sem.acquire();
		assert.equal(sem.activeCount, 1);
		sem.release();
		assert.equal(sem.activeCount, 0);
		assert.equal(sem.waitingCount, 0);
	});

	it("stress test: 20 tasks with limit 3", async () => {
		const sem = new AsyncSemaphore(3);
		let maxConcurrent = 0;
		let currentConcurrent = 0;
		const completed: number[] = [];

		const task = async (id: number) => {
			await sem.acquire();
			try {
				currentConcurrent++;
				maxConcurrent = Math.max(maxConcurrent, currentConcurrent);
				await new Promise((r) => setTimeout(r, Math.random() * 5));
				completed.push(id);
			} finally {
				currentConcurrent--;
				sem.release();
			}
		};

		await Promise.all(Array.from({ length: 20 }, (_, i) => task(i)));

		assert.equal(completed.length, 20, "All 20 tasks should complete");
		assert.ok(maxConcurrent <= 3, `Max concurrent was ${maxConcurrent}, expected <= 3`);
		assert.equal(currentConcurrent, 0, "All tasks should be done");
	});
});
