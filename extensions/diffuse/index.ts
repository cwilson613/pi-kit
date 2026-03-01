/**
 * diffuse — Local image generation via mflux (MLX-native diffusion on Apple Silicon)
 *
 * Registers a `generate_image_local` tool that the LLM can call to generate images
 * using FLUX.1-schnell, FLUX.1-dev, or other mflux-supported models. Runs entirely
 * on-device using Apple Silicon GPU via MLX.
 *
 * Requirements:
 *   - Apple Silicon Mac with sufficient RAM (16GB+ for quantized, 32GB+ for full)
 *   - uv + the diffusion-cli project at ~/workspace/ai/diffusion-cli
 *   - HuggingFace token (for gated models like FLUX.1)
 *
 * Usage from LLM:
 *   "Generate an image of a network diagram"
 *   "Create a portrait photo of a cat in watercolor style"
 *
 * Save: Images are saved to ~/workspace/ai/diffusion-cli/output/ by default.
 */

import { existsSync } from "node:fs";
import { readFile } from "node:fs/promises";
import { homedir } from "node:os";
import { join } from "node:path";
import { StringEnum } from "@mariozechner/pi-ai";
import type { ExtensionAPI } from "@mariozechner/pi-coding-agent";
import { Type } from "@sinclair/typebox";

const DIFFUSION_CLI_DIR = join(homedir(), "workspace", "ai", "diffusion-cli");
const OUTPUT_DIR = join(DIFFUSION_CLI_DIR, "output");

const PRESETS = ["schnell", "dev", "dev-fast", "diagram", "portrait", "wide"] as const;

const PRESET_DESCRIPTIONS: Record<(typeof PRESETS)[number], string> = {
  schnell: "FLUX.1-schnell — fastest, ~10s, 4 steps",
  dev: "FLUX.1-dev — high quality, ~60s, 25 steps",
  "dev-fast": "FLUX.1-dev — balanced, ~30s, 12 steps",
  diagram: "Optimized for technical diagrams (1024x768)",
  portrait: "Portrait orientation (768x1024), high quality",
  wide: "Cinematic wide (1344x768), fast",
};

const PRESET_DEFAULTS: Record<string, { model: string; steps: number; guidance: number; width: number; height: number }> = {
  schnell:    { model: "schnell", steps: 4,  guidance: 0.0, width: 1024, height: 1024 },
  dev:        { model: "dev",     steps: 25, guidance: 3.5, width: 1024, height: 1024 },
  "dev-fast": { model: "dev",     steps: 12, guidance: 3.5, width: 1024, height: 1024 },
  diagram:    { model: "schnell", steps: 4,  guidance: 0.0, width: 1024, height: 768 },
  portrait:   { model: "dev",     steps: 25, guidance: 3.5, width: 768,  height: 1024 },
  wide:       { model: "schnell", steps: 4,  guidance: 0.0, width: 1344, height: 768 },
};

export default function diffuseExtension(pi: ExtensionAPI) {
  pi.registerTool({
    name: "generate_image_local",
    label: "Generate Image (Local)",
    description: [
      "Generate an image locally on Apple Silicon using FLUX.1 via MLX.",
      "Returns the generated image inline. Runs entirely on-device, no cloud API needed.",
      "",
      "Presets:",
      ...PRESETS.map((p) => `  ${p}: ${PRESET_DESCRIPTIONS[p]}`),
      "",
      "For technical diagrams, use the 'diagram' preset.",
      "For fast iteration, use 'schnell'. For quality, use 'dev'.",
      "Quantize to 4 or 8 bits to reduce memory usage and speed up generation.",
    ].join("\n"),

    parameters: Type.Object({
      prompt: Type.String({ description: "Text prompt describing the image to generate" }),
      preset: Type.Optional(StringEnum(PRESETS, { description: "Generation preset. Default: schnell" })),
      width: Type.Optional(Type.Number({ description: "Image width in pixels (multiple of 64)" })),
      height: Type.Optional(Type.Number({ description: "Image height in pixels (multiple of 64)" })),
      steps: Type.Optional(Type.Number({ description: "Number of diffusion steps" })),
      guidance: Type.Optional(Type.Number({ description: "Classifier-free guidance scale" })),
      seed: Type.Optional(Type.Number({ description: "Random seed for reproducibility" })),
      quantize: Type.Optional(StringEnum(["3", "4", "5", "6", "8"] as const, { description: "Quantization bits (lower = faster/less VRAM)" })),
      model: Type.Optional(Type.String({ description: "Override model (HuggingFace repo or local path)" })),
    }),

    async execute(_toolCallId, params, signal, onUpdate, ctx) {
      // Verify diffusion-cli exists
      if (!existsSync(DIFFUSION_CLI_DIR)) {
        throw new Error(
          `diffusion-cli not found at ${DIFFUSION_CLI_DIR}. ` +
          `Set it up with: cd ~/workspace/ai && uv init diffusion-cli && cd diffusion-cli && uv add mflux`
        );
      }

      const preset = params.preset || "schnell";
      const defaults = PRESET_DEFAULTS[preset] || PRESET_DEFAULTS.schnell;

      // Build mflux-generate command
      const mfluxBin = join(DIFFUSION_CLI_DIR, ".venv", "bin", "mflux-generate");
      const ts = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
      const slug = params.prompt.slice(0, 40).replace(/[^a-zA-Z0-9]/g, "_");
      const outputPath = join(OUTPUT_DIR, `${ts}_${slug}.png`);

      const args = [
        "--model", params.model || defaults.model,
        "--prompt", params.prompt,
        "--width", String(params.width || defaults.width),
        "--height", String(params.height || defaults.height),
        "--steps", String(params.steps || defaults.steps),
        "--guidance", String(params.guidance ?? defaults.guidance),
        "--output", outputPath,
        "--metadata",
      ];

      if (params.seed !== undefined) {
        args.push("--seed", String(params.seed));
      }
      if (params.quantize) {
        args.push("--quantize", params.quantize);
      }

      const modelName = params.model || defaults.model;
      onUpdate?.({
        content: [{ type: "text", text: `Generating with ${modelName} (${preset})...` }],
        details: { preset, model: modelName },
      });

      // Ensure output dir exists
      await pi.exec("mkdir", ["-p", OUTPUT_DIR], { signal });

      // Run mflux-generate
      const startTime = Date.now();
      const result = await pi.exec(mfluxBin, args, {
        signal,
        timeout: 600_000, // 10 min max
      });
      const elapsed = ((Date.now() - startTime) / 1000).toFixed(1);

      if (result.code !== 0) {
        const stderr = result.stderr || "";
        // Check for common errors
        if (stderr.includes("GatedRepoError") || stderr.includes("401")) {
          throw new Error(
            "HuggingFace authentication required. The model is gated.\n" +
            "1. Accept the license at https://huggingface.co/black-forest-labs/FLUX.1-schnell\n" +
            "2. Run: cd ~/workspace/ai/diffusion-cli && uv run python -c \"from huggingface_hub import login; login()\"\n" +
            "3. Retry the generation."
          );
        }
        throw new Error(`mflux-generate failed (exit ${result.code}):\n${stderr.slice(-1500)}`);
      }

      // Read the generated image
      if (!existsSync(outputPath)) {
        throw new Error(`Image was not created at ${outputPath}. Stdout: ${result.stdout?.slice(-500)}`);
      }

      const imageBuffer = await readFile(outputPath);
      const base64Data = imageBuffer.toString("base64");

      const summary = [
        `Generated image in ${elapsed}s via mflux/${modelName} (${preset} preset).`,
        `Resolution: ${params.width || defaults.width}×${params.height || defaults.height}`,
        params.seed !== undefined ? `Seed: ${params.seed}` : "",
        params.quantize ? `Quantized: ${params.quantize}-bit` : "",
        `Saved to: ${outputPath}`,
      ].filter(Boolean).join("\n");

      return {
        content: [
          { type: "text", text: summary },
          { type: "image", data: base64Data, mimeType: "image/png" },
        ],
        details: {
          preset,
          model: modelName,
          elapsed: Number(elapsed),
          outputPath,
          width: params.width || defaults.width,
          height: params.height || defaults.height,
          seed: params.seed,
          quantize: params.quantize,
        },
      };
    },
  });

  // Register /diffuse command for quick generation
  pi.registerCommand("diffuse", {
    description: "Generate an image locally (usage: /diffuse <prompt>)",
    handler: async (args, ctx) => {
      if (!args?.trim()) {
        ctx.ui.notify("Usage: /diffuse <prompt>", "warning");
        return;
      }

      // Send as a user message so the LLM calls the tool
      pi.sendUserMessage(
        `Use the generate_image_local tool to create an image with this prompt: ${args}`,
        { deliverAs: "followUp" }
      );
    },
  });
}
