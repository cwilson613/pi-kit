/**
 * vault — Markdown viewport extension
 *
 * Spawns mdserve to render interlinked project markdown as a navigable
 * web UI with wikilink resolution, graph view, and live reload.
 *
 * Commands:
 *   /vault           — Start mdserve on the project root (or resume if running)
 *   /vault [path]    — Start mdserve on a specific directory
 *   /vault stop      — Stop the running mdserve instance
 *   /vault status    — Show whether mdserve is running and on which port
 *   /vault graph     — Open the graph view in the browser
 *
 * Dependency: mdserve binary (managed by /bootstrap)
 */

import { execSync, spawn, type ChildProcess } from "node:child_process";
import type { ExtensionAPI } from "@mariozechner/pi-coding-agent";
import { DEPS } from "../bootstrap/deps.js";

const DEFAULT_PORT = 3333;
const BINARY_NAME = "mdserve";

let mdserveProcess: ChildProcess | null = null;
let mdservePort: number | null = null;
let mdserveDir: string | null = null;

function hasBinary(): boolean {
	const dep = DEPS.find((d) => d.id === "mdserve");
	return dep ? dep.check() : false;
}

function openBrowser(url: string): void {
	try {
		const cmd = process.platform === "darwin" ? "open" : "xdg-open";
		spawn(cmd, [url], { stdio: "ignore", detached: true }).unref();
	} catch { /* user can open manually */ }
}

function stopMdserve(): string {
	if (mdserveProcess) {
		mdserveProcess.kill("SIGTERM");
		mdserveProcess = null;
		const msg = `Stopped mdserve (was serving ${mdserveDir} on port ${mdservePort})`;
		mdservePort = null;
		mdserveDir = null;
		return msg;
	}
	return "mdserve is not running.";
}

function startMdserve(dir: string, port: number): string {
	if (mdserveProcess) {
		if (mdserveDir === dir) {
			return `mdserve already running at http://127.0.0.1:${mdservePort}\n` +
				`Serving: ${mdserveDir}\n` +
				`Use \`/vault stop\` to stop, or \`/vault graph\` to open graph view.`;
		}
		stopMdserve();
	}

	const child = spawn(BINARY_NAME, [dir, "--port", String(port)], {
		stdio: ["ignore", "pipe", "pipe"],
		detached: false,
	});

	mdserveProcess = child;
	mdservePort = port;
	mdserveDir = dir;

	child.stdout?.on("data", (data: Buffer) => {
		const match = data.toString().match(/using (\d+) instead/);
		if (match) mdservePort = parseInt(match[1], 10);
	});

	child.on("exit", () => {
		if (mdserveProcess === child) {
			mdserveProcess = null;
			mdservePort = null;
			mdserveDir = null;
		}
	});

	openBrowser(`http://127.0.0.1:${port}`);

	return `Started mdserve at http://127.0.0.1:${port}\n` +
		`Serving: ${dir}\n` +
		`Graph view: http://127.0.0.1:${port}/graph\n` +
		`Use \`/vault stop\` to stop.`;
}

const NOT_INSTALLED = "`mdserve` is not installed. Run `/bootstrap` to set up pi-kit dependencies.";

export default function (pi: ExtensionAPI) {
	pi.on("session_end", () => {
		if (mdserveProcess) {
			mdserveProcess.kill("SIGTERM");
			mdserveProcess = null;
		}
	});

	pi.addCommand({
		name: "vault",
		description: "Markdown viewport — serve project docs with wikilinks and graph view",
		execute: async (ctx, args) => {
			const subcommand = args.trim().split(/\s+/)[0]?.toLowerCase() || "";

			switch (subcommand) {
				case "stop":
					ctx.say(stopMdserve());
					return;

				case "status":
					if (mdserveProcess) {
						ctx.say(
							`mdserve is running at http://127.0.0.1:${mdservePort}\n` +
							`Serving: ${mdserveDir}`,
						);
					} else if (!hasBinary()) {
						ctx.say(NOT_INSTALLED);
					} else {
						ctx.say("mdserve is not running. Use `/vault` to start.");
					}
					return;

				case "graph":
					if (!hasBinary()) { ctx.say(NOT_INSTALLED); return; }
					if (mdserveProcess && mdservePort) {
						openBrowser(`http://127.0.0.1:${mdservePort}/graph`);
						ctx.say(`Opened graph view at http://127.0.0.1:${mdservePort}/graph`);
					} else {
						const dir = process.cwd();
						ctx.say(startMdserve(dir, DEFAULT_PORT));
						setTimeout(() => {
							if (mdservePort) openBrowser(`http://127.0.0.1:${mdservePort}/graph`);
						}, 1000);
					}
					return;

				default: {
					if (!hasBinary()) { ctx.say(NOT_INSTALLED); return; }
					const dir = subcommand || process.cwd();
					ctx.say(startMdserve(dir, DEFAULT_PORT));
					return;
				}
			}
		},
	});
}
