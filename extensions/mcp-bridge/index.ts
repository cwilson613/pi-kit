// @secret GITHUB_TOKEN "GitHub personal access token for MCP server auth (Scribe, etc.)"

import type { ExtensionAPI } from "@mariozechner/pi-coding-agent";
import { Type } from "@sinclair/typebox";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js";
import * as fs from "node:fs";
import * as path from "node:path";
import {
  type ServerConfig,
  type HttpServerConfig,
  type StdioServerConfig,
  isHttpConfig,
  resolveEnvVars,
  resolveEnvObj,
  isAuthError,
  isTransportError,
  extractText,
  AUTH_REMEDIATION,
} from "./lib.js";

// ---------------------------------------------------------------------------
// Config types
// ---------------------------------------------------------------------------

interface McpConfig {
  servers: Record<string, ServerConfig>;
}

// ---------------------------------------------------------------------------
// Runtime types
// ---------------------------------------------------------------------------

interface ConnectedServer {
  name: string;
  client: Client;
  transport: StdioClientTransport | StreamableHTTPClientTransport;
  config: ServerConfig;
  tools: Array<{ name: string; description?: string; inputSchema: any }>;
}

const DEFAULT_CONNECT_TIMEOUT_MS = 15_000;

// ---------------------------------------------------------------------------
// Extension
// ---------------------------------------------------------------------------

export default function (pi: ExtensionAPI) {
  const servers: Record<string, ConnectedServer> = {};
  const configPath = path.join(
    path.dirname(new URL(import.meta.url).pathname),
    "mcp.json"
  );

  // In-flight reconnect promises, keyed by server name. Prevents concurrent
  // reconnect attempts from racing and leaking duplicate connections.
  const reconnecting = new Map<string, Promise<ConnectedServer | null>>();

  // ── Timeout helper ──────────────────────────────────────────────────────

  /**
   * Race a promise against a deadline. On timeout, attempts to close the
   * transport to avoid leaking child processes or HTTP connections.
   */
  function withTimeout(
    promise: Promise<ConnectedServer>,
    ms: number,
    label: string
  ): Promise<ConnectedServer> {
    let settled = false;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        if (!settled) {
          settled = true;
          reject(new Error(`[mcp-bridge] ${label}: timed out after ${ms}ms`));
          // Best-effort cleanup: the inner promise may still resolve with a
          // ConnectedServer whose transport is alive. Close it.
          promise.then(
            (s) => { try { s.transport.close(); } catch {} },
            () => {} // inner already failed, nothing to clean up
          );
        }
      }, ms);
      promise.then(
        (v) => { if (!settled) { settled = true; clearTimeout(timer); resolve(v); } },
        (e) => { if (!settled) { settled = true; clearTimeout(timer); reject(e); } }
      );
    });
  }

  // ── Server connection ───────────────────────────────────────────────────

  async function connectStdioServer(
    name: string,
    config: StdioServerConfig
  ): Promise<ConnectedServer> {
    const resolvedEnv = config.env ? resolveEnvObj(config.env) : {};

    const transport = new StdioClientTransport({
      command: config.command,
      args: config.args ?? [],
      env: { ...process.env, ...resolvedEnv } as Record<string, string>,
    });

    const client = new Client({
      name: `pi-mcp-bridge/${name}`,
      version: "1.0.0",
    });
    await client.connect(transport);
    const { tools } = await client.listTools();

    return { name, client, transport, config, tools };
  }

  async function connectHttpServer(
    name: string,
    config: HttpServerConfig
  ): Promise<ConnectedServer> {
    const resolvedHeaders = config.headers
      ? resolveEnvObj(config.headers)
      : {};

    const transport = new StreamableHTTPClientTransport(
      new URL(resolveEnvVars(config.url)),
      {
        requestInit: {
          headers: resolvedHeaders,
        },
      }
    );

    const client = new Client({
      name: `pi-mcp-bridge/${name}`,
      version: "1.0.0",
    });
    await client.connect(transport);
    const { tools } = await client.listTools();

    return { name, client, transport, config, tools };
  }

  async function connectServer(
    name: string,
    config: ServerConfig
  ): Promise<ConnectedServer> {
    const timeoutMs = isHttpConfig(config)
      ? config.timeout ?? DEFAULT_CONNECT_TIMEOUT_MS
      : DEFAULT_CONNECT_TIMEOUT_MS;

    const inner = isHttpConfig(config)
      ? connectHttpServer(name, config)
      : connectStdioServer(name, config as StdioServerConfig);

    return withTimeout(inner, timeoutMs, name);
  }

  // ── Reconnection ───────────────────────────────────────────────────────

  /**
   * Reconnect a server, deduplicating concurrent attempts. If a reconnect
   * is already in flight for this server, returns the existing promise.
   */
  function reconnectServer(
    name: string,
    config: ServerConfig
  ): Promise<ConnectedServer | null> {
    const inflight = reconnecting.get(name);
    if (inflight) return inflight;

    const attempt = (async (): Promise<ConnectedServer | null> => {
      // Tear down old connection
      const old = servers[name];
      if (old) {
        try { await old.client.close(); } catch {}
        delete servers[name];
      }

      try {
        const fresh = await connectServer(name, config);
        servers[name] = fresh;
        return fresh;
      } catch (err: any) {
        console.error(`[mcp-bridge] Reconnect failed for ${name}: ${err.message}`);
        return null;
      }
    })();

    // Clear the mutex when done regardless of outcome
    attempt.finally(() => reconnecting.delete(name));
    reconnecting.set(name, attempt);

    return attempt;
  }

  // ── Tool registration ──────────────────────────────────────────────────

  function jsonSchemaToTypebox(schema: any): any {
    if (!schema || typeof schema !== "object") return Type.Object({});
    return Type.Unsafe(schema);
  }

  function registerToolsForServer(server: ConnectedServer): number {
    const serverName = server.name;
    const serverConfig = server.config;
    let count = 0;

    for (const tool of server.tools) {
      const toolName = tool.name;
      const piToolName = `mcp_${serverName}_${toolName}`;

      pi.registerTool({
        name: piToolName,
        label: `${serverName}/${toolName}`,
        description: tool.description ?? `MCP tool from ${serverName}`,
        parameters: jsonSchemaToTypebox(tool.inputSchema),

        async execute(_toolCallId, params, _signal, _onUpdate, _ctx) {
          // Always read current server — may have been replaced by reconnect
          const current = servers[serverName];
          if (!current) {
            return {
              content: [{ type: "text", text: `Error: server ${serverName} is not connected` }],
              details: { server: serverName, tool: toolName, error: true },
            };
          }

          try {
            const result = await current.client.callTool({
              name: toolName,
              arguments: params,
            });

            return {
              content: [{ type: "text", text: extractText(result) }],
              details: { server: serverName, tool: toolName },
            };
          } catch (err: any) {
            // Auth errors — reconnecting won't help
            if (isAuthError(err)) {
              return {
                content: [
                  {
                    type: "text",
                    text: `[mcp-bridge] ${serverName}: authentication failed.\n${AUTH_REMEDIATION}`,
                  },
                ],
                details: { server: serverName, tool: toolName, error: true, auth: true },
              };
            }

            // Transport errors — attempt one reconnect + retry
            if (isTransportError(err)) {
              const reconnected = await reconnectServer(serverName, serverConfig);
              if (reconnected) {
                try {
                  const retry = await reconnected.client.callTool({
                    name: toolName,
                    arguments: params,
                  });
                  return {
                    content: [{ type: "text", text: extractText(retry) }],
                    details: { server: serverName, tool: toolName, reconnected: true },
                  };
                } catch (retryErr: any) {
                  const msg = isAuthError(retryErr)
                    ? `[mcp-bridge] ${serverName}: authentication failed.\n${AUTH_REMEDIATION}`
                    : `Error after reconnect: ${retryErr.message}`;
                  return {
                    content: [{ type: "text", text: msg }],
                    details: {
                      server: serverName,
                      tool: toolName,
                      error: true,
                      ...(isAuthError(retryErr) && { auth: true }),
                    },
                  };
                }
              }
            }

            return {
              content: [{ type: "text", text: `Error: ${err.message}` }],
              details: { server: serverName, tool: toolName, error: true },
            };
          }
        },
      });

      count++;
    }
    return count;
  }

  // ── Lifecycle ──────────────────────────────────────────────────────────

  pi.on("session_start", async (_event, ctx) => {
    if (!fs.existsSync(configPath)) {
      ctx.ui.notify("[mcp-bridge] No mcp.json found", "warning");
      return;
    }

    const config: McpConfig = JSON.parse(
      fs.readFileSync(configPath, "utf-8")
    );

    // Connect all servers in parallel with independent timeouts
    const entries = Object.entries(config.servers);
    const results = await Promise.allSettled(
      entries.map(([name, serverConfig]) => connectServer(name, serverConfig))
    );

    let totalTools = 0;
    for (let i = 0; i < entries.length; i++) {
      const [name] = entries[i];
      const result = results[i];

      if (result.status === "rejected") {
        const reason = result.reason;
        ctx.ui.notify(
          isAuthError(reason)
            ? `[mcp-bridge] ${name}: authentication failed.\n${AUTH_REMEDIATION}`
            : `[mcp-bridge] Failed: ${name} — ${reason?.message ?? reason}`,
          "error"
        );
        continue;
      }

      const connected = result.value;
      servers[name] = connected;
      totalTools += registerToolsForServer(connected);
    }

    if (totalTools > 0) {
      ctx.ui.notify(
        `[mcp-bridge] ${totalTools} tools from ${Object.keys(servers).length} server(s)`,
        "info"
      );
    }
  });

  pi.on("session_shutdown", async () => {
    await Promise.allSettled(
      Object.values(servers).map((s) => s.client.close())
    );
  });

  // ── Commands ───────────────────────────────────────────────────────────

  pi.registerCommand("mcp", {
    description: "List MCP servers and tools",
    handler: async (_args, ctx) => {
      if (Object.keys(servers).length === 0) {
        ctx.ui.notify("No MCP servers connected", "warning");
        return;
      }

      const lines: string[] = [];
      for (const [name, server] of Object.entries(servers)) {
        const kind =
          server.transport instanceof StreamableHTTPClientTransport
            ? "http"
            : "stdio";
        lines.push(
          `\n${name} [${kind}] (${server.tools.length} tools):`
        );
        for (const tool of server.tools) {
          lines.push(
            `  mcp_${name}_${tool.name} — ${tool.description ?? "(no description)"}`
          );
        }
      }
      ctx.ui.notify(lines.join("\n"), "info");
    },
  });
}
