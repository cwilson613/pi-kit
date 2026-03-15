---
name: pi-extensions
description: Pi extension API reference and conventions. Use when creating, modifying, or debugging pi extensions — covers registerCommand, registerTool, event handlers, UI context, and common pitfalls.
---

# Pi Extension API — Quick Reference

Extensions are TypeScript modules exporting a default function that receives `ExtensionAPI`.

```typescript
import type { ExtensionAPI } from "@styrene-lab/pi-coding-agent";

export default function (pi: ExtensionAPI) {
  // register tools, commands, shortcuts, event handlers
}
```

The factory can be `async` if initialization requires it.

## Commands

**Method:** `pi.registerCommand(name, options)`

```typescript
pi.registerCommand("my-command", {
  description: "What this command does",
  handler: async (args: string, ctx: ExtensionCommandContext) => {
    // args is the raw string after the command name
    // ctx provides ui, sessionManager, model, etc.
    ctx.ui.notify("Hello", "info");
  },
});
```

**⚠️ Common mistakes:**
- There is NO `addCommand` method. Only `registerCommand` exists.
- The handler signature is `handler(args, ctx)` — args first, context second.
- There is NO `ctx.say()` method. Use `ctx.ui.notify(message, type)`.
- There is NO `execute` property. The callback property is `handler`.

### UI Methods on `ctx.ui`

| Method | Signature | Notes |
|--------|-----------|-------|
| `notify` | `(message: string, type?: "info" \| "warning" \| "error") => void` | Non-blocking notification |
| `confirm` | `(title: string, message: string, opts?) => Promise<boolean>` | **Two** string args required |
| `select` | `(title: string, options: string[], opts?) => Promise<string \| undefined>` | Returns selected item |
| `input` | `(title: string, placeholder?: string, opts?) => Promise<string \| undefined>` | Text input dialog |
| `setStatus` | `(key: string, text: string \| undefined) => void` | Footer status line |
| `setWidget` | `(key: string, content, options?) => void` | Widget above/below editor |
| `setFooter` | `(factory \| undefined) => void` | Custom footer component |

**⚠️ `confirm()` takes TWO string arguments** (title + message), not one. A single-arg call is a type error.

## Tools

**Method:** `pi.registerTool(toolDefinition)`

```typescript
import { Type } from "@sinclair/typebox";

pi.registerTool({
  name: "my_tool",
  label: "My Tool",
  description: "What this tool does (shown to LLM)",
  promptSnippet: "One-liner for Available Tools section",
  promptGuidelines: ["Bullet points appended to system prompt Guidelines"],
  parameters: Type.Object({
    action: Type.String({ description: "What to do" }),
    target: Type.Optional(Type.String({ description: "Optional target" })),
  }),
  execute: async (toolCallId, params, signal, onUpdate, ctx) => {
    // params is typed from the schema
    // signal: AbortSignal | undefined
    // onUpdate: streaming partial results callback
    // ctx: ExtensionContext (NOT ExtensionCommandContext)
    return {
      content: [{ type: "text", text: "result" }],
    };
  },
});
```

**Key differences from commands:**
- Tools are called by the LLM, commands by the user via `/` prefix.
- Tool `execute` receives `(toolCallId, params, signal, onUpdate, ctx)`.
- Tool `ctx` is `ExtensionContext` (no `waitForIdle`, `newSession`, etc.).
- Tool results must return `{ content: Array<{type: "text", text: string}> }`.

### StringEnum helper

For enum parameters, use the local helper (NOT `@styrene-lab/pi-ai`):

```typescript
import { StringEnum } from "../lib/typebox-helpers";

// In parameters:
action: StringEnum(["start", "stop", "status"], { description: "Action" }),
```

## Event Handlers

**Method:** `pi.on(eventName, handler)`

```typescript
pi.on("session_start", async (event, ctx) => { ... });
pi.on("session_shutdown", async (event, ctx) => { ... });
```

### Valid Event Names

| Event | When | Can return |
|-------|------|------------|
| `session_start` | Session loaded | void |
| `session_shutdown` | Process exiting | void |
| `session_before_compact` | Before compaction | `{ cancel?, compaction? }` |
| `session_compact` | After compaction | void |
| `session_before_switch` | Before session switch | `{ cancel? }` |
| `session_switch` | After session switch | void |
| `session_before_fork` | Before fork | `{ cancel? }` |
| `session_fork` | After fork | void |
| `session_before_tree` | Before tree nav | `{ cancel?, summary? }` |
| `session_tree` | After tree nav | void |
| `context` | Before LLM call | `{ messages? }` |
| `before_agent_start` | After user input | `{ message?, systemPrompt? }` |
| `agent_start` | Agent loop begins | void |
| `agent_end` | Agent loop ends | void |
| `turn_start` / `turn_end` | Each turn | void |
| `message_start` / `message_update` / `message_end` | Message lifecycle | void |
| `tool_execution_start` / `tool_execution_update` / `tool_execution_end` | Tool lifecycle | void |
| `model_select` | Model changed | void |
| `tool_call` | Before tool executes | `{ block?, reason? }` |
| `tool_result` | After tool executes | `{ content?, isError? }` |
| `input` | User input received | `{ action: "continue" \| "transform" \| "handled" }` |
| `resources_discover` | After session_start | `{ skillPaths?, promptPaths?, themePaths? }` |

**⚠️ There is NO `session_end` event.** The cleanup event is `session_shutdown`.

## Cross-Extension Communication

Use the shared event bus for decoupled extension-to-extension messaging:

```typescript
// Emitter
pi.events.emit("my-event", data);

// Listener
pi.events.on("my-event", (data) => { ... });
```

For shared state, use `globalThis` via `Symbol.for()`:

```typescript
const STATE_KEY = Symbol.for("pi:shared-state");
(globalThis as any)[STATE_KEY] = sharedObject;
```

## Process Spawning in Extensions

When spawning child processes from extensions:

- **Use `stdio: "pipe"`** for processes whose output you capture. Never `stdio: "inherit"` — it corrupts the TUI.
- **Use `detached: true` + `child.unref()`** for background processes that should outlive the tool call.
- **Clean up in `session_shutdown`**: if you spawn a persistent process, kill it on shutdown.
- **Set `NONINTERACTIVE=1`** in env for install scripts to prevent stdin prompts hanging the TUI.
- **Respect `signal` (AbortSignal)**: register `signal.addEventListener("abort", ...)` to kill child processes on cancellation.

```typescript
// Background process with cleanup
let child: ChildProcess | null = null;

function startProcess() {
  child = spawn("my-binary", ["serve"], { stdio: "ignore", detached: true });
  child.unref();
  child.on("exit", () => { child = null; });
}

pi.on("session_shutdown", () => {
  if (child) { child.kill("SIGTERM"); child = null; }
});
```

## Conventions

- **One extension per directory** under `extensions/`. Entry point is `index.ts`.
- **Testable domain logic** in separate files (e.g., `auth.ts`), index imports and re-exports.
- **Types** in `types.ts` when shared across files.
- **Config annotation** at top of file: `// @config KEY "description" [default: value]`
- **Extension load order** defined in `package.json` → `pi.extensions` array.
- **No circular imports** between extensions. Use shared-state or events for cross-extension data.
- **Import `type` separately** from runtime imports: `import type { ExtensionAPI } from "..."`.
