/**
 * sermon-widget.ts — TUI Component that scrawls the Crawler's sermon
 * character by character beneath the spinner verb.
 *
 * Renders a single line of dim text that slowly reveals itself, wrapping
 * cyclically through the sermon. Entry point is randomized.
 *
 * Timing:
 *   - Base character interval: 67ms (~15 cps)
 *   - Word boundary pause: 120ms additional
 *   - The effect is biological — hesitant, breathing
 */

import type { TUI, Component } from "@styrene-lab/pi-tui";
import type { Theme } from "@styrene-lab/pi-coding-agent";
import { SERMON } from "./sermon.js";

const CHAR_INTERVAL_MS = 67;
const WORD_PAUSE_MS = 120;

/**
 * Maximum visible characters on the scrawl line.
 * The text is a sliding window — old characters fall off the left
 * as new ones appear on the right, like a ticker that breathes.
 */
const MAX_VISIBLE = 72;

export function createSermonWidget(
  tui: TUI,
  theme: Theme,
): Component & { dispose(): void } {
  // Randomize entry point
  let cursor = Math.floor(Math.random() * SERMON.length);
  let revealed = "";
  let intervalId: ReturnType<typeof setTimeout> | null = null;

  function advance() {
    const ch = SERMON[cursor % SERMON.length];
    cursor = (cursor + 1) % SERMON.length;
    revealed += ch;

    // Sliding window — keep only the tail
    if (revealed.length > MAX_VISIBLE) {
      revealed = revealed.slice(revealed.length - MAX_VISIBLE);
    }

    tui.requestRender();

    // Schedule next character with variable timing
    const nextCh = SERMON[cursor % SERMON.length];
    const delay = nextCh === " " ? CHAR_INTERVAL_MS + WORD_PAUSE_MS : CHAR_INTERVAL_MS;
    intervalId = setTimeout(advance, delay);
  }

  // Start the scrawl
  intervalId = setTimeout(advance, CHAR_INTERVAL_MS);

  return {
    render(width: number): string[] {
      const maxW = Math.min(MAX_VISIBLE, width - 4);
      const visible = revealed.length > maxW
        ? revealed.slice(revealed.length - maxW)
        : revealed;
      // Render in dim/muted color — the text should feel like it's
      // barely there, written in bioluminescent spore
      const line = theme.fg("muted", `  ${visible}`);
      return [line];
    },
    invalidate() {
      // No cached state to invalidate — we re-render from `revealed` each time
    },
    dispose() {
      if (intervalId) {
        clearTimeout(intervalId);
        intervalId = null;
      }
    },
  };
}
