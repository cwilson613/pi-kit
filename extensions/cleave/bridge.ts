import { buildSlashCommandResult } from "../lib/slash-command-bridge.ts";
import type { AssessStructuredResult } from "./assessment.ts";

export function buildAssessBridgeResult(
	bridgedArgs: string[],
	result: AssessStructuredResult,
): ReturnType<typeof buildSlashCommandResult> {
	return buildSlashCommandResult(result.command, bridgedArgs, {
		ok: result.ok,
		summary: result.summary,
		humanText: result.humanText,
		data: {
			subcommand: result.subcommand,
			data: result.data,
			lifecycleHint: result.lifecycle,
			assessEffects: result.effects,
		},
		lifecycle: result.lifecycleRecord,
		effects: {
			sideEffectClass: result.subcommand === "cleave" ? "workspace-write" : "read",
			lifecycleTouched: result.lifecycleRecord ? [result.lifecycleRecord.changeName] : undefined,
		},
		nextSteps: result.nextSteps.map((step) => ({ label: step })),
	});
}
