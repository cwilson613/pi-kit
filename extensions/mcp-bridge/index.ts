import type { ExtensionAPI } from "@mariozechner/pi-coding-agent";
import { Type } from "@sinclair/typebox";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import * as fs from "node:fs";
import * as path from "node:path";

interface ServerConfig {
  command: string;
  args?: string[];
  env?: Record<string, string>;
}

interface McpConfig {
  servers: Record<string, ServerConfig>;
}

interface ConnectedServer {
  client: Client;
  transport: StdioClientTransport;
  tools: Array<{ name: string; description?: string; inputSchema: any }>;
}

export default function (pi: ExtensionAPI) {
  const servers: Record<string, ConnectedServer> = {};
  const configPath = path.join(path.dirname(new URL(import.meta.url).pathname), "mcp.json");

  function resolveEnvVars(value: string): string {
    return value.replace(/\$\{(\w+)\}/g, (_, key) => process.env[key] ?? "");
  }

  function resolveEnvObj(env: Record<string, string>): Record<string, string> {
    const resolved: Record<string, string> = {};
    for (const [k, v] of Object.entries(env)) {
      resolved[k] = resolveEnvVars(v);
    }
    return resolved;
  }

  async function connectServer(name: string, config: ServerConfig): Promise<ConnectedServer | null> {
    try {
      const resolvedEnv = config.env ? resolveEnvObj(config.env) : {};

      const transport = new StdioClientTransport({
        command: config.command,
        args: config.args ?? [],
        env: { ...process.env, ...resolvedEnv } as Record<string, string>,
      });

      const client = new Client({ name: `pi-mcp-bridge/${name}`, version: "1.0.0" });
      await client.connect(transport);

      const { tools } = await client.listTools();

      return { client, transport, tools };
    } catch (err: any) {
      console.error(`[mcp-bridge] Failed to connect to ${name}: ${err.message}`);
      return null;
    }
  }

  function jsonSchemaToTypebox(schema: any): any {
    // Pass the raw JSON schema as-is via Type.Unsafe
    // TypeBox Type.Unsafe wraps arbitrary JSON Schema for the LLM
    if (!schema || typeof schema !== "object") return Type.Object({});
    return Type.Unsafe(schema);
  }

  pi.on("session_start", async (_event, ctx) => {
    if (!fs.existsSync(configPath)) {
      ctx.ui.notify("[mcp-bridge] No mcp.json found", "warning");
      return;
    }

    const config: McpConfig = JSON.parse(fs.readFileSync(configPath, "utf-8"));
    let totalTools = 0;

    for (const [name, serverConfig] of Object.entries(config.servers)) {
      const connected = await connectServer(name, serverConfig);
      if (!connected) {
        ctx.ui.notify(`[mcp-bridge] Failed: ${name}`, "error");
        continue;
      }

      servers[name] = connected;

      for (const tool of connected.tools) {
        const piToolName = `mcp_${name}_${tool.name}`;

        pi.registerTool({
          name: piToolName,
          label: `${name}/${tool.name}`,
          description: tool.description ?? `MCP tool from ${name}`,
          parameters: jsonSchemaToTypebox(tool.inputSchema),

          async execute(toolCallId, params, signal, onUpdate, ctx) {
            try {
              const result = await connected.client.callTool({
                name: tool.name,
                arguments: params,
              });

              const textParts = (result.content as any[])
                .filter((c: any) => c.type === "text")
                .map((c: any) => c.text)
                .join("\n");

              return {
                content: [{ type: "text", text: textParts || "(empty response)" }],
                details: { server: name, tool: tool.name },
              };
            } catch (err: any) {
              return {
                content: [{ type: "text", text: `Error: ${err.message}` }],
                details: { server: name, tool: tool.name, error: true },
              };
            }
          },
        });

        totalTools++;
      }
    }

    if (totalTools > 0) {
      ctx.ui.notify(`[mcp-bridge] ${totalTools} tools from ${Object.keys(servers).length} server(s)`, "info");
    }
  });

  pi.on("session_shutdown", async () => {
    for (const [name, server] of Object.entries(servers)) {
      try {
        await server.client.close();
      } catch {}
    }
  });

  // Command to list connected servers and tools
  pi.registerCommand("mcp", {
    description: "List MCP servers and tools",
    handler: async (_args, ctx) => {
      if (Object.keys(servers).length === 0) {
        ctx.ui.notify("No MCP servers connected", "warning");
        return;
      }

      const lines: string[] = [];
      for (const [name, server] of Object.entries(servers)) {
        lines.push(`\n${name} (${server.tools.length} tools):`);
        for (const tool of server.tools) {
          lines.push(`  mcp_${name}_${tool.name} — ${tool.description ?? "(no description)"}`);
        }
      }
      ctx.ui.notify(lines.join("\n"), "info");
    },
  });
}
