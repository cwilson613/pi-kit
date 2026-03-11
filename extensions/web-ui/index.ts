import { spawn as nodeSpawn } from "node:child_process";
import type { ExtensionAPI } from "@cwilson613/pi-coding-agent";
import { startWebUIServer, type WebUIServer } from "./server.ts";

let server: WebUIServer | null = null;
let spawnFn: typeof nodeSpawn = nodeSpawn;

export function _setSpawnFn(fn: typeof nodeSpawn): typeof nodeSpawn {
  const prev = spawnFn;
  spawnFn = fn;
  return prev;
}

export function _setServer(next: WebUIServer | null): void {
  server = next;
}

/** Returns the current server instance. Prefer this over the live-binding export for reliable cross-Node-version semantics. */
export function _getServer(): WebUIServer | null {
  return server;
}

function openBrowser(url: string, ctx: Parameters<typeof notify>[0]): void {
  let cmd: string;
  let args: string[];

  if (process.platform === "darwin") {
    cmd = "open";
    args = [url];
  } else if (process.platform === "win32") {
    // cmd.exe `start` requires an empty-string window-title when the next token is a URL.
    cmd = "cmd";
    args = ["/c", "start", "", url];
  } else {
    cmd = "xdg-open";
    args = [url];
  }

  const child = spawnFn(cmd, args, { stdio: "ignore" });
  if (typeof child.on === "function") {
    child.on("error", (err: Error) => {
      notify(ctx, `Failed to open browser (${cmd}): ${err.message}`);
    });
  }
  if (typeof child.unref === "function") {
    child.unref();
  }
}

function notify(ctx: { ui?: { notify?: (msg: string, level?: "info" | "warning" | "error") => void } }, message: string): void {
  if (typeof ctx.ui?.notify === "function") ctx.ui.notify(message, "info");
}

export default function webUiExtension(pi: ExtensionAPI): void {
  pi.registerCommand("web-ui", {
    description: "Localhost-only read-only web UI dashboard (/web-ui [start|stop|status|open])",
    handler: async (args, ctx) => {
      const subcommand = args.trim().split(/\s+/)[0]?.toLowerCase() || "status";

      switch (subcommand) {
        case "status":
        case "": {
          if (!server) {
            notify(ctx, "web-ui server is stopped. Run `/web-ui start` to start it.");
          } else {
            const uptimeSec = Math.round((Date.now() - server.startedAt) / 1000);
            notify(ctx, `web-ui server is running at ${server.url} (uptime ${uptimeSec}s)`);
          }
          return;
        }
        case "start": {
          if (server) {
            notify(ctx, `web-ui server is already running at ${server.url}`);
            return;
          }
          server = await startWebUIServer({ repoRoot: ctx.cwd ?? process.cwd() });
          notify(ctx, `web-ui server started at ${server.url}. Run \`/web-ui open\` to open it in your browser.`);
          return;
        }
        case "stop": {
          if (!server) {
            notify(ctx, "web-ui server is not running.");
            return;
          }
          await server.stop().catch(() => {});
          server = null;
          notify(ctx, "web-ui server stopped.");
          return;
        }
        case "open": {
          if (!server) {
            notify(ctx, "web-ui server is not running. Run `/web-ui start` first.");
            return;
          }
          openBrowser(server.url, ctx);
          notify(ctx, `Opening ${server.url} in your default browser…`);
          return;
        }
        default:
          notify(ctx, "Usage: /web-ui [start|stop|status|open]");
      }
    },
  });

  pi.on("session_shutdown", async () => {
    if (!server) return;
    await server.stop().catch(() => {});
    server = null;
  });
}
