import { existsSync } from "node:fs";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { dirname, join, resolve } from "node:path";

export interface OmegonSubprocessSpec {
  command: string;
  argvPrefix: string[];
  omegonEntry: string;
}

let cached: OmegonSubprocessSpec | null = null;

/**
 * Resolve the canonical Omegon-owned subprocess entrypoint without relying on PATH.
 *
 * Internal helpers should spawn `process.execPath` with `bin/omegon.mjs` explicitly,
 * rather than assuming a `pi` or `omegon` binary on PATH points back to this install.
 */
export function resolveOmegonSubprocess(): OmegonSubprocessSpec {
  if (cached) return cached;

  const here = dirname(fileURLToPath(import.meta.url));
  const omegonEntry = join(here, "..", "..", "bin", "omegon.mjs");
  cached = {
    command: process.execPath,
    argvPrefix: [omegonEntry],
    omegonEntry,
  };
  return cached;
}

// ─── Native agent binary ────────────────────────────────────────────────────

/**
 * Resolved native agent binary specification.
 *
 * When available, cleave children can be dispatched to this binary instead
 * of spawning a full Node.js + Omegon TS process. The native binary is
 * faster to start, uses less memory, and has the core 4 tools built in
 * (read, write, edit, bash).
 */
export interface NativeAgentSpec {
  /** Path to the omegon-agent binary */
  binaryPath: string;
  /** Path to the LLM bridge script (passed via --bridge) */
  bridgePath: string;
}

let nativeCached: NativeAgentSpec | null | undefined;
/** Timestamp of last successful resolution — stale after 30s so mid-session builds are picked up. */
let nativeCachedAt = 0;
const NATIVE_CACHE_TTL_MS = 30_000;

/**
 * Resolve the native omegon-agent binary if available.
 *
 * Search order:
 * 1. OMEGON_AGENT_BINARY env var (explicit override for CI/testing)
 * 2. core/target/release/omegon-agent (local development build)
 * 3. Adjacent to the Omegon package: node_modules/.omegon/omegon-agent (npm install)
 *
 * Returns null if no binary is found — callers must fall back to TS subprocess.
 * Result is cached for 30 s — long enough to avoid redundant stat() within a single
 * cleave_run but short enough that a mid-session `cargo build` is picked up.
 */
export function resolveNativeAgent(): NativeAgentSpec | null {
  // Return cached result if still fresh, but never cache a null result
  // (the binary may be built mid-session).
  if (nativeCached && Date.now() - nativeCachedAt < NATIVE_CACHE_TTL_MS) {
    return nativeCached;
  }

  const here = dirname(fileURLToPath(import.meta.url));
  const repoRoot = resolve(here, "..", "..");

  // Bridge is always relative to the repo/package root
  const bridgePath = join(repoRoot, "core", "bridge", "llm-bridge.mjs");

  // 1. Explicit override via env var
  const envPath = process.env.OMEGON_AGENT_BINARY;
  if (envPath && existsSync(envPath)) {
    nativeCached = { binaryPath: envPath, bridgePath };
    nativeCachedAt = Date.now();
    return nativeCached;
  }

  // 2. Local development build (cargo build --release in core/)
  const devBinary = join(repoRoot, "core", "target", "release", "omegon-agent");
  if (existsSync(devBinary)) {
    nativeCached = { binaryPath: devBinary, bridgePath };
    nativeCachedAt = Date.now();
    return nativeCached;
  }

  // 3. npm platform package (@styrene-lab/omegon-{platform})
  const platformPkg = resolvePlatformPackageBinary();
  if (platformPkg && existsSync(platformPkg)) {
    nativeCached = { binaryPath: platformPkg, bridgePath };
    nativeCachedAt = Date.now();
    return nativeCached;
  }

  // 4. Legacy npm install location
  const npmBinary = join(repoRoot, "node_modules", ".omegon", "omegon-agent");
  if (existsSync(npmBinary)) {
    nativeCached = { binaryPath: npmBinary, bridgePath };
    nativeCachedAt = Date.now();
    return nativeCached;
  }

  // Don't cache null — allow immediate retry after cargo build
  return null;
}

/**
 * Resolve the omegon-agent binary from the platform-specific npm package.
 *
 * Maps `process.platform` + `process.arch` to `@styrene-lab/omegon-{platform}`
 * and finds the binary via createRequire resolution.
 */
function resolvePlatformPackageBinary(): string | null {
  const platformMap: Record<string, string> = {
    "darwin-arm64": "@styrene-lab/omegon-darwin-arm64",
    "darwin-x64": "@styrene-lab/omegon-darwin-x64",
    "linux-x64": "@styrene-lab/omegon-linux-x64",
    "linux-arm64": "@styrene-lab/omegon-linux-arm64",
  };

  const key = `${process.platform}-${process.arch}`;
  const pkg = platformMap[key];
  if (!pkg) return null;

  try {
    const here = dirname(fileURLToPath(import.meta.url));
    const repoRoot = resolve(here, "..", "..");
    const req = createRequire(join(repoRoot, "package.json"));
    const pkgJson = req.resolve(`${pkg}/package.json`);
    return join(dirname(pkgJson), "omegon-agent");
  } catch {
    return null;
  }
}

/**
 * Clear the cached native agent spec. For testing only.
 * @internal
 */
export function _clearNativeAgentCache(): void {
  nativeCached = undefined;
  nativeCachedAt = 0;
}
