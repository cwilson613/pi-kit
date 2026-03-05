/**
 * Shared state between extensions loaded in the same pi process.
 *
 * Uses globalThis to guarantee sharing regardless of module loader
 * caching behavior (jiti may create separate instances per extension).
 *
 * Keep this minimal — only data that genuinely needs cross-extension sharing.
 */

const SHARED_KEY = Symbol.for("pi-kit-shared-state");

interface SharedState {
  /** Approximate token count of the last memory injection into context.
   *  Written by project-memory, read by status-bar for the context gauge. */
  memoryTokenEstimate: number;
}

// Initialize once on first import, reuse thereafter via global symbol
if (!(globalThis as any)[SHARED_KEY]) {
  (globalThis as any)[SHARED_KEY] = {
    memoryTokenEstimate: 0,
  } satisfies SharedState;
}

export const sharedState: SharedState = (globalThis as any)[SHARED_KEY];
