// @secret HF_TOKEN "HuggingFace token (gated model access for FLUX.1)"

/**
 * diffuse — Visual generation extension for pi
 *
 * Provides two tools:
 *   - generate_image_local: AI image generation via FLUX.1 (mflux, Apple Silicon MLX)
 *   - render_diagram: Mermaid diagram rendering via mmdc (falls back to source)
 *
 * Both tools save output to ~/.pi/visuals/ for persistence across sessions.
 *
 * Requirements:
 *   generate_image_local:
 *     - Apple Silicon Mac with sufficient RAM (16GB+ quantized, 32GB+ full)
 *     - uv + mflux installed (set DIFFUSION_CLI_DIR or use ~/diffusion-cli default)
 *     - HuggingFace token for gated models (FLUX.1-schnell, FLUX.1-dev)
 *   render_diagram:
 *     - mmdc (optional, for PNG output): npm install -g @mermaid-js/mermaid-cli
 *     - Falls back to syntax-highlighted source if mmdc is not installed
 *
 * Environment:
 *   DIFFUSION_CLI_DIR — path to uv project with mflux installed
 *   PI_VISUALS_DIR    — output directory override (default: ~/.pi/visuals)
 */

import { execSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync, statSync } from "node:fs";
import { readFile } from "node:fs/promises";
import { homedir, tmpdir } from "node:os";
import { join, basename } from "node:path";
import { mkdtempSync } from "node:fs";
import { StringEnum } from "@mariozechner/pi-ai";
import type { ExtensionAPI } from "@mariozechner/pi-coding-agent";
import { Type } from "@sinclair/typebox";

// ---------------------------------------------------------------------------
// Shared output directory
// ---------------------------------------------------------------------------

const VISUALS_DIR = process.env.PI_VISUALS_DIR || join(homedir(), ".pi", "visuals");

function ensureVisualsDir() {
	mkdirSync(VISUALS_DIR, { recursive: true });
}

function visualsPath(filename: string): string {
	ensureVisualsDir();
	return join(VISUALS_DIR, filename);
}

function timestamp(): string {
	return new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
}

function hasCmd(cmd: string): boolean {
	try {
		execSync(`which ${cmd}`, { stdio: "ignore" });
		return true;
	} catch {
		return false;
	}
}

// ---------------------------------------------------------------------------
// Diffusion config
// ---------------------------------------------------------------------------

const DIFFUSION_CLI_DIR = process.env.DIFFUSION_CLI_DIR || join(homedir(), "diffusion-cli");

const PRESETS = ["schnell", "dev", "dev-fast", "diagram", "portrait", "wide"] as const;

const PRESET_DESCRIPTIONS: Record<(typeof PRESETS)[number], string> = {
	schnell:    "FLUX.1-schnell — fastest, ~10s, 4 steps",
	dev:        "FLUX.1-dev — high quality, ~60s, 25 steps",
	"dev-fast": "FLUX.1-dev — balanced, ~30s, 12 steps",
	diagram:    "Optimized for technical diagrams (1024x768)",
	portrait:   "Portrait orientation (768x1024), high quality",
	wide:       "Cinematic wide (1344x768), fast",
};

const PRESET_DEFAULTS: Record<string, { model: string; steps: number; guidance: number; width: number; height: number }> = {
	schnell:    { model: "schnell", steps: 4,  guidance: 0.0, width: 1024, height: 1024 },
	dev:        { model: "dev",     steps: 25, guidance: 3.5, width: 1024, height: 1024 },
	"dev-fast": { model: "dev",     steps: 12, guidance: 3.5, width: 1024, height: 1024 },
	diagram:    { model: "schnell", steps: 4,  guidance: 0.0, width: 1024, height: 768  },
	portrait:   { model: "dev",     steps: 25, guidance: 3.5, width: 768,  height: 1024 },
	wide:       { model: "schnell", steps: 4,  guidance: 0.0, width: 1344, height: 768  },
};

// ---------------------------------------------------------------------------
// Extension
// ---------------------------------------------------------------------------

export default function diffuseExtension(pi: ExtensionAPI) {

	// ------------------------------------------------------------------
	// generate_image_local — FLUX.1 via mflux on Apple Silicon
	// ------------------------------------------------------------------
	pi.registerTool({
		name: "generate_image_local",
		label: "Generate Image (Local)",
		description: [
			"Generate an image locally on Apple Silicon using FLUX.1 via MLX.",
			"Returns the generated image inline. Runs entirely on-device, no cloud API needed.",
			"Output is saved to ~/.pi/visuals/ for persistence.",
			"",
			"Presets:",
			...PRESETS.map((p) => `  ${p}: ${PRESET_DESCRIPTIONS[p]}`),
			"",
			"For technical diagrams, use the 'diagram' preset.",
			"For fast iteration, use 'schnell'. For quality, use 'dev'.",
			"Quantize to 4 or 8 bits to reduce memory usage and speed up generation.",
		].join("\n"),
		promptSnippet: "Generate images locally via FLUX.1 on Apple Silicon (no cloud API)",
		promptGuidelines: [
			"Use 'diagram' preset for technical diagrams, 'schnell' for fast iteration, 'dev' for quality",
			"Quantize to 4 or 8 bits to reduce memory usage and speed up generation",
		],

		parameters: Type.Object({
			prompt:    Type.String({ description: "Text prompt describing the image to generate" }),
			preset:    Type.Optional(StringEnum(PRESETS, { description: "Generation preset. Default: schnell" })),
			width:     Type.Optional(Type.Number({ description: "Image width in pixels (multiple of 64)" })),
			height:    Type.Optional(Type.Number({ description: "Image height in pixels (multiple of 64)" })),
			steps:     Type.Optional(Type.Number({ description: "Number of diffusion steps" })),
			guidance:  Type.Optional(Type.Number({ description: "Classifier-free guidance scale" })),
			seed:      Type.Optional(Type.Number({ description: "Random seed for reproducibility" })),
			quantize:  Type.Optional(StringEnum(["3", "4", "5", "6", "8"] as const, { description: "Quantization bits (lower = faster/less VRAM)" })),
			model:     Type.Optional(Type.String({ description: "Override model (HuggingFace repo or local path)" })),
		}),

		async execute(_toolCallId, params, signal, onUpdate, _ctx) {
			if (!existsSync(DIFFUSION_CLI_DIR)) {
				throw new Error(
					`diffusion-cli not found at ${DIFFUSION_CLI_DIR}. ` +
					`Set it up with: uv init ~/diffusion-cli && cd ~/diffusion-cli && uv add mflux\n` +
					`Or set DIFFUSION_CLI_DIR to point to an existing mflux project.`
				);
			}

			const preset = params.preset || "schnell";
			const defaults = PRESET_DEFAULTS[preset] || PRESET_DEFAULTS.schnell;
			const modelName = params.model || defaults.model;

			const mfluxBin = join(DIFFUSION_CLI_DIR, ".venv", "bin", "mflux-generate");
			const slug = params.prompt.slice(0, 40).replace(/[^a-zA-Z0-9]/g, "_");
			const outputPath = visualsPath(`${timestamp()}_${slug}.png`);

			const args = [
				"--model",    modelName,
				"--prompt",   params.prompt,
				"--width",    String(params.width    || defaults.width),
				"--height",   String(params.height   || defaults.height),
				"--steps",    String(params.steps    || defaults.steps),
				"--guidance", String(params.guidance ?? defaults.guidance),
				"--output",   outputPath,
				"--metadata",
			];
			if (params.seed      !== undefined) args.push("--seed",     String(params.seed));
			if (params.quantize)                args.push("--quantize", params.quantize);

			onUpdate?.({
				content: [{ type: "text", text: `Generating with ${modelName} (${preset})…` }],
				details: { preset, model: modelName },
			});

			const startTime = Date.now();
			const result = await pi.exec(mfluxBin, args, { signal, timeout: 600_000 });
			const elapsed = ((Date.now() - startTime) / 1000).toFixed(1);

			if (result.code !== 0) {
				const stderr = result.stderr || "";
				if (stderr.includes("GatedRepoError") || stderr.includes("401")) {
					throw new Error(
						"HuggingFace authentication required. The model is gated.\n" +
						"1. Accept the license at https://huggingface.co/black-forest-labs/FLUX.1-schnell\n" +
						`2. Run: cd ${DIFFUSION_CLI_DIR} && uv run python -c "from huggingface_hub import login; login()"\n` +
						"3. Retry the generation."
					);
				}
				throw new Error(`mflux-generate failed (exit ${result.code}):\n${stderr.slice(-1500)}`);
			}

			if (!existsSync(outputPath)) {
				throw new Error(`Image was not created at ${outputPath}. Stdout: ${result.stdout?.slice(-500)}`);
			}

			const imageBuffer = await readFile(outputPath);
			const base64Data = imageBuffer.toString("base64");

			const w = params.width  || defaults.width;
			const h = params.height || defaults.height;
			const summary = [
				`Generated in ${elapsed}s via mflux/${modelName} (${preset}).`,
				`Resolution: ${w}×${h}`,
				params.seed     !== undefined ? `Seed: ${params.seed}` : "",
				params.quantize                ? `Quantized: ${params.quantize}-bit` : "",
				`Saved: ${outputPath}`,
			].filter(Boolean).join("  ·  ");

			return {
				content: [
					{ type: "text",  text: summary },
					{ type: "image", data: base64Data, mimeType: "image/png" },
				],
				details: { preset, model: modelName, elapsed: Number(elapsed), outputPath, width: w, height: h, seed: params.seed, quantize: params.quantize },
			};
		},
	});

	// ------------------------------------------------------------------
	// render_diagram — Mermaid code → inline image via mmdc
	// ------------------------------------------------------------------
	pi.registerTool({
		name: "render_diagram",
		label: "Render Diagram",
		description:
			"Render inline Mermaid diagram source code as an image inline in the terminal. " +
			"Use for architecture diagrams, flowcharts, ER diagrams, sequence diagrams, " +
			"class diagrams, state machines, Gantt charts, and any other Mermaid diagram type. " +
			"Output is saved to ~/.pi/visuals/ when rendered as PNG. " +
			"Requires mmdc for PNG output (npm install -g @mermaid-js/mermaid-cli); " +
			"falls back to syntax-highlighted source if mmdc is not installed.",
		promptSnippet: "Render Mermaid diagrams as inline images (flowcharts, ER, sequence, etc.)",
		parameters: Type.Object({
			code:  Type.String({ description: "Mermaid diagram source code (raw syntax, no backtick fences)" }),
			title: Type.Optional(Type.String({ description: "Optional title for the diagram" })),
		}),
		async execute(_toolCallId, params, _signal, _onUpdate, _ctx) {
			// Write source to a persistent .mmd file in visuals dir
			const slug = (params.title || "diagram").replace(/[^a-zA-Z0-9]/g, "_").slice(0, 40);
			const mmdPath  = visualsPath(`${timestamp()}_${slug}.mmd`);
			writeFileSync(mmdPath, params.code, "utf-8");

			const titlePrefix = params.title ? `# ${params.title}\n\n` : "";

			if (hasCmd("mmdc")) {
				const outPng = mmdPath.replace(/\.mmd$/, ".png");
				try {
					execSync(
						`mmdc -i "${mmdPath}" -o "${outPng}" -b transparent -w 1200 2>/dev/null`,
						{ timeout: 15_000 }
					);
					if (existsSync(outPng) && statSync(outPng).size > 0) {
						const data = readFileSync(outPng).toString("base64");
						return {
							content: [
								{ type: "text",  text: `${titlePrefix}📊 Mermaid  ·  Saved: ${outPng}` },
								{ type: "image", data, mimeType: "image/png" },
							],
							details: { rendered: true, mmdPath, pngPath: outPng },
						};
					}
				} catch { /* fall through to source display */ }
			}

			// Fallback: syntax-highlighted source
			const hint = hasCmd("mmdc") ? "" : "\n> Install mmdc for PNG rendering: `npm install -g @mermaid-js/mermaid-cli`";
			return {
				content: [{
					type: "text",
					text: `${titlePrefix}📊 Mermaid source  ·  Saved: ${mmdPath}${hint}\n\n\`\`\`mermaid\n${params.code}\n\`\`\``,
				}],
				details: { rendered: false, mmdPath },
			};
		},
	});

	// ------------------------------------------------------------------
	// /diffuse command — quick image generation shortcut
	// ------------------------------------------------------------------
	pi.registerCommand("diffuse", {
		description: "Generate an image locally (usage: /diffuse <prompt>)",
		handler: async (args, _ctx) => {
			if (!args?.trim()) {
				// Show status instead of error
				const mfluxOk = existsSync(join(DIFFUSION_CLI_DIR, ".venv", "bin", "mflux-generate"));
				const mmdcOk  = hasCmd("mmdc");
				const status = [
					`**Visual generation status**`,
					``,
					`FLUX.1 (generate_image_local): ${mfluxOk ? "✅ ready" : `❌ not found — set up ${DIFFUSION_CLI_DIR}`}`,
					`mmdc (render_diagram): ${mmdcOk ? "✅ ready" : "⚠️  not installed — \`npm install -g @mermaid-js/mermaid-cli\`"}`,
					`Output directory: \`${VISUALS_DIR}\``,
					``,
					`Usage: \`/diffuse <prompt>\``,
				].join("\n");
				pi.sendMessage({ customType: "view", content: status, display: true });
				return;
			}

			pi.sendUserMessage(
				`Use the generate_image_local tool to create an image with this prompt: ${args}`,
				{ deliverAs: "followUp" }
			);
		},
	});
}
