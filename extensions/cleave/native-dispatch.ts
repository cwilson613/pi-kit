/**
 * Native cleave dispatch — calls the Rust omegon-agent cleave subcommand.
 */

import { spawn } from "node:child_process";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { resolveNativeAgent } from "../lib/omegon-subprocess.ts";

export interface NativeDispatchConfig {
	planPath: string;
	directive: string;
	workspacePath: string;
	repoPath: string;
	model: string;
	maxParallel: number;
	timeoutSecs: number;
	idleTimeoutSecs: number;
	maxTurns: number;
}

export interface NativeDispatchResult {
	exitCode: number;
	state: any;
	stderr: string;
}

export async function dispatchViaNative(
	config: NativeDispatchConfig,
	signal?: AbortSignal,
	onProgress?: (line: string) => void,
): Promise<NativeDispatchResult> {
	const nativeAgent = resolveNativeAgent();
	if (!nativeAgent) {
		throw new Error(
			"Native agent binary not found. Run `cargo build --release` in core/.",
		);
	}

	const args = [
		"cleave",
		"--plan", config.planPath,
		"--directive", config.directive,
		"--workspace", config.workspacePath,
		"--cwd", config.repoPath,
		"--model", config.model,
		"--max-parallel", String(config.maxParallel),
		"--timeout", String(config.timeoutSecs),
		"--idle-timeout", String(config.idleTimeoutSecs),
		"--max-turns", String(config.maxTurns),
		"--bridge", nativeAgent.bridgePath,
	];

	const log = (msg: string) => onProgress?.(`[native-dispatch] ${msg}`);

	log(`binary: ${nativeAgent.binaryPath}`);
	log(`bridge: ${nativeAgent.bridgePath}`);
	log(`workspace: ${config.workspacePath}`);
	log(`model: ${config.model}`);
	log(`spawning: ${nativeAgent.binaryPath} ${args.join(" ")}`);

	return new Promise<NativeDispatchResult>((resolve, reject) => {
		let proc: ReturnType<typeof spawn>;
		try {
			proc = spawn(nativeAgent.binaryPath, args, {
				cwd: config.repoPath,
				stdio: ["ignore", "pipe", "pipe"],
				env: {
					...process.env,
					RUST_LOG: "info",
				},
			});
		} catch (e: any) {
			log(`spawn threw: ${e.message}`);
			reject(new Error(`Failed to spawn omegon-agent cleave: ${e.message}`));
			return;
		}

		log(`spawned pid=${proc.pid}`);

		let stderr = "";
		let stdout = "";
		let stderrLines = 0;

		proc.stderr?.on("data", (data) => {
			const text = data.toString();
			stderr += text;
			for (const line of text.split("\n")) {
				const trimmed = line.trim();
				if (trimmed) {
					stderrLines++;
					onProgress?.(trimmed);
				}
			}
		});

		proc.stdout?.on("data", (data) => {
			stdout += data.toString();
		});

		if (signal) {
			if (signal.aborted) {
				log("signal already aborted before spawn!");
				try { proc.kill("SIGTERM"); } catch { /* */ }
				resolve({ exitCode: 130, state: null, stderr: "aborted before start" });
				return;
			}
			const onAbort = () => {
				log("abort signal received — killing child");
				try { proc.kill("SIGTERM"); } catch { /* */ }
				setTimeout(() => {
					try { proc.kill("SIGKILL"); } catch { /* */ }
				}, 3000);
			};
			signal.addEventListener("abort", onAbort, { once: true });
			proc.on("close", () => signal.removeEventListener("abort", onAbort));
		}

		proc.on("error", (err) => {
			log(`proc error event: ${err.message}`);
			reject(new Error(`Failed to spawn omegon-agent cleave: ${err.message}`));
		});

		proc.on("close", (code, sig) => {
			log(`proc closed: code=${code} signal=${sig} stderrLines=${stderrLines} stdoutLen=${stdout.length}`);

			let state: any = null;
			try {
				const statePath = join(config.workspacePath, "state.json");
				const raw = readFileSync(statePath, "utf-8");
				state = JSON.parse(raw);
				const statuses = state?.children?.map((c: any) => `${c.label}=${c.status}`).join(", ");
				log(`state.json loaded: ${state?.children?.length} children, [${statuses}]`);
			} catch (e: any) {
				log(`state.json read failed: ${e.message}`);
			}

			resolve({
				exitCode: code ?? 1,
				state,
				stderr,
			});
		});
	});
}
