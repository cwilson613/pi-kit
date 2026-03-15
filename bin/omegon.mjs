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
import { copyFileSync, cpSync, existsSync, mkdirSync } from "node:fs";
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

  pushPair("--extension", omegonRoot);
  pushPair("--skill", join(omegonRoot, "skills"));
  pushPair("--prompt-template", join(omegonRoot, "prompts"));
  pushPair("--theme", join(omegonRoot, "themes"));
  return injected;
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
migrateLegacyStatePath("auth.json");
migrateLegacyStatePath("settings.json");
migrateLegacyStatePath("sessions", "directory");
process.argv = injectBundledResourceArgs(process.argv);

await import(cli);
