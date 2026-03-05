/**
 * Design Tree Extension
 *
 * Codifies the interactive design paradigm:
 *   EXPLORE → RESEARCH → CRYSTALLIZE → BRANCH → RECURSE
 *
 * Tracks a tree of design documents, their relationships,
 * statuses, and open questions. Provides commands and tools
 * for navigating and evolving the design space.
 *
 * Design docs use YAML frontmatter for metadata:
 * ---
 * id: document-model
 * title: "Document Model"
 * status: exploring
 * parent: vision
 * dependencies: [collaboration-architecture]
 * open_questions:
 *   - Flat vs nested node properties
 * ---
 */

import type { ExtensionAPI, ExtensionContext } from "@mariozechner/pi-coding-agent";
import { Type } from "@sinclair/typebox";
import { StringEnum } from "@mariozechner/pi-ai";
import { Text } from "@mariozechner/pi-tui";
import * as fs from "node:fs";
import * as path from "node:path";

// ─── Types ───────────────────────────────────────────────────────

type NodeStatus = "seed" | "exploring" | "decided" | "blocked" | "deferred";

const VALID_STATUSES: NodeStatus[] = ["seed", "exploring", "decided", "blocked", "deferred"];

interface DesignNode {
  id: string;
  title: string;
  status: NodeStatus;
  parent?: string;
  dependencies: string[];
  related: string[];
  open_questions: string[];
  filePath: string;
  lastModified: number;
}

interface DesignTree {
  nodes: Map<string, DesignNode>;
  docsDir: string;
}

// ─── Frontmatter Parsing ─────────────────────────────────────────

function parseFrontmatter(content: string): Record<string, unknown> | null {
  const match = content.match(/^---\n([\s\S]*?)\n---/);
  if (!match) return null;

  const yaml = match[1];
  const result: Record<string, unknown> = {};

  let currentKey: string | null = null;
  let currentArray: string[] | null = null;

  for (const line of yaml.split("\n")) {
    // Array item: "  - something"
    const arrayMatch = line.match(/^\s+-\s+(.+)/);
    if (arrayMatch && currentKey) {
      if (!currentArray) currentArray = [];
      currentArray.push(arrayMatch[1].trim().replace(/^["']|["']$/g, ""));
      continue;
    }

    // Flush previous array when we hit a non-array line
    if (currentKey && currentArray !== null) {
      result[currentKey] = currentArray;
      currentArray = null;
      currentKey = null;
    }
    // Key had no array items — treat as empty array
    if (currentKey && currentArray === null) {
      // We were expecting an array but got a new key — empty array
      result[currentKey] = [];
      currentKey = null;
    }

    // Key-value pair
    const kvMatch = line.match(/^(\w[\w_]*):\s*(.*)/);
    if (kvMatch) {
      const key = kvMatch[1];
      const value = kvMatch[2].trim();

      if (value === "" || value === "[]") {
        if (value === "[]") {
          result[key] = [];
        } else {
          // Empty value — might be start of array block
          currentKey = key;
          currentArray = null;
        }
      } else if (value.startsWith("[") && value.endsWith("]")) {
        // Inline array: [a, b, c]
        result[key] = value
          .slice(1, -1)
          .split(",")
          .map((s) => s.trim().replace(/^["']|["']$/g, ""))
          .filter(Boolean);
      } else {
        // Scalar value — strip outer quotes
        result[key] = value.replace(/^["'](.*)["']$/, "$1");
      }
    }
  }

  // Flush trailing array or empty key
  if (currentKey && currentArray !== null) {
    result[currentKey] = currentArray;
  } else if (currentKey) {
    result[currentKey] = [];
  }

  return result;
}

/** Quote a YAML value if it contains special characters */
function yamlQuote(value: string): string {
  if (/[:#\[\]{}&*!|>'"%@`]/.test(value) || value.startsWith("- ")) {
    return `"${value.replace(/"/g, '\\"')}"`;
  }
  return value;
}

function generateFrontmatter(node: Omit<DesignNode, "filePath" | "lastModified">): string {
  let fm = "---\n";
  fm += `id: ${node.id}\n`;
  fm += `title: ${yamlQuote(node.title)}\n`;
  fm += `status: ${node.status}\n`;
  if (node.parent) fm += `parent: ${node.parent}\n`;
  if (node.dependencies.length > 0) {
    fm += `dependencies: [${node.dependencies.join(", ")}]\n`;
  }
  if (node.related.length > 0) {
    fm += `related: [${node.related.join(", ")}]\n`;
  }
  if (node.open_questions.length > 0) {
    fm += "open_questions:\n";
    for (const q of node.open_questions) {
      fm += `  - ${yamlQuote(q)}\n`;
    }
  } else {
    fm += "open_questions: []\n";
  }
  fm += "---\n";
  return fm;
}

/** Replace just the status value in existing frontmatter (preserves everything else) */
function replaceStatus(content: string, newStatus: NodeStatus): string {
  return content.replace(
    /^(---\n[\s\S]*?\nstatus:\s*)\S+/m,
    `$1${newStatus}`
  );
}

// ─── Tree Operations ─────────────────────────────────────────────

function scanDesignDocs(docsDir: string): DesignTree {
  const tree: DesignTree = { nodes: new Map(), docsDir };

  if (!fs.existsSync(docsDir)) return tree;

  const files = fs.readdirSync(docsDir).filter((f) => f.endsWith(".md"));

  for (const file of files) {
    const filePath = path.join(docsDir, file);
    const content = fs.readFileSync(filePath, "utf-8");
    const fm = parseFrontmatter(content);

    if (fm && fm.id) {
      const rawStatus = fm.status as string;
      const status: NodeStatus = VALID_STATUSES.includes(rawStatus as NodeStatus)
        ? (rawStatus as NodeStatus)
        : "exploring";

      const node: DesignNode = {
        id: fm.id as string,
        title: (fm.title as string) || file.replace(".md", ""),
        status,
        parent: fm.parent as string | undefined,
        dependencies: (fm.dependencies as string[]) || [],
        related: (fm.related as string[]) || [],
        open_questions: (fm.open_questions as string[]) || [],
        filePath,
        lastModified: fs.statSync(filePath).mtimeMs,
      };
      tree.nodes.set(node.id, node);
    }
  }

  return tree;
}

function getChildren(tree: DesignTree, parentId: string): DesignNode[] {
  return Array.from(tree.nodes.values()).filter((n) => n.parent === parentId);
}

function getRoots(tree: DesignTree): DesignNode[] {
  return Array.from(tree.nodes.values()).filter((n) => !n.parent);
}

function getAllOpenQuestions(tree: DesignTree): { node: DesignNode; question: string }[] {
  const questions: { node: DesignNode; question: string }[] = [];
  for (const node of tree.nodes.values()) {
    for (const q of node.open_questions) {
      questions.push({ node, question: q });
    }
  }
  return questions;
}

/** Get body content (everything after frontmatter), truncated for context injection */
function getDocBody(filePath: string, maxChars: number = 4000): string {
  const content = fs.readFileSync(filePath, "utf-8");
  const bodyMatch = content.match(/^---\n[\s\S]*?\n---\n([\s\S]*)/);
  const body = bodyMatch ? bodyMatch[1].trim() : content;
  if (body.length <= maxChars) return body;
  return body.slice(0, maxChars) + "\n\n[...truncated]";
}

/** Rewrite a node's frontmatter in place, preserving the document body */
function rewriteFrontmatter(node: DesignNode): void {
  const content = fs.readFileSync(node.filePath, "utf-8");
  const bodyMatch = content.match(/^---\n[\s\S]*?\n---\n([\s\S]*)/);
  const body = bodyMatch ? bodyMatch[1] : "";
  const newFm = generateFrontmatter(node);
  fs.writeFileSync(node.filePath, newFm + "\n" + body);
}

// ─── Status Rendering ────────────────────────────────────────────

const STATUS_ICONS: Record<NodeStatus, string> = {
  seed: "◌",
  exploring: "◐",
  decided: "●",
  blocked: "✕",
  deferred: "◑",
};

const STATUS_COLORS: Record<NodeStatus, string> = {
  seed: "muted",
  exploring: "accent",
  decided: "success",
  blocked: "error",
  deferred: "warning",
};

function renderTreeText(
  tree: DesignTree,
  nodeId: string,
  theme: ExtensionContext["ui"]["theme"],
  prefix: string = "",
  isLast: boolean = true,
  compact: boolean = false,
  visited: Set<string> = new Set()
): string {
  const node = tree.nodes.get(nodeId);
  if (!node) return "";

  // Cycle guard
  if (visited.has(nodeId)) {
    const connector = prefix === "" ? "" : isLast ? "└── " : "├── ";
    return prefix + connector + theme.fg("error", `⟳ ${node.title} (cycle)`);
  }
  visited.add(nodeId);

  const icon = STATUS_ICONS[node.status];
  const color = STATUS_COLORS[node.status] as Parameters<typeof theme.fg>[0];
  const connector = prefix === "" ? "" : isLast ? "└── " : "├── ";
  const childPrefix = prefix === "" ? "" : prefix + (isLast ? "    " : "│   ");

  let line = prefix + connector;
  line += theme.fg(color, `${icon} ${node.title}`);

  if (!compact) {
    line += theme.fg("muted", ` (${node.status})`);
  }

  if (node.open_questions.length > 0) {
    line += theme.fg("dim", ` [${node.open_questions.length}?]`);
  }

  const children = getChildren(tree, nodeId);
  const childLines = children.map((child, i) =>
    renderTreeText(tree, child.id, theme, childPrefix, i === children.length - 1, compact, visited)
  );

  return [line, ...childLines].join("\n");
}

// ─── Extension ───────────────────────────────────────────────────

export default function designTreeExtension(pi: ExtensionAPI): void {
  let tree: DesignTree = { nodes: new Map(), docsDir: "" };
  let focusedNode: string | null = null;
  let compactWidget = false;

  function reload(cwd: string): void {
    const docsDir = path.join(cwd, "docs");
    tree = scanDesignDocs(docsDir);
  }

  function updateWidget(ctx: ExtensionContext): void {
    if (tree.nodes.size === 0) {
      ctx.ui.setWidget("design-tree", undefined);
      return;
    }

    const roots = getRoots(tree);
    const lines: string[] = [];

    const decided = Array.from(tree.nodes.values()).filter((n) => n.status === "decided").length;
    const exploring = Array.from(tree.nodes.values()).filter(
      (n) => n.status === "exploring" || n.status === "seed"
    ).length;
    const total = tree.nodes.size;
    const openQ = getAllOpenQuestions(tree).length;

    // Header
    lines.push(
      ctx.ui.theme.fg("accent", ctx.ui.theme.bold("◈ Design Tree")) +
      ctx.ui.theme.fg("muted", ` ${decided}/${total} decided`) +
      ctx.ui.theme.fg("dim", ` · ${exploring} exploring · ${openQ}?`)
    );

    // Compact mode: single line per root with counts
    if (compactWidget && total > 12) {
      for (const root of roots) {
        const childCount = getChildren(tree, root.id).length;
        const rootIcon = STATUS_ICONS[root.status];
        const rootColor = STATUS_COLORS[root.status] as Parameters<typeof ctx.ui.theme.fg>[0];
        lines.push(
          ctx.ui.theme.fg(rootColor, `  ${rootIcon} ${root.title}`) +
          ctx.ui.theme.fg("dim", ` (${childCount} children)`)
        );
      }
    } else {
      for (const root of roots) {
        lines.push(renderTreeText(tree, root.id, ctx.ui.theme, "", true, total > 15));
      }
    }

    // Focused node indicator
    if (focusedNode) {
      const node = tree.nodes.get(focusedNode);
      if (node) {
        lines.push(
          ctx.ui.theme.fg("accent", `▸ `) +
          ctx.ui.theme.fg("accent", ctx.ui.theme.bold(node.title)) +
          (node.open_questions.length > 0
            ? ctx.ui.theme.fg("dim", ` — ${node.open_questions.length} open questions`)
            : "")
        );
      }
    }

    ctx.ui.setWidget("design-tree", lines, { placement: "belowEditor" });
  }

  /** Set status on a node and write to disk */
  function setNodeStatus(node: DesignNode, newStatus: NodeStatus): void {
    let content = fs.readFileSync(node.filePath, "utf-8");
    content = replaceStatus(content, newStatus);
    fs.writeFileSync(node.filePath, content);
    node.status = newStatus;
  }

  // ─── Commands ────────────────────────────────────────────────

  pi.registerCommand("design", {
    description: "Design tree: status | focus [id] | unfocus | decide [id] | explore [id] | block [id] | defer [id] | branch | frontier | new <id> <title> | update [id] | compact",
    getArgumentCompletions: (prefix: string) => {
      const subcommands = [
        "status", "focus", "unfocus", "decide", "explore",
        "branch", "frontier", "new", "update", "compact",
      ];
      const parts = prefix.split(" ");
      if (parts.length <= 1) {
        return subcommands
          .filter((s) => s.startsWith(prefix))
          .map((s) => ({ value: s, label: s }));
      }
      const sub = parts[0];
      if (["focus", "decide", "explore", "block", "defer", "update"].includes(sub) && parts.length === 2) {
        const partial = parts[1] || "";
        return Array.from(tree.nodes.keys())
          .filter((id) => id.startsWith(partial))
          .map((id) => {
            const node = tree.nodes.get(id)!;
            return { value: `${sub} ${id}`, label: `${id} — ${node.title} (${node.status})` };
          });
      }
      return null;
    },
    handler: async (args, ctx) => {
      reload(ctx.cwd);
      const parts = (args || "status").trim().split(/\s+/);
      const subcommand = parts[0];

      switch (subcommand) {
        case "status": {
          if (tree.nodes.size === 0) {
            ctx.ui.notify(
              "No design documents found in docs/. Create one with /design new <id> <title>",
              "info"
            );
            return;
          }
          updateWidget(ctx);
          ctx.ui.notify(`Design tree: ${tree.nodes.size} nodes`, "info");
          break;
        }

        case "focus": {
          const id = parts[1];
          if (!id) {
            // Interactive selection
            const ids = Array.from(tree.nodes.keys());
            if (ids.length === 0) {
              ctx.ui.notify("No design nodes to focus on", "info");
              return;
            }
            const labels = ids.map((nid) => {
              const n = tree.nodes.get(nid)!;
              const icon = STATUS_ICONS[n.status];
              return `${icon} ${nid} — ${n.title} (${n.open_questions.length}?)`;
            });
            const choice = await ctx.ui.select("Focus on which node?", labels);
            if (!choice) return;
            const selectedId = choice.split(" — ")[0].replace(/^[◌◐●✕◑]\s*/, "");
            focusedNode = selectedId;
          } else {
            const node = tree.nodes.get(id);
            if (!node) {
              ctx.ui.notify(`Node '${id}' not found`, "error");
              return;
            }
            focusedNode = id;

            // Auto-transition seed → exploring when focused
            if (node.status === "seed") {
              setNodeStatus(node, "exploring");
              ctx.ui.notify(`${node.title}: seed → exploring`, "info");
            }
          }
          updateWidget(ctx);

          const node = tree.nodes.get(focusedNode!)!;
          const openQ = node.open_questions.length > 0
            ? `\n\nOpen questions:\n${node.open_questions.map((q, i) => `${i + 1}. ${q}`).join("\n")}`
            : "";

          pi.sendMessage(
            {
              customType: "design-focus",
              content: `[Design Focus: ${node.title} (${node.status})]${openQ}\n\nLet's explore this design space.`,
              display: true,
            },
            { triggerTurn: false }
          );
          break;
        }

        case "unfocus": {
          focusedNode = null;
          updateWidget(ctx);
          ctx.ui.notify("Design focus cleared", "info");
          break;
        }

        case "decide": {
          const id = parts[1] || focusedNode;
          if (!id) {
            ctx.ui.notify("Usage: /design decide <node-id> (or focus a node first)", "warning");
            return;
          }
          const node = tree.nodes.get(id);
          if (!node) {
            ctx.ui.notify(`Node '${id}' not found`, "error");
            return;
          }
          setNodeStatus(node, "decided");
          updateWidget(ctx);
          ctx.ui.notify(`✅ '${node.title}' marked as decided`, "success");
          break;
        }

        case "explore": {
          const id = parts[1] || focusedNode;
          if (!id) {
            ctx.ui.notify("Usage: /design explore <node-id>", "warning");
            return;
          }
          const node = tree.nodes.get(id);
          if (!node) {
            ctx.ui.notify(`Node '${id}' not found`, "error");
            return;
          }
          setNodeStatus(node, "exploring");
          focusedNode = id;
          updateWidget(ctx);
          ctx.ui.notify(`◐ '${node.title}' now exploring`, "info");
          break;
        }

        case "block": {
          const id = parts[1] || focusedNode;
          if (!id) {
            ctx.ui.notify("Usage: /design block <node-id>", "warning");
            return;
          }
          const node = tree.nodes.get(id);
          if (!node) {
            ctx.ui.notify(`Node '${id}' not found`, "error");
            return;
          }
          setNodeStatus(node, "blocked");
          updateWidget(ctx);
          ctx.ui.notify(`✕ '${node.title}' marked as blocked`, "warning");
          break;
        }

        case "defer": {
          const id = parts[1] || focusedNode;
          if (!id) {
            ctx.ui.notify("Usage: /design defer <node-id>", "warning");
            return;
          }
          const node = tree.nodes.get(id);
          if (!node) {
            ctx.ui.notify(`Node '${id}' not found`, "error");
            return;
          }
          setNodeStatus(node, "deferred");
          updateWidget(ctx);
          ctx.ui.notify(`◑ '${node.title}' deferred`, "info");
          break;
        }

        case "frontier": {
          const questions = getAllOpenQuestions(tree);
          if (questions.length === 0) {
            ctx.ui.notify("No open questions in the design tree", "info");
            return;
          }

          const items = questions.map(
            ({ node, question }) => `[${node.id}] ${question}`
          );

          const choice = await ctx.ui.select(
            `Open Questions (${questions.length}):`,
            items
          );

          if (choice) {
            const match = choice.match(/^\[([^\]]+)\]/);
            if (match) {
              focusedNode = match[1];
              updateWidget(ctx);

              const node = tree.nodes.get(match[1])!;
              const question = choice.replace(/^\[[^\]]+\]\s*/, "");

              pi.sendMessage(
                {
                  customType: "design-frontier",
                  content: `[Exploring open question from ${node.title}]\n\nQuestion: ${question}\n\nLet's dig into this.`,
                  display: true,
                },
                { triggerTurn: true }
              );
            }
          }
          break;
        }

        case "branch": {
          let nodeId = focusedNode;
          if (!nodeId) {
            const ids = Array.from(tree.nodes.keys());
            const labels = ids.map((id) => {
              const n = tree.nodes.get(id)!;
              return `${id} — ${n.title} (${n.open_questions.length} questions)`;
            });
            const choice = await ctx.ui.select("Branch from which node?", labels);
            if (!choice) return;
            nodeId = choice.split(" — ")[0];
          }

          const node = tree.nodes.get(nodeId);
          if (!node) return;

          if (node.open_questions.length === 0) {
            ctx.ui.notify(`${node.title} has no open questions to branch from`, "info");
            return;
          }

          const selected = await ctx.ui.select(
            `Branch from '${node.title}' — select a question:`,
            node.open_questions
          );

          if (selected) {
            const suggestedId = selected
              .toLowerCase()
              .replace(/[^a-z0-9]+/g, "-")
              .replace(/^-|-$/g, "")
              .slice(0, 40);
            const newId = await ctx.ui.input("Node ID:", suggestedId);
            if (!newId) return;
            const newTitle = await ctx.ui.input("Title:", selected.slice(0, 60));
            if (!newTitle) return;

            const newNode: Omit<DesignNode, "filePath" | "lastModified"> = {
              id: newId,
              title: newTitle,
              status: "seed",
              parent: nodeId,
              dependencies: [],
              related: [],
              open_questions: [],
            };

            const fm = generateFrontmatter(newNode);
            const docContent =
              fm +
              `\n# ${newTitle}\n\n` +
              `> Parent: [${node.title}](${path.basename(node.filePath)})\n` +
              `> Spawned from: "${selected}"\n\n` +
              `## Overview\n\n*To be explored.*\n\n` +
              `## Open Questions\n\n` +
              `1. *Starting question from parent: ${selected}*\n`;

            const newFilePath = path.join(tree.docsDir, `${newId}.md`);
            fs.writeFileSync(newFilePath, docContent);

            reload(ctx.cwd);
            focusedNode = newId;
            updateWidget(ctx);
            ctx.ui.notify(`Created ${newId}.md — branched from ${node.title}`, "success");
          }
          break;
        }

        case "new": {
          const id = parts[1];
          const title = parts.slice(2).join(" ");
          if (!id || !title) {
            ctx.ui.notify("Usage: /design new <id> <title>", "warning");
            return;
          }

          const newNode: Omit<DesignNode, "filePath" | "lastModified"> = {
            id,
            title,
            status: "seed",
            parent: undefined,
            dependencies: [],
            related: [],
            open_questions: [],
          };

          const fm = generateFrontmatter(newNode);
          const docContent =
            fm +
            `\n# ${title}\n\n` +
            `## Overview\n\n*To be explored.*\n\n` +
            `## Open Questions\n\n`;

          const docsDir = path.join(ctx.cwd, "docs");
          if (!fs.existsSync(docsDir)) fs.mkdirSync(docsDir, { recursive: true });
          fs.writeFileSync(path.join(docsDir, `${id}.md`), docContent);

          reload(ctx.cwd);
          focusedNode = id;
          updateWidget(ctx);
          ctx.ui.notify(`Created ${id}.md`, "success");
          break;
        }

        case "update": {
          const id = parts[1] || focusedNode;
          if (!id) {
            ctx.ui.notify("Usage: /design update <node-id>", "warning");
            return;
          }
          const node = tree.nodes.get(id);
          if (!node) {
            ctx.ui.notify(`Node '${id}' not found`, "error");
            return;
          }

          const action = await ctx.ui.select(`Update '${node.title}':`, [
            "Add open question",
            "Remove open question",
            "Add dependency",
            "Add related node",
            "Change parent",
          ]);

          if (!action) return;

          if (action === "Add open question") {
            const question = await ctx.ui.input("New open question:");
            if (!question) return;
            node.open_questions.push(question);
            rewriteFrontmatter(node);
            reload(ctx.cwd);
            updateWidget(ctx);
            ctx.ui.notify(`Added question to ${node.title}`, "success");
          } else if (action === "Remove open question") {
            if (node.open_questions.length === 0) {
              ctx.ui.notify("No open questions to remove", "info");
              return;
            }
            const toRemove = await ctx.ui.select("Remove which question?", node.open_questions);
            if (!toRemove) return;
            node.open_questions = node.open_questions.filter((q) => q !== toRemove);
            rewriteFrontmatter(node);
            reload(ctx.cwd);
            updateWidget(ctx);
            ctx.ui.notify(`Removed question from ${node.title}`, "success");
          } else if (action === "Add dependency") {
            const otherNodes = Array.from(tree.nodes.keys()).filter(
              (nid) => nid !== id && !node.dependencies.includes(nid)
            );
            if (otherNodes.length === 0) {
              ctx.ui.notify("No available nodes to add as dependency", "info");
              return;
            }
            const depLabels = otherNodes.map((nid) => {
              const n = tree.nodes.get(nid)!;
              return `${nid} — ${n.title}`;
            });
            const depChoice = await ctx.ui.select("Add dependency:", depLabels);
            if (!depChoice) return;
            node.dependencies.push(depChoice.split(" — ")[0]);
            rewriteFrontmatter(node);
            reload(ctx.cwd);
            updateWidget(ctx);
            ctx.ui.notify(`Added dependency: ${depChoice.split(" — ")[0]}`, "success");
          } else if (action === "Add related node") {
            const otherNodes = Array.from(tree.nodes.keys()).filter(
              (nid) => nid !== id && !node.related.includes(nid)
            );
            if (otherNodes.length === 0) {
              ctx.ui.notify("No available nodes to add as related", "info");
              return;
            }
            const relLabels = otherNodes.map((nid) => {
              const n = tree.nodes.get(nid)!;
              return `${nid} — ${n.title}`;
            });
            const relChoice = await ctx.ui.select("Add related:", relLabels);
            if (!relChoice) return;
            node.related.push(relChoice.split(" — ")[0]);
            rewriteFrontmatter(node);
            reload(ctx.cwd);
            updateWidget(ctx);
            ctx.ui.notify(`Added related: ${relChoice.split(" — ")[0]}`, "success");
          } else if (action === "Change parent") {
            const candidates = ["(none — make root)", ...Array.from(tree.nodes.keys())
              .filter((nid) => nid !== id)
              .map((nid) => {
                const n = tree.nodes.get(nid)!;
                return `${nid} — ${n.title}`;
              })];
            const parentChoice = await ctx.ui.select("New parent:", candidates);
            if (!parentChoice) return;
            node.parent = parentChoice.startsWith("(none") ? undefined : parentChoice.split(" — ")[0];
            rewriteFrontmatter(node);
            reload(ctx.cwd);
            updateWidget(ctx);
            ctx.ui.notify(`Parent updated for ${node.title}`, "success");
          }
          break;
        }

        case "compact": {
          compactWidget = !compactWidget;
          updateWidget(ctx);
          ctx.ui.notify(`Widget ${compactWidget ? "compact" : "expanded"} mode`, "info");
          break;
        }

        default:
          ctx.ui.notify(
            "Subcommands: status, focus, unfocus, decide, explore, block, defer, branch, frontier, new, update, compact",
            "info"
          );
      }
    },
  });

  // ─── Tools ──────────────────────────────────────────────────

  pi.registerTool({
    name: "design_tree",
    label: "Design Tree",
    description:
      "Query the design tree: list nodes, show status, find open questions, check dependencies. " +
      "Use to understand the current state of the design space before exploring or deciding.",
    promptSnippet: "Query the design exploration tree — nodes, status, open questions, dependencies",
    promptGuidelines: [
      "Use design_tree to check the state of design documents before creating or modifying them",
      "When the user says 'let's explore X', use design_tree to find the relevant node and its open questions",
      "After a design discussion converges, suggest using /design decide <id> to mark the node as decided",
      "When discussion reveals new sub-topics, suggest /design branch to create child nodes",
    ],
    parameters: Type.Object({
      action: StringEnum([
        "list",
        "node",
        "frontier",
        "dependencies",
        "children",
      ] as const),
      node_id: Type.Optional(
        Type.String({ description: "Node ID (required for node, dependencies, children)" })
      ),
    }),
    async execute(_toolCallId, params, _signal, _onUpdate, ctx) {
      reload(ctx.cwd);

      switch (params.action) {
        case "list": {
          const nodes = Array.from(tree.nodes.values()).map((n) => ({
            id: n.id,
            title: n.title,
            status: n.status,
            parent: n.parent || null,
            open_questions: n.open_questions.length,
            dependencies: n.dependencies,
          }));
          return {
            content: [{ type: "text", text: JSON.stringify(nodes, null, 2) }],
            details: { nodes },
          };
        }

        case "node": {
          if (!params.node_id) {
            return {
              content: [{ type: "text", text: "Error: node_id required" }],
              details: {},
              isError: true,
            };
          }
          const node = tree.nodes.get(params.node_id);
          if (!node) {
            return {
              content: [{ type: "text", text: `Node '${params.node_id}' not found` }],
              details: {},
              isError: true,
            };
          }
          const body = getDocBody(node.filePath);
          const children = getChildren(tree, node.id).map((c) => c.id);
          return {
            content: [
              {
                type: "text",
                text:
                  JSON.stringify({ ...node, children }, null, 2) +
                  "\n\n--- Document Content ---\n\n" +
                  body,
              },
            ],
            details: { node },
          };
        }

        case "frontier": {
          const questions = getAllOpenQuestions(tree);
          const grouped: Record<string, string[]> = {};
          for (const { node, question } of questions) {
            if (!grouped[node.id]) grouped[node.id] = [];
            grouped[node.id].push(question);
          }
          return {
            content: [
              {
                type: "text",
                text:
                  `${questions.length} open questions across ${Object.keys(grouped).length} nodes:\n\n` +
                  Object.entries(grouped)
                    .map(
                      ([id, qs]) =>
                        `## ${tree.nodes.get(id)?.title || id}\n${qs.map((q, i) => `  ${i + 1}. ${q}`).join("\n")}`
                    )
                    .join("\n\n"),
              },
            ],
            details: { questions: grouped },
          };
        }

        case "dependencies": {
          if (!params.node_id) {
            return {
              content: [{ type: "text", text: "Error: node_id required" }],
              details: {},
              isError: true,
            };
          }
          const node = tree.nodes.get(params.node_id);
          if (!node) {
            return {
              content: [{ type: "text", text: `Node '${params.node_id}' not found` }],
              details: {},
              isError: true,
            };
          }
          const deps = node.dependencies
            .map((id) => tree.nodes.get(id))
            .filter(Boolean)
            .map((n) => ({ id: n!.id, title: n!.title, status: n!.status }));
          const dependents = Array.from(tree.nodes.values())
            .filter((n) => n.dependencies.includes(params.node_id!))
            .map((n) => ({ id: n.id, title: n.title, status: n.status }));

          return {
            content: [
              {
                type: "text",
                text:
                  `Dependencies of ${node.title}:\n` +
                  JSON.stringify({ depends_on: deps, depended_by: dependents }, null, 2),
              },
            ],
            details: { depends_on: deps, depended_by: dependents },
          };
        }

        case "children": {
          if (!params.node_id) {
            return {
              content: [{ type: "text", text: "Error: node_id required" }],
              details: {},
              isError: true,
            };
          }
          const children = getChildren(tree, params.node_id).map((c) => ({
            id: c.id,
            title: c.title,
            status: c.status,
            open_questions: c.open_questions.length,
          }));
          return {
            content: [
              {
                type: "text",
                text: `Children of ${params.node_id}:\n${JSON.stringify(children, null, 2)}`,
              },
            ],
            details: { children },
          };
        }
      }

      return { content: [{ type: "text", text: "Unknown action" }], details: {} };
    },

    renderCall(args, theme) {
      let text = theme.fg("toolTitle", theme.bold("design_tree "));
      text += theme.fg("accent", args.action);
      if (args.node_id) text += " " + theme.fg("dim", args.node_id);
      return new Text(text, 0, 0);
    },

    renderResult(result, { expanded }, theme) {
      if (result.isError) {
        return new Text(theme.fg("error", result.content?.[0]?.text || "Error"), 0, 0);
      }

      const details = result.details || {};
      let text = "";

      if (details.nodes) {
        const nodes = details.nodes as Array<{ id: string; status: string; open_questions: number }>;
        text = theme.fg("success", `${nodes.length} nodes`) + "\n";
        if (expanded) {
          for (const n of nodes) {
            const icon = STATUS_ICONS[n.status as NodeStatus] || "?";
            const color = STATUS_COLORS[n.status as NodeStatus] || "muted";
            text += theme.fg(color as Parameters<typeof theme.fg>[0], `  ${icon} ${n.id}`) +
              (n.open_questions > 0 ? theme.fg("dim", ` [${n.open_questions}?]`) : "") + "\n";
          }
        }
      } else if (details.node) {
        const n = details.node as DesignNode;
        text = theme.fg("accent", `${STATUS_ICONS[n.status]} ${n.title}`) +
          theme.fg("muted", ` (${n.status})`) +
          (n.open_questions.length > 0 ? theme.fg("dim", ` — ${n.open_questions.length} questions`) : "");
      } else if (details.questions) {
        const q = details.questions as Record<string, string[]>;
        const total = Object.values(q).flat().length;
        text = theme.fg("warning", `${total} open questions`);
      } else {
        text = result.content?.[0]?.text?.slice(0, 100) || "Done";
      }

      return new Text(text, 0, 0);
    },
  });

  // ─── Context Injection ──────────────────────────────────────

  pi.on("before_agent_start", async (_event, ctx) => {
    reload(ctx.cwd);
    if (tree.nodes.size === 0) return;

    // If a node is focused, inject its full context
    if (focusedNode) {
      const node = tree.nodes.get(focusedNode);
      if (node) {
        const openQ =
          node.open_questions.length > 0
            ? `\n\nOpen questions:\n${node.open_questions.map((q, i) => `${i + 1}. ${q}`).join("\n")}`
            : "";
        const deps = node.dependencies
          .map((id) => {
            const d = tree.nodes.get(id);
            return d ? `- ${d.title} (${d.status})` : null;
          })
          .filter(Boolean)
          .join("\n");
        const depsText = deps ? `\nDependencies:\n${deps}` : "";

        // Include the document body so the LLM can reference the actual analysis
        const body = getDocBody(node.filePath, 6000);

        return {
          message: {
            customType: "design-context",
            content:
              `[Design Tree — Focused on: ${node.title} (${node.status})]` +
              depsText +
              openQ +
              `\n\n--- Document Summary ---\n${body}` +
              `\n\nWhen this design discussion reaches a conclusion, suggest /design decide ${node.id}. ` +
              `If new sub-topics emerge, suggest /design branch to create child nodes.`,
            display: false,
          },
        };
      }
    }

    // Otherwise, provide a compact summary (no full doc content)
    const decided = Array.from(tree.nodes.values()).filter(
      (n) => n.status === "decided"
    ).length;
    const exploring = Array.from(tree.nodes.values()).filter(
      (n) => n.status === "exploring" || n.status === "seed"
    ).length;
    const totalQ = getAllOpenQuestions(tree).length;

    return {
      message: {
        customType: "design-context",
        content:
          `[Design Tree: ${tree.nodes.size} nodes — ${decided} decided, ${exploring} exploring, ${totalQ} open questions]\n` +
          `Use the design_tree tool to query the design space. Suggest /design frontier to explore open questions.`,
        display: false,
      },
    };
  });

  // Filter stale design-context messages — keep only the most recent one
  pi.on("context", async (event) => {
    let foundLatest = false;
    // Walk backwards so the first design-context we encounter (most recent) is kept
    const keep = new Array(event.messages.length).fill(true);
    for (let i = event.messages.length - 1; i >= 0; i--) {
      const msg = event.messages[i] as { customType?: string };
      if (msg.customType === "design-context") {
        if (!foundLatest) {
          foundLatest = true; // keep this one
        } else {
          keep[i] = false;
        }
      }
    }
    if (foundLatest) {
      const filtered = event.messages.filter((_, i) => keep[i]);
      if (filtered.length !== event.messages.length) {
        return { messages: filtered };
      }
    }
  });

  // ─── Message Renderer ───────────────────────────────────────

  pi.registerMessageRenderer("design-focus", (message, _options, theme) => {
    const titleMatch = (message.content as string).match(/\[Design Focus: (.+?)\]/);
    const title = titleMatch ? titleMatch[1] : "Unknown";
    let text = theme.fg("accent", theme.bold(`◈ Focus → ${title}`));

    const questionsMatch = (message.content as string).match(/Open questions:\n([\s\S]*?)(?:\n\n|$)/);
    if (questionsMatch) {
      const lines = questionsMatch[1].split("\n").filter(Boolean);
      for (const line of lines) {
        text += "\n  " + theme.fg("dim", line);
      }
    }
    return new Text(text, 0, 0);
  });

  pi.registerMessageRenderer("design-frontier", (message, _options, theme) => {
    const questionMatch = (message.content as string).match(/Question: (.+)/);
    const question = questionMatch ? questionMatch[1] : "Unknown";
    let text = theme.fg("warning", theme.bold("◈ Frontier")) + " ";
    text += theme.fg("muted", question);
    return new Text(text, 0, 0);
  });

  // ─── Session Lifecycle ──────────────────────────────────────

  pi.on("session_start", async (_event, ctx) => {
    reload(ctx.cwd);

    // Restore focused node from session
    const entries = ctx.sessionManager.getEntries();
    const focusEntry = entries
      .filter(
        (e: { type: string; customType?: string }) =>
          e.type === "custom" && e.customType === "design-tree-focus"
      )
      .pop() as { data?: { focusedNode: string | null } } | undefined;

    if (focusEntry?.data?.focusedNode) {
      focusedNode = focusEntry.data.focusedNode;
    }

    if (tree.nodes.size > 0) {
      updateWidget(ctx);
    }
  });

  // Persist focus state
  pi.on("agent_end", async () => {
    if (tree.nodes.size > 0) {
      pi.appendEntry("design-tree-focus", { focusedNode });
    }
  });
}
