/**
 * Secrets Extension
 *
 * Resolves secrets from user-configured sources (env vars, shell commands, keychains)
 * without duplicating or storing secret values. Provides:
 *
 * Layer 1: resolveSecret() — extensions call this to get secrets from user-configured recipes
 * Layer 2: Output redaction — scrubs known secret values from tool results before they reach the agent
 * Layer 3: Bash guard — confirms before commands that access secret stores
 * Layer 4: Recipe file — stores resolution recipes, never literal secrets
 * Layer 5: Local model scrub — redacts secrets from outbound ask_local_model prompts
 *
 * Commands: /secrets list, /secrets configure <name>, /secrets rm <name>, /secrets test <name>
 */

import type { ExtensionAPI } from "@mariozechner/pi-coding-agent";
import { existsSync, readFileSync, writeFileSync, mkdirSync, readdirSync } from "fs";
import { join } from "path";
import { homedir } from "os";
import { execSync } from "child_process";

// ============================================================================
// Config
// ============================================================================

const SECRETS_DIR = join(homedir(), ".pi", "agent");
const SECRETS_FILE = join(SECRETS_DIR, "secrets.json");

/** Fallback secrets not tied to a specific extension */
const BUILTIN_SECRETS: Record<string, string> = {
  ANTHROPIC_API_KEY: "Anthropic API key (offline-driver health check)",
};

/**
 * Scan extension directories for // @secret annotations.
 * Format: // @secret NAME "description"
 */
function scanSecretAnnotations(): Record<string, string> {
  const secrets: Record<string, string> = { ...BUILTIN_SECRETS };
  const pattern = /^\/\/\s*@secret\s+([A-Z_][A-Z0-9_]*)\s+"([^"]+)"/;

  // Extension directories to scan
  const extensionDirs = [
    join(homedir(), ".pi", "agent", "extensions"),
    join(homedir(), ".pi", "agent", "git"),  // pi-kit and other git packages
  ];

  // Also scan project-local extensions
  try {
    const cwd = process.cwd();
    const projectDir = join(cwd, ".pi", "extensions");
    if (existsSync(projectDir)) extensionDirs.push(projectDir);
  } catch {}

  function scanFile(filePath: string) {
    try {
      const content = readFileSync(filePath, "utf-8");
      // Only scan the first 30 lines for annotations (they should be at the top)
      const lines = content.split("\n").slice(0, 30);
      for (const line of lines) {
        const match = line.match(pattern);
        if (match) {
          secrets[match[1]] = match[2];
        }
      }
    } catch {}
  }

  function walkDir(dir: string) {
    if (!existsSync(dir)) return;
    try {
      const entries = readdirSync(dir, { withFileTypes: true });
      for (const entry of entries) {
        const fullPath = join(dir, entry.name);
        if (entry.isFile() && (entry.name.endsWith(".ts") || entry.name.endsWith(".js"))) {
          scanFile(fullPath);
        } else if (entry.isDirectory() && !entry.name.startsWith(".") && entry.name !== "node_modules") {
          walkDir(fullPath);
        }
      }
    } catch {}
  }

  for (const dir of extensionDirs) {
    walkDir(dir);
  }

  return secrets;
}

/** Discovered secrets — scanned once at load time */
const KNOWN_SECRETS = scanSecretAnnotations();

// ============================================================================
// Recipe types
// ============================================================================

/**
 * Recipe format:
 * - "!command args"  → shell command, stdout is the secret
 * - "ENV_VAR_NAME"   → read from environment variable
 * - "literal:value"  → literal value (discouraged, warned about)
 */
type RecipeMap = Record<string, string>;

// ============================================================================
// State — resolved secrets cached in memory, never written to disk
// ============================================================================

let recipes: RecipeMap = {};
const resolvedCache = new Map<string, string>();

// ============================================================================
// Core: Recipe loading
// ============================================================================

function loadRecipes(): RecipeMap {
  if (!existsSync(SECRETS_FILE)) return {};
  try {
    const raw = readFileSync(SECRETS_FILE, "utf-8");
    return JSON.parse(raw) as RecipeMap;
  } catch {
    return {};
  }
}

function saveRecipes(r: RecipeMap): void {
  mkdirSync(SECRETS_DIR, { recursive: true });
  writeFileSync(SECRETS_FILE, JSON.stringify(r, null, 2) + "\n", { mode: 0o600 });
}

// ============================================================================
// Core: Secret resolution
// ============================================================================

function executeRecipe(recipe: string): string | undefined {
  // Shell command
  if (recipe.startsWith("!")) {
    try {
      const cmd = recipe.slice(1).trim();
      const result = execSync(cmd, {
        encoding: "utf-8",
        timeout: 10_000,
        stdio: ["pipe", "pipe", "pipe"],
      }).trim();
      return result || undefined;
    } catch {
      return undefined;
    }
  }

  // Literal value (discouraged)
  if (recipe.startsWith("literal:")) {
    return recipe.slice(8);
  }

  // Environment variable name
  return process.env[recipe] || undefined;
}

/**
 * Resolve a secret by name. Resolution order:
 * 1. In-memory cache (already resolved this session)
 * 2. process.env[name] — always checked first for CI/container compat
 * 3. Recipe from secrets.json
 * 4. undefined — caller handles missing secret gracefully
 */
export function resolveSecret(name: string): string | undefined {
  // Check cache
  const cached = resolvedCache.get(name);
  if (cached) return cached;

  // Always check env first (CI, containers, explicit overrides)
  const envVal = process.env[name];
  if (envVal) {
    resolvedCache.set(name, envVal);
    return envVal;
  }

  // Check recipe
  const recipe = recipes[name];
  if (!recipe) return undefined;

  const value = executeRecipe(recipe);
  if (value) {
    resolvedCache.set(name, value);
  }
  return value;
}

// ============================================================================
// Layer 2: Output redaction
// ============================================================================

function redactString(input: string, secrets: Array<{ name: string; value: string }>): string {
  let result = input;
  for (const { name, value } of secrets) {
    if (value.length < 8) continue; // Don't redact very short values (too many false positives)

    // Escape regex special characters in the secret value
    const escaped = value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    const marker = `[REDACTED:${name}]`;

    // Replace all occurrences of the full value
    result = result.replace(new RegExp(escaped, "g"), marker);

    // Also redact partial prefixes (first 20 chars) for long secrets that may be truncated
    if (value.length > 24) {
      const partialEscaped = value.slice(0, 20).replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
      result = result.replace(new RegExp(partialEscaped, "g"), marker);
    }
  }
  return result;
}

function redactContent(content: any[]): any[] {
  const secrets = Array.from(resolvedCache.entries())
    .filter(([_, v]) => v.length >= 8)
    .map(([name, value]) => ({ name, value }));

  if (secrets.length === 0) return content;

  return content.map((block: any) => {
    if (block.type === "text" && typeof block.text === "string") {
      const redacted = redactString(block.text, secrets);
      if (redacted !== block.text) {
        return { ...block, text: redacted };
      }
    }
    return block;
  });
}

// ============================================================================
// Layer 3: Bash guard patterns
// ============================================================================

const SECRET_ACCESS_PATTERNS = [
  // macOS Keychain
  /\bsecurity\s+find-generic-password/i,
  /\bsecurity\s+find-internet-password/i,
  // Bitwarden
  /\bbw\s+(get|list)\b/i,
  // 1Password
  /\bop\s+(read|get|item)\b/i,
  // pass (GPG password store)
  /\bpass\s+(show|ls)\b/i,
  // Vault
  /\bvault\s+(read|kv\s+get)\b/i,
  // Environment variable dumping
  /\benv\b.*\b(key|token|secret|password|credential)/i,
  /\bprintenv\b.*\b(key|token|secret|password|credential)/i,
  /\bset\b.*\b(key|token|secret|password|credential)/i,
  // Echo/cat of known secret env vars
  /\becho\s+\$[A-Z_]*(KEY|TOKEN|SECRET|PASSWORD|CREDENTIAL)/i,
  /\bcat\b.*\b(secrets?|credentials?|\.env)\b/i,
  // AWS/GCP credential files
  /\bcat\b.*\.(aws|gcloud)\/credentials/i,
  // Our own secrets file — match the specific path, not just any filename mention
  /\.pi\/agent\/secrets\.json/i,
  // Writing to secrets file (via tee, redirect, etc.)
  />\s*.*\.pi\/agent\/secrets\.json/i,
];

function isSecretAccessCommand(command: string): boolean {
  return SECRET_ACCESS_PATTERNS.some((p) => p.test(command));
}

// ============================================================================
// Bitwarden vault health check
// ============================================================================

function bwVaultStatus(): "unlocked" | "locked" | "unauthenticated" | null {
  try {
    const raw = execSync("bw status 2>/dev/null", {
      encoding: "utf-8",
      timeout: 5_000,
      stdio: ["pipe", "pipe", "pipe"],
    }).trim();
    const obj = JSON.parse(raw);
    return obj.status ?? null;
  } catch {
    return null;
  }
}

function recipesUsingBw(): string[] {
  return Object.keys(recipes).filter((k) => /\bbw\b/.test(recipes[k]));
}

// ============================================================================
// macOS Keychain helpers
// ============================================================================

const KEYCHAIN_ACCOUNT = "pi-kit";
const KEYCHAIN_SERVICE_PREFIX = "pi-kit/";

/**
 * Store a value in the macOS login keychain under service "pi-kit/<name>".
 * macOS will prompt Touch ID / password / smart card automatically if the
 * keychain is locked — the OS owns the auth flow, we just call the command.
 */
function storeInKeychain(secretName: string, value: string): void {
  // Use -U to update if item already exists
  execSync(
    `security add-generic-password -U -a ${JSON.stringify(KEYCHAIN_ACCOUNT)} -s ${JSON.stringify(KEYCHAIN_SERVICE_PREFIX + secretName)} -w ${JSON.stringify(value)}`,
    { stdio: ["pipe", "pipe", "pipe"], timeout: 30_000 }
  );
}

function keychainRecipe(secretName: string): string {
  return `!security find-generic-password -a ${JSON.stringify(KEYCHAIN_ACCOUNT)} -ws ${JSON.stringify(KEYCHAIN_SERVICE_PREFIX + secretName)}`;
}

// ============================================================================
// Extension
// ============================================================================

export default function (pi: ExtensionAPI) {
  // Load recipes on init
  recipes = loadRecipes();

  // Pre-resolve all configured secrets at init time (Layer 1)
  // Resolved values are injected into process.env so other extensions
  // can keep using process.env.X without importing from this module.
  // This means the secrets extension MUST load before other extensions
  // that consume secrets (pi loads extensions in alphabetical order by
  // directory name, so "secrets" loads before "web-search" etc.)
  for (const name of Object.keys(recipes)) {
    const value = resolveSecret(name);
    if (value && !process.env[name]) {
      process.env[name] = value;
    }
  }
  // Also track known secrets already in env (for CI compat + redaction)
  for (const name of Object.keys(KNOWN_SECRETS)) {
    if (process.env[name] && !resolvedCache.has(name)) {
      resolvedCache.set(name, process.env[name]!);
    }
  }

  pi.on("session_start", async (_event, ctx) => {
    // Bitwarden vault health check — warn early if vault is locked
    const bwKeys = recipesUsingBw();
    if (bwKeys.length > 0) {
      const vaultStatus = bwVaultStatus();
      if (vaultStatus === "locked") {
        ctx.ui.notify(
          `🔒 Bitwarden vault is locked\n\n` +
          `${bwKeys.length} secret${bwKeys.length > 1 ? "s use" : " uses"} Bitwarden (${bwKeys.join(", ")}) ` +
          `and won't resolve until the vault is unlocked.\n\n` +
          `Unlock with:  bw unlock --raw\n` +
          `Then run:     export BW_SESSION=<key>\n\n` +
          `If the Bitwarden desktop app is running with biometrics enabled,\n` +
          `you can also use:  bw unlock --biometrics`,
          "warn"
        );
      } else if (vaultStatus === "unauthenticated") {
        ctx.ui.notify(
          `🔐 Not logged in to Bitwarden\n\n` +
          `${bwKeys.length} secret${bwKeys.length > 1 ? "s use" : " uses"} Bitwarden but no session exists.\n\n` +
          `Log in with:  bw login\n` +
          `Then unlock:  bw unlock --raw\n` +
          `Then run:     export BW_SESSION=<key>`,
          "warn"
        );
      }
    }

    const count = resolvedCache.size;
    if (count > 0) {
      ctx.ui.notify(
        `🔐 ${count} secret${count !== 1 ? "s" : ""} resolved (${Array.from(resolvedCache.keys()).join(", ")})`,
        "info"
      );
    }
  });

  // ──────────────────────────────────────────────────────────────
  // Layer 2: Redact secrets from tool results
  // ──────────────────────────────────────────────────────────────

  pi.on("tool_result", async (event, _ctx) => {
    if (!event.content || resolvedCache.size === 0) return undefined;

    const redacted = redactContent(event.content);
    // Only return if we actually changed something
    const changed = redacted.some(
      (block: any, i: number) =>
        block.type === "text" &&
        event.content[i]?.type === "text" &&
        block.text !== (event.content[i] as any).text
    );

    if (changed) {
      return { content: redacted };
    }
    return undefined;
  });

  // ──────────────────────────────────────────────────────────────
  // Layer 3 + 5: Bash guard and local model scrub (single handler)
  // ──────────────────────────────────────────────────────────────

  pi.on("tool_call", async (event, ctx) => {
    // Guard: block write/edit to secrets.json
    if (event.toolName === "write" || event.toolName === "edit") {
      const path = (event.input as any).path as string;
      if (path && /\.pi\/agent\/secrets\.json/i.test(path)) {
        return {
          block: true,
          reason: "🔐 Blocked: use /secrets configure to manage secret recipes, not direct file writes.",
        };
      }
    }

    // Layer 3: Bash guard — confirm before secret-access commands
    if (event.toolName === "bash") {
      const command = (event.input as any).command as string;
      if (isSecretAccessCommand(command)) {
        if (!ctx.hasUI) {
          return {
            block: true,
            reason: "🔐 Blocked: command accesses secret store (no UI for confirmation)",
          };
        }

        const choice = await ctx.ui.select(
          `🔐 This command accesses a secret store:\n\n  ${command}\n\nAllow?`,
          ["Yes, allow this time", "No, block it"]
        );

        if (choice !== "Yes, allow this time") {
          return { block: true, reason: "🔐 Blocked by user: secret store access" };
        }
      }
      return undefined;
    }

    // Layer 5: Scrub secrets from local model prompts
    if (event.toolName === "ask_local_model") {
      const input = event.input as any;
      if (!input.prompt || resolvedCache.size === 0) return undefined;

      const secrets = Array.from(resolvedCache.entries())
        .filter(([_, v]) => v.length >= 8)
        .map(([name, value]) => ({ name, value }));

      const cleanPrompt = redactString(input.prompt, secrets);
      const cleanSystem = input.system ? redactString(input.system, secrets) : input.system;

      if (cleanPrompt !== input.prompt || cleanSystem !== input.system) {
        return {
          block: true,
          reason:
            "🔐 Blocked: prompt to local model contains secret values. " +
            "Remove sensitive data before delegating to local inference.",
        };
      }
    }

    return undefined;
  });

  // ──────────────────────────────────────────────────────────────
  // Commands: /secrets list | configure | rm | test
  // ──────────────────────────────────────────────────────────────

  pi.registerCommand("secrets", {
    description: "Manage secret resolution recipes: list, configure <name>, rm <name>, test <name>",
    getArgumentCompletions: (prefix: string) => {
      const parts = prefix.split(/\s+/);
      if (parts.length <= 1) {
        // Complete subcommand
        const subs = ["list", "configure", "rm", "test"];
        const filtered = subs.filter(s => s.startsWith(parts[0] || ""));
        return filtered.length > 0 ? filtered.map(s => ({ value: s, label: s })) : null;
      }
      const sub = parts[0];
      if (sub === "configure" || sub === "rm" || sub === "test") {
        // Complete secret name
        const namePrefix = parts.slice(1).join(" ");
        const allNames = [
          ...Object.keys(KNOWN_SECRETS),
          ...Object.keys(recipes).filter(k => !(k in KNOWN_SECRETS)),
        ];
        const filtered = allNames.filter(n => n.startsWith(namePrefix));
        return filtered.length > 0
          ? filtered.map(n => ({ value: `${sub} ${n}`, label: `${n}  ${KNOWN_SECRETS[n] || "custom"}` }))
          : null;
      }
      return null;
    },
    handler: async (args, ctx) => {
      const parts = (args || "").trim().split(/\s+/);
      const subcommand = parts[0] || "list";
      const secretName = parts.slice(1).join(" ");

      switch (subcommand) {
        case "list": {
          const lines: string[] = ["Secret recipes (~/.pi/agent/secrets.json):", ""];

          for (const [name, desc] of Object.entries(KNOWN_SECRETS)) {
            const recipe = recipes[name];
            const resolved = resolvedCache.has(name);
            const source = recipe
              ? recipe.startsWith("!")
                ? `command: ${recipe.slice(1, 40)}${recipe.length > 41 ? "..." : ""}`
                : recipe.startsWith("literal:")
                  ? "⚠️  literal value (insecure)"
                  : `env: ${recipe}`
              : resolved
                ? "env (auto-detected)"
                : "not configured";

            const status = resolved ? "✅" : "❌";
            lines.push(`  ${status} ${name}`);
            lines.push(`     ${desc}`);
            lines.push(`     Source: ${source}`);
            lines.push("");
          }

          // Show any non-known secrets
          for (const name of Object.keys(recipes)) {
            if (name in KNOWN_SECRETS) continue;
            const recipe = recipes[name];
            const resolved = resolvedCache.has(name);
            const status = resolved ? "✅" : "❌";
            lines.push(`  ${status} ${name} (custom)`);
            lines.push(
              `     Source: ${recipe.startsWith("!") ? `command: ${recipe.slice(1, 40)}` : recipe.startsWith("literal:") ? "⚠️  literal" : `env: ${recipe}`}`
            );
            lines.push("");
          }

          ctx.ui.notify(lines.join("\n"), "info");
          break;
        }

        case "configure": {
          if (!secretName) {
            ctx.ui.notify("Usage: /secrets configure <NAME>", "error");
            return;
          }

          if (!ctx.hasUI) {
            ctx.ui.notify("Cannot configure secrets without interactive UI", "error");
            return;
          }

          const desc = KNOWN_SECRETS[secretName] || "Custom secret";
          const currentRecipe = recipes[secretName];

          // Check which backends are available
          let hasBw = false;
          let hasOp = false;
          let hasKeychain = false;
          try { execSync("which bw", { stdio: "pipe" }); hasBw = true; } catch {}
          try { execSync("which op", { stdio: "pipe" }); hasOp = true; } catch {}
          try { execSync("which security", { stdio: "pipe" }); hasKeychain = true; } catch {}

          const options: string[] = [];

          if (hasBw) {
            options.push("Bitwarden — look up from your vault (recommended)");
          }
          if (hasKeychain) {
            options.push("macOS Keychain — read from login keychain");
          }
          if (hasOp) {
            options.push("1Password — read via op CLI");
          }
          options.push(
            `Environment variable — reads $${secretName} at runtime`,
            "Shell command — custom command (stdout = secret value)",
          );
          if (hasKeychain) {
            options.push("Enter value → store in macOS Keychain (Touch ID / password protected)");
          } else {
            options.push("Paste value — enter the value now (⚠️ stored in plaintext)");
          }
          if (currentRecipe) {
            options.push("Remove this secret's recipe");
          }

          const statusLine = currentRecipe
            ? `Current: ${currentRecipe.startsWith("literal:") ? "literal (hidden)" : currentRecipe}`
            : "Not configured";

          const choice = await ctx.ui.select(
            `Configure: ${secretName}\n${desc}\n${statusLine}\n\nChoose how to resolve this secret:`,
            options
          );

          if (!choice) return;

          if (choice.startsWith("Bitwarden")) {
            // Guide through Bitwarden lookup
            ctx.ui.notify("Searching Bitwarden vault...\nMake sure you're logged in (bw login) and the vault is unlocked (bw unlock).", "info");

            // Check BW status
            let bwStatus = "";
            try {
              bwStatus = execSync("bw status 2>/dev/null", { encoding: "utf-8" }).trim();
            } catch {}

            const statusObj = bwStatus ? JSON.parse(bwStatus) : {};

            if (statusObj.status === "unauthenticated") {
              ctx.ui.notify("❌ Not logged in to Bitwarden.\n\nRun:  bw login\nThen: bw unlock\nThen retry /secrets configure " + secretName, "error");
              return;
            }

            if (statusObj.status === "locked") {
              ctx.ui.notify("🔒 Bitwarden vault is locked.\n\nRun:  bw unlock\nCopy the BW_SESSION export command and run it.\nThen retry /secrets configure " + secretName, "error");
              return;
            }

            // Search for items matching the secret name
            const searchTerm = secretName.toLowerCase().replace(/_/g, " ");
            let items: any[] = [];
            try {
              const raw = execSync(`bw list items --search "${searchTerm}" 2>/dev/null`, {
                encoding: "utf-8",
                timeout: 15_000,
                env: { ...process.env },
              }).trim();
              items = JSON.parse(raw || "[]");
            } catch (e: any) {
              ctx.ui.notify(`❌ Bitwarden search failed. Is your vault unlocked?\n\nRun: bw unlock\nExport the BW_SESSION, then retry.`, "error");
              return;
            }

            if (items.length === 0) {
              // No results — offer to search with custom term or enter item ID
              const alt = await ctx.ui.input(
                `No Bitwarden items found for "${searchTerm}".\n\nEnter a search term, item name, or item ID (or leave empty to cancel):`
              );
              if (!alt) return;

              try {
                const raw = execSync(`bw list items --search "${alt}" 2>/dev/null`, {
                  encoding: "utf-8",
                  timeout: 15_000,
                  env: { ...process.env },
                }).trim();
                items = JSON.parse(raw || "[]");
              } catch {}

              if (items.length === 0) {
                ctx.ui.notify(`No items found for "${alt}" either. Create the entry in Bitwarden first, then retry.`, "error");
                return;
              }
            }

            // Let user pick which item
            const itemChoices = items.map((item: any) => {
              const folder = item.folderId ? ` [folder]` : "";
              const hasNotes = item.notes ? " 📝" : "";
              const fieldCount = item.fields?.length || 0;
              const fields = fieldCount > 0 ? ` (${fieldCount} custom fields)` : "";
              return `${item.name}${folder}${hasNotes}${fields}  — id:${item.id.slice(0, 8)}`;
            });

            const picked = items.length === 1
              ? itemChoices[0]
              : await ctx.ui.select(`Found ${items.length} items. Which one has the secret?`, itemChoices);
            if (!picked) return;

            const pickedIdx = itemChoices.indexOf(picked);
            const pickedItem = items[pickedIdx];

            // Determine which field to read
            const fieldOptions: string[] = [];
            if (pickedItem.login?.password) fieldOptions.push("Password field");
            if (pickedItem.login?.username) fieldOptions.push("Username field");
            if (pickedItem.notes) fieldOptions.push("Notes field");
            if (pickedItem.fields) {
              for (const f of pickedItem.fields) {
                fieldOptions.push(`Custom field: ${f.name}`);
              }
            }

            let bwRecipe: string;

            if (fieldOptions.length === 0) {
              ctx.ui.notify("❌ This item has no readable fields (no password, notes, or custom fields).", "error");
              return;
            } else if (fieldOptions.length === 1) {
              // Only one option, use it
              const fo = fieldOptions[0];
              if (fo === "Password field") {
                bwRecipe = `!bw get password "${pickedItem.name}"`;
              } else if (fo === "Username field") {
                bwRecipe = `!bw get username "${pickedItem.name}"`;
              } else if (fo === "Notes field") {
                bwRecipe = `!bw get notes "${pickedItem.name}"`;
              } else {
                const fieldName = fo.replace("Custom field: ", "");
                bwRecipe = `!bw get item "${pickedItem.name}" | jq -r '.fields[] | select(.name=="${fieldName}") | .value'`;
              }
            } else {
              const fieldChoice = await ctx.ui.select("Which field contains the secret?", fieldOptions);
              if (!fieldChoice) return;

              if (fieldChoice === "Password field") {
                bwRecipe = `!bw get password "${pickedItem.name}"`;
              } else if (fieldChoice === "Username field") {
                bwRecipe = `!bw get username "${pickedItem.name}"`;
              } else if (fieldChoice === "Notes field") {
                bwRecipe = `!bw get notes "${pickedItem.name}"`;
              } else {
                const fieldName = fieldChoice.replace("Custom field: ", "");
                bwRecipe = `!bw get item "${pickedItem.name}" | jq -r '.fields[] | select(.name=="${fieldName}") | .value'`;
              }
            }

            recipes[secretName] = bwRecipe;
          } else if (choice.startsWith("macOS Keychain")) {
            const serviceName = await ctx.ui.input(
              `Enter the Keychain service name for ${secretName}:\n\n` +
              `To find it: Keychain Access app → search for the item → "Where" field\n` +
              `Or list items: security find-generic-password -l "${secretName.toLowerCase()}"`
            );
            if (!serviceName) return;
            recipes[secretName] = `!security find-generic-password -ws '${serviceName}'`;
          } else if (choice.startsWith("1Password")) {
            const ref = await ctx.ui.input(
              `Enter the 1Password item reference for ${secretName}:\n\n` +
              `Format: op://vault/item/field\n` +
              `Example: op://Private/API Keys/brave-search`
            );
            if (!ref) return;
            recipes[secretName] = ref.startsWith("op://") ? `!op read "${ref}"` : `!op read "op://${ref}"`;
          } else if (choice.startsWith("Environment variable")) {
            recipes[secretName] = secretName;
          } else if (choice.startsWith("Shell command")) {
            const cmd = await ctx.ui.input(
              `Enter shell command for ${secretName}:\n\n` +
              `The command's stdout will be used as the secret value.\n` +
              `Examples:\n` +
              `  bw get password "my-api-key"\n` +
              `  security find-generic-password -ws 'service-name'\n` +
              `  cat ~/.config/some-tool/token`
            );
            if (!cmd) return;
            recipes[secretName] = cmd.startsWith("!") ? cmd : `!${cmd}`;
          } else if (choice.startsWith("Enter value → store in macOS Keychain")) {
            // macOS handles auth (Touch ID / password / smart card) transparently
            // when security add-generic-password runs — no need to pre-unlock
            const val = await ctx.ui.input(
              `Enter the value for ${secretName}:\n\n` +
              `It will be stored in the macOS login keychain under "pi-kit/${secretName}".\n` +
              `macOS will prompt for Touch ID, password, or smart card if needed.`
            );
            if (!val) return;

            try {
              storeInKeychain(secretName, val);
              recipes[secretName] = keychainRecipe(secretName);
            } catch (e: any) {
              ctx.ui.notify(
                `❌ Failed to store in Keychain: ${e.message}\n\n` +
                `Make sure the login keychain is accessible. If you're in a headless ` +
                `environment, use the "Shell command" or "Environment variable" option instead.`,
                "error"
              );
              return;
            }
          } else if (choice.startsWith("Paste value")) {
            const val = await ctx.ui.input(
              `⚠️  Enter the value for ${secretName}:\n\n` +
              `This will be stored in plaintext in ~/.pi/agent/secrets.json.\n` +
              `Consider using Bitwarden or Keychain instead.`
            );
            if (!val) return;
            recipes[secretName] = `literal:${val}`;
          } else if (choice.startsWith("Remove")) {
            delete recipes[secretName];
            resolvedCache.delete(secretName);
            saveRecipes(recipes);
            ctx.ui.notify(`Removed recipe for ${secretName}`, "info");
            return;
          }

          saveRecipes(recipes);

          // Try to resolve immediately and inject into process.env
          resolvedCache.delete(secretName);
          const value = resolveSecret(secretName);
          if (value) {
            process.env[secretName] = value;
            ctx.ui.notify(`✅ ${secretName} configured and resolved successfully`, "info");
          } else {
            ctx.ui.notify(
              `⚠️ ${secretName} configured but could not resolve a value. Check the recipe with /secrets test ${secretName}`,
              "warn"
            );
          }
          break;
        }

        case "rm":
        case "remove":
        case "delete": {
          if (!secretName) {
            ctx.ui.notify("Usage: /secrets rm <NAME>", "error");
            return;
          }
          if (recipes[secretName]) {
            delete recipes[secretName];
            resolvedCache.delete(secretName);
            saveRecipes(recipes);
            ctx.ui.notify(`Removed recipe for ${secretName}`, "info");
          } else {
            ctx.ui.notify(`No recipe found for ${secretName}`, "error");
          }
          break;
        }

        case "test": {
          if (!secretName) {
            ctx.ui.notify("Usage: /secrets test <NAME>", "error");
            return;
          }
          const recipe = recipes[secretName];
          if (!recipe && !process.env[secretName]) {
            ctx.ui.notify(`No recipe or env var found for ${secretName}`, "error");
            return;
          }

          // Re-resolve (bypass cache)
          resolvedCache.delete(secretName);
          const value = resolveSecret(secretName);
          if (value) {
            // Show masked value: first 4 chars + masked rest
            const masked =
              value.length > 8
                ? value.slice(0, 4) + "•".repeat(Math.min(value.length - 4, 20)) + ` (${value.length} chars)`
                : "•".repeat(value.length) + ` (${value.length} chars)`;
            ctx.ui.notify(`✅ ${secretName} resolved: ${masked}`, "info");
          } else {
            const source = recipe || `env:${secretName}`;
            ctx.ui.notify(`❌ ${secretName} failed to resolve from: ${source}`, "error");
          }
          break;
        }

        default:
          ctx.ui.notify(
            "Usage: /secrets <list|configure|rm|test> [name]\n\n" +
              "  /secrets list              — show all configured secrets\n" +
              "  /secrets configure <NAME>  — set up resolution for a secret\n" +
              "  /secrets rm <NAME>         — remove a secret recipe\n" +
              "  /secrets test <NAME>       — test if a secret resolves",
            "info"
          );
      }
    },
  });
}
