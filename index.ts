/**
 * Project Memory Extension
 *
 * Persistent, cross-session project knowledge stored as structured markdown.
 * Supports multiple "minds" — composable memory stores with lifecycle management.
 *
 * - Active memory: .pi/memory/memory.md (default mind)
 * - Minds: .pi/memory/minds/<name>/memory.md
 * - Archive: .pi/memory/archive/ or minds/<name>/archive/
 *
 * Tools:
 *   memory_query          — Read active memory
 *   memory_store          — Explicitly add a fact
 *   memory_search_archive — Search archived facts
 *
 * Commands:
 *   /memory               — Interactive mind manager
 *   /memory edit           — Edit current mind in editor
 *   /memory refresh        — Re-evaluate against codebase
 *   /memory clear          — Reset current mind to template
 *
 * Background extraction via subagent.
 */

import * as path from "node:path";
import type { ExtensionAPI, ExtensionContext, ExtensionCommandContext, SessionMessageEntry } from "@mariozechner/pi-coding-agent";
import { DynamicBorder } from "@mariozechner/pi-coding-agent";
import { StringEnum } from "@mariozechner/pi-ai";
import { Type } from "@sinclair/typebox";
import { Container, type SelectItem, SelectList, Text } from "@mariozechner/pi-tui";
import { MemoryStorage } from "./storage.js";
import { SECTIONS, appendToSection, type SectionName } from "./template.js";
import { DEFAULT_CONFIG, type MemoryConfig } from "./types.js";
import {
  type ExtractionTriggerState,
  createTriggerState,
  shouldExtract,
  runExtraction,
  killActiveExtraction,
} from "./extraction.js";
import { MindManager, type MindMeta } from "./minds.js";
import { serializeConversation, convertToLlm } from "@mariozechner/pi-coding-agent";

export default function (pi: ExtensionAPI) {
  let storage: MemoryStorage | null = null;
  let mindManager: MindManager | null = null;
  let triggerState: ExtractionTriggerState = createTriggerState();
  let postCompaction = false;
  let firstTurn = true;
  let config: MemoryConfig = { ...DEFAULT_CONFIG };
  let activeExtractionPromise: Promise<void> | null = null;
  let lastCtx: ExtensionContext | null = null;

  /** Rebuild storage to point at the currently active mind */
  function rebuildStorage(cwd: string): void {
    if (!mindManager) return;
    const activeMind = mindManager.getActiveMindName();
    if (activeMind) {
      const mindDir = mindManager.getMindDir(activeMind);
      const archiveDir = mindManager.getMindArchiveDir(activeMind);
      storage = new MemoryStorage(cwd, mindDir, archiveDir);
    } else {
      storage = new MemoryStorage(cwd);
    }
    storage.init();
  }

  // --- Lifecycle ---

  pi.on("session_start", async (_event, ctx) => {
    mindManager = new MindManager(path.join(ctx.cwd, ".pi", "memory"));
    mindManager.init();
    rebuildStorage(ctx.cwd);
    triggerState = createTriggerState();
    postCompaction = false;
    firstTurn = true;
    activeExtractionPromise = null;
    lastCtx = ctx;
    updateStatus(ctx);
  });

  pi.on("session_shutdown", async (_event, ctx) => {
    // If extraction is already in flight, wait for it
    if (activeExtractionPromise) {
      const timeout = new Promise<void>((resolve) =>
        setTimeout(resolve, config.shutdownExtractionTimeout),
      );
      await Promise.race([activeExtractionPromise, timeout]);
      return;
    }

    // Otherwise, if there's been meaningful activity since last extraction,
    // run a final one with the shorter shutdown timeout
    if (!storage || !lastCtx) return;
    const usage = ctx.getContextUsage();
    if (!usage) return;

    const hasActivity = triggerState.toolCallsSinceExtract >= 2 ||
      (usage.tokens - triggerState.lastExtractedTokens) >= config.minimumTokensBetweenUpdate;

    if (!hasActivity) return;

    const shutdownConfig = { ...config, extractionTimeout: config.shutdownExtractionTimeout };
    const targetStorage = storage;

    try {
      const currentMemory = targetStorage.readMemory();
      const branch = ctx.sessionManager.getBranch();
      const messages = branch
        .filter((e): e is SessionMessageEntry => e.type === "message")
        .map((e) => e.message);

      const recentMessages = messages.slice(-30);
      if (recentMessages.length === 0) return;

      const serialized = serializeConversation(convertToLlm(recentMessages));
      const result = await runExtraction(ctx.cwd, currentMemory, serialized, shutdownConfig);
      targetStorage.writeExtractionResult(result);
    } catch {
      // Best-effort — don't block exit on failure
    }
  });

  pi.on("session_compact", async () => {
    postCompaction = true;
    triggerState.toolCallsSinceExtract = 0;
    triggerState.manualStoresSinceExtract = 0;
  });

  // --- Context Injection ---

  pi.on("before_agent_start", async (event, ctx) => {
    if (!storage) return;
    if (!firstTurn && !postCompaction) return;

    firstTurn = false;
    postCompaction = false;

    const lineCount = storage.countLines();
    if (lineCount <= 10) return;

    const activeMind = mindManager?.getActiveMindName();
    const mindLabel = activeMind ? ` (mind: ${activeMind})` : "";

    return {
      message: {
        customType: "project-memory",
        content: [
          `Project memory available${mindLabel} (${lineCount} lines from this and previous sessions).`,
          "Use **memory_query** to read accumulated knowledge about this project.",
          "Use **memory_store** to persist important discoveries (architecture decisions, constraints, patterns, known issues).",
          "Use **memory_search_archive** to search older archived facts.",
        ].join(" "),
        display: false,
      },
    };
  });

  // --- Background Extraction Triggers ---

  pi.on("tool_execution_end", async (event, ctx) => {
    if (!storage) return;

    triggerState.toolCallsSinceExtract++;

    if (event.toolName === "memory_store" && !event.isError) {
      triggerState.manualStoresSinceExtract++;
    }

    const usage = ctx.getContextUsage();
    if (!usage) return;

    if (shouldExtract(triggerState, usage.tokens, config)) {
      activeExtractionPromise = runBackgroundExtraction(ctx)
        .catch(() => {})
        .finally(() => { activeExtractionPromise = null; });
    }
  });

  async function runBackgroundExtraction(ctx: ExtensionContext): Promise<void> {
    if (!storage || triggerState.isRunning) return;
    triggerState.isRunning = true;

    // Snapshot the storage ref — mind may switch during async extraction
    const targetStorage = storage;

    try {
      const currentMemory = targetStorage.readMemory();
      const branch = ctx.sessionManager.getBranch();
      const messages = branch
        .filter((e): e is SessionMessageEntry => e.type === "message")
        .map((e) => e.message);

      const recentMessages = messages.slice(-30);
      if (recentMessages.length === 0) return;

      const serialized = serializeConversation(convertToLlm(recentMessages));
      const result = await runExtraction(ctx.cwd, currentMemory, serialized, config);
      targetStorage.writeExtractionResult(result);

      const usage = ctx.getContextUsage();
      triggerState.lastExtractedTokens = usage?.tokens ?? 0;
      triggerState.toolCallsSinceExtract = 0;
      triggerState.manualStoresSinceExtract = 0;
      triggerState.isInitialized = true;
    } finally {
      triggerState.isRunning = false;
    }
  }

  // --- Tools ---

  pi.registerTool({
    name: "memory_query",
    label: "Project Memory",
    description: [
      "Read project memory — accumulated knowledge about this project's architecture,",
      "decisions, constraints, known issues, and patterns from this and previous sessions.",
      "Use when you need context about why something was done a certain way,",
      "known problems, or project conventions.",
    ].join(" "),
    parameters: Type.Object({}),
    async execute() {
      if (!storage) {
        return { content: [{ type: "text", text: "Project memory not initialized." }] };
      }
      const memory = storage.readMemory();
      const activeMind = mindManager?.getActiveMindName();
      return {
        content: [{ type: "text", text: memory }],
        details: { lines: storage.countLines(), mind: activeMind ?? "default" },
      };
    },
  });

  pi.registerTool({
    name: "memory_store",
    label: "Store Memory",
    description: [
      "Explicitly add or update a fact in project memory.",
      "Use for important discoveries: architectural decisions, constraints,",
      "non-obvious patterns, tricky bugs, environment details.",
      "Facts persist across sessions.",
    ].join(" "),
    parameters: Type.Object({
      section: StringEnum(
        ["Architecture", "Decisions", "Constraints", "Known Issues", "Patterns & Conventions"] as const,
        { description: "Memory section to add the fact to" },
      ),
      content: Type.String({
        description: "Fact to add (single bullet point, self-contained)",
      }),
    }),
    async execute(_toolCallId, params) {
      if (!storage) {
        return {
          content: [{ type: "text", text: "Project memory not initialized." }],
          isError: true,
        };
      }

      const memory = storage.readMemory();
      const bullet = params.content.startsWith("- ") ? params.content : `- ${params.content}`;
      const updated = appendToSection(memory, params.section as SectionName, bullet);

      if (updated === memory) {
        return {
          content: [{ type: "text", text: `Duplicate — already stored in ${params.section}: ${bullet}` }],
          details: { section: params.section, duplicate: true },
        };
      }

      storage.writeMemory(updated);

      return {
        content: [{ type: "text", text: `Stored in ${params.section}: ${bullet}` }],
        details: { section: params.section, lines: storage.countLines() },
      };
    },
  });

  pi.registerTool({
    name: "memory_search_archive",
    label: "Search Memory Archive",
    description: [
      "Search archived project memories from previous months.",
      "Use when active memory doesn't have historical context you need —",
      "past decisions, old constraints, migration history, removed facts.",
    ].join(" "),
    parameters: Type.Object({
      query: Type.String({ description: "Search terms (file paths, symbol names, concepts)" }),
    }),
    async execute(_toolCallId, params) {
      if (!storage) {
        return { content: [{ type: "text", text: "Project memory not initialized." }] };
      }

      const results = storage.searchArchive(params.query);

      if (results.length === 0) {
        return { content: [{ type: "text", text: "No matches in memory archive." }] };
      }

      const formatted = results
        .map((r) => `## ${r.month}\n${r.matches.join("\n")}`)
        .join("\n\n");

      return {
        content: [{ type: "text", text: formatted }],
        details: { months: results.length, totalMatches: results.reduce((n, r) => n + r.matches.length, 0) },
      };
    },
  });

  // --- Interactive Mind Manager ---

  function buildMindItems(minds: MindMeta[], activeName: string | null): SelectItem[] {
    const statusIcon = { active: "◉", refined: "◈", retired: "◌" };
    const items: SelectItem[] = [];

    // Default mind entry
    const defaultActive = activeName === null;
    items.push({
      value: "__default__",
      label: `${defaultActive ? "▸ " : "  "}default`,
      description: defaultActive ? "active • project default memory" : "project default memory",
    });

    for (const mind of minds) {
      const isActive = activeName === mind.name;
      const icon = statusIcon[mind.status] ?? "?";
      items.push({
        value: mind.name,
        label: `${isActive ? "▸ " : "  "}${icon} ${mind.name}`,
        description: [
          isActive ? "active" : mind.status,
          `${mind.lineCount} lines`,
          mind.description,
          mind.parent ? `(from: ${mind.parent})` : "",
        ].filter(Boolean).join(" • "),
      });
    }

    // Action entries
    items.push({ value: "__create__", label: "  + Create new mind", description: "Start a fresh memory store" });
    items.push({ value: "__edit__", label: "  ✎ Edit current mind", description: "Open in editor" });
    items.push({ value: "__refresh__", label: "  ↻ Refresh current mind", description: "Re-evaluate against codebase" });

    return items;
  }

  async function showMindActions(ctx: ExtensionCommandContext, mindName: string): Promise<void> {
    if (!mindManager || !storage) return;

    const meta = mindManager.readMeta(mindName);
    if (!meta) {
      ctx.ui.notify(`Mind "${mindName}" not found`, "error");
      return;
    }

    const actions: SelectItem[] = [
      { value: "switch", label: "Switch to this mind", description: "Make it the active memory store" },
      { value: "edit", label: "Edit in editor", description: "Open memory.md in editor" },
      { value: "fork", label: "Fork", description: "Create a copy with a new name" },
      { value: "ingest", label: "Ingest into another mind", description: "Merge facts and retire this mind" },
      { value: "status", label: "Change status", description: `Currently: ${meta.status}` },
      { value: "delete", label: "Delete", description: "Remove this mind permanently" },
    ];

    const action = await ctx.ui.custom<string | null>((tui, theme, _kb, done) => {
      const container = new Container();
      container.addChild(new DynamicBorder((s: string) => theme.fg("accent", s)));
      container.addChild(new Text(theme.fg("accent", theme.bold(` Mind: ${mindName} `)), 1, 0));
      container.addChild(new Text(theme.fg("muted", ` ${meta.description}`), 1, 0));

      const selectList = new SelectList(actions, Math.min(actions.length, 10), {
        selectedPrefix: (t) => theme.fg("accent", t),
        selectedText: (t) => theme.fg("accent", t),
        description: (t) => theme.fg("muted", t),
        scrollInfo: (t) => theme.fg("dim", t),
        noMatch: (t) => theme.fg("warning", t),
      });
      selectList.onSelect = (item) => done(item.value);
      selectList.onCancel = () => done(null);
      container.addChild(selectList);
      container.addChild(new Text(theme.fg("dim", " ↑↓ navigate • enter select • esc back"), 1, 0));
      container.addChild(new DynamicBorder((s: string) => theme.fg("accent", s)));

      return {
        render: (w) => container.render(w),
        invalidate: () => container.invalidate(),
        handleInput: (data) => { selectList.handleInput(data); tui.requestRender(); },
      };
    });

    if (!action) return;

    switch (action) {
      case "switch": {
        mindManager.setActiveMind(mindName);
        rebuildStorage(ctx.cwd);
        ctx.ui.notify(`Switched to mind: ${mindName}`, "success");
        updateStatus(ctx);
        break;
      }
      case "edit": {
        const content = mindManager.readMindMemory(mindName);
        const edited = await ctx.ui.editor(`Edit Mind: ${mindName}`, content);
        if (edited !== undefined && edited !== content) {
          mindManager.writeMindMemory(mindName, edited);
          ctx.ui.notify(`Mind "${mindName}" updated`, "success");
        }
        break;
      }
      case "fork": {
        const newName = await ctx.ui.input("New mind name:");
        if (!newName?.trim()) return;
        const sanitized = newName.trim().replace(/[^a-zA-Z0-9_-]/g, "-");
        if (mindManager.mindExists(sanitized)) {
          ctx.ui.notify(`Mind "${sanitized}" already exists`, "error");
          return;
        }
        const desc = await ctx.ui.input("Description:", `Fork of ${mindName}`);
        try {
          mindManager.fork(mindName, sanitized, desc ?? `Fork of ${mindName}`);
          ctx.ui.notify(`Forked "${mindName}" → "${sanitized}"`, "success");
        } catch (err: any) {
          ctx.ui.notify(err.message, "error");
        }
        break;
      }
      case "ingest": {
        const otherMinds = mindManager.list().filter((m) => m.name !== mindName);
        // Build target list: default + other minds
        const targetEntries: { name: string; label: string }[] = [
          { name: "__default__", label: "default (project memory)" },
          ...otherMinds.map((m) => ({ name: m.name, label: `${m.name} (${m.lineCount} lines)` })),
        ];
        if (targetEntries.length === 0) {
          ctx.ui.notify("No targets to ingest into", "warning");
          return;
        }
        const targetIdx = await ctx.ui.select(
          "Ingest into:",
          targetEntries.map((t) => t.label),
        );
        if (targetIdx === undefined) return;
        const targetEntry = targetEntries[targetIdx];
        const targetLabel = targetEntry.name === "__default__" ? "default" : targetEntry.name;
        const ok = await ctx.ui.confirm(
          "Ingest Mind",
          `Merge all facts from "${mindName}" into "${targetLabel}" and retire "${mindName}"?`,
        );
        if (!ok) return;

        let result: { factsIngested: number };
        if (targetEntry.name === "__default__") {
          // Ingest into default memory via a temporary MemoryStorage
          result = mindManager.ingestIntoDefault(mindName, ctx.cwd);
        } else {
          result = mindManager.ingest(mindName, targetEntry.name);
        }
        ctx.ui.notify(`Ingested ${result.factsIngested} facts into "${targetLabel}". "${mindName}" retired.`, "success");
        const wasActive = mindManager.getActiveMindName() === mindName;
        if (wasActive) {
          if (targetEntry.name === "__default__") {
            mindManager.setActiveMind(null);
          } else {
            mindManager.setActiveMind(targetEntry.name);
          }
          rebuildStorage(ctx.cwd);
          updateStatus(ctx);
        }
        break;
      }
      case "status": {
        const statuses = ["active", "refined", "retired"] as const;
        const idx = await ctx.ui.select("New status:", [...statuses]);
        if (idx === undefined) return;
        mindManager.setStatus(mindName, statuses[idx]);
        ctx.ui.notify(`Status of "${mindName}" → ${statuses[idx]}`, "success");
        break;
      }
      case "delete": {
        const ok = await ctx.ui.confirm("Delete Mind", `Permanently delete mind "${mindName}"?`);
        if (!ok) return;
        const wasActive = mindManager.getActiveMindName() === mindName;
        mindManager.delete(mindName);
        if (wasActive) {
          rebuildStorage(ctx.cwd);
          updateStatus(ctx);
        }
        ctx.ui.notify(`Deleted mind: ${mindName}`, "success");
        break;
      }
    }
  }

  function updateStatus(ctx: ExtensionContext): void {
    const activeMind = mindManager?.getActiveMindName();
    if (activeMind) {
      ctx.ui.setStatus("memory", ctx.ui.theme.fg("dim", `mind:${activeMind}`));
    } else {
      ctx.ui.setStatus("memory", undefined);
    }
  }

  // --- Commands ---

  pi.registerCommand("memory", {
    description: "Interactive mind manager — view, switch, create, fork, ingest memory stores",
    getArgumentCompletions: (prefix: string) => {
      const subs = ["edit", "refresh", "clear", "archive"];
      const filtered = subs.filter((s) => s.startsWith(prefix));
      return filtered.length > 0 ? filtered.map((s) => ({ value: s, label: s })) : null;
    },
    handler: async (args, ctx) => {
      if (!storage || !mindManager) {
        ctx.ui.notify("Project memory not initialized", "error");
        return;
      }

      const subcommand = args?.trim().split(/\s+/)[0] ?? "";

      // Direct subcommands (no interactive UI)
      switch (subcommand) {
        case "edit": {
          const memory = storage.readMemory();
          const edited = await ctx.ui.editor("Project Memory:", memory);
          if (edited !== undefined && edited !== memory) {
            storage.writeMemory(edited);
            ctx.ui.notify("Memory updated", "success");
          } else {
            ctx.ui.notify("No changes", "info");
          }
          return;
        }

        case "refresh": {
          ctx.ui.notify("Refreshing memory against codebase...", "info");
          try {
            const currentMemory = storage.readMemory();
            const result = await runExtraction(
              ctx.cwd,
              currentMemory,
              `[Memory refresh requested. No new conversation context — just prune and consolidate existing memory.]`,
              config,
            );
            const { linesWritten, factsArchived } = storage.writeExtractionResult(result);
            ctx.ui.notify(
              `Memory refreshed: ${linesWritten} lines active, ${factsArchived} facts archived`,
              "success",
            );
          } catch (err: any) {
            ctx.ui.notify(`Refresh failed: ${err.message}`, "error");
          }
          return;
        }

        case "clear": {
          const ok = await ctx.ui.confirm("Clear Memory", "Reset current memory to empty template?");
          if (ok) {
            const template = storage.loadTemplate();
            storage.writeMemory(template);
            ctx.ui.notify("Memory cleared", "success");
          }
          return;
        }

        case "archive": {
          const archives = storage.listArchive();
          if (archives.length === 0) {
            ctx.ui.notify("No archive files yet", "info");
          } else {
            const listing = archives.map((a) => `${a.month}: ${a.lines} facts`).join("\n");
            ctx.ui.notify(`Memory Archive:\n${listing}`, "info");
          }
          return;
        }
      }

      // Interactive mind manager
      const minds = mindManager.list();
      const activeName = mindManager.getActiveMindName();
      const items = buildMindItems(minds, activeName);

      const selected = await ctx.ui.custom<string | null>((tui, theme, _kb, done) => {
        const container = new Container();
        container.addChild(new DynamicBorder((s: string) => theme.fg("accent", s)));

        const activeLabel = activeName ?? "default";
        container.addChild(new Text(
          theme.fg("accent", theme.bold(" Memory Minds ")) +
          theme.fg("dim", `(active: ${activeLabel})`),
          1, 0,
        ));

        const selectList = new SelectList(items, Math.min(items.length + 1, 15), {
          selectedPrefix: (t) => theme.fg("accent", t),
          selectedText: (t) => theme.fg("accent", t),
          description: (t) => theme.fg("muted", t),
          scrollInfo: (t) => theme.fg("dim", t),
          noMatch: (t) => theme.fg("warning", t),
        });
        selectList.onSelect = (item) => done(item.value);
        selectList.onCancel = () => done(null);
        container.addChild(selectList);
        container.addChild(new Text(theme.fg("dim", " ↑↓ navigate • enter select/switch • esc close"), 1, 0));
        container.addChild(new DynamicBorder((s: string) => theme.fg("accent", s)));

        return {
          render: (w) => container.render(w),
          invalidate: () => container.invalidate(),
          handleInput: (data) => { selectList.handleInput(data); tui.requestRender(); },
        };
      });

      if (!selected) return;

      // Handle actions
      if (selected === "__create__") {
        const name = await ctx.ui.input("Mind name:");
        if (!name?.trim()) return;
        const sanitized = name.trim().replace(/[^a-zA-Z0-9_-]/g, "-");
        if (mindManager.mindExists(sanitized)) {
          ctx.ui.notify(`Mind "${sanitized}" already exists`, "error");
          return;
        }
        const desc = await ctx.ui.input("Description:");
        mindManager.create(sanitized, desc ?? "");
        const activate = await ctx.ui.confirm("Activate", `Switch to "${sanitized}" now?`);
        if (activate) {
          mindManager.setActiveMind(sanitized);
          rebuildStorage(ctx.cwd);
          updateStatus(ctx);
        }
        ctx.ui.notify(`Created mind: ${sanitized}`, "success");
        return;
      }

      if (selected === "__edit__") {
        const memory = storage.readMemory();
        const edited = await ctx.ui.editor("Edit Current Mind:", memory);
        if (edited !== undefined && edited !== memory) {
          storage.writeMemory(edited);
          ctx.ui.notify("Memory updated", "success");
        }
        return;
      }

      if (selected === "__refresh__") {
        ctx.ui.notify("Refreshing memory against codebase...", "info");
        try {
          const currentMemory = storage.readMemory();
          const result = await runExtraction(
            ctx.cwd,
            currentMemory,
            `[Memory refresh requested. No new conversation context — just prune and consolidate existing memory.]`,
            config,
          );
          const { linesWritten, factsArchived } = storage.writeExtractionResult(result);
          ctx.ui.notify(
            `Memory refreshed: ${linesWritten} lines active, ${factsArchived} facts archived`,
            "success",
          );
        } catch (err: any) {
          ctx.ui.notify(`Refresh failed: ${err.message}`, "error");
        }
        return;
      }

      if (selected === "__default__") {
        if (activeName === null) {
          ctx.ui.notify("Already using default memory", "info");
          return;
        }
        mindManager.setActiveMind(null);
        rebuildStorage(ctx.cwd);
        updateStatus(ctx);
        ctx.ui.notify("Switched to default memory", "success");
        return;
      }

      // Selected an existing mind — show actions
      await showMindActions(ctx, selected);
    },
  });


}
