import { describe, it } from "node:test";
import assert from "node:assert/strict";

import { buildAssessBridgeResult } from "./bridge.ts";
import type { AssessStructuredResult } from "./assessment.ts";

describe("buildAssessBridgeResult", () => {
	it("preserves the full original bridged args while keeping structured assess metadata", () => {
		const result: AssessStructuredResult<{ decision: string }> = {
			command: "assess",
			subcommand: "complexity",
			args: "rename helper function",
			ok: true,
			summary: "Complexity decision: execute",
			humanText: "Execute directly",
			data: { decision: "execute" },
			effects: [{ type: "view", content: "Execute directly" }],
			nextSteps: ["Execute directly"],
		};

		const bridged = buildAssessBridgeResult(["complexity", "rename", "helper", "function"], result);

		assert.deepEqual(bridged.args, ["complexity", "rename", "helper", "function"]);
		assert.equal(bridged.command, "assess");
		assert.equal((bridged.data as any).subcommand, "complexity");
		assert.deepEqual((bridged.data as any).data, { decision: "execute" });
	});
});
