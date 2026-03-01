import type { ExtensionAPI } from "@mariozechner/pi-coding-agent";

export default function (pi: ExtensionAPI) {
  pi.on("session_start", async (_event, ctx) => {
    const sonnet = ctx.modelRegistry.find("anthropic", "claude-sonnet-4-5");
    if (sonnet) {
      const success = await pi.setModel(sonnet);
      if (success) {
        ctx.ui.setStatus("model-budget", "sonnet (budget)");
      }
    }
  });

  pi.registerCommand("opus", {
    description: "Escalate to Opus for complex reasoning",
    handler: async (_args, ctx) => {
      const opus = ctx.modelRegistry.find("anthropic", "claude-opus-4-6");
      if (opus) {
        const success = await pi.setModel(opus);
        if (success) {
          ctx.ui.setStatus("model-budget", "opus (escalated)");
          ctx.ui.notify("Escalated to Opus 4.6", "info");
        } else {
          ctx.ui.notify("Failed to set Opus — no API key?", "error");
        }
      } else {
        ctx.ui.notify("Opus model not found in registry", "error");
      }
    },
  });

  pi.registerCommand("sonnet", {
    description: "Drop back to Sonnet to conserve usage",
    handler: async (_args, ctx) => {
      const sonnet = ctx.modelRegistry.find("anthropic", "claude-sonnet-4-5");
      if (sonnet) {
        const success = await pi.setModel(sonnet);
        if (success) {
          ctx.ui.setStatus("model-budget", "sonnet (budget)");
          ctx.ui.notify("Dropped to Sonnet 4.5", "info");
        }
      }
    },
  });
}
