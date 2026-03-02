import type { ExtensionAPI } from "@mariozechner/pi-coding-agent";
import { Type } from "@sinclair/typebox";
import { StringEnum } from "@mariozechner/pi-ai";

const TIERS = {
  opus: { id: "claude-opus-4-6", label: "opus", icon: "🧠" },
  sonnet: { id: "claude-sonnet-4-6", label: "sonnet", icon: "⚡" },
  haiku: { id: "claude-haiku-4-5", label: "haiku", icon: "💨" },
} as const;

type TierName = keyof typeof TIERS;

async function switchTo(tier: TierName, pi: ExtensionAPI, ctx: any): Promise<boolean> {
  const spec = TIERS[tier];
  const model = ctx.modelRegistry.find("anthropic", spec.id);
  if (!model) return false;
  const success = await pi.setModel(model);
  return success;
}

export default function (pi: ExtensionAPI) {
  // Default to Opus on session start
  pi.on("session_start", async (_event, ctx) => {
    await switchTo("opus", pi, ctx);
  });

  // Tool the agent can call to shift tiers
  pi.registerTool({
    name: "set_model_tier",
    label: "Set Model Tier",
    description:
      "Switch the active model tier based on task complexity. " +
      "Use 'opus' for deep reasoning, architecture, and planning. " +
      "Use 'sonnet' for routine code edits, file operations, and execution. " +
      "Use 'haiku' for simple lookups, formatting, and boilerplate generation. " +
      "Downgrade when the current task is straightforward to conserve budget. " +
      "Upgrade when you encounter something that needs deeper reasoning.",
    promptSnippet: "Switch model tier (opus/sonnet/haiku) to match task complexity and conserve budget",
    promptGuidelines: [
      "Downgrade to sonnet for routine file edits, command execution, and cleanup tasks",
      "Upgrade to opus when encountering architecture decisions, complex debugging, or multi-step planning",
    ],
    parameters: Type.Object({
      tier: StringEnum(["opus", "sonnet", "haiku"], {
        description: "Target model tier",
      }),
      reason: Type.String({
        description: "Brief explanation for the tier change",
      }),
    }),
    execute: async (_toolCallId, params: { tier: TierName; reason: string }, _signal, _onUpdate, ctx) => {
      const success = await switchTo(params.tier, pi, ctx);
      const spec = TIERS[params.tier];
      if (success) {
        ctx.ui.notify(`${spec.icon} → ${spec.label}: ${params.reason}`, "info");
        return {
          content: [
            {
              type: "text" as const,
              text: `Switched to ${spec.label} (${spec.id}): ${params.reason}`,
            },
          ],
        };
      }
      return {
        content: [
          {
            type: "text" as const,
            text: `Failed to switch to ${spec.label} — model not found or no API key`,
          },
        ],
      };
    },
  });

  // Manual commands for direct control
  for (const [name, spec] of Object.entries(TIERS)) {
    pi.registerCommand(name, {
      description: `Switch to ${spec.label} (${spec.icon})`,
      handler: async (_args, ctx) => {
        const success = await switchTo(name as TierName, pi, ctx);
        if (!success) {
          ctx.ui.notify(`Failed to switch to ${spec.label}`, "error");
        }
      },
    });
  }
}
