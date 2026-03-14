/**
 * Core Tool Renderers — Sci-UI rendering for pi built-in tools.
 *
 * Uses registerToolRenderer() to attach renderCall/renderResult
 * to built-in tools (bash, read, edit, write) without replacing them.
 *
 * The built-in renderers handle syntax highlighting, diffs, and streaming —
 * we only override the COLLAPSED view to match the Sci-UI visual language.
 * Expanded views fall through to the built-in renderer.
 */
import type { ExtensionAPI } from "@cwilson613/pi-coding-agent";
import { sciCall, sciOk, sciErr, sciLoading } from "./sci-ui.ts";

/** Shorten a file path for display — keep last 2-3 segments. */
function shortenPath(p: string | null | undefined, maxLen = 55): string {
	if (!p) return "…";
	if (p.length <= maxLen) return p;
	const parts = p.split("/");
	// Show last 3 segments at most
	const tail = parts.slice(-3).join("/");
	return tail.length <= maxLen ? tail : "…" + p.slice(-(maxLen - 1));
}

export default function coreRenderers(pi: ExtensionAPI): void {
	// ── Read ──────────────────────────────────────────────────────────────
	pi.registerToolRenderer("read", {
		renderCall(args: any, theme: any) {
			const p = shortenPath(args?.file_path ?? args?.path);
			let range = "";
			if (args?.offset != null || args?.limit != null) {
				const start = args.offset ?? 1;
				const end = args.limit != null ? start + args.limit - 1 : "";
				range = `:${start}${end ? `-${end}` : ""}`;
			}
			return sciCall("read", `${p}${range}`, theme);
		},
		// renderResult omitted — built-in handles syntax highlighting + truncation
	});

	// ── Edit ──────────────────────────────────────────────────────────────
	pi.registerToolRenderer("edit", {
		renderCall(args: any, theme: any) {
			const p = shortenPath(args?.file_path ?? args?.path);
			// Show the size of the change: lines changed
			const oldLines = (args?.old_text ?? args?.oldText ?? "").split("\n").length;
			const newLines = (args?.new_text ?? args?.newText ?? "").split("\n").length;
			const delta = newLines - oldLines;
			const deltaStr = delta === 0 ? `${oldLines}L` : delta > 0 ? `+${delta}L` : `${delta}L`;
			return sciCall("edit", `${p} (${deltaStr})`, theme);
		},
		// renderResult omitted — built-in handles diff rendering
	});

	// ── Write ─────────────────────────────────────────────────────────────
	pi.registerToolRenderer("write", {
		renderCall(args: any, theme: any) {
			const p = shortenPath(args?.file_path ?? args?.path);
			const content = args?.content ?? "";
			const lines = content.split("\n").length;
			return sciCall("write", `${p} (${lines}L)`, theme);
		},
		// renderResult omitted — built-in handles syntax highlighting
	});

	// ── Bash ──────────────────────────────────────────────────────────────
	pi.registerToolRenderer("bash", {
		renderCall(args: any, theme: any) {
			const cmd = args?.command ?? "";
			// Truncate long commands
			const display = cmd.length > 70 ? cmd.slice(0, 67) + "…" : cmd;
			return sciCall("bash", display, theme);
		},
		// renderResult omitted — built-in handles output display + truncation
	});

	// ── Grep ──────────────────────────────────────────────────────────────
	pi.registerToolRenderer("grep", {
		renderCall(args: any, theme: any) {
			const pattern = args?.pattern ?? "";
			const p = shortenPath(args?.path);
			const glob = args?.glob ? ` (${args.glob})` : "";
			return sciCall("grep", `/${pattern}/ in ${p}${glob}`, theme);
		},
	});

	// ── Find ──────────────────────────────────────────────────────────────
	pi.registerToolRenderer("find", {
		renderCall(args: any, theme: any) {
			const pattern = args?.pattern ?? "";
			const p = shortenPath(args?.path);
			return sciCall("find", `${pattern} in ${p}`, theme);
		},
	});

	// ── Ls ─────────────────────────────────────────────────────────────────
	pi.registerToolRenderer("ls", {
		renderCall(args: any, theme: any) {
			const p = shortenPath(args?.path || ".");
			return sciCall("ls", p, theme);
		},
	});

	// ── View ──────────────────────────────────────────────────────────────
	pi.registerToolRenderer("view", {
		renderCall(args: any, theme: any) {
			const p = shortenPath(args?.path);
			const page = args?.page ? ` p${args.page}` : "";
			return sciCall("view", `${p}${page}`, theme);
		},
	});

	// ── Web Search ────────────────────────────────────────────────────────
	pi.registerToolRenderer("web_search", {
		renderCall(args: any, theme: any) {
			const query = args?.query ?? "";
			const mode = args?.mode ?? "quick";
			const display = query.length > 55 ? query.slice(0, 52) + "…" : query;
			const modeTag = mode !== "quick" ? ` [${mode}]` : "";
			return sciCall("web_search", `${display}${modeTag}`, theme);
		},
	});

	// ── Chronos ───────────────────────────────────────────────────────────
	pi.registerToolRenderer("chronos", {
		renderCall(args: any, theme: any) {
			const sub = args?.subcommand ?? "week";
			const expr = args?.expression ? ` "${args.expression}"` : "";
			return sciCall("chronos", `${sub}${expr}`, theme);
		},
	});

	// ── Render Diagram (D2) ───────────────────────────────────────────────
	pi.registerToolRenderer("render_diagram", {
		renderCall(args: any, theme: any) {
			const title = args?.title ?? "diagram";
			return sciCall("render_diagram", title, theme);
		},
	});

	// ── Render Native Diagram ─────────────────────────────────────────────
	pi.registerToolRenderer("render_native_diagram", {
		renderCall(args: any, theme: any) {
			const title = args?.title ?? "diagram";
			return sciCall("render_native_diagram", title, theme);
		},
	});

	// ── Render Excalidraw ─────────────────────────────────────────────────
	pi.registerToolRenderer("render_excalidraw", {
		renderCall(args: any, theme: any) {
			const p = shortenPath(args?.path);
			return sciCall("render_excalidraw", p, theme);
		},
	});

	// ── Generate Image Local ──────────────────────────────────────────────
	pi.registerToolRenderer("generate_image_local", {
		renderCall(args: any, theme: any) {
			const prompt = args?.prompt ?? "";
			const preset = args?.preset ?? "schnell";
			const display = prompt.length > 50 ? prompt.slice(0, 47) + "…" : prompt;
			return sciCall("generate_image_local", `${display} [${preset}]`, theme);
		},
	});

	// ── Render Composition Still ──────────────────────────────────────────
	pi.registerToolRenderer("render_composition_still", {
		renderCall(args: any, theme: any) {
			const p = shortenPath(args?.composition_path);
			const frame = args?.frame != null ? ` f${args.frame}` : "";
			return sciCall("render_composition_still", `${p}${frame}`, theme);
		},
	});

	// ── Render Composition Video ──────────────────────────────────────────
	pi.registerToolRenderer("render_composition_video", {
		renderCall(args: any, theme: any) {
			const p = shortenPath(args?.composition_path);
			const frames = args?.duration_in_frames ?? "?";
			const fmt = args?.format ?? "gif";
			return sciCall("render_composition_video", `${p} (${frames}f, ${fmt})`, theme);
		},
	});

	// ── Model Tier ────────────────────────────────────────────────────────
	pi.registerToolRenderer("set_model_tier", {
		renderCall(args: any, theme: any) {
			const tier = args?.tier ?? "?";
			return sciCall("set_model_tier", `→ ${tier}`, theme);
		},
	});

	// ── Thinking Level ────────────────────────────────────────────────────
	pi.registerToolRenderer("set_thinking_level", {
		renderCall(args: any, theme: any) {
			const level = args?.level ?? "?";
			return sciCall("set_thinking_level", `→ ${level}`, theme);
		},
	});

	// ── Ask Local Model ───────────────────────────────────────────────────
	pi.registerToolRenderer("ask_local_model", {
		renderCall(args: any, theme: any) {
			const model = args?.model ?? "auto";
			const prompt = args?.prompt ?? "";
			const display = prompt.length > 45 ? prompt.slice(0, 42) + "…" : prompt;
			return sciCall("ask_local_model", `[${model}] ${display}`, theme);
		},
	});

	// ── Manage Ollama ─────────────────────────────────────────────────────
	pi.registerToolRenderer("manage_ollama", {
		renderCall(args: any, theme: any) {
			const action = args?.action ?? "?";
			const model = args?.model ? ` ${args.model}` : "";
			return sciCall("manage_ollama", `${action}${model}`, theme);
		},
	});

	// ── List Local Models ─────────────────────────────────────────────────
	pi.registerToolRenderer("list_local_models", {
		renderCall(_args: any, theme: any) {
			return sciCall("list_local_models", "inventory", theme);
		},
	});

	// ── Switch Offline Driver ─────────────────────────────────────────────
	pi.registerToolRenderer("switch_to_offline_driver", {
		renderCall(args: any, theme: any) {
			const reason = args?.reason ?? "";
			const display = reason.length > 50 ? reason.slice(0, 47) + "…" : reason;
			return sciCall("switch_to_offline_driver", display, theme);
		},
	});

	// ── Manage Tools ──────────────────────────────────────────────────────
	pi.registerToolRenderer("manage_tools", {
		renderCall(args: any, theme: any) {
			const action = args?.action ?? "list";
			const tools = args?.tools?.join(", ") ?? "";
			const profile = args?.profile ?? "";
			const detail = tools || profile || "";
			return sciCall("manage_tools", `${action}${detail ? ` ${detail}` : ""}`, theme);
		},
	});

	// ── Whoami ────────────────────────────────────────────────────────────
	pi.registerToolRenderer("whoami", {
		renderCall(_args: any, theme: any) {
			return sciCall("whoami", "check auth", theme);
		},
	});
}
