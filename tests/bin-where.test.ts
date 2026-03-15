import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { join } from "node:path";

const OMEGON_BIN = join(process.cwd(), "bin", "omegon.mjs");
const PI_BIN = join(process.cwd(), "bin", "pi.mjs");

describe("omegon executable --where", () => {
	it("prints Omegon resolution metadata without starting interactive mode", () => {
		const result = spawnSync(process.execPath, [OMEGON_BIN, "--where"], {
			encoding: "utf8",
			env: { ...process.env },
		});
		assert.equal(result.status, 0, result.stderr);
		const data = JSON.parse(result.stdout);
		assert.match(data.omegonRoot, /omegon$/);
		assert.match(data.cli, /(packages[\\/]coding-agent|node_modules[\\/]@cwilson613[\\/]pi-coding-agent)[\\/]dist[\\/]cli\.js$/);
		assert.ok(data.resolutionMode === "vendor" || data.resolutionMode === "npm");
		assert.equal(data.executable, "omegon");
	});

	it("lets the legacy pi alias re-enter the same omegon-owned runtime", () => {
		const result = spawnSync(process.execPath, [PI_BIN, "--where"], {
			encoding: "utf8",
			env: { ...process.env },
		});
		assert.equal(result.status, 0, result.stderr);
		const data = JSON.parse(result.stdout);
		assert.match(data.omegonRoot, /omegon$/);
		assert.equal(data.executable, "omegon");
	});
});
