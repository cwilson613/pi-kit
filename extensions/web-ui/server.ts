/**
 * Localhost-only HTTP server for the web UI extension.
 *
 * Binds exclusively to 127.0.0.1 (never 0.0.0.0).
 * Serves a read-only dashboard shell and JSON slice routes.
 * All mutation verbs are refused.
 */

import * as http from "node:http";
import * as fs from "node:fs";
import * as path from "node:path";
import { fileURLToPath } from "node:url";
import { buildControlPlaneState, buildSlice } from "./state.ts";
import type { ControlPlaneState } from "./types.ts";

// ── Types ─────────────────────────────────────────────────────────────────────

export interface WebUIServerOptions {
  /** Port to listen on. 0 = OS-assigned ephemeral port. Default: 0. */
  port?: number;
  /** Repository root used for state scans. Default: process.cwd(). */
  repoRoot?: string;
}

export interface WebUIServer {
  /** Bound port (available after start). */
  readonly port: number;
  /** Full localhost URL. */
  readonly url: string;
  /** Unix epoch ms when the server started. */
  readonly startedAt: number;
  /** Stop the server, rejecting new connections. */
  stop(): Promise<void>;
}

// ── HTML shell ────────────────────────────────────────────────────────────────

const STATIC_DIR = path.join(path.dirname(fileURLToPath(import.meta.url)), "static");

// Cache shell content at module load time to avoid synchronous I/O on every request.
let _shellCache: string | null = null;

function serveShell(): string {
  if (_shellCache !== null) return _shellCache;
  const indexPath = path.join(STATIC_DIR, "index.html");
  if (fs.existsSync(indexPath)) {
    _shellCache = fs.readFileSync(indexPath, "utf8");
  } else {
    // Inline fallback if static dir is unavailable
    _shellCache = buildInlineShell();
  }
  return _shellCache;
}

function buildInlineShell(): string {
  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>pi-kit dashboard</title>
  <style>
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body { font-family: 'Menlo', 'Monaco', monospace; background: #0d1117; color: #c9d1d9; padding: 1rem; }
    h1 { font-size: 1.2rem; margin-bottom: 1rem; color: #58a6ff; }
    #status { font-size: 0.8rem; color: #8b949e; margin-bottom: 1rem; }
    section { margin-bottom: 1.5rem; }
    section h2 { font-size: 0.9rem; color: #79c0ff; border-bottom: 1px solid #30363d; padding-bottom: 0.25rem; margin-bottom: 0.5rem; }
    pre { font-size: 0.75rem; background: #161b22; padding: 0.75rem; border-radius: 4px; overflow: auto; white-space: pre-wrap; }
    .badge { display: inline-block; padding: 0.1rem 0.4rem; border-radius: 3px; font-size: 0.7rem; background: #21262d; color: #8b949e; margin-right: 0.25rem; }
    .ok { background: #0d4429; color: #3fb950; }
  </style>
</head>
<body>
  <h1>pi-kit dashboard</h1>
  <div id="status">Loading…</div>
  <div id="root"></div>
  <script>
    const POLL_INTERVAL_MS = 3000;
    let pollTimer = null;

    async function fetchState() {
      const res = await fetch('/api/state');
      if (!res.ok) throw new Error('HTTP ' + res.status);
      return res.json();
    }

    function renderSection(title, data) {
      const section = document.createElement('section');
      const h2 = document.createElement('h2');
      h2.textContent = title;
      const pre = document.createElement('pre');
      pre.textContent = JSON.stringify(data, null, 2);
      section.appendChild(h2);
      section.appendChild(pre);
      return section;
    }

    function render(state) {
      const root = document.getElementById('root');
      root.innerHTML = '';
      root.appendChild(renderSection('Session', state.session));
      root.appendChild(renderSection('Dashboard', state.dashboard));
      root.appendChild(renderSection('Design Tree', state.designTree));
      root.appendChild(renderSection('OpenSpec', state.openspec));
      root.appendChild(renderSection('Cleave', state.cleave));
      root.appendChild(renderSection('Models', state.models));
      root.appendChild(renderSection('Memory', state.memory));
      root.appendChild(renderSection('Health', state.health));
    }

    async function poll() {
      try {
        const state = await fetchState();
        const now = new Date().toLocaleTimeString();
        document.getElementById('status').innerHTML =
          '<span class="badge ok">live</span> Last updated: ' + now +
          ' &mdash; schema v' + state.schemaVersion;
        render(state);
      } catch (err) {
        document.getElementById('status').textContent = 'Error: ' + err.message;
      }
    }

    poll();
    pollTimer = setInterval(poll, POLL_INTERVAL_MS);
  </script>
</body>
</html>`;
}

// ── Routing ───────────────────────────────────────────────────────────────────

const SLICE_ROUTES: Record<string, keyof Omit<ControlPlaneState, "schemaVersion">> = {
  "/api/session":     "session",
  "/api/dashboard":   "dashboard",
  "/api/design-tree": "designTree",
  "/api/openspec":    "openspec",
  "/api/cleave":      "cleave",
  "/api/models":      "models",
  "/api/memory":      "memory",
  "/api/health":      "health",
};

function jsonResponse(
  res: http.ServerResponse,
  status: number,
  body: unknown
): void {
  const payload = JSON.stringify(body);
  res.writeHead(status, {
    "Content-Type": "application/json",
    "Content-Length": Buffer.byteLength(payload),
    "Cache-Control": "no-store",
    "X-Content-Type-Options": "nosniff",
  });
  res.end(payload);
}

function handleRequest(
  req: http.IncomingMessage,
  res: http.ServerResponse,
  repoRoot: string,
  startedAt: number
): void {
  const rawUrl = req.url ?? "/";
  const url = new URL(rawUrl, "http://x").pathname;
  const method = (req.method ?? "GET").toUpperCase();

  // ── Root shell ──
  if (url === "/" || url === "/index.html") {
    if (method !== "GET" && method !== "HEAD") {
      res.writeHead(405, { Allow: "GET, HEAD" });
      res.end();
      return;
    }
    const html = serveShell();
    res.writeHead(200, {
      "Content-Type": "text/html; charset=utf-8",
      "Content-Length": Buffer.byteLength(html),
      "Cache-Control": "no-store",
      "X-Content-Type-Options": "nosniff",
    });
    res.end(method === "HEAD" ? undefined : html);
    return;
  }

  // ── Full state snapshot ──
  if (url === "/api/state") {
    if (method !== "GET" && method !== "HEAD") {
      // Refuse any mutation verb
      return jsonResponse(res, 405, {
        error: "Method Not Allowed",
        detail: "This endpoint is read-only. POST/PUT/PATCH/DELETE are not supported.",
      });
    }
    const state = buildControlPlaneState(repoRoot, startedAt);
    return jsonResponse(res, 200, state);
  }

  // ── Slice routes ──
  const sliceKey = SLICE_ROUTES[url];
  if (sliceKey !== undefined) {
    if (method !== "GET" && method !== "HEAD") {
      return jsonResponse(res, 405, { error: "Method Not Allowed" });
    }
    const slice = buildSlice(sliceKey, repoRoot, startedAt);
    return jsonResponse(res, 200, slice);
  }

  // ── 404 for everything else ──
  jsonResponse(res, 404, { error: "Not Found" });
}

// ── Server lifecycle ──────────────────────────────────────────────────────────

/**
 * Start a localhost-only HTTP server.
 *
 * Binds to 127.0.0.1 only. The OS assigns an ephemeral port when port=0.
 */
export function startWebUIServer(options: WebUIServerOptions = {}): Promise<WebUIServer> {
  const repoRoot = options.repoRoot ?? process.cwd();
  const port = options.port ?? 0;
  const startedAt = Date.now();

  return new Promise<WebUIServer>((resolve, reject) => {
    const server = http.createServer((req, res) => {
      handleRequest(req, res, repoRoot, startedAt);
    });

    server.once("error", reject);

    server.listen(port, "127.0.0.1", () => {
      const addr = server.address();
      if (!addr || typeof addr === "string") {
        server.close();
        return reject(new Error("Failed to obtain server address"));
      }

      const boundPort = addr.port;
      const url = `http://127.0.0.1:${boundPort}`;

      const handle: WebUIServer = {
        get port() { return boundPort; },
        get url() { return url; },
        get startedAt() { return startedAt; },
        stop(): Promise<void> {
          return new Promise((fulfill, fail) => {
            server.closeAllConnections();
            server.close((err) => (err ? fail(err) : fulfill()));
          });
        },
      };

      resolve(handle);
    });
  });
}
