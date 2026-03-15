import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

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
