import { describe, it } from "node:test";
import * as assert from "node:assert/strict";
import { DEPS, checkAll, formatReport, type DepStatus } from "./deps.js";

describe("bootstrap/deps", () => {
	it("has unique dep IDs", () => {
		const ids = DEPS.map((d) => d.id);
		assert.deepStrictEqual(ids, [...new Set(ids)]);
	});

	it("every dep has at least one install command", () => {
		for (const dep of DEPS) {
			assert.ok(dep.install.length > 0, `${dep.id} has no install commands`);
		}
	});

	it("every dep has a purpose and usedBy", () => {
		for (const dep of DEPS) {
			assert.ok(dep.purpose.length > 0, `${dep.id} missing purpose`);
			assert.ok(dep.usedBy.length > 0, `${dep.id} missing usedBy`);
		}
	});

	it("checkAll returns a status for every dep", () => {
		const statuses = checkAll();
		assert.equal(statuses.length, DEPS.length);
		for (const s of statuses) {
			assert.equal(typeof s.available, "boolean");
		}
	});

	it("tiers are valid", () => {
		const validTiers = new Set(["core", "recommended", "optional"]);
		for (const dep of DEPS) {
			assert.ok(validTiers.has(dep.tier), `${dep.id} has invalid tier: ${dep.tier}`);
		}
	});

	it("formatReport produces markdown with tier headers", () => {
		const statuses: DepStatus[] = [
			{ dep: DEPS[0], available: true },
			{ dep: DEPS[DEPS.length - 1], available: false },
		];
		const report = formatReport(statuses);
		assert.ok(report.includes("# pi-kit Dependencies"));
		assert.ok(report.includes("✅") || report.includes("❌"));
	});

	it("core deps include ollama and d2", () => {
		const coreIds = DEPS.filter((d) => d.tier === "core").map((d) => d.id);
		assert.ok(coreIds.includes("ollama"));
		assert.ok(coreIds.includes("d2"));
	});
});
