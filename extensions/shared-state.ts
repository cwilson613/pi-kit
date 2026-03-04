/**
 * Shared state between extensions loaded in the same pi process.
 *
 * Since all extensions are loaded via jiti (which caches modules),
 * importing this from multiple extensions yields the same object.
 *
 * Keep this minimal — only data that genuinely needs cross-extension sharing.
 */

export const sharedState = {
  /** Approximate token count of the last memory injection into context.
   *  Written by project-memory, read by status-bar for the context gauge. */
  memoryTokenEstimate: 0,
};
