#!/usr/bin/env node
/**
 * Omegon entry point.
 *
 * Keeps mutable user state in the shared pi-compatible agent directory while
 * injecting Omegon-packaged resources from the installed package root.
 *
 * Resolution order for the underlying agent core:
 *   1. vendor/pi-mono (dev mode — git submodule present)
 *   2. node_modules/@styrene-lab/pi-coding-agent (installed via npm)
 */
import { copyFileSync, cpSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { homedir } from "node:os";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const omegonRoot = dirname(dirname(__filename));
const defaultStateDir = join(homedir(), ".pi", "agent");
const stateDir = process.env.PI_CODING_AGENT_DIR || defaultStateDir;
const usingExplicitStateOverride = Boolean(process.env.PI_CODING_AGENT_DIR);

const vendorCli = join(omegonRoot, "vendor/pi-mono/packages/coding-agent/dist/cli.js");
const npmCli = join(omegonRoot, "node_modules/@styrene-lab/pi-coding-agent/dist/cli.js");
const cli = existsSync(vendorCli) ? vendorCli : npmCli;
const resolutionMode = cli === vendorCli ? "vendor" : "npm";

function migrateLegacyStatePath(relativePath, kind = "file") {
  if (usingExplicitStateOverride) {
    return;
  }

  const legacyPath = join(omegonRoot, relativePath);
  const targetPath = join(stateDir, relativePath);
  if (!existsSync(legacyPath) || existsSync(targetPath)) {
    return;
  }

  mkdirSync(dirname(targetPath), { recursive: true });
  if (kind === "directory") {
    cpSync(legacyPath, targetPath, { recursive: true, force: false });
    return;
  }
  copyFileSync(legacyPath, targetPath);
}

function injectBundledResourceArgs(argv) {
  const injected = [...argv];
  const pushPair = (flag, value) => {
    if (existsSync(value)) {
      injected.push(flag, value);
    }
  };

  // Omegon is the sole authority for bundled resources.
  // Suppress pi's auto-discovery of skills, prompts, and themes (which scans
  // ~/.pi/agent/*, installed packages, and project .pi/ dirs) so only our
  // manifest-declared resources load. The --no-* flags disable discovery
  // but still allow CLI-injected paths (our --extension manifest).
  // Extensions are NOT suppressed — project-local .pi/extensions/ should still work.
  injected.push("--no-skills", "--no-prompt-templates", "--no-themes");
  pushPair("--extension", omegonRoot);
  return injected;
}

if (process.argv.includes("--version") || process.argv.includes("-v")) {
  const pkg = JSON.parse(readFileSync(join(omegonRoot, "package.json"), "utf8"));
  process.stdout.write(pkg.version + "\n");
  process.exit(0);
}

if (process.argv.includes("--where")) {
  process.stdout.write(JSON.stringify({
    omegonRoot,
    cli,
    resolutionMode,
    agentDir: stateDir,
    stateDir,
    executable: "omegon",
  }, null, 2) + "\n");
  process.exit(0);
}

process.env.PI_CODING_AGENT_DIR = stateDir;

// Suppress the upstream runtime's version check and changelog display.
// Omegon has its own /update command and version-check extension —
// the pi-coding-agent's built-in checks leak upstream version numbers.
process.env.PI_SKIP_VERSION_CHECK = "1";
migrateLegacyStatePath("auth.json");
migrateLegacyStatePath("settings.json");
migrateLegacyStatePath("sessions", "directory");

// Force quiet startup — the splash extension provides the branded header.
// This suppresses the built-in keybinding hints, expanded changelog, and
// resource listing that pi's interactive mode normally renders before
// extensions have a chance to set a custom header.
function forceQuietStartup() {
  try {
    const settingsPath = join(stateDir, "settings.json");
    mkdirSync(stateDir, { recursive: true });
    let settings = {};
    if (existsSync(settingsPath)) {
      settings = JSON.parse(readFileSync(settingsPath, "utf8"));
    }
    let changed = false;
    if (settings.quietStartup === undefined) {
      settings.quietStartup = true;
      changed = true;
    }
    if (settings.collapseChangelog === undefined) {
      settings.collapseChangelog = true;
      changed = true;
    }
    // Belt-and-suspenders: force lastChangelogVersion to a sentinel that is always
    // semver-greater than any upstream changelog entry. This prevents the upstream
    // "Updated to vX.Y.Z" banner even if PI_SKIP_VERSION_CHECK is somehow not set.
    const SENTINEL_VERSION = "999.0.0";
    if (settings.lastChangelogVersion !== SENTINEL_VERSION) {
      settings.lastChangelogVersion = SENTINEL_VERSION;
      changed = true;
    }
    if (changed) {
      writeFileSync(settingsPath, JSON.stringify(settings, null, 2) + "\n", "utf8");
    }
  } catch { /* best effort */ }
}
forceQuietStartup();

function purgeSelfReferentialPackages() {
  try {
    const settingsPath = join(stateDir, "settings.json");
    if (!existsSync(settingsPath)) return;
    const settings = JSON.parse(readFileSync(settingsPath, "utf8"));
    if (!Array.isArray(settings.packages)) return;
    const selfPatterns = [
      /github\.com\/cwilson613\/omegon/i,
      /github\.com\/cwilson613\/pi-kit/i,
      /github\.com\/styrene-lab\/omegon/i,
    ];
    const filtered = settings.packages.filter(
      (pkg) => !selfPatterns.some((re) => re.test(String(pkg))),
    );
    if (filtered.length === settings.packages.length) return;
    settings.packages = filtered;
    writeFileSync(settingsPath, JSON.stringify(settings, null, 2) + "\n", "utf8");
  } catch { /* graceful failure — do not block startup */ }
}
purgeSelfReferentialPackages();

// ---------------------------------------------------------------------------
// CLI launch — subprocess with restart-loop support.
//
// Instead of importing the CLI directly (which makes restart impossible since
// Node can't replace its own process image), we spawn it as a child process.
// If the child exits with code 75 (EX_TEMPFAIL), we re-spawn — this is the
// restart signal from /update and /restart commands.
//
// This keeps the wrapper as the foreground process group leader throughout,
// so the re-spawned CLI always owns the terminal and can receive input.
// ---------------------------------------------------------------------------
import { spawn as nodeSpawn } from "node:child_process";

const RESTART_EXIT_CODE = 75;

const cliArgs = injectBundledResourceArgs(process.argv).slice(2);

const isInteractive = process.stdout.isTTY &&
  !process.argv.includes("-p") &&
  !process.argv.includes("--print") &&
  !process.argv.includes("--help") &&
  !process.argv.includes("-h");

function showPreImportSpinner() {
  if (!isInteractive) return undefined;
  const PRIMARY = "\x1b[38;2;42;180;200m";
  const DIM = "\x1b[38;2;64;88;112m";
  const RST = "\x1b[0m";
  const HIDE_CURSOR = "\x1b[?25l";
  const SHOW_CURSOR = "\x1b[?25h";
  const spinner = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
  let frame = 0;

  const restoreCursor = () => { try { process.stdout.write(SHOW_CURSOR); } catch {} };
  process.on("exit", restoreCursor);

  process.stdout.write(HIDE_CURSOR);
  process.stdout.write(`\n  ${PRIMARY}omegon${RST} ${DIM}loading…${RST}`);

  const spinTimer = setInterval(() => {
    const s = spinner[frame % spinner.length];
    process.stdout.write(`\r  ${PRIMARY}${s} omegon${RST} ${DIM}loading…${RST}`);
    frame++;
  }, 80);

  return () => {
    clearInterval(spinTimer);
    process.removeListener("exit", restoreCursor);
    process.stdout.write(`\r\x1b[2K${SHOW_CURSOR}`);
  };
}

function launchCli() {
  return new Promise((resolve) => {
    const cleanup = showPreImportSpinner();

    const child = nodeSpawn(process.execPath, [cli, ...cliArgs], {
      stdio: "inherit",
      env: process.env,
    });

    // Let the child handle SIGINT (Ctrl+C) — the wrapper ignores it.
    const ignoreInt = () => {};
    process.on("SIGINT", ignoreInt);
    // Forward SIGTERM so graceful shutdown works.
    const fwdTerm = () => child.kill("SIGTERM");
    process.on("SIGTERM", fwdTerm);

    // Clean up spinner once the child's TUI takes over. The child will
    // clear the screen on startup anyway, but a brief delay ensures the
    // spinner doesn't flicker.
    if (cleanup) {
      setTimeout(() => cleanup(), 200);
    }

    child.on("exit", (code, signal) => {
      process.removeListener("SIGINT", ignoreInt);
      process.removeListener("SIGTERM", fwdTerm);
      if (signal) {
        // Re-raise the signal so the wrapper exits with the right status
        process.kill(process.pid, signal);
      }
      resolve(code ?? 1);
    });
  });
}

// Main loop — restart on exit code 75
let exitCode;
do {
  exitCode = await launchCli();
} while (exitCode === RESTART_EXIT_CODE);

process.exit(exitCode);
